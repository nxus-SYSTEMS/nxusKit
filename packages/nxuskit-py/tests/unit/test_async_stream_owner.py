"""Owned stream behavior against the real native ABI; no provider service."""

import asyncio
import queue
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import pytest

from nxuskit import Message, Provider


def test_stream_delivers_literal_chunks_and_releases_admission():
    async def run():
        async with Provider.create_async("loopback", "echo", max_in_flight=1) as owner:
            chunks = [chunk async for chunk in owner.chat_stream_async([Message.user("one two")])]
            assert chunks
            assert "".join(chunk.delta for chunk in chunks) == "one two"
            assert owner.outstanding == 0

    asyncio.run(run())


@pytest.fixture
def held_endpoint():
    entered, release = threading.Event(), threading.Event()

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            self.rfile.read(int(self.headers["Content-Length"]))
            entered.set()
            release.wait(3)
            body = b'data: {"choices":[{"delta":{"content":"literal"},"finish_reason":null}]}\n\ndata: [DONE]\n\n'
            try:
                self.send_response(200)
                self.send_header("Content-Type", "text/event-stream")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)
            except (BrokenPipeError, ConnectionResetError):
                pass

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever)
    thread.start()
    yield f"http://127.0.0.1:{server.server_port}/v1", entered
    release.set()
    server.shutdown()
    server.server_close()
    thread.join()


def http_owner(endpoint):
    return Provider.create_async(
        "openai",
        "local",
        api_key="local-test-only",
        api_url=endpoint[0],
        timeout=2,
        max_in_flight=1,
    )


def test_cancel_during_establishment_keeps_loop_live_and_drains(held_endpoint):
    async def run():
        async with http_owner(held_endpoint) as owner:
            stream = owner.chat_stream_async([Message.user("hello")])
            task = asyncio.create_task(anext(stream))
            assert await asyncio.to_thread(held_endpoint[1].wait, 1)
            ticks = 0
            for _ in range(10):
                await asyncio.sleep(0)
                ticks += 1
            assert ticks == 10 and not task.done()
            with pytest.raises(RuntimeError, match="in-flight"):
                await owner.chat_async([Message.user("not dispatched")])
            with pytest.raises(RuntimeError, match="one stream anext"):
                await anext(stream)
            task.cancel()
            with pytest.raises(asyncio.CancelledError):
                await task
            await asyncio.wait_for(stream.aclose(), 1)
            assert owner.outstanding == 0

    asyncio.run(run())


def test_cancelled_close_waiter_leaves_stream_cleanup_owned(held_endpoint):
    async def run():
        owner = http_owner(held_endpoint)
        stream = owner.chat_stream_async([Message.user("hello")])
        task = asyncio.create_task(anext(stream))
        assert await asyncio.to_thread(held_endpoint[1].wait, 1)
        closing = asyncio.create_task(owner.close_async())
        await asyncio.sleep(0)
        closing.cancel()
        try:
            await closing
        except asyncio.CancelledError:
            pass
        await asyncio.wait_for(owner.close_async(), 1)
        with pytest.raises(asyncio.CancelledError):
            await task
        assert owner.closed and owner.outstanding == 0

    asyncio.run(run())


def test_loop_close_alone_drains_idle_stream_without_explicit_cancel():
    loop = asyncio.new_event_loop()
    owner = Provider.create_async("loopback", "echo", timeout=10)
    stream = owner.chat_stream_async([Message.user("word " * 100)])
    try:
        assert loop.run_until_complete(anext(stream)).delta
        assert owner.outstanding == 1 and not owner.closed
        assert not stream._cancel.is_set()
        loop.close()  # the sole cause: no task.cancel, aclose, or owner.close
        assert owner._drained.wait(1), "loop closure alone must release native ownership"
        assert owner.closed and owner.outstanding == 0
        assert stream._done.wait(1)
    finally:
        # Cleanup after the assertions cannot satisfy the measured obligation.
        if not loop.is_closed():
            loop.close()
        owner.close()


