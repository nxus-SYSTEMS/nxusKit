//go:build nxuskit

package nxuskit

// This file provides FFI-backed provider constructors that delegate to
// libnxuskit via the C ABI. These are drop-in replacements for the native
// Go implementations and implement the same LLMProvider interface.
//
// Each constructor accepts the same functional options as the native version,
// marshals the configuration into a JSON config string, and creates a
// nxuskit-backed provider.

import (
	"encoding/json"
	"fmt"
	"os"
	"time"

	"github.com/nxus-SYSTEMS/nxusKit/packages/nxuskit-go/internal/ffi"
)

// ── Claude ──────────────────────────────────────────────────

// NewClaudeFFIProvider creates a Claude provider backed by libnxuskit.
func NewClaudeFFIProvider(opts ...ClaudeOption) (LLMProvider, error) {
	cfg := &claudeConfig{
		baseURL: claudeDefaultBaseURL,
		timeout: 30 * time.Second,
	}
	if envKey := os.Getenv("ANTHROPIC_API_KEY"); envKey != "" {
		cfg.apiKey = envKey
	}
	for _, opt := range opts {
		if err := opt(cfg); err != nil {
			return nil, NewConfigurationError(err.Error(), err)
		}
	}
	if cfg.apiKey == "" {
		return nil, NewConfigurationError("API key is required for claude provider", nil)
	}
	return newFFIProvider("claude", ffiProviderConfig{
		ProviderType: "claude",
		APIKey:       cfg.apiKey,
		BaseURL:      cfg.baseURL,
		TimeoutMs:    cfg.timeout.Milliseconds(),
	}, claudeDefaultCapabilities())
}

func claudeDefaultCapabilities() ProviderCapabilities {
	maxStop := 8192
	return ProviderCapabilities{
		SupportsSystemMessages: true,
		SupportsStreaming:      true,
		SupportsVision:         true,
		MaxStopSequences:       &maxStop,
		SupportsTools:          true,
		SupportsTopK:           true,
		SupportsJSONMode:       true,
	}
}

// ── OpenAI ──────────────────────────────────────────────────

// NewOpenAIFFIProvider creates an OpenAI provider backed by libnxuskit.
func NewOpenAIFFIProvider(opts ...OpenAIOption) (LLMProvider, error) {
	cfg := &openaiConfig{
		baseURL: openaiDefaultBaseURL,
		timeout: 30 * time.Second,
	}
	if envKey := os.Getenv("OPENAI_API_KEY"); envKey != "" {
		cfg.apiKey = envKey
	}
	for _, opt := range opts {
		if err := opt(cfg); err != nil {
			return nil, NewConfigurationError(err.Error(), err)
		}
	}
	if cfg.apiKey == "" {
		return nil, NewConfigurationError("API key is required for openai provider", nil)
	}
	return newFFIProvider("openai", ffiProviderConfig{
		ProviderType: "openai",
		APIKey:       cfg.apiKey,
		BaseURL:      cfg.baseURL,
		TimeoutMs:    cfg.timeout.Milliseconds(),
	}, openAIDefaultCapabilities())
}

func openAIDefaultCapabilities() ProviderCapabilities {
	return ProviderCapabilities{
		SupportsSystemMessages:   true,
		SupportsStreaming:        true,
		SupportsVision:           true,
		SupportsPresencePenalty:  true,
		SupportsFrequencyPenalty: true,
		SupportsSeed:             true,
		SupportsLogprobs:         true,
		SupportsJSONMode:         true,
		SupportsJSONSchema:       true,
		SupportsTools:            true,
		SupportsResponseFormat:   true,
	}
}

// ── Ollama ──────────────────────────────────────────────────

// NewOllamaFFIProvider creates an Ollama provider backed by libnxuskit.
func NewOllamaFFIProvider(opts ...OllamaOption) (LLMProvider, error) {
	cfg := &ollamaConfig{
		baseURL: ollamaDefaultBaseURL,
		timeout: 120 * time.Second,
	}
	if envURL := os.Getenv("OLLAMA_HOST"); envURL != "" {
		cfg.baseURL = envURL
	}
	for _, opt := range opts {
		if err := opt(cfg); err != nil {
			return nil, NewConfigurationError(err.Error(), err)
		}
	}
	return newFFIProvider("ollama", ffiProviderConfig{
		ProviderType: "ollama",
		BaseURL:      cfg.baseURL,
		TimeoutMs:    cfg.timeout.Milliseconds(),
	}, ollamaDefaultCapabilities())
}

