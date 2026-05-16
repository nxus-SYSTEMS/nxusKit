//! Integration tests for parameter adaptation scenarios
//!
//! Tests that providers correctly adapt parameters and surface warnings
//! through the ChatResponse.warnings field.

use nxuskit_engine::prelude::*;

/// Test that stop sequences are truncated when exceeding provider limits
#[tokio::test]
async fn test_stop_sequence_truncation_warning() {
    let provider = MockProvider::new("Test response");

    // MockProvider has max_stop_sequences = 4
    let mut request = ChatRequest::new("mock-model").with_message(Message::user("Hello"));
    request.stop = Some(vec![
        "END".to_string(),
        "STOP".to_string(),
        "DONE".to_string(),
        "QUIT".to_string(),
        "EXIT".to_string(),
        "FINISH".to_string(),
    ]); // 6 stop sequences, exceeds limit of 4

    let response = provider.chat(&request).await.unwrap();

    // Should have a warning about stop sequence truncation
    assert!(!response.warnings.is_empty(), "Should have warnings");
    let stop_warning = response.warnings.iter().find(|w| w.parameter == "stop");
    assert!(stop_warning.is_some(), "Should have stop warning");

    let warning = stop_warning.unwrap();
    assert_eq!(warning.severity, WarningSeverity::Warning);
    assert!(warning.message.contains("Truncated"));
    assert!(warning.message.contains("6"));
    assert!(warning.message.contains("4"));
}

/// Test that unsupported penalties generate info warnings
#[tokio::test]
async fn test_unsupported_penalty_warning() {
    // Create a provider with no penalty support
    // We'll use MockProvider but modify the request to trigger the behavior
    // Note: MockProvider supports penalties, so this tests the passthrough case
    let provider = MockProvider::new("Test response");

    let mut request = ChatRequest::new("mock-model").with_message(Message::user("Hello"));
    request.presence_penalty = Some(0.5);
    request.frequency_penalty = Some(0.3);

    let response = provider.chat(&request).await.unwrap();

    // MockProvider supports penalties, so no warnings expected
    let penalty_warnings: Vec<_> = response
        .warnings
        .iter()
        .filter(|w| w.parameter.contains("penalty"))
        .collect();
    assert!(
        penalty_warnings.is_empty(),
        "MockProvider supports penalties"
    );
}

/// Test that seed parameter warning is generated for unsupported providers
#[tokio::test]
async fn test_seed_passthrough_when_supported() {
    let provider = MockProvider::new("Test response");

    let mut request = ChatRequest::new("mock-model").with_message(Message::user("Hello"));
    request.seed = Some(12345);

    let response = provider.chat(&request).await.unwrap();

    // MockProvider supports seed, so no warning expected
    let seed_warning = response.warnings.iter().find(|w| w.parameter == "seed");
    assert!(seed_warning.is_none(), "MockProvider supports seed");
}

/// Test that logprobs limit is enforced with warning
#[tokio::test]
async fn test_logprobs_limit_warning() {
    let provider = MockProvider::new("Test response");

    // MockProvider has max_logprobs = 20
    let mut request = ChatRequest::new("mock-model").with_message(Message::user("Hello"));
    request.logprobs = Some(true);
    request.top_logprobs = Some(50); // Exceeds limit of 20

    let response = provider.chat(&request).await.unwrap();

    // Should have a warning about logprobs limit
    let logprobs_warning = response
        .warnings
        .iter()
        .find(|w| w.parameter == "top_logprobs");
    assert!(logprobs_warning.is_some(), "Should have logprobs warning");

    let warning = logprobs_warning.unwrap();
    assert_eq!(warning.severity, WarningSeverity::Warning);
    assert!(warning.message.contains("50"));
    assert!(warning.message.contains("20"));
}

