package nxuskit

import (
	"testing"
)

func TestNewChatRequest(t *testing.T) {
	t.Run("minimal request", func(t *testing.T) {
		req, err := NewChatRequest("gpt-4o")
		if err != nil {
			t.Fatalf("NewChatRequest error: %v", err)
		}
		if req.Model != "gpt-4o" {
			t.Errorf("Model = %q, want %q", req.Model, "gpt-4o")
		}
		if req.ThinkingMode != ThinkingModeAuto {
			t.Errorf("ThinkingMode = %v, want %v", req.ThinkingMode, ThinkingModeAuto)
		}
	})

	t.Run("empty model", func(t *testing.T) {
		_, err := NewChatRequest("")
		if err == nil {
			t.Error("Expected error for empty model")
		}
	})

	t.Run("with messages", func(t *testing.T) {
		req, err := NewChatRequest("gpt-4o",
			WithMessages(
				SystemMessage("You are helpful"),
				UserMessage("Hello"),
			),
		)
		if err != nil {
			t.Fatalf("NewChatRequest error: %v", err)
		}
		if len(req.Messages) != 2 {
			t.Errorf("Messages len = %d, want 2", len(req.Messages))
		}
		if req.Messages[0].Role != RoleSystem {
			t.Errorf("First message role = %v, want %v", req.Messages[0].Role, RoleSystem)
		}
	})
}

func TestWithTemperature(t *testing.T) {
	t.Run("valid temperature", func(t *testing.T) {
		req, err := NewChatRequest("gpt-4o", WithTemperature(0.7))
		if err != nil {
			t.Fatalf("error: %v", err)
		}
		if req.Temperature == nil {
			t.Error("Temperature should not be nil")
		}
		if *req.Temperature != 0.7 {
			t.Errorf("Temperature = %f, want 0.7", *req.Temperature)
		}
	})

	t.Run("temperature at boundary", func(t *testing.T) {
		_, err := NewChatRequest("gpt-4o", WithTemperature(0))
		if err != nil {
			t.Errorf("Temperature 0 should be valid: %v", err)
		}

		_, err = NewChatRequest("gpt-4o", WithTemperature(2))
		if err != nil {
			t.Errorf("Temperature 2 should be valid: %v", err)
		}
	})

	t.Run("temperature out of range", func(t *testing.T) {
		_, err := NewChatRequest("gpt-4o", WithTemperature(-0.1))
		if err == nil {
			t.Error("Expected error for negative temperature")
		}

		_, err = NewChatRequest("gpt-4o", WithTemperature(2.1))
		if err == nil {
			t.Error("Expected error for temperature > 2")
		}
	})
}

func TestWithMaxTokens(t *testing.T) {
	t.Run("valid max_tokens", func(t *testing.T) {
		req, err := NewChatRequest("gpt-4o", WithMaxTokens(1000))
		if err != nil {
			t.Fatalf("error: %v", err)
		}
		if req.MaxTokens == nil {
			t.Error("MaxTokens should not be nil")
		}
		if *req.MaxTokens != 1000 {
			t.Errorf("MaxTokens = %d, want 1000", *req.MaxTokens)
		}
	})

	t.Run("zero max_tokens", func(t *testing.T) {
		_, err := NewChatRequest("gpt-4o", WithMaxTokens(0))
		if err == nil {
			t.Error("Expected error for zero max_tokens")
		}
	})

	t.Run("negative max_tokens", func(t *testing.T) {
		_, err := NewChatRequest("gpt-4o", WithMaxTokens(-1))
		if err == nil {
			t.Error("Expected error for negative max_tokens")
		}
	})
}

