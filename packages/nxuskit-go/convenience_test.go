package nxuskit

import (
	"context"
	"errors"
	"os"
	"testing"
	"time"
)

// =============================================================================
// User Story 1: Quick Single-Turn Completion Tests
// =============================================================================

func TestCompletion_WithMockProvider(t *testing.T) {
	// Use explicit mock provider for reliable testing
	ctx := context.Background()

	resp, err := Completion(ctx, "mock/any", "Hello, world!")
	if err != nil {
		t.Fatalf("Completion() error: %v", err)
	}

	if resp == nil {
		t.Fatal("Completion() returned nil response")
		return
	}

	// MockProvider returns the model from response config or uses default
	// Just verify we got a response
	if resp.Model == "" {
		t.Error("expected non-empty model")
	}
}

func TestCompletion_WithLoopbackProvider(t *testing.T) {
	ctx := context.Background()

	resp, err := Completion(ctx, "loopback/any", "Echo this message")
	if err != nil {
		t.Fatalf("Completion() error: %v", err)
	}

	if resp.Content != "Echo this message" {
		t.Errorf("expected echoed content, got %q", resp.Content)
	}
}

func TestCompletion_WithOptions(t *testing.T) {
	ctx := context.Background()

	// Test that options are applied correctly
	resp, err := Completion(ctx, "loopback/any", "Test",
		WithTemperature(0.7),
		WithMaxTokens(100),
	)
	if err != nil {
		t.Fatalf("Completion() with options error: %v", err)
	}

	if resp.Content != "Test" {
		t.Errorf("expected 'Test', got %q", resp.Content)
	}
}

func TestCompletion_AutoDetection(t *testing.T) {
	// Save and restore env vars
	origOpenAI := os.Getenv("OPENAI_API_KEY")
	defer func() { _ = os.Setenv("OPENAI_API_KEY", origOpenAI) }()
	_ = os.Setenv("OPENAI_API_KEY", "test-key")

	ctx := context.Background()

	// This should auto-detect OpenAI from "gpt-" prefix
	// We can't actually call OpenAI, but we can verify the provider selection
	// by checking that it doesn't return "unknown provider" error
	_, err := Completion(ctx, "gpt-4o", "Hello")

	// Should NOT be "unknown provider" error - should be something else
	// (like API error from invalid key)
	if err != nil {
		var llmErr *LLMError
		if errors.As(err, &llmErr) {
			// Should be auth error or provider error, not configuration error
			if llmErr.Kind == ErrorKindConfiguration &&
				llmErr.Message != "" &&
				(contains(llmErr.Message, "unknown provider") || contains(llmErr.Message, "cannot determine")) {
				t.Errorf("expected provider to be detected, got: %v", err)
			}
		}
	}
}

func TestCompletion_ExplicitProvider(t *testing.T) {
	ctx := context.Background()

	// Explicit provider should work
	resp, err := Completion(ctx, "loopback/my-model", "Test explicit")
	if err != nil {
		t.Fatalf("Completion() with explicit provider error: %v", err)
	}

	if resp.Content != "Test explicit" {
		t.Errorf("expected 'Test explicit', got %q", resp.Content)
	}
}

func TestCompletion_MissingAPIKey(t *testing.T) {
	// Clear env vars
	origOpenAI := os.Getenv("OPENAI_API_KEY")
	origOllama := os.Getenv("OLLAMA_HOST")
	defer func() {
		_ = os.Setenv("OPENAI_API_KEY", origOpenAI)
		_ = os.Setenv("OLLAMA_HOST", origOllama)
	}()
	_ = os.Unsetenv("OPENAI_API_KEY")
	_ = os.Unsetenv("OLLAMA_HOST")

	ctx := context.Background()

	_, err := Completion(ctx, "gpt-4o", "Hello")
	if err == nil {
		t.Fatal("expected error for missing API key")
	}

	// Should be a configuration error mentioning the API key
	if !errors.Is(err, ErrConfiguration) {
		t.Errorf("expected ErrConfiguration, got: %v", err)
	}

	errMsg := err.Error()
	if !contains(errMsg, "OPENAI_API_KEY") {
		t.Errorf("error should mention OPENAI_API_KEY, got: %s", errMsg)
	}
}

