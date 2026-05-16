package nxuskit

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"os"
	"strings"
	"time"

	"github.com/nxus-SYSTEMS/nxusKit/packages/nxuskit-go/internal/httputil"
)

const (
	lmstudioDefaultBaseURL = "http://localhost:1234/v1"
	lmstudioProviderName   = "lmstudio"
)

// LmStudioProvider connects to an LM Studio server (OpenAI-compatible).
type LmStudioProvider struct {
	client *httputil.Client
}

// lmstudioConfig holds configuration for LmStudioProvider.
type lmstudioConfig struct {
	baseURL string
	timeout time.Duration
}

// LmStudioOption configures an LmStudioProvider.
type LmStudioOption func(*lmstudioConfig) error

// WithLmStudioBaseURL sets a custom base URL for the LM Studio server.
func WithLmStudioBaseURL(url string) LmStudioOption {
	return func(c *lmstudioConfig) error {
		if url == "" {
			return fmt.Errorf("base URL cannot be empty")
		}
		c.baseURL = strings.TrimSuffix(url, "/")
		return nil
	}
}

// WithLmStudioTimeout sets a custom timeout for HTTP requests.
func WithLmStudioTimeout(timeout time.Duration) LmStudioOption {
	return func(c *lmstudioConfig) error {
		if timeout <= 0 {
			return fmt.Errorf("timeout must be positive")
		}
		c.timeout = timeout
		return nil
	}
}

// NewLmStudioProvider creates a new LM Studio provider.
// It checks LMSTUDIO_BASE_URL environment variable if no base URL is provided.
func NewLmStudioProvider(opts ...LmStudioOption) (*LmStudioProvider, error) {
	cfg := &lmstudioConfig{
		baseURL: lmstudioDefaultBaseURL,
		timeout: httputil.DefaultTimeout,
	}

	// Check environment variable for base URL
	if envURL := os.Getenv("LMSTUDIO_BASE_URL"); envURL != "" {
		cfg.baseURL = strings.TrimSuffix(envURL, "/")
	}

	// Apply options
	for _, opt := range opts {
		if err := opt(cfg); err != nil {
			return nil, NewConfigurationError(err.Error(), err)
		}
	}

	return &LmStudioProvider{
		client: httputil.NewClient(cfg.baseURL, cfg.timeout),
	}, nil
}

// ProviderName returns "lmstudio".
func (p *LmStudioProvider) ProviderName() string {
	return lmstudioProviderName
}

// Chat sends a chat completion request to LM Studio.
func (p *LmStudioProvider) Chat(ctx context.Context, req *ChatRequest) (*ChatResponse, error) {
	openaiReq := &openaiChatRequest{
		Model:            req.Model,
		Messages:         convertToOpenAIMessages(req.Messages),
		Stream:           false,
		Temperature:      req.Temperature,
		MaxTokens:        req.MaxTokens,
		TopP:             req.TopP,
		PresencePenalty:  req.PresencePenalty,
		FrequencyPenalty: req.FrequencyPenalty,
		Stop:             req.Stop,
		Seed:             req.Seed,
	}

	resp, err := p.client.PostJSON(ctx, "/chat/completions", openaiReq)
	if err != nil {
		return nil, p.wrapError(err)
	}
	defer func() { _ = resp.Body.Close() }()

	if resp.StatusCode != http.StatusOK {
		return nil, p.handleErrorResponse(resp)
	}

	var openaiResp openaiChatResponse
	if err := json.NewDecoder(resp.Body).Decode(&openaiResp); err != nil {
		return nil, NewProviderError(lmstudioProviderName, "failed to decode response", 0, err)
	}

	return convertFromOpenAIResponse(&openaiResp), nil
}

// ChatStream sends a streaming chat completion request to LM Studio.
func (p *LmStudioProvider) ChatStream(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan error) {
	chunks := make(chan StreamChunk)
	errs := make(chan error, 1)

	go func() {
		defer close(chunks)
		defer close(errs)

		openaiReq := &openaiChatRequest{
			Model:            req.Model,
			Messages:         convertToOpenAIMessages(req.Messages),
			Stream:           true,
			Temperature:      req.Temperature,
			MaxTokens:        req.MaxTokens,
			TopP:             req.TopP,
			PresencePenalty:  req.PresencePenalty,
			FrequencyPenalty: req.FrequencyPenalty,
			Stop:             req.Stop,
			Seed:             req.Seed,
		}

		resp, err := p.client.PostJSON(ctx, "/chat/completions", openaiReq)
		if err != nil {
			errs <- p.wrapError(err)
			return
		}
		defer func() { _ = resp.Body.Close() }()

		if resp.StatusCode != http.StatusOK {
			errs <- p.handleErrorResponse(resp)
			return
		}

		// LM Studio uses SSE (Server-Sent Events) for streaming
		sseReader := httputil.NewSSEReader(resp.Body)
		var usage *TokenUsage

		for {
			select {
			case <-ctx.Done():
				errs <- ctx.Err()
				return
			default:
			}

			event, err := sseReader.Read()
			if err != nil {
				// EOF is normal end of stream
				return
			}

			// Check for done signal
			if event.IsDone() {
				return
			}

			var streamResp openaiChatResponse
			if err := json.Unmarshal([]byte(event.Data), &streamResp); err != nil {
				errs <- NewStreamError(lmstudioProviderName, "failed to parse stream chunk", err)
				return
			}

			if len(streamResp.Choices) == 0 {
				continue
			}

			choice := streamResp.Choices[0]
			chunk := StreamChunk{}

			if choice.Delta != nil {
				chunk.Delta = choice.Delta.Content
			}

			// Handle finish reason
			if choice.FinishReason != nil {
				fr := ParseFinishReason(*choice.FinishReason)
				chunk.FinishReason = &fr

				// Usage is typically in the final chunk
				if streamResp.Usage != nil {
					usage = &TokenUsage{
						Actual: &TokenCount{
							PromptTokens:     streamResp.Usage.PromptTokens,
							CompletionTokens: streamResp.Usage.CompletionTokens,
						},
						IsComplete: true,
					}
					chunk.Usage = usage
				}
			}

			chunks <- chunk
		}
	}()

	return chunks, errs
}

