"""Endpoint/auth-only discovery through the accepted Rust discovery client.

LM Studio endpoint: http://localhost:1234/api/v1 (native v1, baseline 0.4.0).
No endpoint probing, fallback, model loading, inference or credential prompting.
Version metadata is an API assumption, not a detected server version. Missing
loaded_instances is unknown; catalog membership is never load evidence.
"""

from __future__ import annotations

import asyncio
import json
import threading
from dataclasses import asdict, dataclass
from typing import Literal

from nxuskit.errors import ProviderError
from nxuskit.types import ModelInfo


@dataclass(frozen=True)
class DiscoveryConfig:
    provider_id: str
    endpoint: str
    auth_selector: str | None = None
    connect_timeout_ms: int = 5000
    read_timeout_ms: int = 5000


class DiscoveryError(ProviderError):
    """Canonical Rust discovery code/message, with no raw response or credential."""

    def __init__(
        self, code: str, message: str, field: str | None = None, retry_after_ms: int | None = None
    ):
        super().__init__(message)
        self.code, self.field, self.retry_after_ms = code, field, retry_after_ms


@dataclass(frozen=True)
class LoadedInstance:
    id: str
    context_length: int


@dataclass(frozen=True)
class ModelLoadInfo:
    model: str
    kind: Literal["llm", "embedding"]
    loaded_instances: tuple[LoadedInstance, ...] | None


@dataclass(frozen=True)
class DiscoveryApi:
    protocol: str
    minimum_server_version: str
    reported_server_version: str | None


@dataclass(frozen=True)
class DiscoverySnapshot:
    models: tuple[ModelInfo, ...]
    model_states: tuple[ModelLoadInfo, ...]
    load_state: Literal["unknown", "not_loaded", "loaded"]
    api: DiscoveryApi | None


def _models(values):
    return tuple(ModelInfo(**v) for v in values)


def _decode(envelope, operation):
    if not envelope["ok"]:
        raise DiscoveryError(**envelope["error"])
    value = envelope["value"]
    if operation == 0:
        return None
    if operation == 1:
        return _models(value)
    return DiscoverySnapshot(
        models=_models(value["models"]),
        model_states=tuple(
            ModelLoadInfo(
                model=s["model"],
                kind=s["kind"],
                loaded_instances=None
                if s["loaded_instances"] is None
                else tuple(LoadedInstance(**i) for i in s["loaded_instances"]),
            )
            for s in value["model_states"]
        ),
        load_state=value["load_state"],
        api=DiscoveryApi(**value["api"]) if value["api"] is not None else None,
    )


