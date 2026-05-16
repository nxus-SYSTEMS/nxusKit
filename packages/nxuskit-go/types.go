package nxuskit

import (
	"encoding/json"
)

// Content part type constants.
const (
	contentTypeText  = "text"
	contentTypeImage = "image"
)

// Role represents the role of a message sender in a conversation.
type Role string

const (
	// RoleSystem represents system instructions that guide the model's behavior.
	RoleSystem Role = "system"
	// RoleUser represents messages from the user.
	RoleUser Role = "user"
	// RoleAssistant represents responses from the AI assistant.
	RoleAssistant Role = "assistant"
)

// String returns the string representation of the Role.
func (r Role) String() string {
	return string(r)
}

// FinishReason indicates why the model stopped generating tokens.
type FinishReason string

const (
	// FinishReasonStop indicates the model reached a natural stopping point.
	FinishReasonStop FinishReason = "stop"
	// FinishReasonLength indicates the model hit the max_tokens limit.
	FinishReasonLength FinishReason = "length"
	// FinishReasonContentFilter indicates content was filtered by safety systems.
	FinishReasonContentFilter FinishReason = "content_filter"
	// FinishReasonToolCalls indicates the model wants to call a tool/function.
	FinishReasonToolCalls FinishReason = "tool_calls"
	// FinishReasonError indicates an error occurred during generation.
	FinishReasonError FinishReason = "error"
)

// String returns the string representation of the FinishReason.
func (f FinishReason) String() string {
	return string(f)
}

// ParseFinishReason converts provider-specific finish reason strings to FinishReason.
// It handles common variations like "end_turn" (Claude) mapping to FinishReasonStop.
func ParseFinishReason(s string) FinishReason {
	switch s {
	case "stop", "end_turn", "end", "complete":
		return FinishReasonStop
	case "length", "max_tokens":
		return FinishReasonLength
	case "content_filter", "content_filtered":
		return FinishReasonContentFilter
	case "tool_calls", "function_call", "tool_use":
		return FinishReasonToolCalls
	case "error":
		return FinishReasonError
	default:
		// Return the raw string as FinishReason for unknown values
		return FinishReason(s)
	}
}

// ThinkingMode controls chain-of-thought reasoning for thinking-capable models.
type ThinkingMode int

const (
	// ThinkingModeAuto lets the provider decide whether to use thinking mode.
	ThinkingModeAuto ThinkingMode = iota
	// ThinkingModeEnabled forces thinking mode on for supported models.
	ThinkingModeEnabled
	// ThinkingModeDisabled forces thinking mode off.
	ThinkingModeDisabled
	// ThinkingModeOmit omits the thinking parameter entirely from requests.
	ThinkingModeOmit
)

// String returns the string representation of the ThinkingMode.
func (t ThinkingMode) String() string {
	switch t {
	case ThinkingModeAuto:
		return "auto"
	case ThinkingModeEnabled:
		return "enabled"
	case ThinkingModeDisabled:
		return "disabled"
	case ThinkingModeOmit:
		return "omit"
	default:
		return "unknown"
	}
}

// ImageSource represents an image for vision-capable models.
// Either URL or Base64 should be set, not both.
type ImageSource struct {
	// URL is a publicly accessible image URL.
	URL string `json:"url,omitempty"`
	// Base64 is the base64-encoded image data.
	Base64 string `json:"base64,omitempty"`
	// MediaType is the MIME type (e.g., "image/png", "image/jpeg").
	MediaType string `json:"media_type,omitempty"`
	// Detail is the detail level for image processing: "low", "high", or "auto".
	Detail *string `json:"detail,omitempty"`
}

// ContentPart represents a single part of multimodal message content.
type ContentPart struct {
	// Type is either "text" or "image".
	Type string `json:"type"`
	// Text contains the text content (for type="text").
	Text string `json:"text,omitempty"`
	// Image contains the image source (for type="image").
	Image *ImageSource `json:"image,omitempty"`
}

// MessageContent represents the content of a message.
// It can be simple text or structured multimodal content with text and images.
type MessageContent struct {
	// Text is the simple text content. Used when Parts is nil.
	Text string `json:"-"`
	// Parts contains multimodal content. When set, Text is ignored.
	Parts []ContentPart `json:"-"`
}

// MarshalJSON implements custom JSON marshaling for MessageContent.
// Simple text is marshaled as a string; multimodal content as an array of parts.
func (mc MessageContent) MarshalJSON() ([]byte, error) {
	if len(mc.Parts) > 0 {
		return json.Marshal(mc.Parts)
	}
	return json.Marshal(mc.Text)
}

