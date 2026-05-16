package httputil

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"
)

func TestNewClient(t *testing.T) {
	client := NewClient("http://localhost:8080", 30*time.Second)

	if client.BaseURL() != "http://localhost:8080" {
		t.Errorf("expected base URL http://localhost:8080, got %s", client.BaseURL())
	}
}

func TestClient_Get(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodGet {
			t.Errorf("expected GET, got %s", r.Method)
		}
		if r.URL.Path != "/test" {
			t.Errorf("expected /test, got %s", r.URL.Path)
		}
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write([]byte(`{"status":"ok"}`))
	}))
	defer server.Close()

	client := NewClient(server.URL, 5*time.Second)
	resp, err := client.Get(context.Background(), "/test")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	defer func() { _ = resp.Body.Close() }()

	if resp.StatusCode != http.StatusOK {
		t.Errorf("expected status 200, got %d", resp.StatusCode)
	}

	body, _ := io.ReadAll(resp.Body)
	if string(body) != `{"status":"ok"}` {
		t.Errorf("unexpected body: %s", body)
	}
}

func TestClient_PostJSON(t *testing.T) {
	type testRequest struct {
		Message string `json:"message"`
	}

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost {
			t.Errorf("expected POST, got %s", r.Method)
		}
		if r.Header.Get("Content-Type") != "application/json" {
			t.Errorf("expected Content-Type application/json, got %s", r.Header.Get("Content-Type"))
		}

		var req testRequest
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			t.Errorf("failed to decode request: %v", err)
		}
		if req.Message != "hello" {
			t.Errorf("expected message 'hello', got %s", req.Message)
		}

		w.WriteHeader(http.StatusOK)
		_, _ = w.Write([]byte(`{"response":"world"}`))
	}))
	defer server.Close()

	client := NewClient(server.URL, 5*time.Second)
	resp, err := client.PostJSON(context.Background(), "/chat", testRequest{Message: "hello"})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	defer func() { _ = resp.Body.Close() }()

	if resp.StatusCode != http.StatusOK {
		t.Errorf("expected status 200, got %d", resp.StatusCode)
	}
}

func TestClient_ContextCancellation(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		time.Sleep(100 * time.Millisecond)
		w.WriteHeader(http.StatusOK)
	}))
	defer server.Close()

	client := NewClient(server.URL, 5*time.Second)

	ctx, cancel := context.WithCancel(context.Background())
	cancel() // Cancel immediately

	_, err := client.Get(ctx, "/test")
	if err == nil {
		t.Error("expected error for canceled context")
	}
}

func TestClient_Do(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("X-Custom", "value")
		w.WriteHeader(http.StatusOK)
	}))
	defer server.Close()

	client := NewClient(server.URL, 5*time.Second)

	req, _ := http.NewRequest(http.MethodGet, server.URL+"/custom", nil)
	req.Header.Set("X-Request", "test")

	resp, err := client.Do(req)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	defer func() { _ = resp.Body.Close() }()

	if resp.Header.Get("X-Custom") != "value" {
		t.Errorf("expected X-Custom header value, got %s", resp.Header.Get("X-Custom"))
	}
}

func TestNewClientWithHeaders(t *testing.T) {
	headers := map[string]string{
		"Authorization": "Bearer test-key",
		"X-Custom":      "value",
	}
	client := NewClientWithHeaders("http://localhost:8080", 30*time.Second, headers)

	if client.BaseURL() != "http://localhost:8080" {
		t.Errorf("expected base URL http://localhost:8080, got %s", client.BaseURL())
	}

	// Verify headers are copied, not referenced
	headers["Authorization"] = "modified"
	if client.headers["Authorization"] != "Bearer test-key" {
		t.Error("headers should be copied, not referenced")
	}
}

func TestClient_SetHeader(t *testing.T) {
	client := NewClient("http://localhost:8080", 30*time.Second)
	client.SetHeader("X-API-Key", "test-key")

	if client.headers["X-API-Key"] != "test-key" {
		t.Errorf("expected X-API-Key header to be set")
	}
}

func TestClient_PostJSONWithHeaders(t *testing.T) {
	type testRequest struct {
		Message string `json:"message"`
	}

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		// Verify default header
		if r.Header.Get("X-Default") != "default-value" {
			t.Errorf("expected X-Default header, got %s", r.Header.Get("X-Default"))
		}
		// Verify custom header
		if r.Header.Get("X-Custom") != "custom-value" {
			t.Errorf("expected X-Custom header, got %s", r.Header.Get("X-Custom"))
		}
		// Verify Content-Type is set
		if r.Header.Get("Content-Type") != "application/json" {
			t.Errorf("expected Content-Type application/json, got %s", r.Header.Get("Content-Type"))
		}

		w.WriteHeader(http.StatusOK)
	}))
	defer server.Close()

	client := NewClientWithHeaders(server.URL, 5*time.Second, map[string]string{
		"X-Default": "default-value",
	})

	customHeaders := map[string]string{
		"X-Custom": "custom-value",
	}

	resp, err := client.PostJSONWithHeaders(context.Background(), "/test", testRequest{Message: "hello"}, customHeaders)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	defer func() { _ = resp.Body.Close() }()

	if resp.StatusCode != http.StatusOK {
		t.Errorf("expected status 200, got %d", resp.StatusCode)
	}
}

func TestClient_PostJSONWithHeaders_Override(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		// Custom header should override default
		if r.Header.Get("Authorization") != "Bearer custom" {
			t.Errorf("expected Authorization to be overridden, got %s", r.Header.Get("Authorization"))
		}
		w.WriteHeader(http.StatusOK)
	}))
	defer server.Close()

	client := NewClientWithHeaders(server.URL, 5*time.Second, map[string]string{
		"Authorization": "Bearer default",
	})

	customHeaders := map[string]string{
		"Authorization": "Bearer custom",
	}

	resp, err := client.PostJSONWithHeaders(context.Background(), "/test", struct{}{}, customHeaders)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	defer func() { _ = resp.Body.Close() }()
}

func TestClient_GetWithHeaders(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		// Verify default header
		if r.Header.Get("X-API-Key") != "test-key" {
			t.Errorf("expected X-API-Key header, got %s", r.Header.Get("X-API-Key"))
		}
		// Verify custom header
		if r.Header.Get("X-Request-ID") != "12345" {
			t.Errorf("expected X-Request-ID header, got %s", r.Header.Get("X-Request-ID"))
		}
		w.WriteHeader(http.StatusOK)
	}))
	defer server.Close()

	client := NewClientWithHeaders(server.URL, 5*time.Second, map[string]string{
		"X-API-Key": "test-key",
	})

	resp, err := client.GetWithHeaders(context.Background(), "/test", map[string]string{
		"X-Request-ID": "12345",
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	defer func() { _ = resp.Body.Close() }()

	if resp.StatusCode != http.StatusOK {
		t.Errorf("expected status 200, got %d", resp.StatusCode)
	}
}
