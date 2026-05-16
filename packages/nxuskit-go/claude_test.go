package nxuskit

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"
)

// TestClaudeProvider_Constructor tests NewClaudeProvider and options
func TestClaudeProvider_Constructor(t *testing.T) {
	t.Run("requires API key", func(t *testing.T) {
		t.Setenv("ANTHROPIC_API_KEY", "")

		_, err := NewClaudeProvider()
		if err == nil {
			t.Error("expected error when no API key provided")
		}
	})

	t.Run("uses API key from environment", func(t *testing.T) {
		t.Setenv("ANTHROPIC_API_KEY", "test-key-from-env")

		provider, err := NewClaudeProvider()
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if provider == nil {
			t.Fatal("expected provider to be created")
		}
	})

	t.Run("option overrides environment", func(t *testing.T) {
		t.Setenv("ANTHROPIC_API_KEY", "env-key")

		provider, err := NewClaudeProvider(WithClaudeAPIKey("option-key"))
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if provider == nil {
			t.Fatal("expected provider to be created")
		}
	})

	t.Run("custom base URL", func(t *testing.T) {
		t.Setenv("ANTHROPIC_API_KEY", "test-key")

		provider, err := NewClaudeProvider(WithClaudeBaseURL("https://custom.api.com"))
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if provider == nil {
			t.Fatal("expected provider to be created")
		}
	})

	t.Run("rejects empty base URL", func(t *testing.T) {
		t.Setenv("ANTHROPIC_API_KEY", "test-key")

		_, err := NewClaudeProvider(WithClaudeBaseURL(""))
		if err == nil {
			t.Error("expected error for empty base URL")
		}
	})

	t.Run("custom timeout", func(t *testing.T) {
		t.Setenv("ANTHROPIC_API_KEY", "test-key")

		provider, err := NewClaudeProvider(WithClaudeTimeout(5 * time.Minute))
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if provider == nil {
			t.Fatal("expected provider to be created")
		}
	})

	t.Run("custom API version", func(t *testing.T) {
		t.Setenv("ANTHROPIC_API_KEY", "test-key")

		provider, err := NewClaudeProvider(WithClaudeVersion("2024-01-01"))
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if provider == nil {
			t.Fatal("expected provider to be created")
		}
	})

	t.Run("rejects empty API key option", func(t *testing.T) {
		t.Setenv("ANTHROPIC_API_KEY", "")

		_, err := NewClaudeProvider(WithClaudeAPIKey(""))
		if err == nil {
			t.Error("expected error for empty API key")
		}
	})
}

func TestClaudeProvider_ProviderName(t *testing.T) {
	t.Setenv("ANTHROPIC_API_KEY", "test-key")

	provider, err := NewClaudeProvider()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if provider.ProviderName() != "claude" {
		t.Errorf("expected 'claude', got '%s'", provider.ProviderName())
	}
}

