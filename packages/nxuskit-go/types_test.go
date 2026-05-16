package nxuskit

import (
	"encoding/json"
	"testing"
)

func TestRole(t *testing.T) {
	tests := []struct {
		role     Role
		expected string
	}{
		{RoleSystem, "system"},
		{RoleUser, "user"},
		{RoleAssistant, "assistant"},
	}

	for _, tt := range tests {
		t.Run(tt.expected, func(t *testing.T) {
			if tt.role.String() != tt.expected {
				t.Errorf("Role.String() = %q, want %q", tt.role.String(), tt.expected)
			}
		})
	}
}

func TestFinishReason(t *testing.T) {
	tests := []struct {
		reason   FinishReason
		expected string
	}{
		{FinishReasonStop, "stop"},
		{FinishReasonLength, "length"},
		{FinishReasonContentFilter, "content_filter"},
		{FinishReasonToolCalls, "tool_calls"},
		{FinishReasonError, "error"},
	}

	for _, tt := range tests {
		t.Run(tt.expected, func(t *testing.T) {
			if tt.reason.String() != tt.expected {
				t.Errorf("FinishReason.String() = %q, want %q", tt.reason.String(), tt.expected)
			}
		})
	}
}

func TestParseFinishReason(t *testing.T) {
	tests := []struct {
		input    string
		expected FinishReason
	}{
		{"stop", FinishReasonStop},
		{"end_turn", FinishReasonStop},
		{"end", FinishReasonStop},
		{"complete", FinishReasonStop},
		{"length", FinishReasonLength},
		{"max_tokens", FinishReasonLength},
		{"content_filter", FinishReasonContentFilter},
		{"content_filtered", FinishReasonContentFilter},
		{"tool_calls", FinishReasonToolCalls},
		{"function_call", FinishReasonToolCalls},
		{"tool_use", FinishReasonToolCalls},
		{"error", FinishReasonError},
		{"unknown_value", FinishReason("unknown_value")},
	}

	for _, tt := range tests {
		t.Run(tt.input, func(t *testing.T) {
			result := ParseFinishReason(tt.input)
			if result != tt.expected {
				t.Errorf("ParseFinishReason(%q) = %q, want %q", tt.input, result, tt.expected)
			}
		})
	}
}

func TestThinkingMode(t *testing.T) {
	tests := []struct {
		mode     ThinkingMode
		expected string
	}{
		{ThinkingModeAuto, "auto"},
		{ThinkingModeEnabled, "enabled"},
		{ThinkingModeDisabled, "disabled"},
		{ThinkingModeOmit, "omit"},
		{ThinkingMode(99), "unknown"},
	}

	for _, tt := range tests {
		t.Run(tt.expected, func(t *testing.T) {
			if tt.mode.String() != tt.expected {
				t.Errorf("ThinkingMode.String() = %q, want %q", tt.mode.String(), tt.expected)
			}
		})
	}
}

func TestMessageConstructors(t *testing.T) {
	t.Run("SystemMessage", func(t *testing.T) {
		msg := SystemMessage("You are helpful")
		if msg.Role != RoleSystem {
			t.Errorf("SystemMessage role = %v, want %v", msg.Role, RoleSystem)
		}
		if msg.Content.Text != "You are helpful" {
			t.Errorf("SystemMessage content = %q, want %q", msg.Content.Text, "You are helpful")
		}
	})

	t.Run("UserMessage", func(t *testing.T) {
		msg := UserMessage("Hello!")
		if msg.Role != RoleUser {
			t.Errorf("UserMessage role = %v, want %v", msg.Role, RoleUser)
		}
		if msg.Content.Text != "Hello!" {
			t.Errorf("UserMessage content = %q, want %q", msg.Content.Text, "Hello!")
		}
	})

	t.Run("AssistantMessage", func(t *testing.T) {
		msg := AssistantMessage("Hi there!")
		if msg.Role != RoleAssistant {
			t.Errorf("AssistantMessage role = %v, want %v", msg.Role, RoleAssistant)
		}
		if msg.Content.Text != "Hi there!" {
			t.Errorf("AssistantMessage content = %q, want %q", msg.Content.Text, "Hi there!")
		}
	})
}

func TestMessageWithImageURL(t *testing.T) {
	msg := UserMessage("What's in this image?").WithImageURL("https://example.com/image.jpg")

	if !msg.Content.IsMultimodal() {
		t.Error("Expected multimodal content after WithImageURL")
	}

	if len(msg.Content.Parts) != 2 {
		t.Errorf("Expected 2 parts, got %d", len(msg.Content.Parts))
	}

	if msg.Content.Parts[0].Type != "text" {
		t.Errorf("First part type = %q, want %q", msg.Content.Parts[0].Type, "text")
	}

	if msg.Content.Parts[1].Type != "image" {
		t.Errorf("Second part type = %q, want %q", msg.Content.Parts[1].Type, "image")
	}

	if msg.Content.Parts[1].Image.URL != "https://example.com/image.jpg" {
		t.Errorf("Image URL = %q, want %q", msg.Content.Parts[1].Image.URL, "https://example.com/image.jpg")
	}
}

