//! Together AI provider implementation

use async_stream::stream;
use async_trait::async_trait;
use futures::Stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};

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

/// Together AI provider
///
/// Provides access to open source language models via Together AI's OpenAI-compatible API.
/// Supports chat completions, streaming, and various open source models.
#[derive(Debug, Clone)]
pub struct TogetherProvider {
    #[allow(dead_code)]
    client: Client,
    #[allow(dead_code)]
    api_key: String,
    #[allow(dead_code)]
    base_url: String,
    #[allow(dead_code)]
    default_model: String,
    #[allow(dead_code)]
    connection_timeout: std::time::Duration,
    #[allow(dead_code)]
    stream_read_timeout: std::time::Duration,
    #[allow(dead_code)]
    total_timeout: std::time::Duration,
}

impl TogetherProvider {
    /// Create a new Together provider with the given API key
    ///
    /// # Deprecated
    /// Use `TogetherProvider::builder()` instead for more configuration options
    pub fn new(api_key: impl Into<String>) -> Self {
        use std::time::Duration;

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
            base_url: "https://api.together.xyz/v1".to_string(),
            default_model: "mistralai/Mixtral-8x7B-Instruct-v0.1".to_string(),
            connection_timeout,
            stream_read_timeout,
            total_timeout,
        }
    }

    /// Set a custom base URL for the Together API
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Create a new builder for TogetherProvider
    pub fn builder() -> TogetherProviderBuilder {
        TogetherProviderBuilder::default()
    }

    /// Get the configured default model
    pub fn default_model(&self) -> &str {
        &self.default_model
    }

    /// Get the configured connection timeout
    pub fn connection_timeout(&self) -> std::time::Duration {
        self.connection_timeout
    }

    /// Get the configured stream read timeout
    pub fn stream_read_timeout(&self) -> std::time::Duration {
        self.stream_read_timeout
    }

    /// Get the configured total timeout
    pub fn total_timeout(&self) -> std::time::Duration {
        self.total_timeout
    }

    /// Convert nxusKit messages to Together API format
    fn convert_messages(&self, messages: &[Message]) -> Vec<TogetherMessage> {
        messages
            .iter()
            .map(|msg| {
                let role = match msg.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                };

                let content = match &msg.content {
                    MessageContent::Text(text) => TogetherMessageContent::Text(text.clone()),
                    MessageContent::Parts(parts) => {
                        // Together supports vision in some models
                        let converted_parts: Vec<TogetherContentPart> = parts
                            .iter()
                            .map(|part| match part {
                                ContentPart::Text { text } => {
                                    TogetherContentPart::Text { text: text.clone() }
                                }
                                ContentPart::Image { source } => {
                                    let url = match &source.data {
                                        ImageData::Url { url } => url.clone(),
                                        ImageData::Base64 { media_type, data } => {
                                            format!("data:{};base64,{}", media_type, data)
                                        }
                                    };
                                    TogetherContentPart::ImageUrl {
                                        image_url: TogetherImageUrl { url },
                                    }
                                }
                            })
                            .collect();

                        TogetherMessageContent::Parts(converted_parts)
                    }
                };

                TogetherMessage {
                    role: role.to_string(),
                    content,
                }
            })
            .collect()
    }

    /// Build Together API request from adapted ChatRequest
    fn build_request(&self, request: &ChatRequest) -> Result<TogetherRequest> {
        let record = crate::capabilities::registry::find("together").ok_or_else(|| {
            NxuskitError::Configuration("provider capability record not found: together".into())
        })?;
        let mut validation_warnings = Vec::new();
        super::validate_typed_capability_request("together", request, &mut validation_warnings)?;
        let messages = self.convert_messages(&request.messages);

        let response_format_typed = request
            .structured_output
            .as_ref()
            .map(crate::capabilities::openai_wire::response_format)
            .unwrap_or(serde_json::Value::Null);
        let (tools, tool_choice, parallel_tool_calls) = match request.tool_call_config.as_ref() {
            Some(cfg) => (
                crate::capabilities::openai_wire::tools_for(&record, cfg),
                crate::capabilities::openai_wire::tool_choice_for(&record, cfg),
                cfg.parallel_tool_calls,
            ),
            None => (serde_json::Value::Null, serde_json::Value::Null, None),
        };

        let mut together_request = TogetherRequest {
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
            response_format_typed,
            tools,
            tool_choice,
            parallel_tool_calls,
        };

        // Legacy json_object passthrough only when the typed path is not set.
        if request.structured_output.is_none()
            && let Some(ref format) = request.response_format
        {
            use crate::types::ResponseFormat;
            match format {
                ResponseFormat::Json => {
                    together_request.response_format = Some(TogetherResponseFormat {
                        r#type: "json_object".to_string(),
                    });
                }
                ResponseFormat::JsonSchema { .. } | ResponseFormat::Text => {}
            }
        }

        Ok(together_request)
    }
}

