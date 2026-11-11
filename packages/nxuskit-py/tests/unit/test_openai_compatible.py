"""Unit tests for OpenAI-compatible provider base class."""

import pytest

from nxuskit import ImageSource, ImageSourceType, Message, ToolDefinition
from nxuskit.providers.openai_compatible import OpenAICompatibleProvider


@pytest.mark.parametrize("ffi", [False, True])
@pytest.mark.parametrize(
    "field,value",
    [
        ("temperature", float("nan")),
        ("temperature", 2.01),
        ("top_p", -0.01),
        ("top_p", True),
        ("max_tokens", 0),
        ("max_tokens", 1048577),
        ("max_tokens", 1.5),
        ("presence_penalty", -2.01),
        ("frequency_penalty", float("inf")),
        ("seed", True),
        ("seed", 9223372036854775808),
        ("seed", -9223372036854775809),
        ("logprobs", 1),
        ("top_logprobs", -1),
        ("top_logprobs", 21),
        ("top_logprobs", True),
        ("reasoning_effort", "surprise"),
        ("reasoning_effort", 1),
        ("stop", [""]),
        ("stop", ["x"] * 17),
    ],
)
def test_request_controls_reject_invalid_before_transport(ffi, field, value, monkeypatch):
    from nxuskit import Provider
    from nxuskit._ffi_sync_provider import SyncFFIProvider
    from nxuskit.errors import ProviderError

    calls = []
    if ffi:

        class LowLevel:
            def chat(self, request):
                calls.append(request)
                raise AssertionError("transport reached")

        provider = SyncFFIProvider(LowLevel(), "gpt-4o", "openai")
    else:
        provider = Provider.openai(model="gpt-4o", api_key="loopback", use_ffi=False)

        def transport(**kwargs):
            calls.append(kwargs)
            raise AssertionError("transport reached")

        monkeypatch.setattr(provider, "_make_request", transport)
    with pytest.raises(ProviderError, match="invalid request") as caught:
        provider.chat([Message.user("hi")], **{field: value})
    assert not caught.value.is_retryable
    assert calls == []


@pytest.mark.parametrize(
    "field,values",
    [
        ("temperature", [0, 2]),
        ("top_p", [0, 1]),
        ("max_tokens", [1, 1048576]),
        ("seed", [-(2**63), 2**63 - 1]),
        ("presence_penalty", [-2, 2]),
        ("frequency_penalty", [-2, 2]),
        ("logprobs", [False, True]),
        ("top_logprobs", [0, 20]),
        ("reasoning_effort", ["low", "medium", "high", "max"]),
        ("stop", [[], ["END"] * 16]),
    ],
)
def test_request_control_exact_boundaries(field, values):
    from nxuskit.types import _validate_request_controls

    for value in values:
        _validate_request_controls("openai", **{field: value})


@pytest.mark.parametrize("profile", ["openai", "claude"])
@pytest.mark.parametrize("stream", [False, True])
@pytest.mark.parametrize("controls", [{"top_k": 1}, {"min_p": 0.0}])
def test_unqualified_native_sampling_rejects_before_transport(
    profile, stream, controls, monkeypatch
):
    from nxuskit import Provider
    from nxuskit.errors import ProviderError

    provider = getattr(Provider, profile)(model="m", api_key="fixture", use_ffi=False)

    def forbidden(**kwargs):
        raise AssertionError("unsupported sampling reached transport")

    monkeypatch.setattr(provider, "_make_request", forbidden)
    with pytest.raises(ProviderError, match="unsupported top_k/min_p"):
        if stream:
            list(provider.chat_stream([Message.user("hi")], **controls))
        else:
            provider.chat([Message.user("hi")], **controls)


@pytest.mark.parametrize(
    "stream,controls",
    [
        (False, {"response_format": {"type": "json_schema", "schema": {"type": "object"}}}),
        (True, {"response_format": {"type": "json_schema", "schema": {"type": "object"}}}),
        (True, {"tools": [ToolDefinition.create("emit_x", parameters={})]}),
        (True, {"tool_choice": "auto"}),
    ],
)
def test_claude_native_sync_unqualified_controls_reject_before_transport(
    stream, controls, monkeypatch
):
    from nxuskit import Provider
    from nxuskit.errors import ProviderError

    provider = Provider.claude(model="m", api_key="fixture", use_ffi=False)

    def forbidden(**kwargs):
        raise AssertionError("unsupported Claude control reached transport")

    monkeypatch.setattr(provider, "_make_request", forbidden)
    with pytest.raises(ProviderError, match="unsupported") as caught:
        if stream:
            list(provider.chat_stream([Message.user("hi")], **controls))
        else:
            provider.chat([Message.user("hi")], **controls)
    assert getattr(caught.value, "code", None) == "unsupported_capability"


@pytest.mark.parametrize("stream", [False, True])
@pytest.mark.parametrize(
    "control,value",
    [
        ("thinking", "enabled"),
        ("presence_penalty", 0.4),
        ("frequency_penalty", -0.3),
        ("seed", 42),
        ("logprobs", True),
        ("top_logprobs", 5),
        ("reasoning_effort", "medium"),
    ],
)
def test_claude_native_sync_canonical_unsupported_rejects_before_transport(
    stream, control, value, monkeypatch
):
    from nxuskit import Provider
    from nxuskit.errors import ProviderError

    provider = Provider.claude(model="m", api_key="fixture", use_ffi=False)

    def forbidden(**kwargs):
        raise AssertionError("unsupported Claude control reached transport")

    monkeypatch.setattr(provider, "_make_request", forbidden)
    with pytest.raises(ProviderError, match="unsupported") as caught:
        if stream:
            list(provider.chat_stream([Message.user("hi")], **{control: value}))
        else:
            provider.chat([Message.user("hi")], **{control: value})
    assert getattr(caught.value, "code", None) == "unsupported_capability"


