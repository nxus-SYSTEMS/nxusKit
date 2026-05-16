//! Safe Rust types for the nxusKit C ABI.
//!
//! These types are serialized to/from JSON at the FFI boundary. Field names
//! and nesting match the C ABI's JSON schema exactly.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Provider configuration
// ---------------------------------------------------------------------------

/// Configuration for creating a provider instance.
///
/// Serialized to JSON and passed to `nxuskit_create_provider()`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderConfig {
    /// Provider identifier: `"claude"`, `"openai"`, `"ollama"`, `"clips"`,
    /// `"mock"`, `"loopback"`, etc.
    pub provider_type: String,

    /// API key for cloud providers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    /// Default model name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Custom API base URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,

    /// Request timeout in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,

    /// License key for tiered feature access.
    ///
    /// Passed through to the SDK binary without client-side validation.
    /// When `None`, the field is omitted from serialized JSON (backward
    /// compatible with older SDK binaries).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub license_key: Option<String>,
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

/// Message role in a conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    /// Tool result message (used for sending tool execution results back).
    Tool,
}

/// Image data source — either a URL or base64-encoded data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ImageData {
    /// Image referenced by URL.
    Url { url: String },
    /// Base64-encoded image data.
    Base64 { media_type: String, data: String },
}

/// Image source with optional detail level.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageSource {
    /// The actual image data (URL or base64).
    #[serde(flatten)]
    pub data: ImageData,
    /// Optional detail level ("low", "high", "auto" — OpenAI-specific).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// A single content part — either text or an image.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    /// Text content.
    Text { text: String },
    /// Image content.
    Image {
        #[serde(flatten)]
        source: ImageSource,
    },
}

/// Message content — either simple text or multimodal parts.
///
/// Serializes as a plain string for text-only messages (backward compatible)
/// or as an array of content parts for multimodal messages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    /// Simple text message.
    Text(String),
    /// Structured content with multiple parts (text + images).
    Parts(Vec<ContentPart>),
}

impl From<String> for MessageContent {
    fn from(s: String) -> Self {
        MessageContent::Text(s)
    }
}

impl From<&str> for MessageContent {
    fn from(s: &str) -> Self {
        MessageContent::Text(s.to_string())
    }
}

impl MessageContent {
    /// Get the text content, extracting from parts if multimodal.
    pub fn text(&self) -> &str {
        match self {
            MessageContent::Text(s) => s,
            MessageContent::Parts(parts) => {
                for part in parts {
                    if let ContentPart::Text { text } = part {
                        return text;
                    }
                }
                ""
            }
        }
    }

    /// Returns true if this content has multiple parts (text + images).
    pub fn is_multimodal(&self) -> bool {
        matches!(self, MessageContent::Parts(_))
    }
}

/// A single message in a chat conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: MessageContent,
}