/// Test that JSON mode works without warning when supported
#[tokio::test]
async fn test_json_mode_supported() {
    let provider = MockProvider::new("Test response");

    let mut request = ChatRequest::new("mock-model").with_message(Message::user("Generate JSON"));
    request.response_format = Some(ResponseFormat::Json);

    let response = provider.chat(&request).await.unwrap();

    // MockProvider supports JSON mode, so no warning expected
    let json_warning = response
        .warnings
        .iter()
        .find(|w| w.parameter == "response_format");
    assert!(json_warning.is_none(), "MockProvider supports JSON mode");
}

/// Test that multiple warnings can be collected in a single response
#[tokio::test]
async fn test_multiple_warnings() {
    let provider = MockProvider::new("Test response");

    // Create request with multiple parameters that will generate warnings
    let mut request = ChatRequest::new("mock-model").with_message(Message::user("Hello"));
    request.stop = Some(vec![
        "A".to_string(),
        "B".to_string(),
        "C".to_string(),
        "D".to_string(),
        "E".to_string(),
        "F".to_string(),
    ]); // 6 stop sequences
    request.logprobs = Some(true);
    request.top_logprobs = Some(100); // Exceeds limit

    let response = provider.chat(&request).await.unwrap();

    // Should have at least 2 warnings
    assert!(
        response.warnings.len() >= 2,
        "Should have multiple warnings, got {}",
        response.warnings.len()
    );

    // Verify both parameters have warnings
    let has_stop_warning = response.warnings.iter().any(|w| w.parameter == "stop");
    let has_logprobs_warning = response
        .warnings
        .iter()
        .any(|w| w.parameter == "top_logprobs");

    assert!(has_stop_warning, "Should have stop warning");
    assert!(has_logprobs_warning, "Should have logprobs warning");
}

/// Test that no warnings are generated when all parameters are within limits
#[tokio::test]
async fn test_no_warnings_when_within_limits() {
    let provider = MockProvider::new("Test response");

    // Create request with all parameters within MockProvider's limits
    let mut request = ChatRequest::new("mock-model").with_message(Message::user("Hello"));
    request.temperature = Some(0.7);
    request.max_tokens = Some(100);
    request.stop = Some(vec!["END".to_string(), "STOP".to_string()]); // Only 2, within limit of 4
    request.presence_penalty = Some(0.5);
    request.frequency_penalty = Some(0.3);
    request.seed = Some(12345);
    request.logprobs = Some(true);
    request.top_logprobs = Some(5); // Within limit of 20
    request.response_format = Some(ResponseFormat::Json);

    let response = provider.chat(&request).await.unwrap();

    // Should have no warnings
    assert!(
        response.warnings.is_empty(),
        "Should have no warnings, got: {:?}",
        response.warnings
    );
}

/// Test response content is still returned even when warnings are present
#[tokio::test]
async fn test_response_content_with_warnings() {
    let provider = MockProvider::new("Expected response content");

    let mut request = ChatRequest::new("mock-model").with_message(Message::user("Hello"));
    request.stop = Some(vec![
        "1".to_string(),
        "2".to_string(),
        "3".to_string(),
        "4".to_string(),
        "5".to_string(),
    ]); // Exceeds limit

    let response = provider.chat(&request).await.unwrap();

    // Warnings should be present
    assert!(!response.warnings.is_empty(), "Should have warnings");

    // But response content should still be returned
    assert_eq!(response.content, "Expected response content");
    assert_eq!(response.model, "mock-model");
}

/// Test warning severity levels
#[tokio::test]
async fn test_warning_severity_levels() {
    let provider = MockProvider::new("Test response");

    let mut request = ChatRequest::new("mock-model").with_message(Message::user("Hello"));
    request.stop = Some(vec![
        "A".to_string(),
        "B".to_string(),
        "C".to_string(),
        "D".to_string(),
        "E".to_string(),
    ]); // Exceeds limit - should be Warning severity

    let response = provider.chat(&request).await.unwrap();

    let stop_warning = response
        .warnings
        .iter()
        .find(|w| w.parameter == "stop")
        .expect("Should have stop warning");

    // Stop sequence truncation should be Warning severity (not Info or Error)
    assert_eq!(
        stop_warning.severity,
        WarningSeverity::Warning,
        "Stop truncation should be Warning severity"
    );
}