func ollamaDefaultCapabilities() ProviderCapabilities {
	return ProviderCapabilities{
		SupportsSystemMessages: true,
		SupportsStreaming:      true,
	}
}

// ── LM Studio ───────────────────────────────────────────────

// NewLmStudioFFIProvider creates an LM Studio provider backed by libnxuskit.
func NewLmStudioFFIProvider(opts ...LmStudioOption) (LLMProvider, error) {
	cfg := &lmstudioConfig{
		baseURL: lmstudioDefaultBaseURL,
		timeout: 120 * time.Second,
	}
	for _, opt := range opts {
		if err := opt(cfg); err != nil {
			return nil, NewConfigurationError(err.Error(), err)
		}
	}
	return newFFIProvider("lmstudio", ffiProviderConfig{
		ProviderType: "lmstudio",
		BaseURL:      cfg.baseURL,
		TimeoutMs:    cfg.timeout.Milliseconds(),
	}, ProviderCapabilities{
		SupportsSystemMessages: true,
		SupportsStreaming:      true,
	})
}

// ── Groq ────────────────────────────────────────────────────

// NewGroqFFIProvider creates a Groq provider backed by libnxuskit.
func NewGroqFFIProvider(opts ...GroqOption) (LLMProvider, error) {
	cfg := &groqConfig{
		timeout: 30 * time.Second,
	}
	if envKey := os.Getenv("GROQ_API_KEY"); envKey != "" {
		cfg.apiKey = envKey
	}
	for _, opt := range opts {
		if err := opt(cfg); err != nil {
			return nil, NewConfigurationError(err.Error(), err)
		}
	}
	if cfg.apiKey == "" {
		return nil, NewConfigurationError("API key is required for groq provider", nil)
	}
	return newFFIProvider("groq", ffiProviderConfig{
		ProviderType: "groq",
		APIKey:       cfg.apiKey,
		TimeoutMs:    cfg.timeout.Milliseconds(),
	}, ProviderCapabilities{
		SupportsSystemMessages: true,
		SupportsStreaming:      true,
		SupportsTools:          true,
		SupportsJSONMode:       true,
	})
}

// ── xAI Grok ─────────────────────────────────────────────────

// NewXaiFFIProvider creates an xAI Grok provider backed by libnxuskit.
func NewXaiFFIProvider(opts ...XaiOption) (LLMProvider, error) {
	cfg := &xaiConfig{
		baseURL: xaiDefaultBaseURL,
		timeout: 30 * time.Second,
	}
	if envKey := os.Getenv("XAI_API_KEY"); envKey != "" {
		cfg.apiKey = envKey
	}
	for _, opt := range opts {
		if err := opt(cfg); err != nil {
			return nil, NewConfigurationError(err.Error(), err)
		}
	}
	if cfg.apiKey == "" {
		return nil, NewConfigurationError("API key is required for xai provider", nil)
	}
	return newFFIProvider("xai", ffiProviderConfig{
		ProviderType: "xai",
		APIKey:       cfg.apiKey,
		BaseURL:      cfg.baseURL,
		TimeoutMs:    cfg.timeout.Milliseconds(),
	}, ProviderCapabilities{
		SupportsSystemMessages: true,
		SupportsStreaming:      true,
		SupportsVision:         true,
		SupportsTools:          true,
		SupportsJSONMode:       true,
		SupportsJSONSchema:     true,
	})
}

// ── Fireworks ───────────────────────────────────────────────

