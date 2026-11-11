package nxuskit

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"math"
	"net/http"
	"net/http/httptest"
	"os"
	"reflect"
	"strings"
	"testing"
	"time"
)

func TestOllamaUTF8Truth(t *testing.T) {
	for _, streaming := range []bool{false, true} {
		for _, invalid := range []bool{false, true} {
			t.Run(fmt.Sprintf("stream=%v/invalid=%v", streaming, invalid), func(t *testing.T) {
				content := "€"
				if invalid {
					content = string([]byte{0xff})
				}
				server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
					fmt.Fprintf(w, "{\"model\":\"m\",\"message\":{\"content\":\"%s\"},\"done\":true}\n", content)
				}))
				defer server.Close()
				provider, err := NewOllamaProvider(WithOllamaBaseURL(server.URL))
				if err != nil {
					t.Fatal(err)
				}
				req := &ChatRequest{Model: "m"}
				if streaming {
					chunks, errs := provider.ChatStream(context.Background(), req)
					text, finals, failures := "", 0, 0
					for chunk := range chunks {
						text += chunk.Delta
						if chunk.IsFinal() {
							finals++
						}
					}
					for range errs {
						failures++
					}
					if invalid {
						if failures != 1 || finals != 0 || text != "" {
							t.Fatalf("failures=%d finals=%d text=%q", failures, finals, text)
						}
					} else if failures != 0 || finals != 1 || text != "€" {
						t.Fatalf("failures=%d finals=%d text=%q", failures, finals, text)
					}
				} else {
					response, err := provider.Chat(context.Background(), req)
					if invalid {
						if err == nil || response != nil {
							t.Fatalf("response=%v error=%v", response, err)
						}
					} else if err != nil || response.Content != "€" {
						t.Fatalf("response=%v error=%v", response, err)
					}
				}
			})
		}
	}
}

func TestOllamaErrorEnvelopeAndUnaryCompletion(t *testing.T) {
	for _, streaming := range []bool{false, true} {
		for _, failure := range []string{"null", `"private upstream detail"`, "false", "{}", "incomplete", "missing-done", "negative-prompt", "negative-completion", "overflow-total"} {
			t.Run(fmt.Sprintf("stream=%v/failure=%s", streaming, failure), func(t *testing.T) {
				body := `{"model":"m","message":{"content":"partial"},"done":true,"error":` + failure + `}`
				if failure == "incomplete" {
					body = `{"model":"m","message":{"content":"partial"},"done":false}`
				} else if failure == "missing-done" {
					body = `{"model":"m","message":{"content":"partial"}}`
				} else if failure == "negative-prompt" {
					body = `{"model":"m","message":{"content":"partial"},"done":true,"prompt_eval_count":-1,"eval_count":1}`
				} else if failure == "negative-completion" {
					body = `{"model":"m","message":{"content":"partial"},"done":true,"prompt_eval_count":1,"eval_count":-1}`
				} else if failure == "overflow-total" {
					body = `{"model":"m","message":{"content":"partial"},"done":true,"prompt_eval_count":9223372036854775807,"eval_count":1}`
				}
				server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
					fmt.Fprintln(w, body)
				}))
				defer server.Close()
				provider, err := NewOllamaProvider(WithOllamaBaseURL(server.URL))
				if err != nil {
					t.Fatal(err)
				}
				req := &ChatRequest{Model: "m", Messages: []Message{UserMessage("hi")}}
				if streaming {
					chunks, errs := provider.ChatStream(context.Background(), req)
					for chunk := range chunks {
						if chunk.IsFinal() {
							t.Fatal("error envelope produced successful terminal")
						}
					}
					count := 0
					for err := range errs {
						count++
						if strings.Contains(err.Error(), "private upstream detail") {
							t.Fatal("error leaked upstream payload")
						}
					}
					if count != 1 {
						t.Fatalf("expected one terminal error, got %d", count)
					}
				} else {
					response, err := provider.Chat(context.Background(), req)
					if err == nil || response != nil {
						t.Fatalf("response=%v error=%v", response, err)
					}
					if strings.Contains(err.Error(), "private upstream detail") {
						t.Fatal("error leaked upstream payload")
					}
				}
			})
		}
	}
}

