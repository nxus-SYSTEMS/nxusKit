//! Claude (Anthropic) provider implementation

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
        ContentPart, FinishReason, ImageData, InferenceMetadata, MessageContent,
        ProviderCapabilities,
    },
};

const CLAUDE_API_BASE: &str = "https://api.anthropic.com/v1";
const ANTHROPIC_VERSION: &str = "2023-06-01";

fn claude_finish_reason(raw: &str) -> Option<FinishReason> {
    if raw == "tool_use" {
        Some(FinishReason::ToolCalls)
    } else {
        FinishReason::from_str_flexible(raw)
    }
}

/// Claude provider for Anthropic's API
#[derive(Clone)]
pub struct ClaudeProvider {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
    default_model: String,
    connection_timeout: Duration,
    stream_read_timeout: Duration,
    total_timeout: Duration,
}

impl ClaudeProvider {
    /// Static adapter declarations, not model or endpoint-effective support.
    pub fn adapter_capabilities() -> ProviderCapabilities {
        // Anthropic does not expose logprobs on either unary or streaming
        // responses. Per FR-007, the streaming-logprobs flag stays `false`
        // and the SSE handler MUST emit `logprobs: None` on every chunk —
        // never phantom data.
        ProviderCapabilities {
            supports_system_messages: true,
            supports_streaming: true,
            supports_vision: true,
            max_stop_sequences: Some(4),
            supports_presence_penalty: false,
            supports_frequency_penalty: false,
            supports_seed: false,
            supports_logprobs: false,
            supports_streaming_logprobs: false,
            supports_json_mode: false,
            supports_json_schema: false,
            penalty_range: None,
            max_logprobs: None,
        }
    }
}

impl ClaudeProvider {
    /// Create a new Claude provider builder
    pub fn builder() -> ClaudeProviderBuilder {
        ClaudeProviderBuilder::default()
    }

