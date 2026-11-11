package nxuskit

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"strings"
	"time"

	"github.com/nxus-SYSTEMS/nxusKit/packages/nxuskit-go/internal/httputil"
)

const (
	claudeDefaultBaseURL = "https://api.anthropic.com/v1"
	claudeProviderName   = "claude"
	claudeDefaultVersion = "2023-06-01"
)

// ClaudeProvider connects to the Anthropic Claude API.
type ClaudeProvider struct {
	client *httputil.Client
}

// claudeConfig holds configuration for ClaudeProvider.
type claudeConfig struct {
	apiKey  string
	baseURL string
	timeout time.Duration
	version string
}

// ClaudeOption configures a ClaudeProvider.
type ClaudeOption func(*claudeConfig) error

// WithClaudeAPIKey sets the API key for authentication.
func WithClaudeAPIKey(key string) ClaudeOption {
	return func(c *claudeConfig) error {
		if key == "" {
			return fmt.Errorf("API key cannot be empty")
		}
		c.apiKey = key
		return nil
	}
}

// WithClaudeBaseURL sets a custom base URL for the Claude API.
func WithClaudeBaseURL(url string) ClaudeOption {
	return func(c *claudeConfig) error {
		if url == "" {
			return fmt.Errorf("base URL cannot be empty")
		}
		c.baseURL = strings.TrimSuffix(url, "/")
		return nil
	}
}

// WithClaudeTimeout sets a custom timeout for HTTP requests.
func WithClaudeTimeout(timeout time.Duration) ClaudeOption {
	return func(c *claudeConfig) error {
		if timeout <= 0 {
			return fmt.Errorf("timeout must be positive")
		}
		c.timeout = timeout
		return nil
	}
}

// WithClaudeVersion sets a custom API version header.
func WithClaudeVersion(version string) ClaudeOption {
	return func(c *claudeConfig) error {
		if version == "" {
			return fmt.Errorf("version cannot be empty")
		}
		c.version = version
		return nil
	}
}

// NewClaudeProvider creates a new Claude provider.
// It checks ANTHROPIC_API_KEY environment variable if no API key is provided via options.
func NewClaudeProvider(opts ...ClaudeOption) (*ClaudeProvider, error) {
	cfg := &claudeConfig{
		baseURL: claudeDefaultBaseURL,
		timeout: httputil.DefaultTimeout,
		version: claudeDefaultVersion,
	}

	// Check environment variable for API key
	if envKey := os.Getenv("ANTHROPIC_API_KEY"); envKey != "" {
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
		return nil, NewConfigurationError("API key is required: set ANTHROPIC_API_KEY environment variable or use WithClaudeAPIKey option", nil)
	}

	// Create HTTP client with required headers for Anthropic
	headers := map[string]string{
		"x-api-key":         cfg.apiKey,
		"anthropic-version": cfg.version,
	}

	return &ClaudeProvider{
		client: httputil.NewClientWithHeaders(cfg.baseURL, cfg.timeout, headers),
	}, nil
}

// ProviderName returns "claude".
func (p *ClaudeProvider) ProviderName() string {
	return claudeProviderName
}

// Chat sends a chat completion request to Claude.
func (p *ClaudeProvider) Chat(ctx context.Context, req *ChatRequest) (*ChatResponse, error) {
	if err := validateGenerationControls(req, claudeProviderName); err != nil {
		return nil, err
	}
	claudeReq := buildClaudeRequest(req)

	resp, err := p.client.PostJSON(ctx, "/messages", claudeReq)
	if err != nil {
		return nil, p.wrapError(err)
	}
	defer func() { _ = resp.Body.Close() }()

	if resp.StatusCode != http.StatusOK {
		return nil, p.handleErrorResponse(resp)
	}

	var claudeResp claudeMessagesResponse
	if err := decodeBoundedProviderResponse(resp, claudeProviderName, "failed to decode response", &claudeResp); err != nil {
		return nil, err
	}

	return convertFromClaudeResponse(&claudeResp), nil
}

