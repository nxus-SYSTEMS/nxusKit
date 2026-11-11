"""Real candidate C ABI + controlled loopback HTTP lifecycle tests."""

import asyncio
import json
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import pytest

from nxuskit import Message, Provider


@pytest.mark.parametrize("use_ffi", [False, True])
@pytest.mark.parametrize("asynchronous", [False, True])
@pytest.mark.parametrize("streaming", [False, True])
@pytest.mark.parametrize("failure", ["error-null", "error-text", "incomplete", "negative-prompt", "negative-completion"])
def test_ollama_error_completion_rejects_all_public_transports(
    use_ffi, asynchronous, streaming, failure
):
    from nxuskit._ffi_errors import ProviderError as FFIProviderError
    from nxuskit.errors import ProviderError

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            self.rfile.read(int(self.headers["Content-Length"]))
            payload = {
                "model": "actual",
                "message": {"role": "assistant", "content": "partial"},
                "done": failure != "incomplete",
                "done_reason": "stop",
            }
            if failure.startswith("error"):
                payload["error"] = None if failure == "error-null" else "private upstream detail"
            if failure == "negative-prompt":
                payload.update(prompt_eval_count=-1, eval_count=1)
            if failure == "negative-completion":
                payload.update(prompt_eval_count=1, eval_count=-1)
            body = json.dumps(payload).encode() + b"\n"
            self.send_response(200)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever)
    thread.start()

    async def run():
        url = f"http://127.0.0.1:{server.server_port}"
        owner = (
            Provider.create_async("ollama", "m", api_url=url, use_ffi=use_ffi)
            if asynchronous
            else Provider.ollama(model="m", api_url=url, use_ffi=use_ffi)
        )
        chunks = []
        try:
            with pytest.raises((ProviderError, FFIProviderError)) as caught:
                if streaming:
                    if asynchronous:
                        async for chunk in owner.chat_stream_async([Message.user("hi")]):
                            chunks.append(chunk)
                    else:
                        chunks.extend(owner.chat_stream([Message.user("hi")]))
                elif asynchronous:
                    await owner.chat_async([Message.user("hi")])
                else:
                    owner.chat([Message.user("hi")])
            assert not any(chunk.is_final() for chunk in chunks)
            assert "private upstream detail" not in str(caught.value)
        finally:
            if asynchronous:
                await owner.close_async()
            elif hasattr(owner, "close"):
                owner.close()

    try:
        asyncio.run(run())
    finally:
        server.shutdown()
        server.server_close()
        thread.join()


@pytest.mark.parametrize("use_ffi", [False, True])
def test_ollama_cloud_declared_profile_reaches_async_owner_only(use_ffi):
    from nxuskit.errors import ProviderError
    from nxuskit.types import ResponseFormat

    requests = []

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            requests.append(
                (self.path, json.loads(self.rfile.read(int(self.headers["Content-Length"]))))
            )
            body = (
                b'{"model":"actual","message":{"role":"assistant","content":"ok"},'
                b'"done":true,"done_reason":"stop","prompt_eval_count":1,"eval_count":1}\n'
            )
            self.send_response(200)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever)
    thread.start()

    async def run():
        url = f"http://127.0.0.1:{server.server_port}"
        owner = Provider.create_async(
            "ollama",
            "gpt-oss:120b-cloud",
            api_url=url,
            use_ffi=use_ffi,
            profile="ollama_cloud_via_daemon",
        )
        try:
            response = await owner.chat_async([Message.user("hi")], seed=42)
            assert response.declared_profile == "ollama_cloud_via_daemon"

            chunks = [
                chunk
                async for chunk in owner.chat_stream_async(
                    [Message.user("hi")], thinking="disabled"
                )
            ]
            assert chunks and all(
                chunk.declared_profile == "ollama_cloud_via_daemon" for chunk in chunks
            )

            baseline = len(requests)
            with pytest.raises(ProviderError) as caught:
                await owner.chat_async(
                    [Message.user("hi")], response_format=ResponseFormat.JSON
                )
            assert getattr(caught.value, "code", None) == "unsupported_capability"
            assert len(requests) == baseline
        finally:
            await owner.close_async()

    try:
        asyncio.run(run())
        assert len(requests) == 2
        assert all(path == "/api/chat" for path, _ in requests)
        assert all(
            "profile" not in body and "declared_profile" not in body for _, body in requests
        )
    finally:
        server.shutdown()
        server.server_close()
        thread.join()


