//! Model listing tests for nxuskit.
//!
//! Tests marked `#[ignore]` require `libnxuskit` at runtime.
//! Run them with: `NXUSKIT_LIB_DIR=/path/to/lib cargo test --test models -- --ignored`

use nxuskit::{ModelInfo, NxuskitProvider, ProviderConfig};

/// Verify list_models returns a non-empty list with valid ModelInfo entries.
#[test]
#[ignore = "requires libnxuskit runtime"]
fn list_models_returns_valid_entries() {
    let config = ProviderConfig {
        provider_type: "mock".into(),
        ..Default::default()
    };
    let provider = NxuskitProvider::new(config).expect("failed to create mock provider");

    let models = provider.list_models().expect("list_models failed");
    assert!(
        !models.is_empty(),
        "mock provider should return at least one model"
    );

    for model in &models {
        assert!(!model.id.is_empty(), "model id should not be empty");
        assert!(!model.name.is_empty(), "model name should not be empty");
    }
}

// --- Tests that run without libnxuskit ---

/// Verify ModelInfo serde round-trip.
#[test]
fn model_info_serde_roundtrip() {
    let model = ModelInfo {
        id: "gpt-4".into(),
        name: "GPT-4".into(),
        description: None,
        size_bytes: Some(1_000_000),
        context_window: Some(128_000),
        metadata: Default::default(),
    };
    let json = serde_json::to_string(&model).unwrap();
    let parsed: ModelInfo = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.id, "gpt-4");
    assert_eq!(parsed.name, "GPT-4");
    assert_eq!(parsed.size_bytes, Some(1_000_000));
    assert_eq!(parsed.context_window, Some(128_000));
}

/// Verify ModelInfo deserialization with optional fields absent.
#[test]
fn model_info_minimal() {
    let json = r#"{"id": "test-model", "name": "Test"}"#;
    let model: ModelInfo = serde_json::from_str(json).unwrap();
    assert_eq!(model.id, "test-model");
    assert_eq!(model.name, "Test");
    assert!(model.size_bytes.is_none());
    assert!(model.context_window.is_none());
}

/// Verify deserialization of a JSON array of ModelInfo (as list_models returns).
#[test]
fn model_info_array_deserialization() {
    let json = r#"[
        {"id": "model-a", "name": "Model A"},
        {"id": "model-b", "name": "Model B", "size_bytes": 500000, "context_window": 4096}
    ]"#;
    let models: Vec<ModelInfo> = serde_json::from_str(json).unwrap();
    assert_eq!(models.len(), 2);
    assert_eq!(models[0].id, "model-a");
    assert_eq!(models[1].context_window, Some(4096));
}

/// Verify ModelInfo deserialization when `id` is absent (Ollama format).
///
/// The C SDK's Ollama provider returns `name` but no `id` field.
/// The wrapper should populate `id` from `name` automatically.
#[test]
fn model_info_missing_id_falls_back_to_name() {
    let json = r#"{"name": "qwen3:14b", "size_bytes": 9276198565}"#;
    let model: ModelInfo = serde_json::from_str(json).unwrap();
    assert_eq!(model.id, "qwen3:14b");
    assert_eq!(model.name, "qwen3:14b");
    assert_eq!(model.size_bytes, Some(9_276_198_565));
}

/// Verify array deserialization of Ollama-format models (no `id` field).
#[test]
fn model_info_array_ollama_format() {
    let json = r#"[
        {"name": "qwen3:14b", "size_bytes": 9276198565},
        {"name": "llama3:8b", "size_bytes": 4661224676}
    ]"#;
    let models: Vec<ModelInfo> = serde_json::from_str(json).unwrap();
    assert_eq!(models.len(), 2);
    assert_eq!(models[0].id, "qwen3:14b");
    assert_eq!(models[0].name, "qwen3:14b");
    assert_eq!(models[1].id, "llama3:8b");
}

/// When both `id` and `name` are present, `id` takes precedence.
#[test]
fn model_info_id_takes_precedence_over_name() {
    let json = r#"{"id": "gpt-4o-2024-08-06", "name": "GPT-4o"}"#;
    let model: ModelInfo = serde_json::from_str(json).unwrap();
    assert_eq!(model.id, "gpt-4o-2024-08-06");
    assert_eq!(model.name, "GPT-4o");
}