func TestMessageWithImageBase64(t *testing.T) {
	msg := UserMessage("Describe this").WithImageBase64("base64data", "image/png")

	if !msg.Content.IsMultimodal() {
		t.Error("Expected multimodal content after WithImageBase64")
	}

	if len(msg.Content.Parts) != 2 {
		t.Errorf("Expected 2 parts, got %d", len(msg.Content.Parts))
	}

	imgPart := msg.Content.Parts[1]
	if imgPart.Image.Base64 != "base64data" {
		t.Errorf("Image Base64 = %q, want %q", imgPart.Image.Base64, "base64data")
	}
	if imgPart.Image.MediaType != "image/png" {
		t.Errorf("Image MediaType = %q, want %q", imgPart.Image.MediaType, "image/png")
	}
}

func TestMessageWithDetail(t *testing.T) {
	msg := UserMessage("Describe").WithImageURL("https://example.com/image.jpg").WithDetail("high")

	if msg.Content.Parts[1].Image.Detail == nil {
		t.Error("Expected detail to be set")
	}
	if *msg.Content.Parts[1].Image.Detail != "high" {
		t.Errorf("Detail = %q, want %q", *msg.Content.Parts[1].Image.Detail, "high")
	}
}

func TestMessageWithMultipleImages(t *testing.T) {
	msg := UserMessage("Compare these").
		WithImageURL("https://example.com/1.jpg").
		WithImageURL("https://example.com/2.jpg")

	if len(msg.Content.Parts) != 3 {
		t.Errorf("Expected 3 parts (1 text + 2 images), got %d", len(msg.Content.Parts))
	}

	imgCount := 0
	for _, part := range msg.Content.Parts {
		if part.Type == "image" {
			imgCount++
		}
	}
	if imgCount != 2 {
		t.Errorf("Expected 2 image parts, got %d", imgCount)
	}
}

func TestMessageContentJSON(t *testing.T) {
	t.Run("simple text marshals as string", func(t *testing.T) {
		mc := MessageContent{Text: "hello"}
		data, err := json.Marshal(mc)
		if err != nil {
			t.Fatalf("Marshal error: %v", err)
		}
		if string(data) != `"hello"` {
			t.Errorf("Marshal = %s, want %q", data, `"hello"`)
		}
	})

	t.Run("multimodal marshals as array", func(t *testing.T) {
		mc := MessageContent{
			Parts: []ContentPart{
				{Type: "text", Text: "hello"},
				{Type: "image", Image: &ImageSource{URL: "https://example.com/img.jpg"}},
			},
		}
		data, err := json.Marshal(mc)
		if err != nil {
			t.Fatalf("Marshal error: %v", err)
		}
		// Should be an array, not a string
		if data[0] != '[' {
			t.Errorf("Expected array, got %s", string(data))
		}
	})

	t.Run("unmarshal string", func(t *testing.T) {
		var mc MessageContent
		err := json.Unmarshal([]byte(`"hello"`), &mc)
		if err != nil {
			t.Fatalf("Unmarshal error: %v", err)
		}
		if mc.Text != "hello" {
			t.Errorf("Text = %q, want %q", mc.Text, "hello")
		}
	})

	t.Run("unmarshal array", func(t *testing.T) {
		var mc MessageContent
		err := json.Unmarshal([]byte(`[{"type":"text","text":"hello"}]`), &mc)
		if err != nil {
			t.Fatalf("Unmarshal error: %v", err)
		}
		if len(mc.Parts) != 1 {
			t.Errorf("Parts len = %d, want 1", len(mc.Parts))
		}
	})
}

func TestMessageContentGetText(t *testing.T) {
	t.Run("simple text", func(t *testing.T) {
		mc := MessageContent{Text: "hello"}
		if mc.GetText() != "hello" {
			t.Errorf("GetText() = %q, want %q", mc.GetText(), "hello")
		}
	})

	t.Run("multimodal with text", func(t *testing.T) {
		mc := MessageContent{
			Parts: []ContentPart{
				{Type: "text", Text: "hello"},
				{Type: "image"},
			},
		}
		if mc.GetText() != "hello" {
			t.Errorf("GetText() = %q, want %q", mc.GetText(), "hello")
		}
	})

	t.Run("multimodal without text", func(t *testing.T) {
		mc := MessageContent{
			Parts: []ContentPart{
				{Type: "image"},
			},
		}
		if mc.GetText() != "" {
			t.Errorf("GetText() = %q, want empty", mc.GetText())
		}
	})
}

func TestChatRequestJSON(t *testing.T) {
	temp := 0.7
	maxTokens := 100

	req := ChatRequest{
		Model: "gpt-4o",
		Messages: []Message{
			SystemMessage("You are helpful"),
			UserMessage("Hello"),
		},
		Temperature: &temp,
		MaxTokens:   &maxTokens,
	}

	data, err := json.Marshal(req)
	if err != nil {
		t.Fatalf("Marshal error: %v", err)
	}

	var decoded ChatRequest
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatalf("Unmarshal error: %v", err)
	}

	if decoded.Model != "gpt-4o" {
		t.Errorf("Model = %q, want %q", decoded.Model, "gpt-4o")
	}
	if len(decoded.Messages) != 2 {
		t.Errorf("Messages len = %d, want 2", len(decoded.Messages))
	}
	if decoded.Temperature == nil || *decoded.Temperature != 0.7 {
		t.Errorf("Temperature = %v, want 0.7", decoded.Temperature)
	}
}

