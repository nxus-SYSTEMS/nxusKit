"""v0.9.3 (Phase 5 / US2): Python logprobs parity tests.

Covers tasks T064 (request serialization), T065 (absent-field regression),
T066 (typed response), T067 (provider_options non-tunneling), and T069
(public-API documentation).
"""

import inspect
import json

import pytest

from nxuskit import (
    ChatRequest,
    ChatResponse,
    LogprobsData,
    TokenLogprob,
    TopLogprob,
)

# ---------------------------------------------------------------------------
# T064: request serialization with first-class fields
# ---------------------------------------------------------------------------


def test_chat_request_with_logprobs_emits_first_class_json_fields():
    req = ChatRequest(
        model="gpt-5.4",
        messages=[{"role": "user", "content": "Score the next token."}],
        logprobs=True,
        top_logprobs=5,
    )

    payload = req.to_dict()
    assert payload["logprobs"] is True
    assert payload["top_logprobs"] == 5

    # Round-trip through json must preserve the same wire shape.
    parsed = json.loads(json.dumps(payload))
    assert parsed["logprobs"] is True
    assert parsed["top_logprobs"] == 5


def test_chat_request_top_logprobs_alone_still_emits_first_class():
    """top_logprobs is only meaningful with logprobs=True, but we still
    serialize it as a first-class field if the caller provided it; engine-side
    validation owns the semantic check (matches Rust behavior)."""
    req = ChatRequest(
        model="gpt-5.4",
        messages=[],
        top_logprobs=3,
    )

    payload = req.to_dict()
    assert payload["top_logprobs"] == 3
    assert "logprobs" not in payload


# ---------------------------------------------------------------------------
# T065: absent-field regression — omitted fields must not appear in JSON
# ---------------------------------------------------------------------------


def test_chat_request_without_logprobs_omits_both_keys():
    req = ChatRequest(
        model="gpt-5.4",
        messages=[{"role": "user", "content": "Hello"}],
    )

    payload = req.to_dict()
    assert "logprobs" not in payload, "logprobs must be absent, not null"
    assert "top_logprobs" not in payload, "top_logprobs must be absent, not null"

    # Same on the JSON wire.
    wire = json.loads(json.dumps(payload))
    assert "logprobs" not in wire
    assert "top_logprobs" not in wire


def test_chat_request_v092_compatible_byte_shape():
    """When no v0.9.3-only fields are set, the dict matches the v0.9.2
    request envelope shape (model + messages only for this minimal case)."""
    req = ChatRequest(
        model="gpt-5.4",
        messages=[{"role": "user", "content": "Hello from v0.9.2"}],
    )

    payload = req.to_dict()
    assert set(payload.keys()) == {"model", "messages"}, (
        f"unexpected keys leaked into v0.9.2-shaped request: {payload.keys()}"
    )


# ---------------------------------------------------------------------------
# T066: typed response exposes selected token + alternative
# ---------------------------------------------------------------------------


def test_logprobs_data_from_dict_yields_typed_objects():
    raw = {
        "content": [
            {
                "token": "Hello",
                "logprob": -0.01,
                "bytes": [72, 101, 108, 108, 111],
                "top_logprobs": [
                    {"token": "Hi", "logprob": -1.2, "bytes": [72, 105]},
                ],
            }
        ]
    }

    data = LogprobsData.from_dict(raw)
    assert isinstance(data, LogprobsData)
    assert len(data.content) == 1

    token = data.content[0]
    assert isinstance(token, TokenLogprob)
    assert token.token == "Hello"
    assert token.logprob == pytest.approx(-0.01)
    assert token.bytes == [72, 101, 108, 108, 111]

    assert len(token.top_logprobs) == 1
    alt = token.top_logprobs[0]
    assert isinstance(alt, TopLogprob)
    assert alt.token == "Hi"
    assert alt.logprob == pytest.approx(-1.2)
    assert alt.bytes == [72, 105]


