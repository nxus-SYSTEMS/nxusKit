"""OpenAI-compatible provider base class.

This module provides a base class for providers that use OpenAI-compatible APIs,
enabling code reuse across multiple providers (Groq, Mistral, Fireworks, etc.).
"""

import base64
import json
from abc import abstractmethod
from typing import Any, Iterator, List, Optional, Union

from nxuskit.message import Message
from nxuskit.providers.base import BaseProvider
from nxuskit.tools import ToolCall, ToolDefinition
from nxuskit.types import (
    ChatResponse,
    ImageSourceType,
    ModelInfo,
    ResponseFormat,
    StreamChunk,
    TokenUsage,
)
from nxuskit.vision import detect_image_type


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