func TestChatResponseJSON(t *testing.T) {
	reason := FinishReasonStop
	resp := ChatResponse{
		Content:      "Hello!",
		Model:        "gpt-4o",
		FinishReason: &reason,
		Usage: TokenUsage{
			Estimated: TokenCount{
				PromptTokens:     10,
				CompletionTokens: 5,
			},
			IsComplete: true,
		},
	}

	data, err := json.Marshal(resp)
	if err != nil {
		t.Fatalf("Marshal error: %v", err)
	}

	var decoded ChatResponse
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatalf("Unmarshal error: %v", err)
	}

	if decoded.Content != "Hello!" {
		t.Errorf("Content = %q, want %q", decoded.Content, "Hello!")
	}
	if decoded.FinishReason == nil || *decoded.FinishReason != FinishReasonStop {
		t.Errorf("FinishReason = %v, want %v", decoded.FinishReason, FinishReasonStop)
	}
}

func TestImageSource(t *testing.T) {
	t.Run("URL variant", func(t *testing.T) {
		img := ImageSource{URL: "https://example.com/image.jpg"}
		if img.URL != "https://example.com/image.jpg" {
			t.Errorf("URL = %q, want %q", img.URL, "https://example.com/image.jpg")
		}
		if img.Base64 != "" {
			t.Error("Base64 should be empty for URL variant")
		}
	})

	t.Run("Base64 variant", func(t *testing.T) {
		img := ImageSource{Base64: "abc123", MediaType: "image/png"}
		if img.Base64 != "abc123" {
			t.Errorf("Base64 = %q, want %q", img.Base64, "abc123")
		}
		if img.MediaType != "image/png" {
			t.Errorf("MediaType = %q, want %q", img.MediaType, "image/png")
		}
	})

	t.Run("with detail", func(t *testing.T) {
		detail := "high"
		img := ImageSource{URL: "https://example.com/image.jpg", Detail: &detail}
		if img.URL != "https://example.com/image.jpg" {
			t.Errorf("URL = %q, want %q", img.URL, "https://example.com/image.jpg")
		}
		if img.Detail == nil || *img.Detail != "high" {
			t.Errorf("Detail = %v, want %q", img.Detail, "high")
		}
	})
}

func TestContentPart(t *testing.T) {
	t.Run("text part", func(t *testing.T) {
		part := ContentPart{Type: "text", Text: "hello"}
		if part.Type != "text" {
			t.Errorf("Type = %q, want %q", part.Type, "text")
		}
		if part.Text != "hello" {
			t.Errorf("Text = %q, want %q", part.Text, "hello")
		}
	})

	t.Run("image part", func(t *testing.T) {
		part := ContentPart{Type: "image", Image: &ImageSource{URL: "https://example.com/img.jpg"}}
		if part.Type != "image" {
			t.Errorf("Type = %q, want %q", part.Type, "image")
		}
		if part.Image == nil {
			t.Error("Image should not be nil")
		}
	})
}

// Tests for ResponseFormat types (T026)

func TestResponseFormatConstructors(t *testing.T) {
	t.Run("ResponseFormatText", func(t *testing.T) {
		rf := ResponseFormatText()
		if rf.Type != "text" {
			t.Errorf("Type = %q, want %q", rf.Type, "text")
		}
		if rf.JSONSchema != nil {
			t.Error("JSONSchema should be nil for text format")
		}
	})

	t.Run("ResponseFormatJSON", func(t *testing.T) {
		rf := ResponseFormatJSON()
		if rf.Type != "json_object" {
			t.Errorf("Type = %q, want %q", rf.Type, "json_object")
		}
		if rf.JSONSchema != nil {
			t.Error("JSONSchema should be nil for json_object format")
		}
	})

	t.Run("ResponseFormatJSONSchema", func(t *testing.T) {
		schema := map[string]any{
			"type": "object",
			"properties": map[string]any{
				"name": map[string]any{"type": "string"},
			},
		}
		rf := ResponseFormatJSONSchema("test_schema", schema)
		if rf.Type != "json_schema" {
			t.Errorf("Type = %q, want %q", rf.Type, "json_schema")
		}
		if rf.JSONSchema == nil {
			t.Fatal("JSONSchema should not be nil")
		}
		if rf.JSONSchema.Name != "test_schema" {
			t.Errorf("JSONSchema.Name = %q, want %q", rf.JSONSchema.Name, "test_schema")
		}
		if !rf.JSONSchema.Strict {
			t.Error("JSONSchema.Strict should be true by default")
		}
	})
}

func TestResponseFormatJSON_Serialization(t *testing.T) {
	t.Run("text format marshals correctly", func(t *testing.T) {
		rf := ResponseFormatText()
		data, err := json.Marshal(rf)
		if err != nil {
			t.Fatalf("Marshal error: %v", err)
		}

		var decoded ResponseFormat
		if err := json.Unmarshal(data, &decoded); err != nil {
			t.Fatalf("Unmarshal error: %v", err)
		}
		if decoded.Type != "text" {
			t.Errorf("Type = %q, want %q", decoded.Type, "text")
		}
	})

	t.Run("json_schema format marshals with schema", func(t *testing.T) {
		schema := map[string]any{"type": "object"}
		rf := ResponseFormatJSONSchema("my_schema", schema)
		data, err := json.Marshal(rf)
		if err != nil {
			t.Fatalf("Marshal error: %v", err)
		}

		var decoded ResponseFormat
		if err := json.Unmarshal(data, &decoded); err != nil {
			t.Fatalf("Unmarshal error: %v", err)
		}
		if decoded.JSONSchema == nil {
			t.Fatal("JSONSchema should not be nil after unmarshal")
		}
		if decoded.JSONSchema.Name != "my_schema" {
			t.Errorf("JSONSchema.Name = %q, want %q", decoded.JSONSchema.Name, "my_schema")
		}
	})
}

