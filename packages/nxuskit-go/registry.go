package nxuskit

import (
	"fmt"
	"os"
	"regexp"
	"sync"
)

// ProviderFactory is a function that creates a provider instance.
// It should read configuration from environment variables.
type ProviderFactory func() (LLMProvider, error)

// providerPattern matches model names to providers.
type providerPattern struct {
	name    string           // Provider name (e.g., "openai")
	regex   []*regexp.Regexp // Compiled patterns
	envVar  string           // Required environment variable
	factory ProviderFactory
}

// providerRegistry manages model pattern to provider mapping.
type providerRegistry struct {
	mu       sync.RWMutex
	patterns []providerPattern          // Ordered by priority
	byName   map[string]ProviderFactory // Direct lookup by name
}

// Global singleton (lazy-initialized)
var (
	registry     *providerRegistry
	registryOnce sync.Once
)

// getRegistry returns the global provider registry, initializing it if needed.
func getRegistry() *providerRegistry {
	registryOnce.Do(func() {
		registry = &providerRegistry{
			patterns: make([]providerPattern, 0),
			byName:   make(map[string]ProviderFactory),
		}
		// Register default providers directly (not via RegisterProvider to avoid lock)
		registerDefaultProvidersInternal(registry)
	})
	return registry
}

// registerDefaultProvidersInternal registers all built-in providers with their model patterns.
// This is called during initialization and directly modifies the registry without locking.
func registerDefaultProvidersInternal(reg *providerRegistry) {
	// Helper to add a provider
	addProvider := func(name string, patterns []string, envVar string, factory ProviderFactory) {
		var regexes []*regexp.Regexp
		for _, p := range patterns {
			if p != "" {
				re, err := regexp.Compile(p)
				if err == nil {
					regexes = append(regexes, re)
				}
			}
		}
		reg.patterns = append(reg.patterns, providerPattern{
			name:    name,
			regex:   regexes,
			envVar:  envVar,
			factory: factory,
		})
		reg.byName[name] = factory
	}

	// Cloud providers (priority order - more specific patterns first)
	addProvider("openai", []string{`^gpt-`, `^o1-`, `^o3-`, `^text-davinci-`, `^text-embedding-`}, "OPENAI_API_KEY", func() (LLMProvider, error) {
		return NewOpenAIProvider()
	})

	addProvider("claude", []string{`^claude-`}, "ANTHROPIC_API_KEY", func() (LLMProvider, error) {
		return NewClaudeProvider()
	})

	addProvider("mistral", []string{`^mistral-`, `^open-mistral-`, `^codestral-`}, "MISTRAL_API_KEY", func() (LLMProvider, error) {
		return NewMistralProvider()
	})

	addProvider("groq", []string{`^llama.*-groq-`, `^mixtral-`, `^gemma`}, "GROQ_API_KEY", func() (LLMProvider, error) {
		return NewGroqProvider()
	})

	addProvider("perplexity", []string{`^llama-.*-sonar-`, `^pplx-`}, "PERPLEXITY_API_KEY", func() (LLMProvider, error) {
		return NewPerplexityProvider()
	})

	addProvider("fireworks", []string{`^accounts/fireworks/`}, "FIREWORKS_API_KEY", func() (LLMProvider, error) {
		return NewFireworksProvider()
	})

	addProvider("xai", []string{`^grok-`}, "XAI_API_KEY", func() (LLMProvider, error) {
		return NewXaiProvider()
	})

	addProvider("together", []string{`^together/`, `^meta-llama/`, `^mistralai/`}, "TOGETHER_API_KEY", func() (LLMProvider, error) {
		return NewTogetherProvider()
	})

	addProvider("openrouter", []string{`^openrouter/`}, "OPENROUTER_API_KEY", func() (LLMProvider, error) {
		return NewOpenRouterProvider()
	})

	// Local providers (lower priority)
	addProvider("lmstudio", []string{}, "LMSTUDIO_HOST", func() (LLMProvider, error) {
		return NewLmStudioProvider()
	})

	// Utility providers (always available, no env var needed)
	addProvider("loopback", []string{}, "", func() (LLMProvider, error) {
		return NewLoopbackProvider(), nil
	})

	addProvider("mock", []string{}, "", func() (LLMProvider, error) {
		return NewMockProvider(), nil
	})

	// Ollama as catch-all fallback (must be last)
	addProvider("ollama", []string{`.*`}, "OLLAMA_HOST", func() (LLMProvider, error) {
		return NewOllamaProvider()
	})
}

