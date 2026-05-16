"""FFI deserialization helpers for the nxuskit C ABI JSON responses.

The canonical type definitions live in nxuskit.types. This module provides
from_dict() factory methods that convert C ABI JSON into those unified types.
"""

from __future__ import annotations

from dataclasses import dataclass

from nxuskit.tools import ToolCall
from nxuskit.types import (
    ChatResponse,
    LogprobsData,
    ModelInfo,
    StreamChunk,
    StreamLogprobsDelta,
    TokenUsage,
)


def _parse_usage(data: dict) -> TokenUsage:
    """Parse C ABI usage JSON into a unified TokenUsage."""
    # The nxuskit core serializes usage as:
    #   { "estimated": { "prompt_tokens": N, "completion_tokens": N }, "actual"?: {...} }
    source = data.get("actual") or data.get("estimated") or data
    prompt = int(source.get("prompt_tokens", 0))
    completion = int(source.get("completion_tokens", 0))
    return TokenUsage(prompt_tokens=prompt, completion_tokens=completion)


def _parse_tool_calls(raw: list | None) -> list[ToolCall] | None:
    """Parse tool_calls array into ToolCall objects."""
    if not raw:
        return None
    result = []
    for tc in raw:
        try:
            result.append(ToolCall.from_dict(tc))
        except (KeyError, TypeError):
            result.append(tc)  # pass through if can't parse
    return result if result else None


@dataclass
class Warning:
    """Provider warning from C ABI response."""

    code: str = ""
    message: str = ""
    severity: str = "info"

    @classmethod
    def from_dict(cls, data: dict) -> Warning:
        return cls(
            code=str(data.get("code", "")),
            message=str(data.get("message", "")),
            severity=str(data.get("severity", "info")),
        )


def chat_response_from_ffi(data: dict) -> ChatResponse:
    """Convert C ABI chat response JSON to unified ChatResponse."""
    usage = _parse_usage(data["usage"]) if data.get("usage") else TokenUsage()

    warn_list = []
    if "warnings" in data:
        warn_list = [Warning.from_dict(w) for w in data["warnings"]]

    tool_calls = _parse_tool_calls(data.get("tool_calls"))

    logprobs = LogprobsData.from_dict(data["logprobs"]) if data.get("logprobs") else None

    return ChatResponse(
        content=data.get("content", ""),
        model=str(data.get("model", "")),
        usage=usage,
        finish_reason=data.get("finish_reason"),
        tool_calls=tool_calls,
        provider=str(data.get("provider", "")),
        warnings=[f"{w.code}: {w.message}" for w in warn_list] if warn_list else [],
        logprobs=logprobs,
    )


def stream_chunk_from_ffi(data: dict) -> StreamChunk:
    """Convert C ABI stream chunk JSON to unified StreamChunk."""
    usage = None
    if data.get("usage"):
        usage = _parse_usage(data["usage"])

    logprobs = None
    if data.get("logprobs") is not None:
        logprobs = StreamLogprobsDelta.from_dict(data["logprobs"])

    return StreamChunk(
        delta=str(data.get("delta", data.get("content", ""))),
        thinking=data.get("thinking"),
        finish_reason=data.get("finish_reason"),
        usage=usage,
        tool_calls=data.get("tool_calls"),
        logprobs=logprobs,
    )


def model_info_from_ffi(data: dict) -> ModelInfo:
    """Convert C ABI model info JSON to unified ModelInfo."""
    return ModelInfo.from_dict(data)


# Backward compatibility — old code that imported these types from _ffi_types
# will still work, but they're the unified types from types.py now.
UsageStats = TokenUsage
__all__ = [
    "Warning",
    "chat_response_from_ffi",
    "stream_chunk_from_ffi",
    "model_info_from_ffi",
    "UsageStats",
    "ChatResponse",
    "StreamChunk",
    "ModelInfo",
    "TokenUsage",
]