func TestWithTopP(t *testing.T) {
	t.Run("valid top_p", func(t *testing.T) {
		req, err := NewChatRequest("gpt-4o", WithTopP(0.9))
		if err != nil {
			t.Fatalf("error: %v", err)
		}
		if req.TopP == nil {
			t.Error("TopP should not be nil")
		}
		if *req.TopP != 0.9 {
			t.Errorf("TopP = %f, want 0.9", *req.TopP)
		}
	})

	t.Run("top_p at boundaries", func(t *testing.T) {
		_, err := NewChatRequest("gpt-4o", WithTopP(0))
		if err != nil {
			t.Errorf("TopP 0 should be valid: %v", err)
		}

		_, err = NewChatRequest("gpt-4o", WithTopP(1))
		if err != nil {
			t.Errorf("TopP 1 should be valid: %v", err)
		}
	})

	t.Run("top_p out of range", func(t *testing.T) {
		_, err := NewChatRequest("gpt-4o", WithTopP(-0.1))
		if err == nil {
			t.Error("Expected error for negative top_p")
		}

		_, err = NewChatRequest("gpt-4o", WithTopP(1.1))
		if err == nil {
			t.Error("Expected error for top_p > 1")
		}
	})
}

func TestWithStream(t *testing.T) {
	req, err := NewChatRequest("gpt-4o", WithStream(true))
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if !req.Stream {
		t.Error("Stream should be true")
	}
}

func TestWithStop(t *testing.T) {
	req, err := NewChatRequest("gpt-4o", WithStop("END", "STOP"))
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if len(req.Stop) != 2 {
		t.Errorf("Stop len = %d, want 2", len(req.Stop))
	}
	if req.Stop[0] != "END" {
		t.Errorf("Stop[0] = %q, want %q", req.Stop[0], "END")
	}
}

func TestWithSeed(t *testing.T) {
	req, err := NewChatRequest("gpt-4o", WithSeed(42))
	if err != nil {
		t.Fatalf("error: %v", err)
	}
	if req.Seed == nil {
		t.Error("Seed should not be nil")
	}
	if *req.Seed != 42 {
		t.Errorf("Seed = %d, want 42", *req.Seed)
	}
}

func TestWithPresencePenalty(t *testing.T) {
	t.Run("valid presence_penalty", func(t *testing.T) {
		req, err := NewChatRequest("gpt-4o", WithPresencePenalty(0.5))
		if err != nil {
			t.Fatalf("error: %v", err)
		}
		if req.PresencePenalty == nil {
			t.Error("PresencePenalty should not be nil")
		}
		if *req.PresencePenalty != 0.5 {
			t.Errorf("PresencePenalty = %f, want 0.5", *req.PresencePenalty)
		}
	})

	t.Run("presence_penalty at boundaries", func(t *testing.T) {
		_, err := NewChatRequest("gpt-4o", WithPresencePenalty(-2))
		if err != nil {
			t.Errorf("PresencePenalty -2 should be valid: %v", err)
		}

		_, err = NewChatRequest("gpt-4o", WithPresencePenalty(2))
		if err != nil {
			t.Errorf("PresencePenalty 2 should be valid: %v", err)
		}
	})

	t.Run("presence_penalty out of range", func(t *testing.T) {
		_, err := NewChatRequest("gpt-4o", WithPresencePenalty(-2.1))
		if err == nil {
			t.Error("Expected error for presence_penalty < -2")
		}

		_, err = NewChatRequest("gpt-4o", WithPresencePenalty(2.1))
		if err == nil {
			t.Error("Expected error for presence_penalty > 2")
		}
	})
}

func TestWithFrequencyPenalty(t *testing.T) {
	t.Run("valid frequency_penalty", func(t *testing.T) {
		req, err := NewChatRequest("gpt-4o", WithFrequencyPenalty(-0.5))
		if err != nil {
			t.Fatalf("error: %v", err)
		}
		if req.FrequencyPenalty == nil {
			t.Error("FrequencyPenalty should not be nil")
		}
		if *req.FrequencyPenalty != -0.5 {
			t.Errorf("FrequencyPenalty = %f, want -0.5", *req.FrequencyPenalty)
		}
	})

	t.Run("frequency_penalty out of range", func(t *testing.T) {
		_, err := NewChatRequest("gpt-4o", WithFrequencyPenalty(-2.1))
		if err == nil {
			t.Error("Expected error for frequency_penalty < -2")
		}

		_, err = NewChatRequest("gpt-4o", WithFrequencyPenalty(2.1))
		if err == nil {
			t.Error("Expected error for frequency_penalty > 2")
		}
	})
}

