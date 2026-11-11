"""Owned, bounded FFI async chat and streaming; not full native/FFI parity.

Cancellation stops the waiter and requests cancellation of SDK-owned work. It
does not acknowledge a remote service stop. Slots and native borrows remain
owned through callback return, including when the caller's loop closes.
"""

from __future__ import annotations

import asyncio
import json
import math
import threading
from typing import Any

from nxuskit._ffi import ffi, last_error, lib
from nxuskit._ffi_sync_provider import SyncFFIProvider
from nxuskit._ffi_types import chat_response_from_ffi, stream_chunk_from_ffi
from nxuskit.errors import ProviderError, RateLimitError, TimeoutError
from nxuskit.message import Message
from nxuskit.tools import ToolDefinition
from nxuskit.types import ChatResponse, ResponseFormat


def _callback_provider_error(provider_name: str, data: dict[str, Any]) -> BaseException:
    code = data.get("code", "internal_error")
    message = data.get("message", "native failure")
    if code == "timeout":
        return TimeoutError(message, provider=provider_name)
    if code == "rate_limited":
        return RateLimitError(
            message,
            status_code=429,
            provider=provider_name,
            retry_after_ms=data.get("retry_after_ms"),
        )
    error = ProviderError(message, provider=provider_name)
    error.code = code
    return error


