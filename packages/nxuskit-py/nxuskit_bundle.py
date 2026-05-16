"""nxusKit - Single-file bundled Python LLM library.

This is a bundled version of nxusKit containing all source code in a single file.
It provides access to multiple LLM providers (Claude, OpenAI, Ollama, Groq, xAI Grok, Mistral,
Fireworks, Together, OpenRouter, Perplexity, LM Studio) with a unified interface.

Usage:
    from nxuskit import Provider, Message

    provider = Provider.claude(api_key="your-key")
    response = provider.chat([Message.user("Hello")])
    print(response.content)
"""

from __future__ import annotations
from abc import ABC, abstractmethod
from abc import abstractmethod
from cffi import FFI
from dataclasses import dataclass
from dataclasses import dataclass, field
from enum import Enum
from enum import IntEnum
from pathlib import Path
from typing import Any
from typing import Any, Dict
from typing import Any, Dict, List, Optional
from typing import Any, Iterator
from typing import Any, Iterator, List, Optional, Protocol, Union
from typing import Any, Iterator, List, Optional, Union
from typing import Any, Optional
from typing import Callable, Iterator, Optional
from typing import Callable, Optional, TypeVar
from typing import Dict, List, Optional
from typing import Iterator, List, Optional
from typing import Iterator, Optional
from typing import List
from typing import List, Optional
from typing import Optional
import asyncio
import base64
import json
import json as _json
import math
import os
import platform
import queue
import random
import re
import requests
import stat
import sys
import time
import warnings



# ============================================================================
# errors.py
# ============================================================================



from typing import Optional

class LLMError(Exception):
    """Base exception for all LLM-related errors."""

    def __init__(
        self,
        message: str,
        status_code: Optional[int] = None,
        provider: Optional[str] = None,
        model: Optional[str] = None,
    ):
        """Initialize LLMError."""
        super().__init__(message)
        self.status_code = status_code
        self.provider = provider
        self.model = model

    @property
    def is_retryable(self) -> bool:
        """Whether this error suggests a retry is appropriate."""
        return False

class AuthenticationError(LLMError):
    """Raised when authentication fails (e.g., invalid API key)."""

    @property
    def is_retryable(self) -> bool:
        """Authentication errors are not retryable."""
        return False

class RateLimitError(LLMError):
    """Raised when rate limit is exceeded."""

    def __init__(
        self,
        message: str,
        status_code: Optional[int] = None,
        provider: Optional[str] = None,
        model: Optional[str] = None,
        retry_after: Optional[float] = None,
    ):
        """Initialize RateLimitError."""
        super().__init__(message, status_code, provider, model)
        self.retry_after = retry_after

    @property
    def is_retryable(self) -> bool:
        """Rate limit errors are retryable."""
        return True

class NetworkError(LLMError):
    """Raised when network communication fails."""

    @property
    def is_retryable(self) -> bool:
        """Network errors are retryable."""
        return True

class TimeoutError(LLMError):
    """Raised when a request times out."""

    @property
    def is_retryable(self) -> bool:
        """Timeout errors are retryable (with potentially longer timeout)."""
        return True

class ProviderError(LLMError):
    """Raised for provider-specific errors."""

    @property
    def is_retryable(self) -> bool:
        """Provider errors may be retryable depending on status code."""
        if self.status_code is None:
            return False
        # 5xx errors are typically retryable
        return 500 <= self.status_code < 600


# ============================================================================
# types.py
# ============================================================================



import warnings
from dataclasses import dataclass, field
from enum import Enum
from typing import Any, Dict, List, Optional

class Role(str, Enum):
    """Message role enumeration."""

    USER = "user"
    ASSISTANT = "assistant"
    SYSTEM = "system"

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

    def is_final(self) -> bool:
        """Check if this is the final chunk (has a finish reason)."""
        return self.finish_reason is not None

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


# ============================================================================
# message.py
# ============================================================================



from dataclasses import dataclass, field
from typing import List

@dataclass
class Message:
    """Represents a message in a conversation."""

    role: Role
    content: str
    images: List[ImageSource] = field(default_factory=list)

    @staticmethod
    def user(content: str) -> "Message":
        """Create a user message."""
        return Message(role=Role.USER, content=content)

    @staticmethod
    def assistant(content: str) -> "Message":
        """Create an assistant message."""
        return Message(role=Role.ASSISTANT, content=content)

    @staticmethod
    def system(content: str) -> "Message":
        """Create a system message."""
        return Message(role=Role.SYSTEM, content=content)

    def with_image_url(self, url: str) -> "Message":
        """Add an image from URL to this message."""
        self.images.append(ImageSource(ImageSourceType.URL, url))
        return self

    def with_image_base64(self, data: str) -> "Message":
        """Add a base64-encoded image to this message."""
        self.images.append(ImageSource(ImageSourceType.BASE64, data))
        return self

    def with_image_file(self, path: str) -> "Message":
        """Add an image from file path to this message."""
        self.images.append(ImageSource(ImageSourceType.FILEPATH, path))
        return self


# ============================================================================
# providers/base.py
# ============================================================================



from abc import ABC, abstractmethod
from typing import Any, Iterator, List, Optional, Union

import requests