@pytest.mark.parametrize("profile", ["openai", "perplexity"])
@pytest.mark.parametrize("stream", [False, True])
def test_openai_compatible_native_sync_thinking_rejects_before_transport(
    profile, stream, monkeypatch
):
    from nxuskit import Provider
    from nxuskit.errors import ProviderError

    provider = getattr(Provider, profile)(model="m", api_key="fixture", use_ffi=False)

    def forbidden(**kwargs):
        raise AssertionError("unsupported thinking reached transport")

    monkeypatch.setattr(provider, "_make_request", forbidden)
    with pytest.raises(ProviderError, match="unsupported request control: thinking") as caught:
        if stream:
            list(provider.chat_stream([Message.user("hi")], thinking="enabled"))
        else:
            provider.chat([Message.user("hi")], thinking="enabled")
    assert getattr(caught.value, "code", None) == "unsupported_capability"


@pytest.mark.parametrize("stream", [False, True])
@pytest.mark.parametrize(
    "profile,control,value",
    [
        ("perplexity", "seed", 42),
        ("xai", "presence_penalty", 0.4),
        ("xai", "frequency_penalty", -0.3),
        ("xai", "seed", 42),
    ],
)
def test_native_declared_unsupported_generation_controls_reject_before_transport(
    profile, control, value, stream, monkeypatch
):
    from nxuskit import Provider
    from nxuskit.errors import ProviderError

    provider = getattr(Provider, profile)(
        model="m", api_key="fixture", use_ffi=False
    )

    def forbidden(**kwargs):
        raise AssertionError("unsupported generation control reached transport")

    monkeypatch.setattr(provider, "_make_request", forbidden)
    with pytest.raises(ProviderError, match=f"unsupported request control: {control}") as caught:
        if stream:
            list(provider.chat_stream([Message.user("hi")], **{control: value}))
        else:
            provider.chat([Message.user("hi")], **{control: value})
    assert getattr(caught.value, "code", None) == "unsupported_capability"


@pytest.mark.parametrize("stream", [False, True])
@pytest.mark.parametrize("control", ["presence_penalty", "frequency_penalty"])
def test_perplexity_native_penalty_range_rejects_negative_before_transport(
    control, stream, monkeypatch
):
    from nxuskit import Provider
    from nxuskit.errors import ProviderError

    provider = Provider.perplexity(model="m", api_key="fixture", use_ffi=False)

    def forbidden(**kwargs):
        raise AssertionError("invalid Perplexity penalty reached transport")

    monkeypatch.setattr(provider, "_make_request", forbidden)
    with pytest.raises(ProviderError, match=f"invalid request control: {control}") as caught:
        if stream:
            list(provider.chat_stream([Message.user("hi")], **{control: -0.1}))
        else:
            provider.chat([Message.user("hi")], **{control: -0.1})
    assert getattr(caught.value, "code", None) == "invalid_request"


class MockOpenAIProvider(OpenAICompatibleProvider):
    """Concrete implementation of OpenAICompatibleProvider for testing."""

    DEFAULT_API_URL = "https://api.mock.example.com"

    @property
    def provider_name(self) -> str:
        return "mock"

    def _build_headers(self) -> dict:
        return {
            "content-type": "application/json",
            "authorization": f"Bearer {self._api_key}",
        }


@pytest.mark.parametrize(
    "raw,canonical", [("length", "length"), ("vendor-new", None), (None, None)]
)
def test_native_raw_finish_and_actual_identity(raw, canonical):
    provider = MockOpenAIProvider(
        model="requested", api_key="loopback-only", api_url="http://127.0.0.1"
    )
    result = provider._parse_response(
        {"model": "actual", "choices": [{"message": {"content": "ok"}, "finish_reason": raw}]},
        "requested",
    )
    assert (result.model, result.provider, result.finish_reason, result.raw_finish_reason) == (
        "actual",
        "mock",
        canonical,
        raw,
    )


@pytest.mark.parametrize(
    "lines,expected_error",
    [
        ([], True),
        ([b"data: {broken}", b""], True),
        ([b'data: {"error":{"message":"denied"}}', b"", b"data: [DONE]"], True),
        (
            [b'data: {"model":["bad"],"choices":[{"delta":{"content":"ok"}}]}', b"data: [DONE]"],
            True,
        ),
        (
            [b'data: {"choices":[{"delta":{"content":"ok"},"finish_reason":17}]}', b"data: [DONE]"],
            True,
        ),
        ([b'data: {"choices":[{"delta":{"content":"partial"}}]}', b""], True),
        (
            [
                b'data: {"model":"actual","choices":[{"delta":{"content":"ok"},"finish_reason":"vendor-new"}]}',
                b"",
                b'data: {"choices":[],"usage":{"prompt_tokens":7,"completion_tokens":3,"total_tokens":10}}',
                b"",
                b"data: [DONE]",
                b"",
                b"data: {broken}",
            ],
            False,
        ),
    ],
)
def test_native_stream_explicit_terminal_and_error(monkeypatch, lines, expected_error):
    from nxuskit.errors import ProviderError

    provider = MockOpenAIProvider(
        model="requested", api_key="loopback-only", api_url="http://127.0.0.1"
    )

    class Response:
        closed = False

        def iter_lines(self):
            yield from lines

        def close(self):
            self.closed = True

    response = Response()
    monkeypatch.setattr(provider, "_make_request", lambda **kwargs: response)
    chunks = []
    if expected_error:
        with pytest.raises(ProviderError):
            for chunk in provider.chat_stream([]):
                chunks.append(chunk)
        assert not any(chunk.is_final() for chunk in chunks)
    else:
        chunks = list(provider.chat_stream([]))
        assert sum(chunk.is_final() for chunk in chunks) == 1
        final = chunks[-1]
        assert (final.terminal, final.finish_reason, final.raw_finish_reason, final.model) == (
            True,
            None,
            "vendor-new",
            "actual",
        )
        assert (final.usage.prompt_tokens, final.usage.completion_tokens) == (7, 3)
        assert "".join(chunk.delta for chunk in chunks) == "ok"
    assert response.closed


@pytest.mark.parametrize(
    "field,bad", [("model", ["bad"]), ("model", 17), ("finish_reason", 17), ("finish_reason", {})]
)
def test_native_unary_rejects_malformed_identity(field, bad):
    from nxuskit.errors import ProviderError

    provider = MockOpenAIProvider(model="requested", api_key="loopback-only")
    data = {"model": "actual", "choices": [{"message": {"content": "ok"}, "finish_reason": "stop"}]}
    if field == "model":
        data[field] = bad
    else:
        data["choices"][0][field] = bad
    with pytest.raises(ProviderError):
        provider._parse_response(data, "requested")