// Tests for Tool types (T027)

func TestNewTool(t *testing.T) {
	params := map[string]any{
		"type": "object",
		"properties": map[string]any{
			"location": map[string]any{"type": "string"},
		},
	}
	tool := NewTool("get_weather", "Get weather for a location", params)

	if tool.Type != "function" {
		t.Errorf("Type = %q, want %q", tool.Type, "function")
	}
	if tool.Function.Name != "get_weather" {
		t.Errorf("Function.Name = %q, want %q", tool.Function.Name, "get_weather")
	}
	if tool.Function.Description != "Get weather for a location" {
		t.Errorf("Function.Description = %q, want %q", tool.Function.Description, "Get weather for a location")
	}
	if tool.Function.Parameters == nil {
		t.Error("Function.Parameters should not be nil")
	}
}

func TestTool_Serialization(t *testing.T) {
	params := map[string]any{
		"type": "object",
		"properties": map[string]any{
			"query": map[string]any{"type": "string"},
		},
		"required": []string{"query"},
	}
	tool := NewTool("search", "Search the web", params)

	data, err := json.Marshal(tool)
	if err != nil {
		t.Fatalf("Marshal error: %v", err)
	}

	var decoded Tool
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatalf("Unmarshal error: %v", err)
	}

	if decoded.Type != "function" {
		t.Errorf("Type = %q, want %q", decoded.Type, "function")
	}
	if decoded.Function.Name != "search" {
		t.Errorf("Function.Name = %q, want %q", decoded.Function.Name, "search")
	}
}

// Tests for ToolChoice types (T028)

func TestToolChoiceConstructors(t *testing.T) {
	t.Run("ToolChoiceAuto", func(t *testing.T) {
		tc := ToolChoiceAuto()
		if tc.Type != "auto" {
			t.Errorf("Type = %q, want %q", tc.Type, "auto")
		}
		if tc.Function != nil {
			t.Error("Function should be nil for auto")
		}
	})

	t.Run("ToolChoiceNone", func(t *testing.T) {
		tc := ToolChoiceNone()
		if tc.Type != "none" {
			t.Errorf("Type = %q, want %q", tc.Type, "none")
		}
	})

	t.Run("ToolChoiceRequired", func(t *testing.T) {
		tc := ToolChoiceRequired()
		if tc.Type != "required" {
			t.Errorf("Type = %q, want %q", tc.Type, "required")
		}
	})

	t.Run("ToolChoiceFunc", func(t *testing.T) {
		tc := ToolChoiceFunc("get_weather")
		if tc.Type != "function" {
			t.Errorf("Type = %q, want %q", tc.Type, "function")
		}
		if tc.Function == nil {
			t.Fatal("Function should not be nil")
		}
		if tc.Function.Name != "get_weather" {
			t.Errorf("Function.Name = %q, want %q", tc.Function.Name, "get_weather")
		}
	})
}

func TestToolChoice_Serialization(t *testing.T) {
	t.Run("auto serializes correctly", func(t *testing.T) {
		tc := ToolChoiceAuto()
		data, err := json.Marshal(tc)
		if err != nil {
			t.Fatalf("Marshal error: %v", err)
		}

		var decoded ToolChoice
		if err := json.Unmarshal(data, &decoded); err != nil {
			t.Fatalf("Unmarshal error: %v", err)
		}
		if decoded.Type != "auto" {
			t.Errorf("Type = %q, want %q", decoded.Type, "auto")
		}
	})

	t.Run("function serializes with name", func(t *testing.T) {
		tc := ToolChoiceFunc("my_func")
		data, err := json.Marshal(tc)
		if err != nil {
			t.Fatalf("Marshal error: %v", err)
		}

		var decoded ToolChoice
		if err := json.Unmarshal(data, &decoded); err != nil {
			t.Fatalf("Unmarshal error: %v", err)
		}
		if decoded.Function == nil {
			t.Fatal("Function should not be nil")
		}
		if decoded.Function.Name != "my_func" {
			t.Errorf("Function.Name = %q, want %q", decoded.Function.Name, "my_func")
		}
	})
}

// Tests for ChatRequest new fields serialization (T029)

