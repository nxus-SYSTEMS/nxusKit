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
	perplexityDefaultBaseURL = "https://api.perplexity.ai"
	perplexityProviderName   = "perplexity"
)

// PerplexityProvider connects to the Perplexity AI API.
type PerplexityProvider struct {
	client *httputil.Client
}

// perplexityConfig holds configuration for PerplexityProvider.
type perplexityConfig struct {
	apiKey  string
	baseURL string
	timeout time.Duration
}

// PerplexityOption configures a PerplexityProvider.
type PerplexityOption func(*perplexityConfig) error

// WithPerplexityAPIKey sets the API key for authentication.
func WithPerplexityAPIKey(key string) PerplexityOption {
	return func(c *perplexityConfig) error {
		if key == "" {
			return fmt.Errorf("API key cannot be empty")
		}
		c.apiKey = key
		return nil
	}
}

// WithPerplexityBaseURL sets a custom base URL for the Perplexity API.
func WithPerplexityBaseURL(url string) PerplexityOption {
	return func(c *perplexityConfig) error {
		if url == "" {
			return fmt.Errorf("base URL cannot be empty")
		}
		c.baseURL = strings.TrimSuffix(url, "/")
		return nil
	}
}

// WithPerplexityTimeout sets a custom timeout for HTTP requests.
func WithPerplexityTimeout(timeout time.Duration) PerplexityOption {
	return func(c *perplexityConfig) error {
		if timeout <= 0 {
			return fmt.Errorf("timeout must be positive")
		}
		c.timeout = timeout
		return nil
	}
}

// NewPerplexityProvider creates a new Perplexity provider.
// It checks PERPLEXITY_API_KEY environment variable if no API key is provided via options.
func NewPerplexityProvider(opts ...PerplexityOption) (*PerplexityProvider, error) {
	cfg := &perplexityConfig{
		baseURL: perplexityDefaultBaseURL,
		timeout: httputil.DefaultTimeout,
	}

	// Check environment variable for API key
	if envKey := os.Getenv("PERPLEXITY_API_KEY"); envKey != "" {
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
		return nil, NewConfigurationError("API key is required: set PERPLEXITY_API_KEY environment variable or use WithPerplexityAPIKey option", nil)
	}

	// Create HTTP client with Authorization header
	headers := map[string]string{
		"Authorization": "Bearer " + cfg.apiKey,
	}

	return &PerplexityProvider{
		client: httputil.NewClientWithHeaders(cfg.baseURL, cfg.timeout, headers),
	}, nil
}

// ProviderName returns "perplexity".
func (p *PerplexityProvider) ProviderName() string {
	return perplexityProviderName
}

// Chat sends a chat completion request to Perplexity.
func (p *PerplexityProvider) Chat(ctx context.Context, req *ChatRequest) (*ChatResponse, error) {
	return openaiCompatibleChat(ctx, p.client, perplexityProviderName, req)
}

// ChatStream sends a streaming chat completion request to Perplexity.
func (p *PerplexityProvider) ChatStream(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan error) {
	return openaiCompatibleChatStream(ctx, p.client, perplexityProviderName, req)
}

// ListModels returns available models from the Perplexity API.
// Note: Perplexity may not support the /models endpoint.
func (p *PerplexityProvider) ListModels(ctx context.Context) ([]ModelInfo, error) {
	// Return hardcoded list as Perplexity doesn't have a models endpoint
	return []ModelInfo{
		{Name: "llama-3.1-sonar-small-128k-online"},
		{Name: "llama-3.1-sonar-large-128k-online"},
		{Name: "llama-3.1-sonar-huge-128k-online"},
	}, nil
}

// Ping checks if the Perplexity API is reachable and the API key is valid.
func (p *PerplexityProvider) Ping(ctx context.Context) error {
	// Send a minimal chat request since Perplexity doesn't have a models endpoint
	_, err := p.Chat(ctx, &ChatRequest{
		Model: "llama-3.1-sonar-small-128k-online",
		Messages: []Message{
			UserMessage("ping"),
		},
		MaxTokens: perplexityIntPtr(1),
	})
	return err
}

func perplexityIntPtr(i int) *int {
	return &i
}

// GetCapabilities returns the features and parameter limits of the Perplexity API.
func (p *PerplexityProvider) GetCapabilities() ProviderCapabilities {
	return ProviderCapabilities{
		SupportsSystemMessages:   true,
		SupportsStreaming:        true,
		SupportsVision:           false,
		MaxStopSequences:         nil,
		SupportsPresencePenalty:  true,
		SupportsFrequencyPenalty: true,
		SupportsSeed:             false,
		SupportsLogprobs:         false,
		MaxLogprobs:              nil,
		SupportsJSONMode:         false,
		SupportsJSONSchema:       false,
		SupportsTools:            false,
		SupportsResponseFormat:   false,
		SupportsTopK:             false,
		SupportsMinP:             false,
		PenaltyRange:             &PenaltyRange{-2.0, 2.0},
	}
}

// StreamWithUsage sends a streaming chat request and provides final token usage.
func (p *PerplexityProvider) StreamWithUsage(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan TokenUsage) {
	chunks, errs := p.ChatStream(ctx, req)
	return wrapStreamWithUsage(chunks, errs)
}

// FreshSession returns the receiver unchanged (stateless provider).
// This method exists for API consistency with nxuskit-engine.
func (p *PerplexityProvider) FreshSession() (LLMProvider, error) {
	return p, nil
}
