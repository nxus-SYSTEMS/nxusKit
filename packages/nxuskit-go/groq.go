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
	groqDefaultBaseURL = "https://api.groq.com/openai/v1"
	groqProviderName   = "groq"
)

// GroqProvider connects to the Groq API.
type GroqProvider struct {
	client *httputil.Client
}

// groqConfig holds configuration for GroqProvider.
type groqConfig struct {
	apiKey  string
	baseURL string
	timeout time.Duration
}

// GroqOption configures a GroqProvider.
type GroqOption func(*groqConfig) error

// WithGroqAPIKey sets the API key for authentication.
func WithGroqAPIKey(key string) GroqOption {
	return func(c *groqConfig) error {
		if key == "" {
			return fmt.Errorf("API key cannot be empty")
		}
		c.apiKey = key
		return nil
	}
}

// WithGroqBaseURL sets a custom base URL for the Groq API.
func WithGroqBaseURL(url string) GroqOption {
	return func(c *groqConfig) error {
		if url == "" {
			return fmt.Errorf("base URL cannot be empty")
		}
		c.baseURL = strings.TrimSuffix(url, "/")
		return nil
	}
}

// WithGroqTimeout sets a custom timeout for HTTP requests.
func WithGroqTimeout(timeout time.Duration) GroqOption {
	return func(c *groqConfig) error {
		if timeout <= 0 {
			return fmt.Errorf("timeout must be positive")
		}
		c.timeout = timeout
		return nil
	}
}

// NewGroqProvider creates a new Groq provider.
// It checks GROQ_API_KEY environment variable if no API key is provided via options.
func NewGroqProvider(opts ...GroqOption) (*GroqProvider, error) {
	cfg := &groqConfig{
		baseURL: groqDefaultBaseURL,
		timeout: httputil.DefaultTimeout,
	}

	// Check environment variable for API key
	if envKey := os.Getenv("GROQ_API_KEY"); envKey != "" {
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
		return nil, NewConfigurationError("API key is required: set GROQ_API_KEY environment variable or use WithGroqAPIKey option", nil)
	}

	// Create HTTP client with Authorization header
	headers := map[string]string{
		"Authorization": "Bearer " + cfg.apiKey,
	}

	return &GroqProvider{
		client: httputil.NewClientWithHeaders(cfg.baseURL, cfg.timeout, headers),
	}, nil
}

// ProviderName returns "groq".
func (p *GroqProvider) ProviderName() string {
	return groqProviderName
}

// Chat sends a chat completion request to Groq.
func (p *GroqProvider) Chat(ctx context.Context, req *ChatRequest) (*ChatResponse, error) {
	return openaiCompatibleChat(ctx, p.client, groqProviderName, req)
}

// ChatStream sends a streaming chat completion request to Groq.
func (p *GroqProvider) ChatStream(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan error) {
	return openaiCompatibleChatStream(ctx, p.client, groqProviderName, req)
}

// ListModels returns available models from the Groq API.
func (p *GroqProvider) ListModels(ctx context.Context) ([]ModelInfo, error) {
	return openaiCompatibleListModels(ctx, p.client, groqProviderName)
}

// Ping checks if the Groq API is reachable and the API key is valid.
func (p *GroqProvider) Ping(ctx context.Context) error {
	return openaiCompatiblePing(ctx, p.client, groqProviderName)
}

// GetCapabilities returns the features and parameter limits of the Groq API.
func (p *GroqProvider) GetCapabilities() ProviderCapabilities {
	maxStop := 4
	return ProviderCapabilities{
		SupportsSystemMessages:   true,
		SupportsStreaming:        true,
		SupportsVision:           false,
		MaxStopSequences:         &maxStop,
		SupportsPresencePenalty:  true,
		SupportsFrequencyPenalty: true,
		SupportsSeed:             false,
		SupportsLogprobs:         false,
		MaxLogprobs:              nil,
		SupportsJSONMode:         true,
		SupportsJSONSchema:       false,
		SupportsTools:            true, // limited models
		SupportsResponseFormat:   true,
		SupportsTopK:             false,
		SupportsMinP:             false,
		PenaltyRange:             &PenaltyRange{-2.0, 2.0},
	}
}

// StreamWithUsage sends a streaming chat request and provides final token usage.
func (p *GroqProvider) StreamWithUsage(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan TokenUsage) {
	return openaiCompatibleStreamWithUsage(ctx, p.client, groqProviderName, req)
}

// FreshSession returns the receiver unchanged (stateless provider).
// This method exists for API consistency with nxuskit-engine.
func (p *GroqProvider) FreshSession() (LLMProvider, error) {
	return p, nil
}
