"""Ollama provider implementation."""

import base64
import json
from typing import Any, Iterator, List, Optional, Union
from urllib.parse import urlsplit

from nxuskit.errors import ProviderError
from nxuskit.message import Message
from nxuskit.providers._resource_bounds import NdjsonRecordFramer
from nxuskit.providers.base import BaseProvider
from nxuskit.tools import ToolDefinition
from nxuskit.types import (
    ChatResponse,
    ImageSourceType,
    LogprobsData,
    ModelInfo,
    ResponseFormat,
    StreamChunk,
    StreamLogprobsDelta,
    TokenUsage,
    _validate_request_controls,
)


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
        *,
        profile: Optional[str] = None,
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
        self._validate_profile(profile)
        self._profile = profile

    @staticmethod
    def _validate_profile(profile):
        if profile not in (None, "ollama_local", "ollama_cloud_via_daemon"):
            error = ProviderError("Invalid declared Ollama profile", provider="ollama")
            error.code = "invalid_request"
            raise error

    def _endpoint_origin(self):
        # Provenance only: strip userinfo, path, query and fragment. It never
        # establishes where the model executed or whether a capability exists.
        try:
            url = urlsplit(self._api_url)
            if url.scheme not in ("http", "https") or not url.hostname:
                return None
            host = url.hostname.lower()
            if ":" in host:
                host = f"[{host}]"
            port = url.port
            suffix = f":{port}" if port and port != {"http": 80, "https": 443}[url.scheme] else ""
            return f"{url.scheme}://{host}{suffix}"
        except ValueError:
            return None

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
            thinking=thinking,
            tool_choice=tool_choice,
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
            url=f"{self._api_url}/api/chat",
            headers=headers,
            json_data=request_body,
            stream=True,
        )

        try:
            data = json.loads(self._read_response_bytes(response).decode("utf-8"))
        except (UnicodeDecodeError, json.JSONDecodeError):
            self._request_error("malformed_response", "Malformed Ollama response")
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
            thinking=thinking,
            tool_choice=tool_choice,
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
            url=f"{self._api_url}/api/chat",
            headers=headers,
            json_data=request_body,
            stream=True,
        )

        framer = NdjsonRecordFramer(provider="ollama")

        def records():
            for transport_chunk in self._iter_stream_response_bytes(response, sse=False):
                yield from framer.feed(transport_chunk)
            yield from framer.finish()

        try:
            for record in records():
                try:
                    data = json.loads(record.decode("utf-8"))
                except (UnicodeDecodeError, json.JSONDecodeError):
                    self._request_error("malformed_response", "Malformed Ollama stream record")
                chunk = self._parse_stream_event(data, effective_model)
                if chunk:
                    yield chunk
                    if chunk.is_final():
                        return
            self._request_error("malformed_response", "Ollama stream ended before done")
        finally:
            response.close()

    def list_models(self) -> List[ModelInfo]:
        """List available models from Ollama."""
        try:
            headers = self._build_headers()
            response = self._make_request(
                method="GET",
                url=f"{self._api_url}/api/tags",
                headers=headers,
                json_data={},
                stream=True,
            )
            data = self._read_response_json(response)
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
        response_format: Optional[Union[ResponseFormat, dict[str, Any]]] = None,
        tools: Optional[List[ToolDefinition]] = None,
        thinking: Optional[str] = None,
        tool_choice: Optional[Any] = None,
        presence_penalty: Optional[float] = None,
        frequency_penalty: Optional[float] = None,
        seed: Optional[int] = None,
        top_k: Optional[int] = None,
        min_p: Optional[float] = None,
        logprobs: Optional[bool] = None,
        top_logprobs: Optional[int] = None,
        reasoning_effort: Optional[str] = None,
    ) -> dict:
        """Build request body for Ollama API."""
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
        for name, value in (
            ("presence_penalty", presence_penalty),
            ("frequency_penalty", frequency_penalty),
            ("reasoning_effort", reasoning_effort),
        ):
            if value is not None:
                self._request_error(
                    "unsupported_capability", f"unsupported request control: {name}"
                )
        if tool_choice is not None:
            self._request_error("unsupported_capability", "tool_choice is not supported by Ollama")
        if thinking not in (None, "omit", "auto", "enabled", "disabled"):
            self._request_error("invalid_request", "Invalid thinking mode")
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

        options = {}
        if temperature is not None:
            options["temperature"] = temperature
        if max_tokens is not None:
            options["num_predict"] = max_tokens
        if top_p is not None:
            options["top_p"] = top_p
        if seed is not None:
            options["seed"] = seed
        if top_k is not None:
            options["top_k"] = top_k
        if min_p is not None:
            options["min_p"] = min_p
        if stop is not None:
            options["stop"] = stop if isinstance(stop, list) else [stop]
        if options:
            request_body["options"] = options
        if logprobs is not None:
            request_body["logprobs"] = logprobs
        if top_logprobs is not None:
            request_body["top_logprobs"] = top_logprobs
        if response_format == ResponseFormat.JSON:
            request_body["format"] = "json"
        elif isinstance(response_format, dict):
            kind = response_format.get("type")
            if (
                kind == "json_schema"
                and set(response_format) == {"type", "schema"}
                and isinstance(response_format["schema"], dict)
            ):
                request_body["format"] = response_format["schema"]
            elif kind in ("json", "text") and set(response_format) == {"type"}:
                if kind == "json":
                    request_body["format"] = "json"
            else:
                self._request_error("invalid_request", "Invalid Ollama response format")
        elif response_format not in (None, ResponseFormat.TEXT):
            self._request_error("invalid_request", "Invalid Ollama response format")
        if self._profile == "ollama_cloud_via_daemon" and "format" in request_body:
            self._request_error(
                "unsupported_capability", "Ollama Cloud structured output is unsupported"
            )
        if thinking in ("enabled", "disabled"):
            request_body["think"] = thinking == "enabled"
        elif thinking == "auto" and any(
            token in effective_model.lower()
            for token in ("qwen3", "deepseek-r1", "deepseek-v3", ":thinking")
        ):
            request_body["think"] = True

        # Ollama supports OpenAI-compatible tool format
        if tools:
            request_body["tools"] = [t.to_dict() for t in tools]

        return request_body

    def _request_error(self, code, message):
        error = ProviderError(message, provider="ollama", model=self._model)
        error.code = code
        raise error

    @staticmethod
    def _finish_reason(data):
        if not data.get("done"):
            return None, None
        raw = data.get("done_reason")
        return (
            raw if raw in ("stop", "length", "tool_calls", "content_filter", "error") else None
        ), raw

    def _validate_response_fields(self, data):
        if not isinstance(data, dict):
            self._request_error("malformed_response", "Malformed Ollama response")
        if "error" in data:
            self._request_error("malformed_response", "Ollama response contains an error")
        message = data.get("message", {})
        if (
            not isinstance(message, dict)
            or type(data.get("done", False)) is not bool
            or any(
                data.get(key) is not None and not isinstance(data[key], str)
                for key in ("model", "done_reason")
            )
            or (message.get("thinking") is not None and not isinstance(message["thinking"], str))
            or ("content" in message and not isinstance(message["content"], str))
            or (
                message.get("tool_calls") is not None
                and not isinstance(message["tool_calls"], list)
            )
            or (data.get("logprobs") is not None and not isinstance(data["logprobs"], list))
            or any(
                key in data and (type(data[key]) is not int or data[key] < 0)
                for key in ("prompt_eval_count", "eval_count")
            )
        ):
            self._request_error("malformed_response", "Malformed Ollama response")

    def _parse_response(self, data: dict, effective_model: str) -> ChatResponse:
        """Parse Ollama API response."""
        self._validate_response_fields(data)
        if data.get("done") is not True:
            self._request_error("malformed_response", "Ollama response ended before done")
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
        finish_reason, raw_finish_reason = self._finish_reason(data)
        try:
            logprobs = (
                LogprobsData.from_dict({"content": data["logprobs"]})
                if data.get("logprobs") is not None
                else None
            )
        except (AttributeError, TypeError, ValueError):
            self._request_error("malformed_response", "Malformed Ollama logprobs")

        return ChatResponse(
            content=content,
            usage=token_usage,
            model=data.get("model") or effective_model,
            finish_reason=finish_reason,
            raw_finish_reason=raw_finish_reason,
            thinking=data.get("message", {}).get("thinking"),
            tool_calls=data.get("message", {}).get("tool_calls"),
            logprobs=logprobs,
            provider="ollama",
            declared_profile=self._profile,
            endpoint_origin=self._endpoint_origin(),
        )

    def _parse_stream_event(self, data: dict, effective_model: str) -> Optional[StreamChunk]:
        """Parse a single stream event from Ollama."""
        self._validate_response_fields(data)
        message = data.get("message", {})
        content = message.get("content", "")
        thinking = message.get("thinking")
        tool_calls = message.get("tool_calls")
        try:
            logprobs = (
                StreamLogprobsDelta.from_dict({"content": data["logprobs"]})
                if data.get("logprobs") is not None
                else None
            )
        except (AttributeError, TypeError, ValueError):
            self._request_error("malformed_response", "Malformed Ollama logprobs")
        is_done = data.get("done", False)

        if content or thinking or tool_calls or logprobs is not None or is_done:
            usage = None
            finish_reason, raw_finish_reason = self._finish_reason(data)
            if is_done:
                prompt_eval = data.get("prompt_eval_count", 0)
                eval_count = data.get("eval_count", 0)
                if prompt_eval or eval_count:
                    usage = TokenUsage(
                        input_tokens=prompt_eval,
                        output_tokens=eval_count,
                    )

            return StreamChunk(
                delta=content,
                terminal=is_done,
                model=data.get("model") or effective_model,
                finish_reason=finish_reason,
                raw_finish_reason=raw_finish_reason,
                thinking=thinking,
                tool_calls=tool_calls,
                logprobs=logprobs,
                declared_profile=self._profile,
                endpoint_origin=self._endpoint_origin(),
                usage=usage,
            )

        return None
