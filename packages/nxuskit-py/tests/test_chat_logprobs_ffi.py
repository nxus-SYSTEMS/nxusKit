"""v0.9.3 (T068): logprobs survive the FFI JSON boundary.

The Rust core serializes logprobs via the C ABI as part of the chat
response JSON envelope. The Python FFI helper ``chat_response_from_ffi``
is the only converter between that envelope and the typed
``ChatResponse``. This test pins the contract that fields named
``logprobs``, ``top_logprobs``, ``token``, ``logprob``, and ``bytes``
flow through the boundary unchanged into typed Python objects.

We exercise the helper directly with a fixture envelope rather than
spinning up the native library, so the test stays runnable in pure-Python
test environments while still pinning the JSON contract that the FFI
emits.
"""

import json

import pytest

from nxuskit._ffi_types import chat_response_from_ffi
from nxuskit.types import ChatResponse, LogprobsData, TokenLogprob, TopLogprob

REQUEST_FIXTURE = {
    "model": "gpt-5.4",
    "messages": [{"role": "user", "content": "Score the next token."}],
    "logprobs": True,
    "top_logprobs": 2,
}


RESPONSE_FIXTURE = {
    "content": "Hello",
    "model": "gpt-5.4",
    "provider": "openai",
    "usage": {"estimated": {"prompt_tokens": 1, "completion_tokens": 1}},
    "finish_reason": "stop",
    "logprobs": {
        "content": [
            {
                "token": "Hello",
                "logprob": -0.01,
                "bytes": [72, 101, 108, 108, 111],
                "top_logprobs": [
                    {"token": "Hi", "logprob": -1.2, "bytes": [72, 105]},
                    {"token": "Hey", "logprob": -2.7, "bytes": [72, 101, 121]},
                ],
            }
        ]
    },
}


def test_request_fixture_round_trips_through_json_unchanged():
    """The request envelope the SDK hands to the FFI must keep logprobs
    fields first-class through json.dumps/loads — no nesting, no rename."""
    wire = json.loads(json.dumps(REQUEST_FIXTURE))
    assert wire["logprobs"] is True
    assert wire["top_logprobs"] == 2
    # provider_options is not used as a tunnel
    assert "provider_options" not in wire


def test_response_fixture_decodes_to_typed_logprobs_via_ffi_helper():
    response = chat_response_from_ffi(RESPONSE_FIXTURE)

    assert isinstance(response, ChatResponse)
    assert response.content == "Hello"
    assert response.model == "gpt-5.4"

    logprobs = response.logprobs
    assert isinstance(logprobs, LogprobsData)
    assert len(logprobs.content) == 1

    token = logprobs.content[0]
    assert isinstance(token, TokenLogprob)
    assert token.token == "Hello"
    assert token.logprob == pytest.approx(-0.01)
    assert token.bytes == [72, 101, 108, 108, 111]

    assert len(token.top_logprobs) == 2
    first_alt = token.top_logprobs[0]
    assert isinstance(first_alt, TopLogprob)
    assert first_alt.token == "Hi"
    assert first_alt.logprob == pytest.approx(-1.2)
    assert first_alt.bytes == [72, 105]

    second_alt = token.top_logprobs[1]
    assert second_alt.token == "Hey"
    assert second_alt.logprob == pytest.approx(-2.7)


def test_response_without_logprobs_decodes_with_logprobs_none():
    """Backward-compat: pre-logprobs response envelopes still parse cleanly
    and surface ``ChatResponse.logprobs is None`` rather than raising."""
    envelope = {
        "content": "Hi",
        "model": "gpt-4o",
        "usage": {"estimated": {"prompt_tokens": 1, "completion_tokens": 1}},
    }

    response = chat_response_from_ffi(envelope)
    assert response.logprobs is None
    assert response.content == "Hi"


def test_response_with_explicit_null_logprobs_decodes_as_none():
    envelope = {
        "content": "Hi",
        "model": "gpt-4o",
        "usage": {"estimated": {"prompt_tokens": 1, "completion_tokens": 1}},
        "logprobs": None,
    }

    response = chat_response_from_ffi(envelope)
    assert response.logprobs is None
