package nxuskit

import (
	"testing"
)

func TestConvertToClaudeMessages_SimpleText(t *testing.T) {
	messages := []Message{
		UserMessage("Hello"),
		AssistantMessage("Hi there!"),
	}

	result, system := convertToClaudeMessages(messages)

	if system != "" {
		t.Errorf("expected empty system prompt, got '%s'", system)
	}
	if len(result) != 2 {
		t.Fatalf("expected 2 messages, got %d", len(result))
	}
	if result[0].Role != "user" {
		t.Errorf("expected role 'user', got '%s'", result[0].Role)
	}
	if content, ok := result[0].Content.(string); !ok || content != "Hello" {
		t.Errorf("expected content 'Hello', got %v", result[0].Content)
	}
}

func TestConvertToClaudeMessages_ExtractSystemPrompt(t *testing.T) {
	messages := []Message{
		SystemMessage("You are a helpful assistant."),
		UserMessage("Hello"),
	}

	result, system := convertToClaudeMessages(messages)

	if system != "You are a helpful assistant." {
		t.Errorf("expected system prompt extracted, got '%s'", system)
	}
	if len(result) != 1 {
		t.Fatalf("expected 1 message (system excluded), got %d", len(result))
	}
}

func TestConvertToClaudeMessages_MultipleSystemPrompts(t *testing.T) {
	messages := []Message{
		SystemMessage("Be helpful."),
		SystemMessage("Be concise."),
		UserMessage("Hello"),
	}

	result, system := convertToClaudeMessages(messages)

	expected := "Be helpful.\nBe concise."
	if system != expected {
		t.Errorf("expected concatenated system prompt '%s', got '%s'", expected, system)
	}
	if len(result) != 1 {
		t.Fatalf("expected 1 message, got %d", len(result))
	}
}

func TestConvertToClaudeMessage_WithBase64Image(t *testing.T) {
	msg := UserMessage("What's in this image?").
		WithImageBase64("aGVsbG8=", "image/png")

	cm := convertToClaudeMessage(msg)

	blocks, ok := cm.Content.([]claudeContentBlock)
	if !ok {
		t.Fatalf("expected []claudeContentBlock, got %T", cm.Content)
	}

	if len(blocks) != 2 {
		t.Fatalf("expected 2 blocks, got %d", len(blocks))
	}

	// First block should be text
	if blocks[0].Type != "text" {
		t.Errorf("expected first block type 'text', got '%s'", blocks[0].Type)
	}
	if blocks[0].Text != "What's in this image?" {
		t.Errorf("expected text content, got '%s'", blocks[0].Text)
	}

	// Second block should be image
	if blocks[1].Type != "image" {
		t.Errorf("expected second block type 'image', got '%s'", blocks[1].Type)
	}
	if blocks[1].Source == nil {
		t.Fatal("expected Source to be set")
	}
	if blocks[1].Source.Type != "base64" {
		t.Errorf("expected source type 'base64', got '%s'", blocks[1].Source.Type)
	}
	if blocks[1].Source.MediaType != "image/png" {
		t.Errorf("expected media type 'image/png', got '%s'", blocks[1].Source.MediaType)
	}
	if blocks[1].Source.Data != "aGVsbG8=" {
		t.Errorf("expected base64 data, got '%s'", blocks[1].Source.Data)
	}
}

func TestConvertToClaudeMessage_WithURLImage(t *testing.T) {
	msg := UserMessage("Describe this").
		WithImageURL("https://example.com/image.png")

	cm := convertToClaudeMessage(msg)

	blocks, ok := cm.Content.([]claudeContentBlock)
	if !ok {
		t.Fatalf("expected []claudeContentBlock, got %T", cm.Content)
	}

	if len(blocks) != 2 {
		t.Fatalf("expected 2 blocks, got %d", len(blocks))
	}

	if blocks[1].Source.Type != "url" {
		t.Errorf("expected source type 'url', got '%s'", blocks[1].Source.Type)
	}
	if blocks[1].Source.URL != "https://example.com/image.png" {
		t.Errorf("expected URL, got '%s'", blocks[1].Source.URL)
	}
}

func TestConvertToClaudeMessage_DefaultMediaType(t *testing.T) {
	msg := UserMessage("test").
		WithImageBase64("data", "")

	cm := convertToClaudeMessage(msg)

	blocks := cm.Content.([]claudeContentBlock)
	// Should default to image/png
	if blocks[1].Source.MediaType != "image/png" {
		t.Errorf("expected default media type 'image/png', got '%s'", blocks[1].Source.MediaType)
	}
}

func TestConvertThinkingModeToClaudeThinking(t *testing.T) {
	tests := []struct {
		mode     ThinkingMode
		expected *claudeThinking
	}{
		{ThinkingModeAuto, nil},
		{ThinkingModeOmit, nil},
		{ThinkingModeEnabled, &claudeThinking{Type: "enabled", BudgetTokens: claudeDefaultThinkingBudget}},
		{ThinkingModeDisabled, &claudeThinking{Type: "disabled"}},
	}

	for _, tt := range tests {
		result := convertThinkingModeToClaudeThinking(tt.mode)
		if tt.expected == nil {
			if result != nil {
				t.Errorf("expected nil for mode %v, got %+v", tt.mode, result)
			}
		} else {
			if result == nil {
				t.Errorf("expected %+v for mode %v, got nil", tt.expected, tt.mode)
			} else if result.Type != tt.expected.Type {
				t.Errorf("expected type '%s' for mode %v, got '%s'", tt.expected.Type, tt.mode, result.Type)
			}
		}
	}
}

