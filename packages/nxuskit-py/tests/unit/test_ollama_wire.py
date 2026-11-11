"""Literal Ollama native wire oracles; no daemon or production oracle helper."""

import json

import pytest

from nxuskit.message import Message
from nxuskit.providers.ollama import OllamaProvider
from nxuskit.types import ResponseFormat


@pytest.mark.parametrize("invalid", [False, True])
def test_unary_http_utf8_truth(httpserver, invalid):
    from nxuskit.errors import ProviderError

    content = b"\xff" if invalid else "€".encode("utf-8")
    body = b'{"model":"m","message":{"content":"' + content + b'"},"done":true}'
    httpserver.expect_request("/api/chat", method="POST").respond_with_data(
        body, content_type="application/json"
    )
    provider = OllamaProvider("m", api_url=httpserver.url_for(""))
    if invalid:
        with pytest.raises(ProviderError):
            provider.chat([Message.user("hi")])
    else:
        assert provider.chat([Message.user("hi")]).content == "€"
    httpserver.check_assertions()


@pytest.mark.parametrize("error_value", ["private upstream detail", None, {}, False])
@pytest.mark.parametrize("streaming", [False, True])
def test_error_member_cannot_be_success(monkeypatch, error_value, streaming):
    from nxuskit.errors import ProviderError

    payload = {
        "model": "actual",
        "message": {"content": "not success"},
        "done": True,
        "done_reason": "stop",
        "error": error_value,
    }
    _assert_native_transport_rejected(monkeypatch, payload, streaming, ProviderError)


@pytest.mark.parametrize("done", [None, False])
def test_unary_requires_explicit_completion(monkeypatch, done):
    from nxuskit.errors import ProviderError

    payload = {"message": {"content": "partial"}}
    if done is not None:
        payload["done"] = done
    _assert_native_transport_rejected(monkeypatch, payload, False, ProviderError)


def _assert_native_transport_rejected(monkeypatch, payload, streaming, error_type):
    import json

    provider = OllamaProvider("requested")

    class Response:
        closed = False

        @property
        def content(self):
            return json.dumps(payload).encode("utf-8")

        def iter_lines(self):
            yield json.dumps(payload).encode()

        def close(self):
            self.closed = True

    response = Response()
    monkeypatch.setattr(provider, "_make_request", lambda **_: response)
    with pytest.raises(error_type) as caught:
        if streaming:
            list(provider.chat_stream([]))
        else:
            provider.chat([])
    assert "private upstream detail" not in str(caught.value)
    if streaming:
        assert response.closed


@pytest.mark.parametrize("line", [b"not-json", b"\xff", b'{"done":false}'])
def test_stream_invalid_record_or_eof_is_error(monkeypatch, line):
    from nxuskit.errors import ProviderError

    provider = OllamaProvider("requested")

    class Response:
        closed = False

        def iter_lines(self):
            yield line

        def close(self):
            self.closed = True

    response = Response()
    monkeypatch.setattr(provider, "_make_request", lambda **_: response)
    with pytest.raises(ProviderError):
        list(provider.chat_stream([]))
    assert response.closed


def test_stream_stops_after_first_explicit_terminal(monkeypatch):
    provider = OllamaProvider("requested")

    class Response:
        closed = False

        def iter_lines(self):
            yield b'{"done":true,"done_reason":"vendor-new"}'
            raise AssertionError("consumed after terminal")

        def close(self):
            self.closed = True

    response = Response()
    monkeypatch.setattr(provider, "_make_request", lambda **_: response)
    chunks = list(provider.chat_stream([]))
    assert len(chunks) == 1 and chunks[0].is_final()
    assert chunks[0].finish_reason is None and chunks[0].raw_finish_reason == "vendor-new"
    assert response.closed


def test_generation_options_are_nested():
    body = OllamaProvider("local")._build_request(
        [Message.user("Hi")], "local", temperature=0.2, max_tokens=7, top_p=0.8, stop="END"
    )
    assert body == {
        "model": "local",
        "messages": [{"role": "user", "content": "Hi"}],
        "stream": False,
        "options": {"temperature": 0.2, "num_predict": 7, "top_p": 0.8, "stop": ["END"]},
    }