class TestOpenAICompatibleBuildRequest:
    """Tests for _build_request method."""

    def test_build_request_simple_text(self):
        """Should build request for simple text message."""
        provider = MockOpenAIProvider(
            model="mock-model",
            api_key="test-key",
        )

        request = provider._build_request(
            [Message.user("Hello")], effective_model="mock-model", stream=False
        )

        assert request["model"] == "mock-model"
        assert request["stream"] is False
        assert len(request["messages"]) == 1
        assert request["messages"][0]["role"] == "user"
        assert request["messages"][0]["content"] == "Hello"

    def test_build_request_with_system_message(self):
        """Should include system messages in request."""
        messages = [
            Message.system("You are helpful"),
            Message.user("Hello"),
        ]

        provider = MockOpenAIProvider(
            model="mock-model",
            api_key="test-key",
        )

        request = provider._build_request(messages, effective_model="mock-model", stream=False)

        assert len(request["messages"]) == 2
        assert request["messages"][0]["role"] == "system"
        assert request["messages"][1]["role"] == "user"

    def test_build_request_with_url_image(self):
        """Should format URL images correctly."""
        message = Message.user("What's this?").with_image_url("https://example.com/image.jpg")

        provider = MockOpenAIProvider(
            model="mock-model",
            api_key="test-key",
        )

        request = provider._build_request([message], effective_model="mock-model", stream=False)

        assert len(request["messages"]) == 1
        msg_content = request["messages"][0]["content"]
        assert isinstance(msg_content, list)
        assert msg_content[0]["type"] == "text"
        assert msg_content[1]["type"] == "image_url"
        assert "https://example.com/image.jpg" in msg_content[1]["image_url"]["url"]

    def test_build_request_with_base64_image(self):
        """Should format base64 images correctly."""
        image = ImageSource(
            source_type=ImageSourceType.BASE64,
            data="iVBORw0KGgo",  # Partial base64
        )
        message = Message.user("Analyze")
        message.images = [image]

        provider = MockOpenAIProvider(
            model="mock-model",
            api_key="test-key",
        )

        request = provider._build_request([message], effective_model="mock-model", stream=False)

        msg_content = request["messages"][0]["content"]
        assert isinstance(msg_content, list)
        assert msg_content[1]["type"] == "image_url"
        assert "data:image/jpeg;base64," in msg_content[1]["image_url"]["url"]

    def test_build_request_stream_flag(self):
        """Should set stream flag correctly."""
        provider = MockOpenAIProvider(
            model="mock-model",
            api_key="test-key",
        )

        request_nonstream = provider._build_request(
            [Message.user("Hi")], effective_model="mock-model", stream=False
        )
        request_stream = provider._build_request(
            [Message.user("Hi")], effective_model="mock-model", stream=True
        )

        assert request_nonstream["stream"] is False
        assert request_stream["stream"] is True


class TestOpenAICompatibleParseResponse:
    """Tests for _parse_response method."""

    def test_parse_response_simple(self):
        """Should parse simple response correctly."""
        provider = MockOpenAIProvider(
            model="mock-model",
            api_key="test-key",
        )

        response_data = {
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Hello back!",
                    },
                    "finish_reason": "stop",
                }
            ],
            "usage": {
                "prompt_tokens": 5,
                "completion_tokens": 8,
                "total_tokens": 13,
            },
        }

        response = provider._parse_response(response_data, "mock-model")

        assert response.content == "Hello back!"
        assert response.model == "mock-model"
        assert response.finish_reason == "stop"
        assert response.usage.prompt_tokens == 5
        assert response.usage.completion_tokens == 8
        assert response.usage.total_tokens == 13

    def test_parse_response_empty_content(self):
        """Should handle empty response content."""
        provider = MockOpenAIProvider(
            model="mock-model",
            api_key="test-key",
        )

        response_data = {
            "choices": [],
            "usage": {
                "prompt_tokens": 0,
                "completion_tokens": 0,
                "total_tokens": 0,
            },
        }

        response = provider._parse_response(response_data, "mock-model")

        assert response.content is None
        assert response.finish_reason is None

    def test_parse_response_missing_usage(self):
        """Should handle missing usage information."""
        provider = MockOpenAIProvider(
            model="mock-model",
            api_key="test-key",
        )

        response_data = {
            "choices": [
                {
                    "message": {"role": "assistant", "content": "Response"},
                    "finish_reason": "stop",
                }
            ],
        }

        response = provider._parse_response(response_data, "mock-model")

        assert response.content == "Response"
        assert response.usage.prompt_tokens == 0
        assert response.usage.completion_tokens == 0


class TestOpenAICompatibleParseStreamEvent:
    """Tests for _parse_stream_event method."""

    def test_parse_stream_event_with_content(self):
        """Should parse stream event with content."""
        provider = MockOpenAIProvider(
            model="mock-model",
            api_key="test-key",
        )

        event_data = {
            "choices": [
                {
                    "delta": {"content": "Hello"},
                    "finish_reason": None,
                }
            ]
        }

        chunk = provider._parse_stream_event(event_data, "mock-model")

        assert chunk is not None
        assert chunk.delta == "Hello"
        assert chunk.model == "mock-model"

    def test_parse_stream_event_final_with_finish_reason(self):
        """Should return chunk with finish_reason when stream ends."""
        provider = MockOpenAIProvider(
            model="mock-model",
            api_key="test-key",
        )

        event_data = {
            "choices": [
                {
                    "delta": {},  # No content
                    "finish_reason": "stop",
                }
            ]
        }

        chunk = provider._parse_stream_event(event_data, "mock-model")

        assert chunk is not None
        assert chunk.delta == ""
        assert chunk.finish_reason == "stop"
        assert chunk.is_final()

    def test_parse_stream_event_empty_choices(self):
        """Should handle empty choices."""
        provider = MockOpenAIProvider(
            model="mock-model",
            api_key="test-key",
        )

        event_data = {"choices": []}

        chunk = provider._parse_stream_event(event_data, "mock-model")

        assert chunk is None