def test_callback_waits_for_consumption_and_idle_consumer_blocks_conversion(monkeypatch):
    from nxuskit import _ffi_async_provider as bridge

    real_lib, real_convert = bridge.lib, bridge.stream_chunk_from_ffi
    entered, returned, converted, deliveries = (queue.Queue() for _ in range(4))
    wait_observed, idle_observed = threading.Event(), threading.Event()
    callbacks = []  # retain the observed C callback until native wait/free

    class ObservedLibrary:
        def __getattr__(self, name):
            return getattr(real_lib, name)

        def nxuskit_stream_operation_start(self, provider, request, deadline, chunk, done, data):
            @bridge.ffi.callback("int32_t(const char *, void *)", error=1)
            def observed(raw, userdata):
                entered.put(True)
                result = chunk(raw, userdata)  # execute the real Python bridge
                returned.put(True)
                return result

            callbacks.append(observed)
            return real_lib.nxuskit_stream_operation_start(
                provider, request, deadline, observed, done, data
            )

    def convert(raw):
        converted.put(raw["index"])
        return real_convert(raw)

    monkeypatch.setattr(bridge, "lib", ObservedLibrary())
    monkeypatch.setattr(bridge, "stream_chunk_from_ffi", convert)
    loop = asyncio.new_event_loop()
    owner = Provider.create_async("loopback", "echo", timeout=10)
    stream = owner.chat_stream_async([Message.user("one two three")])
    schedule = loop.call_soon_threadsafe

    def intercept(callback, *args, **kwargs):
        if callback.__name__ != "deliver":
            return schedule(callback, *args, **kwargs)
        ack = stream._demand[1]
        real_wait = ack.wait

        def observe_wait(timeout=None):
            result = real_wait(timeout)
            if not result:
                wait_observed.set()  # real wait timed out with no consumption ack
            return result

        ack.wait = observe_wait
        deliveries.put((callback, args))  # hold scheduling, not the callback body
        return None

    monkeypatch.setattr(loop, "call_soon_threadsafe", intercept)
    real_condition_wait = stream._condition.wait

    def observe_idle(timeout=None):
        if stream._demand is None:
            idle_observed.set()
        return real_condition_wait(timeout)

    monkeypatch.setattr(stream._condition, "wait", observe_idle)
    task = loop.create_task(anext(stream))
    try:
        loop.run_until_complete(asyncio.sleep(0))
        callback, args = deliveries.get(timeout=1)
        assert entered.get(timeout=1) is True
        assert converted.get(timeout=1) == 0
        assert wait_observed.wait(1)
        assert returned.empty() and not task.done()
        wait_observed.clear()
        callback(*args)  # Future is resolved, but the stopped loop cannot consume it
        assert stream._demand[0].done() and not task.done()
        assert not stream._demand[1].is_set()
        assert wait_observed.wait(1)
        assert returned.empty(), "scheduling/resolution is not consumption acknowledgement"
        assert loop.run_until_complete(task).delta == "one two three"
        assert returned.get(timeout=1) is True
        assert entered.get(timeout=1) is True  # the next real callback has entered
        assert idle_observed.wait(1)
        assert converted.empty() and deliveries.empty() and returned.empty()
        loop.run_until_complete(stream.aclose())
        assert returned.get(timeout=1) is True
        assert owner.outstanding == 0
    finally:
        monkeypatch.setattr(loop, "call_soon_threadsafe", schedule)
        loop.run_until_complete(owner.close_async())
        if not task.done():
            task.cancel()
        loop.run_until_complete(asyncio.gather(task, return_exceptions=True))
        loop.close()


def test_provider_error_is_not_success_and_terminal_is_consumed_once():
    from nxuskit.errors import ProviderError

    async def run():
        async with Provider.create_async("loopback", "u-turn-error-auth") as owner:
            stream = owner.chat_stream_async([Message.user("literal")])
            with pytest.raises(ProviderError, match="(?i)authentication"):
                await anext(stream)
            with pytest.raises(StopAsyncIteration):
                await anext(stream)
            assert owner.outstanding == 0

    asyncio.run(run())


def test_pending_stream_deadline_is_typed_and_quiescent(held_endpoint):
    from nxuskit.errors import TimeoutError

    async def run():
        async with Provider.create_async(
            "openai", "local", api_key="local-test-only", api_url=held_endpoint[0], timeout=0.1
        ) as owner:
            stream = owner.chat_stream_async([Message.user("hello")])
            with pytest.raises(TimeoutError):
                await asyncio.wait_for(anext(stream), 1)
            assert owner.outstanding == 0
            with pytest.raises(StopAsyncIteration):
                await anext(stream)

    asyncio.run(run())


def test_stream_early_close_releases_held_callback_and_provider():
    async def run():
        owner = Provider.create_async("loopback", "echo", max_in_flight=1)
        stream = owner.chat_stream_async([Message.user("word " * 100)])
        first = await anext(stream)
        assert first.delta
        assert owner.outstanding == 1
        await asyncio.wait_for(stream.aclose(), 2)
        assert owner.outstanding == 0
        await asyncio.wait_for(owner.close_async(), 2)
        assert owner.closed

    asyncio.run(run())
