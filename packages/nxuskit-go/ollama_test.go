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

func TestNewOllamaProvider_Default(t *testing.T) {
	provider, err := NewOllamaProvider()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if provider.ProviderName() != "ollama" {
		t.Errorf("expected provider name 'ollama', got '%s'", provider.ProviderName())
	}
}

func TestNewOllamaProvider_WithBaseURL(t *testing.T) {
	provider, err := NewOllamaProvider(
		WithOllamaBaseURL("http://custom:8080"),
	)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if provider == nil {
		t.Fatal("expected provider, got nil")
	}
}

func TestNewOllamaProvider_WithTimeout(t *testing.T) {
	provider, err := NewOllamaProvider(
		WithOllamaTimeout(5 * time.Minute),
	)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if provider == nil {
		t.Fatal("expected provider, got nil")
	}
}

func TestNewOllamaProvider_InvalidOptions(t *testing.T) {
	tests := []struct {
		name string
		opts []OllamaOption
	}{
		{
			name: "empty base URL",
			opts: []OllamaOption{WithOllamaBaseURL("")},
		},
		{
			name: "zero timeout",
			opts: []OllamaOption{WithOllamaTimeout(0)},
		},
		{
			name: "negative timeout",
			opts: []OllamaOption{WithOllamaTimeout(-1 * time.Second)},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := NewOllamaProvider(tt.opts...)
			if err == nil {
				t.Error("expected error for invalid options")
			}
		})
	}
}

func TestNewOllamaProvider_EnvVar(t *testing.T) {
	// Set environment variable
	_ = os.Setenv("OLLAMA_HOST", "http://env-host:9999")
	defer func() { _ = os.Unsetenv("OLLAMA_HOST") }()

	provider, err := NewOllamaProvider()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if provider == nil {
		t.Fatal("expected provider, got nil")
	}
}

func TestNewOllamaProvider_OptionOverridesEnvVar(t *testing.T) {
	_ = os.Setenv("OLLAMA_HOST", "http://env-host:9999")
	defer func() { _ = os.Unsetenv("OLLAMA_HOST") }()

	// Option should override env var
	provider, err := NewOllamaProvider(
		WithOllamaBaseURL("http://option-host:8888"),
	)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if provider == nil {
		t.Fatal("expected provider, got nil")
	}
}

func TestOllamaProvider_Chat(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/chat" {
			t.Errorf("unexpected path: %s", r.URL.Path)
		}
		if r.Method != http.MethodPost {
			t.Errorf("expected POST, got %s", r.Method)
		}

		var req ollamaChatRequest
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			t.Errorf("failed to decode request: %v", err)
		}

		if req.Model != "llama3.2" {
			t.Errorf("expected model 'llama3.2', got '%s'", req.Model)
		}
		if req.Stream {
			t.Error("expected stream=false")
		}

		resp := ollamaChatResponse{
			Model: "llama3.2",
			Message: ollamaMessage{
				Role:    "assistant",
				Content: "Hello! How can I help you?",
			},
			Done:            true,
			EvalCount:       10,
			PromptEvalCount: 5,
		}

		_ = json.NewEncoder(w).Encode(resp)
	}))
	defer server.Close()

	provider, _ := NewOllamaProvider(WithOllamaBaseURL(server.URL))

	req, _ := NewChatRequest("llama3.2",
		WithMessages(UserMessage("Hello")),
	)

	resp, err := provider.Chat(context.Background(), req)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if resp.Content != "Hello! How can I help you?" {
		t.Errorf("unexpected content: %s", resp.Content)
	}
	if resp.Model != "llama3.2" {
		t.Errorf("unexpected model: %s", resp.Model)
	}
	if resp.FinishReason == nil || *resp.FinishReason != FinishReasonStop {
		t.Error("expected FinishReasonStop")
	}
}

