//! OpenRouter provider implementation

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

/// OpenRouter provider
///
/// Provides unified access to multiple LLM providers through OpenRouter's API.
/// OpenRouter routes requests to various providers (OpenAI, Anthropic, etc.) through
/// a single OpenAI-compatible interface.
#[derive(Debug, Clone)]
pub struct OpenRouterProvider {
    #[allow(dead_code)]
    api_key: String,
    #[allow(dead_code)]
    base_url: String,
    #[allow(dead_code)]
    client: Client,
    #[allow(dead_code)]
    app_name: Option<String>,
}

impl OpenRouterProvider {
    /// Static adapter declarations, not model or endpoint-effective support.
    pub fn adapter_capabilities() -> ProviderCapabilities {
        ProviderCapabilities {
            supports_system_messages: true,
            supports_streaming: true,
            supports_vision: true, // Depends on routed model
            max_stop_sequences: Some(4),
            supports_presence_penalty: true,
            supports_frequency_penalty: true,
            supports_seed: true,
            supports_logprobs: false,

            // T054: OpenRouter proxies to varied backends; the router itself
            // does not guarantee logprob passthrough and no recorded fixture
            // proves streaming logprob support in the v0.9.4 Sprint 1 window.
            // Kept false per reconciled planning decision. Flip to true (and
            // wire decode_oai_logprob_delta from openai.rs) when T068 live
            // test confirms the specific routed model supports it and a
            // fixture is committed.
            supports_streaming_logprobs: false,
            supports_json_mode: true,
            supports_json_schema: true,
            penalty_range: Some((-2.0, 2.0)),
            max_logprobs: None,
        }
    }
}

impl OpenRouterProvider {
    /// Create a new OpenRouter provider with the given API key
    ///
    /// Uses default timeouts:
    /// - Connection timeout: 30 seconds
    /// - Read timeout: 120 seconds (for streaming)
    /// - Total timeout: 300 seconds
    pub fn new(api_key: impl Into<String>) -> Self {
        use std::time::Duration;

        // Default timeouts suitable for API calls
        let connection_timeout = Duration::from_secs(30);
        let read_timeout = Duration::from_secs(120);
        let total_timeout = Duration::from_secs(300);

        // Use centralized helper for consistent timeout handling
        let client = super::build_http_client(connection_timeout, read_timeout, total_timeout)
            .expect("Failed to build HTTP client with default timeouts");

        Self {
            api_key: api_key.into(),
            base_url: "https://openrouter.ai/api/v1".to_string(),
            client,
            app_name: None,
        }
    }

    /// Set a custom base URL for the OpenRouter API
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Set the connection, read, and total request timeout.
    pub fn with_timeout(mut self, timeout: std::time::Duration) -> Result<Self> {
        self.client = super::build_http_client(timeout, timeout, timeout)?;
        Ok(self)
    }

    /// Set an application name for request tracking (appears in OpenRouter dashboard)
    pub fn with_app_name(mut self, app_name: impl Into<String>) -> Self {
        self.app_name = Some(app_name.into());
        self
    }

