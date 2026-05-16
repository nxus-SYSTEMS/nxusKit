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
	xaiDefaultBaseURL = "https://api.x.ai/v1"
	xaiProviderName   = "xai"
)

// XaiProvider connects to the xAI Grok API.
type XaiProvider struct {
	client *httputil.Client
}

// xaiConfig holds configuration for XaiProvider.
type xaiConfig struct {
	apiKey  string
	baseURL string
	timeout time.Duration
}

// XaiOption configures an XaiProvider.
type XaiOption func(*xaiConfig) error

// WithXaiAPIKey sets the API key for authentication.
func WithXaiAPIKey(key string) XaiOption {
	return func(c *xaiConfig) error {
		if key == "" {
			return fmt.Errorf("API key cannot be empty")
		}
		c.apiKey = key
		return nil
	}
}

// WithXaiBaseURL sets a custom base URL for the xAI API.
func WithXaiBaseURL(url string) XaiOption {
	return func(c *xaiConfig) error {
		if url == "" {
			return fmt.Errorf("base URL cannot be empty")
		}
		c.baseURL = strings.TrimSuffix(url, "/")
		return nil
	}
}

// WithXaiTimeout sets a custom timeout for HTTP requests.
func WithXaiTimeout(timeout time.Duration) XaiOption {
	return func(c *xaiConfig) error {
		if timeout <= 0 {
			return fmt.Errorf("timeout must be positive")
		}
		c.timeout = timeout
		return nil
	}
}

// NewXaiProvider creates a new xAI Grok provider.
// It checks XAI_API_KEY environment variable if no API key is provided via options.
func NewXaiProvider(opts ...XaiOption) (*XaiProvider, error) {
	cfg := &xaiConfig{
		baseURL: xaiDefaultBaseURL,
		timeout: httputil.DefaultTimeout,
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
		return nil, NewConfigurationError("API key is required: set XAI_API_KEY environment variable or use WithXaiAPIKey option", nil)
	}

	headers := map[string]string{
		"Authorization": "Bearer " + cfg.apiKey,
	}

	return &XaiProvider{
		client: httputil.NewClientWithHeaders(cfg.baseURL, cfg.timeout, headers),
	}, nil
}

// ProviderName returns "xai".
func (p *XaiProvider) ProviderName() string {
	return xaiProviderName
}

// Chat sends a chat completion request to xAI Grok.
func (p *XaiProvider) Chat(ctx context.Context, req *ChatRequest) (*ChatResponse, error) {
	return openaiCompatibleChat(ctx, p.client, xaiProviderName, req)
}

// ChatStream sends a streaming chat completion request to xAI Grok.
func (p *XaiProvider) ChatStream(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan error) {
	return openaiCompatibleChatStream(ctx, p.client, xaiProviderName, req)
}

// ListModels returns available models from the xAI API.
func (p *XaiProvider) ListModels(ctx context.Context) ([]ModelInfo, error) {
	return openaiCompatibleListModels(ctx, p.client, xaiProviderName)
}

// Ping checks if the xAI API is reachable and the API key is valid.
func (p *XaiProvider) Ping(ctx context.Context) error {
	return openaiCompatiblePing(ctx, p.client, xaiProviderName)
}

// GetCapabilities returns the features and parameter limits of the xAI Grok API.
func (p *XaiProvider) GetCapabilities() ProviderCapabilities {
	return ProviderCapabilities{
		SupportsSystemMessages: true,
		SupportsStreaming:      true,
		SupportsVision:         true,
		SupportsJSONMode:       true,
		SupportsJSONSchema:     true,
		SupportsTools:          true,
		SupportsResponseFormat: true,
		SupportsLogprobs:       false,
		SupportsTopK:           false,
		SupportsMinP:           false,
	}
}

// StreamWithUsage sends a streaming chat request and provides final token usage.
func (p *XaiProvider) StreamWithUsage(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan TokenUsage) {
	return openaiCompatibleStreamWithUsage(ctx, p.client, xaiProviderName, req)
}

// FreshSession returns the receiver unchanged (stateless provider).
// This method exists for API consistency with nxuskit-engine.
func (p *XaiProvider) FreshSession() (LLMProvider, error) {
	return p, nil
}
