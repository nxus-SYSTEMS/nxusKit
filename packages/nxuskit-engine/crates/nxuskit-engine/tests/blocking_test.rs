//! Contract tests for BlockingProvider
//!
//! These tests verify the synchronous/blocking wrapper functionality.
//! Requires the `blocking-api` feature to be enabled.

#![cfg(feature = "blocking-api")]

use nxuskit_engine::LLMProvider;
use nxuskit_engine::blocking::BlockingProvider;
use nxuskit_engine::providers::MockProvider;
use nxuskit_engine::types::{ChatRequest, Message};

/// T045: Test BlockingProvider creation from synchronous context
#[test]
fn test_blocking_provider_creation_from_sync() {
    let mock = MockProvider::builder()
        .with_response("Hello!")
        .build()
        .expect("Should build MockProvider");

    let blocking = BlockingProvider::new(mock);
    assert!(
        blocking.is_ok(),
        "Should create BlockingProvider from sync context"
    );
}

/// T045: Test BlockingProvider can be created multiple times
#[test]
fn test_blocking_provider_multiple_instances() {
    let mock1 = MockProvider::builder()
        .with_response("Response 1")
        .build()
        .expect("Should build");
    let mock2 = MockProvider::builder()
        .with_response("Response 2")
        .build()
        .expect("Should build");

    let blocking1 = BlockingProvider::new(mock1).expect("Should create first");
    let blocking2 = BlockingProvider::new(mock2).expect("Should create second");

    // Both should work independently
    let request = ChatRequest::new("test").with_message(Message::user("Hello"));

    let response1 = blocking1.chat(&request).expect("Should get response 1");
    let response2 = blocking2.chat(&request).expect("Should get response 2");

    assert_eq!(response1.content, "Response 1");
    assert_eq!(response2.content, "Response 2");
}

/// T046: Test BlockingProvider::chat() blocks until complete
#[test]
fn test_blocking_provider_chat_blocks() {
    let mock = MockProvider::builder()
        .with_response("Blocking response!")
        .build()
        .expect("Should build");

    let blocking = BlockingProvider::new(mock).expect("Should create");

    let request = ChatRequest::new("test-model").with_message(Message::user("Hello"));

    // This call should block until complete
    let start = std::time::Instant::now();
    let response = blocking.chat(&request);
    let elapsed = start.elapsed();

    assert!(response.is_ok(), "Should get response");
    assert_eq!(response.unwrap().content, "Blocking response!");

    // Should complete quickly for MockProvider (no actual network call)
    assert!(
        elapsed.as_millis() < 1000,
        "MockProvider should respond quickly"
    );
}

/// T046: Test BlockingProvider::chat() returns correct response
#[test]
fn test_blocking_provider_chat_response() {
    let mock = MockProvider::builder()
        .with_response("Expected response content")
        .with_model("custom-model")
        .build()
        .expect("Should build");

    let blocking = BlockingProvider::new(mock).expect("Should create");

    let request = ChatRequest::new("custom-model").with_message(Message::user("Query"));

    let response = blocking.chat(&request).expect("Should get response");

    assert_eq!(response.content, "Expected response content");
    assert_eq!(response.model, "custom-model");
    assert!(response.inference_metadata.is_complete);
}

/// T047: Test BlockingProvider::list_models() requires ModelLister
#[test]
fn test_blocking_provider_list_models() {
    let mock = MockProvider::builder()
        .with_response("Hello!")
        .build()
        .expect("Should build");

    let blocking = BlockingProvider::new(mock).expect("Should create");

    // MockProvider implements ModelLister, so this should work
    let models = blocking.list_models();

    assert!(models.is_ok(), "Should list models");
    let models = models.unwrap();
    assert_eq!(models.len(), 3, "MockProvider has 3 models");

    // Verify model names
    let names: Vec<&str> = models.iter().map(|m| m.name.as_str()).collect();
    assert!(names.contains(&"mock-text-model"));
    assert!(names.contains(&"mock-vision-model"));
}

/// Test BlockingProvider::inner() returns reference to wrapped provider
#[test]
fn test_blocking_provider_inner() {
    let mock = MockProvider::builder()
        .with_response("Hello!")
        .build()
        .expect("Should build");

    let blocking = BlockingProvider::new(mock).expect("Should create");

    assert_eq!(blocking.inner().provider_name(), "mock");
}

/// Test BlockingProvider::into_inner() consumes wrapper
#[test]
fn test_blocking_provider_into_inner() {
    let mock = MockProvider::builder()
        .with_response("Hello!")
        .build()
        .expect("Should build");

    let blocking = BlockingProvider::new(mock).expect("Should create");
    let inner = blocking.into_inner();

    assert_eq!(inner.provider_name(), "mock");
}

/// Test BlockingProvider::provider_name() delegates correctly
#[test]
fn test_blocking_provider_name() {
    let mock = MockProvider::builder()
        .with_response("Hello!")
        .build()
        .expect("Should build");

    let blocking = BlockingProvider::new(mock).expect("Should create");

    assert_eq!(blocking.provider_name(), "mock");
}

/// Test multiple sequential chat calls
#[test]
fn test_blocking_provider_multiple_chats() {
    let mock = MockProvider::builder()
        .with_response("Repeated response")
        .build()
        .expect("Should build");

    let blocking = BlockingProvider::new(mock).expect("Should create");

    for i in 0..5 {
        let request =
            ChatRequest::new("test").with_message(Message::user(format!("Message {}", i)));

        let response = blocking.chat(&request);
        assert!(response.is_ok(), "Chat {} should succeed", i);
        assert_eq!(response.unwrap().content, "Repeated response");
    }
}

/// Test BlockingProvider with LoopbackProvider
#[test]
fn test_blocking_with_loopback() {
    use nxuskit_engine::providers::LoopbackProvider;

    let loopback = LoopbackProvider::new();
    let blocking = BlockingProvider::new(loopback).expect("Should create");

    let request = ChatRequest::new("echo").with_message(Message::user("Echo this back!"));

    let response = blocking.chat(&request).expect("Should get response");
    assert_eq!(response.content, "Echo this back!");
}