func TestChatRequestNewFields_Serialization(t *testing.T) {
	t.Run("with ResponseFormat", func(t *testing.T) {
		req := ChatRequest{
			Model:          "gpt-4o",
			ResponseFormat: ResponseFormatJSON(),
		}
		data, err := json.Marshal(req)
		if err != nil {
			t.Fatalf("Marshal error: %v", err)
		}

		var decoded ChatRequest
		if err := json.Unmarshal(data, &decoded); err != nil {
			t.Fatalf("Unmarshal error: %v", err)
		}
		if decoded.ResponseFormat == nil {
			t.Fatal("ResponseFormat should not be nil")
		}
		if decoded.ResponseFormat.Type != "json_object" {
			t.Errorf("ResponseFormat.Type = %q, want %q", decoded.ResponseFormat.Type, "json_object")
		}
	})

	t.Run("with Tools", func(t *testing.T) {
		tool := NewTool("test_func", "A test function", nil)
		req := ChatRequest{
			Model: "gpt-4o",
			Tools: []Tool{tool},
		}
		data, err := json.Marshal(req)
		if err != nil {
			t.Fatalf("Marshal error: %v", err)
		}

		var decoded ChatRequest
		if err := json.Unmarshal(data, &decoded); err != nil {
			t.Fatalf("Unmarshal error: %v", err)
		}
		if len(decoded.Tools) != 1 {
			t.Fatalf("Tools len = %d, want 1", len(decoded.Tools))
		}
		if decoded.Tools[0].Function.Name != "test_func" {
			t.Errorf("Tools[0].Function.Name = %q, want %q", decoded.Tools[0].Function.Name, "test_func")
		}
	})

	t.Run("with ToolChoice", func(t *testing.T) {
		req := ChatRequest{
			Model:      "gpt-4o",
			ToolChoice: ToolChoiceRequired(),
		}
		data, err := json.Marshal(req)
		if err != nil {
			t.Fatalf("Marshal error: %v", err)
		}

		var decoded ChatRequest
		if err := json.Unmarshal(data, &decoded); err != nil {
			t.Fatalf("Unmarshal error: %v", err)
		}
		if decoded.ToolChoice == nil {
			t.Fatal("ToolChoice should not be nil")
		}
		if decoded.ToolChoice.Type != "required" {
			t.Errorf("ToolChoice.Type = %q, want %q", decoded.ToolChoice.Type, "required")
		}
	})

	t.Run("with TopK", func(t *testing.T) {
		topK := 40
		req := ChatRequest{
			Model: "claude-sonnet-4-20250514",
			TopK:  &topK,
		}
		data, err := json.Marshal(req)
		if err != nil {
			t.Fatalf("Marshal error: %v", err)
		}

		var decoded ChatRequest
		if err := json.Unmarshal(data, &decoded); err != nil {
			t.Fatalf("Unmarshal error: %v", err)
		}
		if decoded.TopK == nil {
			t.Fatal("TopK should not be nil")
		}
		if *decoded.TopK != 40 {
			t.Errorf("TopK = %d, want 40", *decoded.TopK)
		}
	})

	t.Run("with MinP", func(t *testing.T) {
		minP := 0.05
		req := ChatRequest{
			Model: "llama3:latest",
			MinP:  &minP,
		}
		data, err := json.Marshal(req)
		if err != nil {
			t.Fatalf("Marshal error: %v", err)
		}

		var decoded ChatRequest
		if err := json.Unmarshal(data, &decoded); err != nil {
			t.Fatalf("Unmarshal error: %v", err)
		}
		if decoded.MinP == nil {
			t.Fatal("MinP should not be nil")
		}
		if *decoded.MinP != 0.05 {
			t.Errorf("MinP = %f, want 0.05", *decoded.MinP)
		}
	})
}

func TestPenaltyRange(t *testing.T) {
	pr := PenaltyRange{Min: -2.0, Max: 2.0}
	if pr.Min != -2.0 {
		t.Errorf("Min = %f, want -2.0", pr.Min)
	}
	if pr.Max != 2.0 {
		t.Errorf("Max = %f, want 2.0", pr.Max)
	}
}

// -----------------------------------------------------------------------------
// Tests for InferenceStep (T004)
// -----------------------------------------------------------------------------

func TestNewInferenceStep(t *testing.T) {
	step := NewInferenceStep("tool_call", "get_weather")
	if step.StepType != "tool_call" {
		t.Errorf("StepType = %q, want %q", step.StepType, "tool_call")
	}
	if step.Identifier != "get_weather" {
		t.Errorf("Identifier = %q, want %q", step.Identifier, "get_weather")
	}
	if step.Details != nil {
		t.Error("Details should be nil for basic constructor")
	}
}

func TestInferenceStepToolCall(t *testing.T) {
	args := map[string]any{"location": "Seattle", "units": "celsius"}
	step := InferenceStepToolCall("get_weather", args)

	if step.StepType != "tool_call" {
		t.Errorf("StepType = %q, want %q", step.StepType, "tool_call")
	}
	if step.Identifier != "get_weather" {
		t.Errorf("Identifier = %q, want %q", step.Identifier, "get_weather")
	}
	if step.Details == nil {
		t.Fatal("Details should not be nil for tool call")
	}
	if step.Details["arguments"] == nil {
		t.Error("Details should contain 'arguments'")
	}
	argsVal, ok := step.Details["arguments"].(map[string]any)
	if !ok {
		t.Fatal("arguments should be map[string]any")
	}
	if argsVal["location"] != "Seattle" {
		t.Errorf("arguments.location = %v, want %q", argsVal["location"], "Seattle")
	}
}

func TestInferenceStepThinking(t *testing.T) {
	step := InferenceStepThinking("Let me think about this...")

	if step.StepType != "thinking" {
		t.Errorf("StepType = %q, want %q", step.StepType, "thinking")
	}
	if step.Identifier != "thinking" {
		t.Errorf("Identifier = %q, want %q", step.Identifier, "thinking")
	}
	if step.Details == nil {
		t.Fatal("Details should not be nil for thinking step")
	}
	if step.Details["content"] != "Let me think about this..." {
		t.Errorf("Details.content = %v, want %q", step.Details["content"], "Let me think about this...")
	}
}

func TestInferenceStepWithDetails(t *testing.T) {
	step := NewInferenceStep("custom", "my_step")
	details := map[string]any{"key": "value", "count": 42}
	stepWithDetails := step.WithDetails(details)

	// Original should be unchanged (immutable pattern)
	if step.Details != nil {
		t.Error("Original step should still have nil Details")
	}

	// New step should have details
	if stepWithDetails.Details == nil {
		t.Fatal("New step should have Details")
	}
	if stepWithDetails.Details["key"] != "value" {
		t.Errorf("Details.key = %v, want %q", stepWithDetails.Details["key"], "value")
	}
	if stepWithDetails.Details["count"] != 42 {
		t.Errorf("Details.count = %v, want 42", stepWithDetails.Details["count"])
	}
}

