package nxuskit

import (
	"context"
	"errors"
	"testing"
	"time"
)

// testMockProvider is a test implementation of LLMProvider used only in provider_test.go.
type testMockProvider struct {
	// ChatFunc allows customizing Chat behavior in tests.
	ChatFunc func(ctx context.Context, req *ChatRequest) (*ChatResponse, error)
	// ChatStreamFunc allows customizing ChatStream behavior in tests.
	ChatStreamFunc func(ctx context.Context, req *ChatRequest) (chunkCh <-chan StreamChunk, errCh <-chan error)
	// ListModelsFunc allows customizing ListModels behavior in tests.
	ListModelsFunc func(ctx context.Context) ([]ModelInfo, error)
	// Name is the provider name to return.
	Name string
}

// Chat implements LLMProvider.Chat.
func (m *testMockProvider) Chat(ctx context.Context, req *ChatRequest) (*ChatResponse, error) {
	if m.ChatFunc != nil {
		return m.ChatFunc(ctx, req)
	}
	return &ChatResponse{
		Content: "Mock response",
		Model:   req.Model,
		Usage: TokenUsage{
			Estimated:  TokenCount{PromptTokens: 10, CompletionTokens: 5},
			IsComplete: true,
		},
	}, nil
}

// ChatStream implements LLMProvider.ChatStream.
func (m *testMockProvider) ChatStream(ctx context.Context, req *ChatRequest) (chunkCh <-chan StreamChunk, errCh <-chan error) {
	if m.ChatStreamFunc != nil {
		return m.ChatStreamFunc(ctx, req)
	}

	chunks := make(chan StreamChunk, 3)
	errs := make(chan error, 1)

	go func() {
		defer close(chunks)
		defer close(errs)

		chunks <- NewStreamChunk("Hello")
		chunks <- NewStreamChunk(" world")
		chunks <- FinalChunk("!", FinishReasonStop, &TokenUsage{
			Estimated:  TokenCount{PromptTokens: 10, CompletionTokens: 3},
			IsComplete: true,
		})
	}()

	return chunks, errs
}

// ListModels implements LLMProvider.ListModels.
func (m *testMockProvider) ListModels(ctx context.Context) ([]ModelInfo, error) {
	if m.ListModelsFunc != nil {
		return m.ListModelsFunc(ctx)
	}
	return []ModelInfo{
		{Name: "mock-model-1"},
		{Name: "mock-model-2"},
	}, nil
}

// ProviderName implements LLMProvider.ProviderName.
func (m *testMockProvider) ProviderName() string {
	if m.Name != "" {
		return m.Name
	}
	return "mock"
}

// Ping implements LLMProvider.Ping.
func (m *testMockProvider) Ping(ctx context.Context) error {
	return nil
}

// GetCapabilities implements LLMProvider.GetCapabilities.
func (m *testMockProvider) GetCapabilities() ProviderCapabilities {
	return DefaultCapabilities()
}

// StreamWithUsage implements LLMProvider.StreamWithUsage.
func (m *testMockProvider) StreamWithUsage(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan TokenUsage) {
	chunks, errs := m.ChatStream(ctx, req)
	return wrapStreamWithUsage(chunks, errs)
}

// Verify MockProvider implements LLMProvider interface
var _ LLMProvider = (*testMockProvider)(nil)

func TestTestMockProviderImplementsInterface(t *testing.T) {
	// This test verifies the interface is correctly implemented
	var provider LLMProvider = &testMockProvider{Name: "test"}
	if provider.ProviderName() != "test" {
		t.Errorf("ProviderName() = %q, want %q", provider.ProviderName(), "test")
	}
}

func TestTestMockProviderChat(t *testing.T) {
	provider := &testMockProvider{Name: "test"}
	ctx := context.Background()

	req, err := NewChatRequest("test-model", WithMessages(UserMessage("Hello")))
	if err != nil {
		t.Fatalf("NewChatRequest error: %v", err)
	}
	resp, err := provider.Chat(ctx, req)

	if err != nil {
		t.Fatalf("Chat error: %v", err)
	}

	if resp.Content != "Mock response" {
		t.Errorf("Content = %q, want %q", resp.Content, "Mock response")
	}
	if resp.Model != "test-model" {
		t.Errorf("Model = %q, want %q", resp.Model, "test-model")
	}
	if resp.Usage.TotalTokens() != 15 {
		t.Errorf("TotalTokens() = %d, want 15", resp.Usage.TotalTokens())
	}
}

