package nxuskit

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"
)

func TestXaiProvider_Constructor(t *testing.T) {
	t.Run("requires API key", func(t *testing.T) {
		t.Setenv("XAI_API_KEY", "")
		_, err := NewXaiProvider()
		if err == nil {
			t.Error("expected error when no API key provided")
		}
	})

	t.Run("uses API key from environment", func(t *testing.T) {
		t.Setenv("XAI_API_KEY", "test-key")
		provider, err := NewXaiProvider()
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if provider == nil {
			t.Fatal("expected provider to be created")
		}
	})

	t.Run("option overrides environment", func(t *testing.T) {
		t.Setenv("XAI_API_KEY", "env-key")
		provider, err := NewXaiProvider(WithXaiAPIKey("option-key"))
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if provider == nil {
			t.Fatal("expected provider to be created")
		}
	})

	t.Run("custom base URL", func(t *testing.T) {
		t.Setenv("XAI_API_KEY", "test-key")
		provider, err := NewXaiProvider(WithXaiBaseURL("https://custom.api.com"))
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if provider == nil {
			t.Fatal("expected provider to be created")
		}
	})

	t.Run("custom timeout", func(t *testing.T) {
		t.Setenv("XAI_API_KEY", "test-key")
		provider, err := NewXaiProvider(WithXaiTimeout(5 * time.Minute))
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if provider == nil {
			t.Fatal("expected provider to be created")
		}
	})

	t.Run("rejects invalid options", func(t *testing.T) {
		t.Setenv("XAI_API_KEY", "test-key")
		cases := []XaiOption{
			WithXaiAPIKey(""),
			WithXaiBaseURL(""),
			WithXaiTimeout(0),
			WithXaiTimeout(-1 * time.Second),
		}
		for _, opt := range cases {
			if _, err := NewXaiProvider(opt); err == nil {
				t.Fatal("expected invalid option to return an error")
			}
		}
	})
}

func TestXaiProvider_ProviderName(t *testing.T) {
	t.Setenv("XAI_API_KEY", "test-key")
	provider, _ := NewXaiProvider()
	if provider.ProviderName() != "xai" {
		t.Errorf("expected 'xai', got '%s'", provider.ProviderName())
	}
}

func TestXaiProvider_ChatUsesOpenAICompatibleEndpoint(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("Authorization") != "Bearer test-key" {
			t.Errorf("expected Authorization header, got '%s'", r.Header.Get("Authorization"))
		}
		if r.URL.Path != "/chat/completions" {
			t.Errorf("expected path /chat/completions, got %s", r.URL.Path)
		}

		resp := openaiChatResponse{
			Model:   "grok-4",
			Choices: []openaiChoice{{Message: &openaiMessage{Content: "Hello from Grok"}, FinishReason: strPtr("stop")}},
			Usage:   &openaiUsage{PromptTokens: 5, CompletionTokens: 3},
		}
		_ = json.NewEncoder(w).Encode(resp)
	}))
	defer server.Close()

	provider, _ := NewXaiProvider(
		WithXaiAPIKey("test-key"),
		WithXaiBaseURL(server.URL),
	)

	resp, err := provider.Chat(context.Background(), &ChatRequest{
		Model:    "grok-4",
		Messages: []Message{UserMessage("Hi")},
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if resp.Content != "Hello from Grok" {
		t.Errorf("expected content, got '%s'", resp.Content)
	}
}

func TestXaiProvider_ListModels(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/models" {
			t.Errorf("expected path /models, got %s", r.URL.Path)
		}
		resp := openaiModelsResponse{
			Data: []openaiModelInfo{{ID: "grok-4"}},
		}
		_ = json.NewEncoder(w).Encode(resp)
	}))
	defer server.Close()

	provider, _ := NewXaiProvider(
		WithXaiAPIKey("test-key"),
		WithXaiBaseURL(server.URL),
	)

	models, err := provider.ListModels(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(models) != 1 || models[0].Name != "grok-4" {
		t.Fatalf("expected grok-4 model, got %#v", models)
	}
}

