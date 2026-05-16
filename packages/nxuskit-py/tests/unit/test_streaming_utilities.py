"""Unit tests for streaming utility functions."""

import os
import tempfile

import pytest

from nxuskit import StreamChunk
from nxuskit.streaming import (
    StreamBuffer,
    collect_stream,
    stream_to_file,
    stream_with_callback,
)


def create_test_stream():
    """Helper to create a test stream of chunks."""
    chunks = [
        StreamChunk(delta="Hello", model="test-model"),
        StreamChunk(delta=" ", model="test-model"),
        StreamChunk(delta="world", model="test-model"),
        StreamChunk(delta="!", model="test-model", finish_reason="end_turn"),
    ]
    return iter(chunks)


class TestCollectStream:
    """Tests for collect_stream utility."""

    def test_collect_stream_basic(self):
        """collect_stream should accumulate chunks into content."""
        response = collect_stream(create_test_stream())
        assert response.content == "Hello world!"
        assert response.model == "test-model"
        assert response.finish_reason == "end_turn"

    def test_collect_stream_empty(self):
        """collect_stream should handle empty stream."""
        empty_stream = iter([])
        response = collect_stream(empty_stream)
        assert response.content == ""
        assert response.model == "unknown"

    def test_collect_stream_single_chunk(self):
        """collect_stream should handle single chunk."""
        single = iter([StreamChunk(delta="Hello", model="model1")])
        response = collect_stream(single)
        assert response.content == "Hello"
        assert response.model == "model1"

    def test_collect_stream_usage_defaults_to_zero(self):
        """collect_stream should have zero token usage (not included in stream)."""
        response = collect_stream(create_test_stream())
        assert response.usage.input_tokens == 0
        assert response.usage.output_tokens == 0
        assert response.usage.total_tokens == 0


class TestStreamWithCallback:
    """Tests for stream_with_callback utility."""

    def test_stream_with_callback_calls_callback(self):
        """stream_with_callback should call callback for each delta."""
        deltas = []

        def capture(delta):
            deltas.append(delta)

        response = stream_with_callback(create_test_stream(), capture)
        assert deltas == ["Hello", " ", "world", "!"]
        assert response.content == "Hello world!"

    def test_stream_with_callback_order(self):
        """Callback should be called in order of chunks."""
        order = []

        def track_order(delta):
            order.append(len(order))

        stream_with_callback(create_test_stream(), track_order)
        assert order == [0, 1, 2, 3]

    def test_stream_with_callback_empty_stream(self):
        """stream_with_callback should handle empty stream."""
        deltas = []

        def capture(delta):
            deltas.append(delta)

        response = stream_with_callback(iter([]), capture)
        assert deltas == []
        assert response.content == ""

    def test_stream_with_callback_exception_propagates(self):
        """Exceptions in callback should propagate."""

        def failing_callback(delta):
            raise ValueError("Callback error")

        with pytest.raises(ValueError, match="Callback error"):
            stream_with_callback(create_test_stream(), failing_callback)


class TestStreamToFile:
    """Tests for stream_to_file utility."""

    def test_stream_to_file_writes_content(self):
        """stream_to_file should write chunks to file."""
        with tempfile.TemporaryDirectory() as tmpdir:
            file_path = os.path.join(tmpdir, "output.txt")

            response = stream_to_file(create_test_stream(), file_path)

            with open(file_path, "r") as f:
                content = f.read()

            assert content == "Hello world!"
            assert response.content == "Hello world!"

    def test_stream_to_file_creates_file(self):
        """stream_to_file should create file if it doesn't exist."""
        with tempfile.TemporaryDirectory() as tmpdir:
            file_path = os.path.join(tmpdir, "new_file.txt")
            assert not os.path.exists(file_path)

            stream_to_file(create_test_stream(), file_path)

            assert os.path.exists(file_path)

    def test_stream_to_file_overwrites_existing(self):
        """stream_to_file should overwrite existing file."""
        with tempfile.TemporaryDirectory() as tmpdir:
            file_path = os.path.join(tmpdir, "output.txt")

            # Write initial content
            with open(file_path, "w") as f:
                f.write("initial content")

            # Stream to same file
            stream_to_file(create_test_stream(), file_path)

            with open(file_path, "r") as f:
                content = f.read()

            assert content == "Hello world!"

    def test_stream_to_file_with_chunk_size(self):
        """stream_to_file should respect chunk_size parameter."""
        with tempfile.TemporaryDirectory() as tmpdir:
            file_path = os.path.join(tmpdir, "output.txt")

            # Use small chunk_size to test buffering
            response = stream_to_file(create_test_stream(), file_path, chunk_size=5)

            with open(file_path, "r") as f:
                content = f.read()

            assert content == "Hello world!"
            assert response.content == "Hello world!"

    def test_stream_to_file_empty_stream(self):
        """stream_to_file should handle empty stream."""
        with tempfile.TemporaryDirectory() as tmpdir:
            file_path = os.path.join(tmpdir, "output.txt")

            response = stream_to_file(iter([]), file_path)

            with open(file_path, "r") as f:
                content = f.read()

            assert content == ""
            assert response.content == ""


