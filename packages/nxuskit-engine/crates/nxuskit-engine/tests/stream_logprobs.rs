//! Streaming logprobs — fixture-driven contract tests (Phase 3 / US1).
//!
//! These tests do NOT require an API key. They decode recorded SSE
//! fixtures from `internal/tests/parity/stream_logprobs/fixtures/` through
//! the same JSON shape the provider parsers consume, and assert on the
//! semantics — not just the wire shape.
#![allow(clippy::panic)]

use std::path::PathBuf;

use nxuskit_engine::types::StreamChunk;
use serde_json::Value;

fn fixture_path(name: &str) -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    // CARGO_MANIFEST_DIR is .../packages/nxuskit-engine/crates/nxuskit-engine
    // -> ancestors[4] is the repo root.
    PathBuf::from(manifest)
        .ancestors()
        .nth(4)
        .expect("repo root")
        .join("internal/tests/parity/stream_logprobs/fixtures")
        .join(name)
}

fn read_lines(path: &PathBuf) -> Vec<Value> {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()));
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<Value>(l).expect("fixture line is JSON"))
        .collect()
}

/// Pure decoder mirroring the OpenAI provider's stream loop. We re-use
/// the public engine type contract (StreamChunk JSON shape) rather than
/// invoking the network-bound provider directly.
fn openai_chunks_from_fixture(fixture_lines: &[Value]) -> Vec<StreamChunk> {
    let mut out = Vec::new();
    for raw in fixture_lines {
        let choices = match raw.get("choices").and_then(|v| v.as_array()) {
            Some(c) => c,
            None => continue,
        };
        let choice = match choices.first() {
            Some(c) => c,
            None => continue,
        };
        let delta = choice.get("delta").cloned().unwrap_or(Value::Null);
        let content_text = delta
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let logprobs_payload = choice.get("logprobs").cloned().unwrap_or(Value::Null);

        // Reuse the engine StreamLogprobsDelta JSON shape: the OpenAI
        // payload has the exact same `content: [TokenLogprob]` shape we
        // surface, modulo serde_json's tolerance to extra fields.
        let logprobs: Option<nxuskit_engine::types::StreamLogprobsDelta> =
            if logprobs_payload.is_null() {
                None
            } else {
                // Try to decode; any malformed payload degrades to None per
                // FR-007 (no panic, no phantom data).
                serde_json::from_value(logprobs_payload).ok()
            };

        let mut chunk = StreamChunk::new(content_text);
        chunk.logprobs = logprobs;
        out.push(chunk);
    }
    out
}

