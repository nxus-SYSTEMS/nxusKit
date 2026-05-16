package nxuskit

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"
)

// TestOpenAIProvider_Constructor tests NewOpenAIProvider and options
func TestOpenAIProvider_Constructor(t *testing.T) {
	t.Run("requires API key", func(t *testing.T) {
		// Clear environment variable
		t.Setenv("OPENAI_API_KEY", "")

		_, err := NewOpenAIProvider()
		if err == nil {
			t.Error("expected error when no API key provided")
		}
	})

	t.Run("uses API key from environment", func(t *testing.T) {
		t.Setenv("OPENAI_API_KEY", "test-key-from-env")

		provider, err := NewOpenAIProvider()
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if provider == nil {
			t.Fatal("expected provider to be created")
		}
	})

	t.Run("option overrides environment", func(t *testing.T) {
		t.Setenv("OPENAI_API_KEY", "env-key")

		provider, err := NewOpenAIProvider(WithOpenAIAPIKey("option-key"))
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if provider == nil {
			t.Fatal("expected provider to be created")
		}
	})

	t.Run("custom base URL", func(t *testing.T) {
		t.Setenv("OPENAI_API_KEY", "test-key")

		provider, err := NewOpenAIProvider(WithOpenAIBaseURL("https://custom.api.com"))
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if provider == nil {
			t.Fatal("expected provider to be created")
		}
	})

	t.Run("rejects empty base URL", func(t *testing.T) {
		t.Setenv("OPENAI_API_KEY", "test-key")

		_, err := NewOpenAIProvider(WithOpenAIBaseURL(""))
		if err == nil {
			t.Error("expected error for empty base URL")
		}
	})

	t.Run("custom timeout", func(t *testing.T) {
		t.Setenv("OPENAI_API_KEY", "test-key")

		provider, err := NewOpenAIProvider(WithOpenAITimeout(5 * time.Minute))
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if provider == nil {
			t.Fatal("expected provider to be created")
		}
	})

	t.Run("rejects zero timeout", func(t *testing.T) {
		t.Setenv("OPENAI_API_KEY", "test-key")

		_, err := NewOpenAIProvider(WithOpenAITimeout(0))
		if err == nil {
			t.Error("expected error for zero timeout")
		}
	})

	t.Run("rejects negative timeout", func(t *testing.T) {
		t.Setenv("OPENAI_API_KEY", "test-key")

		_, err := NewOpenAIProvider(WithOpenAITimeout(-1 * time.Second))
		if err == nil {
			t.Error("expected error for negative timeout")
		}
	})

	t.Run("rejects empty API key option", func(t *testing.T) {
		t.Setenv("OPENAI_API_KEY", "")

		_, err := NewOpenAIProvider(WithOpenAIAPIKey(""))
		if err == nil {
			t.Error("expected error for empty API key")
		}
	})
}

func TestOpenAIProvider_ProviderName(t *testing.T) {
	t.Setenv("OPENAI_API_KEY", "test-key")

	provider, err := NewOpenAIProvider()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if provider.ProviderName() != "openai" {
		t.Errorf("expected 'openai', got '%s'", provider.ProviderName())
	}
}

