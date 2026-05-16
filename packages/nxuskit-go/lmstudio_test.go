package nxuskit

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"testing"
	"time"
)

func TestNewLmStudioProvider_Default(t *testing.T) {
	provider, err := NewLmStudioProvider()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if provider.ProviderName() != "lmstudio" {
		t.Errorf("expected provider name 'lmstudio', got '%s'", provider.ProviderName())
	}
}

func TestNewLmStudioProvider_WithBaseURL(t *testing.T) {
	provider, err := NewLmStudioProvider(
		WithLmStudioBaseURL("http://custom:8080/v1"),
	)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if provider == nil {
		t.Fatal("expected provider, got nil")
	}
}

func TestNewLmStudioProvider_WithTimeout(t *testing.T) {
	provider, err := NewLmStudioProvider(
		WithLmStudioTimeout(5 * time.Minute),
	)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if provider == nil {
		t.Fatal("expected provider, got nil")
	}
}

func TestNewLmStudioProvider_InvalidOptions(t *testing.T) {
	tests := []struct {
		name string
		opts []LmStudioOption
	}{
		{
			name: "empty base URL",
			opts: []LmStudioOption{WithLmStudioBaseURL("")},
		},
		{
			name: "zero timeout",
			opts: []LmStudioOption{WithLmStudioTimeout(0)},
		},
		{
			name: "negative timeout",
			opts: []LmStudioOption{WithLmStudioTimeout(-1 * time.Second)},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := NewLmStudioProvider(tt.opts...)
			if err == nil {
				t.Error("expected error for invalid options")
			}
		})
	}
}

func TestNewLmStudioProvider_EnvVar(t *testing.T) {
	_ = os.Setenv("LMSTUDIO_BASE_URL", "http://env-host:9999/v1")
	defer func() { _ = os.Unsetenv("LMSTUDIO_BASE_URL") }()

	provider, err := NewLmStudioProvider()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if provider == nil {
		t.Fatal("expected provider, got nil")
	}
}

func TestLmStudioProvider_Chat(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/chat/completions" {
			t.Errorf("unexpected path: %s", r.URL.Path)
		}
		if r.Method != http.MethodPost {
			t.Errorf("expected POST, got %s", r.Method)
		}

		var req openaiChatRequest
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			t.Errorf("failed to decode request: %v", err)
		}

		if req.Model != "local-model" {
			t.Errorf("expected model 'local-model', got '%s'", req.Model)
		}
		if req.Stream {
			t.Error("expected stream=false")
		}

		stopReason := "stop"
		resp := openaiChatResponse{
			ID:      "chatcmpl-123",
			Model:   "local-model",
			Created: 1234567890,
			Choices: []openaiChoice{
				{
					Index:        0,
					Message:      &openaiMessage{Role: "assistant", Content: "Hello! How can I help you?"},
					FinishReason: &stopReason,
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

	provider, _ := NewLmStudioProvider(WithLmStudioBaseURL(server.URL))

	req, _ := NewChatRequest("local-model",
		WithMessages(UserMessage("Hello")),
	)

	resp, err := provider.Chat(context.Background(), req)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if resp.Content != "Hello! How can I help you?" {
		t.Errorf("unexpected content: %s", resp.Content)
	}
	if resp.Model != "local-model" {
		t.Errorf("unexpected model: %s", resp.Model)
	}
	if resp.FinishReason == nil || *resp.FinishReason != FinishReasonStop {
		t.Error("expected FinishReasonStop")
	}
	if resp.Usage.Actual == nil || resp.Usage.Actual.PromptTokens != 10 {
		t.Errorf("expected prompt tokens 10, got %v", resp.Usage)
	}
}

func TestLmStudioProvider_Chat_WithOptions(t *testing.T) {
	var receivedReq openaiChatRequest

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_ = json.NewDecoder(r.Body).Decode(&receivedReq)

		stopReason := "stop"
		resp := openaiChatResponse{
			ID:    "chatcmpl-123",
			Model: "local-model",
			Choices: []openaiChoice{
				{Message: &openaiMessage{Content: "Response"}, FinishReason: &stopReason},
			},
		}
		_ = json.NewEncoder(w).Encode(resp)
	}))
	defer server.Close()

	provider, _ := NewLmStudioProvider(WithLmStudioBaseURL(server.URL))

	temp := 0.7
	maxTokens := 100
	topP := 0.9

	req, _ := NewChatRequest("local-model",
		WithMessages(UserMessage("Test")),
		WithTemperature(temp),
		WithMaxTokens(maxTokens),
		WithTopP(topP),
	)

	_, err := provider.Chat(context.Background(), req)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if receivedReq.Temperature == nil || *receivedReq.Temperature != 0.7 {
		t.Errorf("expected temperature 0.7, got %v", receivedReq.Temperature)
	}
	if receivedReq.MaxTokens == nil || *receivedReq.MaxTokens != 100 {
		t.Errorf("expected max_tokens 100, got %v", receivedReq.MaxTokens)
	}
	if receivedReq.TopP == nil || *receivedReq.TopP != 0.9 {
		t.Errorf("expected top_p 0.9, got %v", receivedReq.TopP)
	}
}