// ChatStream sends a streaming chat completion request to Claude.
func (p *ClaudeProvider) ChatStream(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan error) {
	chunks := make(chan StreamChunk)
	errs := make(chan error, 1)

	go func() {
		defer close(chunks)
		defer close(errs)

		if err := validateGenerationControls(req, claudeProviderName); err != nil {
			errs <- err
			return
		}
		streamReq := *req
		streamReq.Stream = true
		claudeReq := buildClaudeRequest(&streamReq)

		resp, err := p.client.PostJSON(ctx, "/messages", claudeReq)
		if err != nil {
			errs <- p.wrapError(err)
			return
		}
		defer func() { _ = resp.Body.Close() }()

		if resp.StatusCode != http.StatusOK {
			errs <- p.handleErrorResponse(resp)
			return
		}

		// Parse Claude SSE stream
		reader := httputil.NewSSEReader(resp.Body)
		var currentBlockType string
		var rawReason *string
		var usage *TokenUsage

		for {
			select {
			case <-ctx.Done():
				errs <- ctx.Err()
				return
			default:
			}

			event, err := reader.Read()
			if err == io.EOF {
				errs <- NewStreamError(claudeProviderName, "stream ended before message_stop", io.ErrUnexpectedEOF)
				return
			}
			if err != nil {
				errs <- NewStreamError(claudeProviderName, "failed to read stream", err)
				return
			}
			if event == nil {
				continue
			}

			// Handle Claude-specific events
			var envelope claudeStreamEvent
			if err := json.Unmarshal([]byte(event.Data), &envelope); err != nil || envelope.Type == "" {
				errs <- NewStreamError(claudeProviderName, "invalid stream envelope", err)
				return
			}
			chunk, err := p.processClaudeStreamEvent(event, envelope.Type, &currentBlockType)
			if err != nil {
				errs <- NewStreamError(claudeProviderName, "invalid stream event", err)
				return
			}
			if chunk != nil {
				if chunk.RawFinishReason != nil {
					rawReason = chunk.RawFinishReason
				}
				if chunk.Usage != nil && chunk.Usage.Actual != nil {
					if usage == nil {
						usage = &TokenUsage{Actual: &TokenCount{}}
					}
					if envelope.Type == "message_start" {
						usage.Actual.PromptTokens = chunk.Usage.Actual.PromptTokens
					}
					usage.Actual.CompletionTokens = chunk.Usage.Actual.CompletionTokens
				}
				if chunk.Terminal {
					chunk.RawFinishReason = rawReason
					if rawReason != nil {
						chunk.FinishReason = convertClaudeStopReason(*rawReason)
					}
					if usage != nil {
						usage.IsComplete = true
					}
					chunk.Usage = usage
				} else {
					chunk.FinishReason = nil
					chunk.RawFinishReason = nil
				}
				select {
				case chunks <- *chunk:
				case <-ctx.Done():
					errs <- ctx.Err()
					return
				}
				if chunk.Terminal {
					return
				}
			}
		}
	}()

	return chunks, errs
}

// processClaudeStreamEvent processes a single Claude SSE event and returns a StreamChunk if applicable.
func (p *ClaudeProvider) processClaudeStreamEvent(event *httputil.SSEEvent, eventType string, currentBlockType *string) (*StreamChunk, error) {
	switch eventType {
	case "error":
		var failure claudeErrorResponse
		if err := json.Unmarshal([]byte(event.Data), &failure); err != nil {
			return nil, err
		}
		return nil, fmt.Errorf("%s: %s", failure.Error.Type, failure.Error.Message)
	case "message_start":
		var start claudeMessageStart
		if err := json.Unmarshal([]byte(event.Data), &start); err != nil {
			return nil, err
		}
		return &StreamChunk{Usage: &TokenUsage{Actual: &TokenCount{PromptTokens: start.Message.Usage.InputTokens, CompletionTokens: start.Message.Usage.OutputTokens}}}, nil
	case "content_block_start":
		var blockStart claudeContentBlockStart
		if err := json.Unmarshal([]byte(event.Data), &blockStart); err != nil {
			return nil, err
		}
		*currentBlockType = blockStart.ContentBlock.Type
		return nil, nil

	case "content_block_delta":
		var delta claudeContentBlockDelta
		if err := json.Unmarshal([]byte(event.Data), &delta); err != nil {
			return nil, err
		}

		chunk := &StreamChunk{}
		switch delta.Delta.Type {
		case "text_delta":
			chunk.Delta = delta.Delta.Text
		case "thinking_delta":
			chunk.Thinking = &delta.Delta.Thinking
		}
		return chunk, nil

	case "message_delta":
		var msgDelta claudeMessageDelta
		if err := json.Unmarshal([]byte(event.Data), &msgDelta); err != nil {
			return nil, err
		}

		chunk := &StreamChunk{}
		if msgDelta.Delta.StopReason != "" {
			chunk.RawFinishReason = &msgDelta.Delta.StopReason
			chunk.FinishReason = convertClaudeStopReason(msgDelta.Delta.StopReason)
		}
		if msgDelta.Usage != nil {
			chunk.Usage = &TokenUsage{
				Actual: &TokenCount{
					CompletionTokens: msgDelta.Usage.OutputTokens,
				},
				IsComplete: true,
			}
		}
		return chunk, nil

	case "message_stop":
		return &StreamChunk{Terminal: true}, nil
	}

	return nil, nil
}

