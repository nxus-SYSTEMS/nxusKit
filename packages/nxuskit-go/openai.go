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
	openaiDefaultBaseURL = "https://api.openai.com/v1"
	openaiProviderName   = "openai"
)

// OpenAIProvider connects to the OpenAI API.
type OpenAIProvider struct {
	client *httputil.Client
}

// openaiConfig holds configuration for OpenAIProvider.
type openaiConfig struct {
	apiKey  string
	baseURL string
	timeout time.Duration
}

// OpenAIOption configures an OpenAIProvider.
type OpenAIOption func(*openaiConfig) error

// WithOpenAIAPIKey sets the API key for authentication.
func WithOpenAIAPIKey(key string) OpenAIOption {
	return func(c *openaiConfig) error {
		if key == "" {
			return fmt.Errorf("API key cannot be empty")
		}
		c.apiKey = key
		return nil
	}
}

// WithOpenAIBaseURL sets a custom base URL for the OpenAI API.
func WithOpenAIBaseURL(url string) OpenAIOption {
	return func(c *openaiConfig) error {
		if url == "" {
			return fmt.Errorf("base URL cannot be empty")
		}
		c.baseURL = strings.TrimSuffix(url, "/")
		return nil
	}
}

// WithOpenAITimeout sets a custom timeout for HTTP requests.
func WithOpenAITimeout(timeout time.Duration) OpenAIOption {
	return func(c *openaiConfig) error {
		if timeout <= 0 {
			return fmt.Errorf("timeout must be positive")
		}
		c.timeout = timeout
		return nil
	}
}

// NewOpenAIProvider creates a new OpenAI provider.
// It checks OPENAI_API_KEY environment variable if no API key is provided via options.
func NewOpenAIProvider(opts ...OpenAIOption) (*OpenAIProvider, error) {
	cfg := &openaiConfig{
		baseURL: openaiDefaultBaseURL,
		timeout: httputil.DefaultTimeout,
	}

	// Check environment variable for API key
	if envKey := os.Getenv("OPENAI_API_KEY"); envKey != "" {
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
		return nil, NewConfigurationError("API key is required: set OPENAI_API_KEY environment variable or use WithOpenAIAPIKey option", nil)
	}

	// Create HTTP client with Authorization header
	headers := map[string]string{
		"Authorization": "Bearer " + cfg.apiKey,
	}

	return &OpenAIProvider{
		client: httputil.NewClientWithHeaders(cfg.baseURL, cfg.timeout, headers),
	}, nil
}

// ProviderName returns "openai".
func (p *OpenAIProvider) ProviderName() string {
	return openaiProviderName
}

// Chat sends a chat completion request to OpenAI.
func (p *OpenAIProvider) Chat(ctx context.Context, req *ChatRequest) (*ChatResponse, error) {
	return openaiCompatibleChat(ctx, p.client, openaiProviderName, req)
}

// ChatStream sends a streaming chat completion request to OpenAI.
func (p *OpenAIProvider) ChatStream(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan error) {
	return openaiCompatibleChatStream(ctx, p.client, openaiProviderName, req)
}

// ListModels returns available models from the OpenAI API.
func (p *OpenAIProvider) ListModels(ctx context.Context) ([]ModelInfo, error) {
	return openaiCompatibleListModels(ctx, p.client, openaiProviderName)
}

// Ping checks if the OpenAI API is reachable and the API key is valid.
func (p *OpenAIProvider) Ping(ctx context.Context) error {
	return openaiCompatiblePing(ctx, p.client, openaiProviderName)
}

// GetCapabilities returns the features and parameter limits of the OpenAI API.
func (p *OpenAIProvider) GetCapabilities() ProviderCapabilities {
	maxStop := 4
	maxLogprobs := 20
	return ProviderCapabilities{
		SupportsSystemMessages:    true,
		SupportsStreaming:         true,
		SupportsVision:            true,
		MaxStopSequences:          &maxStop,
		SupportsPresencePenalty:   true,
		SupportsFrequencyPenalty:  true,
		SupportsSeed:              true,
		SupportsLogprobs:          true,
		MaxLogprobs:               &maxLogprobs,
		SupportsStreamingLogprobs: true,
		SupportsJSONMode:          true,
		SupportsJSONSchema:        true,
		SupportsTools:             true,
		SupportsResponseFormat:    true,
		SupportsTopK:              false,
		SupportsMinP:              false,
		PenaltyRange:              &PenaltyRange{-2.0, 2.0},
	}
}

// StreamWithUsage sends a streaming chat request and provides final token usage.
func (p *OpenAIProvider) StreamWithUsage(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan TokenUsage) {
	return openaiCompatibleStreamWithUsage(ctx, p.client, openaiProviderName, req)
}

// FreshSession returns the receiver unchanged (stateless provider).
// This method exists for API consistency with nxuskit-engine.
func (p *OpenAIProvider) FreshSession() (LLMProvider, error) {
	return p, nil
}
