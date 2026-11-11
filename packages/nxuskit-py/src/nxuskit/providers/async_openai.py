"""Native async OpenAI slice: HTTPX 0.28, no requests worker substitution.

HTTPX offers a universal wheel, BSD licensing and explicit async response close
(https://www.python-httpx.org/async/). Keep 0.28 compatibility while 1.0 is a
prerelease. This package-only increment does not qualify the standalone bundle.
"""

from __future__ import annotations

import asyncio
import json
import math

import httpx

from nxuskit.providers._resource_bounds import (
    NdjsonRecordFramer,
    SseEventFramer,
    read_bounded_async_chunks,
)

from nxuskit.errors import AuthenticationError, NetworkError, ProviderError, RateLimitError
from nxuskit.errors import TimeoutError as SDKTimeoutError
from nxuskit.providers.claude import ClaudeProvider
from nxuskit.providers.claude import _canonical_reason as _claude_reason
from nxuskit.providers.fireworks import FireworksProvider
from nxuskit.providers.groq import GroqProvider
from nxuskit.providers.lmstudio import LMStudioProvider
from nxuskit.providers.mistral import MistralProvider
from nxuskit.providers.ollama import OllamaProvider
from nxuskit.providers.openai import OpenAIProvider
from nxuskit.providers.openrouter import OpenRouterProvider
from nxuskit.providers.perplexity import PerplexityProvider
from nxuskit.providers.together import TogetherProvider
from nxuskit.providers.xai import XaiProvider
from nxuskit.types import ImageSourceType, StreamChunk, TokenUsage, _validate_request_controls


def _validate_claude_message(data, *, content_required=False):
    def invalid():
        raise ProviderError("invalid Anthropic response shape", provider="claude")

    if not isinstance(data, dict) or "error" in data:
        invalid()
    if data.get("model") is not None and not isinstance(data["model"], str):
        invalid()
    usage = data.get("usage", {})
    if not isinstance(usage, dict):
        invalid()
    for name in ("input_tokens", "output_tokens"):
        if name in usage and (type(usage[name]) is not int or usage[name] < 0):
            invalid()
    if content_required:
        if not isinstance(data.get("content"), list):
            invalid()
        for block in data["content"]:
            if not isinstance(block, dict):
                invalid()
            kind = block.get("type")
            if kind in ("text", "thinking"):
                if not isinstance(block.get(kind), str):
                    invalid()
            elif kind == "tool_use":
                if (
                    not isinstance(block.get("id"), str)
                    or not isinstance(block.get("name"), str)
                    or not isinstance(block.get("input"), dict)
                ):
                    invalid()
            else:
                invalid()


def _validate_response(data, *, streaming=False, provider="openai", metadata_only=False):
    """Validate wire DTOs before the permissive legacy mapper sees them."""

    def invalid():
        raise ProviderError("invalid compatible response shape", provider=provider)

    if not isinstance(data, dict) or "error" in data:
        invalid()
    choices = data.get("choices")
    if not isinstance(choices, list) or (not choices and not streaming):
        invalid()
    usage = data.get("usage")
    if not streaming and "usage" in data and usage is None and provider == "openai":
        invalid()
    if usage is not None:
        if not isinstance(usage, dict):
            invalid()
        for key in ("prompt_tokens", "completion_tokens", "total_tokens"):
            if key in usage and (type(usage[key]) is not int or usage[key] < 0):
                invalid()
    if not choices and usage is None and not metadata_only:
        invalid()
    for choice in choices:
        if not isinstance(choice, dict):
            invalid()
        value = choice.get("delta" if streaming else "message")
        if not isinstance(value, dict):
            invalid()
        if choice.get("finish_reason") is not None and not isinstance(choice["finish_reason"], str):
            invalid()
        if value.get("content") is not None and not isinstance(value["content"], str):
            invalid()
        if not streaming and "content" not in value and not value.get("tool_calls"):
            invalid()
        calls = value.get("tool_calls")
        if calls is not None:
            if not isinstance(calls, list):
                invalid()
            for call in calls:
                if not isinstance(call, dict):
                    invalid()
                if streaming and (type(call.get("index")) is not int or call["index"] < 0):
                    invalid()
                for key in ("id", "type"):
                    if key in call and not isinstance(call[key], str):
                        invalid()
                function = call.get("function", {}) if streaming else call.get("function")
                if not isinstance(function, dict):
                    invalid()
                for key in ("name", "arguments"):
                    if key in function and not isinstance(function[key], str):
                        invalid()
                if not streaming and (
                    not isinstance(call.get("id"), str)
                    or not all(isinstance(function.get(k), str) for k in ("name", "arguments"))
                ):
                    invalid()