@pytest.mark.parametrize("use_ffi", [False, True])
@pytest.mark.parametrize("asynchronous", [False, True])
@pytest.mark.parametrize("streaming", [False, True])
@pytest.mark.parametrize("top_k,min_p", [(None, None), (1, 0.0), (2147483647, 1.0)])
def test_ollama_sampling_controls_reach_all_public_transports(
    use_ffi, asynchronous, streaming, top_k, min_p
):
    requests = []

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            requests.append(
                (self.path, json.loads(self.rfile.read(int(self.headers["Content-Length"]))))
            )
            body = b'{"model":"actual","message":{"role":"assistant","content":"ok"},"done":true,"done_reason":"vendor-new","prompt_eval_count":2,"eval_count":3}\n'
            self.send_response(200)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever)
    thread.start()

    async def run():
        url = f"http://127.0.0.1:{server.server_port}"
        owner = (
            Provider.create_async("ollama", "m", api_url=url, use_ffi=use_ffi)
            if asynchronous
            else Provider.ollama(model="m", api_url=url, use_ffi=use_ffi)
        )
        options = dict(top_k=top_k, min_p=min_p)
        try:
            if streaming:
                chunks = (
                    [c async for c in owner.chat_stream_async([Message.user("hi")], **options)]
                    if asynchronous
                    else list(owner.chat_stream([Message.user("hi")], **options))
                )
                assert "".join(c.delta for c in chunks) == "ok"
                assert sum(c.is_final() for c in chunks) == 1
                assert chunks[-1].raw_finish_reason == "vendor-new"
            else:
                response = (
                    await owner.chat_async([Message.user("hi")], **options)
                    if asynchronous
                    else owner.chat([Message.user("hi")], **options)
                )
                assert response.content == "ok" and response.model == "actual"
        finally:
            if asynchronous:
                await owner.close_async()
            elif hasattr(owner, "close"):
                owner.close()

    try:
        asyncio.run(run())
    finally:
        server.shutdown()
        server.server_close()
        thread.join()
    assert len(requests) == 1 and requests[0][0] == "/api/chat"
    wire = requests[0][1]
    assert wire["stream"] is streaming
    if top_k is None:
        assert "top_k" not in wire.get("options", {}) and "min_p" not in wire.get("options", {})
    else:
        assert wire["options"]["top_k"] == top_k and wire["options"]["min_p"] == min_p


@pytest.mark.parametrize("failure", ["timeout", "disconnect"])
@pytest.mark.parametrize("profile", ["openrouter", "mistral"])
def test_selected_sync_stream_timeout_after_first_chunk(failure, profile):
    from nxuskit._ffi_errors import TimeoutError as FFITimeoutError
    from nxuskit._ffi_errors import ProviderError as FFIProviderError

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            first = b'data: {"choices":[{"delta":{"content":"partial"},"finish_reason":null}]}\n\n'
            last = b"data: [DONE]\n\n"
            self.send_response(200)
            self.send_header("Content-Length", str(len(first) + len(last)))
            self.end_headers()
            self.wfile.write(first)
            self.wfile.flush()
            if failure == "disconnect":
                return
            threading.Event().wait(0.5)
            try:
                self.wfile.write(last)
            except (BrokenPipeError, ConnectionResetError):
                pass

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    worker = threading.Thread(target=server.serve_forever)
    worker.start()
    owner = getattr(Provider, profile)(
        model="m",
        api_key="loopback",
        api_url=f"http://127.0.0.1:{server.server_port}/v1",
        timeout=0.1,
        use_ffi=True,
    )
    chunks = []
    try:
        with pytest.raises(
            FFITimeoutError if failure == "timeout" else FFIProviderError,
            match="timed out" if failure == "timeout" else "provider_error",
        ):
            for chunk in owner.chat_stream([Message.user("hi")]):
                chunks.append(chunk)
        assert "".join(chunk.delta for chunk in chunks) == "partial"
        assert not any(chunk.is_final() for chunk in chunks)
    finally:
        owner.close()
        server.shutdown()
        server.server_close()
        worker.join()


