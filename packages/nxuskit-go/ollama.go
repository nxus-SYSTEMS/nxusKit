package nxuskit

import (
	"bufio"
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
	ollamaDefaultBaseURL = "http://localhost:11434"
	ollamaProviderName   = "ollama"
)

// OllamaProvider connects to a local Ollama instance.
type OllamaProvider struct {
	client *httputil.Client
}

// ollamaConfig holds configuration for OllamaProvider.
type ollamaConfig struct {
	baseURL string
	timeout time.Duration
}

// OllamaOption configures an OllamaProvider.
type OllamaOption func(*ollamaConfig) error

// WithOllamaBaseURL sets a custom base URL for the Ollama server.
func WithOllamaBaseURL(url string) OllamaOption {
	return func(c *ollamaConfig) error {
		if url == "" {
			return fmt.Errorf("base URL cannot be empty")
		}
		c.baseURL = strings.TrimSuffix(url, "/")
		return nil
	}
}

// WithOllamaTimeout sets a custom timeout for HTTP requests.
func WithOllamaTimeout(timeout time.Duration) OllamaOption {
	return func(c *ollamaConfig) error {
		if timeout <= 0 {
			return fmt.Errorf("timeout must be positive")
		}
		c.timeout = timeout
		return nil
	}
}

// NewOllamaProvider creates a new Ollama provider.
// It checks OLLAMA_HOST environment variable if no base URL is provided.
func NewOllamaProvider(opts ...OllamaOption) (*OllamaProvider, error) {
	cfg := &ollamaConfig{
		baseURL: ollamaDefaultBaseURL,
		timeout: httputil.DefaultTimeout,
	}

	// Check environment variable for base URL
	if envURL := os.Getenv("OLLAMA_HOST"); envURL != "" {
		cfg.baseURL = strings.TrimSuffix(envURL, "/")
	}

	// Apply options
	for _, opt := range opts {
		if err := opt(cfg); err != nil {
			return nil, NewConfigurationError(err.Error(), err)
		}
	}

	return &OllamaProvider{
		client: httputil.NewClient(cfg.baseURL, cfg.timeout),
	}, nil
}

// ProviderName returns "ollama".
func (p *OllamaProvider) ProviderName() string {
	return ollamaProviderName
}

// Chat sends a chat completion request to Ollama.
func (p *OllamaProvider) Chat(ctx context.Context, req *ChatRequest) (*ChatResponse, error) {
	ollamaReq := &ollamaChatRequest{
		Model:    req.Model,
		Messages: convertToOllamaMessages(req.Messages),
		Stream:   false,
		Options:  convertOllamaOptions(req),
		Think:    convertThinkingModeToOllama(req.ThinkingMode),
	}

	resp, err := p.client.PostJSON(ctx, "/api/chat", ollamaReq)
	if err != nil {
		return nil, p.wrapError(err)
	}
	defer func() { _ = resp.Body.Close() }()

	if resp.StatusCode != http.StatusOK {
		return nil, p.handleErrorResponse(resp)
	}

	var ollamaResp ollamaChatResponse
	if err := json.NewDecoder(resp.Body).Decode(&ollamaResp); err != nil {
		return nil, NewProviderError(ollamaProviderName, "failed to decode response", 0, err)
	}

	return convertFromOllamaResponse(&ollamaResp), nil
}