// NewFireworksFFIProvider creates a Fireworks provider backed by libnxuskit.
func NewFireworksFFIProvider(opts ...FireworksOption) (LLMProvider, error) {
	cfg := &fireworksConfig{
		timeout: 30 * time.Second,
	}
	if envKey := os.Getenv("FIREWORKS_API_KEY"); envKey != "" {
		cfg.apiKey = envKey
	}
	for _, opt := range opts {
		if err := opt(cfg); err != nil {
			return nil, NewConfigurationError(err.Error(), err)
		}
	}
	if cfg.apiKey == "" {
		return nil, NewConfigurationError("API key is required for fireworks provider", nil)
	}
	return newFFIProvider("fireworks", ffiProviderConfig{
		ProviderType: "fireworks",
		APIKey:       cfg.apiKey,
		TimeoutMs:    cfg.timeout.Milliseconds(),
	}, ProviderCapabilities{
		SupportsSystemMessages: true,
		SupportsStreaming:      true,
	})
}

// ── Together ────────────────────────────────────────────────

// NewTogetherFFIProvider creates a Together provider backed by libnxuskit.
func NewTogetherFFIProvider(opts ...TogetherOption) (LLMProvider, error) {
	cfg := &togetherConfig{
		timeout: 30 * time.Second,
	}
	if envKey := os.Getenv("TOGETHER_API_KEY"); envKey != "" {
		cfg.apiKey = envKey
	}
	for _, opt := range opts {
		if err := opt(cfg); err != nil {
			return nil, NewConfigurationError(err.Error(), err)
		}
	}
	if cfg.apiKey == "" {
		return nil, NewConfigurationError("API key is required for together provider", nil)
	}
	return newFFIProvider("together", ffiProviderConfig{
		ProviderType: "together",
		APIKey:       cfg.apiKey,
		TimeoutMs:    cfg.timeout.Milliseconds(),
	}, ProviderCapabilities{
		SupportsSystemMessages: true,
		SupportsStreaming:      true,
	})
}

// ── OpenRouter ──────────────────────────────────────────────

// NewOpenRouterFFIProvider creates an OpenRouter provider backed by libnxuskit.
func NewOpenRouterFFIProvider(opts ...OpenRouterOption) (LLMProvider, error) {
	cfg := &openrouterConfig{
		timeout: 30 * time.Second,
	}
	if envKey := os.Getenv("OPENROUTER_API_KEY"); envKey != "" {
		cfg.apiKey = envKey
	}
	for _, opt := range opts {
		if err := opt(cfg); err != nil {
			return nil, NewConfigurationError(err.Error(), err)
		}
	}
	if cfg.apiKey == "" {
		return nil, NewConfigurationError("API key is required for openrouter provider", nil)
	}
	return newFFIProvider("openrouter", ffiProviderConfig{
		ProviderType: "openrouter",
		APIKey:       cfg.apiKey,
		TimeoutMs:    cfg.timeout.Milliseconds(),
	}, ProviderCapabilities{
		SupportsSystemMessages: true,
		SupportsStreaming:      true,
		SupportsVision:         true,
		SupportsTools:          true,
	})
}

// ── Perplexity ──────────────────────────────────────────────

// NewPerplexityFFIProvider creates a Perplexity provider backed by libnxuskit.
func NewPerplexityFFIProvider(opts ...PerplexityOption) (LLMProvider, error) {
	cfg := &perplexityConfig{
		timeout: 30 * time.Second,
	}
	if envKey := os.Getenv("PERPLEXITY_API_KEY"); envKey != "" {
		cfg.apiKey = envKey
	}
	for _, opt := range opts {
		if err := opt(cfg); err != nil {
			return nil, NewConfigurationError(err.Error(), err)
		}
	}
	if cfg.apiKey == "" {
		return nil, NewConfigurationError("API key is required for perplexity provider", nil)
	}
	return newFFIProvider("perplexity", ffiProviderConfig{
		ProviderType: "perplexity",
		APIKey:       cfg.apiKey,
		TimeoutMs:    cfg.timeout.Milliseconds(),
	}, ProviderCapabilities{
		SupportsSystemMessages: true,
		SupportsStreaming:      true,
	})
}

// ── Mistral ─────────────────────────────────────────────────