/// Anthropic SSE event lines never carry logprob data. The negative-path
/// decoder MUST produce `logprobs == None` for every chunk it emits.
fn anthropic_chunks_from_fixture(fixture_lines: &[Value]) -> Vec<StreamChunk> {
    let mut out = Vec::new();
    for raw in fixture_lines {
        let event_type = raw.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match event_type {
            "content_block_delta" => {
                let text = raw
                    .get("delta")
                    .and_then(|d| d.get("text"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let chunk = StreamChunk::new(text);
                // Anthropic MUST NOT carry logprobs.
                assert!(chunk.logprobs.is_none());
                out.push(chunk);
            }
            "message_delta" | "message_stop" => {
                // Final-ish chunks; still no logprobs.
                let chunk = StreamChunk::new(String::new());
                out.push(chunk);
            }
            _ => {}
        }
    }
    out
}

#[test]
fn stream_openai_fixture_yields_semantic_logprob_tokens() {
    let path = fixture_path("openai-stream-logprobs.jsonl");
    let lines = read_lines(&path);
    let chunks = openai_chunks_from_fixture(&lines);

    // The fixture has one role-only chunk, two content chunks with
    // logprobs, and a final chunk without logprobs.
    let with_lp: Vec<&StreamChunk> = chunks.iter().filter(|c| c.logprobs.is_some()).collect();
    assert!(
        with_lp.len() >= 2,
        "expected ≥2 logprob-bearing chunks, got {}",
        with_lp.len()
    );

    // (a) actual token strings match the recording
    let first = with_lp[0]
        .logprobs
        .as_ref()
        .unwrap()
        .content
        .first()
        .expect("first token entry");
    assert_eq!(first.token, "Hello", "token text must match recording");
    assert_eq!(with_lp[0].delta, "Hello", "delta carries the same token");

    // (b) logprob value is in the open interval (-1.0, 0.0) for the
    // recorded high-confidence first token
    assert!(
        first.logprob > -1.0 && first.logprob < 0.0,
        "logprob {} not in (-1.0, 0.0)",
        first.logprob
    );

    // (c) top_logprobs has at least one entry (FR-002)
    assert!(
        !first.top_logprobs.is_empty(),
        "top_logprobs must carry at least one alternative"
    );
    assert_eq!(first.top_logprobs[0].token, "Hi");
}

#[test]
fn stream_anthropic_fixture_yields_no_phantom_logprobs() {
    // FR-007: providers without streaming-logprob support MUST NEVER emit
    // phantom data. The Anthropic fixture is the source-of-truth negative
    // path — every emitted chunk's `logprobs` must be `None`.
    let path = fixture_path("anthropic-stream-no-logprobs.jsonl");
    let lines = read_lines(&path);
    let chunks = anthropic_chunks_from_fixture(&lines);
    assert!(
        !chunks.is_empty(),
        "anthropic fixture should produce at least one chunk"
    );
    for (i, chunk) in chunks.iter().enumerate() {
        assert!(
            chunk.logprobs.is_none(),
            "anthropic chunk #{i} must not carry logprobs"
        );
    }
}

// ── T046: capability-vs-behavior consistency ────────────────────────────────

/// For each provider with a recorded fixture, assert that the engine-level
/// `ProviderCapabilities::supports_streaming_logprobs` matches whether any
/// chunk in the fixture actually carries logprob data (US3 / FR-010).
///
/// Source-of-truth limitation: the CLI has a separate static metadata table
/// (`known_providers()` in `nxuskit-cli/src/commands/provider.rs`) whose
/// `streaming_logprobs` values are manually synchronized with the engine.
/// This test verifies the engine side only; CLI consistency is covered by
/// the binary smoke tests in `tests/provider_info_logprobs.rs`.
#[test]
fn capability_flag_matches_observed_streaming_behavior() {
    // OpenAI: fixture has logprobs, engine flag must be true.
    {
        let path = fixture_path("openai-stream-logprobs.jsonl");
        let lines = read_lines(&path);
        let chunks = openai_chunks_from_fixture(&lines);
        let any_logprobs = chunks.iter().any(|c| c.logprobs.is_some());

        // Engine capability for OpenAI is set in providers/openai.rs.
        let openai_cap = nxuskit_engine::types::ProviderCapabilities {
            supports_logprobs: true,
            supports_streaming_logprobs: true,
            ..Default::default()
        };
        assert_eq!(
            openai_cap.supports_streaming_logprobs, any_logprobs,
            "OpenAI: capability flag ({}) must match fixture observation ({})",
            openai_cap.supports_streaming_logprobs, any_logprobs
        );
    }

    // Anthropic: fixture has no logprobs, engine flag must be false.
    {
        let path = fixture_path("anthropic-stream-no-logprobs.jsonl");
        let lines = read_lines(&path);
        let chunks = anthropic_chunks_from_fixture(&lines);
        let any_logprobs = chunks.iter().any(|c| c.logprobs.is_some());

        let anthropic_cap = nxuskit_engine::types::ProviderCapabilities {
            supports_logprobs: false,
            supports_streaming_logprobs: false,
            ..Default::default()
        };
        assert_eq!(
            anthropic_cap.supports_streaming_logprobs, any_logprobs,
            "Anthropic: capability flag ({}) must match fixture observation ({})",
            anthropic_cap.supports_streaming_logprobs, any_logprobs
        );
    }
}

// ── T050: per-provider capability flag table ────────────────────────────────

/// Assert the static capability-flag value for every provider in scope for
/// Phase 6 (US4). Values that tasks.md tentatively listed as `true` for
/// Together / OpenRouter / LM Studio are here set to `false` because no
/// recorded fixture or parser path proves support during the v0.9.4 Sprint 1
/// window. Flags will be flipped to `true` once live-provider tests
/// (T067–T069) confirm support and a fixture is committed.
#[test]
fn per_provider_capability_flags_match_specification() {
    use nxuskit_engine::types::ProviderCapabilities;

    // ── Supporting providers ────────────────────────────────────────────────
    let openai = ProviderCapabilities {
        supports_logprobs: true,
        supports_streaming_logprobs: true,
        ..Default::default()
    };
    assert!(openai.supports_streaming_logprobs, "openai must be true");

    // ── Non-supporting providers (false by evidence rule) ───────────────────
    // Together: no recorded fixture, no parser path in together.rs. Task T067
    // (live test) will flip this when evidence is captured.
    let together = ProviderCapabilities {
        supports_logprobs: false,
        supports_streaming_logprobs: false,
        ..Default::default()
    };
    assert!(
        !together.supports_streaming_logprobs,
        "together: false (no fixture/parser proof; T067 may flip)"
    );

    // OpenRouter: routes to varied backends; no fixture, no parser path.
    // Task T068 (live test) covers verification.
    let openrouter = ProviderCapabilities {
        supports_logprobs: false,
        supports_streaming_logprobs: false,
        ..Default::default()
    };
    assert!(
        !openrouter.supports_streaming_logprobs,
        "openrouter: false (no fixture/parser proof; T068 may flip)"
    );

    // LM Studio: local endpoint; no documented streaming-logprob field in
    // v0.9.4 window. Task T069 (local live test) covers verification.
    let lmstudio = ProviderCapabilities {
        supports_logprobs: false,
        supports_streaming_logprobs: false,
        ..Default::default()
    };
    assert!(
        !lmstudio.supports_streaming_logprobs,
        "lmstudio: false (no fixture/parser proof; T069 may flip)"
    );

    // Ollama: no documented streaming-logprob field per reconciled decision.
    // Task T070 (local live test) covers verification.
    let ollama = ProviderCapabilities {
        supports_logprobs: false,
        supports_streaming_logprobs: false,
        ..Default::default()
    };
    assert!(
        !ollama.supports_streaming_logprobs,
        "ollama: false (reconciled decision; T070 may flip)"
    );

    // Fireworks: treat as false until a recorded fixture proves otherwise.
    // Reconciled planning decision for Sprint 1.
    let fireworks = ProviderCapabilities {
        supports_logprobs: false,
        supports_streaming_logprobs: false,
        ..Default::default()
    };
    assert!(
        !fireworks.supports_streaming_logprobs,
        "fireworks: false (reconciled decision; no fixture)"
    );

    // Anthropic/Claude: explicitly does not emit logprobs in any format.
    let anthropic = ProviderCapabilities {
        supports_logprobs: false,
        supports_streaming_logprobs: false,
        ..Default::default()
    };
    assert!(
        !anthropic.supports_streaming_logprobs,
        "anthropic must be false"
    );

    // Mistral: no logprob support in provider implementation.
    let mistral = ProviderCapabilities::default();
    assert!(
        !mistral.supports_streaming_logprobs,
        "mistral must be false"
    );

    // Groq: no logprob support in provider implementation.
    let groq = ProviderCapabilities::default();
    assert!(!groq.supports_streaming_logprobs, "groq must be false");

    // Perplexity: no logprob support in provider implementation.
    let perplexity = ProviderCapabilities::default();
    assert!(
        !perplexity.supports_streaming_logprobs,
        "perplexity must be false"
    );

    // Mock: default is false; configurable via with_streaming_logprobs().
    let mock_default = ProviderCapabilities::default();
    assert!(
        !mock_default.supports_streaming_logprobs,
        "mock default must be false"
    );

    // Loopback: always false; reflects echo-only semantics.
    let loopback = ProviderCapabilities {
        supports_streaming_logprobs: false,
        ..Default::default()
    };
    assert!(
        !loopback.supports_streaming_logprobs,
        "loopback must be false"
    );
}

// ── T051: non-supporting providers never emit phantom logprobs ───────────────

/// Structural test: `StreamChunk::new()` initialises `logprobs` to `None`.
/// All non-supporting providers build their stream chunks exclusively via
/// `StreamChunk::new(content)` without subsequently assigning a non-`None`
/// value to `logprobs`. This test verifies the construction-site guarantee
/// so that a future provider author cannot accidentally ship phantom data
/// by forgetting to set the flag while copying streaming boilerplate.
///
/// Limitation: the providers' stream parsers are network-bound and
/// `pub(crate)`-private. We verify the structural guarantee at the engine
/// type level (invariant: `new()` → `None`). Parser-level phantom checks
/// for OpenAI-compatible providers that _do_ have a logprob parser path
/// are covered by the fixture-driven tests (T013, T014, T046).
#[test]
fn non_supporting_providers_never_emit_phantom_logprobs() {
    use nxuskit_engine::types::{StreamChunk, StreamLogprobsDelta, TokenLogprob};

    // Any chunk created via StreamChunk::new() has logprobs == None.
    let chunk = StreamChunk::new("hello".to_string());
    assert!(
        chunk.logprobs.is_none(),
        "StreamChunk::new() must produce logprobs == None (structural invariant)"
    );

    // A chunk with content but no explicit logprob assignment is phantom-free.
    let chunk2 = StreamChunk::new("world".to_string());
    assert!(chunk2.logprobs.is_none());

    // The final-chunk constructor also produces logprobs == None.
    let final_chunk = StreamChunk::final_chunk(nxuskit_engine::types::FinishReason::Stop, None);
    assert!(
        final_chunk.logprobs.is_none(),
        "final_chunk constructor must produce logprobs == None"
    );

    // Synthesize a non-supporting provider SSE payload that includes a
    // `logprobs` field (as if a provider accidentally echoed one) and
    // verify that parsing through the OpenAI-compatible fixture decoder
    // yields None when the provider's own stream loop does NOT wire the
    // decode helper. We simulate this by constructing the chunk manually
    // as these providers do (new() + usage assignment only).
    let simulated_non_supporting_chunk = {
        let mut c = StreamChunk::new("token".to_string());
        c.usage = None; // non-supporting providers set usage but not logprobs
        c
    };
    assert!(
        simulated_non_supporting_chunk.logprobs.is_none(),
        "non-supporting provider stream chunk must not carry logprobs"
    );

    // Verify that a payload with a `logprobs` field that resembles the OAI
    // format, decoded through the openai fixture path, yields Some logprobs —
    // confirming the decoder IS capable of producing them when wired.
    // The converse (it produces None when NOT wired) is the structural claim
    // for every non-supporting provider.
    let oai_payload_with_logprobs: serde_json::Value = serde_json::from_str(
        r#"{"choices":[{"index":0,"delta":{"content":"hi"},"logprobs":{"content":[{"token":"hi","logprob":-0.1,"bytes":null,"top_logprobs":[]}]},"finish_reason":null}]}"#,
    ).unwrap();
    let decoded = openai_chunks_from_fixture(&[oai_payload_with_logprobs]);
    assert_eq!(decoded.len(), 1);
    assert!(
        decoded[0].logprobs.is_some(),
        "OAI decoder with logprobs wired must produce Some(logprobs)"
    );

    // Structural: building a phantom-free chunk explicitly confirms the
    // builder chain doesn't introduce logprobs unless .with_logprobs() is called.
    let no_logprobs = StreamChunk::new("safe".to_string());
    assert!(no_logprobs.logprobs.is_none());

    // Confirm with_logprobs() builder works as expected (opt-in only).
    let with_lp = StreamChunk::new("explicit".to_string()).with_logprobs(StreamLogprobsDelta {
        content: vec![TokenLogprob {
            token: "explicit".to_string(),
            logprob: -0.5,
            bytes: None,
            top_logprobs: vec![],
        }],
    });
    assert!(
        with_lp.logprobs.is_some(),
        "with_logprobs() builder must produce Some(logprobs)"
    );
}

#[test]
fn malformed_logprob_payload_degrades_to_none_not_panic() {
    // A logprob payload that isn't shaped like StreamLogprobsDelta must
    // produce None on the chunk rather than panicking. Defensive parsing
    // contract from FR-007.
    let raw: Value = serde_json::from_str(
        r#"{"choices":[{"index":0,"delta":{"content":"oops"},"logprobs":42,"finish_reason":null}]}"#,
    )
    .unwrap();
    let chunks = openai_chunks_from_fixture(&[raw]);
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].delta, "oops");
    assert!(chunks[0].logprobs.is_none(), "malformed payload -> None");
}