// UnmarshalJSON implements custom JSON unmarshaling for MessageContent.
func (mc *MessageContent) UnmarshalJSON(data []byte) error {
	// Try to unmarshal as string first
	var text string
	if err := json.Unmarshal(data, &text); err == nil {
		mc.Text = text
		mc.Parts = nil
		return nil
	}

	// Try to unmarshal as array of parts
	var parts []ContentPart
	if err := json.Unmarshal(data, &parts); err == nil {
		mc.Parts = parts
		mc.Text = ""
		return nil
	}

	// Default to empty
	return nil
}

// IsMultimodal returns true if the content has multiple parts (text + images).
func (mc MessageContent) IsMultimodal() bool {
	return len(mc.Parts) > 0
}

// GetText returns the text content, extracting from parts if multimodal.
func (mc MessageContent) GetText() string {
	if !mc.IsMultimodal() {
		return mc.Text
	}
	// Extract text from parts
	for _, part := range mc.Parts {
		if part.Type == contentTypeText {
			return part.Text
		}
	}
	return ""
}

// Message represents a single turn in a conversation.
type Message struct {
	// Role is the role of the message sender (system, user, or assistant).
	Role Role `json:"role"`
	// Content is the message content (text or multimodal).
	Content MessageContent `json:"content"`
}

// SystemMessage creates a new system message with the given content.
func SystemMessage(content string) Message {
	return Message{
		Role:    RoleSystem,
		Content: MessageContent{Text: content},
	}
}

// UserMessage creates a new user message with the given content.
func UserMessage(content string) Message {
	return Message{
		Role:    RoleUser,
		Content: MessageContent{Text: content},
	}
}

// AssistantMessage creates a new assistant message with the given content.
func AssistantMessage(content string) Message {
	return Message{
		Role:    RoleAssistant,
		Content: MessageContent{Text: content},
	}
}

// WithImageURL adds an image by URL to the message, converting it to multimodal.
// Returns a new Message with the image added.
func (m Message) WithImageURL(url string) Message {
	return m.addImage(&ImageSource{URL: url})
}

// WithImageBase64 adds a base64-encoded image to the message.
// Returns a new Message with the image added.
func (m Message) WithImageBase64(data, mediaType string) Message {
	return m.addImage(&ImageSource{Base64: data, MediaType: mediaType})
}

// WithDetail sets the detail level on the last image in the message.
// Returns a new Message with the detail set.
func (m Message) WithDetail(detail string) Message {
	if len(m.Content.Parts) == 0 {
		return m
	}

	// Find the last image part and set its detail
	newParts := make([]ContentPart, len(m.Content.Parts))
	copy(newParts, m.Content.Parts)

	for i := len(newParts) - 1; i >= 0; i-- {
		if newParts[i].Type == contentTypeImage && newParts[i].Image != nil {
			newImg := *newParts[i].Image
			newImg.Detail = &detail
			newParts[i].Image = &newImg
			break
		}
	}

	return Message{
		Role:    m.Role,
		Content: MessageContent{Parts: newParts},
	}
}

// addImage is a helper to add an image to the message content.
func (m Message) addImage(img *ImageSource) Message {
	var parts []ContentPart

	if m.Content.IsMultimodal() {
		// Already multimodal, append to existing parts
		parts = make([]ContentPart, len(m.Content.Parts), len(m.Content.Parts)+1)
		copy(parts, m.Content.Parts)
	} else {
		// Convert simple text to multimodal
		parts = make([]ContentPart, 0, 2)
		if m.Content.Text != "" {
			parts = append(parts, ContentPart{Type: contentTypeText, Text: m.Content.Text})
		}
	}

	parts = append(parts, ContentPart{Type: contentTypeImage, Image: img})

	return Message{
		Role:    m.Role,
		Content: MessageContent{Parts: parts},
	}
}

