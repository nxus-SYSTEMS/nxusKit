"""Literal wire and socket-lifetime probes for native async OpenAI transport."""

import asyncio
import json
from contextlib import asynccontextmanager

import pytest

from nxuskit import Message, Provider, ToolDefinition


@pytest.mark.parametrize("streaming", [False, True])
@pytest.mark.parametrize(
    "controls",
    [
        {"max_tokens": 0},
        {"max_tokens": False},
        {"max_tokens": -1},
        {"max_tokens": 1.5},
        {"max_tokens": 1048577},
        {"temperature": True},
        {"temperature": -0.1},
        {"temperature": float("nan")},
        {"top_p": True},
        {"top_p": 1.1},
        {"stop": [""]},
        {"stop": ["x"] * 5},
        {"stop": ["x"] * 17},
    ],
)
def test_claude_native_async_invalid_controls_before_http(streaming, controls):
    from nxuskit.errors import ProviderError

    async def run():
        async with endpoint("literal", b"{}") as (url, _, _, requests):
            async with Provider.create_async("claude", "m", api_url=url, use_ffi=False) as provider:
                with pytest.raises(ProviderError, match="invalid request control") as caught:
                    if streaming:
                        provider.chat_stream_async([], **controls)
                    else:
                        await provider.chat_async([], **controls)
                assert getattr(caught.value, "code", None) == "invalid_request"
                assert requests == [] and provider.outstanding == 0

    asyncio.run(run())


@pytest.mark.parametrize("kind", ["message_start", "message_stop"])
def test_claude_native_async_error_member_cannot_be_terminal(kind):
    from nxuskit.errors import ProviderError

    async def run():
        start = {"type": "message_start", "message": {"model": "m"}}
        event = {"type": kind, "message": {"model": "m"}, "error": {"type": "overloaded_error"}}
        events = ([start] if kind == "message_stop" else []) + [event, {"type": "message_stop"}]
        body = b"".join(b"data: " + json.dumps(e).encode() + b"\n\n" for e in events)
        async with endpoint("literal", body) as (url, *_):
            async with Provider.create_async("claude", "m", api_url=url, use_ffi=False) as provider:
                chunks = []
                with pytest.raises(ProviderError):
                    async for chunk in provider.chat_stream_async([]):
                        chunks.append(chunk)
                assert not chunks and provider.outstanding == 0

    asyncio.run(run())


@pytest.mark.parametrize("streaming", [False, True])
def test_claude_native_async_literal_message_lifecycle(streaming):
    unary = b'{"model":"actual","content":[{"type":"text","text":"ok"},{"type":"thinking","thinking":"why"}],"stop_reason":"vendor-new","usage":{"input_tokens":2,"output_tokens":3}}'
    events = [
        {
            "type": "message_start",
            "message": {"model": "actual", "usage": {"input_tokens": 2, "output_tokens": 0}},
        },
        {"type": "content_block_delta", "delta": {"type": "text_delta", "text": "ok"}},
        {"type": "content_block_delta", "delta": {"type": "thinking_delta", "thinking": "why"}},
        {
            "type": "message_delta",
            "delta": {"stop_reason": "vendor-new"},
            "usage": {"output_tokens": 3},
        },
        {"type": "message_stop"},
    ]
    body = (
        b"".join(b"data: " + json.dumps(event).encode() + b"\n\n" for event in events)
        + b"data: invalid\n\n"
        if streaming
        else unary
    )

    async def run():
        async with endpoint("literal", body) as (url, _, _, requests):
            async with Provider.create_async(
                "claude",
                "requested",
                api_key="fixture",
                api_url=url.removesuffix("/v1"),
                use_ffi=False,
            ) as provider:
                if streaming:
                    chunks = [
                        c
                        async for c in provider.chat_stream_async(
                            [Message.user("hi")], max_tokens=7
                        )
                    ]
                    assert "".join(c.delta for c in chunks) == "ok"
                    assert "".join(c.thinking or "" for c in chunks) == "why"
                    assert sum(c.is_final() for c in chunks) == 1
                    assert not any(c.raw_finish_reason for c in chunks[:-1])
                    result = chunks[-1]
                else:
                    result = await provider.chat_async([Message.user("hi")], max_tokens=7)
                    assert result.content == "ok" and result.thinking == "why"
                assert result.model == "actual"
                assert result.raw_finish_reason == "vendor-new" and result.finish_reason is None
                assert result.usage.input_tokens == 2 and result.usage.output_tokens == 3
                assert provider.outstanding == 0
                assert requests == [
                    (
                        b"POST /v1/messages HTTP/1.1",
                        {
                            "model": "requested",
                            "messages": [
                                {"role": "user", "content": [{"type": "text", "text": "hi"}]}
                            ],
                            "max_tokens": 7,
                            "stream": streaming,
                        },
                    )
                ]

    asyncio.run(run())


@pytest.mark.parametrize(
    "streaming,payload",
    [
        (False, b'{"error":{"type":"overloaded_error"}}'),
        (False, b'{"content":[{"type":"text","text":3}]}'),
        (False, b'{"content":[],"usage":{"input_tokens":true,"output_tokens":2}}'),
        (True, b'data: {"type":"message_stop"}\n\n'),
        (True, b"data: [DONE]\n\n"),
        (
            True,
            b'data: {"type":"message_start","message":{"model":3}}\n\ndata: {"type":"message_stop"}\n\n',
        ),
        (
            True,
            b'data: {"type":"message_start","message":{"usage":{"input_tokens":-1}}}\n\ndata: {"type":"message_stop"}\n\n',
        ),
    ],
)
def test_claude_native_async_invalid_shape_never_succeeds(streaming, payload):
    from nxuskit.errors import ProviderError

    async def run():
        async with endpoint("literal", payload) as (url, *_):
            async with Provider.create_async(
                "claude", "m", api_url=url.removesuffix("/v1"), use_ffi=False
            ) as provider:
                chunks = []
                with pytest.raises(ProviderError):
                    if streaming:
                        async for chunk in provider.chat_stream_async([]):
                            chunks.append(chunk)
                    else:
                        await provider.chat_async([])
                assert not any(chunk.is_final() for chunk in chunks)
                assert provider.outstanding == 0

    asyncio.run(run())


@pytest.mark.parametrize(
    "controls",
    [
        {"seed": 1},
        {"top_k": 1},
        {"min_p": 0},
        {"presence_penalty": 0.4},
        {"frequency_penalty": -0.3},
        {"logprobs": True},
        {"top_logprobs": 5},
        {"reasoning_effort": "medium"},
        {"thinking": "enabled"},
        {"response_format": {"type": "json_schema", "schema": {"type": "object"}}},
    ],
)
def test_claude_native_async_unqualified_controls_reject_without_http(controls):
    from nxuskit.errors import ProviderError

    async def run():
        async with endpoint("literal", b"{}") as (url, _, _, requests):
            async with Provider.create_async("claude", "m", api_url=url, use_ffi=False) as provider:
                with pytest.raises(ProviderError, match="unsupported") as caught:
                    provider.chat_stream_async([], **controls)
                assert getattr(caught.value, "code", None) == "unsupported_capability"
                assert requests == [] and provider.outstanding == 0

    asyncio.run(run())


@pytest.mark.parametrize("profile", ["claude", "perplexity"])
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
def test_native_async_invalid_tool_rejects_without_http(profile, tool):
    from nxuskit.errors import ProviderError

    async def run():
        async with endpoint("literal", b"{}") as (url, _, _, requests):
            async with Provider.create_async(
                profile, "m", api_key="loopback", api_url=url, use_ffi=False
            ) as provider:
                with pytest.raises(ProviderError) as failure:
                    await provider.chat_async([Message.user("hi")], tools=[tool])
                assert getattr(failure.value, "code", None) == "invalid_request"
                assert requests == []

    asyncio.run(run())