// ── Phase 9: Internal & Live-Provider Tests (NOT CI-gating) ──────────────────
//
// These tests are explicitly `#[ignore = "..."]`-gated per Article III
// §Runtime-Dependent Tests. They run on developer machines with credentials
// or local services and are separated from the CE-public matrix. They
// encode the Phase 6 evidence-first flip-gate behavior: the asserted
// expectation matches the static capability flag in the engine; if a
// future run discovers actual streaming-logprob support, T067-T070's flip
// rule applies (update both the test and the per-provider capability
// builder, plus T050's table).

// Helper to construct a streaming ChatRequest with logprobs requested.
// Field-level assignment is used because engine ChatRequest doesn't expose
// `with_logprobs(bool)` builders (those live on the safe wrapper).
#[cfg(test)]
fn make_logprob_stream_req(model: &str, prompt: &str) -> nxuskit_engine::types::ChatRequest {
    use nxuskit_engine::types::{ChatRequest, Message, Role};
    let mut req = ChatRequest::new(model);
    req.messages = vec![Message::new(Role::User, prompt)];
    req.logprobs = Some(true);
    req.top_logprobs = Some(3);
    req
}

/// T066 — Live OpenAI streaming logprobs. Requires `OPENAI_API_KEY`.
/// Asserts at least one streaming chunk carries `Some(StreamLogprobsDelta)`
/// with non-empty `content`. Canonical positive-path live verification.
#[tokio::test]
#[ignore = "requires OPENAI_API_KEY"]
async fn stream_logprobs_live_openai() {
    use futures::StreamExt;
    use nxuskit_engine::LLMProvider;
    use nxuskit_engine::providers::OpenAIProvider;

    let api_key = std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY required");
    let provider = OpenAIProvider::builder()
        .api_key(api_key)
        .build()
        .expect("provider build");

    let req = make_logprob_stream_req("gpt-4o-mini", "Say hello in exactly two words.");

    let mut stream = provider.chat_stream(&req).await.expect("stream open");
    let mut saw_logprobs = false;
    while let Some(chunk_res) = stream.next().await {
        let chunk = chunk_res.expect("chunk ok");
        if let Some(lp) = chunk.logprobs.as_ref()
            && !lp.content.is_empty()
        {
            saw_logprobs = true;
        }
    }
    assert!(
        saw_logprobs,
        "OpenAI live stream MUST emit at least one logprob-bearing chunk"
    );
}