// ChatStream sends a streaming chat completion request to Ollama.
func (p *OllamaProvider) ChatStream(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan error) {
	chunks := make(chan StreamChunk)
	errs := make(chan error, 1)

	go func() {
		defer close(chunks)
		defer close(errs)

		ollamaReq := &ollamaChatRequest{
			Model:    req.Model,
			Messages: convertToOllamaMessages(req.Messages),
			Stream:   true,
			Options:  convertOllamaOptions(req),
			Think:    convertThinkingModeToOllama(req.ThinkingMode),
		}

		resp, err := p.client.PostJSON(ctx, "/api/chat", ollamaReq)
		if err != nil {
			errs <- p.wrapError(err)
			return
		}
		defer func() { _ = resp.Body.Close() }()

		if resp.StatusCode != http.StatusOK {
			errs <- p.handleErrorResponse(resp)
			return
		}

		// Ollama uses NDJSON (newline-delimited JSON) for streaming
		scanner := bufio.NewScanner(resp.Body)
		var usage *TokenUsage

		for scanner.Scan() {
			select {
			case <-ctx.Done():
				errs <- ctx.Err()
				return
			default:
			}

			line := scanner.Text()
			if line == "" {
				continue
			}

			var ollamaResp ollamaChatResponse
			if err := json.Unmarshal([]byte(line), &ollamaResp); err != nil {
				errs <- NewStreamError(ollamaProviderName, "failed to parse stream chunk", err)
				return
			}

			chunk := StreamChunk{
				Delta: ollamaResp.Message.Content,
			}

			// Handle thinking content
			if ollamaResp.Thinking != "" {
				chunk.Thinking = &ollamaResp.Thinking
			}

			// Handle final chunk
			if ollamaResp.Done {
				reason := FinishReasonStop
				chunk.FinishReason = &reason

				if ollamaResp.EvalCount > 0 || ollamaResp.PromptEvalCount > 0 {
					usage = &TokenUsage{
						Actual: &TokenCount{
							PromptTokens:     ollamaResp.PromptEvalCount,
							CompletionTokens: ollamaResp.EvalCount,
						},
						IsComplete: true,
					}
					chunk.Usage = usage
				}
			}

			chunks <- chunk
		}

		if err := scanner.Err(); err != nil {
			errs <- NewStreamError(ollamaProviderName, "stream read error", err)
		}
	}()

	return chunks, errs
}

// ListModels returns available models from the Ollama server.
func (p *OllamaProvider) ListModels(ctx context.Context) ([]ModelInfo, error) {
	resp, err := p.client.Get(ctx, "/api/tags")
	if err != nil {
		return nil, p.wrapError(err)
	}
	defer func() { _ = resp.Body.Close() }()

	if resp.StatusCode != http.StatusOK {
		return nil, p.handleErrorResponse(resp)
	}

	var tagsResp ollamaTagsResponse
	if err := json.NewDecoder(resp.Body).Decode(&tagsResp); err != nil {
		return nil, NewProviderError(ollamaProviderName, "failed to decode models response", 0, err)
	}

	models := make([]ModelInfo, len(tagsResp.Models))
	for i, m := range tagsResp.Models {
		models[i] = convertFromOllamaModelInfo(m)
	}

	return models, nil
}

