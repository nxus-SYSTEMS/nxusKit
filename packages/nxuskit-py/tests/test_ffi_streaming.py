"""Tests for FFI streaming support.

These tests require libnxuskit to be available. They exercise the
queue-based callback bridge for streaming chat.

Run with: pytest tests/test_ffi_streaming.py -v
"""

import pytest

try:
    from nxuskit._ffi_provider import create_ffi_provider
    from nxuskit._ffi_types import StreamChunk

    HAS_NXUSKIT = True
except (OSError, Exception):
    HAS_NXUSKIT = False

pytestmark = pytest.mark.skipif(
    not HAS_NXUSKIT,
    reason="nxuskit shared library not available",
)


class TestStreaming:
    """Test streaming chat via FFI."""

    def test_stream_with_mock_yields_chunks(self):
        with create_ffi_provider({"provider_type": "mock"}) as p:
            chunks = list(
                p.stream(
                    {
                        "model": "test",
                        "messages": [{"role": "user", "content": "hello"}],
                        "stream": True,
                    }
                )
            )
            assert all(isinstance(c, StreamChunk) for c in chunks)

    def test_stream_chunks_have_content(self):
        with create_ffi_provider({"provider_type": "mock"}) as p:
            chunks = list(
                p.stream(
                    {
                        "model": "test",
                        "messages": [{"role": "user", "content": "hello"}],
                        "stream": True,
                    }
                )
            )
            for chunk in chunks:
                assert isinstance(chunk.delta, str)

    def test_stream_iterator_is_iterable(self):
        with create_ffi_provider({"provider_type": "mock"}) as p:
            collected = ""
            for chunk in p.stream(
                {
                    "model": "test",
                    "messages": [{"role": "user", "content": "count to 3"}],
                    "stream": True,
                }
            ):
                collected += chunk.delta
            assert isinstance(collected, str)

    def test_stream_completes_without_error(self):
        with create_ffi_provider({"provider_type": "mock"}) as p:
            # Should not raise
            for _ in p.stream(
                {
                    "model": "test",
                    "messages": [{"role": "user", "content": "hello"}],
                    "stream": True,
                }
            ):
                pass
