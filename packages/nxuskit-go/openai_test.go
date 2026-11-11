package nxuskit

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"reflect"
	"strings"
	"sync/atomic"
	"testing"
	"time"
)

func TestCompatibleLogprobControls(t *testing.T) {
	for _, profile := range []string{"openai", "together", "openrouter"} {
		for _, streaming := range []bool{false, true} {
			for _, mode := range []string{"omitted", "disabled", "enabled"} {
				t.Run(fmt.Sprintf("%s/stream=%v/%s", profile, streaming, mode), func(t *testing.T) {
					var calls atomic.Int32
					server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
						calls.Add(1)
						var wire map[string]any
						if err := json.NewDecoder(r.Body).Decode(&wire); err != nil {
							t.Error(err)
							return
						}
						if mode == "omitted" {
							if _, ok := wire["logprobs"]; ok {
								t.Error("omitted logprobs was emitted")
							}
						} else if wire["logprobs"] != (mode == "enabled") {
							t.Errorf("logprobs=%v", wire["logprobs"])
						}
						if mode == "enabled" {
							if wire["top_logprobs"] != float64(5) {
								t.Errorf("top_logprobs=%v", wire["top_logprobs"])
							}
						} else if _, ok := wire["top_logprobs"]; ok {
							t.Error("omitted top_logprobs was emitted")
						}
						if streaming {
							fmt.Fprint(w, "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n")
						} else {
							fmt.Fprint(w, `{"model":"m","choices":[{"message":{"content":"ok"},"finish_reason":"stop"}]}`)
						}
					}))
					defer server.Close()
					var provider LLMProvider
					var err error
					switch profile {
					case "openai":
						provider, err = NewOpenAIProvider(WithOpenAIAPIKey("loopback"), WithOpenAIBaseURL(server.URL))
					case "together":
						provider, err = NewTogetherProvider(WithTogetherAPIKey("loopback"), WithTogetherBaseURL(server.URL))
					case "openrouter":
						provider, err = NewOpenRouterProvider(WithOpenRouterAPIKey("loopback"), WithOpenRouterBaseURL(server.URL))
					}
					if err != nil {
						t.Fatal(err)
					}
					request := &ChatRequest{Model: "m", Messages: []Message{UserMessage("hi")}}
					if mode != "omitted" {
						value := mode == "enabled"
						request.Logprobs = &value
					}
					if mode == "enabled" {
						value := 5
						request.TopLogprobs = &value
					}
					if streaming {
						chunks, errs := provider.ChatStream(context.Background(), request)
						for range chunks {
						}
						for err := range errs {
							t.Error(err)
						}
					} else if _, err := provider.Chat(context.Background(), request); err != nil {
						t.Fatal(err)
					}
					if calls.Load() != 1 {
						t.Fatalf("HTTP calls=%d", calls.Load())
					}
				})
			}
		}
	}
}

func TestLogprobControlsValidation(t *testing.T) {
	on, off, zero, tooMany, one := true, false, 0, 21, 1
	for _, request := range []*ChatRequest{{Logprobs: &on, TopLogprobs: &zero}, {Logprobs: &on, TopLogprobs: &tooMany}, {TopLogprobs: &one}, {Logprobs: &off, TopLogprobs: &one}} {
		var typed *LLMError
		err := validateGenerationControls(request, "openai")
		if !errors.As(err, &typed) || typed.Kind != ErrorKindInvalidRequest {
			t.Errorf("error=%v", err)
		}
	}
	for _, provider := range []string{"ollama", "claude", "lmstudio", "groq", "xai", "fireworks", "mistral", "perplexity"} {
		for _, value := range []*bool{&on, &off} {
			if err := validateGenerationControls(&ChatRequest{Logprobs: value}, provider); !errors.Is(err, ErrNotImplemented) {
				t.Errorf("%s error=%v", provider, err)
			}
		}
	}
}

