package nxuskit

// ollamaMessage represents a message in Ollama's format.
type ollamaMessage struct {
	Role    string   `json:"role"`
	Content string   `json:"content"`
	Images  []string `json:"images,omitempty"` // base64-encoded images
}

// ollamaChatRequest is the request body for /api/chat.
type ollamaChatRequest struct {
	Model    string                 `json:"model"`
	Messages []ollamaMessage        `json:"messages"`
	Stream   bool                   `json:"stream"`
	Options  map[string]interface{} `json:"options,omitempty"`
	Think    *bool                  `json:"think,omitempty"`
}

// ollamaChatResponse is the response from /api/chat (streaming or non-streaming).
type ollamaChatResponse struct {
	Model           string        `json:"model"`
	Message         ollamaMessage `json:"message"`
	Done            bool          `json:"done"`
	Thinking        string        `json:"thinking,omitempty"`
	EvalCount       int           `json:"eval_count,omitempty"`
	PromptEvalCount int           `json:"prompt_eval_count,omitempty"`
}

// ollamaTagsResponse is the response from /api/tags.
type ollamaTagsResponse struct {
	Models []ollamaModelInfo `json:"models"`
}

// ollamaModelInfo contains information about an available Ollama model.
type ollamaModelInfo struct {
	Name       string `json:"name"`
	Size       int64  `json:"size"`
	Digest     string `json:"digest"`
	ModifiedAt string `json:"modified_at"`
}

// ollamaErrorResponse is the error response format from Ollama.
type ollamaErrorResponse struct {
	Error string `json:"error"`
}

// ollamaShowResponse is the response from /api/show for model details.
type ollamaShowResponse struct {
	Modelfile string             `json:"modelfile"`
	Details   ollamaModelDetails `json:"details"`
}

// ollamaModelDetails contains detailed information about a model.
type ollamaModelDetails struct {
	Family   string   `json:"family"`
	Families []string `json:"families"`
}
