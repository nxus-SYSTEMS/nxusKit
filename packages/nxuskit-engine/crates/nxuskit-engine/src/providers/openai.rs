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
            .unwrap_or(serde_json::Value::Null);
        let (tools, tool_choice, parallel_tool_calls) =
            match adapted_request.tool_call_config.as_ref() {
                Some(cfg) => (
                    crate::capabilities::openai_wire::tools_for(&record, cfg),
                    crate::capabilities::openai_wire::tool_choice_for(&record, cfg),
                    cfg.parallel_tool_calls,
                ),
                None => (serde_json::Value::Null, serde_json::Value::Null, None),
            };

        Ok((
            OpenAIRequest {
                model: adapted_request.model.clone(),
                messages: self.convert_messages(&adapted_request.messages),
                temperature: adapted_request.temperature,
                max_tokens: adapted_request.max_tokens,
                top_p: adapted_request.top_p,
                logprobs: adapted_request.logprobs,
                top_logprobs: adapted_request.top_logprobs,
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
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(match status.as_u16() {
                401 | 403 => NxuskitError::Authentication(error_text),
                429 => NxuskitError::rate_limit(retry_after),
                _ => NxuskitError::provider(status.as_u16(), error_text),
            });
        }

        let responses_response: OpenAIResponsesResponse = response.json().await?;
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
            Some("incomplete") => FinishReason::Length,
            Some("failed") | Some("cancelled") => FinishReason::Error,
            _ => FinishReason::Stop,
        };

        let mut chat_response = ChatResponse::new(content, model, usage);
        chat_response.provider = self.provider_name().to_string();
        chat_response.finish_reason = Some(finish_reason);
        chat_response.warnings = adapted.warnings;
        chat_response.inference_metadata = InferenceMetadata::completed(finish_reason)
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
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(match status.as_u16() {
                401 | 403 => NxuskitError::Authentication(error_text),
                429 => NxuskitError::rate_limit(retry_after),
                _ => NxuskitError::provider(status.as_u16(), error_text),
            });
        }

        let openai_response: OpenAIResponse = response.json().await?;

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
        response.logprobs = logprobs;

        // Add parameter adaptation warnings
        response.warnings = adapted.warnings;

        // Populate inference metadata
        response.inference_metadata =
            InferenceMetadata::completed(response.finish_reason.unwrap_or(FinishReason::Stop))
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
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(match status.as_u16() {
                401 | 403 => NxuskitError::Authentication(error_text),
                429 => NxuskitError::rate_limit(retry_after),
                _ => NxuskitError::provider(status.as_u16(), error_text),
            });
        }

        let stream = response.bytes_stream();
        let mut buffer = String::new();

        // Create token estimator for the model (outside stream! macro to avoid lifetime issues)
        let model = adapted.request.model.clone();
        let estimator = TokenEstimator::for_model(&model);
        let prompt_tokens = estimator.count_messages(&adapted.request.messages);

        let output_stream = stream! {
            // Create accumulator with pre-calculated prompt tokens
            let mut accumulator = StreamingTokenAccumulator::new(estimator, prompt_tokens);

            let mut stream = stream;
            let mut finish_reason = crate::types::FinishReason::Stop;

            'stream_loop: loop {
                match stream.next().await {
                    Some(Ok(bytes)) => {
                        let text = String::from_utf8_lossy(&bytes);
                        buffer.push_str(&text);

                        // Process complete SSE events
                        while let Some(pos) = buffer.find("\n\n") {
                            let event = buffer[..pos].to_string();
                            buffer = buffer[pos + 2..].to_string();

                            if let Some(data) = event.strip_prefix("data: ") {
                                if data == "[DONE]" {
                                    // Done - finalize on next iteration
                                    break 'stream_loop;
                                }

                                match serde_json::from_str::<OpenAIStreamChunk>(data) {
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
                                                finish_reason = crate::types::FinishReason::from_str_flexible(&reason_str)
                                                    .unwrap_or(crate::types::FinishReason::Stop);
                                                break 'stream_loop;
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        yield Err(NxuskitError::Stream(format!("Failed to parse stream chunk: {}", e)));
                                        break 'stream_loop;
                                    }
                                }
                            }
                        }
                    }
                    Some(Err(e)) => {
                        accumulator.mark_interrupted();
                        finish_reason = crate::types::FinishReason::Error;
                        yield Err(NxuskitError::Network(e));
                        break 'stream_loop;
                    }
                    None => break 'stream_loop,
                }
            }

            // Send final chunk after stream ends
            let final_usage = accumulator.finalize();
            yield Ok(StreamChunk::final_chunk(finish_reason, Some(final_usage)));
        };

        Ok(Box::new(Box::pin(output_stream)))
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
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(NxuskitError::provider(status.as_u16(), error_text));
        }

        let models_response: OpenAIModelsResponse = response.json().await?;

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
    logprobs: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_logprobs: Option<u8>,
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
    message: OpenAIMessage,
    #[serde(default)]
    finish_reason: Option<String>,
    #[serde(default)]
    logprobs: Option<OpenAIStreamLogprobs>,
}

#[derive(Debug, Deserialize)]
struct OpenAIUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
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

#[cfg(test)]
mod tests {
    use super::*;

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