func TestSelectedProviderMetadataTransport(t *testing.T) {
	for _, profile := range []string{"openrouter", "perplexity"} {
		for _, streaming := range []bool{false, true} {
			for _, overflow := range []bool{false, true} {
				t.Run(fmt.Sprintf("%s/stream=%t/overflow=%t", profile, streaming, overflow), func(t *testing.T) {
					selected := `"id":"r1","provider":"upstream","citations":["https://example.org"],"usage":{"prompt_tokens":1,"completion_tokens":1,"cost":0.25}`
					if overflow {
						selected = `"id":"` + strings.Repeat("x", 65536) + `","citations":["` + strings.Repeat("x", 65536) + `"]`
					}
					body := `{` + selected + `,"model":"actual","choices":[{"message":{"content":"ok"},"finish_reason":"stop"}]}`
					if streaming {
						body = "data: {\"choices\":[{\"delta\":{\"content\":\"text\"}}]}\n\ndata: {" + selected + ",\"choices\":[]}\n\ndata: [DONE]\n\n"
					}
					if streaming && overflow {
						first, second := "id", "provider"
						if profile == "perplexity" {
							first, second = "citations", "search_results"
						}
						body = "data: {\"choices\":[{\"delta\":{\"content\":\"text\"}}]}\n\ndata: {\"" + first + "\":\"" + strings.Repeat("x", 33000) + "\",\"choices\":[]}\n\ndata: {\"" + second + "\":\"" + strings.Repeat("x", 33000) + "\",\"choices\":[]}\n\ndata: [DONE]\n\n"
					}
					server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { _, _ = w.Write([]byte(body)) }))
					defer server.Close()
					var p LLMProvider
					if profile == "openrouter" {
						p, _ = NewOpenRouterProvider(WithOpenRouterAPIKey("loopback"), WithOpenRouterBaseURL(server.URL))
					} else {
						p, _ = NewPerplexityProvider(WithPerplexityAPIKey("loopback"), WithPerplexityBaseURL(server.URL))
					}
					var output any
					var failure error
					if streaming {
						chunks, errs := p.ChatStream(context.Background(), &ChatRequest{Model: "fixture", Messages: []Message{UserMessage("hi")}})
						finals := 0
						text := ""
						for c := range chunks {
							text += c.Delta
							if c.IsFinal() {
								finals++
								output = c
							} else {
								encoded, _ := json.Marshal(c)
								if strings.Contains(string(encoded), "provider_metadata") {
									t.Fatal("intermediate metadata exposed")
								}
							}
						}
						for err := range errs {
							failure = err
						}
						if text != "text" {
							t.Fatalf("text=%q", text)
						}
						if overflow && finals != 0 {
							t.Fatalf("success after metadata overflow: %d", finals)
						}
						if !overflow && finals != 1 {
							t.Fatalf("finals=%d", finals)
						}
					} else {
						output, failure = p.Chat(context.Background(), &ChatRequest{Model: "fixture", Messages: []Message{UserMessage("hi")}})
					}
					if overflow {
						if failure == nil {
							t.Fatal("metadata overflow accepted")
						}
						if !strings.Contains(failure.Error(), "provider_metadata") {
							t.Fatalf("wrong failure: %v", failure)
						}
						if e, ok := failure.(*LLMError); !ok || e.IsRetryable() {
							t.Fatalf("retryable/untyped: %v", failure)
						}
						return
					}
					if failure != nil {
						t.Fatal(failure)
					}
					encoded, _ := json.Marshal(output)
					var actual map[string]json.RawMessage
					_ = json.Unmarshal(encoded, &actual)
					expected := `{"id":"r1","provider":"upstream","usage":{"cost":0.25}}`
					if profile == "perplexity" {
						expected = `{"citations":["https://example.org"],"usage":{"cost":0.25}}`
					}
					var got, want any
					_ = json.Unmarshal(actual["provider_metadata"], &got)
					_ = json.Unmarshal([]byte(expected), &want)
					if !reflect.DeepEqual(got, want) {
						t.Fatalf("metadata=%s expected=%s", actual["provider_metadata"], expected)
					}
				})
			}
		}
	}
}

