//! Fireworks AI provider implementation
//!
//! Fireworks AI provides high-performance inference for open source models through an OpenAI-compatible API.
//!
//! **Note**: This is a premium provider that requires a nxusKit Pro license.
//! In the free tier, all API methods will return `LicenseRequired` errors.

use async_stream::stream;
use async_trait::async_trait;
use futures::Stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::{
    ChatRequest, ChatResponse, LLMProvider, Message, ModelInfo, Role, StreamChunk, TokenCount,
    TokenUsage,
    error::{NxuskitError, Result},
    parameter_adapter::ParameterAdapter,
    token_estimator::{StreamingTokenAccumulator, TokenEstimator},
    types::{
        ContentPart, FinishReason, ImageData, InferenceMetadata, MessageContent,
        ProviderCapabilities,
    },
};

/// Fireworks AI provider
///
/// Provides high-performance access to open source models via Fireworks AI's OpenAI-compatible API.
/// Supports chat completions, streaming, and vision capabilities.
///
/// **Note**: This is a premium provider that requires a nxusKit Pro license.
/// In the free tier, all API methods will return `LicenseRequired` errors.
#[derive(Debug, Clone)]
pub struct FireworksProvider {
    #[allow(dead_code)]
    client: Client,
    #[allow(dead_code)]
    api_key: String,
    #[allow(dead_code)]
    base_url: String,
    #[allow(dead_code)]
    default_model: String,
    #[allow(dead_code)]
    connection_timeout: Duration,
    #[allow(dead_code)]
    stream_read_timeout: Duration,
    #[allow(dead_code)]
    total_timeout: Duration,
}

impl FireworksProvider {
    /// Create a new Fireworks provider with the given API key
    ///
    /// # Deprecated
    /// Use `FireworksProvider::builder()` instead for more configuration options
    pub fn new(api_key: impl Into<String>) -> Self {
        let connection_timeout = Duration::from_secs(60);
        let stream_read_timeout = Duration::from_secs(120);
        let total_timeout = Duration::from_secs(60);

        // Use centralized helper for consistent timeout handling with read_timeout
        let client =
            super::build_http_client(connection_timeout, stream_read_timeout, total_timeout)
                .expect("Failed to build HTTP client");

        Self {
            client,
            api_key: api_key.into(),
            base_url: "https://api.fireworks.ai/inference/v1".to_string(),
            default_model: "accounts/fireworks/models/llama-v3-70b-instruct".to_string(),
            connection_timeout,
            stream_read_timeout,
            total_timeout,
        }
    }

    /// Set a custom base URL for the Fireworks API
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Create a new builder for FireworksProvider
    pub fn builder() -> FireworksProviderBuilder {
        FireworksProviderBuilder::default()
    }

    /// Get the configured default model
    pub fn default_model(&self) -> &str {
        &self.default_model
    }

    /// Get the configured connection timeout
    pub fn connection_timeout(&self) -> Duration {
        self.connection_timeout
    }

    /// Get the configured stream read timeout
    pub fn stream_read_timeout(&self) -> Duration {
        self.stream_read_timeout
    }

    /// Get the configured total timeout
    pub fn total_timeout(&self) -> Duration {
        self.total_timeout
    }