class AsyncFFIProvider:
    """Explicit FFI-only async chat/stream/close owner.

    Construction is local and synchronous. Use async with for deterministic
    ownership; close() is the blocking counterpart for non-event-loop threads.
    """

    def capabilities(self):
        """Stable loaded-library declaration, available after close without a handle."""
        from nxuskit.provider import _library_adapter_capabilities

        if not hasattr(self, "_adapter_snapshot"):
            self._adapter_snapshot = _library_adapter_capabilities(self.provider_name)
        return self._adapter_snapshot

    async def capabilities_async(self):
        return self.capabilities()

    def __init__(
        self,
        provider_type: str,
        model: str,
        *,
        api_key: str | None = None,
        api_url: str | None = None,
        timeout: float = 30.0,
        max_in_flight: int = 4,
        declared_profile: str | None = None,
    ):
        try:
            for symbol in (
                "nxuskit_chat_operation_start",
                "nxuskit_chat_operation_cancel",
                "nxuskit_chat_operation_wait",
                "nxuskit_chat_operation_free",
            ):
                getattr(lib, symbol)
        except AttributeError as error:
            raise RuntimeError("native library lacks the required operation ABI") from error
        if not math.isfinite(timeout) or not 0.001 <= timeout <= 4294967.295:
            raise ValueError("timeout must be finite and between 1ms and u32::MAX ms")
        if type(max_in_flight) is not int or not 1 <= max_in_flight <= 64:
            raise ValueError("max_in_flight must be an integer from 1 to 64")
        if provider_type == "ollama":
            from nxuskit.providers.ollama import OllamaProvider

            OllamaProvider._validate_profile(declared_profile)
        elif declared_profile is not None:
            error = ProviderError(
                "declared profile is only supported for Ollama", provider=provider_type
            )
            error.code = "invalid_request"
            raise error
        self._model, self._provider_name = model, provider_type
        self._declared_profile = declared_profile
        self._deadline_ms = int(timeout * 1000)
        self._limit = max_in_flight
        self._lock = threading.Lock()
        self._operations: set[threading.Event] = set()
        self._closing = False
        self._drained = threading.Event()
        config: dict[str, Any] = {
            "provider_type": provider_type,
            "model": model,
            "timeout_ms": self._deadline_ms,
        }
        if api_key is not None:
            config["api_key"] = api_key
        if api_url is not None:
            config["base_url"] = api_url
        self._handle = lib.nxuskit_create_provider(json.dumps(config).encode())
        if self._handle == ffi.NULL:
            raise ProviderError(last_error() or "provider creation failed", provider=provider_type)

    @property
    def model(self) -> str:
        return self._model

    @property
    def provider_name(self) -> str:
        return self._provider_name

    @property
    def declared_profile(self) -> str | None:
        return self._declared_profile

    @property
    def closed(self) -> bool:
        return self._drained.is_set()

    @property
    def outstanding(self) -> int:
        with self._lock:
            return len(self._operations)

    async def chat_async(
        self,
        messages: list[Message],
        *,
        model: str | None = None,
        temperature: float | None = None,
        max_tokens: int | None = None,
        top_p: float | None = None,
        stop: str | list[str] | None = None,
        response_format: ResponseFormat | dict[str, Any] | None = None,
        tools: list[ToolDefinition] | None = None,
        tool_choice: Any = None,
        thinking: str | None = None,
        presence_penalty: float | None = None,
        frequency_penalty: float | None = None,
        seed: int | None = None,
        top_k: int | None = None,
        min_p: float | None = None,
        logprobs: bool | None = None,
        top_logprobs: int | None = None,
        reasoning_effort: str | None = None,
    ) -> ChatResponse:
        # File loading belongs to the existing sync surface until an explicit
        # async input-loading increment; do not hide blocking filesystem I/O.
        from nxuskit.types import ImageSourceType

        if any(
            image.source_type == ImageSourceType.FILEPATH
            for message in messages
            for image in (message.images or [])
        ):
            raise ValueError("chat_async requires in-memory or URL image inputs")
        request = SyncFFIProvider._request(
            self,
            messages,
            model,
            temperature,
            max_tokens,
            top_p,
            stop,
            response_format,
            tools,
            tool_choice,
            thinking=thinking,
            presence_penalty=presence_penalty,
            frequency_penalty=frequency_penalty,
            seed=seed,
            top_k=top_k,
            min_p=min_p,
            logprobs=logprobs,
            top_logprobs=top_logprobs,
            reasoning_effort=reasoning_effort,
        )
        encoded = json.dumps(request).encode()
        loop = asyncio.get_running_loop()
        future = loop.create_future()
        cancel = threading.Event()
        with self._lock:
            if self._closing:
                raise RuntimeError("provider is closed")
            if len(self._operations) >= self._limit:
                raise RuntimeError("provider in-flight limit reached")
            self._operations.add(cancel)
        worker = threading.Thread(
            target=self._run, args=(encoded, loop, future, cancel), name="nxuskit-chat-owner"
        )
        try:
            worker.start()
        except BaseException:
            self._settled(cancel, False)
            raise
        try:
            return await future
        except asyncio.CancelledError:
            cancel.set()
            raise

    def _run(self, encoded, loop, future, cancel):
        outcome: list[Any] = []

        @ffi.callback("void(int32_t, NxuskitResponse *, const char *, void *)")
        def complete(status, response, error, _):
            try:
                if status:
                    raw = json.loads(ffi.string(error)) if error != ffi.NULL else {}
                    code = raw.get("code", "internal_error")
                    if code == "cancelled":
                        outcome.append(asyncio.CancelledError())
                    else:
                        outcome.append(_callback_provider_error(self.provider_name, raw))
                else:
                    value = chat_response_from_ffi(
                        json.loads(ffi.string(lib.nxuskit_response_json(response)))
                    )
                    value.declared_profile = self.declared_profile
                    outcome.append(value)
            except BaseException as error:
                outcome.append(error)
            finally:
                if response != ffi.NULL:
                    lib.nxuskit_free_response(response)

        operation = ffi.NULL
        try:
            if cancel.is_set() or loop.is_closed():
                outcome.append(asyncio.CancelledError())
            else:
                operation = lib.nxuskit_chat_operation_start(
                    self._handle, encoded, self._deadline_ms, complete, ffi.NULL
                )
                if operation == ffi.NULL:
                    outcome.append(
                        ProviderError(
                            last_error() or "operation rejected", provider=self.provider_name
                        )
                    )
                else:
                    while True:
                        if cancel.is_set() or loop.is_closed():
                            lib.nxuskit_chat_operation_cancel(operation)
                        if lib.nxuskit_chat_operation_wait(operation, 20):
                            break
        except BaseException as error:
            outcome.append(error)
            if operation != ffi.NULL:
                lib.nxuskit_chat_operation_cancel(operation)
                while not lib.nxuskit_chat_operation_wait(operation, 20):
                    pass
        finally:
            if operation != ffi.NULL:
                # Only this worker accesses/frees the handle; callbacks never
                # acquire the owner lock or wait on native completion.
                if not lib.nxuskit_chat_operation_free(operation):
                    raise RuntimeError("native operation not quiescent; ownership retained")
            self._settled(cancel, loop.is_closed())
        result = outcome[0] if outcome else RuntimeError("native callback omitted terminal result")

        def publish():
            if not future.done():
                if isinstance(result, BaseException):
                    future.set_exception(result)
                else:
                    future.set_result(result)

        try:
            loop.call_soon_threadsafe(publish)
        except RuntimeError:
            if loop.is_closed():
                self._begin_close()
            else:
                raise

    def chat_stream_async(
        self,
        messages: list[Message],
        *,
        model: str | None = None,
        temperature: float | None = None,
        max_tokens: int | None = None,
        top_p: float | None = None,
        stop: str | list[str] | None = None,
        response_format: ResponseFormat | dict[str, Any] | None = None,
        tools: list[ToolDefinition] | None = None,
        tool_choice: Any = None,
        thinking: str | None = None,
        presence_penalty: float | None = None,
        frequency_penalty: float | None = None,
        seed: int | None = None,
        top_k: int | None = None,
        min_p: float | None = None,
        logprobs: bool | None = None,
        top_logprobs: int | None = None,
        reasoning_effort: str | None = None,
    ):
        """Return a demand-driven iterator; first anext admits native work.

        Use aclose after early break, or the provider's async context. No GC
        cleanup guarantee. One anext at a time; native buffering is 16 events
        plus one callback, not a byte or transport-buffer bound. No extra Python
        chunk queue is used. Cancellation cannot promise a remote service stop.
        The SDK deadline does not interrupt a held callback: an idle consumer
        must resume iteration or call aclose (or close the provider) to drain.
        """
        from nxuskit.types import ImageSourceType

        for symbol in (
            "nxuskit_stream_operation_start",
            "nxuskit_stream_operation_cancel",
            "nxuskit_stream_operation_wait",
            "nxuskit_stream_operation_free",
        ):
            if not hasattr(lib, symbol):
                raise RuntimeError("native library lacks the required owned stream ABI")
        if any(
            image.source_type == ImageSourceType.FILEPATH
            for message in messages
            for image in (message.images or [])
        ):
            raise ValueError("chat_stream_async requires in-memory or URL image inputs")
        request = SyncFFIProvider._request(
            self,
            messages,
            model,
            temperature,
            max_tokens,
            top_p,
            stop,
            response_format,
            tools,
            tool_choice,
            thinking=thinking,
            presence_penalty=presence_penalty,
            frequency_penalty=frequency_penalty,
            seed=seed,
            top_k=top_k,
            min_p=min_p,
            logprobs=logprobs,
            top_logprobs=top_logprobs,
            reasoning_effort=reasoning_effort,
            streaming=True,
        )
        return _OwnedStream(self, json.dumps(request).encode())

    def _settled(self, cancel, loop_closed):
        with self._lock:
            self._operations.discard(cancel)
            if loop_closed:
                self._closing = True
                for operation in self._operations:
                    operation.set()
            self._free_if_drained()

    def _free_if_drained(self):
        if self._closing and not self._operations and not self._drained.is_set():
            lib.nxuskit_free_provider(self._handle)
            self._handle = ffi.NULL
            self._drained.set()

    def _begin_close(self):
        with self._lock:
            self._closing = True
            for operation in self._operations:
                operation.set()
            self._free_if_drained()

    async def close_async(self) -> None:
        self._begin_close()
        while not self._drained.is_set():
            await asyncio.sleep(0.01)

    def close(self) -> None:
        try:
            asyncio.get_running_loop()
        except RuntimeError:
            self._begin_close()
            self._drained.wait()
        else:
            raise RuntimeError("use close_async on an event-loop thread")

    async def __aenter__(self):
        return self

    async def __aexit__(self, *_):
        await self.close_async()