func TestSelectedProviderMetadataBoundsAndMerge(t *testing.T) {
	current, err := mergeProviderMetadata(nil, []byte(`{"id":"r1","usage":{"cost":0.25,"cost_details":{"x":null}}}`), "openrouter")
	if err != nil {
		t.Fatal(err)
	}
	current, err = mergeProviderMetadata(current, []byte(`{"provider":null,"usage":{"completion_tokens_details":{"reasoning_tokens":3}}}`), "openrouter")
	if err != nil {
		t.Fatal(err)
	}
	encoded, _ := json.Marshal(current)
	if string(encoded) != `{"id":"r1","provider":null,"usage":{"completion_tokens_details":{"reasoning_tokens":3},"cost":0.25,"cost_details":{"x":null}}}` {
		t.Fatalf("merged=%s", encoded)
	}
	current, err = mergeProviderMetadata(current, []byte(`{"usage":null}`), "openrouter")
	if err != nil {
		t.Fatal(err)
	}
	current, err = mergeProviderMetadata(current, []byte(`{"usage":{"cost":null}}`), "openrouter")
	if err != nil {
		t.Fatal(err)
	}
	encoded, _ = json.Marshal(current)
	if string(encoded) != `{"id":"r1","provider":null,"usage":{"cost":null}}` {
		t.Fatalf("null merge=%s", encoded)
	}
	for _, size := range []int{65535, 65536, 65537} {
		overhead := len(`{"citations":[""]}`)
		n := size - overhead
		data := []byte(`{"citations":["` + strings.Repeat("é", n/2) + strings.Repeat("x", n%2) + `"]}`)
		if len(data) != size {
			t.Fatal("fixture byte size")
		}
		_, err := mergeProviderMetadata(nil, data, "perplexity")
		if (err == nil) != (size <= 65536) {
			t.Fatalf("size=%d err=%v", size, err)
		}
	}
	for _, depth := range []int{16, 17} {
		data := []byte(`{"citations":` + strings.Repeat("[", depth-1) + `"leaf"` + strings.Repeat("]", depth-1) + `}`)
		_, err := mergeProviderMetadata(nil, data, "perplexity")
		if (err == nil) != (depth == 16) {
			t.Fatalf("depth=%d err=%v", depth, err)
		}
	}
	if _, err := mergeProviderMetadata(nil, []byte(`{"usage":42}`), "openrouter"); err == nil {
		t.Fatal("nonobject usage accepted")
	}
}

func TestSelectedMetadataDoesNotMaskUnaryError(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_, _ = w.Write([]byte(`{"id":"r1","error":{"message":"denied"},"choices":[{"message":{"content":"not success"}}]}`))
	}))
	defer server.Close()
	p, _ := NewOpenRouterProvider(WithOpenRouterAPIKey("loopback"), WithOpenRouterBaseURL(server.URL))
	result, err := p.Chat(context.Background(), &ChatRequest{Model: "fixture", Messages: []Message{UserMessage("hi")}})
	if err == nil || result != nil {
		t.Fatalf("error became success: %v %v", result, err)
	}
}

func TestOpenAIExplicitTerminalTruth(t *testing.T) {
	cases := []struct {
		name, body string
		wantError  bool
		raw        *string
		canonical  *FinishReason
	}{
		{"empty", "", true, nil, nil},
		{"premature", "data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n", true, nil, nil},
		{"malformed", "data: {broken}\n\n", true, nil, nil},
		{"provider_error_before_done", "data: {\"error\":{\"message\":\"denied\"}}\n\ndata: [DONE]\n\n", true, nil, nil},
		{"unknown", "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"vendor-new\"}]}\n\ndata: {\"choices\":[],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\ndata: [DONE]\n\ndata: {broken}\n\n", false, strPtr("vendor-new"), nil},
		{"absent", "data: [DONE]\n\n", false, nil, nil},
		{"known", "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"length\"}]}\n\ndata: [DONE]\n\n", false, strPtr("length"), finishReasonPtr(FinishReasonLength)},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { _, _ = w.Write([]byte(tc.body)) }))
			defer server.Close()
			provider, err := NewOpenAIProvider(WithOpenAIAPIKey("loopback-only"), WithOpenAIBaseURL(server.URL))
			if err != nil {
				t.Fatal(err)
			}
			chunks, errs := provider.ChatStream(context.Background(), &ChatRequest{Model: "gpt-4o", Messages: []Message{UserMessage("hi")}})
			var all []StreamChunk
			for chunk := range chunks {
				all = append(all, chunk)
			}
			var failures []error
			for err := range errs {
				failures = append(failures, err)
			}
			finals := 0
			for _, chunk := range all {
				if chunk.IsFinal() {
					finals++
				}
			}
			if tc.wantError {
				if len(failures) != 1 || finals != 0 {
					t.Fatalf("errors=%v finals=%d chunks=%+v", failures, finals, all)
				}
				return
			}
			if len(failures) != 0 || finals != 1 || len(all) == 0 {
				t.Fatalf("errors=%v finals=%d chunks=%+v", failures, finals, all)
			}
			last := all[len(all)-1]
			if !last.Terminal {
				t.Fatal("protocol completion must be explicit")
			}
			if (last.RawFinishReason == nil) != (tc.raw == nil) || (tc.raw != nil && *last.RawFinishReason != *tc.raw) {
				t.Fatalf("raw=%v want=%v", last.RawFinishReason, tc.raw)
			}
			if (last.FinishReason == nil) != (tc.canonical == nil) || (tc.canonical != nil && *last.FinishReason != *tc.canonical) {
				t.Fatalf("reason=%v want=%v", last.FinishReason, tc.canonical)
			}
			if tc.name == "unknown" && (last.Usage == nil || last.Usage.Actual == nil || last.Usage.Actual.PromptTokens != 7 || last.Usage.Actual.CompletionTokens != 3) {
				t.Fatalf("late usage lost: %+v", last)
			}
		})
	}
}