    /// Convert nxusKit messages to OpenRouter API format
    fn convert_messages(&self, messages: &[Message]) -> Vec<OpenRouterMessage> {
        messages
            .iter()
            .map(|msg| {
                let role = match msg.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                };

                let content = match &msg.content {
                    MessageContent::Text(text) => OpenRouterMessageContent::Text(text.clone()),
                    MessageContent::Parts(parts) => {
                        // OpenRouter supports vision through underlying providers
                        let converted_parts: Vec<OpenRouterContentPart> = parts
                            .iter()
                            .map(|part| match part {
                                ContentPart::Text { text } => {
                                    OpenRouterContentPart::Text { text: text.clone() }
                                }
                                ContentPart::Image { source } => {
                                    let url = match &source.data {
                                        ImageData::Url { url } => url.clone(),
                                        ImageData::Base64 { media_type, data } => {
                                            format!("data:{};base64,{}", media_type, data)
                                        }
                                    };
                                    OpenRouterContentPart::ImageUrl {
                                        image_url: OpenRouterImageUrl { url },
                                    }
                                }
                            })
                            .collect();

                        OpenRouterMessageContent::Parts(converted_parts)
                    }
                };

                OpenRouterMessage {
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
                "OpenRouter stop sequences must contain 0 to 4 nonempty strings".into(),
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

        // OpenRouter may route these upstream, but the v2 adapter does not
        // serialize/decode them yet. Fail closed rather than silently dropping.
        if request.logprobs.is_some() || request.top_logprobs.is_some() {
            return Err(NxuskitError::unsupported_capability(
                "openrouter",
                "logprobs",
            ));
        }
        if request.reasoning_effort.is_some() {
            return Err(NxuskitError::unsupported_capability(
                "openrouter",
                "reasoning_effort",
            ));
        }
        if !matches!(
            request.thinking_mode,
            crate::types::ThinkingMode::Auto | crate::types::ThinkingMode::Omit
        ) {
            return Err(NxuskitError::unsupported_capability(
                "openrouter",
                "thinking",
            ));
        }

        if let Some(crate::types::ResponseFormat::JsonSchema { schema }) =
            request.response_format.as_ref()
            && !schema.is_object()
        {
            return Err(NxuskitError::InvalidRequest(
                "OpenRouter JSON Schema must be an object".into(),
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
                    "openrouter",
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
                "openrouter",
                "tool_streaming",
            ));
        }

        if request.openai_responses.is_some() {
            return Err(NxuskitError::unsupported_capability(
                "openrouter",
                "openai_responses",
            ));
        }
        if request.provider_options.is_some() {
            return Err(NxuskitError::unsupported_capability(
                "openrouter",
                "provider_options",
            ));
        }

        Ok(())
    }

    /// Build OpenRouter API request from adapted ChatRequest
    fn build_request(&self, request: &ChatRequest) -> Result<OpenRouterRequest> {
        let record = crate::capabilities::registry::find("openrouter").ok_or_else(|| {
            NxuskitError::Configuration("provider capability record not found: openrouter".into())
        })?;
        let mut validation_warnings = Vec::new();
        super::validate_typed_capability_request("openrouter", request, &mut validation_warnings)?;
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

        let mut openrouter_request = OpenRouterRequest {
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
        // absent. Typed model-dependent pass-through takes precedence.
        if request.structured_output.is_none()
            && let Some(ref format) = request.response_format
        {
            use crate::types::ResponseFormat;
            match format {
                ResponseFormat::Json => {
                    openrouter_request.response_format = Some(OpenRouterResponseFormat {
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
                    openrouter_request.response_format_typed = response_format_typed;
                }
                ResponseFormat::Text => {
                    // Default, no action needed
                }
            }
        }

        Ok(openrouter_request)
    }

    /// Get request headers including optional app name
    fn get_headers(&self) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "Authorization",
            format!("Bearer {}", self.api_key).parse().unwrap(),
        );

        if let Some(ref app_name) = self.app_name {
            headers.insert("X-Title", app_name.parse().unwrap());
        }

        headers
    }
}

impl OpenRouterProvider {
    /// Create a fresh session with no accumulated state.
    ///
    /// For OpenRouterProvider, this returns a clone with the same configuration
    /// since it's already stateless (each request is independent).
    ///
    /// # Returns
    ///
    /// A cloned OpenRouterProvider instance with the same configuration.
    pub fn fresh_session(&self) -> Self {
        self.clone()
    }
}

#[async_trait]
impl LLMProvider for OpenRouterProvider {
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse> {
        self.validate_request_surface(request, false)?;
        // Adapt request to provider capabilities
        let capabilities = self.get_capabilities();
        let mut adapted = ParameterAdapter::adapt(request, &capabilities);
        super::validate_typed_capability_request(
            "openrouter",
            &adapted.request,
            &mut adapted.warnings,
        )?;
        let adapted_request = &adapted.request;

        // Build OpenRouter-specific request
        let openrouter_request = self.build_request(adapted_request)?;

        // Make API call
        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .headers(self.get_headers())
            .json(&openrouter_request)
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

        let data: serde_json::Value =
            crate::provider::http::read_response_json_bounded(response).await?;
        let mut provider_metadata = None;
        super::openai::merge_provider_metadata(&mut provider_metadata, &data, "openrouter")?;
        let openrouter_response: OpenRouterResponse = serde_json::from_value(data)?;

        // Extract content and tool calls from first choice.
        let content = openrouter_response
            .choices
            .first()
            .and_then(|choice| choice.message.content.as_deref())
            .unwrap_or("")
            .to_string();
        let tool_calls = openrouter_response
            .choices
            .first()
            .and_then(|choice| choice.message.tool_calls.clone())
            .filter(|calls| !calls.is_empty());

        // Convert finish reason
        let raw_finish_reason = openrouter_response
            .choices
            .first()
            .and_then(|choice| choice.finish_reason.clone());
        let finish_reason = openrouter_response
            .choices
            .first()
            .and_then(|choice| choice.finish_reason.as_ref())
            .and_then(|reason| match reason.as_str() {
                "stop" => Some(crate::types::FinishReason::Stop),
                "length" => Some(crate::types::FinishReason::Length),
                "content_filter" => Some(crate::types::FinishReason::ContentFilter),
                "tool_calls" => Some(crate::types::FinishReason::ToolCalls),
                _ => None,
            });

        let estimator =
            crate::token_estimator::TokenEstimator::for_model(&openrouter_response.model);
        let estimated = TokenCount::new(
            estimator.count_messages(&adapted_request.messages),
            estimator.count(&content),
        );
        let usage = match openrouter_response.usage.and_then(|usage| usage.actual()) {
            Some(actual) => TokenUsage::with_actual(actual, estimated),
            None => TokenUsage::estimated_only(estimated),
        };
        let mut response = ChatResponse::new(content, openrouter_response.model, usage);
        response.provider = self.provider_name().to_string();
        response.tool_calls = tool_calls;

        response.finish_reason = finish_reason;
        response.raw_finish_reason = raw_finish_reason;
        response.provider_metadata = provider_metadata;
        response.warnings = adapted.warnings;

        // Populate inference metadata
        response.inference_metadata = InferenceMetadata {
            is_complete: true,
            finish_reason: response.finish_reason,
            ..InferenceMetadata::default()
        }
        .with_token_usage(response.usage.clone())
        .with_provider_metadata(serde_json::json!({
            "provider": "openrouter"
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
            "openrouter",
            &adapted.request,
            &mut adapted.warnings,
        )?;
        let adapted_request = &adapted.request;

        // Build OpenRouter-specific request with streaming enabled
        let mut openrouter_request = self.build_request(adapted_request)?;
        openrouter_request.stream = Some(true);

        // Make API call
        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .headers(self.get_headers())
            .json(&openrouter_request)
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

        Ok(super::openai::openai_compatible_stream_with_metadata(
            response,
            request,
            "openrouter",
        ))
    }

    fn provider_name(&self) -> &str {
        "openrouter"
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let response = self
            .client
            .get(format!("{}/models", self.base_url))
            .headers(self.get_headers())
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = crate::provider::http::read_response_text_bounded(response)
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(NxuskitError::provider(status.as_u16(), error_text));
        }

        let models_response: OpenRouterModelsResponse =
            crate::provider::http::read_response_json_bounded(response).await?;

        Ok(models_response
            .data
            .into_iter()
            .map(|model| {
                let mut info = ModelInfo::new(model.id.clone());
                info.description = model.description;
                info.context_window = model.context_length;

                // Add pricing info to metadata
                if let Some(pricing) = model.pricing {
                    if let Some(prompt) = pricing.prompt {
                        info.metadata.insert("price_prompt".to_string(), prompt);
                    }
                    if let Some(completion) = pricing.completion {
                        info.metadata
                            .insert("price_completion".to_string(), completion);
                    }
                }

                info
            })
            .collect())
    }

    fn get_capabilities(&self) -> ProviderCapabilities {
        Self::adapter_capabilities()
    }
}

#[derive(Debug, Serialize)]
struct OpenRouterRequest {
    model: String,
    messages: Vec<OpenRouterMessage>,
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
    response_format: Option<OpenRouterResponseFormat>,
    #[serde(rename = "response_format", skip_serializing_if = "is_json_null")]
    response_format_typed: serde_json::Value,
    #[serde(skip_serializing_if = "is_json_null")]
    tools: serde_json::Value,
    #[serde(skip_serializing_if = "is_json_null")]
    tool_choice: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    parallel_tool_calls: Option<bool>,
}

fn is_json_null(value: &serde_json::Value) -> bool {
    value.is_null()
}

#[derive(Debug, Serialize)]
struct OpenRouterResponseFormat {
    r#type: String,
}

#[derive(Debug, Serialize)]
struct OpenRouterMessage {
    role: String,
    content: OpenRouterMessageContent,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum OpenRouterMessageContent {
    Text(String),
    Parts(Vec<OpenRouterContentPart>),
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum OpenRouterContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: OpenRouterImageUrl },
}

#[derive(Debug, Serialize)]
struct OpenRouterImageUrl {
    url: String,
}

#[derive(Debug, Deserialize)]
struct OpenRouterResponse {
    model: String,
    choices: Vec<OpenRouterChoice>,
    usage: Option<super::openai::SelectedProviderUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterChoice {
    message: OpenRouterResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterResponseMessage {
    #[allow(dead_code)]
    role: String,
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterModelsResponse {
    data: Vec<OpenRouterModel>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterModel {
    id: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    context_length: Option<u32>,
    #[serde(default)]
    pricing: Option<OpenRouterPricing>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterPricing {
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    completion: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_openrouter_capabilities() {
        let provider = OpenRouterProvider::new("test-key");
        let caps = provider.get_capabilities();

        assert!(caps.supports_system_messages);
        assert!(caps.supports_streaming);
        assert!(caps.supports_json_mode);
        assert!(caps.supports_json_schema);
        assert_eq!(caps.max_stop_sequences, Some(4));
        assert!(caps.supports_seed);
    }

    fn assert_openrouter_unsupported(request: &ChatRequest, streaming: bool, capability: &str) {
        let provider = OpenRouterProvider::new("test-key");
        match provider.validate_request_surface(request, streaming) {
            Err(NxuskitError::UnsupportedCapability {
                provider,
                capability: actual,
            }) => {
                assert_eq!(provider, "openrouter");
                assert_eq!(actual, capability);
            }
            other => panic!("expected unsupported {capability}, got {other:?}"),
        }
    }

    #[test]
    fn openrouter_request_surface_fails_closed_before_adaptation() {
        use crate::capabilities::{
            OpenAIResponsesOptions, StructuredOutputConfig, StructuredOutputMode, ToolCallConfig,
            ToolChoice, ToolDefinition,
        };

        let provider = OpenRouterProvider::new("test-key");
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
            assert_openrouter_unsupported(&request, false, capability);
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
            strict: Some(false),
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
        assert_openrouter_unsupported(&legacy_tool, true, "tool_streaming");

        let mut typed_tools = base;
        typed_tools.tool_call_config = Some(ToolCallConfig {
            tools: vec![ToolDefinition {
                name: "lookup".into(),
                description: None,
                parameters: serde_json::json!({"type": "object"}),
                strict: None,
            }],
            tool_choice: ToolChoice::Auto,
            parallel_tool_calls: Some(true),
            streaming_tool_calls: None,
        });
        provider
            .validate_request_surface(&typed_tools, false)
            .unwrap();
        assert_openrouter_unsupported(&typed_tools, true, "tool_streaming");

        let mut streaming_flag = ChatRequest::new("m");
        streaming_flag.tool_call_config = Some(ToolCallConfig {
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
        assert_openrouter_unsupported(&streaming_flag, false, "streaming_tool_calls");
    }

    fn openrouter_fixture_server(
        response_body: &str,
    ) -> (
        OpenRouterProvider,
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

        let provider =
            OpenRouterProvider::new("fixture").with_base_url(format!("http://{address}"));
        (provider, server)
    }

    #[test]
    fn openrouter_legacy_schema_and_tool_wire() {
        let provider = OpenRouterProvider::new("test-key");
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

        let mut request =
            ChatRequest::new("openai/gpt-4o").with_message(Message::user("Look up A-1."));
        request.response_format = Some(crate::types::ResponseFormat::JsonSchema {
            schema: schema.clone(),
        });
        request.tools = Some(vec![tool.clone()]);
        request.tool_choice = Some(serde_json::json!({
            "type": "function",
            "function": {"name": "lookup"}
        }));

        provider.validate_request_surface(&request, false).unwrap();
        let wire = serde_json::to_value(provider.build_request(&request).unwrap()).unwrap();
        assert_eq!(wire["response_format"]["type"], "json_schema");
        assert_eq!(wire["response_format"]["json_schema"]["name"], "response");
        assert_eq!(wire["response_format"]["json_schema"]["strict"], true);
        assert_eq!(wire["response_format"]["json_schema"]["schema"], schema);
        assert_eq!(wire["tools"], serde_json::json!([tool]));
        assert_eq!(wire["tool_choice"]["function"]["name"], "lookup");
    }

    #[test]
    fn openrouter_typed_best_effort_schema_and_tools_wire() {
        use crate::capabilities::{
            StructuredOutputConfig, StructuredOutputMode, ToolCallConfig, ToolChoice,
            ToolDefinition,
        };

        let provider = OpenRouterProvider::new("test-key");
        let mut request =
            ChatRequest::new("openai/gpt-4o").with_message(Message::user("Return structured."));
        request.structured_output = Some(StructuredOutputConfig {
            mode: StructuredOutputMode::JsonSchema,
            schema: Some(serde_json::json!({
                "type": "object",
                "properties": {"sku": {"type": "string"}}
            })),
            schema_name: Some("inventory".into()),
            strict: Some(false),
            schema_subset: None,
        });
        request.tool_call_config = Some(ToolCallConfig {
            tools: vec![ToolDefinition {
                name: "lookup".into(),
                description: Some("Look up inventory.".into()),
                parameters: serde_json::json!({"type": "object"}),
                strict: None,
            }],
            tool_choice: ToolChoice::Named("lookup".into()),
            parallel_tool_calls: Some(false),
            streaming_tool_calls: None,
        });

        let value = serde_json::to_value(provider.build_request(&request).unwrap()).unwrap();
        assert_eq!(value["response_format"]["type"], "json_schema");
        assert_eq!(value["response_format"]["json_schema"]["name"], "inventory");
        assert_eq!(value["response_format"]["json_schema"]["strict"], false);
        assert_eq!(value["tools"][0]["function"]["name"], "lookup");
        assert_eq!(value["tool_choice"]["function"]["name"], "lookup");
        assert_eq!(value["parallel_tool_calls"], false);
    }

    #[test]
    fn openrouter_typed_strict_schema_remains_blocked_without_supported_status() {
        use crate::capabilities::{StructuredOutputConfig, StructuredOutputMode};

        let provider = OpenRouterProvider::new("test-key");
        let mut request = ChatRequest::new("openai/gpt-4o");
        request.structured_output = Some(StructuredOutputConfig {
            mode: StructuredOutputMode::JsonSchema,
            schema: Some(serde_json::json!({"type": "object"})),
            schema_name: Some("strict".into()),
            strict: Some(true),
            schema_subset: None,
        });

        assert!(matches!(
            provider.build_request(&request),
            Err(NxuskitError::InvalidRequest(ref message))
                if message.contains("strict json_schema requires Supported")
        ));
    }

    #[tokio::test]
    async fn openrouter_unary_tool_round_trip() {
        let response_body = serde_json::json!({
            "id": "gen-1",
            "provider": "upstream",
            "model": "openai/gpt-4o",
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
            "usage": {
                "prompt_tokens": 3,
                "completion_tokens": 2,
                "total_tokens": 5,
                "cost": 0.001
            }
        })
        .to_string();
        let (provider, server) = openrouter_fixture_server(&response_body);

        let mut request =
            ChatRequest::new("openai/gpt-4o").with_message(Message::user("Look up A-1."));
        request.tools = Some(vec![serde_json::json!({
            "type": "function",
            "function": {
                "name": "lookup",
                "parameters": {"type": "object"}
            }
        })]);
        request.tool_choice = Some(serde_json::Value::String("auto".into()));

        let response = provider.chat(&request).await.unwrap();
        let wire = server.join().unwrap();
        assert_eq!(wire["tools"][0]["function"]["name"], "lookup");
        assert_eq!(wire["tool_choice"], "auto");
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
        assert_eq!(
            response
                .provider_metadata
                .as_ref()
                .and_then(|meta| meta.get("provider"))
                .and_then(serde_json::Value::as_str),
            Some("upstream")
        );
    }

    #[tokio::test]
    async fn openrouter_unsupported_controls_reject_before_http() {
        let provider = OpenRouterProvider::new("test-key").with_base_url("http://127.0.0.1:1");

        let mut request = ChatRequest::new("m");
        request.logprobs = Some(true);

        assert!(matches!(
            provider.chat(&request).await,
            Err(NxuskitError::UnsupportedCapability {
                ref provider,
                ref capability,
            }) if provider == "openrouter" && capability == "logprobs"
        ));
        assert!(matches!(
            provider.chat_stream(&request).await,
            Err(NxuskitError::UnsupportedCapability {
                ref provider,
                ref capability,
            }) if provider == "openrouter" && capability == "logprobs"
        ));
    }

    mod pro_tests {
        use super::*;
        use crate::Message;

        #[tokio::test]
        async fn test_message_conversion() {
            let provider = OpenRouterProvider::new("test-key");

            let messages = vec![Message::system("You are helpful"), Message::user("Hello")];

            let converted = provider.convert_messages(&messages);
            assert_eq!(converted.len(), 2);
            assert_eq!(converted[0].role, "system");
            assert_eq!(converted[1].role, "user");
        }

        #[tokio::test]
        async fn test_with_app_name() {
            let provider = OpenRouterProvider::new("test-key").with_app_name("MyApp");

            assert_eq!(provider.app_name, Some("MyApp".to_string()));

            let headers = provider.get_headers();
            assert!(headers.contains_key("X-Title"));
        }
    }
}