    /// Convert nxusKit messages to Fireworks API format
    fn convert_messages(&self, messages: &[Message]) -> Vec<FireworksMessage> {
        messages
            .iter()
            .map(|msg| {
                let role = match msg.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                };

                let content = match &msg.content {
                    MessageContent::Text(text) => FireworksMessageContent::Text(text.clone()),
                    MessageContent::Parts(parts) => {
                        // Fireworks supports vision in some models
                        let converted_parts: Vec<FireworksContentPart> = parts
                            .iter()
                            .map(|part| match part {
                                ContentPart::Text { text } => {
                                    FireworksContentPart::Text { text: text.clone() }
                                }
                                ContentPart::Image { source } => {
                                    let url = match &source.data {
                                        ImageData::Url { url } => url.clone(),
                                        ImageData::Base64 { media_type, data } => {
                                            format!("data:{};base64,{}", media_type, data)
                                        }
                                    };
                                    FireworksContentPart::ImageUrl {
                                        image_url: FireworksImageUrl { url },
                                    }
                                }
                            })
                            .collect();

                        FireworksMessageContent::Parts(converted_parts)
                    }
                };

                FireworksMessage {
                    role: role.to_string(),
                    content,
                }
            })
            .collect()
    }

    /// Build Fireworks API request from adapted ChatRequest
    fn build_request(&self, request: &ChatRequest) -> Result<FireworksRequest> {
        let messages = self.convert_messages(&request.messages);

        let mut fireworks_request = FireworksRequest {
            model: request.model.clone(),
            messages,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            top_p: request.top_p,
            stream: Some(false),
            stop: request.stop.clone(),
            presence_penalty: request.presence_penalty,
            frequency_penalty: request.frequency_penalty,
            seed: request.seed,
            response_format: None,
        };

        // Handle JSON mode
        if let Some(ref format) = request.response_format {
            use crate::types::ResponseFormat;
            match format {
                ResponseFormat::Json => {
                    fireworks_request.response_format = Some(FireworksResponseFormat {
                        r#type: "json_object".to_string(),
                    });
                }
                ResponseFormat::JsonSchema { .. } => {
                    // Fireworks doesn't support JSON schema, will be handled by adapter with warning
                }
                ResponseFormat::Text => {
                    // Default, no action needed
                }
            }
        }

        Ok(fireworks_request)
    }
}

impl FireworksProvider {
    /// Create a fresh session with no accumulated state.
    ///
    /// For FireworksProvider, this returns a clone with the same configuration
    /// since it's already stateless (each request is independent).
    ///
    /// # Returns
    ///
    /// A cloned FireworksProvider instance with the same configuration.
    pub fn fresh_session(&self) -> Self {
        self.clone()
    }
}

/// Builder for FireworksProvider
#[derive(Debug, Default)]
pub struct FireworksProviderBuilder {
    api_key: Option<String>,
    base_url: Option<String>,
    model: Option<String>,
    timeout: Option<Duration>,
    connection_timeout: Option<Duration>,
    stream_read_timeout: Option<Duration>,
    total_timeout: Option<Duration>,
}

impl FireworksProviderBuilder {
    /// Set the API key for Fireworks AI
    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// Set a custom base URL for the Fireworks API
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    /// Set the default model to use
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Set a general timeout for all operations
    /// This will be used as a fallback for specific timeouts
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Set the connection timeout
    /// Falls back to general timeout if not set
    pub fn connection_timeout(mut self, timeout: Duration) -> Self {
        self.connection_timeout = Some(timeout);
        self
    }

    /// Set the stream read timeout
    /// Falls back to general timeout if not set
    pub fn stream_read_timeout(mut self, timeout: Duration) -> Self {
        self.stream_read_timeout = Some(timeout);
        self
    }

    /// Set the total request timeout
    /// Falls back to general timeout if not set
    pub fn total_timeout(mut self, timeout: Duration) -> Self {
        self.total_timeout = Some(timeout);
        self
    }

    /// Build the FireworksProvider
    pub fn build(self) -> Result<FireworksProvider> {
        let api_key = self
            .api_key
            .ok_or_else(|| NxuskitError::Configuration("API key is required".to_string()))?;

        let default_timeout = Duration::from_secs(60);
        let default_stream_timeout = Duration::from_secs(120);

        // Timeout fallback chain: specific > general > default
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

        // Build reqwest client with configured timeouts using the centralized helper
        // This ensures read_timeout is set for proper streaming support
        let client =
            super::build_http_client(connection_timeout, stream_read_timeout, total_timeout)?;

        Ok(FireworksProvider {
            client,
            api_key,
            base_url: self
                .base_url
                .unwrap_or_else(|| "https://api.fireworks.ai/inference/v1".to_string()),
            default_model: self
                .model
                .unwrap_or_else(|| "accounts/fireworks/models/llama-v3-70b-instruct".to_string()),
            connection_timeout,
            stream_read_timeout,
            total_timeout,
        })
    }
}