// TestOpenAIProvider_Constructor tests NewOpenAIProvider and options
func TestOpenAIProvider_Constructor(t *testing.T) {
	t.Run("requires API key", func(t *testing.T) {
		// Clear environment variable
		t.Setenv("OPENAI_API_KEY", "")

		_, err := NewOpenAIProvider()
		if err == nil {
			t.Error("expected error when no API key provided")
		}
	})

	t.Run("uses API key from environment", func(t *testing.T) {
		t.Setenv("OPENAI_API_KEY", "test-key-from-env")

		provider, err := NewOpenAIProvider()
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if provider == nil {
			t.Fatal("expected provider to be created")
		}
	})

	t.Run("option overrides environment", func(t *testing.T) {
		t.Setenv("OPENAI_API_KEY", "env-key")

		provider, err := NewOpenAIProvider(WithOpenAIAPIKey("option-key"))
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if provider == nil {
			t.Fatal("expected provider to be created")
		}
	})

	t.Run("custom base URL", func(t *testing.T) {
		t.Setenv("OPENAI_API_KEY", "test-key")

		provider, err := NewOpenAIProvider(WithOpenAIBaseURL("https://custom.api.com"))
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if provider == nil {
			t.Fatal("expected provider to be created")
		}
	})

	t.Run("rejects empty base URL", func(t *testing.T) {
		t.Setenv("OPENAI_API_KEY", "test-key")

		_, err := NewOpenAIProvider(WithOpenAIBaseURL(""))
		if err == nil {
			t.Error("expected error for empty base URL")
		}
	})

	t.Run("custom timeout", func(t *testing.T) {
		t.Setenv("OPENAI_API_KEY", "test-key")

		provider, err := NewOpenAIProvider(WithOpenAITimeout(5 * time.Minute))
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if provider == nil {
			t.Fatal("expected provider to be created")
		}
	})

	t.Run("rejects zero timeout", func(t *testing.T) {
		t.Setenv("OPENAI_API_KEY", "test-key")

		_, err := NewOpenAIProvider(WithOpenAITimeout(0))
		if err == nil {
			t.Error("expected error for zero timeout")
		}
	})

	t.Run("rejects negative timeout", func(t *testing.T) {
		t.Setenv("OPENAI_API_KEY", "test-key")

		_, err := NewOpenAIProvider(WithOpenAITimeout(-1 * time.Second))
		if err == nil {
			t.Error("expected error for negative timeout")
		}
	})

	t.Run("rejects empty API key option", func(t *testing.T) {
		t.Setenv("OPENAI_API_KEY", "")

		_, err := NewOpenAIProvider(WithOpenAIAPIKey(""))
		if err == nil {
			t.Error("expected error for empty API key")
		}
	})
}

func TestOpenAIProvider_ProviderName(t *testing.T) {
	t.Setenv("OPENAI_API_KEY", "test-key")

	provider, err := NewOpenAIProvider()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if provider.ProviderName() != "openai" {
		t.Errorf("expected 'openai', got '%s'", provider.ProviderName())
	}
}

