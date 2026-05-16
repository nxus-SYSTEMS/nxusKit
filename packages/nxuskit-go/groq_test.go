package nxuskit

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"
)

func TestGroqProvider_Constructor(t *testing.T) {
	t.Run("requires API key", func(t *testing.T) {
		t.Setenv("GROQ_API_KEY", "")
		_, err := NewGroqProvider()
		if err == nil {
			t.Error("expected error when no API key provided")
		}
	})

	t.Run("uses API key from environment", func(t *testing.T) {
		t.Setenv("GROQ_API_KEY", "test-key")
		provider, err := NewGroqProvider()
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if provider == nil {
			t.Fatal("expected provider to be created")
		}
	})

	t.Run("option overrides environment", func(t *testing.T) {
		t.Setenv("GROQ_API_KEY", "env-key")
		provider, err := NewGroqProvider(WithGroqAPIKey("option-key"))
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if provider == nil {
			t.Fatal("expected provider to be created")
		}
	})

	t.Run("custom base URL", func(t *testing.T) {
		t.Setenv("GROQ_API_KEY", "test-key")
		provider, err := NewGroqProvider(WithGroqBaseURL("https://custom.api.com"))
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if provider == nil {
			t.Fatal("expected provider to be created")
		}
	})

	t.Run("custom timeout", func(t *testing.T) {
		t.Setenv("GROQ_API_KEY", "test-key")
		provider, err := NewGroqProvider(WithGroqTimeout(5 * time.Minute))
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if provider == nil {
			t.Fatal("expected provider to be created")
		}
	})
}

func TestGroqProvider_ProviderName(t *testing.T) {
	t.Setenv("GROQ_API_KEY", "test-key")
	provider, _ := NewGroqProvider()
	if provider.ProviderName() != "groq" {
		t.Errorf("expected 'groq', got '%s'", provider.ProviderName())
	}
}

func TestGroqProvider_Chat(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("Authorization") != "Bearer test-key" {
			t.Errorf("expected Authorization header, got '%s'", r.Header.Get("Authorization"))
		}
		if r.URL.Path != "/chat/completions" {
			t.Errorf("expected path /chat/completions, got %s", r.URL.Path)
		}

		resp := openaiChatResponse{
			Model:   "llama3-70b-8192",
			Choices: []openaiChoice{{Message: &openaiMessage{Content: "Hello!"}, FinishReason: strPtr("stop")}},
			Usage:   &openaiUsage{PromptTokens: 5, CompletionTokens: 3},
		}
		_ = json.NewEncoder(w).Encode(resp)
	}))
	defer server.Close()

	provider, _ := NewGroqProvider(
		WithGroqAPIKey("test-key"),
		WithGroqBaseURL(server.URL),
	)

	resp, err := provider.Chat(context.Background(), &ChatRequest{
		Model:    "llama3-70b-8192",
		Messages: []Message{UserMessage("Hi")},
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if resp.Content != "Hello!" {
		t.Errorf("expected content, got '%s'", resp.Content)
	}
}

func TestGroqProvider_ListModels(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/models" {
			t.Errorf("expected path /models, got %s", r.URL.Path)
		}
		resp := openaiModelsResponse{
			Data: []openaiModelInfo{{ID: "llama3-70b-8192"}},
		}
		_ = json.NewEncoder(w).Encode(resp)
	}))
	defer server.Close()

	provider, _ := NewGroqProvider(
		WithGroqAPIKey("test-key"),
		WithGroqBaseURL(server.URL),
	)

	models, err := provider.ListModels(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(models) != 1 {
		t.Fatalf("expected 1 model, got %d", len(models))
	}
}

