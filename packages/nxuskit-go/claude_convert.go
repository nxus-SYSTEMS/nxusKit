package nxuskit

import (
	"encoding/json"
	"fmt"
)

// Default max tokens for Claude if not specified (Anthropic requires this field).
const claudeDefaultMaxTokens = 4096

// Default thinking budget tokens when thinking mode is enabled.
const claudeDefaultThinkingBudget = 10000

// convertToClaudeMessages converts LLMKit messages to Anthropic format.
// Returns the converted messages and the system prompt (extracted from system messages).
func convertToClaudeMessages(messages []Message) ([]claudeMessage, string) {
	var systemPrompt string
	var result []claudeMessage

	for _, msg := range messages {
		if msg.Role == RoleSystem {
			// Concatenate system messages
			if systemPrompt != "" {
				systemPrompt += "\n"
			}
			systemPrompt += msg.Content.GetText()
			continue
		}

		cm := convertToClaudeMessage(msg)
		result = append(result, cm)
	}

	return result, systemPrompt
}

// convertToClaudeMessage converts a single LLMKit message to Anthropic format.
func convertToClaudeMessage(msg Message) claudeMessage {
	role := msg.Role.String()
	// Claude only accepts "user" or "assistant" roles
	if role == "system" {
		role = "user" // Fallback, though system should be extracted
	}

	cm := claudeMessage{
		Role: role,
	}

	if msg.Content.IsMultimodal() {
		// Convert to content block array format
		blocks := make([]claudeContentBlock, 0, len(msg.Content.Parts))

		for _, part := range msg.Content.Parts {
			switch part.Type {
			case contentTypeText:
				blocks = append(blocks, claudeContentBlock{
					Type: "text",
					Text: part.Text,
				})
			case contentTypeImage:
				if part.Image != nil {
					imgBlock := claudeContentBlock{
						Type: "image",
					}

					if part.Image.Base64 != "" {
						// Base64 image
						mediaType := part.Image.MediaType
						if mediaType == "" {
							mediaType = "image/png"
						}
						imgBlock.Source = &claudeImageSource{
							Type:      "base64",
							MediaType: mediaType,
							Data:      part.Image.Base64,
						}
					} else if part.Image.URL != "" {
						// URL image
						imgBlock.Source = &claudeImageSource{
							Type: "url",
							URL:  part.Image.URL,
						}
					}

					blocks = append(blocks, imgBlock)
				}
			}
		}

		cm.Content = blocks
	} else {
		cm.Content = msg.Content.Text
	}

	return cm
}

// convertThinkingModeToClaudeThinking converts ThinkingMode to Anthropic's thinking object.
// Returns nil if the mode should be omitted.
func convertThinkingModeToClaudeThinking(mode ThinkingMode) *claudeThinking {
	switch mode {
	case ThinkingModeEnabled:
		return &claudeThinking{
			Type:         "enabled",
			BudgetTokens: claudeDefaultThinkingBudget,
		}
	case ThinkingModeDisabled:
		return &claudeThinking{
			Type: "disabled",
		}
	case ThinkingModeAuto, ThinkingModeOmit:
		// Let the API decide or omit entirely
		return nil
	default:
		return nil
	}
}

// convertClaudeStopReason converts Anthropic stop_reason to FinishReason.
func convertClaudeStopReason(reason string) FinishReason {
	return ParseFinishReason(reason)
}

// convertFromClaudeResponse converts an Anthropic response to ChatResponse.
func convertFromClaudeResponse(resp *claudeMessagesResponse) *ChatResponse {
	cr := &ChatResponse{
		Model: resp.Model,
	}

	// Extract content and thinking from content blocks
	var content string
	var thinking string

	for _, block := range resp.Content {
		switch block.Type {
		case "text":
			if content != "" {
				content += "\n"
			}
			content += block.Text
		case "thinking":
			if thinking != "" {
				thinking += "\n"
			}
			thinking += block.Thinking
		}
	}

	cr.Content = content
	if thinking != "" {
		cr.Thinking = &thinking
	}

	// Set finish reason
	if resp.StopReason != "" {
		fr := convertClaudeStopReason(resp.StopReason)
		cr.FinishReason = &fr
	}

	// Set token usage
	cr.Usage = TokenUsage{
		Actual: &TokenCount{
			PromptTokens:     resp.Usage.InputTokens,
			CompletionTokens: resp.Usage.OutputTokens,
		},
		IsComplete: true,
	}

	// Populate InferenceMetadata
	cr.InferenceMetadata = buildInferenceMetadataFromResponse(cr)

	return cr
}

// buildClaudeRequest creates an Anthropic API request from a ChatRequest.
func buildClaudeRequest(req *ChatRequest) *claudeMessagesRequest {
	messages, system := convertToClaudeMessages(req.Messages)

	// Determine max_tokens (Claude requires this)
	maxTokens := claudeDefaultMaxTokens
	if req.MaxTokens != nil {
		maxTokens = *req.MaxTokens
	}

	cr := &claudeMessagesRequest{
		Model:     req.Model,
		MaxTokens: maxTokens,
		Messages:  messages,
		Stream:    req.Stream,
	}

	if system != "" {
		cr.System = system
	}

	if req.Temperature != nil {
		cr.Temperature = req.Temperature
	}

	if req.TopP != nil {
		cr.TopP = req.TopP
	}

	if len(req.Stop) > 0 {
		cr.StopSeq = req.Stop
	}

	// Handle thinking mode
	cr.Thinking = convertThinkingModeToClaudeThinking(req.ThinkingMode)

	return cr
}

// parseClaudeErrorResponse parses an Anthropic error response body.
func parseClaudeErrorResponse(statusCode int, body []byte, providerName string) error {
	var errResp claudeErrorResponse
	if err := parseClaudeError(body, &errResp); err != nil {
		return NewProviderError(providerName, fmt.Sprintf("HTTP %d", statusCode), statusCode, nil)
	}

	msg := errResp.Error.Message
	if msg == "" {
		msg = fmt.Sprintf("HTTP %d", statusCode)
	}

	switch statusCode {
	case 401:
		return NewAuthenticationError(providerName, msg, nil)
	case 400:
		return NewInvalidRequestError(providerName, msg, nil)
	case 429:
		// TODO: Parse retry-after header if available
		return NewRateLimitError(providerName, 0, nil)
	default:
		return NewProviderError(providerName, msg, statusCode, nil)
	}
}

// parseClaudeError is a helper to parse error response JSON.
func parseClaudeError(data []byte, errResp *claudeErrorResponse) error {
	return json.Unmarshal(data, errResp)
}
