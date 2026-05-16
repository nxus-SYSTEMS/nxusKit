//! Loopback provider for testing
//!
//! The Loopback provider echoes request data back for testing purposes.
//! It provides multiple model variants with different behaviors and capabilities,
//! allowing developers to:
//!
//! - **Verify request content** - See exactly what's being sent to providers
//! - **Test graceful degradation** - Models with limited capabilities trigger parameter adaptation warnings
//! - **Test JSON mode handling** - Both native JSON support and prompt-based fallback
//! - **Simulate errors and latency** - Test error handling and timeout logic
//!
//! # Models
//!
//! ## Echo Models (Simple Response)
//! - `echo`: Returns last user message content verbatim
//! - `echo-json-native`: Echo with native JSON mode support
//! - `echo-json-fallback`: Echo without JSON mode (tests fallback)
//!
//! ## Limited Capability Models
//! - `echo-limited-claude`: Claude-like (no penalties, seed, logprobs)
//! - `echo-limited-minimal`: Minimal (only temp, max_tokens, 2 stops)
//!
//! ## U-Turn Models (Request Inspection)
//! - `u-turn-json`: Returns ChatRequest as formatted JSON
//! - `u-turn-summary`: Returns human-readable request summary
//! - `u-turn-slow`: JSON with configurable delay
//!
//! ## Error Models
//! - `u-turn-error-rate-limit`, `u-turn-error-auth`, `u-turn-error-timeout`, `u-turn-error-invalid`
//!
//! # Example
//!
//! ```rust
//! use nxuskit_engine::prelude::*;
//!
//! # async fn example() -> std::result::Result<(), Box<dyn std::error::Error>> {
//! let provider = LoopbackProvider::new();
//! let request = ChatRequest::new("echo")
//!     .with_message(Message::user("I know you are but what am I?"));
//!
//! let response = provider.chat(&request).await?;
//! assert_eq!(response.content, "I know you are but what am I?");
//! # Ok(())
//! # }
//! ```

use async_stream::stream;
use async_trait::async_trait;
use futures::Stream;
use std::time::Duration;

use crate::{
    ChatRequest, ChatResponse, LLMProvider, ModelInfo, ModelLister, StreamChunk, TokenCount,
    TokenUsage,
    error::{NxuskitError, Result},
    parameter_adapter::ParameterAdapter,
    token_estimator::{StreamingTokenAccumulator, TokenEstimator},
    types::{
        ContentPart, FinishReason, InferenceMetadata, LogprobsData, MessageContent,
        ProviderCapabilities, TokenLogprob, TopLogprob,
    },
};

/// A provider that echoes request data back for testing purposes
///
/// The Loopback provider helps developers verify what their client code
/// is sending to LLM providers and test graceful degradation with
/// limited-capability models.
///
/// # Example
///
/// ```rust
/// use nxuskit_engine::prelude::*;
///
/// # async fn example() -> std::result::Result<(), Box<dyn std::error::Error>> {
/// let provider = LoopbackProvider::new();
/// let request = ChatRequest::new("echo")
///     .with_message(Message::user("I know you are but what am I?"));
///
/// let response = provider.chat(&request).await?;
/// assert_eq!(response.content, "I know you are but what am I?");
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct LoopbackProvider {
    /// Delay duration for u-turn-slow model
    delay: Duration,
}

impl LoopbackProvider {
    /// Create a new LoopbackProvider with default settings
    pub fn new() -> Self {
        Self {
            delay: Duration::from_secs(1),
        }
    }

    /// Create a builder for configuring the provider
    pub fn builder() -> LoopbackBuilder {
        LoopbackBuilder::default()
    }
}

impl Default for LoopbackProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl LoopbackProvider {
    /// Create a fresh session with no accumulated state.
    ///
    /// For LoopbackProvider, this returns a clone with the same configuration
    /// since it's already stateless.
    ///
    /// # Returns
    ///
    /// A new LoopbackProvider instance with the same configuration.
    pub fn fresh_session(&self) -> Self {
        self.clone()
    }
}

/// Builder for LoopbackProvider configuration
#[derive(Debug, Default)]
pub struct LoopbackBuilder {
    delay: Option<Duration>,
}

impl LoopbackBuilder {
    /// Set the delay duration for u-turn-slow model
    ///
    /// Default: 1 second
    pub fn delay(mut self, delay: Duration) -> Self {
        self.delay = Some(delay);
        self
    }

