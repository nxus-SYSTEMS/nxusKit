package nxuskit

import (
	"testing"
)

func TestConvertToOpenAIMessage_SimpleText(t *testing.T) {
	msg := UserMessage("Hello, world!")
	om := convertToOpenAIMessage(msg)

	if om.Role != "user" {
		t.Errorf("expected role 'user', got '%s'", om.Role)
	}
	content, ok := om.Content.(string)
	if !ok {
		t.Fatalf("expected string content, got %T", om.Content)
	}
	if content != "Hello, world!" {
		t.Errorf("expected content 'Hello, world!', got '%s'", content)
	}
}

func TestConvertToOpenAIMessage_AllRoles(t *testing.T) {
	tests := []struct {
		msg      Message
		expected string
	}{
		{SystemMessage("system prompt"), "system"},
		{UserMessage("user message"), "user"},
		{AssistantMessage("assistant reply"), "assistant"},
	}

	for _, tt := range tests {
		om := convertToOpenAIMessage(tt.msg)
		if om.Role != tt.expected {
			t.Errorf("expected role '%s', got '%s'", tt.expected, om.Role)
		}
	}
}

func TestConvertToOpenAIMessage_WithBase64Image(t *testing.T) {
	msg := UserMessage("What's in this image?").
		WithImageBase64("aGVsbG8=", "image/png")

	om := convertToOpenAIMessage(msg)

	parts, ok := om.Content.([]openaiContentPart)
	if !ok {
		t.Fatalf("expected []openaiContentPart, got %T", om.Content)
	}

	if len(parts) != 2 {
		t.Fatalf("expected 2 parts, got %d", len(parts))
	}

	// First part should be text
	if parts[0].Type != "text" {
		t.Errorf("expected first part type 'text', got '%s'", parts[0].Type)
	}
	if parts[0].Text != "What's in this image?" {
		t.Errorf("expected text content, got '%s'", parts[0].Text)
	}

	// Second part should be image
	if parts[1].Type != "image_url" {
		t.Errorf("expected second part type 'image_url', got '%s'", parts[1].Type)
	}
	if parts[1].ImageURL == nil {
		t.Fatal("expected ImageURL to be set")
	}
	expectedURL := "data:image/png;base64,aGVsbG8="
	if parts[1].ImageURL.URL != expectedURL {
		t.Errorf("expected URL '%s', got '%s'", expectedURL, parts[1].ImageURL.URL)
	}
}

func TestConvertToOpenAIMessage_WithURLImage(t *testing.T) {
	msg := UserMessage("Describe this").
		WithImageURL("https://example.com/image.png")

	om := convertToOpenAIMessage(msg)

	parts, ok := om.Content.([]openaiContentPart)
	if !ok {
		t.Fatalf("expected []openaiContentPart, got %T", om.Content)
	}

	if len(parts) != 2 {
		t.Fatalf("expected 2 parts, got %d", len(parts))
	}

	if parts[1].ImageURL.URL != "https://example.com/image.png" {
		t.Errorf("expected URL, got '%s'", parts[1].ImageURL.URL)
	}
}

func TestConvertToOpenAIMessage_DefaultMediaType(t *testing.T) {
	msg := UserMessage("test").
		WithImageBase64("data", "")

	om := convertToOpenAIMessage(msg)

	parts, ok := om.Content.([]openaiContentPart)
	if !ok {
		t.Fatalf("expected []openaiContentPart, got %T", om.Content)
	}

	// Should default to image/png
	if parts[1].ImageURL.URL != "data:image/png;base64,data" {
		t.Errorf("expected default media type, got '%s'", parts[1].ImageURL.URL)
	}
}

func TestConvertToOpenAIMessages(t *testing.T) {
	messages := []Message{
		SystemMessage("Be helpful"),
		UserMessage("Hello"),
		AssistantMessage("Hi there!"),
	}

	result := convertToOpenAIMessages(messages)

	if len(result) != 3 {
		t.Fatalf("expected 3 messages, got %d", len(result))
	}
	if result[0].Role != "system" {
		t.Errorf("expected first message role 'system', got '%s'", result[0].Role)
	}
}

func TestConvertOpenAIFinishReason(t *testing.T) {
	tests := []struct {
		input    *string
		expected *FinishReason
	}{
		{nil, nil},
		{strPtr("stop"), finishReasonPtr(FinishReasonStop)},
		{strPtr("length"), finishReasonPtr(FinishReasonLength)},
		{strPtr("content_filter"), finishReasonPtr(FinishReasonContentFilter)},
		{strPtr("tool_calls"), finishReasonPtr(FinishReasonToolCalls)},
	}

	for _, tt := range tests {
		result := convertOpenAIFinishReason(tt.input)
		if tt.expected == nil {
			if result != nil {
				t.Errorf("expected nil for nil input, got %v", *result)
			}
		} else {
			if result == nil {
				t.Errorf("expected %v, got nil", *tt.expected)
			} else if *result != *tt.expected {
				t.Errorf("expected %v, got %v", *tt.expected, *result)
			}
		}
	}
}

func TestConvertFromOpenAIResponse(t *testing.T) {
	stopReason := "stop"
	resp := &openaiChatResponse{
		ID:      "chatcmpl-123",
		Model:   "local-model",
		Created: 1234567890,
		Choices: []openaiChoice{
			{
				Index: 0,
				Message: &openaiMessage{
					Role:    "assistant",
					Content: "Hello!",
				},
				FinishReason: &stopReason,
			},
		},
		Usage: &openaiUsage{
			PromptTokens:     10,
			CompletionTokens: 5,
			TotalTokens:      15,
		},
	}

	cr := convertFromOpenAIResponse(resp)

	if cr.Content != "Hello!" {
		t.Errorf("expected content 'Hello!', got '%s'", cr.Content)
	}
	if cr.Model != "local-model" {
		t.Errorf("expected model 'local-model', got '%s'", cr.Model)
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

func TestConvertFromOpenAIResponse_NoChoices(t *testing.T) {
	resp := &openaiChatResponse{
		ID:      "chatcmpl-123",
		Model:   "local-model",
		Choices: []openaiChoice{},
	}

	cr := convertFromOpenAIResponse(resp)

	if cr.Content != "" {
		t.Errorf("expected empty content, got '%s'", cr.Content)
	}
}

func TestConvertFromOpenAIModelInfo(t *testing.T) {
	info := openaiModelInfo{
		ID:      "TheBloke/Mistral-7B-v0.1-GGUF",
		Object:  "model",
		Created: 1234567890,
		OwnedBy: "local",
	}

	mi := convertFromOpenAIModelInfo(info)

	if mi.Name != "TheBloke/Mistral-7B-v0.1-GGUF" {
		t.Errorf("expected name 'TheBloke/Mistral-7B-v0.1-GGUF', got '%s'", mi.Name)
	}
	if mi.Metadata["owned_by"] != "local" {
		t.Errorf("expected owned_by in metadata")
	}
}

// helper functions
func strPtr(s string) *string {
	return &s
}

func finishReasonPtr(fr FinishReason) *FinishReason {
	return &fr
}