func TestOpenAIProvider_Chat(t *testing.T) {
	t.Run("successful chat request", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			// Verify request headers
			if r.Header.Get("Authorization") != "Bearer test-key" {
				t.Errorf("expected Authorization header, got '%s'", r.Header.Get("Authorization"))
			}
			if r.Header.Get("Content-Type") != "application/json" {
				t.Errorf("expected Content-Type header, got '%s'", r.Header.Get("Content-Type"))
			}

			// Verify request path
			if r.URL.Path != "/chat/completions" {
				t.Errorf("expected path /chat/completions, got %s", r.URL.Path)
			}

			// Parse request body
			var req openaiChatRequest
			if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
				t.Fatalf("failed to decode request: %v", err)
			}

			if req.Model != "gpt-4o" {
				t.Errorf("expected model 'gpt-4o', got '%s'", req.Model)
			}

			// Send response
			resp := openaiChatResponse{
				ID:    "chatcmpl-123",
				Model: "gpt-4o",
				Choices: []openaiChoice{
					{
						Index: 0,
						Message: &openaiMessage{
							Role:    "assistant",
							Content: "Hello! How can I help you?",
						},
						FinishReason: strPtr("stop"),
					},
				},
				Usage: &openaiUsage{
					PromptTokens:     10,
					CompletionTokens: 8,
					TotalTokens:      18,
				},
			}
			_ = json.NewEncoder(w).Encode(resp)
		}))
		defer server.Close()

		provider, err := NewOpenAIProvider(
			WithOpenAIAPIKey("test-key"),
			WithOpenAIBaseURL(server.URL),
		)
		if err != nil {
			t.Fatalf("failed to create provider: %v", err)
		}

		req := &ChatRequest{
			Model: "gpt-4o",
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
		if resp.Model != "gpt-4o" {
			t.Errorf("expected model 'gpt-4o', got '%s'", resp.Model)
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

	t.Run("with optional parameters", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			var req openaiChatRequest
			_ = json.NewDecoder(r.Body).Decode(&req)

			if req.Temperature == nil || *req.Temperature != 0.7 {
				t.Error("expected temperature 0.7")
			}
			if req.MaxTokens == nil || *req.MaxTokens != 100 {
				t.Error("expected max_tokens 100")
			}
			if req.TopP == nil || *req.TopP != 0.9 {
				t.Error("expected top_p 0.9")
			}
			if len(req.Stop) != 1 || req.Stop[0] != "STOP" {
				t.Error("expected stop sequence")
			}

			resp := openaiChatResponse{
				Choices: []openaiChoice{{Message: &openaiMessage{Content: "OK"}}},
			}
			_ = json.NewEncoder(w).Encode(resp)
		}))
		defer server.Close()

		provider, _ := NewOpenAIProvider(
			WithOpenAIAPIKey("test-key"),
			WithOpenAIBaseURL(server.URL),
		)

		temp := 0.7
		maxTokens := 100
		topP := 0.9
		req := &ChatRequest{
			Model:       "gpt-4o",
			Messages:    []Message{UserMessage("Hi")},
			Temperature: &temp,
			MaxTokens:   &maxTokens,
			TopP:        &topP,
			Stop:        []string{"STOP"},
		}

		_, err := provider.Chat(context.Background(), req)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
	})
}

func TestOpenAIProvider_ErrorHandling(t *testing.T) {
	t.Run("authentication error", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.WriteHeader(http.StatusUnauthorized)
			_ = json.NewEncoder(w).Encode(openaiErrorResponse{
				Error: openaiError{
					Message: "Invalid API key",
					Type:    "invalid_request_error",
					Code:    strPtr("invalid_api_key"),
				},
			})
		}))
		defer server.Close()

		provider, _ := NewOpenAIProvider(
			WithOpenAIAPIKey("invalid-key"),
			WithOpenAIBaseURL(server.URL),
		)

		_, err := provider.Chat(context.Background(), &ChatRequest{
			Model:    "gpt-4o",
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
			w.Header().Set("Retry-After", "30")
			w.WriteHeader(http.StatusTooManyRequests)
			_ = json.NewEncoder(w).Encode(openaiErrorResponse{
				Error: openaiError{Message: "Rate limit exceeded"},
			})
		}))
		defer server.Close()

		provider, _ := NewOpenAIProvider(
			WithOpenAIAPIKey("test-key"),
			WithOpenAIBaseURL(server.URL),
		)

		_, err := provider.Chat(context.Background(), &ChatRequest{
			Model:    "gpt-4o",
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
		if llmErr.RetryAfter != 30*time.Second {
			t.Errorf("expected RetryAfter 30s, got %v", llmErr.RetryAfter)
		}
	})

	t.Run("invalid request error", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.WriteHeader(http.StatusBadRequest)
			_ = json.NewEncoder(w).Encode(openaiErrorResponse{
				Error: openaiError{Message: "Invalid model"},
			})
		}))
		defer server.Close()

		provider, _ := NewOpenAIProvider(
			WithOpenAIAPIKey("test-key"),
			WithOpenAIBaseURL(server.URL),
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

func TestOpenAIProvider_ListModels(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/models" {
			t.Errorf("expected path /models, got %s", r.URL.Path)
		}
		if r.Method != http.MethodGet {
			t.Errorf("expected GET, got %s", r.Method)
		}

		resp := openaiModelsResponse{
			Data: []openaiModelInfo{
				{ID: "gpt-4o", OwnedBy: "openai"},
				{ID: "gpt-4o-mini", OwnedBy: "openai"},
			},
		}
		_ = json.NewEncoder(w).Encode(resp)
	}))
	defer server.Close()

	provider, _ := NewOpenAIProvider(
		WithOpenAIAPIKey("test-key"),
		WithOpenAIBaseURL(server.URL),
	)

	models, err := provider.ListModels(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if len(models) != 2 {
		t.Fatalf("expected 2 models, got %d", len(models))
	}
	if models[0].Name != "gpt-4o" {
		t.Errorf("expected model Name 'gpt-4o', got '%s'", models[0].Name)
	}
}