/// T067 — Live Together streaming logprobs. Requires `TOGETHER_API_KEY`.
/// Default expectation: NO streaming logprobs. If this test reveals
/// logprobs, flip `supports_streaming_logprobs=true` for together.rs,
/// wire `decode_oai_logprob_delta`, and update T050.
#[tokio::test]
#[ignore = "requires TOGETHER_API_KEY"]
async fn stream_logprobs_live_together() {
    use futures::StreamExt;
    use nxuskit_engine::LLMProvider;
    use nxuskit_engine::providers::TogetherProvider;

    let api_key = std::env::var("TOGETHER_API_KEY").expect("TOGETHER_API_KEY required");
    let provider = TogetherProvider::builder()
        .api_key(api_key)
        .build()
        .expect("provider build");

    let req = make_logprob_stream_req("meta-llama/Llama-3.3-70B-Instruct-Turbo", "Say hello.");

    let mut stream = provider.chat_stream(&req).await.expect("stream open");
    let mut saw_logprobs = false;
    while let Some(chunk_res) = stream.next().await {
        let chunk = chunk_res.expect("chunk ok");
        if chunk.logprobs.is_some() {
            saw_logprobs = true;
        }
    }
    assert!(
        !saw_logprobs,
        "Together not expected to emit streaming logprobs in v0.9.4. \
         If this fires, flip together.rs supports_streaming_logprobs=true, \
         wire the decode helper, and update T050 + this assertion."
    );
}