/// Enrich model info with descriptions for known models
fn enrich_fireworks_model(info: &mut ModelInfo) {
    let model_id = &info.name;

    // Match by model family patterns
    let (description, context_window) = if model_id.contains("llama") || model_id.contains("Llama")
    {
        if model_id.contains("4") || model_id.contains("maverick") || model_id.contains("Maverick")
        {
            (
                Some(
                    "Llama 4 Maverick: SOTA intelligence with blazing speed across languages (1.05M context)",
                ),
                Some(1_050_000),
            )
        } else if model_id.contains("scout") || model_id.contains("Scout") {
            (
                Some("Llama 4 Scout: Versatile general-purpose LLM with multi-modal capabilities"),
                Some(128_000),
            )
        } else if model_id.contains("3.2") || model_id.contains("3-2") {
            if model_id.contains("vision") || model_id.contains("Vision") {
                (
                    Some(
                        "Llama 3.2 Vision: Multimodal model for image understanding and visual reasoning",
                    ),
                    Some(128_000),
                )
            } else {
                (
                    Some("Llama 3.2: Latest Meta model with improved capabilities"),
                    Some(128_000),
                )
            }
        } else if model_id.contains("3.1") || model_id.contains("3-1") {
            if model_id.contains("405") {
                (
                    Some("Llama 3.1 405B: Massive flagship for the most demanding reasoning tasks"),
                    Some(128_000),
                )
            } else {
                (
                    Some("Llama 3.1: Meta's advanced model optimized for Fireworks speed"),
                    Some(128_000),
                )
            }
        } else if model_id.contains("code") {
            (
                Some("Code Llama: Specialized model for programming tasks and code generation"),
                Some(100_000),
            )
        } else {
            (
                Some("Llama: Meta's open-source model on Fireworks infrastructure"),
                Some(8_192),
            )
        }
    } else if model_id.contains("qwen") || model_id.contains("Qwen") {
        if model_id.contains("3") {
            if model_id.contains("235") {
                (
                    Some(
                        "Qwen 3 235B: Massive 128-expert MoE with 22B active parameters, Apache-2.0",
                    ),
                    Some(128_000),
                )
            } else {
                (
                    Some(
                        "Qwen 3: Latest gen with controllable chain-of-thought and tool calling at frontier scale",
                    ),
                    Some(128_000),
                )
            }
        } else if model_id.contains("2.5") || model_id.contains("2-5") {
            (
                Some("Qwen 2.5: Advanced model for structured data and 30+ languages"),
                Some(128_000),
            )
        } else if model_id.contains("vl") || model_id.contains("VL") {
            (
                Some("Qwen 2-VL: Vision-language model for multimodal understanding"),
                Some(32_768),
            )
        } else {
            (Some("Qwen: Alibaba's multilingual model"), Some(32_768))
        }
    } else if model_id.contains("mixtral") || model_id.contains("Mixtral") {
        if model_id.contains("8x22") {
            (
                Some("Mixtral 8x22B: Large sparse MoE with 176B total parameters, 44B active"),
                Some(64_000),
            )
        } else if model_id.contains("8x7") {
            (
                Some(
                    "Mixtral 8x7B: Efficient MoE, first hosted on Fireworks before public release",
                ),
                Some(32_768),
            )
        } else {
            (
                Some("Mixtral: Mistral's Mixture of Experts model"),
                Some(32_768),
            )
        }
    } else if model_id.contains("deepseek") || model_id.contains("DeepSeek") {
        (
            Some("DeepSeek: Advanced reasoning model with chain-of-thought capabilities"),
            Some(128_000),
        )
    } else if model_id.contains("dbrx") || model_id.contains("DBRX") {
        (
            Some("DBRX: Databricks' powerful MoE model for enterprise applications"),
            Some(32_768),
        )
    } else {
        (None, None)
    };

    if let Some(desc) = description {
        info.description = Some(desc.to_string());
    }
    if let Some(ctx) = context_window {
        info.context_window = Some(ctx);
    }
}

// Pro tier implementation - full functionality
#[async_trait]
impl LLMProvider for FireworksProvider {
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse> {
        // Adapt request to provider capabilities
        let capabilities = self.get_capabilities();
        let adapted = ParameterAdapter::adapt(request, &capabilities);
        let adapted_request = &adapted.request;

        // Build Fireworks-specific request
        let fireworks_request = self.build_request(adapted_request)?;

        // Make API call
        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&fireworks_request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let retry_after = super::parse_retry_after(response.headers());
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(match status.as_u16() {
                429 => NxuskitError::rate_limit(retry_after),
                _ => NxuskitError::provider(status.as_u16(), error_text),
            });
        }

