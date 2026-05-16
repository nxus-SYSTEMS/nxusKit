package nxuskit

import (
	"context"
)

// LLMProvider is the core interface that all LLM provider implementations must satisfy.
// It provides methods for chat completion, streaming, and model discovery.
//
// Implementations should be safe for concurrent use from multiple goroutines.
type LLMProvider interface {
	// Chat sends a chat completion request and returns the complete response.
	//
	// The context can be used for cancellation and timeouts. If the context
	// is canceled, the request will be aborted and an error returned.
	//
	// Returns an *LLMError on failure with appropriate ErrorKind.
	Chat(ctx context.Context, req *ChatRequest) (*ChatResponse, error)

	// ChatStream sends a chat completion request and returns channels for
	// streaming response chunks and errors.
	//
	// The chunk channel will receive StreamChunk values as they arrive.
	// The final chunk will have FinishReason set. The chunk channel is
	// closed when streaming completes or an error occurs.
	//
	// The error channel will receive at most one error if streaming fails.
	// Check for errors by selecting on both channels or by reading the
	// error channel after the chunk channel closes.
	//
	// Both channels are closed when streaming completes (success or failure).
	//
	// Example:
	//
	//	chunks, errs := provider.ChatStream(ctx, req)
	//	for chunk := range chunks {
	//	    fmt.Print(chunk.Delta)
	//	    if chunk.IsFinal() {
	//	        fmt.Println()
	//	    }
	//	}
	//	if err := <-errs; err != nil {
	//	    log.Fatal(err)
	//	}
	ChatStream(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan error)

	// ListModels returns information about models available from this provider.
	//
	// Not all providers support listing models. Providers that don't support
	// this will return an empty slice without error.
	//
	// Returns an *LLMError on failure.
	ListModels(ctx context.Context) ([]ModelInfo, error)

	// ProviderName returns a string identifying this provider.
	//
	// Examples: "openai", "claude", "ollama", "groq"
	//
	// This is used for logging, error messages, and provider detection.
	ProviderName() string

	// Ping verifies connectivity and authentication with the provider.
	//
	// This is a lightweight health check that does not consume tokens.
	// Use this for connection validation, circuit breakers, or startup checks.
	//
	// Returns nil if the provider is reachable and properly authenticated.
	// Returns an error if:
	//   - Network is unreachable
	//   - Credentials are invalid
	//   - Provider service is unavailable
	//
	// Example:
	//
	//   if err := provider.Ping(ctx); err != nil {
	//       log.Printf("Provider %s not available: %v", provider.ProviderName(), err)
	//   }
	Ping(ctx context.Context) error

	// GetCapabilities returns the features and parameter limits of this provider.
	//
	// Use this to check what features a provider supports before making requests,
	// or to conditionally enable UI features based on provider capabilities.
	//
	// The returned capabilities are static for the provider (not model-dependent).
	// For model-specific capabilities, use GetModelCapabilities on supported providers.
	//
	// Example:
	//
	//   caps := provider.GetCapabilities()
	//   if caps.SupportsVision {
	//       // Enable image upload UI
	//   }
	//   if caps.MaxStopSequences != nil && len(stops) > *caps.MaxStopSequences {
	//       // Truncate stop sequences
	//   }
	GetCapabilities() ProviderCapabilities

	// StreamWithUsage sends a streaming chat request and provides final token usage.
	//
	// This combines ChatStream with reliable token usage tracking. The first channel
	// receives streaming chunks as they arrive. The second channel receives a single
	// TokenUsage value after the stream completes (or on error).
	//
	// If the stream completes successfully, TokenUsage.IsComplete is true.
	// If the stream is interrupted, TokenUsage.IsComplete is false and
	// the token counts may be partial.
	//
	// Example:
	//
	//   chunks, usage := provider.StreamWithUsage(ctx, req)
	//   for chunk := range chunks {
	//       fmt.Print(chunk.Delta)
	//   }
	//   tokenUsage := <-usage
	//   fmt.Printf("Tokens used: %d\n", tokenUsage.TotalTokens())
	StreamWithUsage(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan TokenUsage)
}

// CapabilityDetector is an optional interface for providers that support
// per-model capability detection (currently only Ollama).
//
// Providers that implement this interface can report model-specific capabilities
// such as vision support that varies by model.
type CapabilityDetector interface {
	// GetModelCapabilities returns capabilities for a specific model.
	//
	// This performs an API call to detect model capabilities (e.g., Ollama's show API).
	// Results may be cached by the provider.
	//
	// Returns an error if the model doesn't exist or detection fails.
	GetModelCapabilities(ctx context.Context, model string) (ModelCapabilities, error)
}

// -----------------------------------------------------------------------------
// Rust API Parity Interfaces (FR-LKS-001)
// -----------------------------------------------------------------------------

// SessionResetter is implemented by providers that support explicit session reset.
//
// This interface exists for API consistency with nxuskit-engine, where fresh_session()
// is used to ensure deterministic evaluation for CI/testing scenarios.
//
// Most Go providers are stateless and simply return themselves. Stateful
// providers (like MockProvider with a response queue) create new instances
// with fresh state.
//
// All providers in nxuskit implement this interface.
//
// Example:
//
//	provider, _ := nxuskit.NewMockProvider(
//	    nxuskit.WithMockResponse(&nxuskit.ChatResponse{Content: "Hello!"}),
//	)
//
//	// Get fresh session for deterministic testing
//	fresh, err := provider.FreshSession()
//	if err != nil {
//	    return err
//	}
//
//	// Use fresh provider - response queue is reset
//	resp, _ := fresh.Chat(ctx, req)
type SessionResetter interface {
	// FreshSession returns a provider instance with no accumulated state.
	//
	// For stateless providers (most API-based providers), this returns the
	// receiver unchanged: return p, nil
	//
	// For stateful providers, this creates a new instance with fresh state.
	//
	// The error return follows Go idioms for constructor-like methods, even
	// though most implementations will never return an error. This allows
	// future stateful providers to report initialization failures.
	//
	// Returns:
	//   - LLMProvider: A provider instance with fresh state
	//   - error: Any error encountered during session creation
	FreshSession() (LLMProvider, error)
}

// ModelLister is implemented by providers that support model discovery.
//
// This interface exists for API consistency with nxuskit-engine. In Rust, a separate
// ModelLister trait is required for correct vtable dispatch through trait objects
// (Box<dyn ModelLister>). Go interfaces do not have this limitation since
// interface values dispatch correctly.
//
// We provide this interface for cross-language API parity, enabling developers
// familiar with nxuskit-engine to use the same patterns in nxuskit.
//
// Providers implementing ModelLister:
//   - OllamaProvider
//   - LmStudioProvider
//   - MockProvider
//   - LoopbackProvider
//
// API-based providers (OpenAI, Claude, Groq, etc.) do not implement this
// interface as they don't support dynamic model discovery.
//
// Example:
//
//	provider, _ := nxuskit.NewOllamaProvider()
//	if lister, ok := provider.(nxuskit.ModelLister); ok {
//	    models, err := lister.ListAvailableModels(ctx)
//	    if err != nil {
//	        return err
//	    }
//	    for _, m := range models {
//	        fmt.Printf("Model: %s\n", m.Name)
//	    }
//	}
type ModelLister interface {
	// ListAvailableModels returns models available from this provider.
	//
	// This method is functionally equivalent to LLMProvider.ListModels()
	// but exists on a separate interface for Rust API parity.
	//
	// Returns:
	//   - []ModelInfo: List of available models with their metadata
	//   - error: Any error encountered during model discovery
	ListAvailableModels(ctx context.Context) ([]ModelInfo, error)
}