class TestOpenAICompatibleHeaders:
    """Tests for header building."""

    def test_build_headers(self):
        """Should build headers with API key."""
        provider = MockOpenAIProvider(
            model="mock-model",
            api_key="secret-key-123",
        )

        headers = provider._build_headers()

        assert headers["authorization"] == "Bearer secret-key-123"
        assert headers["content-type"] == "application/json"


def test_selected_metadata_citation_heavy_unary():
    """Synthetic 40-result workload; expected values never use the mapper."""
    import json
    from nxuskit.providers.perplexity import PerplexityProvider

    expected = {
        "citations": [f"https://example.org/source/{i}" for i in range(40)],
        "search_results": [
            {
                "title": f"Evidence {i}",
                "url": f"https://example.org/source/{i}",
                "date": "2026-09-14",
                "snippet": "Evidence text. " * 72,
            }
            for i in range(40)
        ],
        "usage": {
            "cost": {"total_cost": 0.0123},
            "search_context_size": "high",
            "citation_tokens": 1200,
            "num_search_queries": 4,
            "reasoning_tokens": 200,
        },
    }
    size = len(
        json.dumps(expected, ensure_ascii=False, separators=(",", ":"), allow_nan=False).encode(
            "utf-8"
        )
    )
    print(f"citation-heavy selected metadata compact UTF-8 bytes={size}")
    assert 40000 < size < 65536
    provider = PerplexityProvider("sonar", api_key="loopback-only")
    data = {
        **expected,
        "model": "sonar",
        "choices": [{"message": {"content": "answer"}, "finish_reason": "stop"}],
        "unselected": "not public metadata",
    }
    result = provider._parse_response(data, "sonar")
    assert result.provider_metadata == expected


def test_selected_metadata_does_not_mask_unary_provider_error():
    from nxuskit.errors import ProviderError
    from nxuskit.providers.openrouter import OpenRouterProvider

    provider = OpenRouterProvider("model", api_key="loopback-only")
    with pytest.raises(ProviderError):
        provider._parse_response(
            {
                "id": "r1",
                "error": {"message": "denied"},
                "choices": [{"message": {"content": "not success"}}],
            },
            "model",
        )


@pytest.mark.parametrize(
    "control,value", [("logprobs", True), ("top_logprobs", 3), ("reasoning_effort", "low")]
)
def test_unqualified_profile_rejects_new_controls(control, value):
    from nxuskit.errors import ProviderError
    from nxuskit.providers.perplexity import PerplexityProvider

    provider = PerplexityProvider("sonar", api_key="loopback-only")
    with pytest.raises(ProviderError, match="unsupported") as failure:
        provider._build_request([Message.user("hi")], "sonar", **{control: value})
    assert getattr(failure.value, "code", None) == "unsupported_capability"
    assert not failure.value.is_retryable


def test_tool_definition_type_is_canonical_function():
    from nxuskit.errors import ProviderError

    tool = ToolDefinition.create("lookup", parameters={"type": "object"})
    tool.type = "not_function"
    with pytest.raises(ProviderError) as failure:
        tool.validate()
    assert getattr(failure.value, "code", None) == "invalid_request"


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
def test_invalid_tool_definition_is_invalid_request(tool):
    from nxuskit.errors import ProviderError
    from nxuskit.providers.perplexity import PerplexityProvider

    provider = PerplexityProvider("sonar", api_key="loopback-only")
    with pytest.raises(ProviderError) as failure:
        provider._build_request([Message.user("hi")], "sonar", tools=[tool])
    assert getattr(failure.value, "code", None) == "invalid_request"

@pytest.mark.parametrize("stream", [False, True])
@pytest.mark.parametrize("case", ["stop", "json", "schema", "tools", "tool_choice"])
def test_perplexity_native_sync_unsupported_request_surface_rejects_before_transport(
    case, stream, monkeypatch
):
    from nxuskit.errors import ProviderError
    from nxuskit.providers.perplexity import PerplexityProvider
    from nxuskit.types import ResponseFormat

    provider = PerplexityProvider("sonar", api_key="loopback-only")
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
    calls = []

    def forbidden(**transport_kwargs):
        calls.append(transport_kwargs)
        raise AssertionError("unsupported Perplexity request reached transport")

    monkeypatch.setattr(provider, "_make_request", forbidden)
    with pytest.raises(ProviderError, match="unsupported request control") as caught:
        if stream:
            list(provider.chat_stream([Message.user("hi")], **kwargs))
        else:
            provider.chat([Message.user("hi")], **kwargs)
    assert getattr(caught.value, "code", None) == "unsupported_capability"
    assert calls == []


@pytest.mark.parametrize("stream", [False, True])
@pytest.mark.parametrize("source_type", [ImageSourceType.URL, ImageSourceType.BASE64, ImageSourceType.FILEPATH])
def test_perplexity_native_sync_images_reject_before_transport(source_type, stream, monkeypatch):
    from nxuskit.errors import ProviderError
    from nxuskit.providers.perplexity import PerplexityProvider

    provider = PerplexityProvider("sonar", api_key="loopback-only")
    message = Message.user("hi")
    value = (
        "https://image.invalid/p.png"
        if source_type == ImageSourceType.URL
        else "aGVsbG8="
        if source_type == ImageSourceType.BASE64
        else "/definitely/not/read/perplexity.png"
    )
    message.images.append(ImageSource(source_type, value, "image/png"))
    calls = []

    def forbidden(**transport_kwargs):
        calls.append(transport_kwargs)
        raise AssertionError("unsupported Perplexity image reached transport")

    monkeypatch.setattr(provider, "_make_request", forbidden)
    with pytest.raises(ProviderError, match="unsupported request control: images") as caught:
        if stream:
            list(provider.chat_stream([message]))
        else:
            provider.chat([message])
    assert getattr(caught.value, "code", None) == "unsupported_capability"
    assert calls == []


def test_perplexity_response_format_validation_precedes_unsupported_and_text_is_omitted():
    from nxuskit.errors import ProviderError
    from nxuskit.providers.perplexity import PerplexityProvider
    from nxuskit.types import ResponseFormat

    provider = PerplexityProvider("sonar", api_key="loopback-only")
    with pytest.raises(ProviderError) as caught:
        provider._build_request(
            [Message.user("hi")],
            "sonar",
            response_format={"type": "json_schema", "schema": []},
        )
    assert getattr(caught.value, "code", None) == "invalid_request"

    request = provider._build_request(
        [Message.user("hi")], "sonar", response_format=ResponseFormat.TEXT
    )
    assert "response_format" not in request