func TestWithThinkingMode(t *testing.T) {
	tests := []struct {
		mode ThinkingMode
	}{
		{ThinkingModeAuto},
		{ThinkingModeEnabled},
		{ThinkingModeDisabled},
		{ThinkingModeOmit},
	}

	for _, tt := range tests {
		t.Run(tt.mode.String(), func(t *testing.T) {
			req, err := NewChatRequest("gpt-4o", WithThinkingMode(tt.mode))
			if err != nil {
				t.Fatalf("error: %v", err)
			}
			if req.ThinkingMode != tt.mode {
				t.Errorf("ThinkingMode = %v, want %v", req.ThinkingMode, tt.mode)
			}
		})
	}
}

func TestMultipleOptions(t *testing.T) {
	req, err := NewChatRequest("gpt-4o",
		WithMessages(UserMessage("Hello")),
		WithTemperature(0.7),
		WithMaxTokens(500),
		WithTopP(0.9),
		WithStream(true),
		WithThinkingMode(ThinkingModeEnabled),
	)
	if err != nil {
		t.Fatalf("error: %v", err)
	}

	if req.Model != "gpt-4o" {
		t.Errorf("Model = %q, want %q", req.Model, "gpt-4o")
	}
	if len(req.Messages) != 1 {
		t.Errorf("Messages len = %d, want 1", len(req.Messages))
	}
	if *req.Temperature != 0.7 {
		t.Errorf("Temperature = %f, want 0.7", *req.Temperature)
	}
	if *req.MaxTokens != 500 {
		t.Errorf("MaxTokens = %d, want 500", *req.MaxTokens)
	}
	if *req.TopP != 0.9 {
		t.Errorf("TopP = %f, want 0.9", *req.TopP)
	}
	if !req.Stream {
		t.Error("Stream should be true")
	}
	if req.ThinkingMode != ThinkingModeEnabled {
		t.Errorf("ThinkingMode = %v, want %v", req.ThinkingMode, ThinkingModeEnabled)
	}
}

func TestOptionError(t *testing.T) {
	// Test that an error from an option stops processing
	_, err := NewChatRequest("gpt-4o",
		WithTemperature(0.7), // valid
		WithMaxTokens(-1),    // invalid - should cause error
		WithTopP(0.9),        // should not be reached
	)

	if err == nil {
		t.Error("Expected error from invalid option")
	}
}

// Tests for new functional options (T030)

func TestWithResponseFormat(t *testing.T) {
	t.Run("text format", func(t *testing.T) {
		req, err := NewChatRequest("gpt-4o", WithResponseFormat(ResponseFormatText()))
		if err != nil {
			t.Fatalf("error: %v", err)
		}
		if req.ResponseFormat == nil {
			t.Fatal("ResponseFormat should not be nil")
		}
		if req.ResponseFormat.Type != "text" {
			t.Errorf("ResponseFormat.Type = %q, want %q", req.ResponseFormat.Type, "text")
		}
	})

	t.Run("json format", func(t *testing.T) {
		req, err := NewChatRequest("gpt-4o", WithResponseFormat(ResponseFormatJSON()))
		if err != nil {
			t.Fatalf("error: %v", err)
		}
		if req.ResponseFormat.Type != "json_object" {
			t.Errorf("ResponseFormat.Type = %q, want %q", req.ResponseFormat.Type, "json_object")
		}
	})

	t.Run("json_schema format", func(t *testing.T) {
		schema := map[string]any{"type": "object"}
		req, err := NewChatRequest("gpt-4o", WithResponseFormat(ResponseFormatJSONSchema("test", schema)))
		if err != nil {
			t.Fatalf("error: %v", err)
		}
		if req.ResponseFormat.Type != "json_schema" {
			t.Errorf("ResponseFormat.Type = %q, want %q", req.ResponseFormat.Type, "json_schema")
		}
		if req.ResponseFormat.JSONSchema == nil {
			t.Fatal("JSONSchema should not be nil")
		}
	})

	t.Run("nil format", func(t *testing.T) {
		req, err := NewChatRequest("gpt-4o", WithResponseFormat(nil))
		if err != nil {
			t.Fatalf("error: %v", err)
		}
		if req.ResponseFormat != nil {
			t.Error("ResponseFormat should be nil")
		}
	})
}