func TestTestMockProviderChatCustom(t *testing.T) {
	reason := FinishReasonLength
	provider := &testMockProvider{
		ChatFunc: func(_ context.Context, req *ChatRequest) (*ChatResponse, error) {
			return &ChatResponse{
				Content:      "Custom response",
				Model:        req.Model,
				FinishReason: &reason,
			}, nil
		},
	}

	ctx := context.Background()
	req, err := NewChatRequest("custom-model")
	if err != nil {
		t.Fatalf("NewChatRequest error: %v", err)
	}
	resp, err := provider.Chat(ctx, req)

	if err != nil {
		t.Fatalf("Chat error: %v", err)
	}

	if resp.Content != "Custom response" {
		t.Errorf("Content = %q, want %q", resp.Content, "Custom response")
	}
	if resp.FinishReason == nil || *resp.FinishReason != FinishReasonLength {
		t.Error("FinishReason should be FinishReasonLength")
	}
}

func TestTestMockProviderChatError(t *testing.T) {
	provider := &testMockProvider{
		ChatFunc: func(_ context.Context, _ *ChatRequest) (*ChatResponse, error) {
			return nil, NewAuthenticationError("test", "invalid key", nil)
		},
	}

	ctx := context.Background()
	req, err := NewChatRequest("test-model")
	if err != nil {
		t.Fatalf("NewChatRequest error: %v", err)
	}
	_, err = provider.Chat(ctx, req)

	if err == nil {
		t.Error("Expected error from Chat")
	}

	var llmErr *LLMError
	if !errors.As(err, &llmErr) {
		t.Error("Error should be *LLMError")
	}
	if llmErr.Kind != ErrorKindAuthentication {
		t.Errorf("Kind = %v, want %v", llmErr.Kind, ErrorKindAuthentication)
	}
}

func TestTestMockProviderChatStream(t *testing.T) {
	provider := &testMockProvider{Name: "test"}
	ctx := context.Background()

	req, err := NewChatRequest("test-model", WithMessages(UserMessage("Hello")))
	if err != nil {
		t.Fatalf("NewChatRequest error: %v", err)
	}
	chunks, errs := provider.ChatStream(ctx, req)

	var content string
	var finalChunk StreamChunk
	for chunk := range chunks {
		content += chunk.Delta
		if chunk.IsFinal() {
			finalChunk = chunk
		}
	}

	// Check for errors
	if err := <-errs; err != nil {
		t.Fatalf("ChatStream error: %v", err)
	}

	if content != "Hello world!" {
		t.Errorf("Streamed content = %q, want %q", content, "Hello world!")
	}

	if !finalChunk.IsFinal() {
		t.Error("Last chunk should be final")
	}

	if finalChunk.Usage == nil {
		t.Error("Final chunk should have usage")
	}
}

func TestTestMockProviderChatStreamCustom(t *testing.T) {
	provider := &testMockProvider{
		ChatStreamFunc: func(_ context.Context, _ *ChatRequest) (chunkCh <-chan StreamChunk, errCh <-chan error) {
			chunks := make(chan StreamChunk, 2)
			errs := make(chan error, 1)

			go func() {
				defer close(chunks)
				defer close(errs)

				chunks <- ThinkingChunk("Let me think...")
				chunks <- FinalChunk("Done", FinishReasonStop, nil)
			}()

			return chunks, errs
		},
	}

	ctx := context.Background()
	req, err := NewChatRequest("test-model")
	if err != nil {
		t.Fatalf("NewChatRequest error: %v", err)
	}
	chunks, errs := provider.ChatStream(ctx, req)

	var hasThinking bool
	for chunk := range chunks {
		if chunk.HasThinking() {
			hasThinking = true
		}
	}

	if err := <-errs; err != nil {
		t.Fatalf("ChatStream error: %v", err)
	}

	if !hasThinking {
		t.Error("Should have received thinking chunk")
	}
}

