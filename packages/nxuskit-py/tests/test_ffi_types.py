"""Tests for _ffi_types — FFI deserialization helpers, no native library needed."""

from nxuskit._ffi_types import (
    Warning,
    chat_response_from_ffi,
    model_info_from_ffi,
    stream_chunk_from_ffi,
)
from nxuskit.types import TokenUsage


def test_selected_metadata_survives_ffi_json():
    import json

    expected = {
        "citations": ["https://example.org/é"],
        "usage": {"cost": None, "reasoning_tokens": 7},
    }
    body = json.loads(
        '{"content":"ok","provider_metadata":{"citations":["https://example.org/é"],"usage":{"cost":null,"reasoning_tokens":7}}}'
    )
    assert chat_response_from_ffi(body).provider_metadata == expected
    body.update(delta="", terminal=True)
    assert stream_chunk_from_ffi(body).provider_metadata == expected
    assert chat_response_from_ffi({}).provider_metadata is None
    assert stream_chunk_from_ffi({}).provider_metadata is None


def test_terminal_truth_json_compatibility():
    import json
    from dataclasses import asdict

    for body, final, raw in [
        ('{"delta":"old"}', False, None),
        ('{"delta":"old","finish_reason":null}', False, None),
        ('{"delta":"old","finish_reason":"stop"}', True, None),
        ('{"delta":"x","terminal":false,"finish_reason":null}', False, None),
        (
            '{"delta":"x","terminal":true,"finish_reason":null,"raw_finish_reason":"vendor-new"}',
            True,
            "vendor-new",
        ),
        ('{"delta":"x","terminal":true,"finish_reason":null,"raw_finish_reason":null}', True, None),
    ]:
        chunk = stream_chunk_from_ffi(json.loads(body))
        assert chunk.is_final() is final, body
        assert asdict(chunk)["raw_finish_reason"] == raw, body
    response = chat_response_from_ffi(
        {"content": "x", "finish_reason": None, "raw_finish_reason": "vendor-new"}
    )
    assert response.finish_reason is None
    assert response.raw_finish_reason == "vendor-new"


def _parse_usage(data: dict) -> TokenUsage:
    """Helper matching the internal _parse_usage from _ffi_types."""
    from nxuskit._ffi_types import _parse_usage

    return _parse_usage(data)


class TestUsageStats:
    def test_from_dict_estimated(self):
        data = {"estimated": {"prompt_tokens": 10, "completion_tokens": 20}}
        stats = _parse_usage(data)
        assert stats.prompt_tokens == 10
        assert stats.completion_tokens == 20
        assert stats.total_tokens == 30

    def test_from_dict_actual_preferred(self):
        data = {
            "estimated": {"prompt_tokens": 10, "completion_tokens": 20},
            "actual": {"prompt_tokens": 15, "completion_tokens": 25},
        }
        stats = _parse_usage(data)
        assert stats.prompt_tokens == 15
        assert stats.completion_tokens == 25
        assert stats.total_tokens == 40

    def test_from_dict_flat_fallback(self):
        data = {"prompt_tokens": 5, "completion_tokens": 7}
        stats = _parse_usage(data)
        assert stats.prompt_tokens == 5
        assert stats.completion_tokens == 7
        assert stats.total_tokens == 12

    def test_from_dict_empty(self):
        stats = _parse_usage({})
        assert stats.prompt_tokens == 0
        assert stats.completion_tokens == 0
        assert stats.total_tokens == 0


class TestWarning:
    def test_from_dict(self):
        w = Warning.from_dict({"code": "rate_limit", "message": "slow down", "severity": "warn"})
        assert w.code == "rate_limit"
        assert w.message == "slow down"
        assert w.severity == "warn"

    def test_from_dict_defaults(self):
        w = Warning.from_dict({})
        assert w.code == ""
        assert w.message == ""
        assert w.severity == "info"


class TestChatResponse:
    def test_from_dict_full(self):
        data = {
            "content": "Hello",
            "model": "gpt-4o",
            "provider": "openai",
            "usage": {"estimated": {"prompt_tokens": 5, "completion_tokens": 10}},
            "finish_reason": "stop",
            "warnings": [{"code": "w1", "message": "warn", "severity": "info"}],
        }
        resp = chat_response_from_ffi(data)
        assert resp.content == "Hello"
        assert resp.model == "gpt-4o"
        assert resp.provider == "openai"
        assert resp.usage is not None
        assert resp.usage.prompt_tokens == 5
        assert resp.finish_reason == "stop"
        assert len(resp.warnings) == 1
        assert "w1" in resp.warnings[0]

    def test_from_dict_minimal(self):
        resp = chat_response_from_ffi({"content": "hi"})
        assert resp.content == "hi"
        assert resp.model == ""
        assert resp.finish_reason is None

    def test_from_dict_empty_usage(self):
        resp = chat_response_from_ffi({"content": "x", "usage": {}})
        # Empty usage dict produces a zero-valued TokenUsage
        assert resp.usage.prompt_tokens == 0


class TestStreamChunk:
    def test_from_dict(self):
        chunk = stream_chunk_from_ffi({"content": "tok"})
        assert chunk.delta == "tok"

    def test_from_dict_defaults(self):
        chunk = stream_chunk_from_ffi({})
        assert chunk.delta == ""


class TestModelInfo:
    def test_from_dict(self):
        info = model_info_from_ffi({"id": "gpt-4o", "name": "GPT-4o", "provider": "openai"})
        assert info.id == "gpt-4o"
        assert info.name == "GPT-4o"
        assert info.provider == "openai"

    def test_from_dict_defaults(self):
        info = model_info_from_ffi({})
        assert info.id == ""
        assert info.name == ""
        assert info.provider == ""