// NewMistralFFIProvider creates a Mistral provider backed by libnxuskit.
func NewMistralFFIProvider(opts ...MistralOption) (LLMProvider, error) {
	cfg := &mistralConfig{
		timeout: 30 * time.Second,
	}
	if envKey := os.Getenv("MISTRAL_API_KEY"); envKey != "" {
		cfg.apiKey = envKey
	}
	for _, opt := range opts {
		if err := opt(cfg); err != nil {
			return nil, NewConfigurationError(err.Error(), err)
		}
	}
	if cfg.apiKey == "" {
		return nil, NewConfigurationError("API key is required for mistral provider", nil)
	}
	return newFFIProvider("mistral", ffiProviderConfig{
		ProviderType: "mistral",
		APIKey:       cfg.apiKey,
		TimeoutMs:    cfg.timeout.Milliseconds(),
	}, ProviderCapabilities{
		SupportsSystemMessages: true,
		SupportsStreaming:      true,
		SupportsJSONMode:       true,
		SupportsTools:          true,
	})
}

// ── Mock (for testing) ──────────────────────────────────────

// NewMockFFIProvider creates a mock provider backed by libnxuskit.
// Useful for testing the FFI path without requiring a real API key.
func NewMockFFIProvider() (LLMProvider, error) {
	return newFFIProvider("mock", ffiProviderConfig{
		ProviderType: "mock",
	}, ProviderCapabilities{
		SupportsSystemMessages: true,
		SupportsStreaming:      true,
	})
}

// ── Loopback (for testing) ──────────────────────────────────

// NewLoopbackFFIProvider creates a loopback provider backed by libnxuskit.
func NewLoopbackFFIProvider() (LLMProvider, error) {
	return newFFIProvider("loopback", ffiProviderConfig{
		ProviderType: "loopback",
	}, ProviderCapabilities{
		SupportsSystemMessages: true,
		SupportsStreaming:      true,
	})
}

// ── CLIPS ───────────────────────────────────────────────────

// NewClipsFFIProvider creates a CLIPS rule engine provider backed by libnxuskit.
func NewClipsFFIProvider(rulesDir string) (LLMProvider, error) {
	if rulesDir == "" {
		return nil, NewConfigurationError("rules directory is required for clips provider", nil)
	}
	return newFFIProvider("clips", ffiProviderConfig{
		ProviderType: "clips",
		Model:        rulesDir, // clips uses model field for rules_directory
	}, ProviderCapabilities{
		SupportsSystemMessages: true,
	})
}

// ── MCP ─────────────────────────────────────────────────────

// NewMcpFFIProvider creates an MCP provider backed by libnxuskit.
func NewMcpFFIProvider(serverURI, model string) (LLMProvider, error) {
	if serverURI == "" {
		return nil, NewConfigurationError("server URI is required for mcp provider", nil)
	}
	return newFFIProvider("mcp", ffiProviderConfig{
		ProviderType: "mcp",
		BaseURL:      serverURI,
		Model:        model,
	}, ProviderCapabilities{
		SupportsSystemMessages: true,
		SupportsStreaming:      true,
	})
}

// ── Local LLM (In-Process Inference) ────────────────────────

// localConfig holds configuration for the in-process local LLM provider.
type localConfig struct {
	modelPath   string
	backend     string
	nGPULayers  int
	contextSize int
	batchSize   int
	threads     int
	searchPaths []string
}

// LocalOption configures a local LLM FFI provider.
type LocalOption func(*localConfig) error

// WithModelPath sets the path to a GGUF model file.
func WithModelPath(path string) LocalOption {
	return func(c *localConfig) error {
		c.modelPath = path
		return nil
	}
}

// WithBackend sets the inference backend: "llama-cpp" or "mistralrs".
// If not set, the first available compiled backend is auto-selected.
func WithBackend(backend string) LocalOption {
	return func(c *localConfig) error {
		c.backend = backend
		return nil
	}
}

// WithGPULayers sets the number of layers to offload to GPU.
// -1 offloads all layers, 0 is CPU-only (default).
func WithGPULayers(n int) LocalOption {
	return func(c *localConfig) error {
		c.nGPULayers = n
		return nil
	}
}

// WithContextSize sets the context window size in tokens.
func WithContextSize(size int) LocalOption {
	return func(c *localConfig) error {
		c.contextSize = size
		return nil
	}
}

// WithBatchSize sets the prompt processing batch size.
func WithBatchSize(size int) LocalOption {
	return func(c *localConfig) error {
		c.batchSize = size
		return nil
	}
}