class BaseProvider(ABC):
    """Base class for all LLM provider implementations."""

    def __init__(
        self,
        model: str,
        api_key: Optional[str] = None,
        api_url: Optional[str] = None,
        timeout: float = 30.0,
        connect_timeout: Optional[float] = None,
        read_timeout: Optional[float] = None,
    ):
        """Initialize base provider.

        Args:
            model: Default model identifier (overridable per-request).
            api_key: API key for authentication (if required).
            api_url: Base URL for API endpoint.
            timeout: Total timeout in seconds (deprecated, use connect_timeout/read_timeout).
            connect_timeout: Connection timeout in seconds.
            read_timeout: Read timeout in seconds.
        """
        self._model = model
        self._api_key = api_key
        self._api_url = api_url
        self._connect_timeout = connect_timeout if connect_timeout is not None else timeout
        self._read_timeout = read_timeout if read_timeout is not None else timeout

    @property
    def model(self) -> str:
        """Get the default model identifier."""
        return self._model

    @property
    @abstractmethod
    def provider_name(self) -> str:
        """Get the provider name."""
        pass

    def _resolve_model(self, model: Optional[str] = None) -> str:
        """Resolve the effective model for a request.

        Args:
            model: Per-request model override. If None or empty, uses
                   the provider's default model.
        """
        return model if model else self._model

    @abstractmethod
    def chat(
        self,
        messages: List[Message],
        *,
        model: Optional[str] = None,
        temperature: Optional[float] = None,
        max_tokens: Optional[int] = None,
        top_p: Optional[float] = None,
        stop: Optional[Union[str, List[str]]] = None,
        response_format: Optional[ResponseFormat] = None,
        tools: Optional[List[ToolDefinition]] = None,
        tool_choice: Optional[Any] = None,
    ) -> ChatResponse:
        """Send a chat request and get a response.

        Args:
            messages: List of messages to send.
            model: Model override for this request. If None, uses the
                   provider's default model.
            temperature: Sampling temperature (0.0-2.0).
            max_tokens: Maximum tokens to generate.
            top_p: Nucleus sampling parameter.
            stop: Stop sequences.
            response_format: Response format (JSON or TEXT).
            tools: List of tool definitions for function calling.
            tool_choice: Tool selection strategy.
        """
        pass

    @abstractmethod
    def chat_stream(
        self,
        messages: List[Message],
        *,
        model: Optional[str] = None,
        temperature: Optional[float] = None,
        max_tokens: Optional[int] = None,
        top_p: Optional[float] = None,
        stop: Optional[Union[str, List[str]]] = None,
        response_format: Optional[ResponseFormat] = None,
        tools: Optional[List[ToolDefinition]] = None,
        tool_choice: Optional[Any] = None,
    ) -> Iterator[StreamChunk]:
        """Send a chat request and stream the response.

        Args:
            messages: List of messages to send.
            model: Model override for this request.
            temperature: Sampling temperature (0.0-2.0).
            max_tokens: Maximum tokens to generate.
            top_p: Nucleus sampling parameter.
            stop: Stop sequences.
            response_format: Response format (JSON or TEXT).
            tools: List of tool definitions for function calling.
            tool_choice: Tool selection strategy.
        """
        pass

    def list_models(self) -> List[ModelInfo]:
        """List available models from this provider.

        Returns an empty list by default. Providers that support model
        listing should override this method.
        """
        return []

    def _handle_error(self, status_code: int, response_text: str) -> None:
        """Handle HTTP errors and raise appropriate exceptions."""
        if status_code == 401:
            raise AuthenticationError(
                f"Authentication failed: {response_text[:200]}",
                status_code=status_code,
                provider=self.provider_name,
                model=self._model,
            )
        elif status_code == 429:
            raise RateLimitError(
                f"Rate limited: {response_text[:200]}",
                status_code=status_code,
                provider=self.provider_name,
                model=self._model,
            )
        elif 500 <= status_code < 600:
            raise ProviderError(
                f"Provider error: {response_text[:200]}",
                status_code=status_code,
                provider=self.provider_name,
                model=self._model,
            )
        elif 400 <= status_code < 500:
            raise ProviderError(
                f"Client error: {response_text[:200]}",
                status_code=status_code,
                provider=self.provider_name,
                model=self._model,
            )
        else:
            raise LLMError(
                f"Unexpected error: {response_text[:200]}",
                status_code=status_code,
                provider=self.provider_name,
                model=self._model,
            )

    def _make_request(
        self,
        method: str,
        url: str,
        headers: dict,
        json_data: dict,
        stream: bool = False,
    ) -> requests.Response:
        """Make HTTP request with error handling."""
        try:
            timeout = (self._connect_timeout, self._read_timeout)
            response = requests.request(
                method=method,
                url=url,
                headers=headers,
                json=json_data,
                timeout=timeout,
                stream=stream,
            )

            if response.status_code >= 400:
                self._handle_error(response.status_code, response.text)

            return response
        except requests.exceptions.Timeout as e:
            raise TimeoutError(
                f"Request timed out: {str(e)}",
                provider=self.provider_name,
                model=self._model,
            ) from e
        except requests.exceptions.RequestException as e:
            raise NetworkError(
                f"Network error: {str(e)}",
                provider=self.provider_name,
                model=self._model,
            ) from e


# ============================================================================
# providers/openai_compatible.py
# ============================================================================



import base64
import json
from abc import abstractmethod
from typing import Any, Iterator, List, Optional, Union

class OpenAICompatibleProvider(BaseProvider):
    """Base class for providers using OpenAI-compatible API format."""

    def _api_endpoint(self, path: str) -> str:
        """Build a versioned API endpoint without duplicating /v1."""
        base = self._api_url.rstrip("/")
        if base.endswith("/v1"):
            return f"{base}{path}"
        return f"{base}/v1{path}"

    def chat(
        self,
        messages: List[Message],
        *,
        model: Optional[str] = None,
        temperature: Optional[float] = None,
        max_tokens: Optional[int] = None,
        top_p: Optional[float] = None,
        stop: Optional[Union[str, List[str]]] = None,
        response_format: Optional[ResponseFormat] = None,
        tools: Optional[List[ToolDefinition]] = None,
        tool_choice: Optional[Any] = None,
    ) -> ChatResponse:
        """Send a chat request using OpenAI-compatible format."""
        effective_model = self._resolve_model(model)
        request_body = self._build_request(
            messages,
            effective_model=effective_model,
            stream=False,
            temperature=temperature,
            max_tokens=max_tokens,
            top_p=top_p,
            stop=stop,
            response_format=response_format,
            tools=tools,
            tool_choice=tool_choice,
        )
        headers = self._build_headers()

        response = self._make_request(
            method="POST",
            url=self._api_endpoint("/chat/completions"),
            headers=headers,
            json_data=request_body,
        )

        data = response.json()
        return self._parse_response(data, effective_model)

    def chat_stream(
        self,
        messages: List[Message],
        *,
        model: Optional[str] = None,
        temperature: Optional[float] = None,
        max_tokens: Optional[int] = None,
        top_p: Optional[float] = None,
        stop: Optional[Union[str, List[str]]] = None,
        response_format: Optional[ResponseFormat] = None,
        tools: Optional[List[ToolDefinition]] = None,
        tool_choice: Optional[Any] = None,
    ) -> Iterator[StreamChunk]:
        """Stream a chat response using OpenAI-compatible format."""
        effective_model = self._resolve_model(model)
        request_body = self._build_request(
            messages,
            effective_model=effective_model,
            stream=True,
            temperature=temperature,
            max_tokens=max_tokens,
            top_p=top_p,
            stop=stop,
            response_format=response_format,
            tools=tools,
            tool_choice=tool_choice,
        )
        headers = self._build_headers()

        response = self._make_request(
            method="POST",
            url=self._api_endpoint("/chat/completions"),
            headers=headers,
            json_data=request_body,
            stream=True,
        )

        for line in response.iter_lines():
            if not line:
                continue

            line = line.decode("utf-8") if isinstance(line, bytes) else line

            if line.startswith("data: "):
                data_str = line[6:]
                if data_str == "[DONE]":
                    break

                try:
                    data = json.loads(data_str)
                    chunk = self._parse_stream_event(data, effective_model)
                    if chunk:
                        yield chunk
                except json.JSONDecodeError:
                    continue

    def list_models(self) -> List[ModelInfo]:
        """List available models from this provider."""
        try:
            headers = self._build_headers()
            response = self._make_request(
                method="GET",
                url=self._api_endpoint("/models"),
                headers=headers,
                json_data={},
            )
            data = response.json()
            models = []
            for m in data.get("data", []):
                models.append(
                    ModelInfo(
                        id=m.get("id", ""),
                        name=m.get("id", ""),
                        provider=self.provider_name,
                    )
                )
            return models
        except Exception:
            return []

    @abstractmethod
    def _build_headers(self) -> dict:
        """Build request headers. Subclasses implement authentication."""
        pass

    def _resolve_image_mime(self, image) -> str:
        """Resolve MIME type for an image source."""
        if image.media_type:
            return image.media_type
        if image.source_type == ImageSourceType.FILEPATH:
            return detect_image_type(image.data)
        if image.source_type == ImageSourceType.BASE64:
            return detect_image_type(image.data)
        return "image/jpeg"

    def _build_request(
        self,
        messages: List[Message],
        effective_model: str,
        stream: bool = False,
        temperature: Optional[float] = None,
        max_tokens: Optional[int] = None,
        top_p: Optional[float] = None,
        stop: Optional[Union[str, List[str]]] = None,
        response_format: Optional[ResponseFormat] = None,
        tools: Optional[List[ToolDefinition]] = None,
        tool_choice: Optional[Any] = None,
    ) -> dict:
        """Build request body for OpenAI-compatible API."""
        formatted_messages = []

        for msg in messages:
            formatted_msg = {
                "role": msg.role.value,
                "content": msg.content,
            }

            if msg.images:
                content_list = [{"type": "text", "text": msg.content}]

                for image in msg.images:
                    mime = self._resolve_image_mime(image)
                    if image.source_type == ImageSourceType.URL:
                        content_list.append(
                            {
                                "type": "image_url",
                                "image_url": {"url": image.data},
                            }
                        )
                    elif image.source_type == ImageSourceType.BASE64:
                        content_list.append(
                            {
                                "type": "image_url",
                                "image_url": {"url": f"data:{mime};base64,{image.data}"},
                            }
                        )
                    elif image.source_type == ImageSourceType.FILEPATH:
                        with open(image.data, "rb") as f:
                            file_data = base64.b64encode(f.read()).decode("utf-8")
                            file_mime = self._resolve_image_mime(image)
                            content_list.append(
                                {
                                    "type": "image_url",
                                    "image_url": {"url": f"data:{file_mime};base64,{file_data}"},
                                }
                            )

                formatted_msg["content"] = content_list

            formatted_messages.append(formatted_msg)

        request_body: dict[str, Any] = {
            "model": effective_model,
            "messages": formatted_messages,
            "stream": stream,
        }

        if temperature is not None:
            request_body["temperature"] = temperature
        if max_tokens is not None:
            request_body["max_tokens"] = max_tokens
        if top_p is not None:
            request_body["top_p"] = top_p
        if stop is not None:
            request_body["stop"] = stop if isinstance(stop, list) else [stop]

        if response_format == ResponseFormat.JSON:
            request_body["response_format"] = {"type": "json_object"}

        if tools:
            request_body["tools"] = [t.to_dict() for t in tools]
        if tool_choice is not None:
            request_body["tool_choice"] = tool_choice

        return request_body

    def _parse_response(self, data: dict, effective_model: str) -> ChatResponse:
        """Parse OpenAI-compatible API response."""
        content = None
        finish_reason = None
        tool_calls = None

        if "choices" in data and len(data["choices"]) > 0:
            choice = data["choices"][0]
            if "message" in choice:
                content = choice["message"].get("content")
                finish_reason = choice.get("finish_reason")
                raw_tc = choice["message"].get("tool_calls")
                if raw_tc:
                    tool_calls = [ToolCall.from_dict(tc) for tc in raw_tc]

        usage = data.get("usage", {})
        token_usage = TokenUsage(
            prompt_tokens=usage.get("prompt_tokens", 0),
            completion_tokens=usage.get("completion_tokens", 0),
            total_tokens=usage.get("total_tokens", 0),
        )

        return ChatResponse(
            content=content,
            usage=token_usage,
            model=effective_model,
            finish_reason=finish_reason,
            tool_calls=tool_calls,
        )

    def _parse_stream_event(self, data: dict, effective_model: str) -> Optional[StreamChunk]:
        """Parse a single stream event from OpenAI-compatible API."""
        if "choices" not in data or len(data["choices"]) == 0:
            return None

        choice = data["choices"][0]
        delta = choice.get("delta", {})
        finish_reason = choice.get("finish_reason")

        content = delta.get("content", "")
        if content or finish_reason:
            # Parse usage from the final chunk if present
            usage = None
            if "usage" in data and data["usage"]:
                u = data["usage"]
                usage = TokenUsage(
                    prompt_tokens=u.get("prompt_tokens", 0),
                    completion_tokens=u.get("completion_tokens", 0),
                    total_tokens=u.get("total_tokens", 0),
                )

            return StreamChunk(
                delta=content or "",
                model=effective_model,
                finish_reason=finish_reason,
                usage=usage,
                tool_calls=delta.get("tool_calls"),
            )

        return None


