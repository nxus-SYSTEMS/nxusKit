//! Tests for MockProvider, ChatRequest/Message builders, and license_key plumbing.
//!
//! These tests run WITHOUT `libnxuskit` — the entire point of MockProvider.

#[allow(unused_imports)]
use nxuskit::{
    AsyncProvider, ChatRequest, ChatResponse, Message, MockProvider, MockProviderBuilder,
    LogprobsData, ModelInfo, NxuskitError, ProviderConfig, Role, StreamReceiver, ThinkingMode,
    TokenLogprob, TopLogprob,
};

// =========================================================================
// Phase 3: User Story 1 — MockProvider
// =========================================================================

/// T005: MockProvider::new() returns configured response via AsyncProvider::chat().
#[tokio::test]
async fn mock_chat_returns_configured_response() {
    let provider = MockProvider::new("Hello");

    let request = ChatRequest {
        model: "test-model".into(),
        messages: vec![Message {
            role: Role::User,
            content: "Hi".into(),
        }],
        ..Default::default()
    };

    let response = provider.chat(request).await.unwrap();
    assert_eq!(response.content, "Hello");
    assert_eq!(response.model, "mock-model");
}

/// T006: Sequential responses are returned in order.
#[tokio::test]
async fn mock_sequential_responses() {
    let provider = MockProvider::with_responses(vec!["A", "B", "C"]);

    let request = ChatRequest {
        model: "test-model".into(),
        messages: vec![],
        ..Default::default()
    };

    let r1 = provider.chat(request.clone()).await.unwrap();
    assert_eq!(r1.content, "A");

    let r2 = provider.chat(request.clone()).await.unwrap();
    assert_eq!(r2.content, "B");

    let r3 = provider.chat(request).await.unwrap();
    assert_eq!(r3.content, "C");
}

/// T007: After exhausting responses, last response repeats.
#[tokio::test]
async fn mock_exhausted_repeats_last() {
    let provider = MockProvider::with_responses(vec!["A", "B"]);

    let request = ChatRequest {
        model: "test-model".into(),
        messages: vec![],
        ..Default::default()
    };

    let _ = provider.chat(request.clone()).await.unwrap(); // A
    let _ = provider.chat(request.clone()).await.unwrap(); // B
    let r3 = provider.chat(request).await.unwrap();
    assert_eq!(r3.content, "B"); // repeat last
}

/// T008: Builder with custom models for list_models().
#[tokio::test]
async fn mock_list_models_configurable() {
    let provider = MockProvider::builder()
        .with_response("Hello")
        .with_models(vec![
            ModelInfo {
                id: "gpt-4o".into(),
                name: "GPT-4o".into(),
                description: None,
                size_bytes: None,
                context_window: Some(128000),
                metadata: Default::default(),
            },
            ModelInfo {
                id: "gpt-3.5".into(),
                name: "GPT-3.5".into(),
                description: None,
                size_bytes: None,
                context_window: Some(16000),
                metadata: Default::default(),
            },
        ])
        .build();

    let models = provider.list_models().await.unwrap();
    assert_eq!(models.len(), 2);
    assert_eq!(models[0].id, "gpt-4o");
    assert_eq!(models[1].id, "gpt-3.5");
}

/// T009: MockProvider records requests for test assertions.
#[tokio::test]
async fn mock_records_requests() {
    let provider = MockProvider::new("OK");

    for model in &["model-a", "model-b", "model-c"] {
        let request = ChatRequest {
            model: (*model).into(),
            messages: vec![],
            ..Default::default()
        };
        provider.chat(request).await.unwrap();
    }

    let requests = provider.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].model, "model-a");
    assert_eq!(requests[1].model, "model-b");
    assert_eq!(requests[2].model, "model-c");
}

/// T010: MockProvider works as Box<dyn AsyncProvider>.
#[tokio::test]
async fn mock_as_trait_object() {
    let provider = MockProvider::new("Hello");
    let dyn_provider: Box<dyn AsyncProvider> = Box::new(provider);

    let request = ChatRequest {
        model: "test-model".into(),
        messages: vec![Message {
            role: Role::User,
            content: "Hi".into(),
        }],
        ..Default::default()
    };

    let response = dyn_provider.chat(request).await.unwrap();
    assert_eq!(response.content, "Hello");
}

/// T011: Zero responses returns default empty response.
#[tokio::test]
async fn mock_zero_responses_returns_default() {
    let provider = MockProvider::with_responses(Vec::<String>::new());

    let request = ChatRequest {
        model: "test-model".into(),
        messages: vec![],
        ..Default::default()
    };

    let response = provider.chat(request).await.unwrap();
    assert_eq!(response.content, "");
    assert_eq!(response.model, "mock");
}