// ChatRequest represents a request to generate a chat completion.
type ChatRequest struct {
	// Model is the model identifier (e.g., "gpt-4o", "claude-sonnet-4-20250514").
	Model string `json:"model"`
	// Messages is the conversation history.
	Messages []Message `json:"messages"`
	// Temperature controls randomness (0.0-2.0). Nil means use provider default.
	Temperature *float64 `json:"temperature,omitempty"`
	// MaxTokens is the maximum number of tokens to generate. Nil means no limit.
	MaxTokens *int `json:"max_tokens,omitempty"`
	// TopP is the nucleus sampling threshold (0.0-1.0). Nil means use provider default.
	TopP *float64 `json:"top_p,omitempty"`
	// Stream enables streaming responses.
	Stream bool `json:"stream,omitempty"`
	// Stop is a list of sequences where the model should stop generating.
	Stop []string `json:"stop,omitempty"`
	// PresencePenalty penalizes new tokens based on presence in text (-2.0 to 2.0).
	PresencePenalty *float64 `json:"presence_penalty,omitempty"`
	// FrequencyPenalty penalizes tokens based on frequency in text (-2.0 to 2.0).
	FrequencyPenalty *float64 `json:"frequency_penalty,omitempty"`
	// Seed for deterministic generation. Nil means random.
	Seed *int `json:"seed,omitempty"`
	// Logprobs enables returning log probabilities of output tokens.
	Logprobs *bool `json:"logprobs,omitempty"`
	// TopLogprobs specifies the number of most likely tokens to return at each position (1-20).
	// Only valid when Logprobs is true.
	TopLogprobs *int `json:"top_logprobs,omitempty"`
	// JSONMode requests the model to output valid JSON. When true, the model is
	// instructed to only output valid JSON. Note: You must also instruct the model
	// to produce JSON in your prompt.
	JSONMode bool `json:"json_mode,omitempty"`
	// ThinkingMode controls chain-of-thought reasoning.
	ThinkingMode ThinkingMode `json:"-"`

	// ResponseFormat specifies the desired output format.
	// Use ResponseFormatText(), ResponseFormatJSON(), or ResponseFormatJSONSchema().
	// Supported by: OpenAI, Ollama, Groq, Together, Mistral, Fireworks, OpenRouter
	ResponseFormat *ResponseFormat `json:"response_format,omitempty"`

	// Tools is a list of tools/functions the model can call.
	// Supported by: OpenAI, Claude, Ollama, Groq, Together, Mistral
	Tools []Tool `json:"tools,omitempty"`

	// ToolChoice controls how the model uses tools.
	// Use ToolChoiceAuto(), ToolChoiceNone(), ToolChoiceRequired(), or ToolChoiceFunc().
	// Only meaningful when Tools is non-empty.
	ToolChoice *ToolChoice `json:"tool_choice,omitempty"`

	// TopK limits sampling to top K tokens. Nil means use provider default.
	// Supported by: Claude, Ollama, Together
	TopK *int `json:"top_k,omitempty"`

	// MinP is the minimum probability threshold (0.0-1.0). Nil means disabled.
	// Alternative to TopP. Supported by: Ollama
	MinP *float64 `json:"min_p,omitempty"`

	// Metadata contains internal data for convenience functions.
	// This is not sent to providers and is used for passing config like timeout.
	Metadata map[string]any `json:"-"`
}

// ChatResponse represents a response from an LLM provider.
type ChatResponse struct {
	// Content is the generated text content.
	Content string `json:"content"`
	// Model is the model that generated the response.
	Model string `json:"model"`
	// Usage contains token consumption statistics.
	Usage TokenUsage `json:"usage"`
	// FinishReason indicates why generation stopped. Nil if unknown.
	FinishReason *FinishReason `json:"finish_reason,omitempty"`
	// Thinking contains chain-of-thought reasoning if enabled.
	Thinking *string `json:"thinking,omitempty"`
	// ToolCalls contains tool calls requested by the model.
	// Present when FinishReason is FinishReasonToolCalls.
	ToolCalls []ToolCall `json:"tool_calls,omitempty"`
	// InferenceMetadata provides structured metadata for the response.
	// This is the preferred way to access metadata in new code.
	InferenceMetadata InferenceMetadata `json:"inference_metadata"`
	// Warnings contains any warnings from the provider.
	Warnings []string `json:"warnings,omitempty"`

	// Metadata contains provider-specific unstructured data.
	//
	// Deprecated: This field is planned for removal in a future major version.
	// New code should use InferenceMetadata.ProviderMetadata instead.
	// During the transition period, providers populate both fields.
	Metadata map[string]any `json:"metadata,omitempty"`
}

// PenaltyRange defines the valid range for penalty parameters.
type PenaltyRange struct {
	Min float64
	Max float64
}

// ResponseFormat specifies the desired output format for the model response.
// Use the convenience constructors (ResponseFormatText, ResponseFormatJSON,
// ResponseFormatJSONSchema) rather than creating instances directly.
type ResponseFormat struct {
	// Type is the format type: "text", "json_object", or "json_schema"
	Type string `json:"type"`

	// JSONSchema is the schema definition for json_schema type.
	// Only used when Type is "json_schema".
	JSONSchema *JSONSchema `json:"json_schema,omitempty"`
}

