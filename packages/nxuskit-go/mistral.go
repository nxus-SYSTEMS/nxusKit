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
	mistralDefaultBaseURL = "https://api.mistral.ai/v1"
	mistralProviderName   = "mistral"
)

// MistralProvider connects to the Mistral AI API.
type MistralProvider struct {
	client *httputil.Client
}

// mistralConfig holds configuration for MistralProvider.
type mistralConfig struct {
	apiKey  string
	baseURL string
	timeout time.Duration
}

// MistralOption configures a MistralProvider.
type MistralOption func(*mistralConfig) error

// WithMistralAPIKey sets the API key for authentication.
func WithMistralAPIKey(key string) MistralOption {
	return func(c *mistralConfig) error {
		if key == "" {
			return fmt.Errorf("API key cannot be empty")
		}
		c.apiKey = key
		return nil
	}
}

// WithMistralBaseURL sets a custom base URL for the Mistral API.
func WithMistralBaseURL(url string) MistralOption {
	return func(c *mistralConfig) error {
		if url == "" {
			return fmt.Errorf("base URL cannot be empty")
		}
		c.baseURL = strings.TrimSuffix(url, "/")
		return nil
	}
}

// WithMistralTimeout sets a custom timeout for HTTP requests.
func WithMistralTimeout(timeout time.Duration) MistralOption {
	return func(c *mistralConfig) error {
		if timeout <= 0 {
			return fmt.Errorf("timeout must be positive")
		}
		c.timeout = timeout
		return nil
	}
}

// NewMistralProvider creates a new Mistral provider.
// It checks MISTRAL_API_KEY environment variable if no API key is provided via options.
func NewMistralProvider(opts ...MistralOption) (*MistralProvider, error) {
	cfg := &mistralConfig{
		baseURL: mistralDefaultBaseURL,
		timeout: httputil.DefaultTimeout,
	}

	// Check environment variable for API key
	if envKey := os.Getenv("MISTRAL_API_KEY"); envKey != "" {
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
		return nil, NewConfigurationError("API key is required: set MISTRAL_API_KEY environment variable or use WithMistralAPIKey option", nil)
	}

	// Create HTTP client with Authorization header
	headers := map[string]string{
		"Authorization": "Bearer " + cfg.apiKey,
	}

	return &MistralProvider{
		client: httputil.NewClientWithHeaders(cfg.baseURL, cfg.timeout, headers),
	}, nil
}

// ProviderName returns "mistral".
func (p *MistralProvider) ProviderName() string {
	return mistralProviderName
}

// Chat sends a chat completion request to Mistral.
func (p *MistralProvider) Chat(ctx context.Context, req *ChatRequest) (*ChatResponse, error) {
	return openaiCompatibleChat(ctx, p.client, mistralProviderName, req)
}

// ChatStream sends a streaming chat completion request to Mistral.
func (p *MistralProvider) ChatStream(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan error) {
	return openaiCompatibleChatStream(ctx, p.client, mistralProviderName, req)
}

// ListModels returns available models from the Mistral API.
func (p *MistralProvider) ListModels(ctx context.Context) ([]ModelInfo, error) {
	return openaiCompatibleListModels(ctx, p.client, mistralProviderName)
}

// Ping checks if the Mistral API is reachable and the API key is valid.
func (p *MistralProvider) Ping(ctx context.Context) error {
	return openaiCompatiblePing(ctx, p.client, mistralProviderName)
}

// GetCapabilities returns the features and parameter limits of the Mistral API.
func (p *MistralProvider) GetCapabilities() ProviderCapabilities {
	maxStop := 4
	return ProviderCapabilities{
		SupportsSystemMessages:   true,
		SupportsStreaming:        true,
		SupportsVision:           false,
		MaxStopSequences:         &maxStop,
		SupportsPresencePenalty:  false,
		SupportsFrequencyPenalty: false,
		SupportsSeed:             true,
		SupportsLogprobs:         false,
		MaxLogprobs:              nil,
		SupportsJSONMode:         true,
		SupportsJSONSchema:       false,
		SupportsTools:            true,
		SupportsResponseFormat:   true,
		SupportsTopK:             false,
		SupportsMinP:             false,
		PenaltyRange:             nil,
	}
}

// StreamWithUsage sends a streaming chat request and provides final token usage.
func (p *MistralProvider) StreamWithUsage(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan TokenUsage) {
	return openaiCompatibleStreamWithUsage(ctx, p.client, mistralProviderName, req)
}

// FreshSession returns the receiver unchanged (stateless provider).
// This method exists for API consistency with nxuskit-engine.
func (p *MistralProvider) FreshSession() (LLMProvider, error) {
	return p, nil
}
