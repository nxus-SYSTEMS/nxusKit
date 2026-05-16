//! Tests for InferenceMetadata population across providers
//!
//! These tests verify that all providers correctly populate inference_metadata
//! in their responses.
#![allow(clippy::panic, clippy::print_stdout, clippy::print_stderr)]

use nxuskit_engine::LLMProvider;
use nxuskit_engine::providers::{LoopbackProvider, MockProvider};
use nxuskit_engine::types::{ChatRequest, FinishReason, Message};

/// T033: Test MockProvider populates inference_metadata
#[tokio::test]
async fn test_mock_populates_inference_metadata() {
    let mock = MockProvider::builder()
        .with_response("Hello from mock!")
        .build()
        .expect("Should build");

    let request = ChatRequest::new("test-model").with_message(Message::user("Hi"));

    let response = mock.chat(&request).await.expect("Should get response");

    // Verify inference_metadata is populated
    assert!(response.inference_metadata.is_complete);
    assert_eq!(
        response.inference_metadata.finish_reason,
        Some(FinishReason::Stop)
    );
}

/// T034: Test that is_complete and finish_reason are always set
#[tokio::test]
async fn test_inference_metadata_always_has_finish_reason() {
    // Test with MockProvider
    let mock = MockProvider::builder()
        .with_response("Mock response")
        .build()
        .expect("Should build");

    let request = ChatRequest::new("model").with_message(Message::user("test"));
    let response = mock.chat(&request).await.expect("Should succeed");

    assert!(
        response.inference_metadata.is_complete,
        "MockProvider should set is_complete"
    );
    assert!(
        response.inference_metadata.finish_reason.is_some(),
        "MockProvider should set finish_reason"
    );

    // Test with LoopbackProvider - use a valid model name
    let loopback = LoopbackProvider::new();
    let loopback_request = ChatRequest::new("echo").with_message(Message::user("test"));
    let response = loopback
        .chat(&loopback_request)
        .await
        .expect("Should succeed");

    assert!(
        response.inference_metadata.is_complete,
        "LoopbackProvider should set is_complete"
    );
    assert!(
        response.inference_metadata.finish_reason.is_some(),
        "LoopbackProvider should set finish_reason"
    );
}

/// Test that provider_metadata contains provider identifier
#[tokio::test]
async fn test_inference_metadata_has_provider_info() {
    let mock = MockProvider::builder()
        .with_response("test")
        .build()
        .expect("Should build");

    let request = ChatRequest::new("model").with_message(Message::user("hi"));
    let response = mock.chat(&request).await.expect("Should succeed");

    // Verify provider_metadata exists
    assert!(
        response.inference_metadata.provider_metadata.is_some(),
        "Should have provider_metadata"
    );

    let metadata = response.inference_metadata.provider_metadata.unwrap();
    assert!(
        metadata.get("provider").is_some(),
        "Should have provider field"
    );
}

/// Test as_clips_output() accessor method for typed CLIPS access
#[cfg(feature = "clips")]
#[tokio::test]
async fn test_as_clips_output_accessor() {
    use nxuskit_engine::providers::ClipsProvider;
    use tempfile::TempDir;

    // Create a simple rule file
    let temp_dir = TempDir::new().expect("Should create temp dir");
    let rules = r#"
(deftemplate item
    (slot name (type STRING))
    (slot value (type INTEGER)))

(deftemplate result
    (slot computed (type INTEGER)))

(defrule compute
    (item (value ?v))
    =>
    (assert (result (computed (* ?v 2)))))
"#;
    std::fs::write(temp_dir.path().join("test-rules.clp"), rules).expect("Should write rules");

    let clips = ClipsProvider::builder()
        .rules_directory(temp_dir.path().to_path_buf())
        .build()
        .expect("Should build");

    let input = r#"{"facts": [{"template": "item", "values": {"name": "test", "value": 5}}]}"#;
    let request = ChatRequest::new("test-rules.clp").with_message(Message::user(input));

    let response = clips.chat(&request).await.expect("Should succeed");

    // Test the typed accessor
    let clips_output = response
        .as_clips_output()
        .expect("Should parse as CLIPS output");

    // Verify we can access typed fields
    assert!(
        !clips_output.conclusions.is_empty(),
        "Should have conclusions"
    );
    assert!(
        clips_output.stats.total_rules_fired > 0,
        "Should have fired rules"
    );
}

/// Test as_clips_output returns None for non-CLIPS content
#[cfg(feature = "clips")]
#[test]
fn test_as_clips_output_returns_none_for_non_clips() {
    use nxuskit_engine::types::{ChatResponse, TokenCount, TokenUsage};

    let response = ChatResponse::new(
        "Hello, this is plain text!".to_string(),
        "gpt-4".to_string(),
        TokenUsage::estimated_only(TokenCount::new(10, 5)),
    );

    // Should return None for non-JSON content
    assert!(
        response.as_clips_output().is_none(),
        "Should return None for plain text"
    );
}