impl Message {
    /// Create a message with the given role and content.
    ///
    /// # Examples
    ///
    /// ```
    /// use nxuskit::{Message, Role};
    ///
    /// let msg = Message::new(Role::User, "Hello");
    /// assert_eq!(msg.role, Role::User);
    /// assert_eq!(msg.content.text(), "Hello");
    /// ```
    pub fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: MessageContent::Text(content.into()),
        }
    }

    /// Create a system message.
    ///
    /// # Examples
    ///
    /// ```
    /// use nxuskit::{Message, Role};
    ///
    /// let msg = Message::system("You are helpful");
    /// assert_eq!(msg.role, Role::System);
    /// ```
    pub fn system(content: impl Into<String>) -> Self {
        Self::new(Role::System, content)
    }

    /// Create a user message.
    ///
    /// # Examples
    ///
    /// ```
    /// use nxuskit::{Message, Role};
    ///
    /// let msg = Message::user("Hello!");
    /// assert_eq!(msg.role, Role::User);
    /// ```
    pub fn user(content: impl Into<String>) -> Self {
        Self::new(Role::User, content)
    }

    /// Create an assistant message.
    ///
    /// # Examples
    ///
    /// ```
    /// use nxuskit::{Message, Role};
    ///
    /// let msg = Message::assistant("I can help with that.");
    /// assert_eq!(msg.role, Role::Assistant);
    /// ```
    pub fn assistant(content: impl Into<String>) -> Self {
        Self::new(Role::Assistant, content)
    }

    /// Add an image from a URL, converting to multimodal if needed.
    ///
    /// # Examples
    ///
    /// ```
    /// use nxuskit::Message;
    ///
    /// let msg = Message::user("What's in this image?")
    ///     .with_image_url("https://example.com/photo.jpg");
    /// assert!(msg.content.is_multimodal());
    /// ```
    pub fn with_image_url(mut self, url: impl Into<String>) -> Self {
        let image_part = ContentPart::Image {
            source: ImageSource {
                data: ImageData::Url { url: url.into() },
                detail: None,
            },
        };
        self.content = match self.content {
            MessageContent::Text(text) => {
                MessageContent::Parts(vec![ContentPart::Text { text }, image_part])
            }
            MessageContent::Parts(mut parts) => {
                parts.push(image_part);
                MessageContent::Parts(parts)
            }
        };
        self
    }

    /// Add a base64-encoded image, converting to multimodal if needed.
    ///
    /// # Examples
    ///
    /// ```
    /// use nxuskit::Message;
    ///
    /// let msg = Message::user("Describe this")
    ///     .with_image_base64("iVBOR...", "image/png");
    /// assert!(msg.content.is_multimodal());
    /// ```
    pub fn with_image_base64(
        mut self,
        data: impl Into<String>,
        media_type: impl Into<String>,
    ) -> Self {
        let image_part = ContentPart::Image {
            source: ImageSource {
                data: ImageData::Base64 {
                    media_type: media_type.into(),
                    data: data.into(),
                },
                detail: None,
            },
        };
        self.content = match self.content {
            MessageContent::Text(text) => {
                MessageContent::Parts(vec![ContentPart::Text { text }, image_part])
            }
            MessageContent::Parts(mut parts) => {
                parts.push(image_part);
                MessageContent::Parts(parts)
            }
        };
        self
    }

    /// Set the detail level for the last added image (OpenAI-specific).
    ///
    /// Values: "low", "high", "auto".
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        if let MessageContent::Parts(ref mut parts) = self.content {
            if let Some(ContentPart::Image { source }) = parts.last_mut() {
                source.detail = Some(detail.into());
            }
        }
        self
    }
}

// ---------------------------------------------------------------------------
// Chat request
// ---------------------------------------------------------------------------

/// Thinking/reasoning mode control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThinkingMode {
    Auto,
    Enabled,
    Disabled,
    Omit,
}

/// Response format configuration for structured outputs.
///
/// Setting [`ResponseFormat::Json`] forces providers to return valid JSON,
/// which suppresses thinking-mode prose in models like qwen3.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseFormat {
    /// Plain text response (default).
    Text,
    /// JSON response (native support where available).
    Json,
    /// JSON response with schema validation.
    JsonSchema {
        /// JSON schema to validate against.
        schema: serde_json::Value,
    },
}