func TestOllamaSamplingWireAndBounds(t *testing.T) {
	for _, streaming := range []bool{false, true} {
		for _, supplied := range []bool{false, true} {
			t.Run(fmt.Sprintf("stream=%v/supplied=%v", streaming, supplied), func(t *testing.T) {
				calls := 0
				server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
					calls++
					var body map[string]any
					if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
						t.Fatal(err)
					}
					options, _ := body["options"].(map[string]any)
					if supplied {
						if options["top_k"] != float64(40) || options["min_p"] != 0.1 {
							t.Errorf("options=%#v", options)
						}
						if !reflect.DeepEqual(options["stop"], []any{"END", "HALT"}) {
							t.Errorf("stop=%#v", options["stop"])
						}
					} else if _, ok := body["options"]; ok {
						t.Errorf("omitted controls emitted options: %#v", options)
					}
					fmt.Fprintln(w, `{"model":"m","message":{"content":"ok"},"done":true}`)
				}))
				defer server.Close()
				provider, err := NewOllamaProvider(WithOllamaBaseURL(server.URL))
				if err != nil {
					t.Fatal(err)
				}
				req := &ChatRequest{Model: "m", Messages: []Message{UserMessage("hi")}}
				if supplied {
					k, p := 40, 0.1
					req.TopK, req.MinP = &k, &p
					req.Stop = []string{"END", "HALT"}
				}
				if streaming {
					chunks, errs := provider.ChatStream(context.Background(), req)
					count := 0
					for range chunks {
						count++
					}
					for err := range errs {
						t.Error(err)
					}
					if count != 1 {
						t.Fatalf("chunks=%d", count)
					}
				} else if _, err := provider.Chat(context.Background(), req); err != nil {
					t.Fatal(err)
				}
				if calls != 1 {
					t.Fatalf("calls=%d", calls)
				}
			})
		}
	}
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		t.Error("invalid sampling reached HTTP")
		fmt.Fprintln(w, `{"done":true}`)
	}))
	defer server.Close()
	provider, err := NewOllamaProvider(WithOllamaBaseURL(server.URL))
	if err != nil {
		t.Fatal(err)
	}
	requests := []*ChatRequest{}
	for _, k := range []int{-1, 0, 2147483648} {
		value := k
		requests = append(requests, &ChatRequest{Model: "m", TopK: &value})
	}
	for _, p := range []float64{-0.1, 1.1, math.NaN(), math.Inf(1), math.Inf(-1)} {
		value := p
		requests = append(requests, &ChatRequest{Model: "m", MinP: &value})
	}
	requests = append(requests, &ChatRequest{Model: "m", Stop: []string{""}}, &ChatRequest{Model: "m", Stop: strings.Split(strings.Repeat("x,", 16)+"x", ",")})
	for _, req := range requests {
		_, err := provider.Chat(context.Background(), req)
		var typed *LLMError
		if !errors.As(err, &typed) || typed.Kind != ErrorKindInvalidRequest {
			t.Errorf("unary error=%v", err)
		}
		chunks, errs := provider.ChatStream(context.Background(), req)
		for range chunks {
			t.Error("invalid request emitted chunk")
		}
		count := 0
		for err := range errs {
			count++
			if !errors.As(err, &typed) || typed.Kind != ErrorKindInvalidRequest {
				t.Errorf("stream error=%v", err)
			}
		}
		if count != 1 {
			t.Errorf("errors=%d", count)
		}
	}
}

