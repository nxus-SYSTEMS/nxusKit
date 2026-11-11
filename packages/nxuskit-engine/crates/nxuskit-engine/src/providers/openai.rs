//! OpenAI provider implementation

use async_stream::stream;
use async_trait::async_trait;
use futures::Stream;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::{
    ChatRequest, ChatResponse, LLMProvider, Message, ModelInfo, Role, StreamChunk, TokenCount,
    TokenUsage,
    error::{NxuskitError, Result},
    parameter_adapter::{AdaptedRequest, ParameterAdapter},
    provider::ProviderContractMapper,
    token_estimator::{StreamingTokenAccumulator, TokenEstimator},
    types::{
        ContentPart, FinishReason, ImageData, InferenceMetadata, LogprobsData, MessageContent,
        ProviderCapabilities,
    },
};

const OPENAI_API_BASE: &str = "https://api.openai.com/v1";

/// OpenAI provider
#[derive(Clone)]
pub struct OpenAIProvider {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
    default_model: String,
    connection_timeout: Duration,
    stream_read_timeout: Duration,
    total_timeout: Duration,
}

impl OpenAIProvider {
    /// Static adapter declarations, not model or endpoint-effective support.
    pub fn adapter_capabilities() -> ProviderCapabilities {
        let caps = ProviderCapabilities {
            supports_system_messages: true,
            supports_streaming: true,
            supports_vision: true,
            max_stop_sequences: Some(4),
            supports_presence_penalty: true,
            supports_frequency_penalty: true,
            supports_seed: true,
            supports_logprobs: true,
            supports_streaming_logprobs: true,
            supports_json_mode: true,
            supports_json_schema: true,
            penalty_range: Some((-2.0, 2.0)),
            max_logprobs: Some(20),
        };
        debug_assert!(
            caps.supports_logprobs || !caps.supports_streaming_logprobs,
            "supports_streaming_logprobs implies supports_logprobs"
        );
        caps
    }
}

impl OpenAIProvider {
    /// Create a new OpenAI provider builder
    pub fn builder() -> OpenAIProviderBuilder {
        OpenAIProviderBuilder::default()
    }

    /// Convert our Message format to OpenAI's format
    fn convert_messages(&self, messages: &[Message]) -> Vec<OpenAIMessage> {
        messages
            .iter()
            .map(|msg| {
                let content_parts = match &msg.content {
                    MessageContent::Text(text) => {
                        vec![OpenAIContentPart::Text { text: text.clone() }]
                    }
                    MessageContent::Parts(parts) => parts
                        .iter()
                        .map(|part| match part {
                            ContentPart::Text { text } => {
                                OpenAIContentPart::Text { text: text.clone() }
                            }
                            ContentPart::Image { source } => {
                                let url = match &source.data {
                                    ImageData::Url { url } => url.clone(),
                                    ImageData::Base64 { media_type, data } => {
                                        format!("data:{};base64,{}", media_type, data)
                                    }
                                };
                                OpenAIContentPart::ImageUrl {
                                    image_url: OpenAIImageUrl {
                                        url,
                                        detail: source.detail.clone(),
                                    },
                                }
                            }
                        })
                        .collect(),
                };

                OpenAIMessage {
                    role: match msg.role {
                        Role::System => "system".to_string(),
                        Role::User => "user".to_string(),
                        Role::Assistant => "assistant".to_string(),
                    },
                    content: Some(content_parts),
                }
            })
            .collect()
    }

    fn build_openai_request(
        &self,
        request: &ChatRequest,
        stream: bool,
    ) -> Result<(OpenAIRequest, AdaptedRequest)> {
        let capabilities = self.get_capabilities();
        let mut adapted = ParameterAdapter::adapt(request, &capabilities);
        if adapted.request.openai_responses.is_some() {
            return Err(NxuskitError::InvalidRequest(
                "OpenAI Responses options require the explicit Responses transport".into(),
            ));
        }
        super::validate_typed_capability_request(
            "openai",
            &adapted.request,
            &mut adapted.warnings,
        )?;
        let adapted_request = &adapted.request;
        let record = crate::capabilities::registry::find("openai").ok_or_else(|| {
            NxuskitError::Configuration("provider capability record not found: openai".into())
        })?;

        // Splice the typed Phase 4 surfaces (Null = omit on the wire).
        let response_format = adapted_request
            .structured_output
            .as_ref()
            .map(crate::capabilities::openai_wire::response_format)
            .unwrap_or_else(|| match &adapted_request.response_format {
                None | Some(crate::types::ResponseFormat::Text) => serde_json::Value::Null,
                Some(crate::types::ResponseFormat::Json) => serde_json::json!({"type":"json_object"}),
                Some(crate::types::ResponseFormat::JsonSchema { schema }) => serde_json::json!({
                    "type":"json_schema", "json_schema":{"name":"response", "strict":true, "schema":schema}
                }),
            });
        let (tools, tool_choice, parallel_tool_calls) =
            match adapted_request.tool_call_config.as_ref() {
                Some(cfg) => (
                    crate::capabilities::openai_wire::tools_for(&record, cfg),
                    crate::capabilities::openai_wire::tool_choice_for(&record, cfg),
                    cfg.parallel_tool_calls,
                ),
                None => (
                    adapted_request
                        .tools
                        .as_ref()
                        .map(|tools| serde_json::json!(tools))
                        .unwrap_or(serde_json::Value::Null),
                    adapted_request
                        .tool_choice
                        .clone()
                        .unwrap_or(serde_json::Value::Null),
                    None,
                ),
            };

        Ok((
            OpenAIRequest {
                model: adapted_request.model.clone(),
                messages: self.convert_messages(&adapted_request.messages),
                temperature: adapted_request.temperature,
                max_tokens: adapted_request.max_tokens,
                top_p: adapted_request.top_p,
                stop: adapted_request.stop.clone(),
                presence_penalty: adapted_request.presence_penalty,
                frequency_penalty: adapted_request.frequency_penalty,
                seed: adapted_request.seed,
                logprobs: adapted_request.logprobs,
                top_logprobs: adapted_request.top_logprobs,
                reasoning_effort: adapted_request.reasoning_effort.clone(),
                stream,
                stream_options: if stream {
                    Some(OpenAIStreamOptions {
                        include_usage: true,
                    })
                } else {
                    None
                },
                response_format,
                tools,
                tool_choice,
                parallel_tool_calls,
            },
            adapted,
        ))
    }

    fn build_openai_responses_request(
        &self,
        request: &ChatRequest,
    ) -> Result<(OpenAIResponsesRequest, AdaptedRequest)> {
        let capabilities = self.get_capabilities();
        let mut adapted = ParameterAdapter::adapt(request, &capabilities);
        super::validate_typed_capability_request(
            "openai",
            &adapted.request,
            &mut adapted.warnings,
        )?;
        let adapted_request = &adapted.request;
        let opts = adapted_request.openai_responses.clone().ok_or_else(|| {
            NxuskitError::InvalidRequest(
                "OpenAI Responses transport requires openai_responses options".into(),
            )
        })?;

        let record = crate::capabilities::registry::find("openai").ok_or_else(|| {
            NxuskitError::Configuration("provider capability record not found: openai".into())
        })?;
        let response_validation = crate::capabilities::validate_request_against_record(
            &record,
            &crate::capabilities::FeatureRequest::openai_responses(opts.clone()),
        );
        if let crate::capabilities::ValidationOutcome::Block { feature, reason } =
            response_validation
        {
            return Err(NxuskitError::InvalidRequest(format!("{feature}: {reason}")));
        }

        let reasoning = opts
            .reasoning
            .as_ref()
            .map(|reasoning| OpenAIResponsesReasoning {
                effort: reasoning.effort.clone(),
                summary: reasoning.summary.clone(),
            });
        let text = opts.text_verbosity.map(OpenAIResponsesText::plain);
        let include = responses_include_paths(&opts);

        Ok((
            OpenAIResponsesRequest {
                model: adapted_request.model.clone(),
                input: self.convert_messages(&adapted_request.messages),
                max_output_tokens: adapted_request.max_tokens,
                reasoning,
                text,
                previous_response_id: opts.previous_response_id,
                include,
                phase: opts.phase,
                tools: if record.features.hosted_tools.web_search
                    == crate::capabilities::CapabilityStatus::Supported
                {
                    opts.hosted_tools
                } else {
                    Vec::new()
                },
                tool_search: if record.features.search_citations.search_controls
                    == crate::capabilities::CapabilityStatus::Supported
                {
                    opts.tool_search
                } else {
                    None
                },
            },
            adapted,
        ))
    }

