"""LLMProvider protocol definition."""

from typing import Any, Iterator, List, Optional, Protocol, Union

from nxuskit.message import Message
from nxuskit.tools import ToolDefinition
from nxuskit.types import AdapterCapabilities, ChatResponse, ModelInfo, ResponseFormat, StreamChunk


def _decode_adapter_capabilities(catalog, name):
    import math
    from dataclasses import fields

    from nxuskit.errors import ProviderError

    def unavailable():
        error = ProviderError("adapter capability snapshot unavailable", provider=name)
        error.code = "unavailable"
        raise error

    if not isinstance(catalog, dict) or not isinstance(catalog.get(name), dict):
        unavailable()
    row = dict(catalog[name])
    if set(row) != {field.name for field in fields(AdapterCapabilities)}:
        unavailable()
    for key, value in row.items():
        if key.startswith("supports_") and type(value) is not bool:
            unavailable()
        if (
            key in ("max_stop_sequences", "max_logprobs")
            and value is not None
            and (type(value) is not int or value < 0)
        ):
            unavailable()
    value = row["penalty_range"]
    if value is not None:

        def finite_bound(bound):
            try:
                return type(bound) in (int, float) and math.isfinite(bound)
            except OverflowError:
                return False

        if (
            not isinstance(value, (tuple, list))
            or len(value) != 2
            or not all(finite_bound(v) for v in value)
            or value[0] > value[1]
        ):
            unavailable()
        row["penalty_range"] = tuple(value)
    return AdapterCapabilities(**row)


def _native_adapter_capabilities(name):
    """Read generated package data only; never load an FFI library."""
    import json
    from importlib import resources

    try:
        document = json.loads(
            resources.files("nxuskit")
            .joinpath("adapter_capabilities.json")
            .read_text(encoding="utf-8")
        )
        catalog = (
            document.get("adapter_capabilities")
            if document.get("schema_version") == 1
            and document.get("declaration_kind") == "static_adapter_support"
            else None
        )
    except (OSError, ValueError, TypeError, AttributeError):
        catalog = None
    return _decode_adapter_capabilities(catalog, name)


def _library_adapter_capabilities(name):
    """Use the loaded binary's declaration, never substitute the package snapshot."""
    import json

    from nxuskit._ffi import ffi, lib

    catalog = None
    try:
        raw = lib.nxuskit_capabilities()
        if raw != ffi.NULL:
            try:
                catalog = json.loads(ffi.string(raw)).get("adapter_capabilities")
            finally:
                lib.nxuskit_free_string(raw)
    except (AttributeError, ValueError, TypeError):
        pass
    return _decode_adapter_capabilities(catalog, name)


class AsyncStream(Protocol):
    """Owned async iterator. Explicitly aclose after early break."""

    def __aiter__(self) -> "AsyncStream": ...
    async def __anext__(self) -> StreamChunk: ...
    async def aclose(self) -> None: ...


class AsyncLLMProvider(Protocol):
    """Common bounded chat/stream lifecycle and static adapter declarations.

    Capabilities are not effective model/endpoint support or discovery.

    Close while the owning loop is usable. FFI additionally supports recovery
    after forced loop closure; native async clients require explicit close.
    """

    @property
    def model(self) -> str: ...
    @property
    def provider_name(self) -> str: ...
    @property
    def closed(self) -> bool: ...
    @property
    def outstanding(self) -> int: ...
    def capabilities(self) -> AdapterCapabilities: ...
    async def capabilities_async(self) -> AdapterCapabilities: ...
    async def chat_async(
        self,
        messages: List[Message],
        *,
        model: Optional[str] = None,
        temperature: Optional[float] = None,
        max_tokens: Optional[int] = None,
        top_p: Optional[float] = None,
        stop: Optional[Union[str, List[str]]] = None,
        response_format: Optional[Union[ResponseFormat, dict[str, Any]]] = None,
        tools: Optional[List[ToolDefinition]] = None,
        tool_choice: Any = None,
        thinking: Optional[str] = None,
        presence_penalty: Optional[float] = None,
        frequency_penalty: Optional[float] = None,
        seed: Optional[int] = None,
        top_k: Optional[int] = None,
        min_p: Optional[float] = None,
        logprobs: Optional[bool] = None,
        top_logprobs: Optional[int] = None,
        reasoning_effort: Optional[str] = None,
    ) -> ChatResponse: ...
    def chat_stream_async(
        self,
        messages: List[Message],
        *,
        model: Optional[str] = None,
        temperature: Optional[float] = None,
        max_tokens: Optional[int] = None,
        top_p: Optional[float] = None,
        stop: Optional[Union[str, List[str]]] = None,
        response_format: Optional[Union[ResponseFormat, dict[str, Any]]] = None,
        tools: Optional[List[ToolDefinition]] = None,
        tool_choice: Any = None,
        thinking: Optional[str] = None,
        presence_penalty: Optional[float] = None,
        frequency_penalty: Optional[float] = None,
        seed: Optional[int] = None,
        top_k: Optional[int] = None,
        min_p: Optional[float] = None,
        logprobs: Optional[bool] = None,
        top_logprobs: Optional[int] = None,
        reasoning_effort: Optional[str] = None,
    ) -> AsyncStream: ...
    async def close_async(self) -> None: ...
    def close(self) -> None: ...
    async def __aenter__(self) -> "AsyncLLMProvider": ...
    async def __aexit__(self, *args: Any) -> None: ...


