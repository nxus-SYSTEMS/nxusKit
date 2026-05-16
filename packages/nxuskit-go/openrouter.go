package nxuskit

import (
	"context"
	"fmt"
	"os"
	"strings"
	"time"

	"github.com/nxus-SYSTEMS/nxusKit/packages/nxuskit-go/internal/httputil"
)

const (
	openrouterDefaultBaseURL = "https://openrouter.ai/api/v1"
	openrouterProviderName   = "openrouter"
)

// OpenRouterProvider connects to the OpenRouter API.
type OpenRouterProvider struct {
	client *httputil.Client
}

// openrouterConfig holds configuration for OpenRouterProvider.
type openrouterConfig struct {
	apiKey      string
	baseURL     string
	timeout     time.Duration
	httpReferer string
	xTitle      string
}

// OpenRouterOption configures an OpenRouterProvider.
type OpenRouterOption func(*openrouterConfig) error

// WithOpenRouterAPIKey sets the API key for authentication.
func WithOpenRouterAPIKey(key string) OpenRouterOption {
	return func(c *openrouterConfig) error {
		if key == "" {
			return fmt.Errorf("API key cannot be empty")
		}
		c.apiKey = key
		return nil
	}
}

// WithOpenRouterBaseURL sets a custom base URL for the OpenRouter API.
func WithOpenRouterBaseURL(url string) OpenRouterOption {
	return func(c *openrouterConfig) error {
		if url == "" {
			return fmt.Errorf("base URL cannot be empty")
		}
		c.baseURL = strings.TrimSuffix(url, "/")
		return nil
	}
}

// WithOpenRouterTimeout sets a custom timeout for HTTP requests.
func WithOpenRouterTimeout(timeout time.Duration) OpenRouterOption {
	return func(c *openrouterConfig) error {
		if timeout <= 0 {
			return fmt.Errorf("timeout must be positive")
		}
		c.timeout = timeout
		return nil
	}
}

// WithOpenRouterHTTPReferer sets the HTTP-Referer header for ranking on OpenRouter.
func WithOpenRouterHTTPReferer(referer string) OpenRouterOption {
	return func(c *openrouterConfig) error {
		c.httpReferer = referer
		return nil
	}
}

// WithOpenRouterXTitle sets the X-Title header for display on OpenRouter rankings.
func WithOpenRouterXTitle(title string) OpenRouterOption {
	return func(c *openrouterConfig) error {
		c.xTitle = title
		return nil
	}
}

// NewOpenRouterProvider creates a new OpenRouter provider.
// It checks OPENROUTER_API_KEY environment variable if no API key is provided via options.
func NewOpenRouterProvider(opts ...OpenRouterOption) (*OpenRouterProvider, error) {
	cfg := &openrouterConfig{
		baseURL: openrouterDefaultBaseURL,
		timeout: httputil.DefaultTimeout,
	}

	// Check environment variable for API key
	if envKey := os.Getenv("OPENROUTER_API_KEY"); envKey != "" {
		cfg.apiKey = envKey
	}

	// Apply options
	for _, opt := range opts {
		if err := opt(cfg); err != nil {
			return nil, NewConfigurationError(err.Error(), err)
		}
	}

	// Validate required configuration
	if cfg.apiKey == "" {
		return nil, NewConfigurationError("API key is required: set OPENROUTER_API_KEY environment variable or use WithOpenRouterAPIKey option", nil)
	}

	// Create HTTP client with Authorization header and optional OpenRouter headers
	headers := map[string]string{
		"Authorization": "Bearer " + cfg.apiKey,
	}

	// Add optional OpenRouter-specific headers
	if cfg.httpReferer != "" {
		headers["HTTP-Referer"] = cfg.httpReferer
	}
	if cfg.xTitle != "" {
		headers["X-Title"] = cfg.xTitle
	}

	return &OpenRouterProvider{
		client: httputil.NewClientWithHeaders(cfg.baseURL, cfg.timeout, headers),
	}, nil
}

// ProviderName returns "openrouter".
func (p *OpenRouterProvider) ProviderName() string {
	return openrouterProviderName
}

// Chat sends a chat completion request to OpenRouter.
func (p *OpenRouterProvider) Chat(ctx context.Context, req *ChatRequest) (*ChatResponse, error) {
	return openaiCompatibleChat(ctx, p.client, openrouterProviderName, req)
}

// ChatStream sends a streaming chat completion request to OpenRouter.
func (p *OpenRouterProvider) ChatStream(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan error) {
	return openaiCompatibleChatStream(ctx, p.client, openrouterProviderName, req)
}

// ListModels returns available models from the OpenRouter API.
func (p *OpenRouterProvider) ListModels(ctx context.Context) ([]ModelInfo, error) {
	return openaiCompatibleListModels(ctx, p.client, openrouterProviderName)
}

// Ping checks if the OpenRouter API is reachable and the API key is valid.
func (p *OpenRouterProvider) Ping(ctx context.Context) error {
	return openaiCompatiblePing(ctx, p.client, openrouterProviderName)
}

// GetCapabilities returns the features and parameter limits of the OpenRouter API.
func (p *OpenRouterProvider) GetCapabilities() ProviderCapabilities {
	maxStop := 4
	return ProviderCapabilities{
		SupportsSystemMessages:   true,
		SupportsStreaming:        true,
		SupportsVision:           true, // model-dependent
		MaxStopSequences:         &maxStop,
		SupportsPresencePenalty:  true,
		SupportsFrequencyPenalty: true,
		SupportsSeed:             true,
		SupportsLogprobs:         true,
		MaxLogprobs:              nil, // model-dependent
		SupportsJSONMode:         true,
		SupportsJSONSchema:       false, // model-dependent
		SupportsTools:            true,  // model-dependent
		SupportsResponseFormat:   true,
		SupportsTopK:             false,
		SupportsMinP:             false,
		PenaltyRange:             &PenaltyRange{-2.0, 2.0},
	}
}

// StreamWithUsage sends a streaming chat request and provides final token usage.
func (p *OpenRouterProvider) StreamWithUsage(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan TokenUsage) {
	return openaiCompatibleStreamWithUsage(ctx, p.client, openrouterProviderName, req)
}

// FreshSession returns the receiver unchanged (stateless provider).
// This method exists for API consistency with nxuskit-engine.
func (p *OpenRouterProvider) FreshSession() (LLMProvider, error) {
	return p, nil
}