func TestOllamaProvider_Chat_WithOptions(t *testing.T) {
	var receivedReq ollamaChatRequest

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_ = json.NewDecoder(r.Body).Decode(&receivedReq)

		resp := ollamaChatResponse{
			Model:   "llama3.2",
			Message: ollamaMessage{Role: "assistant", Content: "Response"},
			Done:    true,
		}
		_ = json.NewEncoder(w).Encode(resp)
	}))
	defer server.Close()

	provider, _ := NewOllamaProvider(WithOllamaBaseURL(server.URL))

	temp := 0.7
	maxTokens := 100

	req, _ := NewChatRequest("llama3.2",
		WithMessages(UserMessage("Test")),
		WithTemperature(temp),
		WithMaxTokens(maxTokens),
	)

	_, err := provider.Chat(context.Background(), req)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if receivedReq.Options["temperature"] != 0.7 {
		t.Errorf("expected temperature 0.7, got %v", receivedReq.Options["temperature"])
	}
	if receivedReq.Options["num_predict"] != float64(100) {
		t.Errorf("expected num_predict 100, got %v", receivedReq.Options["num_predict"])
	}
}

func TestOllamaProvider_Chat_ThinkingMode(t *testing.T) {
	var receivedReq ollamaChatRequest

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_ = json.NewDecoder(r.Body).Decode(&receivedReq)

		resp := ollamaChatResponse{
			Model:    "qwen3",
			Message:  ollamaMessage{Role: "assistant", Content: "The answer is 5"},
			Done:     true,
			Thinking: "Let me solve this step by step...",
		}
		_ = json.NewEncoder(w).Encode(resp)
	}))
	defer server.Close()

	provider, _ := NewOllamaProvider(WithOllamaBaseURL(server.URL))

	req, _ := NewChatRequest("qwen3",
		WithMessages(UserMessage("Solve: 2x + 5 = 15")),
		WithThinkingMode(ThinkingModeEnabled),
	)

	resp, err := provider.Chat(context.Background(), req)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	// Verify think parameter was sent
	if receivedReq.Think == nil || *receivedReq.Think != true {
		t.Error("expected think=true in request")
	}

	// Verify thinking content was extracted
	if resp.Thinking == nil || *resp.Thinking != "Let me solve this step by step..." {
		t.Errorf("expected thinking content, got %v", resp.Thinking)
	}
}

func TestOllamaProvider_Chat_Error(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusNotFound)
		_ = json.NewEncoder(w).Encode(ollamaErrorResponse{
			Error: "model 'unknown' not found",
		})
	}))
	defer server.Close()

	provider, _ := NewOllamaProvider(WithOllamaBaseURL(server.URL))

	req, _ := NewChatRequest("unknown",
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

	if llmErr.HTTPStatusCode != http.StatusNotFound {
		t.Errorf("expected status 404, got %d", llmErr.HTTPStatusCode)
	}
}

func TestOllamaProvider_ChatStream(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		// Return NDJSON stream
		chunks := []ollamaChatResponse{
			{Model: "llama3.2", Message: ollamaMessage{Role: "assistant", Content: "Hello"}, Done: false},
			{Model: "llama3.2", Message: ollamaMessage{Role: "assistant", Content: " there"}, Done: false},
			{Model: "llama3.2", Message: ollamaMessage{Role: "assistant", Content: "!"}, Done: true, EvalCount: 3, PromptEvalCount: 5},
		}

		for _, chunk := range chunks {
			_ = json.NewEncoder(w).Encode(chunk)
		}
	}))
	defer server.Close()

	provider, _ := NewOllamaProvider(WithOllamaBaseURL(server.URL))

	req, _ := NewChatRequest("llama3.2",
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

	if finalChunk.Usage == nil || finalChunk.Usage.Actual == nil {
		t.Error("expected usage in final chunk")
	}
}

