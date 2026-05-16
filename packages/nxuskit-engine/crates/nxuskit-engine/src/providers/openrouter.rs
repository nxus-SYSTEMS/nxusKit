//! OpenRouter provider implementation

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

    /// Build OpenRouter API request from adapted ChatRequest
    fn build_request(&self, request: &ChatRequest) -> Result<OpenRouterRequest> {
        let messages = self.convert_messages(&request.messages);

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
        };

        // Handle JSON mode
        if let Some(ref format) = request.response_format {
            use crate::types::ResponseFormat;
            match format {
                ResponseFormat::Json => {
                    openrouter_request.response_format = Some(OpenRouterResponseFormat {
                        r#type: "json_object".to_string(),
                    });
                }
                ResponseFormat::JsonSchema { .. } => {
                    // OpenRouter passes through to underlying provider
                    // Will be handled by adapter if not supported
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
        // Adapt request to provider capabilities
        let capabilities = self.get_capabilities();
        let adapted = ParameterAdapter::adapt(request, &capabilities);
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
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(match status.as_u16() {
                429 => NxuskitError::rate_limit(retry_after),
                _ => NxuskitError::provider(status.as_u16(), error_text),
            });
        }

        let openrouter_response: OpenRouterResponse = response.json().await?;

        // Extract content from first choice
        let content = openrouter_response
            .choices
            .first()
            .and_then(|choice| choice.message.content.as_deref())
            .unwrap_or("")
            .to_string();

        // Convert finish reason
        let finish_reason = openrouter_response
            .choices
            .first()
            .and_then(|choice| choice.finish_reason.as_ref())
            .and_then(|reason| match reason.as_str() {
                "stop" => Some(crate::types::FinishReason::Stop),
                "length" => Some(crate::types::FinishReason::Length),
                "content_filter" => Some(crate::types::FinishReason::ContentFilter),
                _ => None,
            });

        let mut response = ChatResponse::new(
            content,
            openrouter_response.model,
            TokenUsage::estimated_only(TokenCount::new(
                openrouter_response.usage.prompt_tokens,
                openrouter_response.usage.completion_tokens,
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
                    "provider": "openrouter"
                }));

        Ok(response)
    }

    async fn chat_stream(
        &self,
        request: &ChatRequest,
    ) -> Result<Box<dyn Stream<Item = Result<StreamChunk>> + Send + Unpin>> {
        // Adapt request to provider capabilities
        let capabilities = self.get_capabilities();
        let adapted = ParameterAdapter::adapt(request, &capabilities);
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
        let model = request.model.clone();
        let estimator = TokenEstimator::for_model(&model);
        let prompt_tokens = estimator.count_messages(&request.messages);

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
                                match serde_json::from_str::<OpenRouterStreamResponse>(data) {
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
                                                    "length" => crate::types::FinishReason::Length,
                                                    "content_filter" => crate::types::FinishReason::ContentFilter,
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
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(NxuskitError::provider(status.as_u16(), error_text));
        }

        let models_response: OpenRouterModelsResponse = response.json().await?;

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
            supports_json_schema: false,
            penalty_range: Some((-2.0, 2.0)),
            max_logprobs: None,
        }
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
    seed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<OpenRouterResponseFormat>,
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
    #[allow(dead_code)]
    id: String,
    model: String,
    choices: Vec<OpenRouterChoice>,
    usage: OpenRouterUsage,
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
}

#[derive(Debug, Deserialize)]
struct OpenRouterUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    #[allow(dead_code)]
    total_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct OpenRouterStreamResponse {
    choices: Vec<OpenRouterStreamChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterStreamChoice {
    delta: OpenRouterDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterDelta {
    content: Option<String>,
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
        assert_eq!(caps.max_stop_sequences, Some(4));
        assert!(caps.supports_seed);
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
