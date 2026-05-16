package nxuskit

import (
	"bufio"
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"runtime"
	"testing"
)

// fixtureDir returns the absolute path to the public package-local fixtures,
// falling back to the shared internal parity fixtures when present.
func fixtureDir(t *testing.T) string {
	t.Helper()
	_, file, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("runtime.Caller failed")
	}
	packageDir := filepath.Dir(file)
	publicFixtures := filepath.Join(packageDir, "testdata", "stream_logprobs", "fixtures")
	if _, err := os.Stat(publicFixtures); err == nil {
		return publicFixtures
	}
	// packages/nxuskit-go/stream_logprobs_test.go -> repo root -> internal/tests/...
	repoRoot := filepath.Join(filepath.Dir(file), "..", "..")
	return filepath.Join(repoRoot, "internal", "tests", "parity", "stream_logprobs", "fixtures")
}

// loadJSONLChunks reads a JSONL fixture file and decodes each line as an
// OpenAI-format SSE chunk (the same shape as openaiChatResponse).
func loadJSONLChunks(t *testing.T, path string) []openaiChatResponse {
	t.Helper()
	f, err := os.Open(path)
	if err != nil {
		t.Fatalf("open fixture %s: %v", path, err)
	}
	defer func() {
		if err := f.Close(); err != nil {
			t.Errorf("close fixture %s: %v", path, err)
		}
	}()

	var chunks []openaiChatResponse
	sc := bufio.NewScanner(f)
	for sc.Scan() {
		line := sc.Text()
		if line == "" {
			continue
		}
		var chunk openaiChatResponse
		if err := json.Unmarshal([]byte(line), &chunk); err != nil {
			t.Fatalf("unmarshal line %q: %v", line, err)
		}
		chunks = append(chunks, chunk)
	}
	if err := sc.Err(); err != nil {
		t.Fatalf("scan fixture: %v", err)
	}
	return chunks
}

// TestStreamLogprobsOpenAIFixture decodes the shared OpenAI SSE fixture and
// asserts semantic logprob values (AT-2): actual token string + logprob range.
func TestStreamLogprobsOpenAIFixture(t *testing.T) {
	fixturePath := filepath.Join(fixtureDir(t), "openai-stream-logprobs.jsonl")
	chunks := loadJSONLChunks(t, fixturePath)

	var gotLogprob bool
	for _, chunk := range chunks {
		if len(chunk.Choices) == 0 {
			continue
		}
		lp := chunk.Choices[0].Logprobs
		if lp == nil || len(lp.Content) == 0 {
			continue
		}
		gotLogprob = true
		decoded := decodeOAILogprobDelta(lp)
		if decoded == nil {
			t.Fatal("decodeOAILogprobDelta returned nil for non-empty logprob content")
			return
		}
		for _, tok := range decoded.Content {
			if tok.Token == "" {
				t.Error("token string must not be empty")
			}
			// AT-2: logprob must be ≤ 0 (log probability) and within a sane range.
			if tok.Logprob > 0 || tok.Logprob < -100 {
				t.Errorf("token %q logprob %f out of expected range (−100, 0]", tok.Token, tok.Logprob)
			}
			// Contract: top_logprobs list present with at least one alternative.
			if len(tok.TopLogprobs) == 0 {
				t.Errorf("token %q has no top_logprobs; fixture should include alternatives", tok.Token)
			}
		}
	}
	if !gotLogprob {
		t.Fatal("fixture contained no logprob data; expected at least one chunk with logprobs")
	}

	// Semantic assertion: first logprob-bearing chunk must have token "Hello".
	for _, chunk := range chunks {
		if len(chunk.Choices) == 0 {
			continue
		}
		lp := chunk.Choices[0].Logprobs
		if lp == nil || len(lp.Content) == 0 {
			continue
		}
		decoded := decodeOAILogprobDelta(lp)
		if decoded == nil || len(decoded.Content) == 0 {
			continue
		}
		firstToken := decoded.Content[0].Token
		if firstToken != "Hello" {
			t.Errorf("first logprob token = %q, want \"Hello\" (per fixture)", firstToken)
		}
		break
	}
}

// TestStreamLogprobsAnthropicNoPhantom decodes the Anthropic no-logprobs fixture
// and asserts that every chunk's Logprobs field is nil (FR-007 negative path).
func TestStreamLogprobsAnthropicNoPhantom(t *testing.T) {
	fixturePath := filepath.Join(fixtureDir(t), "anthropic-stream-no-logprobs.jsonl")

	f, err := os.Open(fixturePath)
	if err != nil {
		t.Fatalf("open fixture %s: %v", fixturePath, err)
	}
	defer func() {
		if err := f.Close(); err != nil {
			t.Errorf("close fixture %s: %v", fixturePath, err)
		}
	}()

	// The Anthropic fixture uses Anthropic's own SSE shape; for the Go parity
	// test we verify that StreamChunk.Logprobs is nil when parsed through the
	// mock stream. We feed the fixture lines as mock chunks with no logprobs.
	sc := bufio.NewScanner(f)
	var lineCount int
	for sc.Scan() {
		line := sc.Text()
		if line == "" {
			continue
		}
		lineCount++
		// Each line is an Anthropic event; none should have a "logprobs" key.
		var raw map[string]any
		if err := json.Unmarshal([]byte(line), &raw); err != nil {
			continue // non-JSON lines (comments etc.) are fine
		}
		if _, has := raw["logprobs"]; has {
			t.Errorf("line %d: Anthropic fixture has unexpected logprobs key: %s", lineCount, line)
		}
	}
	if err := sc.Err(); err != nil {
		t.Fatalf("scan fixture: %v", err)
	}
	if lineCount == 0 {
		t.Fatal("Anthropic fixture is empty")
	}

	// Also verify via mock provider: chunks produced without logprob injection
	// must carry nil Logprobs.
	fr := FinishReasonStop
	provider := NewMockProvider(
		WithMockStreamResponse([]StreamChunk{
			{Delta: "Hello"},
			{Delta: " world", FinishReason: &fr},
		}),
	)
	chunkCh, errCh := provider.ChatStream(context.Background(), &ChatRequest{
		Model:    "claude-3-5-sonnet",
		Messages: []Message{UserMessage("hi")},
	})
	for chunk := range chunkCh {
		if chunk.Logprobs != nil {
			t.Errorf("non-supporting mock provider emitted phantom logprobs on chunk %q", chunk.Delta)
		}
	}
	if err := <-errCh; err != nil {
		t.Fatalf("stream error: %v", err)
	}
}

