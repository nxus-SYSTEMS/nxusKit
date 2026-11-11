"""Native stream ownership, with doubles only at the HTTP boundary."""

import socket

import pytest
import requests

from nxuskit.message import Message
from nxuskit.providers.factory import Provider


@pytest.fixture(autouse=True)
def no_network(monkeypatch):
    def blocked(*args, **kwargs):
        raise AssertionError("network forbidden in stream ownership tests")

    monkeypatch.setattr(requests, "request", blocked)
    monkeypatch.setattr(requests.sessions.Session, "request", blocked)
    for name in (
        "getaddrinfo",
        "gethostbyname",
        "gethostbyname_ex",
        "gethostbyaddr",
        "create_connection",
    ):
        monkeypatch.setattr(socket, name, blocked)
    monkeypatch.setattr(socket.socket, "connect", blocked)
    monkeypatch.setattr(socket.socket, "connect_ex", blocked)


def test_unmatched_http_is_blocked_before_dns(monkeypatch):
    def unexpected_dns(*args, **kwargs):
        pytest.fail("unmatched HTTP reached DNS")

    monkeypatch.setattr(socket, "getaddrinfo", unexpected_dns)
    with pytest.raises(AssertionError, match="network forbidden"):
        requests.get("https://unmatched.invalid/")


CASES = [
    (
        "openai",
        "/v1/chat/completions",
        b'data: {"choices":[{"delta":{"content":"alpha"}}]}',
        b'data: {"choices":[{"delta":null}]}',
        b"data: {broken",
    ),
    (
        "ollama",
        "/api/chat",
        b'{"message":{"content":"alpha"},"done":false}',
        b'{"message":null}',
        b"{broken",
    ),
    (
        "claude",
        "/v1/messages",
        b'data: {"type":"content_block_delta","delta":{"type":"text_delta","text":"alpha"}}',
        b'data: {"type":"content_block_delta","delta":null}',
        b"data: {broken",
    ),
]


@pytest.mark.parametrize(
    "provider_name,path,good,bad,malformed", CASES, ids=["openai", "ollama", "claude"]
)
@pytest.mark.parametrize(
    "exit_kind", ["complete", "close", "iteration_error", "parser_error", "never_started"]
)
def test_acquired_response_is_closed_once(
    monkeypatch, provider_name, path, good, bad, malformed, exit_kind
):
    fault = requests.ConnectionError("stream interrupted")

    class Response(requests.Response):
        def __init__(self):
            super().__init__()
            self.status_code = 200
            self.close_count = 0

        def close(self):
            self.close_count += 1

        def iter_lines(self):
            yield b""
            yield malformed  # Existing invalid-JSON tolerance must survive cleanup.
            yield good
            if exit_kind == "iteration_error":
                raise fault
            if exit_kind == "parser_error":
                yield bad  # Real parser sees valid JSON with an invalid value type.
            if provider_name != "ollama":
                yield b"data: [DONE]"
                pytest.fail("provider read beyond SSE completion")

    response = Response()
    calls = []

    def request(method, url, **kwargs):
        assert method == "POST"
        assert url == "https://provider.invalid" + path
        assert kwargs["stream"] is True
        assert kwargs["json"]["model"] == "selected"
        expected_content = (
            [{"type": "text", "text": "hello"}] if provider_name == "claude" else "hello"
        )
        assert kwargs["json"]["messages"] == [{"role": "user", "content": expected_content}]
        calls.append(url)
        return response

    monkeypatch.setattr(requests, "request", request)
    options = {"model": "selected", "api_url": "https://provider.invalid", "use_ffi": False}
    if provider_name != "ollama":
        options["api_key"] = "test"
    provider = getattr(Provider, provider_name)(**options)
    stream = provider.chat_stream([Message.user("hello")])
    assert calls == []
    assert response.close_count == 0
    if exit_kind == "never_started":
        stream.close()
        assert calls == []
        assert response.close_count == 0
        return

    assert next(stream).delta == "alpha"
    assert response.close_count == 0
    if exit_kind == "close":
        stream.close()
    elif exit_kind == "iteration_error":
        with pytest.raises(requests.ConnectionError) as raised:
            next(stream)
        assert raised.value is fault
    elif exit_kind == "parser_error":
        with pytest.raises(AttributeError, match="get"):
            next(stream)
    else:
        assert list(stream) == []
    stream.close()  # Exhaustion/error/explicit close must not double-release.
    assert response.close_count == 1
    assert calls == ["https://provider.invalid" + path]