func TestTestMockProviderChatStreamError(t *testing.T) {
	provider := &testMockProvider{
		ChatStreamFunc: func(_ context.Context, _ *ChatRequest) (chunkCh <-chan StreamChunk, errCh <-chan error) {
			chunks := make(chan StreamChunk)
			errs := make(chan error, 1)

			go func() {
				defer close(chunks)
				defer close(errs)

				errs <- NewStreamError("test", "stream interrupted", nil)
			}()

			return chunks, errs
		},
	}

	ctx := context.Background()
	req, err := NewChatRequest("test-model")
	if err != nil {
		t.Fatalf("NewChatRequest error: %v", err)
	}
	chunks, errs := provider.ChatStream(ctx, req)

	// Drain chunks channel
	//nolint:revive // empty block is intentional - we need to drain the channel
	for range chunks {
	}

	err = <-errs
	if err == nil {
		t.Error("Expected error from ChatStream")
	}

	var llmErr *LLMError
	if !errors.As(err, &llmErr) {
		t.Error("Error should be *LLMError")
	}
	if llmErr.Kind != ErrorKindStream {
		t.Errorf("Kind = %v, want %v", llmErr.Kind, ErrorKindStream)
	}
}

func TestTestMockProviderListModels(t *testing.T) {
	provider := &testMockProvider{Name: "test"}
	ctx := context.Background()

	models, err := provider.ListModels(ctx)
	if err != nil {
		t.Fatalf("ListModels error: %v", err)
	}

	if len(models) != 2 {
		t.Errorf("Models len = %d, want 2", len(models))
	}

	if models[0].Name != "mock-model-1" {
		t.Errorf("Model[0].Name = %q, want %q", models[0].Name, "mock-model-1")
	}
}

func TestTestMockProviderListModelsCustom(t *testing.T) {
	size := int64(3700000000)
	ctxWindow := 128000
	provider := &testMockProvider{
		ListModelsFunc: func(_ context.Context) ([]ModelInfo, error) {
			return []ModelInfo{
				{
					Name:          "custom-model",
					SizeBytes:     &size,
					ContextWindow: &ctxWindow,
					Metadata:      map[string]any{"vision": true},
				},
			}, nil
		},
	}

	models, err := provider.ListModels(context.Background())
	if err != nil {
		t.Fatalf("ListModels error: %v", err)
	}

	if len(models) != 1 {
		t.Errorf("Models len = %d, want 1", len(models))
	}

	model := models[0]
	if model.Name != "custom-model" {
		t.Errorf("Name = %q, want %q", model.Name, "custom-model")
	}
	if !model.SupportsVision() {
		t.Error("Model should support vision")
	}
}

func TestTestMockProviderProviderName(t *testing.T) {
	t.Run("default name", func(t *testing.T) {
		provider := &testMockProvider{}
		if provider.ProviderName() != "mock" {
			t.Errorf("ProviderName() = %q, want %q", provider.ProviderName(), "mock")
		}
	})

	t.Run("custom name", func(t *testing.T) {
		provider := &testMockProvider{Name: "custom"}
		if provider.ProviderName() != "custom" {
			t.Errorf("ProviderName() = %q, want %q", provider.ProviderName(), "custom")
		}
	})
}

// -----------------------------------------------------------------------------
// Tests for SessionResetter Interface (T038)
// -----------------------------------------------------------------------------

// Verify all providers implement SessionResetter
var (
	_ SessionResetter = (*OpenAIProvider)(nil)
	_ SessionResetter = (*ClaudeProvider)(nil)
	_ SessionResetter = (*OllamaProvider)(nil)
	_ SessionResetter = (*LmStudioProvider)(nil)
	_ SessionResetter = (*GroqProvider)(nil)
	_ SessionResetter = (*TogetherProvider)(nil)
	_ SessionResetter = (*MistralProvider)(nil)
	_ SessionResetter = (*FireworksProvider)(nil)
	_ SessionResetter = (*OpenRouterProvider)(nil)
	_ SessionResetter = (*PerplexityProvider)(nil)
	_ SessionResetter = (*MockProvider)(nil)
	_ SessionResetter = (*LoopbackProvider)(nil)
)

func TestSessionResetter_MockProvider(t *testing.T) {
	// MockProvider is stateful - FreshSession should create a new instance
	provider := NewMockProvider(
		WithMockResponse(&ChatResponse{Content: "First"}),
		WithMockResponse(&ChatResponse{Content: "Second"}),
	)

	req := &ChatRequest{Model: "test", Messages: []Message{UserMessage("Hi")}}

	// First call - should get "First"
	resp, _ := provider.Chat(context.Background(), req)
	if resp.Content != "First" {
		t.Errorf("Expected 'First', got %q", resp.Content)
	}

	// Create fresh session
	fresh, err := provider.FreshSession()
	if err != nil {
		t.Fatalf("FreshSession error: %v", err)
	}

	// Fresh session should get "First" again (reset queue index)
	resp, _ = fresh.Chat(context.Background(), req)
	if resp.Content != "First" {
		t.Errorf("After FreshSession, expected 'First', got %q", resp.Content)
	}

	// Original provider should continue from "Second"
	resp, _ = provider.Chat(context.Background(), req)
	if resp.Content != "Second" {
		t.Errorf("Original provider expected 'Second', got %q", resp.Content)
	}
}

