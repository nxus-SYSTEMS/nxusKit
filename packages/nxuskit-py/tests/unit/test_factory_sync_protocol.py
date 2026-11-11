"""Execute the public factory contract with doubles only at HTTP/C ABI boundaries."""

import importlib.util
import json
import socket
import sys
import types
from pathlib import Path

import cffi
import pytest

from nxuskit.message import Message
from nxuskit.providers.factory import Provider
from nxuskit.tools import ToolDefinition
from nxuskit.types import ResponseFormat


@pytest.fixture(autouse=True)
def no_network(monkeypatch):
    """Test isolation only: reject unmatched HTTP before DNS or connection."""
    import requests

    def blocked(*args, **kwargs):
        raise AssertionError("network forbidden in factory protocol tests")

    monkeypatch.setattr(requests, "request", blocked)
    monkeypatch.setattr(requests.sessions.Session, "request", blocked)
    monkeypatch.setattr(socket, "getaddrinfo", blocked)
    monkeypatch.setattr(socket, "create_connection", blocked)
    monkeypatch.setattr(socket.socket, "connect", blocked)
    monkeypatch.setattr(socket.socket, "connect_ex", blocked)


def test_network_guard_blocks_unmatched_http_before_dns(monkeypatch):
    import requests

    def unexpected_dns(*args, **kwargs):
        pytest.fail("unmatched HTTP reached DNS")

    monkeypatch.setattr(socket, "getaddrinfo", unexpected_dns)
    with pytest.raises(AssertionError, match="network forbidden"):
        requests.request("GET", "https://unmatched.invalid/")
    with pytest.raises(AssertionError, match="network forbidden"):
        requests.get("https://unmatched.invalid/")
    with socket.socket() as sock:
        with pytest.raises(AssertionError, match="network forbidden"):
            sock.connect(("127.0.0.1", 9))


@pytest.fixture
def abi(monkeypatch):
    ffi = cffi.FFI()

    class Library:
        def __init__(self):
            self.requests = []
            self.configs = []
            self.frees = []
            self.buffers = []
            self.error = None

        def string(self, value):
            buf = ffi.new("char[]", json.dumps(value).encode())
            self.buffers.append(buf)
            return buf

        def nxuskit_create_provider(self, config):
            self.configs.append(json.loads(config))
            return ffi.cast("void *", 1)

        def nxuskit_chat(self, handle, request):
            self.requests.append(json.loads(request))
            return ffi.NULL if self.error else ffi.cast("void *", 2)

        def nxuskit_response_json(self, response):
            return self.string({"content": "reply", "model": "chosen", "provider": "openai"})

        def nxuskit_free_response(self, response):
            self.frees.append("response")

        def nxuskit_chat_stream(self, handle, request, chunk, done, userdata):
            self.requests.append(json.loads(request))
            chunk(self.string({"delta": "one"}), userdata)
            chunk(self.string({"delta": "two"}), userdata)
            done(
                self.string({"error": {"error_type": "provider", "message": "broken"}})
                if self.error
                else ffi.NULL,
                userdata,
            )
            return ffi.cast("void *", 3)

        def nxuskit_free_stream(self, stream):
            self.frees.append("stream")

        def nxuskit_list_models(self, handle):
            return self.string([{"id": "chosen", "name": "Chosen"}])

        def nxuskit_free_string(self, value):
            self.frees.append("string")

        def nxuskit_free_provider(self, handle):
            self.frees.append("provider")

    library = Library()
    monkeypatch.setitem(
        sys.modules,
        "nxuskit._ffi",
        types.SimpleNamespace(ffi=ffi, lib=library, last_error=lambda: library.error),
    )
    path = Path(__file__).parents[2] / "src/nxuskit/_ffi_provider.py"
    spec = importlib.util.spec_from_file_location("nxuskit._ffi_provider", path)
    module = importlib.util.module_from_spec(spec)
    monkeypatch.setitem(sys.modules, spec.name, module)
    spec.loader.exec_module(module)
    return library, module


def test_factory_ffi_executes_native_shape_and_preserves_options(abi):
    library, _ = abi
    provider = Provider.openai(model="default", api_key="test", use_ffi=True)
    try:
        response = provider.chat(
            [Message.user("hello")],
            model="chosen",
            temperature=0.0,
            max_tokens=7,
            top_p=0.5,
            stop="END",
            response_format=ResponseFormat.JSON,
            tools=[ToolDefinition.create("lookup")],
            tool_choice="required",
        )
        assert response.content == "reply"
        assert library.requests == [
            {
                "model": "chosen",
                "messages": [{"role": "user", "content": "hello"}],
                "temperature": 0.0,
                "max_tokens": 7,
                "top_p": 0.5,
                "stop": ["END"],
                "response_format": {"type": "json"},
                "tools": [{"type": "function", "function": {"name": "lookup"}}],
                "tool_choice": "required",
            }
        ]
        assert provider.model == "default"
        assert provider.provider_name == "openai"
        assert provider.list_models()[0].id == "chosen"
    finally:
        provider.close()
    assert library.frees == ["response", "string", "provider"]


@pytest.mark.parametrize("name", ["openai", "perplexity"])
@pytest.mark.parametrize(
    "tool",
    [
        ToolDefinition.create("", parameters={}),
        ToolDefinition.create("emit_x", parameters=[]),
        ToolDefinition(
            type="not_function",
            function=ToolDefinition.create("emit_x", parameters={"type": "object"}).function,
        ),
    ],
)
def test_ffi_invalid_tool_definition_fails_before_abi(abi, name, tool):
    from nxuskit.errors import ProviderError

    library, _ = abi
    with getattr(Provider, name)(
        model="default", api_key="test", use_ffi=True
    ) as provider:
        with pytest.raises(ProviderError) as failure:
            provider.chat([Message.user("hello")], tools=[tool])
    assert getattr(failure.value, "code", None) == "invalid_request"
    assert library.requests == []


