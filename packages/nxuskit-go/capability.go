package nxuskit

// VisionMode describes a model's vision/image input capabilities.
type VisionMode int

const (
	// VisionModeNone indicates no vision support.
	VisionModeNone VisionMode = iota

	// VisionModeSingleImage indicates single image per message support.
	VisionModeSingleImage

	// VisionModeMultiImage indicates multiple images per message support.
	VisionModeMultiImage
)

// String returns the string representation of VisionMode.
func (v VisionMode) String() string {
	switch v {
	case VisionModeSingleImage:
		return "single"
	case VisionModeMultiImage:
		return "multi"
	default:
		return "none"
	}
}

// SupportsVision returns true if any vision mode is supported.
func (v VisionMode) SupportsVision() bool {
	return v != VisionModeNone
}

// SupportsMultipleImages returns true if multiple images are supported.
func (v VisionMode) SupportsMultipleImages() bool {
	return v == VisionModeMultiImage
}

// ModelCapabilities describes the capabilities of a specific model.
//
// This is used by providers that support per-model capability detection
// (currently only Ollama). Other providers return provider-level capabilities.
type ModelCapabilities struct {
	// VisionMode indicates the model's vision input support level.
	VisionMode VisionMode

	// SupportsStreaming indicates whether the model supports streaming.
	SupportsStreaming bool
}

// DefaultModelCapabilities returns conservative defaults.
func DefaultModelCapabilities() ModelCapabilities {
	return ModelCapabilities{
		VisionMode:        VisionModeNone,
		SupportsStreaming: true,
	}
}
