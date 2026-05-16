package nxuskit

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"
)

func TestOpenRouterProvider_Constructor(t *testing.T) {
	t.Run("requires API key", func(t *testing.T) {
		t.Setenv("OPENROUTER_API_KEY", "")
		_, err := NewOpenRouterProvider()
		if err == nil {
			t.Error("expected error when no API key provided")
		}
	})

	t.Run("uses API key from environment", func(t *testing.T) {
		t.Setenv("OPENROUTER_API_KEY", "test-key")
		provider, err := NewOpenRouterProvider()
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if provider == nil {
			t.Fatal("expected provider to be created")
		}
	})

	t.Run("option overrides environment", func(t *testing.T) {
		t.Setenv("OPENROUTER_API_KEY", "env-key")
		provider, err := NewOpenRouterProvider(WithOpenRouterAPIKey("option-key"))
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if provider == nil {
			t.Fatal("expected provider to be created")
		}
	})

	t.Run("custom base URL", func(t *testing.T) {
		t.Setenv("OPENROUTER_API_KEY", "test-key")
		provider, err := NewOpenRouterProvider(WithOpenRouterBaseURL("https://custom.api.com"))
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if provider == nil {
			t.Fatal("expected provider to be created")
		}
	})

	t.Run("custom timeout", func(t *testing.T) {
		t.Setenv("OPENROUTER_API_KEY", "test-key")
		provider, err := NewOpenRouterProvider(WithOpenRouterTimeout(5 * time.Minute))
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if provider == nil {
			t.Fatal("expected provider to be created")
		}
	})
}

func TestOpenRouterProvider_ProviderName(t *testing.T) {
	t.Setenv("OPENROUTER_API_KEY", "test-key")
	provider, _ := NewOpenRouterProvider()
	if provider.ProviderName() != "openrouter" {
		t.Errorf("expected 'openrouter', got '%s'", provider.ProviderName())
	}
}

func TestOpenRouterProvider_HTTPReferer(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		// Verify HTTP-Referer header
		if r.Header.Get("HTTP-Referer") != "https://myapp.com" {
			t.Errorf("expected HTTP-Referer header 'https://myapp.com', got '%s'", r.Header.Get("HTTP-Referer"))
		}
		if r.Header.Get("X-Title") != "My App" {
			t.Errorf("expected X-Title header 'My App', got '%s'", r.Header.Get("X-Title"))
		}

		resp := openaiChatResponse{
			Choices: []openaiChoice{{Message: &openaiMessage{Content: "OK"}}},
		}
		_ = json.NewEncoder(w).Encode(resp)
	}))
	defer server.Close()

	provider, _ := NewOpenRouterProvider(
		WithOpenRouterAPIKey("test-key"),
		WithOpenRouterBaseURL(server.URL),
		WithOpenRouterHTTPReferer("https://myapp.com"),
		WithOpenRouterXTitle("My App"),
	)

	_, err := provider.Chat(context.Background(), &ChatRequest{
		Model:    "openai/gpt-4o",
		Messages: []Message{UserMessage("Hi")},
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestOpenRouterProvider_Chat(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("Authorization") != "Bearer test-key" {
			t.Errorf("expected Authorization header, got '%s'", r.Header.Get("Authorization"))
		}
		if r.URL.Path != "/chat/completions" {
			t.Errorf("expected path /chat/completions, got %s", r.URL.Path)
		}

		resp := openaiChatResponse{
			Model:   "openai/gpt-4o",
			Choices: []openaiChoice{{Message: &openaiMessage{Content: "Hello!"}, FinishReason: strPtr("stop")}},
			Usage:   &openaiUsage{PromptTokens: 5, CompletionTokens: 3},
		}
		_ = json.NewEncoder(w).Encode(resp)
	}))
	defer server.Close()

	provider, _ := NewOpenRouterProvider(
		WithOpenRouterAPIKey("test-key"),
		WithOpenRouterBaseURL(server.URL),
	)

	resp, err := provider.Chat(context.Background(), &ChatRequest{
		Model:    "openai/gpt-4o",
		Messages: []Message{UserMessage("Hi")},
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if resp.Content != "Hello!" {
		t.Errorf("expected content, got '%s'", resp.Content)
	}
}

func TestOpenRouterProvider_ListModels(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/models" {
			t.Errorf("expected path /models, got %s", r.URL.Path)
		}
		resp := openaiModelsResponse{
			Data: []openaiModelInfo{
				{ID: "openai/gpt-4o"},
				{ID: "anthropic/claude-3.5-sonnet"},
			},
		}
		_ = json.NewEncoder(w).Encode(resp)
	}))
	defer server.Close()

	provider, _ := NewOpenRouterProvider(
		WithOpenRouterAPIKey("test-key"),
		WithOpenRouterBaseURL(server.URL),
	)

	models, err := provider.ListModels(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(models) != 2 {
		t.Fatalf("expected 2 models, got %d", len(models))
	}
}