func TestClaudeProvider_Chat(t *testing.T) {
	t.Run("successful chat request", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			// Verify request headers
			if r.Header.Get("x-api-key") != "test-key" {
				t.Errorf("expected x-api-key header, got '%s'", r.Header.Get("x-api-key"))
			}
			if r.Header.Get("anthropic-version") == "" {
				t.Error("expected anthropic-version header")
			}
			if r.Header.Get("Content-Type") != "application/json" {
				t.Errorf("expected Content-Type header, got '%s'", r.Header.Get("Content-Type"))
			}

			// Verify request path
			if r.URL.Path != "/messages" {
				t.Errorf("expected path /messages, got %s", r.URL.Path)
			}

			// Parse request body
			var req claudeMessagesRequest
			if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
				t.Fatalf("failed to decode request: %v", err)
			}

			if req.Model != "claude-sonnet-4-20250514" {
				t.Errorf("expected model 'claude-sonnet-4-20250514', got '%s'", req.Model)
			}

			// Send response
			resp := claudeMessagesResponse{
				ID:    "msg-123",
				Type:  "message",
				Role:  "assistant",
				Model: "claude-sonnet-4-20250514",
				Content: []claudeContentBlock{
					{Type: "text", Text: "Hello! How can I help you?"},
				},
				StopReason: "end_turn",
				Usage: claudeUsage{
					InputTokens:  10,
					OutputTokens: 8,
				},
			}
			_ = json.NewEncoder(w).Encode(resp)
		}))
		defer server.Close()

		provider, err := NewClaudeProvider(
			WithClaudeAPIKey("test-key"),
			WithClaudeBaseURL(server.URL),
		)
		if err != nil {
			t.Fatalf("failed to create provider: %v", err)
		}

		req := &ChatRequest{
			Model: "claude-sonnet-4-20250514",
			Messages: []Message{
				UserMessage("Hello"),
			},
		}

		resp, err := provider.Chat(context.Background(), req)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}

		if resp.Content != "Hello! How can I help you?" {
			t.Errorf("expected content, got '%s'", resp.Content)
		}
		if resp.Model != "claude-sonnet-4-20250514" {
			t.Errorf("expected model 'claude-sonnet-4-20250514', got '%s'", resp.Model)
		}
		if resp.FinishReason == nil || *resp.FinishReason != FinishReasonStop {
			t.Error("expected FinishReasonStop")
		}
		if resp.Usage.Actual == nil {
			t.Fatal("expected Usage.Actual")
		}
		if resp.Usage.Actual.PromptTokens != 10 {
			t.Errorf("expected 10 prompt tokens, got %d", resp.Usage.Actual.PromptTokens)
		}
		if resp.Usage.Actual.CompletionTokens != 8 {
			t.Errorf("expected 8 completion tokens, got %d", resp.Usage.Actual.CompletionTokens)
		}
	})

	t.Run("with system prompt extraction", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			var req claudeMessagesRequest
			_ = json.NewDecoder(r.Body).Decode(&req)

			// Verify system prompt is extracted
			if req.System != "You are a helpful assistant." {
				t.Errorf("expected system prompt, got '%s'", req.System)
			}

			// Verify only user message is in messages array
			if len(req.Messages) != 1 {
				t.Errorf("expected 1 message, got %d", len(req.Messages))
			}

			resp := claudeMessagesResponse{
				Content:    []claudeContentBlock{{Type: "text", Text: "OK"}},
				StopReason: "end_turn",
			}
			_ = json.NewEncoder(w).Encode(resp)
		}))
		defer server.Close()

		provider, _ := NewClaudeProvider(
			WithClaudeAPIKey("test-key"),
			WithClaudeBaseURL(server.URL),
		)

		req := &ChatRequest{
			Model: "claude-sonnet-4-20250514",
			Messages: []Message{
				SystemMessage("You are a helpful assistant."),
				UserMessage("Hello"),
			},
		}

		_, err := provider.Chat(context.Background(), req)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
	})

	t.Run("with thinking mode enabled", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			var req claudeMessagesRequest
			_ = json.NewDecoder(r.Body).Decode(&req)

			// Verify thinking is enabled
			if req.Thinking == nil {
				t.Fatal("expected thinking to be set")
			}
			if req.Thinking.Type != "enabled" {
				t.Errorf("expected thinking type 'enabled', got '%s'", req.Thinking.Type)
			}
			if req.Thinking.BudgetTokens == 0 {
				t.Error("expected budget_tokens to be set")
			}

			resp := claudeMessagesResponse{
				Content: []claudeContentBlock{
					{Type: "thinking", Thinking: "Let me think about this..."},
					{Type: "text", Text: "The answer is 42."},
				},
				StopReason: "end_turn",
			}
			_ = json.NewEncoder(w).Encode(resp)
		}))
		defer server.Close()

		provider, _ := NewClaudeProvider(
			WithClaudeAPIKey("test-key"),
			WithClaudeBaseURL(server.URL),
		)

		req := &ChatRequest{
			Model:        "claude-sonnet-4-20250514",
			Messages:     []Message{UserMessage("What is the meaning of life?")},
			ThinkingMode: ThinkingModeEnabled,
		}

		resp, err := provider.Chat(context.Background(), req)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}

		if resp.Content != "The answer is 42." {
			t.Errorf("expected content, got '%s'", resp.Content)
		}
		if resp.Thinking == nil {
			t.Fatal("expected thinking to be set")
		}
		if *resp.Thinking != "Let me think about this..." {
			t.Errorf("expected thinking content, got '%s'", *resp.Thinking)
		}
	})

	t.Run("default max_tokens is set", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			var req claudeMessagesRequest
			_ = json.NewDecoder(r.Body).Decode(&req)

			if req.MaxTokens != 4096 {
				t.Errorf("expected default max_tokens 4096, got %d", req.MaxTokens)
			}

			resp := claudeMessagesResponse{
				Content: []claudeContentBlock{{Type: "text", Text: "OK"}},
			}
			_ = json.NewEncoder(w).Encode(resp)
		}))
		defer server.Close()

		provider, _ := NewClaudeProvider(
			WithClaudeAPIKey("test-key"),
			WithClaudeBaseURL(server.URL),
		)

		_, err := provider.Chat(context.Background(), &ChatRequest{
			Model:    "claude-sonnet-4-20250514",
			Messages: []Message{UserMessage("Hi")},
		})
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
	})
}