/// T012: chat_stream() returns an error.
#[tokio::test]
async fn mock_chat_stream_returns_error() {
    let provider = MockProvider::new("Hello");

    let request = ChatRequest {
        model: "test-model".into(),
        messages: vec![],
        ..Default::default()
    };

    let result = provider.chat_stream(request);
    let err = match result {
        Err(e) => e,
        Ok(_) => panic!("expected error from chat_stream on MockProvider"),
    };
    assert!(
        err.to_string().contains("streaming not supported"),
        "unexpected error: {err}"
    );
}

/// T013: MockProvider works without libnxuskit — this test is NOT #[ignore].
#[tokio::test]
async fn mock_no_sdk_binary_needed() {
    // This test proves SC-001 by existing and passing in CI without libnxuskit.
    let provider = MockProvider::new("No binary needed");
    let request = ChatRequest {
        model: "any-model".into(),
        messages: vec![Message {
            role: Role::User,
            content: "test".into(),
        }],
        ..Default::default()
    };
    let response = provider.chat(request).await.unwrap();
    assert_eq!(response.content, "No binary needed");
}

/// T013a: MockProvider records at least 100 requests (SC-006).
#[tokio::test]
async fn mock_records_at_least_100_requests() {
    let provider = MockProvider::new("OK");

    for i in 0..150 {
        let request = ChatRequest {
            model: format!("model-{i}"),
            messages: vec![],
            ..Default::default()
        };
        provider.chat(request).await.unwrap();
    }

    let requests = provider.requests();
    assert_eq!(requests.len(), 150);
    assert_eq!(requests[0].model, "model-0");
    assert_eq!(requests[99].model, "model-99");
    assert_eq!(requests[149].model, "model-149");
}

// =========================================================================
// Phase 4: User Story 2 — Builder/Factory Methods
// =========================================================================

/// T019: ChatRequest::new() produces correct defaults.
#[test]
fn chatrequest_new_with_defaults() {
    let request = ChatRequest::new("gpt-4o");
    assert_eq!(request.model, "gpt-4o");
    assert!(request.messages.is_empty());
    assert_eq!(request.temperature, None);
    assert_eq!(request.max_tokens, None);
    assert_eq!(request.top_p, None);
    assert_eq!(request.stop, None);
    assert!(!request.stream);
    assert_eq!(request.thinking_mode, None);
    assert_eq!(request.provider_options, None);
}

/// T020: Builder chain sets all fields.
#[test]
fn chatrequest_builder_chain() {
    let request = ChatRequest::new("m")
        .with_message(Message::user("Hi"))
        .with_temperature(0.7)
        .with_max_tokens(100)
        .with_top_p(0.9)
        .with_stop(vec!["END".into()])
        .with_thinking_mode(ThinkingMode::Enabled);

    assert_eq!(request.model, "m");
    assert_eq!(request.messages.len(), 1);
    assert_eq!(request.messages[0].content.text(), "Hi");
    assert_eq!(request.temperature, Some(0.7));
    assert_eq!(request.max_tokens, Some(100));
    assert_eq!(request.top_p, Some(0.9));
    assert_eq!(request.stop, Some(vec!["END".into()]));
    assert_eq!(request.thinking_mode, Some(ThinkingMode::Enabled));
}

/// T021: Builder method chaining is order-independent.
#[test]
fn chatrequest_builder_order_independent() {
    let request = ChatRequest::new("m")
        .with_temperature(0.5)
        .with_message(Message::user("Hello"));

    assert_eq!(request.temperature, Some(0.5));
    assert_eq!(request.messages.len(), 1);
    assert_eq!(request.messages[0].content.text(), "Hello");
}

/// T022: Message factory methods produce correct roles.
#[test]
fn message_factory_methods() {
    let sys = Message::system("s");
    assert_eq!(sys.role, Role::System);
    assert_eq!(sys.content.text(), "s");

    let usr = Message::user("u");
    assert_eq!(usr.role, Role::User);
    assert_eq!(usr.content.text(), "u");

    let asst = Message::assistant("a");
    assert_eq!(asst.role, Role::Assistant);
    assert_eq!(asst.content.text(), "a");
}

