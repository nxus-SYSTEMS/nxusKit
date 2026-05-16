// Package nxuskit provides a unified Go interface for multiple LLM providers.
//
// nxuskit is designed to enable developers to interact with various Large Language Model
// providers (OpenAI, Claude, Ollama, etc.) through a consistent API. It provides:
//
//   - Convenience API: Simple functions like Completion() with auto-detection
//   - ChatRequest/ChatResponse types for synchronous chat completions
//   - StreamChunk for incremental streaming responses
//   - LLMProvider interface for implementing custom providers
//   - Typed error handling with errors.Is/errors.As support
//   - Vision/multimodal message support
//   - Thinking mode for chain-of-thought reasoning
//   - Tool/function calling with ToolChoice control
//   - Response format control (text, JSON, JSON Schema)
//   - Advanced sampling: TopK, MinP (provider-dependent)
//   - InferenceMetadata: Structured access to completion status and token usage
//   - SessionResetter: Deterministic testing with fresh provider sessions
//   - ModelLister: Dynamic model discovery for local providers
//
// # Convenience API (Recommended for Getting Started)
//
// The convenience API provides the simplest way to get started. Providers are
// auto-detected from model names or can be explicitly specified:
//
//	// Auto-detected from model name (uses OPENAI_API_KEY)
//	resp, err := nxuskit.Completion(ctx, "gpt-4o", "What is 2+2?")
//
//	// Explicit provider prefix
//	resp, err := nxuskit.Completion(ctx, "openrouter/anthropic/claude-3.5-sonnet", "Hello")
//
//	// With options
//	resp, err := nxuskit.Completion(ctx, "gpt-4o", "Hello",
//	    nxuskit.WithTemperature(0.7),
//	    nxuskit.WithSystemPrompt("You are a helpful assistant."),
//	)
//
//	// Streaming
//	chunks, errs := nxuskit.CompletionStream(ctx, "gpt-4o", "Tell me a story")
//	for chunk := range chunks {
//	    fmt.Print(chunk.Delta)
//	}
//	if err := <-errs; err != nil {
//	    log.Fatal(err)
//	}
//
//	// Multi-turn conversation
//	messages := []nxuskit.Message{
//	    nxuskit.SystemMessage("You are a helpful assistant."),
//	    nxuskit.UserMessage("Hello!"),
//	    nxuskit.AssistantMessage("Hi there! How can I help you today?"),
//	    nxuskit.UserMessage("What's the weather like?"),
//	}
//	resp, err := nxuskit.CompletionWithMessages(ctx, "gpt-4o", messages)
//
// # Model Name Patterns
//
// The convenience API auto-detects providers from model name patterns:
//
//   - "gpt-*", "o1-*", "o3-*" → OpenAI (requires OPENAI_API_KEY)
//   - "claude-*" → Claude (requires ANTHROPIC_API_KEY)
//   - "mistral-*" → Mistral (requires MISTRAL_API_KEY)
//   - "grok-*" → xAI Grok (requires XAI_API_KEY)
//   - "llama-*-groq-*" → Groq (requires GROQ_API_KEY)
//   - Other models → Ollama fallback (requires OLLAMA_HOST)
//
// Use explicit prefix to override: "openrouter/gpt-4o", "ollama/llama3"
//
// # Traditional API
//
// For more control, create providers and requests directly:
//
//	req, err := nxuskit.NewChatRequest("gpt-4o",
//	    nxuskit.WithMessages(
//	        nxuskit.SystemMessage("You are a helpful assistant."),
//	        nxuskit.UserMessage("Hello!"),
//	    ),
//	    nxuskit.WithTemperature(0.7),
//	)
//
// # Error Handling
//
// nxuskit uses typed errors that support Go's errors.Is and errors.As:
//
//	resp, err := provider.Chat(ctx, req)
//	if errors.Is(err, nxuskit.ErrRateLimit) {
//	    // Handle rate limiting
//	}
//
//	var llmErr *nxuskit.LLMError
//	if errors.As(err, &llmErr) {
//	    if llmErr.IsRetryable() {
//	        time.Sleep(llmErr.RetryAfter)
//	        // Retry...
//	    }
//	}
//
// # Streaming
//
// For streaming responses, use ChatStream which returns channels:
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
//
// # Tool/Function Calling
//
// Define tools the model can call to perform actions:
//
//	weatherTool := nxuskit.NewTool(
//	    "get_weather",
//	    "Get the current weather for a location",
//	    map[string]any{
//	        "type": "object",
//	        "properties": map[string]any{
//	            "location": map[string]any{"type": "string"},
//	        },
//	        "required": []string{"location"},
//	    },
//	)
//
//	req, _ := nxuskit.NewChatRequest("gpt-4o",
//	    nxuskit.WithMessages(nxuskit.UserMessage("What's the weather in Tokyo?")),
//	    nxuskit.WithTools(weatherTool),
//	    nxuskit.WithToolChoice(nxuskit.ToolChoiceAuto()),
//	)
//
// # Response Format
//
// Control the output format using ResponseFormat:
//
//	// Request JSON output
//	req, _ := nxuskit.NewChatRequest("gpt-4o",
//	    nxuskit.WithMessages(nxuskit.UserMessage("List 3 colors as JSON")),
//	    nxuskit.WithResponseFormat(nxuskit.ResponseFormatJSON()),
//	)
//
//	// Request structured output with JSON Schema
//	schema := map[string]any{
//	    "type": "object",
//	    "properties": map[string]any{
//	        "colors": map[string]any{"type": "array", "items": map[string]any{"type": "string"}},
//	    },
//	}
//	req, _ := nxuskit.NewChatRequest("gpt-4o",
//	    nxuskit.WithMessages(nxuskit.UserMessage("List 3 colors")),
//	    nxuskit.WithResponseFormat(nxuskit.ResponseFormatJSONSchema("colors", schema)),
//	)
//
// # Provider Implementation
//
// To implement a custom provider, implement the LLMProvider interface:
//
//	type LLMProvider interface {
//	    Chat(ctx context.Context, req *ChatRequest) (*ChatResponse, error)
//	    ChatStream(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan error)
//	    ListModels(ctx context.Context) ([]ModelInfo, error)
//	    ProviderName() string
//	}
//
// # Inference Metadata
//
// ChatResponse includes InferenceMetadata with structured access to completion status,
// token usage, thinking traces, and inference steps:
//
//	resp, _ := provider.Chat(ctx, req)
//	if resp.InferenceMetadata.IsComplete {
//	    fmt.Printf("Completed with reason: %s\n", *resp.InferenceMetadata.FinishReason)
//	}
//	if resp.InferenceMetadata.TokenUsage != nil {
//	    fmt.Printf("Tokens: %d\n", resp.InferenceMetadata.TokenUsage.Actual.TotalTokens())
//	}
//
// # Session Reset (Testing)
//
// All providers implement SessionResetter for deterministic testing. Stateful providers
// like MockProvider return a fresh instance, while stateless providers return themselves:
//
//	fresh, _ := provider.FreshSession()
//	// fresh has reset state for deterministic test behavior
//
// # Model Discovery
//
// Local providers (Ollama, LM Studio, Mock, Loopback) implement ModelLister for
// dynamic model discovery:
//
//	if lister, ok := provider.(nxuskit.ModelLister); ok {
//	    models, _ := lister.ListAvailableModels(ctx)
//	    for _, m := range models {
//	        fmt.Println(m.Name)
//	    }
//	}
package nxuskit
