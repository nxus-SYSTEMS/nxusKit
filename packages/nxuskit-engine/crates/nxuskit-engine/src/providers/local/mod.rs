//! In-process LLM inference provider
//!
//! Provides local model inference through the `LLMProvider` interface without
//! requiring an external server. Supports selectable backends via feature flags:
//! - `provider-local-llama`: llama.cpp via `llama-cpp-2` crate
//! - `provider-local-mistralrs`: mistral.rs via `mistralrs` crate

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use futures::Stream;
use parking_lot::RwLock;

use crate::error::{NxuskitError, Result};
use crate::parameter_adapter::ParameterAdapter;
use crate::provider::LLMProvider;
use crate::types::{
    ChatRequest, ChatResponse, FinishReason, InferenceMetadata, ModelInfo, ProviderCapabilities,
    StreamChunk, TokenCount, TokenUsage,
};

pub mod backend;
pub mod types;

#[cfg(feature = "provider-local-llama")]
pub mod llama_cpp;

#[cfg(feature = "provider-local-mistralrs")]
pub mod mistralrs_backend;

pub mod model_store;

use backend::{BackendCapabilities, InferenceBackend, LoadedModel};
use types::LocalOptions;

/// In-process LLM inference provider.
///
/// Wraps a selectable [`InferenceBackend`] (llama.cpp or mistral.rs) and
/// presents it through the standard `LLMProvider` interface. Models are loaded
/// from local files (GGUF format) and inference runs in-process.
///
/// # Examples
///
/// ```rust,ignore
/// use nxuskit_engine::providers::local::LocalRuntimeProvider;
///
/// let provider = LocalRuntimeProvider::builder()
///     .model_path("/models/llama-3.2-1b.Q4_K_M.gguf")
///     .n_gpu_layers(0)
///     .build()?;
///
/// let response = provider.chat(&request).await?;
/// ```
pub struct LocalRuntimeProvider {
    /// Provider configuration.
    config: LocalProviderConfig,

    /// The active inference backend.
    backend: Box<dyn InferenceBackend>,

    /// Cache of loaded models keyed by file path.
    /// Uses `Arc` so models can be cloned out of the lock before `.await` points,
    /// avoiding holding a `!Send` lock guard across async boundaries.
    model_cache: Arc<RwLock<HashMap<String, Arc<dyn LoadedModel>>>>,
}

/// Configuration for the local runtime provider.
#[derive(Debug, Clone, Default)]
pub struct LocalProviderConfig {
    /// Path to the primary model file.
    pub model_path: Option<String>,

    /// Backend-specific options.
    pub options: LocalOptions,

    /// Directories to scan for model discovery.
    pub search_paths: Vec<String>,
}

/// Information about a cached (loaded) model.
#[derive(Debug, Clone)]
pub struct CachedModelInfo {
    /// File path used to load the model.
    pub path: String,
    /// Human-readable model description.
    pub description: String,
    /// Approximate memory usage in bytes (if available).
    pub memory_bytes: Option<u64>,
}

/// Builder for constructing a [`LocalRuntimeProvider`].
#[derive(Debug)]
pub struct LocalRuntimeProviderBuilder {
    config: LocalProviderConfig,
}

impl LocalRuntimeProviderBuilder {
    /// Create a new builder with default configuration.
    pub fn new() -> Self {
        Self {
            config: LocalProviderConfig::default(),
        }
    }

    /// Set the path to the model file to load.
    pub fn model_path(mut self, path: impl Into<String>) -> Self {
        self.config.model_path = Some(path.into());
        self
    }

    /// Set the number of GPU layers to offload (-1 = all, 0 = CPU only).
    pub fn n_gpu_layers(mut self, layers: i32) -> Self {
        self.config.options.n_gpu_layers = Some(layers);
        self
    }

    /// Set the context window size in tokens.
    pub fn context_size(mut self, size: u32) -> Self {
        self.config.options.context_size = Some(size);
        self
    }