func TestOllamaCancelledUndrainedStreamClosesOwner(t *testing.T) {
	wrote := make(chan struct{})
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		fmt.Fprintln(w, `{"model":"m","message":{"content":"partial"},"done":false}`)
		w.(http.Flusher).Flush()
		close(wrote)
		<-r.Context().Done()
	}))
	defer server.Close()
	provider, err := NewOllamaProvider(WithOllamaBaseURL(server.URL))
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	chunks, errs := provider.ChatStream(ctx, &ChatRequest{Model: "m", Messages: []Message{UserMessage("hi")}})
	<-wrote
	// No consumer is draining chunks: cancellation must release the blocked send.
	time.Sleep(30 * time.Millisecond)
	cancel()
	select {
	case err, ok := <-errs:
		if !ok || !errors.Is(err, context.Canceled) {
			t.Errorf("cancellation error=%v open=%v", err, ok)
		}
	case <-time.After(time.Second):
		t.Error("stream owner remained blocked on an undrained chunk after cancellation")
	}
	// Drain only after the assertion so cleanup cannot mask the blocked-send RED.
	for range chunks {
	}
	for range errs {
	}
}

func TestOllamaLiteralFormatThinkingWire(t *testing.T) {
	for _, tc := range []struct {
		name   string
		format *ResponseFormat
		want   any
	}{
		{"json", ResponseFormatJSON(), "json"},
		{"schema", ResponseFormatJSONSchema("fact", map[string]any{"type": "object", "required": []string{"x"}, "properties": map[string]any{"x": map[string]any{"type": "integer"}}}), map[string]any{"type": "object", "required": []any{"x"}, "properties": map[string]any{"x": map[string]any{"type": "integer"}}}},
		{"text", ResponseFormatText(), nil},
		{"omitted", nil, nil},
	} {
		for _, streaming := range []bool{false, true} {
			t.Run(fmt.Sprintf("%s/stream=%v", tc.name, streaming), func(t *testing.T) {
				server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
					var body map[string]any
					if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
						t.Error(err)
						return
					}
					want := map[string]any{"model": "qwen3:8b", "messages": []any{map[string]any{"role": "user", "content": "Hi"}}, "stream": streaming, "think": true}
					if tc.want != nil {
						want["format"] = tc.want
					}
					if !reflect.DeepEqual(body, want) {
						t.Errorf("wire=%#v want=%#v", body, want)
					}
					fmt.Fprintln(w, `{"model":"qwen3:8b","message":{"content":"ok","thinking":"literal reason"},"done":true}`)
				}))
				defer server.Close()
				provider, err := NewOllamaProvider(WithOllamaBaseURL(server.URL))
				if err != nil {
					t.Fatal(err)
				}
				req := &ChatRequest{Model: "qwen3:8b", Messages: []Message{UserMessage("Hi")}, ResponseFormat: tc.format, ThinkingMode: ThinkingModeAuto}
				if !streaming {
					resp, err := provider.Chat(context.Background(), req)
					if err != nil {
						t.Fatal(err)
					}
					if resp.Thinking == nil || *resp.Thinking != "literal reason" {
						t.Fatalf("thinking=%v", resp.Thinking)
					}
				} else {
					chunks, errs := provider.ChatStream(context.Background(), req)
					count := 0
					for chunk := range chunks {
						count++
						if chunk.Thinking == nil || *chunk.Thinking != "literal reason" {
							t.Errorf("thinking=%v", chunk.Thinking)
						}
					}
					for err := range errs {
						if err != nil {
							t.Error(err)
						}
					}
					if count != 1 {
						t.Errorf("chunks=%d", count)
					}
				}
			})
		}
	}
}

func TestOllamaInvalidFormatRejectsBeforeHTTP(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		t.Error("invalid format reached HTTP")
		fmt.Fprintln(w, `{"model":"local","message":{"content":""},"done":true}`)
	}))
	defer server.Close()
	provider, err := NewOllamaProvider(WithOllamaBaseURL(server.URL))
	if err != nil {
		t.Fatal(err)
	}
	for _, format := range []*ResponseFormat{{Type: "invalid"}, {Type: "json_schema"}, {Type: "json_schema", JSONSchema: &JSONSchema{}}} {
		req := &ChatRequest{Model: "local", ResponseFormat: format}
		_, err := provider.Chat(context.Background(), req)
		var typed *LLMError
		if !errors.As(err, &typed) || typed.Kind != ErrorKindInvalidRequest {
			t.Fatalf("unary error=%v", err)
		}
		chunks, errs := provider.ChatStream(context.Background(), req)
		for range chunks {
			t.Error("invalid format emitted chunk")
		}
		count := 0
		for err := range errs {
			count++
			if !errors.As(err, &typed) || typed.Kind != ErrorKindInvalidRequest {
				t.Errorf("stream error=%v", err)
			}
		}
		if count != 1 {
			t.Errorf("stream errors=%d", count)
		}
	}
}