/// Request payload for [`crate::NxuskitProvider::chat`] and [`crate::NxuskitProvider::chat_stream`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChatRequest {
    /// Model to use for this request.
    pub model: String,

    /// Conversation messages.
    pub messages: Vec<Message>,

    /// Sampling temperature (0.0–2.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,

    /// Maximum tokens to generate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,

    /// Nucleus sampling parameter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,

    /// Stop sequences.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,

    /// Whether to stream (set internally by `chat_stream`).
    #[serde(default)]
    pub stream: bool,

    /// Presence penalty (-2.0 to 2.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,

    /// Frequency penalty (-2.0 to 2.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,

    /// Random seed for deterministic output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,

    /// Whether to return log probabilities for generated tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<bool>,

    /// Number of likely alternative tokens to return at each generated position.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_logprobs: Option<u8>,

    /// Reasoning effort hint for reasoning-capable models.
    ///
    /// GPT-5.4 compatibility rules warn-and-drop `temperature`, `top_p`, and
    /// logprob controls when this value is present and not `"none"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,

    /// Thinking/reasoning mode control.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_mode: Option<ThinkingMode>,

    /// Response format (text, JSON, or JSON with schema).
    ///
    /// Setting this to [`ResponseFormat::Json`] forces the provider to return valid JSON,
    /// which is especially useful for Ollama models with thinking mode (e.g. qwen3).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ResponseFormat>,

    /// Provider-specific options (opaque JSON).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_options: Option<serde_json::Value>,

    /// Tool definitions available for the model to call.
    ///
    /// Each tool describes a function the model can invoke. Pass
    /// [`ToolDefinition`](crate::ToolDefinition) values serialized as JSON.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<serde_json::Value>>,

    /// Controls how the model selects tools.
    ///
    /// String values: `"auto"`, `"none"`, `"required"`.
    /// Object: `{"type":"function","function":{"name":"fn_name"}}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
}

impl ChatRequest {
    /// Create a chat request with the given model and sensible defaults.
    ///
    /// # Examples
    ///
    /// ```
    /// use nxuskit::ChatRequest;
    ///
    /// let request = ChatRequest::new("gpt-4o");
    /// assert_eq!(request.model, "gpt-4o");
    /// assert!(request.messages.is_empty());
    /// ```
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            ..Default::default()
        }
    }

    /// Append a single message.
    ///
    /// # Examples
    ///
    /// ```
    /// use nxuskit::{ChatRequest, Message};
    ///
    /// let request = ChatRequest::new("gpt-4o")
    ///     .with_message(Message::user("Hello"));
    /// assert_eq!(request.messages.len(), 1);
    /// ```
    pub fn with_message(mut self, message: Message) -> Self {
        self.messages.push(message);
        self
    }

    /// Append multiple messages.
    pub fn with_messages(mut self, messages: Vec<Message>) -> Self {
        self.messages.extend(messages);
        self
    }

    /// Set the sampling temperature.
    ///
    /// # Examples
    ///
    /// ```
    /// use nxuskit::ChatRequest;
    ///
    /// let request = ChatRequest::new("gpt-4o").with_temperature(0.7);
    /// assert_eq!(request.temperature, Some(0.7));
    /// ```
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// Set the maximum tokens to generate.
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Set the nucleus sampling parameter (top-p).
    pub fn with_top_p(mut self, top_p: f32) -> Self {
        self.top_p = Some(top_p);
        self
    }

    /// Enable or disable token log probability output.
    ///
    /// When enabled, the response's [`ChatResponse::logprobs`] field is
    /// populated with typed [`LogprobsData`]. Pair with
    /// [`with_top_logprobs`](Self::with_top_logprobs) to also receive
    /// alternative-token probabilities at each position.
    ///
    /// ```
    /// use nxuskit::{ChatRequest, Message};
    ///
    /// let request = ChatRequest::new("gpt-5.4")
    ///     .with_message(Message::user("Score the next token."))
    ///     .with_logprobs(true)
    ///     .with_top_logprobs(5);
    ///
    /// assert_eq!(request.logprobs, Some(true));
    /// assert_eq!(request.top_logprobs, Some(5));
    /// ```
    pub fn with_logprobs(mut self, logprobs: bool) -> Self {
        self.logprobs = Some(logprobs);
        self
    }

    /// Set the number of alternative token log probabilities to return.
    ///
    /// Has no effect unless [`with_logprobs(true)`](Self::with_logprobs) is
    /// also set. Providers that do not support logprobs warn-and-drop the
    /// fields rather than tunneling them through `provider_options`.
    pub fn with_top_logprobs(mut self, top_logprobs: u8) -> Self {
        self.top_logprobs = Some(top_logprobs);
        self
    }

    /// Set reasoning effort for reasoning-capable models.
    pub fn with_reasoning_effort(mut self, effort: impl Into<String>) -> Self {
        self.reasoning_effort = Some(effort.into());
        self
    }

    /// Set stop sequences.
    pub fn with_stop(mut self, stop: Vec<String>) -> Self {
        self.stop = Some(stop);
        self
    }

    /// Set the thinking/reasoning mode.
    pub fn with_thinking_mode(mut self, mode: ThinkingMode) -> Self {
        self.thinking_mode = Some(mode);
        self
    }

    /// Set provider-specific options (opaque JSON).
    pub fn with_provider_options(mut self, options: serde_json::Value) -> Self {
        self.provider_options = Some(options);
        self
    }

    /// Set tool definitions for function calling.
    pub fn with_tools(mut self, tools: Vec<serde_json::Value>) -> Self {
        self.tools = Some(tools);
        self
    }

    /// Set the tool choice policy.
    pub fn with_tool_choice(mut self, choice: serde_json::Value) -> Self {
        self.tool_choice = Some(choice);
        self
    }
}

