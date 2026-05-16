package nxuskit

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"

	"github.com/nxus-SYSTEMS/nxusKit/packages/nxuskit-go/internal/httputil"
)

// openaiCompatConfig holds common configuration for OpenAI-compatible providers.
// Currently unused but kept for future provider consolidation.
type openaiCompatConfig struct {
	baseURL string            //nolint:unused
	apiKey  string            //nolint:unused
	timeout time.Duration     //nolint:unused
	headers map[string]string //nolint:unused
}

var _ = openaiCompatConfig{} // silence unused warning

// buildOpenAIRequest creates an OpenAI API request from a ChatRequest.
func buildOpenAIRequest(req *ChatRequest) *openaiChatRequest {
	or := &openaiChatRequest{
		Model:    req.Model,
		Messages: convertToOpenAIMessages(req.Messages),
		Stream:   req.Stream,
	}

	if req.Temperature != nil {
		or.Temperature = req.Temperature
	}
	if req.MaxTokens != nil {
		or.MaxTokens = req.MaxTokens
	}
	if req.TopP != nil {
		or.TopP = req.TopP
	}
	if len(req.Stop) > 0 {
		or.Stop = req.Stop
	}
	if req.PresencePenalty != nil {
		or.PresencePenalty = req.PresencePenalty
	}
	if req.FrequencyPenalty != nil {
		or.FrequencyPenalty = req.FrequencyPenalty
	}
	if req.Seed != nil {
		or.Seed = req.Seed
	}

	// Handle ResponseFormat
	if req.ResponseFormat != nil {
		or.ResponseFormat = convertToOpenAIResponseFormat(req.ResponseFormat)
	}

	// Handle Tools
	if len(req.Tools) > 0 {
		or.Tools = convertToOpenAITools(req.Tools)
	}

	// Handle ToolChoice
	if req.ToolChoice != nil {
		or.ToolChoice = convertToOpenAIToolChoice(req.ToolChoice)
	}

	// Request usage in streaming responses
	if req.Stream {
		or.StreamOptions = &streamOptions{IncludeUsage: true}
	}

	return or
}

// convertToOpenAIResponseFormat converts ResponseFormat to OpenAI format.
func convertToOpenAIResponseFormat(rf *ResponseFormat) *openaiResponseFormat {
	orf := &openaiResponseFormat{
		Type: rf.Type,
	}
	if rf.JSONSchema != nil {
		orf.JSONSchema = &openaiJSONSchema{
			Name:        rf.JSONSchema.Name,
			Description: rf.JSONSchema.Description,
			Schema:      rf.JSONSchema.Schema,
			Strict:      rf.JSONSchema.Strict,
		}
	}
	return orf
}

// convertToOpenAITools converts Tools to OpenAI format.
func convertToOpenAITools(tools []Tool) []openaiTool {
	result := make([]openaiTool, len(tools))
	for i, t := range tools {
		result[i] = openaiTool{
			Type: t.Type,
			Function: openaiToolFunction{
				Name:        t.Function.Name,
				Description: t.Function.Description,
				Parameters:  t.Function.Parameters,
			},
		}
	}
	return result
}

// convertToOpenAIToolChoice converts ToolChoice to OpenAI format.
// OpenAI accepts "auto", "none", "required", or {"type": "function", "function": {"name": "..."}}
func convertToOpenAIToolChoice(tc *ToolChoice) any {
	switch tc.Type {
	case "auto", "none", "required":
		return tc.Type
	case "function":
		if tc.Function != nil {
			return openaiToolChoice{
				Type:     "function",
				Function: &openaiToolChoiceFunction{Name: tc.Function.Name},
			}
		}
		return "auto"
	default:
		return "auto"
	}
}