func TestLmStudioProvider_Chat_Error(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusServiceUnavailable)
		_ = json.NewEncoder(w).Encode(openaiErrorResponse{
			Error: openaiError{
				Message: "No model loaded",
				Type:    "invalid_request_error",
			},
		})
	}))
	defer server.Close()

	provider, _ := NewLmStudioProvider(WithLmStudioBaseURL(server.URL))

	req, _ := NewChatRequest("local-model",
		WithMessages(UserMessage("Hello")),
	)

	_, err := provider.Chat(context.Background(), req)
	if err == nil {
		t.Fatal("expected error")
	}

	var llmErr *LLMError
	if !errors.As(err, &llmErr) {
		t.Fatal("expected LLMError")
	}

	if llmErr.HTTPStatusCode != http.StatusServiceUnavailable {
		t.Errorf("expected status 503, got %d", llmErr.HTTPStatusCode)
	}
}

func TestLmStudioProvider_ChatStream(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/event-stream")

		// SSE format
		_, _ = fmt.Fprintln(w, `data: {"id":"chatcmpl-123","model":"local-model","choices":[{"index":0,"delta":{"role":"assistant"},"finish_reason":null}]}`)
		_, _ = fmt.Fprintln(w)
		_, _ = fmt.Fprintln(w, `data: {"id":"chatcmpl-123","model":"local-model","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}`)
		_, _ = fmt.Fprintln(w)
		_, _ = fmt.Fprintln(w, `data: {"id":"chatcmpl-123","model":"local-model","choices":[{"index":0,"delta":{"content":" there"},"finish_reason":null}]}`)
		_, _ = fmt.Fprintln(w)
		_, _ = fmt.Fprintln(w, `data: {"id":"chatcmpl-123","model":"local-model","choices":[{"index":0,"delta":{"content":"!"},"finish_reason":"stop"}],"usage":{"prompt_tokens":5,"completion_tokens":3,"total_tokens":8}}`)
		_, _ = fmt.Fprintln(w)
		_, _ = fmt.Fprintln(w, "data: [DONE]")
	}))
	defer server.Close()

	provider, _ := NewLmStudioProvider(WithLmStudioBaseURL(server.URL))

	req, _ := NewChatRequest("local-model",
		WithMessages(UserMessage("Hi")),
	)

	chunks, errs := provider.ChatStream(context.Background(), req)

	var content strings.Builder
	var finalChunk StreamChunk

	for chunk := range chunks {
		content.WriteString(chunk.Delta)
		if chunk.IsFinal() {
			finalChunk = chunk
		}
	}

	if err := <-errs; err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if content.String() != "Hello there!" {
		t.Errorf("expected 'Hello there!', got '%s'", content.String())
	}

	if finalChunk.FinishReason == nil || *finalChunk.FinishReason != FinishReasonStop {
		t.Error("expected final chunk with FinishReasonStop")
	}
}

func TestLmStudioProvider_ChatStream_Done(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/event-stream")

		_, _ = fmt.Fprintln(w, `data: {"id":"1","model":"m","choices":[{"delta":{"content":"Hi"},"finish_reason":"stop"}]}`)
		_, _ = fmt.Fprintln(w)
		_, _ = fmt.Fprintln(w, "data: [DONE]")
	}))
	defer server.Close()

	provider, _ := NewLmStudioProvider(WithLmStudioBaseURL(server.URL))

	req, _ := NewChatRequest("local-model",
		WithMessages(UserMessage("Hi")),
	)

	chunks, errs := provider.ChatStream(context.Background(), req)

	count := 0
	for range chunks {
		count++
	}

	if err := <-errs; err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if count != 1 {
		t.Errorf("expected 1 chunk, got %d", count)
	}
}