// Ping checks if the Ollama server is reachable.
func (p *OllamaProvider) Ping(ctx context.Context) error {
	resp, err := p.client.Get(ctx, "/")
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
func (p *OllamaProvider) wrapError(err error) error {
	if err == context.Canceled || err == context.DeadlineExceeded {
		return err
	}

	// Check for common network error patterns
	errStr := err.Error()
	if strings.Contains(errStr, "connection refused") ||
		strings.Contains(errStr, "no such host") ||
		strings.Contains(errStr, "dial tcp") ||
		strings.Contains(errStr, "timeout") {
		return NewNetworkError(ollamaProviderName, errStr, err)
	}

	return NewProviderError(ollamaProviderName, errStr, 0, err)
}

// handleErrorResponse parses an error response from Ollama.
func (p *OllamaProvider) handleErrorResponse(resp *http.Response) error {
	var errResp ollamaErrorResponse
	if err := json.NewDecoder(resp.Body).Decode(&errResp); err != nil {
		return NewProviderError(ollamaProviderName, fmt.Sprintf("HTTP %d", resp.StatusCode), resp.StatusCode, nil)
	}

	msg := errResp.Error
	if msg == "" {
		msg = fmt.Sprintf("HTTP %d", resp.StatusCode)
	}

	return NewProviderError(ollamaProviderName, msg, resp.StatusCode, nil)
}

// GetCapabilities returns the features and parameter limits of Ollama.
func (p *OllamaProvider) GetCapabilities() ProviderCapabilities {
	return ProviderCapabilities{
		SupportsSystemMessages:   true,
		SupportsStreaming:        true,
		SupportsVision:           true, // model-dependent, conservative true
		MaxStopSequences:         nil,  // unlimited
		SupportsPresencePenalty:  true,
		SupportsFrequencyPenalty: true,
		SupportsSeed:             true,
		SupportsLogprobs:         false,
		MaxLogprobs:              nil,
		SupportsJSONMode:         true,
		SupportsJSONSchema:       true,
		SupportsTools:            true, // model-dependent
		SupportsResponseFormat:   true,
		SupportsTopK:             true,
		SupportsMinP:             true,
		PenaltyRange:             &PenaltyRange{-2.0, 2.0},
	}
}

// StreamWithUsage sends a streaming chat request and provides final token usage.
func (p *OllamaProvider) StreamWithUsage(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan TokenUsage) {
	chunks, errs := p.ChatStream(ctx, req)
	return wrapStreamWithUsage(chunks, errs)
}

// FreshSession returns the receiver unchanged (stateless provider).
// This method exists for API consistency with nxuskit-engine.
func (p *OllamaProvider) FreshSession() (LLMProvider, error) {
	return p, nil
}

// ListAvailableModels returns models available from this provider.
// This method exists for API consistency with nxuskit-engine (ModelLister interface).
func (p *OllamaProvider) ListAvailableModels(ctx context.Context) ([]ModelInfo, error) {
	return p.ListModels(ctx)
}

// GetModelCapabilities returns capabilities for a specific Ollama model.
//
// This uses the Ollama show API to detect model-specific capabilities
// such as vision support.
func (p *OllamaProvider) GetModelCapabilities(ctx context.Context, model string) (ModelCapabilities, error) {
	// Request model details from Ollama show endpoint
	showReq := map[string]string{"name": model}

	resp, err := p.client.PostJSON(ctx, "/api/show", showReq)
	if err != nil {
		return ModelCapabilities{}, p.wrapError(err)
	}
	defer func() { _ = resp.Body.Close() }()

	if resp.StatusCode != http.StatusOK {
		return ModelCapabilities{}, p.handleErrorResponse(resp)
	}

	var showResp ollamaShowResponse
	if err := json.NewDecoder(resp.Body).Decode(&showResp); err != nil {
		return ModelCapabilities{}, NewProviderError(ollamaProviderName, "failed to decode show response", 0, err)
	}

	// Detect vision capability from model details
	visionMode := detectOllamaVisionMode(&showResp)

	return ModelCapabilities{
		VisionMode:        visionMode,
		SupportsStreaming: true,
	}, nil
}

// detectOllamaVisionMode analyzes model info to determine vision capabilities.
func detectOllamaVisionMode(showResp *ollamaShowResponse) VisionMode {
	// Check model families for vision models
	modelFamily := strings.ToLower(showResp.Details.Family)
	for _, family := range showResp.Details.Families {
		if strings.Contains(strings.ToLower(family), "clip") ||
			strings.Contains(strings.ToLower(family), "vision") ||
			strings.Contains(strings.ToLower(family), "llava") {
			return VisionModeMultiImage
		}
	}

	// Check family name
	if strings.Contains(modelFamily, "llava") ||
		strings.Contains(modelFamily, "vision") ||
		strings.Contains(modelFamily, "clip") {
		return VisionModeMultiImage
	}

	// Check model name in parameters or template
	modelInfo := strings.ToLower(showResp.Modelfile)
	if strings.Contains(modelInfo, "llava") ||
		strings.Contains(modelInfo, "vision") ||
		strings.Contains(modelInfo, "bakllava") ||
		strings.Contains(modelInfo, "moondream") {
		return VisionModeMultiImage
	}

	return VisionModeNone
}