// JSONSchema defines a JSON schema for structured outputs.
// Supported by OpenAI and Ollama.
type JSONSchema struct {
	// Name is the schema identifier (required by OpenAI)
	Name string `json:"name"`

	// Description explains what the schema represents (optional)
	Description string `json:"description,omitempty"`

	// Schema is the actual JSON Schema definition
	Schema map[string]any `json:"schema"`

	// Strict enables strict schema validation (OpenAI-specific)
	Strict bool `json:"strict,omitempty"`
}

// ResponseFormatText returns a ResponseFormat for plain text output (default).
func ResponseFormatText() *ResponseFormat {
	return &ResponseFormat{Type: "text"}
}

// ResponseFormatJSON returns a ResponseFormat for JSON object output.
func ResponseFormatJSON() *ResponseFormat {
	return &ResponseFormat{Type: "json_object"}
}

// ResponseFormatJSONSchema returns a ResponseFormat with JSON Schema validation.
func ResponseFormatJSONSchema(name string, schema map[string]any) *ResponseFormat {
	return &ResponseFormat{
		Type: "json_schema",
		JSONSchema: &JSONSchema{
			Name:   name,
			Schema: schema,
			Strict: true,
		},
	}
}

// Tool represents a callable function that the model can invoke.
// Tools enable the model to perform actions like searching, calculating,
// or calling external APIs.
type Tool struct {
	// Type is the tool type. Currently only "function" is supported.
	Type string `json:"type"`

	// Function describes the callable function.
	Function ToolFunction `json:"function"`
}

// ToolFunction describes a function the model can call.
type ToolFunction struct {
	// Name is the function identifier (required).
	// Must match ^[a-zA-Z0-9_-]{1,64}$ pattern.
	Name string `json:"name"`

	// Description explains what the function does (recommended).
	// Helps the model decide when to call this function.
	Description string `json:"description,omitempty"`

	// Parameters is the JSON Schema for function parameters.
	// Should be a valid JSON Schema object with "type": "object".
	Parameters map[string]any `json:"parameters,omitempty"`
}

// NewTool creates a new Tool with the given function definition.
func NewTool(name, description string, parameters map[string]any) Tool {
	return Tool{
		Type: "function",
		Function: ToolFunction{
			Name:        name,
			Description: description,
			Parameters:  parameters,
		},
	}
}

// ToolChoice controls how the model uses available tools.
type ToolChoice struct {
	// Type is "auto", "none", "required", or "function".
	// - "auto": Model decides whether to use tools (default)
	// - "none": Model will not use any tools
	// - "required": Model must use at least one tool
	// - "function": Model must use the specified function
	Type string `json:"type,omitempty"`

	// Function specifies which function to call (when Type is "function").
	Function *ToolChoiceFunction `json:"function,omitempty"`
}

// ToolChoiceFunction specifies a specific function to call.
type ToolChoiceFunction struct {
	Name string `json:"name"`
}

// ToolChoiceAuto returns a ToolChoice that lets the model decide.
func ToolChoiceAuto() *ToolChoice {
	return &ToolChoice{Type: "auto"}
}

// ToolChoiceNone returns a ToolChoice that prevents tool use.
func ToolChoiceNone() *ToolChoice {
	return &ToolChoice{Type: "none"}
}

// ToolChoiceRequired returns a ToolChoice that requires tool use.
func ToolChoiceRequired() *ToolChoice {
	return &ToolChoice{Type: "required"}
}

// ToolChoiceFunc returns a ToolChoice for a specific function.
func ToolChoiceFunc(name string) *ToolChoice {
	return &ToolChoice{
		Type:     "function",
		Function: &ToolChoiceFunction{Name: name},
	}
}

// TopLogprob represents an alternative token at a position with its log probability.
type TopLogprob struct {
	// Token is the alternative token string.
	Token string `json:"token"`
	// Logprob is the log probability of this alternative token.
	Logprob float64 `json:"logprob"`
	// Bytes is the UTF-8 byte representation of the token (may be nil).
	Bytes []int `json:"bytes,omitempty"`
}