@pytest.mark.parametrize("profile", ["groq", "fireworks", "together", "mistral"])
@pytest.mark.parametrize("streaming", [False, True])
def test_remaining_ffi_profiles_honor_endpoint(profile, streaming):
    requests = []

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            requests.append(
                (self.path, json.loads(self.rfile.read(int(self.headers["Content-Length"]))))
            )
            body = (
                b'data: {"choices":[{"delta":{"content":"ok"},"finish_reason":"stop"}]}\n\ndata: [DONE]\n\n'
                if streaming
                else b'{"id":"fixture","model":"actual","choices":[{"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}'
            )
            self.send_response(200)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    worker = threading.Thread(target=server.serve_forever)
    worker.start()

    async def run():
        owner = Provider.create_async(
            profile,
            "m",
            api_key="loopback",
            api_url=f"http://127.0.0.1:{server.server_port}/v1",
            timeout=1,
        )
        try:
            if streaming:
                chunks = [c async for c in owner.chat_stream_async([Message.user("hi")])]
                assert "".join(c.delta for c in chunks) == "ok"
                assert sum(c.is_final() for c in chunks) == 1
            else:
                response = await owner.chat_async([Message.user("hi")])
                assert response.content == "ok"
                assert response.model == "actual"
        finally:
            await owner.close_async()

    try:
        asyncio.run(run())
        assert len(requests) == 1
        assert requests[0][0] == "/v1/chat/completions"
        assert requests[0][1]["model"] == "m"
    finally:
        server.shutdown()
        server.server_close()
        worker.join()


@pytest.mark.parametrize("streaming", [False, True])
@pytest.mark.parametrize("asynchronous", [False, True])
@pytest.mark.parametrize("profile", ["openrouter", "mistral"])
def test_selected_ffi_configured_timeout(streaming, asynchronous, profile):
    from nxuskit.errors import ProviderError, TimeoutError
    from nxuskit._ffi_errors import TimeoutError as FFITimeoutError

    requests = []

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            requests.append(self.path)
            threading.Event().wait(0.3)
            body = (
                b'data: {"choices":[{"delta":{"content":"ok"},"finish_reason":"stop"}]}\n\ndata: [DONE]\n\n'
                if streaming
                else b'{"id":"fixture","model":"m","choices":[{"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}'
            )
            try:
                self.send_response(200)
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)
            except (BrokenPipeError, ConnectionResetError):
                pass

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    worker = threading.Thread(target=server.serve_forever)
    worker.start()

    async def run():
        options = dict(
            api_key="loopback", api_url=f"http://127.0.0.1:{server.server_port}/v1", timeout=0.05
        )
        if asynchronous:
            owner = Provider.create_async(profile, "m", **options)
            try:
                with pytest.raises(
                    (ProviderError, TimeoutError), match="(?i)timeout|timed out|deadline"
                ):
                    if streaming:
                        _ = [c async for c in owner.chat_stream_async([Message.user("hi")])]
                    else:
                        await owner.chat_async([Message.user("hi")])
            finally:
                await owner.close_async()
        else:
            owner = getattr(Provider, profile)(model="m", use_ffi=True, **options)
            try:
                with pytest.raises(FFITimeoutError, match="(?i)timeout|timed out|deadline"):
                    if streaming:
                        list(owner.chat_stream([Message.user("hi")]))
                    else:
                        owner.chat([Message.user("hi")])
            finally:
                owner.close()

    try:
        asyncio.run(run())
        assert requests == ["/v1/chat/completions"]
    finally:
        server.shutdown()
        server.server_close()
        worker.join()


