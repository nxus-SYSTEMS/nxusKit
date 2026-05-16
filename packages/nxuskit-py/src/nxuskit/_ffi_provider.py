"""High-level Provider class that wraps the nxuskit C ABI via cffi.

This module provides the ``FFIProvider`` class and ``create_ffi_provider``
factory function. These are drop-in alternatives to the native Python
providers that delegate all logic to libnxuskit.

Usage::

    from nxuskit._ffi_provider import create_ffi_provider

    with create_ffi_provider({"provider_type": "openai", "api_key": "sk-..."}) as p:
        response = p.chat({"model": "gpt-4o", "messages": [...]})
        print(response.content)
"""

from __future__ import annotations

import json
import queue
from typing import Any, Iterator

from nxuskit._ffi import ffi, last_error, lib
from nxuskit._ffi_errors import (
    ConfigError,
    EditionInsufficientError,
    FeatureUnavailableError,
    LicenseExpiredError,
    LicenseRequiredError,
    NxuskitError,
    ProviderError,
)
from nxuskit._ffi_types import chat_response_from_ffi, model_info_from_ffi, stream_chunk_from_ffi
from nxuskit.types import ChatResponse, ModelInfo, StreamChunk


def _parse_nxuskit_error(err_obj: dict[str, Any]) -> NxuskitError:
    """Parse a structured error dict from the C ABI into a typed exception.

    The C ABI returns errors as JSON objects with ``error_type``, ``message``,
    and optional ``feature`` / ``required_edition`` fields.  This function maps
    those to the appropriate Python exception subclass.
    """
    error_type = err_obj.get("error_type", "internal")
    message = err_obj.get("message", "Unknown error")
    feature = err_obj.get("feature")

    match error_type:
        case "license_required":
            return LicenseRequiredError(message, feature=feature)
        case "license_expired":
            return LicenseExpiredError(message, feature=feature)
        case "edition_insufficient":
            edition = err_obj.get("required_edition")
            return EditionInsufficientError(message, feature=feature, required_edition=edition)
        case "feature_unavailable":
            return FeatureUnavailableError(message, feature=feature)
        case _:
            return ProviderError(f"{error_type}: {message}", provider=feature)


