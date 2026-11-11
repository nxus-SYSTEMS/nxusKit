//! Perplexity AI provider implementation
//!
//! Perplexity AI provides search-augmented language models through an OpenAI-compatible API.

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

/// Perplexity AI provider
///
/// Provides access to search-augmented language models via Perplexity AI's OpenAI-compatible API.
/// Supports chat completions and streaming with real-time web search integration.
#[derive(Debug, Clone)]
pub struct PerplexityProvider {
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

impl PerplexityProvider {
    /// Static adapter declarations, not model or endpoint-effective support.
    pub fn adapter_capabilities() -> ProviderCapabilities {
        ProviderCapabilities {
            supports_system_messages: true,
            supports_streaming: true,
            supports_vision: false,   // Perplexity doesn't support vision
            max_stop_sequences: None, // Perplexity doesn't support stop sequences
            supports_presence_penalty: true,
            supports_frequency_penalty: true,
            supports_seed: false, // Perplexity doesn't support seed
            supports_logprobs: false,

            // T058: Perplexity does not expose a streaming logprob field.
            // Non-supporting; stream chunks use StreamChunk::new() which
            // defaults logprobs to None — no phantom data is possible.
            supports_streaming_logprobs: false,
            supports_json_mode: false, // Perplexity doesn't support JSON mode
            supports_json_schema: false,
            penalty_range: Some((0.0, 2.0)), // Perplexity uses 0-2 range
            max_logprobs: None,
        }
    }
}

impl PerplexityProvider {
    /// Create a new Perplexity provider with the given API key
    ///
    /// # Deprecated
    /// Use `PerplexityProvider::builder()` instead for more configuration options
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
            base_url: "https://api.perplexity.ai".to_string(),
            default_model: "llama-3.1-sonar-small-128k-online".to_string(),
            connection_timeout,
            stream_read_timeout,
            total_timeout,
        }
    }

    /// Set a custom base URL for the Perplexity API
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Create a new builder for PerplexityProvider
    pub fn builder() -> PerplexityProviderBuilder {
        PerplexityProviderBuilder::default()
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

    fn validate_request_surface(&self, request: &ChatRequest) -> Result<()> {
        request.validate_legacy_sampling(self.provider_name())?;

        for (name, value) in [
            ("presence_penalty", request.presence_penalty),
            ("frequency_penalty", request.frequency_penalty),
        ] {
            if value.is_some_and(|value| !(0.0..=2.0).contains(&value)) {
                return Err(NxuskitError::InvalidRequest(format!("invalid {name}")));
            }
        }

        if let Some(stops) = request.stop.as_ref() {
            if stops.len() > 16 || stops.iter().any(String::is_empty) {
                return Err(NxuskitError::InvalidRequest("invalid stop".into()));
            }
            return Err(NxuskitError::unsupported_capability("perplexity", "stop"));
        }
        if request.seed.is_some() {
            return Err(NxuskitError::unsupported_capability("perplexity", "seed"));
        }
        if request.logprobs.is_some() || request.top_logprobs.is_some() {
            return Err(NxuskitError::unsupported_capability(
                "perplexity",
                "logprobs",
            ));
        }
        if request.reasoning_effort.is_some() {
            return Err(NxuskitError::unsupported_capability(
                "perplexity",
                "reasoning_effort",
            ));
        }

        match request.response_format.as_ref() {
            None | Some(crate::types::ResponseFormat::Text) => {}
            Some(crate::types::ResponseFormat::Json) => {
                return Err(NxuskitError::unsupported_capability(
                    "perplexity",
                    "response_format",
                ));
            }
            Some(crate::types::ResponseFormat::JsonSchema { schema }) => {
                if !schema.is_object() {
                    return Err(NxuskitError::InvalidRequest(
                        "invalid response_format".into(),
                    ));
                }
                return Err(NxuskitError::unsupported_capability(
                    "perplexity",
                    "response_format",
                ));
            }
        }

        if !matches!(
            request.thinking_mode,
            crate::types::ThinkingMode::Auto | crate::types::ThinkingMode::Omit
        ) {
            return Err(NxuskitError::unsupported_capability(
                "perplexity",
                "thinking",
            ));
        }

        if let Some(tools) = request.tools.as_ref() {
            request.validate_legacy_tools()?;
            if !tools.is_empty() {
                return Err(NxuskitError::unsupported_capability("perplexity", "tools"));
            }
        }
        if request.tool_choice.is_some() {
            return Err(NxuskitError::unsupported_capability(
                "perplexity",
                "tool_choice",
            ));
        }

        if let Some(config) = request.structured_output.as_ref() {
            if config.validate_shape().is_err() {
                return Err(NxuskitError::InvalidRequest(
                    "invalid structured_output".into(),
                ));
            }
            if !matches!(config.mode, crate::capabilities::StructuredOutputMode::Text) {
                return Err(NxuskitError::unsupported_capability(
                    "perplexity",
                    "structured_output",
                ));
            }
        }

        if let Some(config) = request.tool_call_config.as_ref() {
            if config.validate_shape().is_err() {
                return Err(NxuskitError::InvalidRequest(
                    "invalid tool_call_config".into(),
                ));
            }
            return Err(NxuskitError::unsupported_capability(
                "perplexity",
                "tool_calling",
            ));
        }

        if request.openai_responses.is_some() {
            return Err(NxuskitError::unsupported_capability(
                "perplexity",
                "openai_responses",
            ));
        }
        if request.provider_options.is_some() {
            return Err(NxuskitError::unsupported_capability(
                "perplexity",
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
            return Err(NxuskitError::unsupported_capability("perplexity", "images"));
        }

        Ok(())
    }

    /// Convert nxusKit messages to Perplexity API format
    fn convert_messages(&self, messages: &[Message]) -> Vec<PerplexityMessage> {
        messages
            .iter()
            .map(|msg| {
                let role = match msg.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                };

                let content = match &msg.content {
                    MessageContent::Text(text) => PerplexityMessageContent::Text(text.clone()),
                    MessageContent::Parts(parts) => {
                        // Perplexity doesn't support vision - extract only text parts
                        let text = parts
                            .iter()
                            .filter_map(|part| match part {
                                ContentPart::Text { text } => Some(text.as_str()),
                                ContentPart::Image { .. } => None,
                            })
                            .collect::<Vec<_>>()
                            .join("\n");
                        PerplexityMessageContent::Text(text)
                    }
                };

                PerplexityMessage {
                    role: role.to_string(),
                    content,
                }
            })
            .collect()
    }

    /// Build Perplexity API request from adapted ChatRequest
    fn build_request(&self, request: &ChatRequest) -> Result<PerplexityRequest> {
        let messages = self.convert_messages(&request.messages);

        let perplexity_request = PerplexityRequest {
            model: request.model.clone(),
            messages,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            top_p: request.top_p,
            stream: Some(false),
            presence_penalty: request.presence_penalty,
            frequency_penalty: request.frequency_penalty,
        };

        Ok(perplexity_request)
    }
}

impl PerplexityProvider {
    /// Create a fresh session with no accumulated state.
    ///
    /// For PerplexityProvider, this returns a clone with the same configuration
    /// since it's already stateless (each request is independent).
    ///
    /// # Returns
    ///
    /// A cloned PerplexityProvider instance with the same configuration.
    pub fn fresh_session(&self) -> Self {
        self.clone()
    }
}

/// Builder for PerplexityProvider
#[derive(Debug, Default)]
pub struct PerplexityProviderBuilder {
    api_key: Option<String>,
    base_url: Option<String>,
    model: Option<String>,
    timeout: Option<Duration>,
    connection_timeout: Option<Duration>,
    stream_read_timeout: Option<Duration>,
    total_timeout: Option<Duration>,
}

impl PerplexityProviderBuilder {
    /// Set the API key for Perplexity AI
    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// Set a custom base URL for the Perplexity API
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

    /// Build the PerplexityProvider
    pub fn build(self) -> Result<PerplexityProvider> {
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

        Ok(PerplexityProvider {
            client,
            api_key,
            base_url: self
                .base_url
                .unwrap_or_else(|| "https://api.perplexity.ai".to_string()),
            default_model: self
                .model
                .unwrap_or_else(|| "llama-3.1-sonar-small-128k-online".to_string()),
            connection_timeout,
            stream_read_timeout,
            total_timeout,
        })
    }
}

#[async_trait]
impl LLMProvider for PerplexityProvider {
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse> {
        self.validate_request_surface(request)?;
        // Adapt request to provider capabilities
        let capabilities = self.get_capabilities();
        let adapted = ParameterAdapter::adapt(request, &capabilities);
        let adapted_request = &adapted.request;

        // Build Perplexity-specific request
        let perplexity_request = self.build_request(adapted_request)?;

        // Make API call
        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&perplexity_request)
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
        super::openai::merge_provider_metadata(&mut provider_metadata, &data, "perplexity")?;
        let perplexity_response: PerplexityResponse = serde_json::from_value(data)?;

        // Extract content from first choice
        let content = perplexity_response
            .choices
            .first()
            .and_then(|choice| choice.message.content.as_deref())
            .unwrap_or("")
            .to_string();

        // Convert finish reason
        let raw_finish_reason = perplexity_response
            .choices
            .first()
            .and_then(|choice| choice.finish_reason.clone());
        let finish_reason = perplexity_response
            .choices
            .first()
            .and_then(|choice| choice.finish_reason.as_ref())
            .and_then(|reason| match reason.as_str() {
                "stop" => Some(crate::types::FinishReason::Stop),
                "length" => Some(crate::types::FinishReason::Length),
                _ => None,
            });

        let estimator =
            crate::token_estimator::TokenEstimator::for_model(&perplexity_response.model);
        let estimated = TokenCount::new(
            estimator.count_messages(&adapted_request.messages),
            estimator.count(&content),
        );
        let usage = match perplexity_response.usage.and_then(|usage| usage.actual()) {
            Some(actual) => TokenUsage::with_actual(actual, estimated),
            None => TokenUsage::estimated_only(estimated),
        };
        let mut response = ChatResponse::new(content, perplexity_response.model, usage);
        response.provider = self.provider_name().to_string();

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
            "provider": "perplexity"
        }));

        Ok(response)
    }

    async fn chat_stream(
        &self,
        request: &ChatRequest,
    ) -> Result<Box<dyn Stream<Item = Result<StreamChunk>> + Send + Unpin>> {
        self.validate_request_surface(request)?;
        // Adapt request to provider capabilities
        let capabilities = self.get_capabilities();
        let adapted = ParameterAdapter::adapt(request, &capabilities);
        let adapted_request = &adapted.request;

        // Build Perplexity-specific request with streaming enabled
        let mut perplexity_request = self.build_request(adapted_request)?;
        perplexity_request.stream = Some(true);

        // Make API call
        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&perplexity_request)
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
            adapted_request,
            "perplexity",
        ))
    }

    fn provider_name(&self) -> &str {
        "perplexity"
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        // Perplexity doesn't have a models endpoint, return known models
        Ok(vec![
            {
                let mut info = ModelInfo::new("llama-3.1-sonar-small-128k-online");
                info.description = Some(
                    "Sonar Small: Fast, efficient web-grounded search with real-time answers"
                        .to_string(),
                );
                info.context_window = Some(128_000);
                info.metadata
                    .insert("based_on".to_string(), "llama-3.1".to_string());
                info.metadata.insert(
                    "features".to_string(),
                    "online_search,real_time".to_string(),
                );
                info
            },
            {
                let mut info = ModelInfo::new("llama-3.1-sonar-large-128k-online");
                info.description = Some(
                    "Sonar Large: High-capability web search with comprehensive, cited answers"
                        .to_string(),
                );
                info.context_window = Some(128_000);
                info.metadata
                    .insert("based_on".to_string(), "llama-3.1".to_string());
                info.metadata.insert(
                    "features".to_string(),
                    "online_search,real_time".to_string(),
                );
                info
            },
            {
                let mut info = ModelInfo::new("llama-3.1-sonar-huge-128k-online");
                info.description = Some("Sonar Huge: Most powerful Sonar model based on Llama 3.1 405B for complex queries".to_string());
                info.context_window = Some(128_000);
                info.metadata
                    .insert("based_on".to_string(), "llama-3.1-405b".to_string());
                info.metadata.insert(
                    "features".to_string(),
                    "online_search,real_time".to_string(),
                );
                info
            },
        ])
    }

    fn get_capabilities(&self) -> ProviderCapabilities {
        Self::adapter_capabilities()
    }
}

