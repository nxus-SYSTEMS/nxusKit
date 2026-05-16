//go:build integration

package nxuskit

import (
	"context"
	"os"
	"testing"
	"time"
)

// skipIfNoAPIKey skips the test if the specified environment variable is not set.
func skipIfNoAPIKeyInteg(t *testing.T, envVar string) string {
	t.Helper()
	key := os.Getenv(envVar)
	if key == "" {
		t.Skipf("Skipping: %s not set", envVar)
	}
	return key
}

// --- Ollama (requires local server) ---

func TestOllama_StreamWithUsage(t *testing.T) {
	provider, err := NewOllamaProvider()
	if err != nil {
		t.Fatalf("Failed to create Ollama provider: %v", err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()

	if err := provider.Ping(ctx); err != nil {
		t.Skip("Ollama server not available, skipping test")
	}

	models, err := provider.ListModels(ctx)
	if err != nil || len(models) == 0 {
		t.Skip("No models available on Ollama, skipping test")
	}

	model := models[0].Name

	req := &ChatRequest{
		Model:    model,
		Messages: []Message{UserMessage("Say hello")},
	}

	ctx, cancel = context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	chunks, usageCh := provider.StreamWithUsage(ctx, req)

	var content string
	for chunk := range chunks {
		content += chunk.Delta
	}

	usage := <-usageCh
	_ = usage
	_ = content
}

func TestOllama_ListAvailableModels(t *testing.T) {
	provider, err := NewOllamaProvider()
	if err != nil {
		t.Fatalf("Failed to create Ollama provider: %v", err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()

	if err := provider.Ping(ctx); err != nil {
		t.Skip("Ollama server not available, skipping test")
	}

	models, err := provider.ListAvailableModels(ctx)
	if err != nil {
		t.Fatalf("ListAvailableModels() error: %v", err)
	}

	_ = models
}

// --- LM Studio (requires local server) ---

func TestLmStudio_ListAvailableModels(t *testing.T) {
	provider, err := NewLmStudioProvider()
	if err != nil {
		t.Fatalf("Failed to create LM Studio provider: %v", err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()

	if err := provider.Ping(ctx); err != nil {
		t.Skip("LM Studio server not available, skipping test")
	}

	models, err := provider.ListAvailableModels(ctx)
	if err != nil {
		t.Fatalf("ListAvailableModels() error: %v", err)
	}

	_ = models
}

func TestLmStudio_StreamWithUsage_Integration(t *testing.T) {
	provider, err := NewLmStudioProvider()
	if err != nil {
		t.Fatalf("Failed to create LM Studio provider: %v", err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()

	if err := provider.Ping(ctx); err != nil {
		t.Skip("LM Studio server not available, skipping test")
	}

	models, err := provider.ListModels(ctx)
	if err != nil || len(models) == 0 {
		t.Skip("No models available on LM Studio, skipping test")
	}

	ctx, cancel = context.WithTimeout(context.Background(), 60*time.Second)
	defer cancel()

	req := &ChatRequest{
		Model:    models[0].Name,
		Messages: []Message{UserMessage("Say hello")},
	}

	chunks, usageCh := provider.StreamWithUsage(ctx, req)

	var content string
	for chunk := range chunks {
		content += chunk.Delta
	}

	usage := <-usageCh
	_ = content
	_ = usage
}

// --- API-key providers ---

func TestOpenAI_StreamWithUsage_Integration(t *testing.T) {
	apiKey := skipIfNoAPIKeyInteg(t, "OPENAI_API_KEY")

	provider, err := NewOpenAIProvider(WithOpenAIAPIKey(apiKey))
	if err != nil {
		t.Fatalf("Failed to create OpenAI provider: %v", err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	req := &ChatRequest{
		Model:    "gpt-4o-mini",
		Messages: []Message{UserMessage("Say hello in one word")},
	}

	chunks, usageCh := provider.StreamWithUsage(ctx, req)

	var content string
	for chunk := range chunks {
		content += chunk.Delta
	}

	usage := <-usageCh
	_ = content
	_ = usage
}

func TestClaude_StreamWithUsage_Integration(t *testing.T) {
	apiKey := skipIfNoAPIKeyInteg(t, "ANTHROPIC_API_KEY")

	provider, err := NewClaudeProvider(WithClaudeAPIKey(apiKey))
	if err != nil {
		t.Fatalf("Failed to create Claude provider: %v", err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	req := &ChatRequest{
		Model:    "claude-3-5-haiku-latest",
		Messages: []Message{UserMessage("Say hello in one word")},
	}

	chunks, usageCh := provider.StreamWithUsage(ctx, req)

	var content string
	for chunk := range chunks {
		content += chunk.Delta
	}

	usage := <-usageCh
	_ = content
	_ = usage
}

func TestGroq_StreamWithUsage_Integration(t *testing.T) {
	apiKey := skipIfNoAPIKeyInteg(t, "GROQ_API_KEY")

	provider, err := NewGroqProvider(WithGroqAPIKey(apiKey))
	if err != nil {
		t.Fatalf("Failed to create Groq provider: %v", err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	req := &ChatRequest{
		Model:    "llama-3.1-8b-instant",
		Messages: []Message{UserMessage("Say hello in one word")},
	}

	chunks, usageCh := provider.StreamWithUsage(ctx, req)

	var content string
	for chunk := range chunks {
		content += chunk.Delta
	}

	usage := <-usageCh
	_ = content
	_ = usage
}

func TestTogether_StreamWithUsage_Integration(t *testing.T) {
	apiKey := skipIfNoAPIKeyInteg(t, "TOGETHER_API_KEY")

	provider, err := NewTogetherProvider(WithTogetherAPIKey(apiKey))
	if err != nil {
		t.Fatalf("Failed to create Together provider: %v", err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	req := &ChatRequest{
		Model:    "meta-llama/Llama-3.2-3B-Instruct-Turbo",
		Messages: []Message{UserMessage("Say hello in one word")},
	}

	chunks, usageCh := provider.StreamWithUsage(ctx, req)

	var content string
	for chunk := range chunks {
		content += chunk.Delta
	}

	usage := <-usageCh
	_ = content
	_ = usage
}

func TestMistral_StreamWithUsage_Integration(t *testing.T) {
	apiKey := skipIfNoAPIKeyInteg(t, "MISTRAL_API_KEY")

	provider, err := NewMistralProvider(WithMistralAPIKey(apiKey))
	if err != nil {
		t.Fatalf("Failed to create Mistral provider: %v", err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	req := &ChatRequest{
		Model:    "mistral-small-latest",
		Messages: []Message{UserMessage("Say hello in one word")},
	}

	chunks, usageCh := provider.StreamWithUsage(ctx, req)

	var content string
	for chunk := range chunks {
		content += chunk.Delta
	}

	usage := <-usageCh
	_ = content
	_ = usage
}

func TestFireworks_ChatStream_Integration(t *testing.T) {
	apiKey := skipIfNoAPIKeyInteg(t, "FIREWORKS_API_KEY")

	provider, err := NewFireworksProvider(WithFireworksAPIKey(apiKey))
	if err != nil {
		t.Fatalf("Failed to create Fireworks provider: %v", err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	req := &ChatRequest{
		Model:    "accounts/fireworks/models/llama-v3p1-8b-instruct",
		Messages: []Message{UserMessage("Say hello in one word")},
	}

	chunks, errCh := provider.ChatStream(ctx, req)

	var content string
	for chunk := range chunks {
		content += chunk.Delta
	}

	err = <-errCh
	_ = content
	_ = err
}

func TestOpenRouter_ChatStream_Integration(t *testing.T) {
	apiKey := skipIfNoAPIKeyInteg(t, "OPENROUTER_API_KEY")

	provider, err := NewOpenRouterProvider(WithOpenRouterAPIKey(apiKey))
	if err != nil {
		t.Fatalf("Failed to create OpenRouter provider: %v", err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	req := &ChatRequest{
		Model:    "meta-llama/llama-3.2-3b-instruct:free",
		Messages: []Message{UserMessage("Say hello in one word")},
	}

	chunks, errCh := provider.ChatStream(ctx, req)

	var content string
	for chunk := range chunks {
		content += chunk.Delta
	}

	err = <-errCh
	_ = content
	_ = err
}

func TestPerplexity_ChatStream_Integration(t *testing.T) {
	apiKey := skipIfNoAPIKeyInteg(t, "PERPLEXITY_API_KEY")

	provider, err := NewPerplexityProvider(WithPerplexityAPIKey(apiKey))
	if err != nil {
		t.Fatalf("Failed to create Perplexity provider: %v", err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	req := &ChatRequest{
		Model:    "llama-3.1-sonar-small-128k-online",
		Messages: []Message{UserMessage("Say hello")},
	}

	chunks, errCh := provider.ChatStream(ctx, req)

	var content string
	for chunk := range chunks {
		content += chunk.Delta
	}

	err = <-errCh
	_ = content
	_ = err
}
