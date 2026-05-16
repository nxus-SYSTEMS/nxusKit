//! Smoke tests for nxuskit provider creation and chat cycle.
//!
//! Tests marked `#[ignore]` require `libnxuskit` at runtime.
//! Run them with: `NXUSKIT_LIB_DIR=/path/to/lib cargo test --test smoke -- --ignored`

use nxuskit::{ChatRequest, Message, NxuskitProvider, ProviderConfig, Role};

/// Verify that a mock provider can be created and dropped without panic.
#[test]
#[ignore = "requires libnxuskit runtime"]
fn provider_create_and_drop() {
    let config = ProviderConfig {
        provider_type: "mock".into(),
        ..Default::default()
    };
    let provider = NxuskitProvider::new(config).expect("failed to create mock provider");
    drop(provider); // should not panic
}

/// Verify a synchronous chat request/response cycle.
#[test]
#[ignore = "requires libnxuskit runtime"]
fn chat_request_response_cycle() {
    let config = ProviderConfig {
        provider_type: "mock".into(),
        ..Default::default()
    };
    let provider = NxuskitProvider::new(config).expect("failed to create mock provider");

    let request = ChatRequest {
        model: "mock-model".into(),
        messages: vec![Message {
            role: Role::User,
            content: "Hello".into(),
        }],
        ..Default::default()
    };

    let response = provider.chat(request).expect("chat failed");

    assert!(
        !response.content.is_empty(),
        "response content should not be empty"
    );
    assert!(
        !response.model.is_empty(),
        "response model should not be empty"
    );
}

/// Verify thread safety: concurrent provider creation and chat from multiple threads.
#[test]
#[ignore = "requires libnxuskit runtime"]
fn concurrent_provider_usage() {
    let handles: Vec<_> = (0..4)
        .map(|i| {
            std::thread::spawn(move || {
                let config = ProviderConfig {
                    provider_type: "mock".into(),
                    ..Default::default()
                };
                let provider =
                    NxuskitProvider::new(config).expect("failed to create provider in thread");

                let request = ChatRequest {
                    model: "mock-model".into(),
                    messages: vec![Message {
                        role: Role::User,
                        content: format!("Thread {i} says hello").into(),
                    }],
                    ..Default::default()
                };

                let response = provider.chat(request).expect("chat failed in thread");
                assert!(!response.content.is_empty());
            })
        })
        .collect();

    for h in handles {
        h.join().expect("thread panicked");
    }
}

// --- Tests that run without libnxuskit ---

/// Verify ProviderConfig serde round-trip.
#[test]
fn provider_config_serde_roundtrip() {
    let config = ProviderConfig {
        provider_type: "claude".into(),
        api_key: Some("sk-test".into()),
        model: Some("claude-sonnet-4-5-20250929".into()),
        base_url: None,
        timeout_ms: Some(30000),
        ..Default::default()
    };
    let json = serde_json::to_string(&config).unwrap();
    let parsed: ProviderConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.provider_type, "claude");
    assert_eq!(parsed.api_key.as_deref(), Some("sk-test"));
    assert_eq!(parsed.timeout_ms, Some(30000));
    // base_url should be absent from JSON (skip_serializing_if)
    assert!(!json.contains("base_url"));
}

/// Verify ChatRequest serde round-trip.
#[test]
fn chat_request_serde_roundtrip() {
    let req = ChatRequest {
        model: "test-model".into(),
        messages: vec![
            Message {
                role: Role::System,
                content: "You are helpful.".into(),
            },
            Message {
                role: Role::User,
                content: "Hi".into(),
            },
        ],
        temperature: Some(0.7),
        max_tokens: Some(1024),
        ..Default::default()
    };
    let json = serde_json::to_string(&req).unwrap();
    let parsed: ChatRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.model, "test-model");
    assert_eq!(parsed.messages.len(), 2);
    assert_eq!(parsed.messages[0].role, Role::System);
    assert_eq!(parsed.temperature, Some(0.7));
    assert_eq!(parsed.max_tokens, Some(1024));
    // Optional fields absent from JSON
    assert!(!json.contains("top_p"));
    assert!(!json.contains("seed"));
}

/// Verify ChatRequest default values.
#[test]
fn chat_request_defaults() {
    let req = ChatRequest::default();
    assert!(req.model.is_empty());
    assert!(req.messages.is_empty());
    assert!(!req.stream);
    assert!(req.temperature.is_none());
    assert!(req.max_tokens.is_none());
}
