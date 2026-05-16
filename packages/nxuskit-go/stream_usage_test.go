package nxuskit

import (
	"context"
	"testing"
	"time"
)

func TestStreamWithUsage_ReturnsTokenUsageAfterCompleteStream(t *testing.T) {
	// Use testMockProvider which has ChatStreamFunc field
	provider := &testMockProvider{Name: "test"}
	ctx := context.Background()

	req, err := NewChatRequest("test-model", WithMessages(UserMessage("Hello")))
	if err != nil {
		t.Fatalf("NewChatRequest error: %v", err)
	}

	chunks, usage := provider.StreamWithUsage(ctx, req)

	// Consume all chunks
	var content string
	for chunk := range chunks {
		content += chunk.Delta
	}

	// Get usage from channel
	tokenUsage := <-usage

	if content == "" {
		t.Error("expected content from stream")
	}

	// Verify usage is populated
	if tokenUsage.TotalTokens() == 0 {
		t.Error("expected non-zero token usage")
	}

	if !tokenUsage.IsComplete {
		t.Error("expected IsComplete=true for successful stream")
	}
}

func TestStreamWithUsage_ReturnsIsCompleteFalseOnError(t *testing.T) {
	provider := &testMockProvider{
		ChatStreamFunc: func(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan error) {
			chunks := make(chan StreamChunk, 1)
			errs := make(chan error, 1)

			go func() {
				defer close(chunks)
				defer close(errs)

				// Send one chunk then error
				chunks <- NewStreamChunk("partial")
				errs <- NewStreamError("test", "stream interrupted", nil)
			}()

			return chunks, errs
		},
	}

	ctx := context.Background()
	req, _ := NewChatRequest("test-model")

	chunks, usage := provider.StreamWithUsage(ctx, req)

	// Consume all chunks
	for range chunks {
	}

	// Get usage from channel
	tokenUsage := <-usage

	// Stream errored, so IsComplete should be false
	if tokenUsage.IsComplete {
		t.Error("expected IsComplete=false for errored stream")
	}
}

func TestStreamWithUsage_CapturesUsageFromFinalChunk(t *testing.T) {
	expectedPrompt := 15
	expectedCompletion := 25

	provider := &testMockProvider{
		ChatStreamFunc: func(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan error) {
			chunks := make(chan StreamChunk, 3)
			errs := make(chan error, 1)

			go func() {
				defer close(chunks)
				defer close(errs)

				chunks <- NewStreamChunk("Hello")
				chunks <- NewStreamChunk(" world")
				chunks <- FinalChunk("!", FinishReasonStop, &TokenUsage{
					Estimated: TokenCount{
						PromptTokens:     expectedPrompt,
						CompletionTokens: expectedCompletion,
					},
					IsComplete: true,
				})
			}()

			return chunks, errs
		},
	}

	ctx := context.Background()
	req, _ := NewChatRequest("test-model")

	chunks, usage := provider.StreamWithUsage(ctx, req)

	// Consume all chunks
	for range chunks {
	}

	// Get usage from channel
	tokenUsage := <-usage

	// Verify the captured usage matches what was in the final chunk
	if tokenUsage.Estimated.PromptTokens != expectedPrompt {
		t.Errorf("expected prompt tokens %d, got %d", expectedPrompt, tokenUsage.Estimated.PromptTokens)
	}

	if tokenUsage.Estimated.CompletionTokens != expectedCompletion {
		t.Errorf("expected completion tokens %d, got %d", expectedCompletion, tokenUsage.Estimated.CompletionTokens)
	}

	if tokenUsage.TotalTokens() != expectedPrompt+expectedCompletion {
		t.Errorf("expected total tokens %d, got %d", expectedPrompt+expectedCompletion, tokenUsage.TotalTokens())
	}
}

func TestStreamWithUsage_ContextCancellation(t *testing.T) {
	provider := &testMockProvider{
		ChatStreamFunc: func(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan error) {
			chunks := make(chan StreamChunk)
			errs := make(chan error, 1)

			go func() {
				defer close(chunks)
				defer close(errs)

				// Simulate slow streaming - wait for context cancellation
				select {
				case <-ctx.Done():
					errs <- ctx.Err()
				case <-time.After(5 * time.Second):
					// Should not reach here in test
				}
			}()

			return chunks, errs
		},
	}

	ctx, cancel := context.WithCancel(context.Background())
	req, _ := NewChatRequest("test-model")

	chunks, usage := provider.StreamWithUsage(ctx, req)

	// Cancel immediately
	cancel()

	// Drain chunks
	for range chunks {
	}

	// Get usage - should indicate incomplete
	tokenUsage := <-usage

	if tokenUsage.IsComplete {
		t.Error("expected IsComplete=false after context cancellation")
	}
}