// TokenLogprob represents the log probability data for a single generated token.
type TokenLogprob struct {
	// Token is the generated token string.
	Token string `json:"token"`
	// Logprob is the log probability of the selected token.
	Logprob float64 `json:"logprob"`
	// Bytes is the UTF-8 byte representation of the token (may be nil).
	Bytes []int `json:"bytes,omitempty"`
	// TopLogprobs lists the most likely alternative tokens at this position.
	TopLogprobs []TopLogprob `json:"top_logprobs,omitempty"`
}

// StreamLogprobsDelta carries per-chunk logprob data during streaming.
// It contains the token logprob entries for tokens emitted in a single stream chunk.
// Absent (nil pointer on StreamChunk) means the provider does not emit logprob data.
//
// Example:
//
//	chunks, errs := provider.ChatStream(ctx, req)
//	for chunk := range chunks {
//	    if chunk.Logprobs != nil {
//	        for _, tok := range chunk.Logprobs.Content {
//	            fmt.Printf("token=%q logprob=%.4f\n", tok.Token, tok.Logprob)
//	        }
//	    }
//	}
type StreamLogprobsDelta struct {
	// Content contains token logprob entries for the tokens in this chunk.
	// May be empty for a finish-reason-only chunk even when the provider supports logprobs.
	Content []TokenLogprob `json:"content"`
}

// ProviderCapabilities describes the features and parameter limits of an LLM provider.
//
// Use GetCapabilities() on any LLMProvider to retrieve this information.
// The adapter uses these capabilities to gracefully handle unsupported parameters.
type ProviderCapabilities struct {
	// SupportsSystemMessages indicates whether the provider accepts system role messages.
	SupportsSystemMessages bool

	// SupportsStreaming indicates whether the provider supports streaming responses.
	SupportsStreaming bool

	// SupportsVision indicates whether the provider supports image/vision inputs.
	SupportsVision bool

	// MaxStopSequences is the maximum number of stop sequences allowed.
	// Nil means unlimited.
	MaxStopSequences *int

	// SupportsPresencePenalty indicates support for presence_penalty parameter.
	SupportsPresencePenalty bool

	// SupportsFrequencyPenalty indicates support for frequency_penalty parameter.
	SupportsFrequencyPenalty bool

	// SupportsSeed indicates support for deterministic generation via seed parameter.
	SupportsSeed bool

	// SupportsLogprobs indicates support for log probability output.
	SupportsLogprobs bool

	// MaxLogprobs is the maximum value for top_logprobs (e.g., 20 for OpenAI).
	// Nil means logprobs not supported.
	MaxLogprobs *int

	// SupportsStreamingLogprobs indicates whether this provider emits per-chunk
	// logprob deltas during streaming. Requires SupportsLogprobs == true.
	// Default is false; OpenAI sets this to true.
	SupportsStreamingLogprobs bool

	// SupportsJSONMode indicates native JSON response format support.
	SupportsJSONMode bool

	// SupportsJSONSchema indicates JSON schema validation support.
	SupportsJSONSchema bool

	// SupportsTools indicates support for function/tool calling.
	SupportsTools bool

	// SupportsResponseFormat indicates support for response_format parameter.
	SupportsResponseFormat bool

	// SupportsTopK indicates support for top_k sampling parameter.
	SupportsTopK bool

	// SupportsMinP indicates support for min_p sampling parameter.
	SupportsMinP bool

	// PenaltyRange is the valid range for penalty parameters (min, max).
	// Nil means penalties not supported.
	PenaltyRange *PenaltyRange
}

// DefaultCapabilities returns conservative default capabilities.
func DefaultCapabilities() ProviderCapabilities {
	return ProviderCapabilities{
		SupportsSystemMessages: true,
		SupportsStreaming:      false,
		SupportsVision:         false,
	}
}

// -----------------------------------------------------------------------------
// Inference Metadata Types (FR-LKS-001 Parity)
// -----------------------------------------------------------------------------

// InferenceStep represents a discrete step in the inference process.
//
// This type tracks individual actions taken during inference, such as
// tool/function calls, thinking/reasoning steps, or rule firings.
// It provides a unified way to inspect the inference process across
// different providers.
//
// This type exists for API consistency with nxuskit-engine.
type InferenceStep struct {
	// StepType identifies the kind of step.
	// Common values: "tool_call", "thinking", "rule_firing", "fact_assertion"
	StepType string `json:"step_type"`

	// Identifier is the name of the tool, rule, or step.
	Identifier string `json:"identifier"`

	// Details contains optional step-specific information.
	// For tool calls: {"arguments": {...}, "result": ...}
	// For thinking: {"content": "..."}
	Details map[string]any `json:"details,omitempty"`
}

