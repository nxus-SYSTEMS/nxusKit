"""Ollama provider implementation."""

import base64
import json
from typing import Any, Iterator, List, Optional, Union

from nxuskit.message import Message
from nxuskit.providers.base import BaseProvider
from nxuskit.tools import ToolDefinition
from nxuskit.types import (
    ChatResponse,
    ImageSourceType,
    ModelInfo,
    ResponseFormat,
    StreamChunk,
    TokenUsage,
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
