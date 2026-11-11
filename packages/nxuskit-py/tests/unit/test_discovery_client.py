"""Real loopback discovery, with no inference provider or selected model."""

import asyncio
import json
import threading
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import pytest

from nxuskit.discovery import DiscoveryClientFactory, DiscoveryConfig, DiscoveryError


@contextmanager
def endpoint(body, status=200, held=False):
    entered, disconnected = threading.Event(), threading.Event()
    paths = []

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_GET(self):
            paths.append(self.path)
            entered.set()
            if held:
                self.connection.settimeout(3)
                assert self.connection.recv(1) == b""
                disconnected.set()
                return
            payload = json.dumps(body).encode()
            self.send_response(status)
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield f"http://127.0.0.1:{server.server_port}/api/v1", paths, entered, disconnected
    finally:
        server.shutdown()
        server.server_close()
        thread.join()


def config(url):
    return DiscoveryConfig("lmstudio", url, connect_timeout_ms=1000, read_timeout_ms=1000)


def test_discovery_sync_and_async_share_typed_snapshot_without_provider(monkeypatch):
    from nxuskit import Provider

    def forbidden(*args, **kwargs):
        raise AssertionError("inference construction")

    monkeypatch.setattr(Provider, "lmstudio", forbidden)
    monkeypatch.setattr(Provider, "create_async", forbidden)
    body = {
        "models": [
            {
                "key": "chat-a",
                "type": "llm",
                "publisher": "fixture",
                "display_name": "Chat",
                "size_bytes": 12,
                "max_context_length": 4096,
                "loaded_instances": [{"id": "run-a", "config": {"context_length": 2048}}],
            }
        ]
    }
    with endpoint(body) as (url, paths, _, _):
        with DiscoveryClientFactory.create(config(url)) as client:
            assert client.health() is None
            assert client.list_models()[0].name == "chat-a"
            first = client.snapshot()
            assert first.load_state == "loaded"
            assert first.model_states[0].loaded_instances[0].id == "run-a"
            assert first.model_states[0].loaded_instances[0].context_length == 2048
            assert first.models[0].context_window == 4096
            assert first.api.minimum_server_version == "0.4.0"
            assert first.api.reported_server_version is None
            assert asyncio.run(client.snapshot_async()) == first
        with pytest.raises(DiscoveryError, match="closed"):
            client.health()
        assert paths == ["/api/v1/models"] * 4


@pytest.mark.parametrize("instances,state", [(None, "unknown"), ([], "not_loaded")])
def test_catalog_is_not_loaded_evidence(instances, state):
    body = {
        "models": [
            {
                "key": "downloaded",
                "type": "embedding",
                "publisher": "fixture",
                "display_name": "Downloaded",
                "size_bytes": 1,
                "max_context_length": 512,
                "loaded_instances": instances,
            }
        ]
    }
    with endpoint(body) as (url, _, _, _):
        with DiscoveryClientFactory.create(config(url)) as client:
            assert client.snapshot().load_state == state


@pytest.mark.parametrize(
    "body,status,code",
    [
        ({"models": []}, 401, "authentication"),
        ({"models": []}, 503, "unavailable"),
        ({"data": []}, 200, "malformed_response"),
    ],
)
def test_discovery_errors_are_typed_and_do_not_fallback(body, status, code):
    with endpoint(body, status) as (url, paths, _, _):
        with DiscoveryClientFactory.create(config(url)) as client:
            with pytest.raises(DiscoveryError) as caught:
                client.snapshot()
            assert caught.value.code == code
        assert paths == ["/api/v1/models"]


def test_async_cancel_releases_socket_and_owner():
    with endpoint({}, held=True) as (url, paths, entered, disconnected):

        async def run():
            async with await DiscoveryClientFactory.create_async(config(url)) as client:
                task = asyncio.create_task(client.snapshot_async())
                assert await asyncio.to_thread(entered.wait, 2)
                assert client.outstanding == 1
                task.cancel()
                with pytest.raises(asyncio.CancelledError):
                    await task
                assert await asyncio.to_thread(disconnected.wait, 2)
            assert client.closed and client.outstanding == 0

        asyncio.run(run())
        assert paths == ["/api/v1/models"]


def test_worker_start_failure_does_not_strand_native_operation(monkeypatch):
    async def run():
        client = DiscoveryClientFactory.create(config("http://127.0.0.1:1/api/v1"))
        native_starts = []
        original_start = client._start

        def record_start(kind):
            native_starts.append(kind)
            return original_start(kind)

        monkeypatch.setattr(client, "_start", record_start)

        def fail(_):
            raise RuntimeError("literal worker launch failure")

        with monkeypatch.context() as patch:
            patch.setattr(threading.Thread, "start", fail)
            with pytest.raises(RuntimeError, match="worker launch"):
                await client.snapshot_async()
        assert native_starts == [], "failed worker launch must precede native ownership"
        assert client.outstanding == 0
        await client.close_async()
        assert client.closed

    asyncio.run(run())


def test_unreachable_is_not_an_empty_or_unloaded_catalog():
    import socket

    with socket.socket() as unavailable:
        unavailable.bind(("127.0.0.1", 0))
        url = f"http://127.0.0.1:{unavailable.getsockname()[1]}/api/v1"
        # No daemon. OS routing may reject immediately or time out; neither
        # transport outcome may be fabricated into an empty/unloaded catalog.
        with DiscoveryClientFactory.create(config(url)) as client:
            with pytest.raises(DiscoveryError) as caught:
                client.snapshot()
            assert caught.value.code in {"network", "timeout"}


def test_held_headers_timeout_does_not_claim_unloaded():
    with endpoint({}, held=True) as (url, _, _, disconnected):
        cfg = DiscoveryConfig("lmstudio", url, connect_timeout_ms=50, read_timeout_ms=50)
        with DiscoveryClientFactory.create(cfg) as client:
            with pytest.raises(DiscoveryError) as caught:
                client.snapshot()
            assert caught.value.code == "timeout"
            assert disconnected.wait(2)
