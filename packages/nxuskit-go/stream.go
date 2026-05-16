package nxuskit

// StreamChunk represents an incremental piece of a streaming response.
type StreamChunk struct {
	// Delta is the incremental text content.
	Delta string `json:"delta"`
	// Thinking contains incremental chain-of-thought reasoning (if enabled).
	Thinking *string `json:"thinking,omitempty"`
	// FinishReason is set on the final chunk to indicate why generation stopped.
	FinishReason *FinishReason `json:"finish_reason,omitempty"`
	// Usage contains token counts (typically only on the final chunk).
	Usage *TokenUsage `json:"usage,omitempty"`
	// ToolCalls contains incremental tool call data during streaming.
	// Each entry is a ToolCallDelta with partial function name/arguments.
	ToolCalls []ToolCallDelta `json:"tool_calls,omitempty"`
	// Logprobs carries per-token log probability data for this chunk.
	// Nil when the provider does not support streaming logprobs (FR-007).
	Logprobs *StreamLogprobsDelta `json:"logprobs,omitempty"`
}

// ToolCallDelta represents incremental tool call data in a streaming chunk.
// The client must concatenate argument fragments across deltas to form a complete ToolCall.
type ToolCallDelta struct {
	// Index of this tool call within the chunk's tool_calls array.
	Index int `json:"index"`
	// ID is the provider-generated unique ID (only in first delta for this index).
	ID string `json:"id,omitempty"`
	// Type is always "function" (only in first delta for this index).
	Type string `json:"type,omitempty"`
	// Function contains the incremental function call data.
	Function *FunctionCallDelta `json:"function,omitempty"`
}

// FunctionCallDelta represents incremental function call data within a ToolCallDelta.
type FunctionCallDelta struct {
	// Name is the function name fragment (typically complete in first delta).
	Name string `json:"name,omitempty"`
	// Arguments is the arguments fragment (concatenate across deltas for complete JSON).
	Arguments string `json:"arguments,omitempty"`
}

// IsFinal returns true if this is the final chunk (FinishReason is set).
func (sc StreamChunk) IsFinal() bool {
	return sc.FinishReason != nil
}

// HasThinking returns true if this chunk contains thinking content.
func (sc StreamChunk) HasThinking() bool {
	return sc.Thinking != nil && *sc.Thinking != ""
}

// HasContent returns true if this chunk contains delta content.
func (sc StreamChunk) HasContent() bool {
	return sc.Delta != ""
}

// HasToolCalls returns true if this chunk contains tool call deltas.
func (sc StreamChunk) HasToolCalls() bool {
	return len(sc.ToolCalls) > 0
}

// NewStreamChunk creates a new StreamChunk with the given delta content.
func NewStreamChunk(delta string) StreamChunk {
	return StreamChunk{Delta: delta}
}

// ThinkingChunk creates a new StreamChunk with thinking content.
func ThinkingChunk(thinking string) StreamChunk {
	return StreamChunk{Thinking: &thinking}
}

// FinalChunk creates a final StreamChunk with the given content, reason, and usage.
func FinalChunk(delta string, reason FinishReason, usage *TokenUsage) StreamChunk {
	return StreamChunk{
		Delta:        delta,
		FinishReason: &reason,
		Usage:        usage,
	}
}
