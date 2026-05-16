//! Integration tests for ModelInfo functionality
//!
//! These tests verify that the ModelInfo struct and related functionality
//! work correctly across different providers.
#![allow(clippy::panic, clippy::print_stdout, clippy::print_stderr)]

use nxuskit_engine::prelude::*;

#[tokio::test]
async fn test_mock_provider_returns_model_info() {
    let provider = MockProvider::default();
    let models = provider.list_models().await.unwrap();

    assert_eq!(models.len(), 3);

    // Verify first model (text-only)
    assert_eq!(models[0].name, "mock-text-model");
    assert_eq!(models[0].size_bytes, Some(1_000_000_000));
    assert_eq!(models[0].formatted_size(), Some("953.7 MB".to_string()));
    assert!(models[0].description.is_some());
    assert_eq!(models[0].context_window, Some(4_096));
    assert!(!models[0].supports_vision());
    assert_eq!(models[0].modalities(), vec!["text"]);

    // Verify second model (vision-capable)
    assert_eq!(models[1].name, "mock-vision-model");
    assert_eq!(models[1].size_bytes, Some(3_500_000_000));
    assert_eq!(models[1].formatted_size(), Some("3.3 GB".to_string()));
    assert!(models[1].supports_vision());
    assert_eq!(models[1].modalities(), vec!["text", "vision"]);
    assert_eq!(models[1].max_images(), Some(5));

    // Verify third model (minimal with defaults)
    assert_eq!(models[2].name, "mock-minimal-model");
    assert!(!models[2].supports_vision());
    assert_eq!(models[2].modalities(), vec!["text"]);
}

#[tokio::test]
async fn test_model_info_serialization() {
    let provider = MockProvider::default();
    let models = provider.list_models().await.unwrap();

    // Test JSON serialization round-trip
    let json = serde_json::to_string(&models).unwrap();
    let deserialized: Vec<ModelInfo> = serde_json::from_str(&json).unwrap();

    assert_eq!(models, deserialized);
}

#[tokio::test]
async fn test_model_info_with_metadata() {
    let mut info = ModelInfo::new("test-model");
    info.metadata
        .insert("custom_field".to_string(), "custom_value".to_string());

    // Verify metadata is preserved during serialization
    let json = serde_json::to_string(&info).unwrap();
    let deserialized: ModelInfo = serde_json::from_str(&json).unwrap();

    assert_eq!(info, deserialized);
    assert_eq!(
        deserialized.metadata.get("custom_field"),
        Some(&"custom_value".to_string())
    );
}

#[tokio::test]
async fn test_model_info_optional_fields() {
    // Test with all fields
    let mut full_info = ModelInfo::with_size("full-model", 5_000_000_000);
    full_info.description = Some("A test model".to_string());
    full_info.context_window = Some(128_000);
    full_info
        .metadata
        .insert("version".to_string(), "1.0".to_string());

    assert_eq!(full_info.name, "full-model");
    assert_eq!(full_info.size_bytes, Some(5_000_000_000));
    assert_eq!(full_info.description, Some("A test model".to_string()));
    assert_eq!(full_info.context_window, Some(128_000));
    assert_eq!(full_info.formatted_size(), Some("4.7 GB".to_string()));
    assert_eq!(
        full_info.formatted_context_window(),
        Some("128K".to_string())
    );

    // Test with minimal fields
    let minimal_info = ModelInfo::new("minimal-model");
    assert_eq!(minimal_info.name, "minimal-model");
    assert_eq!(minimal_info.size_bytes, None);
    assert_eq!(minimal_info.description, None);
    assert_eq!(minimal_info.context_window, None);
    assert_eq!(minimal_info.formatted_size(), None);
    assert_eq!(minimal_info.formatted_context_window(), None);
}

#[tokio::test]
#[ignore] // Requires Ollama to be running locally
async fn test_ollama_returns_model_info_with_size() {
    let provider = OllamaProvider::builder().build().unwrap();

    match provider.list_models().await {
        Ok(models) => {
            // If Ollama is running and has models, verify they have ModelInfo structure
            if !models.is_empty() {
                let model = &models[0];
                assert!(!model.name.is_empty());

                // Ollama should provide size information
                if let Some(size) = model.size_bytes {
                    assert!(size > 0);
                    assert!(model.formatted_size().is_some());
                }

                // Check for Ollama-specific metadata
                println!(
                    "Ollama model: {} - size: {} - metadata: {:?}",
                    model.name,
                    model.formatted_size().unwrap_or_default(),
                    model.metadata
                );
            }
        }
        Err(e) => {
            // Expected if Ollama is not running
            println!("Ollama not available (expected): {}", e);
        }
    }
}