func TestOllamaProvider_ChatStream_Thinking(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		chunks := []ollamaChatResponse{
			{Model: "qwen3", Message: ollamaMessage{Content: ""}, Thinking: "Let me think...", Done: false},
			{Model: "qwen3", Message: ollamaMessage{Content: "Answer"}, Done: true},
		}

		for _, chunk := range chunks {
			_ = json.NewEncoder(w).Encode(chunk)
		}
	}))
	defer server.Close()

	provider, _ := NewOllamaProvider(WithOllamaBaseURL(server.URL))

	req, _ := NewChatRequest("qwen3",
		WithMessages(UserMessage("Solve this")),
		WithThinkingMode(ThinkingModeEnabled),
	)

	chunks, errs := provider.ChatStream(context.Background(), req)

	var hasThinking bool
	for chunk := range chunks {
		if chunk.HasThinking() {
			hasThinking = true
		}
	}

	if err := <-errs; err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if !hasThinking {
		t.Error("expected thinking content in stream")
	}
}

func TestOllamaProvider_ChatStream_ContextCancellation(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		// Simulate slow response
		time.Sleep(100 * time.Millisecond)
		_ = json.NewEncoder(w).Encode(ollamaChatResponse{
			Model:   "llama3.2",
			Message: ollamaMessage{Content: "Response"},
			Done:    true,
		})
	}))
	defer server.Close()

	provider, _ := NewOllamaProvider(WithOllamaBaseURL(server.URL))

	ctx, cancel := context.WithCancel(context.Background())
	cancel() // Cancel immediately

	req, _ := NewChatRequest("llama3.2",
		WithMessages(UserMessage("Hi")),
	)

	_, errs := provider.ChatStream(ctx, req)

	err := <-errs
	if err == nil {
		t.Error("expected context cancellation error")
	}
}

func TestOllamaProvider_ListModels(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/tags" {
			t.Errorf("unexpected path: %s", r.URL.Path)
		}

		resp := ollamaTagsResponse{
			Models: []ollamaModelInfo{
				{Name: "llama3.2:latest", Size: 4109853696, Digest: "sha256:abc", ModifiedAt: "2024-01-15T10:30:00Z"},
				{Name: "mistral:7b", Size: 4100000000, Digest: "sha256:def", ModifiedAt: "2024-01-14T09:00:00Z"},
			},
		}
		_ = json.NewEncoder(w).Encode(resp)
	}))
	defer server.Close()

	provider, _ := NewOllamaProvider(WithOllamaBaseURL(server.URL))

	models, err := provider.ListModels(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if len(models) != 2 {
		t.Fatalf("expected 2 models, got %d", len(models))
	}

	if models[0].Name != "llama3.2:latest" {
		t.Errorf("unexpected model name: %s", models[0].Name)
	}

	if models[0].SizeBytes == nil || *models[0].SizeBytes != 4109853696 {
		t.Errorf("unexpected size: %v", models[0].SizeBytes)
	}
}

func TestOllamaProvider_ListModels_Empty(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		resp := ollamaTagsResponse{Models: []ollamaModelInfo{}}
		_ = json.NewEncoder(w).Encode(resp)
	}))
	defer server.Close()

	provider, _ := NewOllamaProvider(WithOllamaBaseURL(server.URL))

	models, err := provider.ListModels(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if len(models) != 0 {
		t.Errorf("expected empty slice, got %d models", len(models))
	}
}

func TestOllamaProvider_Ping_Success(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/" {
			t.Errorf("unexpected path: %s", r.URL.Path)
		}
		w.WriteHeader(http.StatusOK)
		_, _ = fmt.Fprintf(w, `{"version":"0.5.1"}`)
	}))
	defer server.Close()

	provider, _ := NewOllamaProvider(WithOllamaBaseURL(server.URL))

	err := provider.Ping(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestOllamaProvider_Ping_Failure(t *testing.T) {
	// Create a server that immediately closes connections
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusInternalServerError)
	}))
	defer server.Close()

	provider, _ := NewOllamaProvider(WithOllamaBaseURL(server.URL))

	err := provider.Ping(context.Background())
	if err == nil {
		t.Error("expected error for failed ping")
	}
}

