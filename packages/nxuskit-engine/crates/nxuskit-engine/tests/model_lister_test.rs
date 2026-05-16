//! Contract tests for ModelLister trait
//!
//! These tests verify that ModelLister dispatches correctly through trait objects,
//! which is the primary reason for having a separate trait from LLMProvider.
#![allow(clippy::panic, clippy::print_stdout, clippy::print_stderr)]

use nxuskit_engine::provider::ModelLister;
use nxuskit_engine::providers::{LoopbackProvider, MockProvider};
use nxuskit_engine::types::ModelInfo;

/// T010/T023: Test that ModelLister dispatches correctly through Box<dyn ModelLister>
///
/// This is the key test - it verifies that calling list_available_models()
/// through a trait object returns actual models, not an empty list.
#[tokio::test]
async fn test_model_lister_trait_object_dispatch() {
    // Create a MockProvider which implements ModelLister
    let mock = MockProvider::builder()
        .with_response("Hello!")
        .build()
        .expect("Should build MockProvider");

    // Box it as a trait object
    let lister: Box<dyn ModelLister> = Box::new(mock);

    // Call through the trait object - this MUST dispatch correctly
    let models = lister
        .list_available_models()
        .await
        .expect("Should list models");

    // MockProvider returns 3 models
    assert!(
        !models.is_empty(),
        "Should return actual models, not empty list"
    );
    assert_eq!(models.len(), 3, "MockProvider should return 3 models");

    // Verify model names
    let names: Vec<&str> = models.iter().map(|m| m.name.as_str()).collect();
    assert!(names.contains(&"mock-text-model"));
    assert!(names.contains(&"mock-vision-model"));
    assert!(names.contains(&"mock-minimal-model"));
}

/// T010/T023: Test LoopbackProvider through trait object
#[tokio::test]
async fn test_loopback_model_lister_dispatch() {
    let loopback = LoopbackProvider::new();

    // Box as trait object
    let lister: Box<dyn ModelLister> = Box::new(loopback);

    let models = lister
        .list_available_models()
        .await
        .expect("Should list models");

    // LoopbackProvider has 12 models
    assert_eq!(models.len(), 12, "LoopbackProvider should return 12 models");

    // Verify some known models
    let names: Vec<&str> = models.iter().map(|m| m.name.as_str()).collect();
    assert!(names.contains(&"echo"));
    assert!(names.contains(&"u-turn-json"));
    assert!(names.contains(&"u-turn-error-rate-limit"));
}

/// T024: Test provider registry pattern with Vec<Box<dyn ModelLister>>
#[tokio::test]
async fn test_provider_registry_pattern() {
    // Create multiple providers
    let mock = MockProvider::builder()
        .with_response("Hello!")
        .build()
        .expect("Should build");
    let loopback = LoopbackProvider::new();

    // Store them in a registry as trait objects
    let registry: Vec<Box<dyn ModelLister>> = vec![Box::new(mock), Box::new(loopback)];

    // Iterate and list models from each
    let mut total_models = 0;
    for (i, provider) in registry.iter().enumerate() {
        let models = provider
            .list_available_models()
            .await
            .expect("Should list models");

        assert!(!models.is_empty(), "Provider {} should return models", i);
        total_models += models.len();
    }

    // MockProvider (3) + LoopbackProvider (12) = 15
    assert_eq!(
        total_models, 15,
        "Should have 15 total models from registry"
    );
}

/// T024: Test that different providers return their own correct models
#[tokio::test]
async fn test_registry_returns_correct_models_per_provider() {
    let mock = MockProvider::builder()
        .with_response("Hello!")
        .build()
        .expect("Should build");
    let loopback = LoopbackProvider::new();

    let registry: Vec<(&str, Box<dyn ModelLister>)> =
        vec![("mock", Box::new(mock)), ("loopback", Box::new(loopback))];

    for (name, provider) in registry {
        let models = provider
            .list_available_models()
            .await
            .expect("Should list models");

        match name {
            "mock" => {
                assert_eq!(models.len(), 3);
                assert!(models.iter().any(|m| m.name.contains("mock")));
            }
            "loopback" => {
                assert_eq!(models.len(), 12);
                assert!(models.iter().any(|m| m.name == "echo"));
            }
            _ => panic!("Unknown provider"),
        }
    }
}

/// Test that ModelLister can be used with dynamic dispatch in async context
#[tokio::test]
async fn test_model_lister_async_dispatch() {
    async fn list_models_async(lister: &dyn ModelLister) -> Vec<ModelInfo> {
        lister.list_available_models().await.unwrap_or_default()
    }

    let mock = MockProvider::builder()
        .with_response("Test")
        .build()
        .expect("Should build");

    let models = list_models_async(&mock).await;
    assert_eq!(models.len(), 3);
}

/// Test Send + Sync bounds are satisfied for trait objects
#[tokio::test]
async fn test_model_lister_send_sync() {
    let mock = MockProvider::builder()
        .with_response("Test")
        .build()
        .expect("Should build");

    let lister: Box<dyn ModelLister> = Box::new(mock);

    // Spawn to another task - this verifies Send bound
    let handle = tokio::spawn(async move { lister.list_available_models().await });

    let result = handle.await.expect("Task should complete");
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 3);
}