func TestInferenceStep_JSON(t *testing.T) {
	step := InferenceStepToolCall("search", map[string]any{"query": "golang"})

	data, err := json.Marshal(step)
	if err != nil {
		t.Fatalf("Marshal error: %v", err)
	}

	var decoded InferenceStep
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatalf("Unmarshal error: %v", err)
	}

	if decoded.StepType != "tool_call" {
		t.Errorf("StepType = %q, want %q", decoded.StepType, "tool_call")
	}
	if decoded.Identifier != "search" {
		t.Errorf("Identifier = %q, want %q", decoded.Identifier, "search")
	}
}

// -----------------------------------------------------------------------------
// Tests for InferenceMetadata (T005)
// -----------------------------------------------------------------------------

func TestNewInferenceMetadata(t *testing.T) {
	meta := NewInferenceMetadata()

	if meta.IsComplete {
		t.Error("IsComplete should be false by default")
	}
	if meta.ExecutionTimeMs != nil {
		t.Error("ExecutionTimeMs should be nil by default")
	}
	if meta.FinishReason != nil {
		t.Error("FinishReason should be nil by default")
	}
	if meta.TokenUsage != nil {
		t.Error("TokenUsage should be nil by default")
	}
	if meta.ThinkingTrace != nil {
		t.Error("ThinkingTrace should be nil by default")
	}
	if meta.InferenceSteps != nil {
		t.Error("InferenceSteps should be nil by default")
	}
	if meta.ProviderMetadata != nil {
		t.Error("ProviderMetadata should be nil by default")
	}
}

func TestInferenceMetadata_Completed(t *testing.T) {
	meta := NewInferenceMetadata().Completed(FinishReasonStop)

	if !meta.IsComplete {
		t.Error("IsComplete should be true after Completed()")
	}
	if meta.FinishReason == nil {
		t.Fatal("FinishReason should not be nil")
	}
	if *meta.FinishReason != FinishReasonStop {
		t.Errorf("FinishReason = %v, want %v", *meta.FinishReason, FinishReasonStop)
	}
}

func TestInferenceMetadata_Incomplete(t *testing.T) {
	meta := NewInferenceMetadata().Incomplete(FinishReasonLength)

	if meta.IsComplete {
		t.Error("IsComplete should be false after Incomplete()")
	}
	if meta.FinishReason == nil {
		t.Fatal("FinishReason should not be nil")
	}
	if *meta.FinishReason != FinishReasonLength {
		t.Errorf("FinishReason = %v, want %v", *meta.FinishReason, FinishReasonLength)
	}
}

func TestInferenceMetadata_WithExecutionTime(t *testing.T) {
	meta := NewInferenceMetadata().WithExecutionTime(1500)

	if meta.ExecutionTimeMs == nil {
		t.Fatal("ExecutionTimeMs should not be nil")
	}
	if *meta.ExecutionTimeMs != 1500 {
		t.Errorf("ExecutionTimeMs = %d, want 1500", *meta.ExecutionTimeMs)
	}
}

func TestInferenceMetadata_WithTokenUsage(t *testing.T) {
	usage := TokenUsage{
		Actual: &TokenCount{
			PromptTokens:     100,
			CompletionTokens: 50,
		},
		IsComplete: true,
	}
	meta := NewInferenceMetadata().WithTokenUsage(usage)

	if meta.TokenUsage == nil {
		t.Fatal("TokenUsage should not be nil")
	}
	if meta.TokenUsage.Actual == nil {
		t.Fatal("TokenUsage.Actual should not be nil")
	}
	if meta.TokenUsage.Actual.PromptTokens != 100 {
		t.Errorf("PromptTokens = %d, want 100", meta.TokenUsage.Actual.PromptTokens)
	}
	if meta.TokenUsage.Actual.CompletionTokens != 50 {
		t.Errorf("CompletionTokens = %d, want 50", meta.TokenUsage.Actual.CompletionTokens)
	}
}

func TestInferenceMetadata_WithThinkingTrace(t *testing.T) {
	meta := NewInferenceMetadata().WithThinkingTrace("I need to analyze this problem...")

	if meta.ThinkingTrace == nil {
		t.Fatal("ThinkingTrace should not be nil")
	}
	if *meta.ThinkingTrace != "I need to analyze this problem..." {
		t.Errorf("ThinkingTrace = %q, want %q", *meta.ThinkingTrace, "I need to analyze this problem...")
	}
}

func TestInferenceMetadata_WithInferenceSteps(t *testing.T) {
	steps := []InferenceStep{
		InferenceStepThinking("thinking..."),
		InferenceStepToolCall("search", map[string]any{"q": "test"}),
	}
	meta := NewInferenceMetadata().WithInferenceSteps(steps)

	if len(meta.InferenceSteps) != 2 {
		t.Fatalf("InferenceSteps len = %d, want 2", len(meta.InferenceSteps))
	}
	if meta.InferenceSteps[0].StepType != "thinking" {
		t.Errorf("InferenceSteps[0].StepType = %q, want %q", meta.InferenceSteps[0].StepType, "thinking")
	}
	if meta.InferenceSteps[1].StepType != "tool_call" {
		t.Errorf("InferenceSteps[1].StepType = %q, want %q", meta.InferenceSteps[1].StepType, "tool_call")
	}
}