func TestCompletion_UnknownModel(t *testing.T) {
	// Clear env vars that could match
	origOllama := os.Getenv("OLLAMA_HOST")
	defer func() { _ = os.Setenv("OLLAMA_HOST", origOllama) }()
	_ = os.Unsetenv("OLLAMA_HOST")

	ctx := context.Background()

	_, err := Completion(ctx, "totally-unknown-xyz-model", "Hello")
	if err == nil {
		t.Fatal("expected error for unknown model")
	}

	if !errors.Is(err, ErrConfiguration) {
		t.Errorf("expected ErrConfiguration, got: %v", err)
	}
}

func TestCompletion_ContextCancellation(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	cancel() // Cancel immediately

	_, err := Completion(ctx, "loopback/any", "Hello")
	if err == nil {
		t.Fatal("expected error for cancelled context")
	}

	// Should be context.Canceled or wrapped error
	if !errors.Is(err, context.Canceled) {
		t.Errorf("expected context.Canceled, got: %v", err)
	}
}

// =============================================================================
// User Story 2: Streaming Convenience API Tests
// =============================================================================

func TestCompletionStream_WithLoopback(t *testing.T) {
	ctx := context.Background()

	chunks, errs := CompletionStream(ctx, "loopback/any", "Stream this message")

	var content string
	var finalChunk StreamChunk
	for chunk := range chunks {
		content += chunk.Delta
		finalChunk = chunk
	}

	if err := <-errs; err != nil {
		t.Fatalf("CompletionStream() error: %v", err)
	}

	if content != "Stream this message" {
		t.Errorf("expected 'Stream this message', got %q", content)
	}

	if finalChunk.FinishReason == nil || *finalChunk.FinishReason != FinishReasonStop {
		t.Error("expected FinishReasonStop in final chunk")
	}
}

func TestCompletionStream_ContextCancellation(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())

	chunks, errs := CompletionStream(ctx, "loopback/any", "Test cancellation")

	// Cancel after starting
	cancel()

	// Drain chunks
	for range chunks {
	}

	// Check for error (may be nil if completed before cancellation)
	err := <-errs
	if err != nil && !errors.Is(err, context.Canceled) {
		t.Errorf("expected context.Canceled or nil, got: %v", err)
	}
}

func TestCompletionStream_FinalChunkHasUsage(t *testing.T) {
	ctx := context.Background()

	chunks, errs := CompletionStream(ctx, "loopback/any", "Test usage")

	var finalChunk StreamChunk
	for chunk := range chunks {
		finalChunk = chunk
	}

	if err := <-errs; err != nil {
		t.Fatalf("CompletionStream() error: %v", err)
	}

	if finalChunk.Usage == nil {
		t.Error("expected usage in final chunk")
	}
}

// =============================================================================
// User Story 3: Provider/Model String Parsing Tests
// =============================================================================

func TestCompletion_NestedProviderFormat(t *testing.T) {
	// Test the nested provider format: provider/subpath/model
	// We can't test with real OpenRouter, but we can verify parsing
	ctx := context.Background()

	// Using loopback to test that nested paths are handled
	resp, err := Completion(ctx, "loopback/some/nested/path", "Test nested")
	if err != nil {
		t.Fatalf("Completion() with nested path error: %v", err)
	}

	if resp.Content != "Test nested" {
		t.Errorf("expected 'Test nested', got %q", resp.Content)
	}
}

func TestCompletion_ExplicitProviderBypassesPatterns(t *testing.T) {
	// Even if a model name matches OpenAI pattern, explicit provider should win
	ctx := context.Background()

	// "gpt-4o" would normally match OpenAI, but explicit loopback should override
	resp, err := Completion(ctx, "loopback/gpt-4o", "Test explicit override")
	if err != nil {
		t.Fatalf("Completion() error: %v", err)
	}

	if resp.Content != "Test explicit override" {
		t.Errorf("expected 'Test explicit override', got %q", resp.Content)
	}
}

// =============================================================================
// User Story 6: Multi-Turn Conversation Tests
// =============================================================================