// ListModels returns available models from the LM Studio server.
func (p *LmStudioProvider) ListModels(ctx context.Context) ([]ModelInfo, error) {
	resp, err := p.client.Get(ctx, "/models")
	if err != nil {
		return nil, p.wrapError(err)
	}
	defer func() { _ = resp.Body.Close() }()

	if resp.StatusCode != http.StatusOK {
		return nil, p.handleErrorResponse(resp)
	}

	var modelsResp openaiModelsResponse
	if err := json.NewDecoder(resp.Body).Decode(&modelsResp); err != nil {
		return nil, NewProviderError(lmstudioProviderName, "failed to decode models response", 0, err)
	}

	models := make([]ModelInfo, len(modelsResp.Data))
	for i, m := range modelsResp.Data {
		models[i] = convertFromOpenAIModelInfo(m)
	}

	return models, nil
}

// Ping checks if the LM Studio server is reachable.
func (p *LmStudioProvider) Ping(ctx context.Context) error {
	resp, err := p.client.Get(ctx, "/models")
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
func (p *LmStudioProvider) wrapError(err error) error {
	if err == context.Canceled || err == context.DeadlineExceeded {
		return err
	}

	// Check for common network error patterns
	errStr := err.Error()
	if strings.Contains(errStr, "connection refused") ||
		strings.Contains(errStr, "no such host") ||
		strings.Contains(errStr, "dial tcp") ||
		strings.Contains(errStr, "timeout") {
		return NewNetworkError(lmstudioProviderName, errStr, err)
	}

	return NewProviderError(lmstudioProviderName, errStr, 0, err)
}

// handleErrorResponse parses an error response from LM Studio.
func (p *LmStudioProvider) handleErrorResponse(resp *http.Response) error {
	var errResp openaiErrorResponse
	if err := json.NewDecoder(resp.Body).Decode(&errResp); err != nil {
		return NewProviderError(lmstudioProviderName, fmt.Sprintf("HTTP %d", resp.StatusCode), resp.StatusCode, nil)
	}

	msg := errResp.Error.Message
	if msg == "" {
		msg = fmt.Sprintf("HTTP %d", resp.StatusCode)
	}

	return NewProviderError(lmstudioProviderName, msg, resp.StatusCode, nil)
}

// GetCapabilities returns the features and parameter limits of LM Studio.
func (p *LmStudioProvider) GetCapabilities() ProviderCapabilities {
	return ProviderCapabilities{
		SupportsSystemMessages:   true,
		SupportsStreaming:        true,
		SupportsVision:           false,
		MaxStopSequences:         nil,
		SupportsPresencePenalty:  true,
		SupportsFrequencyPenalty: true,
		SupportsSeed:             true,
		SupportsLogprobs:         false,
		MaxLogprobs:              nil,
		SupportsJSONMode:         false, // backend-dependent
		SupportsJSONSchema:       false,
		SupportsTools:            false, // backend-dependent
		SupportsResponseFormat:   false, // backend-dependent
		SupportsTopK:             false,
		SupportsMinP:             false,
		PenaltyRange:             &PenaltyRange{-2.0, 2.0},
	}
}

// StreamWithUsage sends a streaming chat request and provides final token usage.
func (p *LmStudioProvider) StreamWithUsage(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan TokenUsage) {
	chunks, errs := p.ChatStream(ctx, req)
	return wrapStreamWithUsage(chunks, errs)
}

// FreshSession returns the receiver unchanged (stateless provider).
// This method exists for API consistency with nxuskit-engine.
func (p *LmStudioProvider) FreshSession() (LLMProvider, error) {
	return p, nil
}

// ListAvailableModels returns models available from this provider.
// This method exists for API consistency with nxuskit-engine (ModelLister interface).
func (p *LmStudioProvider) ListAvailableModels(ctx context.Context) ([]ModelInfo, error) {
	return p.ListModels(ctx)
}
