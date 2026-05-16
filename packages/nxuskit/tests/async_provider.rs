//! Tests for async provider methods and the `AsyncProvider` trait.
//!
//! Tests marked `#[ignore]` require `libnxuskit` at runtime.
//! Run them with: `NXUSKIT_LIB_DIR=/path/to/lib cargo test --test async_provider -- --ignored`

use nxuskit::{
    AsyncProvider, ChatRequest, ChatResponse, FinishReason, Message, ModelInfo, NxuskitError,
    NxuskitProvider, ProviderConfig, Role, StreamReceiver,
};
use std::sync::Arc;

// =========================================================================
// User Story 1: Non-Blocking Chat Requests
// =========================================================================

/// T007: Verify that `chat_async()` returns a non-empty response.
#[tokio::test]
#[ignore = "requires libnxuskit runtime"]
async fn async_chat_returns_response() {
    let config = ProviderConfig {
        provider_type: "mock".into(),
        ..Default::default()
    };
    let provider = NxuskitProvider::new(config).expect("failed to create mock provider");

    let request = ChatRequest {
        model: "mock-model".into(),
        messages: vec![Message {
            role: Role::User,
            content: "Hello from async!".into(),
        }],
        ..Default::default()
    };

    let response = provider
        .chat_async(request)
        .await
        .expect("chat_async failed");

    assert!(
        !response.content.is_empty(),
        "async response content should not be empty"
    );
    assert!(
        !response.model.is_empty(),
        "async response model should not be empty"
    );
}

/// T008: Verify that `chat_async()` propagates errors as `NxuskitError`, not panics.
#[tokio::test]
#[ignore = "requires libnxuskit runtime"]
async fn async_chat_error_propagation() {
    // Use an invalid provider type to trigger a configuration error.
    let config = ProviderConfig {
        provider_type: "nonexistent-provider-that-should-fail".into(),
        ..Default::default()
    };

    // Provider creation may fail — that's fine, it tests a different path.
    // If creation succeeds (mock SDK accepts anything), send a bad request.
    match NxuskitProvider::new(config) {
        Err(e) => {
            // Error during creation — this is an acceptable error path.
            assert!(
                !format!("{e}").is_empty(),
                "error message should not be empty"
            );
        }
        Ok(provider) => {
            // Provider created — send request with empty model to trigger error.
            let request = ChatRequest {
                model: String::new(),
                messages: vec![],
                ..Default::default()
            };
            let result = provider.chat_async(request).await;
            assert!(
                result.is_err(),
                "chat_async with invalid request should return Err, not panic"
            );
        }
    }
}

/// T009: Verify concurrent `chat_async()` tasks on a shared provider.
#[tokio::test]
#[ignore = "requires libnxuskit runtime"]
async fn async_chat_concurrent_tasks() {
    let config = ProviderConfig {
        provider_type: "mock".into(),
        ..Default::default()
    };
    let provider = Arc::new(NxuskitProvider::new(config).expect("failed to create mock provider"));

    let mut handles = vec![];
    for i in 0..4 {
        let p = provider.clone();
        handles.push(tokio::spawn(async move {
            let request = ChatRequest {
                model: "mock-model".into(),
                messages: vec![Message {
                    role: Role::User,
                    content: format!("Concurrent task {i}").into(),
                }],
                ..Default::default()
            };
            p.chat_async(request)
                .await
                .unwrap_or_else(|e| panic!("chat_async failed in task {i}: {e}"))
        }));
    }

    for (i, handle) in handles.into_iter().enumerate() {
        let response = handle
            .await
            .unwrap_or_else(|e| panic!("task {i} panicked: {e}"));
        assert!(
            !response.content.is_empty(),
            "response from task {i} should not be empty"
        );
    }
}

// =========================================================================
// User Story 2: Non-Blocking Model Discovery
// =========================================================================

/// T012: Verify that `list_models_async()` returns a Vec of models.
#[tokio::test]
#[ignore = "requires libnxuskit runtime"]
async fn async_list_models_returns_models() {
    let config = ProviderConfig {
        provider_type: "mock".into(),
        ..Default::default()
    };
    let provider = NxuskitProvider::new(config).expect("failed to create mock provider");

    let models: Vec<ModelInfo> = provider
        .list_models_async()
        .await
        .expect("list_models_async failed");

    // Mock provider should return at least an empty vec (no panic).
    // The important thing is that it returns Ok, not Err.
    let _ = models;
}

// =========================================================================
// User Story 3: Polymorphic Provider Dispatch
// =========================================================================

/// T015: Verify `AsyncProvider::chat()` works through a trait object.
#[tokio::test]
#[ignore = "requires libnxuskit runtime"]
async fn trait_object_chat() {
    let config = ProviderConfig {
        provider_type: "mock".into(),
        ..Default::default()
    };
    let provider = NxuskitProvider::new(config).expect("failed to create mock provider");
    let dyn_provider: Box<dyn AsyncProvider> = Box::new(provider);

    let request = ChatRequest {
        model: "mock-model".into(),
        messages: vec![Message {
            role: Role::User,
            content: "Hello via trait object!".into(),
        }],
        ..Default::default()
    };

    let response = dyn_provider
        .chat(request)
        .await
        .expect("AsyncProvider::chat() failed");

    assert!(
        !response.content.is_empty(),
        "trait object chat response should not be empty"
    );
}

/// T016: Verify `AsyncProvider::list_models()` works through a trait object.
#[tokio::test]
#[ignore = "requires libnxuskit runtime"]
async fn trait_object_list_models() {
    let config = ProviderConfig {
        provider_type: "mock".into(),
        ..Default::default()
    };
    let provider = NxuskitProvider::new(config).expect("failed to create mock provider");
    let dyn_provider: Box<dyn AsyncProvider> = Box::new(provider);

    let models = dyn_provider
        .list_models()
        .await
        .expect("AsyncProvider::list_models() failed");

    let _ = models; // Mock may return empty; no panic is the key assertion.
}

