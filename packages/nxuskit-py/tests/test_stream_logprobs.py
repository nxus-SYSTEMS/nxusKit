"""Streaming logprobs parity tests for the Python SDK.

CE-public: no API keys, no network calls, fixture-driven and mock-driven.
Tests mirror the Rust (stream_logprobs.rs) and Go (stream_logprobs_test.go)
acceptance assertions to satisfy INV-5 / INV-6 cross-language parity.
"""

from __future__ import annotations

import json
import warnings
from pathlib import Path
from typing import Optional

from nxuskit._ffi_types import stream_chunk_from_ffi
from nxuskit.mock import MockProvider
from nxuskit.types import (
    ChatRequest,
    ProviderCapabilities,
    StreamLogprobsDelta,
    TokenLogprob,
    TopLogprob,
    adapt_gpt54_reasoning_compat,
)

# ---------------------------------------------------------------------------
# Fixture helpers
# ---------------------------------------------------------------------------

FIXTURES_DIR = (
    Path(__file__).parents[3] / "internal" / "tests" / "parity" / "stream_logprobs" / "fixtures"
)


def _load_jsonl(name: str) -> list[dict]:
    path = FIXTURES_DIR / name
    lines = []
    for raw in path.read_text().splitlines():
        raw = raw.strip()
        if raw:
            lines.append(json.loads(raw))
    return lines


# ---------------------------------------------------------------------------
# T026 — fixture-driven test: OpenAI stream with logprobs (AT-3)
# ---------------------------------------------------------------------------


def test_stream_logprobs_openai_fixture() -> None:
    """Decode the shared OpenAI fixture and assert semantic logprob values."""
    chunks = _load_jsonl("openai-stream-logprobs.jsonl")

    got_logprob = False
    first_token: Optional[str] = None

    for chunk in chunks:
        choices = chunk.get("choices", [])
        if not choices:
            continue
        lp_raw = choices[0].get("logprobs")
        if not lp_raw or not lp_raw.get("content"):
            continue

        delta = StreamLogprobsDelta.from_dict(lp_raw)
        got_logprob = True

        for tok in delta.content:
            assert tok.token != "", "token string must not be empty"
            # AT-3: logprob must be ≤ 0 and within a reasonable range.
            assert -100 <= tok.logprob <= 0, (
                f"token {tok.token!r}: logprob {tok.logprob} out of range (-100, 0]"
            )
            assert len(tok.top_logprobs) >= 1, (
                f"token {tok.token!r} has no top_logprobs; fixture must include alternatives"
            )

        if first_token is None and delta.content:
            first_token = delta.content[0].token

    assert got_logprob, "fixture contained no logprob data"
    # Semantic assertion: first logprob-bearing token must be "Hello" per fixture.
    assert first_token == "Hello", f"first logprob token = {first_token!r}, want 'Hello'"


# ---------------------------------------------------------------------------
# T027 — FR-007 negative: Anthropic fixture yields no phantom logprobs
# ---------------------------------------------------------------------------


def test_stream_logprobs_anthropic_no_phantom() -> None:
    """Every line of the Anthropic fixture must lack a 'logprobs' key."""
    lines = _load_jsonl("anthropic-stream-no-logprobs.jsonl")
    assert lines, "Anthropic fixture is empty"

    for i, obj in enumerate(lines):
        assert "logprobs" not in obj, (
            f"line {i}: Anthropic fixture has unexpected logprobs key: {obj}"
        )

    # Also verify via MockProvider: non-injected chunks must have logprobs=None.
    provider = MockProvider(chunks=["Hello", " world"])
    for chunk in provider.chat_stream([]):
        assert chunk.logprobs is None, (
            f"non-supporting mock emitted phantom logprobs on chunk {chunk.delta!r}"
        )


# ---------------------------------------------------------------------------
# T028 — capability-flag parity test
# ---------------------------------------------------------------------------


def test_supports_streaming_logprobs_parity() -> None:
    """Capability flag values must match the Rust and Go expected values."""
    # Default ProviderCapabilities → false.
    caps = ProviderCapabilities()
    assert not caps.supports_streaming_logprobs, (
        "default ProviderCapabilities.supports_streaming_logprobs should be False"
    )

    # OpenAI-equivalent capabilities → true.
    openai_caps = ProviderCapabilities(
        supports_logprobs=True,
        supports_streaming_logprobs=True,
    )
    assert openai_caps.supports_streaming_logprobs
    assert openai_caps.supports_logprobs, "supports_streaming_logprobs implies supports_logprobs"

    # Anthropic-equivalent → false.
    anthropic_caps = ProviderCapabilities(
        supports_logprobs=False,
        supports_streaming_logprobs=False,
    )
    assert not anthropic_caps.supports_streaming_logprobs

    # Default mock → false.
    mock = MockProvider()
    assert not mock.get_capabilities().supports_streaming_logprobs

    # Mock with logprobs injected → true.
    mock_with_lp = MockProvider(
        chunks=["Hi"],
        streaming_logprobs=[
            [StreamLogprobsDelta(content=[TokenLogprob(token="Hi", logprob=-0.01)])]
        ],
    )
    assert mock_with_lp.get_capabilities().supports_streaming_logprobs


