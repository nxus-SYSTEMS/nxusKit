//go:build integration

package nxuskit

import (
	"context"
	"os"
	"testing"
)

// Integration tests for OllamaProvider.
// Run with: go test -tags=integration -v

func TestOllamaProvider_Integration_Ping(t *testing.T) {
	if os.Getenv("OLLAMA_INTEGRATION") == "" {
		t.Skip("Set OLLAMA_INTEGRATION=1 to run this test")
	}

	provider, err := NewOllamaProvider()
	if err != nil {
		t.Fatal(err)
	}

	if err := provider.Ping(context.Background()); err != nil {
		t.Fatalf("Ping failed: %v", err)
	}
}

func TestOllamaProvider_Integration_ListModels(t *testing.T) {
	if os.Getenv("OLLAMA_INTEGRATION") == "" {
		t.Skip("Set OLLAMA_INTEGRATION=1 to run this test")
	}

	provider, err := NewOllamaProvider()
	if err != nil {
		t.Fatal(err)
	}

	models, err := provider.ListModels(context.Background())
	if err != nil {
		t.Fatalf("ListModels failed: %v", err)
	}

	t.Logf("Found %d models", len(models))
	for _, m := range models {
		t.Logf("  - %s (%s)", m.Name, m.FormattedSize())
	}
}

func TestOllamaProvider_Integration_Chat(t *testing.T) {
	if os.Getenv("OLLAMA_INTEGRATION") == "" {
		t.Skip("Set OLLAMA_INTEGRATION=1 to run this test")
	}

	model := os.Getenv("OLLAMA_MODEL")
	if model == "" {
		model = "llama3.2"
	}

	provider, err := NewOllamaProvider()
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

func TestOllamaProvider_Integration_ChatStream(t *testing.T) {
	if os.Getenv("OLLAMA_INTEGRATION") == "" {
		t.Skip("Set OLLAMA_INTEGRATION=1 to run this test")
	}

	model := os.Getenv("OLLAMA_MODEL")
	if model == "" {
		model = "llama3.2"
	}

	provider, err := NewOllamaProvider()
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

func TestOllamaProvider_Integration_JSONMode(t *testing.T) {
	if os.Getenv("OLLAMA_INTEGRATION") == "" {
		t.Skip("Set OLLAMA_INTEGRATION=1 to run this test")
	}

	model := os.Getenv("OLLAMA_MODEL")
	if model == "" {
		model = "llama3.2"
	}

	provider, err := NewOllamaProvider()
	if err != nil {
		t.Fatal(err)
	}

	req, _ := NewChatRequest(model,
		WithMessages(
			SystemMessage("You are a helpful assistant. Always respond in valid JSON format."),
			UserMessage("Return a JSON object with name='ollama' and local=true"),
		),
		WithResponseFormat(ResponseFormatJSON()),
	)

	resp, err := provider.Chat(context.Background(), req)
	if err != nil {
		t.Fatalf("Chat failed: %v", err)
	}

	t.Logf("JSON Response: %s", resp.Content)
}

func TestOllamaProvider_Integration_TopKMinP(t *testing.T) {
	if os.Getenv("OLLAMA_INTEGRATION") == "" {
		t.Skip("Set OLLAMA_INTEGRATION=1 to run this test")
	}

	model := os.Getenv("OLLAMA_MODEL")
	if model == "" {
		model = "llama3.2"
	}

	provider, err := NewOllamaProvider()
	if err != nil {
		t.Fatal(err)
	}

	// Test with TopK and MinP - Ollama supports both
	req, _ := NewChatRequest(model,
		WithMessages(UserMessage("Say hello")),
		WithTopK(40),
		WithMinP(0.05),
	)

	resp, err := provider.Chat(context.Background(), req)
	if err != nil {
		t.Fatalf("Chat with TopK/MinP failed: %v", err)
	}

	t.Logf("Response with TopK=40, MinP=0.05: %s", resp.Content)
}

func TestOllamaProvider_Integration_Capabilities(t *testing.T) {
	if os.Getenv("OLLAMA_INTEGRATION") == "" {
		t.Skip("Set OLLAMA_INTEGRATION=1 to run this test")
	}

	provider, err := NewOllamaProvider()
	if err != nil {
		t.Fatal(err)
	}

	caps := provider.GetCapabilities()

	t.Logf("Ollama Capabilities:")
	t.Logf("  SupportsTools: %v", caps.SupportsTools)
	t.Logf("  SupportsResponseFormat: %v", caps.SupportsResponseFormat)
	t.Logf("  SupportsTopK: %v", caps.SupportsTopK)
	t.Logf("  SupportsMinP: %v", caps.SupportsMinP)
	t.Logf("  SupportsJSONMode: %v", caps.SupportsJSONMode)
	t.Logf("  SupportsJSONSchema: %v", caps.SupportsJSONSchema)

	if !caps.SupportsTopK {
		t.Error("Expected Ollama to support TopK")
	}
	if !caps.SupportsMinP {
		t.Error("Expected Ollama to support MinP")
	}
}