    /// Build the LoopbackProvider
    pub fn build(self) -> Result<LoopbackProvider> {
        Ok(LoopbackProvider {
            delay: self.delay.unwrap_or(Duration::from_secs(1)),
        })
    }
}

/// Internal enum for parsing model names and their capabilities
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoopbackModel {
    // Echo models
    Echo,
    EchoJsonNative,
    EchoJsonFallback,
    EchoLimitedClaude,
    EchoLimitedMinimal,

    // U-Turn models
    UTurnJson,
    UTurnSummary,
    UTurnSlow,

    // Error models
    ErrorRateLimit,
    ErrorAuth,
    ErrorTimeout,
    ErrorInvalid,
}

impl LoopbackModel {
    /// Parse model name to enum variant
    fn from_name(name: &str) -> Option<Self> {
        match name {
            "echo" => Some(Self::Echo),
            "echo-json-native" => Some(Self::EchoJsonNative),
            "echo-json-fallback" => Some(Self::EchoJsonFallback),
            "echo-limited-claude" => Some(Self::EchoLimitedClaude),
            "echo-limited-minimal" => Some(Self::EchoLimitedMinimal),
            "u-turn-json" => Some(Self::UTurnJson),
            "u-turn-summary" => Some(Self::UTurnSummary),
            "u-turn-slow" => Some(Self::UTurnSlow),
            "u-turn-error-rate-limit" => Some(Self::ErrorRateLimit),
            "u-turn-error-auth" => Some(Self::ErrorAuth),
            "u-turn-error-timeout" => Some(Self::ErrorTimeout),
            "u-turn-error-invalid" => Some(Self::ErrorInvalid),
            _ => None,
        }
    }

    /// Get capabilities for this model
    fn capabilities(&self) -> ProviderCapabilities {
        match self {
            // Full capabilities
            Self::Echo
            | Self::EchoJsonNative
            | Self::UTurnJson
            | Self::UTurnSummary
            | Self::UTurnSlow => full_capabilities(),

            // JSON fallback (no native JSON mode)
            Self::EchoJsonFallback => {
                let mut caps = full_capabilities();
                caps.supports_json_mode = false;
                caps.supports_json_schema = false;
                caps
            }

            // Claude-like limitations
            Self::EchoLimitedClaude => ProviderCapabilities {
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
            },

            // Minimal capabilities
            Self::EchoLimitedMinimal => ProviderCapabilities {
                supports_system_messages: true,
                supports_streaming: true,
                supports_vision: false,
                max_stop_sequences: Some(2),
                supports_presence_penalty: false,
                supports_frequency_penalty: false,
                supports_seed: false,
                supports_logprobs: false,

                supports_streaming_logprobs: false,
                supports_json_mode: false,
                supports_json_schema: false,
                penalty_range: None,
                max_logprobs: None,
            },

            // Error models - return full capabilities (they'll error anyway)
            Self::ErrorRateLimit | Self::ErrorAuth | Self::ErrorTimeout | Self::ErrorInvalid => {
                full_capabilities()
            }
        }
    }

    /// Get all available model names
    fn all_names() -> &'static [&'static str] {
        &[
            "echo",
            "echo-json-native",
            "echo-json-fallback",
            "echo-limited-claude",
            "echo-limited-minimal",
            "u-turn-json",
            "u-turn-summary",
            "u-turn-slow",
            "u-turn-error-rate-limit",
            "u-turn-error-auth",
            "u-turn-error-timeout",
            "u-turn-error-invalid",
        ]
    }
}

fn full_capabilities() -> ProviderCapabilities {
    ProviderCapabilities {
        supports_system_messages: true,
        supports_streaming: true,
        supports_vision: true,
        max_stop_sequences: Some(10),
        supports_presence_penalty: true,
        supports_frequency_penalty: true,
        supports_seed: true,
        supports_logprobs: true,

        supports_streaming_logprobs: false,
        supports_json_mode: true,
        supports_json_schema: true,
        penalty_range: Some((-2.0, 2.0)),
        max_logprobs: Some(20),
    }
}