func TestOllamaStreamTerminalControl(t *testing.T) {
	for _, tc := range []struct {
		name, body, content    string
		brokenTransport        bool
		chunks, finals, errors int
		usage                  bool
	}{
		{name: "empty EOF", errors: 1},
		{name: "premature EOF", body: "{\"message\":{\"content\":\"partial\"},\"done\":false}\n", content: "partial", chunks: 1, errors: 1},
		{name: "malformed", body: "{broken}\n", errors: 1},
		{name: "truncated record", body: "{\"done\":", errors: 1},
		{name: "transport failure", body: "{\"done\":false}\n", brokenTransport: true, chunks: 1, errors: 1},
		{name: "done ignores later content", body: "{\"message\":{\"content\":\"end\"},\"done\":true}\n{\"message\":{\"content\":\"forbidden\"},\"done\":false}\n", content: "end", chunks: 1, finals: 1},
		{name: "done ignores later malformed", body: "{\"done\":true}\n{broken}\n", chunks: 1, finals: 1},
		{name: "nonnewline done and usage", body: "{\"message\":{\"content\":\"tail\"},\"done\":true,\"prompt_eval_count\":7,\"eval_count\":3}", content: "tail", chunks: 1, finals: 1, usage: true},
		{name: "terminal only usage", body: "{\"done\":true,\"prompt_eval_count\":7,\"eval_count\":3}\n", chunks: 1, finals: 1, usage: true},
	} {
		t.Run(tc.name, func(t *testing.T) {
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				if r.Method != "POST" || r.URL.Path != "/api/chat" {
					t.Errorf("unexpected request: %s %s", r.Method, r.URL.Path)
				}
				if tc.brokenTransport {
					w.Header().Set("Content-Length", fmt.Sprint(len(tc.body)+1))
				}
				fmt.Fprint(w, tc.body)
			}))
			defer server.Close()
			provider, err := NewOllamaProvider(WithOllamaBaseURL(server.URL))
			if err != nil {
				t.Fatal(err)
			}
			ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
			defer cancel()
			chunks, errs := provider.ChatStream(ctx, &ChatRequest{Model: "local"})
			var content strings.Builder
			count, finals := 0, 0
			for chunk := range chunks {
				count++
				content.WriteString(chunk.Delta)
				if chunk.IsFinal() {
					finals++
				}
				if tc.usage {
					if chunk.Usage == nil || chunk.Usage.Actual == nil {
						t.Fatal("missing terminal usage")
					}
					if actual := chunk.Usage.Actual; actual.PromptTokens != 7 || actual.CompletionTokens != 3 {
						t.Errorf("usage=%+v", actual)
					}
				}
			}
			errorCount := 0
			for err := range errs {
				errorCount++
				var typed *LLMError
				if !errors.As(err, &typed) || typed.Kind != ErrorKindStream {
					t.Errorf("expected stream error, got %v", err)
				}
			}
			if count != tc.chunks || finals != tc.finals || errorCount != tc.errors || content.String() != tc.content {
				t.Fatalf("got chunks=%d finals=%d errors=%d content=%q; want %d/%d/%d/%q", count, finals, errorCount, content.String(), tc.chunks, tc.finals, tc.errors, tc.content)
			}
		})
	}
}

func TestNewOllamaProvider_Default(t *testing.T) {
	provider, err := NewOllamaProvider()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if provider.ProviderName() != "ollama" {
		t.Errorf("expected provider name 'ollama', got '%s'", provider.ProviderName())
	}
}

func TestNewOllamaProvider_WithBaseURL(t *testing.T) {
	provider, err := NewOllamaProvider(
		WithOllamaBaseURL("http://custom:8080"),
	)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if provider == nil {
		t.Fatal("expected provider, got nil")
	}
}