// RegisterProvider registers a provider factory with the global registry.
//
// Parameters:
//   - name: Provider identifier (e.g., "openai", "claude")
//   - patterns: Regex patterns that match model names (e.g., "^gpt-", "^claude-")
//   - envVar: Environment variable that must be set for auto-detection (empty string means always available)
//   - factory: Function to create the provider instance
//
// Providers are matched in registration order for auto-detection.
// Custom providers registered after built-in providers can override by name.
func RegisterProvider(name string, patterns []string, envVar string, factory ProviderFactory) {
	reg := getRegistry()
	reg.mu.Lock()
	defer reg.mu.Unlock()

	// Compile regex patterns
	var regexes []*regexp.Regexp
	for _, p := range patterns {
		if p != "" {
			re, err := regexp.Compile(p)
			if err == nil {
				regexes = append(regexes, re)
			}
		}
	}

	// Add to patterns list (for auto-detection)
	reg.patterns = append(reg.patterns, providerPattern{
		name:    name,
		regex:   regexes,
		envVar:  envVar,
		factory: factory,
	})

	// Add to name lookup (for explicit provider)
	reg.byName[name] = factory
}

// GetProviderForModel returns a provider instance for the given model identifier.
//
// Resolution order:
//  1. If ModelIdentifier.Provider is set, look up by name
//  2. Otherwise, match patterns in priority order (first match with env var set wins)
//
// Returns ErrConfiguration if:
//   - Explicit provider name not found
//   - No patterns match the model name
//   - Matching provider's required env var is not set
func GetProviderForModel(id ModelIdentifier) (LLMProvider, error) {
	reg := getRegistry()
	reg.mu.RLock()
	defer reg.mu.RUnlock()

	// Explicit provider lookup
	if id.IsExplicit() {
		factory, ok := reg.byName[id.Provider]
		if !ok {
			return nil, NewConfigurationError(fmt.Sprintf("unknown provider %q", id.Provider), nil)
		}
		return factory()
	}

	// Auto-detection via pattern matching
	modelName := id.ModelName

	for _, p := range reg.patterns {
		// Skip providers with no patterns - they're only for explicit use
		if len(p.regex) == 0 {
			continue
		}

		// Check if any pattern matches
		matched := false
		for _, re := range p.regex {
			if re.MatchString(modelName) {
				matched = true
				break
			}
		}

		if !matched {
			continue
		}

		// Pattern matched - check if env var is set (if required)
		if p.envVar != "" {
			if os.Getenv(p.envVar) == "" {
				// Pattern matched but env var not set - keep looking
				// But track this for better error message
				continue
			}
		}

		// Found a match with required env var set
		return p.factory()
	}

	// No match found - find the first pattern that matched to give better error
	for _, p := range reg.patterns {
		for _, re := range p.regex {
			if re.MatchString(modelName) {
				// Pattern matched but env var was missing
				return nil, NewConfigurationError(
					fmt.Sprintf("cannot use model %q: %s requires API key. Set %s environment variable or use explicit provider prefix",
						modelName, p.name, p.envVar),
					nil,
				)
			}
		}
	}

	// No patterns matched at all
	return nil, NewConfigurationError(
		fmt.Sprintf("cannot determine provider for model %q: no matching patterns", modelName),
		nil,
	)
}