        let fireworks_response: FireworksResponse = response.json().await?;

        // Extract content from first choice
        let content = fireworks_response
            .choices
            .first()
            .and_then(|choice| choice.message.content.as_deref())
            .unwrap_or("")
            .to_string();

        // Convert finish reason
        let finish_reason = fireworks_response
            .choices
            .first()
            .and_then(|choice| choice.finish_reason.as_ref())
            .and_then(|reason| match reason.as_str() {
                "stop" => Some(crate::types::FinishReason::Stop),
                "length" => Some(crate::types::FinishReason::Length),
                _ => None,
            });

        let mut response = ChatResponse::new(
            content,
            fireworks_response.model,
            TokenUsage::estimated_only(TokenCount::new(
                fireworks_response.usage.prompt_tokens,
                fireworks_response.usage.completion_tokens,
            )),
        );
        response.provider = self.provider_name().to_string();

        response.finish_reason = finish_reason;
        response.warnings = adapted.warnings;

        // Populate inference metadata
        response.inference_metadata =
            InferenceMetadata::completed(response.finish_reason.unwrap_or(FinishReason::Stop))
                .with_token_usage(response.usage.clone())
                .with_provider_metadata(serde_json::json!({
                    "provider": "fireworks"
                }));

        Ok(response)
    }

    async fn chat_stream(
        &self,
        request: &ChatRequest,
    ) -> Result<Box<dyn Stream<Item = Result<StreamChunk>> + Send + Unpin>> {
        // Adapt request to provider capabilities
        let capabilities = self.get_capabilities();
        let adapted = ParameterAdapter::adapt(request, &capabilities);
        let adapted_request = &adapted.request;

        // Build Fireworks-specific request with streaming enabled
        let mut fireworks_request = self.build_request(adapted_request)?;
        fireworks_request.stream = Some(true);

        // Make API call
        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&fireworks_request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let retry_after = super::parse_retry_after(response.headers());
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(match status.as_u16() {
                429 => NxuskitError::rate_limit(retry_after),
                _ => NxuskitError::provider(status.as_u16(), error_text),
            });
        }

        let bytes_stream = response.bytes_stream();

        // Initialize token tracking outside stream! macro to avoid lifetime issues
        let model = adapted_request.model.clone();
        let estimator = TokenEstimator::for_model(&model);
        let prompt_tokens = estimator.count_messages(&adapted_request.messages);

        let output_stream = stream! {
            use futures::StreamExt;

            let mut buffer = String::new();
            let mut stream = bytes_stream;
            let mut accumulator = StreamingTokenAccumulator::new(estimator, prompt_tokens);

            'stream_loop: while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(bytes) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));

                        // Process complete lines
                        while let Some(newline_pos) = buffer.find('\n') {
                            let line = buffer[..newline_pos].trim().to_string();
                            buffer = buffer[newline_pos + 1..].to_string();

                            if line.is_empty() || line == "data: [DONE]" {
                                continue;
                            }

                            if let Some(data) = line.strip_prefix("data: ") {
                                match serde_json::from_str::<FireworksStreamResponse>(data) {
                                    Ok(stream_response) => {
                                        if let Some(choice) = stream_response.choices.first() {
                                            if let Some(content) = &choice.delta.content {
                                                accumulator.add_chunk(content);
                                                let usage = accumulator.running_total();
                                                let mut stream_chunk = StreamChunk::new(content.clone());
                                                stream_chunk.usage = Some(usage);
                                                yield Ok(stream_chunk);
                                            }

                                            // Check for finish reason
                                            if let Some(reason) = &choice.finish_reason {
                                                let finish_reason = match reason.as_str() {
                                                    "stop" => crate::types::FinishReason::Stop,
                                                    "length" => crate::types::FinishReason::Length,
                                                    _ => crate::types::FinishReason::Stop,
                                                };
                                                let final_usage = accumulator.finalize();
                                                yield Ok(StreamChunk::final_chunk(finish_reason, Some(final_usage)));
                                                break 'stream_loop;
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        accumulator.mark_interrupted();
                                        yield Err(NxuskitError::Stream(format!("Failed to parse stream chunk: {}", e)));
                                        break 'stream_loop;
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        accumulator.mark_interrupted();
                        yield Err(NxuskitError::Stream(e.to_string()));
                        break 'stream_loop;
                    }
                }
            }
        };

        Ok(Box::new(Box::pin(output_stream)))
    }

    fn provider_name(&self) -> &str {
        "fireworks"
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let response = self
            .client
            .get(format!("{}/models", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(NxuskitError::provider(status.as_u16(), error_text));
        }

        let models_response: FireworksModelsResponse = response.json().await?;

        Ok(models_response
            .data
            .into_iter()
            .map(|model| {
                let mut info = ModelInfo::new(model.id.clone());
                info.metadata
                    .insert("created".to_string(), model.created.to_string());

                // Enrich with descriptions and metadata for known models
                enrich_fireworks_model(&mut info);

                info
            })
            .collect())
    }

    fn get_capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_system_messages: true,
            supports_streaming: true,
            supports_vision: true, // Some Fireworks models support vision
            max_stop_sequences: Some(4),
            supports_presence_penalty: true,
            supports_frequency_penalty: true,
            supports_seed: true,
            supports_logprobs: false,

            // T057: Fireworks AI uses an OAI-compatible API but no recorded
            // fixture proves streaming logprob support in the v0.9.4 Sprint 1
            // window. Treated as false per reconciled planning decision until a
            // fixture is captured and committed. Wire decode_oai_logprob_delta
            // from openai.rs and flip this flag when evidence exists.
            supports_streaming_logprobs: false,
            supports_json_mode: true,
            supports_json_schema: false,
            penalty_range: Some((-2.0, 2.0)),
            max_logprobs: None,
        }
    }
}