class NativeAsyncOpenAIProvider:
    """Loop-owned client; close_async before closing its event loop.

    Construction is local. Explicit credentials only; environment credentials,
    proxies, redirects and retries are not silently enabled. Cancellation closes
    SDK-owned transport work, not a promise of provider-confirmed remote stop.
    """

    def __init__(
        self,
        model,
        *,
        api_key=None,
        api_url=None,
        timeout=30.0,
        max_in_flight=4,
        provider_type="openai",
        profile=None,
    ):
        if not math.isfinite(timeout) or not 0.001 <= timeout <= 4294967.295:
            raise ValueError("timeout must be finite and between 1ms and u32::MAX ms")
        if type(max_in_flight) is not int or not 1 <= max_in_flight <= 64:
            raise ValueError("max_in_flight must be an integer from 1 to 64")
        # Reuse pure wire/DTO mapping, never its synchronous HTTP methods.
        mapping_type = {
            "openai": OpenAIProvider,
            "openrouter": OpenRouterProvider,
            "perplexity": PerplexityProvider,
            "groq": GroqProvider,
            "xai": XaiProvider,
            "fireworks": FireworksProvider,
            "mistral": MistralProvider,
            "together": TogetherProvider,
            "lmstudio": LMStudioProvider,
            "ollama": OllamaProvider,
            "claude": ClaudeProvider,
        }.get(provider_type)
        if mapping_type is None:
            raise ValueError("unsupported native async profile")
        if provider_type == "ollama":
            OllamaProvider._validate_profile(profile)
            self._mapping = OllamaProvider(model, api_url=api_url, profile=profile)
        elif profile is not None:
            error = ProviderError(
                "declared profile is only supported for Ollama", provider=provider_type
            )
            error.code = "invalid_request"
            raise error
        elif provider_type == "lmstudio":
            # Select the local default explicitly; do not read environment endpoints.
            self._mapping = LMStudioProvider(
                model, api_url=api_url or LMStudioProvider.DEFAULT_API_URL
            )
        else:
            self._mapping = mapping_type(model, api_key=api_key or "", api_url=api_url)
        self._headers = self._mapping._build_headers()
        if not api_key:
            self._headers.pop("authorization", None)
        self._client = httpx.AsyncClient(trust_env=False, follow_redirects=False, timeout=None)
        self._timeout, self._limit = timeout, max_in_flight
        self._loop = None
        self._tasks = set()
        self._streams = set()
        self._closing = False
        self._closed = False
        self._close_task = None

    @property
    def model(self):
        return self._mapping.model

    def capabilities(self):
        """Stable native package declaration, independent of FFI and close state."""
        return self._mapping.capabilities()

    async def capabilities_async(self):
        return self.capabilities()

    @property
    def provider_name(self):
        return self._mapping.provider_name

    @property
    def closed(self):
        return self._closed

    @property
    def outstanding(self):
        return len(self._tasks) + len(self._streams)

    def _bind(self):
        loop = asyncio.get_running_loop()
        if self._loop is None:
            self._loop = loop
        elif self._loop is not loop:
            raise RuntimeError("provider belongs to a different event loop")

    def _admit(self):
        self._bind()
        if self._closing:
            raise RuntimeError("provider is closed")
        if self.outstanding >= self._limit:
            raise RuntimeError("provider in-flight limit reached")

    def _request(self, messages, model, stream, **kwargs):
        _validate_request_controls(self.provider_name, **kwargs)
        response_format = kwargs.get("response_format")
        if self.provider_name == "claude" and isinstance(response_format, dict) and (
            response_format.get("type") != "json_schema"
            or not isinstance(response_format.get("schema"), dict)
        ):
            error = ProviderError(
                "invalid request control: response_format", provider="claude"
            )
            error.code = "invalid_request"
            raise error
        if self.provider_name == "claude" and (
            isinstance(response_format, dict)
            or (
                stream
                and (bool(kwargs.get("tools")) or kwargs.get("tool_choice") is not None)
            )
        ):
            error = ProviderError(
                "unsupported Anthropic structured output or tool streaming", provider="claude"
            )
            error.code = "unsupported_capability"
            raise error
        if self.provider_name == "perplexity" and any(
            message.images for message in messages
        ):
            error = ProviderError(
                "unsupported request control: images", provider="perplexity"
            )
            error.code = "unsupported_capability"
            raise error
        if self.provider_name == "ollama" and any(
            image.source_type == ImageSourceType.URL
            for message in messages
            for image in (message.images or [])
        ):
            error = ProviderError(
                "unsupported Ollama URL image; provide in-memory base64", provider="ollama"
            )
            error.code = "unsupported_capability"
            raise error
        if any(
            image.source_type == ImageSourceType.FILEPATH
            for message in messages
            for image in (message.images or [])
        ):
            raise ValueError("native async requires in-memory or URL image inputs")
        effective = self.model if model is None else model
        unsupported = (
            (
                "presence_penalty",
                "frequency_penalty",
                "seed",
                "top_k",
                "min_p",
                "logprobs",
                "top_logprobs",
                "reasoning_effort",
                "thinking",
            )
            if self.provider_name == "claude"
            else (
                "presence_penalty",
                "frequency_penalty",
                "reasoning_effort",
            )
            if self.provider_name == "ollama"
            else ("thinking",)
        )
        for name in unsupported:
            if kwargs.pop(name, None) is not None:
                error = ProviderError(
                    f"unsupported request control: {name}", provider=self.provider_name
                )
                error.code = "unsupported_capability"
                raise error
        return self._mapping._build_request(
            messages, effective_model=effective, stream=stream, **kwargs
        ), effective

    def _endpoint(self):
        if self.provider_name == "claude":
            return self._mapping._api_url.rstrip("/") + "/v1/messages"
        if self.provider_name == "ollama":
            return self._mapping._api_url.rstrip("/") + "/api/chat"
        return self._mapping._api_endpoint("/chat/completions")

    def _check(self, response):
        code = response.status_code
        if 200 <= code < 300:
            return
        if code == 429:
            raise RateLimitError(
                f"HTTP {code}",
                status_code=code,
                provider=self.provider_name,
                model=self.model,
                retry_after=self._mapping._retry_after_seconds(response.headers),
            )
        error = AuthenticationError if code in (401, 403) else ProviderError
        raise error(f"HTTP {code}", status_code=code, provider=self.provider_name, model=self.model)

    async def _chat(self, request, model):
        response = None
        try:
            async with asyncio.timeout(self._timeout):
                http_request = self._client.build_request(
                    "POST",
                    self._endpoint(),
                    headers=self._headers,
                    json=request,
                )
                response = await self._client.send(http_request, stream=True)
                self._check(response)
                try:
                    body = await read_bounded_async_chunks(
                        response.aiter_bytes(chunk_size=64 * 1024),
                        provider=self.provider_name,
                    )
                    data = json.loads(body)
                except ValueError as error:
                    raise ProviderError(
                        "invalid response JSON", provider=self.provider_name
                    ) from error
                if self.provider_name == "ollama":
                    if (
                        not isinstance(data, dict)
                        or "error" in data
                        or data.get("done") is not True
                    ):
                        raise ProviderError(
                            "invalid or incomplete Ollama response", provider="ollama"
                        )
                elif self.provider_name == "claude":
                    _validate_claude_message(data, content_required=True)
                else:
                    _validate_response(data, provider=self.provider_name)
                return self._mapping._parse_response(data, model)
        except TimeoutError as error:
            raise SDKTimeoutError("deadline exceeded", provider=self.provider_name) from error
        except httpx.RequestError as error:
            raise NetworkError(
                "async HTTP transport failed", provider=self.provider_name
            ) from error
        finally:
            if response is not None:
                await response.aclose()

    async def chat_async(
        self,
        messages,
        *,
        model=None,
        temperature=None,
        max_tokens=None,
        top_p=None,
        stop=None,
        response_format=None,
        tools=None,
        tool_choice=None,
        presence_penalty=None,
        frequency_penalty=None,
        seed=None,
        top_k=None,
        min_p=None,
        logprobs=None,
        top_logprobs=None,
        reasoning_effort=None,
        thinking=None,
    ):
        request, model = self._request(
            messages,
            model,
            False,
            temperature=temperature,
            max_tokens=max_tokens,
            top_p=top_p,
            stop=stop,
            response_format=response_format,
            tools=tools,
            tool_choice=tool_choice,
            presence_penalty=presence_penalty,
            frequency_penalty=frequency_penalty,
            seed=seed,
            top_k=top_k,
            min_p=min_p,
            logprobs=logprobs,
            top_logprobs=top_logprobs,
            reasoning_effort=reasoning_effort,
            thinking=thinking,
        )
        self._admit()
        task = asyncio.create_task(self._chat(request, model))
        self._tasks.add(task)
        try:
            return await task
        finally:
            self._tasks.discard(task)

    def chat_stream_async(
        self,
        messages,
        *,
        model=None,
        temperature=None,
        max_tokens=None,
        top_p=None,
        stop=None,
        response_format=None,
        tools=None,
        tool_choice=None,
        presence_penalty=None,
        frequency_penalty=None,
        seed=None,
        top_k=None,
        min_p=None,
        logprobs=None,
        top_logprobs=None,
        reasoning_effort=None,
        thinking=None,
    ):
        request, model = self._request(
            messages,
            model,
            True,
            temperature=temperature,
            max_tokens=max_tokens,
            top_p=top_p,
            stop=stop,
            response_format=response_format,
            tools=tools,
            tool_choice=tool_choice,
            presence_penalty=presence_penalty,
            frequency_penalty=frequency_penalty,
            seed=seed,
            top_k=top_k,
            min_p=min_p,
            logprobs=logprobs,
            top_logprobs=top_logprobs,
            reasoning_effort=reasoning_effort,
            thinking=thinking,
        )
        return _NativeStream(self, request, model)

    async def _close(self):
        try:
            tasks = list(self._tasks)
            for task in tasks:
                task.cancel()
            await asyncio.gather(*tasks, return_exceptions=True)
            await asyncio.gather(*(stream.aclose() for stream in list(self._streams)))
            await self._client.aclose()
        finally:
            self._closed = self._client.is_closed and not self.outstanding

    async def close_async(self):
        self._bind()
        if self._close_task is None:
            self._closing = True
            self._close_task = asyncio.create_task(self._close())
        await asyncio.shield(self._close_task)

    def close(self):
        if self._closed:
            return
        try:
            asyncio.get_running_loop()
        except RuntimeError:
            if self._loop is None:
                asyncio.run(self.close_async())
            elif not self._loop.is_closed() and not self._loop.is_running():
                self._loop.run_until_complete(self.close_async())
            else:
                raise RuntimeError("close_async requires the owning loop to remain usable")
        else:
            raise RuntimeError("use close_async on an event-loop thread")

    async def __aenter__(self):
        self._bind()
        return self

    async def __aexit__(self, *_):
        await self.close_async()