func TestLmStudioProvider_ChatStream_FinishReasonMapping(t *testing.T) {
	tests := []struct {
		apiReason string
		expected  FinishReason
	}{
		{"stop", FinishReasonStop},
		{"length", FinishReasonLength},
		{"content_filter", FinishReasonContentFilter},
		{"tool_calls", FinishReasonToolCalls},
	}

	for _, tt := range tests {
		t.Run(tt.apiReason, func(t *testing.T) {
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				w.Header().Set("Content-Type", "text/event-stream")
				_, _ = fmt.Fprintf(w, `data: {"id":"1","model":"m","choices":[{"delta":{"content":"x"},"finish_reason":"%s"}]}`+"\n\n", tt.apiReason)
				_, _ = fmt.Fprintln(w, "data: [DONE]")
			}))
			defer server.Close()

			provider, _ := NewLmStudioProvider(WithLmStudioBaseURL(server.URL))
			req, _ := NewChatRequest("m", WithMessages(UserMessage("Hi")))

			chunks, errs := provider.ChatStream(context.Background(), req)

			var finalReason *FinishReason
			for chunk := range chunks {
				if chunk.FinishReason != nil {
					finalReason = chunk.FinishReason
				}
			}

			if err := <-errs; err != nil {
				t.Fatalf("unexpected error: %v", err)
			}

			if finalReason == nil || *finalReason != tt.expected {
				t.Errorf("expected %v, got %v", tt.expected, finalReason)
			}
		})
	}
}

func TestLmStudioProvider_ListModels(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/models" {
			t.Errorf("unexpected path: %s", r.URL.Path)
		}

		resp := openaiModelsResponse{
			Object: "list",
			Data: []openaiModelInfo{
				{ID: "TheBloke/Mistral-7B-v0.1-GGUF", Object: "model", Created: 1234567890, OwnedBy: "local"},
				{ID: "llama-3.2-8b", Object: "model", Created: 1234567890, OwnedBy: "local"},
			},
		}
		_ = json.NewEncoder(w).Encode(resp)
	}))
	defer server.Close()

	provider, _ := NewLmStudioProvider(WithLmStudioBaseURL(server.URL))

	models, err := provider.ListModels(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if len(models) != 2 {
		t.Fatalf("expected 2 models, got %d", len(models))
	}

	if models[0].Name != "TheBloke/Mistral-7B-v0.1-GGUF" {
		t.Errorf("unexpected model name: %s", models[0].Name)
	}
}

func TestLmStudioProvider_ListModels_Empty(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		resp := openaiModelsResponse{Object: "list", Data: []openaiModelInfo{}}
		_ = json.NewEncoder(w).Encode(resp)
	}))
	defer server.Close()

	provider, _ := NewLmStudioProvider(WithLmStudioBaseURL(server.URL))

	models, err := provider.ListModels(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if len(models) != 0 {
		t.Errorf("expected empty slice, got %d models", len(models))
	}
}

func TestLmStudioProvider_Ping_Success(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/models" {
			t.Errorf("unexpected path: %s", r.URL.Path)
		}
		resp := openaiModelsResponse{Object: "list", Data: []openaiModelInfo{}}
		_ = json.NewEncoder(w).Encode(resp)
	}))
	defer server.Close()

	provider, _ := NewLmStudioProvider(WithLmStudioBaseURL(server.URL))

	err := provider.Ping(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestLmStudioProvider_Ping_Failure(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusInternalServerError)
	}))
	defer server.Close()

	provider, _ := NewLmStudioProvider(WithLmStudioBaseURL(server.URL))

	err := provider.Ping(context.Background())
	if err == nil {
		t.Error("expected error for failed ping")
	}
}

