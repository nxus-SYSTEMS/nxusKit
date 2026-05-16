package testutil

import (
	"net/http"

	"github.com/jarcoal/httpmock"
)

// MockChatResponse creates a standard successful chat completion response fixture.
// This returns a map that can be used with httpmock.NewJsonResponderOrPanic.
func MockChatResponse(content string) map[string]any {
	return map[string]any{
		"id":      "chatcmpl-test",
		"object":  "chat.completion",
		"created": 1234567890,
		"model":   "gpt-4o",
		"choices": []map[string]any{
			{
				"index": 0,
				"message": map[string]any{
					"role":    "assistant",
					"content": content,
				},
				"finish_reason": "stop",
			},
		},
		"usage": map[string]any{
			"prompt_tokens":     10,
			"completion_tokens": 5,
			"total_tokens":      15,
		},
	}
}

// MockClaudeResponse creates a standard Claude API response fixture.
func MockClaudeResponse(content string) map[string]any {
	return map[string]any{
		"id":   "msg_test",
		"type": "message",
		"role": "assistant",
		"content": []map[string]any{
			{
				"type": "text",
				"text": content,
			},
		},
		"model":         "claude-sonnet-4-20250514",
		"stop_reason":   "end_turn",
		"stop_sequence": nil,
		"usage": map[string]any{
			"input_tokens":  10,
			"output_tokens": 5,
		},
	}
}

// MockOllamaResponse creates a standard Ollama API response fixture.
func MockOllamaResponse(content string) map[string]any {
	return map[string]any{
		"model":      "llama3:latest",
		"created_at": "2024-01-15T10:00:00Z",
		"message": map[string]any{
			"role":    "assistant",
			"content": content,
		},
		"done":                 true,
		"done_reason":          "stop",
		"total_duration":       1000000000,
		"load_duration":        500000000,
		"prompt_eval_count":    10,
		"prompt_eval_duration": 100000000,
		"eval_count":           5,
		"eval_duration":        400000000,
	}
}

// MockErrorResponse creates an error response fixture with the given status code and message.
func MockErrorResponse(statusCode int, message string) httpmock.Responder {
	return httpmock.NewJsonResponderOrPanic(statusCode, map[string]any{
		"error": map[string]any{
			"message": message,
			"type":    "error",
			"code":    statusCode,
		},
	})
}

// MockRateLimitResponse creates a 429 rate limit response with Retry-After header.
func MockRateLimitResponse(retryAfterSeconds int) httpmock.Responder {
	return func(_ *http.Request) (*http.Response, error) {
		resp := httpmock.NewStringResponse(429, `{"error":{"message":"Rate limit exceeded","type":"rate_limit_error"}}`)
		resp.Header.Set("Retry-After", string(rune(retryAfterSeconds+'0')))
		resp.Header.Set("Content-Type", "application/json")
		return resp, nil
	}
}

// MockRateLimitResponseWithRetryAfter creates a 429 rate limit response with a specific Retry-After value.
func MockRateLimitResponseWithRetryAfter(retryAfter string) httpmock.Responder {
	return func(_ *http.Request) (*http.Response, error) {
		resp := httpmock.NewStringResponse(429, `{"error":{"message":"Rate limit exceeded","type":"rate_limit_error"}}`)
		resp.Header.Set("Retry-After", retryAfter)
		resp.Header.Set("Content-Type", "application/json")
		return resp, nil
	}
}

// SetupOpenAIMock registers a mock responder for OpenAI chat completions endpoint.
func SetupOpenAIMock(content string) {
	httpmock.RegisterResponder("POST", "https://api.openai.com/v1/chat/completions",
		httpmock.NewJsonResponderOrPanic(200, MockChatResponse(content)))
}

// SetupClaudeMock registers a mock responder for Claude messages endpoint.
func SetupClaudeMock(content string) {
	httpmock.RegisterResponder("POST", "https://api.anthropic.com/v1/messages",
		httpmock.NewJsonResponderOrPanic(200, MockClaudeResponse(content)))
}

// SetupOllamaMock registers a mock responder for Ollama chat endpoint.
func SetupOllamaMock(content string) {
	httpmock.RegisterResponder("POST", "http://localhost:11434/api/chat",
		httpmock.NewJsonResponderOrPanic(200, MockOllamaResponse(content)))
}

// ============================================================================
// OpenAI-Compatible Provider Mocks (Fireworks, Groq, Mistral, Together, etc.)
// ============================================================================

// Provider endpoint URLs
const (
	FireworksEndpoint  = "https://api.fireworks.ai/inference/v1/chat/completions"
	GroqEndpoint       = "https://api.groq.com/openai/v1/chat/completions"
	MistralEndpoint    = "https://api.mistral.ai/v1/chat/completions"
	TogetherEndpoint   = "https://api.together.xyz/v1/chat/completions"
	PerplexityEndpoint = "https://api.perplexity.ai/chat/completions"
	OpenRouterEndpoint = "https://openrouter.ai/api/v1/chat/completions"
	LmStudioEndpoint   = "http://localhost:1234/v1/chat/completions"
)

// SetupFireworksMock registers a mock responder for Fireworks chat endpoint.
func SetupFireworksMock(content string) {
	httpmock.RegisterResponder("POST", FireworksEndpoint,
		httpmock.NewJsonResponderOrPanic(200, MockChatResponse(content)))
}

// SetupGroqMock registers a mock responder for Groq chat endpoint.
func SetupGroqMock(content string) {
	httpmock.RegisterResponder("POST", GroqEndpoint,
		httpmock.NewJsonResponderOrPanic(200, MockChatResponse(content)))
}

// SetupMistralMock registers a mock responder for Mistral chat endpoint.
func SetupMistralMock(content string) {
	httpmock.RegisterResponder("POST", MistralEndpoint,
		httpmock.NewJsonResponderOrPanic(200, MockChatResponse(content)))
}

// SetupTogetherMock registers a mock responder for Together chat endpoint.
func SetupTogetherMock(content string) {
	httpmock.RegisterResponder("POST", TogetherEndpoint,
		httpmock.NewJsonResponderOrPanic(200, MockChatResponse(content)))
}

// SetupPerplexityMock registers a mock responder for Perplexity chat endpoint.
func SetupPerplexityMock(content string) {
	httpmock.RegisterResponder("POST", PerplexityEndpoint,
		httpmock.NewJsonResponderOrPanic(200, MockChatResponse(content)))
}

// SetupOpenRouterMock registers a mock responder for OpenRouter chat endpoint.
func SetupOpenRouterMock(content string) {
	httpmock.RegisterResponder("POST", OpenRouterEndpoint,
		httpmock.NewJsonResponderOrPanic(200, MockChatResponse(content)))
}

// SetupLmStudioMock registers a mock responder for LM Studio chat endpoint.
func SetupLmStudioMock(content string) {
	httpmock.RegisterResponder("POST", LmStudioEndpoint,
		httpmock.NewJsonResponderOrPanic(200, MockChatResponse(content)))
}

// SetupProviderErrorMock registers an error response for a given endpoint URL.
func SetupProviderErrorMock(endpoint string, statusCode int, message string) {
	httpmock.RegisterResponder("POST", endpoint, MockErrorResponse(statusCode, message))
}

// SetupProviderRateLimitMock registers a rate limit response for a given endpoint URL.
func SetupProviderRateLimitMock(endpoint string, retryAfterSeconds int) {
	httpmock.RegisterResponder("POST", endpoint, MockRateLimitResponse(retryAfterSeconds))
}