#[derive(Debug, Serialize)]
struct PerplexityRequest {
    model: String,
    messages: Vec<PerplexityMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    presence_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    frequency_penalty: Option<f32>,
}

#[derive(Debug, Serialize)]
struct PerplexityMessage {
    role: String,
    content: PerplexityMessageContent,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum PerplexityMessageContent {
    Text(String),
}

#[derive(Debug, Deserialize)]
struct PerplexityResponse {
    model: String,
    choices: Vec<PerplexityChoice>,
    usage: Option<super::openai::SelectedProviderUsage>,
}

#[derive(Debug, Deserialize)]
struct PerplexityChoice {
    message: PerplexityResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PerplexityResponseMessage {
    #[allow(dead_code)]
    role: String,
    content: Option<String>,
}

// TDD TESTS - Written FIRST before implementation
#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_perplexity_capabilities() {
        let provider = PerplexityProvider::new("test-key");
        let caps = provider.get_capabilities();

        assert!(caps.supports_system_messages);
        assert!(caps.supports_streaming);
        assert!(!caps.supports_vision); // Perplexity doesn't support vision
        assert_eq!(caps.max_stop_sequences, None); // Perplexity doesn't support stop
        assert!(caps.supports_presence_penalty);
        assert!(caps.supports_frequency_penalty);
        assert!(!caps.supports_seed); // Perplexity doesn't support seed
        assert!(!caps.supports_logprobs);
        assert!(!caps.supports_json_mode); // Perplexity doesn't support JSON mode
        assert!(!caps.supports_json_schema);
        assert_eq!(caps.penalty_range, Some((0.0, 2.0)));
    }

