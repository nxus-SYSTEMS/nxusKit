//! Groq provider implementation

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
    types::{ContentPart, InferenceMetadata, MessageContent, ProviderCapabilities},
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
    /// Static adapter declarations, not model or endpoint-effective support.
    pub fn adapter_capabilities() -> ProviderCapabilities {
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
            supports_json_schema: true,
            penalty_range: Some((-2.0, 2.0)),
            max_logprobs: None,
        }
    }
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

    fn validate_legacy_tool_choice(choice: &serde_json::Value) -> Result<()> {
        let invalid = || NxuskitError::InvalidRequest("invalid tool_choice".into());
        if let Some(value) = choice.as_str() {
            return if matches!(value, "auto" | "none" | "required") {
                Ok(())
            } else {
                Err(invalid())
            };
        }
        let object = choice.as_object().ok_or_else(invalid)?;
        if object.get("type").and_then(serde_json::Value::as_str) != Some("function") {
            return Err(invalid());
        }
        let name = object
            .get("function")
            .and_then(serde_json::Value::as_object)
            .and_then(|function| function.get("name"))
            .and_then(serde_json::Value::as_str)
            .filter(|name| !name.is_empty());
        if name.is_none() {
            return Err(invalid());
        }
        Ok(())
    }

    fn validate_request_surface(&self, request: &ChatRequest, streaming: bool) -> Result<()> {
        request.validate_legacy_sampling(self.provider_name())?;

        if let Some(stops) = request.stop.as_ref()
            && (stops.len() > 4 || stops.iter().any(String::is_empty))
        {
            return Err(NxuskitError::InvalidRequest(
                "Groq stop sequences must contain 0 to 4 nonempty strings".into(),
            ));
        }
        if request.top_logprobs.is_some_and(|value| value > 20) {
            return Err(NxuskitError::InvalidRequest(
                "top_logprobs must be between 0 and 20".into(),
            ));
        }
        if request
            .reasoning_effort
            .as_deref()
            .is_some_and(|value| !matches!(value, "low" | "medium" | "high" | "max"))
        {
            return Err(NxuskitError::InvalidRequest(
                "invalid reasoning_effort".into(),
            ));
        }

        if request.logprobs.is_some() || request.top_logprobs.is_some() {
            return Err(NxuskitError::unsupported_capability("groq", "logprobs"));
        }
        if request.reasoning_effort.is_some() {
            return Err(NxuskitError::unsupported_capability(
                "groq",
                "reasoning_effort",
            ));
        }
        if !matches!(
            request.thinking_mode,
            crate::types::ThinkingMode::Auto | crate::types::ThinkingMode::Omit
        ) {
            return Err(NxuskitError::unsupported_capability("groq", "thinking"));
        }

        if let Some(crate::types::ResponseFormat::JsonSchema { schema }) =
            request.response_format.as_ref()
            && !schema.is_object()
        {
            return Err(NxuskitError::InvalidRequest(
                "Groq JSON Schema must be an object".into(),
            ));
        }

        if let Some(config) = request.structured_output.as_ref() {
            let schema_is_non_object = matches!(
                config.mode,
                crate::capabilities::StructuredOutputMode::JsonSchema
            ) && config
                .schema
                .as_ref()
                .is_some_and(|schema| !schema.is_object());
            if config.validate_shape().is_err() || schema_is_non_object {
                return Err(NxuskitError::InvalidRequest(
                    "invalid structured_output".into(),
                ));
            }
        }

        request.validate_legacy_tools()?;
        if let Some(choice) = request.tool_choice.as_ref() {
            Self::validate_legacy_tool_choice(choice)?;
        }
        if let Some(config) = request.tool_call_config.as_ref() {
            let invalid_parameters = config.tools.iter().any(|tool| !tool.parameters.is_object());
            let invalid_named_choice = matches!(
                &config.tool_choice,
                crate::capabilities::ToolChoice::Named(name) if name.is_empty()
            );
            if config.validate_shape().is_err() || invalid_parameters || invalid_named_choice {
                return Err(NxuskitError::InvalidRequest(
                    "invalid tool_call_config".into(),
                ));
            }
            if config.streaming_tool_calls.is_some() {
                return Err(NxuskitError::unsupported_capability(
                    "groq",
                    "streaming_tool_calls",
                ));
            }
        }

        if streaming
            && (request
                .tools
                .as_ref()
                .is_some_and(|tools| !tools.is_empty())
                || request.tool_choice.is_some()
                || request.tool_call_config.is_some())
        {
            return Err(NxuskitError::unsupported_capability(
                "groq",
                "tool_streaming",
            ));
        }

        if request.openai_responses.is_some() {
            return Err(NxuskitError::unsupported_capability(
                "groq",
                "openai_responses",
            ));
        }
        if request.provider_options.is_some() {
            return Err(NxuskitError::unsupported_capability(
                "groq",
                "provider_options",
            ));
        }

        if request.messages.iter().any(|message| {
            matches!(
                &message.content,
                MessageContent::Parts(parts)
                    if parts.iter().any(|part| matches!(part, ContentPart::Image { .. }))
            )
        }) {
            return Err(NxuskitError::unsupported_capability("groq", "images"));
        }

        Ok(())
    }

    /// Build Groq API request from adapted ChatRequest
    fn build_request(&self, request: &ChatRequest) -> Result<GroqRequest> {
        let record = crate::capabilities::registry::find("groq").ok_or_else(|| {
            NxuskitError::Configuration("provider capability record not found: groq".into())
        })?;
        let mut validation_warnings = Vec::new();
        super::validate_typed_capability_request("groq", request, &mut validation_warnings)?;
        let messages = self.convert_messages(&request.messages);

        let mut response_format_typed = request
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
            None => (
                request
                    .tools
                    .as_ref()
                    .map(|tools| serde_json::Value::Array(tools.clone()))
                    .unwrap_or(serde_json::Value::Null),
                request
                    .tool_choice
                    .clone()
                    .unwrap_or(serde_json::Value::Null),
                None,
            ),
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

        // Legacy response_format remains supported when the typed surface is
        // absent. The typed surface takes precedence when both are provided.
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
                ResponseFormat::JsonSchema { schema } => {
                    response_format_typed = serde_json::json!({
                        "type": "json_schema",
                        "json_schema": {
                            "name": "response",
                            "strict": true,
                            "schema": schema
                        }
                    });
                    groq_request.response_format_typed = response_format_typed;
                }
                ResponseFormat::Text => {}
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
        self.validate_request_surface(request, false)?;
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
            let error_text = crate::provider::http::read_response_text_bounded(response)
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(match status.as_u16() {
                429 => NxuskitError::rate_limit(retry_after),
                _ => NxuskitError::provider(status.as_u16(), error_text),
            });
        }

        let groq_response: GroqResponse =
            crate::provider::http::read_response_json_bounded(response).await?;

        // Extract content and tool calls from first choice.
        let content = groq_response
            .choices
            .first()
            .and_then(|choice| choice.message.content.as_deref())
            .unwrap_or("")
            .to_string();
        let tool_calls = groq_response
            .choices
            .first()
            .and_then(|choice| choice.message.tool_calls.clone())
            .filter(|calls| !calls.is_empty());

        // Convert finish reason
        let raw_finish_reason = groq_response
            .choices
            .first()
            .and_then(|choice| choice.finish_reason.clone());
        let finish_reason = groq_response
            .choices
            .first()
            .and_then(|choice| choice.finish_reason.as_ref())
            .and_then(|reason| match reason.as_str() {
                "stop" => Some(crate::types::FinishReason::Stop),
                "length" => Some(crate::types::FinishReason::Length),
                "tool_calls" => Some(crate::types::FinishReason::ToolCalls),
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
        response.tool_calls = tool_calls;

        response.finish_reason = finish_reason;
        response.raw_finish_reason = raw_finish_reason;
        response.warnings = adapted.warnings;

        // Populate inference metadata
        response.inference_metadata = InferenceMetadata {
            is_complete: true,
            finish_reason: response.finish_reason,
            ..InferenceMetadata::default()
        }
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
        self.validate_request_surface(request, true)?;
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
            let error_text = crate::provider::http::read_response_text_bounded(response)
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(match status.as_u16() {
                429 => NxuskitError::rate_limit(retry_after),
                _ => NxuskitError::provider(status.as_u16(), error_text),
            });
        }

        Ok(super::openai::openai_compatible_stream(
            response,
            adapted_request,
        ))
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
            let error_text = crate::provider::http::read_response_text_bounded(response)
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(NxuskitError::provider(status.as_u16(), error_text));
        }

        let models_response: GroqModelsResponse =
            crate::provider::http::read_response_json_bounded(response).await?;

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
        Self::adapter_capabilities()
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
    seed: Option<i64>,
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
    #[serde(default)]
    tool_calls: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
