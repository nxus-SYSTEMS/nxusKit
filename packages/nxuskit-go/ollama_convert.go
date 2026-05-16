package nxuskit

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

	if len(opts) == 0 {
		return nil
	}
	return opts
}

// convertThinkingModeToOllama converts ThinkingMode to Ollama's think parameter.
// Returns nil to omit the parameter, or a pointer to bool to include it.
func convertThinkingModeToOllama(mode ThinkingMode) *bool {
	switch mode {
	case ThinkingModeEnabled:
		t := true
		return &t
	case ThinkingModeDisabled:
		f := false
		return &f
	default:
		// ThinkingModeAuto and ThinkingModeOmit: don't send the parameter
		return nil
	}
}

// convertFromOllamaResponse converts an Ollama response to ChatResponse.
func convertFromOllamaResponse(resp *ollamaChatResponse) *ChatResponse {
	cr := &ChatResponse{
		Content: resp.Message.Content,
		Model:   resp.Model,
	}

	// Set finish reason if done
	if resp.Done {
		reason := FinishReasonStop
		cr.FinishReason = &reason
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
	if resp.Thinking != "" {
		cr.Thinking = &resp.Thinking
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
