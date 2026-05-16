package nxuskit

import (
	"encoding/json"
	"testing"
)

func TestStreamChunkIsFinal(t *testing.T) {
	t.Run("not final", func(t *testing.T) {
		sc := StreamChunk{Delta: "hello"}
		if sc.IsFinal() {
			t.Error("IsFinal() should be false when FinishReason is nil")
		}
	})

	t.Run("final", func(t *testing.T) {
		reason := FinishReasonStop
		sc := StreamChunk{Delta: "hello", FinishReason: &reason}
		if !sc.IsFinal() {
			t.Error("IsFinal() should be true when FinishReason is set")
		}
	})
}

func TestStreamChunkHasThinking(t *testing.T) {
	t.Run("no thinking", func(t *testing.T) {
		sc := StreamChunk{Delta: "hello"}
		if sc.HasThinking() {
			t.Error("HasThinking() should be false when Thinking is nil")
		}
	})

	t.Run("empty thinking", func(t *testing.T) {
		empty := ""
		sc := StreamChunk{Thinking: &empty}
		if sc.HasThinking() {
			t.Error("HasThinking() should be false when Thinking is empty")
		}
	})

	t.Run("has thinking", func(t *testing.T) {
		thinking := "Let me think..."
		sc := StreamChunk{Thinking: &thinking}
		if !sc.HasThinking() {
			t.Error("HasThinking() should be true when Thinking is non-empty")
		}
	})
}

func TestStreamChunkHasContent(t *testing.T) {
	t.Run("no content", func(t *testing.T) {
		sc := StreamChunk{}
		if sc.HasContent() {
			t.Error("HasContent() should be false when Delta is empty")
		}
	})

	t.Run("has content", func(t *testing.T) {
		sc := StreamChunk{Delta: "hello"}
		if !sc.HasContent() {
			t.Error("HasContent() should be true when Delta is non-empty")
		}
	})
}

func TestNewStreamChunk(t *testing.T) {
	sc := NewStreamChunk("hello world")

	if sc.Delta != "hello world" {
		t.Errorf("Delta = %q, want %q", sc.Delta, "hello world")
	}
	if sc.FinishReason != nil {
		t.Error("FinishReason should be nil for regular chunk")
	}
	if sc.Thinking != nil {
		t.Error("Thinking should be nil")
	}
}

func TestThinkingChunk(t *testing.T) {
	sc := ThinkingChunk("Let me analyze this...")

	if sc.Thinking == nil {
		t.Error("Thinking should not be nil")
	}
	if *sc.Thinking != "Let me analyze this..." {
		t.Errorf("Thinking = %q, want %q", *sc.Thinking, "Let me analyze this...")
	}
	if sc.Delta != "" {
		t.Error("Delta should be empty for thinking chunk")
	}
}

func TestFinalChunk(t *testing.T) {
	usage := &TokenUsage{
		Estimated:  TokenCount{PromptTokens: 10, CompletionTokens: 5},
		IsComplete: true,
	}
	sc := FinalChunk("done", FinishReasonStop, usage)

	if sc.Delta != "done" {
		t.Errorf("Delta = %q, want %q", sc.Delta, "done")
	}
	if sc.FinishReason == nil {
		t.Error("FinishReason should not be nil")
	}
	if *sc.FinishReason != FinishReasonStop {
		t.Errorf("FinishReason = %v, want %v", *sc.FinishReason, FinishReasonStop)
	}
	if sc.Usage == nil {
		t.Error("Usage should not be nil")
	}
	if sc.Usage.TotalTokens() != 15 {
		t.Errorf("Usage.TotalTokens() = %d, want 15", sc.Usage.TotalTokens())
	}
}

func TestStreamChunkJSON(t *testing.T) {
	t.Run("simple chunk", func(t *testing.T) {
		sc := NewStreamChunk("hello")
		data, err := json.Marshal(sc)
		if err != nil {
			t.Fatalf("Marshal error: %v", err)
		}

		var decoded StreamChunk
		if err := json.Unmarshal(data, &decoded); err != nil {
			t.Fatalf("Unmarshal error: %v", err)
		}

		if decoded.Delta != "hello" {
			t.Errorf("Delta = %q, want %q", decoded.Delta, "hello")
		}
	})

	t.Run("final chunk with usage", func(t *testing.T) {
		usage := &TokenUsage{
			Estimated:  TokenCount{PromptTokens: 10, CompletionTokens: 5},
			IsComplete: true,
		}
		sc := FinalChunk("", FinishReasonStop, usage)

		data, err := json.Marshal(sc)
		if err != nil {
			t.Fatalf("Marshal error: %v", err)
		}

		var decoded StreamChunk
		if err := json.Unmarshal(data, &decoded); err != nil {
			t.Fatalf("Unmarshal error: %v", err)
		}

		if !decoded.IsFinal() {
			t.Error("IsFinal() should be true")
		}
		if decoded.Usage == nil {
			t.Error("Usage should not be nil")
		}
	})

	t.Run("chunk with thinking", func(t *testing.T) {
		sc := ThinkingChunk("analyzing...")
		data, err := json.Marshal(sc)
		if err != nil {
			t.Fatalf("Marshal error: %v", err)
		}

		var decoded StreamChunk
		if err := json.Unmarshal(data, &decoded); err != nil {
			t.Fatalf("Unmarshal error: %v", err)
		}

		if !decoded.HasThinking() {
			t.Error("HasThinking() should be true")
		}
		if *decoded.Thinking != "analyzing..." {
			t.Errorf("Thinking = %q, want %q", *decoded.Thinking, "analyzing...")
		}
	})
}
