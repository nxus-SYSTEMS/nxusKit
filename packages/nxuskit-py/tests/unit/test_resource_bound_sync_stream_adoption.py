import pytest
import requests

from nxuskit.errors import ProviderError
from nxuskit.providers._resource_bounds import MAX_STREAM_RECORD_EVENT_BYTES
from nxuskit.providers.claude import ClaudeProvider
from nxuskit.providers.ollama import OllamaProvider
from nxuskit.providers.openai import OpenAIProvider


class ChunkResponse(requests.Response):
    def __init__(self, chunks, *, error=None):
        super().__init__()
        self.status_code = 200
        self.raw = object()
        self._content = False
        self._chunks = list(chunks)
        self._error = error
        self.close_count = 0

    def iter_content(self, chunk_size=1, **_kwargs):
        if self._error is not None:
            raise self._error
        yield from self._chunks

    def iter_lines(self, **_kwargs):
        raise AssertionError("real streamed response must use byte chunks, not iter_lines")

    def close(self):
        self.close_count += 1


class LegacyLineResponse:
    def __init__(self, lines):
        self.status_code = 200
        self.headers = {}
        self._lines = list(lines)
        self.closed = False

    def iter_lines(self):
        yield from self._lines

    def close(self):
        self.closed = True


def test_openai_sync_stream_rejects_one_byte_over_sse_event(monkeypatch):
    response = ChunkResponse([b"x" * (MAX_STREAM_RECORD_EVENT_BYTES + 1) + b"\n\n"])
    provider = OpenAIProvider("m", api_key="test", api_url="https://example.invalid/v1")
    monkeypatch.setattr(provider, "_make_request", lambda **_kwargs: response)

    with pytest.raises(ProviderError) as caught:
        list(provider.chat_stream([]))
    assert getattr(caught.value, "code", None) == "malformed_response"
    assert response.close_count == 1


def test_ollama_sync_stream_rejects_one_byte_over_ndjson_record(monkeypatch):
    response = ChunkResponse([b"x" * (MAX_STREAM_RECORD_EVENT_BYTES + 1) + b"\n"])
    provider = OllamaProvider("m", api_url="http://example.invalid")
    monkeypatch.setattr(provider, "_make_request", lambda **_kwargs: response)

    with pytest.raises(ProviderError) as caught:
        list(provider.chat_stream([]))
    assert getattr(caught.value, "code", None) == "malformed_response"
    assert response.close_count == 1


def test_openai_sync_stream_fragmented_utf8_and_crlf(monkeypatch):
    body = (
        'data: {"model":"actual","choices":[{"delta":{"content":"€"},'
        '"finish_reason":null}]}\r\n\r\ndata: [DONE]\r\n\r\n'
    ).encode()
    euro = body.index("€".encode())
    response = ChunkResponse(
        [body[: euro + 1], body[euro + 1 : euro + 2], body[euro + 2 : -3], body[-3:]]
    )
    provider = OpenAIProvider("m", api_key="test", api_url="https://example.invalid/v1")
    monkeypatch.setattr(provider, "_make_request", lambda **_kwargs: response)

    chunks = list(provider.chat_stream([]))
    assert chunks[0].delta == "€"
    assert chunks[-1].is_final()
    assert response.close_count == 1


def test_claude_sync_stream_multiline_sse(monkeypatch):
    body = (
        b"event: content_block_delta\r\n"
        b'data: {"type":"content_block_delta",\r\n'
        b'data: "delta":{"type":"text_delta","text":"hi"}}\r\n\r\n'
        b"event: message_stop\r\n"
        b'data: {"type":"message_stop"}\r\n\r\n'
    )
    response = ChunkResponse([body[:31], body[31:79], body[79:]])
    provider = ClaudeProvider("m", api_key="test", api_url="https://example.invalid")
    monkeypatch.setattr(provider, "_make_request", lambda **_kwargs: response)

    chunks = list(provider.chat_stream([]))
    assert chunks[0].delta == "hi"
    assert chunks[-1].is_final()
    assert response.close_count == 1


def test_ollama_sync_stream_accepts_final_record_without_newline(monkeypatch):
    body = (
        b'{"model":"actual","message":{"content":"hi"},"done":false}\n'
        b'{"model":"actual","done":true}'
    )
    response = ChunkResponse([body[:17], body[17:63], body[63:]])
    provider = OllamaProvider("m", api_url="http://example.invalid")
    monkeypatch.setattr(provider, "_make_request", lambda **_kwargs: response)

    chunks = list(provider.chat_stream([]))
    assert chunks[0].delta == "hi"
    assert chunks[-1].is_final()
    assert response.close_count == 1


def test_sync_sse_legacy_line_only_double_remains_supported(monkeypatch):
    response = LegacyLineResponse(
        [
            b'data: {"model":"actual","choices":[{"delta":{"content":"legacy"},'
            b'"finish_reason":null}]}',
            b"data: [DONE]",
        ]
    )
    provider = OpenAIProvider("m", api_key="test", api_url="https://example.invalid/v1")
    monkeypatch.setattr(provider, "_make_request", lambda **_kwargs: response)

    chunks = list(provider.chat_stream([]))
    assert chunks[0].delta == "legacy"
    assert chunks[-1].is_final()
    assert response.closed


def test_sync_stream_transport_error_propagates_and_closes(monkeypatch):
    fault = requests.ConnectionError("stream interrupted")
    response = ChunkResponse([], error=fault)
    provider = OpenAIProvider("m", api_key="test", api_url="https://example.invalid/v1")
    monkeypatch.setattr(provider, "_make_request", lambda **_kwargs: response)

    with pytest.raises(requests.ConnectionError) as caught:
        list(provider.chat_stream([]))
    assert caught.value is fault
    assert response.close_count == 1


def test_sync_sse_requests_response_instance_iter_lines_override_remains_supported(monkeypatch):
    from io import BytesIO

    response = requests.Response()
    response.status_code = 200
    response.raw = BytesIO()
    response._content = False
    response.iter_lines = lambda: iter(
        [
            b'data: {"model":"actual","choices":[{"delta":{"content":"factory"},'
            b'"finish_reason":null}]}',
            b"data: [DONE]",
        ]
    )

    provider = OpenAIProvider("m", api_key="test", api_url="https://example.invalid/v1")
    monkeypatch.setattr(provider, "_make_request", lambda **_kwargs: response)

    chunks = list(provider.chat_stream([]))
    assert chunks[0].delta == "factory"
    assert chunks[-1].is_final()