func TestClaudeProvider_ErrorHandling(t *testing.T) {
	t.Run("authentication error", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.WriteHeader(http.StatusUnauthorized)
			_ = json.NewEncoder(w).Encode(claudeErrorResponse{
				Type: "error",
				Error: claudeError{
					Type:    "authentication_error",
					Message: "Invalid API key",
				},
			})
		}))
		defer server.Close()

		provider, _ := NewClaudeProvider(
			WithClaudeAPIKey("invalid-key"),
			WithClaudeBaseURL(server.URL),
		)

		_, err := provider.Chat(context.Background(), &ChatRequest{
			Model:    "claude-sonnet-4-20250514",
			Messages: []Message{UserMessage("Hi")},
		})

		if err == nil {
			t.Fatal("expected error")
		}
		llmErr, ok := err.(*LLMError)
		if !ok {
			t.Fatalf("expected *LLMError, got %T", err)
		}
		if llmErr.Kind != ErrorKindAuthentication {
			t.Errorf("expected ErrorKindAuthentication, got %v", llmErr.Kind)
		}
	})

	t.Run("rate limit error", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.WriteHeader(http.StatusTooManyRequests)
			_ = json.NewEncoder(w).Encode(claudeErrorResponse{
				Error: claudeError{Message: "Rate limit exceeded"},
			})
		}))
		defer server.Close()

		provider, _ := NewClaudeProvider(
			WithClaudeAPIKey("test-key"),
			WithClaudeBaseURL(server.URL),
		)

		_, err := provider.Chat(context.Background(), &ChatRequest{
			Model:    "claude-sonnet-4-20250514",
			Messages: []Message{UserMessage("Hi")},
		})

		if err == nil {
			t.Fatal("expected error")
		}
		llmErr, ok := err.(*LLMError)
		if !ok {
			t.Fatalf("expected *LLMError, got %T", err)
		}
		if llmErr.Kind != ErrorKindRateLimit {
			t.Errorf("expected ErrorKindRateLimit, got %v", llmErr.Kind)
		}
	})

	t.Run("invalid request error", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.WriteHeader(http.StatusBadRequest)
			_ = json.NewEncoder(w).Encode(claudeErrorResponse{
				Error: claudeError{Message: "Invalid model"},
			})
		}))
		defer server.Close()

		provider, _ := NewClaudeProvider(
			WithClaudeAPIKey("test-key"),
			WithClaudeBaseURL(server.URL),
		)

		_, err := provider.Chat(context.Background(), &ChatRequest{
			Model:    "invalid-model",
			Messages: []Message{UserMessage("Hi")},
		})

		if err == nil {
			t.Fatal("expected error")
		}
		llmErr, ok := err.(*LLMError)
		if !ok {
			t.Fatalf("expected *LLMError, got %T", err)
		}
		if llmErr.Kind != ErrorKindInvalidRequest {
			t.Errorf("expected ErrorKindInvalidRequest, got %v", llmErr.Kind)
		}
	})
}

func TestClaudeProvider_ListModels(t *testing.T) {
	t.Setenv("ANTHROPIC_API_KEY", "test-key")

	provider, err := NewClaudeProvider()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	models, err := provider.ListModels(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	// Should return hardcoded list of Claude models
	if len(models) == 0 {
		t.Error("expected at least one model")
	}

	// Verify known models are present
	hasOpus := false
	hasSonnet := false
	for _, m := range models {
		if m.Name == "claude-opus-4-20250514" {
			hasOpus = true
		}
		if m.Name == "claude-sonnet-4-20250514" {
			hasSonnet = true
		}
	}
	if !hasOpus {
		t.Error("expected claude-opus-4-20250514 in models")
	}
	if !hasSonnet {
		t.Error("expected claude-sonnet-4-20250514 in models")
	}
}

func TestClaudeProvider_Ping(t *testing.T) {
	t.Run("success", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			// Verify it uses the messages endpoint with a minimal request
			if r.URL.Path != "/messages" {
				t.Errorf("expected path /messages, got %s", r.URL.Path)
			}

			resp := claudeMessagesResponse{
				Content: []claudeContentBlock{{Type: "text", Text: "pong"}},
			}
			_ = json.NewEncoder(w).Encode(resp)
		}))
		defer server.Close()

		provider, _ := NewClaudeProvider(
			WithClaudeAPIKey("test-key"),
			WithClaudeBaseURL(server.URL),
		)

		err := provider.Ping(context.Background())
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
	})

	t.Run("failure", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.WriteHeader(http.StatusUnauthorized)
		}))
		defer server.Close()

		provider, _ := NewClaudeProvider(
			WithClaudeAPIKey("bad-key"),
			WithClaudeBaseURL(server.URL),
		)

		err := provider.Ping(context.Background())
		if err == nil {
			t.Error("expected error")
		}
	})
}

