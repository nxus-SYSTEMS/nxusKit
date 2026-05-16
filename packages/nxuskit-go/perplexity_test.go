package nxuskit

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"
)

func TestPerplexityProvider_Constructor(t *testing.T) {
	t.Run("requires API key", func(t *testing.T) {
		t.Setenv("PERPLEXITY_API_KEY", "")
		_, err := NewPerplexityProvider()
		if err == nil {
			t.Error("expected error when no API key provided")
		}
	})

	t.Run("uses API key from environment", func(t *testing.T) {
		t.Setenv("PERPLEXITY_API_KEY", "test-key")
		provider, err := NewPerplexityProvider()
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if provider == nil {
			t.Fatal("expected provider to be created")
		}
	})

	t.Run("option overrides environment", func(t *testing.T) {
		t.Setenv("PERPLEXITY_API_KEY", "env-key")
		provider, err := NewPerplexityProvider(WithPerplexityAPIKey("option-key"))
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if provider == nil {
			t.Fatal("expected provider to be created")
		}
	})

	t.Run("custom base URL", func(t *testing.T) {
		t.Setenv("PERPLEXITY_API_KEY", "test-key")
		provider, err := NewPerplexityProvider(WithPerplexityBaseURL("https://custom.api.com"))
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if provider == nil {
			t.Fatal("expected provider to be created")
		}
	})

	t.Run("custom timeout", func(t *testing.T) {
		t.Setenv("PERPLEXITY_API_KEY", "test-key")
		provider, err := NewPerplexityProvider(WithPerplexityTimeout(5 * time.Minute))
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if provider == nil {
			t.Fatal("expected provider to be created")
		}
	})
}

func TestPerplexityProvider_ProviderName(t *testing.T) {
	t.Setenv("PERPLEXITY_API_KEY", "test-key")
	provider, _ := NewPerplexityProvider()
	if provider.ProviderName() != "perplexity" {
		t.Errorf("expected 'perplexity', got '%s'", provider.ProviderName())
	}
}