// ---------------------------------------------------------------------------
// Chat response
// ---------------------------------------------------------------------------

/// Individual token count pair.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenCount {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

/// Token consumption statistics.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    /// Always-present estimated counts.
    pub estimated: TokenCount,

    /// Provider-returned actual counts (if available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual: Option<TokenCount>,
}

/// Why generation stopped.
///
/// Matches the nxuskit_engine `FinishReason` enum. Serialized as `snake_case` strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    /// Natural completion — model finished its response.
    Stop,
    /// Maximum token limit reached.
    Length,
    /// Content filtering triggered.
    ContentFilter,
    /// Tool/function call triggered.
    ToolCalls,
    /// Error during generation.
    Error,
}

impl std::fmt::Display for FinishReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stop => write!(f, "stop"),
            Self::Length => write!(f, "length"),
            Self::ContentFilter => write!(f, "content_filter"),
            Self::ToolCalls => write!(f, "tool_calls"),
            Self::Error => write!(f, "error"),
        }
    }
}

/// Severity level for parameter adaptation warnings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WarningSeverity {
    /// Informational (no impact expected).
    Info,
    /// Warning (potential impact on results).
    Warning,
    /// Error (significant issue, fallback used).
    Error,
}

/// A warning about parameter adaptation or unsupported features.
///
/// Matches the nxuskit_engine `ParameterWarning` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterWarning {
    /// Name of the parameter that triggered the warning.
    pub parameter: String,
    /// Human-readable warning message.
    pub message: String,
    /// Severity level of the warning.
    pub severity: WarningSeverity,
}

impl std::fmt::Display for ParameterWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{:?}] {}: {}",
            self.severity, self.parameter, self.message
        )
    }
}

/// Token probability data returned by providers that support logprobs.
///
/// Access selected-token logprobs via `content[i].token` / `content[i].logprob`,
/// and alternative tokens at the same position via `content[i].top_logprobs`.
///
/// ```
/// use nxuskit::{LogprobsData, TokenLogprob, TopLogprob};
///
/// let data = LogprobsData {
///     content: vec![TokenLogprob {
///         token: "Hello".into(),
///         logprob: -0.01,
///         bytes: None,
///         top_logprobs: vec![TopLogprob {
///             token: "Hi".into(),
///             logprob: -1.2,
///             bytes: None,
///         }],
///     }],
/// };
///
/// assert_eq!(data.content[0].token, "Hello");
/// assert_eq!(data.content[0].top_logprobs[0].token, "Hi");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogprobsData {
    /// Token probabilities for each generated token.
    pub content: Vec<TokenLogprob>,
}

/// Probability information for a generated token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenLogprob {
    /// The generated token text.
    pub token: String,

    /// Natural-log probability for the generated token.
    pub logprob: f32,

    /// UTF-8 bytes for the token when the provider returns them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<Vec<u8>>,

    /// Alternative tokens and their probabilities for this position.
    #[serde(default)]
    pub top_logprobs: Vec<TopLogprob>,
}

/// Alternative token probability for a generated position.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopLogprob {
    /// The alternative token text.
    pub token: String,

    /// Natural-log probability for the alternative token.
    pub logprob: f32,

    /// UTF-8 bytes for the token when the provider returns them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<Vec<u8>>,
}