func TestOpenRouterProvider_Ping(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_ = json.NewEncoder(w).Encode(openaiModelsResponse{Data: []openaiModelInfo{}})
	}))
	defer server.Close()

	provider, _ := NewOpenRouterProvider(
		WithOpenRouterAPIKey("test-key"),
		WithOpenRouterBaseURL(server.URL),
	)

	err := provider.Ping(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestOpenRouterProvider_ChatStream(t *testing.T) {
	t.Run("streams content successfully", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.Header().Set("Content-Type", "text/event-stream")
			w.WriteHeader(http.StatusOK)

			chunks := []string{
				`data: {"id":"chatcmpl-1","object":"chat.completion.chunk","model":"openai/gpt-4o","choices":[{"index":0,"delta":{"content":"Open"},"finish_reason":null}]}`,
				`data: {"id":"chatcmpl-1","object":"chat.completion.chunk","model":"openai/gpt-4o","choices":[{"index":0,"delta":{"content":"Router"},"finish_reason":null}]}`,
				`data: {"id":"chatcmpl-1","object":"chat.completion.chunk","model":"openai/gpt-4o","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}`,
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

		provider, _ := NewOpenRouterProvider(
			WithOpenRouterAPIKey("test-key"),
			WithOpenRouterBaseURL(server.URL),
		)

		chunks, errs := provider.ChatStream(context.Background(), &ChatRequest{
			Model:    "openai/gpt-4o",
			Messages: []Message{UserMessage("Hi")},
		})

		var content string
		for chunk := range chunks {
			content += chunk.Delta
		}

		if err := <-errs; err != nil {
			t.Fatalf("unexpected error: %v", err)
		}

		if content != "OpenRouter" {
			t.Errorf("expected 'OpenRouter', got '%s'", content)
		}
	})

	t.Run("handles model not found error", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.WriteHeader(http.StatusNotFound)
			_, _ = w.Write([]byte(`{"error":{"message":"Model not found: invalid/model"}}`))
		}))
		defer server.Close()

		provider, _ := NewOpenRouterProvider(
			WithOpenRouterAPIKey("test-key"),
			WithOpenRouterBaseURL(server.URL),
		)

		chunks, errs := provider.ChatStream(context.Background(), &ChatRequest{
			Model:    "invalid/model",
			Messages: []Message{UserMessage("Hi")},
		})

		for range chunks {
		}

		err := <-errs
		if err == nil {
			t.Fatal("expected error for invalid model")
		}
	})
}

func TestOpenRouterProvider_StreamWithUsage(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/event-stream")
		w.WriteHeader(http.StatusOK)

		chunks := []string{
			`data: {"id":"chatcmpl-1","object":"chat.completion.chunk","model":"gpt-4o","choices":[{"index":0,"delta":{"content":"Multi"},"finish_reason":null}]}`,
			`data: {"id":"chatcmpl-1","object":"chat.completion.chunk","model":"gpt-4o","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}`,
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

	provider, _ := NewOpenRouterProvider(
		WithOpenRouterAPIKey("test-key"),
		WithOpenRouterBaseURL(server.URL),
	)

	chunks, usageCh := provider.StreamWithUsage(context.Background(), &ChatRequest{
		Model:    "gpt-4o",
		Messages: []Message{UserMessage("Hi")},
	})

	var content string
	for chunk := range chunks {
		content += chunk.Delta
	}

	<-usageCh

	if content != "Multi" {
		t.Errorf("expected 'Multi', got '%s'", content)
	}
}

func TestOpenRouterProvider_GetCapabilities(t *testing.T) {
	t.Setenv("OPENROUTER_API_KEY", "test-key")
	provider, _ := NewOpenRouterProvider()

	caps := provider.GetCapabilities()
	if !caps.SupportsStreaming {
		t.Error("expected streaming support")
	}
	// OpenRouter supports many providers, so tools might be available
	if !caps.SupportsVision {
		t.Error("expected vision support for OpenRouter (model-dependent)")
	}
}

func TestOpenRouterProvider_FreshSession(t *testing.T) {
	t.Setenv("OPENROUTER_API_KEY", "test-key")
	provider, _ := NewOpenRouterProvider()

	fresh, err := provider.FreshSession()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if fresh != provider {
		t.Error("expected FreshSession to return same provider (stateless)")
	}
}