#[test]
fn test_model_info_formatting() {
    // Test size formatting
    let model_bytes = ModelInfo::with_size("tiny", 512);
    assert_eq!(model_bytes.formatted_size(), Some("512 B".to_string()));

    let model_kb = ModelInfo::with_size("small", 5_120);
    assert_eq!(model_kb.formatted_size(), Some("5.0 KB".to_string()));

    let model_mb = ModelInfo::with_size("medium", 50_000_000);
    assert_eq!(model_mb.formatted_size(), Some("47.7 MB".to_string()));

    let model_gb = ModelInfo::with_size("large", 4_000_000_000);
    assert_eq!(model_gb.formatted_size(), Some("3.7 GB".to_string()));

    let model_tb = ModelInfo::with_size("huge", 5_000_000_000_000);
    assert_eq!(model_tb.formatted_size(), Some("4.5 TB".to_string()));

    // Test context window formatting
    let mut ctx_plain = ModelInfo::new("ctx-512");
    ctx_plain.context_window = Some(512);
    assert_eq!(
        ctx_plain.formatted_context_window(),
        Some("512".to_string())
    );

    let mut ctx_k = ModelInfo::new("ctx-8k");
    ctx_k.context_window = Some(8_000);
    assert_eq!(ctx_k.formatted_context_window(), Some("8K".to_string()));

    let mut ctx_m = ModelInfo::new("ctx-2m");
    ctx_m.context_window = Some(2_000_000);
    assert_eq!(ctx_m.formatted_context_window(), Some("2M".to_string()));
}

#[tokio::test]
#[ignore] // Requires valid API credentials - run with: cargo test --ignored
async fn test_provider_specific_metadata() {
    // Test Claude metadata - requires ANTHROPIC_API_KEY env var
    if let Ok(api_key) = std::env::var("ANTHROPIC_API_KEY") {
        let claude_provider = ClaudeProvider::builder().api_key(api_key).build().unwrap();
        let claude_models = claude_provider.list_models().await.unwrap();

        // Verify we got models back
        assert!(!claude_models.is_empty(), "Claude should return models");

        // All models should have basic metadata from the API
        for model in &claude_models {
            assert!(
                model.metadata.contains_key("created_at"),
                "Claude models should have created_at"
            );
            assert!(
                model.metadata.contains_key("display_name"),
                "Claude models should have display_name"
            );

            // Known models should have enriched metadata (version, family)
            // but new/unknown models may not - that's OK
            if let (Some(version), Some(family)) =
                (model.metadata.get("version"), model.metadata.get("family"))
            {
                println!(
                    "Claude model: {} - version: {}, family: {}",
                    model.name, version, family
                );
            } else {
                println!(
                    "Claude model: {} - display_name: {}",
                    model.name,
                    model
                        .metadata
                        .get("display_name")
                        .unwrap_or(&"unknown".to_string())
                );
            }
        }
    } else {
        println!("Skipping Claude test - ANTHROPIC_API_KEY not set");
    }

    // Test OpenAI metadata - requires OPENAI_API_KEY env var
    if let Ok(api_key) = std::env::var("OPENAI_API_KEY") {
        let openai_provider = OpenAIProvider::builder().api_key(api_key).build().unwrap();
        let openai_models = openai_provider.list_models().await.unwrap();

        // Verify we got models back
        assert!(!openai_models.is_empty(), "OpenAI should return models");

        // Check that models have expected structure
        for model in &openai_models {
            // OpenAI models should have owner metadata
            if let Some(owner) = model.metadata.get("owned_by") {
                println!("OpenAI model: {} - owned_by: {}", model.name, owner);
            } else {
                println!("OpenAI model: {}", model.name);
            }
        }
    } else {
        println!("Skipping OpenAI test - OPENAI_API_KEY not set");
    }
}

#[test]
fn test_metadata_forward_compatibility() {
    // Test that adding unknown metadata fields doesn't break deserialization
    let json = r#"{
        "name": "future-model",
        "size_bytes": 5000000000,
        "context_window": 256000,
        "description": "A future model",
        "metadata": {
            "version": "5.0",
            "family": "future",
            "unknown_field_1": "value1",
            "unknown_field_2": "value2",
            "capability": "multimodal"
        }
    }"#;

    let model: ModelInfo = serde_json::from_str(json).unwrap();
    assert_eq!(model.name, "future-model");
    assert_eq!(model.size_bytes, Some(5_000_000_000));
    assert_eq!(model.context_window, Some(256_000));
    assert_eq!(model.description, Some("A future model".to_string()));

    // All metadata fields should be preserved
    assert_eq!(model.metadata.len(), 5);
    assert_eq!(model.metadata.get("version"), Some(&"5.0".to_string()));
    assert_eq!(model.metadata.get("family"), Some(&"future".to_string()));
    assert_eq!(
        model.metadata.get("unknown_field_1"),
        Some(&"value1".to_string())
    );
    assert_eq!(
        model.metadata.get("unknown_field_2"),
        Some(&"value2".to_string())
    );
    assert_eq!(
        model.metadata.get("capability"),
        Some(&"multimodal".to_string())
    );
}

#[test]
fn test_metadata_serialization_preserves_all_fields() {
    let mut info = ModelInfo::new("test-model");
    info.metadata
        .insert("custom1".to_string(), "value1".to_string());
    info.metadata
        .insert("custom2".to_string(), "value2".to_string());
    info.metadata
        .insert("custom3".to_string(), "value3".to_string());

    // Serialize and deserialize
    let json = serde_json::to_string(&info).unwrap();
    let deserialized: ModelInfo = serde_json::from_str(&json).unwrap();

    // All metadata should be preserved
    assert_eq!(deserialized.metadata.len(), 3);
    assert_eq!(
        deserialized.metadata.get("custom1"),
        Some(&"value1".to_string())
    );
    assert_eq!(
        deserialized.metadata.get("custom2"),
        Some(&"value2".to_string())
    );
    assert_eq!(
        deserialized.metadata.get("custom3"),
        Some(&"value3".to_string())
    );
}