# ============================================================================
# providers/claude.py
# ============================================================================



import base64
import json
import os
from typing import Any, Iterator, List, Optional, Union

class ClaudeProvider(BaseProvider):
    """Provider for Anthropic's Claude models."""

    DEFAULT_API_URL = "https://api.anthropic.com"
    API_VERSION = "2023-06-01"

    def __init__(
        self,
        model: str,
        api_key: Optional[str] = None,
        api_url: Optional[str] = None,
        timeout: float = 30.0,
        connect_timeout: Optional[float] = None,
        read_timeout: Optional[float] = None,
    ):
        """Initialize Claude provider."""
        if api_key is None:
            api_key = os.getenv("ANTHROPIC_API_KEY")
        if api_url is None:
            api_url = self.DEFAULT_API_URL

        super().__init__(model, api_key, api_url, timeout, connect_timeout, read_timeout)

    @property
    def provider_name(self) -> str:
        """Get provider name."""
        return "claude"

    def chat(
        self,
        messages: List[Message],
        *,
        model: Optional[str] = None,
        temperature: Optional[float] = None,
        max_tokens: Optional[int] = None,
        top_p: Optional[float] = None,
        stop: Optional[Union[str, List[str]]] = None,
        response_format: Optional[ResponseFormat] = None,
        tools: Optional[List[ToolDefinition]] = None,
        tool_choice: Optional[Any] = None,
    ) -> ChatResponse:
        """Send a chat request to Claude."""
        effective_model = self._resolve_model(model)
        request_body = self._build_request(
            messages,
            effective_model=effective_model,
            stream=False,
            temperature=temperature,
            max_tokens=max_tokens,
            top_p=top_p,
            stop=stop,
            response_format=response_format,
            tools=tools,
            tool_choice=tool_choice,
        )
        headers = self._build_headers()

        response = self._make_request(
            method="POST",
            url=f"{self._api_url}/v1/messages",
            headers=headers,
            json_data=request_body,
        )

        data = response.json()
        return self._parse_response(data, effective_model)

    def chat_stream(
        self,
        messages: List[Message],
        *,
        model: Optional[str] = None,
        temperature: Optional[float] = None,
        max_tokens: Optional[int] = None,
        top_p: Optional[float] = None,
        stop: Optional[Union[str, List[str]]] = None,
        response_format: Optional[ResponseFormat] = None,
        tools: Optional[List[ToolDefinition]] = None,
        tool_choice: Optional[Any] = None,
    ) -> Iterator[StreamChunk]:
        """Stream a chat response from Claude."""
        effective_model = self._resolve_model(model)
        request_body = self._build_request(
            messages,
            effective_model=effective_model,
            stream=True,
            temperature=temperature,
            max_tokens=max_tokens,
            top_p=top_p,
            stop=stop,
            response_format=response_format,
            tools=tools,
            tool_choice=tool_choice,
        )
        headers = self._build_headers()

        response = self._make_request(
            method="POST",
            url=f"{self._api_url}/v1/messages",
            headers=headers,
            json_data=request_body,
            stream=True,
        )

        for line in response.iter_lines():
            if not line:
                continue

            line = line.decode("utf-8") if isinstance(line, bytes) else line

            if line.startswith("data: "):
                data_str = line[6:]
                if data_str == "[DONE]":
                    break

                try:
                    data = json.loads(data_str)
                    chunk = self._parse_stream_event(data, effective_model)
                    if chunk:
                        yield chunk
                except json.JSONDecodeError:
                    continue

    def list_models(self) -> List[ModelInfo]:
        """List available Claude models."""
        try:
            headers = self._build_headers()
            response = self._make_request(
                method="GET",
                url=f"{self._api_url}/v1/models",
                headers=headers,
                json_data={},
            )
            data = response.json()
            models = []
            for m in data.get("data", []):
                models.append(
                    ModelInfo(
                        id=m.get("id", ""),
                        name=m.get("display_name", m.get("id", "")),
                        provider="claude",
                    )
                )
            return models
        except Exception:
            return []

    def _build_headers(self) -> dict:
        """Build request headers for Claude API."""
        return {
            "anthropic-version": self.API_VERSION,
            "content-type": "application/json",
            "x-api-key": self._api_key,
        }

    def _resolve_image_mime(self, image) -> str:
        """Resolve MIME type for an image source."""
        if image.media_type:
            return image.media_type
        if image.source_type in (ImageSourceType.FILEPATH, ImageSourceType.BASE64):
            return detect_image_type(image.data)
        return "image/jpeg"

    def _build_request(
        self,
        messages: List[Message],
        effective_model: str,
        stream: bool = False,
        temperature: Optional[float] = None,
        max_tokens: Optional[int] = None,
        top_p: Optional[float] = None,
        stop: Optional[Union[str, List[str]]] = None,
        response_format: Optional[ResponseFormat] = None,
        tools: Optional[List[ToolDefinition]] = None,
        tool_choice: Optional[Any] = None,
    ) -> dict:
        """Build request body for Claude API."""
        system_messages = [m for m in messages if m.role == Role.SYSTEM]
        other_messages = [m for m in messages if m.role != Role.SYSTEM]

        system_prompt = ""
        if system_messages:
            system_prompt = system_messages[0].content

        if response_format == ResponseFormat.JSON:
            json_instruction = (
                "Respond with valid JSON only. "
                "Do not include any text before or after the JSON object."
            )
            if system_prompt:
                system_prompt = f"{system_prompt}\n\n{json_instruction}"
            else:
                system_prompt = json_instruction

        formatted_messages = []
        for msg in other_messages:
            formatted_msg: dict[str, Any] = {"role": msg.role.value, "content": []}

            formatted_msg["content"].append({"type": "text", "text": msg.content})

            for image in msg.images:
                mime = self._resolve_image_mime(image)
                if image.source_type == ImageSourceType.URL:
                    formatted_msg["content"].append(
                        {
                            "type": "image",
                            "source": {"type": "url", "url": image.data},
                        }
                    )
                elif image.source_type == ImageSourceType.BASE64:
                    formatted_msg["content"].append(
                        {
                            "type": "image",
                            "source": {
                                "type": "base64",
                                "media_type": mime,
                                "data": image.data,
                            },
                        }
                    )
                elif image.source_type == ImageSourceType.FILEPATH:
                    with open(image.data, "rb") as f:
                        file_data = base64.b64encode(f.read()).decode("utf-8")
                        file_mime = self._resolve_image_mime(image)
                        formatted_msg["content"].append(
                            {
                                "type": "image",
                                "source": {
                                    "type": "base64",
                                    "media_type": file_mime,
                                    "data": file_data,
                                },
                            }
                        )

            formatted_messages.append(formatted_msg)

        request_body: dict[str, Any] = {
            "model": effective_model,
            "max_tokens": max_tokens or 4096,
            "messages": formatted_messages,
            "stream": stream,
        }

        if temperature is not None:
            request_body["temperature"] = temperature
        if top_p is not None:
            request_body["top_p"] = top_p
        if stop is not None:
            request_body["stop_sequences"] = stop if isinstance(stop, list) else [stop]

        if system_prompt:
            request_body["system"] = system_prompt

        # Tool calling (Anthropic format)
        if tools:
            request_body["tools"] = [
                {
                    "name": t.function.name,
                    "description": t.function.description,
                    "input_schema": t.function.parameters,
                }
                for t in tools
            ]
        if tool_choice is not None:
            # Convert OpenAI-style tool_choice to Anthropic format
            if tool_choice == "auto":
                request_body["tool_choice"] = {"type": "auto"}
            elif tool_choice == "none":
                pass  # Anthropic doesn't have explicit "none"
            elif tool_choice == "required":
                request_body["tool_choice"] = {"type": "any"}
            elif isinstance(tool_choice, dict) and "function" in tool_choice:
                request_body["tool_choice"] = {
                    "type": "tool",
                    "name": tool_choice["function"]["name"],
                }
            else:
                request_body["tool_choice"] = tool_choice

        return request_body

    def _parse_response(self, data: dict, effective_model: str) -> ChatResponse:
        """Parse Claude API response."""
        content = None
        tool_calls = None

        if "content" in data and isinstance(data["content"], list):
            text_parts = []
            tc_list = []
            for block in data["content"]:
                if block.get("type") == "text":
                    text_parts.append(block.get("text", ""))
                elif block.get("type") == "tool_use":
                    tc_list.append(
                        ToolCall(
                            id=block.get("id", ""),
                            type="function",
                            function=type(
                                "FunctionCall",
                                (),
                                {
                                    "name": block.get("name", ""),
                                    "arguments": json.dumps(block.get("input", {})),
                                },
                            )(),
                        )
                    )
            if text_parts:
                content = "".join(text_parts)
            if tc_list:
                tool_calls = tc_list

        usage = data.get("usage", {})
        token_usage = TokenUsage(
            input_tokens=usage.get("input_tokens", 0),
            output_tokens=usage.get("output_tokens", 0),
        )

        return ChatResponse(
            content=content,
            usage=token_usage,
            model=effective_model,
            finish_reason=data.get("stop_reason"),
            tool_calls=tool_calls,
        )

    def _parse_stream_event(self, data: dict, effective_model: str) -> Optional[StreamChunk]:
        """Parse a single stream event from Claude."""
        event_type = data.get("type")

        if event_type == "content_block_delta":
            delta_data = data.get("delta", {})
            if delta_data.get("type") == "text_delta":
                return StreamChunk(
                    delta=delta_data.get("text", ""),
                    model=effective_model,
                )

        elif event_type == "message_delta":
            # Final event with stop reason and usage
            delta_data = data.get("delta", {})
            usage_data = data.get("usage", {})
            usage = None
            if usage_data:
                usage = TokenUsage(
                    output_tokens=usage_data.get("output_tokens", 0),
                )
            return StreamChunk(
                delta="",
                model=effective_model,
                finish_reason=delta_data.get("stop_reason"),
                usage=usage,
            )

        return None


