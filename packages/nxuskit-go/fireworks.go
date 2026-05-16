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
	fireworksDefaultBaseURL = "https://api.fireworks.ai/inference/v1"
	fireworksProviderName   = "fireworks"
)

// FireworksProvider connects to the Fireworks AI API.
type FireworksProvider struct {
	client *httputil.Client
}

// fireworksConfig holds configuration for FireworksProvider.
type fireworksConfig struct {
	apiKey  string
	baseURL string
	timeout time.Duration
}

// FireworksOption configures a FireworksProvider.
type FireworksOption func(*fireworksConfig) error

// WithFireworksAPIKey sets the API key for authentication.
func WithFireworksAPIKey(key string) FireworksOption {
	return func(c *fireworksConfig) error {
		if key == "" {
			return fmt.Errorf("API key cannot be empty")
		}
		c.apiKey = key
		return nil
	}
}

// WithFireworksBaseURL sets a custom base URL for the Fireworks API.
func WithFireworksBaseURL(url string) FireworksOption {
	return func(c *fireworksConfig) error {
		if url == "" {
			return fmt.Errorf("base URL cannot be empty")
		}
		c.baseURL = strings.TrimSuffix(url, "/")
		return nil
	}
}

// WithFireworksTimeout sets a custom timeout for HTTP requests.
func WithFireworksTimeout(timeout time.Duration) FireworksOption {
	return func(c *fireworksConfig) error {
		if timeout <= 0 {
			return fmt.Errorf("timeout must be positive")
		}
		c.timeout = timeout
		return nil
	}
}

// NewFireworksProvider creates a new Fireworks provider.
// It checks FIREWORKS_API_KEY environment variable if no API key is provided via options.
func NewFireworksProvider(opts ...FireworksOption) (*FireworksProvider, error) {
	cfg := &fireworksConfig{
		baseURL: fireworksDefaultBaseURL,
		timeout: httputil.DefaultTimeout,
	}

	// Check environment variable for API key
	if envKey := os.Getenv("FIREWORKS_API_KEY"); envKey != "" {
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
		return nil, NewConfigurationError("API key is required: set FIREWORKS_API_KEY environment variable or use WithFireworksAPIKey option", nil)
	}

	// Create HTTP client with Authorization header
	headers := map[string]string{
		"Authorization": "Bearer " + cfg.apiKey,
	}

	return &FireworksProvider{
		client: httputil.NewClientWithHeaders(cfg.baseURL, cfg.timeout, headers),
	}, nil
}

// ProviderName returns "fireworks".
func (p *FireworksProvider) ProviderName() string {
	return fireworksProviderName
}

// Chat sends a chat completion request to Fireworks.
func (p *FireworksProvider) Chat(ctx context.Context, req *ChatRequest) (*ChatResponse, error) {
	return openaiCompatibleChat(ctx, p.client, fireworksProviderName, req)
}

// ChatStream sends a streaming chat completion request to Fireworks.
func (p *FireworksProvider) ChatStream(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan error) {
	return openaiCompatibleChatStream(ctx, p.client, fireworksProviderName, req)
}

// ListModels returns available models from the Fireworks API.
func (p *FireworksProvider) ListModels(ctx context.Context) ([]ModelInfo, error) {
	return openaiCompatibleListModels(ctx, p.client, fireworksProviderName)
}

// Ping checks if the Fireworks API is reachable and the API key is valid.
func (p *FireworksProvider) Ping(ctx context.Context) error {
	return openaiCompatiblePing(ctx, p.client, fireworksProviderName)
}

// GetCapabilities returns the features and parameter limits of the Fireworks API.
func (p *FireworksProvider) GetCapabilities() ProviderCapabilities {
	maxStop := 4
	return ProviderCapabilities{
		SupportsSystemMessages:   true,
		SupportsStreaming:        true,
		SupportsVision:           false,
		MaxStopSequences:         &maxStop,
		SupportsPresencePenalty:  true,
		SupportsFrequencyPenalty: true,
		SupportsSeed:             true,
		SupportsLogprobs:         false,
		MaxLogprobs:              nil,
		SupportsJSONMode:         true,
		SupportsJSONSchema:       false,
		SupportsTools:            false,
		SupportsResponseFormat:   true,
		SupportsTopK:             false,
		SupportsMinP:             false,
		PenaltyRange:             &PenaltyRange{-2.0, 2.0},
	}
}

// StreamWithUsage sends a streaming chat request and provides final token usage.
func (p *FireworksProvider) StreamWithUsage(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan TokenUsage) {
	return openaiCompatibleStreamWithUsage(ctx, p.client, fireworksProviderName, req)
}

// FreshSession returns the receiver unchanged (stateless provider).
// This method exists for API consistency with nxuskit-engine.
func (p *FireworksProvider) FreshSession() (LLMProvider, error) {
	return p, nil
}