func TestSessionResetter_LoopbackProvider(t *testing.T) {
	// LoopbackProvider is stateless - FreshSession should return self
	provider := NewLoopbackProvider()

	fresh, err := provider.FreshSession()
	if err != nil {
		t.Fatalf("FreshSession error: %v", err)
	}

	// Should be the same instance
	if fresh != provider {
		t.Error("Stateless provider FreshSession should return self")
	}
}

// -----------------------------------------------------------------------------
// Tests for ModelLister Interface (T053-T054)
// -----------------------------------------------------------------------------

// Verify ModelLister providers
var (
	_ ModelLister = (*OllamaProvider)(nil)
	_ ModelLister = (*LmStudioProvider)(nil)
	_ ModelLister = (*MockProvider)(nil)
	_ ModelLister = (*LoopbackProvider)(nil)
)

func TestModelLister_MockProvider(t *testing.T) {
	models := []ModelInfo{
		{Name: "test-model-1"},
		{Name: "test-model-2"},
	}
	provider := NewMockProvider(WithMockModels(models))

	// Use ModelLister interface
	lister, ok := any(provider).(ModelLister)
	if !ok {
		t.Fatal("MockProvider should implement ModelLister")
	}

	result, err := lister.ListAvailableModels(context.Background())
	if err != nil {
		t.Fatalf("ListAvailableModels error: %v", err)
	}

	if len(result) != 2 {
		t.Fatalf("Expected 2 models, got %d", len(result))
	}
	if result[0].Name != "test-model-1" {
		t.Errorf("Expected 'test-model-1', got %q", result[0].Name)
	}
}

func TestModelLister_LoopbackProvider(t *testing.T) {
	provider := NewLoopbackProvider()

	lister, ok := any(provider).(ModelLister)
	if !ok {
		t.Fatal("LoopbackProvider should implement ModelLister")
	}

	result, err := lister.ListAvailableModels(context.Background())
	if err != nil {
		t.Fatalf("ListAvailableModels error: %v", err)
	}

	if len(result) != 1 {
		t.Fatalf("Expected 1 model, got %d", len(result))
	}
	if result[0].Name != "loopback" {
		t.Errorf("Expected 'loopback', got %q", result[0].Name)
	}
}

func TestModelLister_OpenAIProvider_DoesNotImplement(t *testing.T) {
	// OpenAIProvider should NOT implement ModelLister
	// We can't create a real provider without API key, but we can check the type
	var provider *OpenAIProvider
	_, ok := any(provider).(ModelLister)
	if ok {
		t.Error("OpenAIProvider should NOT implement ModelLister")
	}
}

// -----------------------------------------------------------------------------
// Tests for StreamWithUsage (coverage)
// -----------------------------------------------------------------------------

func TestStreamWithUsage_MockProvider(t *testing.T) {
	fr := FinishReasonStop
	chunks := []StreamChunk{
		{Delta: "Hello"},
		{Delta: " world", FinishReason: &fr, Usage: &TokenUsage{
			Actual:     &TokenCount{PromptTokens: 5, CompletionTokens: 2},
			IsComplete: true,
		}},
	}
	provider := NewMockProvider(WithMockStreamResponse(chunks))

	req := &ChatRequest{Model: "test", Messages: []Message{UserMessage("Hi")}}
	chunkCh, usageCh := provider.StreamWithUsage(context.Background(), req)

	var content string
	for chunk := range chunkCh {
		content += chunk.Delta
	}

	usage := <-usageCh
	if content != "Hello world" {
		t.Errorf("Expected 'Hello world', got %q", content)
	}
	if !usage.IsComplete {
		t.Error("Expected usage to be complete")
	}
}

func TestStreamWithUsage_LoopbackProvider(t *testing.T) {
	provider := NewLoopbackProvider()

	req := &ChatRequest{Model: "loopback", Messages: []Message{UserMessage("echo this")}}
	chunkCh, usageCh := provider.StreamWithUsage(context.Background(), req)

	var content string
	for chunk := range chunkCh {
		content += chunk.Delta
	}

	usage := <-usageCh
	if content != "echo this" {
		t.Errorf("Expected 'echo this', got %q", content)
	}
	if !usage.IsComplete {
		t.Error("Expected usage to be complete")
	}
}

