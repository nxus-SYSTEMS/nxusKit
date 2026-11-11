//! Ollama provider implementation

use async_stream::stream;
use async_trait::async_trait;
use futures::Stream;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::{
    ChatRequest, ChatResponse, LLMProvider, Message, ModelInfo, ModelLister, ParameterWarning,
    Role, StreamChunk, ThinkingMode, TokenCount, TokenUsage, WarningSeverity,
    capability::{CapabilityDetector, ModelCapabilities, VisionMode},
    error::{NxuskitError, Result},
    parameter_adapter::ParameterAdapter,
    provider::ProviderContractMapper,
    token_estimator::{StreamingTokenAccumulator, TokenEstimator},
    types::{
        ContentPart, FinishReason, InferenceMetadata, LogprobsData, MessageContent,
        ProviderCapabilities, ResponseFormat, StreamLogprobsDelta,
    },
};

const OLLAMA_API_BASE: &str = "http://localhost:11434";

fn decode_chat_envelope<T: serde::de::DeserializeOwned>(
    value: serde_json::Value,
    unary: bool,
) -> Result<T> {
    if !value.is_object()
        || value.get("error").is_some()
        || (unary && value.get("done") != Some(&serde_json::Value::Bool(true)))
    {
        return Err(NxuskitError::Stream(
            "Invalid or incomplete Ollama response".into(),
        ));
    }
    let mut total = 0_u32;
    for name in ["prompt_eval_count", "eval_count"] {
        if let Some(count) = value.get(name).filter(|count| !count.is_null()) {
            let count = count.as_u64().and_then(|count| u32::try_from(count).ok());
            total = count
                .and_then(|count| total.checked_add(count))
                .ok_or_else(|| NxuskitError::Stream("Malformed Ollama usage counts".into()))?;
        }
    }
    serde_json::from_value(value)
        .map_err(|_| NxuskitError::Stream("Malformed Ollama response".into()))
}

/// Ollama provider for local models
pub struct OllamaProvider {
    client: reqwest::Client,
    base_url: String,
    default_model: String,
    connection_timeout: Duration,
    stream_read_timeout: Duration,
    total_timeout: Duration,
}

impl OllamaProvider {
    /// Static adapter declarations, not model or endpoint-effective support.
    pub fn adapter_capabilities() -> ProviderCapabilities {
        ProviderCapabilities {
            supports_system_messages: true,
            supports_streaming: true,
            supports_vision: true,
            max_stop_sequences: None, // Ollama supports variable stop sequences
            supports_presence_penalty: false,
            supports_frequency_penalty: false,
            supports_seed: true,
            // Current /api/chat accepts logprobs/top_logprobs and returns
            // per-token logprobs on unary and streamed chat responses.
            supports_logprobs: true,
            supports_streaming_logprobs: true,
            supports_json_mode: true,
            // Ollama 0.5.0+ supports JSON schema validation via the format field
            supports_json_schema: true,
            penalty_range: None,
            max_logprobs: Some(20),
        }
    }
}

impl OllamaProvider {
    /// Create a new Ollama provider builder
    pub fn builder() -> OllamaProviderBuilder {
        OllamaProviderBuilder::default()
    }

    /// Detect vision capabilities for a specific model using /api/show endpoint
    ///
    /// This method queries the Ollama API to determine if a model supports vision
    /// by checking its capabilities array. Returns true if the model has "vision"
    /// in its capabilities, false otherwise.
    ///
    /// Note: This makes an API call and may fail if the model doesn't exist or
    /// the Ollama server is unavailable. Failures are logged but don't prevent
    /// model listing - models will default to text-only.
    async fn detect_vision_capability(&self, model_name: &str) -> bool {
        #[derive(Serialize)]
        struct ShowRequest {
            name: String,
        }

        let request = ShowRequest {
            name: model_name.to_string(),
        };

        // Attempt to call /api/show endpoint
        let response = match self
            .client
            .post(format!("{}/api/show", self.base_url))
            .json(&request)
            .timeout(self.connection_timeout)
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(_e) => {
                // Silently fail - just return false (text-only default)
                return false;
            }
        };

        if !response.status().is_success() {
            return false;
        }

        // Parse response
        match response.json::<OllamaShowResponse>().await {
            Ok(show_info) => {
                if let Some(capabilities) = show_info.capabilities {
                    capabilities.iter().any(|c| c == "vision")
                } else {
                    // No capabilities field - likely older Ollama version
                    // Try heuristic: check if model name contains "vision" or "llava"
                    let name_lower = model_name.to_lowercase();
                    name_lower.contains("vision")
                        || name_lower.contains("llava")
                        || name_lower.contains("bakllava")
                }
            }
            Err(_e) => false,
        }
    }

    /// Convert ResponseFormat to Ollama's format field
    ///
    /// Ollama supports two formats:
    /// - `"json"`: Basic JSON mode
    /// - `{ schema object }`: JSON schema validation (Ollama 0.5.0+)
    fn convert_response_format(format: Option<&ResponseFormat>) -> Option<OllamaFormat> {
        match format {
            Some(ResponseFormat::Json) => Some(OllamaFormat::Json("json".to_string())),
            Some(ResponseFormat::JsonSchema { schema }) => {
                Some(OllamaFormat::JsonSchema(schema.clone()))
            }
            Some(ResponseFormat::Text) | None => None,
        }
    }

    /// Convert our Message format to Ollama's format
    fn convert_messages(&self, messages: &[Message]) -> Vec<OllamaMessage> {
        messages
            .iter()
            .map(|msg| {
                let (content, images) = match &msg.content {
                    MessageContent::Text(text) => (text.clone(), None),
                    MessageContent::Parts(parts) => {
                        let mut text_parts = Vec::new();
                        let mut image_data = Vec::new();

                        for part in parts {
                            match part {
                                ContentPart::Text { text } => text_parts.push(text.as_str()),
                                ContentPart::Image { source } => {
                                    // Ollama expects base64-encoded images
                                    if let crate::types::ImageData::Base64 { data, .. } =
                                        &source.data
                                    {
                                        image_data.push(data.clone());
                                    }
                                }
                            }
                        }

                        let content = text_parts.join("\n");
                        let images = if image_data.is_empty() {
                            None
                        } else {
                            Some(image_data)
                        };

                        (content, images)
                    }
                };

                OllamaMessage {
                    role: match msg.role {
                        Role::System => "system".to_string(),
                        Role::User => "user".to_string(),
                        Role::Assistant => "assistant".to_string(),
                    },
                    content,
                    images,
                    thinking: None, // Thinking is only for responses, not outgoing messages
                    tool_calls: None,
                }
            })
            .collect()
    }
}

impl std::fmt::Debug for OllamaProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OllamaProvider")
            .field("base_url", &self.base_url)
            .field("default_model", &self.default_model)
            .field("connection_timeout", &self.connection_timeout)
            .field("stream_read_timeout", &self.stream_read_timeout)
            .field("total_timeout", &self.total_timeout)
            .finish()
    }
}