def test_logprobs_data_handles_missing_bytes():
    """Providers that do not return UTF-8 bytes must still parse cleanly."""
    raw = {
        "content": [
            {
                "token": "x",
                "logprob": -0.5,
                "top_logprobs": [{"token": "y", "logprob": -1.0}],
            }
        ]
    }

    data = LogprobsData.from_dict(raw)
    assert data.content[0].bytes is None
    assert data.content[0].top_logprobs[0].bytes is None


# ---------------------------------------------------------------------------
# T067: provider_options must not auto-tunnel logprobs onto first-class fields
# ---------------------------------------------------------------------------


def test_provider_options_does_not_tunnel_logprobs_to_first_class():
    """If a caller stuffs logprobs into provider_options (legacy pattern),
    the first-class fields must remain unset and the wire JSON must keep
    those values strictly inside provider_options. Guards against silent
    tunneling that would defeat capability gating."""
    req = ChatRequest(
        model="gpt-5.4",
        messages=[{"role": "user", "content": "hi"}],
        provider_options={"logprobs": True, "top_logprobs": 7},
    )

    assert req.logprobs is None
    assert req.top_logprobs is None

    payload = req.to_dict()
    assert "logprobs" not in payload, "first-class logprobs must remain absent"
    assert "top_logprobs" not in payload, "first-class top_logprobs must remain absent"
    assert payload["provider_options"] == {"logprobs": True, "top_logprobs": 7}


def test_provider_options_round_trip_unchanged():
    req = ChatRequest(
        model="gpt-5.4",
        messages=[],
        provider_options={"vendor_param": "value", "another": 42},
    )

    payload = req.to_dict()
    assert payload["provider_options"] == {"vendor_param": "value", "another": 42}


# ---------------------------------------------------------------------------
# Response wiring: ChatResponse.logprobs is typed when present, None when not
# ---------------------------------------------------------------------------


def test_chat_response_default_has_no_logprobs():
    from nxuskit.types import TokenUsage

    resp = ChatResponse(content="hi", usage=TokenUsage(), model="gpt-4o")
    assert resp.logprobs is None


def test_chat_response_can_carry_typed_logprobs():
    from nxuskit.types import TokenUsage

    resp = ChatResponse(
        content="Hello",
        usage=TokenUsage(),
        model="gpt-5.4",
        logprobs=LogprobsData(
            content=[TokenLogprob(token="Hello", logprob=-0.01)],
        ),
    )
    assert resp.logprobs is not None
    assert resp.logprobs.content[0].token == "Hello"


# ---------------------------------------------------------------------------
# T069: public-API documentation tests — fail if docstrings drift
# ---------------------------------------------------------------------------


def test_chat_request_docstring_documents_logprobs_arguments():
    doc = inspect.getdoc(ChatRequest) or ""
    for keyword in ("logprobs", "top_logprobs", "provider_options"):
        assert keyword in doc, f"ChatRequest docstring missing required reference to '{keyword}'"


def test_logprobs_data_docstring_shows_typed_access_examples():
    doc = inspect.getdoc(LogprobsData) or ""
    assert "content" in doc, "LogprobsData docstring must describe content[i] access"
    assert "top_logprobs" in doc, (
        "LogprobsData docstring must describe top_logprobs alternative access"
    )
    assert ">>>" in doc, "LogprobsData docstring must include an executable example"


def test_token_logprob_and_top_logprob_have_docstrings():
    for cls in (TokenLogprob, TopLogprob):
        doc = inspect.getdoc(cls) or ""
        assert doc.strip(), f"{cls.__name__} must have a non-empty docstring"
        assert "logprob" in doc, f"{cls.__name__} docstring must describe its logprob field"


def test_logprobs_types_are_publicly_importable():
    """Smoke test that mirrors the spec's import contract."""
    from nxuskit import LogprobsData as _LD
    from nxuskit import TokenLogprob as _TL
    from nxuskit import TopLogprob as _TP

    assert _LD is LogprobsData
    assert _TL is TokenLogprob
    assert _TP is TopLogprob