/// Per-chunk logprob delta surfaced on streaming responses (v0.9.4+).
///
/// Mirrors `nxuskit_engine::StreamLogprobsDelta`. `content` is empty
/// when the provider emits a chunk with no logprob entries (e.g., a
/// finish-reason-only chunk).
///
/// # Example
/// ```
/// use nxuskit::{StreamLogprobsDelta, TokenLogprob};
///
/// let delta = StreamLogprobsDelta {
///     content: vec![TokenLogprob {
///         token: "Hello".into(),
///         logprob: -0.01,
///         bytes: None,
///         top_logprobs: vec![],
///     }],
/// };
/// assert_eq!(delta.content[0].token, "Hello");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamLogprobsDelta {
    /// Token logprob entries for the tokens emitted in this chunk.
    pub content: Vec<TokenLogprob>,
}

/// A single step in the inference process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceStep {
    /// Step type: `"rule_firing"`, `"tool_call"`, etc.
    pub step_type: String,

    /// Step identifier (e.g., rule name).
    pub identifier: String,

    /// Step-specific details (opaque JSON).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// Detailed metadata about the inference process.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InferenceMetadata {
    /// Execution time in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_time_ms: Option<u64>,

    /// Whether inference completed normally.
    #[serde(default)]
    pub is_complete: bool,

    /// Inference-level finish reason.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<FinishReason>,

    /// Inference-level token usage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_usage: Option<TokenUsage>,

    /// Chain-of-thought reasoning trace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_trace: Option<String>,

    /// Detailed inference steps (e.g., CLIPS rule firings).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inference_steps: Option<Vec<InferenceStep>>,

    /// Provider-specific metadata (opaque JSON).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_metadata: Option<serde_json::Value>,
}

/// Response from [`crate::NxuskitProvider::chat`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    /// Generated text content.
    pub content: String,

    /// Model that generated the response.
    #[serde(default)]
    pub model: String,

    /// Provider that handled the request.
    #[serde(default)]
    pub provider: String,

    /// Token consumption statistics.
    #[serde(default)]
    pub usage: TokenUsage,

    /// Why generation stopped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<FinishReason>,

    /// Additional provider-specific metadata.
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,

    /// Parameter warnings from the SDK.
    #[serde(default)]
    pub warnings: Vec<ParameterWarning>,

    /// Token probability information, when requested and supported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<LogprobsData>,

    /// Tool calls requested by the model.
    ///
    /// Present when `finish_reason` is [`FinishReason::ToolCalls`]. Each entry
    /// identifies a function to call with JSON-encoded arguments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<serde_json::Value>>,

    /// Detailed inference information.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inference_metadata: Option<InferenceMetadata>,
}

// ---------------------------------------------------------------------------
// Streaming
// ---------------------------------------------------------------------------

/// A single incremental piece of content from a streaming response.
///
/// Fields use `#[serde(default)]` for backward compatibility — older SDK
/// binaries that omit new fields will deserialize correctly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    /// Incremental text content.
    #[serde(default, alias = "content")]
    pub delta: String,

    /// Chunk sequence number.
    #[serde(default)]
    pub index: u32,

    /// Chain-of-thought reasoning content (if enabled).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,

    /// Why generation stopped (set on final chunk).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<FinishReason>,

    /// Token usage statistics (typically only on final chunk).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsage>,

    /// Tool call chunks (incremental tool call data during streaming).
    ///
    /// Present when the model is generating tool calls. Each entry is a
    /// [`ToolCallDelta`](crate::ToolCallDelta) serialized as JSON.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<serde_json::Value>>,

    /// Per-chunk logprob delta (v0.9.4+).
    ///
    /// Populated when the provider supports streaming logprobs and the
    /// request enabled them. `None` for providers that don't support
    /// streaming logprobs OR for chunks that carry no logprob data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<StreamLogprobsDelta>,
}

impl StreamChunk {
    /// Check if this is the final chunk (has a finish reason).
    pub fn is_final(&self) -> bool {
        self.finish_reason.is_some()
    }