func TestInferenceMetadata_AddInferenceStep(t *testing.T) {
	meta := NewInferenceMetadata().
		AddInferenceStep(InferenceStepThinking("step 1")).
		AddInferenceStep(InferenceStepToolCall("func", nil))

	if len(meta.InferenceSteps) != 2 {
		t.Fatalf("InferenceSteps len = %d, want 2", len(meta.InferenceSteps))
	}
}

func TestInferenceMetadata_WithProviderMetadata(t *testing.T) {
	provMeta := map[string]any{
		"model_version": "1.0",
		"region":        "us-east-1",
	}
	meta := NewInferenceMetadata().WithProviderMetadata(provMeta)

	if meta.ProviderMetadata == nil {
		t.Fatal("ProviderMetadata should not be nil")
	}
	if meta.ProviderMetadata["model_version"] != "1.0" {
		t.Errorf("ProviderMetadata.model_version = %v, want %q", meta.ProviderMetadata["model_version"], "1.0")
	}
}

func TestInferenceMetadata_BuilderChaining(t *testing.T) {
	usage := TokenUsage{
		Actual:     &TokenCount{PromptTokens: 10, CompletionTokens: 20},
		IsComplete: true,
	}

	meta := NewInferenceMetadata().
		Completed(FinishReasonStop).
		WithExecutionTime(500).
		WithTokenUsage(usage).
		WithThinkingTrace("thinking...").
		AddInferenceStep(InferenceStepToolCall("test", nil)).
		WithProviderMetadata(map[string]any{"key": "value"})

	if !meta.IsComplete {
		t.Error("IsComplete should be true")
	}
	if *meta.ExecutionTimeMs != 500 {
		t.Errorf("ExecutionTimeMs = %d, want 500", *meta.ExecutionTimeMs)
	}
	if meta.TokenUsage == nil {
		t.Error("TokenUsage should not be nil")
	}
	if meta.ThinkingTrace == nil || *meta.ThinkingTrace != "thinking..." {
		t.Error("ThinkingTrace not set correctly")
	}
	if len(meta.InferenceSteps) != 1 {
		t.Errorf("InferenceSteps len = %d, want 1", len(meta.InferenceSteps))
	}
	if meta.ProviderMetadata["key"] != "value" {
		t.Error("ProviderMetadata not set correctly")
	}
}

func TestInferenceMetadata_JSON(t *testing.T) {
	meta := NewInferenceMetadata().
		Completed(FinishReasonStop).
		WithExecutionTime(1000).
		AddInferenceStep(InferenceStepToolCall("search", map[string]any{"q": "test"}))

	data, err := json.Marshal(meta)
	if err != nil {
		t.Fatalf("Marshal error: %v", err)
	}

	var decoded InferenceMetadata
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatalf("Unmarshal error: %v", err)
	}

	if !decoded.IsComplete {
		t.Error("IsComplete should be true after unmarshal")
	}
	if decoded.ExecutionTimeMs == nil || *decoded.ExecutionTimeMs != 1000 {
		t.Error("ExecutionTimeMs not preserved")
	}
	if len(decoded.InferenceSteps) != 1 {
		t.Errorf("InferenceSteps len = %d, want 1", len(decoded.InferenceSteps))
	}
}

// -----------------------------------------------------------------------------
// Tests for ChatResponse.InferenceMetadata (T021)
// -----------------------------------------------------------------------------

func TestChatResponse_InferenceMetadata(t *testing.T) {
	reason := FinishReasonStop
	resp := ChatResponse{
		Content:      "Hello!",
		Model:        "test-model",
		FinishReason: &reason,
		Usage: TokenUsage{
			Actual:     &TokenCount{PromptTokens: 10, CompletionTokens: 5},
			IsComplete: true,
		},
		InferenceMetadata: NewInferenceMetadata().
			Completed(FinishReasonStop).
			WithTokenUsage(TokenUsage{
				Actual:     &TokenCount{PromptTokens: 10, CompletionTokens: 5},
				IsComplete: true,
			}),
		Metadata: map[string]any{"provider_key": "value"}, // Deprecated but still works
	}

	// Verify InferenceMetadata
	if !resp.InferenceMetadata.IsComplete {
		t.Error("InferenceMetadata.IsComplete should be true")
	}
	if resp.InferenceMetadata.FinishReason == nil {
		t.Fatal("InferenceMetadata.FinishReason should not be nil")
	}
	if *resp.InferenceMetadata.FinishReason != FinishReasonStop {
		t.Errorf("InferenceMetadata.FinishReason = %v, want %v", *resp.InferenceMetadata.FinishReason, FinishReasonStop)
	}

	// Verify backward compatibility - Metadata still works
	if resp.Metadata["provider_key"] != "value" {
		t.Errorf("Metadata.provider_key = %v, want %q", resp.Metadata["provider_key"], "value")
	}
}