func TestOpenAIProvider_Chat(t *testing.T) {
	t.Run("successful chat request", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			// Verify request headers
			if r.Header.Get("Authorization") != "Bearer test-key" {
				t.Errorf("expected Authorization header, got '%s'", r.Header.Get("Authorization"))
			}
			if r.Header.Get("Content-Type") != "application/json" {
				t.Errorf("expected Content-Type header, got '%s'", r.Header.Get("Content-Type"))
			}

			// Verify request path
			if r.URL.Path != "/chat/completions" {
				t.Errorf("expected path /chat/completions, got %s", r.URL.Path)
			}

			// Parse request body
			var req openaiChatRequest
			if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
				t.Fatalf("failed to decode request: %v", err)
			}

			if req.Model != "gpt-4o" {
				t.Errorf("expected model 'gpt-4o', got '%s'", req.Model)
			}

			// Send response
			resp := openaiChatResponse{
				ID:    "chatcmpl-123",
				Model: "gpt-4o",
				Choices: []openaiChoice{
					{
						Index: 0,
						Message: &openaiMessage{
							Role:    "assistant",
							Content: "Hello! How can I help you?",
						},
						FinishReason: strPtr("stop"),
					},
				},
				Usage: &openaiUsage{
					PromptTokens:     10,
					CompletionTokens: 8,
					TotalTokens:      18,
				},
			}
			_ = json.NewEncoder(w).Encode(resp)
		}))
		defer server.Close()

		provider, err := NewOpenAIProvider(
			WithOpenAIAPIKey("test-key"),
			WithOpenAIBaseURL(server.URL),
		)
		if err != nil {
			t.Fatalf("failed to create provider: %v", err)
		}

		req := &ChatRequest{
			Model: "gpt-4o",
			Messages: []Message{
				UserMessage("Hello"),
			},
		}

		resp, err := provider.Chat(context.Background(), req)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}

		if resp.Content != "Hello! How can I help you?" {
			t.Errorf("expected content, got '%s'", resp.Content)
		}
		if resp.Model != "gpt-4o" {
			t.Errorf("expected model 'gpt-4o', got '%s'", resp.Model)
		}
		if resp.FinishReason == nil || *resp.FinishReason != FinishReasonStop {
			t.Error("expected FinishReasonStop")
		}
		if resp.Usage.Actual == nil {
			t.Fatal("expected Usage.Actual")
		}
		if resp.Usage.Actual.PromptTokens != 10 {
			t.Errorf("expected 10 prompt tokens, got %d", resp.Usage.Actual.PromptTokens)
		}
		if resp.Usage.Actual.CompletionTokens != 8 {
			t.Errorf("expected 8 completion tokens, got %d", resp.Usage.Actual.CompletionTokens)
		}
	})

	t.Run("with optional parameters", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			var req openaiChatRequest
			_ = json.NewDecoder(r.Body).Decode(&req)

			if req.Temperature == nil || *req.Temperature != 0.7 {
				t.Error("expected temperature 0.7")
			}
			if req.MaxTokens == nil || *req.MaxTokens != 100 {
				t.Error("expected max_tokens 100")
			}
			if req.TopP == nil || *req.TopP != 0.9 {
				t.Error("expected top_p 0.9")
			}
			if len(req.Stop) != 1 || req.Stop[0] != "STOP" {
				t.Error("expected stop sequence")
			}

			resp := openaiChatResponse{
				Choices: []openaiChoice{{Message: &openaiMessage{Content: "OK"}}},
			}
			_ = json.NewEncoder(w).Encode(resp)
		}))
		defer server.Close()

		provider, _ := NewOpenAIProvider(
			WithOpenAIAPIKey("test-key"),
			WithOpenAIBaseURL(server.URL),
		)

		temp := 0.7
		maxTokens := 100
		topP := 0.9
		req := &ChatRequest{
			Model:       "gpt-4o",
			Messages:    []Message{UserMessage("Hi")},
			Temperature: &temp,
			MaxTokens:   &maxTokens,
			TopP:        &topP,
			Stop:        []string{"STOP"},
		}

		_, err := provider.Chat(context.Background(), req)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
	})
}

func TestOpenAIProvider_ErrorHandling(t *testing.T) {
	t.Run("authentication error", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.WriteHeader(http.StatusUnauthorized)
			_ = json.NewEncoder(w).Encode(openaiErrorResponse{
				Error: openaiError{
					Message: "Invalid API key",
					Type:    "invalid_request_error",
					Code:    strPtr("invalid_api_key"),
				},
			})
		}))
		defer server.Close()

		provider, _ := NewOpenAIProvider(
			WithOpenAIAPIKey("invalid-key"),
			WithOpenAIBaseURL(server.URL),
		)

		_, err := provider.Chat(context.Background(), &ChatRequest{
			Model:    "gpt-4o",
			Messages: []Message{UserMessage("Hi")},
		})

		if err == nil {
			t.Fatal("expected error")
		}
		llmErr, ok := err.(*LLMError)
		if !ok {
			t.Fatalf("expected *LLMError, got %T", err)
		}
		if llmErr.Kind != ErrorKindAuthentication {
			t.Errorf("expected ErrorKindAuthentication, got %v", llmErr.Kind)
		}
	})

	t.Run("rate limit error", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.Header().Set("Retry-After", "30")
			w.WriteHeader(http.StatusTooManyRequests)
			_ = json.NewEncoder(w).Encode(openaiErrorResponse{
				Error: openaiError{Message: "Rate limit exceeded"},
			})
		}))
		defer server.Close()

		provider, _ := NewOpenAIProvider(
			WithOpenAIAPIKey("test-key"),
			WithOpenAIBaseURL(server.URL),
		)

		_, err := provider.Chat(context.Background(), &ChatRequest{
			Model:    "gpt-4o",
			Messages: []Message{UserMessage("Hi")},
		})

		if err == nil {
			t.Fatal("expected error")
		}
		llmErr, ok := err.(*LLMError)
		if !ok {
			t.Fatalf("expected *LLMError, got %T", err)
		}
		if llmErr.Kind != ErrorKindRateLimit {
			t.Errorf("expected ErrorKindRateLimit, got %v", llmErr.Kind)
		}
		if llmErr.RetryAfter != 30*time.Second {
			t.Errorf("expected RetryAfter 30s, got %v", llmErr.RetryAfter)
		}
	})

	t.Run("invalid request error", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.WriteHeader(http.StatusBadRequest)
			_ = json.NewEncoder(w).Encode(openaiErrorResponse{
				Error: openaiError{Message: "Invalid model"},
			})
		}))
		defer server.Close()

		provider, _ := NewOpenAIProvider(
			WithOpenAIAPIKey("test-key"),
			WithOpenAIBaseURL(server.URL),
		)

		_, err := provider.Chat(context.Background(), &ChatRequest{
			Model:    "invalid-model",
			Messages: []Message{UserMessage("Hi")},
		})

		if err == nil {
			t.Fatal("expected error")
		}
		llmErr, ok := err.(*LLMError)
		if !ok {
			t.Fatalf("expected *LLMError, got %T", err)
		}
		if llmErr.Kind != ErrorKindInvalidRequest {
			t.Errorf("expected ErrorKindInvalidRequest, got %v", llmErr.Kind)
		}
	})
}