func TestClaudeProvider_ChatStream(t *testing.T) {
	t.Run("successful stream", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			var req claudeMessagesRequest
			_ = json.NewDecoder(r.Body).Decode(&req)
			if !req.Stream {
				t.Error("expected stream=true")
			}

			w.Header().Set("Content-Type", "text/event-stream")
			flusher, _ := w.(http.Flusher)

			// Send Claude SSE events
			events := []string{
				`event: message_start
data: {"type":"message_start","message":{"id":"msg-123","type":"message","role":"assistant","model":"claude-sonnet-4-20250514","content":[]}}`,
				`event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}`,
				`event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}`,
				`event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" there"}}`,
				`event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"!"}}`,
				`event: content_block_stop
data: {"type":"content_block_stop","index":0}`,
				`event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":3}}`,
				`event: message_stop
data: {"type":"message_stop"}`,
			}

			for _, event := range events {
				_, _ = w.Write([]byte(event + "\n\n"))
				flusher.Flush()
			}
		}))
		defer server.Close()

		provider, _ := NewClaudeProvider(
			WithClaudeAPIKey("test-key"),
			WithClaudeBaseURL(server.URL),
		)

		chunks, errs := provider.ChatStream(context.Background(), &ChatRequest{
			Model:    "claude-sonnet-4-20250514",
			Messages: []Message{UserMessage("Hi")},
		})

		var content string
		var finalChunk StreamChunk
		for chunk := range chunks {
			content += chunk.Delta
			finalChunk = chunk
		}

		if err := <-errs; err != nil {
			t.Fatalf("unexpected error: %v", err)
		}

		if content != "Hello there!" {
			t.Errorf("expected 'Hello there!', got '%s'", content)
		}
		if finalChunk.FinishReason == nil || *finalChunk.FinishReason != FinishReasonStop {
			t.Error("expected final chunk with FinishReasonStop")
		}
	})

	t.Run("stream with thinking", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.Header().Set("Content-Type", "text/event-stream")
			flusher, _ := w.(http.Flusher)

			events := []string{
				`event: message_start
data: {"type":"message_start","message":{"id":"msg-123"}}`,
				`event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}`,
				`event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"Let me think..."}}`,
				`event: content_block_stop
data: {"type":"content_block_stop","index":0}`,
				`event: content_block_start
data: {"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}`,
				`event: content_block_delta
data: {"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Answer"}}`,
				`event: content_block_stop
data: {"type":"content_block_stop","index":1}`,
				`event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn"}}`,
				`event: message_stop
data: {"type":"message_stop"}`,
			}

			for _, event := range events {
				_, _ = w.Write([]byte(event + "\n\n"))
				flusher.Flush()
			}
		}))
		defer server.Close()

		provider, _ := NewClaudeProvider(
			WithClaudeAPIKey("test-key"),
			WithClaudeBaseURL(server.URL),
		)

		chunks, errs := provider.ChatStream(context.Background(), &ChatRequest{
			Model:        "claude-sonnet-4-20250514",
			Messages:     []Message{UserMessage("Think about this")},
			ThinkingMode: ThinkingModeEnabled,
		})

		var content string
		var thinking string
		for chunk := range chunks {
			content += chunk.Delta
			if chunk.Thinking != nil {
				thinking += *chunk.Thinking
			}
		}

		if err := <-errs; err != nil {
			t.Fatalf("unexpected error: %v", err)
		}

		if content != "Answer" {
			t.Errorf("expected 'Answer', got '%s'", content)
		}
		if thinking != "Let me think..." {
			t.Errorf("expected thinking content, got '%s'", thinking)
		}
	})

	t.Run("context cancellation", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.Header().Set("Content-Type", "text/event-stream")
			time.Sleep(5 * time.Second)
		}))
		defer server.Close()

		provider, _ := NewClaudeProvider(
			WithClaudeAPIKey("test-key"),
			WithClaudeBaseURL(server.URL),
		)

		ctx, cancel := context.WithTimeout(context.Background(), 100*time.Millisecond)
		defer cancel()

		chunks, errs := provider.ChatStream(ctx, &ChatRequest{
			Model:    "claude-sonnet-4-20250514",
			Messages: []Message{UserMessage("Hi")},
		})

		for range chunks {
		}

		err := <-errs
		if err == nil {
			t.Error("expected error on context cancellation")
		}
	})
}

