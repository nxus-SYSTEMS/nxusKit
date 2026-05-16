package nxuskit

import (
	"context"
	"errors"
	"os"
	"strings"
	"sync"
)

// Default provider sequence: Ollama -> Claude -> OpenAI
const defaultProviderSequence = "ollama,claude,openai"

// ErrNoProviderAvailable indicates no provider in the fallback sequence is available
var ErrNoProviderAvailable = errors.New("nxuskit: no LLM provider available in fallback sequence")

// ProviderFallback manages a sequence of LLM providers with automatic fallback
type ProviderFallback struct {
	mu           sync.RWMutex
	sequence     []string
	providers    map[string]LLMProvider
	configs      map[string]interface{}
	lastProvider string
}

// ProviderFallbackOption configures the fallback mechanism
type ProviderFallbackOption func(*ProviderFallback)

// NewProviderFallback creates a new provider fallback manager
func NewProviderFallback(opts ...ProviderFallbackOption) *ProviderFallback {
	pf := &ProviderFallback{
		providers: make(map[string]LLMProvider),
		configs:   make(map[string]interface{}),
	}

	// Set default sequence from environment or default
	sequence := os.Getenv("LLMKIT_PROVIDER_SEQUENCE")
	if sequence == "" {
		sequence = defaultProviderSequence
	}
	pf.sequence = parseProviderSequence(sequence)

	// Apply options
	for _, opt := range opts {
		opt(pf)
	}

	return pf
}

// WithProviderSequence sets a custom provider sequence
func WithProviderSequence(sequence string) ProviderFallbackOption {
	return func(pf *ProviderFallback) {
		pf.sequence = parseProviderSequence(sequence)
	}
}

// WithProviderSequenceList sets a custom provider sequence from a slice
func WithProviderSequenceList(sequence []string) ProviderFallbackOption {
	return func(pf *ProviderFallback) {
		pf.sequence = sequence
	}
}

// WithOllamaConfig sets Ollama-specific configuration
func WithOllamaConfig(model string) ProviderFallbackOption {
	return func(pf *ProviderFallback) {
		pf.configs["ollama"] = map[string]string{"model": model}
	}
}

// WithClaudeConfig sets Claude-specific configuration
func WithClaudeConfig(apiKey, model string) ProviderFallbackOption {
	return func(pf *ProviderFallback) {
		pf.configs["claude"] = map[string]string{
			"api_key": apiKey,
			"model":   model,
		}
	}
}

// WithOpenAIConfig sets OpenAI-specific configuration
func WithOpenAIConfig(apiKey, model string) ProviderFallbackOption {
	return func(pf *ProviderFallback) {
		pf.configs["openai"] = map[string]string{
			"api_key": apiKey,
			"model":   model,
		}
	}
}

// parseProviderSequence parses a comma-separated provider sequence
func parseProviderSequence(sequence string) []string {
	parts := strings.Split(sequence, ",")
	result := make([]string, 0, len(parts))
	for _, p := range parts {
		p = strings.TrimSpace(strings.ToLower(p))
		if p != "" {
			result = append(result, p)
		}
	}
	return result
}

// GetAvailableProvider returns the first available provider in the sequence
func (pf *ProviderFallback) GetAvailableProvider(ctx context.Context) (LLMProvider, error) {
	pf.mu.Lock()
	defer pf.mu.Unlock()

	for _, name := range pf.sequence {
		// Check if we already have this provider cached
		if provider, ok := pf.providers[name]; ok {
			// Verify it's still available
			if err := provider.Ping(ctx); err == nil {
				pf.lastProvider = name
				return provider, nil
			}
			// Provider no longer available, remove from cache
			delete(pf.providers, name)
		}

		// Try to create the provider
		provider, err := pf.createProvider(ctx, name)
		if err != nil {
			continue // Try next provider
		}

		// Verify it's available
		if err := provider.Ping(ctx); err != nil {
			continue // Try next provider
		}

		// Cache and return
		pf.providers[name] = provider
		pf.lastProvider = name
		return provider, nil
	}

	return nil, ErrNoProviderAvailable
}

// createProvider creates a provider by name
func (pf *ProviderFallback) createProvider(ctx context.Context, name string) (LLMProvider, error) {
	switch name {
	case "ollama":
		return pf.createOllamaProvider()
	case "claude", "anthropic":
		return pf.createClaudeProvider()
	case "openai":
		return pf.createOpenAIProvider()
	case "xai":
		return pf.createXaiProvider()
	case "groq":
		return pf.createGroqProvider()
	case "mistral":
		return pf.createMistralProvider()
	case "perplexity":
		return pf.createPerplexityProvider()
	case "together":
		return pf.createTogetherProvider()
	case "fireworks":
		return pf.createFireworksProvider()
	case "openrouter":
		return pf.createOpenRouterProvider()
	case "lmstudio":
		return pf.createLMStudioProvider()
	default:
		return nil, errors.New("unknown provider: " + name)
	}
}

func (pf *ProviderFallback) createOllamaProvider() (LLMProvider, error) {
	opts := []OllamaOption{}
	if cfg, ok := pf.configs["ollama"].(map[string]string); ok {
		if url := cfg["base_url"]; url != "" {
			opts = append(opts, WithOllamaBaseURL(url))
		}
	}
	return NewOllamaProvider(opts...)
}

