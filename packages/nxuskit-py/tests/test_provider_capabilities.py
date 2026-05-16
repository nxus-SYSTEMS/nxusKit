"""Public provider-capability manifest types, no native library needed."""

from nxuskit import (
    PUBLIC_CAPABILITY_FIELDS,
    CapabilityStatus,
    ManifestPublicationPosture,
    PublicCapabilityManifest,
    PublicProviderCapability,
)


def test_public_capability_fields_are_stable():
    assert PUBLIC_CAPABILITY_FIELDS == (
        "vision_input",
        "tool_calling",
        "thinking_blocks",
        "streaming_logprobs",
        "json_mode",
        "json_schema_strict",
        "json_schema_best_effort",
        "embeddings",
        "rerank",
    )


def test_public_capability_manifest_to_dict_uses_public_keys():
    manifest = PublicCapabilityManifest(
        schema_version="capability-manifest-v2-public-preview/1",
        posture=ManifestPublicationPosture.SPLIT,
        providers=[
            PublicProviderCapability(
                name="openai",
                display_name="OpenAI",
                last_reviewed_on="2026-05-09",
                provider_status="unknown",
                capabilities={
                    "json_schema_strict": CapabilityStatus.SUPPORTED,
                    "tool_calling": CapabilityStatus.PROVIDER_SPECIFIC,
                },
            )
        ],
    )

    data = manifest.to_dict()
    assert data["schema_version"] == "capability-manifest-v2-public-preview/1"
    assert data["posture"] == "split"

    provider = data["providers"][0]
    assert provider["name"] == "openai"
    assert provider["capabilities"]["json_schema_strict"] == "supported"
    assert provider["capabilities"]["tool_calling"] == "provider_specific"

    for internal_key in ("evidence", "model_overrides", "provider_specific", "features"):
        assert internal_key not in provider
