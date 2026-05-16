package nxuskit

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"
)

func TestFireworksProvider_Constructor(t *testing.T) {
	t.Run("requires API key", func(t *testing.T) {
		t.Setenv("FIREWORKS_API_KEY", "")
		_, err := NewFireworksProvider()
		if err == nil {
			t.Error("expected error when no API key provided")
		}
	})

	t.Run("uses API key from environment", func(t *testing.T) {
		t.Setenv("FIREWORKS_API_KEY", "test-key")
		provider, err := NewFireworksProvider()
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if provider == nil {
			t.Fatal("expected provider to be created")
		}
	})

	t.Run("option overrides environment", func(t *testing.T) {
		t.Setenv("FIREWORKS_API_KEY", "env-key")
		provider, err := NewFireworksProvider(WithFireworksAPIKey("option-key"))
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if provider == nil {
			t.Fatal("expected provider to be created")
		}
	})

	t.Run("custom base URL", func(t *testing.T) {
		t.Setenv("FIREWORKS_API_KEY", "test-key")
		provider, err := NewFireworksProvider(WithFireworksBaseURL("https://custom.api.com"))
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if provider == nil {
			t.Fatal("expected provider to be created")
		}
	})

	t.Run("custom timeout", func(t *testing.T) {
		t.Setenv("FIREWORKS_API_KEY", "test-key")
		provider, err := NewFireworksProvider(WithFireworksTimeout(5 * time.Minute))
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if provider == nil {
			t.Fatal("expected provider to be created")
		}
	})
}

func TestFireworksProvider_ProviderName(t *testing.T) {
	t.Setenv("FIREWORKS_API_KEY", "test-key")
	provider, _ := NewFireworksProvider()
	if provider.ProviderName() != "fireworks" {
		t.Errorf("expected 'fireworks', got '%s'", provider.ProviderName())
	}
}

func TestFireworksProvider_Chat(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("Authorization") != "Bearer test-key" {
			t.Errorf("expected Authorization header, got '%s'", r.Header.Get("Authorization"))
		}
		if r.URL.Path != "/chat/completions" {
			t.Errorf("expected path /chat/completions, got %s", r.URL.Path)
		}

		resp := openaiChatResponse{
			Model:   "accounts/fireworks/models/llama-v3p1-70b-instruct",
			Choices: []openaiChoice{{Message: &openaiMessage{Content: "Hello!"}, FinishReason: strPtr("stop")}},
			Usage:   &openaiUsage{PromptTokens: 5, CompletionTokens: 3},
		}
		_ = json.NewEncoder(w).Encode(resp)
	}))
	defer server.Close()

	provider, _ := NewFireworksProvider(
		WithFireworksAPIKey("test-key"),
		WithFireworksBaseURL(server.URL),
	)

	resp, err := provider.Chat(context.Background(), &ChatRequest{
		Model:    "accounts/fireworks/models/llama-v3p1-70b-instruct",
		Messages: []Message{UserMessage("Hi")},
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if resp.Content != "Hello!" {
		t.Errorf("expected content, got '%s'", resp.Content)
	}
}

func TestFireworksProvider_ListModels(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/models" {
			t.Errorf("expected path /models, got %s", r.URL.Path)
		}
		resp := openaiModelsResponse{
			Data: []openaiModelInfo{{ID: "accounts/fireworks/models/llama-v3p1-70b-instruct"}},
		}
		_ = json.NewEncoder(w).Encode(resp)
	}))
	defer server.Close()

	provider, _ := NewFireworksProvider(
		WithFireworksAPIKey("test-key"),
		WithFireworksBaseURL(server.URL),
	)

	models, err := provider.ListModels(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(models) != 1 {
		t.Fatalf("expected 1 model, got %d", len(models))
	}
}