@pytest.mark.parametrize("streaming", [False, True])
@pytest.mark.parametrize("profile", ["openai", "ollama"])
@pytest.mark.parametrize("seed", [-1, -(2**63), 2**63 - 1, None, 7])
def test_async_controls_reach_real_native_wire(streaming, profile, seed):
    requests = []

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            assert self.path == ("/v1/chat/completions" if profile == "openai" else "/api/chat")
            requests.append(json.loads(self.rfile.read(int(self.headers["Content-Length"]))))
            body = (
                b'data: {"choices":[{"delta":{"content":"ok"},"finish_reason":"stop"}]}\n\ndata: [DONE]\n\n'
                if streaming
                else b'{"model":"actual","choices":[{"message":{"content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1}}'
            )
            if profile == "ollama":
                body = b'{"model":"actual","message":{"role":"assistant","content":"ok","thinking":"hmm"},"done":true,"done_reason":"vendor-new","prompt_eval_count":7,"eval_count":3}\n'
            self.send_response(200)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever)
    thread.start()

    async def run():
        owner = Provider.create_async(
            profile,
            "gpt-4o" if profile == "openai" else "qwen3:8b",
            api_key="loopback",
            api_url=f"http://127.0.0.1:{server.server_port}"
            + ("/v1" if profile == "openai" else ""),
        )
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
        if profile == "ollama":
            options = {
                "response_format": options["response_format"],
                "thinking": "disabled",
                "stop": ["END"],
                "seed": seed,
            }
        try:
            if streaming:
                chunks = [
                    chunk
                    async for chunk in owner.chat_stream_async([Message.user("hi")], **options)
                ]
                assert "".join(chunk.delta for chunk in chunks) == "ok"
                assert sum(chunk.is_final() for chunk in chunks) == 1
                if profile == "ollama":
                    assert (
                        chunks[-1].finish_reason,
                        chunks[-1].raw_finish_reason,
                        chunks[-1].terminal,
                    ) == (None, "vendor-new", True)
            else:
                assert (await owner.chat_async([Message.user("hi")], **options)).content == "ok"
        finally:
            await owner.close_async()

    try:
        asyncio.run(run())
    finally:
        server.shutdown()
        server.server_close()
        thread.join()
    assert len(requests) == 1
    wire = requests[0]
    if profile == "ollama":
        assert wire["think"] is False
        assert wire["options"]["stop"] == ["END"]
        if seed is None:
            assert "seed" not in wire["options"]
        else:
            assert wire["options"]["seed"] == seed
        assert wire["format"] == {
            "type": "object",
            "properties": {"ok": {"type": "boolean"}},
            "required": ["ok"],
            "additionalProperties": False,
        }
        assert "response_format" not in wire
        return
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


@pytest.fixture
def endpoint():
    entered = threading.Event()
    release = threading.Event()

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            assert self.path == "/v1/chat/completions"
            request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            entered.set()
            if request["messages"][0]["content"] != [{"type": "text", "text": "fast"}]:
                release.wait(3)
            body = b'{"id":"local","object":"chat.completion","created":0,"model":"local","choices":[{"index":0,"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}'
            try:
                self.send_response(200)
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)
            except (BrokenPipeError, ConnectionResetError):
                pass

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever)
    thread.start()
    yield f"http://127.0.0.1:{server.server_port}/v1", entered, release
    release.set()
    server.shutdown()
    server.server_close()
    thread.join()


@pytest.mark.parametrize("profile", ["openrouter", "perplexity"])
@pytest.mark.parametrize("streaming", [False, True])
@pytest.mark.parametrize("asynchronous", [False, True])
def test_selected_profile_ffi_endpoint_and_metadata(profile, streaming, asynchronous, monkeypatch):
    """A rejecting loopback proxy prevents a default-endpoint regression going live."""
    requests, blocked = [], []
    expected = (
        {"id": None, "provider": "upstream", "usage": {"cost": None}}
        if profile == "openrouter"
        else {"citations": ["https://example.org/evidence"], "usage": {"cost": None}}
    )

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_CONNECT(self):
            blocked.append(self.path)
            self.send_error(503, "test denies non-loopback provider routing")

        def do_POST(self):
            assert self.path == "/v1/chat/completions"
            requests.append(json.loads(self.rfile.read(int(self.headers["Content-Length"]))))
            if streaming:
                body = (
                    "data: "
                    + json.dumps(
                        {"choices": [{"delta": {"content": "ok"}, "finish_reason": "vendor-new"}]}
                    )
                    + "\n\ndata: "
                    + json.dumps(expected)
                    + "\n\ndata: [DONE]\n\n"
                ).encode()
            else:
                body = json.dumps(
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
            self.send_response(200)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever)
    thread.start()
    base = f"http://127.0.0.1:{server.server_port}"
    for name in (
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
    ):
        monkeypatch.setenv(name, base)
    for name in ("NO_PROXY", "no_proxy"):
        monkeypatch.setenv(name, "127.0.0.1,localhost")

    def check(result):
        if streaming:
            assert "".join(chunk.delta for chunk in result) == "ok"
            assert sum(chunk.is_final() for chunk in result) == 1
            assert all(chunk.provider_metadata is None for chunk in result[:-1])
            result = result[-1]
            assert result.terminal
        else:
            assert result.content == "ok"
            assert result.model == "actual"
        assert result.provider_metadata == expected
        assert result.finish_reason is None
        assert result.raw_finish_reason == "vendor-new"

    async def run():
        owner = Provider.create_async(
            profile, "model", api_key="loopback", api_url=base + "/v1", timeout=2
        )
        try:
            check(
                [c async for c in owner.chat_stream_async([Message.user("hi")])]
                if streaming
                else await owner.chat_async([Message.user("hi")])
            )
        finally:
            await owner.close_async()

    try:
        if asynchronous:
            asyncio.run(run())
        else:
            with getattr(Provider, profile)(
                model="model", api_key="loopback", api_url=base + "/v1", timeout=2, use_ffi=True
            ) as owner:
                check(
                    list(owner.chat_stream([Message.user("hi")]))
                    if streaming
                    else owner.chat([Message.user("hi")])
                )
    finally:
        server.shutdown()
        server.server_close()
        thread.join()
    assert blocked == []
    assert len(requests) == 1
    assert requests[0]["model"] == "model"