class FFIProvider:
    """A provider backed by the nxuskit shared library.

    Wraps an opaque ``NxuskitProvider`` handle. Supports synchronous chat,
    streaming chat (via iterator), and model listing.

    Use as a context manager for automatic cleanup::

        with FFIProvider(handle) as provider:
            resp = provider.chat(request)
    """

    def __init__(self, handle: Any, provider_type: str = ""):
        import atexit

        self._handle = handle
        self._provider_type = provider_type
        self._closed = False
        # Register cleanup before interpreter shutdown to avoid __del__ segfault
        atexit.register(self.close)

    def chat(self, request: dict[str, Any]) -> ChatResponse:
        """Send a synchronous chat request.

        Args:
            request: Dict with ``model``, ``messages``, and optional parameters.

        Returns:
            ChatResponse with content, usage, model, etc.

        Raises:
            ProviderError: If the provider returns an error.
        """
        self._check_closed()
        req_json = json.dumps(request).encode("utf-8")

        response = lib.nxuskit_chat(self._handle, req_json)
        if response == ffi.NULL:
            err = last_error() or "unknown error"
            raise ProviderError(f"chat failed: {err}", provider=self._provider_type)

        try:
            json_ptr = lib.nxuskit_response_json(response)
            if json_ptr == ffi.NULL:
                raise ProviderError(
                    "response_json returned NULL",
                    provider=self._provider_type,
                )
            raw = json.loads(ffi.string(json_ptr).decode("utf-8"))
        finally:
            lib.nxuskit_free_response(response)

        # Check for error in response
        if "error" in raw and raw["error"]:
            raise _parse_nxuskit_error(raw["error"])

        return chat_response_from_ffi(raw)

    def stream(self, request: dict[str, Any]) -> Iterator[StreamChunk]:
        """Send a streaming chat request.

        Yields StreamChunk objects as they arrive from the provider.

        Args:
            request: Dict with ``model``, ``messages``, and optional parameters.

        Yields:
            StreamChunk with incremental content.

        Raises:
            ProviderError: If streaming fails.
        """
        self._check_closed()
        req_json = json.dumps(request).encode("utf-8")

        # Queue-based bridge: C callbacks put chunks/done into a thread-safe queue.
        chunk_queue: queue.Queue[StreamChunk | None | Exception] = queue.Queue()

        @ffi.callback("int32_t(const char *, void *)")
        def on_chunk(chunk_json, user_data):
            """Called from Rust/tokio background thread for each chunk."""
            if chunk_json == ffi.NULL:
                return 0
            try:
                raw = json.loads(ffi.string(chunk_json).decode("utf-8"))
                chunk_queue.put(stream_chunk_from_ffi(raw))
            except Exception:
                pass  # Skip malformed chunks
            return 0

        @ffi.callback("void(const char *, void *)")
        def on_done(final_json, user_data):
            """Called from Rust/tokio background thread when stream completes."""
            if final_json != ffi.NULL:
                try:
                    raw = json.loads(ffi.string(final_json).decode("utf-8"))
                    if "error" in raw and raw["error"]:
                        chunk_queue.put(_parse_nxuskit_error(raw["error"]))
                except Exception:
                    pass
            chunk_queue.put(None)  # Sentinel: stream is done

        stream = lib.nxuskit_chat_stream(self._handle, req_json, on_chunk, on_done, ffi.NULL)
        if stream == ffi.NULL:
            err = last_error() or "unknown error"
            raise ProviderError(
                f"chat_stream failed: {err}",
                provider=self._provider_type,
            )

        try:
            while True:
                item = chunk_queue.get()
                if item is None:
                    break  # Stream complete
                if isinstance(item, Exception):
                    raise item
                yield item
        finally:
            lib.nxuskit_free_stream(stream)

    def list_models(self) -> list[ModelInfo]:
        """List available models from this provider.

        Returns:
            List of ModelInfo objects.

        Raises:
            ProviderError: If the provider returns an error.
        """
        self._check_closed()

        result = lib.nxuskit_list_models(self._handle)
        if result == ffi.NULL:
            err = last_error() or "unknown error"
            raise ProviderError(
                f"list_models failed: {err}",
                provider=self._provider_type,
            )

        try:
            raw = json.loads(ffi.string(result).decode("utf-8"))
        finally:
            lib.nxuskit_free_string(result)

        return [model_info_from_ffi(m) for m in raw]

    def close(self) -> None:
        """Free the provider handle. Safe to call multiple times."""
        if not self._closed and self._handle is not None:
            try:
                # Guard against interpreter shutdown where ffi/lib may be None
                if ffi is not None and lib is not None and self._handle != ffi.NULL:
                    lib.nxuskit_free_provider(self._handle)
            except (TypeError, AttributeError, OSError):
                pass  # Module teardown in progress
            self._closed = True

    def _check_closed(self) -> None:
        if self._closed:
            raise ConfigError("Provider has been closed")

    def __enter__(self) -> FFIProvider:
        return self

    def __exit__(self, *args: Any) -> None:
        self.close()

    def __del__(self) -> None:
        # During interpreter shutdown, globals may already be None.
        # Guard against AttributeError / TypeError from stale references.
        try:
            self.close()
        except Exception:
            pass


def create_ffi_provider(config: dict[str, Any]) -> FFIProvider:
    """Create a provider backed by the nxuskit shared library.

    Args:
        config: Configuration dict. Must include ``provider_type``.
            See provider documentation for provider-specific fields.

    Returns:
        FFIProvider instance (use as context manager for cleanup).

    Raises:
        ConfigError: If the configuration is invalid.
    """
    if "provider_type" not in config:
        raise ConfigError("config must include 'provider_type'")

    config_json = json.dumps(config).encode("utf-8")
    handle = lib.nxuskit_create_provider(config_json)
    if handle == ffi.NULL:
        err = last_error() or "unknown error"
        raise ConfigError(
            f"Failed to create provider: {err}",
            provider=config.get("provider_type"),
        )

    return FFIProvider(handle, provider_type=config.get("provider_type", ""))
