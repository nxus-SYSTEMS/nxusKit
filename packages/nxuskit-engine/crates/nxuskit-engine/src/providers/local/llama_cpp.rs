//! llama.cpp inference backend via `llama-cpp-2` crate
//!
//! Feature-gated behind `provider-local-llama`.

use std::num::NonZeroU32;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;

use parking_lot::Mutex;

use async_trait::async_trait;
use futures::Stream;

use crate::error::{NxuskitError, Result};
use crate::types::{ChatRequest, ModelInfo, StreamChunk, TokenCount, TokenUsage};

use super::backend::{
    BackendCapabilities, GenerateFinishReason, GenerateResponse, InferenceBackend, LoadedModel,
};

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;

/// A loaded llama.cpp model handle.
#[derive(Debug)]
pub struct LlamaCppLoadedModel {
    /// The llama.cpp model (Send+Sync, shareable via Arc).
    model: Arc<LlamaModel>,
    /// Backend instance (Send+Sync).
    backend: Arc<LlamaBackend>,
    /// Model file path for identification.
    path: String,
    /// Model size in bytes (tensor memory).
    size_bytes: u64,
    /// Number of parameters.
    n_params: u64,
    /// Training context window size.
    n_ctx_train: u32,
}

impl LoadedModel for LlamaCppLoadedModel {
    fn description(&self) -> String {
        let size_gb = self.size_bytes as f64 / 1024.0 / 1024.0 / 1024.0;
        format!(
            "llama.cpp: {} ({:.2} GB, {} params, ctx={})",
            self.path, size_gb, self.n_params, self.n_ctx_train
        )
    }

    fn memory_bytes(&self) -> Option<u64> {
        Some(self.size_bytes)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Process-wide shared llama.cpp backend instance.
///
/// The `llama-cpp-2` crate enforces a process-wide singleton via an `AtomicBool`
/// flag — only one `LlamaBackend::init()` call can succeed per process. We use
/// a `Mutex<Option<Arc<LlamaBackend>>>` to lazily initialize and share a single
/// instance so that multiple `LlamaCppBackend` instances (and parallel tests) can
/// coexist without hitting `BackendAlreadyInitialized`.
static SHARED_LLAMA_BACKEND: Mutex<Option<Arc<LlamaBackend>>> = Mutex::new(None);

/// llama.cpp inference backend.
pub struct LlamaCppBackend {
    backend: Arc<LlamaBackend>,
}

impl LlamaCppBackend {
    /// Create a new llama.cpp backend.
    ///
    /// The underlying llama.cpp subsystem is initialized once per process
    /// and shared across all instances via `Arc`.
    pub fn new() -> Result<Self> {
        let mut guard = SHARED_LLAMA_BACKEND.lock();
        let backend = match guard.as_ref() {
            Some(existing) => existing.clone(),
            None => {
                let b = Arc::new(LlamaBackend::init().map_err(|e| {
                    NxuskitError::Configuration(format!(
                        "Failed to initialize llama.cpp backend: {e}"
                    ))
                })?);
                *guard = Some(b.clone());
                b
            }
        };
        Ok(Self { backend })
    }
}

impl std::fmt::Debug for LlamaCppBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlamaCppBackend").finish()
    }
}

