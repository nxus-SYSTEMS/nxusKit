//! LM Studio provider implementation
//!
//! Provides native support for LM Studio's local REST API, including:
//! - Model discovery with rich metadata (quantization, context window, load status)
//! - Chat completions (streaming and non-streaming)
//! - OpenAI-compatible API endpoints
//! - Vision/multimodal capability detection

use crate::capability::{CapabilityDetector, ModelCapabilities, VisionMode};
use crate::error::{NxuskitError, Result};
use crate::parameter_adapter::ParameterAdapter;
use crate::provider::{LLMProvider, ModelLister};
use crate::token_estimator::{StreamingTokenAccumulator, TokenEstimator};
use crate::types::{
    ChatRequest, ChatResponse, FinishReason, InferenceMetadata, MessageContent, ModelInfo,
    ProviderCapabilities, Role, StreamChunk, TokenCount, TokenUsage,
};
use async_trait::async_trait;
use futures::Stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Default LM Studio base URL
pub const DEFAULT_LM_STUDIO_URL: &str = "http://127.0.0.1:1234/v1";

/// Default timeout for API requests (30 seconds for local server)
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// LM Studio provider for local model inference
///
/// # Examples
///
/// ```no_run
/// use nxuskit_engine::providers::LmStudioProvider;
/// use nxuskit_engine::types::{ChatRequest, Message};
/// use nxuskit_engine::provider::LLMProvider;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let provider = LmStudioProvider::builder()
///     .model("llama-2-7b-chat.Q4_K_M")
///     .build()?;
///
/// let request = ChatRequest::new("llama-2-7b-chat.Q4_K_M")
///     .with_message(Message::user("Hello!"));
///
/// let response = provider.chat(&request).await?;
/// println!("{}", response.content);
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct LmStudioProvider {
    client: Client,
    base_url: String,
    api_key: Option<String>,
    default_model: String,
    timeout: Duration,
}

impl LmStudioProvider {
    /// Create a new builder for configuring the provider
    pub fn builder() -> LmStudioProviderBuilder {
        LmStudioProviderBuilder::new()
    }

    /// Get the default model name configured for this provider
    pub fn default_model(&self) -> &str {
        &self.default_model
    }

