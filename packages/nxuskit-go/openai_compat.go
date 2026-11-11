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

func mergeProviderMetadata(current map[string]any, raw []byte, provider string) (map[string]any, error) {
	var fields, usageFields []string
	switch provider {
	case "openrouter":
		fields = []string{"id", "provider"}
		usageFields = []string{"cost", "cost_details", "prompt_tokens_details", "completion_tokens_details"}
	case "perplexity":
		fields = []string{"citations", "search_results"}
		usageFields = []string{"cost", "search_context_size", "citation_tokens", "num_search_queries", "reasoning_tokens"}
	default:
		return nil, nil
	}
	decoder := json.NewDecoder(strings.NewReader(string(raw)))
	decoder.UseNumber()
	var data map[string]any
	if err := decoder.Decode(&data); err != nil {
		return nil, NewProviderError(provider, "provider_metadata invalid JSON", 0, err)
	}
	if current == nil {
		current = map[string]any{}
	}
	for _, name := range fields {
		if value, ok := data[name]; ok {
			current[name] = value
		}
	}
	if value, ok := data["usage"]; ok {
		if value == nil {
			current["usage"] = nil
		} else if usage, ok := value.(map[string]any); ok {
			for _, name := range usageFields {
				if value, ok := usage[name]; ok {
					target, _ := current["usage"].(map[string]any)
					if target == nil {
						target = map[string]any{}
						current["usage"] = target
					}
					target[name] = value
				}
			}
		} else {
			return nil, NewProviderError(provider, "provider_metadata usage must be object or null", 0, nil)
		}
	}
	var depthOK func(any, int) bool
	depthOK = func(value any, depth int) bool {
		if depth > 16 {
			return false
		}
		switch value := value.(type) {
		case map[string]any:
			for _, child := range value {
				if !depthOK(child, depth+1) {
					return false
				}
			}
		case []any:
			for _, child := range value {
				if !depthOK(child, depth+1) {
					return false
				}
			}
		}
		return true
	}
	if !depthOK(current, 0) {
		return nil, NewProviderError(provider, "provider_metadata depth exceeds 16", 0, nil)
	}
	encoded, err := json.Marshal(current)
	if err != nil {
		return nil, NewProviderError(provider, "provider_metadata invalid JSON", 0, err)
	}
	if len(encoded) > 65536 {
		return nil, NewProviderError(provider, "provider_metadata compact UTF-8 bytes exceed 65536", 0, nil)
	}
	if len(current) == 0 {
		return nil, nil
	}
	return current, nil
}

// buildOpenAIRequest creates an OpenAI API request from a ChatRequest.
func buildOpenAIRequest(req *ChatRequest) *openaiChatRequest {
	or := &openaiChatRequest{
		Model:       req.Model,
		Messages:    convertToOpenAIMessages(req.Messages),
		Stream:      req.Stream,
		Logprobs:    req.Logprobs,
		TopLogprobs: req.TopLogprobs,
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
	if err := validateGenerationControls(req, providerName); err != nil {
		return nil, err
	}
	openaiReq := buildOpenAIRequest(req)

	resp, err := client.PostJSON(ctx, "/chat/completions", openaiReq)
	if err != nil {
		return nil, wrapOpenAICompatError(err, providerName)
	}
	defer func() { _ = resp.Body.Close() }()

	if resp.StatusCode != http.StatusOK {
		return nil, handleOpenAICompatErrorResponse(resp, providerName)
	}

	var raw json.RawMessage
	if err := decodeBoundedProviderResponse(resp, providerName, "failed to decode response", &raw); err != nil {
		return nil, err
	}
	var envelope map[string]json.RawMessage
	if err := json.Unmarshal(raw, &envelope); err != nil || envelope == nil {
		return nil, NewProviderError(providerName, "invalid response envelope", 0, err)
	}
	if _, failed := envelope["error"]; failed {
		return nil, NewProviderError(providerName, "provider returned an error envelope", 0, nil)
	}
	metadata, err := mergeProviderMetadata(nil, raw, providerName)
	if err != nil {
		return nil, err
	}
	var openaiResp openaiChatResponse
	if err := json.Unmarshal(raw, &openaiResp); err != nil {
		return nil, NewProviderError(providerName, "failed to decode response", 0, err)
	}
	result := convertFromOpenAIResponse(&openaiResp)
	result.ProviderMetadata = metadata
	return result, nil
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

		if err := validateGenerationControls(req, providerName); err != nil {
			errs <- err
			return
		}
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
		var rawFinishReason *string
		var providerMetadata map[string]any

		for {
			select {
			case <-ctx.Done():
				errs <- ctx.Err()
				return
			default:
			}

			event, err := reader.Read()
			if err == io.EOF {
				errs <- NewStreamError(providerName, "stream ended before [DONE]", nil)
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
				final := StreamChunk{Terminal: true, RawFinishReason: rawFinishReason, FinishReason: convertOpenAIFinishReason(rawFinishReason), Usage: usage}
				final.ProviderMetadata = providerMetadata
				select {
				case chunks <- final:
				case <-ctx.Done():
					errs <- ctx.Err()
				}
				return
			}

			// Parse the JSON chunk
			var envelope map[string]json.RawMessage
			if err := json.Unmarshal([]byte(event.Data), &envelope); err != nil || envelope == nil {
				errs <- NewStreamError(providerName, "invalid stream envelope", err)
				return
			}
			if _, failed := envelope["error"]; failed {
				errs <- NewStreamError(providerName, "provider returned a stream error", nil)
				return
			}
			providerMetadata, err = mergeProviderMetadata(providerMetadata, []byte(event.Data), providerName)
			if err != nil {
				errs <- err
				return
			}
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
				if choice.FinishReason != nil {
					rawFinishReason = choice.FinishReason
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

			select {
			case chunks <- chunk:
			case <-ctx.Done():
				errs <- ctx.Err()
				return
			}
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
	if err := decodeBoundedProviderResponse(resp, providerName, "failed to decode models response", &modelsResp); err != nil {
		return nil, err
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
	body, readErr := httputil.ReadBoundedBody(resp.Body)
	if readErr != nil {
		return openAICompatHTTPError(resp, providerName, readErr.Error(), readErr)
	}

	var errResp openaiErrorResponse
	if err := json.Unmarshal(body, &errResp); err != nil {
		return NewProviderError(providerName, fmt.Sprintf("HTTP %d", resp.StatusCode), resp.StatusCode, nil)
	}

	msg := errResp.Error.Message
	if msg == "" {
		msg = fmt.Sprintf("HTTP %d", resp.StatusCode)
	}

	return openAICompatHTTPError(resp, providerName, msg, nil)
}

func openAICompatHTTPError(resp *http.Response, providerName, message string, err error) error {
	switch resp.StatusCode {
	case 401:
		return NewAuthenticationError(providerName, message, err)
	case 400:
		return NewInvalidRequestError(providerName, message, err)
	case 429:
		retryAfter := parseRetryAfterHeader(resp.Header.Get("Retry-After"))
		return NewRateLimitError(providerName, retryAfter, err)
	default:
		return NewProviderError(providerName, message, resp.StatusCode, err)
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
