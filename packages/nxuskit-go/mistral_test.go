package nxuskit

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"
)

func TestMistralProvider_Constructor(t *testing.T) {
	t.Run("requires API key", func(t *testing.T) {
		t.Setenv("MISTRAL_API_KEY", "")
		_, err := NewMistralProvider()
		if err == nil {
			t.Error("expected error when no API key provided")
		}
	})

	t.Run("uses API key from environment", func(t *testing.T) {
		t.Setenv("MISTRAL_API_KEY", "test-key")
		provider, err := NewMistralProvider()
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if provider == nil {
			t.Fatal("expected provider to be created")
		}
	})

	t.Run("option overrides environment", func(t *testing.T) {
		t.Setenv("MISTRAL_API_KEY", "env-key")
		provider, err := NewMistralProvider(WithMistralAPIKey("option-key"))
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if provider == nil {
			t.Fatal("expected provider to be created")
		}
	})

	t.Run("custom base URL", func(t *testing.T) {
		t.Setenv("MISTRAL_API_KEY", "test-key")
		provider, err := NewMistralProvider(WithMistralBaseURL("https://custom.api.com"))
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if provider == nil {
			t.Fatal("expected provider to be created")
		}
	})

	t.Run("custom timeout", func(t *testing.T) {
		t.Setenv("MISTRAL_API_KEY", "test-key")
		provider, err := NewMistralProvider(WithMistralTimeout(5 * time.Minute))
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if provider == nil {
			t.Fatal("expected provider to be created")
		}
	})
}

func TestMistralProvider_ProviderName(t *testing.T) {
	t.Setenv("MISTRAL_API_KEY", "test-key")
	provider, _ := NewMistralProvider()
	if provider.ProviderName() != "mistral" {
		t.Errorf("expected 'mistral', got '%s'", provider.ProviderName())
	}
}

func TestMistralProvider_Chat(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("Authorization") != "Bearer test-key" {
			t.Errorf("expected Authorization header, got '%s'", r.Header.Get("Authorization"))
		}
		if r.URL.Path != "/chat/completions" {
			t.Errorf("expected path /chat/completions, got %s", r.URL.Path)
		}

		resp := openaiChatResponse{
			Model:   "mistral-large-latest",
			Choices: []openaiChoice{{Message: &openaiMessage{Content: "Bonjour!"}, FinishReason: strPtr("stop")}},
			Usage:   &openaiUsage{PromptTokens: 5, CompletionTokens: 3},
		}
		_ = json.NewEncoder(w).Encode(resp)
	}))
	defer server.Close()

	provider, _ := NewMistralProvider(
		WithMistralAPIKey("test-key"),
		WithMistralBaseURL(server.URL),
	)

	resp, err := provider.Chat(context.Background(), &ChatRequest{
		Model:    "mistral-large-latest",
		Messages: []Message{UserMessage("Salut")},
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if resp.Content != "Bonjour!" {
		t.Errorf("expected content, got '%s'", resp.Content)
	}
}

func TestMistralProvider_ListModels(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/models" {
			t.Errorf("expected path /models, got %s", r.URL.Path)
		}
		resp := openaiModelsResponse{
			Data: []openaiModelInfo{
				{ID: "mistral-large-latest"},
				{ID: "mistral-small-latest"},
			},
		}
		_ = json.NewEncoder(w).Encode(resp)
	}))
	defer server.Close()

	provider, _ := NewMistralProvider(
		WithMistralAPIKey("test-key"),
		WithMistralBaseURL(server.URL),
	)

	models, err := provider.ListModels(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(models) != 2 {
		t.Fatalf("expected 2 models, got %d", len(models))
	}
}

func TestMistralProvider_Ping(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_ = json.NewEncoder(w).Encode(openaiModelsResponse{Data: []openaiModelInfo{}})
	}))
	defer server.Close()

	provider, _ := NewMistralProvider(
		WithMistralAPIKey("test-key"),
		WithMistralBaseURL(server.URL),
	)

	err := provider.Ping(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestMistralProvider_ChatStream(t *testing.T) {
	t.Run("streams content successfully", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.Header().Set("Content-Type", "text/event-stream")
			w.WriteHeader(http.StatusOK)

			chunks := []string{
				`data: {"id":"chatcmpl-1","object":"chat.completion.chunk","model":"mistral-large","choices":[{"index":0,"delta":{"content":"Bonjour"},"finish_reason":null}]}`,
				`data: {"id":"chatcmpl-1","object":"chat.completion.chunk","model":"mistral-large","choices":[{"index":0,"delta":{"content":" le monde"},"finish_reason":null}]}`,
				`data: {"id":"chatcmpl-1","object":"chat.completion.chunk","model":"mistral-large","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}`,
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

		provider, _ := NewMistralProvider(
			WithMistralAPIKey("test-key"),
			WithMistralBaseURL(server.URL),
		)

		chunks, errs := provider.ChatStream(context.Background(), &ChatRequest{
			Model:    "mistral-large",
			Messages: []Message{UserMessage("Salut")},
		})

		var content string
		for chunk := range chunks {
			content += chunk.Delta
		}

		if err := <-errs; err != nil {
			t.Fatalf("unexpected error: %v", err)
		}

		if content != "Bonjour le monde" {
			t.Errorf("expected 'Bonjour le monde', got '%s'", content)
		}
	})

	t.Run("handles authentication error", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.WriteHeader(http.StatusUnauthorized)
			_, _ = w.Write([]byte(`{"error":{"message":"Invalid API key"}}`))
		}))
		defer server.Close()

		provider, _ := NewMistralProvider(
			WithMistralAPIKey("invalid-key"),
			WithMistralBaseURL(server.URL),
		)

		chunks, errs := provider.ChatStream(context.Background(), &ChatRequest{
			Model:    "mistral-large",
			Messages: []Message{UserMessage("Hi")},
		})

		for range chunks {
		}

		err := <-errs
		if err == nil {
			t.Fatal("expected error for auth failure")
		}
	})
}

func TestMistralProvider_StreamWithUsage(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/event-stream")
		w.WriteHeader(http.StatusOK)

		chunks := []string{
			`data: {"id":"chatcmpl-1","object":"chat.completion.chunk","model":"mistral","choices":[{"index":0,"delta":{"content":"OK"},"finish_reason":null}]}`,
			`data: {"id":"chatcmpl-1","object":"chat.completion.chunk","model":"mistral","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}`,
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

	provider, _ := NewMistralProvider(
		WithMistralAPIKey("test-key"),
		WithMistralBaseURL(server.URL),
	)

	chunks, usageCh := provider.StreamWithUsage(context.Background(), &ChatRequest{
		Model:    "mistral",
		Messages: []Message{UserMessage("Hi")},
	})

	var content string
	for chunk := range chunks {
		content += chunk.Delta
	}

	<-usageCh

	if content != "OK" {
		t.Errorf("expected 'OK', got '%s'", content)
	}
}

func TestMistralProvider_GetCapabilities(t *testing.T) {
	t.Setenv("MISTRAL_API_KEY", "test-key")
	provider, _ := NewMistralProvider()

	caps := provider.GetCapabilities()
	if !caps.SupportsStreaming {
		t.Error("expected streaming support")
	}
	if !caps.SupportsTools {
		t.Error("expected tool support for Mistral")
	}
}

func TestMistralProvider_FreshSession(t *testing.T) {
	t.Setenv("MISTRAL_API_KEY", "test-key")
	provider, _ := NewMistralProvider()

	fresh, err := provider.FreshSession()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if fresh != provider {
		t.Error("expected FreshSession to return same provider (stateless)")
	}
}