/// Extract text content from a message.
fn extract_text(content: &crate::types::MessageContent) -> String {
    match content {
        crate::types::MessageContent::Text(t) => t.clone(),
        crate::types::MessageContent::Parts(parts) => parts
            .iter()
            .filter_map(|p| match p {
                crate::types::ContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

/// Build the messages into a single prompt string.
///
/// Uses a simple chat format: system message, then alternating user/assistant.
fn build_prompt(request: &ChatRequest) -> String {
    use crate::types::Role;

    let mut prompt = String::new();
    for msg in &request.messages {
        let text = extract_text(&msg.content);
        match msg.role {
            Role::System => {
                prompt.push_str(&format!("### System:\n{}\n\n", text));
            }
            Role::User => {
                prompt.push_str(&format!("### User:\n{}\n\n", text));
            }
            Role::Assistant => {
                prompt.push_str(&format!("### Assistant:\n{}\n\n", text));
            }
        }
    }
    prompt.push_str("### Assistant:\n");
    prompt
}

/// Build a sampler chain from individual parameters.
///
/// Constructed inside `spawn_blocking` because `LlamaSampler` contains
/// a raw pointer (`*mut llama_sampler`) that is `!Send`.
fn build_sampler_from_params(
    temp: f32,
    top_p: f32,
    presence_penalty: Option<f32>,
    frequency_penalty: Option<f32>,
    seed: Option<u64>,
) -> LlamaSampler {
    let mut samplers: Vec<LlamaSampler> = Vec::new();

    // Top-p filtering
    samplers.push(LlamaSampler::top_p(top_p, 1));

    // Temperature
    samplers.push(LlamaSampler::temp(temp));

    // Repetition penalty
    if let Some(penalty) = presence_penalty {
        samplers.push(LlamaSampler::penalties(64, 1.0, 0.0, penalty));
    } else if let Some(penalty) = frequency_penalty {
        samplers.push(LlamaSampler::penalties(64, 1.0, penalty, 0.0));
    }

    // Selection sampler (use seed if provided for reproducibility)
    if let Some(seed) = seed {
        samplers.push(LlamaSampler::dist(seed as u32));
    } else {
        samplers.push(LlamaSampler::dist(42));
    }

    LlamaSampler::chain_simple(samplers)
}

#[async_trait]
impl InferenceBackend for LlamaCppBackend {
    async fn load_model(
        &self,
        model_path: &str,
        n_gpu_layers: i32,
        _context_size: Option<u32>,
        _batch_size: Option<u32>,
        _threads: Option<u32>,
    ) -> Result<Box<dyn LoadedModel>> {
        let path = model_path.to_string();
        let backend = Arc::clone(&self.backend);
        let gpu_layers = if n_gpu_layers < 0 {
            999
        } else {
            n_gpu_layers as u32
        };

        // Model loading is blocking — run on a dedicated thread
        tokio::task::spawn_blocking(move || {
            // Check file exists before calling load_from_file, which panics
            // (rather than returning an error) when the file is missing.
            if !std::path::Path::new(&path).exists() {
                return Err(NxuskitError::Configuration(format!(
                    "Failed to load model '{}': not found",
                    path
                )));
            }

            let model_params = LlamaModelParams::default().with_n_gpu_layers(gpu_layers);

            let model =
                LlamaModel::load_from_file(&backend, &path, &model_params).map_err(|e| {
                    NxuskitError::Configuration(format!("Failed to load model '{}': {}", path, e))
                })?;

            let size_bytes = model.size();
            let n_params = model.n_params();
            let n_ctx_train = model.n_ctx_train();

            Ok(Box::new(LlamaCppLoadedModel {
                model: Arc::new(model),
                backend,
                path,
                size_bytes,
                n_params,
                n_ctx_train,
            }) as Box<dyn LoadedModel>)
        })
        .await
        .map_err(|e| NxuskitError::InvalidRequest(format!("Model loading task failed: {e}")))?
    }

    async fn generate(
        &self,
        model: &dyn LoadedModel,
        request: &ChatRequest,
    ) -> Result<GenerateResponse> {
        // Downcast to our concrete type
        let llama_model = model
            .as_any()
            .downcast_ref::<LlamaCppLoadedModel>()
            .ok_or_else(|| {
                NxuskitError::InvalidRequest(
                    "Invalid model handle for llama.cpp backend".to_string(),
                )
            })?;

        let model_arc = Arc::clone(&llama_model.model);
        let backend_arc = Arc::clone(&llama_model.backend);
        let max_tokens = request.max_tokens.unwrap_or(512);
        let stop_sequences: Vec<String> = request.stop.clone().unwrap_or_default();
        let prompt = build_prompt(request);
        // Capture sampler parameters to build inside spawn_blocking (LlamaSampler is !Send)
        let temp = request.temperature.unwrap_or(0.8);
        let top_p = request.top_p.unwrap_or(0.95);
        let presence_penalty = request.presence_penalty;
        let frequency_penalty = request.frequency_penalty;
        let seed = request.seed;
        let context_size = llama_model.n_ctx_train;
        let model_memory = llama_model.size_bytes;

        tokio::task::spawn_blocking(move || {
            let start = Instant::now();

            // Tokenize prompt
            let tokens = model_arc
                .str_to_token(&prompt, AddBos::Always)
                .map_err(|e| NxuskitError::InvalidRequest(format!("Tokenization failed: {e}")))?;

            let prompt_token_count = tokens.len() as u32;

            // Create context
            let ctx_size = NonZeroU32::new(context_size.max(prompt_token_count + max_tokens))
                .unwrap_or(NonZeroU32::new(2048).unwrap());
            let ctx_params = LlamaContextParams::default().with_n_ctx(Some(ctx_size));
            let mut ctx = model_arc
                .new_context(&backend_arc, ctx_params)
                .map_err(|e| {
                    NxuskitError::InvalidRequest(format!("Failed to create context: {e}"))
                })?;

            // Batch-process prompt
            let mut batch = LlamaBatch::new(ctx_size.get() as usize, 1);
            let last_idx = tokens.len() as i32 - 1;
            for (i, token) in (0_i32..).zip(tokens) {
                batch
                    .add(token, i, &[0], i == last_idx)
                    .map_err(|e| NxuskitError::InvalidRequest(format!("Batch add failed: {e}")))?;
            }
            ctx.decode(&mut batch).map_err(|e| {
                NxuskitError::InvalidRequest(format!("Prompt decoding failed: {e}"))
            })?;

            let time_after_prompt = start.elapsed();

            // Build sampler inside spawn_blocking (LlamaSampler contains !Send raw ptr)
            let mut sampler =
                build_sampler_from_params(temp, top_p, presence_penalty, frequency_penalty, seed);
            let mut decoder = encoding_rs::UTF_8.new_decoder();
            let mut n_cur = batch.n_tokens();
            let n_max = n_cur + max_tokens as i32;
            let mut generated = String::new();
            let mut completion_tokens: u32 = 0;
            let mut finish_reason = GenerateFinishReason::MaxTokens;
            let mut first_token_time: Option<std::time::Duration> = None;

            while n_cur < n_max {
                let token = sampler.sample(&ctx, batch.n_tokens() - 1);
                sampler.accept(token);

                if first_token_time.is_none() {
                    first_token_time = Some(start.elapsed());
                }

                if model_arc.is_eog_token(token) {
                    finish_reason = GenerateFinishReason::Stop;
                    break;
                }

                let piece = model_arc
                    .token_to_piece(token, &mut decoder, true, None)
                    .map_err(|e| {
                        NxuskitError::InvalidRequest(format!("Token decode failed: {e}"))
                    })?;

                generated.push_str(&piece);
                completion_tokens += 1;

                // Check stop sequences
                if stop_sequences
                    .iter()
                    .any(|s| generated.ends_with(s.as_str()))
                {
                    // Trim the stop sequence from output
                    if let Some(s) = stop_sequences
                        .iter()
                        .find(|s| generated.ends_with(s.as_str()))
                    {
                        generated.truncate(generated.len() - s.len());
                    }
                    finish_reason = GenerateFinishReason::StopSequence;
                    break;
                }

                batch.clear();
                batch
                    .add(token, n_cur, &[0], true)
                    .map_err(|e| NxuskitError::InvalidRequest(format!("Batch add failed: {e}")))?;
                n_cur += 1;
                ctx.decode(&mut batch)
                    .map_err(|e| NxuskitError::InvalidRequest(format!("Decode failed: {e}")))?;
            }

            let total_time = start.elapsed();
            let ttft = first_token_time.unwrap_or(time_after_prompt);
            let tps = if total_time.as_secs_f64() > 0.0 {
                completion_tokens as f64 / total_time.as_secs_f64()
            } else {
                0.0
            };

            Ok(GenerateResponse {
                content: generated,
                prompt_tokens: prompt_token_count,
                completion_tokens,
                time_to_first_token_ms: ttft.as_millis() as u64,
                total_inference_time_ms: total_time.as_millis() as u64,
                tokens_per_second: tps,
                model_memory_bytes: Some(model_memory),
                finish_reason,
            })
        })
        .await
        .map_err(|e| NxuskitError::InvalidRequest(format!("Generation task failed: {e}")))?
    }

    async fn generate_stream(
        &self,
        model: &dyn LoadedModel,
        request: &ChatRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>> {
        let llama_model = model
            .as_any()
            .downcast_ref::<LlamaCppLoadedModel>()
            .ok_or_else(|| {
                NxuskitError::InvalidRequest(
                    "Invalid model handle for llama.cpp backend".to_string(),
                )
            })?;

        let model_arc = Arc::clone(&llama_model.model);
        let backend_arc = Arc::clone(&llama_model.backend);
        let max_tokens = request.max_tokens.unwrap_or(512);
        let stop_sequences: Vec<String> = request.stop.clone().unwrap_or_default();
        let prompt = build_prompt(request);
        // Capture sampler parameters to build inside spawn_blocking (LlamaSampler is !Send)
        let temp = request.temperature.unwrap_or(0.8);
        let top_p = request.top_p.unwrap_or(0.95);
        let presence_penalty = request.presence_penalty;
        let frequency_penalty = request.frequency_penalty;
        let seed = request.seed;
        let context_size = llama_model.n_ctx_train;

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<StreamChunk>>();

        // Spawn blocking generation in a background thread
        tokio::task::spawn_blocking(move || {
            let start = Instant::now();

            // Tokenize
            let tokens = match model_arc.str_to_token(&prompt, AddBos::Always) {
                Ok(t) => t,
                Err(e) => {
                    let _ = tx.send(Err(NxuskitError::InvalidRequest(format!(
                        "Tokenization failed: {e}"
                    ))));
                    return;
                }
            };

            let prompt_token_count = tokens.len() as u32;

            // Create context
            let ctx_size = NonZeroU32::new(context_size.max(prompt_token_count + max_tokens))
                .unwrap_or(NonZeroU32::new(2048).unwrap());
            let ctx_params = LlamaContextParams::default().with_n_ctx(Some(ctx_size));
            let mut ctx = match model_arc.new_context(&backend_arc, ctx_params) {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(Err(NxuskitError::InvalidRequest(format!(
                        "Failed to create context: {e}"
                    ))));
                    return;
                }
            };

            // Batch-process prompt
            let mut batch = LlamaBatch::new(ctx_size.get() as usize, 1);
            let last_idx = tokens.len() as i32 - 1;
            for (i, token) in (0_i32..).zip(tokens) {
                if let Err(e) = batch.add(token, i, &[0], i == last_idx) {
                    let _ = tx.send(Err(NxuskitError::InvalidRequest(format!(
                        "Batch add failed: {e}"
                    ))));
                    return;
                }
            }
            if let Err(e) = ctx.decode(&mut batch) {
                let _ = tx.send(Err(NxuskitError::InvalidRequest(format!(
                    "Prompt decoding failed: {e}"
                ))));
                return;
            }

            // Generation loop — stream tokens through channel
            // Build sampler inside spawn_blocking (LlamaSampler contains !Send raw ptr)
            let mut sampler =
                build_sampler_from_params(temp, top_p, presence_penalty, frequency_penalty, seed);
            let mut decoder = encoding_rs::UTF_8.new_decoder();
            let mut n_cur = batch.n_tokens();
            let n_max = n_cur + max_tokens as i32;
            let mut generated = String::new();
            let mut completion_tokens: u32 = 0;
            let mut finish_reason = crate::types::FinishReason::Length;

            while n_cur < n_max {
                let token = sampler.sample(&ctx, batch.n_tokens() - 1);
                sampler.accept(token);

                if model_arc.is_eog_token(token) {
                    finish_reason = crate::types::FinishReason::Stop;
                    break;
                }

                let piece = match model_arc.token_to_piece(token, &mut decoder, true, None) {
                    Ok(p) => p,
                    Err(e) => {
                        let _ = tx.send(Err(NxuskitError::InvalidRequest(format!(
                            "Token decode failed: {e}"
                        ))));
                        return;
                    }
                };

                generated.push_str(&piece);
                completion_tokens += 1;

                // Check stop sequences
                let should_stop = stop_sequences
                    .iter()
                    .any(|s| generated.ends_with(s.as_str()));

                if should_stop {
                    finish_reason = crate::types::FinishReason::Stop;
                    break;
                }

                // Send the token chunk (non-final)
                let chunk = StreamChunk::new(piece);
                if tx.send(Ok(chunk)).is_err() {
                    // Receiver dropped — client cancelled
                    return;
                }

                batch.clear();
                if let Err(e) = batch.add(token, n_cur, &[0], true) {
                    let _ = tx.send(Err(NxuskitError::InvalidRequest(format!(
                        "Batch add failed: {e}"
                    ))));
                    return;
                }
                n_cur += 1;
                if let Err(e) = ctx.decode(&mut batch) {
                    let _ = tx.send(Err(NxuskitError::InvalidRequest(format!(
                        "Decode failed: {e}"
                    ))));
                    return;
                }
            }

            // Send final chunk with usage and finish reason
            let total_time = start.elapsed();
            let _tps = if total_time.as_secs_f64() > 0.0 {
                completion_tokens as f64 / total_time.as_secs_f64()
            } else {
                0.0
            };

            let usage = TokenUsage::with_actual(
                TokenCount::new(prompt_token_count, completion_tokens),
                TokenCount::new(prompt_token_count, completion_tokens),
            );

            let final_chunk = StreamChunk::final_chunk(finish_reason, Some(usage));
            let _ = tx.send(Ok(final_chunk));
        });

        // Convert mpsc receiver into a Stream using async_stream
        let stream = async_stream::stream! {
            let mut rx = rx;
            while let Some(item) = rx.recv().await {
                yield item;
            }
        };
        Ok(Box::pin(stream))
    }

    async fn list_local_models(&self, search_paths: &[String]) -> Result<Vec<ModelInfo>> {
        let mut models = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for dir in search_paths {
            let path = std::path::Path::new(dir);
            if !path.is_dir() {
                continue;
            }

            if let Ok(entries) = std::fs::read_dir(path) {
                for entry in entries.flatten() {
                    let file_path = entry.path();
                    if file_path.extension().and_then(|e| e.to_str()) == Some("gguf") {
                        let name = file_path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("unknown")
                            .to_string();

                        if seen.insert(name.clone()) {
                            let size = file_path.metadata().ok().map(|m| m.len());
                            let mut metadata = std::collections::HashMap::new();
                            metadata.insert("format".to_string(), "gguf".to_string());
                            metadata.insert("path".to_string(), file_path.display().to_string());
                            metadata.insert("backend".to_string(), "llama-cpp".to_string());

                            models.push(ModelInfo {
                                name,
                                size_bytes: size,
                                description: Some(format!("GGUF model: {}", file_path.display())),
                                context_window: None,
                                metadata,
                            });
                        }
                    }
                }
            }
        }

        models.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(models)
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            supports_json_mode: false, // Grammar support exists but complex to wire
            supports_json_schema: false,
            supports_seed: true,
            supports_stop_sequences: true,
            max_context_size: None, // Model-dependent
            supports_vision: false,
        }
    }

    fn backend_name(&self) -> &str {
        "llama-cpp"
    }
}
