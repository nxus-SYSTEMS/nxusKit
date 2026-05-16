package nxuskit

import (
	"context"
	"errors"
	"os"
	"testing"
)

func TestGetProviderForModel_ExplicitProvider(t *testing.T) {
	tests := []struct {
		name         string
		model        ModelIdentifier
		wantProvider string
		wantErr      bool
	}{
		{
			name:         "explicit loopback",
			model:        ModelIdentifier{Provider: "loopback", ModelName: "any"},
			wantProvider: "loopback",
			wantErr:      false,
		},
		{
			name:         "explicit mock",
			model:        ModelIdentifier{Provider: "mock", ModelName: "any"},
			wantProvider: "mock",
			wantErr:      false,
		},
		{
			name:         "unknown provider",
			model:        ModelIdentifier{Provider: "nonexistent", ModelName: "model"},
			wantProvider: "",
			wantErr:      true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			provider, err := GetProviderForModel(tt.model)

			if tt.wantErr {
				if err == nil {
					t.Errorf("GetProviderForModel() expected error, got nil")
				}
				return
			}

			if err != nil {
				t.Fatalf("GetProviderForModel() unexpected error: %v", err)
			}

			if provider.ProviderName() != tt.wantProvider {
				t.Errorf("GetProviderForModel() provider = %q, want %q", provider.ProviderName(), tt.wantProvider)
			}
		})
	}
}

func TestGetProviderForModel_ExplicitProviderWithEnvVar(t *testing.T) {
	// Test that explicit provider works when env var is set
	origOpenAI := os.Getenv("OPENAI_API_KEY")
	defer func() { _ = os.Setenv("OPENAI_API_KEY", origOpenAI) }()
	_ = os.Setenv("OPENAI_API_KEY", "test-key-for-explicit")

	provider, err := GetProviderForModel(ModelIdentifier{Provider: "openai", ModelName: "gpt-4o"})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if provider.ProviderName() != "openai" {
		t.Errorf("expected openai, got %q", provider.ProviderName())
	}
}

func TestGetProviderForModel_AutoDetection(t *testing.T) {
	// Save current env vars
	origOpenAI := os.Getenv("OPENAI_API_KEY")
	origAnthropic := os.Getenv("ANTHROPIC_API_KEY")
	origOllama := os.Getenv("OLLAMA_HOST")

	// Clean up after test
	defer func() {
		_ = os.Setenv("OPENAI_API_KEY", origOpenAI)
		_ = os.Setenv("ANTHROPIC_API_KEY", origAnthropic)
		_ = os.Setenv("OLLAMA_HOST", origOllama)
	}()

	tests := []struct {
		name         string
		model        ModelIdentifier
		envVars      map[string]string
		wantProvider string
		wantErr      bool
	}{
		{
			name:         "gpt model with openai key",
			model:        ModelIdentifier{Provider: "", ModelName: "gpt-4o"},
			envVars:      map[string]string{"OPENAI_API_KEY": "test-key"},
			wantProvider: "openai",
			wantErr:      false,
		},
		{
			name:         "claude model with anthropic key",
			model:        ModelIdentifier{Provider: "", ModelName: "claude-sonnet-4-20250514"},
			envVars:      map[string]string{"ANTHROPIC_API_KEY": "test-key"},
			wantProvider: "claude",
			wantErr:      false,
		},
		{
			name:         "o1 model pattern",
			model:        ModelIdentifier{Provider: "", ModelName: "o1-preview"},
			envVars:      map[string]string{"OPENAI_API_KEY": "test-key"},
			wantProvider: "openai",
			wantErr:      false,
		},
		{
			name:         "o3 model pattern",
			model:        ModelIdentifier{Provider: "", ModelName: "o3-mini"},
			envVars:      map[string]string{"OPENAI_API_KEY": "test-key"},
			wantProvider: "openai",
			wantErr:      false,
		},
		{
			name:    "gpt model without api key",
			model:   ModelIdentifier{Provider: "", ModelName: "gpt-4o"},
			envVars: map[string]string{}, // No keys set
			wantErr: true,
		},
		{
			name:         "fallback to ollama when host set",
			model:        ModelIdentifier{Provider: "", ModelName: "llama3"},
			envVars:      map[string]string{"OLLAMA_HOST": "http://localhost:11434"},
			wantProvider: "ollama",
			wantErr:      false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Use t.Setenv to safely isolate env vars per subtest.
			// t.Setenv automatically restores the original value when the
			// subtest completes and panics if called from a parallel test.
			t.Setenv("OPENAI_API_KEY", "")
			t.Setenv("ANTHROPIC_API_KEY", "")
			t.Setenv("OLLAMA_HOST", "")
			t.Setenv("GROQ_API_KEY", "")
			t.Setenv("MISTRAL_API_KEY", "")

			// Set test env vars
			for k, v := range tt.envVars {
				t.Setenv(k, v)
			}

			provider, err := GetProviderForModel(tt.model)

			if tt.wantErr {
				if err == nil {
					t.Errorf("GetProviderForModel() expected error, got nil")
				}
				return
			}

			if err != nil {
				t.Fatalf("GetProviderForModel() unexpected error: %v", err)
			}

			if provider.ProviderName() != tt.wantProvider {
				t.Errorf("GetProviderForModel() provider = %q, want %q", provider.ProviderName(), tt.wantProvider)
			}
		})
	}
}

