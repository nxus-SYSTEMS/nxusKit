package nxuskit

import (
	"strings"
)

// ModelIdentifier represents a parsed model string.
// It separates the optional provider prefix from the model name.
type ModelIdentifier struct {
	// Provider is the explicitly specified provider name (e.g., "openai", "claude").
	// Empty string means the provider should be auto-detected from model patterns.
	Provider string

	// ModelName is the model identifier to pass to the provider.
	// For nested formats like "openrouter/anthropic/claude-3.5-sonnet",
	// this contains "anthropic/claude-3.5-sonnet".
	ModelName string
}

// String returns the canonical string representation of the model identifier.
// If Provider is empty, returns just the ModelName.
// Otherwise, returns "Provider/ModelName".
func (m ModelIdentifier) String() string {
	if m.Provider == "" {
		return m.ModelName
	}
	return m.Provider + "/" + m.ModelName
}

// IsExplicit returns true if a provider was explicitly specified.
func (m ModelIdentifier) IsExplicit() bool {
	return m.Provider != ""
}

// ParseModel parses a model string into provider and model components.
//
// Formats supported:
//   - "model-name"           → auto-detect provider (Provider="", ModelName="model-name")
//   - "provider/model-name"  → explicit provider (Provider="provider", ModelName="model-name")
//   - "provider/sub/model"   → explicit provider with nested path (Provider="provider", ModelName="sub/model")
//
// The first path component is taken as the provider if a slash is present.
// Everything after the first slash becomes the ModelName.
//
// Examples:
//
//	ParseModel("gpt-4o")
//	// → ModelIdentifier{Provider: "", ModelName: "gpt-4o"}
//
//	ParseModel("openai/gpt-4o")
//	// → ModelIdentifier{Provider: "openai", ModelName: "gpt-4o"}
//
//	ParseModel("openrouter/anthropic/claude-3.5-sonnet")
//	// → ModelIdentifier{Provider: "openrouter", ModelName: "anthropic/claude-3.5-sonnet"}
func ParseModel(model string) ModelIdentifier {
	if model == "" {
		return ModelIdentifier{}
	}

	// Find the first slash
	idx := strings.Index(model, "/")
	if idx == -1 {
		// No slash - bare model name, auto-detect provider
		return ModelIdentifier{
			Provider:  "",
			ModelName: model,
		}
	}

	// Has slash - split into provider and model
	provider := model[:idx]
	modelName := model[idx+1:]

	return ModelIdentifier{
		Provider:  provider,
		ModelName: modelName,
	}
}
