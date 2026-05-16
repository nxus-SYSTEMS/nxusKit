//! v0.9.3 (T077): C ABI passthrough test for logprobs.
//!
//! Pins that the engine's `ChatRequest` and `ChatResponse` JSON envelopes —
//! the exact wire shape every C ABI consumer (Python, Go, future direct C
//! callers) sees — preserve `logprobs`, `top_logprobs`, and the typed
//! response logprobs (selected token + alternatives + bytes) round-trip
//! without loss. Also pins that absent fields stay absent on the wire so
//! v0.9.2 consumers see no schema drift.

use nxuskit_engine::types::{
    ChatRequest, ChatResponse, FinishReason, LogprobsData, Message, TokenCount, TokenLogprob,
    TokenUsage, TopLogprob,
};
use serde_json::Value;

fn empty_usage() -> TokenUsage {
    TokenUsage::estimated_only(TokenCount::new(0, 0))
}

#[test]
fn request_logprobs_fields_serialize_first_class() {
    let mut request = ChatRequest::new("gpt-5.4");
    request = request.with_message(Message::user("Score the next token."));
    request.logprobs = Some(true);
    request.top_logprobs = Some(5);

    let json: Value = serde_json::to_value(&request).expect("serialize request");

    assert_eq!(json["logprobs"], Value::Bool(true));
    assert_eq!(json["top_logprobs"], Value::from(5));
    // Must NOT be tunneled through provider_options
    assert!(
        json.get("provider_options").is_none()
            || json["provider_options"].get("logprobs").is_none(),
        "logprobs leaked into provider_options on the wire: {json:#}"
    );
}

#[test]
fn request_without_logprobs_omits_both_keys_on_the_wire() {
    let mut request = ChatRequest::new("gpt-4o");
    request = request.with_message(Message::user("Hello"));

    let json: Value = serde_json::to_value(&request).expect("serialize request");
    assert!(
        json.get("logprobs").is_none(),
        "logprobs key must be absent (not null) for v0.9.2 compatibility"
    );
    assert!(
        json.get("top_logprobs").is_none(),
        "top_logprobs key must be absent (not null) for v0.9.2 compatibility"
    );
}

#[test]
fn request_round_trips_logprobs_fields_through_json_string() {
    let mut request = ChatRequest::new("gpt-5.4");
    request = request.with_message(Message::user("hi"));
    request.logprobs = Some(true);
    request.top_logprobs = Some(3);

    let wire = serde_json::to_string(&request).expect("to_string");
    let parsed: ChatRequest = serde_json::from_str(&wire).expect("from_str");

    assert_eq!(parsed.logprobs, Some(true));
    assert_eq!(parsed.top_logprobs, Some(3));
}

#[test]
fn response_typed_logprobs_round_trip_through_c_abi_json_envelope() {
    let mut response = ChatResponse::new("Paris.".into(), "gpt-5.4".into(), empty_usage());
    response.provider = "loopback".into();
    response.finish_reason = Some(FinishReason::Stop);
    response.logprobs = Some(LogprobsData {
        content: vec![TokenLogprob {
            token: "Paris".into(),
            logprob: -0.01,
            bytes: Some(vec![80, 97, 114, 105, 115]),
            top_logprobs: vec![
                TopLogprob {
                    token: "Lyon".into(),
                    logprob: -3.2,
                    bytes: Some(vec![76, 121, 111, 110]),
                },
                TopLogprob {
                    token: "Marseille".into(),
                    logprob: -4.7,
                    bytes: Some(vec![77, 97, 114, 115, 101, 105, 108, 108, 101]),
                },
            ],
        }],
    });

    // Serialize as the C ABI does — through serde_json — then re-parse to a
    // typed ChatResponse, which is exactly what Python/Go FFI consumers do.
    let wire = serde_json::to_string(&response).expect("serialize response");
    let parsed: ChatResponse = serde_json::from_str(&wire).expect("deserialize response");

    let logprobs = parsed.logprobs.expect("logprobs survived round trip");
    assert_eq!(logprobs.content.len(), 1);

    let token = &logprobs.content[0];
    assert_eq!(token.token, "Paris");
    assert!((token.logprob - -0.01).abs() < f32::EPSILON);
    assert_eq!(token.bytes.as_deref(), Some(&[80u8, 97, 114, 105, 115][..]));
    assert_eq!(token.top_logprobs.len(), 2);

    let lyon = &token.top_logprobs[0];
    assert_eq!(lyon.token, "Lyon");
    assert!((lyon.logprob - -3.2).abs() < f32::EPSILON);
    assert_eq!(lyon.bytes.as_deref(), Some(&[76u8, 121, 111, 110][..]));

    let marseille = &token.top_logprobs[1];
    assert_eq!(marseille.token, "Marseille");
    assert!((marseille.logprob - -4.7).abs() < f32::EPSILON);
}

#[test]
fn response_envelope_uses_content_field_for_logprobs_token_array() {
    // Pins the wire shape so all SDKs (Rust, Python, Go) and the C ABI
    // agree on the JSON path to the per-token array. Drift here would
    // silently break Python's chat_response_from_ffi and Go parity.
    let mut response = ChatResponse::new("x".into(), "m".into(), empty_usage());
    response.provider = "p".into();
    response.logprobs = Some(LogprobsData {
        content: vec![TokenLogprob {
            token: "x".into(),
            logprob: -0.1,
            bytes: None,
            top_logprobs: vec![],
        }],
    });

    let json: Value = serde_json::to_value(&response).expect("serialize");
    assert!(json["logprobs"].is_object(), "logprobs must be an object");
    assert!(
        json["logprobs"]["content"].is_array(),
        "wire path is logprobs.content[], not logprobs.tokens[]"
    );
    assert_eq!(json["logprobs"]["content"][0]["token"], "x");
}

#[test]
fn response_without_logprobs_serializes_without_logprobs_field() {
    let mut response = ChatResponse::new("hi".into(), "gpt-4o".into(), empty_usage());
    response.provider = "loopback".into();
    response.logprobs = None;

    let json: Value = serde_json::to_value(&response).expect("serialize");
    assert!(
        json.get("logprobs").is_none(),
        "absent logprobs must not serialize as null for v0.9.2 backward compatibility"
    );
}
