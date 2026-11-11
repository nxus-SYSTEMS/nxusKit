"""Internal transport allocation bounds shared by sync and async Python providers."""

from __future__ import annotations

from collections.abc import AsyncIterable, Iterable

from nxuskit.errors import ProviderError

MAX_UNARY_RESPONSE_BODY_BYTES = 16 * 1024 * 1024
MAX_STREAM_RECORD_EVENT_BYTES = 4 * 1024 * 1024
STREAM_BUFFER_DELIMITER_SLACK_BYTES = 4
MAX_STREAM_ACCUMULATOR_BYTES = (
    MAX_STREAM_RECORD_EVENT_BYTES + STREAM_BUFFER_DELIMITER_SLACK_BYTES
)


def _limit_error(provider: str | None, what: str, limit: int) -> ProviderError:
    error = ProviderError(f"{what} exceeds {limit} bytes", provider=provider)
    error.code = "malformed_response"
    return error


def _utf8_error(provider: str | None, what: str) -> ProviderError:
    error = ProviderError(f"{what} is not valid UTF-8", provider=provider)
    error.code = "malformed_response"
    return error


def read_bounded_chunks(
    chunks: Iterable[bytes],
    *,
    provider: str | None = None,
    limit: int = MAX_UNARY_RESPONSE_BODY_BYTES,
) -> bytes:
    body = bytearray()
    for chunk in chunks:
        if not chunk:
            continue
        if len(chunk) > limit - len(body):
            raise _limit_error(provider, "response body", limit)
        body.extend(chunk)
    return bytes(body)


async def read_bounded_async_chunks(
    chunks: AsyncIterable[bytes],
    *,
    provider: str | None = None,
    limit: int = MAX_UNARY_RESPONSE_BODY_BYTES,
) -> bytes:
    body = bytearray()
    async for chunk in chunks:
        if not chunk:
            continue
        if len(chunk) > limit - len(body):
            raise _limit_error(provider, "response body", limit)
        body.extend(chunk)
    return bytes(body)


class SseEventFramer:
    """Incrementally frame SSE events without an unbounded SDK accumulator."""

    def __init__(self, *, provider: str | None = None) -> None:
        self._provider = provider
        self._buffer = bytearray()

    @property
    def pending_bytes(self) -> int:
        return len(self._buffer)

    def _drain(self) -> list[bytes]:
        events: list[bytes] = []
        while True:
            lf = self._buffer.find(b"\n\n")
            crlf = self._buffer.find(b"\r\n\r\n")
            found = [(lf, 2), (crlf, 4)]
            found = [(pos, width) for pos, width in found if pos >= 0]
            if not found:
                if len(self._buffer) > MAX_STREAM_RECORD_EVENT_BYTES + 3:
                    raise _limit_error(
                        self._provider, "SSE event", MAX_STREAM_RECORD_EVENT_BYTES
                    )
                return events

            frame_end, delimiter_len = min(found, key=lambda item: item[0])
            if frame_end > MAX_STREAM_RECORD_EVENT_BYTES:
                del self._buffer[: frame_end + delimiter_len]
                raise _limit_error(
                    self._provider, "SSE event", MAX_STREAM_RECORD_EVENT_BYTES
                )
            events.append(bytes(self._buffer[:frame_end]))
            del self._buffer[: frame_end + delimiter_len]

    def feed(self, chunk: bytes) -> list[bytes]:
        events: list[bytes] = []
        offset = 0
        while offset < len(chunk):
            available = MAX_STREAM_ACCUMULATOR_BYTES - len(self._buffer)
            if available <= 0:
                events.extend(self._drain())
                if len(self._buffer) >= MAX_STREAM_ACCUMULATOR_BYTES:
                    raise _limit_error(
                        self._provider, "SSE event", MAX_STREAM_RECORD_EVENT_BYTES
                    )
                continue
            take = min(available, len(chunk) - offset)
            self._buffer.extend(chunk[offset : offset + take])
            offset += take
            events.extend(self._drain())
        return events


def sse_data_payload(event: bytes, *, provider: str | None = None) -> str | None:
    try:
        text = event.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise _utf8_error(provider, "SSE event") from exc

    data: list[str] = []
    for line in text.splitlines():
        if not line.startswith("data:"):
            continue
        value = line[5:]
        if value.startswith(" "):
            value = value[1:]
        data.append(value)
    return "\n".join(data) if data else None


class NdjsonRecordFramer:
    """Incrementally frame NDJSON records with bounded pending bytes."""

    def __init__(self, *, provider: str | None = None) -> None:
        self._provider = provider
        self._buffer = bytearray()

    @property
    def pending_bytes(self) -> int:
        return len(self._buffer)

    def _drain(self) -> list[bytes]:
        records: list[bytes] = []
        while True:
            newline = self._buffer.find(b"\n")
            if newline < 0:
                pending = len(self._buffer) - (1 if self._buffer.endswith(b"\r") else 0)
                if pending > MAX_STREAM_RECORD_EVENT_BYTES:
                    raise _limit_error(
                        self._provider, "NDJSON record", MAX_STREAM_RECORD_EVENT_BYTES
                    )
                return records

            payload_end = newline
            if payload_end > 0 and self._buffer[payload_end - 1] == 13:
                payload_end -= 1
            if payload_end > MAX_STREAM_RECORD_EVENT_BYTES:
                del self._buffer[: newline + 1]
                raise _limit_error(
                    self._provider, "NDJSON record", MAX_STREAM_RECORD_EVENT_BYTES
                )
            record = bytes(self._buffer[:payload_end])
            del self._buffer[: newline + 1]
            if record.strip():
                records.append(record)

    def feed(self, chunk: bytes) -> list[bytes]:
        records: list[bytes] = []
        offset = 0
        while offset < len(chunk):
            available = MAX_STREAM_ACCUMULATOR_BYTES - len(self._buffer)
            if available <= 0:
                records.extend(self._drain())
                if len(self._buffer) >= MAX_STREAM_ACCUMULATOR_BYTES:
                    raise _limit_error(
                        self._provider, "NDJSON record", MAX_STREAM_RECORD_EVENT_BYTES
                    )
                continue
            take = min(available, len(chunk) - offset)
            self._buffer.extend(chunk[offset : offset + take])
            offset += take
            records.extend(self._drain())
        return records

    def finish(self) -> list[bytes]:
        records = self._drain()
        if not self._buffer:
            return records
        if self._buffer.endswith(b"\r"):
            self._buffer.pop()
        if len(self._buffer) > MAX_STREAM_RECORD_EVENT_BYTES:
            raise _limit_error(
                self._provider, "NDJSON record", MAX_STREAM_RECORD_EVENT_BYTES
            )
        record = bytes(self._buffer)
        self._buffer.clear()
        if record.strip():
            records.append(record)
        return records
