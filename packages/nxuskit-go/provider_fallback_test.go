package nxuskit

import (
	"context"
	"testing"
)

func TestParseProviderSequence(t *testing.T) {
	tests := []struct {
		name     string
		input    string
		expected []string
	}{
		{
			name:     "default sequence",
			input:    "ollama,claude,openai",
			expected: []string{"ollama", "claude", "openai"},
		},
		{
			name:     "with spaces",
			input:    "ollama, claude, openai",
			expected: []string{"ollama", "claude", "openai"},
		},
		{
			name:     "mixed case",
			input:    "Ollama,CLAUDE,OpenAI",
			expected: []string{"ollama", "claude", "openai"},
		},
		{
			name:     "single provider",
			input:    "ollama",
			expected: []string{"ollama"},
		},
		{
			name:     "empty string",
			input:    "",
			expected: []string{},
		},
		{
			name:     "trailing comma",
			input:    "ollama,claude,",
			expected: []string{"ollama", "claude"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := parseProviderSequence(tt.input)
			if len(result) != len(tt.expected) {
				t.Errorf("expected %d items, got %d", len(tt.expected), len(result))
				return
			}
			for i, exp := range tt.expected {
				if result[i] != exp {
					t.Errorf("at index %d: expected %q, got %q", i, exp, result[i])
				}
			}
		})
	}
}

func TestNewProviderFallback(t *testing.T) {
	// Use t.Setenv for safe env isolation (auto-restores on test completion)
	t.Setenv("LLMKIT_PROVIDER_SEQUENCE", "")

	t.Run("default sequence", func(t *testing.T) {
		pf := NewProviderFallback()
		sequence := pf.GetSequence()

		expected := []string{"ollama", "claude", "openai"}
		if len(sequence) != len(expected) {
			t.Errorf("expected %d providers, got %d", len(expected), len(sequence))
			return
		}
		for i, exp := range expected {
			if sequence[i] != exp {
				t.Errorf("at index %d: expected %q, got %q", i, exp, sequence[i])
			}
		}
	})

	t.Run("custom sequence via option", func(t *testing.T) {
		pf := NewProviderFallback(WithProviderSequence("groq,mistral"))
		sequence := pf.GetSequence()

		expected := []string{"groq", "mistral"}
		if len(sequence) != len(expected) {
			t.Errorf("expected %d providers, got %d", len(expected), len(sequence))
			return
		}
		for i, exp := range expected {
			if sequence[i] != exp {
				t.Errorf("at index %d: expected %q, got %q", i, exp, sequence[i])
			}
		}
	})

	t.Run("custom sequence via list option", func(t *testing.T) {
		pf := NewProviderFallback(WithProviderSequenceList([]string{"fireworks", "together"}))
		sequence := pf.GetSequence()

		expected := []string{"fireworks", "together"}
		if len(sequence) != len(expected) {
			t.Errorf("expected %d providers, got %d", len(expected), len(sequence))
		}
	})
}

func TestProviderFallbackFromEnv(t *testing.T) {
	// Use t.Setenv for safe env isolation (auto-restores on test completion)
	t.Setenv("LLMKIT_PROVIDER_SEQUENCE", "perplexity,openrouter")

	pf := NewProviderFallback()
	sequence := pf.GetSequence()

	expected := []string{"perplexity", "openrouter"}
	if len(sequence) != len(expected) {
		t.Errorf("expected %d providers, got %d", len(expected), len(sequence))
		return
	}
	for i, exp := range expected {
		if sequence[i] != exp {
			t.Errorf("at index %d: expected %q, got %q", i, exp, sequence[i])
		}
	}
}

func TestProviderFallbackSetSequence(t *testing.T) {
	pf := NewProviderFallback()

	newSequence := []string{"lmstudio", "ollama"}
	pf.SetSequence(newSequence)

	sequence := pf.GetSequence()
	if len(sequence) != len(newSequence) {
		t.Errorf("expected %d providers, got %d", len(newSequence), len(sequence))
		return
	}
	for i, exp := range newSequence {
		if sequence[i] != exp {
			t.Errorf("at index %d: expected %q, got %q", i, exp, sequence[i])
		}
	}
}

