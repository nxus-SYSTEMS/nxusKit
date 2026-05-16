package nxuskit

// ToolCall represents a tool call requested by the model in a ChatResponse.
// The model returns these when finish_reason is "tool_calls".
type ToolCall struct {
	ID       string       `json:"id"`
	Type     string       `json:"type"` // always "function"
	Function FunctionCall `json:"function"`
}

// FunctionCall contains the function invocation details from the model.
type FunctionCall struct {
	Name      string `json:"name"`
	Arguments string `json:"arguments"` // JSON-encoded arguments
}

// ToolResultMessage sends a tool execution result back to the model.
// Use this as a message in the continuation ChatRequest.
type ToolResultMessage struct {
	Role       string `json:"role"`         // always "tool"
	ToolCallID string `json:"tool_call_id"` // references ToolCall.ID
	Content    string `json:"content"`      // result content (typically JSON)
}

// NewToolResultMessage creates a tool result message.
func NewToolResultMessage(toolCallID, content string) ToolResultMessage {
	return ToolResultMessage{
		Role:       "tool",
		ToolCallID: toolCallID,
		Content:    content,
	}
}
