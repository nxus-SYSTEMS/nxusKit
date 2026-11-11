//! Mistral AI provider implementation

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

/// Mistral AI provider
///
/// Provides access to Mistral AI's language models via their OpenAI-compatible API.
/// Supports chat completions, streaming, and advanced parameters like JSON mode.
#[derive(Debug, Clone)]
pub struct MistralProvider {
    #[allow(dead_code)]
    api_key: String,
    #[allow(dead_code)]
    base_url: String,
    #[allow(dead_code)]
    client: Client,
}

impl MistralProvider {
    /// Static adapter declarations, not model or endpoint-effective support.
    pub fn adapter_capabilities() -> ProviderCapabilities {
        ProviderCapabilities {
            supports_system_messages: true,
            supports_streaming: true,
            supports_vision: true, // Some Mistral models support vision
            max_stop_sequences: Some(4),
            supports_presence_penalty: true,
            supports_frequency_penalty: true,
            supports_seed: true,
            supports_logprobs: false,

            // T058: Mistral AI does not expose a streaming logprob field.
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

impl MistralProvider {
    /// Create a new Mistral provider with the given API key
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
            base_url: "https://api.mistral.ai/v1".to_string(),
            client,
        }
    }

    /// Set a custom base URL for the Mistral API
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Set the connection, read, and total request timeout.
    pub fn with_timeout(mut self, timeout: std::time::Duration) -> Result<Self> {
        self.client = super::build_http_client(timeout, timeout, timeout)?;
        Ok(self)
    }

    /// Convert nxusKit messages to Mistral API format
    fn convert_messages(&self, messages: &[Message]) -> Vec<MistralMessage> {
        messages
            .iter()
            .map(|msg| {
                let role = match msg.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                };

                let content = match &msg.content {
                    MessageContent::Text(text) => MistralMessageContent::Text(text.clone()),
                    MessageContent::Parts(parts) => {
                        // Mistral supports vision in some models
                        let converted_parts: Vec<MistralContentPart> = parts
                            .iter()
                            .map(|part| match part {
                                ContentPart::Text { text } => {
                                    MistralContentPart::Text { text: text.clone() }
                                }
                                ContentPart::Image { source } => {
                                    let url = match &source.data {
                                        ImageData::Url { url } => url.clone(),
                                        ImageData::Base64 { media_type, data } => {
                                            format!("data:{};base64,{}", media_type, data)
                                        }
                                    };
                                    MistralContentPart::ImageUrl {
                                        image_url: MistralImageUrl { url },
                                    }
                                }
                            })
                            .collect();

                        MistralMessageContent::Parts(converted_parts)
                    }
                };

                MistralMessage {
                    role: role.to_string(),
                    content,
                }
            })
            .collect()
    }