    async fn chat_responses(&self, request: &ChatRequest) -> Result<ChatResponse> {
        let (responses_request, adapted) = self.build_openai_responses_request(request)?;

        let response = self
            .client
            .post(format!("{}/responses", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("content-type", "application/json")
            .json(&responses_request)
            .timeout(self.total_timeout)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let retry_after = super::parse_retry_after(response.headers());
            let error_text = crate::provider::http::read_response_text_bounded(response)
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(match status.as_u16() {
                401 | 403 => NxuskitError::Authentication(error_text),
                429 => NxuskitError::rate_limit(retry_after),
                _ => NxuskitError::provider(status.as_u16(), error_text),
            });
        }

        let responses_response: OpenAIResponsesResponse =
            crate::provider::http::read_response_json_bounded(response).await?;
        let content = extract_responses_text(&responses_response);
        let model = responses_response
            .model
            .unwrap_or_else(|| adapted.request.model.clone());
        let usage = responses_response
            .usage
            .map(|usage| {
                let actual = TokenCount::new(usage.input_tokens, usage.output_tokens);
                TokenUsage::with_actual(actual, actual)
            })
            .unwrap_or_else(|| {
                let estimator = TokenEstimator::for_model(&model);
                TokenUsage::estimated_only(TokenCount::new(
                    estimator.count_messages(&adapted.request.messages),
                    estimator.count(&content),
                ))
            });
        let finish_reason = match responses_response.status.as_deref() {
            Some("completed") => Some(FinishReason::Stop),
            Some("failed" | "cancelled") => {
                return Err(NxuskitError::provider(
                    200,
                    format!(
                        "OpenAI Responses terminal status: {}",
                        responses_response.status.as_deref().unwrap()
                    ),
                ));
            }
            _ => None,
        };
        let is_complete = matches!(
            responses_response.status.as_deref(),
            Some("completed" | "incomplete")
        );

        let mut chat_response = ChatResponse::new(content, model, usage);
        chat_response.provider = self.provider_name().to_string();
        chat_response.finish_reason = finish_reason;
        chat_response.raw_finish_reason = responses_response.status;
        chat_response.warnings = adapted.warnings;
        chat_response.inference_metadata = InferenceMetadata {
            is_complete,
            finish_reason,
            ..InferenceMetadata::default()
        }
        .with_token_usage(chat_response.usage.clone())
        .with_provider_metadata(serde_json::json!({
            "provider": "openai",
            "transport": "responses",
            "response_id": responses_response.id,
        }));

        Ok(chat_response)
    }
}

impl std::fmt::Debug for OpenAIProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAIProvider")
            .field("base_url", &self.base_url)
            .field("default_model", &self.default_model)
            .field("connection_timeout", &self.connection_timeout)
            .field("stream_read_timeout", &self.stream_read_timeout)
            .field("total_timeout", &self.total_timeout)
            .field("api_key", &"[REDACTED]")
            .finish()
    }
}

/// Enrich model info with descriptions for known models
fn enrich_openai_model(info: &mut ModelInfo) {
    let model_id = &info.name;

    // Match by exact ID or pattern
    let (description, context_window, version, family) = if model_id.starts_with("gpt-4o") {
        if model_id.contains("mini") {
            (
                Some("Fast and affordable multimodal model for lightweight tasks"),
                Some(128_000),
                Some("4"),
                Some("gpt-4"),
            )
        } else {
            (
                Some("High-intelligence flagship model for complex, multi-step tasks"),
                Some(128_000),
                Some("4"),
                Some("gpt-4"),
            )
        }
    } else if model_id.starts_with("gpt-4-turbo") {
        (
            Some("Previous generation high-intelligence model"),
            Some(128_000),
            Some("4"),
            Some("gpt-4"),
        )
    } else if model_id.starts_with("gpt-4") {
        (
            Some("GPT-4 model for complex tasks"),
            Some(8_192),
            Some("4"),
            Some("gpt-4"),
        )
    } else if model_id.starts_with("gpt-3.5-turbo") {
        (
            Some("Fast, inexpensive model for simple tasks"),
            Some(16_385),
            Some("3.5"),
            Some("gpt-3.5"),
        )
    } else if model_id.starts_with("o1") {
        if model_id.contains("mini") {
            (
                Some("Fast reasoning model for coding, math, and science"),
                Some(128_000),
                Some("1"),
                Some("o1"),
            )
        } else {
            (
                Some("Advanced reasoning model for complex problem solving"),
                Some(128_000),
                Some("1"),
                Some("o1"),
            )
        }
    } else {
        (None, None, None, None)
    };

    if let Some(desc) = description {
        info.description = Some(desc.to_string());
    }
    if let Some(ctx) = context_window {
        info.context_window = Some(ctx);
    }
    if let Some(ver) = version {
        info.metadata.insert("version".to_string(), ver.to_string());
    }
    if let Some(fam) = family {
        info.metadata.insert("family".to_string(), fam.to_string());
    }
}

#[async_trait]
impl LLMProvider for OpenAIProvider {
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse> {
        request.validate_legacy_sampling(self.provider_name())?;
        if request.openai_responses.is_some() {
            return self.chat_responses(request).await;
        }

        let (openai_request, adapted) = self.build_openai_request(request, false)?;

        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("content-type", "application/json")
            .json(&openai_request)
            .timeout(self.total_timeout)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let retry_after = super::parse_retry_after(response.headers());
            let error_text = crate::provider::http::read_response_text_bounded(response)
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(match status.as_u16() {
                401 | 403 => NxuskitError::Authentication(error_text),
                429 => NxuskitError::rate_limit(retry_after),
                _ => NxuskitError::provider(status.as_u16(), error_text),
            });
        }

        let openai_response: OpenAIResponse =
            crate::provider::http::read_response_json_bounded(response).await?;

        let choice =
            openai_response
                .choices
                .into_iter()
                .next()
                .ok_or_else(|| NxuskitError::Provider {
                    status: 500,
                    message: "No choices in response".to_string(),
                })?;

        // Extract finish_reason before consuming the choice
        let finish_reason = choice
            .finish_reason
            .as_deref()
            .and_then(FinishReason::from_str_flexible);
        let logprobs = decode_oai_logprob_delta(choice.logprobs).map(|delta| LogprobsData {
            content: delta.content,
        });

        // Extract text from content parts
        let content_text = choice
            .message
            .content
            .unwrap_or_default()
            .iter()
            .filter_map(|part| match part {
                OpenAIContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");

        let mut response = ChatResponse::new(
            content_text,
            openai_response.model,
            TokenUsage::estimated_only(TokenCount::new(
                openai_response.usage.prompt_tokens,
                openai_response.usage.completion_tokens,
            )),
        );
        response.provider = self.provider_name().to_string();
        response.finish_reason = finish_reason;
        response.raw_finish_reason = choice.finish_reason;
        response.logprobs = logprobs;

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
            "provider": "openai"
        }));

        Ok(response)
    }

    async fn chat_stream(
        &self,
        request: &ChatRequest,
    ) -> Result<Box<dyn Stream<Item = Result<StreamChunk>> + Send + Unpin>> {
        request.validate_legacy_sampling(self.provider_name())?;
        let (openai_request, adapted) = self.build_openai_request(request, true)?;

        // Note: The client already has timeouts configured via build_http_client():
        // - connect_timeout: applies during TCP handshake
        // - read_timeout: applies per-chunk during streaming (resets after each chunk)
        // - timeout (total): applies to the entire request lifecycle
        //
        // For streaming requests, we use total_timeout as the request-level timeout.
        // The read_timeout on the client handles inter-chunk delays during streaming.
        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("content-type", "application/json")
            .json(&openai_request)
            .timeout(self.total_timeout)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let retry_after = super::parse_retry_after(response.headers());
            let error_text = crate::provider::http::read_response_text_bounded(response)
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(match status.as_u16() {
                401 | 403 => NxuskitError::Authentication(error_text),
                429 => NxuskitError::rate_limit(retry_after),
                _ => NxuskitError::provider(status.as_u16(), error_text),
            });
        }

        Ok(openai_compatible_stream(response, &adapted.request))
    }

    fn provider_name(&self) -> &str {
        "openai"
    }

    /// List available OpenAI models with context window information
    ///
    /// Returns hardcoded model information for popular OpenAI models. While OpenAI
    /// provides a models API endpoint, this returns a curated list of commonly used
    /// chat models with their specifications.
    ///
    /// # Returns
    ///
    /// Returns a vector of `ModelInfo` containing:
    /// - Model names and identifiers
    /// - Context window sizes (in tokens)
    /// - Human-readable descriptions
    ///
    /// # Example
    ///
    /// ```no_run
    /// use nxuskit_engine::prelude::*;
    ///
    /// # async fn example() -> std::result::Result<(), Box<dyn std::error::Error>> {
    /// let provider = OpenAIProvider::builder()
    ///     .api_key("your-api-key")
    ///     .build()?;
    /// let models = provider.list_models().await?;
    /// for model in models {
    ///     println!("{} - {}",
    ///         model.name,
    ///         model.formatted_context_window().unwrap_or_default()
    ///     );
    /// }
    /// # Ok(())
    /// # }
    /// ```
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

        let models_response: OpenAIModelsResponse =
            crate::provider::http::read_response_json_bounded(response).await?;

        Ok(models_response
            .data
            .into_iter()
            .map(|model| {
                let mut info = ModelInfo::new(model.id.clone());
                info.metadata
                    .insert("created".to_string(), model.created.to_string());
                info.metadata.insert("owned_by".to_string(), model.owned_by);

                // Enrich with descriptions and metadata for known models
                enrich_openai_model(&mut info);

                info
            })
            .collect())
    }

    fn get_capabilities(&self) -> ProviderCapabilities {
        Self::adapter_capabilities()
    }
}

