//! Groq provider implementation

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
    types::{ContentPart, FinishReason, InferenceMetadata, MessageContent, ProviderCapabilities},
};

/// Groq provider
///
/// Provides ultra-fast access to open source language models via Groq's OpenAI-compatible API.
/// Supports chat completions, streaming, and optimized inference.
#[derive(Debug, Clone)]
pub struct GroqProvider {
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

impl GroqProvider {
    /// Create a new Groq provider with the given API key
    ///
    /// # Deprecated
    /// Use `GroqProvider::builder()` instead for more configuration options
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
            base_url: "https://api.groq.com/openai/v1".to_string(),
            default_model: "llama3-70b-8192".to_string(),
            connection_timeout,
            stream_read_timeout,
            total_timeout,
        }
    }

    /// Set a custom base URL for the Groq API
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Create a new builder for GroqProvider
    pub fn builder() -> GroqProviderBuilder {
        GroqProviderBuilder::default()
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

    /// Convert nxusKit messages to Groq API format
    fn convert_messages(&self, messages: &[Message]) -> Vec<GroqMessage> {
        messages
            .iter()
            .map(|msg| {
                let role = match msg.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                };

                let content = match &msg.content {
                    MessageContent::Text(text) => GroqMessageContent::Text(text.clone()),
                    MessageContent::Parts(parts) => {
                        // Groq doesn't support vision - extract only text parts
                        let text = parts
                            .iter()
                            .filter_map(|part| match part {
                                ContentPart::Text { text } => Some(text.as_str()),
                                ContentPart::Image { .. } => None,
                            })
                            .collect::<Vec<_>>()
                            .join("\n");
                        GroqMessageContent::Text(text)
                    }
                };

                GroqMessage {
                    role: role.to_string(),
                    content,
                }
            })
            .collect()
    }

    /// Build Groq API request from adapted ChatRequest
    fn build_request(&self, request: &ChatRequest) -> Result<GroqRequest> {
        let record = crate::capabilities::registry::find("groq").ok_or_else(|| {
            NxuskitError::Configuration("provider capability record not found: groq".into())
        })?;
        let mut validation_warnings = Vec::new();
        super::validate_typed_capability_request("groq", request, &mut validation_warnings)?;
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

        let mut groq_request = GroqRequest {
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
                    groq_request.response_format = Some(GroqResponseFormat {
                        r#type: "json_object".to_string(),
                    });
                }
                ResponseFormat::JsonSchema { .. } | ResponseFormat::Text => {}
            }
        }

        Ok(groq_request)
    }
}

impl GroqProvider {
    /// Create a fresh session with no accumulated state.
    ///
    /// For GroqProvider, this returns a clone with the same configuration
    /// since it's already stateless (each request is independent).
    ///
    /// # Returns
    ///
    /// A cloned GroqProvider instance with the same configuration.
    pub fn fresh_session(&self) -> Self {
        self.clone()
    }
}

/// Builder for GroqProvider
#[derive(Debug, Default)]
pub struct GroqProviderBuilder {
    api_key: Option<String>,
    base_url: Option<String>,
    model: Option<String>,
    timeout: Option<Duration>,
    connection_timeout: Option<Duration>,
    stream_read_timeout: Option<Duration>,
    total_timeout: Option<Duration>,
}

impl GroqProviderBuilder {
    /// Set the API key for Groq
    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// Set a custom base URL for the Groq API
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

    /// Build the GroqProvider
    pub fn build(self) -> Result<GroqProvider> {
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

        Ok(GroqProvider {
            client,
            api_key,
            base_url: self
                .base_url
                .unwrap_or_else(|| "https://api.groq.com/openai/v1".to_string()),
            default_model: self.model.unwrap_or_else(|| "llama3-70b-8192".to_string()),
            connection_timeout,
            stream_read_timeout,
            total_timeout,
        })
    }
}

