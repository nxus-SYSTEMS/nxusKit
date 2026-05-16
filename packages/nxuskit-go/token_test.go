package nxuskit

import (
	"encoding/json"
	"testing"
)

func TestTokenCount(t *testing.T) {
	t.Run("Total", func(t *testing.T) {
		tc := TokenCount{PromptTokens: 100, CompletionTokens: 50}
		if tc.Total() != 150 {
			t.Errorf("Total() = %d, want 150", tc.Total())
		}
	})

	t.Run("Zero values", func(t *testing.T) {
		tc := TokenCount{}
		if tc.Total() != 0 {
			t.Errorf("Total() = %d, want 0", tc.Total())
		}
	})

	t.Run("JSON roundtrip", func(t *testing.T) {
		tc := TokenCount{PromptTokens: 100, CompletionTokens: 50}
		data, err := json.Marshal(tc)
		if err != nil {
			t.Fatalf("Marshal error: %v", err)
		}

		var decoded TokenCount
		if err := json.Unmarshal(data, &decoded); err != nil {
			t.Fatalf("Unmarshal error: %v", err)
		}

		if decoded.PromptTokens != 100 {
			t.Errorf("PromptTokens = %d, want 100", decoded.PromptTokens)
		}
		if decoded.CompletionTokens != 50 {
			t.Errorf("CompletionTokens = %d, want 50", decoded.CompletionTokens)
		}
	})
}

func TestTokenUsageBestAvailable(t *testing.T) {
	t.Run("with actual", func(t *testing.T) {
		actual := TokenCount{PromptTokens: 100, CompletionTokens: 50}
		tu := TokenUsage{
			Actual:    &actual,
			Estimated: TokenCount{PromptTokens: 90, CompletionTokens: 45},
		}

		best := tu.BestAvailable()
		if best.PromptTokens != 100 {
			t.Errorf("BestAvailable().PromptTokens = %d, want 100", best.PromptTokens)
		}
		if best.CompletionTokens != 50 {
			t.Errorf("BestAvailable().CompletionTokens = %d, want 50", best.CompletionTokens)
		}
	})

	t.Run("without actual", func(t *testing.T) {
		tu := TokenUsage{
			Actual:    nil,
			Estimated: TokenCount{PromptTokens: 90, CompletionTokens: 45},
		}

		best := tu.BestAvailable()
		if best.PromptTokens != 90 {
			t.Errorf("BestAvailable().PromptTokens = %d, want 90", best.PromptTokens)
		}
		if best.CompletionTokens != 45 {
			t.Errorf("BestAvailable().CompletionTokens = %d, want 45", best.CompletionTokens)
		}
	})
}

func TestTokenUsageTotalTokens(t *testing.T) {
	t.Run("with actual", func(t *testing.T) {
		actual := TokenCount{PromptTokens: 100, CompletionTokens: 50}
		tu := TokenUsage{
			Actual:    &actual,
			Estimated: TokenCount{PromptTokens: 90, CompletionTokens: 45},
		}

		if tu.TotalTokens() != 150 {
			t.Errorf("TotalTokens() = %d, want 150", tu.TotalTokens())
		}
	})

	t.Run("without actual", func(t *testing.T) {
		tu := TokenUsage{
			Actual:    nil,
			Estimated: TokenCount{PromptTokens: 90, CompletionTokens: 45},
		}

		if tu.TotalTokens() != 135 {
			t.Errorf("TotalTokens() = %d, want 135", tu.TotalTokens())
		}
	})
}

func TestTokenUsageJSON(t *testing.T) {
	t.Run("with actual", func(t *testing.T) {
		actual := TokenCount{PromptTokens: 100, CompletionTokens: 50}
		tu := TokenUsage{
			Actual:     &actual,
			Estimated:  TokenCount{PromptTokens: 90, CompletionTokens: 45},
			IsComplete: true,
		}

		data, err := json.Marshal(tu)
		if err != nil {
			t.Fatalf("Marshal error: %v", err)
		}

		var decoded TokenUsage
		if err := json.Unmarshal(data, &decoded); err != nil {
			t.Fatalf("Unmarshal error: %v", err)
		}

		if decoded.Actual == nil {
			t.Error("Actual should not be nil")
		}
		if decoded.Actual.PromptTokens != 100 {
			t.Errorf("Actual.PromptTokens = %d, want 100", decoded.Actual.PromptTokens)
		}
		if !decoded.IsComplete {
			t.Error("IsComplete should be true")
		}
	})

	t.Run("without actual", func(t *testing.T) {
		tu := TokenUsage{
			Estimated:  TokenCount{PromptTokens: 90, CompletionTokens: 45},
			IsComplete: false,
		}

		data, err := json.Marshal(tu)
		if err != nil {
			t.Fatalf("Marshal error: %v", err)
		}

		var decoded TokenUsage
		if err := json.Unmarshal(data, &decoded); err != nil {
			t.Fatalf("Unmarshal error: %v", err)
		}

		if decoded.Actual != nil {
			t.Error("Actual should be nil")
		}
		if decoded.Estimated.PromptTokens != 90 {
			t.Errorf("Estimated.PromptTokens = %d, want 90", decoded.Estimated.PromptTokens)
		}
	})
}