func TestOllamaProvider_NetworkError(t *testing.T) {
	provider, _ := NewOllamaProvider(
		WithOllamaBaseURL("http://localhost:99999"), // Invalid port
	)

	req, _ := NewChatRequest("llama3.2",
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

func TestOllamaProvider_GetCapabilities(t *testing.T) {
	provider, _ := NewOllamaProvider()

	caps := provider.GetCapabilities()

	// Ollama supports system messages
	if !caps.SupportsSystemMessages {
		t.Error("expected Ollama to support system messages")
	}

	// Ollama supports streaming
	if !caps.SupportsStreaming {
		t.Error("expected Ollama to support streaming")
	}

	// Ollama supports vision (for some models)
	if !caps.SupportsVision {
		t.Error("expected Ollama to support vision")
	}

	// Ollama has no documented max stop sequences
	if caps.MaxStopSequences != nil {
		t.Errorf("expected MaxStopSequences to be nil for Ollama, got %v", *caps.MaxStopSequences)
	}

	// Ollama supports penalties (via repeat_penalty etc)
	if !caps.SupportsPresencePenalty {
		t.Error("expected Ollama to support presence penalty")
	}
	if !caps.SupportsFrequencyPenalty {
		t.Error("expected Ollama to support frequency penalty")
	}

	// Ollama supports seed
	if !caps.SupportsSeed {
		t.Error("expected Ollama to support seed")
	}

	// Ollama does NOT support logprobs
	if caps.SupportsLogprobs {
		t.Error("expected Ollama to NOT support logprobs")
	}

	// Ollama supports JSON mode
	if !caps.SupportsJSONMode {
		t.Error("expected Ollama to support JSON mode")
	}
}

func TestOllamaProvider_GetModelCapabilities_VisionModel(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path == "/api/show" {
			// Mock llava response - a vision model
			_ = json.NewEncoder(w).Encode(map[string]any{
				"modelfile": "FROM llava:7b\nTEMPLATE ...",
				"details": map[string]any{
					"families": []string{"clip", "llama"},
				},
			})
			return
		}
	}))
	defer server.Close()

	provider, _ := NewOllamaProvider(WithOllamaBaseURL(server.URL))

	caps, err := provider.GetModelCapabilities(context.Background(), "llava:latest")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if !caps.VisionMode.SupportsVision() {
		t.Error("expected llava to support vision")
	}
}

func TestOllamaProvider_GetModelCapabilities_TextOnlyModel(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path == "/api/show" {
			// Mock llama response - text-only model
			_ = json.NewEncoder(w).Encode(map[string]any{
				"modelfile": "FROM llama3.2:3b\nTEMPLATE ...",
				"details": map[string]any{
					"families": []string{"llama"},
				},
			})
			return
		}
	}))
	defer server.Close()

	provider, _ := NewOllamaProvider(WithOllamaBaseURL(server.URL))

	caps, err := provider.GetModelCapabilities(context.Background(), "llama3.2")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if caps.VisionMode.SupportsVision() {
		t.Error("expected llama3.2 to NOT support vision")
	}

	// All models support streaming
	if !caps.SupportsStreaming {
		t.Error("expected llama3.2 to support streaming")
	}
}

func TestOllamaProvider_GetModelCapabilities_UnknownModel(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path == "/api/show" {
			w.WriteHeader(http.StatusNotFound)
			_ = json.NewEncoder(w).Encode(map[string]any{
				"error": "model 'nonexistent' not found",
			})
			return
		}
	}))
	defer server.Close()

	provider, _ := NewOllamaProvider(WithOllamaBaseURL(server.URL))

	_, err := provider.GetModelCapabilities(context.Background(), "nonexistent")
	if err == nil {
		t.Error("expected error for unknown model")
	}
}