func TestOpenAIProvider_ListModels(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/models" {
			t.Errorf("expected path /models, got %s", r.URL.Path)
		}
		if r.Method != http.MethodGet {
			t.Errorf("expected GET, got %s", r.Method)
		}

		resp := openaiModelsResponse{
			Data: []openaiModelInfo{
				{ID: "gpt-4o", OwnedBy: "openai"},
				{ID: "gpt-4o-mini", OwnedBy: "openai"},
			},
		}
		_ = json.NewEncoder(w).Encode(resp)
	}))
	defer server.Close()

	provider, _ := NewOpenAIProvider(
		WithOpenAIAPIKey("test-key"),
		WithOpenAIBaseURL(server.URL),
	)

	models, err := provider.ListModels(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if len(models) != 2 {
		t.Fatalf("expected 2 models, got %d", len(models))
	}
	if models[0].Name != "gpt-4o" {
		t.Errorf("expected model Name 'gpt-4o', got '%s'", models[0].Name)
	}
}

func TestOpenAIProvider_Ping(t *testing.T) {
	t.Run("success", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			if r.URL.Path != "/models" {
				t.Errorf("expected path /models, got %s", r.URL.Path)
			}
			_ = json.NewEncoder(w).Encode(openaiModelsResponse{Data: []openaiModelInfo{}})
		}))
		defer server.Close()

		provider, _ := NewOpenAIProvider(
			WithOpenAIAPIKey("test-key"),
			WithOpenAIBaseURL(server.URL),
		)

		err := provider.Ping(context.Background())
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
	})

	t.Run("failure", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.WriteHeader(http.StatusUnauthorized)
		}))
		defer server.Close()

		provider, _ := NewOpenAIProvider(
			WithOpenAIAPIKey("bad-key"),
			WithOpenAIBaseURL(server.URL),
		)

		err := provider.Ping(context.Background())
		if err == nil {
			t.Error("expected error")
		}
	})
}

