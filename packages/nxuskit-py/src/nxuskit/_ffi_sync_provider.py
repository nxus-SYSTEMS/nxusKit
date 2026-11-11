"""Factory adapter for the existing synchronous ``LLMProvider`` protocol.

The low-level ``FFIProvider`` dictionary API remains separate and unchanged.
Image requests require an SDK with the nested ContentPart.image.source fix.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any, Iterator

from nxuskit.errors import ProviderError
from nxuskit.message import Message
from nxuskit.tools import ToolDefinition
from nxuskit.types import (
    ChatResponse,
    ImageSourceType,
    ModelInfo,
    ResponseFormat,
    StreamChunk,
    _validate_logprob_reasoning_controls,
    _validate_request_controls,
)
from nxuskit.vision import detect_image_type, load_image_base64

if TYPE_CHECKING:
    from nxuskit._ffi_provider import FFIProvider


class SyncFFIProvider:
    """Own an FFI handle while exposing native-style synchronous calls."""

    def __init__(
        self,
        provider: FFIProvider,
        model: str,
        provider_name: str,
        *,
        declared_profile: str | None = None,
    ):
        self._provider = provider
        self._model = model
        self._provider_name = provider_name
        self._declared_profile = declared_profile

    @property
    def model(self) -> str:
        return self._model

    @property
    def provider_name(self) -> str:
        return self._provider_name

    @property
    def declared_profile(self) -> str | None:
        return self._declared_profile

    def capabilities(self):
        """Static loaded-library declaration, not model/endpoint-effective support."""
        from nxuskit.provider import _library_adapter_capabilities

        if not hasattr(self, "_adapter_snapshot"):
            self._adapter_snapshot = _library_adapter_capabilities(self.provider_name)
        return self._adapter_snapshot

    async def capabilities_async(self):
        return self.capabilities()

    def _request(
        self,
        messages: list[Message],
        model: str | None,
        temperature: float | None,
        max_tokens: int | None,
        top_p: float | None,
        stop: str | list[str] | None,
        response_format: ResponseFormat | dict[str, Any] | None,
        tools: list[ToolDefinition] | None,
        tool_choice: Any,
        *,
        thinking: str | None = None,
        presence_penalty: float | None = None,
        frequency_penalty: float | None = None,
        seed: int | None = None,
        top_k: int | None = None,
        min_p: float | None = None,
        logprobs: bool | None = None,
        top_logprobs: int | None = None,
        reasoning_effort: str | None = None,
        streaming: bool = False,
    ) -> dict[str, Any]:
        _validate_request_controls(
            self.provider_name,
            temperature=temperature,
            max_tokens=max_tokens,
            top_p=top_p,
            stop=stop,
            presence_penalty=presence_penalty,
            frequency_penalty=frequency_penalty,
            seed=seed,
            top_k=top_k,
            min_p=min_p,
            logprobs=logprobs,
            top_logprobs=top_logprobs,
            reasoning_effort=reasoning_effort,
            thinking=thinking,
        )
        _validate_logprob_reasoning_controls(
            self.provider_name,
            model or self.model,
            temperature,
            top_p,
            logprobs,
            top_logprobs,
            reasoning_effort,
        )
        if isinstance(response_format, dict) and (
            response_format.get("type") != "json_schema"
            or not isinstance(response_format.get("schema"), dict)
        ):
            error = ProviderError(
                "invalid request control: response_format", provider=self.provider_name
            )
            error.code = "invalid_request"
            raise error
        if response_format is not None and not isinstance(response_format, (dict, ResponseFormat)):
            error = ProviderError(
                "invalid request control: response_format", provider=self.provider_name
            )
            error.code = "invalid_request"
            raise error
        if (
            self.provider_name == "ollama"
            and self.declared_profile == "ollama_cloud_via_daemon"
            and response_format not in (None, ResponseFormat.TEXT)
        ):
            error = ProviderError(
                "Ollama Cloud structured output is unsupported", provider="ollama"
            )
            error.code = "unsupported_capability"
            raise error
        if thinking is not None and self.provider_name != "ollama":
            error = ProviderError(
                "unsupported request control: thinking", provider=self.provider_name
            )
            error.code = "unsupported_capability"
            raise error

        unsupported = (
            (
                ("presence_penalty", presence_penalty),
                ("frequency_penalty", frequency_penalty),
                ("seed", seed),
            )
            if self.provider_name == "claude"
            else (
                ("presence_penalty", presence_penalty),
                ("frequency_penalty", frequency_penalty),
            )
            if self.provider_name == "ollama"
            else ()
        )
        for name, value in unsupported:
            if value is not None:
                error = ProviderError(
                    f"unsupported request control: {name}", provider=self.provider_name
                )
                error.code = "unsupported_capability"
                raise error

        if self.provider_name == "claude" and (
            isinstance(response_format, dict)
            or (streaming and (bool(tools) or tool_choice is not None))
        ):
            error = ProviderError(
                "unsupported Anthropic structured output or tool streaming", provider="claude"
            )
            error.code = "unsupported_capability"
            raise error

        if self.provider_name == "together" and tool_choice is not None:
            if isinstance(tool_choice, dict):
                function = tool_choice.get("function")
                if (
                    tool_choice.get("type") != "function"
                    or not isinstance(function, dict)
                    or not isinstance(function.get("name"), str)
                    or not function["name"]
                ):
                    error = ProviderError(
                        "invalid request control: tool_choice", provider=self.provider_name
                    )
                    error.code = "invalid_request"
                    raise error
                error = ProviderError(
                    "unsupported Together named tool choice", provider=self.provider_name
                )
                error.code = "unsupported_capability"
                raise error
            if tool_choice not in ("auto", "none", "required"):
                error = ProviderError(
                    "invalid request control: tool_choice", provider=self.provider_name
                )
                error.code = "invalid_request"
                raise error

        if (
            self.provider_name
            in ("fireworks", "lmstudio", "groq", "mistral", "together", "openrouter")
            and streaming
            and (bool(tools) or tool_choice is not None)
        ):
            display_name = {
                "fireworks": "Fireworks",
                "lmstudio": "LM Studio",
                "groq": "Groq",
                "mistral": "Mistral",
                "together": "Together",
                "openrouter": "OpenRouter",
            }[self.provider_name]
            error = ProviderError(
                f"unsupported {display_name} tool streaming",
                provider=self.provider_name,
            )
            error.code = "unsupported_capability"
            raise error

        if self.provider_name == "groq" and any(message.images for message in messages):
            error = ProviderError(
                "unsupported request control: images", provider=self.provider_name
            )
            error.code = "unsupported_capability"
            raise error

        if self.provider_name == "perplexity":
            if any(message.images for message in messages):
                error = ProviderError(
                    "unsupported request control: images", provider=self.provider_name
                )
                error.code = "unsupported_capability"
                raise error
            if response_format not in (None, ResponseFormat.TEXT):
                if isinstance(response_format, dict):
                    if response_format.get("type") != "json_schema" or not isinstance(
                        response_format.get("schema"), dict
                    ):
                        error = ProviderError(
                            "invalid request control: response_format",
                            provider=self.provider_name,
                        )
                        error.code = "invalid_request"
                        raise error
                elif response_format != ResponseFormat.JSON:
                    error = ProviderError(
                        "invalid request control: response_format",
                        provider=self.provider_name,
                    )
                    error.code = "invalid_request"
                    raise error
                error = ProviderError(
                    "unsupported request control: response_format", provider=self.provider_name
                )
                error.code = "unsupported_capability"
                raise error
            if tools is not None:
                for tool in tools:
                    tool.validate()
                if tools:
                    error = ProviderError(
                        "unsupported request control: tools", provider=self.provider_name
                    )
                    error.code = "unsupported_capability"
                    raise error
            if tool_choice is not None:
                error = ProviderError(
                    "unsupported request control: tool_choice", provider=self.provider_name
                )
                error.code = "unsupported_capability"
                raise error

        serialized = []
        for message in messages:
            content: str | list[dict[str, Any]] = message.content
            if message.images:
                content = [{"type": "text", "text": message.content}]
                for image in message.images:
                    if image.source_type == ImageSourceType.URL:
                        source = {"type": "url", "url": image.data}
                    elif image.source_type in (ImageSourceType.BASE64, ImageSourceType.FILEPATH):
                        data = (
                            load_image_base64(image.data)
                            if image.source_type == ImageSourceType.FILEPATH
                            else image.data
                        )
                        source = {
                            "type": "base64",
                            "data": data,
                            "media_type": image.media_type or detect_image_type(image.data),
                        }
                    else:
                        raise ValueError(f"Unsupported image source type: {image.source_type}")
                    content.append({"type": "image", "source": source})
            serialized.append({"role": message.role.value, "content": content})

        request: dict[str, Any] = {"model": model or self.model, "messages": serialized}
        for name, value in (
            ("temperature", temperature),
            ("max_tokens", max_tokens),
            ("top_p", top_p),
            ("tool_choice", tool_choice),
            ("thinking_mode", thinking),
            ("presence_penalty", presence_penalty),
            ("frequency_penalty", frequency_penalty),
            ("seed", seed),
            ("top_k", top_k),
            ("min_p", min_p),
            ("logprobs", logprobs),
            ("top_logprobs", top_logprobs),
            ("reasoning_effort", reasoning_effort),
        ):
            if value is not None:
                request[name] = value
        if stop is not None:
            request["stop"] = [stop] if isinstance(stop, str) else list(stop)
        if response_format is not None and not (
            self.provider_name == "perplexity" and response_format == ResponseFormat.TEXT
        ):
            request["response_format"] = (
                dict(response_format)
                if isinstance(response_format, dict)
                else {"type": response_format.value}
            )
        if tools:
            request["tools"] = [tool.to_dict() for tool in tools]
        return request

    def chat(
        self,
        messages: list[Message],
        *,
        model: str | None = None,
        temperature: float | None = None,
        max_tokens: int | None = None,
        top_p: float | None = None,
        stop: str | list[str] | None = None,
        response_format: ResponseFormat | dict[str, Any] | None = None,
        tools: list[ToolDefinition] | None = None,
        tool_choice: Any = None,
        thinking: str | None = None,
        presence_penalty: float | None = None,
        frequency_penalty: float | None = None,
        seed: int | None = None,
        top_k: int | None = None,
        min_p: float | None = None,
        logprobs: bool | None = None,
        top_logprobs: int | None = None,
        reasoning_effort: str | None = None,
    ) -> ChatResponse:
        return self._provider.chat(
            self._request(
                messages,
                model,
                temperature,
                max_tokens,
                top_p,
                stop,
                response_format,
                tools,
                tool_choice,
                thinking=thinking,
                presence_penalty=presence_penalty,
                frequency_penalty=frequency_penalty,
                seed=seed,
                top_k=top_k,
                min_p=min_p,
                logprobs=logprobs,
                top_logprobs=top_logprobs,
                reasoning_effort=reasoning_effort,
            )
        )

    def chat_stream(
        self,
        messages: list[Message],
        *,
        model: str | None = None,
        temperature: float | None = None,
        max_tokens: int | None = None,
        top_p: float | None = None,
        stop: str | list[str] | None = None,
        response_format: ResponseFormat | dict[str, Any] | None = None,
        tools: list[ToolDefinition] | None = None,
        tool_choice: Any = None,
        thinking: str | None = None,
        presence_penalty: float | None = None,
        frequency_penalty: float | None = None,
        seed: int | None = None,
        top_k: int | None = None,
        min_p: float | None = None,
        logprobs: bool | None = None,
        top_logprobs: int | None = None,
        reasoning_effort: str | None = None,
    ) -> Iterator[StreamChunk]:
        # Return the owning generator, rather than wrapping and losing close().
        return self._provider.stream(
            self._request(
                messages,
                model,
                temperature,
                max_tokens,
                top_p,
                stop,
                response_format,
                tools,
                tool_choice,
                thinking=thinking,
                presence_penalty=presence_penalty,
                frequency_penalty=frequency_penalty,
                seed=seed,
                top_k=top_k,
                min_p=min_p,
                logprobs=logprobs,
                top_logprobs=top_logprobs,
                reasoning_effort=reasoning_effort,
                streaming=True,
            )
        )

    def list_models(self) -> list[ModelInfo]:
        return self._provider.list_models()

    def close(self) -> None:
        self._provider.close()

    def __enter__(self) -> SyncFFIProvider:
        return self

    def __exit__(self, *args: Any) -> None:
        self.close()
