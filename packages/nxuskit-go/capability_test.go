package nxuskit

import "testing"

func TestVisionMode_String(t *testing.T) {
	tests := []struct {
		mode     VisionMode
		expected string
	}{
		{VisionModeNone, "none"},
		{VisionModeSingleImage, "single"},
		{VisionModeMultiImage, "multi"},
	}

	for _, tt := range tests {
		t.Run(tt.expected, func(t *testing.T) {
			if got := tt.mode.String(); got != tt.expected {
				t.Errorf("VisionMode.String() = %v, want %v", got, tt.expected)
			}
		})
	}
}

func TestVisionMode_SupportsVision(t *testing.T) {
	if VisionModeNone.SupportsVision() {
		t.Error("VisionModeNone.SupportsVision() should be false")
	}
	if !VisionModeSingleImage.SupportsVision() {
		t.Error("VisionModeSingleImage.SupportsVision() should be true")
	}
	if !VisionModeMultiImage.SupportsVision() {
		t.Error("VisionModeMultiImage.SupportsVision() should be true")
	}
}

func TestVisionMode_SupportsMultipleImages(t *testing.T) {
	if VisionModeNone.SupportsMultipleImages() {
		t.Error("VisionModeNone.SupportsMultipleImages() should be false")
	}
	if VisionModeSingleImage.SupportsMultipleImages() {
		t.Error("VisionModeSingleImage.SupportsMultipleImages() should be false")
	}
	if !VisionModeMultiImage.SupportsMultipleImages() {
		t.Error("VisionModeMultiImage.SupportsMultipleImages() should be true")
	}
}

func TestDefaultModelCapabilities(t *testing.T) {
	caps := DefaultModelCapabilities()

	if caps.VisionMode != VisionModeNone {
		t.Errorf("default VisionMode should be None, got %v", caps.VisionMode)
	}
	if !caps.SupportsStreaming {
		t.Error("default SupportsStreaming should be true")
	}
}

func TestDefaultCapabilities(t *testing.T) {
	caps := DefaultCapabilities()

	if !caps.SupportsSystemMessages {
		t.Error("default SupportsSystemMessages should be true")
	}
	if caps.SupportsStreaming {
		t.Error("default SupportsStreaming should be false")
	}
	if caps.SupportsVision {
		t.Error("default SupportsVision should be false")
	}
	if caps.MaxStopSequences != nil {
		t.Error("default MaxStopSequences should be nil")
	}
	if caps.SupportsPresencePenalty {
		t.Error("default SupportsPresencePenalty should be false")
	}
	if caps.SupportsFrequencyPenalty {
		t.Error("default SupportsFrequencyPenalty should be false")
	}
	if caps.SupportsSeed {
		t.Error("default SupportsSeed should be false")
	}
	if caps.SupportsLogprobs {
		t.Error("default SupportsLogprobs should be false")
	}
	if caps.MaxLogprobs != nil {
		t.Error("default MaxLogprobs should be nil")
	}
	if caps.SupportsJSONMode {
		t.Error("default SupportsJSONMode should be false")
	}
	if caps.SupportsJSONSchema {
		t.Error("default SupportsJSONSchema should be false")
	}
	if caps.PenaltyRange != nil {
		t.Error("default PenaltyRange should be nil")
	}
}