def test_ffi_schema_and_sampling_reach_json_boundary(abi):
    library, _ = abi
    with Provider.openai(model="chosen", api_key="test", use_ffi=True) as provider:
        options = dict(
            response_format={
                "type": "json_schema",
                "schema": {
                    "type": "object",
                    "properties": {"x": {"type": "integer"}},
                    "required": ["x"],
                },
            },
            presence_penalty=-0.25,
            frequency_penalty=0.5,
            seed=7,
            logprobs=True,
            top_logprobs=3,
            reasoning_effort="high",
        )
        provider.chat([Message.user("hi")], **options)
        list(provider.chat_stream([Message.user("hi")], **options))
    expected = {
        "model": "chosen",
        "messages": [{"role": "user", "content": "hi"}],
        "response_format": {
            "type": "json_schema",
            "schema": {
                "type": "object",
                "properties": {"x": {"type": "integer"}},
                "required": ["x"],
            },
        },
        "presence_penalty": -0.25,
        "frequency_penalty": 0.5,
        "seed": 7,
        "logprobs": True,
        "top_logprobs": 3,
        "reasoning_effort": "high",
    }
    assert library.requests == [expected, expected]


@pytest.mark.parametrize(
    "name",
    [
        "claude",
        "openai",
        "groq",
        "xai",
        "mistral",
        "fireworks",
        "together",
        "openrouter",
        "perplexity",
        "lmstudio",
    ],
)
@pytest.mark.parametrize("streaming", [False, True])
def test_ffi_non_ollama_thinking_rejects_before_abi(abi, name, streaming):
    from nxuskit.errors import ProviderError

    library, _ = abi
    with getattr(Provider, name)(model="selected", use_ffi=True) as provider:
        with pytest.raises(ProviderError, match="unsupported request control: thinking") as caught:
            if streaming:
                list(provider.chat_stream([Message.user("hello")], thinking="enabled"))
            else:
                provider.chat([Message.user("hello")], thinking="enabled")
    assert getattr(caught.value, "code", None) == "unsupported_capability"
    assert library.requests == []


def test_ffi_claude_stop_provider_limit_and_wire(abi):
    from nxuskit.errors import ProviderError

    library, _ = abi
    stops = ["a", "b", "c", "d"]
    with Provider.claude(model="selected", use_ffi=True) as provider:
        assert provider.chat([Message.user("hello")], stop=stops).content == "reply"
        list(provider.chat_stream([Message.user("hello")], stop=stops))
        with pytest.raises(ProviderError, match="invalid request control: stop") as caught:
            provider.chat([Message.user("hello")], stop=[*stops, "e"])

    assert getattr(caught.value, "code", None) == "invalid_request"
    expected = {
        "model": "selected",
        "messages": [{"role": "user", "content": "hello"}],
        "stop": stops,
    }
    assert library.requests == [expected, expected]


def test_ffi_claude_schema_and_tool_streaming_reject_before_abi(abi):
    from nxuskit.errors import ProviderError

    library, _ = abi
    tool = ToolDefinition.create(
        "lookup", parameters={"type": "object", "properties": {}}
    )
    schema = {"type": "json_schema", "schema": {"type": "object"}}

    with Provider.claude(model="selected", use_ffi=True) as provider:
        with pytest.raises(ProviderError, match="unsupported Anthropic") as schema_error:
            provider.chat([Message.user("hello")], response_format=schema)
        assert getattr(schema_error.value, "code", None) == "unsupported_capability"

        for kwargs in ({"tools": [tool]}, {"tool_choice": "required"}):
            with pytest.raises(ProviderError, match="unsupported Anthropic") as stream_error:
                list(provider.chat_stream([Message.user("hello")], **kwargs))
            assert getattr(stream_error.value, "code", None) == "unsupported_capability"

        assert provider.chat([Message.user("hello")], tools=[tool]).content == "reply"
        assert (
            provider.chat(
                [Message.user("hello")], response_format=ResponseFormat.JSON
            ).content
            == "reply"
        )

    assert library.requests == [
        {
            "model": "selected",
            "messages": [{"role": "user", "content": "hello"}],
            "tools": [tool.to_dict()],
        },
        {
            "model": "selected",
            "messages": [{"role": "user", "content": "hello"}],
            "response_format": {"type": "json"},
        },
    ]


def test_ffi_ollama_thinking_reaches_json_boundary(abi):
    library, _ = abi
    with Provider.ollama(model="selected", use_ffi=True) as provider:
        options = {"thinking": "disabled", "seed": -1}
        assert provider.chat([Message.user("hello")], **options).content == "reply"
        list(provider.chat_stream([Message.user("hello")], **options))
    expected = {
        "model": "selected",
        "messages": [{"role": "user", "content": "hello"}],
        "thinking_mode": "disabled",
        "seed": -1,
    }
    assert library.requests == [expected, expected]