impl OpenAIProvider {
    /// Create a fresh session with no accumulated state.
    ///
    /// For OpenAIProvider, this returns a clone with the same configuration
    /// since it's already stateless (each request is independent).
    ///
    /// # Returns
    ///
    /// A cloned OpenAIProvider instance with the same configuration.
    pub fn fresh_session(&self) -> Self {
        self.clone()
    }
}

/// Builder for OpenAIProvider
#[derive(Debug, Default)]
pub struct OpenAIProviderBuilder {
    api_key: Option<String>,
    base_url: Option<String>,
    model: Option<String>,
    timeout: Option<Duration>,
    connection_timeout: Option<Duration>,
    stream_read_timeout: Option<Duration>,
    total_timeout: Option<Duration>,
}

impl OpenAIProviderBuilder {
    /// Set the API key
    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// Set the base URL (default: <https://api.openai.com/v1>)
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    /// Set the default model
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Set the request timeout (applies to both connection and total request)
    ///
    /// This is a convenience method that sets a general timeout.
    /// For more granular control, use `connection_timeout()`, `stream_read_timeout()`,
    /// or `total_timeout()` instead.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Set the connection establishment timeout
    ///
    /// This timeout applies to the initial connection to the API.
    /// If not set, falls back to the general `timeout` or default (60s).
    pub fn connection_timeout(mut self, timeout: Duration) -> Self {
        self.connection_timeout = Some(timeout);
        self
    }

    /// Set the timeout for reading each chunk in streaming responses
    ///
    /// This timeout applies to reading each individual chunk from a stream.
    /// If not set, falls back to the general `timeout` or default (120s for streams).
    pub fn stream_read_timeout(mut self, timeout: Duration) -> Self {
        self.stream_read_timeout = Some(timeout);
        self
    }

    /// Set the total timeout for the entire request
    ///
    /// This timeout applies to the entire request duration (connection + body).
    /// If not set, falls back to the general `timeout` or default (60s).
    pub fn total_timeout(mut self, timeout: Duration) -> Self {
        self.total_timeout = Some(timeout);
        self
    }

    /// Build the OpenAIProvider
    pub fn build(self) -> Result<OpenAIProvider> {
        let api_key = self
            .api_key
            .ok_or_else(|| NxuskitError::Configuration("API key is required".to_string()))?;

        // Default timeout values
        let default_timeout = Duration::from_secs(60);
        let default_stream_timeout = Duration::from_secs(120);

        // Fallback chain: specific timeout -> general timeout -> default
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

        // Build HTTP client with properly applied timeouts
        // This fixes the bug where timeouts were stored but not applied to the client
        let client =
            super::build_http_client(connection_timeout, stream_read_timeout, total_timeout)?;

        Ok(OpenAIProvider {
            client,
            api_key,
            base_url: self.base_url.unwrap_or_else(|| OPENAI_API_BASE.to_string()),
            default_model: self.model.unwrap_or_else(|| "gpt-4".to_string()),
            connection_timeout,
            stream_read_timeout,
            total_timeout,
        })
    }
}

// Internal OpenAI API types

#[derive(Debug, Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
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
    logprobs: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_logprobs: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<String>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<OpenAIStreamOptions>,
    /// Typed structured-output payload (Phase 4 / US2). Spliced from
    /// [`crate::types::ChatRequest::structured_output`] via
    /// [`crate::capabilities::openai_wire::response_format`]. Skipped when
    /// `Null` (the omit-the-field signal).
    #[serde(skip_serializing_if = "is_json_null")]
    response_format: serde_json::Value,
    /// Typed tools payload (Phase 4 / US2). Spliced from
    /// [`crate::types::ChatRequest::tool_call_config`] via
    /// [`crate::capabilities::openai_wire::tools`].
    #[serde(skip_serializing_if = "is_json_null")]
    tools: serde_json::Value,
    /// Typed tool-choice payload (Phase 4 / US2). Spliced from
    /// [`crate::types::ChatRequest::tool_call_config`] via
    /// [`crate::capabilities::openai_wire::tool_choice`].
    #[serde(skip_serializing_if = "is_json_null")]
    tool_choice: serde_json::Value,
    /// Typed parallel-tool-calls hint. Carried through from
    /// [`crate::capabilities::ToolCallConfig::parallel_tool_calls`].
    #[serde(skip_serializing_if = "Option::is_none")]
    parallel_tool_calls: Option<bool>,
}

fn is_json_null(v: &serde_json::Value) -> bool {
    v.is_null()
}