fn synthesize_logprobs(content: &str, top_logprobs: Option<u8>) -> LogprobsData {
    let token = first_logprob_token(content);
    let bytes = (!token.is_empty()).then(|| token.as_bytes().to_vec());
    let alternatives = [
        ("Lyon", -3.2_f32, &[76_u8, 121, 111, 110][..]),
        (
            "Marseille",
            -4.7_f32,
            &[77_u8, 97, 114, 115, 101, 105, 108, 108, 101][..],
        ),
        (
            "Toulouse",
            -5.1_f32,
            &[84_u8, 111, 117, 108, 111, 117, 115, 101][..],
        ),
        ("Nice", -5.6_f32, &[78_u8, 105, 99, 101][..]),
        ("Nantes", -6.0_f32, &[78_u8, 97, 110, 116, 101, 115][..]),
    ];
    let alternative_count = top_logprobs
        .map(|count| usize::from(count).min(alternatives.len()))
        .unwrap_or(0);

    LogprobsData {
        content: vec![TokenLogprob {
            token,
            logprob: -0.01,
            bytes,
            top_logprobs: alternatives
                .iter()
                .take(alternative_count)
                .map(|(token, logprob, bytes)| TopLogprob {
                    token: (*token).to_string(),
                    logprob: *logprob,
                    bytes: Some(bytes.to_vec()),
                })
                .collect(),
        }],
    }
}

fn first_logprob_token(content: &str) -> String {
    let token = content
        .split_whitespace()
        .next()
        .unwrap_or(content)
        .trim_matches(|c: char| !c.is_alphanumeric());

    if token.is_empty() {
        content.to_string()
    } else {
        token.to_string()
    }
}