    /// Check if this chunk contains thinking content.
    pub fn has_thinking(&self) -> bool {
        self.thinking.as_ref().is_some_and(|t| !t.is_empty())
    }

    /// Check if this chunk contains tool calls.
    pub fn has_tool_calls(&self) -> bool {
        self.tool_calls.as_ref().is_some_and(|tc| !tc.is_empty())
    }
}

// ---------------------------------------------------------------------------
// Model discovery
// ---------------------------------------------------------------------------

/// Information about an available model.
///
/// The C SDK returns `name` for all providers but only some providers include
/// a separate `id` field. When `id` is absent it is populated from `name`.
///
/// The `metadata` map carries provider-specific key-value pairs such as
/// `"modalities"` (`"text,vision"`), `"max_images"`, `"family"`, `"digest"`,
/// etc. Helper methods like [`supports_vision`](Self::supports_vision),
/// [`modalities`](Self::modalities), and [`max_images`](Self::max_images)
/// provide typed access to common capability metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "ModelInfoRaw")]
pub struct ModelInfo {
    /// Model identifier (falls back to `name` when the SDK omits it).
    pub id: String,

    /// Human-readable model name.
    #[serde(default)]
    pub name: String,

    /// Human-readable description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Model file size in bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,

    /// Maximum context length.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u32>,

    /// Provider-specific metadata.
    ///
    /// Common keys include `"modalities"` (`"text,vision"`), `"max_images"`,
    /// `"family"`, `"digest"`, and `"quantization_level"`. Use the helper
    /// methods for typed access to well-known fields.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// Internal helper for deserializing `ModelInfo` with optional `id`.
#[derive(Deserialize)]
struct ModelInfoRaw {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    description: Option<String>,
    size_bytes: Option<u64>,
    context_window: Option<u32>,
    #[serde(default)]
    metadata: HashMap<String, String>,
}

impl From<ModelInfoRaw> for ModelInfo {
    fn from(raw: ModelInfoRaw) -> Self {
        let id = if raw.id.is_empty() {
            raw.name.clone()
        } else {
            raw.id
        };
        Self {
            id,
            name: raw.name,
            description: raw.description,
            size_bytes: raw.size_bytes,
            context_window: raw.context_window,
            metadata: raw.metadata,
        }
    }
}

impl ModelInfo {
    /// Check if this model supports vision/image inputs.
    ///
    /// Returns `true` if the `"modalities"` metadata includes `"vision"`.
    ///
    /// # Example
    /// ```
    /// use nxuskit::ModelInfo;
    ///
    /// let mut info = ModelInfo { id: "gpt-4o".into(), name: "gpt-4o".into(),
    ///     description: None, size_bytes: None, context_window: None,
    ///     metadata: Default::default() };
    /// info.metadata.insert("modalities".into(), "text,vision".into());
    /// assert!(info.supports_vision());
    /// ```
    pub fn supports_vision(&self) -> bool {
        self.metadata
            .get("modalities")
            .map(|m| m.contains("vision"))
            .unwrap_or(false)
    }