@pytest.mark.parametrize("streaming", [False, True])
def test_reasoning_conflict_rejects_before_ffi_transport(endpoint, streaming):
    from nxuskit.errors import ProviderError

    async def run():
        owner = Provider.create_async("openai", "gpt-5.4", api_key="loopback", api_url=endpoint[0])
        try:
            with pytest.raises(ProviderError, match="unsupported"):
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
            assert not endpoint[1].is_set()
        finally:
            await owner.close_async()

    asyncio.run(run())


def make_owner(endpoint):
    return Provider.create_async(
        "openai",
        "local",
        api_key="local-test-only",
        api_url=endpoint[0],
        timeout=1,
        max_in_flight=1,
    )


def test_cancelled_waiter_remains_owned_until_native_quiescence(endpoint):
    async def run():
        owner = make_owner(endpoint)
        task = asyncio.create_task(owner.chat_async([Message.user("hello")]))
        assert await asyncio.to_thread(endpoint[1].wait, 2)
        with pytest.raises(RuntimeError, match="in-flight"):
            await owner.chat_async([Message.user("not dispatched")])
        task.cancel()
        with pytest.raises(asyncio.CancelledError):
            await task
        await asyncio.wait_for(owner.close_async(), 2)
        assert owner.closed and owner.outstanding == 0
        with pytest.raises(RuntimeError, match="closed"):
            await owner.chat_async([Message.user("not dispatched")])

    asyncio.run(run())


def test_cancelled_close_waiter_does_not_cancel_cleanup(endpoint):
    async def run():
        owner = make_owner(endpoint)
        task = asyncio.create_task(owner.chat_async([Message.user("hello")]))
        assert await asyncio.to_thread(endpoint[1].wait, 2)
        close = asyncio.create_task(owner.close_async())
        await asyncio.sleep(0)  # schedule close, not a timing assumption
        close.cancel()
        try:
            await close
        except asyncio.CancelledError:
            pass
        await asyncio.wait_for(owner.close_async(), 2)
        with pytest.raises(asyncio.CancelledError):
            await task
        assert owner.closed and owner.outstanding == 0

    asyncio.run(run())


def test_loop_closure_does_not_orphan_native_callback(endpoint):
    loop = asyncio.new_event_loop()
    owner = make_owner(endpoint)
    task = loop.create_task(owner.chat_async([Message.user("hello")]))
    loop.run_until_complete(asyncio.to_thread(endpoint[1].wait, 2))
    task.cancel()
    with pytest.raises(asyncio.CancelledError):
        loop.run_until_complete(task)
    loop.close()
    # Native worker lifetime, not loop lifetime, settles ownership.
    owner.close()
    assert owner.closed and owner.outstanding == 0


def test_success_is_not_silent_cancellation(endpoint):
    async def run():
        owner = make_owner(endpoint)
        endpoint[2].set()
        response = await owner.chat_async([Message.user("hello")])
        assert response.content == "ok"
        assert response.model == "local" and response.finish_reason == "stop"
        assert response.usage.prompt_tokens == 1 and response.usage.completion_tokens == 1
        await owner.close_async()
        assert owner.closed and owner.outstanding == 0

    asyncio.run(run())