def test_ffi_ollama_declared_profile_is_python_only_provenance(abi):
    from nxuskit.errors import ProviderError

    library, _ = abi
    with Provider.ollama(
        model="selected",
        use_ffi=True,
        profile="ollama_cloud_via_daemon",
    ) as provider:
        assert provider.declared_profile == "ollama_cloud_via_daemon"
        response = provider.chat([Message.user("hello")], seed=42)
        chunks = list(provider.chat_stream([Message.user("hello")], thinking="disabled"))
        assert response.declared_profile == "ollama_cloud_via_daemon"
        assert chunks and all(
            chunk.declared_profile == "ollama_cloud_via_daemon" for chunk in chunks
        )

        baseline_count = len(library.requests)
        for response_format in (
            ResponseFormat.JSON,
            {"type": "json_schema", "schema": {"type": "object"}},
        ):
            with pytest.raises(ProviderError) as caught:
                provider.chat([Message.user("hello")], response_format=response_format)
            assert getattr(caught.value, "code", None) == "unsupported_capability"
        assert len(library.requests) == baseline_count

    assert library.configs == [
        {"provider_type": "ollama", "model": "selected", "timeout_ms": 30000}
    ]
    assert all("profile" not in request and "declared_profile" not in request for request in library.requests)


@pytest.mark.parametrize(
    "name,control,value",
    [
        ("claude", "presence_penalty", 0.4),
        ("claude", "frequency_penalty", -0.3),
        ("claude", "seed", 42),
        ("ollama", "presence_penalty", 0.4),
        ("ollama", "frequency_penalty", -0.3),
    ],
)
@pytest.mark.parametrize("streaming", [False, True])
def test_ffi_claude_ollama_native_unsupported_controls_reject_before_abi(
    abi, name, control, value, streaming
):
    from nxuskit.errors import ProviderError

    library, _ = abi
    kwargs = {"api_key": "fixture"} if name == "claude" else {}
    with getattr(Provider, name)(model="selected", use_ffi=True, **kwargs) as provider:
        with pytest.raises(ProviderError, match=f"unsupported request control: {control}") as caught:
            if streaming:
                list(provider.chat_stream([Message.user("hello")], **{control: value}))
            else:
                provider.chat([Message.user("hello")], **{control: value})
    assert getattr(caught.value, "code", None) == "unsupported_capability"
    assert library.requests == []


@pytest.mark.parametrize(
    "name,control,value,expected_code",
    [
        ("perplexity", "seed", 42, "unsupported_capability"),
        ("xai", "presence_penalty", 0.4, "unsupported_capability"),
        ("xai", "frequency_penalty", -0.3, "unsupported_capability"),
        ("xai", "seed", 42, "unsupported_capability"),
        ("perplexity", "presence_penalty", -0.1, "invalid_request"),
        ("perplexity", "frequency_penalty", -0.1, "invalid_request"),
    ],
)
@pytest.mark.parametrize("streaming", [False, True])
def test_ffi_perplexity_xai_generation_controls_fail_before_abi(
    abi, name, control, value, expected_code, streaming
):
    from nxuskit.errors import ProviderError

    library, _ = abi
    with getattr(Provider, name)(
        model="selected", api_key="fixture", use_ffi=True
    ) as provider:
        with pytest.raises(ProviderError) as caught:
            if streaming:
                list(provider.chat_stream([Message.user("hello")], **{control: value}))
            else:
                provider.chat([Message.user("hello")], **{control: value})
    assert getattr(caught.value, "code", None) == expected_code
    assert control in str(caught.value)
    assert library.requests == []

@pytest.mark.parametrize("streaming", [False, True])
@pytest.mark.parametrize("case", ["stop", "json", "schema", "tools", "tool_choice"])
def test_ffi_perplexity_unsupported_request_surface_fails_before_abi(
    abi, case, streaming
):
    from nxuskit.errors import ProviderError

    library, _ = abi
    kwargs = {
        "stop": {"stop": ["END"]},
        "json": {"response_format": ResponseFormat.JSON},
        "schema": {
            "response_format": {"type": "json_schema", "schema": {"type": "object"}}
        },
        "tools": {
            "tools": [
                ToolDefinition.create(
                    "emit_x", parameters={"type": "object", "properties": {}}
                )
            ]
        },
        "tool_choice": {"tool_choice": "required"},
    }[case]
    with Provider.perplexity(model="selected", api_key="fixture", use_ffi=True) as provider:
        with pytest.raises(ProviderError, match="unsupported request control") as caught:
            if streaming:
                list(provider.chat_stream([Message.user("hello")], **kwargs))
            else:
                provider.chat([Message.user("hello")], **kwargs)
    assert getattr(caught.value, "code", None) == "unsupported_capability"
    assert library.requests == []


@pytest.mark.parametrize("streaming", [False, True])
@pytest.mark.parametrize("source_type", ["url", "base64", "filepath"])
def test_ffi_perplexity_images_fail_before_abi(abi, source_type, streaming):
    from nxuskit.errors import ProviderError

    library, _ = abi
    message = Message.user("hello")
    if source_type == "url":
        message.with_image_url("https://image.invalid/p.png")
    elif source_type == "base64":
        message.with_image_base64("aGVsbG8=")
    else:
        message.with_image_file("/definitely/not/read/perplexity.png")
    with Provider.perplexity(model="selected", api_key="fixture", use_ffi=True) as provider:
        with pytest.raises(ProviderError, match="unsupported request control: images") as caught:
            if streaming:
                list(provider.chat_stream([message]))
            else:
                provider.chat([message])
    assert getattr(caught.value, "code", None) == "unsupported_capability"
    assert library.requests == []


def test_ffi_perplexity_response_format_validation_and_text_omission(abi):
    from nxuskit.errors import ProviderError

    library, _ = abi
    with Provider.perplexity(model="selected", api_key="fixture", use_ffi=True) as provider:
        with pytest.raises(ProviderError) as caught:
            provider.chat(
                [Message.user("hello")],
                response_format={"type": "json_schema", "schema": []},
            )
        assert getattr(caught.value, "code", None) == "invalid_request"

        assert provider.chat(
            [Message.user("hello")], response_format=ResponseFormat.TEXT
        ).content == "reply"
        list(
            provider.chat_stream(
                [Message.user("hello")], response_format=ResponseFormat.TEXT
            )
        )

    expected = {
        "model": "selected",
        "messages": [{"role": "user", "content": "hello"}],
    }
    assert library.requests == [expected, expected]