# ============================================================================
# providers/openai.py
# ============================================================================



import os
from typing import Optional

class OpenAIProvider(OpenAICompatibleProvider):
    """Provider for OpenAI models."""

    DEFAULT_API_URL = "https://api.openai.com"

    def __init__(
        self,
        model: str,
        api_key: Optional[str] = None,
        api_url: Optional[str] = None,
        timeout: float = 30.0,
        connect_timeout: Optional[float] = None,
        read_timeout: Optional[float] = None,
    ):
        """Initialize OpenAI provider."""
        if api_key is None:
            api_key = os.getenv("OPENAI_API_KEY")
        if api_url is None:
            api_url = self.DEFAULT_API_URL

        super().__init__(model, api_key, api_url, timeout, connect_timeout, read_timeout)

    @property
    def provider_name(self) -> str:
        """Get provider name."""
        return "openai"

    def _build_headers(self) -> dict:
        """Build request headers for OpenAI API."""
        return {
            "content-type": "application/json",
            "authorization": f"Bearer {self._api_key}",
        }


# ============================================================================
# providers/ollama.py
# ============================================================================



import base64
import json
from typing import Any, Iterator, List, Optional, Union

class OllamaProvider(BaseProvider):
    """Provider for Ollama local models."""

    DEFAULT_API_URL = "http://localhost:11434"

    def __init__(
        self,
        model: str,
        api_url: Optional[str] = None,
        timeout: float = 30.0,
        connect_timeout: Optional[float] = None,
        read_timeout: Optional[float] = None,
    ):
        """Initialize Ollama provider."""
        if api_url is None:
            api_url = self.DEFAULT_API_URL

        super().__init__(
            model,
            api_key=None,
            api_url=api_url,
            timeout=timeout,
            connect_timeout=connect_timeout,
            read_timeout=read_timeout,
        )

    @property
    def provider_name(self) -> str:
        """Get provider name."""
        return "ollama"

    def chat(
        self,
        messages: List[Message],
        *,
        model: Optional[str] = None,
        temperature: Optional[float] = None,
        max_tokens: Optional[int] = None,
        top_p: Optional[float] = None,
        stop: Optional[Union[str, List[str]]] = None,
        response_format: Optional[ResponseFormat] = None,
        tools: Optional[List[ToolDefinition]] = None,
        tool_choice: Optional[Any] = None,
    ) -> ChatResponse:
        """Send a chat request to Ollama."""
        effective_model = self._resolve_model(model)
        request_body = self._build_request(
            messages,
            effective_model=effective_model,
            stream=False,
            temperature=temperature,
            max_tokens=max_tokens,
            top_p=top_p,
            stop=stop,
            response_format=response_format,
            tools=tools,
        )
        headers = self._build_headers()

        response = self._make_request(
            method="POST",
            url=f"{self._api_url}/api/chat",
            headers=headers,
            json_data=request_body,
        )

        data = response.json()
        return self._parse_response(data, effective_model)

    def chat_stream(
        self,
        messages: List[Message],
        *,
        model: Optional[str] = None,
        temperature: Optional[float] = None,
        max_tokens: Optional[int] = None,
        top_p: Optional[float] = None,
        stop: Optional[Union[str, List[str]]] = None,
        response_format: Optional[ResponseFormat] = None,
        tools: Optional[List[ToolDefinition]] = None,
        tool_choice: Optional[Any] = None,
    ) -> Iterator[StreamChunk]:
        """Stream a chat response from Ollama."""
        effective_model = self._resolve_model(model)
        request_body = self._build_request(
            messages,
            effective_model=effective_model,
            stream=True,
            temperature=temperature,
            max_tokens=max_tokens,
            top_p=top_p,
            stop=stop,
            response_format=response_format,
            tools=tools,
        )
        headers = self._build_headers()

        response = self._make_request(
            method="POST",
            url=f"{self._api_url}/api/chat",
            headers=headers,
            json_data=request_body,
            stream=True,
        )

        for line in response.iter_lines():
            if not line:
                continue

            line = line.decode("utf-8") if isinstance(line, bytes) else line

            try:
                data = json.loads(line)
                chunk = self._parse_stream_event(data, effective_model)
                if chunk:
                    yield chunk
            except json.JSONDecodeError:
                continue

    def list_models(self) -> List[ModelInfo]:
        """List available models from Ollama."""
        try:
            headers = self._build_headers()
            response = self._make_request(
                method="GET",
                url=f"{self._api_url}/api/tags",
                headers=headers,
                json_data={},
            )
            data = response.json()
            models = []
            for m in data.get("models", []):
                details = m.get("details", {})
                metadata: dict[str, str] = {}
                if details.get("family"):
                    metadata["family"] = details["family"]
                if details.get("quantization_level"):
                    metadata["quantization_level"] = details["quantization_level"]
                # Ollama doesn't report modalities directly
                models.append(
                    ModelInfo(
                        id=m.get("name", ""),
                        name=m.get("name", ""),
                        size_bytes=m.get("size"),
                        provider="ollama",
                        metadata=metadata,
                    )
                )
            return models
        except Exception:
            return []

    def _build_headers(self) -> dict:
        """Build request headers for Ollama API."""
        return {"content-type": "application/json"}

    def _build_request(
        self,
        messages: List[Message],
        effective_model: str,
        stream: bool = False,
        temperature: Optional[float] = None,
        max_tokens: Optional[int] = None,
        top_p: Optional[float] = None,
        stop: Optional[Union[str, List[str]]] = None,
        response_format: Optional[ResponseFormat] = None,
        tools: Optional[List[ToolDefinition]] = None,
    ) -> dict:
        """Build request body for Ollama API."""
        formatted_messages = []

        for msg in messages:
            formatted_msg: dict[str, Any] = {
                "role": msg.role.value,
                "content": msg.content,
            }

            images = []
            for image in msg.images:
                if image.source_type == ImageSourceType.BASE64:
                    images.append(image.data)
                elif image.source_type == ImageSourceType.FILEPATH:
                    with open(image.data, "rb") as f:
                        file_data = base64.b64encode(f.read()).decode("utf-8")
                        images.append(file_data)

            if images:
                formatted_msg["images"] = images

            formatted_messages.append(formatted_msg)

        request_body: dict[str, Any] = {
            "model": effective_model,
            "messages": formatted_messages,
            "stream": stream,
        }

        if temperature is not None:
            request_body["temperature"] = temperature
        if max_tokens is not None:
            request_body["num_predict"] = max_tokens
        if top_p is not None:
            request_body["top_p"] = top_p
        if stop is not None:
            request_body["stop"] = stop if isinstance(stop, list) else [stop]
        if response_format == ResponseFormat.JSON:
            request_body["format"] = "json"

        # Ollama supports OpenAI-compatible tool format
        if tools:
            request_body["tools"] = [t.to_dict() for t in tools]

        return request_body

    def _parse_response(self, data: dict, effective_model: str) -> ChatResponse:
        """Parse Ollama API response."""
        content = None

        if "message" in data:
            content = data["message"].get("content", "")

        prompt_eval_count = data.get("prompt_eval_count", 0)
        eval_count = data.get("eval_count", 0)

        token_usage = TokenUsage(
            input_tokens=prompt_eval_count,
            output_tokens=eval_count,
        )

        # Ollama signals completion with "done": true
        finish_reason = "stop" if data.get("done") else None

        return ChatResponse(
            content=content,
            usage=token_usage,
            model=effective_model,
            finish_reason=finish_reason,
        )

    def _parse_stream_event(self, data: dict, effective_model: str) -> Optional[StreamChunk]:
        """Parse a single stream event from Ollama."""
        if "message" not in data:
            return None

        message = data["message"]
        content = message.get("content", "")
        is_done = data.get("done", False)

        if content or is_done:
            usage = None
            finish_reason = None
            if is_done:
                finish_reason = "stop"
                prompt_eval = data.get("prompt_eval_count", 0)
                eval_count = data.get("eval_count", 0)
                if prompt_eval or eval_count:
                    usage = TokenUsage(
                        input_tokens=prompt_eval,
                        output_tokens=eval_count,
                    )

            return StreamChunk(
                delta=content,
                model=effective_model,
                finish_reason=finish_reason,
                usage=usage,
            )

        return None


