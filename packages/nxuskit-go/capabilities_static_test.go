package nxuskit

import "testing"

func TestGetStaticCapabilities_OpenAI(t *testing.T) {
	tests := []struct {
		model            string
		expectVision     bool
		expectStreaming  bool
		expectJSON       bool
		expectMaxContext int
		expectMaxImages  int
		expectSystemMsgs bool
	}{
		{
			model:            "gpt-4o",
			expectVision:     true,
			expectStreaming:  true,
			expectJSON:       true,
			expectMaxContext: 128000,
			expectMaxImages:  20,
			expectSystemMsgs: true,
		},
		{
			model:            "gpt-4-turbo",
			expectVision:     true,
			expectStreaming:  true,
			expectJSON:       true,
			expectMaxContext: 128000,
			expectMaxImages:  20,
			expectSystemMsgs: true,
		},
		{
			model:            "gpt-3.5-turbo",
			expectVision:     false,
			expectStreaming:  true,
			expectJSON:       true,
			expectMaxContext: 16385,
			expectMaxImages:  0,
			expectSystemMsgs: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.model, func(t *testing.T) {
			caps := GetStaticCapabilities("openai", tt.model)
			if caps == nil {
				t.Fatalf("expected capabilities for openai/%s, got nil", tt.model)
				return
			}

			if caps.SupportsVision != tt.expectVision {
				t.Errorf("SupportsVision = %v, want %v", caps.SupportsVision, tt.expectVision)
			}
			if caps.SupportsStreaming != tt.expectStreaming {
				t.Errorf("SupportsStreaming = %v, want %v", caps.SupportsStreaming, tt.expectStreaming)
			}
			if caps.SupportsJSON != tt.expectJSON {
				t.Errorf("SupportsJSON = %v, want %v", caps.SupportsJSON, tt.expectJSON)
			}
			if caps.MaxContextWindow != tt.expectMaxContext {
				t.Errorf("MaxContextWindow = %d, want %d", caps.MaxContextWindow, tt.expectMaxContext)
			}
			if caps.MaxImages != tt.expectMaxImages {
				t.Errorf("MaxImages = %d, want %d", caps.MaxImages, tt.expectMaxImages)
			}
			if caps.SupportsSystemMessages != tt.expectSystemMsgs {
				t.Errorf("SupportsSystemMessages = %v, want %v", caps.SupportsSystemMessages, tt.expectSystemMsgs)
			}
			if caps.UpdatedAt == "" {
				t.Error("UpdatedAt should not be empty")
			}
		})
	}
}

func TestGetStaticCapabilities_Claude(t *testing.T) {
	tests := []struct {
		model            string
		expectVision     bool
		expectStreaming  bool
		expectJSON       bool
		expectMaxContext int
		expectMaxImages  int
		expectSystemMsgs bool
	}{
		{
			model:            "claude-3-opus-20240229",
			expectVision:     true,
			expectStreaming:  true,
			expectJSON:       false, // Claude uses tool_use
			expectMaxContext: 200000,
			expectMaxImages:  20,
			expectSystemMsgs: true,
		},
		{
			model:            "claude-3-sonnet-20240229",
			expectVision:     true,
			expectStreaming:  true,
			expectJSON:       false,
			expectMaxContext: 200000,
			expectMaxImages:  20,
			expectSystemMsgs: true,
		},
		{
			model:            "claude-3-haiku-20240307",
			expectVision:     true,
			expectStreaming:  true,
			expectJSON:       false,
			expectMaxContext: 200000,
			expectMaxImages:  20,
			expectSystemMsgs: true,
		},
		{
			model:            "claude-3-5-sonnet-20241022",
			expectVision:     true,
			expectStreaming:  true,
			expectJSON:       false,
			expectMaxContext: 200000,
			expectMaxImages:  20,
			expectSystemMsgs: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.model, func(t *testing.T) {
			caps := GetStaticCapabilities("claude", tt.model)
			if caps == nil {
				t.Fatalf("expected capabilities for claude/%s, got nil", tt.model)
				return
			}

			if caps.SupportsVision != tt.expectVision {
				t.Errorf("SupportsVision = %v, want %v", caps.SupportsVision, tt.expectVision)
			}
			if caps.SupportsStreaming != tt.expectStreaming {
				t.Errorf("SupportsStreaming = %v, want %v", caps.SupportsStreaming, tt.expectStreaming)
			}
			if caps.SupportsJSON != tt.expectJSON {
				t.Errorf("SupportsJSON = %v, want %v", caps.SupportsJSON, tt.expectJSON)
			}
			if caps.MaxContextWindow != tt.expectMaxContext {
				t.Errorf("MaxContextWindow = %d, want %d", caps.MaxContextWindow, tt.expectMaxContext)
			}
			if caps.MaxImages != tt.expectMaxImages {
				t.Errorf("MaxImages = %d, want %d", caps.MaxImages, tt.expectMaxImages)
			}
			if caps.SupportsSystemMessages != tt.expectSystemMsgs {
				t.Errorf("SupportsSystemMessages = %v, want %v", caps.SupportsSystemMessages, tt.expectSystemMsgs)
			}
			if caps.UpdatedAt == "" {
				t.Error("UpdatedAt should not be empty")
			}
		})
	}
}

func TestGetStaticCapabilities_Unknown(t *testing.T) {
	// Test unknown provider
	caps := GetStaticCapabilities("unknown-provider", "any-model")
	if caps != nil {
		t.Errorf("expected nil for unknown provider, got %+v", caps)
	}

	// Test known provider with unknown model
	caps = GetStaticCapabilities("openai", "unknown-model")
	if caps != nil {
		t.Errorf("expected nil for unknown model, got %+v", caps)
	}

	// Test empty strings
	caps = GetStaticCapabilities("", "")
	if caps != nil {
		t.Errorf("expected nil for empty strings, got %+v", caps)
	}
}

func TestStaticCapabilities_AllModelsHaveUpdatedAt(t *testing.T) {
	// Ensure all configured models have UpdatedAt set
	providers := []string{"openai", "claude"}
	models := map[string][]string{
		"openai": {"gpt-4o", "gpt-4-turbo", "gpt-3.5-turbo"},
		"claude": {"claude-opus-4-20250514", "claude-sonnet-4-20250514", "claude-haiku-4-5-20251001", "claude-3-5-sonnet-20241022", "claude-3-5-haiku-20241022", "claude-3-opus-20240229", "claude-3-sonnet-20240229", "claude-3-haiku-20240307"},
	}

	for _, provider := range providers {
		for _, model := range models[provider] {
			caps := GetStaticCapabilities(provider, model)
			if caps == nil {
				t.Errorf("missing capabilities for %s/%s", provider, model)
				continue
			}
			if caps.UpdatedAt == "" {
				t.Errorf("missing UpdatedAt for %s/%s", provider, model)
			}
		}
	}
}
