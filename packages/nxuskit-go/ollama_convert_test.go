package nxuskit

import (
	"testing"
)

func TestConvertToOllamaMessage_SimpleText(t *testing.T) {
	msg := UserMessage("Hello, world!")
	om := convertToOllamaMessage(msg)

	if om.Role != "user" {
		t.Errorf("expected role 'user', got '%s'", om.Role)
	}
	if om.Content != "Hello, world!" {
		t.Errorf("expected content 'Hello, world!', got '%s'", om.Content)
	}
	if len(om.Images) != 0 {
		t.Errorf("expected no images, got %d", len(om.Images))
	}
}

func TestConvertToOllamaMessage_AllRoles(t *testing.T) {
	tests := []struct {
		msg      Message
		expected string
	}{
		{SystemMessage("system prompt"), "system"},
		{UserMessage("user message"), "user"},
		{AssistantMessage("assistant reply"), "assistant"},
	}

	for _, tt := range tests {
		om := convertToOllamaMessage(tt.msg)
		if om.Role != tt.expected {
			t.Errorf("expected role '%s', got '%s'", tt.expected, om.Role)
		}
	}
}

func TestConvertToOllamaMessage_WithImage(t *testing.T) {
	msg := UserMessage("What's in this image?").
		WithImageBase64("aGVsbG8=", "image/png")

	om := convertToOllamaMessage(msg)

	if om.Role != "user" {
		t.Errorf("expected role 'user', got '%s'", om.Role)
	}
	if om.Content != "What's in this image?" {
		t.Errorf("expected text content, got '%s'", om.Content)
	}
	if len(om.Images) != 1 {
		t.Fatalf("expected 1 image, got %d", len(om.Images))
	}
	if om.Images[0] != "aGVsbG8=" {
		t.Errorf("expected base64 data, got '%s'", om.Images[0])
	}
}

func TestConvertToOllamaMessage_MultipleImages(t *testing.T) {
	msg := UserMessage("Compare these images").
		WithImageBase64("image1", "image/png").
		WithImageBase64("image2", "image/jpeg")

	om := convertToOllamaMessage(msg)

	if len(om.Images) != 2 {
		t.Fatalf("expected 2 images, got %d", len(om.Images))
	}
	if om.Images[0] != "image1" || om.Images[1] != "image2" {
		t.Errorf("unexpected images: %v", om.Images)
	}
}

func TestConvertToOllamaMessages(t *testing.T) {
	messages := []Message{
		SystemMessage("Be helpful"),
		UserMessage("Hello"),
		AssistantMessage("Hi there!"),
	}

	result := convertToOllamaMessages(messages)

	if len(result) != 3 {
		t.Fatalf("expected 3 messages, got %d", len(result))
	}
	if result[0].Role != "system" {
		t.Errorf("expected first message role 'system', got '%s'", result[0].Role)
	}
	if result[1].Role != "user" {
		t.Errorf("expected second message role 'user', got '%s'", result[1].Role)
	}
	if result[2].Role != "assistant" {
		t.Errorf("expected third message role 'assistant', got '%s'", result[2].Role)
	}
}

func TestConvertOllamaOptions(t *testing.T) {
	temp := 0.7
	maxTokens := 100
	topP := 0.9
	seed := 42

	req := &ChatRequest{
		Temperature: &temp,
		MaxTokens:   &maxTokens,
		TopP:        &topP,
		Seed:        &seed,
	}

	opts := convertOllamaOptions(req)

	if opts["temperature"] != 0.7 {
		t.Errorf("expected temperature 0.7, got %v", opts["temperature"])
	}
	if opts["num_predict"] != 100 {
		t.Errorf("expected num_predict 100, got %v", opts["num_predict"])
	}
	if opts["top_p"] != 0.9 {
		t.Errorf("expected top_p 0.9, got %v", opts["top_p"])
	}
	if opts["seed"] != 42 {
		t.Errorf("expected seed 42, got %v", opts["seed"])
	}
}