impl TogetherProvider {
    /// Create a fresh session with no accumulated state.
    ///
    /// For TogetherProvider, this returns a clone with the same configuration
    /// since it's already stateless (each request is independent).
    ///
    /// # Returns
    ///
    /// A cloned TogetherProvider instance with the same configuration.
    pub fn fresh_session(&self) -> Self {
        self.clone()
    }
}

/// Builder for TogetherProvider
#[derive(Debug, Default)]
pub struct TogetherProviderBuilder {
    #[allow(dead_code)]
    api_key: Option<String>,
    #[allow(dead_code)]
    base_url: Option<String>,
    #[allow(dead_code)]
    model: Option<String>,
    #[allow(dead_code)]
    timeout: Option<std::time::Duration>,
    #[allow(dead_code)]
    connection_timeout: Option<std::time::Duration>,
    #[allow(dead_code)]
    stream_read_timeout: Option<std::time::Duration>,
    #[allow(dead_code)]
    total_timeout: Option<std::time::Duration>,
}

impl TogetherProviderBuilder {
    /// Set the API key for Together AI
    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// Set a custom base URL for the Together API
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
    pub fn timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Set the connection timeout
    /// Falls back to general timeout if not set
    pub fn connection_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.connection_timeout = Some(timeout);
        self
    }

    /// Set the stream read timeout
    /// Falls back to general timeout if not set
    pub fn stream_read_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.stream_read_timeout = Some(timeout);
        self
    }

    /// Set the total request timeout
    /// Falls back to general timeout if not set
    pub fn total_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.total_timeout = Some(timeout);
        self
    }

    /// Build the TogetherProvider
    pub fn build(self) -> Result<TogetherProvider> {
        use std::time::Duration;

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

        Ok(TogetherProvider {
            client,
            api_key,
            base_url: self
                .base_url
                .unwrap_or_else(|| "https://api.together.xyz/v1".to_string()),
            default_model: self
                .model
                .unwrap_or_else(|| "mistralai/Mixtral-8x7B-Instruct-v0.1".to_string()),
            connection_timeout,
            stream_read_timeout,
            total_timeout,
        })
    }
}