def test_cancel_one_request_does_not_cancel_shared_provider_other_request(endpoint):
    async def run():
        owner = Provider.create_async(
            "openai",
            "local",
            api_key="local-test-only",
            api_url=endpoint[0],
            timeout=2,
            max_in_flight=2,
        )
        slow = asyncio.create_task(owner.chat_async([Message.user("hello")]))
        assert await asyncio.to_thread(endpoint[1].wait, 2)
        fast = asyncio.create_task(owner.chat_async([Message.user("fast")]))
        slow.cancel()
        with pytest.raises(asyncio.CancelledError):
            await slow
        assert (await fast).content == "ok"
        await owner.close_async()

    asyncio.run(run())


def test_real_callback_borrow_survives_cancelled_close_and_closed_loop(endpoint, monkeypatch):
    from nxuskit import _ffi_async_provider as module

    entered = threading.Event()
    release = threading.Event()
    counts = {
        "nxuskit_free_provider": 0,
        "nxuskit_free_response": 0,
        "nxuskit_chat_operation_free": 0,
    }
    real_lib = module.lib
    real_convert = module.chat_response_from_ffi

    class CountingNativeBoundary:
        def __getattr__(self, name):
            fn = getattr(real_lib, name)
            if name not in counts:
                return fn

            def call(*args):
                result = fn(*args)
                counts[name] += 1
                return result

            return call

    def held_conversion(value):
        entered.set()
        assert release.wait(3)
        return real_convert(value)

    monkeypatch.setattr(module, "lib", CountingNativeBoundary())
    monkeypatch.setattr(module, "chat_response_from_ffi", held_conversion)
    endpoint[2].set()
    owner = make_owner(endpoint)
    loop = asyncio.new_event_loop()
    task = loop.create_task(owner.chat_async([Message.user("hello")]))
    assert loop.run_until_complete(asyncio.to_thread(entered.wait, 2))
    close = loop.create_task(owner.close_async())
    loop.run_until_complete(asyncio.sleep(0))
    close.cancel()
    task.cancel()
    for pending in (close, task):
        with pytest.raises(asyncio.CancelledError):
            loop.run_until_complete(pending)
    assert owner.outstanding == 1 and not owner.closed
    assert counts == {name: 0 for name in counts}
    loop.close()
    release.set()
    owner.close()
    assert owner.outstanding == 0 and owner.closed
    assert counts == {name: 1 for name in counts}


def test_deadline_is_distinct_and_never_started_cancel_sends_nothing(endpoint):
    from nxuskit.errors import TimeoutError

    async def run():
        owner = Provider.create_async(
            "openai", "local", api_key="local-test-only", api_url=endpoint[0], timeout=0.15
        )
        task = asyncio.create_task(owner.chat_async([Message.user("hello")]))
        task.cancel()
        with pytest.raises(asyncio.CancelledError):
            await task
        assert owner.outstanding == 0 and not endpoint[1].is_set()
        with pytest.raises(TimeoutError, match="deadline"):
            await owner.chat_async([Message.user("hello")])
        await owner.close_async()

    asyncio.run(run())


def test_older_library_rejected_before_provider_allocation(endpoint, monkeypatch):
    from nxuskit import _ffi_async_provider as module

    native = module.lib
    allocated = []

    class OldLibrary:
        def __getattr__(self, name):
            if name.startswith("nxuskit_chat_operation_"):
                raise AttributeError(name)
            if name == "nxuskit_create_provider":

                def create(*args):
                    handle = native.nxuskit_create_provider(*args)
                    allocated.append(handle)
                    return handle

                return create
            return getattr(native, name)

    monkeypatch.setattr(module, "lib", OldLibrary())
    try:
        with pytest.raises(RuntimeError, match="operation ABI"):
            make_owner(endpoint)
        assert not allocated
    finally:
        for handle in allocated:
            native.nxuskit_free_provider(handle)


def test_native_scheduling_exception_releases_slot_and_reaches_waiter(endpoint, monkeypatch):
    from nxuskit import _ffi_async_provider as module

    native = module.lib

    class BrokenSchedule:
        def __getattr__(self, name):
            if name == "nxuskit_chat_operation_start":

                def fail(*_):
                    raise RuntimeError("schedule failure")

                return fail
            return getattr(native, name)

    monkeypatch.setattr(module, "lib", BrokenSchedule())

    async def run():
        owner = make_owner(endpoint)
        try:
            with pytest.raises(RuntimeError, match="schedule failure"):
                await asyncio.wait_for(owner.chat_async([Message.user("hello")]), 0.5)
            assert owner.outstanding == 0
        finally:
            await owner.close_async()

    asyncio.run(run())
