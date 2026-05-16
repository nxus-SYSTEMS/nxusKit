package nxuskit

// StaticModelCapabilities contains known capability information for specific models.
// This provides fallback capability data when dynamic detection is unavailable.
//
// Unlike ModelCapabilities (which is for per-model runtime detection) and
// ProviderCapabilities (which is provider-level), StaticModelCapabilities
// contains pre-configured information about well-known models.
type StaticModelCapabilities struct {
	// SupportsStreaming indicates whether the model supports streaming responses.
	SupportsStreaming bool

	// SupportsVision indicates whether the model supports image/vision inputs.
	SupportsVision bool

	// MaxImages is the maximum number of images supported per request.
	// 0 means no images supported, -1 means unlimited.
	MaxImages int

	// SupportsSystemMessages indicates whether the model accepts system role messages.
	SupportsSystemMessages bool

	// SupportsJSON indicates whether the model supports JSON mode output.
	SupportsJSON bool

	// MaxContextWindow is the maximum context window size in tokens.
	MaxContextWindow int

	// UpdatedAt is the ISO 8601 date when this information was last verified.
	UpdatedAt string
}

// staticCapabilities contains pre-configured capability data for known models.
// Organized by provider name, then model name.
var staticCapabilities = map[string]map[string]StaticModelCapabilities{
	"openai": {
		"gpt-4o": {
			SupportsStreaming:      true,
			SupportsVision:         true,
			MaxImages:              20,
			SupportsSystemMessages: true,
			SupportsJSON:           true,
			MaxContextWindow:       128000,
			UpdatedAt:              "2024-12-01",
		},
		"gpt-4-turbo": {
			SupportsStreaming:      true,
			SupportsVision:         true,
			MaxImages:              20,
			SupportsSystemMessages: true,
			SupportsJSON:           true,
			MaxContextWindow:       128000,
			UpdatedAt:              "2024-12-01",
		},
		"gpt-3.5-turbo": {
			SupportsStreaming:      true,
			SupportsVision:         false,
			MaxImages:              0,
			SupportsSystemMessages: true,
			SupportsJSON:           true,
			MaxContextWindow:       16385,
			UpdatedAt:              "2024-12-01",
		},
	},
	"xai": {
		"grok-4": {
			SupportsStreaming:      true,
			SupportsVision:         true,
			MaxImages:              -1,
			SupportsSystemMessages: true,
			SupportsJSON:           true,
			MaxContextWindow:       256000,
			UpdatedAt:              "2026-05-13",
		},
	},
	"claude": {
		"claude-opus-4-20250514": {
			SupportsStreaming:      true,
			SupportsVision:         true,
			MaxImages:              20,
			SupportsSystemMessages: true,
			SupportsJSON:           false,
			MaxContextWindow:       200000,
			UpdatedAt:              "2025-05-14",
		},
		"claude-sonnet-4-20250514": {
			SupportsStreaming:      true,
			SupportsVision:         true,
			MaxImages:              20,
			SupportsSystemMessages: true,
			SupportsJSON:           false,
			MaxContextWindow:       200000,
			UpdatedAt:              "2025-05-14",
		},
		"claude-haiku-4-5-20251001": {
			SupportsStreaming:      true,
			SupportsVision:         true,
			MaxImages:              20,
			SupportsSystemMessages: true,
			SupportsJSON:           false,
			MaxContextWindow:       200000,
			UpdatedAt:              "2025-10-01",
		},
		"claude-3-5-sonnet-20241022": {
			SupportsStreaming:      true,
			SupportsVision:         true,
			MaxImages:              20,
			SupportsSystemMessages: true,
			SupportsJSON:           false,
			MaxContextWindow:       200000,
			UpdatedAt:              "2024-12-01",
		},
		"claude-3-5-haiku-20241022": {
			SupportsStreaming:      true,
			SupportsVision:         true,
			MaxImages:              20,
			SupportsSystemMessages: true,
			SupportsJSON:           false,
			MaxContextWindow:       200000,
			UpdatedAt:              "2024-12-01",
		},
		"claude-3-opus-20240229": {
			SupportsStreaming:      true,
			SupportsVision:         true,
			MaxImages:              20,
			SupportsSystemMessages: true,
			SupportsJSON:           false,
			MaxContextWindow:       200000,
			UpdatedAt:              "2024-12-01",
		},
		"claude-3-sonnet-20240229": {
			SupportsStreaming:      true,
			SupportsVision:         true,
			MaxImages:              20,
			SupportsSystemMessages: true,
			SupportsJSON:           false,
			MaxContextWindow:       200000,
			UpdatedAt:              "2024-12-01",
		},
		"claude-3-haiku-20240307": {
			SupportsStreaming:      true,
			SupportsVision:         true,
			MaxImages:              20,
			SupportsSystemMessages: true,
			SupportsJSON:           false,
			MaxContextWindow:       200000,
			UpdatedAt:              "2024-12-01",
		},
	},
}

// GetStaticCapabilities returns static capability information for a known model.
// Returns nil if the provider or model is not found in the static configuration.
//
// This function is useful as a fallback when dynamic capability detection
// is unavailable or fails.
//
// Example:
//
//	caps := GetStaticCapabilities("openai", "gpt-4o")
//	if caps != nil {
//	    fmt.Printf("Max context: %d tokens\n", caps.MaxContextWindow)
//	}
func GetStaticCapabilities(provider, model string) *StaticModelCapabilities {
	providerModels, ok := staticCapabilities[provider]
	if !ok {
		return nil
	}

	caps, ok := providerModels[model]
	if !ok {
		return nil
	}

	return &caps
}