    /// Set the prompt processing batch size.
    pub fn batch_size(mut self, size: u32) -> Self {
        self.config.options.batch_size = Some(size);
        self
    }

    /// Set the number of CPU threads for inference.
    pub fn threads(mut self, count: u32) -> Self {
        self.config.options.threads = Some(count);
        self
    }

    /// Set the backend to use (`"llama-cpp"` or `"mistralrs"`).
    pub fn backend(mut self, name: impl Into<String>) -> Self {
        self.config.options.backend = Some(name.into());
        self
    }

    /// Add a directory to scan for model discovery.
    pub fn search_path(mut self, path: impl Into<String>) -> Self {
        self.config.search_paths.push(path.into());
        self
    }

    /// Apply provider options from a `LocalOptions` struct.
    pub fn with_options(mut self, opts: LocalOptions) -> Self {
        self.config.options = opts;
        self
    }

    /// Build the provider, selecting the appropriate backend.
    ///
    /// # Errors
    ///
    /// Returns `NxuskitError::Configuration` if the requested backend is not
    /// available (feature not enabled) or if no backends are compiled in.
    pub fn build(self) -> Result<LocalRuntimeProvider> {
        let backend = select_backend(&self.config.options)?;

        Ok(LocalRuntimeProvider {
            config: self.config,
            backend,
            model_cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }
}

impl Default for LocalRuntimeProviderBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalRuntimeProvider {
    /// Create a new builder.
    pub fn builder() -> LocalRuntimeProviderBuilder {
        LocalRuntimeProviderBuilder::new()
    }

    /// Get the backend capabilities.
    fn backend_capabilities(&self) -> BackendCapabilities {
        self.backend.capabilities()
    }

    /// Ensure the model is loaded, loading it on first access.
    ///
    /// Uses the model path from config or from the request's `model` field.
    async fn ensure_model_loaded(&self, model_id: &str) -> Result<()> {
        // Check if already cached (lock is dropped before any .await)
        {
            let cache = self.model_cache.read();
            if cache.contains_key(model_id) {
                return Ok(());
            }
        }

        // Load the model (no lock held across this .await)
        let opts = &self.config.options;
        let loaded = self
            .backend
            .load_model(
                model_id,
                opts.n_gpu_layers.unwrap_or(0),
                opts.context_size,
                opts.batch_size,
                opts.threads,
            )
            .await?;

        let mut cache = self.model_cache.write();
        cache.insert(model_id.to_string(), Arc::from(loaded));
        Ok(())
    }

    /// Get a cloned Arc handle to a cached model, releasing the lock immediately.
    fn get_cached_model(&self, model_path: &str) -> Result<Arc<dyn LoadedModel>> {
        let cache = self.model_cache.read();
        cache.get(model_path).cloned().ok_or_else(|| {
            NxuskitError::InvalidRequest("Model was unloaded unexpectedly".to_string())
        })
    }

    /// Pre-load a model into the cache for faster subsequent requests.
    ///
    /// # Errors
    ///
    /// Returns an error if the model file doesn't exist or can't be loaded.
    pub async fn preload_model(&self, model_path: &str) -> Result<()> {
        self.ensure_model_loaded(model_path).await
    }

    /// Unload a model from the cache, releasing memory.
    ///
    /// Returns `true` if the model was cached and removed, `false` if it
    /// wasn't in the cache. Note: if other tasks still hold an `Arc` to
    /// the model, the memory won't be freed until all references are dropped.
    pub fn unload_model(&self, model_path: &str) -> bool {
        let mut cache = self.model_cache.write();
        cache.remove(model_path).is_some()
    }

    /// Return information about currently cached models.
    pub fn cached_models(&self) -> Vec<CachedModelInfo> {
        let cache = self.model_cache.read();
        cache
            .iter()
            .map(|(path, model)| CachedModelInfo {
                path: path.clone(),
                description: model.description(),
                memory_bytes: model.memory_bytes(),
            })
            .collect()
    }

    /// Get the model path to use for a request.
    fn resolve_model_path(&self, request: &ChatRequest) -> Result<String> {
        // Prefer the request's model field, then config's model_path
        if !request.model.is_empty() {
            Ok(request.model.clone())
        } else if let Some(ref path) = self.config.model_path {
            Ok(path.clone())
        } else {
            Err(NxuskitError::Configuration(
                "No model specified. Set model_path in config or model in request.".to_string(),
            ))
        }
    }
}

impl std::fmt::Debug for LocalRuntimeProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalRuntimeProvider")
            .field("config", &self.config)
            .field("backend", &self.backend.backend_name())
            .finish()
    }
}

