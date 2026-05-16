//! Mistral AI provider implementation

use async_stream::stream;
use async_trait::async_trait;
use futures::Stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};

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
        let response_format_typed = request
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
            None => (serde_json::Value::Null, serde_json::Value::Null, None),
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

        // Legacy json_object passthrough only when the typed path is NOT
        // set (avoids emitting `response_format` twice).
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
                ResponseFormat::JsonSchema { .. } | ResponseFormat::Text => {}
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
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(match status.as_u16() {
                429 => NxuskitError::rate_limit(retry_after),
                _ => NxuskitError::provider(status.as_u16(), error_text),
            });
        }

        let mistral_response: MistralResponse = response.json().await?;

        // Extract content from first choice
        let content = mistral_response
            .choices
            .first()
            .and_then(|choice| choice.message.content.as_deref())
            .unwrap_or("")
            .to_string();

        // Convert finish reason
        let finish_reason = mistral_response
            .choices
            .first()
            .and_then(|choice| choice.finish_reason.as_ref())
            .and_then(|reason| match reason.as_str() {
                "stop" => Some(crate::types::FinishReason::Stop),
                "length" => Some(crate::types::FinishReason::Length),
                "model_length" => Some(crate::types::FinishReason::Length),
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

        response.finish_reason = finish_reason;
        response.warnings = adapted.warnings;

        // Populate inference metadata
        response.inference_metadata =
            InferenceMetadata::completed(response.finish_reason.unwrap_or(FinishReason::Stop))
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
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(match status.as_u16() {
                429 => NxuskitError::rate_limit(retry_after),
                _ => NxuskitError::provider(status.as_u16(), error_text),
            });
        }

        let bytes_stream = response.bytes_stream();

        // Initialize token tracking outside stream! macro to avoid lifetime issues
        let model = adapted_request.model.clone();
        let estimator = TokenEstimator::for_model(&model);
        let prompt_tokens = estimator.count_messages(&adapted_request.messages);

        let output_stream = stream! {
            use futures::StreamExt;

            let mut buffer = String::new();
            let mut stream = bytes_stream;
            let mut accumulator = StreamingTokenAccumulator::new(estimator, prompt_tokens);

            'stream_loop: while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(bytes) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));

                        // Process complete lines
                        while let Some(newline_pos) = buffer.find('\n') {
                            let line = buffer[..newline_pos].trim().to_string();
                            buffer = buffer[newline_pos + 1..].to_string();

                            if line.is_empty() || line == "data: [DONE]" {
                                continue;
                            }

                            if let Some(data) = line.strip_prefix("data: ") {
                                match serde_json::from_str::<MistralStreamResponse>(data) {
                                    Ok(stream_response) => {
                                        if let Some(choice) = stream_response.choices.first() {
                                            if let Some(content) = &choice.delta.content {
                                                accumulator.add_chunk(content);
                                                let usage = accumulator.running_total();
                                                let mut stream_chunk = StreamChunk::new(content.clone());
                                                stream_chunk.usage = Some(usage);
                                                yield Ok(stream_chunk);
                                            }

                                            // Check for finish reason
                                            if let Some(reason) = &choice.finish_reason {
                                                let finish_reason = match reason.as_str() {
                                                    "stop" => crate::types::FinishReason::Stop,
                                                    "length" | "model_length" => crate::types::FinishReason::Length,
                                                    _ => crate::types::FinishReason::Stop,
                                                };
                                                let final_usage = accumulator.finalize();
                                                yield Ok(StreamChunk::final_chunk(finish_reason, Some(final_usage)));
                                                break 'stream_loop;
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        accumulator.mark_interrupted();
                                        yield Err(NxuskitError::Stream(format!("Failed to parse stream chunk: {}", e)));
                                        break 'stream_loop;
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        accumulator.mark_interrupted();
                        yield Err(NxuskitError::Stream(e.to_string()));
                        break 'stream_loop;
                    }
                }
            }
        };

        Ok(Box::new(Box::pin(output_stream)))
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
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(NxuskitError::provider(status.as_u16(), error_text));
        }

        let models_response: MistralModelsResponse = response.json().await?;

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
            supports_json_schema: false,
            penalty_range: Some((-2.0, 2.0)),
            max_logprobs: None,
        }
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
    seed: Option<u64>,
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
}

#[derive(Debug, Deserialize)]
struct MistralUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    #[allow(dead_code)]
    total_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct MistralStreamResponse {
    choices: Vec<MistralStreamChoice>,
}

#[derive(Debug, Deserialize)]
struct MistralStreamChoice {
    delta: MistralDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MistralDelta {
    content: Option<String>,
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
        assert_eq!(caps.max_stop_sequences, Some(4));
        assert!(caps.supports_seed);
        assert!(!caps.supports_logprobs);
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