func TestOpenAIProvider_Ping(t *testing.T) {
	t.Run("success", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			if r.URL.Path != "/models" {
				t.Errorf("expected path /models, got %s", r.URL.Path)
			}
			_ = json.NewEncoder(w).Encode(openaiModelsResponse{Data: []openaiModelInfo{}})
		}))
		defer server.Close()

		provider, _ := NewOpenAIProvider(
			WithOpenAIAPIKey("test-key"),
			WithOpenAIBaseURL(server.URL),
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

		provider, _ := NewOpenAIProvider(
			WithOpenAIAPIKey("bad-key"),
			WithOpenAIBaseURL(server.URL),
		)

		err := provider.Ping(context.Background())
		if err == nil {
			t.Error("expected error")
		}
	})
}

func TestOpenAIProvider_ChatStream(t *testing.T) {
	t.Run("successful stream", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			// Verify request
			var req openaiChatRequest
			_ = json.NewDecoder(r.Body).Decode(&req)
			if !req.Stream {
				t.Error("expected stream=true")
			}
			if req.StreamOptions == nil || !req.StreamOptions.IncludeUsage {
				t.Error("expected stream_options.include_usage=true")
			}

			w.Header().Set("Content-Type", "text/event-stream")
			flusher, _ := w.(http.Flusher)

			// Send chunks
			chunks := []string{
				`{"choices":[{"delta":{"content":"Hello"}}]}`,
				`{"choices":[{"delta":{"content":" there"}}]}`,
				`{"choices":[{"delta":{"content":"!"},"finish_reason":"stop"}],"usage":{"prompt_tokens":5,"completion_tokens":3}}`,
			}

			for _, chunk := range chunks {
				_, _ = w.Write([]byte("data: " + chunk + "\n\n"))
				flusher.Flush()
			}
			_, _ = w.Write([]byte("data: [DONE]\n\n"))
			flusher.Flush()
		}))
		defer server.Close()

		provider, _ := NewOpenAIProvider(
			WithOpenAIAPIKey("test-key"),
			WithOpenAIBaseURL(server.URL),
		)

		chunks, errs := provider.ChatStream(context.Background(), &ChatRequest{
			Model:    "gpt-4o",
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
		if finalChunk.Usage == nil {
			t.Error("expected usage in final chunk")
		}
	})

	t.Run("context cancellation", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.Header().Set("Content-Type", "text/event-stream")
			// Slow response that will be canceled
			time.Sleep(5 * time.Second)
		}))
		defer server.Close()

		provider, _ := NewOpenAIProvider(
			WithOpenAIAPIKey("test-key"),
			WithOpenAIBaseURL(server.URL),
		)

		ctx, cancel := context.WithTimeout(context.Background(), 100*time.Millisecond)
		defer cancel()

		chunks, errs := provider.ChatStream(ctx, &ChatRequest{
			Model:    "gpt-4o",
			Messages: []Message{UserMessage("Hi")},
		})

		// Drain chunks
		for range chunks {
		}

		err := <-errs
		if err == nil {
			t.Error("expected error on context cancellation")
		}
	})
}

func TestOpenAIProvider_Vision(t *testing.T) {
	t.Run("with base64 image", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			var req openaiChatRequest
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
			if imgPart["type"] != "image_url" {
				t.Errorf("expected type 'image_url', got '%v'", imgPart["type"])
			}

			resp := openaiChatResponse{
				Choices: []openaiChoice{{Message: &openaiMessage{Content: "I see an image"}}},
			}
			_ = json.NewEncoder(w).Encode(resp)
		}))
		defer server.Close()

		provider, _ := NewOpenAIProvider(
			WithOpenAIAPIKey("test-key"),
			WithOpenAIBaseURL(server.URL),
		)

		msg := UserMessage("What's in this image?").
			WithImageBase64("aGVsbG8=", "image/png")

		resp, err := provider.Chat(context.Background(), &ChatRequest{
			Model:    "gpt-4o",
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
			var req openaiChatRequest
			_ = json.NewDecoder(r.Body).Decode(&req)

			msg := req.Messages[0]
			content := msg.Content.([]any)
			imgPart := content[1].(map[string]any)
			imageURL := imgPart["image_url"].(map[string]any)

			if imageURL["url"] != "https://example.com/image.png" {
				t.Errorf("expected URL, got '%v'", imageURL["url"])
			}

			resp := openaiChatResponse{
				Choices: []openaiChoice{{Message: &openaiMessage{Content: "OK"}}},
			}
			_ = json.NewEncoder(w).Encode(resp)
		}))
		defer server.Close()

		provider, _ := NewOpenAIProvider(
			WithOpenAIAPIKey("test-key"),
			WithOpenAIBaseURL(server.URL),
		)

		msg := UserMessage("Describe").
			WithImageURL("https://example.com/image.png")

		_, err := provider.Chat(context.Background(), &ChatRequest{
			Model:    "gpt-4o",
			Messages: []Message{msg},
		})
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
	})
}