def test_factory_stream_early_close_releases_stream_not_provider(abi):
    library, _ = abi
    with Provider.openai(model="default", use_ffi=True) as provider:
        stream = provider.chat_stream([Message.user("hello")], stop=["END"])
        assert next(stream).delta == "one"
        stream.close()
        assert library.frees == ["stream"]
        assert provider.chat([Message.user("again")]).content == "reply"
    assert library.requests[0] == {
        "model": "default",
        "messages": [{"role": "user", "content": "hello"}],
        "stop": ["END"],
    }
    assert library.frees == ["stream", "response", "provider"]


def test_ffi_rate_limit_error_preserves_canonical_retry_metadata(abi):
    _, module = abi
    error = module._parse_nxuskit_error(
        {
            "error_type": "rate_limited",
            "message": "limited",
            "retry_after_ms": 2000,
        }
    )
    assert error.error_type == "rate_limited"
    assert error.retry_after_ms == 2000
    assert error.retry_after == 2
    assert error.is_retryable


def test_factory_errors_and_closed_handle_are_not_swallowed(abi):
    library, module = abi
    provider = Provider.openai(model="default", use_ffi=True)
    library.error = "unavailable"
    with pytest.raises(module.ProviderError, match="unavailable"):
        provider.chat([Message.user("hello")])
    stream = provider.chat_stream([Message.user("hello")])
    assert [next(stream).delta, next(stream).delta] == ["one", "two"]
    with pytest.raises(module.ProviderError, match="broken"):
        next(stream)
    provider.close()
    provider.close()
    with pytest.raises(module.ConfigError, match="closed"):
        provider.chat([Message.user("hello")])
    assert library.frees == ["stream", "provider"]


def test_direct_ffi_request_dictionary_api_is_unchanged(abi):
    library, module = abi
    with module.create_ffi_provider({"provider_type": "mock"}) as provider:
        assert provider.chat({"model": "raw", "messages": []}).content == "reply"
        assert [c.delta for c in provider.stream({"model": "raw", "messages": []})] == [
            "one",
            "two",
        ]
    assert library.requests == [{"model": "raw", "messages": []}] * 2


def test_native_factory_call_shape_remains_executable(monkeypatch):
    import requests

    def request(method, url, **kwargs):
        assert method == "POST"
        assert url == "https://provider.invalid/v1/chat/completions"
        assert kwargs["json"]["messages"] == [{"role": "user", "content": "hello"}]
        response = requests.Response()
        response.status_code = 200
        response._content = json.dumps(
            {
                "model": "default",
                "choices": [{"message": {"content": "native"}, "finish_reason": "stop"}],
            }
        ).encode()
        return response

    monkeypatch.setattr(requests, "request", request)
    provider = Provider.openai(
        model="default", api_key="test", use_ffi=False, api_url="https://provider.invalid/v1"
    )
    assert provider.chat([Message.user("hello")]).content == "native"
    assert provider.model == "default"
    assert provider.provider_name == "openai"


def test_factory_images_keep_url_base64_and_local_file_bytes(abi, tmp_path):
    from nxuskit.types import ImageSource, ImageSourceType

    library, _ = abi
    image = tmp_path / "sample.png"
    image.write_bytes(b"hello")
    message = Message.user("describe").with_image_url("https://image.invalid/p.png")
    message.images.append(ImageSource(ImageSourceType.BASE64, "aGVsbG8=", "image/png"))
    message.with_image_file(str(image))
    with Provider.openai(model="image-model", use_ffi=True) as provider:
        assert provider.chat([message]).content == "reply"
    assert library.requests == [
        {
            "model": "image-model",
            "messages": [
                {
                    "role": "user",
                    "content": [
                        {"type": "text", "text": "describe"},
                        {
                            "type": "image",
                            "source": {"type": "url", "url": "https://image.invalid/p.png"},
                        },
                        {
                            "type": "image",
                            "source": {
                                "type": "base64",
                                "media_type": "image/png",
                                "data": "aGVsbG8=",
                            },
                        },
                        {
                            "type": "image",
                            "source": {
                                "type": "base64",
                                "media_type": "image/png",
                                "data": "aGVsbG8=",
                            },
                        },
                    ],
                }
            ],
        }
    ]
    assert message.content == "describe"
    assert len(message.images) == 3


def test_missing_image_file_fails_before_abi_chat(abi, tmp_path):
    library, _ = abi
    with Provider.openai(model="default", use_ffi=True) as provider:
        with pytest.raises(FileNotFoundError):
            provider.chat([Message.user("x").with_image_file(str(tmp_path / "absent.png"))])
    assert library.requests == []


def test_native_stream_and_discovery_keep_call_shape(monkeypatch):
    from io import BytesIO

    import requests

    def request(method, url, **kwargs):
        response = requests.Response()
        response.status_code = 200
        response.raw = BytesIO()
        if method == "POST":
            assert url == "https://provider.invalid/v1/chat/completions"
            assert kwargs["json"]["stream"] is True
            assert kwargs["json"]["messages"] == [{"role": "user", "content": "hello"}]
            response.iter_lines = lambda: iter(
                [b'data: {"choices":[{"delta":{"content":"native"}}]}', b"data: [DONE]"]
            )
        else:
            assert method == "GET"
            assert url == "https://provider.invalid/v1/models"
            response._content = b'{"data":[{"id":"default"}]}'
        return response

    monkeypatch.setattr(requests, "request", request)
    provider = Provider.openai(
        model="default", api_key="test", use_ffi=False, api_url="https://provider.invalid/v1"
    )
    chunks = list(provider.chat_stream([Message.user("hello")]))
    assert [chunk.delta for chunk in chunks] == ["native", ""]
    assert [chunk.is_final() for chunk in chunks] == [False, True]
    assert provider.list_models()[0].id == "default"