func TestXaiProvider_Ping(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/models" {
			t.Errorf("expected path /models, got %s", r.URL.Path)
		}
		_ = json.NewEncoder(w).Encode(openaiModelsResponse{Data: []openaiModelInfo{}})
	}))
	defer server.Close()

	provider, _ := NewXaiProvider(
		WithXaiAPIKey("test-key"),
		WithXaiBaseURL(server.URL),
	)

	if err := provider.Ping(context.Background()); err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestXaiProvider_ChatStream(t *testing.T) {
	t.Run("streams content successfully", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.Header().Set("Content-Type", "text/event-stream")
			w.WriteHeader(http.StatusOK)

			chunks := []string{
				`data: {"id":"chatcmpl-1","object":"chat.completion.chunk","model":"grok-4","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}`,
				`data: {"id":"chatcmpl-1","object":"chat.completion.chunk","model":"grok-4","choices":[{"index":0,"delta":{"content":" from Grok"},"finish_reason":null}]}`,
				`data: {"id":"chatcmpl-1","object":"chat.completion.chunk","model":"grok-4","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}`,
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

		provider, _ := NewXaiProvider(
			WithXaiAPIKey("test-key"),
			WithXaiBaseURL(server.URL),
		)

		chunks, errs := provider.ChatStream(context.Background(), &ChatRequest{
			Model:    "grok-4",
			Messages: []Message{UserMessage("Hi")},
		})

		var content string
		for chunk := range chunks {
			content += chunk.Delta
		}

		if err := <-errs; err != nil {
			t.Fatalf("unexpected error: %v", err)
		}

		if content != "Hello from Grok" {
			t.Errorf("expected 'Hello from Grok', got '%s'", content)
		}
	})

	t.Run("handles rate limit error", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.Header().Set("Retry-After", "30")
			w.WriteHeader(http.StatusTooManyRequests)
			_, _ = w.Write([]byte(`{"error":{"message":"Rate limit exceeded","type":"rate_limit_error"}}`))
		}))
		defer server.Close()

		provider, _ := NewXaiProvider(
			WithXaiAPIKey("test-key"),
			WithXaiBaseURL(server.URL),
		)

		chunks, errs := provider.ChatStream(context.Background(), &ChatRequest{
			Model:    "grok-4",
			Messages: []Message{UserMessage("Hi")},
		})

		for range chunks {
		}

		if err := <-errs; err == nil {
			t.Fatal("expected error for rate limit response")
		}
	})
}

func TestXaiProvider_StreamWithUsage(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/event-stream")
		w.WriteHeader(http.StatusOK)

		chunks := []string{
			`data: {"id":"chatcmpl-1","object":"chat.completion.chunk","model":"grok-4","choices":[{"index":0,"delta":{"content":"Fast!"},"finish_reason":null}]}`,
			`data: {"id":"chatcmpl-1","object":"chat.completion.chunk","model":"grok-4","choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":1,"total_tokens":4}}`,
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

	provider, _ := NewXaiProvider(
		WithXaiAPIKey("test-key"),
		WithXaiBaseURL(server.URL),
	)

	chunks, usageCh := provider.StreamWithUsage(context.Background(), &ChatRequest{
		Model:    "grok-4",
		Messages: []Message{UserMessage("Hi")},
	})

	var content string
	for chunk := range chunks {
		content += chunk.Delta
	}

	usage := <-usageCh
	if content != "Fast!" {
		t.Errorf("expected 'Fast!', got '%s'", content)
	}
	if usage.TotalTokens() != 4 {
		t.Errorf("expected total usage 4, got %d", usage.TotalTokens())
	}
}

func TestXaiProvider_GetCapabilities(t *testing.T) {
	t.Setenv("XAI_API_KEY", "test-key")
	provider, _ := NewXaiProvider()

	caps := provider.GetCapabilities()
	if !caps.SupportsStreaming {
		t.Error("expected streaming support")
	}
	if !caps.SupportsVision {
		t.Error("expected vision support for xAI Grok")
	}
	if !caps.SupportsTools {
		t.Error("expected tool support for xAI Grok")
	}
}

func TestXaiProvider_FreshSession(t *testing.T) {
	t.Setenv("XAI_API_KEY", "test-key")
	provider, _ := NewXaiProvider()

	fresh, err := provider.FreshSession()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if fresh != provider {
		t.Error("expected FreshSession to return same provider (stateless)")
	}
}