/// Enrich model info with descriptions for known model families
fn enrich_ollama_model(info: &mut ModelInfo) {
    let model_name = info.name.to_lowercase();

    // Match common model families (Ollama uses local models with varying names)
    let description = if model_name.contains("llama") {
        if model_name.contains("3.2") || model_name.contains("3.3") {
            Some("Llama 3.2/3.3: Latest Meta model for local inference")
        } else if model_name.contains("3.1") {
            Some("Llama 3.1: Meta's advanced model for local deployment")
        } else if model_name.contains("3") {
            Some("Llama 3: Meta's powerful open-source model")
        } else if model_name.contains("2") {
            Some("Llama 2: Meta's open-source foundation model")
        } else if model_name.contains("code") {
            Some("Code Llama: Specialized for code generation and understanding")
        } else {
            Some("Llama: Meta's open-source language model running locally")
        }
    } else if model_name.contains("mistral") {
        Some("Mistral: Efficient open-source model with strong performance")
    } else if model_name.contains("mixtral") {
        Some("Mixtral: Mistral's Mixture of Experts model for local use")
    } else if model_name.contains("qwen") {
        Some("Qwen: Alibaba's multilingual model for local deployment")
    } else if model_name.contains("gemma") {
        Some("Gemma: Google's lightweight open model")
    } else if model_name.contains("phi") {
        Some("Phi: Microsoft's efficient small language model")
    } else if model_name.contains("deepseek") {
        Some("DeepSeek: Advanced reasoning model for local inference")
    } else if model_name.contains("neural-chat") {
        Some("Neural Chat: Intel's conversational AI model")
    } else if model_name.contains("vicuna") {
        Some("Vicuna: Open-source chatbot trained by fine-tuning LLaMA")
    } else if model_name.contains("orca") {
        Some("Orca: Microsoft's model trained with explanation tuning")
    } else if model_name.contains("wizardlm") || model_name.contains("wizard") {
        Some("WizardLM: Evol-Instruct fine-tuned model for complex instructions")
    } else if model_name.contains("falcon") {
        Some("Falcon: TII's open-source large language model")
    } else if model_name.contains("starcoder") || model_name.contains("star") {
        Some("StarCoder: Code generation model trained on The Stack")
    } else {
        None
    };

    if let Some(desc) = description {
        info.description = Some(desc.to_string());
    }
}

#[async_trait]
impl LLMProvider for OllamaProvider {
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse> {
        request.validate_legacy_sampling(self.provider_name())?;
        // Adapt request to provider capabilities
        let capabilities = self.get_capabilities();
        let mut adapted = ParameterAdapter::adapt(request, &capabilities);
        let adapted_request = &adapted.request;

        let messages = self.convert_messages(&adapted_request.messages);

        // Add warning if max_tokens is potentially problematic for thinking models
        if Self::is_max_tokens_potentially_problematic(
            adapted_request.max_tokens,
            &adapted_request.model,
        ) {
            adapted.warnings.push(ParameterWarning {
                parameter: "max_tokens".to_string(),
                message: format!(
                    "max_tokens={} may be too low for thinking model '{}'. \
                     Thinking tokens count toward max_tokens and may exhaust the budget \
                     before content generation. Consider removing max_tokens limit or setting it higher.",
                    adapted_request.max_tokens.unwrap_or(0),
                    adapted_request.model
                ),
                severity: WarningSeverity::Warning,
            });
        }

        let ollama_request = OllamaRequest {
            model: adapted_request.model.clone(),
            messages,
            stream: false,
            options: Some(OllamaOptions {
                temperature: adapted_request.temperature,
                num_predict: adapted_request.max_tokens.map(|t| t as i32),
                top_p: adapted_request.top_p,
                stop: adapted_request.stop.clone(),
                seed: adapted_request.seed,
                top_k: adapted_request.top_k,
                min_p: adapted_request.min_p,
            }),
            think: Self::get_think_param(adapted_request.thinking_mode, &adapted_request.model),
            format: Self::convert_response_format(adapted_request.response_format.as_ref()),
            tools: adapted_request
                .tools
                .clone()
                .filter(|tools| !tools.is_empty()),
            logprobs: adapted_request.logprobs,
            top_logprobs: adapted_request.top_logprobs,
        };

        let response = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .header("content-type", "application/json")
            .json(&ollama_request)
            .timeout(self.total_timeout)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let retry_after = super::parse_retry_after(response.headers());
            let error_text = crate::provider::http::read_response_text_bounded(response)
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(match status.as_u16() {
                429 => NxuskitError::rate_limit(retry_after),
                _ => NxuskitError::provider(status.as_u16(), error_text),
            });
        }

        let ollama_response: OllamaResponse = decode_chat_envelope(
            crate::provider::http::read_response_json_bounded(response).await?,
            true,
        )?;

        let usage = TokenUsage::estimated_only(TokenCount::new(
            ollama_response.prompt_eval_count.unwrap_or(0),
            ollama_response.eval_count.unwrap_or(0),
        ));

        let mut response = ChatResponse::new(
            ollama_response.message.content,
            ollama_response.model,
            usage,
        );
        response.provider = self.provider_name().to_string();
        response.raw_finish_reason = ollama_response.done_reason;
        response.finish_reason = response
            .raw_finish_reason
            .as_deref()
            .and_then(FinishReason::from_str_flexible);
        response.tool_calls = ollama_response
            .message
            .tool_calls
            .filter(|calls| !calls.is_empty());
        response.logprobs = ollama_response
            .logprobs
            .map(|content| LogprobsData { content });

        // Add parameter adaptation warnings
        response.warnings = adapted.warnings;

        // Populate inference metadata
        response.inference_metadata = InferenceMetadata {
            is_complete: true,
            finish_reason: response.finish_reason,
            ..InferenceMetadata::default()
        }
        .with_token_usage(response.usage.clone())
        .with_provider_metadata(serde_json::json!({
            "provider": "ollama"
        }));