func TestNewOllamaProvider_WithTimeout(t *testing.T) {
	provider, err := NewOllamaProvider(
		WithOllamaTimeout(5 * time.Minute),
	)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if provider == nil {
		t.Fatal("expected provider, got nil")
	}
}

func TestNewOllamaProvider_InvalidOptions(t *testing.T) {
	tests := []struct {
		name string
		opts []OllamaOption
	}{
		{
			name: "empty base URL",
			opts: []OllamaOption{WithOllamaBaseURL("")},
		},
		{
			name: "zero timeout",
			opts: []OllamaOption{WithOllamaTimeout(0)},
		},
		{
			name: "negative timeout",
			opts: []OllamaOption{WithOllamaTimeout(-1 * time.Second)},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := NewOllamaProvider(tt.opts...)
			if err == nil {
				t.Error("expected error for invalid options")
			}
		})
	}
}

func TestNewOllamaProvider_EnvVar(t *testing.T) {
	// Set environment variable
	_ = os.Setenv("OLLAMA_HOST", "http://env-host:9999")
	defer func() { _ = os.Unsetenv("OLLAMA_HOST") }()

	provider, err := NewOllamaProvider()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if provider == nil {
		t.Fatal("expected provider, got nil")
	}
}

func TestNewOllamaProvider_OptionOverridesEnvVar(t *testing.T) {
	_ = os.Setenv("OLLAMA_HOST", "http://env-host:9999")
	defer func() { _ = os.Unsetenv("OLLAMA_HOST") }()

	// Option should override env var
	provider, err := NewOllamaProvider(
		WithOllamaBaseURL("http://option-host:8888"),
	)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if provider == nil {
		t.Fatal("expected provider, got nil")
	}
}

func TestOllamaProvider_Chat(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/chat" {
			t.Errorf("unexpected path: %s", r.URL.Path)
		}
		if r.Method != http.MethodPost {
			t.Errorf("expected POST, got %s", r.Method)
		}

		var req ollamaChatRequest
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			t.Errorf("failed to decode request: %v", err)
		}

		if req.Model != "llama3.2" {
			t.Errorf("expected model 'llama3.2', got '%s'", req.Model)
		}
		if req.Stream {
			t.Error("expected stream=false")
		}

		resp := ollamaChatResponse{
			Model: "llama3.2",
			Message: ollamaMessage{
				Role:    "assistant",
				Content: "Hello! How can I help you?",
			},
			Done:            true,
			DoneReason:      strPtr("stop"),
			EvalCount:       10,
			PromptEvalCount: 5,
		}

		_ = json.NewEncoder(w).Encode(resp)
	}))
	defer server.Close()

	provider, _ := NewOllamaProvider(WithOllamaBaseURL(server.URL))

	req, _ := NewChatRequest("llama3.2",
		WithMessages(UserMessage("Hello")),
	)

	resp, err := provider.Chat(context.Background(), req)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if resp.Content != "Hello! How can I help you?" {
		t.Errorf("unexpected content: %s", resp.Content)
	}
	if resp.Model != "llama3.2" {
		t.Errorf("unexpected model: %s", resp.Model)
	}
	if resp.FinishReason == nil || *resp.FinishReason != FinishReasonStop {
		t.Error("expected FinishReasonStop")
	}
}

func TestOllamaProvider_Chat_WithOptions(t *testing.T) {
	var receivedReq ollamaChatRequest

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_ = json.NewDecoder(r.Body).Decode(&receivedReq)

		resp := ollamaChatResponse{
			Model:   "llama3.2",
			Message: ollamaMessage{Role: "assistant", Content: "Response"},
			Done:    true,
		}
		_ = json.NewEncoder(w).Encode(resp)
	}))
	defer server.Close()

	provider, _ := NewOllamaProvider(WithOllamaBaseURL(server.URL))

	temp := 0.7
	maxTokens := 100

	req, _ := NewChatRequest("llama3.2",
		WithMessages(UserMessage("Test")),
		WithTemperature(temp),
		WithMaxTokens(maxTokens),
	)

	_, err := provider.Chat(context.Background(), req)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if receivedReq.Options["temperature"] != 0.7 {
		t.Errorf("expected temperature 0.7, got %v", receivedReq.Options["temperature"])
	}
	if receivedReq.Options["num_predict"] != float64(100) {
		t.Errorf("expected num_predict 100, got %v", receivedReq.Options["num_predict"])
	}
}