// openaiCompatibleChat sends a chat request using OpenAI format.
// This is used by OpenAI-compatible providers (Groq, Fireworks, Together, etc.).
func openaiCompatibleChat(
	ctx context.Context,
	client *httputil.Client,
	providerName string,
	req *ChatRequest,
) (*ChatResponse, error) {
	openaiReq := buildOpenAIRequest(req)

	resp, err := client.PostJSON(ctx, "/chat/completions", openaiReq)
	if err != nil {
		return nil, wrapOpenAICompatError(err, providerName)
	}
	defer func() { _ = resp.Body.Close() }()

	if resp.StatusCode != http.StatusOK {
		return nil, handleOpenAICompatErrorResponse(resp, providerName)
	}

	var openaiResp openaiChatResponse
	if err := json.NewDecoder(resp.Body).Decode(&openaiResp); err != nil {
		return nil, NewProviderError(providerName, "failed to decode response", 0, err)
	}

	return convertFromOpenAIResponse(&openaiResp), nil
}

// openaiCompatibleChatStream handles streaming using OpenAI format.
func openaiCompatibleChatStream(
	ctx context.Context,
	client *httputil.Client,
	providerName string,
	req *ChatRequest,
) (<-chan StreamChunk, <-chan error) {
	chunks := make(chan StreamChunk)
	errs := make(chan error, 1)

	go func() {
		defer close(chunks)
		defer close(errs)

		streamReq := *req
		streamReq.Stream = true
		openaiReq := buildOpenAIRequest(&streamReq)

		resp, err := client.PostJSON(ctx, "/chat/completions", openaiReq)
		if err != nil {
			errs <- wrapOpenAICompatError(err, providerName)
			return
		}
		defer func() { _ = resp.Body.Close() }()

		if resp.StatusCode != http.StatusOK {
			errs <- handleOpenAICompatErrorResponse(resp, providerName)
			return
		}

		// Parse SSE stream
		reader := httputil.NewSSEReader(resp.Body)
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
				return
			}
			if err != nil {
				errs <- NewStreamError(providerName, "failed to read stream", err)
				return
			}
			if event == nil {
				continue
			}

			// Check for [DONE] marker
			if event.IsDone() {
				return
			}

			// Parse the JSON chunk
			var streamResp openaiChatResponse
			if err := json.Unmarshal([]byte(event.Data), &streamResp); err != nil {
				errs <- NewStreamError(providerName, "failed to parse stream chunk", err)
				return
			}

			chunk := StreamChunk{}

			// Extract content from delta
			if len(streamResp.Choices) > 0 {
				choice := streamResp.Choices[0]
				if choice.Delta != nil {
					chunk.Delta = choice.Delta.Content
				}
				if choice.FinishReason != nil && *choice.FinishReason != "" {
					fr := ParseFinishReason(*choice.FinishReason)
					chunk.FinishReason = &fr
				}
				// Decode streaming logprobs (FR-007: nil when provider doesn't emit).
				chunk.Logprobs = decodeOAILogprobDelta(choice.Logprobs)
			}

			// Capture usage from final chunk
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

			chunks <- chunk
		}
	}()

	return chunks, errs
}

// openaiCompatibleListModels fetches models using OpenAI format.
func openaiCompatibleListModels(
	ctx context.Context,
	client *httputil.Client,
	providerName string,
) ([]ModelInfo, error) {
	resp, err := client.Get(ctx, "/models")
	if err != nil {
		return nil, wrapOpenAICompatError(err, providerName)
	}
	defer func() { _ = resp.Body.Close() }()

	if resp.StatusCode != http.StatusOK {
		return nil, handleOpenAICompatErrorResponse(resp, providerName)
	}

	var modelsResp openaiModelsResponse
	if err := json.NewDecoder(resp.Body).Decode(&modelsResp); err != nil {
		return nil, NewProviderError(providerName, "failed to decode models response", 0, err)
	}

	models := make([]ModelInfo, len(modelsResp.Data))
	for i, m := range modelsResp.Data {
		models[i] = convertFromOpenAIModelInfo(m)
	}

	return models, nil
}

// openaiCompatiblePing verifies API connectivity.
func openaiCompatiblePing(
	ctx context.Context,
	client *httputil.Client,
	providerName string,
) error {
	resp, err := client.Get(ctx, "/models")
	if err != nil {
		return wrapOpenAICompatError(err, providerName)
	}
	defer func() { _ = resp.Body.Close() }()

	if resp.StatusCode != http.StatusOK {
		return handleOpenAICompatErrorResponse(resp, providerName)
	}

	return nil
}

