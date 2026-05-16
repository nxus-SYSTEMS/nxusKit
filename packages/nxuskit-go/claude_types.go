package nxuskit

// Anthropic/Claude API request/response types for the messages endpoint.

// claudeMessagesRequest represents the Anthropic API request format.
type claudeMessagesRequest struct {
	Model       string          `json:"model"`
	MaxTokens   int             `json:"max_tokens"`
	System      string          `json:"system,omitempty"`
	Messages    []claudeMessage `json:"messages"`
	Stream      bool            `json:"stream,omitempty"`
	Temperature *float64        `json:"temperature,omitempty"`
	TopP        *float64        `json:"top_p,omitempty"`
	StopSeq     []string        `json:"stop_sequences,omitempty"`
	Thinking    *claudeThinking `json:"thinking,omitempty"`
}

// claudeThinking controls extended thinking mode.
type claudeThinking struct {
	Type         string `json:"type"`                    // "enabled" or "disabled"
	BudgetTokens int    `json:"budget_tokens,omitempty"` // Max tokens for thinking
}

// claudeMessage represents a message in Anthropic format.
type claudeMessage struct {
	Role    string `json:"role"`    // "user" or "assistant"
	Content any    `json:"content"` // string or []claudeContentBlock
}

// claudeContentBlock for multimodal/thinking messages.
type claudeContentBlock struct {
	Type     string             `json:"type"` // "text", "image", "thinking"
	Text     string             `json:"text,omitempty"`
	Thinking string             `json:"thinking,omitempty"`
	Source   *claudeImageSource `json:"source,omitempty"`
}

// claudeImageSource for image content.
type claudeImageSource struct {
	Type      string `json:"type"` // "base64" or "url"
	MediaType string `json:"media_type,omitempty"`
	Data      string `json:"data,omitempty"`
	URL       string `json:"url,omitempty"`
}

// claudeMessagesResponse represents the Anthropic API response.
type claudeMessagesResponse struct {
	ID         string               `json:"id"`
	Type       string               `json:"type"`
	Role       string               `json:"role"`
	Content    []claudeContentBlock `json:"content"`
	Model      string               `json:"model"`
	StopReason string               `json:"stop_reason,omitempty"`
	Usage      claudeUsage          `json:"usage"`
}

// claudeUsage represents token usage information.
type claudeUsage struct {
	InputTokens  int `json:"input_tokens"`
	OutputTokens int `json:"output_tokens"`
}

// Streaming event types for Anthropic's SSE format.

// claudeStreamEvent is the base event type.
type claudeStreamEvent struct {
	Type string `json:"type"`
}

// claudeMessageStart is sent at the beginning of a message.
// Currently unused but kept for future streaming implementation.
type claudeMessageStart struct {
	Type    string                 `json:"type"`
	Message claudeMessagesResponse `json:"message"`
}

var _ = claudeMessageStart{} // silence unused warning

// claudeContentBlockStart signals the start of a content block.
type claudeContentBlockStart struct {
	Type         string             `json:"type"`
	Index        int                `json:"index"`
	ContentBlock claudeContentBlock `json:"content_block"`
}

// claudeContentBlockDelta contains incremental content.
type claudeContentBlockDelta struct {
	Type  string                     `json:"type"`
	Index int                        `json:"index"`
	Delta claudeContentBlockDeltaVal `json:"delta"`
}

// claudeContentBlockDeltaVal contains the delta value.
type claudeContentBlockDeltaVal struct {
	Type     string `json:"type"` // "text_delta" or "thinking_delta"
	Text     string `json:"text,omitempty"`
	Thinking string `json:"thinking,omitempty"`
}

// claudeContentBlockStop signals the end of a content block.
// Currently unused but kept for future streaming implementation.
type claudeContentBlockStop struct {
	Type  string `json:"type"`
	Index int    `json:"index"`
}

var _ = claudeContentBlockStop{} // silence unused warning

// claudeMessageDelta contains final message metadata.
type claudeMessageDelta struct {
	Type  string                `json:"type"`
	Delta claudeMessageDeltaVal `json:"delta"`
	Usage *claudeUsage          `json:"usage,omitempty"`
}

// claudeMessageDeltaVal contains the stop reason.
type claudeMessageDeltaVal struct {
	StopReason string `json:"stop_reason,omitempty"`
}

// claudeMessageStop signals the end of the message.
// Currently unused but kept for future streaming implementation.
type claudeMessageStop struct {
	Type string `json:"type"`
}

var _ = claudeMessageStop{} // silence unused warning

// claudeErrorResponse for error handling.
type claudeErrorResponse struct {
	Type  string      `json:"type"`
	Error claudeError `json:"error"`
}

// claudeError represents an error from the Anthropic API.
type claudeError struct {
	Type    string `json:"type"`
	Message string `json:"message"`
}
