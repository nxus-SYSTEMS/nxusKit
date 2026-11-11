"""OpenAI-compatible provider base class.

This module provides a base class for providers that use OpenAI-compatible APIs,
enabling code reuse across multiple providers (Groq, Mistral, Fireworks, etc.).
"""

import base64
import json
from abc import abstractmethod
from typing import Any, Iterator, List, Optional, Union

from nxuskit.message import Message
from nxuskit.errors import ProviderError
from nxuskit.providers.base import BaseProvider
from nxuskit.providers._resource_bounds import SseEventFramer, sse_data_payload
from nxuskit.tools import ToolCall, ToolDefinition
from nxuskit.types import (
    ChatResponse,
    ImageSourceType,
    ModelInfo,
    ResponseFormat,
    StreamChunk,
    TokenUsage,
    _validate_logprob_reasoning_controls,
    _validate_request_controls,
)
from nxuskit.vision import detect_image_type


class OpenAICompatibleProvider(BaseProvider):
    """Base class for providers using OpenAI-compatible API format."""

    def _merge_provider_metadata(self, current, data):
        if self.provider_name == "openrouter":
            fields = ("id", "provider")
            usage_fields = (
                "cost",
                "cost_details",
                "prompt_tokens_details",
                "completion_tokens_details",
            )
        elif self.provider_name == "perplexity":
            fields = ("citations", "search_results")
            usage_fields = (
                "cost",
                "search_context_size",
                "citation_tokens",
                "num_search_queries",
                "reasoning_tokens",
            )
        else:
            return None
        selected = dict(current or {})
        for name in fields:
            if name in data:
                selected[name] = data[name]
        if "usage" in data:
            usage = data["usage"]
            if usage is None:
                selected["usage"] = None
            elif isinstance(usage, dict):
                updates = {name: usage[name] for name in usage_fields if name in usage}
                if updates:
                    selected["usage"] = {**(selected.get("usage") or {}), **updates}
            else:
                raise ProviderError(
                    "provider_metadata usage must be object or null", provider=self.provider_name
                )

        def check_depth(value, depth=0):
            if depth > 16:
                raise ValueError("depth exceeds 16")
            if isinstance(value, dict):
                for child in value.values():
                    check_depth(child, depth + 1)
            elif isinstance(value, list):
                for child in value:
                    check_depth(child, depth + 1)

        try:
            check_depth(selected)
            encoded = json.dumps(
                selected, ensure_ascii=False, separators=(",", ":"), allow_nan=False
            ).encode("utf-8")
            if len(encoded) > 65536:
                raise ValueError("compact UTF-8 bytes exceed 65536")
        except (ValueError, TypeError) as error:
            raise ProviderError(
                f"provider_metadata invalid or limit exceeded: {error}", provider=self.provider_name
            ) from error
        return selected or None

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
        headers = self._build_headers()

        response = self._make_request(
            method="POST",
            url=self._api_endpoint("/chat/completions"),
            headers=headers,
            json_data=request_body,
            stream=True,
        )

        data = self._read_response_json(response)
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
        headers = self._build_headers()

        response = self._make_request(
            method="POST",
            url=self._api_endpoint("/chat/completions"),
            headers=headers,
            json_data=request_body,
            stream=True,
        )

        raw_reason = None
        usage = None
        actual_model = effective_model
        provider_metadata = None
        framer = SseEventFramer(provider=self.provider_name)
        try:
            for transport_chunk in self._iter_stream_response_bytes(response, sse=True):
                for event_bytes in framer.feed(transport_chunk):
                    data_str = sse_data_payload(event_bytes, provider=self.provider_name)
                    if data_str is None:
                        continue
                    if data_str == "[DONE]":
                        yield StreamChunk(
                            delta="",
                            model=actual_model,
                            terminal=True,
                            finish_reason=self._canonical_reason(raw_reason),
                            raw_finish_reason=raw_reason,
                            usage=usage,
                            provider_metadata=provider_metadata,
                        )
                        return

                    try:
                        data = json.loads(data_str)
                        if not isinstance(data, dict) or "error" in data:
                            raise ProviderError(
                                "invalid or failed stream envelope", provider=self.provider_name
                            )
                        provider_metadata = self._merge_provider_metadata(provider_metadata, data)
                        chunk = self._parse_stream_event(data, actual_model)
                        if chunk:
                            actual_model = chunk.model
                            if chunk.raw_finish_reason is not None:
                                raw_reason = chunk.raw_finish_reason
                            if chunk.usage is not None:
                                usage = chunk.usage
                            chunk.finish_reason = None
                            chunk.raw_finish_reason = None
                            chunk.terminal = False
                            if not (
                                chunk.delta
                                or chunk.thinking
                                or chunk.usage is not None
                                or chunk.tool_calls
                                or chunk.logprobs
                            ):
                                continue
                            yield chunk
                    except json.JSONDecodeError as error:
                        raise ProviderError(
                            "invalid stream JSON", provider=self.provider_name
                        ) from error
            raise ProviderError("stream ended before [DONE]", provider=self.provider_name)
        finally:
            response.close()

    def list_models(self) -> List[ModelInfo]:
        """List available models from this provider."""
        try:
            headers = self._build_headers()
            response = self._make_request(
                method="GET",
                url=self._api_endpoint("/models"),
                headers=headers,
                json_data={},
                stream=True,
            )
            data = self._read_response_json(response)
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
    ) -> dict:
        """Build request body for OpenAI-compatible API."""
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
        # These controls are qualified on OpenAI here, not on every adapter
        # merely because it uses a compatible request shape.
        _validate_logprob_reasoning_controls(
            self.provider_name,
            effective_model,
            temperature,
            top_p,
            logprobs,
            top_logprobs,
            reasoning_effort,
        )
        if thinking is not None:
            error = ProviderError(f"unsupported request control: thinking", provider=self.provider_name)
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
            self.provider_name in ("fireworks", "lmstudio", "groq", "mistral", "together", "openrouter")
            and stream
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
        elif isinstance(response_format, dict):
            if response_format.get("type") != "json_schema" or not isinstance(
                response_format.get("schema"), dict
            ):
                error = ProviderError(
                    "invalid request control: response_format", provider=self.provider_name
                )
                error.code = "invalid_request"
                raise error
            request_body["response_format"] = {
                "type": "json_schema",
                "json_schema": {
                    "name": "response",
                    "strict": True,
                    "schema": response_format["schema"],
                },
            }
        elif response_format not in (None, ResponseFormat.TEXT):
            error = ProviderError(
                "invalid request control: response_format", provider=self.provider_name
            )
            error.code = "invalid_request"
            raise error

        for name, value in (
            ("presence_penalty", presence_penalty),
            ("frequency_penalty", frequency_penalty),
            ("seed", seed),
            ("logprobs", logprobs),
            ("top_logprobs", top_logprobs),
            ("reasoning_effort", reasoning_effort),
        ):
            if value is not None:
                request_body[name] = value

        if tools:
            request_body["tools"] = [t.to_dict() for t in tools]
        if tool_choice is not None:
            request_body["tool_choice"] = tool_choice

        return request_body

    def _canonical_reason(self, raw):
        if raw is not None and not isinstance(raw, str):
            raise ProviderError("invalid finish_reason type", provider=self.provider_name)
        return raw if raw in ("stop", "length", "tool_calls", "content_filter", "error") else None

    def _response_model(self, data, effective_model):
        model = data.get("model")
        if model is not None and not isinstance(model, str):
            raise ProviderError("invalid model type", provider=self.provider_name)
        return model or effective_model

    def _parse_response(self, data: dict, effective_model: str) -> ChatResponse:
        """Parse OpenAI-compatible API response."""
        if not isinstance(data, dict) or "error" in data:
            raise ProviderError("invalid or failed response envelope", provider=self.provider_name)
        provider_metadata = self._merge_provider_metadata(None, data)
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

        usage = data.get("usage") or {}
        token_usage = TokenUsage(
            prompt_tokens=usage.get("prompt_tokens", 0),
            completion_tokens=usage.get("completion_tokens", 0),
            total_tokens=usage.get("total_tokens", 0),
        )

        return ChatResponse(
            content=content,
            usage=token_usage,
            model=self._response_model(data, effective_model),
            provider=self.provider_name,
            finish_reason=self._canonical_reason(finish_reason),
            raw_finish_reason=finish_reason,
            tool_calls=tool_calls,
            provider_metadata=provider_metadata,
        )

    def _parse_stream_event(self, data: dict, effective_model: str) -> Optional[StreamChunk]:
        """Parse a single stream event from OpenAI-compatible API."""
        if not isinstance(data, dict) or "error" in data:
            raise ProviderError("invalid or failed stream envelope", provider=self.provider_name)
        actual_model = self._response_model(data, effective_model)
        if not data.get("choices") and not data.get("usage"):
            return None

        choice = data["choices"][0] if data.get("choices") else {}
        delta = choice.get("delta", {})
        finish_reason = choice.get("finish_reason")
        canonical_reason = self._canonical_reason(finish_reason)

        content = delta.get("content", "")
        if content or finish_reason is not None or data.get("usage") or delta.get("tool_calls"):
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
                model=actual_model,
                finish_reason=canonical_reason,
                raw_finish_reason=finish_reason,
                terminal=finish_reason is not None,
                usage=usage,
                tool_calls=delta.get("tool_calls"),
            )

        return None