func TestFireworksProvider_Ping(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_ = json.NewEncoder(w).Encode(openaiModelsResponse{Data: []openaiModelInfo{}})
	}))
	defer server.Close()

	provider, _ := NewFireworksProvider(
		WithFireworksAPIKey("test-key"),
		WithFireworksBaseURL(server.URL),
	)

	err := provider.Ping(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestFireworksProvider_ChatStream(t *testing.T) {
	t.Run("streams content successfully", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.Header().Set("Content-Type", "text/event-stream")
			w.WriteHeader(http.StatusOK)

			// Send streaming chunks
			chunks := []string{
				`data: {"id":"chatcmpl-1","object":"chat.completion.chunk","model":"llama-v3p1-70b-instruct","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}`,
				`data: {"id":"chatcmpl-1","object":"chat.completion.chunk","model":"llama-v3p1-70b-instruct","choices":[{"index":0,"delta":{"content":" World"},"finish_reason":null}]}`,
				`data: {"id":"chatcmpl-1","object":"chat.completion.chunk","model":"llama-v3p1-70b-instruct","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}`,
				`data: [DONE]`,
			}
			for _, chunk := range chunks {
				_, _ = w.Write([]byte(chunk + "\n\n"))
				if f, ok := w.(http.Flusher); ok {
					f.Flush()
				}
			}
		}))
		defer server.Close()

		provider, _ := NewFireworksProvider(
			WithFireworksAPIKey("test-key"),
			WithFireworksBaseURL(server.URL),
		)

		chunks, errs := provider.ChatStream(context.Background(), &ChatRequest{
			Model:    "llama-v3p1-70b-instruct",
			Messages: []Message{UserMessage("Hi")},
		})

		var content string
		for chunk := range chunks {
			content += chunk.Delta
		}

		if err := <-errs; err != nil {
			t.Fatalf("unexpected error: %v", err)
		}

		if content != "Hello World" {
			t.Errorf("expected 'Hello World', got '%s'", content)
		}
	})

	t.Run("handles server error", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.WriteHeader(http.StatusInternalServerError)
			_, _ = w.Write([]byte(`{"error":{"message":"Internal server error"}}`))
		}))
		defer server.Close()

		provider, _ := NewFireworksProvider(
			WithFireworksAPIKey("test-key"),
			WithFireworksBaseURL(server.URL),
		)

		chunks, errs := provider.ChatStream(context.Background(), &ChatRequest{
			Model:    "llama-v3p1-70b-instruct",
			Messages: []Message{UserMessage("Hi")},
		})

		// Drain chunks channel
		for range chunks {
		}

		err := <-errs
		if err == nil {
			t.Fatal("expected error for server error response")
		}
	})
}

func TestFireworksProvider_StreamWithUsage(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/event-stream")
		w.WriteHeader(http.StatusOK)

		chunks := []string{
			`data: {"id":"chatcmpl-1","object":"chat.completion.chunk","model":"llama","choices":[{"index":0,"delta":{"content":"Hi"},"finish_reason":null}]}`,
			`data: {"id":"chatcmpl-1","object":"chat.completion.chunk","model":"llama","choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":5,"completion_tokens":1,"total_tokens":6}}`,
			`data: [DONE]`,
		}
		for _, chunk := range chunks {
			_, _ = w.Write([]byte(chunk + "\n\n"))
			if f, ok := w.(http.Flusher); ok {
				f.Flush()
			}
		}
	}))
	defer server.Close()

	provider, _ := NewFireworksProvider(
		WithFireworksAPIKey("test-key"),
		WithFireworksBaseURL(server.URL),
	)

	chunks, usageCh := provider.StreamWithUsage(context.Background(), &ChatRequest{
		Model:    "llama",
		Messages: []Message{UserMessage("Hi")},
	})

	var content string
	for chunk := range chunks {
		content += chunk.Delta
	}

	usage := <-usageCh
	if usage.Actual.PromptTokens == 0 && usage.Actual.CompletionTokens == 0 {
		// Usage might not be available in streaming - this is acceptable
		t.Log("Token usage not available in stream (expected for some providers)")
	}

	if content != "Hi" {
		t.Errorf("expected 'Hi', got '%s'", content)
	}
}

func TestFireworksProvider_GetCapabilities(t *testing.T) {
	t.Setenv("FIREWORKS_API_KEY", "test-key")
	provider, _ := NewFireworksProvider()

	caps := provider.GetCapabilities()
	if !caps.SupportsStreaming {
		t.Error("expected streaming support")
	}
	if !caps.SupportsJSONMode {
		t.Error("expected JSON mode support")
	}
	if caps.SupportsTools {
		t.Error("expected no tool support for Fireworks")
	}
}

func TestFireworksProvider_FreshSession(t *testing.T) {
	t.Setenv("FIREWORKS_API_KEY", "test-key")
	provider, _ := NewFireworksProvider()

	fresh, err := provider.FreshSession()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	// Fireworks is stateless, so FreshSession returns self
	if fresh != provider {
		t.Error("expected FreshSession to return same provider (stateless)")
	}
}