        Ok(response)
    }

    async fn chat_stream(
        &self,
        request: &ChatRequest,
    ) -> Result<Box<dyn Stream<Item = Result<StreamChunk>> + Send + Unpin>> {
        request.validate_legacy_sampling(self.provider_name())?;
        let messages = self.convert_messages(&request.messages);

        // Note: For streaming, warnings about max_tokens for thinking models
        // are not easily surfaced. Users should call chat() first to see warnings,
        // or use non-streaming mode for thinking-intensive tasks.

        let ollama_request = OllamaRequest {
            model: request.model.clone(),
            messages,
            stream: true,
            options: Some(OllamaOptions {
                temperature: request.temperature,
                num_predict: request.max_tokens.map(|t| t as i32),
                top_p: request.top_p,
                stop: request.stop.clone(),
                seed: request.seed,
                top_k: request.top_k,
                min_p: request.min_p,
            }),
            think: Self::get_think_param(request.thinking_mode, &request.model),
            format: Self::convert_response_format(request.response_format.as_ref()),
            tools: request.tools.clone().filter(|tools| !tools.is_empty()),
            logprobs: request.logprobs,
            top_logprobs: request.top_logprobs,
        };

        // Note: The client already has timeouts configured via build_http_client():
        // - connect_timeout: applies during TCP handshake
        // - read_timeout: applies per-chunk during streaming (resets after each chunk)
        // - timeout (total): applies to the entire request lifecycle
        //
        // For streaming requests, we use total_timeout as the request-level timeout.
        // The read_timeout on the client handles inter-chunk delays during streaming.
        let response = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .header("content-type", "application/json")
            .json(&ollama_request)
            .timeout(self.total_timeout)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let retry_after = super::parse_retry_after(response.headers());
            let error_text = crate::provider::http::read_response_text_bounded(response)
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(match status.as_u16() {
                429 => NxuskitError::rate_limit(retry_after),
                _ => NxuskitError::provider(status.as_u16(), error_text),
            });
        }

        let stream = response.bytes_stream();

        // Create token estimator for the model (outside stream! macro to avoid lifetime issues)
        let model = request.model.clone();
        let estimator = TokenEstimator::for_model(&model);
        let prompt_tokens = estimator.count_messages(&request.messages);

        let output_stream = stream! {
            // Create accumulator with pre-calculated prompt tokens
            let mut accumulator = StreamingTokenAccumulator::new(estimator, prompt_tokens);

            let mut stream = stream;
            let mut buffer = Vec::new();
            let mut completed = false;
            let mut raw_finish_reason = None;

            'stream_loop: loop {
                let mut transport_bytes = None;
                let eof = match stream.next().await {
                    Some(Ok(bytes)) => {
                        transport_bytes = Some(bytes);
                        false
                    }
                    Some(Err(e)) => {
                        yield Err(NxuskitError::Network(e));
                        return;
                    }
                    None => {
                        // NDJSON permits a final record without a newline.
                        if !buffer.is_empty() {
                            buffer.push(b'\n');
                        }
                        true
                    }
                };
                let mut chunk_cursor = transport_bytes.as_ref().map(|bytes| {
                    crate::provider::http::BoundedStreamChunkCursor::new(bytes.as_ref())
                });

                loop {
                    let has_more = match chunk_cursor.as_mut() {
                        Some(cursor) => match cursor.append_next(&mut buffer) {
                            Ok(has_more) => has_more,
                            Err(error) => {
                                yield Err(NxuskitError::Stream(error.to_string()));
                                return;
                            }
                        },
                        None => false,
                    };


                // Process complete NDJSON lines, including the final EOF record.
                while let Some(frame) = next_ollama_ndjson_frame(&mut buffer) {
                    let parsed = frame
                        .map_err(|_| NxuskitError::Stream("Malformed Ollama stream record".into()))
                        .and_then(|value| decode_chat_envelope::<OllamaStreamChunk>(value, false));
                    match parsed {
                        Ok(chunk_data) => {
                            // Actual usage commonly arrives in a message-free terminal record.
                            if let (Some(prompt), Some(completion)) = (
                                chunk_data.prompt_eval_count,
                                chunk_data.eval_count,
                            ) {
                                accumulator.set_actual(TokenCount::new(prompt, completion));
                            }
                            let logprobs = chunk_data
                                .logprobs
                                .map(|content| StreamLogprobsDelta { content });
                            if let Some(message) = chunk_data.message {
                                let has_content = !message.content.is_empty();
                                let has_thinking = message.thinking.as_ref()
                                    .is_some_and(|t| !t.is_empty());
                                let has_tools = message.tool_calls.as_ref()
                                    .is_some_and(|calls| !calls.is_empty());

                                // Preserve every public stream payload class.
                                if has_content || has_thinking || has_tools || logprobs.is_some() {
                                    if has_content {
                                        accumulator.add_chunk(&message.content);
                                    }
                                    if has_thinking
                                        && let Some(ref thinking) = message.thinking
                                    {
                                        accumulator.add_thinking_chunk(thinking);
                                    }

                                    let mut stream_chunk = StreamChunk::new(message.content);
                                    if has_thinking {
                                        stream_chunk.thinking = message.thinking;
                                    }
                                    stream_chunk.tool_calls = message
                                        .tool_calls
                                        .filter(|calls| !calls.is_empty());
                                    stream_chunk.logprobs = logprobs;
                                    stream_chunk.usage = Some(accumulator.running_total());
                                    yield Ok(stream_chunk);
                                }
                            } else if let Some(logprobs) = logprobs {
                                let mut stream_chunk = StreamChunk::new(String::new());
                                stream_chunk.logprobs = Some(logprobs);
                                stream_chunk.usage = Some(accumulator.running_total());
                                yield Ok(stream_chunk);
                            }
                            if chunk_data.done {
                                completed = true;
                                raw_finish_reason = chunk_data.done_reason;
                                break 'stream_loop;
                            }
                        }
                        Err(e) => {
                            yield Err(e);
                            return;
                        }
                    }
                }
                    if !has_more {
                        break;
                    }
                }
                if eof {
                    break;
                }
            }

            if !completed {
                yield Err(NxuskitError::Stream("Ollama stream ended before done".to_string()));
                return;
            }

            let final_usage = accumulator.finalize();
            let mut terminal = StreamChunk::new(String::new());
            terminal.terminal = true;
            terminal.finish_reason = raw_finish_reason.as_deref().and_then(FinishReason::from_str_flexible);
            terminal.raw_finish_reason = raw_finish_reason;
            terminal.usage = Some(final_usage);
            yield Ok(terminal);
        };

        Ok(Box::new(Box::pin(output_stream)))
    }

    fn provider_name(&self) -> &str {
        "ollama"
    }

    /// List all available models from the local Ollama instance
    ///
    /// This method queries the Ollama API's `/api/tags` endpoint to retrieve
    /// a list of all locally available models with their metadata.
    ///
    /// # Returns
    ///
    /// Returns a vector of `ModelInfo` containing model details including:
    /// - name: Model identifier (e.g., "llama2:latest", "mistral:7b")
    /// - size_bytes: Model size in bytes (from Ollama API)
    /// - metadata: Provider-specific fields (digest, modified_at)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Ollama is not running or unreachable
    /// - The API returns a non-success status code
    /// - Response deserialization fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// use nxuskit_engine::prelude::*;
    ///
    /// # async fn example() -> std::result::Result<(), Box<dyn std::error::Error>> {
    /// let provider = OllamaProvider::builder().build()
    ///     .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
    /// let models = provider.list_models().await
    ///     .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
    /// for model in models {
    ///     println!("{} - {}",
    ///         model.name,
    ///         model.formatted_size().unwrap_or_default()
    ///     );
    /// }
    /// # Ok(())
    /// # }
    /// ```
    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let response = self
            .client
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(NxuskitError::provider(
                response.status().as_u16(),
                "Failed to list models".to_string(),
            ));
        }

        let tags_response: OllamaTagsResponse =
            crate::provider::http::read_response_json_bounded(response).await?;

        // Check if vision detection is enabled via environment variable
        let enable_vision_detection = std::env::var("OLLAMA_DETECT_VISION")
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false);

        // Convert OllamaModel to ModelInfo
        let mut models = Vec::new();
        for m in tags_response.models {
            let mut info = if let Some(size) = m.size {
                ModelInfo::with_size(&m.name, size as u64)
            } else {
                ModelInfo::new(&m.name)
            };

            // Add provider-specific metadata
            if let Some(digest) = m.digest {
                info.metadata.insert("digest".to_string(), digest);
            }
            if let Some(modified_at) = m.modified_at {
                info.metadata.insert("modified_at".to_string(), modified_at);
            }

            // Detect vision capabilities if enabled
            let has_vision = if enable_vision_detection {
                self.detect_vision_capability(&m.name).await
            } else {
                false
            };

            if has_vision {
                info.metadata
                    .insert("modalities".to_string(), "text,vision".to_string());
                // Ollama doesn't specify max_images limit - leave as None (unlimited)
            } else {
                // Default to text-only modality
                info.metadata
                    .insert("modalities".to_string(), "text".to_string());
            }

            // Enrich with descriptions for known model families
            enrich_ollama_model(&mut info);

            models.push(info);
        }

        Ok(models)
    }

    fn get_capabilities(&self) -> ProviderCapabilities {
        Self::adapter_capabilities()
    }

    fn as_capability_detector(&self) -> Option<&dyn CapabilityDetector> {
        Some(self)
    }
}

#[async_trait]
impl ModelLister for OllamaProvider {
    async fn list_available_models(&self) -> Result<Vec<ModelInfo>> {
        // Delegate to list_models() which already has the correct implementation
        self.list_models().await
    }
}

