"""Utilities for working with streaming responses."""

from typing import Callable, Iterator, Optional

from nxuskit.types import ChatResponse, StreamChunk, TokenUsage


def collect_stream(stream: Iterator[StreamChunk]) -> ChatResponse:
    """
    Collect all chunks from a stream and return a ChatResponse.

    Args:
        stream: Iterator of StreamChunk objects from chat_stream()

    Returns:
        ChatResponse with accumulated content and final metadata
    """
    content = ""
    final_chunk = None

    for chunk in stream:
        content += chunk.delta
        final_chunk = chunk

    # Use final chunk's usage if available, otherwise default to zero
    if final_chunk and final_chunk.usage:
        token_usage = final_chunk.usage
    else:
        token_usage = TokenUsage(
            input_tokens=0,
            output_tokens=0,
            total_tokens=0,
        )

    model = final_chunk.model if final_chunk else "unknown"
    finish_reason = final_chunk.finish_reason if final_chunk else None

    return ChatResponse(
        content=content,
        usage=token_usage,
        model=model,
        finish_reason=finish_reason,
    )


def stream_with_callback(
    stream: Iterator[StreamChunk],
    callback: Callable[[str], None],
) -> ChatResponse:
    """
    Stream chunks while calling a callback for each chunk's delta.

    Useful for real-time processing of streamed content (e.g., printing to console).

    Args:
        stream: Iterator of StreamChunk objects from chat_stream()
        callback: Function called with each delta string

    Returns:
        ChatResponse with accumulated content and final metadata

    Example:
        def print_chunk(delta: str):
            print(delta, end="", flush=True)

        response = stream_with_callback(
            provider.chat_stream([Message.user("Generate a story")]),
            print_chunk
        )
    """
    content = ""
    final_chunk = None

    for chunk in stream:
        content += chunk.delta
        callback(chunk.delta)
        final_chunk = chunk

    if final_chunk and final_chunk.usage:
        token_usage = final_chunk.usage
    else:
        token_usage = TokenUsage(
            input_tokens=0,
            output_tokens=0,
            total_tokens=0,
        )

    model = final_chunk.model if final_chunk else "unknown"
    finish_reason = final_chunk.finish_reason if final_chunk else None

    return ChatResponse(
        content=content,
        usage=token_usage,
        model=model,
        finish_reason=finish_reason,
    )


def stream_to_file(
    stream: Iterator[StreamChunk],
    file_path: str,
    chunk_size: Optional[int] = None,
) -> ChatResponse:
    """
    Stream response chunks to a file.

    Args:
        stream: Iterator of StreamChunk objects from chat_stream()
        file_path: Path to write streamed content
        chunk_size: Optional buffer size for writes (defaults to immediate writes)

    Returns:
        ChatResponse with accumulated content and final metadata
    """
    content = ""
    final_chunk = None
    buffer = []
    buffer_bytes = 0

    with open(file_path, "w", encoding="utf-8") as f:
        for chunk in stream:
            content += chunk.delta
            buffer.append(chunk.delta)
            buffer_bytes += len(chunk.delta.encode("utf-8"))

            # Flush buffer if chunk_size exceeded or no chunk_size specified
            if chunk_size is None or buffer_bytes >= chunk_size:
                f.write("".join(buffer))
                f.flush()
                buffer = []
                buffer_bytes = 0

            final_chunk = chunk

        # Flush remaining content
        if buffer:
            f.write("".join(buffer))
            f.flush()

    if final_chunk and final_chunk.usage:
        token_usage = final_chunk.usage
    else:
        token_usage = TokenUsage(
            input_tokens=0,
            output_tokens=0,
            total_tokens=0,
        )

    model = final_chunk.model if final_chunk else "unknown"
    finish_reason = final_chunk.finish_reason if final_chunk else None

    return ChatResponse(
        content=content,
        usage=token_usage,
        model=model,
        finish_reason=finish_reason,
    )


class StreamBuffer:
    """
    Buffered streaming response handler.

    Accumulates chunks and provides access to intermediate states.
    """

    def __init__(self, max_buffer_size: int = 1024):
        """
        Initialize StreamBuffer.

        Args:
            max_buffer_size: Maximum size of internal buffer in characters
        """
        self.max_buffer_size = max_buffer_size
        self.content = ""
        self.final_chunk = None

    def add_chunk(self, chunk: StreamChunk) -> None:
        """Add a chunk to the buffer."""
        self.content += chunk.delta
        self.final_chunk = chunk

        # Trim buffer if it exceeds max size
        if len(self.content) > self.max_buffer_size:
            # Keep last max_buffer_size characters
            self.content = self.content[-self.max_buffer_size :]

    def get_content(self) -> str:
        """Get accumulated content."""
        return self.content

    def get_response(self) -> ChatResponse:
        """Get current state as ChatResponse."""
        if self.final_chunk and self.final_chunk.usage:
            token_usage = self.final_chunk.usage
        else:
            token_usage = TokenUsage(
                input_tokens=0,
                output_tokens=0,
                total_tokens=0,
            )

        model = self.final_chunk.model if self.final_chunk else "unknown"
        finish_reason = self.final_chunk.finish_reason if self.final_chunk else None

        return ChatResponse(
            content=self.content,
            usage=token_usage,
            model=model,
            finish_reason=finish_reason,
        )

    def process_stream(self, stream: Iterator[StreamChunk]) -> ChatResponse:
        """Process entire stream through buffer."""
        for chunk in stream:
            self.add_chunk(chunk)

        return self.get_response()