/// Enrich model info with descriptions for known models
fn enrich_groq_model(info: &mut ModelInfo) {
    let model_id = &info.name;

    // Match by model family patterns
    let (description, context_window) = if model_id.contains("llama") {
        if model_id.contains("3.3") || model_id.contains("3-3") {
            (
                Some(
                    "Llama 3.3 70B: Latest Meta model with exceptional performance for versatile tasks",
                ),
                Some(128_000),
            )
        } else if model_id.contains("3.1") || model_id.contains("3-1") {
            if model_id.contains("8b") {
                (
                    Some(
                        "Llama 3.1 8B: Fast, efficient model with optimal price-performance at Groq speed",
                    ),
                    Some(128_000),
                )
            } else if model_id.contains("70") {
                (
                    Some(
                        "Llama 3.1 70B: High-capability model for complex reasoning and multilingual tasks",
                    ),
                    Some(128_000),
                )
            } else {
                (
                    Some(
                        "Llama 3.1: Meta's advanced model optimized for Groq's lightning-fast inference",
                    ),
                    Some(128_000),
                )
            }
        } else if model_id.contains("guard") {
            (
                Some("Llama Guard: Safety model for content moderation and policy compliance"),
                Some(8_192),
            )
        } else {
            (
                Some("Llama: Meta's open-source model running at Groq speed (1200+ tokens/sec)"),
                Some(8_192),
            )
        }
    } else if model_id.contains("gemma") {
        if model_id.contains("2") {
            (
                Some(
                    "Gemma 2 9B: Google's efficient model (being phased out in favor of Llama 3.1 8B)",
                ),
                Some(8_192),
            )
        } else {
            (
                Some("Gemma: Google's lightweight open model optimized for Groq infrastructure"),
                Some(8_192),
            )
        }
    } else if model_id.contains("mixtral") {
        (
            Some(
                "Mixtral 8x7B: Mistral's MoE model with superior multilingual capabilities (deprecated)",
            ),
            Some(32_768),
        )
    } else if model_id.contains("deepseek") {
        (
            Some("DeepSeek: Advanced reasoning model with chain-of-thought capabilities"),
            Some(128_000),
        )
    } else if model_id.contains("qwen") {
        (
            Some("Qwen: Alibaba's multilingual model for structured data and diverse languages"),
            Some(128_000),
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

#[async_trait]
impl LLMProvider for GroqProvider {
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse> {
        // Adapt request to provider capabilities
        let capabilities = self.get_capabilities();
        let mut adapted = ParameterAdapter::adapt(request, &capabilities);
        super::validate_typed_capability_request("groq", &adapted.request, &mut adapted.warnings)?;
        let adapted_request = &adapted.request;

        // Build Groq-specific request
        let groq_request = self.build_request(adapted_request)?;

        // Make API call
        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&groq_request)
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

        let groq_response: GroqResponse = response.json().await?;

        // Extract content from first choice
        let content = groq_response
            .choices
            .first()
            .and_then(|choice| choice.message.content.as_deref())
            .unwrap_or("")
            .to_string();

        // Convert finish reason
        let finish_reason = groq_response
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
            groq_response.model,
            TokenUsage::estimated_only(TokenCount::new(
                groq_response.usage.prompt_tokens,
                groq_response.usage.completion_tokens,
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
                    "provider": "groq"
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
        super::validate_typed_capability_request("groq", &adapted.request, &mut adapted.warnings)?;
        let adapted_request = &adapted.request;

        // Build Groq-specific request with streaming enabled
        let mut groq_request = self.build_request(adapted_request)?;
        groq_request.stream = Some(true);

        // Make API call
        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&groq_request)
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
                                match serde_json::from_str::<GroqStreamResponse>(data) {
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
        "groq"
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

        let models_response: GroqModelsResponse = response.json().await?;

        Ok(models_response
            .data
            .into_iter()
            .map(|model| {
                let mut info = ModelInfo::new(model.id.clone());
                info.metadata
                    .insert("created".to_string(), model.created.to_string());

                // Enrich with descriptions and metadata for known models
                enrich_groq_model(&mut info);

                info
            })
            .collect())
    }

    fn get_capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_system_messages: true,
            supports_streaming: true,
            supports_vision: false, // Groq doesn't support vision
            max_stop_sequences: Some(4),
            supports_presence_penalty: true,
            supports_frequency_penalty: true,
            supports_seed: true,
            supports_logprobs: false,

            // T058: Groq does not expose a streaming logprob field.
            // Non-supporting; stream chunks use StreamChunk::new() which
            // defaults logprobs to None — no phantom data is possible.
            supports_streaming_logprobs: false,
            supports_json_mode: true,
            supports_json_schema: false,
            penalty_range: Some((-2.0, 2.0)),
            max_logprobs: None,
        }
    }
}

#[derive(Debug, Serialize)]
struct GroqRequest {
    model: String,
    messages: Vec<GroqMessage>,
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
    response_format: Option<GroqResponseFormat>,
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
struct GroqResponseFormat {
    r#type: String,
}

#[derive(Debug, Serialize)]
struct GroqMessage {
    role: String,
    content: GroqMessageContent,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum GroqMessageContent {
    Text(String),
}

#[derive(Debug, Deserialize)]
struct GroqResponse {
    #[allow(dead_code)]
    id: String,
    model: String,
    choices: Vec<GroqChoice>,
    usage: GroqUsage,
}

#[derive(Debug, Deserialize)]
struct GroqChoice {
    message: GroqResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GroqResponseMessage {
    #[allow(dead_code)]
    role: String,
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GroqUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    #[allow(dead_code)]
    total_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct GroqStreamResponse {
    choices: Vec<GroqStreamChoice>,
}

#[derive(Debug, Deserialize)]
struct GroqStreamChoice {
    delta: GroqDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GroqDelta {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GroqModelsResponse {
    data: Vec<GroqModel>,
}

#[derive(Debug, Deserialize)]
struct GroqModel {
    id: String,
    created: u64,
}

// TDD TESTS - Written FIRST before implementation
#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_groq_new_with_default_base_url() {
        let provider = GroqProvider::new("test-key");
        assert_eq!(provider.base_url, "https://api.groq.com/openai/v1");
        assert_eq!(provider.api_key, "test-key");
    }

    #[tokio::test]
    async fn test_groq_with_custom_base_url() {
        let provider = GroqProvider::new("test-key").with_base_url("https://custom.groq.com/v1");
        assert_eq!(provider.base_url, "https://custom.groq.com/v1");
    }

    #[tokio::test]
    async fn test_groq_capabilities() {
        let provider = GroqProvider::new("test-key");
        let caps = provider.get_capabilities();

        assert!(caps.supports_system_messages);
        assert!(caps.supports_streaming);
        assert!(!caps.supports_vision); // Groq doesn't support vision
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
        let provider = GroqProvider::new("test-key");
        assert_eq!(provider.provider_name(), "groq");
    }

    // ===== TDD Tests for Builder Pattern (Red Phase - These will fail initially) =====

    #[tokio::test]
    async fn test_builder_pattern_basic() {
        let provider = GroqProvider::builder()
            .api_key("test-key")
            .build()
            .expect("Failed to build provider");

        assert_eq!(provider.api_key, "test-key");
        assert_eq!(provider.base_url, "https://api.groq.com/openai/v1");
    }

    #[tokio::test]
    async fn test_builder_requires_api_key() {
        let result = GroqProvider::builder().build();
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
        let provider = GroqProvider::builder()
            .api_key("test-key")
            .base_url("https://custom.groq.com/v1")
            .build()
            .expect("Failed to build provider");

        assert_eq!(provider.base_url, "https://custom.groq.com/v1");
    }

    #[tokio::test]
    async fn test_builder_with_default_model() {
        let provider = GroqProvider::builder()
            .api_key("test-key")
            .model("llama3-70b-8192")
            .build()
            .expect("Failed to build provider");

        assert_eq!(provider.default_model, "llama3-70b-8192");
    }

    #[tokio::test]
    async fn test_builder_with_timeout() {
        use std::time::Duration;

        let provider = GroqProvider::builder()
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

        let provider = GroqProvider::builder()
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
        let provider = GroqProvider::builder()
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

        let provider = GroqProvider::builder()
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
            let provider = GroqProvider::new("test-key");

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
                GroqMessageContent::Text(text) => assert_eq!(text, "You are helpful"),
            }
        }

        #[tokio::test]
        async fn test_build_request_basic() {
            let provider = GroqProvider::new("test-key");

            let request = ChatRequest::new("llama3-70b-8192")
                .with_message(Message::user("Hello"))
                .with_temperature(0.7)
                .with_max_tokens(100);

            let groq_request = provider.build_request(&request).unwrap();

            assert_eq!(groq_request.model, "llama3-70b-8192");
            assert_eq!(groq_request.temperature, Some(0.7));
            assert_eq!(groq_request.max_tokens, Some(100));
            assert_eq!(groq_request.stream, Some(false));
        }

        #[tokio::test]
        async fn test_build_request_with_json_mode() {
            let provider = GroqProvider::new("test-key");

            let mut request =
                ChatRequest::new("llama3-70b-8192").with_message(Message::user("Hello"));
            request.response_format = Some(crate::types::ResponseFormat::Json);

            let groq_request = provider.build_request(&request).unwrap();

            assert!(groq_request.response_format.is_some());
            assert_eq!(groq_request.response_format.unwrap().r#type, "json_object");
        }

        #[tokio::test]
        async fn test_build_request_with_all_parameters() {
            let provider = GroqProvider::new("test-key");

            let mut request = ChatRequest::new("test-model")
                .with_message(Message::user("Hello"))
                .with_temperature(0.8)
                .with_max_tokens(500)
                .with_top_p(0.9);

            request.stop = Some(vec!["STOP".to_string()]);
            request.presence_penalty = Some(0.5);
            request.frequency_penalty = Some(0.3);
            request.seed = Some(42);

            let groq_request = provider.build_request(&request).unwrap();

            assert_eq!(groq_request.temperature, Some(0.8));
            assert_eq!(groq_request.max_tokens, Some(500));
            assert_eq!(groq_request.top_p, Some(0.9));
            assert_eq!(groq_request.stop, Some(vec!["STOP".to_string()]));
            assert_eq!(groq_request.presence_penalty, Some(0.5));
            assert_eq!(groq_request.frequency_penalty, Some(0.3));
            assert_eq!(groq_request.seed, Some(42));
        }

        #[tokio::test]
        async fn test_build_request_serializes_typed_structured_output_json_schema() {
            use crate::capabilities::{StructuredOutputConfig, StructuredOutputMode};

            let provider = GroqProvider::new("test-key");
            let mut request =
                ChatRequest::new("llama-3.3-70b-versatile").with_message(Message::user("JSON"));
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

            let groq_request = provider.build_request(&request).unwrap();
            let value = serde_json::to_value(&groq_request).unwrap();
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

            let provider = GroqProvider::new("test-key");
            let mut request = ChatRequest::new("llama-3.3-70b-versatile")
                .with_message(Message::user("Call a tool."));
            request.tool_call_config = Some(ToolCallConfig {
                tools: vec![ToolDefinition {
                    name: "search_inventory".into(),
                    description: Some("Search inventory.".into()),
                    parameters: serde_json::json!({"type": "object"}),
                    strict: Some(true),
                }],
                tool_choice: ToolChoice::Named("search_inventory".into()),
                parallel_tool_calls: Some(false),
                streaming_tool_calls: None,
            });

            let groq_request = provider.build_request(&request).unwrap();
            let value = serde_json::to_value(&groq_request).unwrap();
            assert_eq!(value["tools"][0]["type"], "function");
            assert_eq!(value["tools"][0]["function"]["name"], "search_inventory");
            assert_eq!(value["tool_choice"]["type"], "function");
            assert_eq!(value["tool_choice"]["function"]["name"], "search_inventory");
            assert_eq!(value["parallel_tool_calls"], false);
        }

        #[tokio::test]
        async fn test_build_request_filters_recognized_compound_tool_before_wire() {
            use crate::capabilities::{ToolCallConfig, ToolChoice, ToolDefinition};

            let provider = GroqProvider::new("test-key");
            let mut request = ChatRequest::new("llama-3.3-70b-versatile")
                .with_message(Message::user("Search and call a function."));
            request.tool_call_config = Some(ToolCallConfig {
                tools: vec![
                    ToolDefinition {
                        name: "web_search".into(),
                        description: None,
                        parameters: serde_json::json!({"type": "object"}),
                        strict: None,
                    },
                    ToolDefinition {
                        name: "search_inventory".into(),
                        description: None,
                        parameters: serde_json::json!({"type": "object"}),
                        strict: None,
                    },
                ],
                tool_choice: ToolChoice::Auto,
                parallel_tool_calls: None,
                streaming_tool_calls: None,
            });

            let groq_request = provider.build_request(&request).unwrap();
            let value = serde_json::to_value(&groq_request).unwrap();
            let tools = value["tools"].as_array().expect("tools array");
            assert_eq!(tools.len(), 1);
            assert_eq!(tools[0]["function"]["name"], "search_inventory");
        }
    }
}