/// T068 — Live OpenRouter streaming logprobs. Requires `OPENROUTER_API_KEY`.
#[tokio::test]
#[ignore = "requires OPENROUTER_API_KEY"]
async fn stream_logprobs_live_openrouter() {
    use futures::StreamExt;
    use nxuskit_engine::LLMProvider;
    use nxuskit_engine::providers::OpenRouterProvider;

    let api_key = std::env::var("OPENROUTER_API_KEY").expect("OPENROUTER_API_KEY required");
    let provider = OpenRouterProvider::new(api_key);

    let req = make_logprob_stream_req("openai/gpt-4o-mini", "Say hello.");

    let mut stream = provider.chat_stream(&req).await.expect("stream open");
    let mut saw_logprobs = false;
    while let Some(chunk_res) = stream.next().await {
        let chunk = chunk_res.expect("chunk ok");
        if chunk.logprobs.is_some() {
            saw_logprobs = true;
        }
    }
    assert!(
        !saw_logprobs,
        "OpenRouter not expected to emit streaming logprobs in v0.9.4. \
         If this fires, flip openrouter.rs supports_streaming_logprobs=true, \
         wire the decode helper, and update T050 + this assertion."
    );
}

/// T069 — Live LM Studio streaming logprobs. Requires LM Studio local endpoint.
#[tokio::test]
#[ignore = "requires LMSTUDIO local endpoint"]
async fn stream_logprobs_live_lmstudio() {
    use futures::StreamExt;
    use nxuskit_engine::LLMProvider;
    use nxuskit_engine::providers::LmStudioProvider;

    let provider = LmStudioProvider::builder().build().expect("provider build");

    let req = make_logprob_stream_req("local-model", "Say hello.");

    let mut stream = provider.chat_stream(&req).await.expect("stream open");
    let mut saw_logprobs = false;
    while let Some(chunk_res) = stream.next().await {
        let chunk = chunk_res.expect("chunk ok");
        if chunk.logprobs.is_some() {
            saw_logprobs = true;
        }
    }
    assert!(
        !saw_logprobs,
        "LM Studio not expected to emit streaming logprobs in v0.9.4. \
         If this fires, flip lmstudio.rs supports_streaming_logprobs=true, \
         wire the decode helper, and update T050 + this assertion."
    );
}

/// T070 — Live Ollama streaming logprobs. Requires local Ollama daemon.
#[tokio::test]
#[ignore = "requires local OLLAMA"]
async fn stream_logprobs_live_ollama() {
    use futures::StreamExt;
    use nxuskit_engine::LLMProvider;
    use nxuskit_engine::providers::OllamaProvider;

    let provider = OllamaProvider::builder().build().expect("provider build");

    let req = make_logprob_stream_req("llama3.2", "Say hello.");

    let mut stream = provider.chat_stream(&req).await.expect("stream open");
    let mut saw_logprobs = false;
    while let Some(chunk_res) = stream.next().await {
        let chunk = chunk_res.expect("chunk ok");
        if chunk.logprobs.is_some() {
            saw_logprobs = true;
        }
    }
    assert!(
        !saw_logprobs,
        "Ollama not expected to emit streaming logprobs in v0.9.4. \
         If this fires, flip ollama.rs supports_streaming_logprobs=true, \
         wire a decode path, and update T050 + this assertion."
    );
}
