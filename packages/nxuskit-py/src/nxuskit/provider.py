"""LLMProvider protocol definition."""

from typing import Any, Iterator, List, Optional, Protocol, Union

from nxuskit.message import Message
from nxuskit.tools import ToolDefinition
from nxuskit.types import ChatResponse, ModelInfo, ResponseFormat, StreamChunk


class LLMProvider(Protocol):
    """Protocol for LLM providers.

    All providers (native HTTP and FFI) satisfy this structural typing
    contract. Use this as a type annotation when writing code that accepts
    any provider.
    """

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
        response_format: Optional[ResponseFormat] = None,
        tools: Optional[List[ToolDefinition]] = None,
        tool_choice: Optional[Any] = None,
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
        response_format: Optional[ResponseFormat] = None,
        tools: Optional[List[ToolDefinition]] = None,
        tool_choice: Optional[Any] = None,
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