func TestClaudeProvider_Vision(t *testing.T) {
	t.Run("with base64 image", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			var req claudeMessagesRequest
			_ = json.NewDecoder(r.Body).Decode(&req)

			if len(req.Messages) != 1 {
				t.Fatalf("expected 1 message, got %d", len(req.Messages))
			}

			msg := req.Messages[0]
			content, ok := msg.Content.([]any)
			if !ok {
				t.Fatalf("expected []any content, got %T", msg.Content)
			}

			if len(content) != 2 {
				t.Fatalf("expected 2 content parts, got %d", len(content))
			}

			// Check text part
			textPart := content[0].(map[string]any)
			if textPart["type"] != "text" {
				t.Errorf("expected type 'text', got '%v'", textPart["type"])
			}

			// Check image part
			imgPart := content[1].(map[string]any)
			if imgPart["type"] != "image" {
				t.Errorf("expected type 'image', got '%v'", imgPart["type"])
			}

			resp := claudeMessagesResponse{
				Content: []claudeContentBlock{{Type: "text", Text: "I see an image"}},
			}
			_ = json.NewEncoder(w).Encode(resp)
		}))
		defer server.Close()

		provider, _ := NewClaudeProvider(
			WithClaudeAPIKey("test-key"),
			WithClaudeBaseURL(server.URL),
		)

		msg := UserMessage("What's in this image?").
			WithImageBase64("aGVsbG8=", "image/png")

		resp, err := provider.Chat(context.Background(), &ChatRequest{
			Model:    "claude-sonnet-4-20250514",
			Messages: []Message{msg},
		})
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if resp.Content != "I see an image" {
			t.Errorf("unexpected content: %s", resp.Content)
		}
	})

	t.Run("with URL image", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			var req claudeMessagesRequest
			_ = json.NewDecoder(r.Body).Decode(&req)

			msg := req.Messages[0]
			content := msg.Content.([]any)
			imgPart := content[1].(map[string]any)
			source := imgPart["source"].(map[string]any)

			if source["type"] != "url" {
				t.Errorf("expected source type 'url', got '%v'", source["type"])
			}
			if source["url"] != "https://example.com/image.png" {
				t.Errorf("expected URL, got '%v'", source["url"])
			}

			resp := claudeMessagesResponse{
				Content: []claudeContentBlock{{Type: "text", Text: "OK"}},
			}
			_ = json.NewEncoder(w).Encode(resp)
		}))
		defer server.Close()

		provider, _ := NewClaudeProvider(
			WithClaudeAPIKey("test-key"),
			WithClaudeBaseURL(server.URL),
		)

		msg := UserMessage("Describe").
			WithImageURL("https://example.com/image.png")

		_, err := provider.Chat(context.Background(), &ChatRequest{
			Model:    "claude-sonnet-4-20250514",
			Messages: []Message{msg},
		})
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
	})
}

func TestClaudeProvider_GetCapabilities(t *testing.T) {
	provider, err := NewClaudeProvider(WithClaudeAPIKey("test-key"))
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	caps := provider.GetCapabilities()

	// Claude supports system messages (via system parameter)
	if !caps.SupportsSystemMessages {
		t.Error("expected Claude to support system messages")
	}

	// Claude supports streaming
	if !caps.SupportsStreaming {
		t.Error("expected Claude to support streaming")
	}

	// Claude supports vision
	if !caps.SupportsVision {
		t.Error("expected Claude to support vision")
	}

	// Claude supports 8192 max stop sequences
	if caps.MaxStopSequences == nil || *caps.MaxStopSequences != 8192 {
		t.Errorf("expected MaxStopSequences to be 8192 for Claude, got %v", caps.MaxStopSequences)
	}

	// Claude does NOT support presence/frequency penalties
	if caps.SupportsPresencePenalty {
		t.Error("expected Claude to NOT support presence penalty")
	}
	if caps.SupportsFrequencyPenalty {
		t.Error("expected Claude to NOT support frequency penalty")
	}

	// Claude does NOT support seed
	if caps.SupportsSeed {
		t.Error("expected Claude to NOT support seed")
	}

	// Claude does NOT support logprobs
	if caps.SupportsLogprobs {
		t.Error("expected Claude to NOT support logprobs")
	}

	// Claude does NOT support JSON mode natively (uses prompting)
	if caps.SupportsJSONMode {
		t.Error("expected Claude to NOT support native JSON mode")
	}
}