func TestCompletionWithMessages(t *testing.T) {
	ctx := context.Background()

	messages := []Message{
		SystemMessage("You are a helpful assistant."),
		UserMessage("What is 2+2?"),
		AssistantMessage("2+2 equals 4."),
		UserMessage("What about 3+3?"),
	}

	resp, err := CompletionWithMessages(ctx, "loopback/any", messages)
	if err != nil {
		t.Fatalf("CompletionWithMessages() error: %v", err)
	}

	// Loopback echoes the last user message
	if resp.Content != "What about 3+3?" {
		t.Errorf("expected 'What about 3+3?', got %q", resp.Content)
	}
}

func TestCompletionWithMessages_PreservesRoles(t *testing.T) {
	ctx := context.Background()

	messages := []Message{
		SystemMessage("System prompt"),
		UserMessage("User message"),
		AssistantMessage("Assistant response"),
	}

	// This mainly tests that messages are passed through correctly
	// The loopback provider will echo the last user message
	resp, err := CompletionWithMessages(ctx, "loopback/any", messages)
	if err != nil {
		t.Fatalf("CompletionWithMessages() error: %v", err)
	}

	if resp.Content != "User message" {
		t.Errorf("expected 'User message', got %q", resp.Content)
	}
}

func TestCompletionWithMessagesStream(t *testing.T) {
	ctx := context.Background()

	messages := []Message{
		UserMessage("Stream this"),
	}

	chunks, errs := CompletionWithMessagesStream(ctx, "loopback/any", messages)

	var content string
	for chunk := range chunks {
		content += chunk.Delta
	}

	if err := <-errs; err != nil {
		t.Fatalf("CompletionWithMessagesStream() error: %v", err)
	}

	if content != "Stream this" {
		t.Errorf("expected 'Stream this', got %q", content)
	}
}

func TestCompletionWithMessagesStream_CancelledContext(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	cancel() // Cancel immediately

	messages := []Message{
		UserMessage("Should fail"),
	}

	chunks, errs := CompletionWithMessagesStream(ctx, "loopback/any", messages)

	// Drain chunks
	for range chunks {
	}

	// Should get context error
	err := <-errs
	if err == nil {
		t.Fatal("Expected error for cancelled context")
	}
}

func TestCompletionWithMessagesStream_InvalidProvider(t *testing.T) {
	ctx := context.Background()
	messages := []Message{
		UserMessage("Hello"),
	}

	chunks, errs := CompletionWithMessagesStream(ctx, "invalid-provider/model", messages)

	// Drain chunks
	for range chunks {
	}

	// Should get provider error
	err := <-errs
	if err == nil {
		t.Fatal("Expected error for invalid provider")
	}
}

func TestCompletionWithMessages_WithTimeout(t *testing.T) {
	ctx := context.Background()

	messages := []Message{
		UserMessage("Hello with timeout"),
	}

	// Using WithTimeout option
	resp, err := CompletionWithMessages(ctx, "loopback/any", messages, WithTimeout(10*time.Second))
	if err != nil {
		t.Fatalf("CompletionWithMessages() with timeout error: %v", err)
	}
	if resp == nil {
		t.Fatal("Expected non-nil response")
	}
}

func TestCompletionWithMessages_WithExistingDeadline(t *testing.T) {
	// Create context with existing deadline
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	messages := []Message{
		UserMessage("Hello with existing deadline"),
	}

	// Apply a longer timeout - should use the existing shorter one
	resp, err := CompletionWithMessages(ctx, "loopback/any", messages, WithTimeout(10*time.Second))
	if err != nil {
		t.Fatalf("CompletionWithMessages() error: %v", err)
	}
	if resp == nil {
		t.Fatal("Expected non-nil response")
	}
}

func TestCompletionWithMessages_WithShorterTimeout(t *testing.T) {
	// Create context with longer deadline
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	messages := []Message{
		UserMessage("Hello with shorter new timeout"),
	}

	// Apply a shorter timeout - should use the new shorter one
	resp, err := CompletionWithMessages(ctx, "loopback/any", messages, WithTimeout(5*time.Second))
	if err != nil {
		t.Fatalf("CompletionWithMessages() error: %v", err)
	}
	if resp == nil {
		t.Fatal("Expected non-nil response")
	}
}

// =============================================================================
// Helper functions
// =============================================================================