    fn assert_unsupported(request: &ChatRequest, capability: &str) {
        let provider = PerplexityProvider::new("test-key");
        match provider.validate_request_surface(request) {
            Err(NxuskitError::UnsupportedCapability {
                provider,
                capability: actual,
            }) => {
                assert_eq!(provider, "perplexity");
                assert_eq!(actual, capability);
            }
            other => panic!("expected unsupported {capability}, got {other:?}"),
        }
    }

    #[test]
    fn test_perplexity_request_surface_rejects_explicit_unsupported_controls() {
        let base = ChatRequest::new("sonar").with_message(Message::user("hi"));

        let mut request = base.clone();
        request.stop = Some(vec!["END".into()]);
        assert_unsupported(&request, "stop");

        let mut request = base.clone();
        request.seed = Some(42);
        assert_unsupported(&request, "seed");

        let mut request = base.clone();
        request.logprobs = Some(true);
        assert_unsupported(&request, "logprobs");

        let mut request = base.clone();
        request.reasoning_effort = Some("low".into());
        assert_unsupported(&request, "reasoning_effort");

        let mut request = base.clone();
        request.response_format = Some(crate::types::ResponseFormat::Json);
        assert_unsupported(&request, "response_format");

        let mut request = base.clone();
        request.thinking_mode = crate::types::ThinkingMode::Enabled;
        assert_unsupported(&request, "thinking");

        let mut request = base.clone();
        request.tools = Some(vec![serde_json::json!({
            "type": "function",
            "function": {
                "name": "emit_x",
                "parameters": {"type": "object"}
            }
        })]);
        assert_unsupported(&request, "tools");

        let mut request = base.clone();
        request.tool_choice = Some(serde_json::json!("required"));
        assert_unsupported(&request, "tool_choice");

        let mut request = base.clone();
        request.structured_output = Some(crate::capabilities::StructuredOutputConfig {
            mode: crate::capabilities::StructuredOutputMode::JsonObject,
            schema: None,
            schema_name: None,
            strict: None,
            schema_subset: None,
        });
        assert_unsupported(&request, "structured_output");

        let mut request = base.clone();
        request.tool_call_config = Some(crate::capabilities::ToolCallConfig {
            tools: vec![crate::capabilities::ToolDefinition {
                name: "emit_x".into(),
                description: None,
                parameters: serde_json::json!({"type": "object"}),
                strict: None,
            }],
            tool_choice: crate::capabilities::ToolChoice::Auto,
            parallel_tool_calls: None,
            streaming_tool_calls: None,
        });
        assert_unsupported(&request, "tool_calling");

        let mut request = base.clone();
        request.openai_responses = Some(crate::capabilities::OpenAIResponsesOptions::default());
        assert_unsupported(&request, "openai_responses");

        let request = ChatRequest::new("sonar")
            .with_message(Message::user("hi").with_image_url("https://image.invalid/p.png"));
        assert_unsupported(&request, "images");
    }

