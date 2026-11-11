"""Core types and data structures for nxuskit."""

import warnings
from dataclasses import dataclass, field
from enum import Enum
from typing import Any, Dict, List, Optional


class Role(str, Enum):
    """Message role enumeration."""

    USER = "user"
    ASSISTANT = "assistant"
    SYSTEM = "system"


@dataclass(frozen=True)
class AdapterCapabilities:
    """Static shipped adapter declarations, NOT model/endpoint-effective support."""

    supports_system_messages: bool
    supports_streaming: bool
    supports_vision: bool
    max_stop_sequences: Optional[int]
    supports_presence_penalty: bool
    supports_frequency_penalty: bool
    supports_seed: bool
    supports_logprobs: bool
    supports_streaming_logprobs: bool
    supports_json_mode: bool
    supports_json_schema: bool
    penalty_range: Optional[tuple[float, float]]
    max_logprobs: Optional[int]


class ImageSourceType(str, Enum):
    """Supported image source types."""

    URL = "url"
    BASE64 = "base64"
    FILEPATH = "filepath"


class ResponseFormat(str, Enum):
    """Response format options for structured output.

    Providers handle these differently:
    - OpenAI/compatible: Uses {"response_format": {"type": "json_object"}}
    - Claude: Appends JSON instruction to system message (Claude's recommended approach)
    """

    JSON = "json"
    TEXT = "text"  # Default, no special handling


class CapabilityStatus(str, Enum):
    """Evidence-gated public capability status value."""

    SUPPORTED = "supported"
    UNSUPPORTED = "unsupported"
    RECOGNIZED = "recognized"
    PROVIDER_SPECIFIC = "provider_specific"
    FUTURE = "future"
    UNKNOWN = "unknown"