@pytest.mark.parametrize("mode,expected", [("enabled", True), ("disabled", False), ("auto", True)])
def test_thinking_request_literal(mode, expected):
    body = OllamaProvider("qwen3:8b")._build_request([], "qwen3:8b", thinking=mode)
    assert body == {"model": "qwen3:8b", "messages": [], "stream": False, "think": expected}


def test_format_is_direct_schema_and_omissions_stay_absent():
    provider = OllamaProvider("local")
    schema = {"type": "object", "properties": {"x": {"type": "integer"}}, "required": ["x"]}
    body = provider._build_request(
        [], "local", response_format={"type": "json_schema", "schema": schema}
    )
    assert body["format"] == {
        "type": "object",
        "properties": {"x": {"type": "integer"}},
        "required": ["x"],
    }
    assert "json_schema" not in body["format"]
    assert provider._build_request([], "local", response_format=ResponseFormat.TEXT) == {
        "model": "local",
        "messages": [],
        "stream": False,
    }
    assert (
        provider._build_request([], "local", response_format=ResponseFormat.JSON)["format"]
        == "json"
    )


@pytest.mark.parametrize(
    "reason,canonical",
    [("stop", "stop"), ("length", "length"), ("future_reason", None), (None, None)],
)
def test_unary_and_terminal_without_message_preserve_finish_truth(reason, canonical):
    provider = OllamaProvider("requested")
    data = {"model": "actual", "done": True, "prompt_eval_count": 10, "eval_count": 2}
    if reason is not None:
        data["done_reason"] = reason
    chunk = provider._parse_stream_event(data, "requested")
    assert chunk is not None
    assert chunk.is_final()
    assert (chunk.model, chunk.finish_reason, chunk.raw_finish_reason) == (
        "actual",
        canonical,
        reason,
    )
    assert chunk.usage.total_tokens == 12
    data["message"] = {"content": "answer", "thinking": "reason"}
    response = provider._parse_response(data, "requested")
    assert (response.content, response.thinking, response.provider) == (
        "answer",
        "reason",
        "ollama",
    )
    assert (response.model, response.finish_reason, response.raw_finish_reason) == (
        "actual",
        canonical,
        reason,
    )


def test_thinking_only_stream_chunk_is_not_dropped():
    chunk = OllamaProvider("local")._parse_stream_event(
        {"message": {"content": "", "thinking": "reason"}, "done": False}, "local"
    )
    assert chunk is not None
    assert (chunk.delta, chunk.thinking, chunk.finish_reason) == ("", "reason", None)


@pytest.mark.parametrize(
    "patch",
    [
        {"model": ["private-payload"]},
        {"message": {"content": "ok", "thinking": {"private-payload": 1}}},
        {"done_reason": 17},
        {"done": "true"},
        {"done": 1},
        {"done": None},
        {"message": []},
        {"message": {"content": 123}},
        {"message": {"content": []}},
        {"prompt_eval_count": -1},
        {"prompt_eval_count": True},
        {"eval_count": "3"},
        {"eval_count": 1.5},
        {"eval_count": None},
    ],
)
def test_malformed_response_truth_fields_are_sanitized_errors(patch):
    from nxuskit.errors import ProviderError

    provider = OllamaProvider("requested")
    payload = {
        "model": "actual",
        "message": {"content": "ok", "thinking": "why"},
        "done": True,
        "done_reason": "stop",
        **patch,
    }
    for parse in (provider._parse_response, provider._parse_stream_event):
        with pytest.raises(ProviderError) as caught:
            parse(payload, "requested")
        assert caught.value.code == "malformed_response"
        assert str(caught.value) == "Malformed Ollama response"


def test_absent_optional_response_fields_keep_existing_meaning():
    provider = OllamaProvider("requested")
    response = provider._parse_response({"message": {"content": "ok"}, "done": True}, "requested")
    assert (
        response.model,
        response.thinking,
        response.raw_finish_reason,
        response.finish_reason,
    ) == (
        "requested",
        None,
        None,
        None,
    )