/// Extract text content from a message
fn message_to_string(msg: &crate::Message) -> String {
    match &msg.content {
        MessageContent::Text(text) => text.clone(),
        MessageContent::Parts(parts) => parts
            .iter()
            .filter_map(|p| match p {
                ContentPart::Text { text } => Some(text.as_str()),
                ContentPart::Image { .. } => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

/// Get the last user message content from a request
fn get_last_user_message(request: &ChatRequest) -> Option<String> {
    request
        .messages
        .iter()
        .rev()
        .find(|m| m.role == crate::types::Role::User)
        .map(message_to_string)
}

/// Estimate token count from text (rough: ~4 chars per token)
fn estimate_tokens(text: &str) -> u32 {
    (text.len() / 4).max(1) as u32
}

/// Calculate usage based on request and response
fn calculate_usage(request: &ChatRequest, response_content: &str) -> TokenUsage {
    let prompt_tokens: u32 = request
        .messages
        .iter()
        .map(|m| estimate_tokens(&message_to_string(m)))
        .sum();

    let completion_tokens = estimate_tokens(response_content);

    let estimated_count = TokenCount::new(prompt_tokens, completion_tokens);
    TokenUsage::estimated_only(estimated_count)
}

/// Format the request as JSON
fn format_request_as_json(request: &ChatRequest) -> String {
    serde_json::to_string_pretty(request).unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e))
}

/// Format the request as a human-readable summary
fn format_request_as_summary(request: &ChatRequest) -> String {
    let mut summary = String::new();
    summary.push_str("=== Loopback Request Summary ===\n\n");

    summary.push_str(&format!("Model: {}\n", request.model));
    summary.push_str(&format!("Messages: {}\n", request.messages.len()));

    for (i, msg) in request.messages.iter().enumerate() {
        let content = message_to_string(msg);
        let preview = if content.len() > 50 {
            format!("{}...", &content[..50])
        } else {
            content
        };
        summary.push_str(&format!("  [{}] {:?}: {}\n", i + 1, msg.role, preview));
    }

    summary.push_str("\nParameters:\n");

    if let Some(temp) = request.temperature {
        summary.push_str(&format!("  temperature: {}\n", temp));
    }
    if let Some(max_tokens) = request.max_tokens {
        summary.push_str(&format!("  max_tokens: {}\n", max_tokens));
    }
    if let Some(stop) = &request.stop {
        summary.push_str(&format!("  stop: {:?}\n", stop));
    }
    if let Some(presence_penalty) = request.presence_penalty {
        summary.push_str(&format!("  presence_penalty: {}\n", presence_penalty));
    }
    if let Some(frequency_penalty) = request.frequency_penalty {
        summary.push_str(&format!("  frequency_penalty: {}\n", frequency_penalty));
    }
    if let Some(seed) = request.seed {
        summary.push_str(&format!("  seed: {}\n", seed));
    }
    if let Some(logprobs) = request.logprobs
        && logprobs
    {
        if let Some(top) = request.top_logprobs {
            summary.push_str(&format!("  logprobs: true (top {})\n", top));
        } else {
            summary.push_str("  logprobs: true\n");
        }
    }
    if let Some(format) = &request.response_format {
        summary.push_str(&format!("  response_format: {:?}\n", format));
    }

    summary.push_str("\n================================");
    summary
}

/// Get model info for all loopback models
fn loopback_model_info() -> Vec<ModelInfo> {
    vec![
        // Echo models - full metadata
        {
            let mut info = ModelInfo::new("echo");
            info.description = Some("Returns last user message content verbatim".to_string());
            info.context_window = Some(128_000);
            info.metadata
                .insert("capabilities".to_string(), "full".to_string());
            info
        },
        {
            let mut info = ModelInfo::new("echo-json-native");
            info.description = Some("Echo with native JSON mode support".to_string());
            info.context_window = Some(128_000);
            info.metadata
                .insert("capabilities".to_string(), "full".to_string());
            info.metadata
                .insert("json_mode".to_string(), "native".to_string());
            info
        },
        {
            let mut info = ModelInfo::new("echo-json-fallback");
            info.description = Some("Echo without JSON mode (tests prompt fallback)".to_string());
            info.context_window = Some(128_000);
            info.metadata
                .insert("capabilities".to_string(), "limited".to_string());
            info.metadata
                .insert("json_mode".to_string(), "fallback".to_string());
            info
        },
        // Limited models - indicate limitations
        {
            let mut info = ModelInfo::new("echo-limited-claude");
            info.description = Some(
                "Echo with Claude-like limitations (no penalties, seed, logprobs)".to_string(),
            );
            info.context_window = Some(200_000);
            info.metadata
                .insert("capabilities".to_string(), "claude-like".to_string());
            info.metadata.insert(
                "limitations".to_string(),
                "no penalties, no seed, no logprobs".to_string(),
            );
            info
        },
        {
            let mut info = ModelInfo::new("echo-limited-minimal");
            info.description =
                Some("Echo with minimal capabilities (temp, max_tokens, 2 stops only)".to_string());
            info.context_window = Some(4_096);
            info.metadata
                .insert("capabilities".to_string(), "minimal".to_string());
            info.metadata.insert(
                "limitations".to_string(),
                "only temperature, max_tokens, 2 stop sequences".to_string(),
            );
            info
        },
        // U-Turn models
        {
            let mut info = ModelInfo::new("u-turn-json");
            info.description = Some("Returns full ChatRequest as formatted JSON".to_string());
            info.context_window = Some(128_000);
            info.metadata
                .insert("output_format".to_string(), "json".to_string());
            info
        },
        {
            let mut info = ModelInfo::new("u-turn-summary");
            info.description = Some("Returns human-readable request summary".to_string());
            info.context_window = Some(128_000);
            info.metadata
                .insert("output_format".to_string(), "text".to_string());
            info
        },
        {
            let mut info = ModelInfo::new("u-turn-slow");
            info.description = Some("JSON response with configurable delay".to_string());
            info.context_window = Some(128_000);
            info.metadata
                .insert("output_format".to_string(), "json".to_string());
            info.metadata
                .insert("has_delay".to_string(), "true".to_string());
            info
        },
        // Error models - minimal metadata
        {
            let mut info = ModelInfo::new("u-turn-error-rate-limit");
            info.description = Some("Simulates rate limit error".to_string());
            info
        },
        {
            let mut info = ModelInfo::new("u-turn-error-auth");
            info.description = Some("Simulates authentication error".to_string());
            info
        },
        {
            let mut info = ModelInfo::new("u-turn-error-timeout");
            info.description = Some("Simulates network timeout error".to_string());
            info
        },
        {
            let mut info = ModelInfo::new("u-turn-error-invalid");
            info.description = Some("Simulates invalid request error".to_string());
            info
        },
    ]
}

#[async_trait]
impl LLMProvider for LoopbackProvider {
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse> {
        // Parse model name
        let model = LoopbackModel::from_name(&request.model).ok_or_else(|| {
            NxuskitError::InvalidRequest(format!(
                "Unknown loopback model '{}'. Available models: {}",
                request.model,
                LoopbackModel::all_names().join(", ")
            ))
        })?;

        // Handle error models first
        match model {
            LoopbackModel::ErrorRateLimit => {
                return Err(NxuskitError::RateLimit { retry_after: None });
            }
            LoopbackModel::ErrorAuth => {
                return Err(NxuskitError::Authentication(
                    "Authentication failed (simulated)".to_string(),
                ));
            }
            LoopbackModel::ErrorTimeout => {
                return Err(NxuskitError::Stream(
                    "Request timeout (simulated)".to_string(),
                ));
            }
            LoopbackModel::ErrorInvalid => {
                return Err(NxuskitError::InvalidRequest(
                    "Invalid request (simulated)".to_string(),
                ));
            }
            _ => {}
        }

        // Apply delay for slow model
        if model == LoopbackModel::UTurnSlow {
            tokio::time::sleep(self.delay).await;
        }

        // Adapt parameters to model capabilities
        let capabilities = model.capabilities();
        let adapted = ParameterAdapter::adapt(request, &capabilities);

        // Generate response content based on model
        let content = match model {
            LoopbackModel::Echo
            | LoopbackModel::EchoJsonNative
            | LoopbackModel::EchoJsonFallback
            | LoopbackModel::EchoLimitedClaude
            | LoopbackModel::EchoLimitedMinimal => {
                get_last_user_message(request).unwrap_or_default()
            }
            LoopbackModel::UTurnJson | LoopbackModel::UTurnSlow => format_request_as_json(request),
            LoopbackModel::UTurnSummary => format_request_as_summary(request),
            // Error models handled above
            _ => unreachable!(),
        };

        // Calculate usage
        let usage = calculate_usage(request, &content);

        // Build response
        let mut response = ChatResponse::new(content, request.model.clone(), usage);
        response.provider = self.provider_name().to_string();
        response.warnings = adapted.warnings;
        response.finish_reason = Some(FinishReason::Stop);
        if adapted.request.logprobs == Some(true) {
            response.logprobs = Some(synthesize_logprobs(
                &response.content,
                adapted.request.top_logprobs,
            ));
        }

        // Populate inference metadata
        response.inference_metadata = InferenceMetadata::completed(FinishReason::Stop)
            .with_token_usage(response.usage.clone())
            .with_provider_metadata(serde_json::json!({
                "provider": "loopback",
                "model_type": request.model.clone()
            }));

        Ok(response)
    }

    async fn chat_stream(
        &self,
        request: &ChatRequest,
    ) -> Result<Box<dyn Stream<Item = Result<StreamChunk>> + Send + Unpin>> {
        // Parse model and handle errors
        let model = LoopbackModel::from_name(&request.model).ok_or_else(|| {
            NxuskitError::InvalidRequest(format!(
                "Unknown loopback model '{}'. Available models: {}",
                request.model,
                LoopbackModel::all_names().join(", ")
            ))
        })?;

        // Handle error models
        match model {
            LoopbackModel::ErrorRateLimit => {
                return Err(NxuskitError::RateLimit { retry_after: None });
            }
            LoopbackModel::ErrorAuth => {
                return Err(NxuskitError::Authentication(
                    "Authentication failed (simulated)".to_string(),
                ));
            }
            LoopbackModel::ErrorTimeout => {
                return Err(NxuskitError::Stream(
                    "Request timeout (simulated)".to_string(),
                ));
            }
            LoopbackModel::ErrorInvalid => {
                return Err(NxuskitError::InvalidRequest(
                    "Invalid request (simulated)".to_string(),
                ));
            }
            _ => {}
        }

        let delay = self.delay;
        let is_slow = model == LoopbackModel::UTurnSlow;

        // Generate content
        let content = match model {
            LoopbackModel::Echo
            | LoopbackModel::EchoJsonNative
            | LoopbackModel::EchoJsonFallback
            | LoopbackModel::EchoLimitedClaude
            | LoopbackModel::EchoLimitedMinimal => {
                get_last_user_message(request).unwrap_or_default()
            }
            LoopbackModel::UTurnJson | LoopbackModel::UTurnSlow => format_request_as_json(request),
            LoopbackModel::UTurnSummary => format_request_as_summary(request),
            _ => unreachable!(),
        };

        // Initialize token tracking outside stream! macro
        let model = request.model.clone();
        let estimator = TokenEstimator::for_model(&model);
        let prompt_tokens = estimator.count_messages(&request.messages);

        let output_stream = stream! {
            // Apply delay for slow model
            if is_slow {
                tokio::time::sleep(delay).await;
            }

            // Initialize accumulator for this test fixture
            let mut accumulator = StreamingTokenAccumulator::new(estimator, prompt_tokens);
            accumulator.add_chunk(&content);
            let usage = accumulator.running_total();

            // Yield content as single chunk with running usage
            let mut chunk = StreamChunk::new(content);
            chunk.usage = Some(usage);
            yield Ok(chunk);

            // Yield final chunk with complete usage
            let final_usage = accumulator.finalize();
            yield Ok(StreamChunk::final_chunk(crate::types::FinishReason::Stop, Some(final_usage)));
        };

        Ok(Box::new(Box::pin(output_stream)))
    }

    fn provider_name(&self) -> &str {
        "loopback"
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        Ok(loopback_model_info())
    }

    fn get_capabilities(&self) -> ProviderCapabilities {
        // Return full capabilities by default
        // Individual chat() calls use model-specific capabilities
        full_capabilities()
    }
}

#[async_trait]
impl ModelLister for LoopbackProvider {
    async fn list_available_models(&self) -> Result<Vec<ModelInfo>> {
        // Return the same models as list_models() to ensure consistent behavior
        self.list_models().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Message;
    use crate::types::ResponseFormat;
    use futures::StreamExt;

    // ============ Phase 2: Echo Model Tests ============

    #[tokio::test]
    async fn test_echo_basic() {
        let provider = LoopbackProvider::new();
        let request =
            ChatRequest::new("echo").with_message(Message::user("I know you are but what am I?"));

        let response = provider.chat(&request).await.unwrap();
        assert_eq!(response.content, "I know you are but what am I?");
    }

    #[tokio::test]
    async fn test_echo_last_message_only() {
        let provider = LoopbackProvider::new();
        let request = ChatRequest::new("echo")
            .with_message(Message::user("First message"))
            .with_message(Message::assistant("Response"))
            .with_message(Message::user("Second message"));

        let response = provider.chat(&request).await.unwrap();
        assert_eq!(response.content, "Second message");
    }

    #[tokio::test]
    async fn test_echo_synthesizes_logprobs_when_requested() {
        let provider = LoopbackProvider::new();
        let mut request = ChatRequest::new("echo").with_message(Message::user("Paris."));
        request.logprobs = Some(true);
        request.top_logprobs = Some(5);

        let response = provider.chat(&request).await.unwrap();
        let logprobs = response.logprobs.expect("logprobs should be synthesized");

        assert_eq!(response.content, "Paris.");
        assert_eq!(logprobs.content[0].token, "Paris");
        assert!((logprobs.content[0].logprob - -0.01).abs() < f32::EPSILON);
        assert_eq!(
            logprobs.content[0].bytes.as_deref(),
            Some(&[80_u8, 97, 114, 105, 115][..])
        );
        assert_eq!(logprobs.content[0].top_logprobs[0].token, "Lyon");
        assert!((logprobs.content[0].top_logprobs[0].logprob - -3.2).abs() < f32::EPSILON);
    }

    #[tokio::test]
    async fn test_echo_no_user_message() {
        let provider = LoopbackProvider::new();
        let request = ChatRequest::new("echo").with_message(Message::system("System prompt only"));

        let response = provider.chat(&request).await.unwrap();
        assert_eq!(response.content, "");
    }

    // ============ Phase 3: U-Turn Model Tests ============

    #[tokio::test]
    async fn test_uturn_json_valid() {
        let provider = LoopbackProvider::new();
        let request = ChatRequest::new("u-turn-json").with_message(Message::user("Hello"));

        let response = provider.chat(&request).await.unwrap();

        // Verify it's valid JSON
        let parsed: serde_json::Value = serde_json::from_str(&response.content).unwrap();
        assert!(parsed.is_object());
    }

    #[tokio::test]
    async fn test_uturn_json_all_fields() {
        let provider = LoopbackProvider::new();
        let request = ChatRequest::new("u-turn-json")
            .with_message(Message::user("Hello"))
            .with_temperature(0.7)
            .with_max_tokens(100);

        let response = provider.chat(&request).await.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&response.content).unwrap();

        assert_eq!(parsed["model"], "u-turn-json");
        assert!(parsed["messages"].is_array());
        assert_eq!(parsed["temperature"], 0.7);
        assert_eq!(parsed["max_tokens"], 100);
    }

    #[tokio::test]
    async fn test_uturn_summary_readable() {
        let provider = LoopbackProvider::new();
        let request = ChatRequest::new("u-turn-summary").with_message(Message::user("Hello"));

        let response = provider.chat(&request).await.unwrap();

        assert!(
            response
                .content
                .contains("=== Loopback Request Summary ===")
        );
        assert!(response.content.contains("Model: u-turn-summary"));
        assert!(response.content.contains("Messages: 1"));
    }

    #[tokio::test]
    async fn test_uturn_summary_all_params() {
        let provider = LoopbackProvider::new();
        let mut request = ChatRequest::new("u-turn-summary")
            .with_message(Message::user("Hello"))
            .with_temperature(0.7)
            .with_max_tokens(100);
        request.stop = Some(vec!["END".to_string()]);

        let response = provider.chat(&request).await.unwrap();

        assert!(response.content.contains("temperature: 0.7"));
        assert!(response.content.contains("max_tokens: 100"));
        assert!(response.content.contains("stop:"));
    }

    // ============ Phase 4: Limited Capability Tests ============

    #[tokio::test]
    async fn test_limited_claude_no_penalties() {
        let provider = LoopbackProvider::new();
        let mut request =
            ChatRequest::new("echo-limited-claude").with_message(Message::user("Hello"));
        request.presence_penalty = Some(0.5);

        let response = provider.chat(&request).await.unwrap();
        assert_eq!(response.content, "Hello");
        assert!(!response.warnings.is_empty());
        assert!(
            response
                .warnings
                .iter()
                .any(|w| w.parameter == "presence_penalty")
        );
    }

    #[tokio::test]
    async fn test_limited_claude_no_seed() {
        let provider = LoopbackProvider::new();
        let mut request =
            ChatRequest::new("echo-limited-claude").with_message(Message::user("Hello"));
        request.seed = Some(12345);

        let response = provider.chat(&request).await.unwrap();
        assert!(!response.warnings.is_empty());
        assert!(response.warnings.iter().any(|w| w.parameter == "seed"));
    }

    #[tokio::test]
    async fn test_limited_minimal_stop_truncation() {
        let provider = LoopbackProvider::new();
        let mut request =
            ChatRequest::new("echo-limited-minimal").with_message(Message::user("Hello"));
        request.stop = Some(vec![
            "A".to_string(),
            "B".to_string(),
            "C".to_string(),
            "D".to_string(),
            "E".to_string(),
        ]);

        let response = provider.chat(&request).await.unwrap();
        assert!(!response.warnings.is_empty());
        assert!(
            response
                .warnings
                .iter()
                .any(|w| w.parameter == "stop" || w.message.contains("truncat"))
        );
    }

    #[tokio::test]
    async fn test_limited_minimal_no_vision() {
        let _provider = LoopbackProvider::new();
        let capabilities =
            LoopbackModel::from_name("echo-limited-minimal").map(|m| m.capabilities());

        assert!(!capabilities.unwrap().supports_vision);
    }

    // ============ Phase 5: JSON Mode Tests ============

    #[tokio::test]
    async fn test_json_native_no_warning() {
        let provider = LoopbackProvider::new();
        let mut request = ChatRequest::new("echo-json-native").with_message(Message::user("Hello"));
        request.response_format = Some(ResponseFormat::Json);

        let response = provider.chat(&request).await.unwrap();
        // Should not have JSON-related warnings since this model supports native JSON
        assert!(
            response
                .warnings
                .iter()
                .all(|w| !w.message.contains("json") && !w.message.contains("JSON"))
        );
    }

    #[tokio::test]
    async fn test_json_fallback_warning() {
        let provider = LoopbackProvider::new();
        let mut request =
            ChatRequest::new("echo-json-fallback").with_message(Message::user("Hello"));
        request.response_format = Some(ResponseFormat::Json);

        let response = provider.chat(&request).await.unwrap();
        // Should have JSON fallback warning
        assert!(
            response
                .warnings
                .iter()
                .any(|w| w.message.to_lowercase().contains("json"))
        );
    }

    #[tokio::test]
    async fn test_json_capabilities_differ() {
        let native_caps = LoopbackModel::from_name("echo-json-native")
            .map(|m| m.capabilities())
            .unwrap();
        let fallback_caps = LoopbackModel::from_name("echo-json-fallback")
            .map(|m| m.capabilities())
            .unwrap();

        assert!(native_caps.supports_json_mode);
        assert!(!fallback_caps.supports_json_mode);
    }

    // ============ Phase 6: Delay Tests ============

    #[tokio::test]
    async fn test_slow_model_has_delay() {
        let provider = LoopbackProvider::builder()
            .delay(Duration::from_millis(100))
            .build()
            .unwrap();

        let request = ChatRequest::new("u-turn-slow").with_message(Message::user("Hello"));

        let start = std::time::Instant::now();
        let _response = provider.chat(&request).await.unwrap();
        let elapsed = start.elapsed();

        assert!(elapsed >= Duration::from_millis(100));
    }

    #[tokio::test]
    async fn test_slow_model_configurable() {
        let provider = LoopbackProvider::builder()
            .delay(Duration::from_millis(50))
            .build()
            .unwrap();

        let request = ChatRequest::new("u-turn-slow").with_message(Message::user("Hello"));

        let start = std::time::Instant::now();
        let _response = provider.chat(&request).await.unwrap();
        let elapsed = start.elapsed();

        assert!(elapsed >= Duration::from_millis(50));
        assert!(elapsed < Duration::from_millis(500)); // Not too slow
    }

    #[tokio::test]
    async fn test_slow_stream_has_delay() {
        let provider = LoopbackProvider::builder()
            .delay(Duration::from_millis(100))
            .build()
            .unwrap();

        let request = ChatRequest::new("u-turn-slow").with_message(Message::user("Hello"));

        let start = std::time::Instant::now();
        let mut stream = provider.chat_stream(&request).await.unwrap();

        // Consume first chunk
        let _ = stream.next().await;
        let elapsed = start.elapsed();

        assert!(elapsed >= Duration::from_millis(100));
    }

    // ============ Phase 7: Error Tests ============

    #[tokio::test]
    async fn test_error_rate_limit() {
        let provider = LoopbackProvider::new();
        let request =
            ChatRequest::new("u-turn-error-rate-limit").with_message(Message::user("Hello"));

        let result = provider.chat(&request).await;
        assert!(matches!(result, Err(NxuskitError::RateLimit { .. })));
    }

    #[tokio::test]
    async fn test_error_auth() {
        let provider = LoopbackProvider::new();
        let request = ChatRequest::new("u-turn-error-auth").with_message(Message::user("Hello"));

        let result = provider.chat(&request).await;
        assert!(matches!(result, Err(NxuskitError::Authentication(_))));
    }

    #[tokio::test]
    async fn test_error_timeout() {
        let provider = LoopbackProvider::new();
        let request = ChatRequest::new("u-turn-error-timeout").with_message(Message::user("Hello"));

        let result = provider.chat(&request).await;
        assert!(matches!(result, Err(NxuskitError::Stream(_))));
    }

    #[tokio::test]
    async fn test_error_invalid() {
        let provider = LoopbackProvider::new();
        let request = ChatRequest::new("u-turn-error-invalid").with_message(Message::user("Hello"));

        let result = provider.chat(&request).await;
        assert!(matches!(result, Err(NxuskitError::InvalidRequest(_))));
    }

    #[tokio::test]
    async fn test_unknown_model_error() {
        let provider = LoopbackProvider::new();
        let request = ChatRequest::new("unknown-model").with_message(Message::user("Hello"));

        let result = provider.chat(&request).await;
        assert!(matches!(result, Err(NxuskitError::InvalidRequest(_))));

        if let Err(NxuskitError::InvalidRequest(msg)) = result {
            assert!(msg.contains("Unknown loopback model"));
            assert!(msg.contains("echo")); // Lists available models
        }
    }

    // ============ Phase 8: Model Discovery Tests ============

    #[tokio::test]
    async fn test_list_models_count() {
        let provider = LoopbackProvider::new();
        let models = provider.list_models().await.unwrap();
        assert_eq!(models.len(), 12);
    }

    #[tokio::test]
    async fn test_list_models_metadata() {
        let provider = LoopbackProvider::new();
        let models = provider.list_models().await.unwrap();

        let echo = models.iter().find(|m| m.name == "echo").unwrap();
        assert!(echo.description.is_some());
        assert!(echo.context_window.is_some());
    }

    #[tokio::test]
    async fn test_list_models_limited_metadata() {
        let provider = LoopbackProvider::new();
        let models = provider.list_models().await.unwrap();

        let limited = models
            .iter()
            .find(|m| m.name == "echo-limited-claude")
            .unwrap();
        assert!(limited.metadata.contains_key("limitations"));
    }

    // ============ Streaming Tests ============

    #[tokio::test]
    async fn test_stream_echo() {
        let provider = LoopbackProvider::new();
        let request = ChatRequest::new("echo").with_message(Message::user("Hello streaming world"));

        let mut stream = provider.chat_stream(&request).await.unwrap();
        let mut chunks = Vec::new();

        while let Some(chunk) = stream.next().await {
            chunks.push(chunk.unwrap());
        }

        // Should have content chunk + final chunk
        assert!(chunks.len() >= 2);
        assert!(chunks.last().unwrap().is_final());

        // First chunk should contain content
        assert_eq!(chunks[0].delta, "Hello streaming world");
    }
}