# ============================================================================
# providers/groq.py
# ============================================================================



import os
from typing import Optional

class GroqProvider(OpenAICompatibleProvider):
    """Provider for Groq models."""

    DEFAULT_API_URL = "https://api.groq.com/openai"

    def __init__(
        self,
        model: str,
        api_key: Optional[str] = None,
        api_url: Optional[str] = None,
        timeout: float = 30.0,
        connect_timeout: Optional[float] = None,
        read_timeout: Optional[float] = None,
    ):
        """Initialize Groq provider."""
        if api_key is None:
            api_key = os.getenv("GROQ_API_KEY")
        if api_url is None:
            api_url = self.DEFAULT_API_URL

        super().__init__(model, api_key, api_url, timeout, connect_timeout, read_timeout)

    @property
    def provider_name(self) -> str:
        """Get provider name."""
        return "groq"

    def _build_headers(self) -> dict:
        """Build request headers for Groq API."""
        return {
            "content-type": "application/json",
            "authorization": f"Bearer {self._api_key}",
        }


# ============================================================================
# providers/xai.py
# ============================================================================



import os
from typing import Optional

class XaiProvider(OpenAICompatibleProvider):
    """Provider for xAI Grok models."""

    DEFAULT_API_URL = "https://api.x.ai/v1"

    def __init__(
        self,
        model: str,
        api_key: Optional[str] = None,
        api_url: Optional[str] = None,
        timeout: float = 30.0,
        connect_timeout: Optional[float] = None,
        read_timeout: Optional[float] = None,
    ):
        """Initialize xAI Grok provider."""
        if api_key is None:
            api_key = os.getenv("XAI_API_KEY")
        if api_url is None:
            api_url = self.DEFAULT_API_URL

        super().__init__(model, api_key, api_url, timeout, connect_timeout, read_timeout)

    @property
    def provider_name(self) -> str:
        """Get provider name."""
        return "xai"

    def _build_headers(self) -> dict:
        """Build request headers for xAI API."""
        return {
            "content-type": "application/json",
            "authorization": f"Bearer {self._api_key}",
        }


