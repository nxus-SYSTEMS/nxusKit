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
    parameter_adapter::ParameterAdapter,
    token_estimator::{StreamingTokenAccumulator, TokenEstimator},
    types::{
        ContentPart, FinishReason, ImageData, InferenceMetadata, MessageContent,
        ProviderCapabilities,
    },
};

const CLAUDE_API_BASE: &str = "https://api.anthropic.com/v1";
const ANTHROPIC_VERSION: &str = "2023-06-01";

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
        // Adapt request to provider capabilities
        let capabilities = self.get_capabilities();
        let adapted = ParameterAdapter::adapt(request, &capabilities);
        let adapted_request = &adapted.request;

        let (system, messages) = self.convert_messages(&adapted_request.messages);

        let claude_request = ClaudeRequest {
            model: adapted_request.model.clone(),
            messages,
            system,
            max_tokens: adapted_request.max_tokens.unwrap_or(4096),
            temperature: adapted_request.temperature,
            top_p: adapted_request.top_p,
            stream: false,
        };

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

        let claude_response: ClaudeResponse = response.json().await?;

        let mut text_parts = Vec::new();
        let mut thinking_parts = Vec::new();
        for block in claude_response.content {
            match block {
                ContentBlock::Text { text } => text_parts.push(text),
                ContentBlock::Thinking { thinking } => {
                    if !thinking.is_empty() {
                        thinking_parts.push(thinking);
                    }
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

        // Map Claude's stop_reason to our FinishReason
        response.finish_reason = claude_response.stop_reason.as_deref().map(|sr| match sr {
            "end_turn" | "stop_sequence" => FinishReason::Stop,
            "max_tokens" => FinishReason::Length,
            "tool_use" => FinishReason::ToolCalls,
            "content_filter" => FinishReason::ContentFilter,
            _ => FinishReason::Stop,
        });

        // Add parameter adaptation warnings
        response.warnings = adapted.warnings;

        // Populate inference metadata
        let thinking_trace = if thinking_parts.is_empty() {
            None
        } else {
            Some(thinking_parts.join("\n"))
        };
        response.inference_metadata =
            InferenceMetadata::completed(response.finish_reason.unwrap_or(FinishReason::Stop))
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
        let (system, messages) = self.convert_messages(&request.messages);

        let claude_request = ClaudeRequest {
            model: request.model.clone(),
            messages,
            system,
            max_tokens: request.max_tokens.unwrap_or(4096),
            temperature: request.temperature,
            top_p: request.top_p,
            stream: true,
        };

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

        // Initialize token tracking outside stream! macro to avoid lifetime issues
        let model = request.model.clone();
        let estimator = TokenEstimator::for_model(&model);
        let prompt_tokens = estimator.count_messages(&request.messages);

        let output_stream = stream! {
            let mut stream = stream;
            let mut accumulator = StreamingTokenAccumulator::new(estimator, prompt_tokens);
            let mut input_tokens_captured = false;

            'stream_loop: while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes);
                        buffer.push_str(&text);

                        // Process complete SSE events
                        while let Some(pos) = buffer.find("\n\n") {
                            let event = buffer[..pos].to_string();
                            buffer = buffer[pos + 2..].to_string();

                            // Parse SSE event which has format:
                            // event: <event_type>
                            // data: <json>
                            let mut data_line = None;
                            for line in event.lines() {
                                if let Some(data) = line.strip_prefix("data: ") {
                                    data_line = Some(data);
                                    break;
                                }
                            }

                            if let Some(data) = data_line {
                                if data == "[DONE]" {
                                    continue;
                                }

                                match serde_json::from_str::<ClaudeStreamEvent>(data) {
                                    Ok(event) => {
                                        match event.event_type.as_str() {
                                            "message_start" => {
                                                // Capture input_tokens from message_start event
                                                if !input_tokens_captured
                                                    && let Some(msg) = &event.message
                                                    && let Some(usage) = &msg.usage
                                                {
                                                    accumulator.set_actual(
                                                        TokenCount::new(usage.input_tokens, 0)
                                                    );
                                                    input_tokens_captured = true;
                                                }
                                            }
                                            "content_block_delta" => {
                                                if let Some(delta) = event.delta
                                                    && let Some(text) = delta.text
                                                {
                                                    accumulator.add_chunk(&text);
                                                    let usage = accumulator.running_total();
                                                    let mut chunk = StreamChunk::new(text);
                                                    chunk.usage = Some(usage);
                                                    yield Ok(chunk);
                                                }
                                            }
                                            "message_delta" => {
                                                // Capture output_tokens from message_delta event
                                                if let Some(msg) = &event.message
                                                    && let Some(usage) = &msg.usage
                                                {
                                                    // Create actual counts combining captured input and current output
                                                    accumulator.set_actual(
                                                        TokenCount::new(usage.input_tokens, usage.output_tokens)
                                                    );
                                                }
                                            }
                                            "message_stop" => {
                                                let final_usage = accumulator.finalize();
                                                yield Ok(StreamChunk::final_chunk(
                                                    crate::types::FinishReason::Stop,
                                                    Some(final_usage),
                                                ));
                                                break 'stream_loop;
                                            }
                                            _ => {}
                                        }
                                    }
                                    Err(e) => {
                                        accumulator.mark_interrupted();
                                        yield Err(NxuskitError::Stream(format!("Failed to parse stream event: {}", e)));
                                        break 'stream_loop;
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        accumulator.mark_interrupted();
                        yield Err(NxuskitError::Network(e));
                        break 'stream_loop;
                    }
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
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(NxuskitError::provider(status.as_u16(), error_text));
        }

        let models_response: ClaudeModelsResponse = response.json().await?;

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
}

#[derive(Debug, Deserialize)]
struct StreamDelta {
    #[serde(default)]
    text: Option<String>,
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
