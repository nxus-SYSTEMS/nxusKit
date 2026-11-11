"""Base classes for provider implementations."""

import json
from abc import ABC, abstractmethod
from typing import Any, Iterator, List, Optional, Union

import requests

from nxuskit.errors import (
    AuthenticationError,
    LLMError,
    NetworkError,
    ProviderError,
    RateLimitError,
    TimeoutError,
)
from nxuskit.message import Message
from nxuskit.providers._resource_bounds import read_bounded_chunks
from nxuskit.tools import ToolDefinition
from nxuskit.types import ChatResponse, ModelInfo, ResponseFormat, StreamChunk


class BaseProvider(ABC):
    """Base class for all LLM provider implementations."""

    def capabilities(self):
        """Stable shipped adapter declaration; not effective model/endpoint support."""
        from nxuskit.provider import _native_adapter_capabilities

        if not hasattr(self, "_adapter_snapshot"):
            self._adapter_snapshot = _native_adapter_capabilities(self.provider_name)
        return self._adapter_snapshot

    async def capabilities_async(self):
        return self.capabilities()

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

        Unsupported canonical controls must fail explicitly before transport.
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

        Unsupported canonical controls must fail explicitly before transport.
        """
        pass

    def list_models(self) -> List[ModelInfo]:
        """List available models from this provider.

        Returns an empty list by default. Providers that support model
        listing should override this method.
        """
        return []

    @staticmethod
    def _retry_after_seconds(headers) -> Optional[float]:
        if headers is None:
            return None
        raw = headers.get("Retry-After")
        if raw is None:
            raw = headers.get("retry-after")
        if raw is None:
            return None
        try:
            seconds = int(str(raw).strip())
        except (TypeError, ValueError):
            return None
        return float(seconds) if seconds >= 0 else None

    def _handle_error(self, status_code: int, response_text: str, headers=None) -> None:
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
                retry_after=self._retry_after_seconds(headers),
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

    @staticmethod
    def _close_response(response) -> None:
        close = getattr(response, "close", None)
        if not callable(close):
            return
        if isinstance(response, requests.Response) and response.raw is None:
            return
        close()

    @staticmethod
    def _iter_response_content(response, chunk_size: int):
        if isinstance(response, requests.Response):
            buffered = getattr(response, "_content", False)
            if isinstance(buffered, (bytes, bytearray)):
                return (bytes(buffered),)
            if response.raw is None:
                return None
            return response.iter_content(chunk_size=chunk_size)
        method = getattr(type(response), "iter_content", None)
        if callable(method):
            return response.iter_content(chunk_size=chunk_size)
        return None

    @staticmethod
    def _iter_stream_response_bytes(response, *, sse: bool):
        """Yield raw stream bytes; iter_lines is compatibility-only for test doubles."""
        lines = None
        if isinstance(response, requests.Response):
            buffered = getattr(response, "_content", False)
            if isinstance(buffered, (bytes, bytearray)):
                if buffered:
                    yield bytes(buffered)
                return
            instance_lines = getattr(response, "__dict__", {}).get("iter_lines")
            if callable(instance_lines):
                lines = instance_lines
            elif response.raw is not None:
                yield from response.iter_content(chunk_size=64 * 1024)
                return
        else:
            method = getattr(type(response), "iter_content", None)
            if callable(method):
                yield from response.iter_content(chunk_size=64 * 1024)
                return

        if lines is None:
            lines = getattr(response, "iter_lines", None)
        if not callable(lines):
            error = ProviderError("stream body unavailable")
            error.code = "malformed_response"
            raise error

        delimiter = b"\n\n" if sse else b"\n"
        for line in lines():
            if isinstance(line, str):
                line = line.encode("utf-8")
            if not isinstance(line, (bytes, bytearray)):
                error = ProviderError("stream record is not bytes")
                error.code = "malformed_response"
                raise error
            yield bytes(line) + delimiter

    def _read_response_bytes(self, response: requests.Response) -> bytes:
        """Read a successful sync response body with the canonical unary limit."""
        try:
            chunks = self._iter_response_content(response, 64 * 1024)
            if chunks is not None:
                return read_bounded_chunks(chunks, provider=self.provider_name)

            body = getattr(response, "content", None)
            if isinstance(body, str):
                body = body.encode("utf-8")
            if not isinstance(body, (bytes, bytearray)):
                error = ProviderError("response body unavailable", provider=self.provider_name)
                error.code = "malformed_response"
                raise error
            return read_bounded_chunks((bytes(body),), provider=self.provider_name)
        except requests.exceptions.Timeout as error:
            raise TimeoutError(
                f"Request timed out: {str(error)}",
                provider=self.provider_name,
                model=self._model,
            ) from error
        except requests.exceptions.RequestException as error:
            raise NetworkError(
                f"Network error: {str(error)}",
                provider=self.provider_name,
                model=self._model,
            ) from error
        finally:
            self._close_response(response)

    def _read_response_json(self, response: requests.Response):
        """Read and decode a successful sync JSON response after bounded transport."""
        if not isinstance(response, requests.Response):
            has_iter = callable(getattr(type(response), "iter_content", None))
            body = getattr(response, "content", None)
            json_reader = getattr(response, "json", None)
            if (
                not has_iter
                and not isinstance(body, (bytes, bytearray, str))
                and callable(json_reader)
            ):
                try:
                    return json_reader()
                finally:
                    self._close_response(response)
        return json.loads(self._read_response_bytes(response))

    def _read_error_preview(self, response: requests.Response, limit: int = 200) -> str:
        """Read only the bounded prefix needed by the existing HTTP error mapper."""
        chunks = self._iter_response_content(response, 8192)
        if chunks is None:
            text = getattr(response, "text", "")
            if isinstance(text, str):
                return text[:limit]
            body = getattr(response, "content", b"")
            if isinstance(body, (bytes, bytearray)):
                return bytes(body[:limit]).decode("utf-8", errors="replace")
            return ""

        preview = bytearray()
        try:
            for chunk in chunks:
                if not chunk:
                    continue
                remaining = limit - len(preview)
                if remaining <= 0:
                    break
                preview.extend(chunk[:remaining])
                if len(preview) >= limit:
                    break
        except requests.exceptions.RequestException:
            pass
        return bytes(preview).decode("utf-8", errors="replace")

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
                try:
                    response_text = self._read_error_preview(response)
                finally:
                    self._close_response(response)
                self._handle_error(response.status_code, response_text, response.headers)

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