// -----------------------------------------------------------------------------
// Tests for Additional MockProvider Methods (coverage)
// -----------------------------------------------------------------------------

func TestMockProvider_AddError(t *testing.T) {
	provider := NewMockProvider()
	testErr := errors.New("test error")
	provider.AddError(testErr)

	req := &ChatRequest{Model: "test", Messages: []Message{UserMessage("Hi")}}
	_, err := provider.Chat(context.Background(), req)
	if err != testErr {
		t.Errorf("Expected test error, got %v", err)
	}
}

func TestMockProvider_AddStreamResponse(t *testing.T) {
	provider := NewMockProvider()
	fr := FinishReasonStop
	chunks := []StreamChunk{
		{Delta: "Added"},
		{Delta: " stream", FinishReason: &fr},
	}
	provider.AddStreamResponse(chunks)

	req := &ChatRequest{Model: "test", Messages: []Message{UserMessage("Hi")}}
	chunkCh, errCh := provider.ChatStream(context.Background(), req)

	var content string
	for chunk := range chunkCh {
		content += chunk.Delta
	}
	if err := <-errCh; err != nil {
		t.Fatalf("Unexpected error: %v", err)
	}
	if content != "Added stream" {
		t.Errorf("Expected 'Added stream', got %q", content)
	}
}

// -----------------------------------------------------------------------------
// Tests for FreshSession implementations (coverage)

func TestFreshSession_Claude(t *testing.T) {
	provider, err := NewClaudeProvider(WithClaudeAPIKey("test-key"))
	if err != nil {
		t.Fatalf("Failed to create Claude provider: %v", err)
	}
	fresh, err := provider.FreshSession()
	if err != nil {
		t.Errorf("FreshSession should not return error: %v", err)
	}
	if fresh == nil {
		t.Error("FreshSession should not return nil")
	}
	// For stateless providers, FreshSession returns self
	if fresh != provider {
		t.Error("FreshSession should return self for stateless providers")
	}
}

func TestFreshSession_Fireworks(t *testing.T) {
	provider, err := NewFireworksProvider(WithFireworksAPIKey("test-key"))
	if err != nil {
		t.Fatalf("Failed to create Fireworks provider: %v", err)
	}
	fresh, err := provider.FreshSession()
	if err != nil {
		t.Errorf("FreshSession should not return error: %v", err)
	}
	if fresh == nil {
		t.Error("FreshSession should not return nil")
	}
	if fresh != provider {
		t.Error("FreshSession should return self for stateless providers")
	}
}

func TestFreshSession_Groq(t *testing.T) {
	provider, err := NewGroqProvider(WithGroqAPIKey("test-key"))
	if err != nil {
		t.Fatalf("Failed to create Groq provider: %v", err)
	}
	fresh, err := provider.FreshSession()
	if err != nil {
		t.Errorf("FreshSession should not return error: %v", err)
	}
	if fresh == nil {
		t.Error("FreshSession should not return nil")
	}
	if fresh != provider {
		t.Error("FreshSession should return self for stateless providers")
	}
}

func TestFreshSession_Together(t *testing.T) {
	provider, err := NewTogetherProvider(WithTogetherAPIKey("test-key"))
	if err != nil {
		t.Fatalf("Failed to create Together provider: %v", err)
	}
	fresh, err := provider.FreshSession()
	if err != nil {
		t.Errorf("FreshSession should not return error: %v", err)
	}
	if fresh == nil {
		t.Error("FreshSession should not return nil")
	}
	if fresh != provider {
		t.Error("FreshSession should return self for stateless providers")
	}
}

func TestFreshSession_Mistral(t *testing.T) {
	provider, err := NewMistralProvider(WithMistralAPIKey("test-key"))
	if err != nil {
		t.Fatalf("Failed to create Mistral provider: %v", err)
	}
	fresh, err := provider.FreshSession()
	if err != nil {
		t.Errorf("FreshSession should not return error: %v", err)
	}
	if fresh == nil {
		t.Error("FreshSession should not return nil")
	}
	if fresh != provider {
		t.Error("FreshSession should return self for stateless providers")
	}
}

func TestFreshSession_OpenRouter(t *testing.T) {
	provider, err := NewOpenRouterProvider(WithOpenRouterAPIKey("test-key"))
	if err != nil {
		t.Fatalf("Failed to create OpenRouter provider: %v", err)
	}
	fresh, err := provider.FreshSession()
	if err != nil {
		t.Errorf("FreshSession should not return error: %v", err)
	}
	if fresh == nil {
		t.Error("FreshSession should not return nil")
	}
	if fresh != provider {
		t.Error("FreshSession should return self for stateless providers")
	}
}