def test_factory_return_annotation_exposes_the_public_protocol():
    from typing import get_type_hints

    from nxuskit.provider import LLMProvider

    assert get_type_hints(Provider.openai)["return"] is LLMProvider


@pytest.mark.parametrize(
    "name",
    [
        "claude",
        "openai",
        "ollama",
        "groq",
        "xai",
        "mistral",
        "fireworks",
        "together",
        "openrouter",
        "perplexity",
        "lmstudio",
    ],
)
def test_every_ffi_factory_keeps_model_and_provider_identity(abi, name):
    library, _ = abi
    with getattr(Provider, name)(model="selected", use_ffi=True) as provider:
        assert provider.model == "selected"
        assert provider.provider_name == name
        assert provider.chat([Message.user("hello")]).content == "reply"
    assert library.configs[0]["provider_type"] == name
    assert library.requests == [
        {"model": "selected", "messages": [{"role": "user", "content": "hello"}]}
    ]


def test_factory_stream_preserves_images_and_text_format(abi):
    library, _ = abi
    with Provider.openai(model="selected", use_ffi=True) as provider:
        chunks = list(
            provider.chat_stream(
                [Message.user("describe").with_image_url("https://image.invalid/p.png")],
                response_format=ResponseFormat.TEXT,
            )
        )
        assert [chunk.delta for chunk in chunks] == ["one", "two"]
    assert library.requests == [
        {
            "model": "selected",
            "response_format": {"type": "text"},
            "messages": [
                {
                    "role": "user",
                    "content": [
                        {"type": "text", "text": "describe"},
                        {
                            "type": "image",
                            "source": {"type": "url", "url": "https://image.invalid/p.png"},
                        },
                    ],
                }
            ],
        }
    ]
    assert library.frees == ["stream", "provider"]


@pytest.mark.parametrize("streaming", [False, True])
@pytest.mark.parametrize(
    "controls,expected_code",
    [
        ({"stop": ["a", "b", "c", "d", "e"]}, "invalid_request"),
        ({"logprobs": True}, "unsupported_capability"),
        ({"top_logprobs": 3}, "unsupported_capability"),
        ({"reasoning_effort": "low"}, "unsupported_capability"),
        ({"thinking": "enabled"}, "unsupported_capability"),
    ],
)
def test_ffi_fireworks_bounds_and_advanced_controls_fail_before_abi(
    abi, streaming, controls, expected_code
):
    from nxuskit.errors import ProviderError

    library, _ = abi
    with Provider.fireworks(
        model="selected", api_key="fixture", use_ffi=True
    ) as provider:
        with pytest.raises(ProviderError) as caught:
            if streaming:
                list(provider.chat_stream([Message.user("hello")], **controls))
            else:
                provider.chat([Message.user("hello")], **controls)

    assert getattr(caught.value, "code", None) == expected_code
    assert library.requests == []


def test_ffi_fireworks_schema_and_unary_tools_reach_abi_and_stream_tools_fail_before_abi(abi):
    from nxuskit.errors import ProviderError

    library, _ = abi
    schema = {
        "type": "object",
        "additionalProperties": False,
        "required": ["sku"],
        "properties": {"sku": {"type": "string"}},
    }
    tool = ToolDefinition.create(
        "lookup",
        description="Look up inventory.",
        parameters={
            "type": "object",
            "properties": {"sku": {"type": "string"}},
            "required": ["sku"],
        },
    )
    choice = {"type": "function", "function": {"name": "lookup"}}

    with Provider.fireworks(
        model="selected",
        api_key="fixture",
        use_ffi=True,
    ) as provider:
        assert provider.chat(
            [Message.user("Look up A-1.")],
            response_format={"type": "json_schema", "schema": schema},
            tools=[tool],
            tool_choice=choice,
        ).content == "reply"

        assert library.requests == [
            {
                "model": "selected",
                "messages": [{"role": "user", "content": "Look up A-1."}],
                "tool_choice": choice,
                "response_format": {"type": "json_schema", "schema": schema},
                "tools": [tool.to_dict()],
            }
        ]

        for kwargs in ({"tools": [tool]}, {"tool_choice": choice}):
            with pytest.raises(ProviderError, match="unsupported Fireworks tool streaming") as caught:
                list(provider.chat_stream([Message.user("stream")], **kwargs))
            assert getattr(caught.value, "code", None) == "unsupported_capability"

        assert len(library.requests) == 1


@pytest.mark.parametrize("streaming", [False, True])
@pytest.mark.parametrize(
    "controls,expected_code",
    [
        ({"stop": ["a", "b", "c", "d", "e"]}, "invalid_request"),
        ({"logprobs": True}, "unsupported_capability"),
    ],
)
def test_ffi_lmstudio_bounds_and_advanced_controls_fail_before_abi(
    abi, streaming, controls, expected_code
):
    from nxuskit.errors import ProviderError

    library, _ = abi
    with Provider.lmstudio(model="selected", use_ffi=True) as provider:
        with pytest.raises(ProviderError) as caught:
            if streaming:
                list(provider.chat_stream([Message.user("hello")], **controls))
            else:
                provider.chat([Message.user("hello")], **controls)

    assert getattr(caught.value, "code", None) == expected_code
    assert library.requests == []