class DiscoveryClient:
    """C-backed sync/async owner, bounded to four concurrent requests.

    Cancellation requests native transport cancellation; no remote-stop claim.
    close marks closed and drains existing work, preserving Rust close semantics.
    Async wait workers retain ownership through native completion/free, even if
    the asyncio waiter or loop is abandoned. Use context management to close.
    """

    def __init__(self, config: DiscoveryConfig):
        from nxuskit._ffi import ffi, last_error, lib

        self._ffi, self._lib = ffi, lib
        self._lock = threading.Lock()
        self._operations = set()
        self._closing = False
        self._drained = threading.Event()
        self._handle = lib.nxuskit_discovery_create(json.dumps(asdict(config)).encode())
        if self._handle == ffi.NULL:
            error = json.loads(last_error() or "{}")
            raise DiscoveryError(
                error.get("error_type", "invalid_request"),
                error.get("message", "Discovery creation failed"),
            )

    @property
    def closed(self) -> bool:
        return self._drained.is_set()

    @property
    def outstanding(self) -> int:
        with self._lock:
            return len(self._operations)

    def _start(self, kind):
        with self._lock:
            if self._closing:
                raise DiscoveryError("unavailable", "Discovery client is closed")
            if len(self._operations) >= 4:
                raise DiscoveryError("unavailable", "Discovery in-flight limit reached")
            op = self._lib.nxuskit_discovery_operation_start(self._handle, kind)
            if op == self._ffi.NULL:
                raise DiscoveryError("internal", "Discovery scheduling failed")
            self._operations.add(op)
            return op

    def _free_if_drained(self):
        if self._closing and not self._operations and not self._drained.is_set():
            self._lib.nxuskit_discovery_free(self._handle)
            self._handle = self._ffi.NULL
            self._drained.set()

    def _finish(self, op, kind, cancelled=None, loop=None):
        try:
            while not self._lib.nxuskit_discovery_operation_wait(op, 20):
                if (cancelled is not None and cancelled.is_set()) or (
                    loop is not None and loop.is_closed()
                ):
                    self._lib.nxuskit_discovery_operation_cancel(op)
            raw = self._lib.nxuskit_discovery_operation_result(op)
            if raw == self._ffi.NULL:
                raise DiscoveryError("internal", "Discovery result unavailable")
            try:
                envelope = json.loads(self._ffi.string(raw))
            finally:
                self._lib.nxuskit_free_string(raw)
            return _decode(envelope, kind)
        finally:
            # A completed native operation owns no outstanding transport/callback.
            if not self._lib.nxuskit_discovery_operation_wait(op, 0):
                self._lib.nxuskit_discovery_operation_cancel(op)
                while not self._lib.nxuskit_discovery_operation_wait(op, 20):
                    pass
            self._lib.nxuskit_discovery_operation_free(op)
            with self._lock:
                self._operations.discard(op)
                self._free_if_drained()

    def _sync(self, kind):
        try:
            asyncio.get_running_loop()
        except RuntimeError:
            return self._finish(self._start(kind), kind)
        raise RuntimeError("use async discovery methods on an event-loop thread")

    async def _async(self, kind):
        loop = asyncio.get_running_loop()
        future = loop.create_future()
        cancelled = threading.Event()
        ready = threading.Event()
        op = None

        def deliver(value, error):
            if not future.done():
                if error is not None:
                    future.set_exception(error)
                else:
                    future.set_result(value)

        def worker():
            ready.wait()
            if op is None:
                return  # native scheduling failed; no operation to drain
            value = error = None
            try:
                value = self._finish(op, kind, cancelled, loop)
            except BaseException as exc:
                error = exc
            try:
                loop.call_soon_threadsafe(deliver, value, error)
            except RuntimeError:
                pass  # loop closed; native ownership already drained

        # Establish a cleanup owner before creating native work. Thread-launch
        # failure therefore needs no native wait on this event-loop thread.
        threading.Thread(target=worker, daemon=True, name="nxuskit-discovery-wait").start()
        try:
            op = self._start(kind)
        finally:
            ready.set()
        try:
            return await future
        except asyncio.CancelledError:
            cancelled.set()
            raise

    def health(self) -> None:
        return self._sync(0)

    def list_models(self) -> tuple[ModelInfo, ...]:
        return self._sync(1)

    def snapshot(self) -> DiscoverySnapshot:
        return self._sync(2)

    async def health_async(self) -> None:
        return await self._async(0)

    async def list_models_async(self) -> tuple[ModelInfo, ...]:
        return await self._async(1)

    async def snapshot_async(self) -> DiscoverySnapshot:
        return await self._async(2)

    def _begin_close(self):
        with self._lock:
            if not self._closing:
                if not self._lib.nxuskit_discovery_close(self._handle):
                    raise DiscoveryError("internal", "Discovery close failed")
                self._closing = True
            self._free_if_drained()

    def close(self) -> None:
        try:
            asyncio.get_running_loop()
        except RuntimeError:
            pass
        else:
            raise RuntimeError("use close_async on an event-loop thread")
        self._begin_close()
        self._drained.wait()

    async def close_async(self) -> None:
        self._begin_close()
        await asyncio.to_thread(self._drained.wait)

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()

    async def __aenter__(self):
        return self

    async def __aexit__(self, *_):
        await self.close_async()


class DiscoveryClientFactory:
    @staticmethod
    def create(config: DiscoveryConfig) -> DiscoveryClient:
        return DiscoveryClient(config)

    @staticmethod
    async def create_async(config: DiscoveryConfig) -> DiscoveryClient:
        # Construction validates configuration only, with no HTTP or auth resolution.
        return DiscoveryClient(config)