    /// Extract text content from MessageContent
    /// For now, only handles simple text messages (vision support can be added later)
    fn extract_text_content(content: &MessageContent) -> String {
        match content {
            MessageContent::Text(text) => text.clone(),
            MessageContent::Parts(parts) => {
                // Extract text from all text parts, ignoring images for now
                parts
                    .iter()
                    .filter_map(|part| match part {
                        crate::types::ContentPart::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }
    }

    /// List all available models from LM Studio
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nxuskit_engine::providers::LmStudioProvider;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let provider = LmStudioProvider::builder()
    ///     .model("llama-2-7b-chat")
    ///     .build()?;
    ///
    /// let models = provider.list_models().await?;
    /// for model in models {
    ///     println!("{}: {:?}", model.name, model.metadata);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let url = format!("{}/models", self.base_url);
        let mut request = self.client.get(&url).timeout(self.timeout);

        if let Some(api_key) = &self.api_key {
            request = request.bearer_auth(api_key);
        }

        let response = request.send().await.map_err(|e| {
            if e.is_timeout() {
                NxuskitError::Configuration(format!(
                    "Timeout connecting to LM Studio at {}. Is LM Studio running?",
                    self.base_url
                ))
            } else if e.is_connect() {
                NxuskitError::Configuration(format!(
                    "Failed to connect to LM Studio at {}. Is LM Studio running with API server enabled?",
                    self.base_url
                ))
            } else {
                NxuskitError::Network(e)
            }
        })?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(NxuskitError::provider(status, error_text));
        }

        let models_response: LmStudioModelsResponse = response.json().await?;

        // Convert LM Studio model objects to ModelInfo
        Ok(models_response
            .data
            .into_iter()
            .map(|model| self.convert_model_info(model))
            .collect())
    }

    /// Convert LM Studio model object to ModelInfo
    fn convert_model_info(&self, model: LmStudioModelObject) -> ModelInfo {
        let mut metadata = std::collections::HashMap::new();

        // Add LM Studio-specific metadata with lmstudio_ prefix
        if let Some(quantization) = &model.quantization {
            metadata.insert("lmstudio_quantization".to_string(), quantization.clone());
        }
        if let Some(architecture) = &model.architecture {
            metadata.insert("lmstudio_architecture".to_string(), architecture.clone());
        }
        if let Some(context_length) = model.context_length {
            metadata.insert(
                "lmstudio_context_length".to_string(),
                context_length.to_string(),
            );
        }
        if let Some(state) = &model.state {
            metadata.insert("lmstudio_state".to_string(), state.clone());
        }
        if let Some(format) = &model.format {
            metadata.insert("lmstudio_format".to_string(), format.clone());
        }
        if let Some(size_bytes) = model.size_bytes {
            metadata.insert("lmstudio_size_bytes".to_string(), size_bytes.to_string());
        }

        // Build description
        let mut desc_parts = Vec::new();
        if let Some(arch) = &model.architecture {
            desc_parts.push(arch.clone());
        }
        if let Some(quant) = &model.quantization {
            desc_parts.push(format!("({})", quant));
        }
        let description = if desc_parts.is_empty() {
            None
        } else {
            Some(desc_parts.join(" "))
        };

        ModelInfo {
            name: model.id.clone(),
            size_bytes: None, // LM Studio doesn't provide size in API response
            description,
            context_window: model.context_length.map(|c| c as u32),
            metadata,
        }
    }

    /// Build request headers
    fn build_headers(&self) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            "application/json".parse().unwrap(),
        );
        if let Some(api_key) = &self.api_key {
            headers.insert(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", api_key).parse().unwrap(),
            );
        }
        headers
    }
}

impl LmStudioProvider {
    /// Create a fresh session with no accumulated state.
    ///
    /// For LmStudioProvider, this returns a new instance with the same configuration
    /// since it's already stateless (each request is independent).
    ///
    /// # Returns
    ///
    /// A new LmStudioProvider instance with the same configuration.
    pub fn fresh_session(&self) -> Result<Self> {
        LmStudioProvider::builder()
            .base_url(self.base_url.clone())
            .model(self.default_model.clone())
            .timeout(self.timeout)
            .build()
    }
}

/// Builder for LmStudioProvider
#[derive(Debug, Clone)]
pub struct LmStudioProviderBuilder {
    base_url: Option<String>,
    api_key: Option<String>,
    model: Option<String>,
    timeout: Duration,
}

impl LmStudioProviderBuilder {
    fn new() -> Self {
        Self {
            base_url: None,
            api_key: None,
            model: None,
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// Set the base URL for LM Studio API
    ///
    /// Default: `http://127.0.0.1:1234/v1`
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into());
        self
    }

    /// Set the API key (optional for LM Studio)
    pub fn api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    /// Set the default model name
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Set request timeout
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Build the LmStudioProvider
    pub fn build(self) -> Result<LmStudioProvider> {
        let model = self
            .model
            .ok_or_else(|| NxuskitError::Configuration("Model name is required".to_string()))?;

        let base_url = self
            .base_url
            .unwrap_or_else(|| DEFAULT_LM_STUDIO_URL.to_string());

        // Build reqwest client with configured timeouts using the centralized helper
        // LMStudio uses a single timeout for simplicity, applied as total_timeout
        // Connection and read timeouts use reasonable defaults for local LLM usage
        let connection_timeout = Duration::from_secs(30); // Local connections are fast
        let read_timeout = self.timeout; // Use configured timeout for streaming chunks
        let client = super::build_http_client(connection_timeout, read_timeout, self.timeout)?;

        Ok(LmStudioProvider {
            client,
            base_url,
            api_key: self.api_key,
            default_model: model,
            timeout: self.timeout,
        })
    }
}

