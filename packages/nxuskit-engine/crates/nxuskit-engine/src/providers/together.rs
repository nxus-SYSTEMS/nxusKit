//! Together AI provider implementation

use async_trait::async_trait;
use futures::Stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::{
    ChatRequest, ChatResponse, LLMProvider, Message, ModelInfo, Role, StreamChunk, TokenCount,
    TokenUsage,
    error::{NxuskitError, Result},
    parameter_adapter::ParameterAdapter,
    types::{ContentPart, ImageData, InferenceMetadata, MessageContent, ProviderCapabilities},
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
    /// Static adapter declarations, not model or endpoint-effective support.
    pub fn adapter_capabilities() -> ProviderCapabilities {
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
            supports_json_schema: true,
            penalty_range: Some((-2.0, 2.0)),
            max_logprobs: None,
        }
    }
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
                "Together stop sequences must contain 0 to 4 nonempty strings".into(),
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
            return Err(NxuskitError::unsupported_capability("together", "logprobs"));
        }
        if request.reasoning_effort.is_some() {
            return Err(NxuskitError::unsupported_capability(
                "together",
                "reasoning_effort",
            ));
        }
        if !matches!(
            request.thinking_mode,
            crate::types::ThinkingMode::Auto | crate::types::ThinkingMode::Omit
        ) {
            return Err(NxuskitError::unsupported_capability("together", "thinking"));
        }

        if let Some(crate::types::ResponseFormat::JsonSchema { schema }) =
            request.response_format.as_ref()
            && !schema.is_object()
        {
            return Err(NxuskitError::InvalidRequest(
                "Together JSON Schema must be an object".into(),
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
            if choice.is_object() {
                return Err(NxuskitError::unsupported_capability(
                    "together",
                    "named_tool_choice",
                ));
            }
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
            if matches!(
                &config.tool_choice,
                crate::capabilities::ToolChoice::Named(_)
            ) {
                return Err(NxuskitError::unsupported_capability(
                    "together",
                    "named_tool_choice",
                ));
            }
            if config.streaming_tool_calls.is_some() {
                return Err(NxuskitError::unsupported_capability(
                    "together",
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
                "together",
                "tool_streaming",
            ));
        }

        if request.openai_responses.is_some() {
            return Err(NxuskitError::unsupported_capability(
                "together",
                "openai_responses",
            ));
        }
        if request.provider_options.is_some() {
            return Err(NxuskitError::unsupported_capability(
                "together",
                "provider_options",
            ));
        }

        Ok(())
    }

    /// Build Together API request from adapted ChatRequest
    fn build_request(&self, request: &ChatRequest) -> Result<TogetherRequest> {
        let record = crate::capabilities::registry::find("together").ok_or_else(|| {
            NxuskitError::Configuration("provider capability record not found: together".into())
        })?;
        let mut validation_warnings = Vec::new();
        super::validate_typed_capability_request("together", request, &mut validation_warnings)?;
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

        // Legacy response_format remains supported when the typed surface is
        // absent. The typed surface takes precedence when both are provided.
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
                ResponseFormat::JsonSchema { schema } => {
                    response_format_typed = serde_json::json!({
                        "type": "json_schema",
                        "json_schema": {
                            "name": "response",
                            "strict": true,
                            "schema": schema
                        }
                    });
                    together_request.response_format_typed = response_format_typed;
                }
                ResponseFormat::Text => {}
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
        self.validate_request_surface(request, false)?;
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
            let error_text = crate::provider::http::read_response_text_bounded(response)
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(match status.as_u16() {
                429 => NxuskitError::rate_limit(retry_after),
                _ => NxuskitError::provider(status.as_u16(), error_text),
            });
        }

        let together_response: TogetherResponse =
            crate::provider::http::read_response_json_bounded(response).await?;

        // Extract content and tool calls from first choice.
        let content = together_response
            .choices
            .first()
            .and_then(|choice| choice.message.content.as_deref())
            .unwrap_or("")
            .to_string();
        let tool_calls = together_response
            .choices
            .first()
            .and_then(|choice| choice.message.tool_calls.clone())
            .filter(|calls| !calls.is_empty());

        // Convert finish reason
        let raw_finish_reason = together_response
            .choices
            .first()
            .and_then(|choice| choice.finish_reason.clone());
        let finish_reason = together_response
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
            together_response.model,
            TokenUsage::estimated_only(TokenCount::new(
                together_response.usage.prompt_tokens,
                together_response.usage.completion_tokens,
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
            "provider": "together"
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
            let error_text = crate::provider::http::read_response_text_bounded(response)
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(NxuskitError::provider(status.as_u16(), error_text));
        }

        let models_response: TogetherModelsResponse =
            crate::provider::http::read_response_json_bounded(response).await?;

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
        Self::adapter_capabilities()
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
    seed: Option<i64>,
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
    #[serde(default)]
    tool_calls: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
struct TogetherUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    #[allow(dead_code)]
    total_tokens: u32,
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
        assert!(caps.supports_json_schema);
        assert_eq!(caps.penalty_range, Some((-2.0, 2.0)));
    }

    #[tokio::test]
    async fn test_provider_name() {
        let provider = TogetherProvider::new("test-key");
        assert_eq!(provider.provider_name(), "together");
    }

    fn assert_together_unsupported(request: &ChatRequest, capability: &str) {
        let provider = TogetherProvider::new("test-key");
        match provider.validate_request_surface(request, false) {
            Err(NxuskitError::UnsupportedCapability {
                provider,
                capability: actual,
            }) => {
                assert_eq!(provider, "together");
                assert_eq!(actual, capability);
            }
            other => panic!("expected unsupported {capability}, got {other:?}"),
        }
    }

    #[test]
    fn together_request_surface_fails_closed_before_adaptation() {
        use crate::capabilities::{
            OpenAIResponsesOptions, StructuredOutputConfig, StructuredOutputMode, ToolCallConfig,
            ToolChoice, ToolDefinition,
        };

        let provider = TogetherProvider::new("test-key");
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
        ] {
            assert_together_unsupported(&request, capability);
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
            }) if provider == "together" && capability == "tool_streaming"
        ));

        let mut legacy_named_choice = base.clone();
        legacy_named_choice.tool_choice = Some(serde_json::json!({
            "type": "function",
            "function": {"name": "lookup"}
        }));
        assert_together_unsupported(&legacy_named_choice, "named_tool_choice");

        let mut typed_tools = base;
        typed_tools.tool_call_config = Some(ToolCallConfig {
            tools: vec![ToolDefinition {
                name: "lookup".into(),
                description: None,
                parameters: serde_json::json!({"type": "object"}),
                strict: None,
            }],
            tool_choice: ToolChoice::Required,
            parallel_tool_calls: Some(true),
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
            }) if provider == "together" && capability == "tool_streaming"
        ));

        let mut streaming_tool_flag = ChatRequest::new("m");
        streaming_tool_flag.tool_call_config = Some(ToolCallConfig {
            tools: vec![ToolDefinition {
                name: "lookup".into(),
                description: None,
                parameters: serde_json::json!({"type": "object"}),
                strict: None,
            }],
            tool_choice: ToolChoice::Required,
            parallel_tool_calls: None,
            streaming_tool_calls: Some(false),
        });
        assert!(matches!(
            provider.validate_request_surface(&streaming_tool_flag, false),
            Err(NxuskitError::UnsupportedCapability {
                ref provider,
                ref capability,
            }) if provider == "together" && capability == "streaming_tool_calls"
        ));
    }

    fn together_fixture_server(
        response_body: &str,
    ) -> (TogetherProvider, std::thread::JoinHandle<serde_json::Value>) {
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

        let provider = TogetherProvider::new("fixture").with_base_url(format!("http://{address}"));
        (provider, server)
    }

    #[test]
    fn together_legacy_schema_and_tool_wire() {
        let provider = TogetherProvider::new("test-key");
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

        let mut request = ChatRequest::new("meta-llama/Llama-3.3-70B-Instruct-Turbo")
            .with_message(Message::user("Look up A-1."));
        request.response_format = Some(crate::types::ResponseFormat::JsonSchema {
            schema: schema.clone(),
        });
        request.tools = Some(vec![tool.clone()]);
        request.tool_choice = Some(serde_json::Value::String("required".into()));

        provider.validate_request_surface(&request, false).unwrap();
        let wire = serde_json::to_value(provider.build_request(&request).unwrap()).unwrap();
        assert_eq!(wire["response_format"]["type"], "json_schema");
        assert_eq!(wire["response_format"]["json_schema"]["name"], "response");
        assert_eq!(wire["response_format"]["json_schema"]["strict"], true);
        assert_eq!(wire["response_format"]["json_schema"]["schema"], schema);
        assert_eq!(wire["tools"], serde_json::json!([tool]));
        assert_eq!(wire["tool_choice"], "required");
    }

    #[tokio::test]
    async fn together_unary_tool_round_trip() {
        let response_body = serde_json::json!({
            "id": "fixture",
            "model": "meta-llama/Llama-3.3-70B-Instruct-Turbo",
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
        let (provider, server) = together_fixture_server(&response_body);

        let mut request = ChatRequest::new("meta-llama/Llama-3.3-70B-Instruct-Turbo")
            .with_message(Message::user("Look up A-1."));
        request.tools = Some(vec![serde_json::json!({
            "type": "function",
            "function": {
                "name": "lookup",
                "parameters": {"type": "object"}
            }
        })]);
        request.tool_choice = Some(serde_json::Value::String("required".into()));

        let response = provider.chat(&request).await.unwrap();
        let wire = server.join().unwrap();
        assert_eq!(wire["tools"][0]["function"]["name"], "lookup");
        assert_eq!(wire["tool_choice"], "required");
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
    async fn together_unsupported_controls_reject_before_http() {
        let provider = TogetherProvider::new("test-key").with_base_url("http://127.0.0.1:1");

        let mut request = ChatRequest::new("m");
        request.logprobs = Some(true);

        assert!(matches!(
            provider.chat(&request).await,
            Err(NxuskitError::UnsupportedCapability {
                ref provider,
                ref capability,
            }) if provider == "together" && capability == "logprobs"
        ));
        assert!(matches!(
            provider.chat_stream(&request).await,
            Err(NxuskitError::UnsupportedCapability {
                ref provider,
                ref capability,
            }) if provider == "together" && capability == "logprobs"
        ));
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