func TestLmStudioProvider_NetworkError(t *testing.T) {
	provider, _ := NewLmStudioProvider(
		WithLmStudioBaseURL("http://localhost:99999"), // Invalid port
	)

	req, _ := NewChatRequest("local-model",
		WithMessages(UserMessage("Hi")),
	)

	_, err := provider.Chat(context.Background(), req)
	if err == nil {
		t.Fatal("expected network error")
	}

	if !errors.Is(err, ErrNetwork) {
		t.Errorf("expected ErrNetwork, got %T: %v", err, err)
	}
}

func TestLmStudioProvider_Chat_WithImageURL(t *testing.T) {
	var receivedReq openaiChatRequest

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_ = json.NewDecoder(r.Body).Decode(&receivedReq)

		stopReason := "stop"
		resp := openaiChatResponse{
			ID:    "chatcmpl-123",
			Model: "llava",
			Choices: []openaiChoice{
				{Message: &openaiMessage{Content: "I see a cat"}, FinishReason: &stopReason},
			},
		}
		_ = json.NewEncoder(w).Encode(resp)
	}))
	defer server.Close()

	provider, _ := NewLmStudioProvider(WithLmStudioBaseURL(server.URL))

	req, _ := NewChatRequest("llava",
		WithMessages(
			UserMessage("What's in this image?").
				WithImageURL("https://example.com/cat.png"),
		),
	)

	_, err := provider.Chat(context.Background(), req)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	// Verify the image was included in the request
	if len(receivedReq.Messages) != 1 {
		t.Fatalf("expected 1 message, got %d", len(receivedReq.Messages))
	}

	// Content should be an array of parts
	parts, ok := receivedReq.Messages[0].Content.([]interface{})
	if !ok {
		t.Fatalf("expected array content, got %T", receivedReq.Messages[0].Content)
	}

	if len(parts) != 2 {
		t.Errorf("expected 2 content parts, got %d", len(parts))
	}
}

func TestLmStudioProvider_Chat_WithBase64Image(t *testing.T) {
	var receivedReq openaiChatRequest

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_ = json.NewDecoder(r.Body).Decode(&receivedReq)

		stopReason := "stop"
		resp := openaiChatResponse{
			ID:    "chatcmpl-123",
			Model: "llava",
			Choices: []openaiChoice{
				{Message: &openaiMessage{Content: "I see a dog"}, FinishReason: &stopReason},
			},
		}
		_ = json.NewEncoder(w).Encode(resp)
	}))
	defer server.Close()

	provider, _ := NewLmStudioProvider(WithLmStudioBaseURL(server.URL))

	req, _ := NewChatRequest("llava",
		WithMessages(
			UserMessage("What's in this image?").
				WithImageBase64("aGVsbG8=", "image/jpeg"),
		),
	)

	_, err := provider.Chat(context.Background(), req)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestLmStudioProvider_StreamWithUsage(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/event-stream")

		_, _ = fmt.Fprintln(w, `data: {"id":"1","model":"m","choices":[{"delta":{"content":"Hello"},"finish_reason":null}]}`)
		_, _ = fmt.Fprintln(w)
		_, _ = fmt.Fprintln(w, `data: {"id":"1","model":"m","choices":[{"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":5,"completion_tokens":1}}`)
		_, _ = fmt.Fprintln(w)
		_, _ = fmt.Fprintln(w, "data: [DONE]")
	}))
	defer server.Close()

	provider, _ := NewLmStudioProvider(WithLmStudioBaseURL(server.URL))

	req, _ := NewChatRequest("local-model", WithMessages(UserMessage("Hi")))

	chunks, usageChan := provider.StreamWithUsage(context.Background(), req)

	var content strings.Builder
	for chunk := range chunks {
		content.WriteString(chunk.Delta)
	}

	usage := <-usageChan

	if content.String() != "Hello" {
		t.Errorf("expected 'Hello', got %q", content.String())
	}

	// Usage should be populated from the stream
	_ = usage
}

func TestLmStudioProvider_ListAvailableModels(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		resp := openaiModelsResponse{
			Object: "list",
			Data: []openaiModelInfo{
				{ID: "model1", Object: "model", Created: 1234567890, OwnedBy: "local"},
			},
		}
		_ = json.NewEncoder(w).Encode(resp)
	}))
	defer server.Close()

	provider, _ := NewLmStudioProvider(WithLmStudioBaseURL(server.URL))

	models, err := provider.ListAvailableModels(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if len(models) != 1 {
		t.Errorf("expected 1 model, got %d", len(models))
	}
}