    fn validate_legacy_tool_choice(choice: &serde_json::Value) -> Result<()> {
        let invalid = || NxuskitError::InvalidRequest("invalid tool_choice".into());
        if let Some(value) = choice.as_str() {
            return if matches!(value, "auto" | "none" | "any" | "required") {
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
                "Mistral stop sequences must contain 0 to 4 nonempty strings".into(),
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
            return Err(NxuskitError::unsupported_capability("mistral", "logprobs"));
        }
        if request.reasoning_effort.is_some() {
            return Err(NxuskitError::unsupported_capability(
                "mistral",
                "reasoning_effort",
            ));
        }
        if !matches!(
            request.thinking_mode,
            crate::types::ThinkingMode::Auto | crate::types::ThinkingMode::Omit
        ) {
            return Err(NxuskitError::unsupported_capability("mistral", "thinking"));
        }

        if let Some(crate::types::ResponseFormat::JsonSchema { schema }) =
            request.response_format.as_ref()
            && !schema.is_object()
        {
            return Err(NxuskitError::InvalidRequest(
                "Mistral JSON Schema must be an object".into(),
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
                    "mistral",
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
                "mistral",
                "tool_streaming",
            ));
        }

        if request.openai_responses.is_some() {
            return Err(NxuskitError::unsupported_capability(
                "mistral",
                "openai_responses",
            ));
        }
        if request.provider_options.is_some() {
            return Err(NxuskitError::unsupported_capability(
                "mistral",
                "provider_options",
            ));
        }

        Ok(())
    }

    /// Build Mistral API request from adapted ChatRequest
    fn build_request(&self, request: &ChatRequest) -> Result<MistralRequest> {
        let record = crate::capabilities::registry::find("mistral").ok_or_else(|| {
            NxuskitError::Configuration("provider capability record not found: mistral".into())
        })?;
        let mut validation_warnings = Vec::new();
        super::validate_typed_capability_request("mistral", request, &mut validation_warnings)?;
        let messages = self.convert_messages(&request.messages);

        // Splice typed Phase 4 surfaces first; they take precedence over
        // the legacy `ChatRequest::response_format` enum.
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

        let mut mistral_request = MistralRequest {
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
                    mistral_request.response_format = Some(MistralResponseFormat {
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
                    mistral_request.response_format_typed = response_format_typed;
                }
                ResponseFormat::Text => {}
            }
        }

        Ok(mistral_request)
    }
}

impl MistralProvider {
    /// Create a fresh session with no accumulated state.
    ///
    /// For MistralProvider, this returns a clone with the same configuration
    /// since it's already stateless (each request is independent).
    ///
    /// # Returns
    ///
    /// A cloned MistralProvider instance with the same configuration.
    pub fn fresh_session(&self) -> Self {
        self.clone()
    }
}

/// Enrich model info with descriptions for known models
fn enrich_mistral_model(info: &mut ModelInfo) {
    let model_id = &info.name;

    // Match by model family patterns
    let (description, context_window) = if model_id.contains("large") {
        if model_id.contains("2") {
            (
                Some(
                    "Mistral Large 2: Flagship model with top-tier reasoning, mathematics, and 128k context",
                ),
                Some(128_000),
            )
        } else {
            (
                Some(
                    "Mistral Large: Premier model with advanced reasoning and multilingual capabilities",
                ),
                Some(128_000),
            )
        }
    } else if model_id.contains("medium") {
        (
            Some(
                "Mistral Medium: Balanced model pushing efficiency and usability for various tasks",
            ),
            Some(32_768),
        )
    } else if model_id.contains("small") {
        if model_id.contains("3") {
            (
                Some(
                    "Mistral Small 3: Versatile model for programming, math, document understanding, and dialogue",
                ),
                Some(32_768),
            )
        } else {
            (
                Some(
                    "Mistral Small: Refined intermediary between open-weight and flagship, lower latency than Mixtral",
                ),
                Some(32_768),
            )
        }
    } else if model_id.contains("mixtral") || model_id.contains("Mixtral") {
        if model_id.contains("8x22") {
            (
                Some("Mixtral 8x22B: Large open-source MoE with 176B params, Apache license"),
                Some(64_000),
            )
        } else if model_id.contains("8x7") {
            (
                Some(
                    "Mixtral 8x7B: Efficient open-source MoE, excellent for multilingual and mathematical tasks",
                ),
                Some(32_768),
            )
        } else {
            (
                Some("Mixtral: Mistral's sparse Mixture of Experts model (Apache/open-source)"),
                Some(32_768),
            )
        }
    } else if model_id.contains("codestral") || model_id.contains("Codestral") {
        (
            Some("Codestral: Cutting-edge language model specialized for coding tasks"),
            Some(32_768),
        )
    } else if model_id.contains("pixtral") || model_id.contains("Pixtral") {
        (
            Some("Pixtral: Multimodal frontier model for vision and language understanding"),
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
impl LLMProvider for MistralProvider {
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse> {
        self.validate_request_surface(request, false)?;
        // Adapt request to provider capabilities
        let capabilities = self.get_capabilities();
        let mut adapted = ParameterAdapter::adapt(request, &capabilities);
        super::validate_typed_capability_request(
            "mistral",
            &adapted.request,
            &mut adapted.warnings,
        )?;
        let adapted_request = &adapted.request;

        // Build Mistral-specific request
        let mistral_request = self.build_request(adapted_request)?;

        // Make API call
        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&mistral_request)
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

        let mistral_response: MistralResponse =
            crate::provider::http::read_response_json_bounded(response).await?;

        // Extract content and tool calls from first choice.
        let content = mistral_response
            .choices
            .first()
            .and_then(|choice| choice.message.content.as_deref())
            .unwrap_or("")
            .to_string();
        let tool_calls = mistral_response
            .choices
            .first()
            .and_then(|choice| choice.message.tool_calls.clone())
            .filter(|calls| !calls.is_empty());

        // Convert finish reason
        let raw_finish_reason = mistral_response
            .choices
            .first()
            .and_then(|choice| choice.finish_reason.clone());
        let finish_reason = mistral_response
            .choices
            .first()
            .and_then(|choice| choice.finish_reason.as_ref())
            .and_then(|reason| match reason.as_str() {
                "stop" => Some(crate::types::FinishReason::Stop),
                "length" => Some(crate::types::FinishReason::Length),
                "model_length" => Some(crate::types::FinishReason::Length),
                "tool_calls" => Some(crate::types::FinishReason::ToolCalls),
                _ => None,
            });

        let mut response = ChatResponse::new(
            content,
            mistral_response.model,
            TokenUsage::estimated_only(TokenCount::new(
                mistral_response.usage.prompt_tokens,
                mistral_response.usage.completion_tokens,
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
            "provider": "mistral"
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
            "mistral",
            &adapted.request,
            &mut adapted.warnings,
        )?;
        let adapted_request = &adapted.request;

        // Build Mistral-specific request with streaming enabled
        let mut mistral_request = self.build_request(adapted_request)?;
        mistral_request.stream = Some(true);

        // Make API call
        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&mistral_request)
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
        "mistral"
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

        let models_response: MistralModelsResponse =
            crate::provider::http::read_response_json_bounded(response).await?;

        Ok(models_response
            .data
            .into_iter()
            .map(|model| {
                let mut info = ModelInfo::new(model.id.clone());
                info.metadata
                    .insert("created".to_string(), model.created.to_string());

                // Enrich with descriptions and metadata for known models
                enrich_mistral_model(&mut info);

                info
            })
            .collect())
    }

    fn get_capabilities(&self) -> ProviderCapabilities {
        Self::adapter_capabilities()
    }
}

#[derive(Debug, Serialize)]
struct MistralRequest {
    model: String,
    messages: Vec<MistralMessage>,
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
    /// Legacy response_format (json_object only). When typed
    /// `response_format_typed` is set it takes precedence and this field is
    /// left `None` to avoid double-emitting the wire key.
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<MistralResponseFormat>,
    /// Typed structured-output payload (Phase 4 / T036). Spliced from
    /// [`crate::types::ChatRequest::structured_output`] via
    /// [`crate::capabilities::openai_wire::response_format`]. When set,
    /// renames to `response_format` on the wire (the legacy field above
    /// is omitted when the typed path is active).
    #[serde(rename = "response_format", skip_serializing_if = "is_json_null")]
    response_format_typed: serde_json::Value,
    /// Typed tools payload (Phase 4 / T036).
    #[serde(skip_serializing_if = "is_json_null")]
    tools: serde_json::Value,
    /// Typed tool-choice payload (Phase 4 / T036).
    #[serde(skip_serializing_if = "is_json_null")]
    tool_choice: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    parallel_tool_calls: Option<bool>,
}

fn is_json_null(v: &serde_json::Value) -> bool {
    v.is_null()
}

#[derive(Debug, Serialize)]
struct MistralResponseFormat {
    r#type: String,
}

#[derive(Debug, Serialize)]
struct MistralMessage {
    role: String,
    content: MistralMessageContent,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum MistralMessageContent {
    Text(String),
    Parts(Vec<MistralContentPart>),
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum MistralContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: MistralImageUrl },
}

#[derive(Debug, Serialize)]
struct MistralImageUrl {
    url: String,
}

#[derive(Debug, Deserialize)]
struct MistralResponse {
    #[allow(dead_code)]
    id: String,
    model: String,
    choices: Vec<MistralChoice>,
    usage: MistralUsage,
}

#[derive(Debug, Deserialize)]
struct MistralChoice {
    message: MistralResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MistralResponseMessage {
    #[allow(dead_code)]
    role: String,
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
struct MistralUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    #[allow(dead_code)]
    total_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct MistralModelsResponse {
    data: Vec<MistralModel>,
}

#[derive(Debug, Deserialize)]
struct MistralModel {
    id: String,
    created: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mistral_capabilities() {
        let provider = MistralProvider::new("test-key");
        let caps = provider.get_capabilities();

        assert!(caps.supports_system_messages);
        assert!(caps.supports_streaming);
        assert!(caps.supports_json_mode);
        assert!(caps.supports_json_schema);
        assert_eq!(caps.max_stop_sequences, Some(4));
        assert!(caps.supports_seed);
        assert!(!caps.supports_logprobs);
    }

    fn assert_mistral_unsupported(request: &ChatRequest, capability: &str) {
        let provider = MistralProvider::new("test-key");
        match provider.validate_request_surface(request, false) {
            Err(NxuskitError::UnsupportedCapability {
                provider,
                capability: actual,
            }) => {
                assert_eq!(provider, "mistral");
                assert_eq!(actual, capability);
            }
            other => panic!("expected unsupported {capability}, got {other:?}"),
        }
    }

    #[test]
    fn mistral_request_surface_fails_closed_before_adaptation() {
        use crate::capabilities::{
            OpenAIResponsesOptions, StructuredOutputConfig, StructuredOutputMode, ToolCallConfig,
            ToolChoice, ToolDefinition,
        };

        let provider = MistralProvider::new("test-key");
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
            assert_mistral_unsupported(&request, capability);
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
            }) if provider == "mistral" && capability == "tool_streaming"
        ));

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
        assert!(matches!(
            provider.validate_request_surface(&typed_tools, true),
            Err(NxuskitError::UnsupportedCapability {
                ref provider,
                ref capability,
            }) if provider == "mistral" && capability == "tool_streaming"
        ));

        let mut streaming_tool_flag = ChatRequest::new("m");
        streaming_tool_flag.tool_call_config = Some(ToolCallConfig {
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
            provider.validate_request_surface(&streaming_tool_flag, false),
            Err(NxuskitError::UnsupportedCapability {
                ref provider,
                ref capability,
            }) if provider == "mistral" && capability == "streaming_tool_calls"
        ));
    }

    fn mistral_fixture_server(
        response_body: &str,
    ) -> (MistralProvider, std::thread::JoinHandle<serde_json::Value>) {
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

        let provider = MistralProvider::new("fixture").with_base_url(format!("http://{address}"));
        (provider, server)
    }

    #[test]
    fn mistral_legacy_schema_and_tool_wire() {
        let provider = MistralProvider::new("test-key");
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
            ChatRequest::new("mistral-large-latest").with_message(Message::user("Look up A-1."));
        request.response_format = Some(crate::types::ResponseFormat::JsonSchema {
            schema: schema.clone(),
        });
        request.tools = Some(vec![tool.clone()]);
        request.tool_choice = Some(serde_json::Value::String("auto".into()));

        provider.validate_request_surface(&request, false).unwrap();
        let wire = serde_json::to_value(provider.build_request(&request).unwrap()).unwrap();
        assert_eq!(wire["response_format"]["type"], "json_schema");
        assert_eq!(wire["response_format"]["json_schema"]["name"], "response");
        assert_eq!(wire["response_format"]["json_schema"]["strict"], true);
        assert_eq!(wire["response_format"]["json_schema"]["schema"], schema);
        assert_eq!(wire["tools"], serde_json::json!([tool]));
        assert_eq!(wire["tool_choice"], "auto");
    }

    #[tokio::test]
    async fn mistral_unary_tool_round_trip() {
        let response_body = serde_json::json!({
            "id": "fixture",
            "model": "mistral-large-latest",
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
        let (provider, server) = mistral_fixture_server(&response_body);

        let mut request =
            ChatRequest::new("mistral-large-latest").with_message(Message::user("Look up A-1."));
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
    }

    #[tokio::test]
    async fn mistral_unsupported_controls_reject_before_http() {
        let provider = MistralProvider::new("test-key").with_base_url("http://127.0.0.1:1");

        let mut request = ChatRequest::new("m");
        request.logprobs = Some(true);

        assert!(matches!(
            provider.chat(&request).await,
            Err(NxuskitError::UnsupportedCapability {
                ref provider,
                ref capability,
            }) if provider == "mistral" && capability == "logprobs"
        ));
        assert!(matches!(
            provider.chat_stream(&request).await,
            Err(NxuskitError::UnsupportedCapability {
                ref provider,
                ref capability,
            }) if provider == "mistral" && capability == "logprobs"
        ));
    }

    mod pro_tests {
        use super::*;
        use crate::Message;

        #[tokio::test]
        async fn test_message_conversion() {
            let provider = MistralProvider::new("test-key");

            let messages = vec![Message::system("You are helpful"), Message::user("Hello")];

            let converted = provider.convert_messages(&messages);
            assert_eq!(converted.len(), 2);
            assert_eq!(converted[0].role, "system");
            assert_eq!(converted[1].role, "user");
        }

        // -----------------------------------------------------------------
        // T036: typed structured-output and tool-call serialization through
        // the Mistral adapter. Each test asserts on the actual request bytes.
        // -----------------------------------------------------------------

        #[test]
        fn test_build_request_serializes_typed_structured_output_json_schema() {
            use crate::capabilities::{StructuredOutputConfig, StructuredOutputMode};

            let provider = MistralProvider::new("test-key");
            let mut request = ChatRequest::new("mistral-large-latest")
                .with_message(Message::user("Return a country/capital pair."));
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

            let mistral_request = provider.build_request(&request).unwrap();
            let value = serde_json::to_value(&mistral_request).unwrap();
            assert_eq!(value["response_format"]["type"], "json_schema");
            assert_eq!(
                value["response_format"]["json_schema"]["name"],
                "country_capital"
            );
            assert_eq!(value["response_format"]["json_schema"]["strict"], true);
            // Tool fields omitted when no tool_call_config.
            assert!(value.get("tools").is_none());
            assert!(value.get("tool_choice").is_none());
        }

        #[test]
        fn test_build_request_serializes_typed_structured_output_json_object() {
            use crate::capabilities::{StructuredOutputConfig, StructuredOutputMode};

            let provider = MistralProvider::new("test-key");
            let mut request = ChatRequest::new("mistral-large-latest")
                .with_message(Message::user("Reply with JSON."));
            request.structured_output = Some(StructuredOutputConfig {
                mode: StructuredOutputMode::JsonObject,
                schema: None,
                schema_name: None,
                strict: None,
                schema_subset: None,
            });
            let mistral_request = provider.build_request(&request).unwrap();
            let value = serde_json::to_value(&mistral_request).unwrap();
            assert_eq!(value["response_format"]["type"], "json_object");
        }

        #[test]
        fn test_build_request_serializes_typed_tools_and_tool_choice() {
            use crate::capabilities::{ToolCallConfig, ToolChoice, ToolDefinition};

            let provider = MistralProvider::new("test-key");
            let mut request = ChatRequest::new("mistral-large-latest")
                .with_message(Message::user("Call a tool."));
            request.tool_call_config = Some(ToolCallConfig {
                tools: vec![ToolDefinition {
                    name: "search_inventory".into(),
                    description: Some("Search the warehouse inventory.".into()),
                    parameters: serde_json::json!({"type": "object"}),
                    strict: Some(true),
                }],
                tool_choice: ToolChoice::Auto,
                parallel_tool_calls: Some(true),
                streaming_tool_calls: None,
            });
            let mistral_request = provider.build_request(&request).unwrap();
            let value = serde_json::to_value(&mistral_request).unwrap();
            let tools = value["tools"].as_array().expect("tools array");
            assert_eq!(tools.len(), 1);
            assert_eq!(tools[0]["function"]["name"], "search_inventory");
            assert_eq!(value["tool_choice"], "auto");
            assert_eq!(value["parallel_tool_calls"], true);
        }

        #[test]
        fn test_build_request_typed_takes_precedence_over_legacy_response_format() {
            use crate::capabilities::{StructuredOutputConfig, StructuredOutputMode};
            use crate::types::ResponseFormat;

            let provider = MistralProvider::new("test-key");
            let mut request = ChatRequest::new("mistral-large-latest")
                .with_message(Message::user("Reply with JSON."));
            // Legacy JSON-mode AND typed JsonSchema set simultaneously —
            // typed must win and the wire `response_format` must NOT be
            // double-emitted as the legacy `{"type":"json_object"}` shape.
            request.response_format = Some(ResponseFormat::Json);
            request.structured_output = Some(StructuredOutputConfig {
                mode: StructuredOutputMode::JsonSchema,
                schema: Some(serde_json::json!({"type": "object"})),
                schema_name: Some("typed_wins".into()),
                strict: Some(true),
                schema_subset: None,
            });
            let mistral_request = provider.build_request(&request).unwrap();
            let value = serde_json::to_value(&mistral_request).unwrap();
            assert_eq!(value["response_format"]["type"], "json_schema");
            assert_eq!(
                value["response_format"]["json_schema"]["name"],
                "typed_wins"
            );
        }

        #[test]
        fn test_build_request_omits_typed_fields_when_unset() {
            let provider = MistralProvider::new("test-key");
            let request =
                ChatRequest::new("mistral-large-latest").with_message(Message::user("Plain chat."));
            let mistral_request = provider.build_request(&request).unwrap();
            let value = serde_json::to_value(&mistral_request).unwrap();
            assert!(value.get("response_format").is_none());
            assert!(value.get("tools").is_none());
            assert!(value.get("tool_choice").is_none());
            assert!(value.get("parallel_tool_calls").is_none());
        }
    }
}