@pytest.mark.parametrize(
    "patch",
    [
        {"message": {"content": 123}},
        {"message": {"content": []}},
        {"message": {"content": True}},
        {"prompt_eval_count": -1},
        {"prompt_eval_count": True},
        {"eval_count": False},
        {"eval_count": "3"},
        {"eval_count": 1.5},
        {"eval_count": None},
    ],
)
@pytest.mark.parametrize("streaming", [False, True])
def test_ollama_native_async_malformed_content_usage_is_typed_error(patch, streaming):
    from nxuskit.errors import ProviderError

    async def run():
        payload = {
            "message": {"content": "ok"},
            "done": True,
            "prompt_eval_count": 2,
            "eval_count": 3,
            **patch,
        }
        body = json.dumps(payload).encode() + (b"\n" if streaming else b"")
        async with endpoint("literal", body) as (url, *_):
            async with Provider.create_async("ollama", "m", api_url=url, use_ffi=False) as provider:
                chunks = []
                with pytest.raises(ProviderError, match="Malformed Ollama response") as caught:
                    if streaming:
                        async for chunk in provider.chat_stream_async([]):
                            chunks.append(chunk)
                    else:
                        await provider.chat_async([])
                assert caught.value.code == "malformed_response"
                assert not chunks and provider.outstanding == 0

    asyncio.run(run())


@pytest.mark.parametrize("streaming", [False, True])
def test_ollama_native_async_schema_thinking_seed_and_terminal(streaming):
    first = b'{"model":"actual","message":{"content":"ok","thinking":"why"},"done":false}\n'
    last = b'{"model":"actual","done":true,"done_reason":"vendor-new","prompt_eval_count":2,"eval_count":3}\n'
    body = (
        first + last + b"invalid after terminal\n"
        if streaming
        else b'{"model":"actual","message":{"content":"ok","thinking":"why"},"done":true,"done_reason":"vendor-new","prompt_eval_count":2,"eval_count":3}'
    )

    async def run():
        async with endpoint("literal", body) as (url, _, _, requests):
            async with Provider.create_async(
                "ollama", "requested", api_url=url.removesuffix("/v1"), use_ffi=False
            ) as provider:
                controls = dict(
                    thinking="enabled",
                    seed=-1,
                    max_tokens=7,
                    response_format={"type": "json_schema", "schema": {"type": "object"}},
                )
                if streaming:
                    chunks = [
                        c
                        async for c in provider.chat_stream_async([Message.user("hi")], **controls)
                    ]
                    assert "".join(c.delta for c in chunks) == "ok"
                    assert "".join(c.thinking or "" for c in chunks) == "why"
                    assert sum(c.is_final() for c in chunks) == 1
                    result = chunks[-1]
                else:
                    result = await provider.chat_async([Message.user("hi")], **controls)
                    assert result.content == "ok" and result.thinking == "why"
                assert result.raw_finish_reason == "vendor-new" and result.finish_reason is None
                assert result.model == "actual" and result.usage.total_tokens == 5
                assert result.declared_profile is None
                assert result.endpoint_origin == url.removesuffix("/v1")
                assert requests == [
                    (
                        b"POST /api/chat HTTP/1.1",
                        {
                            "model": "requested",
                            "messages": [{"role": "user", "content": "hi"}],
                            "stream": streaming,
                            "options": {"num_predict": 7, "seed": -1},
                            "think": True,
                            "format": {"type": "object"},
                        },
                    )
                ]
            assert provider.closed and provider.outstanding == 0

    asyncio.run(run())


@pytest.mark.parametrize(
    "tail", [b"", b"not-json\n", b'{"error":"failure"}\n', b'{"done":"true"}\n']
)
def test_ollama_native_async_incomplete_stream_is_not_success(tail):
    from nxuskit.errors import ProviderError

    async def run():
        body = b'{"message":{"content":"partial"},"done":false}\n' + tail
        async with endpoint("literal", body) as (url, *_):
            async with Provider.create_async(
                "ollama", "m", api_url=url.removesuffix("/v1"), use_ffi=False
            ) as provider:
                chunks = []
                with pytest.raises(ProviderError):
                    async for chunk in provider.chat_stream_async([Message.user("hi")]):
                        chunks.append(chunk)
                assert "".join(c.delta for c in chunks) == "partial"
                assert not any(c.is_final() for c in chunks)
                assert provider.outstanding == 0

    asyncio.run(run())


@pytest.mark.parametrize(
    "control,value",
    [
        ("presence_penalty", 0.4),
        ("frequency_penalty", -0.3),
        ("reasoning_effort", "medium"),
    ],
)
def test_ollama_native_async_unsupported_controls_fail_before_transport(control, value):
    from nxuskit.errors import ProviderError

    async def run():
        async with endpoint("literal", b"{}") as (url, _, _, requests):
            async with Provider.create_async("ollama", "m", api_url=url, use_ffi=False) as provider:
                for streaming in (False, True):
                    with pytest.raises(
                        ProviderError, match="unsupported request control"
                    ) as caught:
                        if streaming:
                            _ = [
                                c async for c in provider.chat_stream_async([], **{control: value})
                            ]
                        else:
                            await provider.chat_async([], **{control: value})
                    assert getattr(caught.value, "code", None) == "unsupported_capability"
                assert requests == [] and provider.outstanding == 0

    asyncio.run(run())