#[derive(Debug, Serialize)]
struct OpenAIResponsesRequest {
    model: String,
    input: Vec<OpenAIMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning: Option<OpenAIResponsesReasoning>,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<OpenAIResponsesText>,
    #[serde(skip_serializing_if = "Option::is_none")]
    previous_response_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    include: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    phase: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tools: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_search: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct OpenAIResponsesReasoning {
    #[serde(skip_serializing_if = "Option::is_none")]
    effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
}

#[derive(Debug, Serialize)]
struct OpenAIResponsesText {
    format: OpenAIResponsesTextFormat,
    verbosity: crate::capabilities::TextVerbosity,
}

impl OpenAIResponsesText {
    fn plain(verbosity: crate::capabilities::TextVerbosity) -> Self {
        Self {
            format: OpenAIResponsesTextFormat {
                r#type: "text".into(),
            },
            verbosity,
        }
    }
}

#[derive(Debug, Serialize)]
struct OpenAIResponsesTextFormat {
    #[serde(rename = "type")]
    r#type: String,
}

fn responses_include_paths(opts: &crate::capabilities::OpenAIResponsesOptions) -> Vec<String> {
    let mut include = opts.include.clone();
    if opts
        .reasoning
        .as_ref()
        .is_some_and(|reasoning| reasoning.include_encrypted_content)
        && !include
            .iter()
            .any(|path| path == "reasoning.encrypted_content")
    {
        include.push("reasoning.encrypted_content".into());
    }
    include
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAIStreamOptions {
    include_usage: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAIMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<Vec<OpenAIContentPart>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum OpenAIContentPart {
    Text { text: String },
    ImageUrl { image_url: OpenAIImageUrl },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenAIImageUrl {
    url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIResponse {
    choices: Vec<OpenAIChoice>,
    model: String,
    usage: OpenAIUsage,
}

#[derive(Debug, Deserialize)]
struct OpenAIResponsesResponse {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    output_text: Option<String>,
    #[serde(default)]
    output: Vec<serde_json::Value>,
    #[serde(default)]
    usage: Option<OpenAIResponsesUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAIResponsesUsage {
    #[serde(default)]
    input_tokens: u32,
    #[serde(default)]
    output_tokens: u32,
}

fn extract_responses_text(response: &OpenAIResponsesResponse) -> String {
    if let Some(text) = response.output_text.as_deref()
        && !text.is_empty()
    {
        return text.to_string();
    }

    let mut chunks = Vec::new();
    for item in &response.output {
        let Some(content) = item.get("content").and_then(|v| v.as_array()) else {
            continue;
        };
        for part in content {
            let part_type = part.get("type").and_then(|v| v.as_str());
            if matches!(part_type, Some("output_text" | "text"))
                && let Some(text) = part.get("text").and_then(|v| v.as_str())
            {
                chunks.push(text);
            }
        }
    }
    chunks.join("\n")
}

#[derive(Debug, Deserialize)]
struct OpenAIChoice {
    message: OpenAIResponseMessage,
    #[serde(default)]
    finish_reason: Option<String>,
    #[serde(default)]
    logprobs: Option<OpenAIStreamLogprobs>,
}

// Response content accepts standard Chat Completions text and the existing
// part-array compatibility shape. Keep outgoing OpenAIMessage unchanged.
#[derive(Debug, Deserialize)]
struct OpenAIResponseMessage {
    #[serde(default, deserialize_with = "deserialize_response_content")]
    content: Option<Vec<OpenAIContentPart>>,
}

fn deserialize_response_content<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<Vec<OpenAIContentPart>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Content {
        Text(String),
        Parts(Vec<OpenAIContentPart>),
    }
    Ok(
        Option::<Content>::deserialize(deserializer)?.map(|content| match content {
            Content::Text(text) => vec![OpenAIContentPart::Text { text }],
            Content::Parts(parts) => parts,
        }),
    )
}

#[derive(Debug, Deserialize)]
struct OpenAIUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
}

/// Selected metadata updates may carry no token counters. Never fabricate actual
/// usage from an absent/null/partial pair; malformed supplied counters still fail.
#[derive(Debug, Deserialize)]
pub(super) struct SelectedProviderUsage {
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
}

impl SelectedProviderUsage {
    pub(super) fn actual(&self) -> Option<TokenCount> {
        self.prompt_tokens
            .zip(self.completion_tokens)
            .map(|(prompt, completion)| TokenCount::new(prompt, completion))
    }
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamChunk {
    choices: Vec<OpenAIStreamChoice>,
    #[serde(default)]
    usage: Option<OpenAIUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamChoice {
    delta: OpenAIDelta,
    #[serde(default)]
    finish_reason: Option<String>,
    /// Per-chunk logprob payload. `null` when not requested or when the
    /// chunk carries no token logprobs (e.g. role-only / final chunks).
    #[serde(default)]
    logprobs: Option<OpenAIStreamLogprobs>,
}

#[derive(Debug, Deserialize)]
struct OpenAIDelta {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OpenAIStreamLogprobs {
    #[serde(default)]
    content: Option<Vec<OpenAITokenLogprob>>,
}

#[derive(Debug, Deserialize)]
struct OpenAITokenLogprob {
    token: String,
    logprob: f32,
    #[serde(default)]
    bytes: Option<Vec<u8>>,
    #[serde(default)]
    top_logprobs: Vec<OpenAITopLogprob>,
}

#[derive(Debug, Deserialize)]
struct OpenAITopLogprob {
    token: String,
    logprob: f32,
    #[serde(default)]
    bytes: Option<Vec<u8>>,
}

/// Decode an OpenAI-style streaming logprob payload into the engine type.
///
/// Returns `None` when the payload is absent OR carries no non-empty
/// `content` array (e.g. role-only / final chunks). This helper is reused
/// by other OpenAI-compatible providers (Together, OpenRouter, LM Studio)
/// in Phase 6.
pub(crate) fn decode_oai_logprob_delta(
    payload: Option<OpenAIStreamLogprobs>,
) -> Option<crate::types::StreamLogprobsDelta> {
    let lp = payload?;
    let content = lp.content?;
    if content.is_empty() {
        return None;
    }
    let tokens: Vec<crate::types::TokenLogprob> = content
        .into_iter()
        .map(|t| crate::types::TokenLogprob {
            token: t.token,
            logprob: t.logprob,
            bytes: t.bytes,
            top_logprobs: t
                .top_logprobs
                .into_iter()
                .map(|tl| crate::types::TopLogprob {
                    token: tl.token,
                    logprob: tl.logprob,
                    bytes: tl.bytes,
                })
                .collect(),
        })
        .collect();
    Some(crate::types::StreamLogprobsDelta { content: tokens })
}

#[derive(Debug, Deserialize)]
struct OpenAIModelsResponse {
    data: Vec<OpenAIModel>,
}

#[derive(Debug, Deserialize)]
struct OpenAIModel {
    id: String,
    created: u64,
    owned_by: String,
}

// ============================================================================
/// Shared legacy JSON-stream decoder; provider-specific requests/status remain owned by adapters.
pub(super) fn openai_compatible_stream(
    response: reqwest::Response,
    request: &ChatRequest,
) -> Box<dyn Stream<Item = Result<StreamChunk>> + Send + Unpin> {
    openai_compatible_stream_with_metadata(response, request, "")
}

pub(super) fn merge_provider_metadata(
    current: &mut Option<serde_json::Map<String, serde_json::Value>>,
    data: &serde_json::Value,
    provider: &str,
) -> Result<()> {
    use serde_json::Value;
    let (fields, usage_fields): (&[&str], &[&str]) = match provider {
        "openrouter" => (
            &["id", "provider"],
            &[
                "cost",
                "cost_details",
                "prompt_tokens_details",
                "completion_tokens_details",
            ],
        ),
        "perplexity" => (
            &["citations", "search_results"],
            &[
                "cost",
                "search_context_size",
                "citation_tokens",
                "num_search_queries",
                "reasoning_tokens",
            ],
        ),
        _ => return Ok(()),
    };
    let selected = current.get_or_insert_with(serde_json::Map::new);
    if data.get("error").is_some() {
        return Err(NxuskitError::Stream(
            "provider returned an error envelope".into(),
        ));
    }
    for name in fields {
        if let Some(value) = data.get(*name) {
            selected.insert((*name).into(), value.clone());
        }
    }
    if let Some(usage) = data.get("usage") {
        if usage.is_null() {
            selected.insert("usage".into(), Value::Null);
        } else if let Some(usage) = usage.as_object() {
            for name in usage_fields {
                if let Some(value) = usage.get(*name) {
                    let target = selected
                        .entry("usage")
                        .or_insert_with(|| serde_json::json!({}));
                    if target.is_null() {
                        *target = serde_json::json!({});
                    }
                    target
                        .as_object_mut()
                        .expect("selected usage object")
                        .insert((*name).into(), value.clone());
                }
            }
        } else {
            return Err(NxuskitError::Stream(
                "provider_metadata usage must be object or null".into(),
            ));
        }
    }
    fn depth_ok(value: &Value, depth: usize) -> bool {
        if depth > 16 {
            return false;
        }
        match value {
            Value::Object(values) => values.values().all(|v| depth_ok(v, depth + 1)),
            Value::Array(values) => values.iter().all(|v| depth_ok(v, depth + 1)),
            _ => true,
        }
    }
    if !selected.values().all(|v| depth_ok(v, 1)) {
        return Err(NxuskitError::Stream(
            "provider_metadata depth exceeds 16".into(),
        ));
    }
    if serde_json::to_vec(selected)?.len() > 65536 {
        return Err(NxuskitError::Stream(
            "provider_metadata compact UTF-8 bytes exceed 65536".into(),
        ));
    }
    if selected.is_empty() {
        *current = None;
    }
    Ok(())
}

pub(super) fn openai_compatible_stream_with_metadata(
    response: reqwest::Response,
    request: &ChatRequest,
    provider: &'static str,
) -> Box<dyn Stream<Item = Result<StreamChunk>> + Send + Unpin> {
    let stream = response.bytes_stream();
    let mut buffer = Vec::new();

    // Create token estimator for the model (outside stream! macro to avoid lifetime issues)
    let model = request.model.clone();
    let estimator = TokenEstimator::for_model(&model);
    let prompt_tokens = estimator.count_messages(&request.messages);

    let output_stream = stream! {
        let mut provider_metadata = None;
        // Create accumulator with pre-calculated prompt tokens
        let mut accumulator = StreamingTokenAccumulator::new(estimator, prompt_tokens);

        let mut stream = stream;
        let mut raw_finish_reason = None;

        'stream_loop: loop {
            match stream.next().await {
                Some(Ok(bytes)) => {
                    let mut chunk_cursor =
                        crate::provider::http::BoundedStreamChunkCursor::new(&bytes);
                    loop {
                        let has_more = match chunk_cursor.append_next(&mut buffer) {
                            Ok(has_more) => has_more,
                            Err(error) => { yield Err(NxuskitError::Stream(error.to_string())); return; }
                        };
                    while let Some(frame) = contract::next_openai_sse_frame(&mut buffer) {
                        match frame {
                            Ok(contract::OpenAiSseFrame::Done) => break 'stream_loop,
                            Ok(contract::OpenAiSseFrame::Ignored) => continue,
                            Err(error) => {
                                yield Err(NxuskitError::Stream(format!("{error}")));
                                return;
                            }
                            Ok(contract::OpenAiSseFrame::Data(mut data)) => {
                            if data.get("error").is_some() {
                                yield Err(NxuskitError::Stream("provider returned an error envelope".into()));
                                return;
                            }
                            if let Err(error) = merge_provider_metadata(&mut provider_metadata, &data, provider) {
                                yield Err(error);
                                return;
                            }
                            if matches!(provider, "openrouter" | "perplexity") {
                                match serde_json::from_value::<Option<SelectedProviderUsage>>(data.get("usage").cloned().unwrap_or(serde_json::Value::Null)) {
                                    Ok(usage) => {
                                        if let Some(actual) = usage.and_then(|u| u.actual()) { accumulator.set_actual(actual); }
                                    }
                                    Err(error) => { yield Err(NxuskitError::Stream(format!("Invalid token usage: {error}"))); return; }
                                }
                                let selected_update = data.get("usage").is_some() || match provider {
                                    "openrouter" => data.get("id").is_some() || data.get("provider").is_some(),
                                    _ => data.get("citations").is_some() || data.get("search_results").is_some(),
                                };
                                if let Some(object) = data.as_object_mut() {
                                    // Already handled independently from selected metadata above.
                                    object.remove("usage");
                                    if selected_update { object.entry("choices").or_insert_with(|| serde_json::json!([])); }
                                }
                            }
                            match serde_json::from_value::<OpenAIStreamChunk>(data) {
                                Ok(chunk_data) => {
                                    // Capture actual usage if provided
                                    if let Some(usage) = chunk_data.usage {
                                        let actual = TokenCount::new(usage.prompt_tokens, usage.completion_tokens);
                                        accumulator.set_actual(actual);
                                    }

                                    if let Some(choice) = chunk_data.choices.into_iter().next() {
                                        let logprobs_delta = decode_oai_logprob_delta(choice.logprobs);
                                        if let Some(content) = choice.delta.content {
                                            accumulator.add_chunk(&content);
                                            let mut stream_chunk = StreamChunk::new(content);
                                            stream_chunk.usage = Some(accumulator.running_total());
                                            stream_chunk.logprobs = logprobs_delta;
                                            yield Ok(stream_chunk);
                                        } else if logprobs_delta.is_some() {
                                            // Logprobs without content delta — rare but possible;
                                            // surface via an empty-delta chunk so consumers see them.
                                            let mut stream_chunk = StreamChunk::new(String::new());
                                            stream_chunk.usage = Some(accumulator.running_total());
                                            stream_chunk.logprobs = logprobs_delta;
                                            yield Ok(stream_chunk);
                                        }
                                        if let Some(reason_str) = choice.finish_reason {
                                            raw_finish_reason = Some(reason_str);
                                        }
                                    }
                                }
                                Err(e) => {
                                    yield Err(NxuskitError::Stream(format!("Failed to parse stream chunk: {}", e)));
                                    return;
                                }
                            }
                            }
                        }
                    }
                        if !has_more {
                            break;
                        }
                    }
                }
                Some(Err(e)) => {
                    yield Err(NxuskitError::Network(e));
                    return;
                }
                None => {
                    yield Err(NxuskitError::Stream("stream ended before [DONE]".to_string()));
                    return;
                }
            }
        }

        // Only the protocol sentinel completes the stream; usage can follow a reason.
        let final_usage = accumulator.finalize();
        let mut final_chunk = StreamChunk::new(String::new());
        final_chunk.terminal = true;
        final_chunk.finish_reason = raw_finish_reason.as_deref().and_then(FinishReason::from_str_flexible);
        final_chunk.raw_finish_reason = raw_finish_reason;
        final_chunk.provider_metadata = provider_metadata;
        final_chunk.usage = Some(final_usage);
        yield Ok(final_chunk);
    };

    Box::new(Box::pin(output_stream))
}

#[path = "openai_contract.rs"]
pub(super) mod contract;

pub use contract::OpenAIContractMapper;

#[cfg(test)]
use contract::{OpenAiSseFrame, next_openai_sse_frame};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_metadata_bounds_merge_and_null() {
        let mut selected = None;
        merge_provider_metadata(&mut selected, &serde_json::json!({"provider":"vendor","usage":{"cost":0.25,"cost_details":{"x":null}}}), "openrouter").unwrap();
        merge_provider_metadata(&mut selected, &serde_json::json!({"provider":null,"usage":{"completion_tokens_details":{"reasoning_tokens":3}}}), "openrouter").unwrap();
        assert_eq!(
            serde_json::to_value(&selected).unwrap(),
            serde_json::json!({"provider":null,"usage":{"cost":0.25,"cost_details":{"x":null},"completion_tokens_details":{"reasoning_tokens":3}}})
        );
        merge_provider_metadata(
            &mut selected,
            &serde_json::json!({"usage":null}),
            "openrouter",
        )
        .unwrap();
        merge_provider_metadata(
            &mut selected,
            &serde_json::json!({"usage":{"cost":null}}),
            "openrouter",
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(&selected).unwrap(),
            serde_json::json!({"provider":null,"usage":{"cost":null}})
        );
        for size in [65535, 65536, 65537] {
            let overhead = serde_json::to_vec(&serde_json::json!({"citations":[""]}))
                .unwrap()
                .len();
            let n = size - overhead;
            let value = "é".repeat(n / 2) + &"x".repeat(n % 2);
            let data = serde_json::json!({"citations":[value]});
            assert_eq!(serde_json::to_vec(&data).unwrap().len(), size);
            let result = merge_provider_metadata(&mut None, &data, "perplexity");
            assert_eq!(result.is_ok(), size <= 65536);
            if let Err(error) = result {
                assert!(!error.is_retryable());
            }
        }
        for depth in [16, 17] {
            let mut value = serde_json::json!("leaf");
            for _ in 0..depth - 1 {
                value = serde_json::json!([value]);
            }
            assert_eq!(
                merge_provider_metadata(
                    &mut None,
                    &serde_json::json!({"citations":value}),
                    "perplexity"
                )
                .is_ok(),
                depth == 16
            );
        }
        assert!(
            merge_provider_metadata(&mut None, &serde_json::json!({"usage":42}), "openrouter")
                .is_err()
        );
    }

    #[tokio::test]
    async fn selected_metadata_stream_overflow_is_terminal_error() {
        let body = format!(
            "data: {{\"choices\":[{{\"delta\":{{\"content\":\"text\"}}}}]}}\n\ndata: {{\"choices\":[],\"id\":\"{}\"}}\n\ndata: {{\"choices\":[],\"provider\":\"{}\"}}\n\ndata: [DONE]\n\n",
            "x".repeat(33000),
            "x".repeat(33000)
        );
        let (p, server) = legacy_transport_provider(&body);
        let response = p
            .client
            .post(format!("{}/chat/completions", p.base_url))
            .json(&serde_json::json!({}))
            .send()
            .await
            .unwrap();
        let chunks: Vec<_> = openai_compatible_stream_with_metadata(
            response,
            &ChatRequest::new("fixture"),
            "openrouter",
        )
        .collect()
        .await;
        server.join().unwrap();
        assert_eq!(
            chunks
                .iter()
                .filter_map(|c| c.as_ref().ok())
                .map(|c| c.delta.as_str())
                .collect::<String>(),
            "text"
        );
        assert_eq!(chunks.iter().filter(|c| c.is_err()).count(), 1);
        assert!(
            !chunks
                .iter()
                .any(|c| c.as_ref().is_ok_and(StreamChunk::is_final))
        );
        let error = chunks.last().unwrap().as_ref().unwrap_err();
        assert!(error.to_string().contains("provider_metadata"));
        assert!(!error.is_retryable());
    }

    fn legacy_transport_provider(body: &str) -> (OpenAIProvider, std::thread::JoinHandle<()>) {
        legacy_transport_provider_at(body, "/chat/completions")
    }

    fn legacy_transport_provider_at(
        body: &str,
        path: &'static str,
    ) -> (OpenAIProvider, std::thread::JoinHandle<()>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let body = body.to_owned();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut headers = Vec::new();
            let mut byte = [0];
            while !headers.ends_with(b"\r\n\r\n") {
                socket.read_exact(&mut byte).unwrap();
                headers.push(byte[0]);
            }
            let headers = String::from_utf8(headers).unwrap();
            assert!(headers.starts_with(&format!("POST {path} HTTP/1.1\r\n")));
            let length: usize = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse().unwrap())
                })
                .unwrap();
            socket.read_exact(&mut vec![0; length]).unwrap();
            write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
        });
        let provider = OpenAIProvider::builder()
            .api_key("loopback-only")
            .base_url(format!("http://{address}"))
            .build()
            .unwrap();
        (provider, server)
    }

