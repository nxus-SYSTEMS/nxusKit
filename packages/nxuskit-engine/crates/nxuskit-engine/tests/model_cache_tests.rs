//! Model cache tests — verify pre-load, unload, and concurrent access.
//!
//! These tests require the local provider features and real model files.
//! Most are marked `#[ignore]`.

#![cfg(any(feature = "provider-local-llama", feature = "provider-local-mistralrs"))]

use nxuskit_engine::LLMProvider;
use nxuskit_engine::providers::local::LocalRuntimeProvider;
use nxuskit_engine::types::{ChatRequest, Message};

// ---------------------------------------------------------------------------
// T072: Pre-load and cached_models()
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore]
async fn test_preload_model_appears_in_cache() {
    let model_path = std::env::var("TEST_GGUF_MODEL")
        .unwrap_or_else(|_| "/models/tinyllama-1.1b.Q4_K_M.gguf".to_string());

    let provider = LocalRuntimeProvider::builder()
        .model_path(&model_path)
        .build()
        .expect("build");

    // Before pre-load: cache should be empty
    assert!(
        provider.cached_models().is_empty(),
        "Cache should be empty before preload"
    );

    // Pre-load
    provider.preload_model(&model_path).await.expect("preload");

    // After pre-load: cache should contain the model
    let cached = provider.cached_models();
    assert_eq!(cached.len(), 1, "Should have 1 cached model");
    assert_eq!(cached[0].path, model_path);
}

// ---------------------------------------------------------------------------
// T073: Pre-load then unload
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore]
async fn test_preload_then_unload() {
    let model_path = std::env::var("TEST_GGUF_MODEL")
        .unwrap_or_else(|_| "/models/tinyllama-1.1b.Q4_K_M.gguf".to_string());

    let provider = LocalRuntimeProvider::builder()
        .model_path(&model_path)
        .build()
        .expect("build");

    provider.preload_model(&model_path).await.expect("preload");
    assert_eq!(provider.cached_models().len(), 1);

    let removed = provider.unload_model(&model_path);
    assert!(removed, "Should return true when removing cached model");
    assert!(
        provider.cached_models().is_empty(),
        "Cache should be empty after unload"
    );

    // Unload again should return false
    let removed_again = provider.unload_model(&model_path);
    assert!(!removed_again, "Should return false when model not cached");
}

// ---------------------------------------------------------------------------
// T074: Concurrent access safety
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore]
async fn test_concurrent_chat_requests() {
    let model_path = std::env::var("TEST_GGUF_MODEL")
        .unwrap_or_else(|_| "/models/tinyllama-1.1b.Q4_K_M.gguf".to_string());

    let provider = std::sync::Arc::new(
        LocalRuntimeProvider::builder()
            .model_path(&model_path)
            .build()
            .expect("build"),
    );

    // Pre-load to avoid cold-start races
    provider.preload_model(&model_path).await.expect("preload");

    // Spawn 3 concurrent requests
    let mut handles = Vec::new();
    for i in 0..3 {
        let p = provider.clone();
        let mp = model_path.clone();
        handles.push(tokio::spawn(async move {
            let request = ChatRequest::new(&mp).with_message(Message::user(format!("Say {}", i)));
            p.chat(&request).await
        }));
    }

    // All should succeed without data corruption
    for handle in handles {
        let result = handle.await.expect("join");
        assert!(
            result.is_ok(),
            "Concurrent request should succeed: {:?}",
            result.err()
        );
    }
}