func TestConvertClaudeStopReason(t *testing.T) {
	tests := []struct {
		input    string
		expected FinishReason
	}{
		{"end_turn", FinishReasonStop},
		{"max_tokens", FinishReasonLength},
		{"stop", FinishReasonStop},
	}

	for _, tt := range tests {
		result := convertClaudeStopReason(tt.input)
		if result != tt.expected {
			t.Errorf("for '%s': expected %v, got %v", tt.input, tt.expected, result)
		}
	}
}

func TestConvertFromClaudeResponse(t *testing.T) {
	resp := &claudeMessagesResponse{
		ID:    "msg-123",
		Type:  "message",
		Role:  "assistant",
		Model: "claude-sonnet-4-20250514",
		Content: []claudeContentBlock{
			{Type: "text", Text: "Hello!"},
		},
		StopReason: "end_turn",
		Usage: claudeUsage{
			InputTokens:  10,
			OutputTokens: 5,
		},
	}

	cr := convertFromClaudeResponse(resp)

	if cr.Content != "Hello!" {
		t.Errorf("expected content 'Hello!', got '%s'", cr.Content)
	}
	if cr.Model != "claude-sonnet-4-20250514" {
		t.Errorf("expected model 'claude-sonnet-4-20250514', got '%s'", cr.Model)
	}
	if cr.FinishReason == nil || *cr.FinishReason != FinishReasonStop {
		t.Error("expected FinishReasonStop")
	}
	if cr.Usage.Actual == nil {
		t.Fatal("expected Actual token usage")
	}
	if cr.Usage.Actual.PromptTokens != 10 {
		t.Errorf("expected 10 prompt tokens, got %d", cr.Usage.Actual.PromptTokens)
	}
	if cr.Usage.Actual.CompletionTokens != 5 {
		t.Errorf("expected 5 completion tokens, got %d", cr.Usage.Actual.CompletionTokens)
	}
}

func TestConvertFromClaudeResponse_WithThinking(t *testing.T) {
	resp := &claudeMessagesResponse{
		ID:    "msg-123",
		Model: "claude-sonnet-4-20250514",
		Content: []claudeContentBlock{
			{Type: "thinking", Thinking: "Let me think..."},
			{Type: "text", Text: "The answer is 42."},
		},
		StopReason: "end_turn",
		Usage: claudeUsage{
			InputTokens:  10,
			OutputTokens: 20,
		},
	}

	cr := convertFromClaudeResponse(resp)

	if cr.Content != "The answer is 42." {
		t.Errorf("expected content 'The answer is 42.', got '%s'", cr.Content)
	}
	if cr.Thinking == nil {
		t.Fatal("expected Thinking to be set")
	}
	if *cr.Thinking != "Let me think..." {
		t.Errorf("expected thinking content, got '%s'", *cr.Thinking)
	}
}

func TestBuildClaudeRequest(t *testing.T) {
	temp := 0.7
	maxTokens := 1000

	req := &ChatRequest{
		Model: "claude-sonnet-4-20250514",
		Messages: []Message{
			SystemMessage("Be helpful"),
			UserMessage("Hello"),
		},
		Temperature:  &temp,
		MaxTokens:    &maxTokens,
		ThinkingMode: ThinkingModeEnabled,
	}

	cr := buildClaudeRequest(req)

	if cr.Model != "claude-sonnet-4-20250514" {
		t.Errorf("expected model, got '%s'", cr.Model)
	}
	if cr.MaxTokens != 1000 {
		t.Errorf("expected max_tokens 1000, got %d", cr.MaxTokens)
	}
	if cr.System != "Be helpful" {
		t.Errorf("expected system prompt, got '%s'", cr.System)
	}
	if len(cr.Messages) != 1 {
		t.Errorf("expected 1 message (system extracted), got %d", len(cr.Messages))
	}
	if cr.Temperature == nil || *cr.Temperature != 0.7 {
		t.Error("expected temperature 0.7")
	}
	if cr.Thinking == nil {
		t.Fatal("expected Thinking to be set")
	}
	if cr.Thinking.Type != "enabled" {
		t.Errorf("expected thinking type 'enabled', got '%s'", cr.Thinking.Type)
	}
}

func TestBuildClaudeRequest_DefaultMaxTokens(t *testing.T) {
	req := &ChatRequest{
		Model: "claude-sonnet-4-20250514",
		Messages: []Message{
			UserMessage("Hello"),
		},
	}

	cr := buildClaudeRequest(req)

	if cr.MaxTokens != claudeDefaultMaxTokens {
		t.Errorf("expected default max_tokens %d, got %d", claudeDefaultMaxTokens, cr.MaxTokens)
	}
}