// Fireworks API request/response types

#[derive(Debug, Serialize)]
struct FireworksRequest {
    model: String,
    messages: Vec<FireworksMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    presence_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    frequency_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<FireworksResponseFormat>,
}

#[derive(Debug, Serialize)]
struct FireworksResponseFormat {
    r#type: String,
}

#[derive(Debug, Serialize)]
struct FireworksMessage {
    role: String,
    content: FireworksMessageContent,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum FireworksMessageContent {
    Text(String),
    Parts(Vec<FireworksContentPart>),
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum FireworksContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: FireworksImageUrl },
}

#[derive(Debug, Serialize)]
struct FireworksImageUrl {
    url: String,
}

#[derive(Debug, Deserialize)]
struct FireworksResponse {
    #[allow(dead_code)]
    id: String,
    model: String,
    choices: Vec<FireworksChoice>,
    usage: FireworksUsage,
}

#[derive(Debug, Deserialize)]
struct FireworksChoice {
    message: FireworksResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FireworksResponseMessage {
    #[allow(dead_code)]
    role: String,
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FireworksUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    #[allow(dead_code)]
    total_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct FireworksStreamResponse {
    choices: Vec<FireworksStreamChoice>,
}

#[derive(Debug, Deserialize)]
struct FireworksStreamChoice {
    delta: FireworksDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FireworksDelta {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FireworksModelsResponse {
    data: Vec<FireworksModel>,
}

#[derive(Debug, Deserialize)]
struct FireworksModel {
    id: String,
    created: u64,
}

// TDD TESTS - Written FIRST before implementation
#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fireworks_capabilities() {
        let provider = FireworksProvider::new("test-key");
        let caps = provider.get_capabilities();

        assert!(caps.supports_system_messages);
        assert!(caps.supports_streaming);
        assert!(caps.supports_json_mode);
        assert_eq!(caps.max_stop_sequences, Some(4));
        assert!(caps.supports_seed);
    }

    // Pro tests
    mod pro_tests {
        use super::*;
        use crate::Message;
        use crate::types::{ContentPart, ImageData, MessageContent};

        #[tokio::test]
        async fn test_fireworks_new_with_default_base_url() {
            let provider = FireworksProvider::new("test-key");
            assert_eq!(provider.base_url, "https://api.fireworks.ai/inference/v1");
            assert_eq!(provider.api_key, "test-key");
        }

        #[tokio::test]
        async fn test_fireworks_with_custom_base_url() {
            let provider =
                FireworksProvider::new("test-key").with_base_url("https://custom.fireworks.ai/v1");
            assert_eq!(provider.base_url, "https://custom.fireworks.ai/v1");
        }

        #[tokio::test]
        async fn test_message_conversion_text_only() {
            let provider = FireworksProvider::new("test-key");

            let messages = vec![
                Message::system("You are helpful"),
                Message::user("Hello"),
                Message::assistant("Hi there!"),
            ];

            let converted = provider.convert_messages(&messages);
            assert_eq!(converted.len(), 3);
            assert_eq!(converted[0].role, "system");
            assert_eq!(converted[1].role, "user");
            assert_eq!(converted[2].role, "assistant");

            match &converted[0].content {
                FireworksMessageContent::Text(text) => assert_eq!(text, "You are helpful"),
                _ => panic!("Expected text content"),
            }
        }

        #[tokio::test]
        async fn test_message_conversion_with_vision() {
            let provider = FireworksProvider::new("test-key");

            let mut user_msg = Message::user("");
            user_msg.content = MessageContent::Parts(vec![
                ContentPart::Text {
                    text: "What's in this image?".to_string(),
                },
                ContentPart::Image {
                    source: crate::types::ImageSource {
                        data: ImageData::Url {
                            url: "https://example.com/image.jpg".to_string(),
                        },
                        detail: None,
                    },
                },
            ]);

            let messages = vec![user_msg];
            let converted = provider.convert_messages(&messages);

            match &converted[0].content {
                FireworksMessageContent::Parts(parts) => {
                    assert_eq!(parts.len(), 2);
                    match &parts[0] {
                        FireworksContentPart::Text { text } => {
                            assert_eq!(text, "What's in this image?");
                        }
                        _ => panic!("Expected text part"),
                    }
                    match &parts[1] {
                        FireworksContentPart::ImageUrl { image_url } => {
                            assert_eq!(image_url.url, "https://example.com/image.jpg");
                        }
                        _ => panic!("Expected image_url part"),
                    }
                }
                _ => panic!("Expected parts content"),
            }
        }

        #[tokio::test]
        async fn test_build_request_basic() {
            let provider = FireworksProvider::new("test-key");

            let request = ChatRequest::new("accounts/fireworks/models/llama-v3-70b-instruct")
                .with_message(Message::user("Hello"))
                .with_temperature(0.7)
                .with_max_tokens(100);

            let fireworks_request = provider.build_request(&request).unwrap();

            assert_eq!(
                fireworks_request.model,
                "accounts/fireworks/models/llama-v3-70b-instruct"
            );
            assert_eq!(fireworks_request.temperature, Some(0.7));
            assert_eq!(fireworks_request.max_tokens, Some(100));
            assert_eq!(fireworks_request.stream, Some(false));
        }

        #[tokio::test]
        async fn test_build_request_with_json_mode() {
            let provider = FireworksProvider::new("test-key");

            let mut request = ChatRequest::new("accounts/fireworks/models/llama-v3-70b-instruct")
                .with_message(Message::user("Hello"));
            request.response_format = Some(crate::types::ResponseFormat::Json);

            let fireworks_request = provider.build_request(&request).unwrap();

            assert!(fireworks_request.response_format.is_some());
            assert_eq!(
                fireworks_request.response_format.unwrap().r#type,
                "json_object"
            );
        }

        #[tokio::test]
        async fn test_build_request_with_all_parameters() {
            let provider = FireworksProvider::new("test-key");

            let mut request = ChatRequest::new("test-model")
                .with_message(Message::user("Hello"))
                .with_temperature(0.8)
                .with_max_tokens(500)
                .with_top_p(0.9);

            request.stop = Some(vec!["STOP".to_string()]);
            request.presence_penalty = Some(0.5);
            request.frequency_penalty = Some(0.3);
            request.seed = Some(42);

            let fireworks_request = provider.build_request(&request).unwrap();

            assert_eq!(fireworks_request.temperature, Some(0.8));
            assert_eq!(fireworks_request.max_tokens, Some(500));
            assert_eq!(fireworks_request.top_p, Some(0.9));
            assert_eq!(fireworks_request.stop, Some(vec!["STOP".to_string()]));
            assert_eq!(fireworks_request.presence_penalty, Some(0.5));
            assert_eq!(fireworks_request.frequency_penalty, Some(0.3));
            assert_eq!(fireworks_request.seed, Some(42));
        }

        #[tokio::test]
        async fn test_provider_name() {
            let provider = FireworksProvider::new("test-key");
            assert_eq!(provider.provider_name(), "fireworks");
        }

        // ===== TDD Tests for Builder Pattern =====

        #[tokio::test]
        async fn test_builder_pattern_basic() {
            let provider = FireworksProvider::builder()
                .api_key("test-key")
                .build()
                .expect("Failed to build provider");

            assert_eq!(provider.api_key, "test-key");
            assert_eq!(provider.base_url, "https://api.fireworks.ai/inference/v1");
        }

        #[tokio::test]
        async fn test_builder_requires_api_key() {
            let result = FireworksProvider::builder().build();
            assert!(result.is_err());

            if let Err(e) = result {
                match e {
                    NxuskitError::Configuration(msg) => {
                        assert!(msg.contains("API key"));
                    }
                    _ => panic!("Expected Configuration error"),
                }
            }
        }

        #[tokio::test]
        async fn test_builder_with_custom_base_url() {
            let provider = FireworksProvider::builder()
                .api_key("test-key")
                .base_url("https://custom.fireworks.ai/v1")
                .build()
                .expect("Failed to build provider");

            assert_eq!(provider.base_url, "https://custom.fireworks.ai/v1");
        }

        #[tokio::test]
        async fn test_builder_with_default_model() {
            let provider = FireworksProvider::builder()
                .api_key("test-key")
                .model("accounts/fireworks/models/llama-v3-70b-instruct")
                .build()
                .expect("Failed to build provider");

            assert_eq!(
                provider.default_model,
                "accounts/fireworks/models/llama-v3-70b-instruct"
            );
        }

        #[tokio::test]
        async fn test_builder_with_timeout() {
            use std::time::Duration;

            let provider = FireworksProvider::builder()
                .api_key("test-key")
                .timeout(Duration::from_secs(30))
                .build()
                .expect("Failed to build provider");

            // All timeouts should use the general timeout
            assert_eq!(provider.connection_timeout, Duration::from_secs(30));
            assert_eq!(provider.stream_read_timeout, Duration::from_secs(30));
            assert_eq!(provider.total_timeout, Duration::from_secs(30));
        }

        #[tokio::test]
        async fn test_builder_with_specific_timeouts() {
            use std::time::Duration;

            let provider = FireworksProvider::builder()
                .api_key("test-key")
                .connection_timeout(Duration::from_secs(10))
                .stream_read_timeout(Duration::from_secs(120))
                .total_timeout(Duration::from_secs(60))
                .build()
                .expect("Failed to build provider");

            assert_eq!(provider.connection_timeout, Duration::from_secs(10));
            assert_eq!(provider.stream_read_timeout, Duration::from_secs(120));
            assert_eq!(provider.total_timeout, Duration::from_secs(60));
        }

        #[tokio::test]
        async fn test_builder_timeout_fallback_chain() {
            use std::time::Duration;

            // Specific timeout should override general timeout
            let provider = FireworksProvider::builder()
                .api_key("test-key")
                .timeout(Duration::from_secs(30))
                .connection_timeout(Duration::from_secs(15))
                .build()
                .expect("Failed to build provider");

            assert_eq!(provider.connection_timeout, Duration::from_secs(15)); // Specific wins
            assert_eq!(provider.stream_read_timeout, Duration::from_secs(30)); // Falls back to general
            assert_eq!(provider.total_timeout, Duration::from_secs(30)); // Falls back to general
        }

        #[tokio::test]
        async fn test_builder_default_timeouts() {
            use std::time::Duration;

            let provider = FireworksProvider::builder()
                .api_key("test-key")
                .build()
                .expect("Failed to build provider");

            // Should use default timeouts (60s for connection/total, 120s for stream)
            assert_eq!(provider.connection_timeout, Duration::from_secs(60));
            assert_eq!(provider.stream_read_timeout, Duration::from_secs(120));
            assert_eq!(provider.total_timeout, Duration::from_secs(60));
        }
    }
}