func TestFreshSession_Perplexity(t *testing.T) {
	provider, err := NewPerplexityProvider(WithPerplexityAPIKey("test-key"))
	if err != nil {
		t.Fatalf("Failed to create Perplexity provider: %v", err)
	}
	fresh, err := provider.FreshSession()
	if err != nil {
		t.Errorf("FreshSession should not return error: %v", err)
	}
	if fresh == nil {
		t.Error("FreshSession should not return nil")
	}
	if fresh != provider {
		t.Error("FreshSession should return self for stateless providers")
	}
}

func TestFreshSession_OpenAI(t *testing.T) {
	provider, err := NewOpenAIProvider(WithOpenAIAPIKey("test-key"))
	if err != nil {
		t.Fatalf("Failed to create OpenAI provider: %v", err)
	}
	fresh, err := provider.FreshSession()
	if err != nil {
		t.Errorf("FreshSession should not return error: %v", err)
	}
	if fresh == nil {
		t.Error("FreshSession should not return nil")
	}
	if fresh != provider {
		t.Error("FreshSession should return self for stateless providers")
	}
}

func TestFreshSession_LmStudio(t *testing.T) {
	provider, err := NewLmStudioProvider()
	if err != nil {
		t.Fatalf("Failed to create LM Studio provider: %v", err)
	}
	fresh, err := provider.FreshSession()
	if err != nil {
		t.Errorf("FreshSession should not return error: %v", err)
	}
	if fresh == nil {
		t.Error("FreshSession should not return nil")
	}
	if fresh != provider {
		t.Error("FreshSession should return self for stateless providers")
	}
}

func TestFreshSession_Ollama(t *testing.T) {
	provider, err := NewOllamaProvider()
	if err != nil {
		t.Fatalf("Failed to create Ollama provider: %v", err)
	}
	fresh, err := provider.FreshSession()
	if err != nil {
		t.Errorf("FreshSession should not return error: %v", err)
	}
	if fresh == nil {
		t.Error("FreshSession should not return nil")
	}
	if fresh != provider {
		t.Error("FreshSession should return self for stateless providers")
	}
}

// -----------------------------------------------------------------------------
// Tests for error message formatting

func TestLLMError_RateLimitError(t *testing.T) {
	err := NewRateLimitError("test-provider", 30*time.Second, nil)
	errStr := err.Error()
	if errStr == "" {
		t.Error("Error string should not be empty")
	}
	if err.RetryAfter != 30*time.Second {
		t.Errorf("Expected RetryAfter 30s, got %v", err.RetryAfter)
	}
}

func TestLLMError_AuthenticationError(t *testing.T) {
	err := NewAuthenticationError("test-provider", "invalid key", nil)
	errStr := err.Error()
	if errStr == "" {
		t.Error("Error string should not be empty")
	}
}

func TestLLMError_ConfigurationError(t *testing.T) {
	err := NewConfigurationError("missing config", nil)
	errStr := err.Error()
	if errStr == "" {
		t.Error("Error string should not be empty")
	}
}

// -----------------------------------------------------------------------------
// Tests for ParameterAdapter.WarningMessages (coverage)

func TestParameterAdapter_WarningMessages(t *testing.T) {
	maxStop := 2
	caps := ProviderCapabilities{
		SupportsSystemMessages: true,
		MaxStopSequences:       &maxStop,
	}

	req := &ChatRequest{
		Model:    "test",
		Messages: []Message{UserMessage("Hi")},
		Stop:     []string{"stop1", "stop2", "stop3", "stop4", "stop5"},
	}

	result := ParameterAdapter{}.Adapt(req, caps)

	// Should have warnings due to stop sequence truncation
	if !result.HasWarnings() {
		t.Error("Expected warnings for stop sequence truncation")
	}

	// Test WarningMessages helper
	warnings := result.WarningMessages()
	if len(warnings) == 0 {
		t.Error("Expected warning messages")
	}

	// Each warning should be a non-empty string
	for i, w := range warnings {
		if w == "" {
			t.Errorf("Warning message %d should not be empty", i)
		}
	}
}

// Live-service integration tests (Ollama, LM Studio, API-key providers) have
// been moved to provider_integration_test.go behind //go:build integration.
// Run with: go test -tags=integration ./...