// TestAllProviderCapabilities validates that all providers report accurate capability values.
// This test ensures new capability fields are populated correctly for all providers.
func TestAllProviderCapabilities(t *testing.T) {
	// Test data structure for provider capability expectations
	type capExpect struct {
		name                   string
		supportsTools          bool
		supportsResponseFormat bool
		supportsTopK           bool
		supportsMinP           bool
		maxStopSequences       *int
		supportsJSONMode       bool
		supportsJSONSchema     bool
	}

	intPtr := func(v int) *int { return &v }

	tests := []struct {
		providerName string
		getProvider  func() (LLMProvider, error)
		expect       capExpect
	}{
		{
			providerName: "openai",
			getProvider: func() (LLMProvider, error) {
				return NewOpenAIProvider(WithOpenAIAPIKey("test-key"))
			},
			expect: capExpect{
				name:                   "openai",
				supportsTools:          true,
				supportsResponseFormat: true,
				supportsTopK:           false,
				supportsMinP:           false,
				maxStopSequences:       intPtr(4),
				supportsJSONMode:       true,
				supportsJSONSchema:     true,
			},
		},
		{
			providerName: "claude",
			getProvider: func() (LLMProvider, error) {
				return NewClaudeProvider(WithClaudeAPIKey("test-key"))
			},
			expect: capExpect{
				name:                   "claude",
				supportsTools:          true,
				supportsResponseFormat: false,
				supportsTopK:           true,
				supportsMinP:           false,
				maxStopSequences:       intPtr(8192),
				supportsJSONMode:       false,
				supportsJSONSchema:     false,
			},
		},
		{
			providerName: "ollama",
			getProvider: func() (LLMProvider, error) {
				return NewOllamaProvider()
			},
			expect: capExpect{
				name:                   "ollama",
				supportsTools:          true,
				supportsResponseFormat: true,
				supportsTopK:           true,
				supportsMinP:           true,
				maxStopSequences:       nil,
				supportsJSONMode:       true,
				supportsJSONSchema:     true,
			},
		},
		{
			providerName: "groq",
			getProvider: func() (LLMProvider, error) {
				return NewGroqProvider(WithGroqAPIKey("test-key"))
			},
			expect: capExpect{
				name:                   "groq",
				supportsTools:          true,
				supportsResponseFormat: true,
				supportsTopK:           false,
				supportsMinP:           false,
				maxStopSequences:       intPtr(4),
				supportsJSONMode:       true,
				supportsJSONSchema:     false,
			},
		},
		{
			providerName: "xai",
			getProvider: func() (LLMProvider, error) {
				return NewXaiProvider(WithXaiAPIKey("test-key"))
			},
			expect: capExpect{
				name:                   "xai",
				supportsTools:          true,
				supportsResponseFormat: true,
				supportsTopK:           false,
				supportsMinP:           false,
				maxStopSequences:       nil,
				supportsJSONMode:       true,
				supportsJSONSchema:     true,
			},
		},
		{
			providerName: "together",
			getProvider: func() (LLMProvider, error) {
				return NewTogetherProvider(WithTogetherAPIKey("test-key"))
			},
			expect: capExpect{
				name:                   "together",
				supportsTools:          true,
				supportsResponseFormat: true,
				supportsTopK:           true,
				supportsMinP:           false,
				maxStopSequences:       intPtr(4),
				supportsJSONMode:       true,
				supportsJSONSchema:     false,
			},
		},
		{
			providerName: "mistral",
			getProvider: func() (LLMProvider, error) {
				return NewMistralProvider(WithMistralAPIKey("test-key"))
			},
			expect: capExpect{
				name:                   "mistral",
				supportsTools:          true,
				supportsResponseFormat: true,
				supportsTopK:           false,
				supportsMinP:           false,
				maxStopSequences:       intPtr(4),
				supportsJSONMode:       true,
				supportsJSONSchema:     false,
			},
		},
		{
			providerName: "fireworks",
			getProvider: func() (LLMProvider, error) {
				return NewFireworksProvider(WithFireworksAPIKey("test-key"))
			},
			expect: capExpect{
				name:                   "fireworks",
				supportsTools:          false,
				supportsResponseFormat: true,
				supportsTopK:           false,
				supportsMinP:           false,
				maxStopSequences:       intPtr(4),
				supportsJSONMode:       true,
				supportsJSONSchema:     false,
			},
		},
		{
			providerName: "openrouter",
			getProvider: func() (LLMProvider, error) {
				return NewOpenRouterProvider(WithOpenRouterAPIKey("test-key"))
			},
			expect: capExpect{
				name:                   "openrouter",
				supportsTools:          true,
				supportsResponseFormat: true,
				supportsTopK:           false,
				supportsMinP:           false,
				maxStopSequences:       intPtr(4),
				supportsJSONMode:       true,
				supportsJSONSchema:     false,
			},
		},
		{
			providerName: "perplexity",
			getProvider: func() (LLMProvider, error) {
				return NewPerplexityProvider(WithPerplexityAPIKey("test-key"))
			},
			expect: capExpect{
				name:                   "perplexity",
				supportsTools:          false,
				supportsResponseFormat: false,
				supportsTopK:           false,
				supportsMinP:           false,
				maxStopSequences:       nil,
				supportsJSONMode:       false,
				supportsJSONSchema:     false,
			},
		},
		{
			providerName: "lmstudio",
			getProvider: func() (LLMProvider, error) {
				return NewLmStudioProvider()
			},
			expect: capExpect{
				name:                   "lmstudio",
				supportsTools:          false,
				supportsResponseFormat: false,
				supportsTopK:           false,
				supportsMinP:           false,
				maxStopSequences:       nil,
				supportsJSONMode:       false,
				supportsJSONSchema:     false,
			},
		},
		{
			providerName: "mock",
			getProvider: func() (LLMProvider, error) {
				return NewMockProvider(), nil
			},
			expect: capExpect{
				name:                   "mock",
				supportsTools:          true,
				supportsResponseFormat: true,
				supportsTopK:           true,
				supportsMinP:           true,
				maxStopSequences:       nil,
				supportsJSONMode:       true,
				supportsJSONSchema:     true,
			},
		},
		{
			providerName: "loopback",
			getProvider: func() (LLMProvider, error) {
				return NewLoopbackProvider(), nil
			},
			expect: capExpect{
				name:                   "loopback",
				supportsTools:          true,
				supportsResponseFormat: true,
				supportsTopK:           true,
				supportsMinP:           true,
				maxStopSequences:       nil,
				supportsJSONMode:       true,
				supportsJSONSchema:     true,
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.providerName, func(t *testing.T) {
			provider, err := tt.getProvider()
			if err != nil {
				t.Fatalf("failed to create provider: %v", err)
			}

			caps := provider.GetCapabilities()

			// Verify new capability fields
			if caps.SupportsTools != tt.expect.supportsTools {
				t.Errorf("SupportsTools = %v, want %v", caps.SupportsTools, tt.expect.supportsTools)
			}
			if caps.SupportsResponseFormat != tt.expect.supportsResponseFormat {
				t.Errorf("SupportsResponseFormat = %v, want %v", caps.SupportsResponseFormat, tt.expect.supportsResponseFormat)
			}
			if caps.SupportsTopK != tt.expect.supportsTopK {
				t.Errorf("SupportsTopK = %v, want %v", caps.SupportsTopK, tt.expect.supportsTopK)
			}
			if caps.SupportsMinP != tt.expect.supportsMinP {
				t.Errorf("SupportsMinP = %v, want %v", caps.SupportsMinP, tt.expect.supportsMinP)
			}

			// Verify MaxStopSequences
			if tt.expect.maxStopSequences == nil {
				if caps.MaxStopSequences != nil {
					t.Errorf("MaxStopSequences = %v, want nil", *caps.MaxStopSequences)
				}
			} else {
				if caps.MaxStopSequences == nil {
					t.Errorf("MaxStopSequences = nil, want %v", *tt.expect.maxStopSequences)
				} else if *caps.MaxStopSequences != *tt.expect.maxStopSequences {
					t.Errorf("MaxStopSequences = %v, want %v", *caps.MaxStopSequences, *tt.expect.maxStopSequences)
				}
			}

			// Verify JSON mode capabilities
			if caps.SupportsJSONMode != tt.expect.supportsJSONMode {
				t.Errorf("SupportsJSONMode = %v, want %v", caps.SupportsJSONMode, tt.expect.supportsJSONMode)
			}
			if caps.SupportsJSONSchema != tt.expect.supportsJSONSchema {
				t.Errorf("SupportsJSONSchema = %v, want %v", caps.SupportsJSONSchema, tt.expect.supportsJSONSchema)
			}
		})
	}
}

// TestNewCapabilityFieldsPopulated verifies that the new capability fields are actually populated
// (not just default zero values) for providers that support them.
func TestNewCapabilityFieldsPopulated(t *testing.T) {
	// OpenAI should have all new fields set appropriately
	openai, err := NewOpenAIProvider(WithOpenAIAPIKey("test-key"))
	if err != nil {
		t.Fatalf("failed to create OpenAI provider: %v", err)
	}
	caps := openai.GetCapabilities()

	// Verify SupportsTools is explicitly true (not default false)
	if !caps.SupportsTools {
		t.Error("OpenAI SupportsTools should be true")
	}

	// Verify SupportsResponseFormat is explicitly true
	if !caps.SupportsResponseFormat {
		t.Error("OpenAI SupportsResponseFormat should be true")
	}

	// Claude should have TopK support
	claude, err := NewClaudeProvider(WithClaudeAPIKey("test-key"))
	if err != nil {
		t.Fatalf("failed to create Claude provider: %v", err)
	}
	claudeCaps := claude.GetCapabilities()

	if !claudeCaps.SupportsTopK {
		t.Error("Claude SupportsTopK should be true")
	}
	if claudeCaps.SupportsMinP {
		t.Error("Claude SupportsMinP should be false")
	}

	// Ollama should support both TopK and MinP
	ollama, err := NewOllamaProvider()
	if err != nil {
		t.Fatalf("failed to create Ollama provider: %v", err)
	}
	ollamaCaps := ollama.GetCapabilities()

	if !ollamaCaps.SupportsTopK {
		t.Error("Ollama SupportsTopK should be true")
	}
	if !ollamaCaps.SupportsMinP {
		t.Error("Ollama SupportsMinP should be true")
	}
}