#[async_trait]
impl crate::LLMProvider for LocalRuntimeProvider {
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse> {
        // Adapt request parameters to our capabilities (T039/T041 parity)
        let capabilities = self.get_capabilities();
        let adapted = ParameterAdapter::adapt(request, &capabilities);
        let adapted_request = &adapted.request;

        let model_path = self.resolve_model_path(adapted_request)?;
        self.ensure_model_loaded(&model_path).await?;

        // Clone Arc handle to release the lock before the .await
        let model = self.get_cached_model(&model_path)?;
        let result = self
            .backend
            .generate(model.as_ref(), adapted_request)
            .await?;

        // Map GenerateFinishReason to FinishReason
        let finish_reason = match result.finish_reason {
            backend::GenerateFinishReason::Stop | backend::GenerateFinishReason::StopSequence => {
                FinishReason::Stop
            }
            backend::GenerateFinishReason::MaxTokens => FinishReason::Length,
        };

        let usage = TokenUsage::with_actual(
            TokenCount::new(result.prompt_tokens, result.completion_tokens),
            TokenCount::new(result.prompt_tokens, result.completion_tokens),
        );

        let mut metadata = HashMap::new();
        metadata.insert(
            "time_to_first_token_ms".to_string(),
            serde_json::json!(result.time_to_first_token_ms),
        );
        metadata.insert(
            "tokens_per_second".to_string(),
            serde_json::json!(result.tokens_per_second),
        );
        metadata.insert(
            "total_inference_time_ms".to_string(),
            serde_json::json!(result.total_inference_time_ms),
        );
        if let Some(mem) = result.model_memory_bytes {
            metadata.insert("model_memory_bytes".to_string(), serde_json::json!(mem));
        }
        metadata.insert(
            "backend".to_string(),
            serde_json::json!(self.backend.backend_name()),
        );

        let inference_metadata = InferenceMetadata::completed(finish_reason)
            .with_execution_time(result.total_inference_time_ms)
            .with_token_usage(usage.clone())
            .with_provider_metadata(serde_json::json!({
                "backend": self.backend.backend_name(),
                "tokens_per_second": result.tokens_per_second,
                "time_to_first_token_ms": result.time_to_first_token_ms,
            }));

        Ok(ChatResponse {
            content: result.content,
            model: model_path,
            provider: self.provider_name().to_string(),
            usage,
            finish_reason: Some(finish_reason),
            metadata,
            warnings: adapted.warnings,
            logprobs: None,
            inference_metadata,
            tool_calls: None,
        })
    }

    async fn chat_stream(
        &self,
        request: &ChatRequest,
    ) -> Result<Box<dyn Stream<Item = Result<StreamChunk>> + Send + Unpin>> {
        // Adapt request parameters (T040 parity)
        let capabilities = self.get_capabilities();
        let adapted = ParameterAdapter::adapt(request, &capabilities);
        let adapted_request = &adapted.request;

        let model_path = self.resolve_model_path(adapted_request)?;
        self.ensure_model_loaded(&model_path).await?;

        // Clone Arc handle to release the lock before the .await
        let model = self.get_cached_model(&model_path)?;
        let stream = self
            .backend
            .generate_stream(model.as_ref(), adapted_request)
            .await?;

        // The backend returns a Pin<Box<dyn Stream + Send>>.
        // We need to return Box<dyn Stream + Send + Unpin>, so wrap with Box::pin.
        Ok(Box::new(futures::StreamExt::boxed(stream)))
    }