def test_ffi_lmstudio_schema_and_unary_tools_reach_abi_and_stream_tools_fail_before_abi(abi):
    from nxuskit.errors import ProviderError

    library, _ = abi
    schema = {
        "type": "object",
        "additionalProperties": False,
        "required": ["sku"],
        "properties": {"sku": {"type": "string"}},
    }
    tool = ToolDefinition.create(
        "lookup",
        description="Look up inventory.",
        parameters={
            "type": "object",
            "properties": {"sku": {"type": "string"}},
            "required": ["sku"],
        },
    )
    choice = {"type": "function", "function": {"name": "lookup"}}

    with Provider.lmstudio(model="selected", use_ffi=True) as provider:
        assert provider.chat(
            [Message.user("Look up A-1.")],
            response_format={"type": "json_schema", "schema": schema},
            tools=[tool],
            tool_choice=choice,
        ).content == "reply"

        assert library.requests == [
            {
                "model": "selected",
                "messages": [{"role": "user", "content": "Look up A-1."}],
                "tool_choice": choice,
                "response_format": {"type": "json_schema", "schema": schema},
                "tools": [tool.to_dict()],
            }
        ]

        for kwargs in ({"tools": [tool]}, {"tool_choice": choice}):
            with pytest.raises(
                ProviderError, match="unsupported LM Studio tool streaming"
            ) as caught:
                list(provider.chat_stream([Message.user("stream")], **kwargs))
            assert getattr(caught.value, "code", None) == "unsupported_capability"

        assert len(library.requests) == 1


@pytest.mark.parametrize("streaming", [False, True])
@pytest.mark.parametrize(
    "controls,expected_code",
    [
        ({"stop": ["a", "b", "c", "d", "e"]}, "invalid_request"),
        ({"logprobs": True}, "unsupported_capability"),
        ({"top_logprobs": 3}, "unsupported_capability"),
        ({"reasoning_effort": "low"}, "unsupported_capability"),
        ({"thinking": "enabled"}, "unsupported_capability"),
    ],
)
def test_ffi_groq_bounds_and_advanced_controls_fail_before_abi(
    abi, streaming, controls, expected_code
):
    from nxuskit.errors import ProviderError

    library, _ = abi
    with Provider.groq(model="selected", api_key="fixture", use_ffi=True) as provider:
        with pytest.raises(ProviderError) as caught:
            if streaming:
                list(provider.chat_stream([Message.user("hello")], **controls))
            else:
                provider.chat([Message.user("hello")], **controls)

    assert getattr(caught.value, "code", None) == expected_code
    assert library.requests == []


def test_ffi_groq_images_and_tool_streaming_fail_before_abi(abi):
    from nxuskit.errors import ProviderError

    library, _ = abi
    with Provider.groq(model="selected", api_key="fixture", use_ffi=True) as provider:
        image = Message.user("describe").with_image_url("https://example.invalid/image.png")
        for streaming in (False, True):
            with pytest.raises(
                ProviderError, match="unsupported request control: images"
            ) as caught:
                if streaming:
                    list(provider.chat_stream([image]))
                else:
                    provider.chat([image])
            assert getattr(caught.value, "code", None) == "unsupported_capability"

        tool = ToolDefinition.create("lookup", parameters={"type": "object"})
        choice = {"type": "function", "function": {"name": "lookup"}}
        for kwargs in ({"tools": [tool]}, {"tool_choice": choice}):
            with pytest.raises(
                ProviderError, match="unsupported Groq tool streaming"
            ) as caught:
                list(provider.chat_stream([Message.user("stream")], **kwargs))
            assert getattr(caught.value, "code", None) == "unsupported_capability"

    assert library.requests == []


def test_ffi_groq_schema_and_unary_tools_reach_abi_and_stream_tools_fail_before_abi(abi):
    from nxuskit.errors import ProviderError

    library, _ = abi
    schema = {
        "type": "object",
        "additionalProperties": False,
        "required": ["sku"],
        "properties": {"sku": {"type": "string"}},
    }
    tool = ToolDefinition.create(
        "lookup",
        description="Look up inventory.",
        parameters={
            "type": "object",
            "properties": {"sku": {"type": "string"}},
            "required": ["sku"],
        },
    )
    choice = {"type": "function", "function": {"name": "lookup"}}

    with Provider.groq(
        model="selected",
        api_key="fixture",
        use_ffi=True,
    ) as provider:
        assert provider.chat(
            [Message.user("Look up A-1.")],
            response_format={"type": "json_schema", "schema": schema},
            tools=[tool],
            tool_choice=choice,
        ).content == "reply"

        assert library.requests == [
            {
                "model": "selected",
                "messages": [{"role": "user", "content": "Look up A-1."}],
                "tool_choice": choice,
                "response_format": {"type": "json_schema", "schema": schema},
                "tools": [tool.to_dict()],
            }
        ]

        for kwargs in ({"tools": [tool]}, {"tool_choice": choice}):
            with pytest.raises(ProviderError, match="unsupported Groq tool streaming") as caught:
                list(provider.chat_stream([Message.user("stream")], **kwargs))
            assert getattr(caught.value, "code", None) == "unsupported_capability"

        assert len(library.requests) == 1


