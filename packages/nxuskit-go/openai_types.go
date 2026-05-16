package nxuskit

// openaiMessage represents a message in OpenAI's format.
type openaiMessage struct {
	Role    string      `json:"role"`
	Content interface{} `json:"content"` // string or []openaiContentPart
}

// openaiContentPart represents a content part in multimodal messages.
type openaiContentPart struct {
	Type     string          `json:"type"`
	Text     string          `json:"text,omitempty"`
	ImageURL *openaiImageURL `json:"image_url,omitempty"`
}

// openaiImageURL represents an image URL in OpenAI format.
type openaiImageURL struct {
	URL    string  `json:"url"`
	Detail *string `json:"detail,omitempty"`
}

// openaiChatRequest is the request body for /v1/chat/completions.
type openaiChatRequest struct {
	Model            string                `json:"model"`
	Messages         []openaiMessage       `json:"messages"`
	Stream           bool                  `json:"stream"`
	Temperature      *float64              `json:"temperature,omitempty"`
	MaxTokens        *int                  `json:"max_tokens,omitempty"`
	TopP             *float64              `json:"top_p,omitempty"`
	PresencePenalty  *float64              `json:"presence_penalty,omitempty"`
	FrequencyPenalty *float64              `json:"frequency_penalty,omitempty"`
	Stop             []string              `json:"stop,omitempty"`
	Seed             *int                  `json:"seed,omitempty"`
	StreamOptions    *streamOptions        `json:"stream_options,omitempty"`
	ResponseFormat   *openaiResponseFormat `json:"response_format,omitempty"`
	Tools            []openaiTool          `json:"tools,omitempty"`
	ToolChoice       any                   `json:"tool_choice,omitempty"` // string or object
}

// openaiResponseFormat specifies the output format for OpenAI API.
type openaiResponseFormat struct {
	Type       string            `json:"type"`
	JSONSchema *openaiJSONSchema `json:"json_schema,omitempty"`
}

// openaiJSONSchema defines a JSON schema for structured outputs.
type openaiJSONSchema struct {
	Name        string         `json:"name"`
	Description string         `json:"description,omitempty"`
	Schema      map[string]any `json:"schema"`
	Strict      bool           `json:"strict,omitempty"`
}

// openaiTool represents a tool/function that can be called by the model.
type openaiTool struct {
	Type     string             `json:"type"`
	Function openaiToolFunction `json:"function"`
}

// openaiToolFunction describes a function the model can call.
type openaiToolFunction struct {
	Name        string         `json:"name"`
	Description string         `json:"description,omitempty"`
	Parameters  map[string]any `json:"parameters,omitempty"`
}

// openaiToolChoice specifies a specific function to call.
type openaiToolChoice struct {
	Type     string                    `json:"type"`
	Function *openaiToolChoiceFunction `json:"function,omitempty"`
}

// openaiToolChoiceFunction specifies which function to call.
type openaiToolChoiceFunction struct {
	Name string `json:"name"`
}

// streamOptions controls streaming behavior.
type streamOptions struct {
	IncludeUsage bool `json:"include_usage,omitempty"`
}

// openaiChatResponse is the response from /v1/chat/completions (non-streaming).
type openaiChatResponse struct {
	ID      string         `json:"id"`
	Object  string         `json:"object"`
	Created int64          `json:"created"`
	Model   string         `json:"model"`
	Choices []openaiChoice `json:"choices"`
	Usage   *openaiUsage   `json:"usage,omitempty"`
}

// openaiChoice represents a completion choice.
type openaiChoice struct {
	Index        int                   `json:"index"`
	Message      *openaiMessage        `json:"message,omitempty"`
	Delta        *openaiDelta          `json:"delta,omitempty"`
	FinishReason *string               `json:"finish_reason,omitempty"`
	Logprobs     *openaiLogprobContent `json:"logprobs,omitempty"`
}

// openaiDelta represents incremental content in streaming responses.
type openaiDelta struct {
	Role    string `json:"role,omitempty"`
	Content string `json:"content,omitempty"`
}

// openaiTopLogprob represents an alternative token in OpenAI's logprob format.
type openaiTopLogprob struct {
	Token   string  `json:"token"`
	Logprob float64 `json:"logprob"`
	Bytes   []int   `json:"bytes,omitempty"`
}

// openaiTokenLogprob represents a single token's logprob data in OpenAI format.
type openaiTokenLogprob struct {
	Token       string             `json:"token"`
	Logprob     float64            `json:"logprob"`
	Bytes       []int              `json:"bytes,omitempty"`
	TopLogprobs []openaiTopLogprob `json:"top_logprobs,omitempty"`
}

// openaiLogprobContent is the logprobs.content array from an SSE chunk.
type openaiLogprobContent struct {
	Content []openaiTokenLogprob `json:"content,omitempty"`
}

// openaiUsage contains token usage statistics.
type openaiUsage struct {
	PromptTokens     int `json:"prompt_tokens"`
	CompletionTokens int `json:"completion_tokens"`
	TotalTokens      int `json:"total_tokens"`
}

// openaiModelsResponse is the response from /v1/models.
type openaiModelsResponse struct {
	Object string            `json:"object"`
	Data   []openaiModelInfo `json:"data"`
}

// openaiModelInfo contains information about an available model.
type openaiModelInfo struct {
	ID      string `json:"id"`
	Object  string `json:"object"`
	Created int64  `json:"created"`
	OwnedBy string `json:"owned_by"`
}

// openaiErrorResponse is the error response format from OpenAI-compatible APIs.
type openaiErrorResponse struct {
	Error openaiError `json:"error"`
}

// openaiError contains error details.
type openaiError struct {
	Message string  `json:"message"`
	Type    string  `json:"type"`
	Code    *string `json:"code,omitempty"`
}