func TestGetProviderForModel_MissingAPIKeyError(t *testing.T) {
	// Clear env vars
	origOpenAI := os.Getenv("OPENAI_API_KEY")
	origOllama := os.Getenv("OLLAMA_HOST")
	defer func() {
		_ = os.Setenv("OPENAI_API_KEY", origOpenAI)
		_ = os.Setenv("OLLAMA_HOST", origOllama)
	}()
	_ = os.Unsetenv("OPENAI_API_KEY")
	_ = os.Unsetenv("OLLAMA_HOST")

	_, err := GetProviderForModel(ModelIdentifier{Provider: "", ModelName: "gpt-4o"})

	if err == nil {
		t.Fatal("expected error for missing API key")
	}

	// Should be a configuration error
	if !errors.Is(err, ErrConfiguration) {
		t.Errorf("expected ErrConfiguration, got %v", err)
	}

	// Error message should mention the provider and env var
	errMsg := err.Error()
	if errMsg == "" {
		t.Error("error message should not be empty")
	}
}

func TestRegisterProvider_CustomProvider(t *testing.T) {
	// Register a custom test provider
	RegisterProvider(
		"testcustom",
		[]string{`^testcustom-`},
		"TESTCUSTOM_API_KEY",
		func() (LLMProvider, error) {
			return NewMockProvider(
				WithMockResponse(&ChatResponse{Content: "custom response"}),
			), nil
		},
	)

	// Set env var
	origKey := os.Getenv("TESTCUSTOM_API_KEY")
	defer func() { _ = os.Setenv("TESTCUSTOM_API_KEY", origKey) }()
	_ = os.Setenv("TESTCUSTOM_API_KEY", "test-key")

	// Should match custom pattern
	provider, err := GetProviderForModel(ModelIdentifier{Provider: "", ModelName: "testcustom-model"})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	// Verify it's our custom provider (mock provider)
	resp, err := provider.Chat(context.Background(), &ChatRequest{Model: "testcustom-model"})
	if err != nil {
		t.Fatalf("Chat() error: %v", err)
	}
	if resp.Content != "custom response" {
		t.Errorf("expected custom response, got %q", resp.Content)
	}
}

func TestProviderPatternPriority(t *testing.T) {
	// Test that more specific patterns match before generic ones
	// OpenAI patterns should match gpt-* before Ollama's catch-all

	origOpenAI := os.Getenv("OPENAI_API_KEY")
	origOllama := os.Getenv("OLLAMA_HOST")
	defer func() {
		_ = os.Setenv("OPENAI_API_KEY", origOpenAI)
		_ = os.Setenv("OLLAMA_HOST", origOllama)
	}()

	// Set both env vars
	_ = os.Setenv("OPENAI_API_KEY", "test-key")
	_ = os.Setenv("OLLAMA_HOST", "http://localhost:11434")

	// gpt-4o should match OpenAI, not Ollama
	provider, err := GetProviderForModel(ModelIdentifier{Provider: "", ModelName: "gpt-4o"})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if provider.ProviderName() != "openai" {
		t.Errorf("gpt-4o should match openai, got %q", provider.ProviderName())
	}
}

func TestGetProviderForModel_UnknownModel(t *testing.T) {
	// Clear all env vars that could provide fallback
	origOllama := os.Getenv("OLLAMA_HOST")
	defer func() { _ = os.Setenv("OLLAMA_HOST", origOllama) }()
	_ = os.Unsetenv("OLLAMA_HOST")

	// An unknown model with no env vars set should fail
	_, err := GetProviderForModel(ModelIdentifier{Provider: "", ModelName: "totally-unknown-model-xyz"})
	if err == nil {
		t.Error("expected error for unknown model")
	}

	if !errors.Is(err, ErrConfiguration) {
		t.Errorf("expected ErrConfiguration, got %v", err)
	}
}