@pytest.mark.parametrize("streaming", [False, True])
@pytest.mark.parametrize(
    "controls,expected_code",
    [
        ({"stop": ["a", "b", "c", "d", "e"]}, "invalid_request"),
        ({"logprobs": True}, "unsupported_capability"),
        ({"top_logprobs": 3}, "unsupported_capability"),
        ({"reasoning_effort": "low"}, "unsupported_capability"),
        ({"thinking": "enabled"}, "unsupported_capability"),
    ],
)
def test_ffi_mistral_bounds_and_advanced_controls_fail_before_abi(
    abi, streaming, controls, expected_code
):
    from nxuskit.errors import ProviderError

    library, _ = abi
    with Provider.mistral(model="selected", api_key="fixture", use_ffi=True) as provider:
        with pytest.raises(ProviderError) as caught:
            if streaming:
                list(provider.chat_stream([Message.user("hello")], **controls))
            else:
                provider.chat([Message.user("hello")], **controls)

    assert getattr(caught.value, "code", None) == expected_code
    assert library.requests == []


def test_ffi_mistral_tool_streaming_fails_before_abi(abi):
    from nxuskit.errors import ProviderError

    library, _ = abi
    with Provider.mistral(model="selected", api_key="fixture", use_ffi=True) as provider:
        tool = ToolDefinition.create("lookup", parameters={"type": "object"})
        choice = {"type": "function", "function": {"name": "lookup"}}
        for kwargs in ({"tools": [tool]}, {"tool_choice": choice}):
            with pytest.raises(
                ProviderError, match="unsupported Mistral tool streaming"
            ) as caught:
                list(provider.chat_stream([Message.user("stream")], **kwargs))
            assert getattr(caught.value, "code", None) == "unsupported_capability"

    assert library.requests == []


def test_ffi_mistral_schema_and_unary_tools_reach_abi_and_stream_tools_fail_before_abi(abi):
    from nxuskit.errors import ProviderError

    library, _ = abi
    schema = {
        "type": "object",
        "additionalProperties": False,
        "required": ["sku"],
        "properties": {"sku": {"type": "string"}},
    }
    tool = ToolDefinition.create(
        "lookup",
        description="Look up inventory.",
        parameters={
            "type": "object",
            "properties": {"sku": {"type": "string"}},
            "required": ["sku"],
        },
    )

    with Provider.mistral(
        model="selected",
        api_key="fixture",
        use_ffi=True,
    ) as provider:
        assert provider.chat(
            [Message.user("Look up A-1.")],
            response_format={"type": "json_schema", "schema": schema},
            tools=[tool],
            tool_choice="auto",
        ).content == "reply"

        assert library.requests == [
            {
                "model": "selected",
                "messages": [{"role": "user", "content": "Look up A-1."}],
                "tool_choice": "auto",
                "response_format": {"type": "json_schema", "schema": schema},
                "tools": [tool.to_dict()],
            }
        ]

        for kwargs in ({"tools": [tool]}, {"tool_choice": "auto"}):
            with pytest.raises(
                ProviderError, match="unsupported Mistral tool streaming"
            ) as caught:
                list(provider.chat_stream([Message.user("stream")], **kwargs))
            assert getattr(caught.value, "code", None) == "unsupported_capability"

        assert len(library.requests) == 1


@pytest.mark.parametrize("streaming", [False, True])
@pytest.mark.parametrize(
    "controls,expected_code",
    [
        ({"stop": ["a", "b", "c", "d", "e"]}, "invalid_request"),
        ({"logprobs": True}, "unsupported_capability"),
        ({"top_logprobs": 3}, "unsupported_capability"),
        ({"reasoning_effort": "low"}, "unsupported_capability"),
        ({"thinking": "enabled"}, "unsupported_capability"),
    ],
)
def test_ffi_together_bounds_and_advanced_controls_fail_before_abi(
    abi, streaming, controls, expected_code
):
    from nxuskit.errors import ProviderError

    library, _ = abi
    with Provider.together(model="selected", api_key="fixture", use_ffi=True) as provider:
        with pytest.raises(ProviderError) as caught:
            if streaming:
                list(provider.chat_stream([Message.user("hello")], **controls))
            else:
                provider.chat([Message.user("hello")], **controls)

    assert getattr(caught.value, "code", None) == expected_code
    assert library.requests == []


def test_ffi_together_tool_streaming_fails_before_abi(abi):
    from nxuskit.errors import ProviderError

    library, _ = abi
    with Provider.together(model="selected", api_key="fixture", use_ffi=True) as provider:
        tool = ToolDefinition.create("lookup", parameters={"type": "object"})
        for kwargs in ({"tools": [tool]}, {"tool_choice": "required"}):
            with pytest.raises(
                ProviderError, match="unsupported Together tool streaming"
            ) as caught:
                list(provider.chat_stream([Message.user("stream")], **kwargs))
            assert getattr(caught.value, "code", None) == "unsupported_capability"

    assert library.requests == []


def test_ffi_together_schema_and_unary_tools_reach_abi_and_stream_tools_fail_before_abi(abi):
    from nxuskit.errors import ProviderError

    library, _ = abi
    schema = {
        "type": "object",
        "additionalProperties": False,
        "required": ["sku"],
        "properties": {"sku": {"type": "string"}},
    }
    tool = ToolDefinition.create(
        "lookup",
        description="Look up inventory.",
        parameters={
            "type": "object",
            "properties": {"sku": {"type": "string"}},
            "required": ["sku"],
        },
    )

    with Provider.together(
        model="selected",
        api_key="fixture",
        use_ffi=True,
    ) as provider:
        assert provider.chat(
            [Message.user("Look up A-1.")],
            response_format={"type": "json_schema", "schema": schema},
            tools=[tool],
            tool_choice="required",
        ).content == "reply"

        assert library.requests == [
            {
                "model": "selected",
                "messages": [{"role": "user", "content": "Look up A-1."}],
                "tool_choice": "required",
                "response_format": {"type": "json_schema", "schema": schema},
                "tools": [tool.to_dict()],
            }
        ]

        for kwargs in ({"tools": [tool]}, {"tool_choice": "required"}):
            with pytest.raises(
                ProviderError, match="unsupported Together tool streaming"
            ) as caught:
                list(provider.chat_stream([Message.user("stream")], **kwargs))
            assert getattr(caught.value, "code", None) == "unsupported_capability"

        assert len(library.requests) == 1