func TestOpenAIProvider_GetCapabilities(t *testing.T) {
	t.Setenv("OPENAI_API_KEY", "test-key")
	provider, err := NewOpenAIProvider()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	caps := provider.GetCapabilities()

	// OpenAI supports system messages
	if !caps.SupportsSystemMessages {
		t.Error("expected OpenAI to support system messages")
	}

	// OpenAI supports streaming
	if !caps.SupportsStreaming {
		t.Error("expected OpenAI to support streaming")
	}

	// OpenAI supports vision
	if !caps.SupportsVision {
		t.Error("expected OpenAI to support vision")
	}

	// OpenAI has max stop sequences of 4
	if caps.MaxStopSequences == nil || *caps.MaxStopSequences != 4 {
		t.Errorf("expected MaxStopSequences=4, got %v", caps.MaxStopSequences)
	}

	// OpenAI supports penalties
	if !caps.SupportsPresencePenalty {
		t.Error("expected OpenAI to support presence penalty")
	}
	if !caps.SupportsFrequencyPenalty {
		t.Error("expected OpenAI to support frequency penalty")
	}

	// OpenAI supports seed
	if !caps.SupportsSeed {
		t.Error("expected OpenAI to support seed")
	}

	// OpenAI supports logprobs with max 20
	if !caps.SupportsLogprobs {
		t.Error("expected OpenAI to support logprobs")
	}
	if caps.MaxLogprobs == nil || *caps.MaxLogprobs != 20 {
		t.Errorf("expected MaxLogprobs=20, got %v", caps.MaxLogprobs)
	}

	// OpenAI supports JSON mode and schema
	if !caps.SupportsJSONMode {
		t.Error("expected OpenAI to support JSON mode")
	}
	if !caps.SupportsJSONSchema {
		t.Error("expected OpenAI to support JSON schema")
	}

	// OpenAI penalty range is -2 to 2
	if caps.PenaltyRange == nil {
		t.Fatal("expected PenaltyRange to be set")
	}
	if caps.PenaltyRange.Min != -2.0 || caps.PenaltyRange.Max != 2.0 {
		t.Errorf("expected penalty range [-2, 2], got [%f, %f]", caps.PenaltyRange.Min, caps.PenaltyRange.Max)
	}
}

func TestOpenAIProvider_RateLimitWithRetryAfter(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Retry-After", "30")
		w.WriteHeader(http.StatusTooManyRequests)
		_, _ = w.Write([]byte(`{"error": {"message": "Rate limit exceeded", "type": "rate_limit_error"}}`))
	}))
	defer server.Close()

	provider, _ := NewOpenAIProvider(
		WithOpenAIAPIKey("test-key"),
		WithOpenAIBaseURL(server.URL),
	)

	_, err := provider.Chat(context.Background(), &ChatRequest{
		Model:    "gpt-4o",
		Messages: []Message{UserMessage("Hello")},
	})

	if err == nil {
		t.Fatal("expected error for rate limit response")
	}

	var llmErr *LLMError
	if !isLLMError(err, &llmErr) {
		t.Fatalf("expected LLMError, got %T", err)
	}

	if llmErr.Kind != ErrorKindRateLimit {
		t.Errorf("expected rate limit error, got %v", llmErr.Kind)
	}

	// Check that RetryAfter was parsed
	expectedRetry := 30 * time.Second
	if llmErr.RetryAfter != expectedRetry {
		t.Errorf("expected RetryAfter=%v, got %v", expectedRetry, llmErr.RetryAfter)
	}
}

// isLLMError is a helper to check if an error is an LLMError
func isLLMError(err error, target **LLMError) bool {
	if e, ok := err.(*LLMError); ok {
		*target = e
		return true
	}
	return false
}

func TestOpenAIProvider_StreamWithUsage(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/event-stream")
		flusher, _ := w.(http.Flusher)

		chunks := []string{
			`{"choices":[{"delta":{"content":"Hi"}}]}`,
			`{"choices":[{"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":5,"completion_tokens":1}}`,
		}

		for _, chunk := range chunks {
			_, _ = w.Write([]byte("data: " + chunk + "\n\n"))
			flusher.Flush()
		}
		_, _ = w.Write([]byte("data: [DONE]\n\n"))
		flusher.Flush()
	}))
	defer server.Close()

	provider, _ := NewOpenAIProvider(
		WithOpenAIAPIKey("test-key"),
		WithOpenAIBaseURL(server.URL),
	)

	chunks, usageChan := provider.StreamWithUsage(context.Background(), &ChatRequest{
		Model:    "gpt-4o",
		Messages: []Message{UserMessage("Hi")},
	})

	var content string
	for chunk := range chunks {
		content += chunk.Delta
	}

	usage := <-usageChan

	if content != "Hi" {
		t.Errorf("expected 'Hi', got '%s'", content)
	}

	// Usage should be populated
	_ = usage
}