func (pf *ProviderFallback) createClaudeProvider() (LLMProvider, error) {
	opts := []ClaudeOption{}
	if cfg, ok := pf.configs["claude"].(map[string]string); ok {
		if key := cfg["api_key"]; key != "" {
			opts = append(opts, WithClaudeAPIKey(key))
		}
		if url := cfg["base_url"]; url != "" {
			opts = append(opts, WithClaudeBaseURL(url))
		}
	}
	return NewClaudeProvider(opts...)
}

func (pf *ProviderFallback) createOpenAIProvider() (LLMProvider, error) {
	opts := []OpenAIOption{}
	if cfg, ok := pf.configs["openai"].(map[string]string); ok {
		if key := cfg["api_key"]; key != "" {
			opts = append(opts, WithOpenAIAPIKey(key))
		}
		if url := cfg["base_url"]; url != "" {
			opts = append(opts, WithOpenAIBaseURL(url))
		}
	}
	return NewOpenAIProvider(opts...)
}

func (pf *ProviderFallback) createGroqProvider() (LLMProvider, error) {
	return NewGroqProvider()
}

func (pf *ProviderFallback) createXaiProvider() (LLMProvider, error) {
	return NewXaiProvider()
}

func (pf *ProviderFallback) createMistralProvider() (LLMProvider, error) {
	return NewMistralProvider()
}

func (pf *ProviderFallback) createPerplexityProvider() (LLMProvider, error) {
	return NewPerplexityProvider()
}

func (pf *ProviderFallback) createTogetherProvider() (LLMProvider, error) {
	return NewTogetherProvider()
}

func (pf *ProviderFallback) createFireworksProvider() (LLMProvider, error) {
	return NewFireworksProvider()
}

func (pf *ProviderFallback) createOpenRouterProvider() (LLMProvider, error) {
	return NewOpenRouterProvider()
}

func (pf *ProviderFallback) createLMStudioProvider() (LLMProvider, error) {
	return NewLmStudioProvider()
}

// GetLastProvider returns the name of the last successfully used provider
func (pf *ProviderFallback) GetLastProvider() string {
	pf.mu.RLock()
	defer pf.mu.RUnlock()
	return pf.lastProvider
}

// GetSequence returns the current provider sequence
func (pf *ProviderFallback) GetSequence() []string {
	pf.mu.RLock()
	defer pf.mu.RUnlock()
	result := make([]string, len(pf.sequence))
	copy(result, pf.sequence)
	return result
}

// SetSequence updates the provider sequence
func (pf *ProviderFallback) SetSequence(sequence []string) {
	pf.mu.Lock()
	defer pf.mu.Unlock()
	pf.sequence = sequence
	// Clear cached providers when sequence changes
	pf.providers = make(map[string]LLMProvider)
}

// Chat performs a chat request using the first available provider
func (pf *ProviderFallback) Chat(ctx context.Context, req *ChatRequest) (*ChatResponse, error) {
	provider, err := pf.GetAvailableProvider(ctx)
	if err != nil {
		return nil, err
	}
	return provider.Chat(ctx, req)
}

// ChatStream performs a streaming chat request using the first available provider
func (pf *ProviderFallback) ChatStream(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan error) {
	chunks := make(chan StreamChunk)
	errs := make(chan error, 1)

	go func() {
		defer close(chunks)
		defer close(errs)

		provider, err := pf.GetAvailableProvider(ctx)
		if err != nil {
			errs <- err
			return
		}

		providerChunks, providerErrs := provider.ChatStream(ctx, req)

		for chunk := range providerChunks {
			chunks <- chunk
		}

		if err := <-providerErrs; err != nil {
			errs <- err
		}
	}()

	return chunks, errs
}

// ListModels returns models from all available providers
func (pf *ProviderFallback) ListModels(ctx context.Context) ([]ModelInfo, error) {
	provider, err := pf.GetAvailableProvider(ctx)
	if err != nil {
		return nil, err
	}
	return provider.ListModels(ctx)
}

// ProviderName returns "fallback"
func (pf *ProviderFallback) ProviderName() string {
	return "fallback"
}

// Ping checks if any provider in the sequence is available
func (pf *ProviderFallback) Ping(ctx context.Context) error {
	_, err := pf.GetAvailableProvider(ctx)
	return err
}

// GetCapabilities returns capabilities of the first available provider
func (pf *ProviderFallback) GetCapabilities() ProviderCapabilities {
	ctx := context.Background()
	provider, err := pf.GetAvailableProvider(ctx)
	if err != nil {
		return DefaultCapabilities()
	}
	return provider.GetCapabilities()
}

// StreamWithUsage performs a streaming chat with usage tracking
func (pf *ProviderFallback) StreamWithUsage(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan TokenUsage) {
	chunks := make(chan StreamChunk)
	usage := make(chan TokenUsage, 1)

	go func() {
		defer close(chunks)
		defer close(usage)

		provider, err := pf.GetAvailableProvider(ctx)
		if err != nil {
			usage <- TokenUsage{IsComplete: false}
			return
		}

		providerChunks, providerUsage := provider.StreamWithUsage(ctx, req)

		for chunk := range providerChunks {
			chunks <- chunk
		}

		usage <- <-providerUsage
	}()

	return chunks, usage
}

// Ensure ProviderFallback implements LLMProvider
var _ LLMProvider = (*ProviderFallback)(nil)