func TestGroqProvider_Ping(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_ = json.NewEncoder(w).Encode(openaiModelsResponse{Data: []openaiModelInfo{}})
	}))
	defer server.Close()

	provider, _ := NewGroqProvider(
		WithGroqAPIKey("test-key"),
		WithGroqBaseURL(server.URL),
	)

	err := provider.Ping(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestGroqProvider_ChatStream(t *testing.T) {
	t.Run("streams content successfully", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.Header().Set("Content-Type", "text/event-stream")
			w.WriteHeader(http.StatusOK)

			chunks := []string{
				`data: {"id":"chatcmpl-1","object":"chat.completion.chunk","model":"llama3-70b-8192","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}`,
				`data: {"id":"chatcmpl-1","object":"chat.completion.chunk","model":"llama3-70b-8192","choices":[{"index":0,"delta":{"content":" from Groq"},"finish_reason":null}]}`,
				`data: {"id":"chatcmpl-1","object":"chat.completion.chunk","model":"llama3-70b-8192","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}`,
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

		provider, _ := NewGroqProvider(
			WithGroqAPIKey("test-key"),
			WithGroqBaseURL(server.URL),
		)

		chunks, errs := provider.ChatStream(context.Background(), &ChatRequest{
			Model:    "llama3-70b-8192",
			Messages: []Message{UserMessage("Hi")},
		})

		var content string
		for chunk := range chunks {
			content += chunk.Delta
		}

		if err := <-errs; err != nil {
			t.Fatalf("unexpected error: %v", err)
		}

		if content != "Hello from Groq" {
			t.Errorf("expected 'Hello from Groq', got '%s'", content)
		}
	})

	t.Run("handles rate limit error", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.Header().Set("Retry-After", "30")
			w.WriteHeader(http.StatusTooManyRequests)
			_, _ = w.Write([]byte(`{"error":{"message":"Rate limit exceeded","type":"rate_limit_error"}}`))
		}))
		defer server.Close()

		provider, _ := NewGroqProvider(
			WithGroqAPIKey("test-key"),
			WithGroqBaseURL(server.URL),
		)

		chunks, errs := provider.ChatStream(context.Background(), &ChatRequest{
			Model:    "llama3-70b-8192",
			Messages: []Message{UserMessage("Hi")},
		})

		for range chunks {
		}

		err := <-errs
		if err == nil {
			t.Fatal("expected error for rate limit response")
		}
	})
}

func TestGroqProvider_StreamWithUsage(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/event-stream")
		w.WriteHeader(http.StatusOK)

		chunks := []string{
			`data: {"id":"chatcmpl-1","object":"chat.completion.chunk","model":"llama3","choices":[{"index":0,"delta":{"content":"Fast!"},"finish_reason":null}]}`,
			`data: {"id":"chatcmpl-1","object":"chat.completion.chunk","model":"llama3","choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":1,"total_tokens":4}}`,
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

	provider, _ := NewGroqProvider(
		WithGroqAPIKey("test-key"),
		WithGroqBaseURL(server.URL),
	)

	chunks, usageCh := provider.StreamWithUsage(context.Background(), &ChatRequest{
		Model:    "llama3",
		Messages: []Message{UserMessage("Hi")},
	})

	var content string
	for chunk := range chunks {
		content += chunk.Delta
	}

	<-usageCh // Consume usage channel

	if content != "Fast!" {
		t.Errorf("expected 'Fast!', got '%s'", content)
	}
}

func TestGroqProvider_GetCapabilities(t *testing.T) {
	t.Setenv("GROQ_API_KEY", "test-key")
	provider, _ := NewGroqProvider()

	caps := provider.GetCapabilities()
	if !caps.SupportsStreaming {
		t.Error("expected streaming support")
	}
	if !caps.SupportsTools {
		t.Error("expected tool support for Groq")
	}
}

func TestGroqProvider_FreshSession(t *testing.T) {
	t.Setenv("GROQ_API_KEY", "test-key")
	provider, _ := NewGroqProvider()

	fresh, err := provider.FreshSession()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if fresh != provider {
		t.Error("expected FreshSession to return same provider (stateless)")
	}
}