    fn provider_name(&self) -> &str {
        "local"
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        // T061: Use model_store for comprehensive discovery
        let mut models = model_store::discover_models(&self.config.search_paths, true);

        // Also include any backend-discovered models
        let backend_models = self
            .backend
            .list_local_models(&self.config.search_paths)
            .await?;

        // Merge, deduplicating by name
        let existing_names: std::collections::HashSet<String> =
            models.iter().map(|m| m.name.clone()).collect();
        for m in backend_models {
            if !existing_names.contains(&m.name) {
                models.push(m);
            }
        }

        models.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(models)
    }

    fn get_capabilities(&self) -> ProviderCapabilities {
        let backend_caps = self.backend_capabilities();
        ProviderCapabilities {
            supports_system_messages: true,
            supports_streaming: true,
            supports_vision: backend_caps.supports_vision,
            max_stop_sequences: Some(4),
            supports_presence_penalty: false,
            supports_frequency_penalty: false,
            supports_seed: backend_caps.supports_seed,
            supports_logprobs: false,

            supports_streaming_logprobs: false,
            supports_json_mode: backend_caps.supports_json_mode,
            supports_json_schema: backend_caps.supports_json_schema,
            penalty_range: None,
            max_logprobs: None,
        }
    }
}

#[async_trait]
impl crate::provider::ModelLister for LocalRuntimeProvider {
    async fn list_available_models(&self) -> Result<Vec<ModelInfo>> {
        self.list_models().await
    }
}

/// Select the inference backend based on configuration and compiled features.
///
/// If `options.backend` is set, selects that specific backend.
/// Otherwise, selects the first available compiled backend.
fn select_backend(options: &LocalOptions) -> Result<Box<dyn InferenceBackend>> {
    let requested = options.backend.as_deref();

    match requested {
        #[cfg(feature = "provider-local-llama")]
        Some("llama-cpp") => Ok(Box::new(llama_cpp::LlamaCppBackend::new()?)),
        #[cfg(feature = "provider-local-mistralrs")]
        Some("mistralrs") => Ok(Box::new(mistralrs_backend::MistralRsBackend::new()?)),
        Some(name) => Err(NxuskitError::Configuration(format!(
            "Unknown or unavailable backend: '{}'. Available backends: {}",
            name,
            available_backends().join(", ")
        ))),
        None => select_first_available(),
    }
}

/// Select the first available compiled backend.
#[allow(clippy::needless_return)] // `return` needed for mutually-exclusive cfg blocks
fn select_first_available() -> Result<Box<dyn InferenceBackend>> {
    #[cfg(feature = "provider-local-llama")]
    {
        return Ok(Box::new(llama_cpp::LlamaCppBackend::new()?));
    }

    #[cfg(all(
        feature = "provider-local-mistralrs",
        not(feature = "provider-local-llama")
    ))]
    {
        return Ok(Box::new(mistralrs_backend::MistralRsBackend::new()?));
    }

    #[cfg(not(any(feature = "provider-local-llama", feature = "provider-local-mistralrs")))]
    {
        Err(NxuskitError::Configuration(
            "No local inference backend available. Enable 'provider-local-llama' or \
             'provider-local-mistralrs' feature flag."
                .to_string(),
        ))
    }
}

/// List the names of backends compiled into this build.
#[allow(clippy::vec_init_then_push)] // conditional cfg pushes cannot use vec![]
fn available_backends() -> Vec<&'static str> {
    let mut backends = Vec::new();

    #[cfg(feature = "provider-local-llama")]
    backends.push("llama-cpp");

    #[cfg(feature = "provider-local-mistralrs")]
    backends.push("mistralrs");

    backends
}