// WithThreads sets the CPU thread count for inference.
// If not set, the backend auto-detects the optimal count.
func WithThreads(n int) LocalOption {
	return func(c *localConfig) error {
		c.threads = n
		return nil
	}
}

// WithSearchPath adds a directory to scan for GGUF model files during
// model discovery (list_models).
func WithSearchPath(path string) LocalOption {
	return func(c *localConfig) error {
		c.searchPaths = append(c.searchPaths, path)
		return nil
	}
}

// NewLocalFFIProvider creates an in-process local LLM provider backed by
// libnxuskit. Requires the SDK to be compiled with provider-local-llama
// or provider-local-mistralrs features.
func NewLocalFFIProvider(opts ...LocalOption) (LLMProvider, error) {
	cfg := &localConfig{}
	for _, opt := range opts {
		if err := opt(cfg); err != nil {
			return nil, NewConfigurationError(err.Error(), err)
		}
	}

	// Build the raw config map — nxuskit-core parses these fields directly
	rawCfg := map[string]interface{}{
		"provider_type": "local",
	}
	if cfg.modelPath != "" {
		rawCfg["model"] = cfg.modelPath
	}
	if cfg.backend != "" {
		rawCfg["backend"] = cfg.backend
	}
	if cfg.nGPULayers != 0 {
		rawCfg["n_gpu_layers"] = cfg.nGPULayers
	}
	if cfg.contextSize != 0 {
		rawCfg["context_size"] = cfg.contextSize
	}
	if cfg.batchSize != 0 {
		rawCfg["batch_size"] = cfg.batchSize
	}
	if cfg.threads != 0 {
		rawCfg["threads"] = cfg.threads
	}
	if len(cfg.searchPaths) > 0 {
		rawCfg["search_paths"] = cfg.searchPaths
	}

	return newFFIProviderRaw("local", rawCfg, localDefaultCapabilities())
}

func localDefaultCapabilities() ProviderCapabilities {
	maxStop := 4
	return ProviderCapabilities{
		SupportsSystemMessages: true,
		SupportsStreaming:      true,
		SupportsSeed:           true,
		MaxStopSequences:       &maxStop,
	}
}

// newFFIProviderRaw creates an FFI provider from a raw config map.
// Used by providers with non-standard config fields (local, z3).
func newFFIProviderRaw(
	name string,
	config map[string]interface{},
	caps ProviderCapabilities,
) (*ffiProvider, error) {
	if err := checkNxuskitVersion(); err != nil {
		return nil, err
	}

	configJSON, err := json.Marshal(config)
	if err != nil {
		return nil, NewConfigurationError(fmt.Sprintf("failed to marshal config: %v", err), err)
	}

	handle, err := ffi.CreateProvider(string(configJSON))
	if err != nil {
		return nil, NewConfigurationError(err.Error(), err)
	}

	return &ffiProvider{
		handle:       handle,
		providerName: name,
		capabilities: caps,
	}, nil
}

// ── FFI Provider Registration ───────────────────────────────

// RegisterFFIProviders registers all FFI-backed providers with the
// provider registry for auto-detection via the convenience API.
// This should be called during init if FFI providers are desired
// instead of native Go implementations.
func RegisterFFIProviders() error {
	if err := ffiAvailable(); err != nil {
		return fmt.Errorf("cannot register FFI providers: %w", err)
	}

	// Claude
	RegisterProvider("claude-ffi", []string{"claude-"}, "ANTHROPIC_API_KEY", func() (LLMProvider, error) {
		return NewClaudeFFIProvider()
	})

	// OpenAI
	RegisterProvider("openai-ffi", []string{"gpt-", "o1-", "o3-"}, "OPENAI_API_KEY", func() (LLMProvider, error) {
		return NewOpenAIFFIProvider()
	})

	// Ollama
	RegisterProvider("ollama-ffi", []string{}, "OLLAMA_HOST", func() (LLMProvider, error) {
		return NewOllamaFFIProvider()
	})

	return nil
}

// ffiAvailable checks if the nxuskit library is linked and functional.
func ffiAvailable() error {
	return checkNxuskitVersion()
}
