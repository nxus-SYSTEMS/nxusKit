//! Contract tests for InferenceMetadata and InferenceStep types
//!
//! These tests verify the serialization behavior and backward compatibility
//! of the new inference metadata types.

use nxuskit_engine::types::{
    ChatResponse, FinishReason, InferenceMetadata, InferenceStep, TokenCount, TokenUsage,
};

/// T008: Contract test for InferenceMetadata serialization roundtrip
#[test]
fn test_inference_metadata_serialization_roundtrip() {
    let metadata = InferenceMetadata {
        execution_time_ms: Some(42),
        is_complete: true,
        finish_reason: Some(FinishReason::Stop),
        token_usage: Some(TokenUsage::estimated_only(TokenCount::new(10, 20))),
        thinking_trace: Some("Reasoning about the problem...".to_string()),
        inference_steps: Some(vec![
            InferenceStep::new("rule_firing", "calculate-discount").with_details(
                serde_json::json!({
                    "salience": 10,
                    "module": "pricing"
                }),
            ),
            InferenceStep::new("tool_call", "get_weather")
                .with_details(serde_json::json!({"location": "Seattle"})),
        ]),
        provider_metadata: Some(serde_json::json!({
            "conflict_strategy": "depth",
            "custom_field": 123
        })),
    };

    // Serialize to JSON
    let json = serde_json::to_string(&metadata).expect("Should serialize");

    // Deserialize back
    let restored: InferenceMetadata = serde_json::from_str(&json).expect("Should deserialize");

    // Verify all fields match
    assert_eq!(restored.execution_time_ms, metadata.execution_time_ms);
    assert_eq!(restored.is_complete, metadata.is_complete);
    assert_eq!(restored.finish_reason, metadata.finish_reason);
    assert!(restored.token_usage.is_some());
    assert_eq!(restored.thinking_trace, metadata.thinking_trace);
    assert!(restored.inference_steps.is_some());
    assert_eq!(restored.inference_steps.as_ref().unwrap().len(), 2);
    assert!(restored.provider_metadata.is_some());
}

/// T008: Test InferenceStep serialization
#[test]
fn test_inference_step_serialization_roundtrip() {
    let step = InferenceStep::rule_firing("my-rule", 10);

    let json = serde_json::to_string(&step).expect("Should serialize");
    let restored: InferenceStep = serde_json::from_str(&json).expect("Should deserialize");

    assert_eq!(restored.step_type, "rule_firing");
    assert_eq!(restored.identifier, "my-rule");
    assert!(restored.details.is_some());
}

/// T008: Test default InferenceMetadata
#[test]
fn test_inference_metadata_default() {
    let metadata = InferenceMetadata::default();

    assert!(!metadata.is_complete);
    assert!(metadata.execution_time_ms.is_none());
    assert!(metadata.finish_reason.is_none());
    assert!(metadata.token_usage.is_none());
    assert!(metadata.thinking_trace.is_none());
    assert!(metadata.inference_steps.is_none());
    assert!(metadata.provider_metadata.is_none());
}

/// T008: Test InferenceMetadata builder methods
#[test]
fn test_inference_metadata_builders() {
    let metadata = InferenceMetadata::completed(FinishReason::Stop)
        .with_execution_time(100)
        .with_thinking_trace("Thinking...")
        .with_inference_steps(vec![InferenceStep::new("test", "step")])
        .with_provider_metadata(serde_json::json!({"key": "value"}));

    assert!(metadata.is_complete);
    assert_eq!(metadata.finish_reason, Some(FinishReason::Stop));
    assert_eq!(metadata.execution_time_ms, Some(100));
    assert_eq!(metadata.thinking_trace, Some("Thinking...".to_string()));
    assert!(metadata.inference_steps.is_some());
    assert!(metadata.provider_metadata.is_some());
}

