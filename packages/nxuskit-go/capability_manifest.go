package nxuskit

// CapabilityStatus is an evidence-gated public capability status value.
type CapabilityStatus string

const (
	// CapabilityStatusSupported means the feature is documented, SDK-mapped, and tested.
	CapabilityStatusSupported CapabilityStatus = "supported"
	// CapabilityStatusUnsupported means the provider lacks the feature or the SDK blocks it.
	CapabilityStatusUnsupported CapabilityStatus = "unsupported"
	// CapabilityStatusRecognized means the provider documents the feature, but nxusKit has not mapped it yet.
	CapabilityStatusRecognized CapabilityStatus = "recognized"
	// CapabilityStatusProviderSpecific means the feature exists outside a shared SDK surface.
	CapabilityStatusProviderSpecific CapabilityStatus = "provider_specific"
	// CapabilityStatusFuture means the feature is intentionally deferred.
	CapabilityStatusFuture CapabilityStatus = "future"
	// CapabilityStatusUnknown means evidence has not been reviewed.
	CapabilityStatusUnknown CapabilityStatus = "unknown"
)

// ManifestPublicationPosture describes the public/internal manifest split.
type ManifestPublicationPosture string

const (
	// ManifestPublicationPostureSplit means public preview fields are a stable
	// projection and internal evidence fields remain private to the SDK.
	ManifestPublicationPostureSplit ManifestPublicationPosture = "split"
)

// PublicCapabilityFields returns the stable Capability Manifest v2 public
// preview capability field names.
func PublicCapabilityFields() []string {
	return []string{
		"vision_input",
		"tool_calling",
		"thinking_blocks",
		"streaming_logprobs",
		"json_mode",
		"json_schema_strict",
		"json_schema_best_effort",
		"embeddings",
		"rerank",
	}
}

// PublicProviderCapability is the provider-level public preview projection.
type PublicProviderCapability struct {
	Name           string                      `json:"name"`
	DisplayName    string                      `json:"display_name"`
	LastReviewedOn string                      `json:"last_reviewed_on"`
	ProviderStatus string                      `json:"provider_status"`
	Capabilities   map[string]CapabilityStatus `json:"capabilities"`
}

// PublicCapabilityManifest is the Capability Manifest v2 public preview
// projection. It intentionally excludes internal evidence, model override,
// provider-specific, and nested feature-record fields.
type PublicCapabilityManifest struct {
	SchemaVersion string                     `json:"schema_version"`
	Posture       ManifestPublicationPosture `json:"posture"`
	Providers     []PublicProviderCapability `json:"providers"`
}
