//! mistral.rs inference backend via `mistralrs` crate
//!
//! Feature-gated behind `provider-local-mistralrs`.
//!
//! Provides high-performance local LLM inference with features like:
//! - PagedAttention for efficient KV-cache management
//! - ISQ (In-Situ Quantization) for on-the-fly quantization
//! - Automatic chat template detection from model metadata
//! - Speculative decoding support

use std::collections::HashMap;
use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;

use crate::error::{NxuskitError, Result};
use crate::types::{ChatRequest, ModelInfo, StreamChunk};

use super::backend::{BackendCapabilities, GenerateResponse, InferenceBackend, LoadedModel};

/// mistral.rs inference backend.
///
/// Uses the `mistralrs` crate for high-performance local inference with
/// PagedAttention and other advanced features.
#[derive(Debug)]
pub struct MistralRsBackend {
    // Backend state will be initialized when models are loaded
}

impl MistralRsBackend {
    /// Create a new mistral.rs backend.
    pub fn new() -> Result<Self> {
        Ok(Self {})
    }
}

/// A model loaded through mistral.rs.
#[derive(Debug)]
struct MistralRsLoadedModel {
    model_path: String,
    // The actual mistralrs::Model handle would go here
}

impl LoadedModel for MistralRsLoadedModel {
    fn description(&self) -> String {
        format!("mistralrs:{}", self.model_path)
    }

    fn memory_bytes(&self) -> Option<u64> {
        None // TODO: Extract from mistralrs model stats
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[async_trait]
impl InferenceBackend for MistralRsBackend {
    async fn load_model(
        &self,
        model_path: &str,
        _n_gpu_layers: i32,
        _context_size: Option<u32>,
        _batch_size: Option<u32>,
        _threads: Option<u32>,
    ) -> Result<Box<dyn LoadedModel>> {
        // TODO: Use mistralrs::ModelBuilder to load the model
        // For now, create a stub that validates the path exists
        if !std::path::Path::new(model_path).exists() {
            return Err(NxuskitError::Configuration(format!(
                "Model file not found: '{}'. Provide a valid GGUF model path.",
                model_path
            )));
        }

        Ok(Box::new(MistralRsLoadedModel {
            model_path: model_path.to_string(),
        }))
    }

    async fn generate(
        &self,
        _model: &dyn LoadedModel,
        _request: &ChatRequest,
    ) -> Result<GenerateResponse> {
        // TODO: Implement actual mistralrs generation
        Err(NxuskitError::Configuration(
            "mistral.rs backend generation not yet fully implemented. \
             Use provider-local-llama for production inference."
                .to_string(),
        ))
    }

    async fn generate_stream(
        &self,
        _model: &dyn LoadedModel,
        _request: &ChatRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>> {
        // TODO: Implement actual mistralrs streaming
        Err(NxuskitError::Configuration(
            "mistral.rs backend streaming not yet fully implemented. \
             Use provider-local-llama for production inference."
                .to_string(),
        ))
    }

    async fn list_local_models(&self, search_paths: &[String]) -> Result<Vec<ModelInfo>> {
        // Scan search paths for GGUF files (same as llama-cpp)
        let mut models = Vec::new();
        for dir in search_paths {
            let path = std::path::Path::new(dir);
            if !path.is_dir() {
                continue;
            }
            if let Ok(entries) = std::fs::read_dir(path) {
                for entry in entries.flatten() {
                    let file_path = entry.path();
                    if file_path.extension().is_some_and(|e| e == "gguf") {
                        let name = file_path
                            .file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_default();
                        let size = std::fs::metadata(&file_path).ok().map(|m| m.len());

                        let mut metadata = HashMap::new();
                        metadata.insert("backend".to_string(), "mistralrs".to_string());
                        metadata
                            .insert("path".to_string(), file_path.to_string_lossy().to_string());

                        models.push(ModelInfo {
                            name,
                            size_bytes: size,
                            description: Some("GGUF model (mistral.rs)".to_string()),
                            context_window: None,
                            metadata,
                        });
                    }
                }
            }
        }
        Ok(models)
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            supports_vision: false,
            supports_seed: true,
            supports_json_mode: true,
            supports_json_schema: false,
            supports_stop_sequences: true,
            max_context_size: None,
        }
    }

    fn backend_name(&self) -> &str {
        "mistralrs"
    }
}
