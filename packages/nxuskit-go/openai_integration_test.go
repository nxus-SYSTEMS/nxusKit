//go:build integration

package nxuskit

import (
	"context"
	"os"
	"testing"
)

// Integration tests for OpenAIProvider.
// Run with: go test -tags=integration -v -run TestOpenAI
// Requires: OPENAI_API_KEY environment variable

func skipIfNoOpenAIKey(t *testing.T) {
	t.Helper()
	if os.Getenv("OPENAI_API_KEY") == "" {
		t.Skip("Skipping: OPENAI_API_KEY not set")
	}
}

func TestOpenAIProvider_Integration_Ping(t *testing.T) {
	skipIfNoOpenAIKey(t)

	provider, err := NewOpenAIProvider()
	if err != nil {
		t.Fatalf("failed to create provider: %v", err)
	}

	if err := provider.Ping(context.Background()); err != nil {
		t.Fatalf("Ping failed: %v", err)
	}
}

func TestOpenAIProvider_Integration_ListModels(t *testing.T) {
	skipIfNoOpenAIKey(t)

	provider, err := NewOpenAIProvider()
	if err != nil {
		t.Fatalf("failed to create provider: %v", err)
	}

	models, err := provider.ListModels(context.Background())
	if err != nil {
		t.Fatalf("ListModels failed: %v", err)
	}

	t.Logf("Found %d models", len(models))
	// Log first 10 models
	for i, m := range models {
		if i >= 10 {
			t.Logf("  ... and %d more", len(models)-10)
			break
		}
		t.Logf("  - %s", m.Name)
	}
}

func TestOpenAIProvider_Integration_Chat(t *testing.T) {
	skipIfNoOpenAIKey(t)

	provider, err := NewOpenAIProvider()
	if err != nil {
		t.Fatalf("failed to create provider: %v", err)
	}

	req, err := NewChatRequest("gpt-4o-mini",
		WithMessages(
			SystemMessage("You are a helpful assistant. Be very brief."),
			UserMessage("What is 2+2?"),
		),
		WithMaxTokens(50),
	)
	if err != nil {
		t.Fatalf("failed to create request: %v", err)
	}

	resp, err := provider.Chat(context.Background(), req)
	if err != nil {
		t.Fatalf("Chat failed: %v", err)
	}

	t.Logf("Response: %s", resp.Content)
	t.Logf("Model: %s", resp.Model)
	t.Logf("Finish reason: %v", resp.FinishReason)
	if resp.Usage.Actual != nil {
		t.Logf("Tokens: %d prompt, %d completion",
			resp.Usage.Actual.PromptTokens,
			resp.Usage.Actual.CompletionTokens)
	}
}

func TestOpenAIProvider_Integration_ChatStream(t *testing.T) {
	skipIfNoOpenAIKey(t)

	provider, err := NewOpenAIProvider()
	if err != nil {
		t.Fatalf("failed to create provider: %v", err)
	}

	req, err := NewChatRequest("gpt-4o-mini",
		WithMessages(UserMessage("Count from 1 to 5")),
		WithMaxTokens(50),
	)
	if err != nil {
		t.Fatalf("failed to create request: %v", err)
	}

	chunks, errs := provider.ChatStream(context.Background(), req)

	var content string
	var chunkCount int
	for chunk := range chunks {
		content += chunk.Delta
		chunkCount++
	}

	if err := <-errs; err != nil {
		t.Fatalf("Stream failed: %v", err)
	}

	t.Logf("Received %d chunks", chunkCount)
	t.Logf("Full content: %s", content)
}

func TestOpenAIProvider_Integration_JSONMode(t *testing.T) {
	skipIfNoOpenAIKey(t)

	provider, err := NewOpenAIProvider()
	if err != nil {
		t.Fatalf("failed to create provider: %v", err)
	}

	req, err := NewChatRequest("gpt-4o-mini",
		WithMessages(
			SystemMessage("You are a helpful assistant. Always respond in JSON format."),
			UserMessage("Return a JSON object with name='test' and value=42"),
		),
		WithResponseFormat(ResponseFormatJSON()),
		WithMaxTokens(100),
	)
	if err != nil {
		t.Fatalf("failed to create request: %v", err)
	}

	resp, err := provider.Chat(context.Background(), req)
	if err != nil {
		t.Fatalf("Chat failed: %v", err)
	}

	t.Logf("JSON Response: %s", resp.Content)

	// Verify it's valid JSON by checking it starts with {
	if len(resp.Content) == 0 || resp.Content[0] != '{' {
		t.Errorf("Expected JSON object, got: %s", resp.Content)
	}
}

func TestOpenAIProvider_Integration_Tools(t *testing.T) {
	skipIfNoOpenAIKey(t)

	provider, err := NewOpenAIProvider()
	if err != nil {
		t.Fatalf("failed to create provider: %v", err)
	}

	weatherTool := NewTool(
		"get_weather",
		"Get the current weather for a location",
		map[string]any{
			"type": "object",
			"properties": map[string]any{
				"location": map[string]any{
					"type":        "string",
					"description": "City name",
				},
			},
			"required": []string{"location"},
		},
	)

	req, err := NewChatRequest("gpt-4o-mini",
		WithMessages(UserMessage("What's the weather in Paris?")),
		WithTools(weatherTool),
		WithToolChoice(ToolChoiceAuto()),
		WithMaxTokens(100),
	)
	if err != nil {
		t.Fatalf("failed to create request: %v", err)
	}

	resp, err := provider.Chat(context.Background(), req)
	if err != nil {
		t.Fatalf("Chat failed: %v", err)
	}

	t.Logf("Response: %s", resp.Content)
	t.Logf("Finish reason: %v", resp.FinishReason)

	// The model may or may not call the tool
	if resp.FinishReason != nil && *resp.FinishReason == FinishReasonToolCalls {
		t.Log("Model chose to call a tool")
	}
}