# ============================================================================
# providers/mistral.py
# ============================================================================



import os
from typing import Optional

class MistralProvider(OpenAICompatibleProvider):
    """Provider for Mistral models."""

    DEFAULT_API_URL = "https://api.mistral.ai"

    def __init__(
        self,
        model: str,
        api_key: Optional[str] = None,
        api_url: Optional[str] = None,
        timeout: float = 30.0,
        connect_timeout: Optional[float] = None,
        read_timeout: Optional[float] = None,
    ):
        """Initialize Mistral provider."""
        if api_key is None:
            api_key = os.getenv("MISTRAL_API_KEY")
        if api_url is None:
            api_url = self.DEFAULT_API_URL

        super().__init__(model, api_key, api_url, timeout, connect_timeout, read_timeout)

    @property
    def provider_name(self) -> str:
        """Get provider name."""
        return "mistral"

    def _build_headers(self) -> dict:
        """Build request headers for Mistral API."""
        return {
            "content-type": "application/json",
            "authorization": f"Bearer {self._api_key}",
        }


# ============================================================================
# providers/fireworks.py
# ============================================================================



import os
from typing import Optional

class FireworksProvider(OpenAICompatibleProvider):
    """Provider for Fireworks models."""

    DEFAULT_API_URL = "https://api.fireworks.ai/inference"

    def __init__(
        self,
        model: str,
        api_key: Optional[str] = None,
        api_url: Optional[str] = None,
        timeout: float = 30.0,
        connect_timeout: Optional[float] = None,
        read_timeout: Optional[float] = None,
    ):
        """Initialize Fireworks provider."""
        if api_key is None:
            api_key = os.getenv("FIREWORKS_API_KEY")
        if api_url is None:
            api_url = self.DEFAULT_API_URL

        super().__init__(model, api_key, api_url, timeout, connect_timeout, read_timeout)

    @property
    def provider_name(self) -> str:
        """Get provider name."""
        return "fireworks"

    def _build_headers(self) -> dict:
        """Build request headers for Fireworks API."""
        return {
            "content-type": "application/json",
            "authorization": f"Bearer {self._api_key}",
        }


# ============================================================================
# providers/together.py
# ============================================================================



import os
from typing import Optional

class TogetherProvider(OpenAICompatibleProvider):
    """Provider for Together models."""

    DEFAULT_API_URL = "https://api.together.xyz"

    def __init__(
        self,
        model: str,
        api_key: Optional[str] = None,
        api_url: Optional[str] = None,
        timeout: float = 30.0,
        connect_timeout: Optional[float] = None,
        read_timeout: Optional[float] = None,
    ):
        """Initialize Together provider."""
        if api_key is None:
            api_key = os.getenv("TOGETHER_API_KEY")
        if api_url is None:
            api_url = self.DEFAULT_API_URL

        super().__init__(model, api_key, api_url, timeout, connect_timeout, read_timeout)

    @property
    def provider_name(self) -> str:
        """Get provider name."""
        return "together"

    def _build_headers(self) -> dict:
        """Build request headers for Together API."""
        return {
            "content-type": "application/json",
            "authorization": f"Bearer {self._api_key}",
        }


# ============================================================================
# providers/openrouter.py
# ============================================================================



import os
from typing import Optional

class OpenRouterProvider(OpenAICompatibleProvider):
    """Provider for OpenRouter models."""

    DEFAULT_API_URL = "https://openrouter.ai/api"

    def __init__(
        self,
        model: str,
        api_key: Optional[str] = None,
        api_url: Optional[str] = None,
        timeout: float = 30.0,
        connect_timeout: Optional[float] = None,
        read_timeout: Optional[float] = None,
    ):
        """Initialize OpenRouter provider."""
        if api_key is None:
            api_key = os.getenv("OPENROUTER_API_KEY")
        if api_url is None:
            api_url = self.DEFAULT_API_URL

        super().__init__(model, api_key, api_url, timeout, connect_timeout, read_timeout)

    @property
    def provider_name(self) -> str:
        """Get provider name."""
        return "openrouter"

    def _build_headers(self) -> dict:
        """Build request headers for OpenRouter API."""
        headers = {
            "content-type": "application/json",
            "authorization": f"Bearer {self._api_key}",
        }
        # OpenRouter requires HTTP-Referer header for attribution
        headers["HTTP-Referer"] = "https://github.com/nxus-SYSTEMS/nxusKit"
        return headers


# ============================================================================
# providers/perplexity.py
# ============================================================================



import os
from typing import Optional

class PerplexityProvider(OpenAICompatibleProvider):
    """Provider for Perplexity models."""

    DEFAULT_API_URL = "https://api.perplexity.ai"

    def __init__(
        self,
        model: str,
        api_key: Optional[str] = None,
        api_url: Optional[str] = None,
        timeout: float = 30.0,
        connect_timeout: Optional[float] = None,
        read_timeout: Optional[float] = None,
    ):
        """Initialize Perplexity provider."""
        if api_key is None:
            api_key = os.getenv("PERPLEXITY_API_KEY")
        if api_url is None:
            api_url = self.DEFAULT_API_URL

        super().__init__(model, api_key, api_url, timeout, connect_timeout, read_timeout)

    @property
    def provider_name(self) -> str:
        """Get provider name."""
        return "perplexity"

    def _build_headers(self) -> dict:
        """Build request headers for Perplexity API."""
        return {
            "content-type": "application/json",
            "authorization": f"Bearer {self._api_key}",
        }