func TestWithTools(t *testing.T) {
	t.Run("single tool", func(t *testing.T) {
		tool := NewTool("get_weather", "Get weather", nil)
		req, err := NewChatRequest("gpt-4o", WithTools(tool))
		if err != nil {
			t.Fatalf("error: %v", err)
		}
		if len(req.Tools) != 1 {
			t.Errorf("Tools len = %d, want 1", len(req.Tools))
		}
		if req.Tools[0].Function.Name != "get_weather" {
			t.Errorf("Tools[0].Function.Name = %q, want %q", req.Tools[0].Function.Name, "get_weather")
		}
	})

	t.Run("multiple tools", func(t *testing.T) {
		tool1 := NewTool("search", "Search the web", nil)
		tool2 := NewTool("calculate", "Calculate math", nil)
		req, err := NewChatRequest("gpt-4o", WithTools(tool1, tool2))
		if err != nil {
			t.Fatalf("error: %v", err)
		}
		if len(req.Tools) != 2 {
			t.Errorf("Tools len = %d, want 2", len(req.Tools))
		}
	})

	t.Run("no tools", func(t *testing.T) {
		req, err := NewChatRequest("gpt-4o", WithTools())
		if err != nil {
			t.Fatalf("error: %v", err)
		}
		if len(req.Tools) != 0 {
			t.Errorf("Tools len = %d, want 0", len(req.Tools))
		}
	})
}

func TestWithToolChoice(t *testing.T) {
	t.Run("auto", func(t *testing.T) {
		req, err := NewChatRequest("gpt-4o", WithToolChoice(ToolChoiceAuto()))
		if err != nil {
			t.Fatalf("error: %v", err)
		}
		if req.ToolChoice == nil {
			t.Fatal("ToolChoice should not be nil")
		}
		if req.ToolChoice.Type != "auto" {
			t.Errorf("ToolChoice.Type = %q, want %q", req.ToolChoice.Type, "auto")
		}
	})

	t.Run("none", func(t *testing.T) {
		req, err := NewChatRequest("gpt-4o", WithToolChoice(ToolChoiceNone()))
		if err != nil {
			t.Fatalf("error: %v", err)
		}
		if req.ToolChoice.Type != "none" {
			t.Errorf("ToolChoice.Type = %q, want %q", req.ToolChoice.Type, "none")
		}
	})

	t.Run("required", func(t *testing.T) {
		req, err := NewChatRequest("gpt-4o", WithToolChoice(ToolChoiceRequired()))
		if err != nil {
			t.Fatalf("error: %v", err)
		}
		if req.ToolChoice.Type != "required" {
			t.Errorf("ToolChoice.Type = %q, want %q", req.ToolChoice.Type, "required")
		}
	})

	t.Run("function", func(t *testing.T) {
		req, err := NewChatRequest("gpt-4o", WithToolChoice(ToolChoiceFunc("my_func")))
		if err != nil {
			t.Fatalf("error: %v", err)
		}
		if req.ToolChoice.Type != "function" {
			t.Errorf("ToolChoice.Type = %q, want %q", req.ToolChoice.Type, "function")
		}
		if req.ToolChoice.Function.Name != "my_func" {
			t.Errorf("ToolChoice.Function.Name = %q, want %q", req.ToolChoice.Function.Name, "my_func")
		}
	})

	t.Run("nil", func(t *testing.T) {
		req, err := NewChatRequest("gpt-4o", WithToolChoice(nil))
		if err != nil {
			t.Fatalf("error: %v", err)
		}
		if req.ToolChoice != nil {
			t.Error("ToolChoice should be nil")
		}
	})
}

