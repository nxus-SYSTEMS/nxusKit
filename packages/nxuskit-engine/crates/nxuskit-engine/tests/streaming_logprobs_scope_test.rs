//! v0.9.4: streaming logprobs are now in scope. This file replaces the
//! v0.9.3 regression guard with a "scope flipped" marker — the scope
//! change is visible in review per the original test's instruction.
//!
//! Streaming logprob assertions for the type system live in:
//!   - `src/types.rs::stream_logprobs_delta_tests`
//!   - `tests/stream_logprobs.rs` (provider/parser/wrapper coverage)
//!
//! What we still pin here:
//!   - Defaults: a freshly constructed `StreamChunk` MUST have
//!     `logprobs == None` and the field MUST be omitted from serialized
//!     JSON. Phantom data on chunks built without explicit logprob input
//!     would violate FR-007.

use nxuskit_engine::types::StreamChunk;
use serde_json::Value;

#[test]
fn stream_chunk_default_has_none_logprobs_and_omits_field() {
    let chunk = StreamChunk::new("hello".to_string());
    assert!(chunk.logprobs.is_none(), "default logprobs must be None");

    let json: Value = serde_json::to_value(&chunk).expect("serialize");
    assert!(
        json.get("logprobs").is_none(),
        "absent logprobs must be omitted from JSON, got: {json}"
    );
}

#[test]
fn stream_chunk_thinking_only_has_no_logprobs() {
    let chunk = StreamChunk::thinking("planning...".to_string());
    assert!(chunk.logprobs.is_none());
    let json: Value = serde_json::to_value(&chunk).expect("serialize");
    assert!(json.get("logprobs").is_none());
}

#[test]
fn stream_chunk_parses_payloads_without_logprobs_key_as_none() {
    let json = r#"{"delta":"hello"}"#;
    let parsed: StreamChunk = serde_json::from_str(json).expect("parse");
    assert!(parsed.logprobs.is_none());
}
