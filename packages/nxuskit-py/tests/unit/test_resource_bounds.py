import asyncio

import pytest

from nxuskit.errors import ProviderError
from nxuskit.providers._resource_bounds import (
    MAX_STREAM_ACCUMULATOR_BYTES,
    MAX_STREAM_RECORD_EVENT_BYTES,
    MAX_UNARY_RESPONSE_BODY_BYTES,
    NdjsonRecordFramer,
    SseEventFramer,
    read_bounded_async_chunks,
    read_bounded_chunks,
    sse_data_payload,
)


def assert_limit_error(error: ProviderError, limit: int) -> None:
    assert getattr(error, "code", None) == "malformed_response"
    assert str(limit) in str(error)
    assert not error.is_retryable


def test_unary_sync_exact_and_one_over():
    exact = read_bounded_chunks([b"a" * MAX_UNARY_RESPONSE_BODY_BYTES], provider="test")
    assert len(exact) == MAX_UNARY_RESPONSE_BODY_BYTES
    with pytest.raises(ProviderError) as caught:
        read_bounded_chunks(
            [b"a" * MAX_UNARY_RESPONSE_BODY_BYTES, b"x"], provider="test"
        )
    assert_limit_error(caught.value, MAX_UNARY_RESPONSE_BODY_BYTES)


def test_unary_async_exact_and_one_over():
    async def chunks(parts):
        for part in parts:
            yield part

    async def run():
        exact = await read_bounded_async_chunks(
            chunks([b"a" * MAX_UNARY_RESPONSE_BODY_BYTES]), provider="test"
        )
        assert len(exact) == MAX_UNARY_RESPONSE_BODY_BYTES
        with pytest.raises(ProviderError) as caught:
            await read_bounded_async_chunks(
                chunks([b"a" * MAX_UNARY_RESPONSE_BODY_BYTES, b"x"]),
                provider="test",
            )
        assert_limit_error(caught.value, MAX_UNARY_RESPONSE_BODY_BYTES)

    asyncio.run(run())


def test_sse_exact_limit_and_one_over():
    framer = SseEventFramer(provider="test")
    events = framer.feed(b"x" * MAX_STREAM_RECORD_EVENT_BYTES + b"\n\n")
    assert events == [b"x" * MAX_STREAM_RECORD_EVENT_BYTES]

    framer = SseEventFramer(provider="test")
    with pytest.raises(ProviderError) as caught:
        framer.feed(b"x" * (MAX_STREAM_RECORD_EVENT_BYTES + 1) + b"\n\n")
    assert_limit_error(caught.value, MAX_STREAM_RECORD_EVENT_BYTES)


def test_sse_fragmented_utf8_and_crlf_delimiter():
    raw = 'data: {"x":"€"}'.encode()
    euro = raw.index("€".encode())
    framer = SseEventFramer(provider="test")
    assert framer.feed(raw[: euro + 1]) == []
    assert framer.feed(raw[euro + 1 :] + b"\r\n\r") == []
    events = framer.feed(b"\n")
    assert len(events) == 1
    assert sse_data_payload(events[0], provider="test") == '{"x":"€"}'


def test_sse_multiline_data_and_large_transport_chunk():
    event = b"data: one\r\ndata: two"
    framer = SseEventFramer(provider="test")
    assert sse_data_payload(event, provider="test") == "one\ntwo"

    payload_len = MAX_STREAM_RECORD_EVENT_BYTES // 2 + 1024
    frame = b"data: " + b"a" * payload_len + b"\n\n"
    chunk = frame + frame
    assert len(chunk) > MAX_STREAM_ACCUMULATOR_BYTES
    framer = SseEventFramer(provider="test")
    events = framer.feed(chunk)
    assert len(events) == 2
    assert framer.pending_bytes <= MAX_STREAM_ACCUMULATOR_BYTES


def test_ndjson_exact_limit_and_one_over():
    framer = NdjsonRecordFramer(provider="test")
    records = framer.feed(b"x" * MAX_STREAM_RECORD_EVENT_BYTES + b"\n")
    assert records == [b"x" * MAX_STREAM_RECORD_EVENT_BYTES]

    framer = NdjsonRecordFramer(provider="test")
    with pytest.raises(ProviderError) as caught:
        framer.feed(b"x" * (MAX_STREAM_RECORD_EVENT_BYTES + 1) + b"\n")
    assert_limit_error(caught.value, MAX_STREAM_RECORD_EVENT_BYTES)


def test_ndjson_fragmented_utf8_and_crlf():
    raw = '{"x":"€"}'.encode()
    euro = raw.index("€".encode())
    framer = NdjsonRecordFramer(provider="test")
    assert framer.feed(raw[: euro + 1]) == []
    assert framer.feed(raw[euro + 1 :] + b"\r") == []
    records = framer.feed(b"\n")
    assert len(records) == 1
    assert records[0].decode() == '{"x":"€"}'


def test_ndjson_eof_final_record():
    framer = NdjsonRecordFramer(provider="test")
    assert framer.feed(b'{"done":true}') == []
    assert framer.finish() == [b'{"done":true}']


def test_invalid_utf8_sse_is_typed_error():
    with pytest.raises(ProviderError) as caught:
        sse_data_payload(b"data: \xff", provider="test")
    assert getattr(caught.value, "code", None) == "malformed_response"