class _NativeStream:
    """Pull one SSE/NDJSON event per anext; no producer queue or hidden prefetch task."""

    def __init__(self, owner, request, model):
        self._owner, self._request, self._model = owner, request, model
        self._started = self._closed = False
        self._response = self._lines = self._read_task = self._close_task = None
        self._raw_reason = self._usage = None
        self._provider_metadata = None
        self._claude_started = False

    def __aiter__(self):
        return self

    def _decode_bounded_stream_frame(self, frame):
        try:
            return frame.decode("utf-8")
        except UnicodeDecodeError as error:
            failure = ProviderError(
                "invalid stream UTF-8", provider=self._owner.provider_name
            )
            failure.code = "malformed_response"
            raise failure from error

    async def _iter_bounded_lines(self):
        provider = self._owner.provider_name
        if provider == "ollama":
            framer = NdjsonRecordFramer(provider="ollama")
            async for transport_chunk in self._response.aiter_bytes():
                for record in framer.feed(transport_chunk):
                    yield self._decode_bounded_stream_frame(record)
            for record in framer.finish():
                yield self._decode_bounded_stream_frame(record)
            return

        framer = SseEventFramer(provider=provider)
        async for transport_chunk in self._response.aiter_bytes():
            for event in framer.feed(transport_chunk):
                text = self._decode_bounded_stream_frame(event)
                for line in text.splitlines():
                    yield line
                yield ""

    async def _read(self):
        try:
            # timeout_at alone is insufficient when buffered lines never suspend.
            if self._owner._loop.time() >= self._deadline:
                raise TimeoutError
            async with asyncio.timeout_at(self._deadline):
                if self._response is None:
                    request = self._owner._client.build_request(
                        "POST",
                        self._owner._endpoint(),
                        headers=self._owner._headers,
                        json=self._request,
                    )
                    self._response = await self._owner._client.send(request, stream=True)
                    self._owner._check(self._response)
                    self._lines = self._iter_bounded_lines().__aiter__()
                data = []
                while True:
                    try:
                        line = await anext(self._lines)
                    except StopAsyncIteration as error:
                        raise ProviderError(
                            "stream ended before terminal event", provider=self._owner.provider_name
                        ) from error
                    if self._owner.provider_name == "ollama":
                        if not line:
                            continue
                        try:
                            event = json.loads(line)
                        except ValueError as error:
                            raise ProviderError("invalid stream JSON", provider="ollama") from error
                        if not isinstance(event, dict) or "error" in event:
                            raise ProviderError("invalid Ollama stream response", provider="ollama")
                        chunk = self._owner._mapping._parse_stream_event(event, self._model)
                        if chunk is not None:
                            self._model = chunk.model
                            if self._owner._loop.time() >= self._deadline:
                                raise TimeoutError
                            return chunk
                        continue
                    if line.startswith("data:"):
                        data.append(line[5:].removeprefix(" "))
                    elif not line and data:
                        payload, data = "\n".join(data), []
                        if self._owner._loop.time() >= self._deadline:
                            raise TimeoutError
                        if payload == "[DONE]":
                            if self._owner.provider_name == "claude":
                                raise ProviderError(
                                    "invalid Anthropic terminal event", provider="claude"
                                )
                            return StreamChunk(
                                delta="",
                                model=self._model,
                                terminal=True,
                                finish_reason=self._owner._mapping._canonical_reason(
                                    self._raw_reason
                                ),
                                raw_finish_reason=self._raw_reason,
                                usage=self._usage,
                                provider_metadata=self._provider_metadata,
                            )
                        try:
                            event = json.loads(payload)
                        except ValueError as error:
                            raise ProviderError(
                                "invalid stream JSON", provider=self._owner.provider_name
                            ) from error
                        if self._owner.provider_name == "claude":
                            _validate_claude_message(event)
                            if (
                                not isinstance(event, dict)
                                or not isinstance(event.get("type"), str)
                                or event["type"] == "error"
                            ):
                                raise ProviderError(
                                    "invalid Anthropic stream event", provider="claude"
                                )
                            if event["type"] == "message_start":
                                if self._claude_started:
                                    raise ProviderError(
                                        "duplicate Anthropic start", provider="claude"
                                    )
                                message = event.get("message", {})
                                _validate_claude_message(message)
                                self._claude_started = True
                                self._model = message.get("model") or self._model
                                usage = message.get("usage", {})
                                self._usage = TokenUsage(
                                    input_tokens=usage.get("input_tokens", 0),
                                    output_tokens=usage.get("output_tokens", 0),
                                )
                                continue
                            if not self._claude_started:
                                if event["type"] == "ping":
                                    continue
                                raise ProviderError(
                                    "Anthropic event before message_start", provider="claude"
                                )
                            if event["type"] == "message_stop":
                                return StreamChunk(
                                    delta="",
                                    model=self._model,
                                    terminal=True,
                                    raw_finish_reason=self._raw_reason,
                                    finish_reason=_claude_reason(self._raw_reason),
                                    usage=self._usage,
                                )
                            _validate_claude_message(event)
                            if event["type"] == "content_block_delta":
                                delta = event.get("delta")
                                if (
                                    not isinstance(delta, dict)
                                    or delta.get("type") not in ("text_delta", "thinking_delta")
                                    or not isinstance(
                                        delta.get(
                                            "text" if delta["type"] == "text_delta" else "thinking"
                                        ),
                                        str,
                                    )
                                ):
                                    raise ProviderError(
                                        "unsupported or invalid Anthropic content delta",
                                        provider="claude",
                                    )
                            if event["type"] == "message_delta" and not isinstance(
                                event.get("delta"), dict
                            ):
                                raise ProviderError(
                                    "invalid Anthropic message delta", provider="claude"
                                )
                            chunk = self._owner._mapping._parse_stream_event(event, self._model)
                            if chunk is None:
                                continue
                            if chunk.raw_finish_reason is not None:
                                self._raw_reason = chunk.raw_finish_reason
                            if chunk.usage is not None:
                                self._usage = TokenUsage(
                                    input_tokens=self._usage.input_tokens if self._usage else 0,
                                    output_tokens=chunk.usage.output_tokens,
                                )
                            chunk.raw_finish_reason = chunk.finish_reason = None
                            return chunk
                        fields = {
                            "openrouter": ("id", "provider", "usage"),
                            "perplexity": ("citations", "search_results", "usage"),
                        }.get(self._owner.provider_name, ())
                        metadata_only = isinstance(event, dict) and any(k in event for k in fields)
                        if metadata_only:
                            event = {"choices": [], **event}
                        _validate_response(
                            event,
                            streaming=True,
                            provider=self._owner.provider_name,
                            metadata_only=metadata_only,
                        )
                        self._provider_metadata = self._owner._mapping._merge_provider_metadata(
                            self._provider_metadata, event
                        )
                        if (
                            fields
                            and isinstance(event.get("usage"), dict)
                            and not all(
                                k in event["usage"] for k in ("prompt_tokens", "completion_tokens")
                            )
                        ):
                            event = {k: v for k, v in event.items() if k != "usage"}
                        chunk = self._owner._mapping._parse_stream_event(event, self._model)
                        # Tool-only deltas and usage-only terminal events are
                        # meaningful even without text or a finish reason.
                        if chunk is None:
                            choices = event["choices"]
                            calls = choices[0]["delta"].get("tool_calls") if choices else None
                            usage = event.get("usage")
                            if calls or usage is not None:
                                chunk = StreamChunk(
                                    delta="",
                                    model=self._model,
                                    tool_calls=calls,
                                    usage=TokenUsage(
                                        **{
                                            k: usage.get(k, 0)
                                            for k in (
                                                "prompt_tokens",
                                                "completion_tokens",
                                                "total_tokens",
                                            )
                                        }
                                    )
                                    if usage is not None
                                    else None,
                                )
                        if chunk is not None:
                            self._model = chunk.model
                            if chunk.raw_finish_reason is not None:
                                self._raw_reason = chunk.raw_finish_reason
                            if chunk.usage is not None:
                                self._usage = chunk.usage
                            chunk.finish_reason = None
                            chunk.raw_finish_reason = None
                            chunk.terminal = False
                            if not (
                                chunk.delta
                                or chunk.thinking
                                or chunk.usage is not None
                                or chunk.tool_calls
                                or chunk.logprobs
                            ):
                                continue
                            if self._owner._loop.time() >= self._deadline:
                                raise TimeoutError
                            return chunk
        except TimeoutError as error:
            raise SDKTimeoutError(
                "deadline exceeded", provider=self._owner.provider_name
            ) from error
        except httpx.RequestError as error:
            raise NetworkError(
                "async HTTP transport failed", provider=self._owner.provider_name
            ) from error

    async def __anext__(self):
        self._owner._bind()
        if self._closed:
            raise StopAsyncIteration
        if self._read_task is not None:
            raise RuntimeError("only one stream anext may be pending")
        if not self._started:
            self._owner._admit()
            self._owner._streams.add(self)
            self._started = True
            self._deadline = self._owner._loop.time() + self._owner._timeout
        self._read_task = asyncio.create_task(self._read())
        try:
            chunk = await self._read_task
            if chunk.terminal:
                await self.aclose()
            return chunk
        except BaseException:
            await self.aclose()
            raise
        finally:
            self._read_task = None

    async def _close(self):
        task = self._read_task
        if task is not None and not task.done():
            task.cancel()
            await asyncio.gather(task, return_exceptions=True)
        if self._response is not None:
            await self._response.aclose()
        self._owner._streams.discard(self)

    async def aclose(self):
        self._owner._bind()
        if self._close_task is None:
            self._closed = True
            self._close_task = asyncio.create_task(self._close())
        await asyncio.shield(self._close_task)
