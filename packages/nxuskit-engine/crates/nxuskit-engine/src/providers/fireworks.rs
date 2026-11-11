//! Fireworks AI provider implementation
//!
//! Fireworks AI provides high-performance inference for open source models through an OpenAI-compatible API.
//!
//! **Note**: This is a premium provider that requires a nxusKit Pro license.
//! In the free tier, all API methods will return `LicenseRequired` errors.

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
    types::{ContentPart, ImageData, InferenceMetadata, MessageContent, ProviderCapabilities},
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
    /// Static adapter declarations, not model or endpoint-effective support.
    pub fn adapter_capabilities() -> ProviderCapabilities {
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
            supports_json_schema: true,
            penalty_range: Some((-2.0, 2.0)),
            max_logprobs: None,
        }
    }
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

    /// Build Fireworks API request from an already validated/adapted ChatRequest.
    fn build_request(&self, request: &ChatRequest) -> Result<FireworksRequest> {
        let record = crate::capabilities::registry::find("fireworks").ok_or_else(|| {
            NxuskitError::Configuration("provider capability record not found: fireworks".into())
        })?;
        let messages = self.convert_messages(&request.messages);

        let mut response_format_typed = request
            .structured_output
            .as_ref()
            .map(crate::capabilities::openai_wire::response_format)
            .unwrap_or(serde_json::Value::Null);
        let (tools, tool_choice) = match request.tool_call_config.as_ref() {
            Some(config) => (
                crate::capabilities::openai_wire::tools_for(&record, config),
                crate::capabilities::openai_wire::tool_choice_for(&record, config),
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
            ),
        };

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
            response_format_typed,
            tools,
            tool_choice,
        };

        // Legacy response_format remains supported when the typed surface is
        // absent. The typed surface takes precedence when both are provided.
        if request.structured_output.is_none()
            && let Some(ref format) = request.response_format
        {
            use crate::types::ResponseFormat;
            match format {
                ResponseFormat::Json => {
                    fireworks_request.response_format = Some(FireworksResponseFormat {
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
                    fireworks_request.response_format_typed = response_format_typed;
                }
                ResponseFormat::Text => {}
            }
        }

        Ok(fireworks_request)
    }

    fn validate_request_surface(&self, request: &ChatRequest, streaming: bool) -> Result<()> {
        request.validate_legacy_sampling(self.provider_name())?;

        if let Some(stops) = request.stop.as_ref()
            && (stops.len() > 4 || stops.iter().any(String::is_empty))
        {
            return Err(NxuskitError::InvalidRequest(
                "Fireworks stop sequences must contain 0 to 4 nonempty strings".into(),
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
        if let Some(crate::types::ResponseFormat::JsonSchema { schema }) =
            request.response_format.as_ref()
            && !schema.is_object()
        {
            return Err(NxuskitError::InvalidRequest(
                "Fireworks JSON Schema must be an object".into(),
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
            if config.parallel_tool_calls.is_some() {
                return Err(NxuskitError::unsupported_capability(
                    "fireworks",
                    "parallel_tool_calls",
                ));
            }
            if config.streaming_tool_calls.is_some() {
                return Err(NxuskitError::unsupported_capability(
                    "fireworks",
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
                "fireworks",
                "tool_streaming",
            ));
        }

        for (capability, present) in [
            ("logprobs", request.logprobs.is_some()),
            ("top_logprobs", request.top_logprobs.is_some()),
            ("reasoning_effort", request.reasoning_effort.is_some()),
        ] {
            if present {
                return Err(NxuskitError::unsupported_capability(
                    "fireworks",
                    capability,
                ));
            }
        }

        if !matches!(
            request.thinking_mode,
            crate::types::ThinkingMode::Auto | crate::types::ThinkingMode::Omit
        ) {
            return Err(NxuskitError::unsupported_capability(
                "fireworks",
                "thinking",
            ));
        }
        if request.openai_responses.is_some() {
            return Err(NxuskitError::unsupported_capability(
                "fireworks",
                "openai_responses",
            ));
        }
        if request.provider_options.is_some() {
            return Err(NxuskitError::unsupported_capability(
                "fireworks",
                "provider_options",
            ));
        }

        Ok(())
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
        self.validate_request_surface(request, false)?;
        // Adapt request to provider capabilities.
        let capabilities = self.get_capabilities();
        let mut adapted = ParameterAdapter::adapt(request, &capabilities);
        super::validate_typed_capability_request(
            "fireworks",
            &adapted.request,
            &mut adapted.warnings,
        )?;
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
            let error_text = crate::provider::http::read_response_text_bounded(response)
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(match status.as_u16() {
                429 => NxuskitError::rate_limit(retry_after),
                _ => NxuskitError::provider(status.as_u16(), error_text),
            });
        }

        let fireworks_response: FireworksResponse =
            crate::provider::http::read_response_json_bounded(response).await?;

        // Extract content from first choice
        let content = fireworks_response
            .choices
            .first()
            .and_then(|choice| choice.message.content.as_deref())
            .unwrap_or("")
            .to_string();

        let tool_calls = fireworks_response
            .choices
            .first()
            .and_then(|choice| choice.message.tool_calls.clone())
            .filter(|calls| !calls.is_empty());

        // Convert finish reason.
        let raw_finish_reason = fireworks_response
            .choices
            .first()
            .and_then(|choice| choice.finish_reason.clone());
        let finish_reason = fireworks_response
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
            fireworks_response.model,
            TokenUsage::estimated_only(TokenCount::new(
                fireworks_response.usage.prompt_tokens,
                fireworks_response.usage.completion_tokens,
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
            "provider": "fireworks"
        }));

        Ok(response)
    }

    async fn chat_stream(
        &self,
        request: &ChatRequest,
    ) -> Result<Box<dyn Stream<Item = Result<StreamChunk>> + Send + Unpin>> {
        self.validate_request_surface(request, true)?;
        // Adapt request to provider capabilities.
        let capabilities = self.get_capabilities();
        let mut adapted = ParameterAdapter::adapt(request, &capabilities);
        super::validate_typed_capability_request(
            "fireworks",
            &adapted.request,
            &mut adapted.warnings,
        )?;
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
            let error_text = crate::provider::http::read_response_text_bounded(response)
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(NxuskitError::provider(status.as_u16(), error_text));
        }

        let models_response: FireworksModelsResponse =
            crate::provider::http::read_response_json_bounded(response).await?;

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
        Self::adapter_capabilities()
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
    seed: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<FireworksResponseFormat>,
    #[serde(rename = "response_format", skip_serializing_if = "is_json_null")]
    response_format_typed: serde_json::Value,
    #[serde(skip_serializing_if = "is_json_null")]
    tools: serde_json::Value,
    #[serde(skip_serializing_if = "is_json_null")]
    tool_choice: serde_json::Value,
}

fn is_json_null(value: &serde_json::Value) -> bool {
    value.is_null()
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
    #[serde(default)]
    tool_calls: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
struct FireworksUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    #[allow(dead_code)]
    total_tokens: u32,
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
        assert!(caps.supports_json_schema);
        assert_eq!(caps.max_stop_sequences, Some(4));
        assert!(caps.supports_seed);
    }

    fn assert_fireworks_unsupported(request: &ChatRequest, capability: &str) {
        let provider = FireworksProvider::new("test-key");
        match provider.validate_request_surface(request, false) {
            Err(NxuskitError::UnsupportedCapability {
                provider,
                capability: actual,
            }) => {
                assert_eq!(provider, "fireworks");
                assert_eq!(actual, capability);
            }
            other => panic!("expected unsupported {capability}, got {other:?}"),
        }
    }

    #[test]
    fn fireworks_request_surface_rejects_explicit_unsupported_controls() {
        let base = ChatRequest::new("accounts/fireworks/models/llama-v3-70b-instruct");

        let mut request = base.clone();
        request.logprobs = Some(true);
        assert_fireworks_unsupported(&request, "logprobs");

        let mut request = base.clone();
        request.top_logprobs = Some(3);
        assert_fireworks_unsupported(&request, "top_logprobs");

        let mut request = base.clone();
        request.reasoning_effort = Some("low".into());
        assert_fireworks_unsupported(&request, "reasoning_effort");

        let mut request = base.clone();
        request.thinking_mode = crate::types::ThinkingMode::Enabled;
        assert_fireworks_unsupported(&request, "thinking");

        let mut request = base;
        request.openai_responses = Some(crate::capabilities::OpenAIResponsesOptions::default());
        assert_fireworks_unsupported(&request, "openai_responses");
    }

    #[test]
    fn fireworks_request_surface_preserves_invalid_request_precedence() {
        let provider = FireworksProvider::new("test-key");

        let mut request = ChatRequest::new("accounts/fireworks/models/llama-v3-70b-instruct");
        request.stop = Some(vec![
            "a".into(),
            "b".into(),
            "c".into(),
            "d".into(),
            "e".into(),
        ]);
        assert!(matches!(
            provider.validate_request_surface(&request, false),
            Err(NxuskitError::InvalidRequest(_))
        ));

        let mut request = ChatRequest::new("accounts/fireworks/models/llama-v3-70b-instruct");
        request.top_logprobs = Some(21);
        assert!(matches!(
            provider.validate_request_surface(&request, false),
            Err(NxuskitError::InvalidRequest(_))
        ));

        let mut request = ChatRequest::new("accounts/fireworks/models/llama-v3-70b-instruct");
        request.reasoning_effort = Some("invalid".into());
        assert!(matches!(
            provider.validate_request_surface(&request, false),
            Err(NxuskitError::InvalidRequest(_))
        ));
    }

    #[test]
    fn fireworks_four_stop_sequences_remain_supported() {
        let provider = FireworksProvider::new("test-key");
        let mut request = ChatRequest::new("accounts/fireworks/models/llama-v3-70b-instruct");
        request.stop = Some(vec!["a".into(), "b".into(), "c".into(), "d".into()]);
        request.presence_penalty = Some(-0.5);
        request.frequency_penalty = Some(0.5);
        request.seed = Some(42);

        provider.validate_request_surface(&request, false).unwrap();
        let wire = provider.build_request(&request).unwrap();
        assert_eq!(wire.stop, request.stop);
        assert_eq!(wire.presence_penalty, Some(-0.5));
        assert_eq!(wire.frequency_penalty, Some(0.5));
        assert_eq!(wire.seed, Some(42));
    }

    #[tokio::test]
    async fn fireworks_unsupported_logprobs_reject_before_http() {
        let provider = FireworksProvider::builder()
            .api_key("test-key")
            .base_url("http://127.0.0.1:1")
            .build()
            .unwrap();
        let mut request = ChatRequest::new("accounts/fireworks/models/llama-v3-70b-instruct");
        request.logprobs = Some(true);

        assert!(matches!(
            provider.chat(&request).await,
            Err(NxuskitError::UnsupportedCapability {
                ref provider,
                ref capability,
            }) if provider == "fireworks" && capability == "logprobs"
        ));
        assert!(matches!(
            provider.chat_stream(&request).await,
            Err(NxuskitError::UnsupportedCapability {
                ref provider,
                ref capability,
            }) if provider == "fireworks" && capability == "logprobs"
        ));
    }

    fn fireworks_fixture_server(
        response_body: &str,
    ) -> (
        FireworksProvider,
        std::thread::JoinHandle<serde_json::Value>,
    ) {
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

            let headers = std::str::from_utf8(&request[..header_end]).unwrap();
            assert!(
                headers.starts_with("POST /chat/completions HTTP/1.1"),
                "{headers}"
            );
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

        let provider = FireworksProvider::builder()
            .api_key("test-key")
            .base_url(format!("http://{address}"))
            .build()
            .unwrap();
        (provider, server)
    }

    #[test]
    fn fireworks_typed_schema_wire() {
        use crate::capabilities::{StructuredOutputConfig, StructuredOutputMode};

        let provider = FireworksProvider::new("test-key");
        let schema = serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["sku"],
            "properties": {"sku": {"type": "string"}}
        });
        let mut request = ChatRequest::new("accounts/fireworks/models/llama-v3-70b-instruct")
            .with_message(Message::user("Return an inventory record."));
        request.structured_output = Some(StructuredOutputConfig {
            mode: StructuredOutputMode::JsonSchema,
            schema: Some(schema.clone()),
            schema_name: Some("inventory_record".into()),
            strict: Some(true),
            schema_subset: None,
        });

        provider.validate_request_surface(&request, false).unwrap();
        let wire = serde_json::to_value(provider.build_request(&request).unwrap()).unwrap();
        assert_eq!(wire["response_format"]["type"], "json_schema");
        assert_eq!(
            wire["response_format"]["json_schema"]["name"],
            "inventory_record"
        );
        assert_eq!(wire["response_format"]["json_schema"]["strict"], true);
        assert_eq!(
            wire["response_format"]["json_schema"]["schema"]["additionalProperties"],
            false
        );

        let mut legacy = ChatRequest::new("accounts/fireworks/models/llama-v3-70b-instruct");
        legacy.response_format = Some(crate::types::ResponseFormat::JsonSchema {
            schema: schema.clone(),
        });
        provider.validate_request_surface(&legacy, false).unwrap();
        let legacy_wire = serde_json::to_value(provider.build_request(&legacy).unwrap()).unwrap();
        assert_eq!(legacy_wire["response_format"]["type"], "json_schema");
        assert_eq!(
            legacy_wire["response_format"]["json_schema"]["name"],
            "response"
        );
        assert_eq!(
            legacy_wire["response_format"]["json_schema"]["strict"],
            true
        );
        assert_eq!(
            legacy_wire["response_format"]["json_schema"]["schema"],
            schema
        );
    }

    #[tokio::test]
    async fn fireworks_unary_tool_round_trip() {
        use crate::capabilities::{ToolCallConfig, ToolChoice, ToolDefinition};

        let response_body = serde_json::json!({
            "id": "chatcmpl_fixture",
            "model": "accounts/fireworks/models/llama-v3-70b-instruct",
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
        let (provider, server) = fireworks_fixture_server(&response_body);

        let mut request = ChatRequest::new("accounts/fireworks/models/llama-v3-70b-instruct")
            .with_message(Message::user("Look up A-1."));
        request.tool_call_config = Some(ToolCallConfig {
            tools: vec![ToolDefinition {
                name: "lookup".into(),
                description: Some("Look up inventory.".into()),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {"sku": {"type": "string"}},
                    "required": ["sku"]
                }),
                strict: Some(true),
            }],
            tool_choice: ToolChoice::Named("lookup".into()),
            parallel_tool_calls: None,
            streaming_tool_calls: None,
        });

        let response = provider.chat(&request).await.unwrap();
        let wire = server.join().unwrap();
        assert_eq!(wire["tools"][0]["type"], "function");
        assert_eq!(wire["tools"][0]["function"]["name"], "lookup");
        assert_eq!(wire["tools"][0]["function"]["strict"], true);
        assert_eq!(wire["tool_choice"]["type"], "function");
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
    async fn fireworks_tool_validation_and_streaming_fail_closed() {
        use crate::capabilities::{ToolCallConfig, ToolChoice, ToolDefinition};

        let provider = FireworksProvider::builder()
            .api_key("test-key")
            .base_url("http://127.0.0.1:1")
            .build()
            .unwrap();

        let mut malformed = ChatRequest::new("m");
        malformed.tools = Some(vec![serde_json::json!({
            "type": "function",
            "function": {"name": "", "parameters": {}}
        })]);
        assert!(matches!(
            provider.validate_request_surface(&malformed, false),
            Err(NxuskitError::InvalidRequest(_))
        ));

        let mut malformed_choice = ChatRequest::new("m");
        malformed_choice.tool_choice = Some(serde_json::json!("sometimes"));
        assert!(matches!(
            provider.validate_request_surface(&malformed_choice, false),
            Err(NxuskitError::InvalidRequest(_))
        ));

        for (parallel, streaming, capability) in [
            (Some(false), None, "parallel_tool_calls"),
            (None, Some(false), "streaming_tool_calls"),
        ] {
            let mut typed = ChatRequest::new("m");
            typed.tool_call_config = Some(ToolCallConfig {
                tools: vec![ToolDefinition {
                    name: "lookup".into(),
                    description: None,
                    parameters: serde_json::json!({"type": "object"}),
                    strict: None,
                }],
                tool_choice: ToolChoice::Auto,
                parallel_tool_calls: parallel,
                streaming_tool_calls: streaming,
            });
            assert!(matches!(
                provider.validate_request_surface(&typed, false),
                Err(NxuskitError::UnsupportedCapability {
                    provider: ref actual_provider,
                    capability: ref actual_capability,
                }) if actual_provider == "fireworks" && actual_capability == capability
            ));
        }

        let mut streaming = ChatRequest::new("m");
        streaming.tools = Some(vec![serde_json::json!({
            "type": "function",
            "function": {"name": "lookup", "parameters": {"type": "object"}}
        })]);
        assert!(matches!(
            provider.chat_stream(&streaming).await,
            Err(NxuskitError::UnsupportedCapability {
                ref provider,
                ref capability,
            }) if provider == "fireworks" && capability == "tool_streaming"
        ));
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