func TestWithTopK(t *testing.T) {
	t.Run("valid top_k", func(t *testing.T) {
		req, err := NewChatRequest("claude-sonnet-4-20250514", WithTopK(40))
		if err != nil {
			t.Fatalf("error: %v", err)
		}
		if req.TopK == nil {
			t.Fatal("TopK should not be nil")
		}
		if *req.TopK != 40 {
			t.Errorf("TopK = %d, want 40", *req.TopK)
		}
	})

	t.Run("top_k = 1", func(t *testing.T) {
		req, err := NewChatRequest("gpt-4o", WithTopK(1))
		if err != nil {
			t.Fatalf("error: %v", err)
		}
		if *req.TopK != 1 {
			t.Errorf("TopK = %d, want 1", *req.TopK)
		}
	})

	t.Run("zero top_k", func(t *testing.T) {
		_, err := NewChatRequest("gpt-4o", WithTopK(0))
		if err == nil {
			t.Error("Expected error for zero top_k")
		}
	})

	t.Run("negative top_k", func(t *testing.T) {
		_, err := NewChatRequest("gpt-4o", WithTopK(-1))
		if err == nil {
			t.Error("Expected error for negative top_k")
		}
	})
}

func TestWithMinP(t *testing.T) {
	t.Run("valid min_p", func(t *testing.T) {
		req, err := NewChatRequest("llama3:latest", WithMinP(0.05))
		if err != nil {
			t.Fatalf("error: %v", err)
		}
		if req.MinP == nil {
			t.Fatal("MinP should not be nil")
		}
		if *req.MinP != 0.05 {
			t.Errorf("MinP = %f, want 0.05", *req.MinP)
		}
	})

	t.Run("min_p at boundaries", func(t *testing.T) {
		_, err := NewChatRequest("gpt-4o", WithMinP(0))
		if err != nil {
			t.Errorf("MinP 0 should be valid: %v", err)
		}

		_, err = NewChatRequest("gpt-4o", WithMinP(1))
		if err != nil {
			t.Errorf("MinP 1 should be valid: %v", err)
		}
	})

	t.Run("min_p out of range", func(t *testing.T) {
		_, err := NewChatRequest("gpt-4o", WithMinP(-0.1))
		if err == nil {
			t.Error("Expected error for negative min_p")
		}

		_, err = NewChatRequest("gpt-4o", WithMinP(1.1))
		if err == nil {
			t.Error("Expected error for min_p > 1")
		}
	})
}

func TestNewOptionsWithAllFields(t *testing.T) {
	tool := NewTool("search", "Search", nil)
	schema := map[string]any{"type": "object"}

	req, err := NewChatRequest("gpt-4o",
		WithMessages(UserMessage("Hello")),
		WithTemperature(0.7),
		WithMaxTokens(500),
		WithTopP(0.9),
		WithResponseFormat(ResponseFormatJSONSchema("test", schema)),
		WithTools(tool),
		WithToolChoice(ToolChoiceAuto()),
		WithTopK(40),
		WithMinP(0.05),
	)
	if err != nil {
		t.Fatalf("error: %v", err)
	}

	if req.Model != "gpt-4o" {
		t.Errorf("Model = %q, want %q", req.Model, "gpt-4o")
	}
	if req.ResponseFormat == nil || req.ResponseFormat.Type != "json_schema" {
		t.Error("ResponseFormat should be json_schema")
	}
	if len(req.Tools) != 1 {
		t.Error("Should have 1 tool")
	}
	if req.ToolChoice == nil || req.ToolChoice.Type != "auto" {
		t.Error("ToolChoice should be auto")
	}
	if req.TopK == nil || *req.TopK != 40 {
		t.Error("TopK should be 40")
	}
	if req.MinP == nil || *req.MinP != 0.05 {
		t.Error("MinP should be 0.05")
	}
}
