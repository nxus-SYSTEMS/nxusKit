package nxuskit

import (
	"context"
	"time"
)

// DefaultTimeout is the default timeout for convenience functions.
const DefaultTimeout = 2 * time.Minute

// Completion sends a single prompt to an auto-detected provider and returns the response.
//
// The provider is determined by:
//  1. Explicit prefix: "provider/model" (e.g., "openai/gpt-4o")
//  2. Model pattern matching (e.g., "gpt-*" → OpenAI, "claude-*" → Claude)
//  3. Environment variable availability
//
// Example:
//
//	// Simple completion with auto-detected provider
//	resp, err := nxuskit.Completion(ctx, "gpt-4o", "What is 2+2?")
//
//	// With options
//	resp, err := nxuskit.Completion(ctx, "gpt-4o", "Hello",
//	    nxuskit.WithTemperature(0.7),
//	    nxuskit.WithMaxTokens(100),
//	)
//
//	// Explicit provider
//	resp, err := nxuskit.Completion(ctx, "openrouter/anthropic/claude-3.5-sonnet", "Hello")
func Completion(ctx context.Context, model, prompt string, opts ...Option) (*ChatResponse, error) {
	messages := []Message{UserMessage(prompt)}
	return CompletionWithMessages(ctx, model, messages, opts...)
}

// CompletionStream sends a streaming request to an auto-detected provider.
//
// Returns two channels:
//   - chunks: receives StreamChunk values as they arrive
//   - errs: receives at most one error if streaming fails
//
// Example:
//
//	chunks, errs := nxuskit.CompletionStream(ctx, "gpt-4o", "Tell me a story")
//	for chunk := range chunks {
//	    fmt.Print(chunk.Delta)
//	}
//	if err := <-errs; err != nil {
//	    log.Fatal(err)
//	}
func CompletionStream(ctx context.Context, model, prompt string, opts ...Option) (<-chan StreamChunk, <-chan error) {
	messages := []Message{UserMessage(prompt)}
	return CompletionWithMessagesStream(ctx, model, messages, opts...)
}

// CompletionWithMessages sends a multi-turn conversation to an auto-detected provider.
//
// Use this when you need to include conversation history or multiple messages.
//
// Example:
//
//	messages := []nxuskit.Message{
//	    nxuskit.SystemMessage("You are a helpful assistant."),
//	    nxuskit.UserMessage("What is the capital of France?"),
//	    nxuskit.AssistantMessage("The capital of France is Paris."),
//	    nxuskit.UserMessage("What about Germany?"),
//	}
//	resp, err := nxuskit.CompletionWithMessages(ctx, "gpt-4o", messages)
func CompletionWithMessages(ctx context.Context, model string, messages []Message, opts ...Option) (*ChatResponse, error) {
	// Check context before doing anything
	if err := ctx.Err(); err != nil {
		return nil, err
	}

	// Parse model identifier
	id := ParseModel(model)

	// Get provider
	provider, err := GetProviderForModel(id)
	if err != nil {
		return nil, err
	}

	// Build request with the actual model name (not the full identifier)
	req, err := buildRequest(id.ModelName, messages, opts)
	if err != nil {
		return nil, err
	}

	// Apply timeout if specified in options
	ctx, cancel := applyTimeoutFromRequest(ctx, req)
	defer cancel()

	// Call provider
	return provider.Chat(ctx, req)
}

// CompletionWithMessagesStream sends a streaming multi-turn conversation to an auto-detected provider.
//
// Combines the flexibility of multi-turn conversations with streaming output.
func CompletionWithMessagesStream(ctx context.Context, model string, messages []Message, opts ...Option) (<-chan StreamChunk, <-chan error) {
	chunks := make(chan StreamChunk)
	errs := make(chan error, 1)

	go func() {
		defer close(chunks)
		defer close(errs)

		// Check context before doing anything
		if err := ctx.Err(); err != nil {
			errs <- err
			return
		}

		// Parse model identifier
		id := ParseModel(model)

		// Get provider
		provider, err := GetProviderForModel(id)
		if err != nil {
			errs <- err
			return
		}

		// Build request with the actual model name
		req, err := buildRequest(id.ModelName, messages, opts)
		if err != nil {
			errs <- err
			return
		}

		// Apply timeout if specified in options
		ctx, cancel := applyTimeoutFromRequest(ctx, req)
		defer cancel()

		// Call provider's stream method
		providerChunks, providerErrs := provider.ChatStream(ctx, req)

		// Forward chunks
		for chunk := range providerChunks {
			select {
			case <-ctx.Done():
				errs <- ctx.Err()
				return
			case chunks <- chunk:
			}
		}

		// Forward error if any
		if err := <-providerErrs; err != nil {
			errs <- err
		}
	}()

	return chunks, errs
}

// buildRequest creates a ChatRequest from messages and options.
func buildRequest(model string, messages []Message, opts []Option) (*ChatRequest, error) {
	// Create base request
	req := &ChatRequest{
		Model:        model,
		Messages:     messages,
		ThinkingMode: ThinkingModeAuto,
	}

	// Apply all options (including WithSystemPrompt, WithImages, WithTimeout)
	for _, opt := range opts {
		if err := opt(req); err != nil {
			return nil, err
		}
	}

	return req, nil
}

// applyTimeoutFromRequest applies the timeout from convenienceConfig to the context.
// This is called after buildRequest to apply any timeout specified via WithTimeout.
// Returns the new context and a cancel function that should be deferred.
func applyTimeoutFromRequest(ctx context.Context, req *ChatRequest) (context.Context, context.CancelFunc) {
	// Check if timeout was specified via WithTimeout option
	if timeout, ok := getTimeoutFromRequest(req); ok {
		// Check if context already has a deadline
		if deadline, hasDeadline := ctx.Deadline(); hasDeadline {
			// Use the shorter of the two
			proposedDeadline := time.Now().Add(timeout)
			if proposedDeadline.Before(deadline) {
				return context.WithTimeout(ctx, timeout)
			}
			// Existing deadline is shorter, return a no-op cancel
			return ctx, func() {}
		}
		return context.WithTimeout(ctx, timeout)
	} else if _, ok := ctx.Deadline(); !ok {
		// No timeout specified and no deadline set, apply default timeout
		return context.WithTimeout(ctx, DefaultTimeout)
	}
	// Context already has a deadline and no timeout option specified
	return ctx, func() {}
}