    async fn legacy_transport_stream(body: &str) -> Vec<Result<StreamChunk>> {
        let (provider, server) = legacy_transport_provider(body);
        let events = tokio::time::timeout(Duration::from_secs(5), async {
            provider
                .chat_stream(&ChatRequest::new("gpt-4o"))
                .await
                .unwrap()
                .collect::<Vec<_>>()
                .await
        })
        .await
        .unwrap();
        server.join().unwrap();
        events
    }

    #[tokio::test]
    async fn legacy_transport_unary_raw_reason_truth() {
        for (wire, raw, canonical) in [
            ("\"length\"", Some("length"), Some(FinishReason::Length)),
            ("\"vendor-new\"", Some("vendor-new"), None),
            ("null", None, None),
        ] {
            let body = format!(
                "{{\"model\":\"actual\",\"choices\":[{{\"message\":{{\"content\":\"ok\"}},\"finish_reason\":{wire}}}],\"usage\":{{\"prompt_tokens\":7,\"completion_tokens\":3}}}}"
            );
            let (provider, server) = legacy_transport_provider(&body);
            let response = provider.chat(&ChatRequest::new("gpt-4o")).await.unwrap();
            server.join().unwrap();
            assert_eq!(response.model, "actual");
            assert_eq!(response.finish_reason, canonical);
            assert_eq!(response.raw_finish_reason.as_deref(), raw);
            assert_eq!(response.inference_metadata.finish_reason, canonical);
        }
    }