@pytest.mark.parametrize("size", [65535, 65536, 65537])
@pytest.mark.parametrize("unicode_text", [False, True])
def test_selected_metadata_byte_boundary(size, unicode_text):
    import json
    from nxuskit.errors import ProviderError
    from nxuskit.providers.perplexity import PerplexityProvider

    overhead = len(json.dumps({"citations": [""]}, separators=(",", ":")).encode())
    count = size - overhead
    value = ("é" * (count // 2) + "x" * (count % 2)) if unicode_text else "x" * count
    selected = {"citations": [value]}
    assert len(json.dumps(selected, ensure_ascii=False, separators=(",", ":")).encode()) == size
    data = {**selected, "choices": [{"message": {"content": "ok"}}]}
    provider = PerplexityProvider("sonar", api_key="loopback-only")
    if size > 65536:
        with pytest.raises(ProviderError, match="provider_metadata") as failure:
            provider._parse_response(data, "sonar")
        assert not failure.value.is_retryable
    else:
        assert provider._parse_response(data, "sonar").provider_metadata == selected


@pytest.mark.parametrize("depth", [16, 17])
def test_selected_metadata_depth_boundary(depth):
    from nxuskit.errors import ProviderError
    from nxuskit.providers.perplexity import PerplexityProvider

    value = "leaf"
    for _ in range(depth - 1):
        value = [value]
    data = {"citations": value, "choices": [{"message": {"content": "ok"}}]}
    provider = PerplexityProvider("sonar", api_key="loopback-only")
    if depth == 17:
        with pytest.raises(ProviderError, match="provider_metadata"):
            provider._parse_response(data, "sonar")
    else:
        assert provider._parse_response(data, "sonar").provider_metadata == {"citations": value}


@pytest.mark.parametrize("reset_usage", [False, True])
@pytest.mark.parametrize("overflow", [False, True])
def test_selected_metadata_stream_merge_and_error(monkeypatch, overflow, reset_usage):
    import json
    from nxuskit.errors import ProviderError
    from nxuskit.providers.openrouter import OpenRouterProvider

    events = [
        {
            "id": "r1",
            "provider": "vendor",
            "choices": [{"delta": {"content": "text"}}],
            "usage": {"cost": 0.1, "cost_details": {"upstream_inference_cost": 0.08}},
        },
        {"choices": [], "usage": {"completion_tokens_details": {"reasoning_tokens": 3}}},
        {"choices": [], "provider": None, "usage": {"cost": None}},
    ]
    if reset_usage:
        events.extend([{"usage": None}, {"usage": {"cost": None}}])
    if overflow:
        events.append({"id": "x" * 65536, "choices": []})

    class Response:
        def iter_lines(self):
            for event in events:
                yield ("data: " + json.dumps(event)).encode()
            yield b"data: [DONE]"

        def close(self):
            pass

    provider = OpenRouterProvider("model", api_key="loopback-only")
    monkeypatch.setattr(provider, "_make_request", lambda **kwargs: Response())
    chunks = []
    if overflow:
        with pytest.raises(ProviderError, match="provider_metadata") as failure:
            for chunk in provider.chat_stream([]):
                chunks.append(chunk)
        assert not failure.value.is_retryable
        assert "".join(c.delta for c in chunks) == "text"
        assert not any(c.is_final() for c in chunks)
    else:
        chunks = list(provider.chat_stream([]))
        expected_usage = (
            {"cost": None}
            if reset_usage
            else {
                "cost": None,
                "cost_details": {"upstream_inference_cost": 0.08},
                "completion_tokens_details": {"reasoning_tokens": 3},
            }
        )
        assert chunks[-1].provider_metadata == {
            "id": "r1",
            "provider": None,
            "usage": expected_usage,
        }
        assert sum(c.is_final() for c in chunks) == 1
    assert all(c.provider_metadata is None for c in chunks if not c.is_final())


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
def test_fireworks_native_sync_bounds_and_advanced_controls_fail_before_transport(
    streaming, controls, expected_code, monkeypatch
):
    from nxuskit import Provider
    from nxuskit.errors import ProviderError

    provider = Provider.fireworks(model="m", api_key="fixture", use_ffi=False)

    def forbidden(**kwargs):
        raise AssertionError("Fireworks invalid/unsupported control reached transport")

    monkeypatch.setattr(provider, "_make_request", forbidden)
    with pytest.raises(ProviderError) as caught:
        if streaming:
            list(provider.chat_stream([Message.user("hi")], **controls))
        else:
            provider.chat([Message.user("hi")], **controls)

    assert getattr(caught.value, "code", None) == expected_code


def test_fireworks_native_sync_schema_and_unary_tools_wire():
    from nxuskit.providers.fireworks import FireworksProvider

    provider = FireworksProvider(
        model="accounts/fireworks/models/llama-v3-70b-instruct",
        api_key="fixture",
    )
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

    wire = provider._build_request(
        [Message.user("Look up A-1.")],
        effective_model=provider._model,
        stream=False,
        response_format={"type": "json_schema", "schema": schema},
        tools=[tool],
        tool_choice={"type": "function", "function": {"name": "lookup"}},
    )

    assert wire["response_format"] == {
        "type": "json_schema",
        "json_schema": {"name": "response", "strict": True, "schema": schema},
    }
    assert wire["tools"] == [tool.to_dict()]
    assert wire["tool_choice"] == {
        "type": "function",
        "function": {"name": "lookup"},
    }


def test_lmstudio_native_sync_core_wire_preserves_supported_controls_and_vision():
    from nxuskit.providers.lmstudio import LMStudioProvider
    from nxuskit.types import ResponseFormat

    provider = LMStudioProvider(model="local-model")
    message = Message.user("Describe this.").with_image_url("https://example.invalid/a.png")
    stops = ["a", "b", "c", "d"]

    wire = provider._build_request(
        [message],
        effective_model=provider._model,
        stream=False,
        temperature=0.4,
        max_tokens=123,
        top_p=0.8,
        stop=stops,
        response_format=ResponseFormat.JSON,
        presence_penalty=-0.5,
        frequency_penalty=0.5,
        seed=42,
    )

    assert wire["temperature"] == 0.4
    assert wire["max_tokens"] == 123
    assert wire["top_p"] == 0.8
    assert wire["stop"] == stops
    assert wire["presence_penalty"] == -0.5
    assert wire["frequency_penalty"] == 0.5
    assert wire["seed"] == 42
    assert wire["response_format"] == {"type": "json_object"}
    assert wire["messages"][0]["content"][0] == {
        "type": "text",
        "text": "Describe this.",
    }
    assert wire["messages"][0]["content"][1] == {
        "type": "image_url",
        "image_url": {"url": "https://example.invalid/a.png"},
    }


def test_lmstudio_native_sync_schema_and_unary_tools_wire():
    from nxuskit.providers.lmstudio import LMStudioProvider

    provider = LMStudioProvider(model="local-model")
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

    wire = provider._build_request(
        [Message.user("Look up A-1.")],
        effective_model=provider._model,
        stream=False,
        response_format={"type": "json_schema", "schema": schema},
        tools=[tool],
        tool_choice=choice,
    )

    assert wire["response_format"] == {
        "type": "json_schema",
        "json_schema": {"name": "response", "strict": True, "schema": schema},
    }
    assert wire["tools"] == [tool.to_dict()]
    assert wire["tool_choice"] == choice


@pytest.mark.parametrize(
    "kwargs",
    [
        {"tools": [ToolDefinition.create("lookup", parameters={"type": "object"})]},
        {"tool_choice": {"type": "function", "function": {"name": "lookup"}}},
    ],
)
def test_lmstudio_native_sync_tool_streaming_fails_before_transport(kwargs, monkeypatch):
    from nxuskit import Provider
    from nxuskit.errors import ProviderError

    provider = Provider.lmstudio(model="m", use_ffi=False)

    def forbidden(**request):
        raise AssertionError("LM Studio tool streaming reached transport")

    monkeypatch.setattr(provider, "_make_request", forbidden)
    with pytest.raises(ProviderError, match="unsupported LM Studio tool streaming") as caught:
        list(provider.chat_stream([Message.user("stream")], **kwargs))

    assert getattr(caught.value, "code", None) == "unsupported_capability"


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
def test_lmstudio_native_sync_bounds_and_advanced_controls_fail_before_transport(
    streaming, controls, expected_code, monkeypatch
):
    from nxuskit import Provider
    from nxuskit.errors import ProviderError

    provider = Provider.lmstudio(model="m", use_ffi=False)

    def forbidden(**kwargs):
        raise AssertionError("LM Studio invalid/unsupported control reached transport")

    monkeypatch.setattr(provider, "_make_request", forbidden)
    with pytest.raises(ProviderError) as caught:
        if streaming:
            list(provider.chat_stream([Message.user("hi")], **controls))
        else:
            provider.chat([Message.user("hi")], **controls)

    assert getattr(caught.value, "code", None) == expected_code


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
def test_groq_native_sync_bounds_and_advanced_controls_fail_before_transport(
    streaming, controls, expected_code, monkeypatch
):
    from nxuskit import Provider
    from nxuskit.errors import ProviderError

    provider = Provider.groq(model="m", api_key="fixture", use_ffi=False)

    def forbidden(**kwargs):
        raise AssertionError("Groq invalid/unsupported control reached transport")

    monkeypatch.setattr(provider, "_make_request", forbidden)
    with pytest.raises(ProviderError) as caught:
        if streaming:
            list(provider.chat_stream([Message.user("hi")], **controls))
        else:
            provider.chat([Message.user("hi")], **controls)

    assert getattr(caught.value, "code", None) == expected_code


@pytest.mark.parametrize("streaming", [False, True])
def test_groq_native_sync_images_fail_before_transport(streaming, monkeypatch):
    from nxuskit import Provider
    from nxuskit.errors import ProviderError

    provider = Provider.groq(model="m", api_key="fixture", use_ffi=False)
    message = Message.user("describe").with_image_url("https://example.invalid/image.png")

    def forbidden(**kwargs):
        raise AssertionError("Groq image request reached transport")

    monkeypatch.setattr(provider, "_make_request", forbidden)
    with pytest.raises(ProviderError, match="unsupported request control: images") as caught:
        if streaming:
            list(provider.chat_stream([message]))
        else:
            provider.chat([message])
    assert getattr(caught.value, "code", None) == "unsupported_capability"


@pytest.mark.parametrize(
    "kwargs",
    [
        {"tools": [ToolDefinition.create("lookup", parameters={"type": "object"})]},
        {"tool_choice": {"type": "function", "function": {"name": "lookup"}}},
    ],
)
def test_groq_native_sync_tool_streaming_fails_before_transport(kwargs, monkeypatch):
    from nxuskit import Provider
    from nxuskit.errors import ProviderError

    provider = Provider.groq(model="m", api_key="fixture", use_ffi=False)

    def forbidden(**request):
        raise AssertionError("Groq tool streaming reached transport")

    monkeypatch.setattr(provider, "_make_request", forbidden)
    with pytest.raises(ProviderError, match="unsupported Groq tool streaming") as caught:
        list(provider.chat_stream([Message.user("stream")], **kwargs))

    assert getattr(caught.value, "code", None) == "unsupported_capability"


def test_groq_native_sync_schema_and_unary_tools_wire():
    from nxuskit.providers.groq import GroqProvider

    provider = GroqProvider(model="llama-3.3-70b-versatile", api_key="fixture")
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

    wire = provider._build_request(
        [Message.user("Look up A-1.")],
        effective_model=provider._model,
        stream=False,
        response_format={"type": "json_schema", "schema": schema},
        tools=[tool],
        tool_choice=choice,
    )

    assert wire["response_format"] == {
        "type": "json_schema",
        "json_schema": {"name": "response", "strict": True, "schema": schema},
    }
    assert wire["tools"] == [tool.to_dict()]
    assert wire["tool_choice"] == choice


def test_mistral_native_sync_core_wire_preserves_supported_controls_and_vision():
    from nxuskit.providers.mistral import MistralProvider
    from nxuskit.types import ResponseFormat

    provider = MistralProvider(model="pixtral-large-latest", api_key="fixture")
    message = Message.user("Describe this.").with_image_url("https://example.invalid/a.png")
    stops = ["a", "b", "c", "d"]

    wire = provider._build_request(
        [message],
        effective_model=provider._model,
        stream=False,
        temperature=0.4,
        max_tokens=123,
        top_p=0.8,
        stop=stops,
        response_format=ResponseFormat.JSON,
        presence_penalty=-0.5,
        frequency_penalty=0.5,
        seed=-42,
    )

    assert wire["temperature"] == 0.4
    assert wire["max_tokens"] == 123
    assert wire["top_p"] == 0.8
    assert wire["stop"] == stops
    assert wire["presence_penalty"] == -0.5
    assert wire["frequency_penalty"] == 0.5
    assert wire["seed"] == -42
    assert wire["response_format"] == {"type": "json_object"}
    assert wire["messages"][0]["content"][0] == {
        "type": "text",
        "text": "Describe this.",
    }
    assert wire["messages"][0]["content"][1] == {
        "type": "image_url",
        "image_url": {"url": "https://example.invalid/a.png"},
    }


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
def test_mistral_native_sync_bounds_and_advanced_controls_fail_before_transport(
    streaming, controls, expected_code, monkeypatch
):
    from nxuskit import Provider
    from nxuskit.errors import ProviderError

    provider = Provider.mistral(model="m", api_key="fixture", use_ffi=False)

    def forbidden(**kwargs):
        raise AssertionError("Mistral invalid/unsupported control reached transport")

    monkeypatch.setattr(provider, "_make_request", forbidden)
    with pytest.raises(ProviderError) as caught:
        if streaming:
            list(provider.chat_stream([Message.user("hi")], **controls))
        else:
            provider.chat([Message.user("hi")], **controls)

    assert getattr(caught.value, "code", None) == expected_code


@pytest.mark.parametrize(
    "kwargs",
    [
        {"tools": [ToolDefinition.create("lookup", parameters={"type": "object"})]},
        {"tool_choice": {"type": "function", "function": {"name": "lookup"}}},
    ],
)
def test_mistral_native_sync_tool_streaming_fails_before_transport(kwargs, monkeypatch):
    from nxuskit import Provider
    from nxuskit.errors import ProviderError

    provider = Provider.mistral(model="m", api_key="fixture", use_ffi=False)

    def forbidden(**request):
        raise AssertionError("Mistral tool streaming reached transport")

    monkeypatch.setattr(provider, "_make_request", forbidden)
    with pytest.raises(ProviderError, match="unsupported Mistral tool streaming") as caught:
        list(provider.chat_stream([Message.user("stream")], **kwargs))

    assert getattr(caught.value, "code", None) == "unsupported_capability"


def test_mistral_native_sync_schema_and_unary_tools_wire():
    from nxuskit.providers.mistral import MistralProvider

    provider = MistralProvider(model="mistral-large-latest", api_key="fixture")
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

    wire = provider._build_request(
        [Message.user("Look up A-1.")],
        effective_model=provider._model,
        stream=False,
        response_format={"type": "json_schema", "schema": schema},
        tools=[tool],
        tool_choice="auto",
    )

    assert wire["response_format"] == {
        "type": "json_schema",
        "json_schema": {"name": "response", "strict": True, "schema": schema},
    }
    assert wire["tools"] == [tool.to_dict()]
    assert wire["tool_choice"] == "auto"


def test_together_native_sync_core_wire_preserves_supported_controls_and_vision():
    from nxuskit.providers.together import TogetherProvider
    from nxuskit.types import ResponseFormat

    provider = TogetherProvider(model="meta-llama/Llama-3.2-11B-Vision-Instruct-Turbo", api_key="fixture")
    message = Message.user("Describe this.").with_image_url("https://example.invalid/a.png")
    stops = ["a", "b", "c", "d"]

    wire = provider._build_request(
        [message],
        effective_model=provider._model,
        stream=False,
        temperature=0.4,
        max_tokens=123,
        top_p=0.8,
        stop=stops,
        response_format=ResponseFormat.JSON,
        presence_penalty=-0.5,
        frequency_penalty=0.5,
        seed=-42,
    )

    assert wire["temperature"] == 0.4
    assert wire["max_tokens"] == 123
    assert wire["top_p"] == 0.8
    assert wire["stop"] == stops
    assert wire["presence_penalty"] == -0.5
    assert wire["frequency_penalty"] == 0.5
    assert wire["seed"] == -42
    assert wire["response_format"] == {"type": "json_object"}
    assert wire["messages"][0]["content"][0] == {
        "type": "text",
        "text": "Describe this.",
    }
    assert wire["messages"][0]["content"][1] == {
        "type": "image_url",
        "image_url": {"url": "https://example.invalid/a.png"},
    }


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
def test_together_native_sync_bounds_and_advanced_controls_fail_before_transport(
    streaming, controls, expected_code, monkeypatch
):
    from nxuskit import Provider
    from nxuskit.errors import ProviderError

    provider = Provider.together(model="m", api_key="fixture", use_ffi=False)

    def forbidden(**kwargs):
        raise AssertionError("Together invalid/unsupported control reached transport")

    monkeypatch.setattr(provider, "_make_request", forbidden)
    with pytest.raises(ProviderError) as caught:
        if streaming:
            list(provider.chat_stream([Message.user("hi")], **controls))
        else:
            provider.chat([Message.user("hi")], **controls)

    assert getattr(caught.value, "code", None) == expected_code


@pytest.mark.parametrize(
    "kwargs",
    [
        {"tools": [ToolDefinition.create("lookup", parameters={"type": "object"})]},
        {"tool_choice": "required"},
    ],
)
def test_together_native_sync_tool_streaming_fails_before_transport(kwargs, monkeypatch):
    from nxuskit import Provider
    from nxuskit.errors import ProviderError

    provider = Provider.together(model="m", api_key="fixture", use_ffi=False)

    def forbidden(**request):
        raise AssertionError("Together tool streaming reached transport")

    monkeypatch.setattr(provider, "_make_request", forbidden)
    with pytest.raises(ProviderError, match="unsupported Together tool streaming") as caught:
        list(provider.chat_stream([Message.user("stream")], **kwargs))

    assert getattr(caught.value, "code", None) == "unsupported_capability"


def test_together_native_sync_schema_and_unary_tools_wire():
    from nxuskit.providers.together import TogetherProvider

    provider = TogetherProvider(model="meta-llama/Llama-3.3-70B-Instruct-Turbo", api_key="fixture")
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

    wire = provider._build_request(
        [Message.user("Look up A-1.")],
        effective_model=provider._model,
        stream=False,
        response_format={"type": "json_schema", "schema": schema},
        tools=[tool],
        tool_choice="required",
    )

    assert wire["response_format"] == {
        "type": "json_schema",
        "json_schema": {"name": "response", "strict": True, "schema": schema},
    }
    assert wire["tools"] == [tool.to_dict()]
    assert wire["tool_choice"] == "required"


@pytest.mark.parametrize(
    "tool_choice,expected_code",
    [
        ({"type": "function", "function": {"name": "lookup"}}, "unsupported_capability"),
        ({"type": "function", "function": {"name": ""}}, "invalid_request"),
        ("bogus", "invalid_request"),
    ],
)
def test_together_native_sync_named_or_invalid_tool_choice_fails_before_transport(
    tool_choice, expected_code, monkeypatch
):
    from nxuskit import Provider
    from nxuskit.errors import ProviderError

    provider = Provider.together(model="m", api_key="fixture", use_ffi=False)

    def forbidden(**request):
        raise AssertionError("Together unsupported/invalid tool choice reached transport")

    monkeypatch.setattr(provider, "_make_request", forbidden)
    with pytest.raises(ProviderError) as caught:
        provider.chat([Message.user("hi")], tool_choice=tool_choice)

    assert getattr(caught.value, "code", None) == expected_code


def test_openrouter_native_sync_core_wire_preserves_supported_controls_and_vision():
    from nxuskit.providers.openrouter import OpenRouterProvider
    from nxuskit.types import ResponseFormat

    provider = OpenRouterProvider(model="openai/gpt-4o", api_key="fixture")
    message = Message.user("Describe this.").with_image_url("https://example.invalid/a.png")
    stops = ["a", "b", "c", "d"]

    wire = provider._build_request(
        [message],
        effective_model=provider._model,
        stream=False,
        temperature=0.4,
        max_tokens=123,
        top_p=0.8,
        stop=stops,
        response_format=ResponseFormat.JSON,
        presence_penalty=-0.5,
        frequency_penalty=0.5,
        seed=-42,
    )

    assert wire["temperature"] == 0.4
    assert wire["max_tokens"] == 123
    assert wire["top_p"] == 0.8
    assert wire["stop"] == stops
    assert wire["presence_penalty"] == -0.5
    assert wire["frequency_penalty"] == 0.5
    assert wire["seed"] == -42
    assert wire["response_format"] == {"type": "json_object"}
    assert wire["messages"][0]["content"][0] == {
        "type": "text",
        "text": "Describe this.",
    }
    assert wire["messages"][0]["content"][1] == {
        "type": "image_url",
        "image_url": {"url": "https://example.invalid/a.png"},
    }


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
def test_openrouter_native_sync_bounds_and_advanced_controls_fail_before_transport(
    streaming, controls, expected_code, monkeypatch
):
    from nxuskit import Provider
    from nxuskit.errors import ProviderError

    provider = Provider.openrouter(model="m", api_key="fixture", use_ffi=False)

    def forbidden(**kwargs):
        raise AssertionError("OpenRouter invalid/unsupported control reached transport")

    monkeypatch.setattr(provider, "_make_request", forbidden)
    with pytest.raises(ProviderError) as caught:
        if streaming:
            list(provider.chat_stream([Message.user("hi")], **controls))
        else:
            provider.chat([Message.user("hi")], **controls)

    assert getattr(caught.value, "code", None) == expected_code


@pytest.mark.parametrize(
    "kwargs",
    [
        {"tools": [ToolDefinition.create("lookup", parameters={"type": "object"})]},
        {"tool_choice": "auto"},
    ],
)
def test_openrouter_native_sync_tool_streaming_fails_before_transport(kwargs, monkeypatch):
    from nxuskit import Provider
    from nxuskit.errors import ProviderError

    provider = Provider.openrouter(model="m", api_key="fixture", use_ffi=False)

    def forbidden(**request):
        raise AssertionError("OpenRouter tool streaming reached transport")

    monkeypatch.setattr(provider, "_make_request", forbidden)
    with pytest.raises(ProviderError, match="unsupported OpenRouter tool streaming") as caught:
        list(provider.chat_stream([Message.user("stream")], **kwargs))

    assert getattr(caught.value, "code", None) == "unsupported_capability"


def test_openrouter_native_sync_schema_and_named_unary_tool_wire():
    from nxuskit.providers.openrouter import OpenRouterProvider

    provider = OpenRouterProvider(model="openai/gpt-4o", api_key="fixture")
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

    wire = provider._build_request(
        [Message.user("Look up A-1.")],
        effective_model=provider._model,
        stream=False,
        response_format={"type": "json_schema", "schema": schema},
        tools=[tool],
        tool_choice=choice,
    )

    assert wire["response_format"] == {
        "type": "json_schema",
        "json_schema": {"name": "response", "strict": True, "schema": schema},
    }
    assert wire["tools"] == [tool.to_dict()]
    assert wire["tool_choice"] == choice


def test_empty_tools_three_state_native_sync_r47b():
    from nxuskit import Provider
    from nxuskit.errors import ProviderError

    tool = ToolDefinition.create("lookup", parameters={"type": "object"})
    for profile in ("claude", "groq"):
        provider = getattr(Provider, profile)(model="m", api_key="fixture", use_ffi=False)
        omitted = provider._build_request([Message.user("hi")], "m", stream=True)
        empty = provider._build_request([Message.user("hi")], "m", stream=True, tools=[])
        assert "tools" not in omitted
        assert "tools" not in empty
        with pytest.raises(ProviderError) as caught:
            provider._build_request([Message.user("hi")], "m", stream=True, tools=[tool])
        assert getattr(caught.value, "code", None) == "unsupported_capability"