class LLMProvider(Protocol):
    """Protocol for LLM providers.

    All providers (native HTTP and FFI) satisfy this structural typing
    contract. Use this as a type annotation when writing code that accepts
    any provider.
    """

    def capabilities(self) -> AdapterCapabilities: ...
    async def capabilities_async(self) -> AdapterCapabilities: ...

    @property
    def model(self) -> str:
        """Get the default model identifier."""
        ...

    @property
    def provider_name(self) -> str:
        """Get the provider name."""
        ...

    def chat(
        self,
        messages: List[Message],
        *,
        model: Optional[str] = None,
        temperature: Optional[float] = None,
        max_tokens: Optional[int] = None,
        top_p: Optional[float] = None,
        stop: Optional[Union[str, List[str]]] = None,
        response_format: Optional[Union[ResponseFormat, dict[str, Any]]] = None,
        tools: Optional[List[ToolDefinition]] = None,
        tool_choice: Optional[Any] = None,
        thinking: Optional[str] = None,
        presence_penalty: Optional[float] = None,
        frequency_penalty: Optional[float] = None,
        seed: Optional[int] = None,
        top_k: Optional[int] = None,
        min_p: Optional[float] = None,
        logprobs: Optional[bool] = None,
        top_logprobs: Optional[int] = None,
        reasoning_effort: Optional[str] = None,
    ) -> ChatResponse:
        """Send a chat request and get a response.

        Args:
            messages: List of messages in the conversation.
            model: Model override for this request.
            temperature: Sampling temperature (0.0-2.0).
            max_tokens: Maximum tokens to generate.
            top_p: Nucleus sampling parameter.
            stop: Stop sequences.
            response_format: Response format (JSON or TEXT).
            tools: Tool definitions for function calling.
            tool_choice: Tool selection strategy.
            thinking: Provider-specific thinking control.
            presence_penalty: Presence penalty when supported.
            frequency_penalty: Frequency penalty when supported.
            seed: Signed 64-bit seed when supported.
            top_k: Provider-specific top-k control; unsupported providers reject explicitly.
            min_p: Provider-specific minimum-p control; unsupported providers reject explicitly.
            logprobs: Request log probabilities when supported.
            top_logprobs: Number of top log probabilities when supported.
            reasoning_effort: Provider reasoning-effort control when supported.

        Returns:
            ChatResponse with the model's reply.
        """
        ...

    def chat_stream(
        self,
        messages: List[Message],
        *,
        model: Optional[str] = None,
        temperature: Optional[float] = None,
        max_tokens: Optional[int] = None,
        top_p: Optional[float] = None,
        stop: Optional[Union[str, List[str]]] = None,
        response_format: Optional[Union[ResponseFormat, dict[str, Any]]] = None,
        tools: Optional[List[ToolDefinition]] = None,
        tool_choice: Optional[Any] = None,
        thinking: Optional[str] = None,
        presence_penalty: Optional[float] = None,
        frequency_penalty: Optional[float] = None,
        seed: Optional[int] = None,
        top_k: Optional[int] = None,
        min_p: Optional[float] = None,
        logprobs: Optional[bool] = None,
        top_logprobs: Optional[int] = None,
        reasoning_effort: Optional[str] = None,
    ) -> Iterator[StreamChunk]:
        """Send a chat request and stream the response.

        Args:
            messages: List of messages in the conversation.
            model: Model override for this request.
            temperature: Sampling temperature (0.0-2.0).
            max_tokens: Maximum tokens to generate.
            top_p: Nucleus sampling parameter.
            stop: Stop sequences.
            response_format: Response format (JSON or TEXT).
            tools: Tool definitions for function calling.
            tool_choice: Tool selection strategy.
            thinking: Provider-specific thinking control.
            presence_penalty: Presence penalty when supported.
            frequency_penalty: Frequency penalty when supported.
            seed: Signed 64-bit seed when supported.
            top_k: Provider-specific top-k control; unsupported providers reject explicitly.
            min_p: Provider-specific minimum-p control; unsupported providers reject explicitly.
            logprobs: Request log probabilities when supported.
            top_logprobs: Number of top log probabilities when supported.
            reasoning_effort: Provider reasoning-effort control when supported.

        Yields:
            StreamChunk objects as they arrive.
        """
        ...

    def list_models(self) -> List[ModelInfo]:
        """List available models from this provider.

        Returns:
            List of ModelInfo objects. May be empty if the provider
            does not support model listing.
        """
        ...