class TestStreamBuffer:
    """Tests for StreamBuffer class."""

    def test_buffer_add_chunk(self):
        """StreamBuffer should accumulate chunks."""
        buffer = StreamBuffer()
        buffer.add_chunk(StreamChunk(delta="Hello", model="test"))
        buffer.add_chunk(StreamChunk(delta=" world", model="test"))

        assert buffer.get_content() == "Hello world"

    def test_buffer_get_response(self):
        """StreamBuffer should return ChatResponse."""
        buffer = StreamBuffer()
        buffer.add_chunk(StreamChunk(delta="Test", model="model1"))

        response = buffer.get_response()
        assert response.content == "Test"
        assert response.model == "model1"

    def test_buffer_respects_max_size(self):
        """StreamBuffer should limit size to max_buffer_size."""
        buffer = StreamBuffer(max_buffer_size=10)
        buffer.add_chunk(StreamChunk(delta="12345", model="test"))
        buffer.add_chunk(StreamChunk(delta="67890", model="test"))
        buffer.add_chunk(StreamChunk(delta="ABCDE", model="test"))

        # Total is 15 chars, should be trimmed to last 10
        content = buffer.get_content()
        assert len(content) == 10
        assert content == "67890ABCDE"

    def test_buffer_process_stream(self):
        """StreamBuffer.process_stream should consume entire stream."""
        buffer = StreamBuffer()
        response = buffer.process_stream(create_test_stream())

        assert response.content == "Hello world!"
        assert response.finish_reason == "end_turn"

    def test_buffer_with_large_stream(self):
        """StreamBuffer should handle large streams."""
        buffer = StreamBuffer(max_buffer_size=100)

        # Create stream with many chunks
        chunks = [StreamChunk(delta=f"chunk{i}", model="test") for i in range(20)]

        for chunk in chunks:
            buffer.add_chunk(chunk)

        content = buffer.get_content()
        # Should contain last chunks up to max_buffer_size
        assert "chunk19" in content
        assert len(content) <= 100

    def test_buffer_intermediate_state(self):
        """StreamBuffer should allow reading intermediate states."""
        buffer = StreamBuffer()

        buffer.add_chunk(StreamChunk(delta="First", model="test"))
        response1 = buffer.get_response()
        assert response1.content == "First"

        buffer.add_chunk(StreamChunk(delta=" Second", model="test"))
        response2 = buffer.get_response()
        assert response2.content == "First Second"

    def test_buffer_with_stop_reason(self):
        """StreamBuffer should track final stop_reason."""
        buffer = StreamBuffer()
        buffer.add_chunk(StreamChunk(delta="Content", model="test"))
        buffer.add_chunk(StreamChunk(delta="", model="test", finish_reason="end_turn"))

        response = buffer.get_response()
        assert response.finish_reason == "end_turn"

    def test_buffer_empty(self):
        """StreamBuffer should handle empty state."""
        buffer = StreamBuffer()
        response = buffer.get_response()

        assert response.content == ""
        assert response.model == "unknown"
        assert response.finish_reason is None
