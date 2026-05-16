package nxuskit

import (
	"github.com/nxus-SYSTEMS/nxusKit/packages/nxuskit-go/internal/format"
)

// Vision modality constants for model capability detection.
const (
	modalityVision = "vision"
	modalityImage  = "image"
)

// ModelInfo contains information about an available model.
type ModelInfo struct {
	// Name is the model identifier (e.g., "gpt-4o", "claude-sonnet-4-20250514").
	Name string `json:"name"`
	// SizeBytes is the model size in bytes (for local models). Nil if unknown.
	SizeBytes *int64 `json:"size_bytes,omitempty"`
	// Description is a human-readable description of the model.
	Description *string `json:"description,omitempty"`
	// ContextWindow is the maximum context length in tokens. Nil if unknown.
	ContextWindow *int `json:"context_window,omitempty"`
	// Metadata contains provider-specific additional information.
	Metadata map[string]any `json:"metadata,omitempty"`
}

// FormattedSize returns a human-readable size string (e.g., "3.7 GB").
// Returns empty string if SizeBytes is nil or zero.
func (m ModelInfo) FormattedSize() string {
	if m.SizeBytes == nil {
		return ""
	}
	return format.Bytes(*m.SizeBytes)
}

// FormattedContextWindow returns a human-readable context window (e.g., "128K").
// Returns empty string if ContextWindow is nil or zero.
func (m ModelInfo) FormattedContextWindow() string {
	if m.ContextWindow == nil {
		return ""
	}
	return format.ContextWindow(*m.ContextWindow)
}

// SupportsVision returns true if the model supports vision/image input.
// Checks the metadata for a "vision" or "supports_vision" key.
func (m ModelInfo) SupportsVision() bool {
	if m.Metadata == nil {
		return false
	}

	// Check various metadata keys that might indicate vision support
	if v, ok := m.Metadata["vision"].(bool); ok && v {
		return true
	}
	if v, ok := m.Metadata["supports_vision"].(bool); ok && v {
		return true
	}

	// Check if "vision" is in modalities
	modalities := m.Modalities()
	for _, mod := range modalities {
		if mod == modalityVision || mod == modalityImage {
			return true
		}
	}

	return false
}

// Modalities returns a list of supported modalities from metadata.
// Common modalities include "text", "vision", "audio", "code".
func (m ModelInfo) Modalities() []string {
	if m.Metadata == nil {
		return nil
	}

	// Try to get modalities as []string
	if mods, ok := m.Metadata["modalities"].([]string); ok {
		return mods
	}

	// Try to get modalities as []any and convert
	if mods, ok := m.Metadata["modalities"].([]any); ok {
		result := make([]string, 0, len(mods))
		for _, mod := range mods {
			if s, ok := mod.(string); ok {
				result = append(result, s)
			}
		}
		return result
	}

	return nil
}