def test_ollama_native_async_logprobs_reach_wire_and_response():
    async def run():
        payload = json.dumps(
            {
                "model": "actual",
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
        async with endpoint("literal", payload) as (url, _, _, requests):
            async with Provider.create_async(
                "ollama", "m", api_url=url.removesuffix("/v1"), use_ffi=False
            ) as provider:
                response = await provider.chat_async(
                    [Message.user("hi")], logprobs=True, top_logprobs=5
                )
                assert requests == [
                    (
                        b"POST /api/chat HTTP/1.1",
                        {
                            "model": "m",
                            "messages": [{"role": "user", "content": "hi"}],
                            "stream": False,
                            "logprobs": True,
                            "top_logprobs": 5,
                        },
                    )
                ]
                assert response.logprobs is not None
                assert response.logprobs.content[0].token == "ok"
                assert provider.outstanding == 0

    asyncio.run(run())


@pytest.mark.parametrize(
    "profile,control,value,expected_code",
    [
        ("perplexity", "seed", 42, "unsupported_capability"),
        ("xai", "presence_penalty", 0.4, "unsupported_capability"),
        ("xai", "frequency_penalty", -0.3, "unsupported_capability"),
        ("xai", "seed", 42, "unsupported_capability"),
        ("perplexity", "presence_penalty", -0.1, "invalid_request"),
        ("perplexity", "frequency_penalty", -0.1, "invalid_request"),
    ],
)
def test_native_async_profile_generation_controls_fail_before_transport(
    profile, control, value, expected_code
):
    from nxuskit.errors import ProviderError

    async def run():
        async with endpoint("literal", b"{}") as (url, _, _, requests):
            async with Provider.create_async(
                profile,
                "m",
                api_key="loopback",
                api_url=url,
                use_ffi=False,
            ) as provider:
                for streaming in (False, True):
                    with pytest.raises(ProviderError) as caught:
                        if streaming:
                            _ = [
                                chunk
                                async for chunk in provider.chat_stream_async(
                                    [Message.user("hi")], **{control: value}
                                )
                            ]
                        else:
                            await provider.chat_async([Message.user("hi")], **{control: value})
                    assert getattr(caught.value, "code", None) == expected_code
                assert requests == [] and provider.outstanding == 0

    asyncio.run(run())


@pytest.mark.parametrize("case", ["stop", "json", "schema", "tools", "tool_choice"])
def test_perplexity_native_async_unsupported_request_surface_fails_before_transport(case):
    from nxuskit.errors import ProviderError
    from nxuskit.types import ResponseFormat

    async def run():
        async with endpoint("literal", b"{}") as (url, _, _, requests):
            async with Provider.create_async(
                "perplexity",
                "m",
                api_key="loopback",
                api_url=url,
                use_ffi=False,
            ) as provider:
                kwargs = {
                    "stop": {"stop": ["END"]},
                    "json": {"response_format": ResponseFormat.JSON},
                    "schema": {
                        "response_format": {
                            "type": "json_schema",
                            "schema": {"type": "object"},
                        }
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
                for streaming in (False, True):
                    with pytest.raises(ProviderError) as caught:
                        if streaming:
                            _ = [
                                chunk
                                async for chunk in provider.chat_stream_async(
                                    [Message.user("hi")], **kwargs
                                )
                            ]
                        else:
                            await provider.chat_async([Message.user("hi")], **kwargs)
                    assert getattr(caught.value, "code", None) == "unsupported_capability"
                assert requests == [] and provider.outstanding == 0

    asyncio.run(run())


@pytest.mark.parametrize("source_type", ["url", "base64", "filepath"])
def test_perplexity_native_async_images_fail_before_transport(source_type):
    from nxuskit.errors import ProviderError

    async def run():
        async with endpoint("literal", b"{}") as (url, _, _, requests):
            async with Provider.create_async(
                "perplexity",
                "m",
                api_key="loopback",
                api_url=url,
                use_ffi=False,
            ) as provider:
                message = Message.user("hi")
                if source_type == "url":
                    message.with_image_url("https://image.invalid/p.png")
                elif source_type == "base64":
                    message.with_image_base64("aGVsbG8=")
                else:
                    message.with_image_file("/definitely/not/read/perplexity.png")
                for streaming in (False, True):
                    with pytest.raises(
                        ProviderError, match="unsupported request control: images"
                    ) as caught:
                        if streaming:
                            _ = [chunk async for chunk in provider.chat_stream_async([message])]
                        else:
                            await provider.chat_async([message])
                    assert getattr(caught.value, "code", None) == "unsupported_capability"
                assert requests == [] and provider.outstanding == 0

    asyncio.run(run())


@pytest.mark.parametrize("cancel_pending", [False, True])
def test_ollama_native_async_close_and_cancel_release_socket(cancel_pending):
    async def run():
        async with endpoint("ollama_stream") as (url, _, disconnected, _):
            async with Provider.create_async(
                "ollama", "m", api_url=url.removesuffix("/v1"), use_ffi=False
            ) as provider:
                stream = provider.chat_stream_async([Message.user("hi")])
                assert (await asyncio.wait_for(anext(stream), 1)).delta == "hello"
                if cancel_pending:
                    pending = asyncio.create_task(anext(stream))
                    await asyncio.sleep(0)
                    pending.cancel()
                    with pytest.raises(asyncio.CancelledError):
                        await pending
                else:
                    await stream.aclose()
                await asyncio.wait_for(disconnected.wait(), 1)
                assert provider.outstanding == 0
                with pytest.raises(StopAsyncIteration):
                    await anext(stream)

    asyncio.run(run())


def test_ollama_native_async_url_image_is_not_silently_dropped():
    from nxuskit.errors import ProviderError

    async def run():
        async with endpoint("literal", b'{"done":true,"message":{"content":"ok"}}') as (
            url,
            _,
            _,
            requests,
        ):
            async with Provider.create_async("ollama", "m", api_url=url, use_ffi=False) as provider:
                for streaming in (False, True):
                    messages = [
                        Message.user("hi").with_image_url("https://example.invalid/image.png")
                    ]
                    with pytest.raises(ProviderError, match="unsupported.*URL image") as caught:
                        if streaming:
                            _ = [c async for c in provider.chat_stream_async(messages)]
                        else:
                            await provider.chat_async(messages)
                    assert getattr(caught.value, "code", None) == "unsupported_capability"
                assert requests == []

    asyncio.run(run())


@pytest.mark.parametrize("profile", ["groq", "xai", "fireworks", "mistral", "together", "lmstudio"])
@pytest.mark.parametrize("streaming", [False, True])
def test_compatible_native_async_profile_wire_and_terminal(profile, streaming):
    payload = (
        b'data: {"model":"actual","choices":[{"delta":{"content":"ok"},"finish_reason":"vendor-new"}]}\n\ndata: [DONE]\n\n'
        if streaming
        else b'{"model":"actual","choices":[{"message":{"content":"ok"},"finish_reason":"vendor-new"}],"usage":{"prompt_tokens":2,"completion_tokens":3,"total_tokens":5}}'
    )

    async def run():
        async with endpoint("literal", payload) as (url, _, _, requests):
            async with Provider.create_async(
                profile, "requested", api_key="loopback", api_url=url, use_ffi=False
            ) as provider:
                assert provider.provider_name == profile
                if streaming:
                    chunks = [
                        c
                        async for c in provider.chat_stream_async(
                            [Message.user("hi")], max_tokens=7
                        )
                    ]
                    assert "".join(c.delta for c in chunks) == "ok"
                    assert sum(c.is_final() for c in chunks) == 1
                    result = chunks[-1]
                    assert result.terminal
                else:
                    result = await provider.chat_async([Message.user("hi")], max_tokens=7)
                    assert result.content == "ok"
                    assert result.provider == profile
                    assert result.usage.total_tokens == 5
                assert result.model == "actual"
                assert result.raw_finish_reason == "vendor-new" and result.finish_reason is None
                assert requests == [
                    (
                        b"POST /v1/chat/completions HTTP/1.1",
                        {
                            "model": "requested",
                            "messages": [{"role": "user", "content": "hi"}],
                            "stream": streaming,
                            "max_tokens": 7,
                        },
                    )
                ]
            assert provider.closed and provider.outstanding == 0

    asyncio.run(run())


@pytest.mark.parametrize("profile", ["groq", "xai", "fireworks", "mistral", "together", "lmstudio"])
def test_compatible_native_async_profile_error_and_close(profile):
    from nxuskit.errors import AuthenticationError

    async def run():
        async with endpoint("auth") as (url, *_):
            async with Provider.create_async(profile, "m", api_url=url, use_ffi=False) as provider:
                with pytest.raises(AuthenticationError) as caught:
                    await provider.chat_async([Message.user("hi")])
                assert caught.value.provider == profile and caught.value.status_code == 401
                assert provider.outstanding == 0
        async with endpoint("stream") as (url, _, disconnected, _):
            async with Provider.create_async(profile, "m", api_url=url, use_ffi=False) as provider:
                stream = provider.chat_stream_async([Message.user("hi")])
                assert (await asyncio.wait_for(anext(stream), 1)).delta == "hello"
                assert provider.outstanding == 1
                await stream.aclose()
                await asyncio.wait_for(disconnected.wait(), 1)
                assert provider.outstanding == 0
            assert provider.closed

    asyncio.run(run())


@pytest.mark.parametrize("profile", ["openrouter", "perplexity"])
@pytest.mark.parametrize("streaming", [False, True])
def test_selected_native_async_profile_metadata(profile, streaming):
    expected = (
        {"id": None, "provider": "upstream", "usage": {"cost": None}}
        if profile == "openrouter"
        else {"citations": ["https://example.org/evidence"], "usage": {"cost": None}}
    )
    if streaming:
        payload = (
            b'data: {"model":"actual","choices":[{"delta":{"content":"ok"},"finish_reason":"vendor-new"}],"usage":{"prompt_tokens":7,"completion_tokens":3,"total_tokens":10}}\n\n'
            + b"data: "
            + json.dumps(expected).encode()
            + b"\n\ndata: [DONE]\n\n"
        )
    else:
        payload = json.dumps(
            {
                **expected,
                "model": "actual",
                "choices": [
                    {
                        "message": {"role": "assistant", "content": "ok"},
                        "finish_reason": "vendor-new",
                    }
                ],
            }
        ).encode()

    async def run():
        async with endpoint("literal", payload) as (url, _, _, requests):
            owner = Provider.create_async(
                profile, "requested", api_key="loopback", api_url=url, use_ffi=False
            )
            try:
                assert owner.provider_name == profile
                if streaming:
                    chunks = [c async for c in owner.chat_stream_async([Message.user("hi")])]
                    assert "".join(c.delta for c in chunks) == "ok"
                    assert sum(c.is_final() for c in chunks) == 1
                    assert all(c.provider_metadata is None for c in chunks[:-1])
                    result = chunks[-1]
                    assert result.usage.prompt_tokens == 7
                    assert result.usage.completion_tokens == 3
                else:
                    result = await owner.chat_async([Message.user("hi")])
                    assert result.content == "ok"
                    assert result.provider == profile
                assert result.model == "actual"
                assert result.raw_finish_reason == "vendor-new"
                assert result.finish_reason is None
                assert result.provider_metadata == expected
                assert len(requests) == 1
            finally:
                await owner.close_async()
            assert owner.closed and owner.outstanding == 0

    asyncio.run(run())


@pytest.mark.parametrize("profile", ["openrouter", "perplexity"])
def test_selected_native_async_metadata_limit_has_no_success(profile):
    from nxuskit.errors import ProviderError

    extra = {"id": "x" * 65537} if profile == "openrouter" else {"citations": ["x" * 65537]}
    payload = (
        b'data: {"choices":[{"delta":{"content":"partial"},"finish_reason":null}]}\n\n'
        + b"data: "
        + json.dumps(extra).encode()
        + b"\n\ndata: [DONE]\n\n"
    )

    async def run():
        async with endpoint("literal", payload) as (url, _, _, _):
            owner = Provider.create_async(
                profile, "m", api_key="loopback", api_url=url, use_ffi=False
            )
            chunks = []
            try:
                with pytest.raises(ProviderError, match="provider_metadata") as caught:
                    async for chunk in owner.chat_stream_async([Message.user("hi")]):
                        chunks.append(chunk)
                assert not caught.value.is_retryable
                assert "".join(c.delta for c in chunks) == "partial"
                assert not any(c.is_final() for c in chunks)
                assert owner.outstanding == 0
            finally:
                await owner.close_async()

    asyncio.run(run())


def test_perplexity_native_endpoint_does_not_invent_version():
    async def run():
        payload = b'{"model":"m","choices":[{"message":{"content":"ok"},"finish_reason":"stop"}]}'
        async with endpoint("literal", payload) as (url, _, _, requests):
            base = url.removesuffix("/v1")
            owner = Provider.create_async(
                "perplexity", "m", api_key="loopback", api_url=base, use_ffi=False
            )
            try:
                assert (await owner.chat_async([Message.user("hi")])).content == "ok"
                assert requests[0][0] == b"POST /chat/completions HTTP/1.1"
            finally:
                await owner.close_async()

    asyncio.run(run())


@pytest.mark.parametrize("streaming", [False, True])
@pytest.mark.parametrize("asynchronous", [False, True])
@pytest.mark.parametrize("seed", [-1, -(2**63), 2**63 - 1, None, 7])
def test_native_sampling_schema_wire_parity(streaming, asynchronous, seed):
    async def run():
        payload = (
            b'data: {"choices":[{"delta":{"content":"ok"},"finish_reason":"stop"}]}\n\ndata: [DONE]\n\n'
            if streaming
            else b'{"model":"actual","choices":[{"message":{"content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1}}'
        )
        async with endpoint("literal", payload) as (url, _, _, requests):
            options = dict(
                stop=["END"],
                presence_penalty=0.5,
                frequency_penalty=-0.5,
                seed=seed,
                logprobs=True,
                top_logprobs=3,
                reasoning_effort="low",
                response_format={
                    "type": "json_schema",
                    "schema": {
                        "type": "object",
                        "properties": {"ok": {"type": "boolean"}},
                        "required": ["ok"],
                        "additionalProperties": False,
                    },
                },
            )
            if asynchronous:
                owner = Provider.create_async(
                    "openai", "gpt-4o", api_key="loopback", api_url=url, use_ffi=False
                )
                try:
                    if streaming:
                        chunks = [
                            c
                            async for c in owner.chat_stream_async([Message.user("hi")], **options)
                        ]
                        assert "".join(c.delta for c in chunks) == "ok"
                        assert sum(c.is_final() for c in chunks) == 1
                    else:
                        assert (
                            await owner.chat_async([Message.user("hi")], **options)
                        ).content == "ok"
                finally:
                    await owner.close_async()
            else:
                owner = Provider.openai(
                    model="gpt-4o", api_key="loopback", api_url=url, use_ffi=False
                )
                if streaming:
                    chunks = await asyncio.to_thread(
                        lambda: list(owner.chat_stream([Message.user("hi")], **options))
                    )
                    assert "".join(c.delta for c in chunks) == "ok"
                    assert sum(c.is_final() for c in chunks) == 1
                else:
                    assert (
                        await asyncio.to_thread(owner.chat, [Message.user("hi")], **options)
                    ).content == "ok"
            assert len(requests) == 1
            wire = requests[0][1]
            assert wire["stop"] == ["END"]
            assert wire["presence_penalty"] == 0.5
            assert wire["frequency_penalty"] == -0.5
            if seed is None:
                assert "seed" not in wire
            else:
                assert wire["seed"] == seed
            assert wire["logprobs"] is True
            assert wire["top_logprobs"] == 3
            assert wire["reasoning_effort"] == "low"
            assert wire["response_format"] == {
                "type": "json_schema",
                "json_schema": {
                    "name": "response",
                    "strict": True,
                    "schema": {
                        "type": "object",
                        "properties": {"ok": {"type": "boolean"}},
                        "required": ["ok"],
                        "additionalProperties": False,
                    },
                },
            }

    asyncio.run(run())


@pytest.mark.parametrize("asynchronous", [False, True])
@pytest.mark.parametrize("streaming", [False, True])
def test_reasoning_conflict_rejects_before_native_transport(asynchronous, streaming):
    from nxuskit.errors import ProviderError

    async def run():
        async with endpoint("literal", b"{}") as (url, _, _, requests):
            owner = (
                Provider.create_async(
                    "openai", "gpt-5.4", api_key="loopback", api_url=url, use_ffi=False
                )
                if asynchronous
                else Provider.openai(
                    model="gpt-5.4", api_key="loopback", api_url=url, use_ffi=False
                )
            )
            try:
                with pytest.raises(ProviderError, match="unsupported") as caught:
                    if asynchronous:
                        if streaming:
                            _ = [
                                c
                                async for c in owner.chat_stream_async(
                                    [Message.user("hi")], reasoning_effort="high", logprobs=True
                                )
                            ]
                        else:
                            await owner.chat_async(
                                [Message.user("hi")], reasoning_effort="high", logprobs=True
                            )
                    elif streaming:
                        await asyncio.to_thread(
                            lambda: list(
                                owner.chat_stream(
                                    [Message.user("hi")], reasoning_effort="high", logprobs=True
                                )
                            )
                        )
                    else:
                        await asyncio.to_thread(
                            owner.chat, [Message.user("hi")], reasoning_effort="high", logprobs=True
                        )
                assert getattr(caught.value, "code", None) == "unsupported_capability"
            finally:
                if asynchronous:
                    await owner.close_async()
            assert requests == []

    asyncio.run(run())


@asynccontextmanager
async def endpoint(mode, payload=None):
    entered, disconnected = asyncio.Event(), asyncio.Event()
    requests = []
    handlers = set()

    async def handle(reader, writer):
        handlers.add(asyncio.current_task())
        try:
            headers = await reader.readuntil(b"\r\n\r\n")
            size = next(
                int(line.split(b":", 1)[1])
                for line in headers.split(b"\r\n")
                if line.lower().startswith(b"content-length:")
            )
            requests.append((headers.split(b"\r\n")[0], json.loads(await reader.readexactly(size))))
            entered.set()
            if mode == "literal":
                body = payload
                writer.write(
                    b"HTTP/1.1 200 OK\r\nContent-Length: "
                    + str(len(body)).encode()
                    + b"\r\nConnection: close\r\n\r\n"
                    + body
                )
                await writer.drain()
                return
            if mode == "ollama_stream":
                writer.write(
                    b'HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nConnection: close\r\n\r\n{"message":{"content":"hello"},"done":false}\n'
                )
                await writer.drain()
            elif mode in ("stream", "complete", "malformed"):
                writer.write(
                    b'HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: {"choices":[{"delta":{"content":"hello"},"finish_reason":null}]}\n\n'
                )
                if mode == "complete":
                    writer.write(
                        b'data: {"choices":[{"delta":{},"finish_reason":"stop"}]}\n\ndata: [DONE]\n\n'
                    )
                elif mode == "malformed":
                    writer.write(b"data: not-json\n\n")
                await writer.drain()
                if mode != "stream":
                    return
            elif mode == "auth":
                writer.write(
                    b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                )
                await writer.drain()
                return
            elif mode == "unary":
                body = b'{"model":"local","choices":[{"message":{"content":"literal answer"},"finish_reason":"stop"}],"usage":{"prompt_tokens":2,"completion_tokens":3,"total_tokens":5}}'
                writer.write(
                    b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: "
                    + str(len(body)).encode()
                    + b"\r\nConnection: close\r\n\r\n"
                    + body
                )
                await writer.drain()
                return
            await reader.read()
            disconnected.set()
        finally:
            writer.close()
            await writer.wait_closed()
            handlers.discard(asyncio.current_task())

    server = await asyncio.start_server(handle, "127.0.0.1", 0)
    try:
        yield (
            f"http://127.0.0.1:{server.sockets[0].getsockname()[1]}/v1",
            entered,
            disconnected,
            requests,
        )
    finally:
        server.close()
        await server.wait_closed()
        for task in list(handlers):
            task.cancel()
        await asyncio.gather(*handlers, return_exceptions=True)


def owner(url):
    return Provider.create_async(
        "openai",
        "local",
        api_key="literal-local-only",
        api_url=url,
        timeout=2,
        max_in_flight=1,
        use_ffi=False,
    )


@pytest.mark.parametrize(
    "reason,canonical", [("length", "length"), ("vendor-new", None), (None, None)]
)
def test_async_protocol_terminal_preserves_raw_reason_and_late_usage(reason, canonical):
    async def run():
        body = (
            'data: {"model":"actual","choices":[{"delta":{"content":"ok"},"finish_reason":'
            + json.dumps(reason)
            + '}]}\n\ndata: {"choices":[],"usage":{"prompt_tokens":7,"completion_tokens":3,"total_tokens":10}}\n\ndata: [DONE]\n\ndata: invalid\n\n'
        ).encode()
        async with endpoint("literal", body) as (url, *_):
            async with owner(url) as provider:
                chunks = [chunk async for chunk in provider.chat_stream_async([Message.user("hi")])]
                assert "".join(chunk.delta for chunk in chunks) == "ok"
                assert sum(chunk.is_final() for chunk in chunks) == 1
                final = chunks[-1]
                assert (
                    final.terminal,
                    final.finish_reason,
                    final.raw_finish_reason,
                    final.model,
                ) == (True, canonical, reason, "actual")
                assert (final.usage.prompt_tokens, final.usage.completion_tokens) == (7, 3)

    asyncio.run(run())


@pytest.mark.parametrize("streaming", [False, True])
def test_native_cancellation_during_headers_closes_socket(streaming):
    async def run():
        async with endpoint("headers") as (url, entered, disconnected, _):
            async with owner(url) as provider:
                stream = provider.chat_stream_async([Message.user("hello")]) if streaming else None
                task = asyncio.create_task(
                    anext(stream) if streaming else provider.chat_async([Message.user("hello")])
                )
                await asyncio.wait_for(entered.wait(), 1)
                await asyncio.sleep(0)  # loop remains live while headers are held
                assert not task.done() and provider.outstanding == 1
                task.cancel()
                with pytest.raises(asyncio.CancelledError):
                    await task
                await asyncio.wait_for(disconnected.wait(), 1)
                assert provider.outstanding == 0

    asyncio.run(run())


def test_native_buffered_stream_cannot_outlive_deadline():
    from nxuskit.errors import TimeoutError

    async def run():
        async with endpoint("complete") as (url, _, _, _):
            async with owner(url) as provider:
                stream = provider.chat_stream_async([Message.user("hello")])
                assert (await anext(stream)).delta == "hello"
                # The peer has sent the finish event in the same write; the next
                # pull can complete without yielding to the timeout callback.
                await asyncio.sleep(
                    max(0, stream._deadline - asyncio.get_running_loop().time()) + 0.01
                )
                with pytest.raises(TimeoutError):
                    await anext(stream)
                assert provider.outstanding == 0
                with pytest.raises(StopAsyncIteration):
                    await anext(stream)

    asyncio.run(run())


def test_native_tool_only_stream_preserves_each_literal_delta():
    body = (
        b'data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_a","type":"function","function":{"name":"lookup","arguments":"{\\"x\\":"}}]},"finish_reason":null}]}\n\n'
        b'data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"1}"}}]},"finish_reason":null}]}\n\n'
        b'data: {"choices":[{"delta":{},"finish_reason":"tool_calls"}]}\n\ndata: [DONE]\n\n'
    )

    async def run():
        async with endpoint("literal", body) as (url, _, _, _):
            async with owner(url) as provider:
                chunks = [c async for c in provider.chat_stream_async([Message.user("hello")])]
                assert len(chunks) == 3
                assert chunks[0].tool_calls == [
                    {
                        "index": 0,
                        "id": "call_a",
                        "type": "function",
                        "function": {"name": "lookup", "arguments": '{"x":'},
                    }
                ]
                assert chunks[1].tool_calls == [{"index": 0, "function": {"arguments": "1}"}}]
                assert chunks[2].finish_reason == "tool_calls"
                assert provider.outstanding == 0

    asyncio.run(run())


@pytest.mark.parametrize(
    "payload",
    [
        b"{}",
        b'{"error":{"message":"no"}}',
        b"null",
        b"not-json",
        b'{"choices":{}}',
        b'{"choices":[{"message":null}]}',
        b'{"choices":[{"message":{"content":42}}]}',
        b'{"choices":[{"message":{"content":"ok"}}],"usage":[]}',
        b'{"choices":[{"message":{"content":"ok"}}],"usage":null}',
    ],
)
def test_native_malformed_unary_is_typed_terminal(payload):
    from nxuskit.errors import ProviderError

    async def run():
        async with endpoint("literal", payload) as (url, _, _, _):
            async with owner(url) as provider:
                with pytest.raises(ProviderError):
                    await provider.chat_async([Message.user("hello")])
                assert provider.outstanding == 0

    asyncio.run(run())


@pytest.mark.parametrize(
    "payload",
    [
        b"{}",
        b"null",
        b'{"choices":{}}',
        b'{"choices":[{"delta":null}]}',
        b'{"choices":[{"delta":{"content":42}}]}',
        b'{"choices":[{"delta":{"tool_calls":{}}}]}',
    ],
)
def test_native_malformed_stream_is_typed_terminal(payload):
    from nxuskit.errors import ProviderError

    async def run():
        async with endpoint("literal", b"data: " + payload + b"\n\ndata: [DONE]\n\n") as (
            url,
            _,
            _,
            _,
        ):
            async with owner(url) as provider:
                stream = provider.chat_stream_async([Message.user("hello")])
                with pytest.raises(ProviderError):
                    await anext(stream)
                assert provider.outstanding == 0
                with pytest.raises(StopAsyncIteration):
                    await anext(stream)

    asyncio.run(run())


def test_native_cancelled_close_waiter_does_not_cancel_owned_cleanup():
    async def run():
        async with endpoint("headers") as (url, entered, disconnected, _):
            provider = owner(url)
            task = asyncio.create_task(provider.chat_async([Message.user("hello")]))
            await asyncio.wait_for(entered.wait(), 1)
            with pytest.raises(RuntimeError, match="in-flight"):
                await provider.chat_async([Message.user("not dispatched")])
            closing = asyncio.create_task(provider.close_async())
            await asyncio.sleep(0)
            closing.cancel()
            try:
                await closing
            except asyncio.CancelledError:
                pass
            await asyncio.wait_for(provider.close_async(), 1)
            with pytest.raises(asyncio.CancelledError):
                await task
            await asyncio.wait_for(disconnected.wait(), 1)
            assert provider.closed and provider.outstanding == 0
            with pytest.raises(RuntimeError, match="closed"):
                await provider.chat_async([Message.user("not dispatched")])

    asyncio.run(run())


def test_native_stream_done_and_malformed_terminal_are_distinct():
    from nxuskit.errors import ProviderError

    async def run():
        async with endpoint("complete") as (url, _, _, _):
            async with owner(url) as provider:
                stream = provider.chat_stream_async([Message.user("hello")])
                chunks = [chunk async for chunk in stream]
                assert [chunk.delta for chunk in chunks] == ["hello", ""]
                assert chunks[-1].finish_reason == "stop"
                assert provider.outstanding == 0
                with pytest.raises(StopAsyncIteration):
                    await anext(stream)
        async with endpoint("malformed") as (url, _, _, _):
            async with owner(url) as provider:
                stream = provider.chat_stream_async([Message.user("hello")])
                assert (await anext(stream)).delta == "hello"
                with pytest.raises(ProviderError, match="invalid stream JSON"):
                    await anext(stream)
                assert provider.outstanding == 0
                with pytest.raises(StopAsyncIteration):
                    await anext(stream)

    asyncio.run(run())


def test_native_auth_and_deadline_errors_preserve_type():
    from nxuskit.errors import AuthenticationError, TimeoutError

    async def run():
        async with endpoint("auth") as (url, _, _, _):
            async with owner(url) as provider:
                with pytest.raises(AuthenticationError) as error:
                    await provider.chat_async([Message.user("hello")])
                assert error.value.status_code == 401
        async with endpoint("headers") as (url, _, disconnected, _):
            async with Provider.create_async(
                "openai", "local", api_url=url, use_ffi=False, timeout=0.05
            ) as provider:
                with pytest.raises(TimeoutError):
                    await provider.chat_async([Message.user("hello")])
                await asyncio.wait_for(disconnected.wait(), 1)
                assert provider.outstanding == 0

    asyncio.run(run())


def test_native_profile_scope_rejects_without_ffi_fallback():
    with pytest.raises(ValueError, match="only openai"):
        Provider.create_async("unqualified", "local", use_ffi=False)


def test_native_stream_early_close_closes_socket_and_releases_slot():
    async def run():
        async with endpoint("stream") as (url, _, disconnected, _):
            async with owner(url) as provider:
                stream = provider.chat_stream_async([Message.user("hello")])
                assert (await asyncio.wait_for(anext(stream), 1)).delta == "hello"
                assert provider.outstanding == 1
                await asyncio.wait_for(stream.aclose(), 1)
                await asyncio.wait_for(disconnected.wait(), 1)
                assert provider.outstanding == 0

    asyncio.run(run())


def test_native_literal_request_response_and_local_identity():
    async def run():
        async with endpoint("unary") as (url, _, _, requests):
            async with owner(url) as provider:
                result = await provider.chat_async([Message.user("question")], max_tokens=7)
                assert result.content == "literal answer"
                assert result.model == "local" and result.usage.total_tokens == 5
                assert provider.provider_name == "openai" and provider.model == "local"
                assert requests == [
                    (
                        b"POST /v1/chat/completions HTTP/1.1",
                        {
                            "model": "local",
                            "messages": [{"role": "user", "content": "question"}],
                            "stream": False,
                            "max_tokens": 7,
                        },
                    )
                ]
            assert provider.closed

    asyncio.run(run())


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
def test_fireworks_native_async_bounds_and_advanced_controls_fail_before_transport(
    controls, expected_code
):
    from nxuskit.errors import ProviderError

    async def run():
        async with endpoint("literal", b"{}") as (url, _, _, requests):
            async with Provider.create_async(
                "fireworks",
                "m",
                api_key="fixture",
                api_url=url,
                use_ffi=False,
            ) as provider:
                for streaming in (False, True):
                    with pytest.raises(ProviderError) as caught:
                        if streaming:
                            _ = [
                                chunk
                                async for chunk in provider.chat_stream_async(
                                    [Message.user("hi")], **controls
                                )
                            ]
                        else:
                            await provider.chat_async([Message.user("hi")], **controls)
                    assert getattr(caught.value, "code", None) == expected_code
                assert requests == [] and provider.outstanding == 0

    asyncio.run(run())


@pytest.mark.parametrize(
    "controls,expected_code",
    [
        ({"stop": ["a", "b", "c", "d", "e"]}, "invalid_request"),
        ({"logprobs": True}, "unsupported_capability"),
    ],
)
def test_lmstudio_native_async_bounds_and_advanced_controls_fail_before_transport(
    controls, expected_code
):
    from nxuskit.errors import ProviderError

    async def run():
        async with endpoint("literal", b"{}") as (url, _, _, requests):
            async with Provider.create_async(
                "lmstudio",
                "m",
                api_url=url,
                use_ffi=False,
            ) as provider:
                for streaming in (False, True):
                    with pytest.raises(ProviderError) as caught:
                        if streaming:
                            _ = [
                                chunk
                                async for chunk in provider.chat_stream_async(
                                    [Message.user("hi")], **controls
                                )
                            ]
                        else:
                            await provider.chat_async([Message.user("hi")], **controls)
                    assert getattr(caught.value, "code", None) == expected_code
                assert requests == [] and provider.outstanding == 0

    asyncio.run(run())


def test_lmstudio_native_async_schema_and_unary_tools_wire_and_stream_tools_fail_closed():
    from nxuskit.errors import ProviderError

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
    payload = (
        b'{"model":"actual","choices":[{"message":{"content":"ok"},'
        b'"finish_reason":"stop"}],"usage":{"prompt_tokens":2,'
        b'"completion_tokens":3,"total_tokens":5}}'
    )

    async def run():
        async with endpoint("literal", payload) as (url, _, _, requests):
            async with Provider.create_async(
                "lmstudio",
                "requested",
                api_url=url,
                use_ffi=False,
            ) as provider:
                result = await provider.chat_async(
                    [Message.user("Look up A-1.")],
                    response_format={"type": "json_schema", "schema": schema},
                    tools=[tool],
                    tool_choice=choice,
                )
                assert result.content == "ok"
                assert requests == [
                    (
                        b"POST /v1/chat/completions HTTP/1.1",
                        {
                            "model": "requested",
                            "messages": [{"role": "user", "content": "Look up A-1."}],
                            "stream": False,
                            "response_format": {
                                "type": "json_schema",
                                "json_schema": {
                                    "name": "response",
                                    "strict": True,
                                    "schema": schema,
                                },
                            },
                            "tools": [tool.to_dict()],
                            "tool_choice": choice,
                        },
                    )
                ]

                for kwargs in ({"tools": [tool]}, {"tool_choice": choice}):
                    with pytest.raises(
                        ProviderError, match="unsupported LM Studio tool streaming"
                    ) as caught:
                        _ = [
                            chunk
                            async for chunk in provider.chat_stream_async(
                                [Message.user("stream")], **kwargs
                            )
                        ]
                    assert getattr(caught.value, "code", None) == "unsupported_capability"

                assert len(requests) == 1
                assert provider.outstanding == 0

    asyncio.run(run())


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
def test_groq_native_async_bounds_and_advanced_controls_fail_before_transport(
    controls, expected_code
):
    from nxuskit.errors import ProviderError

    async def run():
        async with endpoint("literal", b"{}") as (url, _, _, requests):
            async with Provider.create_async(
                "groq",
                "m",
                api_key="fixture",
                api_url=url,
                use_ffi=False,
            ) as provider:
                for streaming in (False, True):
                    with pytest.raises(ProviderError) as caught:
                        if streaming:
                            _ = [
                                chunk
                                async for chunk in provider.chat_stream_async(
                                    [Message.user("hi")], **controls
                                )
                            ]
                        else:
                            await provider.chat_async([Message.user("hi")], **controls)
                    assert getattr(caught.value, "code", None) == expected_code
                assert requests == [] and provider.outstanding == 0

    asyncio.run(run())


def test_groq_native_async_images_and_tool_streaming_fail_before_transport():
    from nxuskit.errors import ProviderError

    async def run():
        async with endpoint("literal", b"{}") as (url, _, _, requests):
            async with Provider.create_async(
                "groq",
                "m",
                api_key="fixture",
                api_url=url,
                use_ffi=False,
            ) as provider:
                message = Message.user("describe").with_image_url(
                    "https://example.invalid/image.png"
                )
                for streaming in (False, True):
                    with pytest.raises(
                        ProviderError, match="unsupported request control: images"
                    ) as caught:
                        if streaming:
                            _ = [chunk async for chunk in provider.chat_stream_async([message])]
                        else:
                            await provider.chat_async([message])
                    assert getattr(caught.value, "code", None) == "unsupported_capability"

                tool = ToolDefinition.create("lookup", parameters={"type": "object"})
                choice = {"type": "function", "function": {"name": "lookup"}}
                for kwargs in ({"tools": [tool]}, {"tool_choice": choice}):
                    with pytest.raises(
                        ProviderError, match="unsupported Groq tool streaming"
                    ) as caught:
                        _ = [
                            chunk
                            async for chunk in provider.chat_stream_async(
                                [Message.user("stream")], **kwargs
                            )
                        ]
                    assert getattr(caught.value, "code", None) == "unsupported_capability"

                assert requests == [] and provider.outstanding == 0

    asyncio.run(run())


def test_groq_native_async_schema_and_unary_tools_wire():
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
    payload = (
        b'{"model":"actual","choices":[{"message":{"content":"ok"},'
        b'"finish_reason":"stop"}],"usage":{"prompt_tokens":2,'
        b'"completion_tokens":3,"total_tokens":5}}'
    )

    async def run():
        async with endpoint("literal", payload) as (url, _, _, requests):
            async with Provider.create_async(
                "groq",
                "requested",
                api_key="fixture",
                api_url=url,
                use_ffi=False,
            ) as provider:
                result = await provider.chat_async(
                    [Message.user("Look up A-1.")],
                    response_format={"type": "json_schema", "schema": schema},
                    tools=[tool],
                    tool_choice=choice,
                )
                assert result.content == "ok"
                assert requests == [
                    (
                        b"POST /v1/chat/completions HTTP/1.1",
                        {
                            "model": "requested",
                            "messages": [{"role": "user", "content": "Look up A-1."}],
                            "stream": False,
                            "response_format": {
                                "type": "json_schema",
                                "json_schema": {
                                    "name": "response",
                                    "strict": True,
                                    "schema": schema,
                                },
                            },
                            "tools": [tool.to_dict()],
                            "tool_choice": choice,
                        },
                    )
                ]
                assert provider.outstanding == 0

    asyncio.run(run())


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
def test_mistral_native_async_bounds_and_advanced_controls_fail_before_transport(
    controls, expected_code
):
    from nxuskit.errors import ProviderError

    async def run():
        async with endpoint("literal", b"{}") as (url, _, _, requests):
            async with Provider.create_async(
                "mistral",
                "m",
                api_key="fixture",
                api_url=url,
                use_ffi=False,
            ) as provider:
                for streaming in (False, True):
                    with pytest.raises(ProviderError) as caught:
                        if streaming:
                            _ = [
                                chunk
                                async for chunk in provider.chat_stream_async(
                                    [Message.user("hi")], **controls
                                )
                            ]
                        else:
                            await provider.chat_async([Message.user("hi")], **controls)
                    assert getattr(caught.value, "code", None) == expected_code
                assert requests == [] and provider.outstanding == 0

    asyncio.run(run())


def test_mistral_native_async_tool_streaming_fails_before_transport():
    from nxuskit.errors import ProviderError

    async def run():
        async with endpoint("literal", b"{}") as (url, _, _, requests):
            async with Provider.create_async(
                "mistral",
                "m",
                api_key="fixture",
                api_url=url,
                use_ffi=False,
            ) as provider:
                tool = ToolDefinition.create("lookup", parameters={"type": "object"})
                choice = {"type": "function", "function": {"name": "lookup"}}
                for kwargs in ({"tools": [tool]}, {"tool_choice": choice}):
                    with pytest.raises(
                        ProviderError, match="unsupported Mistral tool streaming"
                    ) as caught:
                        _ = [
                            chunk
                            async for chunk in provider.chat_stream_async(
                                [Message.user("stream")], **kwargs
                            )
                        ]
                    assert getattr(caught.value, "code", None) == "unsupported_capability"
                assert requests == [] and provider.outstanding == 0

    asyncio.run(run())


def test_mistral_native_async_schema_and_unary_tools_wire():
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
    payload = (
        b'{"model":"actual","choices":[{"message":{"content":"ok"},'
        b'"finish_reason":"stop"}],"usage":{"prompt_tokens":2,'
        b'"completion_tokens":3,"total_tokens":5}}'
    )

    async def run():
        async with endpoint("literal", payload) as (url, _, _, requests):
            async with Provider.create_async(
                "mistral",
                "requested",
                api_key="fixture",
                api_url=url,
                use_ffi=False,
            ) as provider:
                result = await provider.chat_async(
                    [Message.user("Look up A-1.")],
                    response_format={"type": "json_schema", "schema": schema},
                    tools=[tool],
                    tool_choice="auto",
                )
                assert result.content == "ok"
                assert requests == [
                    (
                        b"POST /v1/chat/completions HTTP/1.1",
                        {
                            "model": "requested",
                            "messages": [{"role": "user", "content": "Look up A-1."}],
                            "stream": False,
                            "response_format": {
                                "type": "json_schema",
                                "json_schema": {
                                    "name": "response",
                                    "strict": True,
                                    "schema": schema,
                                },
                            },
                            "tools": [tool.to_dict()],
                            "tool_choice": "auto",
                        },
                    )
                ]
                assert provider.outstanding == 0

    asyncio.run(run())


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
def test_together_native_async_bounds_and_advanced_controls_fail_before_transport(
    controls, expected_code
):
    from nxuskit.errors import ProviderError

    async def run():
        async with endpoint("literal", b"{}") as (url, _, _, requests):
            async with Provider.create_async(
                "together",
                "m",
                api_key="fixture",
                api_url=url,
                use_ffi=False,
            ) as provider:
                for streaming in (False, True):
                    with pytest.raises(ProviderError) as caught:
                        if streaming:
                            _ = [
                                chunk
                                async for chunk in provider.chat_stream_async(
                                    [Message.user("hi")], **controls
                                )
                            ]
                        else:
                            await provider.chat_async([Message.user("hi")], **controls)
                    assert getattr(caught.value, "code", None) == expected_code
                assert requests == [] and provider.outstanding == 0

    asyncio.run(run())


def test_together_native_async_tool_streaming_fails_before_transport():
    from nxuskit.errors import ProviderError

    async def run():
        async with endpoint("literal", b"{}") as (url, _, _, requests):
            async with Provider.create_async(
                "together",
                "m",
                api_key="fixture",
                api_url=url,
                use_ffi=False,
            ) as provider:
                tool = ToolDefinition.create("lookup", parameters={"type": "object"})
                for kwargs in ({"tools": [tool]}, {"tool_choice": "required"}):
                    with pytest.raises(
                        ProviderError, match="unsupported Together tool streaming"
                    ) as caught:
                        _ = [
                            chunk
                            async for chunk in provider.chat_stream_async(
                                [Message.user("stream")], **kwargs
                            )
                        ]
                    assert getattr(caught.value, "code", None) == "unsupported_capability"
                assert requests == [] and provider.outstanding == 0

    asyncio.run(run())


def test_together_native_async_schema_and_unary_tools_wire():
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
    payload = (
        b'{"model":"actual","choices":[{"message":{"content":"ok"},'
        b'"finish_reason":"stop"}],"usage":{"prompt_tokens":2,'
        b'"completion_tokens":3,"total_tokens":5}}'
    )

    async def run():
        async with endpoint("literal", payload) as (url, _, _, requests):
            async with Provider.create_async(
                "together",
                "requested",
                api_key="fixture",
                api_url=url,
                use_ffi=False,
            ) as provider:
                result = await provider.chat_async(
                    [Message.user("Look up A-1.")],
                    response_format={"type": "json_schema", "schema": schema},
                    tools=[tool],
                    tool_choice="required",
                )
                assert result.content == "ok"
                assert requests == [
                    (
                        b"POST /v1/chat/completions HTTP/1.1",
                        {
                            "model": "requested",
                            "messages": [{"role": "user", "content": "Look up A-1."}],
                            "stream": False,
                            "response_format": {
                                "type": "json_schema",
                                "json_schema": {
                                    "name": "response",
                                    "strict": True,
                                    "schema": schema,
                                },
                            },
                            "tools": [tool.to_dict()],
                            "tool_choice": "required",
                        },
                    )
                ]
                assert provider.outstanding == 0

    asyncio.run(run())


@pytest.mark.parametrize(
    "tool_choice,expected_code",
    [
        ({"type": "function", "function": {"name": "lookup"}}, "unsupported_capability"),
        ({"type": "function", "function": {"name": ""}}, "invalid_request"),
        ("bogus", "invalid_request"),
    ],
)
def test_together_native_async_named_or_invalid_tool_choice_fails_before_transport(
    tool_choice, expected_code
):
    from nxuskit.errors import ProviderError

    async def run():
        async with endpoint("literal", b"{}") as (url, _, _, requests):
            async with Provider.create_async(
                "together",
                "m",
                api_key="fixture",
                api_url=url,
                use_ffi=False,
            ) as provider:
                with pytest.raises(ProviderError) as caught:
                    await provider.chat_async([Message.user("hi")], tool_choice=tool_choice)
                assert getattr(caught.value, "code", None) == expected_code
                assert requests == [] and provider.outstanding == 0

    asyncio.run(run())


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
def test_openrouter_native_async_bounds_and_advanced_controls_fail_before_transport(
    controls, expected_code
):
    from nxuskit.errors import ProviderError

    async def run():
        async with endpoint("literal", b"{}") as (url, _, _, requests):
            async with Provider.create_async(
                "openrouter",
                "m",
                api_key="fixture",
                api_url=url,
                use_ffi=False,
            ) as provider:
                for streaming in (False, True):
                    with pytest.raises(ProviderError) as caught:
                        if streaming:
                            _ = [
                                chunk
                                async for chunk in provider.chat_stream_async(
                                    [Message.user("hi")], **controls
                                )
                            ]
                        else:
                            await provider.chat_async([Message.user("hi")], **controls)
                    assert getattr(caught.value, "code", None) == expected_code
                assert requests == [] and provider.outstanding == 0

    asyncio.run(run())


def test_openrouter_native_async_tool_streaming_fails_before_transport():
    from nxuskit.errors import ProviderError

    async def run():
        async with endpoint("literal", b"{}") as (url, _, _, requests):
            async with Provider.create_async(
                "openrouter",
                "m",
                api_key="fixture",
                api_url=url,
                use_ffi=False,
            ) as provider:
                tool = ToolDefinition.create("lookup", parameters={"type": "object"})
                for kwargs in ({"tools": [tool]}, {"tool_choice": "auto"}):
                    with pytest.raises(
                        ProviderError, match="unsupported OpenRouter tool streaming"
                    ) as caught:
                        _ = [
                            chunk
                            async for chunk in provider.chat_stream_async(
                                [Message.user("stream")], **kwargs
                            )
                        ]
                    assert getattr(caught.value, "code", None) == "unsupported_capability"
                assert requests == [] and provider.outstanding == 0

    asyncio.run(run())


def test_openrouter_native_async_schema_and_named_unary_tool_wire():
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
    payload = (
        b'{"id":"gen-1","provider":"upstream","model":"actual",'
        b'"choices":[{"message":{"content":"ok"},"finish_reason":"stop"}],'
        b'"usage":{"prompt_tokens":2,"completion_tokens":3,"total_tokens":5,"cost":0.001}}'
    )

    async def run():
        async with endpoint("literal", payload) as (url, _, _, requests):
            async with Provider.create_async(
                "openrouter",
                "requested",
                api_key="fixture",
                api_url=url,
                use_ffi=False,
            ) as provider:
                result = await provider.chat_async(
                    [Message.user("Look up A-1.")],
                    response_format={"type": "json_schema", "schema": schema},
                    tools=[tool],
                    tool_choice=choice,
                )
                assert result.content == "ok"
                assert requests == [
                    (
                        b"POST /v1/chat/completions HTTP/1.1",
                        {
                            "model": "requested",
                            "messages": [{"role": "user", "content": "Look up A-1."}],
                            "stream": False,
                            "response_format": {
                                "type": "json_schema",
                                "json_schema": {
                                    "name": "response",
                                    "strict": True,
                                    "schema": schema,
                                },
                            },
                            "tools": [tool.to_dict()],
                            "tool_choice": choice,
                        },
                    )
                ]
                assert result.provider_metadata["provider"] == "upstream"
                assert provider.outstanding == 0

    asyncio.run(run())


def test_empty_tools_three_state_native_async_r47b():
    from nxuskit.errors import ProviderError

    async def run():
        tool = ToolDefinition.create("lookup", parameters={"type": "object"})
        for profile in ("claude", "groq"):
            async with Provider.create_async(
                profile, "m", api_key="fixture", use_ffi=False
            ) as provider:
                omitted, _ = provider._request([Message.user("hi")], None, True)
                empty, _ = provider._request([Message.user("hi")], None, True, tools=[])
                assert "tools" not in omitted
                assert "tools" not in empty
                with pytest.raises(ProviderError) as caught:
                    provider._request([Message.user("hi")], None, True, tools=[tool])
                assert getattr(caught.value, "code", None) == "unsupported_capability"

    asyncio.run(run())


def test_resource_bound_native_async_unary_one_byte_over():
    from nxuskit.errors import ProviderError
    from nxuskit.providers._resource_bounds import MAX_UNARY_RESPONSE_BODY_BYTES

    async def run():
        body = b"x" * (MAX_UNARY_RESPONSE_BODY_BYTES + 1)
        async with endpoint("literal", body) as (url, *_):
            async with Provider.create_async(
                "openai", "m", api_key="loopback", api_url=url, use_ffi=False
            ) as provider:
                with pytest.raises(ProviderError) as caught:
                    await provider.chat_async([])
                assert getattr(caught.value, "code", None) == "malformed_response"
                assert provider.outstanding == 0

    asyncio.run(run())


def test_resource_bound_native_async_sse_event_one_byte_over():
    from nxuskit.errors import ProviderError
    from nxuskit.providers._resource_bounds import MAX_STREAM_RECORD_EVENT_BYTES

    async def run():
        body = b"data: " + b"x" * MAX_STREAM_RECORD_EVENT_BYTES + b"\n\n"
        async with endpoint("literal", body) as (url, *_):
            async with Provider.create_async(
                "openai", "m", api_key="loopback", api_url=url, use_ffi=False
            ) as provider:
                with pytest.raises(ProviderError) as caught:
                    _ = [chunk async for chunk in provider.chat_stream_async([])]
                assert getattr(caught.value, "code", None) == "malformed_response"
                assert provider.outstanding == 0

    asyncio.run(run())


def test_resource_bound_native_async_ndjson_record_one_byte_over():
    from nxuskit.errors import ProviderError
    from nxuskit.providers._resource_bounds import MAX_STREAM_RECORD_EVENT_BYTES

    async def run():
        body = b"x" * (MAX_STREAM_RECORD_EVENT_BYTES + 1) + b"\n"
        async with endpoint("literal", body) as (url, *_):
            async with Provider.create_async(
                "ollama", "m", api_url=url.removesuffix("/v1"), use_ffi=False
            ) as provider:
                with pytest.raises(ProviderError) as caught:
                    _ = [chunk async for chunk in provider.chat_stream_async([])]
                assert getattr(caught.value, "code", None) == "malformed_response"
                assert provider.outstanding == 0

    asyncio.run(run())


def test_resource_bound_native_async_ollama_eof_final_record_without_newline():
    async def run():
        body = (
            b'{"model":"actual","message":{"content":"hi"},"done":false}\n'
            b'{"model":"actual","done":true}'
        )
        async with endpoint("literal", body) as (url, *_):
            async with Provider.create_async(
                "ollama", "m", api_url=url.removesuffix("/v1"), use_ffi=False
            ) as provider:
                chunks = [chunk async for chunk in provider.chat_stream_async([])]
                assert chunks[0].delta == "hi"
                assert chunks[-1].is_final()
                assert provider.outstanding == 0

    asyncio.run(run())