func TestOpenAIProvider_ChatStream(t *testing.T) {
	t.Run("successful stream", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			// Verify request
			var req openaiChatRequest
			_ = json.NewDecoder(r.Body).Decode(&req)
			if !req.Stream {
				t.Error("expected stream=true")
			}
			if req.StreamOptions == nil || !req.StreamOptions.IncludeUsage {
				t.Error("expected stream_options.include_usage=true")
			}

			w.Header().Set("Content-Type", "text/event-stream")
			flusher, _ := w.(http.Flusher)

			// Send chunks
			chunks := []string{
				`{"choices":[{"delta":{"content":"Hello"}}]}`,
				`{"choices":[{"delta":{"content":" there"}}]}`,
				`{"choices":[{"delta":{"content":"!"},"finish_reason":"stop"}],"usage":{"prompt_tokens":5,"completion_tokens":3}}`,
			}

			for _, chunk := range chunks {
				_, _ = w.Write([]byte("data: " + chunk + "\n\n"))
				flusher.Flush()
			}
			_, _ = w.Write([]byte("data: [DONE]\n\n"))
			flusher.Flush()
		}))
		defer server.Close()

		provider, _ := NewOpenAIProvider(
			WithOpenAIAPIKey("test-key"),
			WithOpenAIBaseURL(server.URL),
		)

		chunks, errs := provider.ChatStream(context.Background(), &ChatRequest{
			Model:    "gpt-4o",
			Messages: []Message{UserMessage("Hi")},
		})

		var content string
		var finalChunk StreamChunk
		for chunk := range chunks {
			content += chunk.Delta
			finalChunk = chunk
		}

		if err := <-errs; err != nil {
			t.Fatalf("unexpected error: %v", err)
		}

		if content != "Hello there!" {
			t.Errorf("expected 'Hello there!', got '%s'", content)
		}
		if finalChunk.FinishReason == nil || *finalChunk.FinishReason != FinishReasonStop {
			t.Error("expected final chunk with FinishReasonStop")
		}
		if finalChunk.Usage == nil {
			t.Error("expected usage in final chunk")
		}
	})

	t.Run("context cancellation", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.Header().Set("Content-Type", "text/event-stream")
			// Slow response that will be canceled
			time.Sleep(5 * time.Second)
		}))
		defer server.Close()

		provider, _ := NewOpenAIProvider(
			WithOpenAIAPIKey("test-key"),
			WithOpenAIBaseURL(server.URL),
		)

		ctx, cancel := context.WithTimeout(context.Background(), 100*time.Millisecond)
		defer cancel()

		chunks, errs := provider.ChatStream(ctx, &ChatRequest{
			Model:    "gpt-4o",
			Messages: []Message{UserMessage("Hi")},
		})

		// Drain chunks
		for range chunks {
		}

		err := <-errs
		if err == nil {
			t.Error("expected error on context cancellation")
		}
	})
}

func TestOpenAIProvider_Vision(t *testing.T) {
	t.Run("with base64 image", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			var req openaiChatRequest
			_ = json.NewDecoder(r.Body).Decode(&req)

			if len(req.Messages) != 1 {
				t.Fatalf("expected 1 message, got %d", len(req.Messages))
			}

			msg := req.Messages[0]
			content, ok := msg.Content.([]any)
			if !ok {
				t.Fatalf("expected []any content, got %T", msg.Content)
			}

			if len(content) != 2 {
				t.Fatalf("expected 2 content parts, got %d", len(content))
			}

			// Check text part
			textPart := content[0].(map[string]any)
			if textPart["type"] != "text" {
				t.Errorf("expected type 'text', got '%v'", textPart["type"])
			}

			// Check image part
			imgPart := content[1].(map[string]any)
			if imgPart["type"] != "image_url" {
				t.Errorf("expected type 'image_url', got '%v'", imgPart["type"])
			}

			resp := openaiChatResponse{
				Choices: []openaiChoice{{Message: &openaiMessage{Content: "I see an image"}}},
			}
			_ = json.NewEncoder(w).Encode(resp)
		}))
		defer server.Close()

		provider, _ := NewOpenAIProvider(
			WithOpenAIAPIKey("test-key"),
			WithOpenAIBaseURL(server.URL),
		)

		msg := UserMessage("What's in this image?").
			WithImageBase64("aGVsbG8=", "image/png")

		resp, err := provider.Chat(context.Background(), &ChatRequest{
			Model:    "gpt-4o",
			Messages: []Message{msg},
		})
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if resp.Content != "I see an image" {
			t.Errorf("unexpected content: %s", resp.Content)
		}
	})

	t.Run("with URL image", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			var req openaiChatRequest
			_ = json.NewDecoder(r.Body).Decode(&req)

			msg := req.Messages[0]
			content := msg.Content.([]any)
			imgPart := content[1].(map[string]any)
			imageURL := imgPart["image_url"].(map[string]any)

			if imageURL["url"] != "https://example.com/image.png" {
				t.Errorf("expected URL, got '%v'", imageURL["url"])
			}

			resp := openaiChatResponse{
				Choices: []openaiChoice{{Message: &openaiMessage{Content: "OK"}}},
			}
			_ = json.NewEncoder(w).Encode(resp)
		}))
		defer server.Close()

		provider, _ := NewOpenAIProvider(
			WithOpenAIAPIKey("test-key"),
			WithOpenAIBaseURL(server.URL),
		)

		msg := UserMessage("Describe").
			WithImageURL("https://example.com/image.png")

		_, err := provider.Chat(context.Background(), &ChatRequest{
			Model:    "gpt-4o",
			Messages: []Message{msg},
		})
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
	})
}