    /// Get list of supported modalities.
    ///
    /// Returns modality strings parsed from the `"modalities"` metadata key.
    /// Defaults to `vec!["text"]` if not specified.
    ///
    /// # Example
    /// ```
    /// use nxuskit::ModelInfo;
    ///
    /// let mut info = ModelInfo { id: "claude-3-5-sonnet".into(), name: "claude-3-5-sonnet".into(),
    ///     description: None, size_bytes: None, context_window: None,
    ///     metadata: Default::default() };
    /// info.metadata.insert("modalities".into(), "text,vision".into());
    /// assert_eq!(info.modalities(), vec!["text", "vision"]);
    /// ```
    pub fn modalities(&self) -> Vec<String> {
        self.metadata
            .get("modalities")
            .map(|m| {
                m.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_else(|| vec!["text".to_string()])
    }

    /// Get the maximum number of images supported per request.
    ///
    /// Reads from the `"max_images"` metadata key. Returns `None` if the
    /// key is absent or not a valid integer.
    ///
    /// # Example
    /// ```
    /// use nxuskit::ModelInfo;
    ///
    /// let mut info = ModelInfo { id: "gpt-4o".into(), name: "gpt-4o".into(),
    ///     description: None, size_bytes: None, context_window: None,
    ///     metadata: Default::default() };
    /// info.metadata.insert("max_images".into(), "10".into());
    /// assert_eq!(info.max_images(), Some(10));
    /// ```
    pub fn max_images(&self) -> Option<usize> {
        self.metadata
            .get("max_images")
            .and_then(|s| s.parse::<usize>().ok())
    }
}

// ---------------------------------------------------------------------------
// Runtime capabilities (mirrors nxuskit-core Capabilities JSON)
// ---------------------------------------------------------------------------

/// Domain availability within the SDK build.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityDomains {
    /// Cloud/remote LLM providers.
    #[serde(default)]
    pub llm: bool,
    /// CLIPS rule engine.
    #[serde(default)]
    pub clips: bool,
    /// Z3 constraint solver.
    #[serde(default)]
    pub solver: bool,
    /// Bayesian network inference.
    #[serde(default)]
    pub bayesian: bool,
    /// ZEN decision tables.
    #[serde(default)]
    pub zen: bool,
    /// Local LLaMA models.
    #[serde(default)]
    pub local_llama: bool,
    /// Local mistral.rs models.
    #[serde(default)]
    pub local_mistralrs: bool,
}

/// Runtime capability manifest returned by `nxuskit_capabilities()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capabilities {
    /// ABI version (e.g. "0.8").
    pub abi_version: String,
    /// SDK version (e.g. "0.7.9").
    pub sdk_version: String,
    /// Edition identifier ("oss", "pro", or "enterprise").
    pub edition: String,
    /// Per-domain availability.
    pub domains: CapabilityDomains,
}

// ---------------------------------------------------------------------------
// Capability Manifest v2 public preview
// ---------------------------------------------------------------------------

/// Capability status values in the public Capability Manifest v2 preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityStatus {
    /// Officially documented, SDK-mapped, and covered by fixture or adapter evidence.
    Supported,
    /// Provider lacks the feature or the SDK intentionally blocks it.
    Unsupported,
    /// Provider documents the feature, but nxusKit has not mapped or tested it yet.
    Recognized,
    /// Feature exists behind a provider-specific namespace rather than a shared SDK surface.
    ProviderSpecific,
    /// Known feature deliberately deferred to a future public surface.
    Future,
    /// Evidence has not been reviewed.
    Unknown,
}

/// Publication posture for the public preview projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManifestPublicationPosture {
    Split,
}

/// Stable public preview capability field names.
///
/// These are the only capability keys guaranteed in the public preview
/// projection. Internal evidence, model overrides, and provider-specific
/// details stay outside this wrapper-level shape.
pub const PUBLIC_CAPABILITY_FIELDS: &[&str] = &[
    "vision_input",
    "tool_calling",
    "thinking_blocks",
    "streaming_logprobs",
    "json_mode",
    "json_schema_strict",
    "json_schema_best_effort",
    "embeddings",
    "rerank",
];

/// Public preview provider capability record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicProviderCapability {
    /// Public provider identifier.
    pub name: String,
    /// Human-readable provider name.
    pub display_name: String,
    /// ISO 8601 date when the public capability row was last reviewed.
    pub last_reviewed_on: String,
    /// Preview provider-level status; currently `"unknown"` until promotion rules land.
    pub provider_status: String,
    /// Flat public capability status map keyed by [`PUBLIC_CAPABILITY_FIELDS`].
    pub capabilities: HashMap<String, CapabilityStatus>,
}

/// Capability Manifest v2 public preview projection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicCapabilityManifest {
    /// Public manifest schema version.
    pub schema_version: String,
    /// Publication posture describing the public/internal split.
    pub posture: ManifestPublicationPosture,
    /// Provider capability rows.
    pub providers: Vec<PublicProviderCapability>,
}