func TestPerplexityProvider_Chat(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("Authorization") != "Bearer test-key" {
			t.Errorf("expected Authorization header, got '%s'", r.Header.Get("Authorization"))
		}
		if r.URL.Path != "/chat/completions" {
			t.Errorf("expected path /chat/completions, got %s", r.URL.Path)
		}

		resp := openaiChatResponse{
			Model:   "llama-3.1-sonar-small-128k-online",
			Choices: []openaiChoice{{Message: &openaiMessage{Content: "Based on my search..."}, FinishReason: strPtr("stop")}},
			Usage:   &openaiUsage{PromptTokens: 5, CompletionTokens: 10},
		}
		_ = json.NewEncoder(w).Encode(resp)
	}))
	defer server.Close()

	provider, _ := NewPerplexityProvider(
		WithPerplexityAPIKey("test-key"),
		WithPerplexityBaseURL(server.URL),
	)

	resp, err := provider.Chat(context.Background(), &ChatRequest{
		Model:    "llama-3.1-sonar-small-128k-online",
		Messages: []Message{UserMessage("What's the latest news?")},
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if resp.Content != "Based on my search..." {
		t.Errorf("expected content, got '%s'", resp.Content)
	}
}

func TestPerplexityProvider_ListModels(t *testing.T) {
	t.Setenv("PERPLEXITY_API_KEY", "test-key")

	provider, _ := NewPerplexityProvider()

	models, err := provider.ListModels(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	// Should return hardcoded list
	if len(models) == 0 {
		t.Error("expected at least one model")
	}

	// Verify known models
	hasModel := false
	for _, m := range models {
		if m.Name == "llama-3.1-sonar-small-128k-online" {
			hasModel = true
			break
		}
	}
	if !hasModel {
		t.Error("expected llama-3.1-sonar-small-128k-online in models")
	}
}

func TestPerplexityProvider_Ping(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		// Verify minimal request
		var req openaiChatRequest
		_ = json.NewDecoder(r.Body).Decode(&req)

		if req.MaxTokens == nil || *req.MaxTokens != 1 {
			t.Error("expected max_tokens=1 for ping")
		}

		resp := openaiChatResponse{
			Choices: []openaiChoice{{Message: &openaiMessage{Content: "pong"}}},
		}
		_ = json.NewEncoder(w).Encode(resp)
	}))
	defer server.Close()

	provider, _ := NewPerplexityProvider(
		WithPerplexityAPIKey("test-key"),
		WithPerplexityBaseURL(server.URL),
	)

	err := provider.Ping(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestPerplexityProvider_ChatStream(t *testing.T) {
	t.Run("streams content successfully", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.Header().Set("Content-Type", "text/event-stream")
			w.WriteHeader(http.StatusOK)

			chunks := []string{
				`data: {"id":"chatcmpl-1","object":"chat.completion.chunk","model":"sonar","choices":[{"index":0,"delta":{"content":"Based on"},"finish_reason":null}]}`,
				`data: {"id":"chatcmpl-1","object":"chat.completion.chunk","model":"sonar","choices":[{"index":0,"delta":{"content":" my search"},"finish_reason":null}]}`,
				`data: {"id":"chatcmpl-1","object":"chat.completion.chunk","model":"sonar","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}`,
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

		provider, _ := NewPerplexityProvider(
			WithPerplexityAPIKey("test-key"),
			WithPerplexityBaseURL(server.URL),
		)

		chunks, errs := provider.ChatStream(context.Background(), &ChatRequest{
			Model:    "sonar",
			Messages: []Message{UserMessage("What's the news?")},
		})

		var content string
		for chunk := range chunks {
			content += chunk.Delta
		}

		if err := <-errs; err != nil {
			t.Fatalf("unexpected error: %v", err)
		}

		if content != "Based on my search" {
			t.Errorf("expected 'Based on my search', got '%s'", content)
		}
	})

	t.Run("handles server error", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.WriteHeader(http.StatusServiceUnavailable)
			_, _ = w.Write([]byte(`{"error":{"message":"Service temporarily unavailable"}}`))
		}))
		defer server.Close()

		provider, _ := NewPerplexityProvider(
			WithPerplexityAPIKey("test-key"),
			WithPerplexityBaseURL(server.URL),
		)

		chunks, errs := provider.ChatStream(context.Background(), &ChatRequest{
			Model:    "sonar",
			Messages: []Message{UserMessage("Hi")},
		})

		for range chunks {
		}

		err := <-errs
		if err == nil {
			t.Fatal("expected error for server error")
		}
	})
}

func TestPerplexityProvider_StreamWithUsage(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/event-stream")
		w.WriteHeader(http.StatusOK)

		chunks := []string{
			`data: {"id":"chatcmpl-1","object":"chat.completion.chunk","model":"sonar","choices":[{"index":0,"delta":{"content":"Done"},"finish_reason":null}]}`,
			`data: {"id":"chatcmpl-1","object":"chat.completion.chunk","model":"sonar","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}`,
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

	provider, _ := NewPerplexityProvider(
		WithPerplexityAPIKey("test-key"),
		WithPerplexityBaseURL(server.URL),
	)

	chunks, usageCh := provider.StreamWithUsage(context.Background(), &ChatRequest{
		Model:    "sonar",
		Messages: []Message{UserMessage("Hi")},
	})

	var content string
	for chunk := range chunks {
		content += chunk.Delta
	}

	<-usageCh

	if content != "Done" {
		t.Errorf("expected 'Done', got '%s'", content)
	}
}

func TestPerplexityProvider_GetCapabilities(t *testing.T) {
	t.Setenv("PERPLEXITY_API_KEY", "test-key")
	provider, _ := NewPerplexityProvider()

	caps := provider.GetCapabilities()
	if !caps.SupportsStreaming {
		t.Error("expected streaming support")
	}
	// Perplexity doesn't support tools
	if caps.SupportsTools {
		t.Error("expected no tool support for Perplexity")
	}
}

func TestPerplexityProvider_FreshSession(t *testing.T) {
	t.Setenv("PERPLEXITY_API_KEY", "test-key")
	provider, _ := NewPerplexityProvider()

	fresh, err := provider.FreshSession()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if fresh != provider {
		t.Error("expected FreshSession to return same provider (stateless)")
	}
}