/// T023: Builder-constructed and struct-initialized ChatRequest produce identical JSON.
#[test]
fn chatrequest_builder_serialization_matches_struct_init() {
    let built = ChatRequest::new("gpt-4o")
        .with_message(Message::user("Hello"))
        .with_temperature(0.7);

    let manual = ChatRequest {
        model: "gpt-4o".into(),
        messages: vec![Message {
            role: Role::User,
            content: "Hello".into(),
        }],
        temperature: Some(0.7),
        ..Default::default()
    };

    assert_eq!(
        serde_json::to_string(&built).unwrap(),
        serde_json::to_string(&manual).unwrap(),
    );
}

/// v0.9.3: logprobs request helpers serialize as first-class fields.
#[test]
fn chatrequest_logprobs_helpers_serialize_first_class_fields() {
    let request = ChatRequest::new("gpt-5.4")
        .with_message(Message::user("Score the next token."))
        .with_logprobs(true)
        .with_top_logprobs(5);

    assert_eq!(request.logprobs, Some(true));
    assert_eq!(request.top_logprobs, Some(5));

    let json = serde_json::to_value(&request).unwrap();
    assert_eq!(json["logprobs"], true);
    assert_eq!(json["top_logprobs"], 5);
    assert!(json.get("provider_options").is_none());
}

/// v0.9.3: missing logprob fields remain backward-compatible.
#[test]
fn chatrequest_missing_logprobs_deserializes_as_none() {
    let json = r#"{"model":"gpt-4o","messages":[]}"#;
    let parsed: ChatRequest = serde_json::from_str(json).unwrap();

    assert_eq!(parsed.logprobs, None);
    assert_eq!(parsed.top_logprobs, None);
}

/// v0.9.3: response logprobs deserialize into typed data.
#[test]
fn chatresponse_logprobs_deserializes_typed_data() {
    let json = r#"{
        "content":"Hello",
        "model":"gpt-5.4",
        "usage":{"estimated":{"prompt_tokens":1,"completion_tokens":1}},
        "logprobs":{
            "content":[{
                "token":"Hello",
                "logprob":-0.01,
                "bytes":[72,101,108,108,111],
                "top_logprobs":[
                    {"token":"Hi","logprob":-1.2,"bytes":[72,105]}
                ]
            }]
        }
    }"#;

    let parsed: ChatResponse = serde_json::from_str(json).unwrap();
    let logprobs = parsed.logprobs.expect("logprobs should be present");
    assert_eq!(logprobs.content.len(), 1);

    let token: &TokenLogprob = &logprobs.content[0];
    assert_eq!(token.token, "Hello");
    assert_eq!(token.bytes.as_deref(), Some(&[72, 101, 108, 108, 111][..]));
    assert_eq!(token.top_logprobs.len(), 1);

    let top: &TopLogprob = &token.top_logprobs[0];
    assert_eq!(top.token, "Hi");
    assert!((top.logprob - -1.2).abs() < f32::EPSILON);
}

/// v0.9.3: typed response logprobs serialize using the engine JSON shape.
#[test]
fn chatresponse_logprobs_serializes_typed_data() {
    let response = ChatResponse {
        content: "Hello".into(),
        model: "gpt-5.4".into(),
        provider: "openai".into(),
        usage: Default::default(),
        finish_reason: None,
        metadata: Default::default(),
        warnings: vec![],
        logprobs: Some(LogprobsData {
            content: vec![TokenLogprob {
                token: "Hello".into(),
                logprob: -0.01,
                bytes: Some(vec![72, 101, 108, 108, 111]),
                top_logprobs: vec![TopLogprob {
                    token: "Hi".into(),
                    logprob: -1.2,
                    bytes: Some(vec![72, 105]),
                }],
            }],
        }),
        tool_calls: None,
        inference_metadata: None,
    };

    let json = serde_json::to_value(&response).unwrap();
    assert_eq!(json["logprobs"]["content"][0]["token"], "Hello");
    assert_eq!(json["logprobs"]["content"][0]["top_logprobs"][0]["token"], "Hi");
}

/// v0.9.3 (T058): a request without logprobs serializes byte-identically
/// to the v0.9.2 pre-logprobs fixture for fields in scope. Both `logprobs`
/// and `top_logprobs` must be omitted from the wire entirely (not present
/// as `null`) so v0.9.2 consumers see no schema drift.
#[test]
fn chatrequest_without_logprobs_matches_v092_fixture_bytes() {
    let request = ChatRequest::new("gpt-5.4")
        .with_message(Message::user("Hello from v0.9.2"));

    let serialized = serde_json::to_string(&request).unwrap();
    let fixture = include_str!("fixtures/v092-chat-request-no-logprobs.json");

    assert_eq!(
        serialized.trim(),
        fixture.trim(),
        "v0.9.3 request serialization drifted from v0.9.2 fixture for the no-logprobs case",
    );

    let json: serde_json::Value = serde_json::from_str(&serialized).unwrap();
    assert!(json.get("logprobs").is_none(), "logprobs key must be absent, not null");
    assert!(json.get("top_logprobs").is_none(), "top_logprobs key must be absent, not null");
}