/// T008: Test InferenceMetadata incomplete builder
#[test]
fn test_inference_metadata_incomplete() {
    let metadata = InferenceMetadata::incomplete(FinishReason::Length);

    assert!(!metadata.is_complete);
    assert_eq!(metadata.finish_reason, Some(FinishReason::Length));
}

/// T009: Contract test for ChatResponse backward compatibility
/// Verifies that deserializing old JSON (without inference_metadata) works
#[test]
fn test_chat_response_backward_compatibility_missing_metadata() {
    // Old format JSON without inference_metadata field
    let old_json = r#"{
        "content": "Hello, world!",
        "model": "test-model",
        "usage": {
            "actual": null,
            "estimated": {"prompt_tokens": 10, "completion_tokens": 20},
            "is_complete": true
        },
        "finish_reason": "stop",
        "metadata": {},
        "warnings": []
    }"#;

    // Should deserialize without error
    let response: ChatResponse =
        serde_json::from_str(old_json).expect("Should deserialize old format");

    assert_eq!(response.content, "Hello, world!");
    assert_eq!(response.model, "test-model");

    // inference_metadata should have default values
    assert!(!response.inference_metadata.is_complete);
    assert!(response.inference_metadata.execution_time_ms.is_none());
}

/// T009: Test ChatResponse with inference_metadata included
#[test]
fn test_chat_response_with_inference_metadata() {
    let json = r#"{
        "content": "Hello!",
        "model": "test-model",
        "usage": {
            "actual": null,
            "estimated": {"prompt_tokens": 5, "completion_tokens": 10},
            "is_complete": true
        },
        "finish_reason": "stop",
        "metadata": {},
        "warnings": [],
        "inference_metadata": {
            "execution_time_ms": 50,
            "is_complete": true,
            "finish_reason": "stop"
        }
    }"#;

    let response: ChatResponse = serde_json::from_str(json).expect("Should deserialize");

    assert!(response.inference_metadata.is_complete);
    assert_eq!(response.inference_metadata.execution_time_ms, Some(50));
    assert_eq!(
        response.inference_metadata.finish_reason,
        Some(FinishReason::Stop)
    );
}

/// T009: Test ChatResponse serialization includes inference_metadata
#[test]
fn test_chat_response_serialization_includes_metadata() {
    let usage = TokenUsage::estimated_only(TokenCount::new(10, 20));
    let mut response = ChatResponse::new("Test".to_string(), "model".to_string(), usage);
    response.inference_metadata =
        InferenceMetadata::completed(FinishReason::Stop).with_execution_time(42);

    let json = serde_json::to_string(&response).expect("Should serialize");

    // Verify the JSON contains inference_metadata
    assert!(json.contains("inference_metadata"));
    assert!(json.contains("execution_time_ms"));
    assert!(json.contains("42"));
}

/// Test InferenceStep helper constructors
#[test]
fn test_inference_step_helpers() {
    // Rule firing
    let rule = InferenceStep::rule_firing("my-rule", 10);
    assert_eq!(rule.step_type, "rule_firing");
    assert_eq!(rule.identifier, "my-rule");
    assert!(rule.details.as_ref().unwrap()["salience"] == 10);

    // Tool call
    let tool = InferenceStep::tool_call("get_weather", serde_json::json!({"location": "NYC"}));
    assert_eq!(tool.step_type, "tool_call");
    assert_eq!(tool.identifier, "get_weather");

    // Thinking
    let thinking = InferenceStep::thinking("This is my reasoning...");
    assert_eq!(thinking.step_type, "thinking");
    assert_eq!(thinking.identifier, "reasoning");
}

/// Test InferenceStep with long thinking content gets truncated in snippet
#[test]
fn test_inference_step_thinking_truncation() {
    let long_content = "A".repeat(200);
    let thinking = InferenceStep::thinking(&long_content);

    let snippet = thinking.details.as_ref().unwrap()["snippet"]
        .as_str()
        .unwrap();
    assert!(snippet.len() <= 103); // 100 chars + "..."
    assert!(snippet.ends_with("..."));
}
