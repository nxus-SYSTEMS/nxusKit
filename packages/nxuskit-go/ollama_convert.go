package nxuskit

import (
	"math"
	"strings"
)

func validateOllamaSampling(req *ChatRequest) error {
	if err := validateGenerationControls(req, ollamaProviderName); err != nil {
		return err
	}
	if len(req.Stop) > 16 {
		return NewInvalidRequestError(ollamaProviderName, "stop must contain at most 16 sequences", nil)
	}
	for _, stop := range req.Stop {
		if stop == "" {
			return NewInvalidRequestError(ollamaProviderName, "stop sequences must be nonempty", nil)
		}
	}
	if req.TopK != nil && (*req.TopK < 1 || *req.TopK > 2147483647) {
		return NewInvalidRequestError(ollamaProviderName, "top_k must be between 1 and 2147483647", nil)
	}
	if req.MinP != nil && (math.IsNaN(*req.MinP) || math.IsInf(*req.MinP, 0) || *req.MinP < 0 || *req.MinP > 1) {
		return NewInvalidRequestError(ollamaProviderName, "min_p must be finite and between 0 and 1", nil)
	}
	return nil
}

// convertToOllamaMessages converts LLMKit messages to Ollama format.
func convertToOllamaMessages(messages []Message) []ollamaMessage {
	result := make([]ollamaMessage, len(messages))
	for i, msg := range messages {
		result[i] = convertToOllamaMessage(msg)
	}
	return result
}

// convertToOllamaMessage converts a single LLMKit message to Ollama format.
func convertToOllamaMessage(msg Message) ollamaMessage {
	om := ollamaMessage{
		Role: msg.Role.String(),
	}

	if msg.Content.IsMultimodal() {
		// Extract text and images from parts
		var text string
		var images []string

		for _, part := range msg.Content.Parts {
			switch part.Type {
			case contentTypeText:
				text = part.Text
			case contentTypeImage:
				if part.Image != nil {
					if part.Image.Base64 != "" {
						images = append(images, part.Image.Base64)
					} else if part.Image.URL != "" {
						// Ollama expects base64, but we include URL as-is
						// The model may or may not handle it
						images = append(images, part.Image.URL)
					}
				}
			}
		}

		om.Content = text
		om.Images = images
	} else {
		om.Content = msg.Content.Text
	}

	return om
}

// convertOllamaOptions converts ChatRequest options to Ollama options map.
func convertOllamaOptions(req *ChatRequest) map[string]interface{} {
	opts := make(map[string]interface{})
	if len(req.Stop) != 0 {
		opts["stop"] = req.Stop
	}

	if req.Temperature != nil {
		opts["temperature"] = *req.Temperature
	}
	if req.MaxTokens != nil {
		opts["num_predict"] = *req.MaxTokens
	}
	if req.TopP != nil {
		opts["top_p"] = *req.TopP
	}
	if req.Seed != nil {
		opts["seed"] = *req.Seed
	}
	if req.TopK != nil {
		opts["top_k"] = *req.TopK
	}
	if req.MinP != nil {
		opts["min_p"] = *req.MinP
	}

	if len(opts) == 0 {
		return nil
	}
	return opts
}

// convertThinkingModeToOllama converts ThinkingMode to Ollama's think parameter.
// Returns nil to omit the parameter, or a pointer to bool to include it.
func convertThinkingModeToOllama(mode ThinkingMode, model string) *bool {
	switch mode {
	case ThinkingModeAuto:
		name := strings.ToLower(model)
		for _, marker := range []string{"qwen3", "deepseek-r1", "deepseek-v3", ":thinking"} {
			if strings.Contains(name, marker) {
				value := true
				return &value
			}
		}
		return nil
	case ThinkingModeEnabled:
		t := true
		return &t
	case ThinkingModeDisabled:
		f := false
		return &f
	default:
		// Omit: don't send the parameter.
		return nil
	}
}

// Only the schema body belongs in Ollama's format field, never its OpenAI wrapper.
func convertOllamaResponseFormat(format *ResponseFormat) (any, error) {
	if format == nil {
		return nil, nil
	}
	switch format.Type {
	case "text":
		return nil, nil
	case "json_object":
		return "json", nil
	case "json_schema":
		if format.JSONSchema != nil && format.JSONSchema.Schema != nil {
			return format.JSONSchema.Schema, nil
		}
	}
	return nil, NewInvalidRequestError(ollamaProviderName, "invalid Ollama response format", nil)
}

// convertFromOllamaResponse converts an Ollama response to ChatResponse.
func canonicalOllamaFinish(raw *string) *FinishReason {
	if raw == nil {
		return nil
	}
	reason := FinishReason(*raw)
	switch reason {
	case FinishReasonStop, FinishReasonLength, FinishReasonToolCalls, FinishReasonContentFilter, FinishReasonError:
		return &reason
	default:
		return nil
	}
}

func convertFromOllamaResponse(resp *ollamaChatResponse) *ChatResponse {
	cr := &ChatResponse{
		Content: resp.Message.Content,
		Model:   resp.Model,
	}

	// Set finish reason if done
	if resp.Done {
		cr.FinishReason = canonicalOllamaFinish(resp.DoneReason)
		cr.RawFinishReason = resp.DoneReason
	}

	// Set token usage if available
	if resp.EvalCount > 0 || resp.PromptEvalCount > 0 {
		cr.Usage = TokenUsage{
			Actual: &TokenCount{
				PromptTokens:     resp.PromptEvalCount,
				CompletionTokens: resp.EvalCount,
			},
			IsComplete: resp.Done,
		}
	}

	// Set thinking if present
	if resp.Message.Thinking != "" {
		cr.Thinking = &resp.Message.Thinking
	}

	// Populate InferenceMetadata
	cr.InferenceMetadata = buildInferenceMetadataFromResponse(cr)

	return cr
}

// convertFromOllamaModelInfo converts Ollama model info to LLMKit ModelInfo.
func convertFromOllamaModelInfo(info ollamaModelInfo) ModelInfo {
	return ModelInfo{
		Name:      info.Name,
		SizeBytes: &info.Size,
		Metadata: map[string]any{
			"digest":      info.Digest,
			"modified_at": info.ModifiedAt,
		},
	}
}