// wrapOpenAICompatError wraps network and context errors.
func wrapOpenAICompatError(err error, providerName string) error {
	if err == context.Canceled || err == context.DeadlineExceeded {
		return err
	}

	errStr := err.Error()
	if strings.Contains(errStr, "connection refused") ||
		strings.Contains(errStr, "no such host") ||
		strings.Contains(errStr, "dial tcp") ||
		strings.Contains(errStr, "timeout") {
		return NewNetworkError(providerName, errStr, err)
	}

	return NewProviderError(providerName, errStr, 0, err)
}

// handleOpenAICompatErrorResponse parses an error response.
func handleOpenAICompatErrorResponse(resp *http.Response, providerName string) error {
	body, _ := io.ReadAll(resp.Body)

	var errResp openaiErrorResponse
	if err := json.Unmarshal(body, &errResp); err != nil {
		return NewProviderError(providerName, fmt.Sprintf("HTTP %d", resp.StatusCode), resp.StatusCode, nil)
	}

	msg := errResp.Error.Message
	if msg == "" {
		msg = fmt.Sprintf("HTTP %d", resp.StatusCode)
	}

	switch resp.StatusCode {
	case 401:
		return NewAuthenticationError(providerName, msg, nil)
	case 400:
		return NewInvalidRequestError(providerName, msg, nil)
	case 429:
		retryAfter := parseRetryAfterHeader(resp.Header.Get("Retry-After"))
		return NewRateLimitError(providerName, retryAfter, nil)
	default:
		return NewProviderError(providerName, msg, resp.StatusCode, nil)
	}
}

// parseRetryAfterHeader parses the Retry-After header using the shared ParseRetryAfter function.
func parseRetryAfterHeader(value string) time.Duration {
	if d := ParseRetryAfter(value); d != nil {
		return *d
	}
	return 0
}

// openaiCompatibleStreamWithUsage wraps ChatStream to provide final token usage.
func openaiCompatibleStreamWithUsage(
	ctx context.Context,
	client *httputil.Client,
	providerName string,
	req *ChatRequest,
) (<-chan StreamChunk, <-chan TokenUsage) {
	chunks, errs := openaiCompatibleChatStream(ctx, client, providerName, req)
	return wrapStreamWithUsage(chunks, errs)
}

// decodeOAILogprobDelta converts an OpenAI SSE logprob payload into StreamLogprobsDelta.
// Returns nil when the payload is nil or has no content entries.
func decodeOAILogprobDelta(lp *openaiLogprobContent) *StreamLogprobsDelta {
	if lp == nil || len(lp.Content) == 0 {
		return nil
	}
	tokens := make([]TokenLogprob, len(lp.Content))
	for i, tok := range lp.Content {
		top := make([]TopLogprob, len(tok.TopLogprobs))
		for j, t := range tok.TopLogprobs {
			top[j] = TopLogprob(t)
		}
		tokens[i] = TokenLogprob{
			Token:       tok.Token,
			Logprob:     tok.Logprob,
			Bytes:       tok.Bytes,
			TopLogprobs: top,
		}
	}
	return &StreamLogprobsDelta{Content: tokens}
}

// wrapStreamWithUsage wraps chunk and error channels to extract final token usage.
// This is a shared helper for all providers.
func wrapStreamWithUsage(chunks <-chan StreamChunk, errs <-chan error) (<-chan StreamChunk, <-chan TokenUsage) {
	outChunks := make(chan StreamChunk)
	usageChan := make(chan TokenUsage, 1)

	go func() {
		defer close(outChunks)
		defer close(usageChan)

		var finalUsage TokenUsage
		for chunk := range chunks {
			if chunk.Usage != nil {
				finalUsage = *chunk.Usage
			}
			outChunks <- chunk
		}

		// Check for stream error
		if err := <-errs; err != nil {
			finalUsage.IsComplete = false
		}

		usageChan <- finalUsage
	}()

	return outChunks, usageChan
}