@pytest.mark.parametrize(
    "tool_choice,expected_code",
    [
        ({"type": "function", "function": {"name": "lookup"}}, "unsupported_capability"),
        ({"type": "function", "function": {"name": ""}}, "invalid_request"),
        ("bogus", "invalid_request"),
    ],
)
def test_ffi_together_named_or_invalid_tool_choice_fails_before_abi(
    abi, tool_choice, expected_code
):
    from nxuskit.errors import ProviderError

    library, _ = abi
    with Provider.together(model="selected", api_key="fixture", use_ffi=True) as provider:
        with pytest.raises(ProviderError) as caught:
            provider.chat([Message.user("hello")], tool_choice=tool_choice)

    assert getattr(caught.value, "code", None) == expected_code
    assert library.requests == []


@pytest.mark.parametrize("streaming", [False, True])
@pytest.mark.parametrize(
    "controls,expected_code",
    [
        ({"stop": ["a", "b", "c", "d", "e"]}, "invalid_request"),
        ({"logprobs": True}, "unsupported_capability"),
        ({"top_logprobs": 3}, "unsupported_capability"),
        ({"reasoning_effort": "low"}, "unsupported_capability"),
        ({"thinking": "enabled"}, "unsupported_capability"),
    ],
)
def test_ffi_openrouter_bounds_and_advanced_controls_fail_before_abi(
    abi, streaming, controls, expected_code
):
    from nxuskit.errors import ProviderError

    library, _ = abi
    with Provider.openrouter(model="selected", api_key="fixture", use_ffi=True) as provider:
        with pytest.raises(ProviderError) as caught:
            if streaming:
                list(provider.chat_stream([Message.user("hello")], **controls))
            else:
                provider.chat([Message.user("hello")], **controls)

    assert getattr(caught.value, "code", None) == expected_code
    assert library.requests == []


def test_ffi_openrouter_tool_streaming_fails_before_abi(abi):
    from nxuskit.errors import ProviderError

    library, _ = abi
    with Provider.openrouter(model="selected", api_key="fixture", use_ffi=True) as provider:
        tool = ToolDefinition.create("lookup", parameters={"type": "object"})
        for kwargs in ({"tools": [tool]}, {"tool_choice": "auto"}):
            with pytest.raises(
                ProviderError, match="unsupported OpenRouter tool streaming"
            ) as caught:
                list(provider.chat_stream([Message.user("stream")], **kwargs))
            assert getattr(caught.value, "code", None) == "unsupported_capability"

    assert library.requests == []


def test_ffi_openrouter_schema_and_named_unary_tools_reach_abi_and_stream_tools_fail_before_abi(abi):
    from nxuskit.errors import ProviderError

    library, _ = abi
    schema = {
        "type": "object",
        "additionalProperties": False,
        "required": ["sku"],
        "properties": {"sku": {"type": "string"}},
    }
    tool = ToolDefinition.create(
        "lookup",
        description="Look up inventory.",
        parameters={
            "type": "object",
            "properties": {"sku": {"type": "string"}},
            "required": ["sku"],
        },
    )
    choice = {"type": "function", "function": {"name": "lookup"}}

    with Provider.openrouter(
        model="selected",
        api_key="fixture",
        use_ffi=True,
    ) as provider:
        assert provider.chat(
            [Message.user("Look up A-1.")],
            response_format={"type": "json_schema", "schema": schema},
            tools=[tool],
            tool_choice=choice,
        ).content == "reply"

        assert library.requests == [
            {
                "model": "selected",
                "messages": [{"role": "user", "content": "Look up A-1."}],
                "tool_choice": choice,
                "response_format": {"type": "json_schema", "schema": schema},
                "tools": [tool.to_dict()],
            }
        ]

        for kwargs in ({"tools": [tool]}, {"tool_choice": choice}):
            with pytest.raises(
                ProviderError, match="unsupported OpenRouter tool streaming"
            ) as caught:
                list(provider.chat_stream([Message.user("stream")], **kwargs))
            assert getattr(caught.value, "code", None) == "unsupported_capability"

        assert len(library.requests) == 1


def test_empty_tools_three_state_ffi_shared_builder_r47b():
    from nxuskit._ffi_sync_provider import SyncFFIProvider
    from nxuskit.errors import ProviderError

    tool = ToolDefinition.create("lookup", parameters={"type": "object"})
    for profile in ("claude", "groq"):
        class Dummy:
            provider_name = profile
            model = "m"
            declared_profile = None

        omitted = SyncFFIProvider._request(
            Dummy(), [Message.user("hi")], None, None, None, None, None, None, None, None,
            streaming=True,
        )
        empty = SyncFFIProvider._request(
            Dummy(), [Message.user("hi")], None, None, None, None, None, None, [], None,
            streaming=True,
        )
        assert "tools" not in omitted
        assert "tools" not in empty
        with pytest.raises(ProviderError) as caught:
            SyncFFIProvider._request(
                Dummy(), [Message.user("hi")], None, None, None, None, None, None, [tool], None,
                streaming=True,
            )
        assert getattr(caught.value, "code", None) == "unsupported_capability"
