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
use crate::types::{
    ChatRequest, ChatResponse, ContentPart, ImageData, InferenceMetadata, MessageContent,
    ModelInfo, ProviderCapabilities, Role, StreamChunk, TokenCount, TokenUsage,
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
    /// Static adapter declarations, not model or endpoint-effective support.
    pub fn adapter_capabilities() -> ProviderCapabilities {
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
            supports_json_schema: true,
            penalty_range: Some((-2.0, 2.0)),
            max_logprobs: None,
        }
    }
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

    fn convert_messages(&self, messages: &[crate::types::Message]) -> Vec<LmStudioMessage> {
        messages
            .iter()
            .map(|msg| {
                let role = match msg.role {
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::System => "system",
                };

                let content = match &msg.content {
                    MessageContent::Text(text) => LmStudioMessageContent::Text(text.clone()),
                    MessageContent::Parts(parts) => {
                        let converted = parts
                            .iter()
                            .map(|part| match part {
                                ContentPart::Text { text } => {
                                    LmStudioContentPart::Text { text: text.clone() }
                                }
                                ContentPart::Image { source } => {
                                    let url = match &source.data {
                                        ImageData::Url { url } => url.clone(),
                                        ImageData::Base64 { media_type, data } => {
                                            format!("data:{media_type};base64,{data}")
                                        }
                                    };
                                    LmStudioContentPart::ImageUrl {
                                        image_url: LmStudioImageUrl { url },
                                    }
                                }
                            })
                            .collect();
                        LmStudioMessageContent::Parts(converted)
                    }
                };

                LmStudioMessage {
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

    fn validate_request_surface(&self, request: &ChatRequest) -> Result<()> {
        request.validate_legacy_sampling(self.provider_name())?;

        if let Some(stops) = request.stop.as_ref()
            && (stops.len() > 4 || stops.iter().any(String::is_empty))
        {
            return Err(NxuskitError::InvalidRequest(
                "LM Studio stop sequences must contain 0 to 4 nonempty strings".into(),
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
                "LM Studio JSON Schema must be an object".into(),
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
                    "lmstudio",
                    "parallel_tool_calls",
                ));
            }
            if config.streaming_tool_calls.is_some() {
                return Err(NxuskitError::unsupported_capability(
                    "lmstudio",
                    "streaming_tool_calls",
                ));
            }
        }

        for (capability, present) in [
            ("logprobs", request.logprobs.is_some()),
            ("top_logprobs", request.top_logprobs.is_some()),
            ("reasoning_effort", request.reasoning_effort.is_some()),
        ] {
            if present {
                return Err(NxuskitError::unsupported_capability("lmstudio", capability));
            }
        }

        if !matches!(
            request.thinking_mode,
            crate::types::ThinkingMode::Auto | crate::types::ThinkingMode::Omit
        ) {
            return Err(NxuskitError::unsupported_capability("lmstudio", "thinking"));
        }
        if request.openai_responses.is_some() {
            return Err(NxuskitError::unsupported_capability(
                "lmstudio",
                "openai_responses",
            ));
        }
        if request.provider_options.is_some() {
            return Err(NxuskitError::unsupported_capability(
                "lmstudio",
                "provider_options",
            ));
        }

        Ok(())
    }

    fn build_chat_request(
        &self,
        request: &ChatRequest,
        streaming: bool,
    ) -> Result<LmStudioChatRequest> {
        let record = crate::capabilities::registry::find("lmstudio").ok_or_else(|| {
            NxuskitError::Configuration("provider capability record not found: lmstudio".into())
        })?;
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

        let mut lms_request = LmStudioChatRequest {
            model: request.model.clone(),
            messages: self.convert_messages(&request.messages),
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            top_p: request.top_p,
            stop: request.stop.clone(),
            presence_penalty: request.presence_penalty,
            frequency_penalty: request.frequency_penalty,
            seed: request.seed,
            response_format: None,
            response_format_typed,
            tools,
            tool_choice,
            stream: Some(streaming),
        };

        if request.structured_output.is_none()
            && let Some(ref format) = request.response_format
        {
            match format {
                crate::types::ResponseFormat::Json => {
                    lms_request.response_format = Some(LmStudioResponseFormat {
                        r#type: "json_object".into(),
                    });
                }
                crate::types::ResponseFormat::JsonSchema { schema } => {
                    response_format_typed = serde_json::json!({
                        "type": "json_schema",
                        "json_schema": {
                            "name": "response",
                            "strict": true,
                            "schema": schema
                        }
                    });
                    lms_request.response_format_typed = response_format_typed;
                }
                crate::types::ResponseFormat::Text => {}
            }
        }

        Ok(lms_request)
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
            let error_text = crate::provider::http::read_response_text_bounded(response)
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(NxuskitError::provider(status, error_text));
        }

        let models_response: LmStudioModelsResponse =
            crate::provider::http::read_response_json_bounded(response).await?;

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
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    presence_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    frequency_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<LmStudioResponseFormat>,
    #[serde(rename = "response_format", skip_serializing_if = "is_json_null")]
    response_format_typed: serde_json::Value,
    #[serde(skip_serializing_if = "is_json_null")]
    tools: serde_json::Value,
    #[serde(skip_serializing_if = "is_json_null")]
    tool_choice: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

fn is_json_null(value: &serde_json::Value) -> bool {
    value.is_null()
}

#[derive(Debug, Serialize)]
struct LmStudioResponseFormat {
    r#type: String,
}

#[derive(Debug, Serialize)]
struct LmStudioMessage {
    role: String,
    content: LmStudioMessageContent,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum LmStudioMessageContent {
    Text(String),
    Parts(Vec<LmStudioContentPart>),
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum LmStudioContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: LmStudioImageUrl },
}

#[derive(Debug, Serialize)]
struct LmStudioImageUrl {
    url: String,
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
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)] // total_tokens is in API response but we compute from parts
struct LmStudioUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[async_trait]
impl LLMProvider for LmStudioProvider {
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse> {
        self.validate_request_surface(request)?;
        // Adapt request to provider capabilities.
        let capabilities = self.get_capabilities();
        let mut adapted = ParameterAdapter::adapt(request, &capabilities);
        super::validate_typed_capability_request(
            "lmstudio",
            &adapted.request,
            &mut adapted.warnings,
        )?;
        let adapted_request = &adapted.request;

        let url = format!("{}/chat/completions", self.base_url);
        let lms_request = self.build_chat_request(adapted_request, false)?;

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
            let retry_after = super::parse_retry_after(response.headers());
            let error_text = crate::provider::http::read_response_text_bounded(response)
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
                429 => NxuskitError::rate_limit(retry_after),
                _ => NxuskitError::provider(status.as_u16(), error_text),
            });
        }

        let lms_response: LmStudioChatResponse =
            crate::provider::http::read_response_json_bounded(response)
                .await
                .map_err(|e| {
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

        let tool_calls = choice
            .message
            .tool_calls
            .clone()
            .filter(|calls| !calls.is_empty());

        let mut response = ChatResponse::new(
            choice.message.content.clone().unwrap_or_default(),
            lms_response
                .model
                .unwrap_or_else(|| adapted_request.model.clone()),
            usage,
        );
        response.provider = self.provider_name().to_string();
        response.tool_calls = tool_calls;

        response.raw_finish_reason = choice.finish_reason.clone();
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
        response.inference_metadata = InferenceMetadata {
            is_complete: true,
            finish_reason: response.finish_reason,
            ..InferenceMetadata::default()
        }
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
        self.validate_request_surface(request)?;
        if request
            .tools
            .as_ref()
            .is_some_and(|tools| !tools.is_empty())
            || request.tool_choice.is_some()
            || request.tool_call_config.is_some()
        {
            return Err(NxuskitError::unsupported_capability(
                "lmstudio",
                "tool_streaming",
            ));
        }
        let capabilities = self.get_capabilities();
        let mut adapted = ParameterAdapter::adapt(request, &capabilities);
        super::validate_typed_capability_request(
            "lmstudio",
            &adapted.request,
            &mut adapted.warnings,
        )?;
        let adapted_request = &adapted.request;
        let url = format!("{}/chat/completions", self.base_url);
        let lms_request = self.build_chat_request(adapted_request, true)?;

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
        "lmstudio"
    }

    fn get_capabilities(&self) -> ProviderCapabilities {
        Self::adapter_capabilities()
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
    fn lmstudio_request_core_wire_preserves_supported_controls_and_vision() {
        use crate::capabilities::{StructuredOutputConfig, StructuredOutputMode};

        let provider = LmStudioProvider::builder()
            .model("vision-model")
            .build()
            .unwrap();
        let mut request = ChatRequest::new("vision-model").with_message(
            crate::types::Message::user("Describe both images.")
                .with_image_url("https://example.invalid/a.png")
                .with_image_base64("ZmFrZQ==", "image/png"),
        );
        request.temperature = Some(0.4);
        request.max_tokens = Some(123);
        request.top_p = Some(0.8);
        request.stop = Some(vec!["a".into(), "b".into(), "c".into(), "d".into()]);
        request.presence_penalty = Some(-0.5);
        request.frequency_penalty = Some(0.5);
        request.seed = Some(42);
        request.response_format = Some(crate::types::ResponseFormat::Json);

        provider.validate_request_surface(&request).unwrap();
        let wire =
            serde_json::to_value(provider.build_chat_request(&request, false).unwrap()).unwrap();

        assert_eq!(wire["model"], "vision-model");
        assert!((wire["temperature"].as_f64().unwrap() - 0.4).abs() < 1e-6);
        assert_eq!(wire["max_tokens"], 123);
        assert!((wire["top_p"].as_f64().unwrap() - 0.8).abs() < 1e-6);
        assert_eq!(wire["stop"], serde_json::json!(["a", "b", "c", "d"]));
        assert!((wire["presence_penalty"].as_f64().unwrap() + 0.5).abs() < 1e-6);
        assert!((wire["frequency_penalty"].as_f64().unwrap() - 0.5).abs() < 1e-6);
        assert_eq!(wire["seed"], 42);
        assert_eq!(
            wire["response_format"],
            serde_json::json!({"type":"json_object"})
        );
        assert_eq!(wire["stream"], false);
        assert_eq!(wire["messages"][0]["content"][0]["type"], "text");
        assert_eq!(
            wire["messages"][0]["content"][1]["image_url"]["url"],
            "https://example.invalid/a.png"
        );
        assert_eq!(
            wire["messages"][0]["content"][2]["image_url"]["url"],
            "data:image/png;base64,ZmFrZQ=="
        );

        let stream_wire =
            serde_json::to_value(provider.build_chat_request(&request, true).unwrap()).unwrap();
        assert_eq!(stream_wire["stream"], true);
        assert_eq!(stream_wire["stop"], wire["stop"]);
        assert_eq!(stream_wire["messages"], wire["messages"]);

        let mut typed = ChatRequest::new("vision-model");
        typed.structured_output = Some(StructuredOutputConfig {
            mode: StructuredOutputMode::JsonObject,
            schema: None,
            schema_name: None,
            strict: None,
            schema_subset: None,
        });
        provider.validate_request_surface(&typed).unwrap();
        let typed_wire =
            serde_json::to_value(provider.build_chat_request(&typed, false).unwrap()).unwrap();
        assert_eq!(
            typed_wire["response_format"],
            serde_json::json!({"type":"json_object"})
        );
    }

    fn assert_lmstudio_unsupported(request: &ChatRequest, capability: &str) {
        let provider = LmStudioProvider::builder().model("m").build().unwrap();
        match provider.validate_request_surface(request) {
            Err(NxuskitError::UnsupportedCapability {
                provider,
                capability: actual,
            }) => {
                assert_eq!(provider, "lmstudio");
                assert_eq!(actual, capability);
            }
            other => panic!("expected unsupported {capability}, got {other:?}"),
        }
    }

    #[test]
    fn lmstudio_request_surface_fails_closed_without_silent_drop() {
        use crate::capabilities::{
            StructuredOutputConfig, StructuredOutputMode, ToolCallConfig, ToolChoice,
            ToolDefinition,
        };

        let base = ChatRequest::new("m");

        let mut invalid_stop = base.clone();
        invalid_stop.stop = Some(vec![
            "a".into(),
            "b".into(),
            "c".into(),
            "d".into(),
            "e".into(),
        ]);
        let provider = LmStudioProvider::builder().model("m").build().unwrap();
        assert!(matches!(
            provider.validate_request_surface(&invalid_stop),
            Err(NxuskitError::InvalidRequest(_))
        ));

        let mut invalid_schema = base.clone();
        invalid_schema.response_format = Some(crate::types::ResponseFormat::JsonSchema {
            schema: serde_json::json!([]),
        });
        assert!(matches!(
            provider.validate_request_surface(&invalid_schema),
            Err(NxuskitError::InvalidRequest(_))
        ));

        let mut schema = base.clone();
        schema.response_format = Some(crate::types::ResponseFormat::JsonSchema {
            schema: serde_json::json!({"type":"object"}),
        });
        provider.validate_request_surface(&schema).unwrap();

        let mut typed_schema = base.clone();
        typed_schema.structured_output = Some(StructuredOutputConfig {
            mode: StructuredOutputMode::JsonSchema,
            schema: Some(serde_json::json!({"type":"object"})),
            schema_name: Some("response".into()),
            strict: Some(true),
            schema_subset: None,
        });
        provider.validate_request_surface(&typed_schema).unwrap();

        let mut malformed_tool = base.clone();
        malformed_tool.tools = Some(vec![serde_json::json!({
            "type":"function",
            "function":{"name":"","parameters":{}}
        })]);
        assert!(matches!(
            provider.validate_request_surface(&malformed_tool),
            Err(NxuskitError::InvalidRequest(_))
        ));

        let mut tools = base.clone();
        tools.tools = Some(vec![serde_json::json!({
            "type":"function",
            "function":{"name":"lookup","parameters":{"type":"object"}}
        })]);
        provider.validate_request_surface(&tools).unwrap();

        let mut typed_tools = base.clone();
        typed_tools.tool_call_config = Some(ToolCallConfig {
            tools: vec![ToolDefinition {
                name: "lookup".into(),
                description: None,
                parameters: serde_json::json!({"type":"object"}),
                strict: None,
            }],
            tool_choice: ToolChoice::Auto,
            parallel_tool_calls: None,
            streaming_tool_calls: None,
        });
        provider.validate_request_surface(&typed_tools).unwrap();

        for (capability, request) in [
            ("logprobs", {
                let mut request = base.clone();
                request.logprobs = Some(true);
                request
            }),
            ("top_logprobs", {
                let mut request = base.clone();
                request.top_logprobs = Some(3);
                request
            }),
            ("reasoning_effort", {
                let mut request = base.clone();
                request.reasoning_effort = Some("low".into());
                request
            }),
            ("thinking", {
                let mut request = base;
                request.thinking_mode = crate::types::ThinkingMode::Enabled;
                request
            }),
        ] {
            assert_lmstudio_unsupported(&request, capability);
        }
    }

    #[tokio::test]
    async fn lmstudio_unsupported_controls_reject_before_http() {
        let provider = LmStudioProvider::builder()
            .model("m")
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
            }) if provider == "lmstudio" && capability == "logprobs"
        ));
        assert!(matches!(
            provider.chat_stream(&request).await,
            Err(NxuskitError::UnsupportedCapability {
                ref provider,
                ref capability,
            }) if provider == "lmstudio" && capability == "logprobs"
        ));
    }

    fn lmstudio_fixture_server(
        response_body: &str,
    ) -> (LmStudioProvider, std::thread::JoinHandle<serde_json::Value>) {
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

        let provider = LmStudioProvider::builder()
            .model("local-model")
            .base_url(format!("http://{address}"))
            .build()
            .unwrap();
        (provider, server)
    }

    #[test]
    fn lmstudio_typed_schema_wire() {
        use crate::capabilities::{StructuredOutputConfig, StructuredOutputMode};

        let provider = LmStudioProvider::builder()
            .model("local-model")
            .build()
            .unwrap();
        let schema = serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["sku"],
            "properties": {"sku": {"type": "string"}}
        });
        let mut request = ChatRequest::new("local-model")
            .with_message(crate::types::Message::user("Return an inventory record."));
        request.structured_output = Some(StructuredOutputConfig {
            mode: StructuredOutputMode::JsonSchema,
            schema: Some(schema.clone()),
            schema_name: Some("inventory_record".into()),
            strict: Some(true),
            schema_subset: None,
        });

        provider.validate_request_surface(&request).unwrap();
        let wire =
            serde_json::to_value(provider.build_chat_request(&request, false).unwrap()).unwrap();
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

        let mut legacy = ChatRequest::new("local-model");
        legacy.response_format = Some(crate::types::ResponseFormat::JsonSchema {
            schema: schema.clone(),
        });
        provider.validate_request_surface(&legacy).unwrap();
        let legacy_wire =
            serde_json::to_value(provider.build_chat_request(&legacy, false).unwrap()).unwrap();
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
    async fn lmstudio_unary_tool_round_trip() {
        use crate::capabilities::{ToolCallConfig, ToolChoice, ToolDefinition};

        let response_body = serde_json::json!({
            "model": "local-model",
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
        let (provider, server) = lmstudio_fixture_server(&response_body);

        let mut request = ChatRequest::new("local-model")
            .with_message(crate::types::Message::user("Look up A-1."));
        request.tool_call_config = Some(ToolCallConfig {
            tools: vec![ToolDefinition {
                name: "lookup".into(),
                description: Some("Look up inventory.".into()),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {"sku": {"type": "string"}},
                    "required": ["sku"]
                }),
                strict: None,
            }],
            tool_choice: ToolChoice::Named("lookup".into()),
            parallel_tool_calls: None,
            streaming_tool_calls: None,
        });

        let response = provider.chat(&request).await.unwrap();
        let wire = server.join().unwrap();
        assert_eq!(wire["tools"][0]["type"], "function");
        assert_eq!(wire["tools"][0]["function"]["name"], "lookup");
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
    async fn lmstudio_tool_validation_and_streaming_fail_closed() {
        use crate::capabilities::{ToolCallConfig, ToolChoice, ToolDefinition};

        let provider = LmStudioProvider::builder()
            .model("m")
            .base_url("http://127.0.0.1:1")
            .build()
            .unwrap();

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
                provider.validate_request_surface(&typed),
                Err(NxuskitError::UnsupportedCapability {
                    provider: ref actual_provider,
                    capability: ref actual_capability,
                }) if actual_provider == "lmstudio" && actual_capability == capability
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
            }) if provider == "lmstudio" && capability == "tool_streaming"
        ));
    }

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