/// Enrich model info with descriptions for known models
fn enrich_together_model(info: &mut ModelInfo) {
    let model_id = &info.name;

    // Match by model family patterns
    let (description, context_window) = if model_id.contains("llama") || model_id.contains("Llama")
    {
        if model_id.contains("3.3") || model_id.contains("3-3") {
            (
                Some(
                    "Llama 3.3: Latest Meta model optimized for multilingual tasks and instruction following",
                ),
                Some(128_000),
            )
        } else if model_id.contains("3.1") || model_id.contains("3-1") {
            if model_id.contains("405") {
                (
                    Some("Llama 3.1 405B: Massive flagship model for most complex reasoning tasks"),
                    Some(128_000),
                )
            } else if model_id.contains("70") {
                (
                    Some(
                        "Llama 3.1 70B: High-performance model for chat, translation, and summarization",
                    ),
                    Some(128_000),
                )
            } else {
                (
                    Some(
                        "Llama 3.1: Meta's advanced model with extended context and improved capabilities",
                    ),
                    Some(128_000),
                )
            }
        } else if model_id.contains("vision") || model_id.contains("Vision") {
            (
                Some("Llama Vision: Multimodal model for image understanding and vision tasks"),
                Some(128_000),
            )
        } else {
            (
                Some("Llama: Meta's open-source language model"),
                Some(8_192),
            )
        }
    } else if model_id.contains("mixtral") || model_id.contains("Mixtral") {
        if model_id.contains("8x22") {
            (
                Some("Mixtral 8x22B: Large MoE model with 176B total params, 44B active"),
                Some(64_000),
            )
        } else if model_id.contains("8x7") {
            (
                Some(
                    "Mixtral 8x7B: Efficient sparse MoE with 6x faster inference, excellent for multilingual",
                ),
                Some(32_768),
            )
        } else {
            (
                Some("Mixtral: Mistral's Mixture of Experts model for efficient inference"),
                Some(32_768),
            )
        }
    } else if model_id.contains("qwen") || model_id.contains("Qwen") {
        if model_id.contains("2.5") || model_id.contains("2-5") {
            if model_id.contains("72") {
                (
                    Some(
                        "Qwen 2.5 72B: Top open-source model for structured data and 30+ languages",
                    ),
                    Some(128_000),
                )
            } else {
                (
                    Some("Qwen 2.5: Advanced model for multilingual and structured data tasks"),
                    Some(128_000),
                )
            }
        } else if model_id.contains("3") {
            (
                Some("Qwen 3: Latest generation with controllable chain-of-thought reasoning"),
                Some(128_000),
            )
        } else {
            (
                Some("Qwen: Alibaba's multilingual large language model"),
                Some(32_768),
            )
        }
    } else if model_id.contains("deepseek") || model_id.contains("DeepSeek") {
        if model_id.contains("r1") || model_id.contains("R1") {
            (
                Some("DeepSeek R1: Advanced reasoning model with chain-of-thought capabilities"),
                Some(128_000),
            )
        } else if model_id.contains("coder") {
            (
                Some("DeepSeek Coder: Specialized model for code generation and understanding"),
                Some(64_000),
            )
        } else {
            (
                Some("DeepSeek: Advanced reasoning and coding model"),
                Some(64_000),
            )
        }
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

#[async_trait]
impl LLMProvider for TogetherProvider {
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse> {
        // Adapt request to provider capabilities
        let capabilities = self.get_capabilities();
        let mut adapted = ParameterAdapter::adapt(request, &capabilities);
        super::validate_typed_capability_request(
            "together",
            &adapted.request,
            &mut adapted.warnings,
        )?;
        let adapted_request = &adapted.request;

        // Build Together-specific request
        let together_request = self.build_request(adapted_request)?;

        // Make API call
        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&together_request)
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

        let together_response: TogetherResponse = response.json().await?;

        // Extract content from first choice
        let content = together_response
            .choices
            .first()
            .and_then(|choice| choice.message.content.as_deref())
            .unwrap_or("")
            .to_string();

        // Convert finish reason
        let finish_reason = together_response
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
            together_response.model,
            TokenUsage::estimated_only(TokenCount::new(
                together_response.usage.prompt_tokens,
                together_response.usage.completion_tokens,
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
                    "provider": "together"
                }));

        Ok(response)
    }

    async fn chat_stream(
        &self,
        request: &ChatRequest,
    ) -> Result<Box<dyn Stream<Item = Result<StreamChunk>> + Send + Unpin>> {
        // Adapt request to provider capabilities
        let capabilities = self.get_capabilities();
        let mut adapted = ParameterAdapter::adapt(request, &capabilities);
        super::validate_typed_capability_request(
            "together",
            &adapted.request,
            &mut adapted.warnings,
        )?;
        let adapted_request = &adapted.request;

        // Build Together-specific request with streaming enabled
        let mut together_request = self.build_request(adapted_request)?;
        together_request.stream = Some(true);

        // Make API call
        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&together_request)
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
                                match serde_json::from_str::<TogetherStreamResponse>(data) {
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
        "together"
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

        let models_response: TogetherModelsResponse = response.json().await?;

        Ok(models_response
            .data
            .into_iter()
            .map(|model| {
                let mut info = ModelInfo::new(model.id.clone());
                info.metadata
                    .insert("created".to_string(), model.created.to_string());

                // Enrich with descriptions and metadata for known models
                enrich_together_model(&mut info);

                info
            })
            .collect())
    }

    fn get_capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_system_messages: true,
            supports_streaming: true,
            supports_vision: true, // Some Together models support vision
            max_stop_sequences: Some(4),
            supports_presence_penalty: true,
            supports_frequency_penalty: true,
            supports_seed: true,
            supports_logprobs: false,

            // T053: Together uses an OAI-compatible SSE format but no recorded
            // fixture or live verification proves streaming logprob support in
            // the v0.9.4 Sprint 1 window. Kept false per reconciled planning
            // decision. Flip to true (and wire decode_oai_logprob_delta from
            // openai.rs) when T067 live test confirms support and a fixture is
            // committed to internal/tests/parity/stream_logprobs/fixtures/.
            supports_streaming_logprobs: false,
            supports_json_mode: true,
            supports_json_schema: false,
            penalty_range: Some((-2.0, 2.0)),
            max_logprobs: None,
        }
    }
}