    #[tokio::test]
    async fn legacy_responses_terminal_truth() {
        for (status, raw, canonical, complete, failure) in [
            (
                Some("completed"),
                Some("completed"),
                Some(FinishReason::Stop),
                true,
                false,
            ),
            (Some("vendor-new"), Some("vendor-new"), None, false, false),
            (None, None, None, false, false),
            (Some("incomplete"), Some("incomplete"), None, true, false),
            (Some("failed"), None, None, false, true),
            (Some("cancelled"), None, None, false, true),
        ] {
            let body=serde_json::json!({"status":status,"model":"actual","output_text":"partial","usage":{"input_tokens":7,"output_tokens":3}}).to_string();
            let (p, server) = legacy_transport_provider_at(&body, "/responses");
            let mut request = ChatRequest::new("gpt-5.5");
            request.openai_responses = Some(crate::capabilities::OpenAIResponsesOptions::default());
            let result = p.chat(&request).await;
            server.join().unwrap();
            if failure {
                assert!(result.is_err(), "{status:?}: {result:?}");
                continue;
            }
            let response = result.unwrap();
            assert_eq!(response.raw_finish_reason.as_deref(), raw, "{status:?}");
            assert_eq!(response.finish_reason, canonical, "{status:?}");
            assert_eq!(
                response.inference_metadata.finish_reason, canonical,
                "{status:?}"
            );
            assert_eq!(
                response.inference_metadata.is_complete, complete,
                "{status:?}"
            );
        }
    }