# ---------------------------------------------------------------------------
# Additional: serde round-trip and FFI parsing for StreamLogprobsDelta
# ---------------------------------------------------------------------------


def test_stream_logprobs_delta_round_trip() -> None:
    """StreamLogprobsDelta.from_dict should faithfully reconstruct from a dict."""
    raw = {
        "content": [
            {
                "token": " Hello",
                "logprob": -0.00731,
                "bytes": [32, 72, 101, 108, 108, 111],
                "top_logprobs": [
                    {"token": " Hi", "logprob": -2.1, "bytes": [32, 72, 105]},
                ],
            }
        ]
    }
    delta = StreamLogprobsDelta.from_dict(raw)
    assert len(delta.content) == 1
    tok = delta.content[0]
    assert tok.token == " Hello"
    assert abs(tok.logprob - (-0.00731)) < 1e-6
    assert tok.bytes == [32, 72, 101, 108, 108, 111]
    assert len(tok.top_logprobs) == 1
    assert tok.top_logprobs[0].token == " Hi"


def test_stream_chunk_from_ffi_parses_logprobs() -> None:
    """stream_chunk_from_ffi must populate StreamChunk.logprobs from JSON."""
    data = {
        "delta": "Hello",
        "logprobs": {
            "content": [
                {
                    "token": "Hello",
                    "logprob": -0.01,
                    "top_logprobs": [{"token": "Hi", "logprob": -1.5}],
                }
            ]
        },
    }
    chunk = stream_chunk_from_ffi(data)
    assert chunk.logprobs is not None
    assert len(chunk.logprobs.content) == 1
    assert chunk.logprobs.content[0].token == "Hello"


def test_stream_chunk_from_ffi_absent_logprobs_is_none() -> None:
    """stream_chunk_from_ffi must leave logprobs as None when key is absent."""
    chunk = stream_chunk_from_ffi({"delta": "Hello"})
    assert chunk.logprobs is None


# ---------------------------------------------------------------------------
# Mock provider streaming logprob injection
# ---------------------------------------------------------------------------


def test_mock_provider_injects_logprob_deltas() -> None:
    """MockProvider correctly injects StreamLogprobsDelta per-chunk."""
    lp = StreamLogprobsDelta(
        content=[
            TokenLogprob(
                token="Hello",
                logprob=-0.00731,
                bytes=[72, 101, 108, 108, 111],
                top_logprobs=[TopLogprob(token="Hi", logprob=-2.1)],
            )
        ]
    )
    provider = MockProvider(
        chunks=["Hello", "!"],
        streaming_logprobs=[[lp, None]],
    )
    received = list(provider.chat_stream([]))
    assert len(received) == 2

    # First chunk has injected logprobs.
    assert received[0].logprobs is not None
    tok = received[0].logprobs.content[0]
    assert tok.token == "Hello"
    assert abs(tok.logprob - (-0.00731)) < 1e-6
    assert len(tok.top_logprobs) == 1

    # Second chunk has no logprobs (nil slot).
    assert received[1].logprobs is None


# ---------------------------------------------------------------------------
# GPT-5.4 warn-and-drop adapter (T038)
# ---------------------------------------------------------------------------


def test_adapt_gpt54_reasoning_compat_drops_params_and_warns() -> None:
    """GPT-5.4 + reasoning effort ≠ none drops temperature/top_p/logprobs."""
    req = ChatRequest(
        model="gpt-5.4",
        messages=[],
        temperature=0.7,
        top_p=0.9,
        logprobs=True,
        top_logprobs=5,
    )
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        adapted, warn_msgs = adapt_gpt54_reasoning_compat(req, "medium")

    assert adapted.temperature is None, "temperature should be dropped"
    assert adapted.top_p is None, "top_p should be dropped"
    assert adapted.logprobs is None, "logprobs should be dropped"
    assert adapted.top_logprobs is None, "top_logprobs should be dropped"
    assert len(warn_msgs) == 3
    # Verify warnings mention each dropped param.
    all_text = " ".join(warn_msgs)
    assert "temperature" in all_text
    assert "top_p" in all_text
    assert "logprobs" in all_text
    # Python warnings module also emitted UserWarning.
    assert len(caught) == 3


def test_adapt_gpt54_reasoning_compat_none_keeps_params() -> None:
    """reasoning_effort='none' must not drop any params."""
    req = ChatRequest(model="gpt-5.4", messages=[], temperature=0.7, logprobs=True)
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        adapted, warn_msgs = adapt_gpt54_reasoning_compat(req, "none")

    assert adapted.temperature == 0.7
    assert adapted.logprobs is True
    assert warn_msgs == []
    assert len(caught) == 0


def test_adapt_gpt54_reasoning_compat_non_gpt54_unaffected() -> None:
    """Non-GPT-5.4 models must not be touched by the guard."""
    req = ChatRequest(model="gpt-4o", messages=[], temperature=0.7)
    adapted, warn_msgs = adapt_gpt54_reasoning_compat(req, "high")
    assert adapted.temperature == 0.7
    assert warn_msgs == []