#[derive(Debug, Serialize)]
struct TogetherRequest {
    model: String,
    messages: Vec<TogetherMessage>,
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
    response_format: Option<TogetherResponseFormat>,
    #[serde(rename = "response_format", skip_serializing_if = "is_json_null")]
    response_format_typed: serde_json::Value,
    #[serde(skip_serializing_if = "is_json_null")]
    tools: serde_json::Value,
    #[serde(skip_serializing_if = "is_json_null")]
    tool_choice: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    parallel_tool_calls: Option<bool>,
}

fn is_json_null(v: &serde_json::Value) -> bool {
    v.is_null()
}

#[derive(Debug, Serialize)]
struct TogetherResponseFormat {
    r#type: String,
}

#[derive(Debug, Serialize)]
struct TogetherMessage {
    role: String,
    content: TogetherMessageContent,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum TogetherMessageContent {
    Text(String),
    Parts(Vec<TogetherContentPart>),
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum TogetherContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: TogetherImageUrl },
}

#[derive(Debug, Serialize)]
struct TogetherImageUrl {
    url: String,
}

#[derive(Debug, Deserialize)]
struct TogetherResponse {
    #[allow(dead_code)]
    id: String,
    model: String,
    choices: Vec<TogetherChoice>,
    usage: TogetherUsage,
}