    #[test]
    fn test_perplexity_request_surface_preserves_invalid_request_precedence() {
        let provider = PerplexityProvider::new("test-key");
        let base = ChatRequest::new("sonar").with_message(Message::user("hi"));

        let mut request = base.clone();
        request.presence_penalty = Some(-0.1);
        assert!(matches!(
            provider.validate_request_surface(&request),
            Err(NxuskitError::InvalidRequest(_))
        ));

        let mut request = base.clone();
        request.stop = Some(vec!["".into()]);
        assert!(matches!(
            provider.validate_request_surface(&request),
            Err(NxuskitError::InvalidRequest(_))
        ));

        let mut request = base.clone();
        request.response_format = Some(crate::types::ResponseFormat::JsonSchema {
            schema: serde_json::json!([]),
        });
        assert!(matches!(
            provider.validate_request_surface(&request),
            Err(NxuskitError::InvalidRequest(_))
        ));

        let mut request = base.clone();
        request.tools = Some(vec![serde_json::json!({
            "type": "function",
            "function": {"name": "", "parameters": {}}
        })]);
        assert!(matches!(
            provider.validate_request_surface(&request),
            Err(NxuskitError::InvalidRequest(_))
        ));

        let mut request = base.clone();
        request.structured_output = Some(crate::capabilities::StructuredOutputConfig {
            mode: crate::capabilities::StructuredOutputMode::JsonSchema,
            schema: None,
            schema_name: None,
            strict: None,
            schema_subset: None,
        });
        assert!(matches!(
            provider.validate_request_surface(&request),
            Err(NxuskitError::InvalidRequest(_))
        ));

        let mut request = base;
        request.tool_call_config = Some(crate::capabilities::ToolCallConfig {
            tools: vec![crate::capabilities::ToolDefinition {
                name: "".into(),
                description: None,
                parameters: serde_json::json!({"type": "object"}),
                strict: None,
            }],
            tool_choice: crate::capabilities::ToolChoice::Auto,
            parallel_tool_calls: None,
            streaming_tool_calls: None,
        });
        assert!(matches!(
            provider.validate_request_surface(&request),
            Err(NxuskitError::InvalidRequest(_))
        ));
    }