@pytest.mark.parametrize(
    "kwargs,code",
    [
        ({"thinking": "sometimes"}, "invalid_request"),
        ({"tool_choice": "required"}, "unsupported_capability"),
        (
            {"response_format": {"type": "object", "name": "fact", "schema": {}, "strict": True}},
            "invalid_request",
        ),
    ],
)
def test_invalid_controls_fail_before_transport(monkeypatch, kwargs, code):
    from nxuskit.errors import ProviderError

    provider = OllamaProvider("local")

    def forbidden(**_):
        raise AssertionError("invalid request reached HTTP")

    monkeypatch.setattr(provider, "_make_request", forbidden)
    for call in (
        lambda: provider.chat([], **kwargs),
        lambda: list(provider.chat_stream([], **kwargs)),
    ):
        with pytest.raises(ProviderError) as caught:
            call()
        assert caught.value.code == code


@pytest.mark.parametrize("seed", [-1, -(2**63), 2**63 - 1, None, 7])
def test_public_unary_and_stream_forward_same_controls_and_close(monkeypatch, seed):
    import json

    provider = OllamaProvider("local")
    calls = []

    class Response:
        closed = False

        @property
        def content(self):
            return json.dumps(
                {
                    "model": "actual",
                    "message": {"content": "yes", "thinking": "why"},
                    "done": True,
                    "done_reason": "length",
                }
            ).encode("utf-8")

        def iter_lines(self):
            yield b'{"message":{"thinking":"why"},"done":false}'
            yield b'{"model":"actual","done":true,"done_reason":"length"}'

        def close(self):
            self.closed = True

    response = Response()

    def transport(**kwargs):
        calls.append(kwargs)
        return response

    monkeypatch.setattr(provider, "_make_request", transport)
    controls = {
        "thinking": "disabled",
        "max_tokens": 7,
        "top_k": 37,
        "min_p": 0.15,
        "response_format": ResponseFormat.JSON,
        "seed": seed,
    }
    assert provider.chat([], **controls).thinking == "why"
    chunks = list(provider.chat_stream([], **controls))
    assert len(chunks) == 2 and chunks[-1].raw_finish_reason == "length"
    assert response.closed
    for call, streaming in zip(calls, (False, True)):
        assert call["url"] == "http://localhost:11434/api/chat"
        assert call["json_data"] == {
            "model": "local",
            "messages": [],
            "stream": streaming,
            "options": {
                "num_predict": 7,
                "top_k": 37,
                "min_p": 0.15,
                **({"seed": seed} if seed is not None else {}),
            },
            "think": False,
            "format": "json",
        }


@pytest.mark.parametrize(
    "controls",
    [
        {"seed": -(2**63) - 1},
        {"seed": 2**63},
        {"seed": True},
        {"seed": 1.5},
        {"top_k": 0},
        {"top_k": 2147483648},
        {"top_k": True},
        {"top_k": 1.5},
        {"min_p": -0.1},
        {"min_p": 1.1},
        {"min_p": float("nan")},
        {"min_p": float("inf")},
        {"min_p": True},
        {"temperature": float("nan")},
        {"temperature": 2.1},
        {"max_tokens": 0},
        {"top_p": -0.1},
        {"stop": [""]},
        {"top_logprobs": 21},
    ],
)
def test_ollama_invalid_controls_reject_before_transport(monkeypatch, controls):
    from nxuskit.errors import ProviderError

    provider = OllamaProvider("local")

    def forbidden(**_):
        raise AssertionError("invalid controls reached HTTP")

    monkeypatch.setattr(provider, "_make_request", forbidden)
    for call in (
        provider.chat,
        lambda messages, **kwargs: list(provider.chat_stream(messages, **kwargs)),
    ):
        with pytest.raises(ProviderError, match="invalid request control"):
            call([], **controls)