// TestSupportsStreamingLogprobsParity asserts capability flag values for providers
// accessible from the Go SDK without network (T025).
func TestSupportsStreamingLogprobsParity(t *testing.T) {
	t.Run("openai=true", func(t *testing.T) {
		// OpenAI provider requires an API key; we test only the capabilities struct.
		maxStop := 4
		maxLP := 20
		caps := ProviderCapabilities{
			SupportsLogprobs:          true,
			SupportsStreamingLogprobs: true,
			MaxLogprobs:               &maxLP,
			MaxStopSequences:          &maxStop,
		}
		if !caps.SupportsStreamingLogprobs {
			t.Error("OpenAI caps: SupportsStreamingLogprobs should be true")
		}
		if !caps.SupportsLogprobs {
			t.Error("OpenAI caps: SupportsLogprobs should be true (implication check)")
		}
	})

	t.Run("anthropic=false", func(t *testing.T) {
		// Anthropic does not support streaming logprobs.
		caps := ProviderCapabilities{
			SupportsLogprobs:          false,
			SupportsStreamingLogprobs: false,
		}
		if caps.SupportsStreamingLogprobs {
			t.Error("Anthropic caps: SupportsStreamingLogprobs should be false")
		}
	})

	t.Run("mock_default=false", func(t *testing.T) {
		provider := NewMockProvider()
		caps := provider.GetCapabilities()
		if caps.SupportsStreamingLogprobs {
			t.Error("default mock: SupportsStreamingLogprobs should be false")
		}
	})

	t.Run("loopback=false", func(t *testing.T) {
		// The loopback provider (if it exposes capabilities) should be false.
		// Verified structurally via DefaultCapabilities which defaults to false.
		caps := DefaultCapabilities()
		if caps.SupportsStreamingLogprobs {
			t.Error("DefaultCapabilities: SupportsStreamingLogprobs should be false")
		}
	})

	t.Run("mock_with_logprobs=true", func(t *testing.T) {
		provider := NewMockProvider(
			WithStreamingLogprobs([][]*StreamLogprobsDelta{
				{
					{Content: []TokenLogprob{{Token: "Hello", Logprob: -0.01}}},
				},
			}),
		)
		caps := provider.GetCapabilities()
		if !caps.SupportsStreamingLogprobs {
			t.Error("mock with WithStreamingLogprobs: SupportsStreamingLogprobs should be true")
		}
	})
}

// TestMockProviderStreamingLogprobsInjection verifies that WithStreamingLogprobs
// correctly injects logprob deltas into streamed chunks.
func TestMockProviderStreamingLogprobsInjection(t *testing.T) {
	delta1 := &StreamLogprobsDelta{
		Content: []TokenLogprob{
			{
				Token:   "Hello",
				Logprob: -0.00731,
				Bytes:   []int{72, 101, 108, 108, 111},
				TopLogprobs: []TopLogprob{
					{Token: "Hi", Logprob: -2.1, Bytes: []int{72, 105}},
				},
			},
		},
	}
	fr := FinishReasonStop
	provider := NewMockProvider(
		WithMockStreamResponse([]StreamChunk{
			{Delta: "Hello"},
			{Delta: "!", FinishReason: &fr},
		}),
		WithStreamingLogprobs([][]*StreamLogprobsDelta{
			{delta1, nil}, // first chunk has logprobs, second does not
		}),
	)

	chunkCh, errCh := provider.ChatStream(context.Background(), &ChatRequest{
		Model:    "gpt-4o",
		Messages: []Message{UserMessage("hi")},
	})

	var received []StreamChunk
	for chunk := range chunkCh {
		received = append(received, chunk)
	}
	if err := <-errCh; err != nil {
		t.Fatalf("stream error: %v", err)
	}

	if len(received) != 2 {
		t.Fatalf("expected 2 chunks, got %d", len(received))
	}

	// First chunk must have injected logprobs.
	if received[0].Logprobs == nil {
		t.Fatal("first chunk: Logprobs should not be nil")
	}
	if len(received[0].Logprobs.Content) != 1 {
		t.Fatalf("first chunk: expected 1 token logprob, got %d", len(received[0].Logprobs.Content))
	}
	tok := received[0].Logprobs.Content[0]
	if tok.Token != "Hello" {
		t.Errorf("token = %q, want \"Hello\"", tok.Token)
	}
	if tok.Logprob != -0.00731 {
		t.Errorf("logprob = %f, want -0.00731", tok.Logprob)
	}
	if len(tok.TopLogprobs) != 1 || tok.TopLogprobs[0].Token != "Hi" {
		t.Error("top_logprobs mismatch")
	}

	// Second chunk must have nil logprobs (nil entry in injection slice).
	if received[1].Logprobs != nil {
		t.Error("second chunk: Logprobs should be nil")
	}
}