class _OwnedStream:
    """One owner thread and one demand/ack rendezvous, never a chunk queue."""

    def __init__(self, owner, encoded):
        self._owner, self._encoded = owner, encoded
        self._cancel = threading.Event()
        self._done = threading.Event()
        self._condition = threading.Condition()
        self._demand = None
        self._loop = None
        self._started = False
        self._next_active = False
        self._terminal = None
        self._terminal_seen = False

    def __aiter__(self):
        return self

    async def __anext__(self):
        loop = asyncio.get_running_loop()
        if self._loop is not None and self._loop is not loop:
            raise RuntimeError("stream belongs to a different event loop")
        if self._next_active:
            raise RuntimeError("only one stream anext may be pending")
        if self._done.is_set():
            self._raise_terminal()
        self._next_active = True
        demand = (loop.create_future(), threading.Event())
        try:
            if not self._started:
                self._loop = loop
                with self._owner._lock:
                    if self._owner._closing:
                        raise RuntimeError("provider is closed")
                    if len(self._owner._operations) >= self._owner._limit:
                        raise RuntimeError("provider in-flight limit reached")
                    self._owner._operations.add(self._cancel)
                self._started = True
                try:
                    threading.Thread(target=self._run, name="nxuskit-stream-owner").start()
                except BaseException:
                    self._owner._settled(self._cancel, False)
                    self._done.set()
                    raise
            with self._condition:
                self._demand = demand
                self._condition.notify_all()
            if self._done.is_set():
                self._finish_pending()
            return await demand[0]
        except asyncio.CancelledError:
            self._cancel.set()
            raise
        finally:
            with self._condition:
                if self._demand is demand:
                    self._demand = None
                demand[1].set()  # acknowledge consumption, not merely scheduling
                self._condition.notify_all()
            self._next_active = False

    def _raise_terminal(self):
        if not self._terminal_seen:
            self._terminal_seen = True
            if self._terminal is not None:
                raise self._terminal
        raise StopAsyncIteration

    def _finish_pending(self):
        with self._condition:
            demand = self._demand
        if demand is not None and not demand[0].done():
            try:
                self._raise_terminal()
            except BaseException as error:
                demand[0].set_exception(error)

    def _stopping(self):
        return self._cancel.is_set() or self._loop.is_closed()

    def _run(self):
        @ffi.callback("int32_t(const char *, void *)", error=1)
        def chunk(raw, _):
            try:
                with self._condition:
                    while self._demand is None and not self._stopping():
                        self._condition.wait(0.02)
                    if self._stopping():
                        return 1
                    demand = self._demand
                value = stream_chunk_from_ffi(json.loads(ffi.string(raw)))
                value.declared_profile = self._owner.declared_profile

                def deliver():
                    if not demand[0].done():
                        demand[0].set_result(value)

                self._loop.call_soon_threadsafe(deliver)
                while not demand[1].wait(0.02):
                    if self._stopping():
                        return 1
                return int(self._stopping())
            except BaseException as error:
                self._terminal = error
                self._cancel.set()
                return 1

        @ffi.callback("void(int32_t, const char *, void *)")
        def terminal(status, raw, _):
            try:
                if status and self._terminal is None:
                    data = json.loads(ffi.string(raw)) if raw != ffi.NULL else {}
                    code = data.get("code", "internal_error")
                    if code == "cancelled":
                        self._terminal = asyncio.CancelledError()
                    else:
                        self._terminal = _callback_provider_error(
                            self._owner.provider_name, data
                        )
            except BaseException as error:
                self._terminal = error

        operation = ffi.NULL
        try:
            if self._stopping():
                self._terminal = asyncio.CancelledError()
            else:
                operation = lib.nxuskit_stream_operation_start(
                    self._owner._handle,
                    self._encoded,
                    self._owner._deadline_ms,
                    chunk,
                    terminal,
                    ffi.NULL,
                )
                if operation == ffi.NULL:
                    self._terminal = ProviderError(last_error() or "stream rejected")
                else:
                    while True:
                        if self._stopping():
                            self._cancel.set()
                            lib.nxuskit_stream_operation_cancel(operation)
                        if lib.nxuskit_stream_operation_wait(operation, 20):
                            break
        except BaseException as error:
            self._terminal = error
            self._cancel.set()
            if operation != ffi.NULL:
                lib.nxuskit_stream_operation_cancel(operation)
                while not lib.nxuskit_stream_operation_wait(operation, 20):
                    pass
        finally:
            if operation != ffi.NULL and not lib.nxuskit_stream_operation_free(operation):
                raise RuntimeError("native stream not quiescent; ownership retained")
            self._owner._settled(self._cancel, self._loop.is_closed())
            self._done.set()
        try:
            self._loop.call_soon_threadsafe(self._finish_pending)
        except RuntimeError:
            if self._loop.is_closed():
                self._owner._begin_close()
            else:
                raise

    async def aclose(self):
        self._cancel.set()
        with self._condition:
            self._condition.notify_all()
        if not self._started:
            self._done.set()
        while not self._done.is_set():
            await asyncio.sleep(0.01)