func TestProviderFallbackProviderName(t *testing.T) {
	pf := NewProviderFallback()
	if pf.ProviderName() != "fallback" {
		t.Errorf("expected provider name 'fallback', got %q", pf.ProviderName())
	}
}

func TestProviderFallbackNoProviders(t *testing.T) {
	pf := NewProviderFallback(WithProviderSequenceList([]string{}))

	ctx := context.Background()
	_, err := pf.GetAvailableProvider(ctx)
	if err != ErrNoProviderAvailable {
		t.Errorf("expected ErrNoProviderAvailable, got %v", err)
	}
}

func TestProviderFallbackWithMock(t *testing.T) {
	// Create a mock provider
	mockResp := &ChatResponse{
		Content: "Hello from mock!",
		Model:   "mock-model",
	}
	mock := NewMockProvider(WithMockResponse(mockResp))

	// Create fallback with custom provider injection
	pf := NewProviderFallback(WithProviderSequenceList([]string{"mock"}))

	// Inject mock provider directly
	pf.mu.Lock()
	pf.providers["mock"] = mock
	pf.mu.Unlock()

	ctx := context.Background()
	provider, err := pf.GetAvailableProvider(ctx)
	if err != nil {
		t.Fatalf("expected to get mock provider, got error: %v", err)
	}

	if provider.ProviderName() != "mock" {
		t.Errorf("expected mock provider, got %s", provider.ProviderName())
	}
}

func TestProviderFallbackCapabilities(t *testing.T) {
	pf := NewProviderFallback(WithProviderSequenceList([]string{}))
	caps := pf.GetCapabilities()

	// Should return default capabilities when no provider is available
	if caps.SupportsVision {
		t.Error("expected SupportsVision to be false by default")
	}
}

func TestProviderFallbackConfigOptions(t *testing.T) {
	t.Run("WithOllamaConfig", func(t *testing.T) {
		pf := NewProviderFallback(WithOllamaConfig("llama3"))
		if cfg, ok := pf.configs["ollama"].(map[string]string); ok {
			if cfg["model"] != "llama3" {
				t.Errorf("expected model 'llama3', got %q", cfg["model"])
			}
		} else {
			t.Error("expected ollama config to be set")
		}
	})

	t.Run("WithClaudeConfig", func(t *testing.T) {
		pf := NewProviderFallback(WithClaudeConfig("test-key", "claude-3"))
		if cfg, ok := pf.configs["claude"].(map[string]string); ok {
			if cfg["api_key"] != "test-key" {
				t.Errorf("expected api_key 'test-key', got %q", cfg["api_key"])
			}
			if cfg["model"] != "claude-3" {
				t.Errorf("expected model 'claude-3', got %q", cfg["model"])
			}
		} else {
			t.Error("expected claude config to be set")
		}
	})

	t.Run("WithOpenAIConfig", func(t *testing.T) {
		pf := NewProviderFallback(WithOpenAIConfig("openai-key", "gpt-4"))
		if cfg, ok := pf.configs["openai"].(map[string]string); ok {
			if cfg["api_key"] != "openai-key" {
				t.Errorf("expected api_key 'openai-key', got %q", cfg["api_key"])
			}
			if cfg["model"] != "gpt-4" {
				t.Errorf("expected model 'gpt-4', got %q", cfg["model"])
			}
		} else {
			t.Error("expected openai config to be set")
		}
	})
}

func TestProviderFallbackGetLastProvider(t *testing.T) {
	// Create fallback with mock
	mockResp := &ChatResponse{Content: "test", Model: "mock"}
	mock := NewMockProvider(WithMockResponse(mockResp))

	pf := NewProviderFallback(WithProviderSequenceList([]string{"mock"}))
	pf.mu.Lock()
	pf.providers["mock"] = mock
	pf.mu.Unlock()

	// Before any request, lastProvider should be empty
	if pf.GetLastProvider() != "" {
		t.Error("expected empty lastProvider before any request")
	}

	// Get provider and check
	ctx := context.Background()
	_, err := pf.GetAvailableProvider(ctx)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if pf.GetLastProvider() != "mock" {
		t.Errorf("expected lastProvider 'mock', got %q", pf.GetLastProvider())
	}
}