impl Default for LmStudioProviderBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// LM Studio API response types

#[derive(Debug, Deserialize)]
struct LmStudioModelsResponse {
    data: Vec<LmStudioModelObject>,
}

#[derive(Debug, Deserialize)]
struct LmStudioModelObject {
    id: String,
    #[serde(default)]
    architecture: Option<String>,
    #[serde(default)]
    quantization: Option<String>,
    #[serde(default)]
    context_length: Option<usize>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    size_bytes: Option<u64>,
}

/// OpenAI-compatible chat completion request
#[derive(Debug, Serialize)]
struct LmStudioChatRequest {
    model: String,
    messages: Vec<LmStudioMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Debug, Serialize)]
struct LmStudioMessage {
    role: String,
    content: String,
}

/// OpenAI-compatible chat completion response
#[derive(Debug, Deserialize)]
struct LmStudioChatResponse {
    choices: Vec<LmStudioChoice>,
    #[serde(default)]
    usage: Option<LmStudioUsage>,
    #[serde(default)]
    model: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LmStudioChoice {
    message: LmStudioResponseMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LmStudioResponseMessage {
    content: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)] // total_tokens is in API response but we compute from parts
struct LmStudioUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

/// SSE streaming response chunk
#[derive(Debug, Deserialize)]
struct LmStudioStreamChunk {
    choices: Vec<LmStudioStreamChoice>,
}

#[derive(Debug, Deserialize)]
struct LmStudioStreamChoice {
    delta: LmStudioDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LmStudioDelta {
    #[serde(default)]
    content: Option<String>,
}

#[async_trait]
impl LLMProvider for LmStudioProvider {
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse> {
        // Adapt request to provider capabilities
        let capabilities = self.get_capabilities();
        let adapted = ParameterAdapter::adapt(request, &capabilities);
        let adapted_request = &adapted.request;

        let url = format!("{}/chat/completions", self.base_url);

        // Convert messages to LM Studio format
        let messages: Vec<LmStudioMessage> = adapted_request
            .messages
            .iter()
            .map(|msg| LmStudioMessage {
                role: match msg.role {
                    Role::User => "user".to_string(),
                    Role::Assistant => "assistant".to_string(),
                    Role::System => "system".to_string(),
                },
                content: Self::extract_text_content(&msg.content),
            })
            .collect();

        let lms_request = LmStudioChatRequest {
            model: adapted_request.model.clone(),
            messages,
            temperature: adapted_request.temperature,
            max_tokens: adapted_request.max_tokens,
            stream: Some(false),
        };

        let mut req = self
            .client
            .post(&url)
            .headers(self.build_headers())
            .json(&lms_request)
            .timeout(self.timeout);

        if let Some(api_key) = &self.api_key {
            req = req.bearer_auth(api_key);
        }

        let response = req.send().await.map_err(|e| {
            if e.is_timeout() {
                NxuskitError::Configuration(format!(
                    "Request timeout after {:?}. Model may be loading or responding slowly.",
                    self.timeout
                ))
            } else if e.is_connect() {
                NxuskitError::Configuration(format!(
                    "Failed to connect to LM Studio at {}. Ensure LM Studio is running with a model loaded.",
                    self.base_url
                ))
            } else {
                NxuskitError::Network(e)
            }
        })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());

            return Err(match status.as_u16() {
                400 => {
                    if error_text.contains("model") && error_text.contains("not loaded") {
                        NxuskitError::Configuration(format!(
                            "Model '{}' is not loaded in LM Studio. Please load a model first.",
                            request.model
                        ))
                    } else {
                        NxuskitError::InvalidRequest(error_text)
                    }
                }
                404 => NxuskitError::Configuration(format!(
                    "Model '{}' not found. Check available models with list_models().",
                    request.model
                )),
                _ => NxuskitError::provider(status.as_u16(), error_text),
            });
        }