/// T017: Verify `Box<dyn AsyncProvider>` can be moved into a spawned task.
#[tokio::test]
#[ignore = "requires libnxuskit runtime"]
async fn trait_object_send_across_tasks() {
    let config = ProviderConfig {
        provider_type: "mock".into(),
        ..Default::default()
    };
    let provider = NxuskitProvider::new(config).expect("failed to create mock provider");
    let dyn_provider: Box<dyn AsyncProvider> = Box::new(provider);

    let handle = tokio::spawn(async move {
        let request = ChatRequest {
            model: "mock-model".into(),
            messages: vec![Message {
                role: Role::User,
                content: "Hello from spawned task!".into(),
            }],
            ..Default::default()
        };
        dyn_provider.chat(request).await
    });

    let response = handle
        .await
        .expect("spawned task panicked")
        .expect("chat through trait object failed");

    assert!(!response.content.is_empty());
}

/// T018: Verify true polymorphism with multiple AsyncProvider implementors.
///
/// Defines a minimal MockAsyncProvider and uses it alongside NxuskitProvider
/// in a `Vec<Box<dyn AsyncProvider>>` to demonstrate polymorphic dispatch.
#[tokio::test]
#[ignore = "requires libnxuskit runtime"]
async fn trait_polymorphism_with_mock() {
    // A minimal mock that implements AsyncProvider with hardcoded responses.
    struct MockAsyncProvider;

    #[async_trait::async_trait]
    impl AsyncProvider for MockAsyncProvider {
        async fn chat(&self, _request: ChatRequest) -> Result<ChatResponse, NxuskitError> {
            Ok(ChatResponse {
                content: "mock-async-response".into(),
                model: "mock-async-model".into(),
                provider: "mock-async".into(),
                usage: Default::default(),
                finish_reason: Some(FinishReason::Stop),
                metadata: Default::default(),
                warnings: vec![],
                logprobs: None,
                tool_calls: None,
                inference_metadata: None,
            })
        }

        fn chat_stream(&self, _request: ChatRequest) -> Result<StreamReceiver, NxuskitError> {
            Err(NxuskitError::Internal {
                message: "streaming not supported by mock".into(),
            })
        }

        async fn list_models(&self) -> Result<Vec<ModelInfo>, NxuskitError> {
            Ok(vec![ModelInfo {
                id: "mock-async-model".into(),
                name: "Mock Async Model".into(),
                description: None,
                size_bytes: None,
                context_window: None,
                metadata: Default::default(),
            }])
        }
    }

    // Create a heterogeneous collection of AsyncProvider implementors.
    let config = ProviderConfig {
        provider_type: "mock".into(),
        ..Default::default()
    };
    let real_provider = NxuskitProvider::new(config).expect("failed to create mock provider");

    let providers: Vec<Box<dyn AsyncProvider>> =
        vec![Box::new(real_provider), Box::new(MockAsyncProvider)];

    for (i, provider) in providers.iter().enumerate() {
        let request = ChatRequest {
            model: "test-model".into(),
            messages: vec![Message {
                role: Role::User,
                content: format!("Polymorphic test {i}").into(),
            }],
            ..Default::default()
        };
        let response = provider
            .chat(request)
            .await
            .unwrap_or_else(|e| panic!("provider {i} chat failed: {e}"));
        assert!(
            !response.content.is_empty(),
            "provider {i} returned empty content"
        );
    }
}

// =========================================================================
// Phase 6: Edge Case — Mixed Sync + Async Concurrent Access
// =========================================================================

/// T024: Verify mixed sync and async concurrent access on the same provider.
#[tokio::test]
#[ignore = "requires libnxuskit runtime"]
async fn mixed_sync_async_concurrent() {
    let config = ProviderConfig {
        provider_type: "mock".into(),
        ..Default::default()
    };
    let provider = Arc::new(NxuskitProvider::new(config).expect("failed to create mock provider"));

    // Spawn 2 async chat_async() tasks.
    let mut async_handles = vec![];
    for i in 0..2 {
        let p = provider.clone();
        async_handles.push(tokio::spawn(async move {
            let request = ChatRequest {
                model: "mock-model".into(),
                messages: vec![Message {
                    role: Role::User,
                    content: format!("Async task {i}").into(),
                }],
                ..Default::default()
            };
            p.chat_async(request).await
        }));
    }

    // Spawn 2 sync chat() calls on std::thread::spawn.
    let mut sync_handles = vec![];
    for i in 0..2 {
        let p = provider.clone();
        sync_handles.push(std::thread::spawn(move || {
            let request = ChatRequest {
                model: "mock-model".into(),
                messages: vec![Message {
                    role: Role::User,
                    content: format!("Sync thread {i}").into(),
                }],
                ..Default::default()
            };
            p.chat(request)
        }));
    }

    // Collect all results.
    for (i, handle) in async_handles.into_iter().enumerate() {
        let response = handle
            .await
            .unwrap_or_else(|e| panic!("async task {i} panicked: {e}"))
            .unwrap_or_else(|e| panic!("async task {i} failed: {e}"));
        assert!(!response.content.is_empty());
    }
    for (i, handle) in sync_handles.into_iter().enumerate() {
        let response = handle
            .join()
            .unwrap_or_else(|_| panic!("sync thread {i} panicked"))
            .unwrap_or_else(|e| panic!("sync thread {i} failed: {e}"));
        assert!(!response.content.is_empty());
    }
}