/// Capability detection implementation for Ollama models
#[async_trait]
impl CapabilityDetector for OllamaProvider {
    async fn get_model_capabilities(&self, model_name: &str) -> Result<ModelCapabilities> {
        let has_vision = self.detect_vision_capability(model_name).await;

        let vision_mode = if has_vision {
            // Detect if single or multi-image
            Self::detect_vision_mode(model_name)
        } else {
            VisionMode::None
        };

        Ok(ModelCapabilities {
            vision_mode,
            supports_streaming: true, // Ollama always supports streaming
            supports_function_calling: false, // Ollama does not expose function calling metadata
        })
    }
}

impl OllamaProvider {
    /// Detect whether a vision model supports single or multiple images
    ///
    /// Uses model family heuristics to determine multi-image capability.
    /// For unknown models, conservatively defaults to single-image.
    ///
    /// Note: Multi-image support in Ollama is model-dependent and may have bugs.
    /// As of Ollama 0.13.5:
    /// - minicpm-v: Confirmed working with multiple images
    /// - qwen3-vl: Confirmed working with multiple images (fixes qwen2.5vl issues)
    /// - qwen2.5vl: BROKEN - GGML assertion failure with multi-image
    /// - llava: May accept multiple but doesn't truly process them well
    /// - llama3.2-vision: Architecturally single-image only
    /// - granite-vision: Crashes with multiple images
    fn detect_vision_mode(model_name: &str) -> VisionMode {
        let name_lower = model_name.to_lowercase();

        // Models confirmed to work with multiple images in Ollama
        if name_lower.contains("minicpm") && name_lower.contains("v") {
            // MiniCPM-V confirmed working with multi-image in Ollama 0.13.5
            return VisionMode::MultiImage;
        }

        // qwen3-vl: Confirmed working with multi-image (tested 2026-01-07)
        if name_lower.contains("qwen3") && name_lower.contains("vl") {
            return VisionMode::MultiImage;
        }

        // Models known to be broken or single-image only
        // qwen2.5vl: Has GGML_ASSERT(a->ne[2] * 4 == b->ne[0]) failure with multi-image
        if name_lower.contains("qwen") && name_lower.contains("vl") {
            return VisionMode::SingleImage;
        }

        // llama3.2-vision is architecturally single-image
        if name_lower.contains("llama3.2") && name_lower.contains("vision") {
            return VisionMode::SingleImage;
        }

        // Granite vision models crash with multiple images
        if name_lower.contains("granite") && name_lower.contains("vision") {
            return VisionMode::SingleImage;
        }

        // LLaVA models: technically accept multiple but may not process well
        // Being conservative here based on user reports
        if name_lower.contains("llava") || name_lower.contains("bakllava") {
            return VisionMode::SingleImage;
        }

        // For other vision models, default to single-image (conservative)
        VisionMode::SingleImage
    }

    /// Detect whether a model is a thinking-capable model
    ///
    /// Thinking models (e.g., Qwen3, DeepSeek-R1) produce chain-of-thought
    /// reasoning before their final response. This function uses model name
    /// heuristics to identify such models.
    ///
    /// # Arguments
    /// * `model_name` - The model name/tag to check
    ///
    /// # Returns
    /// `true` if the model is likely a thinking model, `false` otherwise
    ///
    /// # Known Thinking Models
    /// - `qwen3*` (all Qwen3 variants including qwen3-vl, qwen3-coder)
    /// - `deepseek-r1*` (DeepSeek reasoning models)
    /// - `deepseek-v3*` (DeepSeek v3 with reasoning)
    /// - Models with `:thinking` tag variant
    pub fn is_thinking_model(model_name: &str) -> bool {
        let name_lower = model_name.to_lowercase();

        // Qwen3 models all support thinking
        if name_lower.contains("qwen3") {
            return true;
        }

        // DeepSeek reasoning models
        if name_lower.contains("deepseek-r1") || name_lower.contains("deepseek-v3") {
            return true;
        }

        // Explicit thinking tag
        if name_lower.contains(":thinking") {
            return true;
        }

        false
    }

    /// Check if max_tokens is set to a potentially problematic low value for thinking models
    ///
    /// Returns `true` if max_tokens is set and likely too low for thinking models,
    /// which could result in empty responses as thinking exhausts the token budget.
    fn is_max_tokens_potentially_problematic(max_tokens: Option<u32>, model_name: &str) -> bool {
        match max_tokens {
            Some(tokens) if Self::is_thinking_model(model_name) => {
                // Thinking typically uses 50-500+ tokens before content
                // Values below ~200 are likely to cause issues
                tokens < 200
            }
            _ => false,
        }
    }

    /// Convert ThinkingMode to Ollama's `think` parameter with smart Auto behavior
    ///
    /// This method implements intelligent defaults for the `Auto` mode:
    /// - For thinking-capable models (qwen3, deepseek-r1, etc.): enables thinking
    /// - For other models: omits the parameter
    ///
    /// # Arguments
    /// * `mode` - The thinking mode requested by the user
    /// * `model_name` - The model name to check for thinking capability
    ///
    /// # Returns
    /// * `Some(true)` - Send `think: true` to Ollama
    /// * `Some(false)` - Send `think: false` to Ollama
    /// * `None` - Don't send `think` parameter at all
    fn get_think_param(mode: ThinkingMode, model_name: &str) -> Option<bool> {
        match mode {
            ThinkingMode::Auto => {
                // Smart default: enable thinking for thinking-capable models
                if Self::is_thinking_model(model_name) {
                    Some(true)
                } else {
                    None // Non-thinking models: omit parameter
                }
            }
            ThinkingMode::Enabled => Some(true),
            ThinkingMode::Disabled => Some(false),
            ThinkingMode::Omit => None, // Explicitly omit parameter
        }
    }
}

impl OllamaProvider {
    /// Create a fresh session with no accumulated state.
    ///
    /// For OllamaProvider, this returns a new instance with the same configuration
    /// since it's already stateless (each request is independent).
    ///
    /// # Returns
    ///
    /// A new OllamaProvider instance with the same configuration.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use nxuskit_engine::providers::OllamaProvider;
    ///
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let provider = OllamaProvider::builder()
    ///     .model("llama2")
    ///     .build()?;
    ///
    /// // Use provider for inference...
    ///
    /// // Get fresh session for reproducible testing
    /// let fresh = provider.fresh_session()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn fresh_session(&self) -> Result<Self> {
        OllamaProvider::builder()
            .base_url(self.base_url.clone())
            .model(self.default_model.clone())
            .connection_timeout(self.connection_timeout)
            .stream_read_timeout(self.stream_read_timeout)
            .total_timeout(self.total_timeout)
            .build()
    }
}

/// Builder for OllamaProvider
#[derive(Debug, Default)]
pub struct OllamaProviderBuilder {
    base_url: Option<String>,
    model: Option<String>,
    timeout: Option<Duration>,
    connection_timeout: Option<Duration>,
    stream_read_timeout: Option<Duration>,
    total_timeout: Option<Duration>,
}

impl OllamaProviderBuilder {
    /// Set the base URL (default: <http://localhost:11434>)
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    /// Set the default model
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Set the request timeout (applies to both connection and total request)
    ///
    /// This is a convenience method that sets a general timeout.
    /// For more granular control, use `connection_timeout()`, `stream_read_timeout()`,
    /// or `total_timeout()` instead.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Set the connection establishment timeout
    ///
    /// This timeout applies to the initial connection to the API.
    /// If not set, falls back to the general `timeout` or default (120s).
    pub fn connection_timeout(mut self, timeout: Duration) -> Self {
        self.connection_timeout = Some(timeout);
        self
    }

    /// Set the timeout for reading each chunk in streaming responses
    ///
    /// This timeout applies to reading each individual chunk from a stream.
    /// If not set, falls back to the general `timeout` or default (180s for streams).
    pub fn stream_read_timeout(mut self, timeout: Duration) -> Self {
        self.stream_read_timeout = Some(timeout);
        self
    }

