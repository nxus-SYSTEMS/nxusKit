package testutil

import (
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"

	"github.com/jarcoal/httpmock"
)

// MockStreamingChunks creates SSE-formatted chunk strings for streaming tests.
// Each part becomes a separate SSE data event.
func MockStreamingChunks(parts []string) []string {
	chunks := make([]string, 0, len(parts)+1)
	for _, part := range parts {
		chunk := fmt.Sprintf(`{"id":"chatcmpl-test","object":"chat.completion.chunk","created":1234567890,"model":"gpt-4o","choices":[{"index":0,"delta":{"content":%q},"finish_reason":null}]}`, part)
		chunks = append(chunks, fmt.Sprintf("data: %s\n\n", chunk))
	}
	// Add the final [DONE] marker
	chunks = append(chunks, "data: [DONE]\n\n")
	return chunks
}

// MockClaudeStreamingChunks creates Claude-style SSE chunks for streaming tests.
func MockClaudeStreamingChunks(parts []string) []string {
	chunks := make([]string, 0, len(parts)+5)

	// Message start and content block start combined
	chunks = append(chunks,
		`event: message_start
data: {"type":"message_start","message":{"id":"msg_test","type":"message","role":"assistant","content":[],"model":"claude-sonnet-4-20250514","stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":10,"output_tokens":0}}}

`,
		`event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}

`)

	// Content delta events
	for _, part := range parts {
		chunks = append(chunks, fmt.Sprintf(`event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":%q}}

`, part))
	}

	// Content block stop, message delta, and message stop combined
	chunks = append(chunks,
		`event: content_block_stop
data: {"type":"content_block_stop","index":0}

`,
		`event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"output_tokens":5}}

`,
		`event: message_stop
data: {"type":"message_stop"}

`)

	return chunks
}

// MockOllamaStreamingChunks creates Ollama-style newline-delimited JSON chunks.
func MockOllamaStreamingChunks(parts []string) []string {
	chunks := make([]string, 0, len(parts)+1)
	for _, part := range parts {
		chunk := fmt.Sprintf(`{"model":"llama3:latest","created_at":"2024-01-15T10:00:00Z","message":{"role":"assistant","content":%q},"done":false}`, part)
		chunks = append(chunks, chunk+"\n")
	}
	// Final done message
	chunks = append(chunks, `{"model":"llama3:latest","created_at":"2024-01-15T10:00:00Z","message":{"role":"assistant","content":""},"done":true,"done_reason":"stop","total_duration":1000000000,"eval_count":5}`+"\n")
	return chunks
}

// SSEResponder creates an httpmock responder that streams SSE chunks.
func SSEResponder(chunks []string) httpmock.Responder {
	return func(_ *http.Request) (*http.Response, error) {
		pr, pw := io.Pipe()
		go func() {
			defer func() { _ = pw.Close() }()
			for _, chunk := range chunks {
				_, _ = pw.Write([]byte(chunk))
			}
		}()
		return &http.Response{
			StatusCode: 200,
			Body:       io.NopCloser(pr),
			Header: http.Header{
				"Content-Type": []string{"text/event-stream"},
			},
		}, nil
	}
}

// SSEResponderWithDelay creates an SSE responder with a delay between chunks.
func SSEResponderWithDelay(chunks []string, delay time.Duration) httpmock.Responder {
	return func(_ *http.Request) (*http.Response, error) {
		pr, pw := io.Pipe()
		go func() {
			defer func() { _ = pw.Close() }()
			for _, chunk := range chunks {
				_, _ = pw.Write([]byte(chunk))
				time.Sleep(delay)
			}
		}()
		return &http.Response{
			StatusCode: 200,
			Body:       io.NopCloser(pr),
			Header: http.Header{
				"Content-Type": []string{"text/event-stream"},
			},
		}, nil
	}
}

// NDJSONResponder creates an httpmock responder for newline-delimited JSON (Ollama-style).
func NDJSONResponder(chunks []string) httpmock.Responder {
	return func(_ *http.Request) (*http.Response, error) {
		body := strings.Join(chunks, "")
		return httpmock.NewStringResponse(200, body), nil
	}
}