def test_ollama_logprobs_wire_and_unary_mapping(monkeypatch):
    from nxuskit.message import Message

    provider = OllamaProvider("local")
    captured = {}

    class Response:
        content = json.dumps(
            {
                "model": "local",
                "message": {"role": "assistant", "content": "ok"},
                "done": True,
                "done_reason": "stop",
                "prompt_eval_count": 1,
                "eval_count": 1,
                "logprobs": [
                    {
                        "token": "ok",
                        "logprob": -0.1,
                        "bytes": [111, 107],
                        "top_logprobs": [{"token": "OK", "logprob": -1.0}],
                    }
                ],
            }
        ).encode()

    def fake_request(**kwargs):
        captured.update(kwargs)
        return Response()

    monkeypatch.setattr(provider, "_make_request", fake_request)
    response = provider.chat(
        [Message.user("hi")], logprobs=True, top_logprobs=5
    )
    assert captured["json_data"]["logprobs"] is True
    assert captured["json_data"]["top_logprobs"] == 5
    assert response.logprobs is not None
    assert response.logprobs.content[0].token == "ok"


@pytest.mark.parametrize(
    "control,value",
    [
        ("presence_penalty", 0.4),
        ("frequency_penalty", -0.3),
        ("reasoning_effort", "medium"),
    ],
)
def test_ollama_unsupported_canonical_controls_reject_before_transport(
    monkeypatch, control, value
):
    from nxuskit.errors import ProviderError

    provider = OllamaProvider("local")

    def forbidden(**_):
        raise AssertionError("unsupported controls reached HTTP")

    monkeypatch.setattr(provider, "_make_request", forbidden)
    for call in (
        provider.chat,
        lambda messages, **kwargs: list(provider.chat_stream(messages, **kwargs)),
    ):
        with pytest.raises(ProviderError, match="unsupported") as caught:
            call([], **{control: value})
        assert getattr(caught.value, "code", None) == "unsupported_capability"


def test_profile_is_declaration_not_detected_execution():
    unknown = OllamaProvider("cloud-model", api_url="http://localhost:11434")
    local = OllamaProvider(
        "cloud-model",
        api_url="https://user:secret@example.com:443/private?q=secret",
        profile="ollama_local",
    )
    data = {"done": True, "done_reason": "stop", "message": {"content": "ok"}}
    first = unknown._parse_response(data, "cloud-model")
    second = local._parse_response(data, "cloud-model")
    assert first.declared_profile is None
    assert first.endpoint_origin == "http://localhost:11434"
    assert second.declared_profile == "ollama_local"
    assert second.endpoint_origin == "https://example.com"
    assert "secret" not in repr(second)


def test_cloud_structured_output_rejected_before_http(monkeypatch):
    from nxuskit.errors import ProviderError

    provider = OllamaProvider("any", profile="ollama_cloud_via_daemon")

    def forbidden(**_):
        raise AssertionError("cloud format reached HTTP")

    monkeypatch.setattr(provider, "_make_request", forbidden)
    for fmt in (ResponseFormat.JSON, {"type": "json_schema", "schema": {"type": "object"}}):
        for call in (
            lambda: provider.chat([], response_format=fmt),
            lambda: list(provider.chat_stream([], response_format=fmt)),
        ):
            with pytest.raises(ProviderError) as caught:
                call()
            assert caught.value.code == "unsupported_capability"
    assert "format" not in provider._build_request([], "any", response_format=ResponseFormat.TEXT)


def test_invalid_profile_rejects_and_ffi_preserves_declaration(monkeypatch):
    from nxuskit.errors import ProviderError
    from nxuskit.providers import factory

    calls = []
    sentinel = object()

    def fake_ffi(*args, **kwargs):
        calls.append((args, kwargs))
        return sentinel

    monkeypatch.setattr(factory, "_make_ffi_provider", fake_ffi)
    with pytest.raises(ProviderError) as caught:
        OllamaProvider("any", profile="local")
    assert caught.value.code == "invalid_request"

    assert factory.Provider.ollama(use_ffi=True, profile="ollama_local") is sentinel
    assert calls and calls[0][1]["declared_profile"] == "ollama_local"
    assert (
        factory.Provider.ollama(use_ffi=False)
        ._parse_response({"message": {}, "done": True}, "any")
        .declared_profile
        is None
    )