    #[tokio::test]
    async fn legacy_request_controls_reach_wire() {
        use std::io::{Read, Write};
        for streaming in [false, true] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let endpoint = format!("http://{}", listener.local_addr().unwrap());
            let server = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut headers = Vec::new();
                let mut byte = [0];
                while !headers.ends_with(b"\r\n\r\n") {
                    socket.read_exact(&mut byte).unwrap();
                    headers.push(byte[0]);
                }
                let headers = String::from_utf8(headers).unwrap();
                assert!(headers.starts_with("POST /chat/completions HTTP/1.1"));
                let length: usize = headers
                    .lines()
                    .find_map(|line| {
                        let (k, v) = line.split_once(':')?;
                        k.eq_ignore_ascii_case("content-length")
                            .then(|| v.trim().parse().unwrap())
                    })
                    .unwrap();
                let mut bytes = vec![0; length];
                socket.read_exact(&mut bytes).unwrap();
                let request: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                let body = if streaming {
                    "data: [DONE]\n\n"
                } else {
                    "{\"model\":\"actual\",\"choices\":[{\"message\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1}}"
                };
                write!(
                    socket,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
                .unwrap();
                request
            });
            let p = OpenAIProvider::builder()
                .api_key("loopback")
                .base_url(endpoint)
                .build()
                .unwrap();
            let mut request = ChatRequest::new("gpt-4o").with_message(Message::user("hi"));
            request.stop = Some(vec!["END".into()]);
            request.presence_penalty = Some(0.5);
            request.frequency_penalty = Some(-0.5);
            request.seed = Some(7);
            request.logprobs = Some(true);
            request.top_logprobs = Some(3);
            request.reasoning_effort = Some("low".into());
            request.response_format = Some(crate::types::ResponseFormat::JsonSchema {
                schema: serde_json::json!({"type":"object","properties":{"ok":{"type":"boolean"}},"required":["ok"],"additionalProperties":false}),
            });
            request.tools = Some(vec![
                serde_json::json!({"type":"function","function":{"name":"lookup","parameters":{"type":"object","properties":{}}}}),
            ]);
            request.tool_choice = Some(serde_json::json!("required"));
            if streaming {
                let chunks: Vec<_> = p.chat_stream(&request).await.unwrap().collect().await;
                assert!(chunks.iter().all(Result::is_ok));
            } else {
                p.chat(&request).await.unwrap();
            }
            let wire = server.join().unwrap();
            assert_eq!(wire["stop"], serde_json::json!(["END"]));
            assert_eq!(wire["presence_penalty"], 0.5);
            assert_eq!(wire["frequency_penalty"], -0.5);
            assert_eq!(wire["seed"], 7);
            assert_eq!(wire["logprobs"], true);
            assert_eq!(wire["top_logprobs"], 3);
            assert_eq!(wire["reasoning_effort"], "low");
            assert_eq!(
                wire["response_format"],
                serde_json::json!({"type":"json_schema","json_schema":{"name":"response","strict":true,"schema":{"type":"object","properties":{"ok":{"type":"boolean"}},"required":["ok"],"additionalProperties":false}}})
            );
            assert_eq!(
                wire["tools"],
                serde_json::json!([{"type":"function","function":{"name":"lookup","parameters":{"type":"object","properties":{}}}}])
            );
            assert_eq!(wire["tool_choice"], "required");
        }
    }

    #[tokio::test]
    async fn legacy_transport_terminal_failures_never_succeed() {
        for body in [
            "",
            "data: {broken}\n\n",
            "data: {\"choices\":[]}",
            "data: {\"choices\":[{\"delta\":{\"content\":\"partial\"},\"finish_reason\":null}]}\n\n",
        ] {
            let events = legacy_transport_stream(body).await;
            assert_eq!(
                events.iter().filter(|event| event.is_err()).count(),
                1,
                "{body}: {events:?}"
            );
            assert!(events.last().unwrap().is_err(), "{events:?}");
            assert!(
                !events
                    .iter()
                    .any(|event| event.as_ref().is_ok_and(StreamChunk::is_final)),
                "{events:?}"
            );
        }
    }

    #[tokio::test]
    async fn legacy_transport_terminal_reason_and_late_usage() {
        for (wire, raw, canonical) in [
            ("\"length\"", Some("length"), Some(FinishReason::Length)),
            ("\"vendor-new\"", Some("vendor-new"), None),
            ("null", None, None),
        ] {
            let body = format!(
                "data: {{\"choices\":[{{\"delta\":{{\"content\":\"€\"}},\"finish_reason\":{wire}}}]}}\r\n\r\ndata: {{\"choices\":[],\"usage\":{{\"prompt_tokens\":7,\"completion_tokens\":3}}}}\r\n\r\ndata: [DONE]\r\n\r\ndata: {{broken}}\r\n\r\n"
            );
            let events = legacy_transport_stream(&body).await;
            assert!(events.iter().all(Result::is_ok), "{events:?}");
            let chunks: Vec<_> = events.into_iter().map(Result::unwrap).collect();
            assert_eq!(
                chunks
                    .iter()
                    .map(|chunk| chunk.delta.as_str())
                    .collect::<String>(),
                "€"
            );
            assert_eq!(chunks.iter().filter(|chunk| chunk.is_final()).count(), 1);
            let final_chunk = chunks.last().unwrap();
            assert!(final_chunk.terminal);
            assert_eq!(final_chunk.finish_reason, canonical);
            assert_eq!(final_chunk.raw_finish_reason.as_deref(), raw);
            let actual = final_chunk.usage.as_ref().unwrap().actual.as_ref().unwrap();
            assert_eq!((actual.prompt_tokens, actual.completion_tokens), (7, 3));
        }
    }

    #[test]
    fn sse_framer_accepts_crlf_and_preserves_split_utf8() {
        let mut buffer = br#"data: {"choices":[{"delta":{"content":""#.to_vec();
        buffer.push(0xe2);
        assert!(next_openai_sse_frame(&mut buffer).is_none());
        buffer.extend_from_slice(&[0x82, 0xac]);
        buffer.extend_from_slice(b"\"},\"finish_reason\":null}]}\r\n\r\n");
        let frame = next_openai_sse_frame(&mut buffer).unwrap().unwrap();
        let OpenAiSseFrame::Data(value) = frame else {
            panic!("expected data frame");
        };
        assert_eq!(value["choices"][0]["delta"]["content"], "€");

        let mut invalid = b"data: {\xff}\n\n".to_vec();
        let Err(error) = next_openai_sse_frame(&mut invalid).unwrap() else {
            panic!("expected malformed response error");
        };
        assert_eq!(
            error.code,
            crate::error::ProviderErrorCode::MalformedResponse
        );
    }

    #[test]
    fn test_builder_missing_api_key() {
        let result = OpenAIProvider::builder().build();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            NxuskitError::Configuration(_)
        ));
    }

    #[test]
    fn test_builder_with_api_key() {
        let provider = OpenAIProvider::builder()
            .api_key("test-key")
            .build()
            .unwrap();
        assert_eq!(provider.provider_name(), "openai");
    }

    #[test]
    fn test_debug_redacts_api_key() {
        let provider = OpenAIProvider::builder()
            .api_key("secret-key")
            .build()
            .unwrap();
        let debug_str = format!("{:?}", provider);
        assert!(!debug_str.contains("secret-key"));
        assert!(debug_str.contains("[REDACTED]"));
    }

    #[test]
    fn test_message_conversion() {
        let provider = OpenAIProvider::builder().api_key("test").build().unwrap();

        let messages = vec![
            Message::system("You are helpful"),
            Message::user("Hello"),
            Message::assistant("Hi there"),
        ];

        let openai_msgs = provider.convert_messages(&messages);
        assert_eq!(openai_msgs.len(), 3);
        assert_eq!(openai_msgs[0].role, "system");
        assert_eq!(openai_msgs[1].role, "user");
        assert_eq!(openai_msgs[2].role, "assistant");
    }

    #[test]
    fn test_build_stream_request_includes_logprobs() {
        let provider = OpenAIProvider::builder().api_key("test").build().unwrap();
        let mut request = ChatRequest::new("gpt-5.4")
            .with_message(Message::user("Score the next token."))
            .with_reasoning_effort("none");
        request.logprobs = Some(true);
        request.top_logprobs = Some(3);

        let (openai_request, adapted) = provider.build_openai_request(&request, true).unwrap();
        let value = serde_json::to_value(openai_request).unwrap();

        assert!(adapted.warnings.is_empty());
        assert_eq!(value["stream"], true);
        assert_eq!(value["logprobs"], true);
        assert_eq!(value["top_logprobs"], 3);
        assert_eq!(value["stream_options"]["include_usage"], true);
    }

    #[test]
    fn test_build_stream_request_applies_gpt54_reasoning_guard() {
        let provider = OpenAIProvider::builder().api_key("test").build().unwrap();
        let mut request = ChatRequest::new("gpt-5.4")
            .with_message(Message::user("Score the next token."))
            .with_temperature(0.7)
            .with_top_p(0.9)
            .with_reasoning_effort("medium");
        request.logprobs = Some(true);
        request.top_logprobs = Some(3);

        let (openai_request, adapted) = provider.build_openai_request(&request, true).unwrap();
        let value = serde_json::to_value(openai_request).unwrap();

        assert!(value.get("temperature").is_none());
        assert!(value.get("top_p").is_none());
        assert!(value.get("logprobs").is_none());
        assert!(value.get("top_logprobs").is_none());
        assert!(
            adapted
                .warnings
                .iter()
                .any(|w| w.parameter == "logprobs" && w.message.contains("reasoning.effort"))
        );
    }

    // ---------------------------------------------------------------------
    // T035: typed structured-output and tool-call serialization through the
    // OpenAI adapter. Each test asserts on the actual request JSON bytes.
    // ---------------------------------------------------------------------

    #[test]
    fn test_build_request_serializes_typed_structured_output_json_schema() {
        use crate::capabilities::{StructuredOutputConfig, StructuredOutputMode};

        let provider = OpenAIProvider::builder().api_key("test").build().unwrap();
        let mut request = ChatRequest::new("gpt-5.5")
            .with_message(Message::user("Return a country/capital pair."));
        request.structured_output = Some(StructuredOutputConfig {
            mode: StructuredOutputMode::JsonSchema,
            schema: Some(serde_json::json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["country", "capital"],
                "properties": {
                    "country": {"type": "string"},
                    "capital": {"type": "string"}
                }
            })),
            schema_name: Some("country_capital".into()),
            strict: Some(true),
            schema_subset: None,
        });

        let (openai_request, _adapted) = provider.build_openai_request(&request, false).unwrap();
        let value = serde_json::to_value(openai_request).unwrap();

        assert_eq!(value["response_format"]["type"], "json_schema");
        assert_eq!(
            value["response_format"]["json_schema"]["name"],
            "country_capital"
        );
        assert_eq!(value["response_format"]["json_schema"]["strict"], true);
        assert_eq!(
            value["response_format"]["json_schema"]["schema"]["type"],
            "object"
        );
        assert_eq!(
            value["response_format"]["json_schema"]["schema"]["additionalProperties"],
            false
        );
        // Tool-call fields must not appear when no tool_call_config is set.
        assert!(value.get("tools").is_none());
        assert!(value.get("tool_choice").is_none());
        assert!(value.get("parallel_tool_calls").is_none());
    }

    #[test]
    fn test_build_request_serializes_typed_structured_output_json_object() {
        use crate::capabilities::{StructuredOutputConfig, StructuredOutputMode};

        let provider = OpenAIProvider::builder().api_key("test").build().unwrap();
        let mut request =
            ChatRequest::new("gpt-5.5").with_message(Message::user("Reply with JSON."));
        request.structured_output = Some(StructuredOutputConfig {
            mode: StructuredOutputMode::JsonObject,
            schema: None,
            schema_name: None,
            strict: None,
            schema_subset: None,
        });

        let (openai_request, _adapted) = provider.build_openai_request(&request, false).unwrap();
        let value = serde_json::to_value(openai_request).unwrap();
        assert_eq!(value["response_format"]["type"], "json_object");
    }

    #[test]
    fn test_build_request_serializes_typed_tools_and_tool_choice() {
        use crate::capabilities::{ToolCallConfig, ToolChoice, ToolDefinition};

        let provider = OpenAIProvider::builder().api_key("test").build().unwrap();
        let mut request = ChatRequest::new("gpt-5.5").with_message(Message::user("Call a tool."));
        request.tool_call_config = Some(ToolCallConfig {
            tools: vec![
                ToolDefinition {
                    name: "search_inventory".into(),
                    description: Some("Search the warehouse inventory.".into()),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {"sku": {"type": "string"}},
                        "required": ["sku"],
                    }),
                    strict: Some(true),
                },
                ToolDefinition {
                    name: "lookup_user".into(),
                    description: None,
                    parameters: serde_json::json!({"type": "object"}),
                    strict: None,
                },
            ],
            tool_choice: ToolChoice::Named("search_inventory".into()),
            parallel_tool_calls: Some(false),
            streaming_tool_calls: None,
        });

        let (openai_request, _adapted) = provider.build_openai_request(&request, false).unwrap();
        let value = serde_json::to_value(openai_request).unwrap();

        // tools[]: two function entries, second one drops description and strict.
        let tools = value["tools"].as_array().expect("tools is an array");
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0]["type"], "function");
        assert_eq!(tools[0]["function"]["name"], "search_inventory");
        assert_eq!(
            tools[0]["function"]["description"],
            "Search the warehouse inventory."
        );
        assert_eq!(tools[0]["function"]["strict"], true);
        assert_eq!(tools[1]["function"]["name"], "lookup_user");
        assert!(tools[1]["function"].get("description").is_none());
        assert!(tools[1]["function"].get("strict").is_none());

        // tool_choice: Named maps to {type: "function", function: {name: ...}}.
        assert_eq!(value["tool_choice"]["type"], "function");
        assert_eq!(value["tool_choice"]["function"]["name"], "search_inventory");

        // parallel_tool_calls=false carried through.
        assert_eq!(value["parallel_tool_calls"], false);
    }

    #[test]
    fn test_build_request_omits_typed_fields_when_unset() {
        let provider = OpenAIProvider::builder().api_key("test").build().unwrap();
        let request = ChatRequest::new("gpt-5.5")
            .with_message(Message::user("Plain chat — no typed surfaces."));

        let (openai_request, _adapted) = provider.build_openai_request(&request, false).unwrap();
        let value = serde_json::to_value(openai_request).unwrap();

        assert!(value.get("response_format").is_none());
        assert!(value.get("tools").is_none());
        assert!(value.get("tool_choice").is_none());
        assert!(value.get("parallel_tool_calls").is_none());
    }

    #[test]
    fn test_build_request_blocks_malformed_typed_json_schema_pre_network() {
        use crate::capabilities::{StructuredOutputConfig, StructuredOutputMode};

        let provider = OpenAIProvider::builder().api_key("test").build().unwrap();
        let mut request = ChatRequest::new("gpt-5.5").with_message(Message::user("Return JSON."));
        request.structured_output = Some(StructuredOutputConfig {
            mode: StructuredOutputMode::JsonSchema,
            schema: None,
            schema_name: Some("missing_schema".into()),
            strict: Some(true),
            schema_subset: None,
        });

        let err = provider
            .build_openai_request(&request, false)
            .expect_err("malformed typed JsonSchema must block before serialization");
        assert!(
            matches!(err, NxuskitError::InvalidRequest(ref msg) if msg.contains("structured_output")),
            "expected InvalidRequest for structured_output, got {err:?}"
        );
    }

    #[test]
    fn test_build_responses_request_serializes_responses_controls() {
        use crate::capabilities::{OpenAIResponsesOptions, ReasoningConfig, TextVerbosity};

        let provider = OpenAIProvider::builder().api_key("test").build().unwrap();
        let mut request = ChatRequest::new("gpt-5.5")
            .with_message(Message::user("Plan the release."))
            .with_max_tokens(300);
        request.openai_responses = Some(OpenAIResponsesOptions {
            reasoning: Some(ReasoningConfig {
                effort: Some("high".into()),
                summary: Some("auto".into()),
                include_encrypted_content: false,
                preserve_blocks: false,
            }),
            text_verbosity: Some(TextVerbosity::High),
            previous_response_id: Some("resp_previous".into()),
            include: vec!["reasoning.encrypted_content".into()],
            phase: Some("implementation".into()),
            hosted_tools: vec![],
            tool_search: None,
        });

        let (responses_request, _adapted) =
            provider.build_openai_responses_request(&request).unwrap();
        let value = serde_json::to_value(responses_request).unwrap();

        assert_eq!(value["model"], "gpt-5.5");
        assert_eq!(value["max_output_tokens"], 300);
        assert_eq!(value["reasoning"]["effort"], "high");
        assert_eq!(value["reasoning"]["summary"], "auto");
        assert_eq!(value["text"]["format"]["type"], "text");
        assert_eq!(value["text"]["verbosity"], "high");
        assert_eq!(value["previous_response_id"], "resp_previous");
        assert_eq!(value["include"][0], "reasoning.encrypted_content");
        assert_eq!(value["phase"], "implementation");
    }

    #[test]
    fn test_responses_hosted_tools_and_tool_search_are_explicit_opt_in() {
        use crate::capabilities::OpenAIResponsesOptions;

        let provider = OpenAIProvider::builder().api_key("test").build().unwrap();
        let mut default_request =
            ChatRequest::new("gpt-5.5").with_message(Message::user("No tools."));
        default_request.openai_responses = Some(OpenAIResponsesOptions::default());
        let (responses_request, _adapted) = provider
            .build_openai_responses_request(&default_request)
            .unwrap();
        let value = serde_json::to_value(responses_request).unwrap();
        assert!(value.get("tools").is_none());
        assert!(value.get("tool_search").is_none());

        let mut explicit_request =
            ChatRequest::new("gpt-5.5").with_message(Message::user("Search."));
        explicit_request.openai_responses = Some(OpenAIResponsesOptions {
            hosted_tools: vec![serde_json::json!({
                "type": "web_search_preview",
                "search_context_size": "medium"
            })],
            tool_search: Some(serde_json::json!({"max_results": 5})),
            ..OpenAIResponsesOptions::default()
        });
        let (responses_request, _adapted) = provider
            .build_openai_responses_request(&explicit_request)
            .unwrap();
        let value = serde_json::to_value(responses_request).unwrap();
        assert_eq!(value["tools"][0]["type"], "web_search_preview");
        assert_eq!(value["tool_search"]["max_results"], 5);
    }

    #[test]
    fn test_responses_response_extracts_output_text_and_usage() {
        let parsed: OpenAIResponsesResponse = serde_json::from_value(serde_json::json!({
            "id": "resp_123",
            "model": "gpt-5.5",
            "status": "completed",
            "output": [
                {
                    "type": "message",
                    "content": [
                        {"type": "output_text", "text": "Release plan complete."}
                    ]
                }
            ],
            "usage": {
                "input_tokens": 12,
                "output_tokens": 4
            }
        }))
        .unwrap();

        assert_eq!(parsed.id.as_deref(), Some("resp_123"));
        assert_eq!(extract_responses_text(&parsed), "Release plan complete.");
        let usage = parsed.usage.expect("usage parses");
        assert_eq!(usage.input_tokens, 12);
        assert_eq!(usage.output_tokens, 4);
    }

    #[test]
    fn test_chat_completions_omits_responses_only_fields_when_unset() {
        let provider = OpenAIProvider::builder().api_key("test").build().unwrap();
        let request = ChatRequest::new("gpt-5.5").with_message(Message::user("Plain chat."));

        let (openai_request, _adapted) = provider.build_openai_request(&request, false).unwrap();
        let value = serde_json::to_value(openai_request).unwrap();
        assert!(value.get("reasoning").is_none());
        assert!(value.get("text").is_none());
        assert!(value.get("previous_response_id").is_none());
        assert!(value.get("include").is_none());
        assert!(value.get("phase").is_none());
        assert!(value.get("tool_search").is_none());
    }

    #[test]
    fn test_chat_completions_rejects_responses_only_fields() {
        use crate::capabilities::OpenAIResponsesOptions;

        let provider = OpenAIProvider::builder().api_key("test").build().unwrap();
        let mut request =
            ChatRequest::new("gpt-5.5").with_message(Message::user("Use Responses controls."));
        request.openai_responses = Some(OpenAIResponsesOptions::default());

        let err = provider
            .build_openai_request(&request, false)
            .expect_err("Responses-only options must not pass through Chat Completions");
        assert!(
            matches!(err, NxuskitError::InvalidRequest(ref msg) if msg.contains("Responses")),
            "expected InvalidRequest for Responses-only fields, got {err:?}"
        );
    }

    #[tokio::test]
    #[ignore] // Requires valid OpenAI API key
    async fn test_list_models() {
        use std::env;
        let api_key = env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY not set");
        let provider = OpenAIProvider::builder().api_key(api_key).build().unwrap();
        let models = provider.list_models().await.unwrap();

        // Verify we get models from the API
        assert!(!models.is_empty(), "Should return at least one model");

        // Verify all models have the expected structure from API
        for model in &models {
            assert!(!model.name.is_empty(), "Model should have a name");
            assert!(
                model.metadata.contains_key("created"),
                "Model should have 'created' metadata"
            );
            assert!(
                model.metadata.contains_key("owned_by"),
                "Model should have 'owned_by' metadata"
            );
        }
    }
}
