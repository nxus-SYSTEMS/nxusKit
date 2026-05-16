//! Types for the local LLM inference provider
//!
//! Contains `LocalOptions`, `ModelMetadata`, and related configuration types.

use serde::{Deserialize, Serialize};

/// Provider-specific configuration for local inference backends.
///
/// Controls which backend engine is used and how it allocates resources
/// for model loading and inference.
///
/// # Examples
///
/// ```rust,ignore
/// use nxuskit_engine::providers::local::types::LocalOptions;
///
/// let opts = LocalOptions {
///     backend: Some("llama-cpp".into()),
///     n_gpu_layers: Some(0),       // CPU only
///     context_size: Some(4096),
///     batch_size: Some(512),
///     threads: None,               // auto-detect
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalOptions {
    /// Backend engine: `"llama-cpp"` or `"mistralrs"`.
    /// If `None`, selects the first available compiled backend.
    pub backend: Option<String>,

    /// GPU layer offloading count.
    /// - `-1` = offload all layers to GPU
    /// - `0` = CPU only (default)
    /// - `N` = offload first N layers
    pub n_gpu_layers: Option<i32>,

    /// Context window size in tokens. Backend default if `None`.
    pub context_size: Option<u32>,

    /// Prompt processing batch size. Backend default if `None`.
    pub batch_size: Option<u32>,

    /// CPU threads for inference. Auto-detect if `None`.
    pub threads: Option<u32>,
}

impl Default for LocalOptions {
    fn default() -> Self {
        Self {
            backend: None,
            n_gpu_layers: Some(0),
            context_size: None,
            batch_size: None,
            threads: None,
        }
    }
}

/// Extended metadata for discovered models.
///
/// Surfaced in `ModelInfo.metadata` for models found by the model store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMetadata {
    /// Provenance: `"local"`, `"ollama"`, or `"explicit"`
    pub source: String,

    /// Absolute path to the model file on disk
    pub file_path: String,

    /// File size in bytes
    pub file_size_bytes: u64,

    /// Model format (e.g., `"gguf"`)
    pub format: String,

    /// Quantization type if detectable (e.g., `"Q4_K_M"`)
    pub quantization: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_options_default() {
        let opts = LocalOptions::default();
        assert!(opts.backend.is_none());
        assert_eq!(opts.n_gpu_layers, Some(0));
        assert!(opts.context_size.is_none());
        assert!(opts.batch_size.is_none());
        assert!(opts.threads.is_none());
    }

    #[test]
    fn local_options_serde_roundtrip() {
        let opts = LocalOptions {
            backend: Some("llama-cpp".into()),
            n_gpu_layers: Some(-1),
            context_size: Some(8192),
            batch_size: Some(1024),
            threads: Some(8),
        };
        let json = serde_json::to_string(&opts).unwrap();
        let decoded: LocalOptions = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.backend.as_deref(), Some("llama-cpp"));
        assert_eq!(decoded.n_gpu_layers, Some(-1));
        assert_eq!(decoded.context_size, Some(8192));
        assert_eq!(decoded.batch_size, Some(1024));
        assert_eq!(decoded.threads, Some(8));
    }

    #[test]
    fn local_options_serde_with_nulls() {
        let json = r#"{"backend":null,"n_gpu_layers":null,"context_size":null,"batch_size":null,"threads":null}"#;
        let opts: LocalOptions = serde_json::from_str(json).unwrap();
        assert!(opts.backend.is_none());
        assert!(opts.n_gpu_layers.is_none());
    }

    #[test]
    fn model_metadata_serde_roundtrip() {
        let meta = ModelMetadata {
            source: "local".into(),
            file_path: "/models/test.gguf".into(),
            file_size_bytes: 4_000_000_000,
            format: "gguf".into(),
            quantization: Some("Q4_K_M".into()),
        };
        let json = serde_json::to_string(&meta).unwrap();
        let decoded: ModelMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.source, "local");
        assert_eq!(decoded.file_size_bytes, 4_000_000_000);
        assert_eq!(decoded.quantization.as_deref(), Some("Q4_K_M"));
    }
}