// SetupOpenAIStreamingMock registers a streaming mock for OpenAI.
func SetupOpenAIStreamingMock(parts []string) {
	chunks := MockStreamingChunks(parts)
	httpmock.RegisterResponder("POST", "https://api.openai.com/v1/chat/completions",
		SSEResponder(chunks))
}

// SetupClaudeStreamingMock registers a streaming mock for Claude.
func SetupClaudeStreamingMock(parts []string) {
	chunks := MockClaudeStreamingChunks(parts)
	httpmock.RegisterResponder("POST", "https://api.anthropic.com/v1/messages",
		SSEResponder(chunks))
}

// SetupOllamaStreamingMock registers a streaming mock for Ollama.
func SetupOllamaStreamingMock(parts []string) {
	chunks := MockOllamaStreamingChunks(parts)
	httpmock.RegisterResponder("POST", "http://localhost:11434/api/chat",
		NDJSONResponder(chunks))
}

// ============================================================================
// OpenAI-Compatible Provider Streaming Mocks
// ============================================================================

// SetupFireworksStreamingMock registers a streaming mock for Fireworks.
func SetupFireworksStreamingMock(parts []string) {
	chunks := MockStreamingChunks(parts)
	httpmock.RegisterResponder("POST", FireworksEndpoint, SSEResponder(chunks))
}

// SetupGroqStreamingMock registers a streaming mock for Groq.
func SetupGroqStreamingMock(parts []string) {
	chunks := MockStreamingChunks(parts)
	httpmock.RegisterResponder("POST", GroqEndpoint, SSEResponder(chunks))
}

// SetupMistralStreamingMock registers a streaming mock for Mistral.
func SetupMistralStreamingMock(parts []string) {
	chunks := MockStreamingChunks(parts)
	httpmock.RegisterResponder("POST", MistralEndpoint, SSEResponder(chunks))
}

// SetupTogetherStreamingMock registers a streaming mock for Together.
func SetupTogetherStreamingMock(parts []string) {
	chunks := MockStreamingChunks(parts)
	httpmock.RegisterResponder("POST", TogetherEndpoint, SSEResponder(chunks))
}

// SetupPerplexityStreamingMock registers a streaming mock for Perplexity.
func SetupPerplexityStreamingMock(parts []string) {
	chunks := MockStreamingChunks(parts)
	httpmock.RegisterResponder("POST", PerplexityEndpoint, SSEResponder(chunks))
}

// SetupOpenRouterStreamingMock registers a streaming mock for OpenRouter.
func SetupOpenRouterStreamingMock(parts []string) {
	chunks := MockStreamingChunks(parts)
	httpmock.RegisterResponder("POST", OpenRouterEndpoint, SSEResponder(chunks))
}

// SetupLmStudioStreamingMock registers a streaming mock for LM Studio.
func SetupLmStudioStreamingMock(parts []string) {
	chunks := MockStreamingChunks(parts)
	httpmock.RegisterResponder("POST", LmStudioEndpoint, SSEResponder(chunks))
}

// SetupProviderStreamingMock registers a generic streaming mock for any endpoint.
func SetupProviderStreamingMock(endpoint string, parts []string) {
	chunks := MockStreamingChunks(parts)
	httpmock.RegisterResponder("POST", endpoint, SSEResponder(chunks))
}

// SetupProviderStreamingErrorMock registers a streaming mock that returns an error mid-stream.
func SetupProviderStreamingErrorMock(endpoint string, partsBeforeError []string, errorMessage string) {
	chunks := make([]string, 0, len(partsBeforeError)+1)
	for _, part := range partsBeforeError {
		chunk := fmt.Sprintf(`{"id":"chatcmpl-test","object":"chat.completion.chunk","created":1234567890,"model":"gpt-4o","choices":[{"index":0,"delta":{"content":%q},"finish_reason":null}]}`, part)
		chunks = append(chunks, fmt.Sprintf("data: %s\n\n", chunk))
	}
	// Add error chunk
	errorChunk := fmt.Sprintf(`{"error":{"message":%q,"type":"server_error"}}`, errorMessage)
	chunks = append(chunks, fmt.Sprintf("data: %s\n\n", errorChunk))
	httpmock.RegisterResponder("POST", endpoint, SSEResponder(chunks))
}