func TestConvertOllamaOptions_Empty(t *testing.T) {
	req := &ChatRequest{}
	opts := convertOllamaOptions(req)

	if opts != nil {
		t.Errorf("expected nil options for empty request, got %v", opts)
	}
}

func TestConvertThinkingModeToOllama(t *testing.T) {
	tests := []struct {
		mode     ThinkingMode
		expected *bool
	}{
		{ThinkingModeAuto, nil},
		{ThinkingModeEnabled, boolPtr(true)},
		{ThinkingModeDisabled, boolPtr(false)},
		{ThinkingModeOmit, nil},
	}

	for _, tt := range tests {
		result := convertThinkingModeToOllama(tt.mode)
		if tt.expected == nil {
			if result != nil {
				t.Errorf("mode %s: expected nil, got %v", tt.mode, *result)
			}
		} else {
			if result == nil {
				t.Errorf("mode %s: expected %v, got nil", tt.mode, *tt.expected)
			} else if *result != *tt.expected {
				t.Errorf("mode %s: expected %v, got %v", tt.mode, *tt.expected, *result)
			}
		}
	}
}

func TestConvertFromOllamaResponse(t *testing.T) {
	resp := &ollamaChatResponse{
		Model: "llama3.2",
		Message: ollamaMessage{
			Role:    "assistant",
			Content: "Hello!",
		},
		Done:            true,
		EvalCount:       10,
		PromptEvalCount: 5,
		Thinking:        "Let me think...",
	}

	cr := convertFromOllamaResponse(resp)

	if cr.Content != "Hello!" {
		t.Errorf("expected content 'Hello!', got '%s'", cr.Content)
	}
	if cr.Model != "llama3.2" {
		t.Errorf("expected model 'llama3.2', got '%s'", cr.Model)
	}
	if cr.FinishReason == nil || *cr.FinishReason != FinishReasonStop {
		t.Error("expected FinishReasonStop")
	}
	if cr.Usage.Actual == nil {
		t.Fatal("expected Actual token usage")
	}
	if cr.Usage.Actual.PromptTokens != 5 {
		t.Errorf("expected 5 prompt tokens, got %d", cr.Usage.Actual.PromptTokens)
	}
	if cr.Usage.Actual.CompletionTokens != 10 {
		t.Errorf("expected 10 completion tokens, got %d", cr.Usage.Actual.CompletionTokens)
	}
	if cr.Thinking == nil || *cr.Thinking != "Let me think..." {
		t.Errorf("expected thinking content, got %v", cr.Thinking)
	}
}

func TestConvertFromOllamaResponse_NotDone(t *testing.T) {
	resp := &ollamaChatResponse{
		Model: "llama3.2",
		Message: ollamaMessage{
			Role:    "assistant",
			Content: "partial",
		},
		Done: false,
	}

	cr := convertFromOllamaResponse(resp)

	if cr.FinishReason != nil {
		t.Errorf("expected nil FinishReason for non-done response, got %v", *cr.FinishReason)
	}
}

func TestConvertFromOllamaModelInfo(t *testing.T) {
	info := ollamaModelInfo{
		Name:       "llama3.2:latest",
		Size:       4109853696,
		Digest:     "sha256:abc123",
		ModifiedAt: "2024-01-15T10:30:00Z",
	}

	mi := convertFromOllamaModelInfo(info)

	if mi.Name != "llama3.2:latest" {
		t.Errorf("expected name 'llama3.2:latest', got '%s'", mi.Name)
	}
	if mi.SizeBytes == nil || *mi.SizeBytes != 4109853696 {
		t.Errorf("expected size 4109853696, got %v", mi.SizeBytes)
	}
	if mi.Metadata["digest"] != "sha256:abc123" {
		t.Errorf("expected digest in metadata")
	}
}

// helper function
func boolPtr(b bool) *bool {
	return &b
}
