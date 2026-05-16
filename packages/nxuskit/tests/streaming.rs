//! Streaming tests for nxuskit.
//!
//! Tests marked `#[ignore]` require `libnxuskit` at runtime.
//! Run them with: `NXUSKIT_LIB_DIR=/path/to/lib cargo test --test streaming -- --ignored`

use nxuskit::{ChatRequest, Message, NxuskitProvider, ProviderConfig, Role, StreamChunk};

/// Verify that chat_stream returns a StreamReceiver and chunks can be collected.
#[test]
#[ignore = "requires libnxuskit runtime"]
fn stream_collect_chunks() {
    let config = ProviderConfig {
        provider_type: "mock".into(),
        ..Default::default()
    };
    let provider = NxuskitProvider::new(config).expect("failed to create mock provider");

    let request = ChatRequest {
        model: "mock-model".into(),
        messages: vec![Message {
            role: Role::User,
            content: "Hello streaming".into(),
        }],
        ..Default::default()
    };

    let receiver = provider.chat_stream(request).expect("chat_stream failed");

    let chunks: Vec<StreamChunk> = receiver.filter_map(|r| r.ok()).collect();

    assert!(!chunks.is_empty(), "should receive at least one chunk");

    let full_content: String = chunks.iter().map(|c| c.delta.as_str()).collect();
    assert!(
        !full_content.is_empty(),
        "aggregated content should not be empty"
    );
}

/// Verify that cancelling a stream terminates it promptly.
#[test]
#[ignore = "requires libnxuskit runtime"]
fn stream_cancellation() {
    let config = ProviderConfig {
        provider_type: "mock".into(),
        ..Default::default()
    };
    let provider = NxuskitProvider::new(config).expect("failed to create mock provider");

    let request = ChatRequest {
        model: "mock-model".into(),
        messages: vec![Message {
            role: Role::User,
            content: "Hello streaming cancel".into(),
        }],
        ..Default::default()
    };

    let mut receiver = provider.chat_stream(request).expect("chat_stream failed");

    // Read one chunk then cancel.
    if let Some(Ok(_chunk)) = receiver.next_chunk() {
        receiver.cancel();
    }

    // After cancel, remaining chunks should drain quickly (or be empty).
    let remaining: Vec<_> = receiver.collect();
    // Just verify it doesn't hang — the exact count depends on SDK timing.
    let _ = remaining;
}

/// Verify that dropping StreamReceiver without consuming doesn't leak or panic.
#[test]
#[ignore = "requires libnxuskit runtime"]
fn stream_drop_without_consuming() {
    let config = ProviderConfig {
        provider_type: "mock".into(),
        ..Default::default()
    };
    let provider = NxuskitProvider::new(config).expect("failed to create mock provider");

    let request = ChatRequest {
        model: "mock-model".into(),
        messages: vec![Message {
            role: Role::User,
            content: "Hello streaming drop".into(),
        }],
        ..Default::default()
    };

    let _receiver = provider.chat_stream(request).expect("chat_stream failed");
    // Drop receiver immediately without reading any chunks.
}

// --- Tests that run without libnxuskit ---

/// Verify StreamChunk serde round-trip.
#[test]
fn stream_chunk_serde_roundtrip() {
    let chunk = StreamChunk {
        delta: "Hello ".into(),
        index: 0,
        thinking: None,
        finish_reason: None,
        usage: None,
        tool_calls: None,
        logprobs: None,
    };
    let json = serde_json::to_string(&chunk).unwrap();
    let parsed: StreamChunk = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.delta, "Hello ");
    assert_eq!(parsed.index, 0);
}

/// Verify StreamChunk deserialization with default index.
#[test]
fn stream_chunk_default_index() {
    let json = r#"{"content": "world"}"#;
    let chunk: StreamChunk = serde_json::from_str(json).unwrap();
    assert_eq!(chunk.delta, "world");
    assert_eq!(chunk.index, 0); // default
}