        let lms_response: LmStudioChatResponse = response.json().await.map_err(|e| {
            NxuskitError::Stream(format!("Failed to parse LM Studio response: {}", e))
        })?;

        // Convert to ChatResponse
        let choice = lms_response
            .choices
            .first()
            .ok_or_else(|| NxuskitError::Provider {
                status: 200,
                message: "No choices in response".to_string(),
            })?;

        let usage = lms_response
            .usage
            .map(|u| {
                let token_count = TokenCount::new(u.prompt_tokens, u.completion_tokens);
                TokenUsage::with_actual(token_count, token_count)
            })
            .unwrap_or_else(|| {
                let token_count = TokenCount::new(0, 0);
                TokenUsage::estimated_only(token_count)
            });

        let mut response = ChatResponse::new(
            choice.message.content.clone(),
            lms_response
                .model
                .unwrap_or_else(|| adapted_request.model.clone()),
            usage,
        );
        response.provider = self.provider_name().to_string();

        // Add finish_reason if available
        if let Some(finish_reason_str) = &choice.finish_reason
            && let Some(finish_reason) =
                crate::types::FinishReason::from_str_flexible(finish_reason_str)
        {
            response = response.with_finish_reason(finish_reason);
        }

        // Add parameter adaptation warnings
        response.warnings = adapted.warnings;

        // Populate inference metadata
        response.inference_metadata =
            InferenceMetadata::completed(response.finish_reason.unwrap_or(FinishReason::Stop))
                .with_token_usage(response.usage.clone())
                .with_provider_metadata(serde_json::json!({
                    "provider": "lmstudio"
                }));

        Ok(response)
    }

    async fn chat_stream(
        &self,
        request: &ChatRequest,
    ) -> Result<Box<dyn Stream<Item = Result<StreamChunk>> + Send + Unpin>> {
        use async_stream::stream;
        use futures::StreamExt;

        let url = format!("{}/chat/completions", self.base_url);

        // Convert messages to LM Studio format
        let messages: Vec<LmStudioMessage> = request
            .messages
            .iter()
            .map(|msg| LmStudioMessage {
                role: match msg.role {
                    Role::User => "user".to_string(),
                    Role::Assistant => "assistant".to_string(),
                    Role::System => "system".to_string(),
                },
                content: Self::extract_text_content(&msg.content),
            })
            .collect();

        let lms_request = LmStudioChatRequest {
            model: request.model.clone(),
            messages,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stream: Some(true),
        };

        let mut req = self
            .client
            .post(&url)
            .headers(self.build_headers())
            .json(&lms_request)
            .timeout(self.timeout);

        if let Some(api_key) = &self.api_key {
            req = req.bearer_auth(api_key);
        }

        let response = req.send().await.map_err(|e| {
            if e.is_timeout() {
                NxuskitError::Configuration(format!("Request timeout after {:?}", self.timeout))
            } else if e.is_connect() {
                NxuskitError::Configuration(format!(
                    "Failed to connect to LM Studio at {}",
                    self.base_url
                ))
            } else {
                NxuskitError::Network(e)
            }
        })?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(NxuskitError::provider(status, error_text));
        }

        let byte_stream = response.bytes_stream();

        // Initialize token tracking outside stream! macro to avoid lifetime issues
        let model = request.model.clone();
        let estimator = TokenEstimator::for_model(&model);
        let prompt_tokens = estimator.count_messages(&request.messages);

