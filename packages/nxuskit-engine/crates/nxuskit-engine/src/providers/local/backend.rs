//! Inference backend abstraction
//!
//! Internal trait for swappable inference engines (llama.cpp, mistral.rs).
//! Not part of the public API — consumers interact through `LocalRuntimeProvider`.

use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::types::{ChatRequest, ModelInfo, StreamChunk};

/// Capabilities reported by a specific inference backend.
///
/// Informs the `ProviderCapabilities` returned by `LocalRuntimeProvider`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendCapabilities {
    /// Whether the backend supports JSON-constrained output via grammars.
    pub supports_json_mode: bool,

    /// Whether the backend supports JSON schema-constrained generation.
    pub supports_json_schema: bool,

    /// Whether the backend supports a deterministic seed.
    pub supports_seed: bool,

    /// Whether the backend supports stop sequences.
    pub supports_stop_sequences: bool,

    /// Maximum context window the backend can handle (model-dependent).
    pub max_context_size: Option<u32>,

    /// Whether the backend supports multimodal (vision) input.
    pub supports_vision: bool,
}

impl Default for BackendCapabilities {
    fn default() -> Self {
        Self {
            supports_json_mode: false,
            supports_json_schema: false,
            supports_seed: true,
            supports_stop_sequences: true,
            max_context_size: None,
            supports_vision: false,
        }
    }
}

/// Result of a full (non-streaming) generation.
#[derive(Debug, Clone)]
pub struct GenerateResponse {
    /// Generated text content.
    pub content: String,

    /// Number of tokens in the prompt.
    pub prompt_tokens: u32,

    /// Number of generated tokens.
    pub completion_tokens: u32,

    /// Time to first token in milliseconds.
    pub time_to_first_token_ms: u64,

    /// Total inference time in milliseconds.
    pub total_inference_time_ms: u64,

    /// Tokens generated per second.
    pub tokens_per_second: f64,

    /// Model memory footprint in bytes.
    pub model_memory_bytes: Option<u64>,

    /// The finish reason (e.g., natural stop, max tokens).
    pub finish_reason: GenerateFinishReason,
}

/// Why generation stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenerateFinishReason {
    /// Natural end-of-sequence token.
    Stop,
    /// Hit max_tokens limit.
    MaxTokens,
    /// Hit a stop sequence.
    StopSequence,
}

/// Handle to a model loaded into memory by a backend.
///
/// Backends return this from `load_model()` and accept it in `generate()`.
/// The inner representation is backend-specific and opaque to callers.
pub trait LoadedModel: Send + Sync + std::fmt::Debug {
    /// Human-readable description (model name, path, quantization).
    fn description(&self) -> String;

    /// Memory footprint of the loaded model in bytes, if known.
    fn memory_bytes(&self) -> Option<u64>;

    /// Downcast support for backend-specific access.
    fn as_any(&self) -> &dyn std::any::Any;
}

/// Internal trait abstracting over inference engine backends.
///
/// Each backend (llama.cpp, mistral.rs) implements this trait. The
/// `LocalRuntimeProvider` delegates to whichever backend is active.
///
/// This trait is `Send + Sync` to support concurrent requests.
#[async_trait]
pub trait InferenceBackend: Send + Sync {
    /// Load a model file into memory with the given options.
    ///
    /// # Arguments
    /// * `model_path` - Absolute path to the model file (e.g., GGUF)
    /// * `request` - Chat request containing model parameters
    ///
    /// # Errors
    /// Returns `NxuskitError::Configuration` for invalid paths or formats,
    /// `NxuskitError::InvalidRequest` for unsupported models or OOM.
    async fn load_model(
        &self,
        model_path: &str,
        n_gpu_layers: i32,
        context_size: Option<u32>,
        batch_size: Option<u32>,
        threads: Option<u32>,
    ) -> Result<Box<dyn LoadedModel>>;

    /// Run full (non-streaming) text generation.
    ///
    /// # Arguments
    /// * `model` - A previously loaded model handle
    /// * `request` - The chat request with messages, temperature, etc.
    ///
    /// # Errors
    /// Returns `NxuskitError::InvalidRequest` for generation failures.
    async fn generate(
        &self,
        model: &dyn LoadedModel,
        request: &ChatRequest,
    ) -> Result<GenerateResponse>;

    /// Run streaming text generation, yielding tokens as they are produced.
    ///
    /// Returns a pinned stream of `StreamChunk` results. Each chunk contains
    /// a single token's worth of text. The final chunk has `finish_reason`
    /// and `usage` set.
    ///
    /// # Arguments
    /// * `model` - A previously loaded model handle
    /// * `request` - The chat request with messages, temperature, etc.
    ///
    /// # Errors
    /// Returns `NxuskitError::InvalidRequest` for generation failures.
    async fn generate_stream(
        &self,
        model: &dyn LoadedModel,
        request: &ChatRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>>;

    /// Scan filesystem paths for models this backend can load.
    ///
    /// Returns `ModelInfo` entries for each discovered model file.
    async fn list_local_models(&self, search_paths: &[String]) -> Result<Vec<ModelInfo>>;

    /// Report what this backend supports.
    fn capabilities(&self) -> BackendCapabilities;

    /// Backend identifier (e.g., `"llama-cpp"`, `"mistralrs"`).
    fn backend_name(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_capabilities_default() {
        let caps = BackendCapabilities::default();
        assert!(!caps.supports_json_mode);
        assert!(!caps.supports_json_schema);
        assert!(caps.supports_seed);
        assert!(caps.supports_stop_sequences);
        assert!(caps.max_context_size.is_none());
        assert!(!caps.supports_vision);
    }

    #[test]
    fn backend_capabilities_serde_roundtrip() {
        let caps = BackendCapabilities {
            supports_json_mode: true,
            supports_json_schema: false,
            supports_seed: true,
            supports_stop_sequences: true,
            max_context_size: Some(32768),
            supports_vision: false,
        };
        let json = serde_json::to_string(&caps).unwrap();
        let decoded: BackendCapabilities = serde_json::from_str(&json).unwrap();
        assert!(decoded.supports_json_mode);
        assert_eq!(decoded.max_context_size, Some(32768));
    }

    #[test]
    fn generate_finish_reason_equality() {
        assert_eq!(GenerateFinishReason::Stop, GenerateFinishReason::Stop);
        assert_ne!(GenerateFinishReason::Stop, GenerateFinishReason::MaxTokens);
        assert_ne!(
            GenerateFinishReason::MaxTokens,
            GenerateFinishReason::StopSequence
        );
    }
}
