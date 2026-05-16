//go:build integration

package nxuskit

import (
	"context"
	"os"
	"testing"
)

// Integration tests for GroqProvider.
// Run with: go test -tags=integration -v -run TestGroq
// Requires: GROQ_API_KEY environment variable

func skipIfNoGroqKey(t *testing.T) {
	t.Helper()
	if os.Getenv("GROQ_API_KEY") == "" {
		t.Skip("Skipping: GROQ_API_KEY not set")
	}
}

func TestGroqProvider_Integration_Ping(t *testing.T) {
	skipIfNoGroqKey(t)

	provider, err := NewGroqProvider()
	if err != nil {
		t.Fatalf("failed to create provider: %v", err)
	}

	if err := provider.Ping(context.Background()); err != nil {
		t.Fatalf("Ping failed: %v", err)
	}
}

func TestGroqProvider_Integration_ListModels(t *testing.T) {
	skipIfNoGroqKey(t)

	provider, err := NewGroqProvider()
	if err != nil {
		t.Fatalf("failed to create provider: %v", err)
	}

	models, err := provider.ListModels(context.Background())
	if err != nil {
		t.Fatalf("ListModels failed: %v", err)
	}

	t.Logf("Found %d models", len(models))
	for _, m := range models {
		t.Logf("  - %s", m.Name)
	}
}

func TestGroqProvider_Integration_Chat(t *testing.T) {
	skipIfNoGroqKey(t)

	provider, err := NewGroqProvider()
	if err != nil {
		t.Fatalf("failed to create provider: %v", err)
	}

	// Use llama model which is commonly available on Groq
	req, err := NewChatRequest("llama-3.3-70b-versatile",
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

func TestGroqProvider_Integration_ChatStream(t *testing.T) {
	skipIfNoGroqKey(t)

	provider, err := NewGroqProvider()
	if err != nil {
		t.Fatalf("failed to create provider: %v", err)
	}

	req, err := NewChatRequest("llama-3.3-70b-versatile",
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

func TestGroqProvider_Integration_JSONMode(t *testing.T) {
	skipIfNoGroqKey(t)

	provider, err := NewGroqProvider()
	if err != nil {
		t.Fatalf("failed to create provider: %v", err)
	}

	req, err := NewChatRequest("llama-3.3-70b-versatile",
		WithMessages(
			SystemMessage("You are a helpful assistant. Always respond in JSON format."),
			UserMessage("Return a JSON object with name='groq' and fast=true"),
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
}

func TestGroqProvider_Integration_FastResponse(t *testing.T) {
	skipIfNoGroqKey(t)

	provider, err := NewGroqProvider()
	if err != nil {
		t.Fatalf("failed to create provider: %v", err)
	}

	// Groq is known for fast inference - test that we get a response
	req, err := NewChatRequest("llama-3.3-70b-versatile",
		WithMessages(
			UserMessage("In one word, what color is the sky?"),
		),
		WithMaxTokens(10),
		WithTemperature(0),
	)
	if err != nil {
		t.Fatalf("failed to create request: %v", err)
	}

	resp, err := provider.Chat(context.Background(), req)
	if err != nil {
		t.Fatalf("Chat failed: %v", err)
	}

	t.Logf("Fast response: %s", resp.Content)
}