    /// Set the total timeout for the entire request
    ///
    /// This timeout applies to the entire request duration (connection + body).
    /// If not set, falls back to the general `timeout` or default (120s).
    pub fn total_timeout(mut self, timeout: Duration) -> Self {
        self.total_timeout = Some(timeout);
        self
    }

    /// Build the OllamaProvider
    pub fn build(self) -> Result<OllamaProvider> {
        // Default timeout values (Ollama is often used locally, so longer timeouts)
        let default_timeout = Duration::from_secs(120);
        let default_stream_timeout = Duration::from_secs(180);

        // Fallback chain: specific timeout -> general timeout -> default
        let connection_timeout = self
            .connection_timeout
            .or(self.timeout)
            .unwrap_or(default_timeout);

        let stream_read_timeout = self
            .stream_read_timeout
            .or(self.timeout)
            .unwrap_or(default_stream_timeout);

        let total_timeout = self
            .total_timeout
            .or(self.timeout)
            .unwrap_or(default_timeout);

        // Build HTTP client with properly applied timeouts
        // This fixes the bug where timeouts were stored but not applied to the client
        let client =
            super::build_http_client(connection_timeout, stream_read_timeout, total_timeout)?;

        Ok(OllamaProvider {
            client,
            base_url: self.base_url.unwrap_or_else(|| OLLAMA_API_BASE.to_string()),
            default_model: self.model.unwrap_or_else(|| "llama2".to_string()),
            connection_timeout,
            stream_read_timeout,
            total_timeout,
        })
    }
}

// Internal Ollama API types

#[derive(Debug, Serialize)]
struct OllamaRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
    /// Control thinking mode for thinking-capable models (qwen3, deepseek-r1, etc.)
    /// - `Some(true)`: Enable thinking (default for thinking models)
    /// - `Some(false)`: Disable thinking for faster responses
    /// - `None`: Use model's default behavior
    #[serde(skip_serializing_if = "Option::is_none")]
    think: Option<bool>,
    /// Response format for structured output
    /// - `Some(OllamaFormat::Json)`: Request JSON output
    /// - `Some(OllamaFormat::JsonSchema { schema })`: Request JSON output with schema validation
    /// - `None`: Use default text output
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<OllamaFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    logprobs: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_logprobs: Option<u8>,
}

/// Ollama format specification for structured output
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
enum OllamaFormat {
    /// Simple JSON mode - just the string "json"
    Json(String),
    /// JSON schema mode - the full schema object
    JsonSchema(serde_json::Value),
}

#[derive(Debug, Serialize)]
struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_k: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    min_p: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    images: Option<Vec<String>>,
    /// Chain-of-thought reasoning from thinking-enabled models (e.g., Qwen3)
    #[serde(default)]
    thinking: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
struct OllamaResponse {
    model: String,
    message: OllamaMessage,
    #[serde(default)]
    done_reason: Option<String>,
    #[serde(default)]
    prompt_eval_count: Option<u32>,
    #[serde(default)]
    eval_count: Option<u32>,
    #[serde(default)]
    logprobs: Option<Vec<crate::types::TokenLogprob>>,
}

#[derive(Debug, Deserialize)]
struct OllamaStreamChunk {
    #[serde(default)]
    message: Option<OllamaMessage>,
    done: bool,
    #[serde(default)]
    done_reason: Option<String>,
    #[serde(default)]
    prompt_eval_count: Option<u32>,
    #[serde(default)]
    eval_count: Option<u32>,
    #[serde(default)]
    logprobs: Option<Vec<crate::types::TokenLogprob>>,
}

#[derive(Debug, Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModel>,
}

#[derive(Debug, Deserialize)]
struct OllamaModel {
    name: String,
    #[serde(default)]
    #[allow(dead_code)]
    size: Option<i64>,
    #[serde(default)]
    #[allow(dead_code)]
    digest: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    modified_at: Option<String>,
}

/// Response from /api/show endpoint for detailed model information
#[derive(Debug, Deserialize)]
struct OllamaShowResponse {
    #[serde(default)]
    capabilities: Option<Vec<String>>,
    #[serde(default)]
    #[allow(dead_code)]
    modelfile: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    template: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    details: Option<serde_json::Value>,
}

// ============================================================================
#[path = "ollama_contract.rs"]
mod contract;

pub use contract::OllamaContractMapper;