func TestOllamaProvider_Chat_ThinkingMode(t *testing.T) {
	var receivedReq ollamaChatRequest

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_ = json.NewDecoder(r.Body).Decode(&receivedReq)

		resp := ollamaChatResponse{
			Model:   "qwen3",
			Message: ollamaMessage{Role: "assistant", Content: "The answer is 5", Thinking: "Let me solve this step by step..."},
			Done:    true,
		}
		_ = json.NewEncoder(w).Encode(resp)
	}))
	defer server.Close()

	provider, _ := NewOllamaProvider(WithOllamaBaseURL(server.URL))

	req, _ := NewChatRequest("qwen3",
		WithMessages(UserMessage("Solve: 2x + 5 = 15")),
		WithThinkingMode(ThinkingModeEnabled),
	)

	resp, err := provider.Chat(context.Background(), req)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	// Verify think parameter was sent
	if receivedReq.Think == nil || *receivedReq.Think != true {
		t.Error("expected think=true in request")
	}

	// Verify thinking content was extracted
	if resp.Thinking == nil || *resp.Thinking != "Let me solve this step by step..." {
		t.Errorf("expected thinking content, got %v", resp.Thinking)
	}
}

func TestOllamaProvider_Chat_Error(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusNotFound)
		_ = json.NewEncoder(w).Encode(ollamaErrorResponse{
			Error: "model 'unknown' not found",
		})
	}))
	defer server.Close()

	provider, _ := NewOllamaProvider(WithOllamaBaseURL(server.URL))

	req, _ := NewChatRequest("unknown",
		WithMessages(UserMessage("Hello")),
	)

	_, err := provider.Chat(context.Background(), req)
	if err == nil {
		t.Fatal("expected error")
	}

	var llmErr *LLMError
	if !errors.As(err, &llmErr) {
		t.Fatal("expected LLMError")
	}

	if llmErr.HTTPStatusCode != http.StatusNotFound {
		t.Errorf("expected status 404, got %d", llmErr.HTTPStatusCode)
	}
}

func TestOllamaProvider_ChatStream(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		// Return NDJSON stream
		chunks := []ollamaChatResponse{
			{Model: "llama3.2", Message: ollamaMessage{Role: "assistant", Content: "Hello"}, Done: false},
			{Model: "llama3.2", Message: ollamaMessage{Role: "assistant", Content: " there"}, Done: false},
			{Model: "llama3.2", Message: ollamaMessage{Role: "assistant", Content: "!"}, Done: true, DoneReason: strPtr("stop"), EvalCount: 3, PromptEvalCount: 5},
		}

		for _, chunk := range chunks {
			_ = json.NewEncoder(w).Encode(chunk)
		}
	}))
	defer server.Close()

	provider, _ := NewOllamaProvider(WithOllamaBaseURL(server.URL))

	req, _ := NewChatRequest("llama3.2",
		WithMessages(UserMessage("Hi")),
	)

	chunks, errs := provider.ChatStream(context.Background(), req)

	var content strings.Builder
	var finalChunk StreamChunk

	for chunk := range chunks {
		content.WriteString(chunk.Delta)
		if chunk.IsFinal() {
			finalChunk = chunk
		}
	}

	if err := <-errs; err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if content.String() != "Hello there!" {
		t.Errorf("expected 'Hello there!', got '%s'", content.String())
	}

	if finalChunk.FinishReason == nil || *finalChunk.FinishReason != FinishReasonStop {
		t.Error("expected final chunk with FinishReasonStop")
	}

	if finalChunk.Usage == nil || finalChunk.Usage.Actual == nil {
		t.Error("expected usage in final chunk")
	}
}