/// v0.9.3 (T060): logprobs flow through first-class request fields, never
/// through `provider_options`. Even if a caller stuffs `logprobs` into
/// `provider_options`, the engine-bound JSON must keep it strictly inside
/// `provider_options` and the first-class fields must remain untouched.
/// This guards against silent tunneling that would defeat capability gating.
#[test]
fn chatrequest_provider_options_does_not_tunnel_logprobs_to_top_level() {
    let request = ChatRequest::new("gpt-5.4")
        .with_message(Message::user("hi"))
        .with_provider_options(serde_json::json!({
            "logprobs": true,
            "top_logprobs": 7,
        }));

    assert_eq!(request.logprobs, None, "with_provider_options must not set first-class logprobs");
    assert_eq!(request.top_logprobs, None, "with_provider_options must not set first-class top_logprobs");

    let json = serde_json::to_value(&request).unwrap();
    assert!(json.get("logprobs").is_none(), "top-level logprobs must remain absent");
    assert!(json.get("top_logprobs").is_none(), "top-level top_logprobs must remain absent");
    assert_eq!(json["provider_options"]["logprobs"], true);
    assert_eq!(json["provider_options"]["top_logprobs"], 7);
}

/// v0.9.3: responses without logprobs still parse cleanly.
#[test]
fn chatresponse_missing_logprobs_deserializes_as_none() {
    let json = r#"{
        "content":"Hello",
        "model":"gpt-4o",
        "usage":{"estimated":{"prompt_tokens":1,"completion_tokens":1}}
    }"#;

    let parsed: ChatResponse = serde_json::from_str(json).unwrap();
    assert!(parsed.logprobs.is_none());
}

// =========================================================================
// Phase 5: User Story 3 — License Key Plumbing
// =========================================================================

/// T028: license_key is serialized when present.
#[test]
fn license_key_serialized_when_present() {
    let config = ProviderConfig {
        provider_type: "ollama".into(),
        license_key: Some("PRO-XXXX".into()),
        ..Default::default()
    };
    let json = serde_json::to_string(&config).unwrap();
    assert!(json.contains(r#""license_key":"PRO-XXXX""#));
}

/// T029: license_key is absent from JSON when None.
#[test]
fn license_key_absent_when_none() {
    let config = ProviderConfig {
        provider_type: "ollama".into(),
        ..Default::default()
    };
    let json = serde_json::to_string(&config).unwrap();
    assert!(!json.contains("license_key"));
}

/// T030: license_key round-trips through JSON.
#[test]
fn license_key_roundtrip() {
    let config = ProviderConfig {
        provider_type: "ollama".into(),
        license_key: Some("ENT-ABCD-1234".into()),
        ..Default::default()
    };
    let json = serde_json::to_string(&config).unwrap();
    let parsed: ProviderConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.license_key.as_deref(), Some("ENT-ABCD-1234"));
}

/// T031: license_key with special characters passes through unmodified.
#[test]
fn license_key_special_characters() {
    let key = "PRO-日本語-\"quotes\"-émojis-🔑";
    let config = ProviderConfig {
        provider_type: "test".into(),
        license_key: Some(key.into()),
        ..Default::default()
    };
    let json = serde_json::to_string(&config).unwrap();
    let parsed: ProviderConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.license_key.as_deref(), Some(key));
}

/// T032: Deserializing JSON without license_key yields None.
#[test]
fn license_key_deserialize_missing_field() {
    let json = r#"{"provider_type":"claude","api_key":"sk-test"}"#;
    let config: ProviderConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.license_key, None);
    assert_eq!(config.provider_type, "claude");
}

/// T033: MockProvider works alongside a ProviderConfig with license_key.
#[tokio::test]
async fn mock_provider_works_with_license_key_config() {
    // This validates that MockProvider can coexist with license-key-bearing configs.
    let _config = ProviderConfig {
        provider_type: "mock".into(),
        license_key: Some("PRO-XXXX".into()),
        ..Default::default()
    };

    let provider = MockProvider::new("OK");
    let request = ChatRequest {
        model: "test-model".into(),
        messages: vec![],
        ..Default::default()
    };

    let response = provider.chat(request).await.unwrap();
    assert_eq!(response.content, "OK");
}
