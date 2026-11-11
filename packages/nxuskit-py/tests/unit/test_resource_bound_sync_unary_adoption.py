import pytest
import requests

from nxuskit.errors import NetworkError, ProviderError, RateLimitError
from nxuskit.errors import TimeoutError as SDKTimeoutError
from nxuskit.providers._resource_bounds import MAX_UNARY_RESPONSE_BODY_BYTES
from nxuskit.providers.ollama import OllamaProvider


class FakeResponse:
    def __init__(self, body: bytes, status_code: int = 200, *, bomb_text: bool = False):
        self._body = body
        self.status_code = status_code
        self.headers = {}
        self.closed = False
        self._bomb_text = bomb_text

    @property
    def content(self):
        return self._body

    @property
    def text(self):
        if self._bomb_text:
            raise AssertionError("unbounded response.text must not be read")
        return self._body.decode("utf-8", errors="replace")

    def iter_content(self, chunk_size=1, **_kwargs):
        for offset in range(0, len(self._body), chunk_size):
            yield self._body[offset : offset + chunk_size]

    def close(self):
        self.closed = True


class LegacyBufferedResponse:
    """Old unit-test double: body property only, no iter_content or close."""

    def __init__(self, body: bytes):
        self.content = body
        self.status_code = 200
        self.headers = {}


class RaisingResponse(FakeResponse):
    def __init__(self, error):
        super().__init__(b"")
        self._error = error

    def iter_content(self, chunk_size=1, **_kwargs):
        raise self._error
        yield b""


def ollama_body(content: bytes) -> bytes:
    return b'{"model":"m","message":{"content":"' + content + b'"},"done":true}'


def test_sync_unary_transport_forces_streaming_before_sdk_read(monkeypatch):
    seen = {}

    def fake_request(**kwargs):
        seen["stream"] = kwargs["stream"]
        return FakeResponse(ollama_body(b"ok"))

    monkeypatch.setattr(requests, "request", fake_request)
    response = OllamaProvider("m", api_url="http://example.invalid").chat([])
    assert response.content == "ok"
    assert seen["stream"] is True


def test_sync_unary_one_byte_over_is_typed_nonretryable(monkeypatch):
    prefix = b'{"model":"m","message":{"content":"'
    suffix = b'"},"done":true}'
    fill = MAX_UNARY_RESPONSE_BODY_BYTES + 1 - len(prefix) - len(suffix)
    body = prefix + (b"a" * fill) + suffix
    assert len(body) == MAX_UNARY_RESPONSE_BODY_BYTES + 1

    monkeypatch.setattr(requests, "request", lambda **_kwargs: FakeResponse(body))
    with pytest.raises(ProviderError) as caught:
        OllamaProvider("m", api_url="http://example.invalid").chat([])
    assert getattr(caught.value, "code", None) == "malformed_response"
    assert not caught.value.is_retryable


def test_sync_rate_limit_uses_bounded_preview_not_response_text(monkeypatch):
    response = FakeResponse(
        b"x" * (MAX_UNARY_RESPONSE_BODY_BYTES + 1),
        status_code=429,
        bomb_text=True,
    )
    response.headers["Retry-After"] = "2"
    monkeypatch.setattr(requests, "request", lambda **_kwargs: response)

    with pytest.raises(RateLimitError) as caught:
        OllamaProvider("m", api_url="http://example.invalid").chat([])
    assert caught.value.retry_after == 2.0
    assert response.closed


def test_sync_unary_legacy_buffered_response_double_remains_supported(monkeypatch):
    provider = OllamaProvider("m", api_url="http://example.invalid")
    monkeypatch.setattr(
        provider,
        "_make_request",
        lambda **_kwargs: LegacyBufferedResponse(ollama_body(b"legacy")),
    )
    assert provider.chat([]).content == "legacy"


def test_sync_unary_streamed_timeout_maps_to_sdk_timeout(monkeypatch):
    monkeypatch.setattr(
        requests,
        "request",
        lambda **_kwargs: RaisingResponse(requests.exceptions.Timeout("read timed out")),
    )
    with pytest.raises(SDKTimeoutError):
        OllamaProvider("m", api_url="http://example.invalid").chat([])


def test_sync_unary_streamed_disconnect_maps_to_network_error(monkeypatch):
    monkeypatch.setattr(
        requests,
        "request",
        lambda **_kwargs: RaisingResponse(requests.exceptions.ConnectionError("disconnected")),
    )
    with pytest.raises(NetworkError):
        OllamaProvider("m", api_url="http://example.invalid").chat([])


def test_sync_unary_prebuffered_requests_response_remains_supported(monkeypatch):
    from io import BytesIO

    response = requests.Response()
    response.status_code = 200
    response.raw = BytesIO()
    response._content = ollama_body(b"buffered")

    provider = OllamaProvider("m", api_url="http://example.invalid")
    monkeypatch.setattr(provider, "_make_request", lambda **_kwargs: response)
    assert provider.chat([]).content == "buffered"