func TestOllamaProvider_ChatStream_Thinking(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		chunks := []ollamaChatResponse{
			{Model: "qwen3", Message: ollamaMessage{Content: "", Thinking: "Let me think..."}, Done: false},
			{Model: "qwen3", Message: ollamaMessage{Content: "Answer"}, Done: true},
		}

		for _, chunk := range chunks {
			_ = json.NewEncoder(w).Encode(chunk)
		}
	}))
	defer server.Close()

	provider, _ := NewOllamaProvider(WithOllamaBaseURL(server.URL))

	req, _ := NewChatRequest("qwen3",
		WithMessages(UserMessage("Solve this")),
		WithThinkingMode(ThinkingModeEnabled),
	)

	chunks, errs := provider.ChatStream(context.Background(), req)

	var hasThinking bool
	for chunk := range chunks {
		if chunk.HasThinking() {
			hasThinking = true
		}
	}

	if err := <-errs; err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if !hasThinking {
		t.Error("expected thinking content in stream")
	}
}

func TestOllamaProvider_ChatStream_ContextCancellation(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		// Simulate slow response
		time.Sleep(100 * time.Millisecond)
		_ = json.NewEncoder(w).Encode(ollamaChatResponse{
			Model:   "llama3.2",
			Message: ollamaMessage{Content: "Response"},
			Done:    true,
		})
	}))
	defer server.Close()

	provider, _ := NewOllamaProvider(WithOllamaBaseURL(server.URL))

	ctx, cancel := context.WithCancel(context.Background())
	cancel() // Cancel immediately

	req, _ := NewChatRequest("llama3.2",
		WithMessages(UserMessage("Hi")),
	)

	_, errs := provider.ChatStream(ctx, req)

	err := <-errs
	if err == nil {
		t.Error("expected context cancellation error")
	}
}

func TestOllamaProvider_ListModels(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/tags" {
			t.Errorf("unexpected path: %s", r.URL.Path)
		}

		resp := ollamaTagsResponse{
			Models: []ollamaModelInfo{
				{Name: "llama3.2:latest", Size: 4109853696, Digest: "sha256:abc", ModifiedAt: "2024-01-15T10:30:00Z"},
				{Name: "mistral:7b", Size: 4100000000, Digest: "sha256:def", ModifiedAt: "2024-01-14T09:00:00Z"},
			},
		}
		_ = json.NewEncoder(w).Encode(resp)
	}))
	defer server.Close()

	provider, _ := NewOllamaProvider(WithOllamaBaseURL(server.URL))

	models, err := provider.ListModels(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if len(models) != 2 {
		t.Fatalf("expected 2 models, got %d", len(models))
	}

	if models[0].Name != "llama3.2:latest" {
		t.Errorf("unexpected model name: %s", models[0].Name)
	}

	if models[0].SizeBytes == nil || *models[0].SizeBytes != 4109853696 {
		t.Errorf("unexpected size: %v", models[0].SizeBytes)
	}
}

func TestOllamaProvider_ListModels_Empty(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		resp := ollamaTagsResponse{Models: []ollamaModelInfo{}}
		_ = json.NewEncoder(w).Encode(resp)
	}))
	defer server.Close()

	provider, _ := NewOllamaProvider(WithOllamaBaseURL(server.URL))

	models, err := provider.ListModels(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if len(models) != 0 {
		t.Errorf("expected empty slice, got %d models", len(models))
	}
}

func TestOllamaProvider_Ping_Success(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/" {
			t.Errorf("unexpected path: %s", r.URL.Path)
		}
		w.WriteHeader(http.StatusOK)
		_, _ = fmt.Fprintf(w, `{"version":"0.5.1"}`)
	}))
	defer server.Close()

	provider, _ := NewOllamaProvider(WithOllamaBaseURL(server.URL))

	err := provider.Ping(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestOllamaProvider_Ping_Failure(t *testing.T) {
	// Create a server that immediately closes connections
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusInternalServerError)
	}))
	defer server.Close()

	provider, _ := NewOllamaProvider(WithOllamaBaseURL(server.URL))

	err := provider.Ping(context.Background())
	if err == nil {
		t.Error("expected error for failed ping")
	}
}

