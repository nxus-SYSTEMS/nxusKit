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
	togetherDefaultBaseURL = "https://api.together.xyz/v1"
	togetherProviderName   = "together"
)

// TogetherProvider connects to the Together AI API.
type TogetherProvider struct {
	client *httputil.Client
}

// togetherConfig holds configuration for TogetherProvider.
type togetherConfig struct {
	apiKey  string
	baseURL string
	timeout time.Duration
}

// TogetherOption configures a TogetherProvider.
type TogetherOption func(*togetherConfig) error

// WithTogetherAPIKey sets the API key for authentication.
func WithTogetherAPIKey(key string) TogetherOption {
	return func(c *togetherConfig) error {
		if key == "" {
			return fmt.Errorf("API key cannot be empty")
		}
		c.apiKey = key
		return nil
	}
}

// WithTogetherBaseURL sets a custom base URL for the Together API.
func WithTogetherBaseURL(url string) TogetherOption {
	return func(c *togetherConfig) error {
		if url == "" {
			return fmt.Errorf("base URL cannot be empty")
		}
		c.baseURL = strings.TrimSuffix(url, "/")
		return nil
	}
}

// WithTogetherTimeout sets a custom timeout for HTTP requests.
func WithTogetherTimeout(timeout time.Duration) TogetherOption {
	return func(c *togetherConfig) error {
		if timeout <= 0 {
			return fmt.Errorf("timeout must be positive")
		}
		c.timeout = timeout
		return nil
	}
}

// NewTogetherProvider creates a new Together provider.
// It checks TOGETHER_API_KEY environment variable if no API key is provided via options.
func NewTogetherProvider(opts ...TogetherOption) (*TogetherProvider, error) {
	cfg := &togetherConfig{
		baseURL: togetherDefaultBaseURL,
		timeout: httputil.DefaultTimeout,
	}

	// Check environment variable for API key
	if envKey := os.Getenv("TOGETHER_API_KEY"); envKey != "" {
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
		return nil, NewConfigurationError("API key is required: set TOGETHER_API_KEY environment variable or use WithTogetherAPIKey option", nil)
	}

	// Create HTTP client with Authorization header
	headers := map[string]string{
		"Authorization": "Bearer " + cfg.apiKey,
	}

	return &TogetherProvider{
		client: httputil.NewClientWithHeaders(cfg.baseURL, cfg.timeout, headers),
	}, nil
}

// ProviderName returns "together".
func (p *TogetherProvider) ProviderName() string {
	return togetherProviderName
}

// Chat sends a chat completion request to Together.
func (p *TogetherProvider) Chat(ctx context.Context, req *ChatRequest) (*ChatResponse, error) {
	return openaiCompatibleChat(ctx, p.client, togetherProviderName, req)
}

// ChatStream sends a streaming chat completion request to Together.
func (p *TogetherProvider) ChatStream(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan error) {
	return openaiCompatibleChatStream(ctx, p.client, togetherProviderName, req)
}

// ListModels returns available models from the Together API.
func (p *TogetherProvider) ListModels(ctx context.Context) ([]ModelInfo, error) {
	return openaiCompatibleListModels(ctx, p.client, togetherProviderName)
}

// Ping checks if the Together API is reachable and the API key is valid.
func (p *TogetherProvider) Ping(ctx context.Context) error {
	return openaiCompatiblePing(ctx, p.client, togetherProviderName)
}

// GetCapabilities returns the features and parameter limits of the Together API.
func (p *TogetherProvider) GetCapabilities() ProviderCapabilities {
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
		MaxLogprobs:              nil, // no documented limit
		SupportsJSONMode:         true,
		SupportsJSONSchema:       false,
		SupportsTools:            true, // limited models
		SupportsResponseFormat:   true,
		SupportsTopK:             true,
		SupportsMinP:             false,
		PenaltyRange:             &PenaltyRange{-2.0, 2.0},
	}
}

// StreamWithUsage sends a streaming chat request and provides final token usage.
func (p *TogetherProvider) StreamWithUsage(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan TokenUsage) {
	return openaiCompatibleStreamWithUsage(ctx, p.client, togetherProviderName, req)
}

// FreshSession returns the receiver unchanged (stateless provider).
// This method exists for API consistency with nxuskit-engine.
func (p *TogetherProvider) FreshSession() (LLMProvider, error) {
	return p, nil
}