// ListModels returns available Claude models.
// Since Anthropic doesn't provide a models endpoint, we return a hardcoded list.
func (p *ClaudeProvider) ListModels(ctx context.Context) ([]ModelInfo, error) {
	// Hardcoded list of Claude models
	models := []ModelInfo{
		{Name: "claude-opus-4-20250514", Metadata: map[string]any{"context_window": 200000}},
		{Name: "claude-sonnet-4-20250514", Metadata: map[string]any{"context_window": 200000}},
		{Name: "claude-haiku-4-5-20251001", Metadata: map[string]any{"context_window": 200000}},
		{Name: "claude-3-5-sonnet-20241022", Metadata: map[string]any{"context_window": 200000}},
		{Name: "claude-3-5-haiku-20241022", Metadata: map[string]any{"context_window": 200000}},
		{Name: "claude-3-opus-20240229", Metadata: map[string]any{"context_window": 200000}},
		{Name: "claude-3-sonnet-20240229", Metadata: map[string]any{"context_window": 200000}},
		{Name: "claude-3-haiku-20240307", Metadata: map[string]any{"context_window": 200000}},
	}

	return models, nil
}

// Ping checks if the Claude API is reachable and the API key is valid.
func (p *ClaudeProvider) Ping(ctx context.Context) error {
	// Send a minimal messages request to verify connectivity
	pingReq := &claudeMessagesRequest{
		Model:     "claude-haiku-4-5-20251001",
		MaxTokens: 1,
		Messages: []claudeMessage{
			{Role: "user", Content: "ping"},
		},
	}

	resp, err := p.client.PostJSON(ctx, "/messages", pingReq)
	if err != nil {
		return p.wrapError(err)
	}
	defer func() { _ = resp.Body.Close() }()

	if resp.StatusCode != http.StatusOK {
		return p.handleErrorResponse(resp)
	}

	return nil
}

// wrapError wraps network and context errors appropriately.
func (p *ClaudeProvider) wrapError(err error) error {
	if err == context.Canceled || err == context.DeadlineExceeded {
		return err
	}

	errStr := err.Error()
	if strings.Contains(errStr, "connection refused") ||
		strings.Contains(errStr, "no such host") ||
		strings.Contains(errStr, "dial tcp") ||
		strings.Contains(errStr, "timeout") {
		return NewNetworkError(claudeProviderName, errStr, err)
	}

	return NewProviderError(claudeProviderName, errStr, 0, err)
}

// handleErrorResponse parses an error response from Claude.
func (p *ClaudeProvider) handleErrorResponse(resp *http.Response) error {
	body, err := httputil.ReadBoundedBody(resp.Body)
	if err != nil {
		return claudeHTTPErrorForStatus(resp.StatusCode, err.Error(), claudeProviderName, err)
	}
	return parseClaudeErrorResponse(resp.StatusCode, body, claudeProviderName)
}

// GetCapabilities returns the features and parameter limits of the Claude API.
func (p *ClaudeProvider) GetCapabilities() ProviderCapabilities {
	maxStop := 8192
	return ProviderCapabilities{
		SupportsSystemMessages:   true,
		SupportsStreaming:        true,
		SupportsVision:           true,
		MaxStopSequences:         &maxStop,
		SupportsPresencePenalty:  false,
		SupportsFrequencyPenalty: false,
		SupportsSeed:             false,
		SupportsLogprobs:         false,
		MaxLogprobs:              nil,
		SupportsJSONMode:         false,
		SupportsJSONSchema:       false,
		SupportsTools:            true,
		SupportsResponseFormat:   false,
		SupportsTopK:             true,
		SupportsMinP:             false,
		PenaltyRange:             nil,
	}
}

// StreamWithUsage sends a streaming chat request and provides final token usage.
func (p *ClaudeProvider) StreamWithUsage(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan TokenUsage) {
	chunks, errs := p.ChatStream(ctx, req)
	return wrapStreamWithUsage(chunks, errs)
}

// FreshSession returns the receiver unchanged (stateless provider).
// This method exists for API consistency with nxuskit-engine.
func (p *ClaudeProvider) FreshSession() (LLMProvider, error) {
	return p, nil
}