func TestOllamaProvider_NetworkError(t *testing.T) {
	provider, _ := NewOllamaProvider(
		WithOllamaBaseURL("http://localhost:99999"), // Invalid port
	)

	req, _ := NewChatRequest("llama3.2",
		WithMessages(UserMessage("Hi")),
	)

	_, err := provider.Chat(context.Background(), req)
	if err == nil {
		t.Fatal("expected network error")
	}

	if !errors.Is(err, ErrNetwork) {
		t.Errorf("expected ErrNetwork, got %T: %v", err, err)
	}
}

func TestOllamaProvider_GetCapabilities(t *testing.T) {
	provider, _ := NewOllamaProvider()

	caps := provider.GetCapabilities()

	// Ollama supports system messages
	if !caps.SupportsSystemMessages {
		t.Error("expected Ollama to support system messages")
	}

	// Ollama supports streaming
	if !caps.SupportsStreaming {
		t.Error("expected Ollama to support streaming")
	}

	// Ollama supports vision (for some models)
	if !caps.SupportsVision {
		t.Error("expected Ollama to support vision")
	}

	// Ollama has no documented max stop sequences
	if caps.MaxStopSequences != nil {
		t.Errorf("expected MaxStopSequences to be nil for Ollama, got %v", *caps.MaxStopSequences)
	}

	// Ollama supports penalties (via repeat_penalty etc)
	if !caps.SupportsPresencePenalty {
		t.Error("expected Ollama to support presence penalty")
	}
	if !caps.SupportsFrequencyPenalty {
		t.Error("expected Ollama to support frequency penalty")
	}

	// Ollama supports seed
	if !caps.SupportsSeed {
		t.Error("expected Ollama to support seed")
	}

	// Ollama does NOT support logprobs
	if caps.SupportsLogprobs {
		t.Error("expected Ollama to NOT support logprobs")
	}

	// Ollama supports JSON mode
	if !caps.SupportsJSONMode {
		t.Error("expected Ollama to support JSON mode")
	}
}

func TestOllamaProvider_GetModelCapabilities_VisionModel(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path == "/api/show" {
			// Mock llava response - a vision model
			_ = json.NewEncoder(w).Encode(map[string]any{
				"modelfile": "FROM llava:7b\nTEMPLATE ...",
				"details": map[string]any{
					"families": []string{"clip", "llama"},
				},
			})
			return
		}
	}))
	defer server.Close()

	provider, _ := NewOllamaProvider(WithOllamaBaseURL(server.URL))

	caps, err := provider.GetModelCapabilities(context.Background(), "llava:latest")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if !caps.VisionMode.SupportsVision() {
		t.Error("expected llava to support vision")
	}
}

func TestOllamaProvider_GetModelCapabilities_TextOnlyModel(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path == "/api/show" {
			// Mock llama response - text-only model
			_ = json.NewEncoder(w).Encode(map[string]any{
				"modelfile": "FROM llama3.2:3b\nTEMPLATE ...",
				"details": map[string]any{
					"families": []string{"llama"},
				},
			})
			return
		}
	}))
	defer server.Close()

	provider, _ := NewOllamaProvider(WithOllamaBaseURL(server.URL))

	caps, err := provider.GetModelCapabilities(context.Background(), "llama3.2")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if caps.VisionMode.SupportsVision() {
		t.Error("expected llama3.2 to NOT support vision")
	}

	// All models support streaming
	if !caps.SupportsStreaming {
		t.Error("expected llama3.2 to support streaming")
	}
}

func TestOllamaProvider_GetModelCapabilities_UnknownModel(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path == "/api/show" {
			w.WriteHeader(http.StatusNotFound)
			_ = json.NewEncoder(w).Encode(map[string]any{
				"error": "model 'nonexistent' not found",
			})
			return
		}
	}))
	defer server.Close()

	provider, _ := NewOllamaProvider(WithOllamaBaseURL(server.URL))

	_, err := provider.GetModelCapabilities(context.Background(), "nonexistent")
	if err == nil {
		t.Error("expected error for unknown model")
	}
}
