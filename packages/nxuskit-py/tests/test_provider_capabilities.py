"""Public provider-capability manifest types, no native library needed."""

import asyncio
import dataclasses

import pytest

from nxuskit import (
    PUBLIC_CAPABILITY_FIELDS,
    CapabilityStatus,
    ManifestPublicationPosture,
    PublicCapabilityManifest,
    PublicProviderCapability,
)


@pytest.mark.parametrize(
    "profile,logprobs,seed",
    [("openai", True, True), ("claude", False, False), ("ollama", False, True)],
)
def test_native_adapter_snapshot_literal_and_async_close(profile, logprobs, seed):
    from nxuskit import AdapterCapabilities, Provider

    native = getattr(Provider, profile)(model="m", use_ffi=False)
    snapshot = native.capabilities()
    assert isinstance(snapshot, AdapterCapabilities)
    assert snapshot.supports_logprobs is logprobs
    assert snapshot.supports_seed is seed
    assert snapshot.supports_streaming is True
    with pytest.raises(dataclasses.FrozenInstanceError):
        snapshot.supports_seed = False

    async def run():
        owner = Provider.create_async(profile, "m", use_ffi=False)
        assert owner.capabilities() == snapshot
        assert await owner.capabilities_async() == snapshot
        await owner.close_async()
        assert owner.capabilities() == snapshot
        assert await owner.capabilities_async() == snapshot

    asyncio.run(run())


def test_adapter_unknown_and_missing_resource_are_unavailable(monkeypatch):
    from importlib import resources

    from nxuskit.errors import ProviderError
    from nxuskit.provider import _native_adapter_capabilities

    with pytest.raises(ProviderError, match="unavailable"):
        _native_adapter_capabilities("unqualified")

    def missing(*args):
        raise FileNotFoundError("fixture")

    monkeypatch.setattr(resources, "files", missing)
    with pytest.raises(ProviderError, match="unavailable"):
        _native_adapter_capabilities("openai")


@pytest.mark.parametrize(
    "profile",
    [
        "openai",
        "claude",
        "ollama",
        "groq",
        "xai",
        "fireworks",
        "mistral",
        "together",
        "openrouter",
        "perplexity",
        "lmstudio",
    ],
)
def test_loaded_library_adapter_parity_and_after_close(profile):
    from nxuskit import Provider

    native = getattr(Provider, profile)(model="m", use_ffi=False).capabilities()
    kwargs = {} if profile in ("ollama", "lmstudio") else {"api_key": "fixture"}
    owner = getattr(Provider, profile)(model="m", use_ffi=True, **kwargs)
    assert owner.capabilities() == native
    owner.close()
    assert owner.capabilities() == native

    async def run():
        owner = Provider.create_async(profile, "m", use_ffi=True, **kwargs)
        assert owner.capabilities() == native
        assert await owner.capabilities_async() == native
        await owner.close_async()
        assert await owner.capabilities_async() == native

    asyncio.run(run())


@pytest.mark.parametrize(
    "document",
    [
        {"domains": {"llm": True}},
        {"adapter_capabilities": {}},
        {"adapter_capabilities": {"openai": {"supports_seed": True}}},
    ],
)
def test_old_or_malformed_library_catalog_never_falls_back(monkeypatch, document):
    import json
    import sys
    from types import SimpleNamespace

    from cffi import FFI

    from nxuskit.errors import ProviderError
    from nxuskit.provider import _library_adapter_capabilities

    ffi = FFI()
    raw = ffi.new("char[]", json.dumps(document).encode())
    freed = []
    lib = SimpleNamespace(
        nxuskit_capabilities=lambda: raw, nxuskit_free_string=lambda p: freed.append(p)
    )
    monkeypatch.setitem(sys.modules, "nxuskit._ffi", SimpleNamespace(ffi=ffi, lib=lib))
    with pytest.raises(ProviderError, match="unavailable") as caught:
        _library_adapter_capabilities("openai")
    assert caught.value.code == "unavailable"
    assert freed == [raw]


def test_oversized_penalty_bound_is_typed_unavailable():
    from nxuskit.errors import ProviderError
    from nxuskit.provider import _decode_adapter_capabilities

    row = {
        "supports_system_messages": True,
        "supports_streaming": True,
        "supports_vision": True,
        "max_stop_sequences": 4,
        "supports_presence_penalty": True,
        "supports_frequency_penalty": True,
        "supports_seed": True,
        "supports_logprobs": True,
        "supports_streaming_logprobs": True,
        "supports_json_mode": True,
        "supports_json_schema": True,
        "penalty_range": [-(10**400), 10**400],
        "max_logprobs": 20,
    }
    with pytest.raises(ProviderError, match="unavailable") as caught:
        _decode_adapter_capabilities({"openai": row}, "openai")
    assert caught.value.code == "unavailable"


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
