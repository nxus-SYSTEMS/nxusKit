//go:build integration

package nxuskit

import (
	"context"
	"os"
	"testing"
)

// Integration tests for LmStudioProvider.
// Run with: go test -tags=integration -v

func TestLmStudioProvider_Integration_Ping(t *testing.T) {
	if os.Getenv("LMSTUDIO_INTEGRATION") == "" {
		t.Skip("Set LMSTUDIO_INTEGRATION=1 to run this test")
	}

	provider, err := NewLmStudioProvider()
	if err != nil {
		t.Fatal(err)
	}

	if err := provider.Ping(context.Background()); err != nil {
		t.Fatalf("Ping failed: %v", err)
	}
}

func TestLmStudioProvider_Integration_ListModels(t *testing.T) {
	if os.Getenv("LMSTUDIO_INTEGRATION") == "" {
		t.Skip("Set LMSTUDIO_INTEGRATION=1 to run this test")
	}

	provider, err := NewLmStudioProvider()
	if err != nil {
		t.Fatal(err)
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

func TestLmStudioProvider_Integration_Chat(t *testing.T) {
	if os.Getenv("LMSTUDIO_INTEGRATION") == "" {
		t.Skip("Set LMSTUDIO_INTEGRATION=1 to run this test")
	}

	model := os.Getenv("LMSTUDIO_MODEL")
	if model == "" {
		model = "local-model"
	}

	provider, err := NewLmStudioProvider()
	if err != nil {
		t.Fatal(err)
	}

	req, _ := NewChatRequest(model,
		WithMessages(UserMessage("Say hello in exactly 3 words")),
	)

	resp, err := provider.Chat(context.Background(), req)
	if err != nil {
		t.Fatalf("Chat failed: %v", err)
	}

	t.Logf("Response: %s", resp.Content)
	t.Logf("Model: %s", resp.Model)
	if resp.Usage.Actual != nil {
		t.Logf("Tokens: %d prompt, %d completion",
			resp.Usage.Actual.PromptTokens,
			resp.Usage.Actual.CompletionTokens)
	}
}

func TestLmStudioProvider_Integration_ChatStream(t *testing.T) {
	if os.Getenv("LMSTUDIO_INTEGRATION") == "" {
		t.Skip("Set LMSTUDIO_INTEGRATION=1 to run this test")
	}

	model := os.Getenv("LMSTUDIO_MODEL")
	if model == "" {
		model = "local-model"
	}

	provider, err := NewLmStudioProvider()
	if err != nil {
		t.Fatal(err)
	}

	req, _ := NewChatRequest(model,
		WithMessages(UserMessage("Count from 1 to 5")),
	)

	chunks, errs := provider.ChatStream(context.Background(), req)

	var content string
	var chunkCount int
	for chunk := range chunks {
		content += chunk.Delta
		chunkCount++
		if chunk.IsFinal() {
			t.Logf("Final chunk with reason: %v", *chunk.FinishReason)
		}
	}

	if err := <-errs; err != nil {
		t.Fatalf("Stream failed: %v", err)
	}

	t.Logf("Received %d chunks", chunkCount)
	t.Logf("Full content: %s", content)
}
