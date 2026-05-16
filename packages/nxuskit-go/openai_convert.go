package nxuskit

import "fmt"

// convertToOpenAIMessages converts LLMKit messages to OpenAI format.
func convertToOpenAIMessages(messages []Message) []openaiMessage {
	result := make([]openaiMessage, len(messages))
	for i, msg := range messages {
		result[i] = convertToOpenAIMessage(msg)
	}
	return result
}

// convertToOpenAIMessage converts a single LLMKit message to OpenAI format.
func convertToOpenAIMessage(msg Message) openaiMessage {
	om := openaiMessage{
		Role: msg.Role.String(),
	}

	if msg.Content.IsMultimodal() {
		// Convert to content array format
		parts := make([]openaiContentPart, 0, len(msg.Content.Parts))

		for _, part := range msg.Content.Parts {
			switch part.Type {
			case contentTypeText:
				parts = append(parts, openaiContentPart{
					Type: "text",
					Text: part.Text,
				})
			case contentTypeImage:
				if part.Image != nil {
					imgPart := openaiContentPart{
						Type: "image_url",
					}

					var url string
					if part.Image.Base64 != "" {
						// Format as data URL
						mediaType := part.Image.MediaType
						if mediaType == "" {
							mediaType = "image/png"
						}
						url = fmt.Sprintf("data:%s;base64,%s", mediaType, part.Image.Base64)
					} else {
						url = part.Image.URL
					}

					imgPart.ImageURL = &openaiImageURL{
						URL:    url,
						Detail: part.Image.Detail,
					}
					parts = append(parts, imgPart)
				}
			}
		}

		om.Content = parts
	} else {
		om.Content = msg.Content.Text
	}

	return om
}

// convertOpenAIFinishReason converts OpenAI finish reason to LLMKit FinishReason.
func convertOpenAIFinishReason(reason *string) *FinishReason {
	if reason == nil {
		return nil
	}

	fr := ParseFinishReason(*reason)
	return &fr
}

// convertFromOpenAIResponse converts an OpenAI response to ChatResponse.
func convertFromOpenAIResponse(resp *openaiChatResponse) *ChatResponse {
	cr := &ChatResponse{
		Model: resp.Model,
	}

	// Extract content from first choice
	if len(resp.Choices) > 0 {
		choice := resp.Choices[0]
		if choice.Message != nil {
			if content, ok := choice.Message.Content.(string); ok {
				cr.Content = content
			}
		}
		cr.FinishReason = convertOpenAIFinishReason(choice.FinishReason)
	}

	// Set token usage
	if resp.Usage != nil {
		cr.Usage = TokenUsage{
			Actual: &TokenCount{
				PromptTokens:     resp.Usage.PromptTokens,
				CompletionTokens: resp.Usage.CompletionTokens,
			},
			IsComplete: true,
		}
	}

	// Populate InferenceMetadata
	cr.InferenceMetadata = buildInferenceMetadataFromResponse(cr)

	return cr
}

// buildInferenceMetadataFromResponse creates InferenceMetadata from a ChatResponse.
// This helper is used across all providers to ensure consistent metadata population.
func buildInferenceMetadataFromResponse(resp *ChatResponse) InferenceMetadata {
	meta := NewInferenceMetadata()

	// Set completion status based on finish reason
	if resp.FinishReason != nil {
		if *resp.FinishReason == FinishReasonStop {
			meta = meta.Completed(*resp.FinishReason)
		} else {
			meta = meta.Incomplete(*resp.FinishReason)
		}
	}

	// Set token usage
	if resp.Usage.Actual != nil || resp.Usage.Estimated.PromptTokens > 0 {
		meta = meta.WithTokenUsage(resp.Usage)
	}

	// Set thinking trace
	if resp.Thinking != nil {
		meta = meta.WithThinkingTrace(*resp.Thinking)
		// Also add as an inference step
		meta = meta.AddInferenceStep(InferenceStepThinking(*resp.Thinking))
	}

	// Copy provider metadata
	if resp.Metadata != nil {
		meta = meta.WithProviderMetadata(resp.Metadata)
	}

	return meta
}

// convertFromOpenAIModelInfo converts OpenAI model info to LLMKit ModelInfo.
func convertFromOpenAIModelInfo(info openaiModelInfo) ModelInfo {
	return ModelInfo{
		Name: info.ID,
		Metadata: map[string]any{
			"owned_by": info.OwnedBy,
			"created":  info.Created,
		},
	}
}