func TestOpenAIProvider_GetCapabilities(t *testing.T) {
	t.Setenv("OPENAI_API_KEY", "test-key")
	provider, err := NewOpenAIProvider()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	caps := provider.GetCapabilities()

	// OpenAI supports system messages
	if !caps.SupportsSystemMessages {
		t.Error("expected OpenAI to support system messages")
	}

	// OpenAI supports streaming
	if !caps.SupportsStreaming {
		t.Error("expected OpenAI to support streaming")
	}

	// OpenAI supports vision
	if !caps.SupportsVision {
		t.Error("expected OpenAI to support vision")
	}

	// OpenAI has max stop sequences of 4
	if caps.MaxStopSequences == nil || *caps.MaxStopSequences != 4 {
		t.Errorf("expected MaxStopSequences=4, got %v", caps.MaxStopSequences)
	}

	// OpenAI supports penalties
	if !caps.SupportsPresencePenalty {
		t.Error("expected OpenAI to support presence penalty")
	}
	if !caps.SupportsFrequencyPenalty {
		t.Error("expected OpenAI to support frequency penalty")
	}

	// OpenAI supports seed
	if !caps.SupportsSeed {
		t.Error("expected OpenAI to support seed")
	}

	// OpenAI supports logprobs with max 20
	if !caps.SupportsLogprobs {
		t.Error("expected OpenAI to support logprobs")
	}
	if caps.MaxLogprobs == nil || *caps.MaxLogprobs != 20 {
		t.Errorf("expected MaxLogprobs=20, got %v", caps.MaxLogprobs)
	}

	// OpenAI supports JSON mode and schema
	if !caps.SupportsJSONMode {
		t.Error("expected OpenAI to support JSON mode")
	}
	if !caps.SupportsJSONSchema {
		t.Error("expected OpenAI to support JSON schema")
	}

	// OpenAI penalty range is -2 to 2
	if caps.PenaltyRange == nil {
		t.Fatal("expected PenaltyRange to be set")
	}
	if caps.PenaltyRange.Min != -2.0 || caps.PenaltyRange.Max != 2.0 {
		t.Errorf("expected penalty range [-2, 2], got [%f, %f]", caps.PenaltyRange.Min, caps.PenaltyRange.Max)
	}
}

func TestOpenAIProvider_RateLimitWithRetryAfter(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Retry-After", "30")
		w.WriteHeader(http.StatusTooManyRequests)
		_, _ = w.Write([]byte(`{"error": {"message": "Rate limit exceeded", "type": "rate_limit_error"}}`))
	}))
	defer server.Close()

	provider, _ := NewOpenAIProvider(
		WithOpenAIAPIKey("test-key"),
		WithOpenAIBaseURL(server.URL),
	)

	_, err := provider.Chat(context.Background(), &ChatRequest{
		Model:    "gpt-4o",
		Messages: []Message{UserMessage("Hello")},
	})

	if err == nil {
		t.Fatal("expected error for rate limit response")
	}

	var llmErr *LLMError
	if !isLLMError(err, &llmErr) {
		t.Fatalf("expected LLMError, got %T", err)
	}

	if llmErr.Kind != ErrorKindRateLimit {
		t.Errorf("expected rate limit error, got %v", llmErr.Kind)
	}

	// Check that RetryAfter was parsed
	expectedRetry := 30 * time.Second
	if llmErr.RetryAfter != expectedRetry {
		t.Errorf("expected RetryAfter=%v, got %v", expectedRetry, llmErr.RetryAfter)
	}
}

// isLLMError is a helper to check if an error is an LLMError
func isLLMError(err error, target **LLMError) bool {
	if e, ok := err.(*LLMError); ok {
		*target = e
		return true
	}
	return false
}

func TestOpenAIProvider_StreamWithUsage(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/event-stream")
		flusher, _ := w.(http.Flusher)

		chunks := []string{
			`{"choices":[{"delta":{"content":"Hi"}}]}`,
			`{"choices":[{"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":5,"completion_tokens":1}}`,
		}

		for _, chunk := range chunks {
			_, _ = w.Write([]byte("data: " + chunk + "\n\n"))
			flusher.Flush()
		}
		_, _ = w.Write([]byte("data: [DONE]\n\n"))
		flusher.Flush()
	}))
	defer server.Close()

	provider, _ := NewOpenAIProvider(
		WithOpenAIAPIKey("test-key"),
		WithOpenAIBaseURL(server.URL),
	)

	chunks, usageChan := provider.StreamWithUsage(context.Background(), &ChatRequest{
		Model:    "gpt-4o",
		Messages: []Message{UserMessage("Hi")},
	})

	var content string
	for chunk := range chunks {
		content += chunk.Delta
	}

	usage := <-usageChan

	if content != "Hi" {
		t.Errorf("expected 'Hi', got '%s'", content)
	}

	// Usage should be populated
	_ = usage
}
