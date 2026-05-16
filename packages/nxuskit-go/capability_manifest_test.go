package nxuskit

import (
	"encoding/json"
	"slices"
	"testing"
)

func TestPublicCapabilityManifestFields(t *testing.T) {
	got := PublicCapabilityFields()
	want := []string{
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

	if !slices.Equal(got, want) {
		t.Fatalf("PublicCapabilityFields() = %#v, want %#v", got, want)
	}
}

func TestPublicCapabilityManifestJSONKeys(t *testing.T) {
	manifest := PublicCapabilityManifest{
		SchemaVersion: "capability-manifest-v2-public-preview/1",
		Posture:       ManifestPublicationPostureSplit,
		Providers: []PublicProviderCapability{
			{
				Name:           "openai",
				DisplayName:    "OpenAI",
				LastReviewedOn: "2026-05-09",
				ProviderStatus: "unknown",
				Capabilities: map[string]CapabilityStatus{
					"json_schema_strict": CapabilityStatusSupported,
					"tool_calling":       CapabilityStatusProviderSpecific,
				},
			},
		},
	}

	raw, err := json.Marshal(manifest)
	if err != nil {
		t.Fatalf("marshal manifest: %v", err)
	}

	var decoded map[string]any
	if err := json.Unmarshal(raw, &decoded); err != nil {
		t.Fatalf("unmarshal manifest: %v", err)
	}

	if decoded["schema_version"] != "capability-manifest-v2-public-preview/1" {
		t.Fatalf("schema_version = %#v", decoded["schema_version"])
	}
	if decoded["posture"] != "split" {
		t.Fatalf("posture = %#v", decoded["posture"])
	}

	providers := decoded["providers"].([]any)
	provider := providers[0].(map[string]any)
	capabilities := provider["capabilities"].(map[string]any)

	if capabilities["json_schema_strict"] != "supported" {
		t.Fatalf("json_schema_strict = %#v", capabilities["json_schema_strict"])
	}
	if capabilities["tool_calling"] != "provider_specific" {
		t.Fatalf("tool_calling = %#v", capabilities["tool_calling"])
	}
	for _, internalKey := range []string{"evidence", "model_overrides", "provider_specific", "features"} {
		if _, ok := provider[internalKey]; ok {
			t.Fatalf("public provider leaked internal key %q", internalKey)
		}
	}
}