func TestChatResponse_InferenceMetadata_JSON(t *testing.T) {
	reason := FinishReasonStop
	resp := ChatResponse{
		Content:      "Hello!",
		Model:        "test-model",
		FinishReason: &reason,
		InferenceMetadata: NewInferenceMetadata().
			Completed(FinishReasonStop).
			WithExecutionTime(1500),
	}

	data, err := json.Marshal(resp)
	if err != nil {
		t.Fatalf("Marshal error: %v", err)
	}

	var decoded ChatResponse
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatalf("Unmarshal error: %v", err)
	}

	if !decoded.InferenceMetadata.IsComplete {
		t.Error("InferenceMetadata.IsComplete should be true after unmarshal")
	}
	if decoded.InferenceMetadata.ExecutionTimeMs == nil {
		t.Fatal("InferenceMetadata.ExecutionTimeMs should not be nil")
	}
	if *decoded.InferenceMetadata.ExecutionTimeMs != 1500 {
		t.Errorf("InferenceMetadata.ExecutionTimeMs = %d, want 1500", *decoded.InferenceMetadata.ExecutionTimeMs)
	}
}

// -----------------------------------------------------------------------------
// Tests for Backward Compatibility (T060, T061)
// -----------------------------------------------------------------------------

func TestChatResponse_BackwardCompatibility_MetadataPopulated(t *testing.T) {
	// T060: Verify Metadata field is still populated (backward compatibility)
	reason := FinishReasonStop
	providerMeta := map[string]any{
		"provider_version": "1.0",
		"custom_key":       "custom_value",
	}

	resp := ChatResponse{
		Content:      "Test response",
		Model:        "test-model",
		FinishReason: &reason,
		Metadata:     providerMeta, // Deprecated field
		InferenceMetadata: NewInferenceMetadata().
			Completed(FinishReasonStop).
			WithProviderMetadata(providerMeta),
	}

	// Verify deprecated Metadata field is still accessible
	if resp.Metadata == nil {
		t.Fatal("Metadata field should still be populated for backward compatibility")
	}
	if resp.Metadata["provider_version"] != "1.0" {
		t.Errorf("Metadata.provider_version = %v, want %q", resp.Metadata["provider_version"], "1.0")
	}
	if resp.Metadata["custom_key"] != "custom_value" {
		t.Errorf("Metadata.custom_key = %v, want %q", resp.Metadata["custom_key"], "custom_value")
	}
}

func TestChatResponse_BackwardCompatibility_MetadataMatchesProviderMetadata(t *testing.T) {
	// T061: Verify Metadata and InferenceMetadata.ProviderMetadata contain same data
	reason := FinishReasonStop
	providerMeta := map[string]any{
		"model_info":  "gpt-4o-2024-05-13",
		"server_time": 1234567890,
		"nested": map[string]any{
			"inner_key": "inner_value",
		},
	}

	resp := ChatResponse{
		Content:      "Test response",
		Model:        "test-model",
		FinishReason: &reason,
		Metadata:     providerMeta, // Deprecated field
		InferenceMetadata: NewInferenceMetadata().
			Completed(FinishReasonStop).
			WithProviderMetadata(providerMeta),
	}

	// Verify both fields exist
	if resp.Metadata == nil {
		t.Fatal("Metadata should not be nil")
	}
	if resp.InferenceMetadata.ProviderMetadata == nil {
		t.Fatal("InferenceMetadata.ProviderMetadata should not be nil")
	}

	// Verify they contain the same keys and values
	if resp.Metadata["model_info"] != resp.InferenceMetadata.ProviderMetadata["model_info"] {
		t.Errorf("model_info mismatch: Metadata=%v, ProviderMetadata=%v",
			resp.Metadata["model_info"], resp.InferenceMetadata.ProviderMetadata["model_info"])
	}
	if resp.Metadata["server_time"] != resp.InferenceMetadata.ProviderMetadata["server_time"] {
		t.Errorf("server_time mismatch: Metadata=%v, ProviderMetadata=%v",
			resp.Metadata["server_time"], resp.InferenceMetadata.ProviderMetadata["server_time"])
	}

	// Verify nested data
	metaNested, ok1 := resp.Metadata["nested"].(map[string]any)
	provNested, ok2 := resp.InferenceMetadata.ProviderMetadata["nested"].(map[string]any)
	if !ok1 || !ok2 {
		t.Fatal("Nested maps should be accessible")
	}
	if metaNested["inner_key"] != provNested["inner_key"] {
		t.Errorf("nested.inner_key mismatch")
	}
}

func TestChatResponse_BackwardCompatibility_JSON(t *testing.T) {
	// Verify JSON serialization preserves backward compatibility
	reason := FinishReasonStop
	providerMeta := map[string]any{"key": "value"}

	resp := ChatResponse{
		Content:      "Test",
		Model:        "model",
		FinishReason: &reason,
		Metadata:     providerMeta,
		InferenceMetadata: NewInferenceMetadata().
			Completed(FinishReasonStop).
			WithProviderMetadata(providerMeta),
	}

	data, err := json.Marshal(resp)
	if err != nil {
		t.Fatalf("Marshal error: %v", err)
	}

	var decoded ChatResponse
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatalf("Unmarshal error: %v", err)
	}

	// Both should be present after round-trip
	if decoded.Metadata == nil {
		t.Error("Metadata should be preserved after JSON round-trip")
	}
	if decoded.InferenceMetadata.ProviderMetadata == nil {
		t.Error("InferenceMetadata.ProviderMetadata should be preserved after JSON round-trip")
	}
	if decoded.Metadata["key"] != "value" {
		t.Errorf("Metadata.key = %v after round-trip, want %q", decoded.Metadata["key"], "value")
	}
	if decoded.InferenceMetadata.ProviderMetadata["key"] != "value" {
		t.Errorf("ProviderMetadata.key = %v after round-trip, want %q", decoded.InferenceMetadata.ProviderMetadata["key"], "value")
	}
}