use contract::next_ollama_ndjson_frame;
#[cfg(test)]
#[allow(clippy::print_stdout, clippy::print_stderr)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn legacy_stop_seed_sampling_reach_ollama_wire() {
        use std::io::{Read, Write};
        for streaming in [false, true] {
            for supplied in [false, true] {
                let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
                let endpoint = format!("http://{}", listener.local_addr().unwrap());
                let server = std::thread::spawn(move || {
                    let (mut socket, _) = listener.accept().unwrap();
                    socket
                        .set_read_timeout(Some(Duration::from_secs(5)))
                        .unwrap();
                    let mut headers = Vec::new();
                    let mut byte = [0];
                    while !headers.ends_with(b"\r\n\r\n") {
                        socket.read_exact(&mut byte).unwrap();
                        headers.push(byte[0]);
                    }
                    let headers = String::from_utf8(headers).unwrap();
                    assert!(headers.starts_with("POST /api/chat HTTP/1.1"));
                    let length: usize = headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse().unwrap())
                        })
                        .unwrap();
                    let mut bytes = vec![0; length];
                    socket.read_exact(&mut bytes).unwrap();
                    let request: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                    let body = "{\"model\":\"local\",\"message\":{\"role\":\"assistant\",\"content\":\"ok\"},\"done\":true,\"done_reason\":\"stop\",\"prompt_eval_count\":1,\"eval_count\":1}\n";
                    write!(
                        socket,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                    .unwrap();
                    request
                });
                let provider = OllamaProvider::builder()
                    .base_url(endpoint)
                    .build()
                    .unwrap();
                let mut request = ChatRequest::new("local").with_message(Message::user("hi"));
                if supplied {
                    request.stop = Some(vec!["END".into(), "HALT".into()]);
                    request.seed = Some(7);
                    let mut encoded = serde_json::to_value(&request).unwrap();
                    encoded["top_k"] = serde_json::json!(40);
                    encoded["min_p"] = serde_json::json!(0.1);
                    request = serde_json::from_value(encoded).unwrap();
                }
                if streaming {
                    let chunks: Vec<_> = provider
                        .chat_stream(&request)
                        .await
                        .unwrap()
                        .collect()
                        .await;
                    assert!(chunks.iter().all(Result::is_ok));
                    assert_eq!(
                        chunks
                            .iter()
                            .filter(|c| c.as_ref().is_ok_and(StreamChunk::is_final))
                            .count(),
                        1
                    );
                } else {
                    assert_eq!(provider.chat(&request).await.unwrap().content, "ok");
                }
                let wire = server.join().unwrap();
                assert_eq!(wire["stream"], streaming);
                if supplied {
                    assert_eq!(wire["options"]["stop"], serde_json::json!(["END", "HALT"]));
                    assert_eq!(wire["options"]["seed"], 7);
                    assert_eq!(wire["options"]["top_k"], 40);
                    assert_eq!(wire["options"]["min_p"], 0.1);
                } else {
                    assert!(wire["options"].get("stop").is_none());
                    assert!(wire["options"].get("seed").is_none());
                    assert!(wire["options"].get("top_k").is_none());
                    assert!(wire["options"].get("min_p").is_none());
                }
            }
        }
    }

    #[tokio::test]
    async fn legacy_logprobs_and_tools_reach_ollama_wire() {
        use std::io::{Read, Write};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut headers = Vec::new();
            let mut byte = [0];
            while !headers.ends_with(b"\r\n\r\n") {
                socket.read_exact(&mut byte).unwrap();
                headers.push(byte[0]);
            }
            let headers = String::from_utf8(headers).unwrap();
            assert!(headers.starts_with("POST /api/chat HTTP/1.1"));
            let length: usize = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse().unwrap())
                })
                .unwrap();
            let mut bytes = vec![0; length];
            socket.read_exact(&mut bytes).unwrap();
            let request: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            let body = serde_json::json!({
                "model": "local",
                "message": {
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{
                        "function": {
                            "name": "emit_x",
                            "arguments": {"x": 1}
                        }
                    }]
                },
                "done": true,
                "done_reason": "tool_calls",
                "prompt_eval_count": 1,
                "eval_count": 1,
                "logprobs": [{
                    "token": "1",
                    "logprob": -0.1,
                    "bytes": [49],
                    "top_logprobs": [{
                        "token": "2",
                        "logprob": -1.2,
                        "bytes": [50]
                    }]
                }]
            })
            .to_string();
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
            request
        });

        let provider = OllamaProvider::builder()
            .base_url(endpoint)
            .build()
            .unwrap();
        let mut request = ChatRequest::new("local").with_message(Message::user("hi"));
        request.logprobs = Some(true);
        request.top_logprobs = Some(5);
        request.tools = Some(vec![serde_json::json!({
            "type": "function",
            "function": {
                "name": "emit_x",
                "description": "Return one integer",
                "parameters": {
                    "type": "object",
                    "properties": {"x": {"type": "integer"}},
                    "required": ["x"]
                }
            }
        })]);

        let response = provider.chat(&request).await.unwrap();
        let wire = server.join().unwrap();
        assert_eq!(wire["logprobs"], true);
        assert_eq!(wire["top_logprobs"], 5);
        assert_eq!(wire["tools"][0]["function"]["name"], "emit_x");
        assert_eq!(
            response
                .logprobs
                .as_ref()
                .and_then(|data| data.content.first())
                .map(|entry| entry.token.as_str()),
            Some("1")
        );
        assert_eq!(
            response
                .tool_calls
                .as_ref()
                .and_then(|calls| calls.first())
                .and_then(|call| call["function"]["name"].as_str()),
            Some("emit_x")
        );
        assert_eq!(response.finish_reason, Some(FinishReason::ToolCalls));
    }

    async fn legacy_stream_events(
        body: &'static str,
        broken_transport: bool,
    ) -> Vec<Result<StreamChunk>> {
        legacy_stream_bytes(body.as_bytes(), broken_transport).await
    }

    async fn legacy_stream_bytes(
        body: &'static [u8],
        broken_transport: bool,
    ) -> Vec<Result<StreamChunk>> {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") {
                socket.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            let headers = String::from_utf8(request).unwrap();
            assert!(headers.starts_with("POST /api/chat HTTP/1.1\r\n"));
            let length: usize = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse().unwrap())
                })
                .unwrap();
            socket.read_exact(&mut vec![0; length]).unwrap();
            write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len() + usize::from(broken_transport)).unwrap();
            socket.write_all(body).unwrap();
        });
        let provider = OllamaProvider::builder()
            .base_url(format!("http://{address}"))
            .build()
            .unwrap();
        let request = ChatRequest::new("local");
        let events = tokio::time::timeout(Duration::from_secs(5), async {
            provider
                .chat_stream(&request)
                .await
                .unwrap()
                .collect::<Vec<_>>()
                .await
        })
        .await
        .unwrap();
        server.join().unwrap();
        events
    }

    fn assert_stream_failed_without_success(events: &[Result<StreamChunk>]) {
        assert_eq!(
            events.iter().filter(|e| e.is_err()).count(),
            1,
            "{events:?}"
        );
        assert!(
            events.last().unwrap().is_err(),
            "error must be terminal: {events:?}"
        );
        assert!(
            !events
                .iter()
                .any(|e| e.as_ref().is_ok_and(StreamChunk::is_final)),
            "no successful final after failure: {events:?}"
        );
    }

    #[tokio::test]
    async fn legacy_stream_invalid_utf8_is_not_replaced_with_success() {
        assert_stream_failed_without_success(
            &legacy_stream_bytes(
                b"{\"message\":{\"role\":\"assistant\",\"content\":\"\xff\"},\"done\":true}\n",
                false,
            )
            .await,
        );
    }

    #[tokio::test]
    async fn legacy_terminal_malformed_is_not_success() {
        assert_stream_failed_without_success(&legacy_stream_events("{broken}\n", false).await);
    }

    #[tokio::test]
    async fn legacy_terminal_network_error_is_not_success() {
        assert_stream_failed_without_success(
            &legacy_stream_events("{\"done\":false}\n", true).await,
        );
    }

    #[tokio::test]
    async fn legacy_terminal_premature_eof_is_not_success() {
        assert_stream_failed_without_success(
            &legacy_stream_events("{\"done\":false}\n", false).await,
        );
    }

    #[tokio::test]
    async fn legacy_terminal_truncated_record_is_not_success() {
        assert_stream_failed_without_success(&legacy_stream_events("{\"done\":", false).await);
    }

    #[tokio::test]
    async fn legacy_terminal_nonnewline_content_is_retained() {
        let events = legacy_stream_events(
            "{\"message\":{\"role\":\"assistant\",\"content\":\"tail\"},\"done\":true}",
            false,
        )
        .await;
        assert_eq!(events.len(), 2, "{events:?}");
        assert_eq!(events[0].as_ref().unwrap().delta, "tail");
        assert!(events[1].as_ref().unwrap().is_final());
    }

    #[tokio::test]
    async fn legacy_stream_usage_cannot_wrap_or_overflow() {
        for record in [
            "{\"done\":true,\"prompt_eval_count\":-1,\"eval_count\":1}\n",
            "{\"done\":true,\"prompt_eval_count\":1,\"eval_count\":-1}\n",
            "{\"done\":true,\"prompt_eval_count\":4294967296,\"eval_count\":0}\n",
            "{\"done\":true,\"prompt_eval_count\":4294967295,\"eval_count\":1}\n",
        ] {
            assert_stream_failed_without_success(&legacy_stream_events(record, false).await);
        }
    }

    #[test]
    fn usage_envelope_bounds_apply_to_unary_and_stream() {
        for unary in [false, true] {
            for counts in [
                serde_json::json!({"prompt_eval_count": 0, "eval_count": 0}),
                serde_json::json!({"prompt_eval_count": 4294967295_u64, "eval_count": 0}),
                serde_json::json!({"prompt_eval_count": null}),
                serde_json::json!({}),
            ] {
                let mut value = counts;
                value["done"] = serde_json::json!(true);
                assert!(decode_chat_envelope::<serde_json::Value>(value, unary).is_ok());
            }
            for count in [
                serde_json::json!(-1),
                serde_json::json!(1.5),
                serde_json::json!(4294967296_u64),
            ] {
                for name in ["prompt_eval_count", "eval_count"] {
                    let mut value = serde_json::json!({"done": true});
                    value[name] = count.clone();
                    assert!(decode_chat_envelope::<serde_json::Value>(value, unary).is_err());
                }
            }
        }
    }

    #[tokio::test]
    async fn legacy_terminal_usage_only_is_retained() {
        let events = legacy_stream_events(
            "{\"done\":true,\"prompt_eval_count\":7,\"eval_count\":3}\n",
            false,
        )
        .await;
        assert_eq!(events.len(), 1, "{events:?}");
        let chunk = events[0].as_ref().unwrap();
        assert!(chunk.is_final());
        let actual = chunk
            .usage
            .as_ref()
            .unwrap()
            .actual
            .as_ref()
            .expect("terminal actual usage");
        assert_eq!((actual.prompt_tokens, actual.completion_tokens), (7, 3));
    }

    #[tokio::test]
    async fn legacy_terminal_done_stops_before_trailing_data() {
        let events = legacy_stream_events("{\"done\":true}\n{broken}\n", false).await;
        assert_eq!(events.len(), 1, "{events:?}");
        assert!(events[0].as_ref().unwrap().is_final());
    }

    #[tokio::test]
    async fn legacy_terminal_raw_reason_is_not_fabricated() {
        for (body, reason, raw) in [
            (
                "{\"done\":true,\"done_reason\":\"length\"}\n",
                Some(FinishReason::Length),
                Some("length"),
            ),
            (
                "{\"done\":true,\"done_reason\":\"vendor-new\"}\n",
                None,
                Some("vendor-new"),
            ),
            ("{\"done\":true,\"done_reason\":null}\n", None, None),
            ("{\"done\":true}\n", None, None),
        ] {
            let events = legacy_stream_events(body, false).await;
            assert_eq!(events.len(), 1);
            let chunk = events[0].as_ref().unwrap();
            assert!(chunk.is_final());
            assert_eq!(chunk.finish_reason, reason);
            assert_eq!(chunk.raw_finish_reason.as_deref(), raw);
        }
    }

    #[test]
    fn ndjson_framer_preserves_split_utf8_and_reports_invalid_frames() {
        let mut buffer = br#"{"message":{"content":""#.to_vec();
        buffer.push(0xe2);
        assert!(next_ollama_ndjson_frame(&mut buffer).is_none());
        buffer.extend_from_slice(&[0x82, 0xac]);
        buffer.extend_from_slice(b"\"},\"done\":false}\n");
        let value = next_ollama_ndjson_frame(&mut buffer).unwrap().unwrap();
        assert_eq!(value["message"]["content"], "€");

        let mut invalid = b"{\xff}\n".to_vec();
        let error = next_ollama_ndjson_frame(&mut invalid).unwrap().unwrap_err();
        assert_eq!(
            error.code,
            crate::error::ProviderErrorCode::MalformedResponse
        );
    }

    #[test]
    fn test_builder() {
        let provider = OllamaProvider::builder().build().unwrap();
        assert_eq!(provider.provider_name(), "ollama");
        assert_eq!(provider.base_url, OLLAMA_API_BASE);
    }

    #[test]
    fn test_builder_with_custom_url() {
        let provider = OllamaProvider::builder()
            .base_url("http://custom:11434")
            .build()
            .unwrap();
        assert_eq!(provider.base_url, "http://custom:11434");
    }

    #[test]
    fn test_message_conversion() {
        let provider = OllamaProvider::builder().build().unwrap();

        let messages = vec![
            Message::system("You are helpful"),
            Message::user("Hello"),
            Message::assistant("Hi there"),
        ];

        let ollama_msgs = provider.convert_messages(&messages);
        assert_eq!(ollama_msgs.len(), 3);
        assert_eq!(ollama_msgs[0].role, "system");
        assert_eq!(ollama_msgs[1].role, "user");
        assert_eq!(ollama_msgs[2].role, "assistant");
    }

    #[tokio::test]
    #[ignore] // Requires Ollama to be running locally
    async fn test_list_models_integration() {
        let provider = OllamaProvider::builder().build().unwrap();

        // This test requires Ollama to be running
        match provider.list_models().await {
            Ok(models) => {
                println!("Found {} models", models.len());
                for model in models {
                    println!(
                        "  - {} {}",
                        model.name,
                        model
                            .formatted_size()
                            .map(|s| format!("({})", s))
                            .unwrap_or_default()
                    );
                }
            }
            Err(e) => {
                // If Ollama is not running, we expect a network error
                println!("Expected error (Ollama not running): {}", e);
            }
        }
    }

    #[test]
    fn test_ollama_model_deserialization() {
        // Test that we can deserialize model responses with various fields
        let json = r#"{
            "name": "llama2:latest",
            "size": 3826793677,
            "digest": "sha256:1234567890abcdef",
            "modified_at": "2024-01-15T10:30:00Z"
        }"#;

        let model: OllamaModel = serde_json::from_str(json).unwrap();
        assert_eq!(model.name, "llama2:latest");
        assert_eq!(model.size, Some(3826793677));
        assert!(model.digest.is_some());
        assert!(model.modified_at.is_some());
    }

    #[test]
    fn test_ollama_model_minimal_deserialization() {
        // Test that we can deserialize with just the name field
        let json = r#"{"name": "llama2"}"#;

        let model: OllamaModel = serde_json::from_str(json).unwrap();
        assert_eq!(model.name, "llama2");
        assert_eq!(model.size, None);
        assert_eq!(model.digest, None);
        assert_eq!(model.modified_at, None);
    }

    // Tests for thinking field support (T019-T022)

    #[test]
    fn test_ollama_message_with_thinking_deserialization() {
        // Test that OllamaMessage correctly deserializes the thinking field
        let json = r#"{"role":"assistant","content":"","thinking":"Okay, analyzing..."}"#;
        let message: OllamaMessage = serde_json::from_str(json).unwrap();
        assert_eq!(message.role, "assistant");
        assert_eq!(message.content, "");
        assert_eq!(message.thinking, Some("Okay, analyzing...".to_string()));
    }

    #[test]
    fn test_ollama_message_without_thinking_deserialization() {
        // Test backward compatibility - no thinking field
        let json = r#"{"role":"assistant","content":"Hello!"}"#;
        let message: OllamaMessage = serde_json::from_str(json).unwrap();
        assert_eq!(message.role, "assistant");
        assert_eq!(message.content, "Hello!");
        assert!(message.thinking.is_none());
    }

    #[test]
    fn test_ollama_stream_chunk_with_thinking() {
        // Test deserializing stream chunk with thinking
        let json = r#"{"model":"qwen3-vl:2b","message":{"role":"assistant","content":"","thinking":"Okay"},"done":false}"#;
        let chunk: OllamaStreamChunk = serde_json::from_str(json).unwrap();
        assert!(!chunk.done);
        let message = chunk.message.unwrap();
        assert_eq!(message.content, "");
        assert_eq!(message.thinking, Some("Okay".to_string()));
    }

    #[test]
    fn test_thinking_only_chunk_yields() {
        // Verify that a chunk with only thinking content (empty content) is valid
        let json = r#"{"role":"assistant","content":"","thinking":"reasoning..."}"#;
        let message: OllamaMessage = serde_json::from_str(json).unwrap();

        let has_content = !message.content.is_empty();
        let has_thinking = message.thinking.as_ref().is_some_and(|t| !t.is_empty());

        assert!(!has_content); // content is empty
        assert!(has_thinking); // thinking is present
        assert!(has_content || has_thinking); // Should yield this chunk
    }

    #[test]
    fn test_content_only_chunk_yields() {
        // Verify that a chunk with only content (no thinking) works
        let json = r#"{"role":"assistant","content":"Hello!"}"#;
        let message: OllamaMessage = serde_json::from_str(json).unwrap();

        let has_content = !message.content.is_empty();
        let has_thinking = message.thinking.as_ref().is_some_and(|t| !t.is_empty());

        assert!(has_content); // content is present
        assert!(!has_thinking); // no thinking
        assert!(has_content || has_thinking); // Should yield this chunk
    }

    #[test]
    fn test_empty_thinking_treated_as_absent() {
        // Verify that empty string thinking is treated as absent
        let json = r#"{"role":"assistant","content":"","thinking":""}"#;
        let message: OllamaMessage = serde_json::from_str(json).unwrap();

        let has_content = !message.content.is_empty();
        let has_thinking = message.thinking.as_ref().is_some_and(|t| !t.is_empty());

        assert!(!has_content);
        assert!(!has_thinking); // Empty string thinking is treated as absent
        assert!(!(has_content || has_thinking)); // Should NOT yield this chunk
    }

    #[test]
    fn test_both_content_and_thinking() {
        // Verify chunk with both content and thinking
        let json = r#"{"role":"assistant","content":"Hello","thinking":"deciding greeting"}"#;
        let message: OllamaMessage = serde_json::from_str(json).unwrap();

        let has_content = !message.content.is_empty();
        let has_thinking = message.thinking.as_ref().is_some_and(|t| !t.is_empty());

        assert!(has_content);
        assert!(has_thinking);
        assert_eq!(message.content, "Hello");
        assert_eq!(message.thinking, Some("deciding greeting".to_string()));
    }

    // Tests for JSON mode / structured output support

    #[test]
    fn test_ollama_format_json_serialization() {
        // Test that OllamaFormat::Json serializes to just "json"
        let format = OllamaFormat::Json("json".to_string());
        let json = serde_json::to_string(&format).unwrap();
        assert_eq!(json, r#""json""#);
    }

    #[test]
    fn test_ollama_format_json_schema_serialization() {
        // Test that OllamaFormat::JsonSchema serializes to the schema object
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "age": { "type": "integer" }
            },
            "required": ["name", "age"]
        });
        let format = OllamaFormat::JsonSchema(schema.clone());
        let json = serde_json::to_string(&format).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, schema);
    }

    #[test]
    fn test_convert_response_format_json() {
        use crate::types::ResponseFormat;

        let format = OllamaProvider::convert_response_format(Some(&ResponseFormat::Json));
        assert!(format.is_some());
        let json = serde_json::to_string(&format.unwrap()).unwrap();
        assert_eq!(json, r#""json""#);
    }

    #[test]
    fn test_convert_response_format_json_schema() {
        use crate::types::ResponseFormat;

        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "result": { "type": "string" }
            }
        });
        let format = OllamaProvider::convert_response_format(Some(&ResponseFormat::JsonSchema {
            schema: schema.clone(),
        }));
        assert!(format.is_some());
        let json = serde_json::to_string(&format.unwrap()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, schema);
    }

    #[test]
    fn test_convert_response_format_text() {
        use crate::types::ResponseFormat;

        let format = OllamaProvider::convert_response_format(Some(&ResponseFormat::Text));
        assert!(format.is_none());
    }

    #[test]
    fn test_convert_response_format_none() {
        let format = OllamaProvider::convert_response_format(None);
        assert!(format.is_none());
    }

    #[test]
    fn test_ollama_request_with_json_format_serialization() {
        // Test that OllamaRequest serializes correctly with format field
        let request = OllamaRequest {
            model: "llama3".to_string(),
            messages: vec![OllamaMessage {
                role: "user".to_string(),
                content: "Generate JSON".to_string(),
                images: None,
                thinking: None,
                tool_calls: None,
            }],
            stream: false,
            options: None,
            think: None,
            format: Some(OllamaFormat::Json("json".to_string())),
            tools: None,
            logprobs: None,
            top_logprobs: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains(r#""format":"json""#));
    }

    #[test]
    fn test_ollama_request_with_json_schema_format_serialization() {
        // Test that OllamaRequest serializes correctly with schema format
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            }
        });
        let request = OllamaRequest {
            model: "llama3".to_string(),
            messages: vec![OllamaMessage {
                role: "user".to_string(),
                content: "Generate JSON".to_string(),
                images: None,
                thinking: None,
                tool_calls: None,
            }],
            stream: false,
            options: None,
            think: None,
            format: Some(OllamaFormat::JsonSchema(schema)),
            tools: None,
            logprobs: None,
            top_logprobs: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        // Parse the JSON to check the format field properly (avoids key ordering issues)
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let format = parsed.get("format").expect("format field should exist");
        assert_eq!(format.get("type").unwrap(), "object");
        assert!(format.get("properties").is_some());
    }

    #[test]
    fn test_ollama_request_without_format_serialization() {
        // Test that OllamaRequest omits format field when None
        let request = OllamaRequest {
            model: "llama3".to_string(),
            messages: vec![OllamaMessage {
                role: "user".to_string(),
                content: "Hello".to_string(),
                images: None,
                thinking: None,
                tool_calls: None,
            }],
            stream: false,
            options: None,
            think: None,
            format: None,
            tools: None,
            logprobs: None,
            top_logprobs: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(!json.contains("format"));
    }

    #[test]
    fn test_ollama_capabilities_json_support() {
        // Test that Ollama provider reports JSON mode and schema support
        let provider = OllamaProvider::builder().build().unwrap();
        let capabilities = provider.get_capabilities();
        assert!(capabilities.supports_json_mode);
        assert!(capabilities.supports_json_schema);
    }

    // Tests for smart ThinkingMode behavior

    #[test]
    fn test_get_think_param_auto_thinking_model() {
        use crate::ThinkingMode;
        // Auto mode with thinking model should enable thinking
        assert_eq!(
            OllamaProvider::get_think_param(ThinkingMode::Auto, "qwen3:8b"),
            Some(true)
        );
        assert_eq!(
            OllamaProvider::get_think_param(ThinkingMode::Auto, "qwen3-vl:8b"),
            Some(true)
        );
        assert_eq!(
            OllamaProvider::get_think_param(ThinkingMode::Auto, "deepseek-r1:latest"),
            Some(true)
        );
    }

    #[test]
    fn test_get_think_param_auto_non_thinking_model() {
        use crate::ThinkingMode;
        // Auto mode with non-thinking model should omit parameter
        assert_eq!(
            OllamaProvider::get_think_param(ThinkingMode::Auto, "llama3:8b"),
            None
        );
        assert_eq!(
            OllamaProvider::get_think_param(ThinkingMode::Auto, "mistral:latest"),
            None
        );
        assert_eq!(
            OllamaProvider::get_think_param(ThinkingMode::Auto, "codellama:7b"),
            None
        );
    }

    #[test]
    fn test_get_think_param_enabled() {
        use crate::ThinkingMode;
        // Enabled should always return Some(true)
        assert_eq!(
            OllamaProvider::get_think_param(ThinkingMode::Enabled, "llama3:8b"),
            Some(true)
        );
        assert_eq!(
            OllamaProvider::get_think_param(ThinkingMode::Enabled, "qwen3:8b"),
            Some(true)
        );
    }

    #[test]
    fn test_get_think_param_disabled() {
        use crate::ThinkingMode;
        // Disabled should always return Some(false)
        assert_eq!(
            OllamaProvider::get_think_param(ThinkingMode::Disabled, "llama3:8b"),
            Some(false)
        );
        assert_eq!(
            OllamaProvider::get_think_param(ThinkingMode::Disabled, "qwen3:8b"),
            Some(false)
        );
    }

    #[test]
    fn test_get_think_param_omit() {
        use crate::ThinkingMode;
        // Omit should always return None (don't send parameter)
        assert_eq!(
            OllamaProvider::get_think_param(ThinkingMode::Omit, "llama3:8b"),
            None
        );
        assert_eq!(
            OllamaProvider::get_think_param(ThinkingMode::Omit, "qwen3:8b"),
            None
        );
        assert_eq!(
            OllamaProvider::get_think_param(ThinkingMode::Omit, "deepseek-r1:latest"),
            None
        );
    }
}