    /// Convert our Message format to Claude's format
    fn convert_messages(&self, messages: &[Message]) -> (Option<String>, Vec<ClaudeMessage>) {
        let mut system = None;
        let mut claude_messages = Vec::new();

        for msg in messages {
            match msg.role {
                Role::System => {
                    // Extract text from system message
                    system = Some(match &msg.content {
                        MessageContent::Text(text) => text.clone(),
                        MessageContent::Parts(parts) => {
                            // Concatenate text parts for system message
                            parts
                                .iter()
                                .filter_map(|p| match p {
                                    ContentPart::Text { text } => Some(text.as_str()),
                                    ContentPart::Image { .. } => None,
                                })
                                .collect::<Vec<_>>()
                                .join("\n")
                        }
                    });
                }
                Role::User | Role::Assistant => {
                    let content_parts = match &msg.content {
                        MessageContent::Text(text) => {
                            vec![ClaudeContentPart::Text { text: text.clone() }]
                        }
                        MessageContent::Parts(parts) => parts
                            .iter()
                            .map(|part| match part {
                                ContentPart::Text { text } => {
                                    ClaudeContentPart::Text { text: text.clone() }
                                }
                                ContentPart::Image { source } => ClaudeContentPart::Image {
                                    source: match &source.data {
                                        ImageData::Url { url } => ClaudeImageSource {
                                            source_type: "url".to_string(),
                                            media_type: None,
                                            data: None,
                                            url: Some(url.clone()),
                                        },
                                        ImageData::Base64 { media_type, data } => {
                                            ClaudeImageSource {
                                                source_type: "base64".to_string(),
                                                media_type: Some(media_type.clone()),
                                                data: Some(data.clone()),
                                                url: None,
                                            }
                                        }
                                    },
                                },
                            })
                            .collect(),
                    };

                    claude_messages.push(ClaudeMessage {
                        role: match msg.role {
                            Role::User => "user".to_string(),
                            Role::Assistant => "assistant".to_string(),
                            _ => unreachable!(),
                        },
                        content: Some(content_parts),
                    });
                }
            }
        }

        (system, claude_messages)
    }
    fn validate_request_surface(&self, request: &ChatRequest, streaming: bool) -> Result<()> {
        request.validate_legacy_sampling(self.provider_name())?;

        if let Some(stops) = request.stop.as_ref()
            && (stops.len() > 4 || stops.iter().any(String::is_empty))
        {
            return Err(NxuskitError::InvalidRequest(
                "Claude stop sequences must contain 0 to 4 nonempty strings".into(),
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

        for (capability, present) in [
            ("presence_penalty", request.presence_penalty.is_some()),
            ("frequency_penalty", request.frequency_penalty.is_some()),
            ("seed", request.seed.is_some()),
            ("logprobs", request.logprobs.is_some()),
            ("top_logprobs", request.top_logprobs.is_some()),
            ("reasoning_effort", request.reasoning_effort.is_some()),
        ] {
            if present {
                return Err(NxuskitError::unsupported_capability("claude", capability));
            }
        }

        if !matches!(
            request.thinking_mode,
            crate::types::ThinkingMode::Auto | crate::types::ThinkingMode::Omit
        ) {
            return Err(NxuskitError::unsupported_capability("claude", "thinking"));
        }

        if let Some(crate::types::ResponseFormat::JsonSchema { schema }) =
            request.response_format.as_ref()
        {
            if !schema.is_object() {
                return Err(NxuskitError::InvalidRequest(
                    "invalid response_format".into(),
                ));
            }
            return Err(NxuskitError::unsupported_capability(
                "claude",
                "response_format",
            ));
        }

        if let Some(config) = request.structured_output.as_ref() {
            if config.validate_shape().is_err() {
                return Err(NxuskitError::InvalidRequest(
                    "invalid structured_output".into(),
                ));
            }
            if matches!(
                config.mode,
                crate::capabilities::StructuredOutputMode::JsonSchema
            ) {
                return Err(NxuskitError::unsupported_capability(
                    "claude",
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
            if config.parallel_tool_calls.is_some() {
                return Err(NxuskitError::unsupported_capability(
                    "claude",
                    "parallel_tool_calls",
                ));
            }
            if config.streaming_tool_calls.is_some() {
                return Err(NxuskitError::unsupported_capability(
                    "claude",
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
                "claude",
                "tool_streaming",
            ));
        }

        if request.provider_options.is_some() {
            return Err(NxuskitError::unsupported_capability(
                "claude",
                "provider_options",
            ));
        }
        if request.openai_responses.is_some() {
            return Err(NxuskitError::unsupported_capability(
                "claude",
                "openai_responses",
            ));
        }

        Ok(())
    }
    fn json_object_requested(request: &ChatRequest) -> bool {
        matches!(
            request.response_format.as_ref(),
            Some(crate::types::ResponseFormat::Json)
        ) || matches!(
            request.structured_output.as_ref().map(|config| config.mode),
            Some(crate::capabilities::StructuredOutputMode::JsonObject)
        )
    }

    fn append_json_instruction(system: Option<String>) -> Option<String> {
        let instruction = "Respond with valid JSON only. Do not include any text before or after the JSON object.";
        Some(match system {
            Some(existing) if !existing.is_empty() => format!("{existing}\n\n{instruction}"),
            _ => instruction.to_string(),
        })
    }

    fn legacy_tool_to_anthropic(tool: &serde_json::Value) -> Result<serde_json::Value> {
        let invalid = || NxuskitError::InvalidRequest("invalid tool definition".into());
        let object = tool.as_object().ok_or_else(invalid)?;
        if object.get("type").and_then(serde_json::Value::as_str) != Some("function") {
            return Err(invalid());
        }
        let function = object
            .get("function")
            .and_then(serde_json::Value::as_object)
            .ok_or_else(invalid)?;
        let name = function
            .get("name")
            .and_then(serde_json::Value::as_str)
            .filter(|name| !name.is_empty())
            .ok_or_else(invalid)?;
        let input_schema = function
            .get("parameters")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        if !input_schema.is_object() {
            return Err(invalid());
        }
        let description = match function.get("description") {
            None | Some(serde_json::Value::Null) => None,
            Some(value) => Some(value.as_str().ok_or_else(invalid)?.to_string()),
        };

        let mut mapped = serde_json::Map::new();
        mapped.insert("name".into(), serde_json::Value::String(name.to_string()));
        if let Some(description) = description {
            mapped.insert("description".into(), serde_json::Value::String(description));
        }
        mapped.insert("input_schema".into(), input_schema);
        Ok(serde_json::Value::Object(mapped))
    }

    fn tool_choice_to_anthropic(
        choice: Option<&serde_json::Value>,
    ) -> Result<Option<serde_json::Value>> {
        let Some(choice) = choice else {
            return Ok(None);
        };
        if choice.is_null() {
            return Ok(None);
        }
        let invalid = || NxuskitError::InvalidRequest("invalid tool_choice".into());
        if let Some(value) = choice.as_str() {
            return match value {
                "auto" => Ok(Some(serde_json::json!({"type": "auto"}))),
                "none" => Ok(None),
                "required" => Ok(Some(serde_json::json!({"type": "any"}))),
                _ => Err(invalid()),
            };
        }
        let object = choice.as_object().ok_or_else(invalid)?;
        if object.get("type").and_then(serde_json::Value::as_str) != Some("function") {
            return Err(invalid());
        }
        let function = object
            .get("function")
            .and_then(serde_json::Value::as_object)
            .ok_or_else(invalid)?;
        let name = function
            .get("name")
            .and_then(serde_json::Value::as_str)
            .filter(|name| !name.is_empty())
            .ok_or_else(invalid)?;
        Ok(Some(serde_json::json!({"type": "tool", "name": name})))
    }

    fn build_tool_fields(
        request: &ChatRequest,
        warnings: &mut Vec<crate::types::ParameterWarning>,
    ) -> Result<(Option<Vec<serde_json::Value>>, Option<serde_json::Value>)> {
        if let Some(config) = request.tool_call_config.as_ref() {
            let record = crate::capabilities::registry::find("anthropic").ok_or_else(|| {
                NxuskitError::Configuration(
                    "provider capability record not found: anthropic".into(),
                )
            })?;
            match crate::capabilities::validate_request_against_record(
                &record,
                &crate::capabilities::FeatureRequest::tool_call(config.clone()),
            ) {
                crate::capabilities::ValidationOutcome::Allow => {}
                crate::capabilities::ValidationOutcome::Warn { feature, reason } => {
                    warnings.push(crate::types::ParameterWarning {
                        parameter: feature.to_string(),
                        message: reason,
                        severity: crate::types::WarningSeverity::Warning,
                    });
                }
                crate::capabilities::ValidationOutcome::Block { feature, reason } => {
                    return Err(NxuskitError::InvalidRequest(format!("{feature}: {reason}")));
                }
            }

            let wire_tools = crate::capabilities::openai_wire::tools_for(&record, config);
            let tools = wire_tools
                .as_array()
                .map(|entries| {
                    entries
                        .iter()
                        .map(Self::legacy_tool_to_anthropic)
                        .collect::<Result<Vec<_>>>()
                })
                .transpose()?
                .filter(|entries| !entries.is_empty());

            let wire_choice = crate::capabilities::openai_wire::tool_choice_for(&record, config);
            let tool_choice = Self::tool_choice_to_anthropic(Some(&wire_choice))?;
            return Ok((tools, tool_choice));
        }

        let tools = request
            .tools
            .as_ref()
            .map(|entries| {
                entries
                    .iter()
                    .map(Self::legacy_tool_to_anthropic)
                    .collect::<Result<Vec<_>>>()
            })
            .transpose()?
            .filter(|entries| !entries.is_empty());
        let tool_choice = Self::tool_choice_to_anthropic(request.tool_choice.as_ref())?;
        Ok((tools, tool_choice))
    }

    fn build_request(
        &self,
        request: &ChatRequest,
        streaming: bool,
    ) -> Result<(ClaudeRequest, AdaptedRequest)> {
        self.validate_request_surface(request, streaming)?;

        let json_object_requested = Self::json_object_requested(request);
        let mut request_for_adaptation = request.clone();
        if matches!(
            request_for_adaptation.response_format.as_ref(),
            Some(crate::types::ResponseFormat::Json)
        ) {
            request_for_adaptation.response_format = None;
        }

        let capabilities = self.get_capabilities();
        let mut adapted = ParameterAdapter::adapt(&request_for_adaptation, &capabilities);
        let request_for_tools = adapted.request.clone();
        let (tools, tool_choice) =
            Self::build_tool_fields(&request_for_tools, &mut adapted.warnings)?;

        let (mut system, messages) = self.convert_messages(&adapted.request.messages);
        if json_object_requested {
            system = Self::append_json_instruction(system);
        }

        let claude_request = ClaudeRequest {
            model: adapted.request.model.clone(),
            messages,
            system,
            max_tokens: adapted.request.max_tokens.unwrap_or(4096),
            temperature: adapted.request.temperature,
            top_p: adapted.request.top_p,
            stop_sequences: adapted.request.stop.clone(),
            tools,
            tool_choice,
            stream: streaming,
        };
        Ok((claude_request, adapted))
    }
}

impl std::fmt::Debug for ClaudeProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClaudeProvider")
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
fn enrich_claude_model(info: &mut ModelInfo) {
    let model_id = &info.name;

    // Match by exact ID or pattern
    let (description, context_window, version, family) =
        if model_id.contains("claude-3-5-sonnet") || model_id.contains("claude-3.5-sonnet") {
            (
                Some("Most intelligent model, best for complex tasks requiring deep reasoning"),
                Some(200_000),
                Some("3.5"),
                Some("sonnet"),
            )
        } else if model_id.contains("claude-3-5-haiku") || model_id.contains("claude-3.5-haiku") {
            (
                Some("Fastest model, optimized for speed and responsiveness"),
                Some(200_000),
                Some("3.5"),
                Some("haiku"),
            )
        } else if model_id.contains("claude-3-opus") {
            (
                Some("Powerful model for highly complex tasks, top-level performance"),
                Some(200_000),
                Some("3"),
                Some("opus"),
            )
        } else if model_id.contains("claude-3-sonnet") {
            (
                Some("Balanced model for complex tasks at lower cost"),
                Some(200_000),
                Some("3"),
                Some("sonnet"),
            )
        } else if model_id.contains("claude-3-haiku") {
            (
                Some("Fast and compact model for simple queries"),
                Some(200_000),
                Some("3"),
                Some("haiku"),
            )
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

    // All Claude 3+ models support vision
    if model_id.starts_with("claude-3") {
        info.metadata
            .insert("modalities".to_string(), "text,vision".to_string());
        info.metadata
            .insert("max_images".to_string(), "100".to_string());
    }
}

#[async_trait]
impl LLMProvider for ClaudeProvider {
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse> {
        let (claude_request, adapted) = self.build_request(request, false)?;

        let response = self
            .client
            .post(format!("{}/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&claude_request)
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

        let claude_response: ClaudeResponse =
            crate::provider::http::read_response_json_bounded(response).await?;

        let mut text_parts = Vec::new();
        let mut thinking_parts = Vec::new();
        let mut tool_calls = Vec::new();
        for block in claude_response.content {
            match block {
                ContentBlock::Text { text } => text_parts.push(text),
                ContentBlock::Thinking { thinking } => {
                    if !thinking.is_empty() {
                        thinking_parts.push(thinking);
                    }
                }
                ContentBlock::ToolUse { id, name, input } => {
                    tool_calls.push(serde_json::json!({
                        "id": id,
                        "type": "function",
                        "function": {
                            "name": name,
                            "arguments": serde_json::to_string(&input).unwrap_or_else(|_| "{}".into())
                        }
                    }));
                }
                ContentBlock::Unknown => {}
            }
        }
        let content = text_parts.join("\n");

        let actual_count = TokenCount::new(
            claude_response.usage.input_tokens,
            claude_response.usage.output_tokens,
        );
        let estimated_count = TokenCount::new(
            claude_response.usage.input_tokens,
            claude_response.usage.output_tokens,
        );
        let mut response = ChatResponse::new(
            content,
            claude_response.model,
            TokenUsage::with_actual(actual_count, estimated_count),
        );
        response.provider = self.provider_name().to_string();
        if !tool_calls.is_empty() {
            response.tool_calls = Some(tool_calls);
        }

        // Map Claude's stop_reason to our FinishReason
        response.finish_reason = claude_response
            .stop_reason
            .as_deref()
            .and_then(claude_finish_reason);
        response.raw_finish_reason = claude_response.stop_reason;

        // Add parameter adaptation warnings
        response.warnings = adapted.warnings;

        // Populate inference metadata
        let thinking_trace = if thinking_parts.is_empty() {
            None
        } else {
            Some(thinking_parts.join("\n"))
        };
        response.inference_metadata = InferenceMetadata {
            is_complete: true,
            finish_reason: response.finish_reason,
            ..InferenceMetadata::default()
        }
        .with_token_usage(response.usage.clone())
        .with_provider_metadata(serde_json::json!({
            "provider": "claude"
        }));
        if let Some(trace) = thinking_trace {
            response.inference_metadata.thinking_trace = Some(trace);
        }

        Ok(response)
    }

    async fn chat_stream(
        &self,
        request: &ChatRequest,
    ) -> Result<Box<dyn Stream<Item = Result<StreamChunk>> + Send + Unpin>> {
        let (claude_request, _adapted) = self.build_request(request, true)?;

        // Note: The client already has timeouts configured via build_http_client():
        // - connect_timeout: applies during TCP handshake
        // - read_timeout: applies per-chunk during streaming (resets after each chunk)
        // - timeout (total): applies to the entire request lifecycle
        //
        // For streaming requests, we use total_timeout as the request-level timeout.
        // The read_timeout on the client handles inter-chunk delays during streaming.
        let response = self
            .client
            .post(format!("{}/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&claude_request)
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

        let stream = response.bytes_stream();
        let mut buffer = Vec::new();
        let estimator = TokenEstimator::for_model(&request.model);
        let prompt_tokens = estimator.count_messages(&request.messages);

        let output_stream = stream! {
            use super::openai::contract::{next_openai_sse_frame, OpenAiSseFrame};
            let mut stream = stream;
            let mut accumulator = StreamingTokenAccumulator::new(estimator, prompt_tokens);
            let mut input_tokens = 0;
            let mut output_tokens = 0;
            let mut raw_reason = None;
            loop {
                match stream.next().await {
                    Some(Ok(bytes)) => {
                        let mut chunk_cursor =
                            crate::provider::http::BoundedStreamChunkCursor::new(&bytes);
                        loop {
                            let has_more = match chunk_cursor.append_next(&mut buffer) {
                                Ok(has_more) => has_more,
                                Err(error) => { yield Err(NxuskitError::Stream(error.to_string())); return; }
                            };
                        while let Some(frame) = next_openai_sse_frame(&mut buffer) {
                            let data = match frame {
                                Ok(OpenAiSseFrame::Data(data)) => data,
                                Ok(OpenAiSseFrame::Ignored) => continue,
                                Ok(OpenAiSseFrame::Done) => {
                                    yield Err(NxuskitError::Stream("Anthropic requires message_stop, not [DONE]".into()));
                                    return;
                                }
                                Err(error) => { yield Err(NxuskitError::Stream(error.to_string())); return; }
                            };
                            let event: ClaudeStreamEvent = match serde_json::from_value(data) {
                                Ok(event) => event,
                                Err(error) => { yield Err(NxuskitError::Stream(error.to_string())); return; }
                            };
                            match event.event_type.as_str() {
                                "error" => {
                                    yield Err(NxuskitError::Stream(format!("Anthropic stream error: {:?}", event.error)));
                                    return;
                                }
                                "message_start" => {
                                    if let Some(usage) = event.message.and_then(|m| m.usage) {
                                        input_tokens = usage.input_tokens;
                                        output_tokens = usage.output_tokens;
                                        accumulator.set_actual(TokenCount::new(input_tokens, output_tokens));
                                    }
                                }
                                "content_block_delta" => {
                                    if let Some(delta) = event.delta {
                                        let text = delta.text.unwrap_or_default();
                                        if !text.is_empty() || delta.thinking.is_some() {
                                            accumulator.add_chunk(&text);
                                            if let Some(thinking) = delta.thinking.as_deref() {
                                                accumulator.add_thinking_chunk(thinking);
                                            }
                                            let mut chunk = StreamChunk::new(text);
                                            chunk.thinking = delta.thinking;
                                            chunk.usage = Some(accumulator.running_total());
                                            yield Ok(chunk);
                                        }
                                    }
                                }
                                "message_delta" => {
                                    if let Some(delta) = event.delta
                                        && delta.stop_reason.is_some()
                                    {
                                        raw_reason = delta.stop_reason;
                                    }
                                    if let Some(usage) = event.usage {
                                        if let Some(value) = usage.input_tokens { input_tokens = value; }
                                        if let Some(value) = usage.output_tokens { output_tokens = value; }
                                        accumulator.set_actual(TokenCount::new(input_tokens, output_tokens));
                                    }
                                }
                                "message_stop" => {
                                    let mut chunk = StreamChunk::new(String::new());
                                    chunk.terminal = true;
                                    chunk.finish_reason = raw_reason.as_deref().and_then(claude_finish_reason);
                                    chunk.raw_finish_reason = raw_reason;
                                    chunk.usage = Some(accumulator.finalize());
                                    yield Ok(chunk);
                                    return;
                                }
                                "" => { yield Err(NxuskitError::Stream("Missing Anthropic event type".into())); return; }
                                _ => {}
                            }
                        }
                            if !has_more {
                                break;
                            }
                        }
                    }
                    Some(Err(error)) => { yield Err(NxuskitError::Network(error)); return; }
                    None => { yield Err(NxuskitError::Stream("Anthropic stream ended before message_stop".into())); return; }
                }
            }
        };

        Ok(Box::new(Box::pin(output_stream)))
    }

    fn provider_name(&self) -> &str {
        "claude"
    }

    /// List available Claude models with context window information
    ///
    /// Returns hardcoded model information for Claude models. The Anthropic API
    /// does not provide a models listing endpoint, so we maintain a curated list
    /// of available models with their specifications.
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
    /// let provider = ClaudeProvider::builder()
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
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = crate::provider::http::read_response_text_bounded(response)
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(NxuskitError::provider(status.as_u16(), error_text));
        }

        let models_response: ClaudeModelsResponse =
            crate::provider::http::read_response_json_bounded(response).await?;

        Ok(models_response
            .data
            .into_iter()
            .map(|model| {
                let mut info = ModelInfo::new(model.id.clone());
                info.metadata
                    .insert("created_at".to_string(), model.created_at);
                info.metadata
                    .insert("display_name".to_string(), model.display_name);
                info.metadata.insert("type".to_string(), model.model_type);

                // Enrich with descriptions and metadata for known models
                enrich_claude_model(&mut info);

                info
            })
            .collect())
    }

    fn get_capabilities(&self) -> ProviderCapabilities {
        Self::adapter_capabilities()
    }
}

impl ClaudeProvider {
    /// Create a fresh session with no accumulated state.
    ///
    /// For ClaudeProvider, this returns a clone with the same configuration
    /// since it's already stateless (each request is independent).
    ///
    /// # Returns
    ///
    /// A cloned ClaudeProvider instance with the same configuration.
    pub fn fresh_session(&self) -> Self {
        self.clone()
    }
}

/// Builder for ClaudeProvider
#[derive(Debug, Default)]
pub struct ClaudeProviderBuilder {
    api_key: Option<String>,
    base_url: Option<String>,
    model: Option<String>,
    timeout: Option<Duration>,
    connection_timeout: Option<Duration>,
    stream_read_timeout: Option<Duration>,
    total_timeout: Option<Duration>,
}

impl ClaudeProviderBuilder {
    /// Set the API key
    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// Set the base URL (default: <https://api.anthropic.com/v1>)
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

    /// Build the ClaudeProvider
    pub fn build(self) -> Result<ClaudeProvider> {
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

        Ok(ClaudeProvider {
            client,
            api_key,
            base_url: self.base_url.unwrap_or_else(|| CLAUDE_API_BASE.to_string()),
            default_model: self
                .model
                .unwrap_or_else(|| "claude-sonnet-4-5".to_string()),
            connection_timeout,
            stream_read_timeout,
            total_timeout,
        })
    }
}

// Internal Claude API types

#[derive(Debug, Serialize)]
struct ClaudeRequest {
    model: String,
    messages: Vec<ClaudeMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_sequences: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<serde_json::Value>,
    stream: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct ClaudeMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<Vec<ClaudeContentPart>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClaudeContentPart {
    Text { text: String },
    Image { source: ClaudeImageSource },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ClaudeImageSource {
    #[serde(rename = "type")]
    source_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    media_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ClaudeResponse {
    content: Vec<ContentBlock>,
    model: String,
    usage: ClaudeUsage,
    #[serde(default)]
    stop_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "thinking")]
    Thinking {
        #[serde(default)]
        thinking: String,
    },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        #[serde(default)]
        input: serde_json::Value,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
struct ClaudeUsage {
    input_tokens: u32,
    output_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct ClaudeStreamEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    delta: Option<StreamDelta>,
    #[serde(default)]
    message: Option<ClaudeStreamMessage>,
    #[serde(default)]
    usage: Option<ClaudeStreamUsage>,
    #[serde(default)]
    error: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct ClaudeStreamUsage {
    input_tokens: Option<u32>,
    output_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct StreamDelta {
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    thinking: Option<String>,
    #[serde(default)]
    stop_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ClaudeStreamMessage {
    #[serde(default)]
    usage: Option<ClaudeUsage>,
}

// Response structures for the models API
#[derive(Debug, Deserialize)]
struct ClaudeModelsResponse {
    data: Vec<ClaudeModelInfo>,
}

#[derive(Debug, Deserialize)]
struct ClaudeModelInfo {
    id: String,
    created_at: String,
    display_name: String,
    #[serde(rename = "type")]
    model_type: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn literal_claude_server(body: &str) -> (ClaudeProvider, std::thread::JoinHandle<()>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
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
            assert!(headers.starts_with("POST /messages HTTP/1.1"));
            let length: usize = headers
                .lines()
                .find_map(|line| {
                    let (k, v) = line.split_once(':')?;
                    k.eq_ignore_ascii_case("content-length")
                        .then(|| v.trim().parse().unwrap())
                })
                .unwrap();
            socket.read_exact(&mut vec![0; length]).unwrap();
            write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
        });
        (
            ClaudeProvider::builder()
                .api_key("loopback")
                .base_url(url)
                .build()
                .unwrap(),
            server,
        )
    }

    fn assert_claude_unsupported(request: &ChatRequest, streaming: bool, capability: &str) {
        let provider = ClaudeProvider::builder().api_key("test").build().unwrap();
        match provider.validate_request_surface(request, streaming) {
            Err(NxuskitError::UnsupportedCapability {
                provider,
                capability: actual,
            }) => {
                assert_eq!(provider, "claude");
                assert_eq!(actual, capability);
            }
            other => panic!("expected unsupported {capability}, got {other:?}"),
        }
    }

    #[test]
    fn claude_request_surface_rejects_explicit_unsupported_controls() {
        let base = ChatRequest::new("claude-sonnet-4-5");

        let mut request = base.clone();
        request.presence_penalty = Some(0.4);
        assert_claude_unsupported(&request, false, "presence_penalty");

        let mut request = base.clone();
        request.frequency_penalty = Some(-0.3);
        assert_claude_unsupported(&request, false, "frequency_penalty");

        let mut request = base.clone();
        request.seed = Some(42);
        assert_claude_unsupported(&request, false, "seed");

        let mut request = base.clone();
        request.logprobs = Some(true);
        assert_claude_unsupported(&request, false, "logprobs");

        let mut request = base.clone();
        request.top_logprobs = Some(3);
        assert_claude_unsupported(&request, false, "top_logprobs");

        let mut request = base.clone();
        request.reasoning_effort = Some("low".into());
        assert_claude_unsupported(&request, false, "reasoning_effort");

        let mut request = base.clone();
        request.thinking_mode = crate::types::ThinkingMode::Enabled;
        assert_claude_unsupported(&request, false, "thinking");

        let mut request = base.clone();
        request.response_format = Some(crate::types::ResponseFormat::JsonSchema {
            schema: serde_json::json!({"type": "object"}),
        });
        assert_claude_unsupported(&request, false, "response_format");

        let mut request = base.clone();
        request.structured_output = Some(crate::capabilities::StructuredOutputConfig {
            mode: crate::capabilities::StructuredOutputMode::JsonSchema,
            schema: Some(serde_json::json!({"type": "object"})),
            schema_name: Some("response".into()),
            strict: Some(true),
            schema_subset: None,
        });
        assert_claude_unsupported(&request, false, "structured_output");

        let mut request = base.clone();
        request.openai_responses = Some(crate::capabilities::OpenAIResponsesOptions::default());
        assert_claude_unsupported(&request, false, "openai_responses");

        let mut request = base;
        request.tools = Some(vec![serde_json::json!({
            "type": "function",
            "function": {"name": "lookup", "parameters": {"type": "object"}}
        })]);
        assert_claude_unsupported(&request, true, "tool_streaming");
    }

    #[test]
    fn claude_empty_tools_are_no_tools_for_streaming() {
        let provider = ClaudeProvider::builder().api_key("test").build().unwrap();

        let omitted = ChatRequest::new("claude-sonnet-4-5");
        provider.validate_request_surface(&omitted, true).unwrap();

        let mut empty = ChatRequest::new("claude-sonnet-4-5");
        empty.tools = Some(vec![]);
        provider.validate_request_surface(&empty, true).unwrap();
        let (wire, _) = provider.build_request(&empty, true).unwrap();
        let wire = serde_json::to_value(wire).unwrap();
        assert!(
            wire.get("tools").is_none(),
            "empty tools must serialize as no tools: {wire}"
        );

        let mut nonempty = ChatRequest::new("claude-sonnet-4-5");
        nonempty.tools = Some(vec![serde_json::json!({
            "type": "function",
            "function": {"name": "lookup", "parameters": {"type": "object"}}
        })]);
        assert!(matches!(
            provider.validate_request_surface(&nonempty, true),
            Err(NxuskitError::UnsupportedCapability { .. })
        ));
    }

    #[test]
    fn claude_request_surface_preserves_invalid_request_precedence() {
        let provider = ClaudeProvider::builder().api_key("test").build().unwrap();

        let mut request = ChatRequest::new("claude-sonnet-4-5");
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

        let mut request = ChatRequest::new("claude-sonnet-4-5");
        request.stop = Some(vec!["".into()]);
        assert!(matches!(
            provider.validate_request_surface(&request, false),
            Err(NxuskitError::InvalidRequest(_))
        ));

        let mut request = ChatRequest::new("claude-sonnet-4-5");
        request.top_logprobs = Some(21);
        assert!(matches!(
            provider.validate_request_surface(&request, false),
            Err(NxuskitError::InvalidRequest(_))
        ));

        let mut request = ChatRequest::new("claude-sonnet-4-5");
        request.reasoning_effort = Some("invalid".into());
        assert!(matches!(
            provider.validate_request_surface(&request, false),
            Err(NxuskitError::InvalidRequest(_))
        ));

        let mut request = ChatRequest::new("claude-sonnet-4-5");
        request.response_format = Some(crate::types::ResponseFormat::JsonSchema {
            schema: serde_json::json!([]),
        });
        assert!(matches!(
            provider.validate_request_surface(&request, false),
            Err(NxuskitError::InvalidRequest(_))
        ));
    }

    #[test]
    fn claude_legacy_tools_and_tool_choice_fail_closed_on_malformed_shapes() {
        for tool in [
            serde_json::json!({
                "type": "not_function",
                "function": {"name": "lookup", "parameters": {"type": "object"}}
            }),
            serde_json::json!({
                "type": "function",
                "function": {"name": "", "parameters": {"type": "object"}}
            }),
            serde_json::json!({
                "type": "function",
                "function": {"name": "lookup", "parameters": []}
            }),
        ] {
            assert!(
                matches!(
                    ClaudeProvider::legacy_tool_to_anthropic(&tool),
                    Err(NxuskitError::InvalidRequest(_))
                ),
                "malformed legacy tool must fail locally: {tool}"
            );
        }

        for choice in [
            serde_json::json!("sometimes"),
            serde_json::json!({"type": "function"}),
            serde_json::json!({"type": "function", "function": {"name": ""}}),
            serde_json::json!({"type": "other", "function": {"name": "lookup"}}),
        ] {
            assert!(
                matches!(
                    ClaudeProvider::tool_choice_to_anthropic(Some(&choice)),
                    Err(NxuskitError::InvalidRequest(_))
                ),
                "malformed legacy tool_choice must fail locally: {choice}"
            );
        }
    }

    #[test]
    fn claude_stop_sequences_reach_production_wire_unary_and_streaming() {
        let provider = ClaudeProvider::builder().api_key("test").build().unwrap();
        let mut request = ChatRequest::new("claude-sonnet-4-5").with_message(Message::user("hi"));
        request.stop = Some(vec!["END".into(), "DONE".into()]);

        let (unary, _) = provider.build_request(&request, false).unwrap();
        let unary = serde_json::to_value(unary).unwrap();
        assert_eq!(unary["stop_sequences"], serde_json::json!(["END", "DONE"]));
        assert_eq!(unary["stream"], false);

        let (streaming, _) = provider.build_request(&request, true).unwrap();
        let streaming = serde_json::to_value(streaming).unwrap();
        assert_eq!(
            streaming["stop_sequences"],
            serde_json::json!(["END", "DONE"])
        );
        assert_eq!(streaming["stream"], true);
    }

    #[tokio::test]
    async fn claude_unsupported_controls_reject_before_http() {
        let provider = ClaudeProvider::builder()
            .api_key("test")
            .base_url("http://127.0.0.1:1")
            .build()
            .unwrap();

        let mut unary = ChatRequest::new("claude-sonnet-4-5");
        unary.seed = Some(42);
        assert!(matches!(
            provider.chat(&unary).await,
            Err(NxuskitError::UnsupportedCapability {
                ref provider,
                ref capability,
            }) if provider == "claude" && capability == "seed"
        ));

        let mut streaming = ChatRequest::new("claude-sonnet-4-5");
        streaming.tools = Some(vec![serde_json::json!({
            "type": "function",
            "function": {"name": "lookup", "parameters": {"type": "object"}}
        })]);
        assert!(matches!(
            provider.chat_stream(&streaming).await,
            Err(NxuskitError::UnsupportedCapability {
                ref provider,
                ref capability,
            }) if provider == "claude" && capability == "tool_streaming"
        ));
    }

    #[tokio::test]
    async fn claude_unary_json_and_tools_round_trip() {
        let provider = ClaudeProvider::builder().api_key("test").build().unwrap();

        let mut legacy = ChatRequest::new("claude-sonnet-4-5")
            .with_message(Message::system("System rules"))
            .with_message(Message::user("Return inventory."));
        legacy.response_format = Some(crate::types::ResponseFormat::Json);
        legacy.tools = Some(vec![serde_json::json!({
            "type": "function",
            "function": {
                "name": "lookup",
                "description": "Look up inventory",
                "parameters": {
                    "type": "object",
                    "properties": {"sku": {"type": "string"}}
                }
            }
        })]);
        legacy.tool_choice = Some(serde_json::json!("required"));

        let (legacy_wire, _) = provider.build_request(&legacy, false).unwrap();
        let legacy_wire = serde_json::to_value(legacy_wire).unwrap();
        assert_eq!(
            legacy_wire["system"],
            "System rules\n\nRespond with valid JSON only. Do not include any text before or after the JSON object."
        );
        assert_eq!(
            legacy_wire["tools"],
            serde_json::json!([{
                "name": "lookup",
                "description": "Look up inventory",
                "input_schema": {
                    "type": "object",
                    "properties": {"sku": {"type": "string"}}
                }
            }])
        );
        assert_eq!(
            legacy_wire["tool_choice"],
            serde_json::json!({"type": "any"})
        );

        let mut typed =
            ChatRequest::new("claude-sonnet-4-5").with_message(Message::user("Return JSON."));
        typed.structured_output = Some(crate::capabilities::StructuredOutputConfig {
            mode: crate::capabilities::StructuredOutputMode::JsonObject,
            schema: None,
            schema_name: None,
            strict: None,
            schema_subset: None,
        });
        typed.tool_call_config = Some(crate::capabilities::ToolCallConfig {
            tools: vec![
                crate::capabilities::ToolDefinition {
                    name: "web_search".into(),
                    description: None,
                    parameters: serde_json::json!({"type": "object"}),
                    strict: None,
                },
                crate::capabilities::ToolDefinition {
                    name: "search_inventory".into(),
                    description: Some("Search inventory".into()),
                    parameters: serde_json::json!({"type": "object"}),
                    strict: None,
                },
            ],
            tool_choice: crate::capabilities::ToolChoice::Named("search_inventory".into()),
            parallel_tool_calls: None,
            streaming_tool_calls: None,
        });

        let (typed_wire, typed_adapted) = provider.build_request(&typed, false).unwrap();
        let typed_wire = serde_json::to_value(typed_wire).unwrap();
        assert_eq!(
            typed_wire["system"],
            "Respond with valid JSON only. Do not include any text before or after the JSON object."
        );
        let tools = typed_wire["tools"]
            .as_array()
            .expect("Anthropic tools array");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "search_inventory");
        assert_eq!(
            typed_wire["tool_choice"],
            serde_json::json!({"type": "tool", "name": "search_inventory"})
        );
        assert!(typed_adapted.warnings.iter().any(|warning| {
            warning.parameter == "tool_calling" && warning.message.contains("web_search")
        }));

        let body = r#"{"content":[{"type":"tool_use","id":"toolu_1","name":"lookup","input":{"sku":"A-1"}}],"model":"claude-sonnet-4-5","usage":{"input_tokens":3,"output_tokens":2},"stop_reason":"tool_use"}"#;
        let (loopback, server) = literal_claude_server(body).await;
        let response = loopback
            .chat(&ChatRequest::new("claude-sonnet-4-5"))
            .await
            .unwrap();
        server.join().unwrap();
        assert_eq!(response.finish_reason, Some(FinishReason::ToolCalls));
        assert_eq!(
            response.tool_calls,
            Some(vec![serde_json::json!({
                "id": "toolu_1",
                "type": "function",
                "function": {
                    "name": "lookup",
                    "arguments": "{\"sku\":\"A-1\"}"
                }
            })])
        );
    }

    #[test]
    fn claude_typed_tool_hints_and_malformed_legacy_tools_fail_closed() {
        let provider = ClaudeProvider::builder().api_key("test").build().unwrap();

        for (parallel, streaming, capability) in [
            (Some(false), None, "parallel_tool_calls"),
            (None, Some(false), "streaming_tool_calls"),
        ] {
            let mut request = ChatRequest::new("claude-sonnet-4-5");
            request.tool_call_config = Some(crate::capabilities::ToolCallConfig {
                tools: vec![crate::capabilities::ToolDefinition {
                    name: "lookup".into(),
                    description: None,
                    parameters: serde_json::json!({"type": "object"}),
                    strict: None,
                }],
                tool_choice: crate::capabilities::ToolChoice::Auto,
                parallel_tool_calls: parallel,
                streaming_tool_calls: streaming,
            });
            assert!(matches!(
                provider.validate_request_surface(&request, false),
                Err(NxuskitError::UnsupportedCapability {
                    provider: ref actual_provider,
                    capability: ref actual_capability,
                }) if actual_provider == "claude" && actual_capability == capability
            ));
        }

        let mut malformed = ChatRequest::new("claude-sonnet-4-5");
        malformed.tools = Some(vec![serde_json::json!({
            "type": "function",
            "function": {"name": "", "parameters": {}}
        })]);
        assert!(matches!(
            provider.build_request(&malformed, false),
            Err(NxuskitError::InvalidRequest(_))
        ));
    }

    #[tokio::test]
    async fn legacy_claude_terminal_errors_never_complete() {
        for body in [
            "",
            "data: {broken}\n\n",
            "data: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"busy\"}}\n\ndata: {\"type\":\"message_stop\"}\n\n",
            "data: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"partial\"}}\n\n",
        ] {
            let (p, server) = literal_claude_server(body).await;
            let all: Vec<_> = p
                .chat_stream(&ChatRequest::new("claude"))
                .await
                .unwrap()
                .collect()
                .await;
            server.join().unwrap();
            assert_eq!(
                all.iter().filter(|c| c.is_err()).count(),
                1,
                "{body}: {all:?}"
            );
            assert!(
                !all.iter()
                    .any(|c| c.as_ref().is_ok_and(StreamChunk::is_final)),
                "{all:?}"
            );
        }
    }

    #[tokio::test]
    async fn legacy_claude_terminal_raw_and_usage() {
        let body = "data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":7,\"output_tokens\":0}}}\r\n\r\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"hmm\"}}\r\n\r\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"vendor-new\"},\"usage\":{\"output_tokens\":3}}\r\n\r\ndata: {\"type\":\"message_stop\"}\r\n\r\ndata: {broken}\r\n\r\n";
        let (p, server) = literal_claude_server(body).await;
        let all: Vec<_> = p
            .chat_stream(&ChatRequest::new("claude"))
            .await
            .unwrap()
            .collect()
            .await;
        server.join().unwrap();
        assert!(all.iter().all(Result::is_ok), "{all:?}");
        let all: Vec<_> = all.into_iter().map(Result::unwrap).collect();
        assert_eq!(all.iter().filter(|c| c.is_final()).count(), 1);
        assert!(all.iter().any(|c| c.thinking.as_deref() == Some("hmm")));
        let thinking_chunk = all.iter().find(|c| c.thinking.is_some()).unwrap();
        assert!(
            thinking_chunk
                .usage
                .as_ref()
                .unwrap()
                .estimated
                .completion_tokens
                > 0
        );
        let last = all.last().unwrap();
        assert!(last.terminal);
        assert_eq!(last.finish_reason, None);
        assert_eq!(last.raw_finish_reason.as_deref(), Some("vendor-new"));
        let usage = last.usage.as_ref().unwrap().actual.as_ref().unwrap();
        assert_eq!(usage.prompt_tokens, 7);
        assert_eq!(usage.completion_tokens, 3);
    }

    #[tokio::test]
    async fn legacy_claude_reason_presence_and_usage_are_independent() {
        // Literal wire cases: absence/null must preserve the earlier reason,
        // while known/unknown present strings replace it. Usage is independent.
        for (delta, raw, canonical) in [
            ("", "end_turn", Some(FinishReason::Stop)),
            (
                ",\"delta\":{\"stop_reason\":null}",
                "end_turn",
                Some(FinishReason::Stop),
            ),
            (
                ",\"delta\":{\"stop_reason\":\"max_tokens\"}",
                "max_tokens",
                Some(FinishReason::Length),
            ),
            (
                ",\"delta\":{\"stop_reason\":\"vendor-new\"}",
                "vendor-new",
                None,
            ),
        ] {
            let body = format!(
                "data: {{\"type\":\"message_start\",\"message\":{{\"usage\":{{\"input_tokens\":7,\"output_tokens\":0}}}}}}\n\n\
                 data: {{\"type\":\"message_delta\",\"delta\":{{\"stop_reason\":\"end_turn\"}}}}\n\n\
                 data: {{\"type\":\"message_delta\"{delta},\"usage\":{{\"input_tokens\":11,\"output_tokens\":13}}}}\n\n\
                 data: {{\"type\":\"message_stop\"}}\n\n"
            );
            let (provider, server) = literal_claude_server(&body).await;
            // The server owns its response; the caller may release its buffer
            // before any streaming request consumes that response.
            drop(body);
            let chunks: Vec<_> = provider
                .chat_stream(&ChatRequest::new("claude"))
                .await
                .unwrap()
                .collect()
                .await;
            server.join().unwrap();
            let chunks: Vec<_> = chunks.into_iter().collect::<Result<Vec<_>>>().unwrap();
            assert_eq!(chunks.iter().filter(|chunk| chunk.terminal).count(), 1);
            let terminal = chunks.last().unwrap();
            assert!(terminal.terminal);
            assert_eq!(terminal.raw_finish_reason.as_deref(), Some(raw));
            assert_eq!(terminal.finish_reason, canonical);
            let usage = terminal.usage.as_ref().unwrap().actual.as_ref().unwrap();
            assert_eq!(usage.prompt_tokens, 11);
            assert_eq!(usage.completion_tokens, 13);
        }
    }

    #[tokio::test]
    async fn legacy_claude_unary_raw_truth() {
        let (p,server)=literal_claude_server("{\"id\":\"msg\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"actual\",\"content\":[{\"type\":\"text\",\"text\":\"ok\"}],\"stop_reason\":\"vendor-new\",\"usage\":{\"input_tokens\":7,\"output_tokens\":3}}").await;
        let response = p.chat(&ChatRequest::new("claude")).await.unwrap();
        server.join().unwrap();
        assert_eq!(response.raw_finish_reason.as_deref(), Some("vendor-new"));
        assert_eq!(response.finish_reason, None);
        assert_eq!(response.inference_metadata.finish_reason, None);
    }

    #[test]
    fn test_builder_missing_api_key() {
        let result = ClaudeProvider::builder().build();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            NxuskitError::Configuration(_)
        ));
    }

    #[test]
    fn test_builder_with_api_key() {
        let provider = ClaudeProvider::builder()
            .api_key("test-key")
            .build()
            .unwrap();
        assert_eq!(provider.provider_name(), "claude");
    }

    #[test]
    fn test_debug_redacts_api_key() {
        let provider = ClaudeProvider::builder()
            .api_key("secret-key")
            .build()
            .unwrap();
        let debug_str = format!("{:?}", provider);
        assert!(!debug_str.contains("secret-key"));
        assert!(debug_str.contains("[REDACTED]"));
    }

    #[test]
    fn test_message_conversion() {
        let provider = ClaudeProvider::builder().api_key("test").build().unwrap();

        let messages = vec![
            Message::system("You are helpful"),
            Message::user("Hello"),
            Message::assistant("Hi there"),
        ];

        let (system, claude_msgs) = provider.convert_messages(&messages);
        assert_eq!(system, Some("You are helpful".to_string()));
        assert_eq!(claude_msgs.len(), 2);
        assert_eq!(claude_msgs[0].role, "user");
        assert_eq!(claude_msgs[1].role, "assistant");
    }

    #[tokio::test]
    #[ignore] // Requires valid Anthropic API key
    async fn test_list_models() {
        use std::env;
        let api_key = env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY not set");
        let provider = ClaudeProvider::builder().api_key(api_key).build().unwrap();
        let models = provider.list_models().await.unwrap();

        // Verify we get models from the API
        assert!(!models.is_empty(), "Should return at least one model");

        // Verify all models have the expected structure from API
        for model in &models {
            assert!(!model.name.is_empty(), "Model should have a name");
            assert!(
                model.metadata.contains_key("created_at"),
                "Model should have 'created_at' metadata"
            );
            assert!(
                model.metadata.contains_key("display_name"),
                "Model should have 'display_name' metadata"
            );
            assert!(
                model.metadata.contains_key("type"),
                "Model should have 'type' metadata"
            );
        }
    }

    #[test]
    fn test_sse_event_parsing() {
        // Test parsing of Claude SSE event format with multi-line events
        let sse_event = "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}";

        // Parse the data line
        let mut data_line = None;
        for line in sse_event.lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                data_line = Some(data);
                break;
            }
        }

        assert!(data_line.is_some(), "Should find data line in SSE event");

        let data = data_line.unwrap();
        let event: ClaudeStreamEvent = serde_json::from_str(data).expect("Should parse JSON");

        assert_eq!(event.event_type, "content_block_delta");
        assert!(event.delta.is_some());
        assert_eq!(event.delta.unwrap().text, Some("Hello".to_string()));
    }

    #[test]
    fn test_sse_message_stop_parsing() {
        // Test parsing of message_stop event
        let sse_event = "event: message_stop\ndata: {\"type\":\"message_stop\"}";

        let mut data_line = None;
        for line in sse_event.lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                data_line = Some(data);
                break;
            }
        }

        assert!(data_line.is_some());

        let data = data_line.unwrap();
        let event: ClaudeStreamEvent = serde_json::from_str(data).expect("Should parse JSON");

        assert_eq!(event.event_type, "message_stop");
        assert!(event.delta.is_none());
    }

    #[test]
    fn test_timeout_backward_compatibility() {
        // Test that existing code using .timeout() still works
        let provider = ClaudeProvider::builder()
            .api_key("test")
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap();

        // All three timeout values should be set to the same value
        assert_eq!(provider.connection_timeout, Duration::from_secs(30));
        assert_eq!(provider.stream_read_timeout, Duration::from_secs(30));
        assert_eq!(provider.total_timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_timeout_defaults() {
        // Test that defaults are applied when no timeout is set
        let provider = ClaudeProvider::builder().api_key("test").build().unwrap();

        assert_eq!(provider.connection_timeout, Duration::from_secs(60));
        assert_eq!(provider.stream_read_timeout, Duration::from_secs(120));
        assert_eq!(provider.total_timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_granular_timeouts() {
        // Test that specific timeouts override general timeout
        let provider = ClaudeProvider::builder()
            .api_key("test")
            .timeout(Duration::from_secs(30))
            .connection_timeout(Duration::from_secs(10))
            .stream_read_timeout(Duration::from_secs(300))
            .total_timeout(Duration::from_secs(90))
            .build()
            .unwrap();

        assert_eq!(provider.connection_timeout, Duration::from_secs(10));
        assert_eq!(provider.stream_read_timeout, Duration::from_secs(300));
        assert_eq!(provider.total_timeout, Duration::from_secs(90));
    }

    #[test]
    fn test_partial_granular_timeouts() {
        // Test mixing general timeout with specific ones
        let provider = ClaudeProvider::builder()
            .api_key("test")
            .timeout(Duration::from_secs(45))
            .stream_read_timeout(Duration::from_secs(200))
            .build()
            .unwrap();

        // connection and total should use the general timeout
        assert_eq!(provider.connection_timeout, Duration::from_secs(45));
        assert_eq!(provider.stream_read_timeout, Duration::from_secs(200));
        assert_eq!(provider.total_timeout, Duration::from_secs(45));
    }

    #[test]
    fn test_only_specific_timeouts() {
        // Test using only specific timeouts without general timeout
        let provider = ClaudeProvider::builder()
            .api_key("test")
            .connection_timeout(Duration::from_secs(5))
            .build()
            .unwrap();

        // connection uses specified value, others use defaults
        assert_eq!(provider.connection_timeout, Duration::from_secs(5));
        assert_eq!(provider.stream_read_timeout, Duration::from_secs(120)); // default
        assert_eq!(provider.total_timeout, Duration::from_secs(60)); // default
    }
}
