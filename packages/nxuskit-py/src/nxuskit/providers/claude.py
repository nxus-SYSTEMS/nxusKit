"""Claude provider implementation."""

import base64
import json
import os
from typing import Any, Iterator, List, Optional, Union

from nxuskit.message import Message
from nxuskit.providers.base import BaseProvider
from nxuskit.tools import ToolCall, ToolDefinition
from nxuskit.types import (
    ChatResponse,
    ImageSourceType,
    ModelInfo,
    ResponseFormat,
    Role,
    StreamChunk,
    TokenUsage,
)
from nxuskit.vision import detect_image_type


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