PUBLIC_CAPABILITY_FIELDS: tuple[str, ...] = (
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


class ManifestPublicationPosture(str, Enum):
    """Publication posture for the public/internal manifest split."""

    SPLIT = "split"


@dataclass
class PublicProviderCapability:
    """Provider-level Capability Manifest v2 public preview projection."""

    name: str
    display_name: str
    last_reviewed_on: str
    provider_status: str
    capabilities: Dict[str, CapabilityStatus]

    def to_dict(self) -> Dict[str, Any]:
        """Serialize using stable public preview JSON keys."""
        return {
            "name": self.name,
            "display_name": self.display_name,
            "last_reviewed_on": self.last_reviewed_on,
            "provider_status": self.provider_status,
            "capabilities": {key: str(value.value) for key, value in self.capabilities.items()},
        }


@dataclass
class PublicCapabilityManifest:
    """Capability Manifest v2 public preview projection."""

    schema_version: str
    posture: ManifestPublicationPosture
    providers: List[PublicProviderCapability] = field(default_factory=list)

    def to_dict(self) -> Dict[str, Any]:
        """Serialize using stable public preview JSON keys."""
        return {
            "schema_version": self.schema_version,
            "posture": str(self.posture.value),
            "providers": [provider.to_dict() for provider in self.providers],
        }


@dataclass
class ImageSource:
    """Represents an image attached to a message."""

    source_type: ImageSourceType
    data: str
    media_type: Optional[str] = None


@dataclass
class TokenUsage:
    """Token usage information from a response.

    Supports both Anthropic naming (input_tokens/output_tokens) and
    OpenAI naming (prompt_tokens/completion_tokens). Both are always
    available as properties regardless of which convention was used
    at construction time.
    """

    input_tokens: int = 0
    output_tokens: int = 0
    total_tokens: int = 0

    def __init__(
        self,
        input_tokens: int = 0,
        output_tokens: int = 0,
        total_tokens: int = 0,
        prompt_tokens: Optional[int] = None,
        completion_tokens: Optional[int] = None,
    ):
        # Accept either naming convention
        if prompt_tokens is not None and input_tokens == 0:
            self.input_tokens = prompt_tokens
        else:
            self.input_tokens = input_tokens
        if completion_tokens is not None and output_tokens == 0:
            self.output_tokens = completion_tokens
        else:
            self.output_tokens = output_tokens
        if total_tokens:
            self.total_tokens = total_tokens
        else:
            self.total_tokens = self.input_tokens + self.output_tokens

    @property
    def prompt_tokens(self) -> int:
        """OpenAI-compatible alias for input_tokens."""
        return self.input_tokens

    @property
    def completion_tokens(self) -> int:
        """OpenAI-compatible alias for output_tokens."""
        return self.output_tokens


@dataclass
class ChatRequest:
    """Request payload for a chat completion.

    Mirrors the Rust ``nxuskit::ChatRequest`` shape and serializes to the
    same first-class JSON the Rust wrapper, Go SDK, and C ABI consume.
    Optional fields are omitted from ``to_dict`` / ``to_json`` rather than
    serialized as ``null`` so v0.9.2 consumers see no schema drift.

    Notes:
        - ``top_logprobs`` is only meaningful when ``logprobs`` is true.
        - Unsupported provider/model combinations warn-and-drop logprobs
          rather than tunneling through ``provider_options``; populating
          ``provider_options`` here never auto-populates the first-class
          ``logprobs`` / ``top_logprobs`` fields.
        - Unary logprobs shipped in v0.9.3. Streaming logprobs are exposed on
          ``StreamChunk.logprobs`` in v0.9.4+ when provider capability metadata
          reports support.

    Example:
        >>> from nxuskit import ChatRequest
        >>> req = ChatRequest(
        ...     model="gpt-5.4",
        ...     messages=[{"role": "user", "content": "Score the next token."}],
        ...     logprobs=True,
        ...     top_logprobs=5,
        ... )
        >>> payload = req.to_dict()
        >>> payload["logprobs"], payload["top_logprobs"]
        (True, 5)
    """

    model: str
    messages: List[Dict[str, Any]] = field(default_factory=list)
    temperature: Optional[float] = None
    max_tokens: Optional[int] = None
    top_p: Optional[float] = None
    stop: Optional[List[str]] = None
    logprobs: Optional[bool] = None
    top_logprobs: Optional[int] = None
    provider_options: Optional[Dict[str, Any]] = None

    def to_dict(self) -> Dict[str, Any]:
        """Serialize to the FFI/wire dict, omitting unset optional fields."""
        out: Dict[str, Any] = {"model": self.model, "messages": list(self.messages)}
        if self.temperature is not None:
            out["temperature"] = self.temperature
        if self.max_tokens is not None:
            out["max_tokens"] = self.max_tokens
        if self.top_p is not None:
            out["top_p"] = self.top_p
        if self.stop is not None:
            out["stop"] = list(self.stop)
        if self.logprobs is not None:
            out["logprobs"] = self.logprobs
        if self.top_logprobs is not None:
            out["top_logprobs"] = self.top_logprobs
        if self.provider_options is not None:
            out["provider_options"] = dict(self.provider_options)
        return out


def _validate_request_controls(provider, **controls):
    """Validate supplied facade controls before serialization or transport."""
    import math

    from nxuskit.errors import ProviderError

    numeric = {
        "temperature": (0, 2),
        "top_p": (0, 1),
        "min_p": (0, 1),
        "presence_penalty": (-2, 2),
        "frequency_penalty": (-2, 2),
    }
    integers = {
        "max_tokens": (1, 1048576),
        "seed": (-(2**63), 2**63 - 1),
        "top_k": (1, 2147483647),
        "top_logprobs": (0, 20),
    }
    for name, value in controls.items():
        if value is None:
            continue
        valid = True
        if name in numeric:
            low, high = numeric[name]
            valid = type(value) in (int, float) and low <= value <= high and math.isfinite(value)
        elif name in integers:
            low, high = integers[name]
            valid = type(value) is int and low <= value <= high
        elif name == "logprobs":
            valid = type(value) is bool
        elif name == "reasoning_effort":
            valid = isinstance(value, str) and value in ("low", "medium", "high", "max")
        elif name == "thinking":
            valid = isinstance(value, str) and value in ("auto", "enabled", "disabled", "omit")
        elif name == "stop":
            values = [value] if isinstance(value, str) else value
            valid = (
                isinstance(values, list)
                and len(values) <= 16
                and all(isinstance(item, str) and bool(item) for item in values)
            )
        if not valid:
            error = ProviderError(f"invalid request control: {name}", provider=provider)
            error.code = "invalid_request"
            raise error

    if provider in ("claude", "fireworks", "lmstudio", "groq", "mistral", "together", "openrouter") and controls.get("stop") is not None:
        stop = controls["stop"]
        values = [stop] if isinstance(stop, str) else stop
        if len(values) > 4:
            error = ProviderError("invalid request control: stop", provider=provider)
            error.code = "invalid_request"
            raise error

    if provider == "perplexity":
        for name in ("presence_penalty", "frequency_penalty"):
            value = controls.get(name)
            if value is not None and value < 0:
                error = ProviderError(f"invalid request control: {name}", provider=provider)
                error.code = "invalid_request"
                raise error

    if provider != "ollama" and any(controls.get(name) is not None for name in ("top_k", "min_p")):
        error = ProviderError("unsupported top_k/min_p sampling controls", provider=provider)
        error.code = "unsupported_capability"
        raise error

    unsupported = {
        "perplexity": ("seed", "stop"),
        "xai": ("presence_penalty", "frequency_penalty", "seed"),
    }.get(provider, ())
    for name in unsupported:
        if controls.get(name) is not None:
            error = ProviderError(f"unsupported request control: {name}", provider=provider)
            error.code = "unsupported_capability"
            raise error


def _validate_logprob_reasoning_controls(
    provider, model, temperature, top_p, logprobs, top_logprobs, reasoning_effort
):
    """Strict facade boundary; legacy warn-and-drop helper remains separate."""
    from nxuskit.errors import ProviderError

    if provider not in ("openai", "ollama") and any(
        value is not None for value in (logprobs, top_logprobs)
    ):
        error = ProviderError("unsupported logprob controls", provider=provider)
        error.code = "unsupported_capability"
        raise error
    if provider != "openai" and reasoning_effort is not None:
        error = ProviderError("unsupported reasoning control", provider=provider)
        error.code = "unsupported_capability"
        raise error
    if (
        model.lower().startswith("gpt-5.4")
        and reasoning_effort
        and reasoning_effort.lower() != "none"
        and any(value is not None for value in (temperature, top_p, logprobs, top_logprobs))
    ):
        error = ProviderError(
            "unsupported sampling/logprob controls with GPT-5.4 reasoning", provider=provider
        )
        error.code = "unsupported_capability"
        raise error


def adapt_gpt54_reasoning_compat(
    request: "ChatRequest",
    reasoning_effort: Optional[str],
) -> "tuple[ChatRequest, list[str]]":
    """GPT-5.4 reasoning-compat warn-and-drop rule.

    When the request targets a GPT-5.4 family model AND ``reasoning_effort``
    is not ``None`` / ``"none"``, the following parameters are incompatible
    with the model's reasoning mode and are dropped with a warning:
    ``temperature``, ``top_p``, and ``logprobs`` / ``top_logprobs``.

    Returns a ``(modified_request, warning_messages)`` tuple. The original
    request is not mutated. Mirrors the Rust ``adapt_gpt54_reasoning_compat``
    and the Go ``adaptGPT54ReasoningCompat`` functions.

    Example:
        >>> req = ChatRequest(model="gpt-5.4", messages=[], temperature=0.7, logprobs=True)
        >>> adapted, warns = adapt_gpt54_reasoning_compat(req, "medium")
        >>> adapted.temperature is None
        True
        >>> adapted.logprobs is None
        True
        >>> any("temperature" in w for w in warns)
        True
    """
    import copy as _copy  # noqa: PLC0415

    if not request.model.lower().startswith("gpt-5.4"):
        return request, []
    if not reasoning_effort or reasoning_effort.lower() == "none":
        return request, []

    adapted = _copy.copy(request)
    warn_msgs: List[str] = []

    if adapted.temperature is not None:
        warn_msgs.append(
            f"GPT-5.4 with reasoning.effort='{reasoning_effort}' does not accept "
            "temperature; dropped"
        )
        adapted.temperature = None
    if adapted.top_p is not None:
        warn_msgs.append(
            f"GPT-5.4 with reasoning.effort='{reasoning_effort}' does not accept top_p; dropped"
        )
        adapted.top_p = None
    if adapted.logprobs is not None or adapted.top_logprobs is not None:
        warn_msgs.append(
            f"GPT-5.4 with reasoning.effort='{reasoning_effort}' does not accept logprobs; dropped"
        )
        adapted.logprobs = None
        adapted.top_logprobs = None

    for msg in warn_msgs:
        warnings.warn(msg, UserWarning, stacklevel=2)

    return adapted, warn_msgs


@dataclass
class TopLogprob:
    """Alternative token probability for a generated position.

    Mirrors the Rust ``nxuskit::TopLogprob`` and the C ABI JSON shape.
    Holds the alternative ``token`` text and its natural-log ``logprob``;
    optional ``bytes`` carries the UTF-8 bytes when the provider returns
    them.
    """

    token: str
    logprob: float
    bytes: Optional[List[int]] = None

    @classmethod
    def from_dict(cls, data: dict) -> "TopLogprob":
        return cls(
            token=str(data.get("token", "")),
            logprob=float(data.get("logprob", 0.0)),
            bytes=list(data["bytes"]) if data.get("bytes") is not None else None,
        )


@dataclass
class TokenLogprob:
    """Probability information for a single generated token.

    Mirrors the Rust ``nxuskit::TokenLogprob`` and the C ABI JSON shape.
    The selected token's probability is held directly; alternatives at the
    same position live in ``top_logprobs``.
    """

    token: str
    logprob: float
    bytes: Optional[List[int]] = None
    top_logprobs: List[TopLogprob] = field(default_factory=list)

    @classmethod
    def from_dict(cls, data: dict) -> "TokenLogprob":
        return cls(
            token=str(data.get("token", "")),
            logprob=float(data.get("logprob", 0.0)),
            bytes=list(data["bytes"]) if data.get("bytes") is not None else None,
            top_logprobs=[TopLogprob.from_dict(t) for t in data.get("top_logprobs", [])],
        )


@dataclass(frozen=True)
class StreamLogprobsDelta:
    """Per-chunk logprob data emitted during streaming.

    Carries the token logprob entries for tokens produced in a single stream
    chunk. Absent (``None`` on ``StreamChunk.logprobs``) means the provider
    does not support streaming logprobs (FR-007).

    Example:
        >>> from nxuskit.types import StreamLogprobsDelta, TokenLogprob
        >>> delta = StreamLogprobsDelta(content=[TokenLogprob(token="Hello", logprob=-0.01)])
        >>> delta.content[0].token
        'Hello'
        >>> delta.content[0].logprob
        -0.01
    """

    content: List[TokenLogprob] = field(default_factory=list)

    @classmethod
    def from_dict(cls, data: dict) -> "StreamLogprobsDelta":
        return cls(
            content=[TokenLogprob.from_dict(t) for t in data.get("content", [])],
        )


@dataclass
class LogprobsData:
    """Token probability data returned by providers that support logprobs.

    Mirrors the Rust ``nxuskit::LogprobsData`` and the C ABI JSON shape.
    Access selected-token logprobs via ``content[i].token`` /
    ``content[i].logprob``; alternative tokens at the same position via
    ``content[i].top_logprobs``.

    Example:
        >>> from nxuskit import LogprobsData, TokenLogprob, TopLogprob
        >>> data = LogprobsData(content=[TokenLogprob(
        ...     token="Hello", logprob=-0.01,
        ...     top_logprobs=[TopLogprob(token="Hi", logprob=-1.2)],
        ... )])
        >>> data.content[0].token
        'Hello'
        >>> data.content[0].top_logprobs[0].token
        'Hi'
    """

    content: List[TokenLogprob] = field(default_factory=list)

    @classmethod
    def from_dict(cls, data: dict) -> "LogprobsData":
        return cls(
            content=[TokenLogprob.from_dict(t) for t in data.get("content", [])],
        )


@dataclass
class ChatResponse:
    """Response from a chat completion request.

    Used by both native HTTP providers and the FFI path.
    """

    content: Optional[str]
    usage: TokenUsage
    model: str
    finish_reason: Optional[str] = None
    tool_calls: Optional[list] = None
    provider: Optional[str] = None
    warnings: List[str] = field(default_factory=list)
    logprobs: Optional[LogprobsData] = None
    thinking: Optional[str] = None
    raw_finish_reason: Optional[str] = None
    # Caller declaration, not observed execution locality or verified support.
    declared_profile: Optional[str] = None
    endpoint_origin: Optional[str] = None
    provider_metadata: Optional[Dict[str, Any]] = field(default=None, kw_only=True)

    @property
    def stop_reason(self) -> Optional[str]:
        """Deprecated: use finish_reason instead."""
        warnings.warn(
            "ChatResponse.stop_reason is deprecated, use finish_reason instead",
            DeprecationWarning,
            stacklevel=2,
        )
        return self.finish_reason


@dataclass
class StreamChunk:
    """Chunk of streamed response data.

    Used by both native HTTP providers and the FFI path.
    Fields default to None for backward compatibility.

    The ``logprobs`` field carries per-token log probability data for tokens
    in this chunk. It is ``None`` when the provider does not support streaming
    logprobs (FR-007).

    Example:
        >>> from nxuskit.types import StreamChunk, StreamLogprobsDelta, TokenLogprob
        >>> chunk = StreamChunk(
        ...     delta="Hello",
        ...     logprobs=StreamLogprobsDelta(content=[TokenLogprob(token="Hello", logprob=-0.01)]),
        ... )
        >>> chunk.logprobs.content[0].token
        'Hello'
    """

    delta: str
    model: Optional[str] = None
    finish_reason: Optional[str] = None
    thinking: Optional[str] = None
    usage: Optional[TokenUsage] = None
    tool_calls: Optional[list] = None
    logprobs: Optional[StreamLogprobsDelta] = None
    raw_finish_reason: Optional[str] = None
    declared_profile: Optional[str] = None
    endpoint_origin: Optional[str] = None

    # Keyword-only preserves every existing positional constructor argument.
    terminal: bool = field(default=False, kw_only=True)
    provider_metadata: Optional[Dict[str, Any]] = field(default=None, kw_only=True)

    def is_final(self) -> bool:
        """Recognize explicit terminality and older reason-bearing chunks."""
        return self.terminal or self.finish_reason is not None

    def has_thinking(self) -> bool:
        """Check if this chunk contains thinking content."""
        return self.thinking is not None and self.thinking != ""

    def has_tool_calls(self) -> bool:
        """Check if this chunk contains tool call deltas."""
        return self.tool_calls is not None and len(self.tool_calls) > 0


@dataclass
class ModelInfo:
    """Information about an available model.

    Includes capability detection helpers matching the Go and Rust SDKs.
    """

    id: str = ""
    name: str = ""
    description: Optional[str] = None
    size_bytes: Optional[int] = None
    context_window: Optional[int] = None
    provider: str = ""
    metadata: dict = field(default_factory=dict)

    def supports_vision(self) -> bool:
        """Check if this model supports vision/image inputs."""
        modalities = self.metadata.get("modalities", "")
        return "vision" in modalities

    def modalities(self) -> List[str]:
        """Get list of supported modalities."""
        raw = self.metadata.get("modalities", "")
        if not raw:
            return ["text"]
        return [m.strip() for m in raw.split(",") if m.strip()]

    def max_images(self) -> Optional[int]:
        """Get maximum number of images supported per request."""
        val = self.metadata.get("max_images", "")
        if not val:
            return None
        try:
            return int(val)
        except (ValueError, TypeError):
            return None

    @classmethod
    def from_dict(cls, data: dict) -> "ModelInfo":
        """Create ModelInfo from a dictionary (FFI JSON deserialization)."""
        return cls(
            id=data.get("id", data.get("name", "")),
            name=data.get("name", ""),
            description=data.get("description"),
            size_bytes=data.get("size_bytes"),
            context_window=data.get("context_window"),
            provider=data.get("provider", ""),
            metadata=data.get("metadata", {}),
        )


@dataclass
class ProviderCapabilities:
    """Capability metadata for an LLM provider.

    Mirrors the Rust ``ProviderCapabilities`` and Go ``ProviderCapabilities``
    structs. Used by parity tests and capability-aware adapters.

    ``supports_streaming_logprobs == True`` implies ``supports_logprobs == True``.
    """

    supports_system_messages: bool = True
    supports_streaming: bool = False
    supports_vision: bool = False
    supports_presence_penalty: bool = False
    supports_frequency_penalty: bool = False
    supports_seed: bool = False
    supports_logprobs: bool = False
    supports_streaming_logprobs: bool = False
    supports_json_mode: bool = False
    supports_json_schema: bool = False
    supports_tools: bool = False
    supports_response_format: bool = False
    supports_top_k: bool = False
    supports_min_p: bool = False
    max_stop_sequences: Optional[int] = None
    max_logprobs: Optional[int] = None


@dataclass
class ToolCallDelta:
    """Incremental tool call data in a streaming chunk."""

    index: int
    id: Optional[str] = None
    type: Optional[str] = None
    function: Optional["FunctionCallDelta"] = None


@dataclass
class FunctionCallDelta:
    """Incremental function call data within a ToolCallDelta."""

    name: Optional[str] = None
    arguments: Optional[str] = None