#[derive(Debug, Deserialize)]
struct TogetherChoice {
    message: TogetherResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TogetherResponseMessage {
    #[allow(dead_code)]
    role: String,
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TogetherUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    #[allow(dead_code)]
    total_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct TogetherStreamResponse {
    choices: Vec<TogetherStreamChoice>,
}

#[derive(Debug, Deserialize)]
struct TogetherStreamChoice {
    delta: TogetherDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TogetherDelta {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TogetherModelsResponse {
    data: Vec<TogetherModel>,
}

#[derive(Debug, Deserialize)]
struct TogetherModel {
    id: String,
    created: u64,
}

// TDD TESTS - These should be written FIRST and will initially fail
#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_together_new_with_default_base_url() {
        let provider = TogetherProvider::new("test-key");
        assert_eq!(provider.base_url, "https://api.together.xyz/v1");
        assert_eq!(provider.api_key, "test-key");
    }

    #[tokio::test]
    async fn test_together_with_custom_base_url() {
        let provider =
            TogetherProvider::new("test-key").with_base_url("https://custom.together.xyz/v1");
        assert_eq!(provider.base_url, "https://custom.together.xyz/v1");
    }

    #[tokio::test]
    async fn test_together_capabilities() {
        let provider = TogetherProvider::new("test-key");
        let caps = provider.get_capabilities();

        assert!(caps.supports_system_messages);
        assert!(caps.supports_streaming);
        assert!(caps.supports_vision);
        assert_eq!(caps.max_stop_sequences, Some(4));
        assert!(caps.supports_presence_penalty);
        assert!(caps.supports_frequency_penalty);
        assert!(caps.supports_seed);
        assert!(!caps.supports_logprobs);
        assert!(caps.supports_json_mode);
        assert!(!caps.supports_json_schema);
        assert_eq!(caps.penalty_range, Some((-2.0, 2.0)));
    }

    #[tokio::test]
    async fn test_provider_name() {
        let provider = TogetherProvider::new("test-key");
        assert_eq!(provider.provider_name(), "together");
    }

    // ===== TDD Tests for Builder Pattern (Red Phase - These will fail initially) =====

    #[tokio::test]
    async fn test_builder_pattern_basic() {
        let provider = TogetherProvider::builder()
            .api_key("test-key")
            .build()
            .expect("Failed to build provider");

        assert_eq!(provider.api_key, "test-key");
        assert_eq!(provider.base_url, "https://api.together.xyz/v1");
    }

    #[tokio::test]
    async fn test_builder_requires_api_key() {
        let result = TogetherProvider::builder().build();
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
        let provider = TogetherProvider::builder()
            .api_key("test-key")
            .base_url("https://custom.together.xyz/v1")
            .build()
            .expect("Failed to build provider");

        assert_eq!(provider.base_url, "https://custom.together.xyz/v1");
    }

    #[tokio::test]
    async fn test_builder_with_default_model() {
        let provider = TogetherProvider::builder()
            .api_key("test-key")
            .model("mistralai/Mixtral-8x7B-Instruct-v0.1")
            .build()
            .expect("Failed to build provider");

        assert_eq!(
            provider.default_model,
            "mistralai/Mixtral-8x7B-Instruct-v0.1"
        );
    }

    #[tokio::test]
    async fn test_builder_with_timeout() {
        use std::time::Duration;

        let provider = TogetherProvider::builder()
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

        let provider = TogetherProvider::builder()
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
        let provider = TogetherProvider::builder()
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

        let provider = TogetherProvider::builder()
            .api_key("test-key")
            .build()
            .expect("Failed to build provider");

        // Should use default timeouts (60s for connection/total, 120s for stream)
        assert_eq!(provider.connection_timeout, Duration::from_secs(60));
        assert_eq!(provider.stream_read_timeout, Duration::from_secs(120));
        assert_eq!(provider.total_timeout, Duration::from_secs(60));
    }

    mod pro_tests {
        use super::*;
        use crate::Message;

        #[tokio::test]
        async fn test_message_conversion_text_only() {
            let provider = TogetherProvider::new("test-key");

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
                TogetherMessageContent::Text(text) => assert_eq!(text, "You are helpful"),
                _ => panic!("Expected text content"),
            }
        }

        #[tokio::test]
        async fn test_message_conversion_with_vision() {
            use crate::types::{ImageData, MessageContent};

            let provider = TogetherProvider::new("test-key");

            let mut user_msg = Message::user("");
            user_msg.content = MessageContent::Parts(vec![
                crate::types::ContentPart::Text {
                    text: "What's in this image?".to_string(),
                },
                crate::types::ContentPart::Image {
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
                TogetherMessageContent::Parts(parts) => {
                    assert_eq!(parts.len(), 2);
                    match &parts[0] {
                        TogetherContentPart::Text { text } => {
                            assert_eq!(text, "What's in this image?");
                        }
                        _ => panic!("Expected text part"),
                    }
                    match &parts[1] {
                        TogetherContentPart::ImageUrl { image_url } => {
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
            let provider = TogetherProvider::new("test-key");

            let request = ChatRequest::new("mistralai/Mixtral-8x7B-Instruct-v0.1")
                .with_message(Message::user("Hello"))
                .with_temperature(0.7)
                .with_max_tokens(100);

            let together_request = provider.build_request(&request).unwrap();

            assert_eq!(
                together_request.model,
                "mistralai/Mixtral-8x7B-Instruct-v0.1"
            );
            assert_eq!(together_request.temperature, Some(0.7));
            assert_eq!(together_request.max_tokens, Some(100));
            assert_eq!(together_request.stream, Some(false));
        }

        #[tokio::test]
        async fn test_build_request_with_json_mode() {
            let provider = TogetherProvider::new("test-key");

            let mut request = ChatRequest::new("mistralai/Mixtral-8x7B-Instruct-v0.1")
                .with_message(Message::user("Hello"));
            request.response_format = Some(crate::types::ResponseFormat::Json);

            let together_request = provider.build_request(&request).unwrap();

            assert!(together_request.response_format.is_some());
            assert_eq!(
                together_request.response_format.unwrap().r#type,
                "json_object"
            );
        }

        #[tokio::test]
        async fn test_build_request_with_all_parameters() {
            let provider = TogetherProvider::new("test-key");

            let mut request = ChatRequest::new("test-model")
                .with_message(Message::user("Hello"))
                .with_temperature(0.8)
                .with_max_tokens(500)
                .with_top_p(0.9);

            request.stop = Some(vec!["STOP".to_string()]);
            request.presence_penalty = Some(0.5);
            request.frequency_penalty = Some(0.3);
            request.seed = Some(42);

            let together_request = provider.build_request(&request).unwrap();

            assert_eq!(together_request.temperature, Some(0.8));
            assert_eq!(together_request.max_tokens, Some(500));
            assert_eq!(together_request.top_p, Some(0.9));
            assert_eq!(together_request.stop, Some(vec!["STOP".to_string()]));
            assert_eq!(together_request.presence_penalty, Some(0.5));
            assert_eq!(together_request.frequency_penalty, Some(0.3));
            assert_eq!(together_request.seed, Some(42));
        }

        #[tokio::test]
        async fn test_build_request_serializes_typed_structured_output_json_schema() {
            use crate::capabilities::{StructuredOutputConfig, StructuredOutputMode};

            let provider = TogetherProvider::new("test-key");
            let mut request = ChatRequest::new("meta-llama/Llama-3.3-70B-Instruct-Turbo")
                .with_message(Message::user("Return JSON."));
            request.structured_output = Some(StructuredOutputConfig {
                mode: StructuredOutputMode::JsonSchema,
                schema: Some(serde_json::json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["country"],
                    "properties": {"country": {"type": "string"}}
                })),
                schema_name: Some("country_capital".into()),
                strict: Some(true),
                schema_subset: None,
            });

            let together_request = provider.build_request(&request).unwrap();
            let value = serde_json::to_value(&together_request).unwrap();
            assert_eq!(value["response_format"]["type"], "json_schema");
            assert_eq!(
                value["response_format"]["json_schema"]["name"],
                "country_capital"
            );
            assert_eq!(value["response_format"]["json_schema"]["strict"], true);
            assert_eq!(
                value["response_format"]["json_schema"]["schema"]["additionalProperties"],
                false
            );
        }

        #[tokio::test]
        async fn test_build_request_serializes_typed_tools_and_tool_choice() {
            use crate::capabilities::{ToolCallConfig, ToolChoice, ToolDefinition};

            let provider = TogetherProvider::new("test-key");
            let mut request = ChatRequest::new("meta-llama/Llama-3.3-70B-Instruct-Turbo")
                .with_message(Message::user("Call a tool."));
            request.tool_call_config = Some(ToolCallConfig {
                tools: vec![ToolDefinition {
                    name: "search_inventory".into(),
                    description: Some("Search inventory.".into()),
                    parameters: serde_json::json!({"type": "object"}),
                    strict: Some(true),
                }],
                tool_choice: ToolChoice::Required,
                parallel_tool_calls: Some(true),
                streaming_tool_calls: None,
            });

            let together_request = provider.build_request(&request).unwrap();
            let value = serde_json::to_value(&together_request).unwrap();
            assert_eq!(value["tools"][0]["type"], "function");
            assert_eq!(value["tools"][0]["function"]["name"], "search_inventory");
            assert_eq!(value["tool_choice"], "required");
            assert_eq!(value["parallel_tool_calls"], true);
        }

        #[tokio::test]
        async fn test_build_request_omits_typed_fields_when_unset() {
            let provider = TogetherProvider::new("test-key");
            let request = ChatRequest::new("meta-llama/Llama-3.3-70B-Instruct-Turbo")
                .with_message(Message::user("Plain chat."));

            let together_request = provider.build_request(&request).unwrap();
            let value = serde_json::to_value(&together_request).unwrap();
            assert!(value.get("response_format").is_none());
            assert!(value.get("tools").is_none());
            assert!(value.get("tool_choice").is_none());
            assert!(value.get("parallel_tool_calls").is_none());
        }
    }
}