struct GroqUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    #[allow(dead_code)]
    total_tokens: u32,
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
        assert!(caps.supports_json_schema);
        assert_eq!(caps.penalty_range, Some((-2.0, 2.0)));
    }

    fn assert_groq_unsupported(request: &ChatRequest, capability: &str) {
        let provider = GroqProvider::new("test-key");
        match provider.validate_request_surface(request, false) {
            Err(NxuskitError::UnsupportedCapability {
                provider,
                capability: actual,
            }) => {
                assert_eq!(provider, "groq");
                assert_eq!(actual, capability);
            }
            other => panic!("expected unsupported {capability}, got {other:?}"),
        }
    }

    #[test]
    fn groq_request_surface_fails_closed_before_adaptation() {
        use crate::capabilities::{
            OpenAIResponsesOptions, StructuredOutputConfig, StructuredOutputMode, ToolCallConfig,
            ToolChoice, ToolDefinition,
        };

        let provider = GroqProvider::new("test-key");
        let base = ChatRequest::new("m").with_message(Message::user("hi"));

        let mut invalid_stop = base.clone();
        invalid_stop.stop = Some(vec![
            "a".into(),
            "b".into(),
            "c".into(),
            "d".into(),
            "e".into(),
        ]);
        assert!(matches!(
            provider.validate_request_surface(&invalid_stop, false),
            Err(NxuskitError::InvalidRequest(_))
        ));

        let mut empty_stop = base.clone();
        empty_stop.stop = Some(vec!["".into()]);
        assert!(matches!(
            provider.validate_request_surface(&empty_stop, false),
            Err(NxuskitError::InvalidRequest(_))
        ));

        let mut invalid_top_logprobs = base.clone();
        invalid_top_logprobs.top_logprobs = Some(21);
        assert!(matches!(
            provider.validate_request_surface(&invalid_top_logprobs, false),
            Err(NxuskitError::InvalidRequest(_))
        ));

        let mut invalid_reasoning = base.clone();
        invalid_reasoning.reasoning_effort = Some("extreme".into());
        assert!(matches!(
            provider.validate_request_surface(&invalid_reasoning, false),
            Err(NxuskitError::InvalidRequest(_))
        ));

        for (capability, request) in [
            ("logprobs", {
                let mut request = base.clone();
                request.logprobs = Some(true);
                request
            }),
            ("reasoning_effort", {
                let mut request = base.clone();
                request.reasoning_effort = Some("low".into());
                request
            }),
            ("thinking", {
                let mut request = base.clone();
                request.thinking_mode = crate::types::ThinkingMode::Enabled;
                request
            }),
            ("openai_responses", {
                let mut request = base.clone();
                request.openai_responses = Some(OpenAIResponsesOptions::default());
                request
            }),
            ("images", {
                ChatRequest::new("m").with_message(
                    Message::user("describe").with_image_url("https://example.invalid/image.png"),
                )
            }),
        ] {
            assert_groq_unsupported(&request, capability);
        }

        let mut malformed_schema = base.clone();
        malformed_schema.response_format = Some(crate::types::ResponseFormat::JsonSchema {
            schema: serde_json::json!([]),
        });
        assert!(matches!(
            provider.validate_request_surface(&malformed_schema, false),
            Err(NxuskitError::InvalidRequest(_))
        ));

        let mut legacy_schema = base.clone();
        legacy_schema.response_format = Some(crate::types::ResponseFormat::JsonSchema {
            schema: serde_json::json!({"type": "object"}),
        });
        provider
            .validate_request_surface(&legacy_schema, false)
            .unwrap();

        let mut typed_schema = base.clone();
        typed_schema.structured_output = Some(StructuredOutputConfig {
            mode: StructuredOutputMode::JsonSchema,
            schema: Some(serde_json::json!({"type": "object"})),
            schema_name: Some("response".into()),
            strict: Some(true),
            schema_subset: None,
        });
        provider
            .validate_request_surface(&typed_schema, false)
            .unwrap();

        let mut malformed_tool = base.clone();
        malformed_tool.tools = Some(vec![serde_json::json!({
            "type": "function",
            "function": {"name": "", "parameters": {}}
        })]);
        assert!(matches!(
            provider.validate_request_surface(&malformed_tool, false),
            Err(NxuskitError::InvalidRequest(_))
        ));

        let mut legacy_tool = base.clone();
        legacy_tool.tools = Some(vec![serde_json::json!({
            "type": "function",
            "function": {"name": "lookup", "parameters": {"type": "object"}}
        })]);
        provider
            .validate_request_surface(&legacy_tool, false)
            .unwrap();
        assert!(matches!(
            provider.validate_request_surface(&legacy_tool, true),
            Err(NxuskitError::UnsupportedCapability {
                ref provider,
                ref capability,
            }) if provider == "groq" && capability == "tool_streaming"
        ));

        let mut typed_tools = base.clone();
        typed_tools.tool_call_config = Some(ToolCallConfig {
            tools: vec![ToolDefinition {
                name: "lookup".into(),
                description: None,
                parameters: serde_json::json!({"type": "object"}),
                strict: None,
            }],
            tool_choice: ToolChoice::Auto,
            parallel_tool_calls: Some(false),
            streaming_tool_calls: None,
        });
        provider
            .validate_request_surface(&typed_tools, false)
            .unwrap();
        assert!(matches!(
            provider.validate_request_surface(&typed_tools, true),
            Err(NxuskitError::UnsupportedCapability {
                ref provider,
                ref capability,
            }) if provider == "groq" && capability == "tool_streaming"
        ));

        let mut streaming_tools = base;
        streaming_tools.tool_call_config = Some(ToolCallConfig {
            tools: vec![ToolDefinition {
                name: "lookup".into(),
                description: None,
                parameters: serde_json::json!({"type": "object"}),
                strict: None,
            }],
            tool_choice: ToolChoice::Auto,
            parallel_tool_calls: None,
            streaming_tool_calls: Some(false),
        });
        assert!(matches!(
            provider.validate_request_surface(&streaming_tools, false),
            Err(NxuskitError::UnsupportedCapability {
                ref provider,
                ref capability,
            }) if provider == "groq" && capability == "streaming_tool_calls"
        ));
    }

    fn groq_fixture_server(
        response_body: &str,
    ) -> (GroqProvider, std::thread::JoinHandle<serde_json::Value>) {
        use std::io::{Read, Write};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let response_body = response_body.to_owned();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();

            let mut request = Vec::new();
            let mut buffer = [0u8; 4096];
            let (header_end, content_length) = loop {
                let read = socket.read(&mut buffer).unwrap();
                assert!(read > 0, "connection closed before request body");
                request.extend_from_slice(&buffer[..read]);
                if let Some(index) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                    let header_end = index + 4;
                    let headers = std::str::from_utf8(&request[..header_end]).unwrap();
                    let content_length = headers
                        .lines()
                        .find_map(|line| {
                            line.strip_prefix("Content-Length:")
                                .or_else(|| line.strip_prefix("content-length:"))
                                .map(str::trim)
                                .map(str::parse::<usize>)
                        })
                        .transpose()
                        .unwrap()
                        .unwrap_or(0);
                    break (header_end, content_length);
                }
            };
            while request.len() < header_end + content_length {
                let read = socket.read(&mut buffer).unwrap();
                assert!(read > 0, "connection closed before complete request body");
                request.extend_from_slice(&buffer[..read]);
            }

            let wire: serde_json::Value =
                serde_json::from_slice(&request[header_end..header_end + content_length]).unwrap();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            socket.write_all(response.as_bytes()).unwrap();
            wire
        });

        let provider = GroqProvider::builder()
            .api_key("fixture")
            .model("fixture-model")
            .base_url(format!("http://{address}"))
            .build()
            .unwrap();
        (provider, server)
    }

    #[test]
    fn groq_legacy_schema_and_tool_wire() {
        let provider = GroqProvider::new("test-key");
        let schema = serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["sku"],
            "properties": {"sku": {"type": "string"}}
        });
        let tool = serde_json::json!({
            "type": "function",
            "function": {
                "name": "lookup",
                "description": "Look up inventory.",
                "parameters": {
                    "type": "object",
                    "properties": {"sku": {"type": "string"}},
                    "required": ["sku"]
                }
            }
        });
        let choice = serde_json::json!({
            "type": "function",
            "function": {"name": "lookup"}
        });

        let mut request =
            ChatRequest::new("fixture-model").with_message(Message::user("Look up A-1."));
        request.response_format = Some(crate::types::ResponseFormat::JsonSchema {
            schema: schema.clone(),
        });
        request.tools = Some(vec![tool.clone()]);
        request.tool_choice = Some(choice.clone());

        provider.validate_request_surface(&request, false).unwrap();
        let wire = serde_json::to_value(provider.build_request(&request).unwrap()).unwrap();
        assert_eq!(wire["response_format"]["type"], "json_schema");
        assert_eq!(wire["response_format"]["json_schema"]["name"], "response");
        assert_eq!(wire["response_format"]["json_schema"]["strict"], true);
        assert_eq!(wire["response_format"]["json_schema"]["schema"], schema);
        assert_eq!(wire["tools"], serde_json::json!([tool]));
        assert_eq!(wire["tool_choice"], choice);
    }

    #[tokio::test]
    async fn groq_unary_tool_round_trip() {
        let response_body = serde_json::json!({
            "id": "fixture",
            "model": "fixture-model",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "lookup",
                            "arguments": "{\"sku\":\"A-1\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {"prompt_tokens": 3, "completion_tokens": 2, "total_tokens": 5}
        })
        .to_string();
        let (provider, server) = groq_fixture_server(&response_body);

        let mut request =
            ChatRequest::new("fixture-model").with_message(Message::user("Look up A-1."));
        request.tools = Some(vec![serde_json::json!({
            "type": "function",
            "function": {
                "name": "lookup",
                "parameters": {"type": "object"}
            }
        })]);
        request.tool_choice = Some(serde_json::json!({
            "type": "function",
            "function": {"name": "lookup"}
        }));

        let response = provider.chat(&request).await.unwrap();
        let wire = server.join().unwrap();
        assert_eq!(wire["tools"][0]["function"]["name"], "lookup");
        assert_eq!(wire["tool_choice"]["function"]["name"], "lookup");
        assert_eq!(
            response.finish_reason,
            Some(crate::types::FinishReason::ToolCalls)
        );
        assert_eq!(response.raw_finish_reason.as_deref(), Some("tool_calls"));
        assert_eq!(
            response.tool_calls,
            Some(vec![serde_json::json!({
                "id": "call_1",
                "type": "function",
                "function": {
                    "name": "lookup",
                    "arguments": "{\"sku\":\"A-1\"}"
                }
            })])
        );
    }

    #[tokio::test]
    async fn groq_unsupported_controls_reject_before_http() {
        let provider = GroqProvider::builder()
            .api_key("test-key")
            .base_url("http://127.0.0.1:1")
            .build()
            .unwrap();

        let mut request = ChatRequest::new("m");
        request.logprobs = Some(true);

        assert!(matches!(
            provider.chat(&request).await,
            Err(NxuskitError::UnsupportedCapability {
                ref provider,
                ref capability,
            }) if provider == "groq" && capability == "logprobs"
        ));
        assert!(matches!(
            provider.chat_stream(&request).await,
            Err(NxuskitError::UnsupportedCapability {
                ref provider,
                ref capability,
            }) if provider == "groq" && capability == "logprobs"
        ));
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