# ============================================================================
# providers/lmstudio.py
# ============================================================================



import os
from typing import Optional

class LMStudioProvider(OpenAICompatibleProvider):
    """Provider for LM Studio models (local deployment)."""

    DEFAULT_API_URL = "http://localhost:1234"

    def __init__(
        self,
        model: str,
        api_url: Optional[str] = None,
        timeout: float = 30.0,
        connect_timeout: Optional[float] = None,
        read_timeout: Optional[float] = None,
    ):
        """Initialize LM Studio provider.

        LM Studio is a local deployment and does not require an API key.
        """
        if api_url is None:
            api_url = os.getenv("LMSTUDIO_BASE_URL", self.DEFAULT_API_URL)

        # No API key for local deployment
        super().__init__(
            model,
            api_key=None,
            api_url=api_url,
            timeout=timeout,
            connect_timeout=connect_timeout,
            read_timeout=read_timeout,
        )

    @property
    def provider_name(self) -> str:
        """Get provider name."""
        return "lmstudio"

    def _build_headers(self) -> dict:
        """Build request headers for LM Studio API."""
        return {
            "content-type": "application/json",
        }


# ============================================================================
# providers/factory.py
# ============================================================================



import os
from typing import Any, Optional

# Type alias: factory methods return either native or FFI providers.
AnyProvider = Any

def _should_use_ffi(use_ffi: Optional[bool]) -> bool:
    """Determine whether to use the C ABI (FFI) path.

    Priority: explicit parameter > NXUSKIT_USE_FFI env var > False.
    """
    if use_ffi is not None:
        return use_ffi
    return os.environ.get("NXUSKIT_USE_FFI", "").lower() in ("1", "true", "yes")

def _make_ffi_provider(
    provider_type: str,
    model: str,
    api_key: Optional[str] = None,
    api_url: Optional[str] = None,
    timeout: float = 30.0,
    license_key: Optional[str] = None,
) -> AnyProvider:
    """Create a provider via the nxuskit C ABI shared library.

    Lazy-imports the FFI module so cffi is only required when actually used.
    """
    from nxuskit._ffi_provider import create_ffi_provider

    config: dict[str, Any] = {
        "provider_type": provider_type,
        "model": model,
        "timeout_ms": int(timeout * 1000),
    }
    if api_key is not None:
        config["api_key"] = api_key
    if api_url is not None:
        config["base_url"] = api_url
    if license_key is not None:
        config["license_key"] = license_key

    return create_ffi_provider(config)

class Provider:
    """Factory for creating LLM provider instances.

    Each static method accepts an optional ``use_ffi`` parameter:

    * ``True``  — use the nxuskit C ABI shared library (all domains routed
      through Rust engine).
    * ``False`` — use the native Python HTTP implementation (default).
    * ``None``  — fall back to ``NXUSKIT_USE_FFI`` environment variable.
    """

    @staticmethod
    def claude(
        model: str = "claude-sonnet-4-20250514",
        api_key: Optional[str] = None,
        api_url: Optional[str] = None,
        timeout: float = 30.0,
        connect_timeout: Optional[float] = None,
        read_timeout: Optional[float] = None,
        use_ffi: Optional[bool] = None,
        license_key: Optional[str] = None,
    ) -> AnyProvider:
        """Create a Claude provider instance."""
        if _should_use_ffi(use_ffi):
            return _make_ffi_provider("claude", model, api_key, api_url, timeout, license_key)
        return ClaudeProvider(
            model=model,
            api_key=api_key,
            api_url=api_url,
            timeout=timeout,
            connect_timeout=connect_timeout,
            read_timeout=read_timeout,
        )

    @staticmethod
    def openai(
        model: str = "gpt-4o",
        api_key: Optional[str] = None,
        api_url: Optional[str] = None,
        timeout: float = 30.0,
        connect_timeout: Optional[float] = None,
        read_timeout: Optional[float] = None,
        use_ffi: Optional[bool] = None,
        license_key: Optional[str] = None,
    ) -> AnyProvider:
        """Create an OpenAI provider instance."""
        if _should_use_ffi(use_ffi):
            return _make_ffi_provider("openai", model, api_key, api_url, timeout, license_key)
        return OpenAIProvider(
            model=model,
            api_key=api_key,
            api_url=api_url,
            timeout=timeout,
            connect_timeout=connect_timeout,
            read_timeout=read_timeout,
        )

    @staticmethod
    def ollama(
        model: str = "llama3.2",
        api_url: Optional[str] = None,
        timeout: float = 30.0,
        connect_timeout: Optional[float] = None,
        read_timeout: Optional[float] = None,
        use_ffi: Optional[bool] = None,
        license_key: Optional[str] = None,
    ) -> AnyProvider:
        """Create an Ollama provider instance."""
        if _should_use_ffi(use_ffi):
            return _make_ffi_provider("ollama", model, None, api_url, timeout, license_key)
        return OllamaProvider(
            model=model,
            api_url=api_url,
            timeout=timeout,
            connect_timeout=connect_timeout,
            read_timeout=read_timeout,
        )

    @staticmethod
    def groq(
        model: str = "llama-3.3-70b-versatile",
        api_key: Optional[str] = None,
        api_url: Optional[str] = None,
        timeout: float = 30.0,
        connect_timeout: Optional[float] = None,
        read_timeout: Optional[float] = None,
        use_ffi: Optional[bool] = None,
        license_key: Optional[str] = None,
    ) -> AnyProvider:
        """Create a Groq provider instance."""
        if _should_use_ffi(use_ffi):
            return _make_ffi_provider("groq", model, api_key, api_url, timeout, license_key)
        return GroqProvider(
            model=model,
            api_key=api_key,
            api_url=api_url,
            timeout=timeout,
            connect_timeout=connect_timeout,
            read_timeout=read_timeout,
        )

    @staticmethod
    def xai(
        model: str = "grok-4",
        api_key: Optional[str] = None,
        api_url: Optional[str] = None,
        timeout: float = 30.0,
        connect_timeout: Optional[float] = None,
        read_timeout: Optional[float] = None,
        use_ffi: Optional[bool] = None,
        license_key: Optional[str] = None,
    ) -> AnyProvider:
        """Create an xAI Grok provider instance."""
        if _should_use_ffi(use_ffi):
            return _make_ffi_provider("xai", model, api_key, api_url, timeout, license_key)
        return XaiProvider(
            model=model,
            api_key=api_key,
            api_url=api_url,
            timeout=timeout,
            connect_timeout=connect_timeout,
            read_timeout=read_timeout,
        )

    @staticmethod
    def mistral(
        model: str = "mistral-large-latest",
        api_key: Optional[str] = None,
        api_url: Optional[str] = None,
        timeout: float = 30.0,
        connect_timeout: Optional[float] = None,
        read_timeout: Optional[float] = None,
        use_ffi: Optional[bool] = None,
        license_key: Optional[str] = None,
    ) -> AnyProvider:
        """Create a Mistral provider instance."""
        if _should_use_ffi(use_ffi):
            return _make_ffi_provider("mistral", model, api_key, api_url, timeout, license_key)
        return MistralProvider(
            model=model,
            api_key=api_key,
            api_url=api_url,
            timeout=timeout,
            connect_timeout=connect_timeout,
            read_timeout=read_timeout,
        )

    @staticmethod
    def fireworks(
        model: str = "accounts/fireworks/models/llama-v3p1-70b-instruct",
        api_key: Optional[str] = None,
        api_url: Optional[str] = None,
        timeout: float = 30.0,
        connect_timeout: Optional[float] = None,
        read_timeout: Optional[float] = None,
        use_ffi: Optional[bool] = None,
        license_key: Optional[str] = None,
    ) -> AnyProvider:
        """Create a Fireworks provider instance."""
        if _should_use_ffi(use_ffi):
            return _make_ffi_provider("fireworks", model, api_key, api_url, timeout, license_key)
        return FireworksProvider(
            model=model,
            api_key=api_key,
            api_url=api_url,
            timeout=timeout,
            connect_timeout=connect_timeout,
            read_timeout=read_timeout,
        )

    @staticmethod
    def together(
        model: str = "meta-llama/Llama-3.3-70B-Instruct-Turbo",
        api_key: Optional[str] = None,
        api_url: Optional[str] = None,
        timeout: float = 30.0,
        connect_timeout: Optional[float] = None,
        read_timeout: Optional[float] = None,
        use_ffi: Optional[bool] = None,
        license_key: Optional[str] = None,
    ) -> AnyProvider:
        """Create a Together provider instance."""
        if _should_use_ffi(use_ffi):
            return _make_ffi_provider("together", model, api_key, api_url, timeout, license_key)
        return TogetherProvider(
            model=model,
            api_key=api_key,
            api_url=api_url,
            timeout=timeout,
            connect_timeout=connect_timeout,
            read_timeout=read_timeout,
        )

    @staticmethod
    def openrouter(
        model: str = "anthropic/claude-sonnet-4-20250514",
        api_key: Optional[str] = None,
        api_url: Optional[str] = None,
        timeout: float = 30.0,
        connect_timeout: Optional[float] = None,
        read_timeout: Optional[float] = None,
        use_ffi: Optional[bool] = None,
        license_key: Optional[str] = None,
    ) -> AnyProvider:
        """Create an OpenRouter provider instance."""
        if _should_use_ffi(use_ffi):
            return _make_ffi_provider("openrouter", model, api_key, api_url, timeout, license_key)
        return OpenRouterProvider(
            model=model,
            api_key=api_key,
            api_url=api_url,
            timeout=timeout,
            connect_timeout=connect_timeout,
            read_timeout=read_timeout,
        )

    @staticmethod
    def perplexity(
        model: str = "llama-3.1-sonar-small-128k-online",
        api_key: Optional[str] = None,
        api_url: Optional[str] = None,
        timeout: float = 30.0,
        connect_timeout: Optional[float] = None,
        read_timeout: Optional[float] = None,
        use_ffi: Optional[bool] = None,
        license_key: Optional[str] = None,
    ) -> AnyProvider:
        """Create a Perplexity provider instance."""
        if _should_use_ffi(use_ffi):
            return _make_ffi_provider("perplexity", model, api_key, api_url, timeout, license_key)
        return PerplexityProvider(
            model=model,
            api_key=api_key,
            api_url=api_url,
            timeout=timeout,
            connect_timeout=connect_timeout,
            read_timeout=read_timeout,
        )

    @staticmethod
    def lmstudio(
        model: str = "local-model",
        api_url: Optional[str] = None,
        timeout: float = 30.0,
        connect_timeout: Optional[float] = None,
        read_timeout: Optional[float] = None,
        use_ffi: Optional[bool] = None,
        license_key: Optional[str] = None,
    ) -> AnyProvider:
        """Create an LM Studio provider instance."""
        if _should_use_ffi(use_ffi):
            return _make_ffi_provider("lmstudio", model, None, api_url, timeout, license_key)
        return LMStudioProvider(
            model=model,
            api_url=api_url,
            timeout=timeout,
            connect_timeout=connect_timeout,
            read_timeout=read_timeout,
        )