    #[test]
    fn test_perplexity_supported_request_surface_still_builds() {
        let provider = PerplexityProvider::new("test-key");
        let mut request = ChatRequest::new("sonar")
            .with_message(Message::user("hi"))
            .with_temperature(0.7)
            .with_max_tokens(100)
            .with_top_p(0.9);
        request.presence_penalty = Some(0.0);
        request.frequency_penalty = Some(2.0);
        request.response_format = Some(crate::types::ResponseFormat::Text);
        request.thinking_mode = crate::types::ThinkingMode::Omit;
        request.tools = Some(vec![]);

        provider.validate_request_surface(&request).unwrap();
        let wire = provider.build_request(&request).unwrap();
        assert_eq!(wire.model, "sonar");
        assert_eq!(wire.temperature, Some(0.7));
        assert_eq!(wire.max_tokens, Some(100));
        assert_eq!(wire.top_p, Some(0.9));
        assert_eq!(wire.presence_penalty, Some(0.0));
        assert_eq!(wire.frequency_penalty, Some(2.0));
    }

    #[tokio::test]
    async fn test_perplexity_unsupported_stop_rejects_before_http() {
        let provider = PerplexityProvider::builder()
            .api_key("test-key")
            .base_url("http://127.0.0.1:1")
            .build()
            .unwrap();
        let mut request = ChatRequest::new("sonar").with_message(Message::user("hi"));
        request.stop = Some(vec!["END".into()]);

        assert!(matches!(
            provider.chat(&request).await,
            Err(NxuskitError::UnsupportedCapability {
                ref provider,
                ref capability,
            }) if provider == "perplexity" && capability == "stop"
        ));
        assert!(matches!(
            provider.chat_stream(&request).await,
            Err(NxuskitError::UnsupportedCapability {
                ref provider,
                ref capability,
            }) if provider == "perplexity" && capability == "stop"
        ));
    }

    mod pro_tests {
        use super::*;
        use crate::Message;