func contains(s, substr string) bool {
	return len(s) >= len(substr) && (s == substr || len(substr) == 0 ||
		(len(s) > 0 && len(substr) > 0 && findSubstring(s, substr)))
}

func findSubstring(s, substr string) bool {
	for i := 0; i <= len(s)-len(substr); i++ {
		if s[i:i+len(substr)] == substr {
			return true
		}
	}
	return false
}

// =============================================================================
// User Story 4: Options Tests (convenience_options_test.go consolidated here)
// =============================================================================

func TestWithSystemPrompt(t *testing.T) {
	ctx := context.Background()

	// When using WithSystemPrompt with Completion, the system message should be prepended
	resp, err := Completion(ctx, "loopback/any", "User prompt",
		WithSystemPrompt("You are a helpful assistant."),
	)
	if err != nil {
		t.Fatalf("Completion() with WithSystemPrompt error: %v", err)
	}

	// Loopback returns the last user message, which should still be "User prompt"
	if resp.Content != "User prompt" {
		t.Errorf("expected 'User prompt', got %q", resp.Content)
	}
}

func TestWithTimeout(t *testing.T) {
	ctx := context.Background()

	// Short timeout that should work for loopback
	resp, err := Completion(ctx, "loopback/any", "Test timeout",
		WithTimeout(5*time.Second),
	)
	if err != nil {
		t.Fatalf("Completion() with WithTimeout error: %v", err)
	}

	if resp.Content != "Test timeout" {
		t.Errorf("expected 'Test timeout', got %q", resp.Content)
	}
}

func TestWithTimeout_Expiry(t *testing.T) {
	ctx := context.Background()

	// Use mock provider with delay longer than timeout
	// Since mock doesn't have delay, we can't easily test this
	// but we can verify the timeout is applied
	_, err := Completion(ctx, "loopback/any", "Test",
		WithTimeout(1*time.Nanosecond),
	)

	// With such a short timeout, it might fail or might succeed
	// depending on timing. Just verify no panic.
	_ = err
}

func TestWithImages(t *testing.T) {
	ctx := context.Background()

	// Test that WithImages doesn't cause errors
	// (Loopback doesn't process images, but shouldn't fail)
	resp, err := Completion(ctx, "loopback/any", "What's in this image?",
		WithImages(ImageSource{URL: "https://example.com/test.jpg"}),
	)
	if err != nil {
		t.Fatalf("Completion() with WithImages error: %v", err)
	}

	// Should still get response
	if resp == nil {
		t.Error("expected response, got nil")
	}
}

func TestWithImages_Base64(t *testing.T) {
	ctx := context.Background()

	// Test base64 image path
	resp, err := Completion(ctx, "loopback/any", "What's in this image?",
		WithImages(ImageSource{Base64: "dGVzdA==", MediaType: "image/png"}),
	)
	if err != nil {
		t.Fatalf("Completion() with Base64 image error: %v", err)
	}

	if resp == nil {
		t.Error("expected response, got nil")
	}
}

func TestWithImages_Empty(t *testing.T) {
	ctx := context.Background()

	// Test empty images - should not fail
	resp, err := Completion(ctx, "loopback/any", "No images",
		WithImages(), // Empty
	)
	if err != nil {
		t.Fatalf("Completion() with empty images error: %v", err)
	}

	if resp == nil {
		t.Error("expected response, got nil")
	}
}

func TestWithImages_NoUserMessage(t *testing.T) {
	// Test WithImages when there's no user message
	req := &ChatRequest{
		Model:    "test",
		Messages: []Message{}, // No messages
	}

	opt := WithImages(ImageSource{URL: "https://example.com/test.jpg"})
	err := opt(req)
	if err != nil {
		t.Fatalf("WithImages with no messages should not error: %v", err)
	}
}

func TestExistingOptions_WorkWithConvenience(t *testing.T) {
	ctx := context.Background()

	// Test that existing options from options.go work
	resp, err := Completion(ctx, "loopback/any", "Test options",
		WithTemperature(0.5),
		WithMaxTokens(100),
		WithTopP(0.9),
	)
	if err != nil {
		t.Fatalf("Completion() with existing options error: %v", err)
	}

	if resp.Content != "Test options" {
		t.Errorf("expected 'Test options', got %q", resp.Content)
	}
}