// NewInferenceStep creates a new inference step with the given type and identifier.
func NewInferenceStep(stepType, identifier string) InferenceStep {
	return InferenceStep{
		StepType:   stepType,
		Identifier: identifier,
	}
}

// InferenceStepToolCall creates an inference step for a tool/function call.
func InferenceStepToolCall(toolName string, arguments map[string]any) InferenceStep {
	return InferenceStep{
		StepType:   "tool_call",
		Identifier: toolName,
		Details:    map[string]any{"arguments": arguments},
	}
}

// InferenceStepThinking creates an inference step for thinking/reasoning content.
func InferenceStepThinking(content string) InferenceStep {
	return InferenceStep{
		StepType:   "thinking",
		Identifier: "thinking",
		Details:    map[string]any{"content": content},
	}
}

// WithDetails returns a copy of the step with the given details added.
func (s InferenceStep) WithDetails(details map[string]any) InferenceStep {
	s.Details = details
	return s
}

// InferenceMetadata provides unified metadata for inference results.
//
// This structure provides a common interface for accessing inference
// metadata regardless of the underlying provider. It consolidates
// information that was previously scattered or provider-specific.
//
// This type exists for API consistency with nxuskit-engine.
type InferenceMetadata struct {
	// ExecutionTimeMs is the execution duration in milliseconds.
	// Nil if not measured.
	ExecutionTimeMs *int64 `json:"execution_time_ms,omitempty"`

	// IsComplete indicates whether the response finished normally.
	// True if FinishReason is "stop" or equivalent.
	IsComplete bool `json:"is_complete"`

	// FinishReason explains why generation stopped.
	FinishReason *FinishReason `json:"finish_reason,omitempty"`

	// TokenUsage contains token consumption statistics.
	// Nil if the provider doesn't report token usage.
	TokenUsage *TokenUsage `json:"token_usage,omitempty"`

	// ThinkingTrace contains reasoning/thinking content if enabled.
	// This is separate from InferenceSteps for quick access.
	ThinkingTrace *string `json:"thinking_trace,omitempty"`

	// InferenceSteps contains discrete inference actions.
	// For LLM providers: tool calls, thinking blocks.
	// For rule engines: rule firings, fact assertions.
	InferenceSteps []InferenceStep `json:"inference_steps,omitempty"`

	// ProviderMetadata contains provider-specific data not covered above.
	// This replaces the unstructured Metadata field in ChatResponse.
	ProviderMetadata map[string]any `json:"provider_metadata,omitempty"`
}

// NewInferenceMetadata creates an empty InferenceMetadata.
func NewInferenceMetadata() InferenceMetadata {
	return InferenceMetadata{}
}

// Completed returns a copy with IsComplete set to true and the given finish reason.
func (m InferenceMetadata) Completed(reason FinishReason) InferenceMetadata {
	m.IsComplete = true
	m.FinishReason = &reason
	return m
}

// Incomplete returns a copy with IsComplete set to false and the given finish reason.
func (m InferenceMetadata) Incomplete(reason FinishReason) InferenceMetadata {
	m.IsComplete = false
	m.FinishReason = &reason
	return m
}

// WithExecutionTime returns a copy with the execution time set in milliseconds.
func (m InferenceMetadata) WithExecutionTime(ms int64) InferenceMetadata {
	m.ExecutionTimeMs = &ms
	return m
}

// WithTokenUsage returns a copy with the token usage statistics set.
func (m InferenceMetadata) WithTokenUsage(usage TokenUsage) InferenceMetadata {
	m.TokenUsage = &usage
	return m
}

// WithThinkingTrace returns a copy with the thinking/reasoning trace set.
func (m InferenceMetadata) WithThinkingTrace(trace string) InferenceMetadata {
	m.ThinkingTrace = &trace
	return m
}

// WithInferenceSteps returns a copy with the inference steps set.
func (m InferenceMetadata) WithInferenceSteps(steps []InferenceStep) InferenceMetadata {
	m.InferenceSteps = steps
	return m
}

// AddInferenceStep returns a copy with the given step appended.
func (m InferenceMetadata) AddInferenceStep(step InferenceStep) InferenceMetadata {
	m.InferenceSteps = append(m.InferenceSteps, step)
	return m
}

// WithProviderMetadata returns a copy with the provider-specific metadata set.
func (m InferenceMetadata) WithProviderMetadata(metadata map[string]any) InferenceMetadata {
	m.ProviderMetadata = metadata
	return m
}