        #[tokio::test]
        async fn test_perplexity_new_with_default_base_url() {
            let provider = PerplexityProvider::new("test-key");
            assert_eq!(provider.base_url, "https://api.perplexity.ai");
            assert_eq!(provider.api_key, "test-key");
        }

        #[tokio::test]
        async fn test_perplexity_with_custom_base_url() {
            let provider =
                PerplexityProvider::new("test-key").with_base_url("https://custom.perplexity.ai");
            assert_eq!(provider.base_url, "https://custom.perplexity.ai");
        }

        #[tokio::test]
        async fn test_message_conversion_text_only() {
            let provider = PerplexityProvider::new("test-key");

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
                PerplexityMessageContent::Text(text) => assert_eq!(text, "You are helpful"),
            }
        }

        #[tokio::test]
        async fn test_build_request_basic() {
            let provider = PerplexityProvider::new("test-key");

            let request = ChatRequest::new("llama-3.1-sonar-large-128k-online")
                .with_message(Message::user("Hello"))
                .with_temperature(0.7)
                .with_max_tokens(100);

            let perplexity_request = provider.build_request(&request).unwrap();

            assert_eq!(
                perplexity_request.model,
                "llama-3.1-sonar-large-128k-online"
            );
            assert_eq!(perplexity_request.temperature, Some(0.7));
            assert_eq!(perplexity_request.max_tokens, Some(100));
            assert_eq!(perplexity_request.stream, Some(false));
        }

        #[tokio::test]
        async fn test_build_request_with_penalties() {
            let provider = PerplexityProvider::new("test-key");

            let mut request = ChatRequest::new("test-model")
                .with_message(Message::user("Hello"))
                .with_temperature(0.8)
                .with_max_tokens(500)
                .with_top_p(0.9);

            request.presence_penalty = Some(0.5);
            request.frequency_penalty = Some(0.3);

            let perplexity_request = provider.build_request(&request).unwrap();

            assert_eq!(perplexity_request.temperature, Some(0.8));
            assert_eq!(perplexity_request.max_tokens, Some(500));
            assert_eq!(perplexity_request.top_p, Some(0.9));
            assert_eq!(perplexity_request.presence_penalty, Some(0.5));
            assert_eq!(perplexity_request.frequency_penalty, Some(0.3));
        }

        #[tokio::test]
        async fn test_provider_name() {
            let provider = PerplexityProvider::new("test-key");
            assert_eq!(provider.provider_name(), "perplexity");
        }

        #[tokio::test]
        async fn test_list_models() {
            let provider = PerplexityProvider::new("test-key");
            let models = provider.list_models().await.unwrap();

            assert_eq!(models.len(), 3);
            assert_eq!(models[0].name, "llama-3.1-sonar-small-128k-online");
            assert_eq!(models[1].name, "llama-3.1-sonar-large-128k-online");
            assert_eq!(models[2].name, "llama-3.1-sonar-huge-128k-online");
        }

        // ===== TDD Tests for Builder Pattern (Red Phase - These will fail initially) =====

        #[tokio::test]
        async fn test_builder_pattern_basic() {
            let provider = PerplexityProvider::builder()
                .api_key("test-key")
                .build()
                .expect("Failed to build provider");

            assert_eq!(provider.api_key, "test-key");
            assert_eq!(provider.base_url, "https://api.perplexity.ai");
        }

        #[tokio::test]
        async fn test_builder_requires_api_key() {
            let result = PerplexityProvider::builder().build();
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
            let provider = PerplexityProvider::builder()
                .api_key("test-key")
                .base_url("https://custom.perplexity.ai")
                .build()
                .expect("Failed to build provider");

            assert_eq!(provider.base_url, "https://custom.perplexity.ai");
        }

        #[tokio::test]
        async fn test_builder_with_default_model() {
            let provider = PerplexityProvider::builder()
                .api_key("test-key")
                .model("llama-3.1-sonar-large-128k-online")
                .build()
                .expect("Failed to build provider");

            assert_eq!(provider.default_model, "llama-3.1-sonar-large-128k-online");
        }

        #[tokio::test]
        async fn test_builder_with_timeout() {
            use std::time::Duration;

            let provider = PerplexityProvider::builder()
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

            let provider = PerplexityProvider::builder()
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
            let provider = PerplexityProvider::builder()
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

            let provider = PerplexityProvider::builder()
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