# Public API exports
__all__ = [
    "__version__",
    "__author__",
    "__license__",
    # Types
    "Role",
    "ImageSourceType",
    "ImageSource",
    "TokenUsage",
    "ChatRequest",
    "ChatResponse",
    "StreamChunk",
    "ModelInfo",
    "ResponseFormat",
    "CapabilityStatus",
    "ManifestPublicationPosture",
    "PUBLIC_CAPABILITY_FIELDS",
    "PublicProviderCapability",
    "PublicCapabilityManifest",
    "LogprobsData",
    "TokenLogprob",
    "TopLogprob",
    # Message
    "Message",
    # Errors
    "LLMError",
    "AuthenticationError",
    "RateLimitError",
    "NetworkError",
    "ProviderError",
    "TimeoutError",
    # FFI / entitlement errors
    "NxuskitError",
    "FeatureUnavailableError",
    "LicenseRequiredError",
    "LicenseExpiredError",
    "EditionInsufficientError",
    # Provider protocol
    "LLMProvider",
    # Provider factory
    "Provider",
    # Streaming utilities
    "collect_stream",
    "stream_with_callback",
    "stream_to_file",
    "StreamBuffer",
    # Vision utilities
    "load_image_base64",
    "detect_image_type",
    "is_valid_url",
    "is_base64",
    "add_images_to_message",
    "image_to_data_url",
    "ImageLoader",
    # Retry utilities
    "RetryConfig",
    "should_retry",
    "retry_with_backoff",
    "retry_on_rate_limit",
    "RetryIterator",
    "AdaptiveRateLimiter",
    # Solver types
    "SolverStreamChunk",
    "VariableType",
    "VariableDef",
    "DomainDef",
    "ConstraintType",
    "ConstraintDef",
    "ObjectiveDirection",
    "ObjectiveDef",
    "MultiObjectiveMode",
    "SolverConfig",
    "SolveStatus",
    "SolverValue",
    "SolverStats",
    "SolverExplanation",
    "SolveResult",
    "SolverCapabilities",
    "SessionStatus",
    # Tool calling
    "ToolDefinition",
    "FunctionDefinition",
    "ToolCall",
    "FunctionCall",
    "ToolResultMessage",
    "tool_choice_auto",
    "tool_choice_none",
    "tool_choice_required",
    "tool_choice_named",
    # CLIPS Session (FFI-dependent, lazy-loaded)
    "ClipsSession",
    "ClipsError",
    # License management (FFI-dependent, lazy-loaded)
    "ActivationResult",
    "LicenseResolution",
    "TokenInfo",
    "TrialResult",
    "license_activate",
    "license_deactivate",
    "license_machine_id",
    "license_resolve",
    "license_trial_activate",
    "license_trial_issue",
    "license_validate",
    # OAuth authentication (FFI-dependent, lazy-loaded)
    "OAuthResult",
    "OAuthStatus",
    "oauth_start",
    "oauth_status",
    "oauth_revoke",
    # ZEN evaluation (FFI-dependent, lazy-loaded)
    "zen_evaluate",
    "zen_evaluate_async",
    # Security validation
    "SecurityValidator",
    "SecurityValidationResult",
    "SecurityIssue",
    "SecuritySeverity",
]
