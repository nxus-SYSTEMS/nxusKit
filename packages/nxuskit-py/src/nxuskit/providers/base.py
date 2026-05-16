"""Base classes for provider implementations."""

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
from nxuskit.tools import ToolDefinition
from nxuskit.types import ChatResponse, ModelInfo, ResponseFormat, StreamChunk


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