        let output_stream = stream! {
            let mut buffer = String::new();
            let mut stream = byte_stream;
            let mut accumulator = StreamingTokenAccumulator::new(estimator, prompt_tokens);

            'stream_loop: while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(bytes) => {
                        let text = match std::str::from_utf8(&bytes) {
                            Ok(t) => t,
                            Err(e) => {
                                yield Err(NxuskitError::Stream(format!("Invalid UTF-8: {}", e)));
                                continue;
                            }
                        };

                        buffer.push_str(text);

                        // Process complete SSE events
                        while let Some(event_end) = buffer.find("\n\n") {
                            let event = buffer[..event_end].to_string();
                            buffer = buffer[event_end + 2..].to_string();

                            // Parse SSE event
                            for line in event.lines() {
                                if let Some(data) = line.strip_prefix("data: ") {
                                    // Check for stream end
                                    if data.trim() == "[DONE]" {
                                        let final_usage = accumulator.finalize();
                                        yield Ok(StreamChunk::final_chunk(crate::types::FinishReason::Stop, Some(final_usage)));
                                        break 'stream_loop;
                                    }

                                    // Parse JSON chunk
                                    match serde_json::from_str::<LmStudioStreamChunk>(data) {
                                        Ok(chunk) => {
                                            if let Some(choice) = chunk.choices.first() {
                                                if let Some(content) = &choice.delta.content {
                                                    accumulator.add_chunk(content);
                                                    let usage = accumulator.running_total();
                                                    let mut stream_chunk = StreamChunk::new(content.clone());
                                                    stream_chunk.usage = Some(usage);
                                                    yield Ok(stream_chunk);
                                                }

                                                // Handle finish_reason
                                                if let Some(finish_reason_str) = &choice.finish_reason {
                                                    let finish_reason = crate::types::FinishReason::from_str_flexible(finish_reason_str)
                                                        .unwrap_or(crate::types::FinishReason::Stop);
                                                    let final_usage = accumulator.finalize();
                                                    yield Ok(StreamChunk::final_chunk(finish_reason, Some(final_usage)));
                                                    break 'stream_loop;
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            accumulator.mark_interrupted();
                                            yield Err(NxuskitError::Stream(format!(
                                                "Failed to parse chunk: {}",
                                                e
                                            )));
                                            break 'stream_loop;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        accumulator.mark_interrupted();
                        yield Err(NxuskitError::Network(e));
                        break 'stream_loop;
                    }
                }
            }
        };

        Ok(Box::new(Box::pin(output_stream)))
    }

    fn provider_name(&self) -> &str {
        "lmstudio"
    }

    fn get_capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_system_messages: true,
            supports_streaming: true,
            supports_vision: true,
            max_stop_sequences: Some(4),
            supports_presence_penalty: true,
            supports_frequency_penalty: true,
            supports_seed: true,
            supports_logprobs: false,

            // T055: LM Studio exposes an OAI-compatible /v1/chat/completions
            // endpoint but no documented streaming-logprob field was found in
            // the v0.9.4 Sprint 1 window and no fixture exists. Kept false per
            // reconciled planning decision (default non-supporting). Flip to
            // true (and wire decode_oai_logprob_delta from openai.rs) when
            // T069 local live test confirms support and a fixture is committed.
            supports_streaming_logprobs: false,
            supports_json_mode: true,
            supports_json_schema: false,
            penalty_range: Some((-2.0, 2.0)),
            max_logprobs: None,
        }
    }

    fn as_capability_detector(&self) -> Option<&dyn CapabilityDetector> {
        Some(self)
    }
}

#[async_trait]
impl ModelLister for LmStudioProvider {
    async fn list_available_models(&self) -> Result<Vec<ModelInfo>> {
        // Delegate to list_models() which already has the correct implementation
        self.list_models().await
    }
}

#[async_trait]
impl CapabilityDetector for LmStudioProvider {
    async fn get_model_capabilities(&self, model: &str) -> Result<ModelCapabilities> {
        // Try to get metadata from list_models for enhanced accuracy
        match self.list_models().await {
            Ok(models) => {
                if let Some(info) = models.iter().find(|m| m.name == model) {
                    // Check for modalities in metadata
                    let has_vision_metadata = info
                        .metadata
                        .get("lmstudio_modalities")
                        .map(|m| m.contains("vision") || m.contains("image"))
                        .unwrap_or(false);

                    let vision_mode = if has_vision_metadata {
                        VisionMode::SingleImage
                    } else {
                        Self::detect_vision_from_name(model)
                    };

                    Ok(ModelCapabilities {
                        vision_mode,
                        supports_streaming: true,
                        supports_function_calling: false,
                    })
                } else {
                    // Model not found in list, use name-based detection
                    Ok(ModelCapabilities {
                        vision_mode: Self::detect_vision_from_name(model),
                        supports_streaming: true,
                        supports_function_calling: false,
                    })
                }
            }
            Err(_) => {
                // If we can't fetch models, fall back to name-based detection
                Ok(ModelCapabilities {
                    vision_mode: Self::detect_vision_from_name(model),
                    supports_streaming: true,
                    supports_function_calling: false,
                })
            }
        }
    }
}

impl LmStudioProvider {
    /// Detect vision capabilities from model name
    ///
    /// Uses heuristics based on common vision model naming patterns.
    /// Falls back to metadata-based detection when available.
    pub fn detect_vision_from_name(model_name: &str) -> VisionMode {
        let model_lower = model_name.to_lowercase();

        // Common vision model patterns
        let vision_patterns = [
            "llava",      // LLaVA family
            "bakllava",   // BakLLaVA
            "moondream",  // Moondream
            "cogvlm",     // CogVLM
            "qwen-vl",    // Qwen-VL
            "internvl",   // InternVL
            "vision",     // Generic vision indicator
            "vl-",        // Vision-Language prefix
            "multimodal", // Explicit multimodal
        ];

        for pattern in &vision_patterns {
            if model_lower.contains(pattern) {
                // Most vision models support single image
                // Could be enhanced with specific multi-image detection
                return VisionMode::SingleImage;
            }
        }

        // No vision support detected
        VisionMode::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_requires_model() {
        let result = LmStudioProvider::builder().build();
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Model name is required")
        );
    }