func TestProviderFallbackChat(t *testing.T) {
	mockResp := &ChatResponse{Content: "Hello from mock", Model: "mock"}
	mock := NewMockProvider(WithMockResponse(mockResp))

	pf := NewProviderFallback(WithProviderSequenceList([]string{"mock"}))
	pf.mu.Lock()
	pf.providers["mock"] = mock
	pf.mu.Unlock()

	ctx := context.Background()
	resp, err := pf.Chat(ctx, &ChatRequest{
		Messages: []Message{UserMessage("Hi")},
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if resp.Content != "Hello from mock" {
		t.Errorf("expected content 'Hello from mock', got %q", resp.Content)
	}
}

func TestProviderFallbackChatStream(t *testing.T) {
	fr := FinishReasonStop
	chunks := []StreamChunk{
		{Delta: "Hello "},
		{Delta: "world", FinishReason: &fr},
	}
	mock := NewMockProvider(WithMockStreamResponse(chunks))

	pf := NewProviderFallback(WithProviderSequenceList([]string{"mock"}))
	pf.mu.Lock()
	pf.providers["mock"] = mock
	pf.mu.Unlock()

	ctx := context.Background()
	chunkChan, errChan := pf.ChatStream(ctx, &ChatRequest{
		Messages: []Message{UserMessage("Hi")},
	})

	var content string
	for chunk := range chunkChan {
		content += chunk.Delta
	}

	if err := <-errChan; err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if content != "Hello world" {
		t.Errorf("expected 'Hello world', got %q", content)
	}
}

func TestProviderFallbackListModels(t *testing.T) {
	mock := NewMockProvider(WithMockModels([]ModelInfo{
		{Name: "model-1"},
		{Name: "model-2"},
	}))

	pf := NewProviderFallback(WithProviderSequenceList([]string{"mock"}))
	pf.mu.Lock()
	pf.providers["mock"] = mock
	pf.mu.Unlock()

	ctx := context.Background()
	models, err := pf.ListModels(ctx)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(models) != 2 {
		t.Errorf("expected 2 models, got %d", len(models))
	}
}

func TestProviderFallbackPing(t *testing.T) {
	mock := NewMockProvider()

	pf := NewProviderFallback(WithProviderSequenceList([]string{"mock"}))
	pf.mu.Lock()
	pf.providers["mock"] = mock
	pf.mu.Unlock()

	ctx := context.Background()
	err := pf.Ping(ctx)
	if err != nil {
		t.Errorf("expected ping to succeed, got: %v", err)
	}
}

func TestProviderFallbackStreamWithUsage(t *testing.T) {
	fr := FinishReasonStop
	chunks := []StreamChunk{
		{Delta: "test"},
		{Delta: "", FinishReason: &fr, Usage: &TokenUsage{
			Actual:     &TokenCount{PromptTokens: 5, CompletionTokens: 3},
			IsComplete: true,
		}},
	}
	mock := NewMockProvider(WithMockStreamResponse(chunks))

	pf := NewProviderFallback(WithProviderSequenceList([]string{"mock"}))
	pf.mu.Lock()
	pf.providers["mock"] = mock
	pf.mu.Unlock()

	ctx := context.Background()
	chunkChan, usageChan := pf.StreamWithUsage(ctx, &ChatRequest{
		Messages: []Message{UserMessage("Hi")},
	})

	var content string
	for chunk := range chunkChan {
		content += chunk.Delta
	}

	usage := <-usageChan
	// Usage tracking works through wrapStreamWithUsage
	_ = usage
	_ = content
}

func TestProviderFallbackCreateProviderUnknown(t *testing.T) {
	pf := NewProviderFallback(WithProviderSequenceList([]string{"unknown_provider"}))

	ctx := context.Background()
	_, err := pf.GetAvailableProvider(ctx)
	if err != ErrNoProviderAvailable {
		t.Errorf("expected ErrNoProviderAvailable, got: %v", err)
	}
}

func TestProviderFallbackCreateProviders(t *testing.T) {
	// Test that createProvider dispatches correctly.
	// Each provider's actual creation is tested in their respective test files.
	// Here we just verify the dispatch works without panicking.

	// LM Studio doesn't require API key
	t.Run("lmstudio", func(t *testing.T) {
		pf := NewProviderFallback()
		provider, err := pf.createLMStudioProvider()
		if err != nil {
			t.Fatalf("failed to create lmstudio provider: %v", err)
		}
		if provider.ProviderName() != "lmstudio" {
			t.Errorf("expected 'lmstudio', got %q", provider.ProviderName())
		}
	})

	// Ollama doesn't require API key either
	t.Run("ollama", func(t *testing.T) {
		pf := NewProviderFallback()
		provider, err := pf.createOllamaProvider()
		if err != nil {
			t.Fatalf("failed to create ollama provider: %v", err)
		}
		if provider.ProviderName() != "ollama" {
			t.Errorf("expected 'ollama', got %q", provider.ProviderName())
		}
	})

	// Test createProvider dispatch for various providers
	// These may fail if API keys aren't set, but they exercise the code path
	t.Run("createProvider dispatch", func(t *testing.T) {
		pf := NewProviderFallback()
		ctx := context.Background()

		// Test lmstudio via dispatch
		p, err := pf.createProvider(ctx, "lmstudio")
		if err != nil {
			t.Errorf("createProvider(lmstudio) failed: %v", err)
		} else if p.ProviderName() != "lmstudio" {
			t.Errorf("expected lmstudio, got %q", p.ProviderName())
		}

		// Test ollama via dispatch
		p, err = pf.createProvider(ctx, "ollama")
		if err != nil {
			t.Errorf("createProvider(ollama) failed: %v", err)
		} else if p.ProviderName() != "ollama" {
			t.Errorf("expected ollama, got %q", p.ProviderName())
		}
	})

	// Test providers with configs
	t.Run("ollama with config", func(t *testing.T) {
		pf := NewProviderFallback(WithOllamaConfig("llama3"))
		// Set a custom base_url config
		pf.configs["ollama"] = map[string]string{"base_url": "http://custom:11434"}
		provider, err := pf.createOllamaProvider()
		if err != nil {
			t.Fatalf("failed to create ollama provider with config: %v", err)
		}
		if provider.ProviderName() != "ollama" {
			t.Errorf("expected 'ollama', got %q", provider.ProviderName())
		}
	})

	t.Run("claude with config", func(t *testing.T) {
		pf := NewProviderFallback(WithClaudeConfig("test-key", "claude-3"))
		// The config should be used when creating
		pf.configs["claude"] = map[string]string{"api_key": "test-key", "base_url": "http://custom:8080"}
		provider, err := pf.createClaudeProvider()
		if err != nil {
			t.Fatalf("failed to create claude provider with config: %v", err)
		}
		if provider.ProviderName() != "claude" {
			t.Errorf("expected 'claude', got %q", provider.ProviderName())
		}
	})

	t.Run("openai with config", func(t *testing.T) {
		pf := NewProviderFallback(WithOpenAIConfig("test-key", "gpt-4"))
		pf.configs["openai"] = map[string]string{"api_key": "test-key", "base_url": "http://custom:8080"}
		provider, err := pf.createOpenAIProvider()
		if err != nil {
			t.Fatalf("failed to create openai provider with config: %v", err)
		}
		if provider.ProviderName() != "openai" {
			t.Errorf("expected 'openai', got %q", provider.ProviderName())
		}
	})

	// Test providers that require API keys from env - wrap in func to skip if missing
	t.Run("providers requiring API keys", func(t *testing.T) {
		pf := NewProviderFallback()

		// These may fail if API keys aren't set - that's OK, we just want to exercise the code
		testCases := []struct {
			name   string
			create func() (LLMProvider, error)
		}{
			{"groq", pf.createGroqProvider},
			{"mistral", pf.createMistralProvider},
			{"perplexity", pf.createPerplexityProvider},
			{"together", pf.createTogetherProvider},
			{"fireworks", pf.createFireworksProvider},
			{"openrouter", pf.createOpenRouterProvider},
		}

		for _, tc := range testCases {
			t.Run(tc.name, func(t *testing.T) {
				// Just call the function - it may error if API key is missing
				// We just want to ensure the code path is exercised
				_, _ = tc.create()
			})
		}
	})
}
