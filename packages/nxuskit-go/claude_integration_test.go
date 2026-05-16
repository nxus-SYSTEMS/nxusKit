//go:build integration

package nxuskit

import (
	"context"
	"os"
	"testing"
)

// Integration tests for ClaudeProvider.
// Run with: go test -tags=integration -v -run TestClaude
// Requires: ANTHROPIC_API_KEY environment variable

func skipIfNoClaudeKey(t *testing.T) {
	t.Helper()
	if os.Getenv("ANTHROPIC_API_KEY") == "" {
		t.Skip("Skipping: ANTHROPIC_API_KEY not set")
	}
}

func TestClaudeProvider_Integration_Chat(t *testing.T) {
	skipIfNoClaudeKey(t)

	provider, err := NewClaudeProvider()
	if err != nil {
		t.Fatalf("failed to create provider: %v", err)
	}

	req, err := NewChatRequest("claude-sonnet-4-20250514",
		WithMessages(
			UserMessage("What is 2+2? Answer with just the number."),
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

func TestClaudeProvider_Integration_ChatStream(t *testing.T) {
	skipIfNoClaudeKey(t)

	provider, err := NewClaudeProvider()
	if err != nil {
		t.Fatalf("failed to create provider: %v", err)
	}

	req, err := NewChatRequest("claude-sonnet-4-20250514",
		WithMessages(UserMessage("Count from 1 to 5, one number per line.")),
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

func TestClaudeProvider_Integration_TopK(t *testing.T) {
	skipIfNoClaudeKey(t)

	provider, err := NewClaudeProvider()
	if err != nil {
		t.Fatalf("failed to create provider: %v", err)
	}

	// Claude supports TopK
	req, err := NewChatRequest("claude-sonnet-4-20250514",
		WithMessages(UserMessage("Say hello")),
		WithTopK(40),
		WithMaxTokens(50),
	)
	if err != nil {
		t.Fatalf("failed to create request: %v", err)
	}

	resp, err := provider.Chat(context.Background(), req)
	if err != nil {
		t.Fatalf("Chat with TopK failed: %v", err)
	}

	t.Logf("Response with TopK=40: %s", resp.Content)
}

func TestClaudeProvider_Integration_SystemMessage(t *testing.T) {
	skipIfNoClaudeKey(t)

	provider, err := NewClaudeProvider()
	if err != nil {
		t.Fatalf("failed to create provider: %v", err)
	}

	req, err := NewChatRequest("claude-sonnet-4-20250514",
		WithMessages(
			SystemMessage("You are a pirate. Always respond like a pirate."),
			UserMessage("Say hello"),
		),
		WithMaxTokens(100),
	)
	if err != nil {
		t.Fatalf("failed to create request: %v", err)
	}

	resp, err := provider.Chat(context.Background(), req)
	if err != nil {
		t.Fatalf("Chat failed: %v", err)
	}

	t.Logf("Pirate response: %s", resp.Content)
}

func TestClaudeProvider_Integration_Tools(t *testing.T) {
	skipIfNoClaudeKey(t)

	provider, err := NewClaudeProvider()
	if err != nil {
		t.Fatalf("failed to create provider: %v", err)
	}

	calculatorTool := NewTool(
		"calculate",
		"Perform a mathematical calculation",
		map[string]any{
			"type": "object",
			"properties": map[string]any{
				"expression": map[string]any{
					"type":        "string",
					"description": "Mathematical expression to evaluate",
				},
			},
			"required": []string{"expression"},
		},
	)

	req, err := NewChatRequest("claude-sonnet-4-20250514",
		WithMessages(UserMessage("What is 15 * 23?")),
		WithTools(calculatorTool),
		WithToolChoice(ToolChoiceAuto()),
		WithMaxTokens(200),
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
}