    #[test]
    fn test_builder_with_defaults() {
        let provider = LmStudioProvider::builder()
            .model("test-model")
            .build()
            .unwrap();

        assert_eq!(provider.base_url, DEFAULT_LM_STUDIO_URL);
        assert_eq!(provider.default_model, "test-model");
        assert_eq!(provider.timeout, DEFAULT_TIMEOUT);
        assert!(provider.api_key.is_none());
    }

    #[test]
    fn test_builder_with_custom_values() {
        let provider = LmStudioProvider::builder()
            .model("llama2")
            .base_url("http://localhost:8080/v1")
            .api_key("test-key")
            .timeout(Duration::from_secs(60))
            .build()
            .unwrap();

        assert_eq!(provider.base_url, "http://localhost:8080/v1");
        assert_eq!(provider.default_model, "llama2");
        assert_eq!(provider.api_key, Some("test-key".to_string()));
        assert_eq!(provider.timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_model_info_conversion() {
        let provider = LmStudioProvider::builder().model("test").build().unwrap();

        let lms_model = LmStudioModelObject {
            id: "llama-2-7b-chat.Q4_K_M".to_string(),
            architecture: Some("Llama".to_string()),
            quantization: Some("Q4_K_M".to_string()),
            context_length: Some(4096),
            state: Some("loaded".to_string()),
            format: Some("GGUF".to_string()),
            size_bytes: Some(4_000_000_000),
        };

        let model_info = provider.convert_model_info(lms_model);

        assert_eq!(model_info.name, "llama-2-7b-chat.Q4_K_M");
        assert_eq!(model_info.description, Some("Llama (Q4_K_M)".to_string()));
        assert_eq!(model_info.context_window, Some(4096));
        assert_eq!(
            model_info.metadata.get("lmstudio_quantization"),
            Some(&"Q4_K_M".to_string())
        );
        assert_eq!(
            model_info.metadata.get("lmstudio_state"),
            Some(&"loaded".to_string())
        );
    }
}
