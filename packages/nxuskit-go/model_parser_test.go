package nxuskit

import (
	"testing"
)

func TestParseModel(t *testing.T) {
	tests := []struct {
		name          string
		input         string
		wantProvider  string
		wantModelName string
	}{
		// Auto-detect (no provider prefix)
		{
			name:          "bare model name",
			input:         "gpt-4o",
			wantProvider:  "",
			wantModelName: "gpt-4o",
		},
		{
			name:          "claude model",
			input:         "claude-sonnet-4-20250514",
			wantProvider:  "",
			wantModelName: "claude-sonnet-4-20250514",
		},
		{
			name:          "model with dashes",
			input:         "llama-3.1-70b-versatile",
			wantProvider:  "",
			wantModelName: "llama-3.1-70b-versatile",
		},

		// Explicit provider prefix
		{
			name:          "explicit openai provider",
			input:         "openai/gpt-4o",
			wantProvider:  "openai",
			wantModelName: "gpt-4o",
		},
		{
			name:          "explicit claude provider",
			input:         "claude/claude-sonnet-4-20250514",
			wantProvider:  "claude",
			wantModelName: "claude-sonnet-4-20250514",
		},
		{
			name:          "explicit ollama provider",
			input:         "ollama/llama3",
			wantProvider:  "ollama",
			wantModelName: "llama3",
		},

		// Nested format (aggregator/underlying-provider/model)
		{
			name:          "openrouter nested format",
			input:         "openrouter/anthropic/claude-3.5-sonnet",
			wantProvider:  "openrouter",
			wantModelName: "anthropic/claude-3.5-sonnet",
		},
		{
			name:          "together nested format",
			input:         "together/meta-llama/Llama-3-70b-chat",
			wantProvider:  "together",
			wantModelName: "meta-llama/Llama-3-70b-chat",
		},
		{
			name:          "fireworks nested format",
			input:         "fireworks/accounts/fireworks/models/llama-v3p1-70b",
			wantProvider:  "fireworks",
			wantModelName: "accounts/fireworks/models/llama-v3p1-70b",
		},

		// Edge cases
		{
			name:          "empty string",
			input:         "",
			wantProvider:  "",
			wantModelName: "",
		},
		{
			name:          "single slash only",
			input:         "/",
			wantProvider:  "",
			wantModelName: "",
		},
		{
			name:          "trailing slash",
			input:         "openai/",
			wantProvider:  "openai",
			wantModelName: "",
		},
		{
			name:          "leading slash",
			input:         "/gpt-4o",
			wantProvider:  "",
			wantModelName: "gpt-4o",
		},
		{
			name:          "multiple consecutive slashes",
			input:         "openai//gpt-4o",
			wantProvider:  "openai",
			wantModelName: "/gpt-4o",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := ParseModel(tt.input)

			if got.Provider != tt.wantProvider {
				t.Errorf("ParseModel(%q).Provider = %q, want %q", tt.input, got.Provider, tt.wantProvider)
			}
			if got.ModelName != tt.wantModelName {
				t.Errorf("ParseModel(%q).ModelName = %q, want %q", tt.input, got.ModelName, tt.wantModelName)
			}
		})
	}
}

func TestModelIdentifier_String(t *testing.T) {
	tests := []struct {
		name     string
		id       ModelIdentifier
		expected string
	}{
		{
			name:     "bare model",
			id:       ModelIdentifier{Provider: "", ModelName: "gpt-4o"},
			expected: "gpt-4o",
		},
		{
			name:     "explicit provider",
			id:       ModelIdentifier{Provider: "openai", ModelName: "gpt-4o"},
			expected: "openai/gpt-4o",
		},
		{
			name:     "nested model",
			id:       ModelIdentifier{Provider: "openrouter", ModelName: "anthropic/claude-3.5-sonnet"},
			expected: "openrouter/anthropic/claude-3.5-sonnet",
		},
		{
			name:     "empty model",
			id:       ModelIdentifier{Provider: "", ModelName: ""},
			expected: "",
		},
		{
			name:     "provider only",
			id:       ModelIdentifier{Provider: "openai", ModelName: ""},
			expected: "openai/",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := tt.id.String(); got != tt.expected {
				t.Errorf("ModelIdentifier.String() = %q, want %q", got, tt.expected)
			}
		})
	}
}

func TestModelIdentifier_IsExplicit(t *testing.T) {
	tests := []struct {
		name     string
		id       ModelIdentifier
		expected bool
	}{
		{
			name:     "auto-detect",
			id:       ModelIdentifier{Provider: "", ModelName: "gpt-4o"},
			expected: false,
		},
		{
			name:     "explicit provider",
			id:       ModelIdentifier{Provider: "openai", ModelName: "gpt-4o"},
			expected: true,
		},
		{
			name:     "empty both",
			id:       ModelIdentifier{Provider: "", ModelName: ""},
			expected: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := tt.id.IsExplicit(); got != tt.expected {
				t.Errorf("ModelIdentifier.IsExplicit() = %v, want %v", got, tt.expected)
			}
		})
	}
}
