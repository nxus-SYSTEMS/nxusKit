//! Core data types for LLM interactions

use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Image data source - either a URL or base64-encoded data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ImageData {
    /// Image referenced by URL
    Url { url: String },
    /// Base64-encoded image data
    Base64 { media_type: String, data: String },
}

/// Image source with optional detail level
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageSource {
    /// The actual image data (URL or base64)
    #[serde(flatten)]
    pub data: ImageData,

    /// Optional detail level for providers that support it
    /// Values: "low", "high", "auto" (OpenAI-specific)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// A single content part - either text or an image
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    /// Text content
    Text { text: String },
    /// Image content
    Image {
        #[serde(flatten)]
        source: ImageSource,
    },
}

/// Message content - either simple text or structured parts
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    /// Simple text message (backward compatible)
    Text(String),
    /// Structured content with multiple parts
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

impl From<Vec<ContentPart>> for MessageContent {
    fn from(parts: Vec<ContentPart>) -> Self {
        MessageContent::Parts(parts)
    }
}

/// Role of a message in a conversation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// System message (instructions/context)
    System,
    /// User message (input)
    User,
    /// Assistant message (response)
    Assistant,
}

/// A single message in a conversation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    /// Role of the message sender
    pub role: Role,
    /// Message content (text or multimodal)
    pub content: MessageContent,
}

impl Message {
    /// Create a new message with the given role and content
    pub fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: MessageContent::Text(content.into()),
        }
    }

    /// Create a system message
    pub fn system(content: impl Into<String>) -> Self {
        Self::new(Role::System, content)
    }

    /// Create a user message
    pub fn user(content: impl Into<String>) -> Self {
        Self::new(Role::User, content)
    }

    /// Create an assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        Self::new(Role::Assistant, content)
    }

    /// Add an image from URL
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

    /// Add an image from base64-encoded data
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

    /// Set detail level for the last added image (OpenAI-specific)
    /// Values: "low", "high", "auto"
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        if let MessageContent::Parts(ref mut parts) = self.content
            && let Some(ContentPart::Image { source }) = parts.last_mut()
        {
            source.detail = Some(detail.into());
        }
        self
    }

    /// Add an image from a file path
    ///
    /// This method asynchronously reads the file, validates its format,
    /// and encodes it to base64 before adding it to the message.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the image file
    ///
    /// # Returns
    ///
    /// Returns the modified Message on success, or an error if:
    /// - File doesn't exist
    /// - File is not a valid image format (JPEG, PNG, GIF, WebP)
    /// - File is too large for the provider
    /// - I/O error occurs
    ///
    /// # Example
    ///
    /// ```no_run
    /// use nxuskit_engine::types::Message;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let message = Message::user("What's in this image?")
    ///     .with_image_file("./photo.jpg").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn with_image_file(mut self, path: impl AsRef<Path>) -> crate::error::Result<Self> {
        use crate::utils::{validate_image_format, validate_image_size};

        let path = path.as_ref();

        // Check file exists
        if !path.exists() {
            return Err(crate::error::NxuskitError::ImageFileNotFound(
                path.display().to_string(),
            ));
        }

        // Read file asynchronously
        let data = tokio::fs::read(path).await?;

        // Validate format (magic bytes)
        let media_type = validate_image_format(&data)?;

        // Validate size (we'll use a conservative limit for now)
        // In a real scenario, the user would specify the provider
        validate_image_size(data.len() as u64, "claude")?;

        // Encode to base64
        let base64_data = base64::engine::general_purpose::STANDARD.encode(&data);

        // Create image part
        let image_part = ContentPart::Image {
            source: ImageSource {
                data: ImageData::Base64 {
                    media_type: media_type.to_string(),
                    data: base64_data,
                },
                detail: None,
            },
        };

        // Add to message content
        self.content = match self.content {
            MessageContent::Text(text) => {
                MessageContent::Parts(vec![ContentPart::Text { text }, image_part])
            }
            MessageContent::Parts(mut parts) => {
                parts.push(image_part);
                MessageContent::Parts(parts)
            }
        };

        Ok(self)
    }
}

/// Simple token count pair (prompt + completion)
///
/// Used as the building block for both actual and estimated token counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TokenCount {
    /// Number of tokens in the prompt/input
    pub prompt_tokens: u32,
    /// Number of tokens in the completion/output
    pub completion_tokens: u32,
}

impl TokenCount {
    /// Create a new token count
    pub fn new(prompt_tokens: u32, completion_tokens: u32) -> Self {
        Self {
            prompt_tokens,
            completion_tokens,
        }
    }

    /// Get total token count
    pub fn total(&self) -> u32 {
        self.prompt_tokens + self.completion_tokens
    }

    /// Check if both counts are zero
    pub fn is_zero(&self) -> bool {
        self.prompt_tokens == 0 && self.completion_tokens == 0
    }
}

/// Token usage information with dual actual/estimated counts
///
/// Provides both provider-returned actual counts (when available) and
/// client-side estimated counts (always available). Callers should use
/// `best_available()` for convenience or check `actual` first for precision.
///
/// # Example
///
/// ```ignore
/// let usage: TokenUsage = /* from stream */;
///
/// // Option 1: Use best available (convenience)
/// let tokens = usage.best_available();
/// println!("Used {} tokens", tokens.total());
///
/// // Option 2: Prefer actual, fall back to estimated (explicit)
/// let tokens = usage.actual.as_ref().unwrap_or(&usage.estimated);
/// println!("Used {} tokens", tokens.total());
///
/// // Option 3: Check for actual specifically
/// if let Some(actual) = &usage.actual {
///     println!("Actual: {} tokens", actual.total());
/// } else {
///     println!("Estimated: {} tokens", usage.estimated.total());
/// }
///
/// // Check completion status
/// if !usage.is_complete {
///     println!("Warning: Stream was interrupted");
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Estimated token counts (always present, guaranteed fallback)
    ///
    /// Accuracy: 95-99% with tiktoken-rs feature, 70-90% with heuristic fallback
    pub estimated: TokenCount,

    /// Actual token counts from provider (None if provider doesn't support streaming tokens)
    ///
    /// Accuracy: 100% when present
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual: Option<TokenCount>,

    /// Whether the stream completed normally
    ///
    /// - `true`: Stream finished, counts are final
    /// - `false`: Stream was interrupted, counts are partial
    pub is_complete: bool,
}

impl TokenUsage {
    /// Create with both actual and estimated counts (stream completed)
    pub fn with_actual(actual: TokenCount, estimated: TokenCount) -> Self {
        Self {
            estimated,
            actual: Some(actual),
            is_complete: true,
        }
    }

    /// Create with estimated counts only (no actual available)
    pub fn estimated_only(estimated: TokenCount) -> Self {
        Self {
            estimated,
            actual: None,
            is_complete: true,
        }
    }

    /// Create partial usage (stream interrupted)
    pub fn partial(actual: Option<TokenCount>, estimated: TokenCount) -> Self {
        Self {
            estimated,
            actual,
            is_complete: false,
        }
    }

    /// Get the best available token count (actual if present, otherwise estimated)
    pub fn best_available(&self) -> &TokenCount {
        self.actual.as_ref().unwrap_or(&self.estimated)
    }

    /// Convenience: get total tokens from best available source
    pub fn total_tokens(&self) -> u32 {
        self.best_available().total()
    }

    /// Check if actual counts are available
    pub fn has_actual(&self) -> bool {
        self.actual.is_some()
    }
}

/// Reason why a chat completion finished
///
/// Indicates why the model stopped generating tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    /// Natural completion - model finished its response
    Stop,
    /// Maximum token limit reached
    Length,
    /// Content filtering triggered (safety/policy violation)
    ContentFilter,
    /// Tool/function call triggered (for future function calling support)
    ToolCalls,
    /// Error during generation
    Error,
}

impl FinishReason {
    /// Parse finish reason from provider-specific string
    ///
    /// Maps various provider formats to the standard enum.
    pub fn from_str_flexible(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "stop" | "end_turn" | "stop_sequence" => Some(FinishReason::Stop),
            "length" | "max_tokens" | "max_length" => Some(FinishReason::Length),
            "content_filter" | "content_policy" | "safety" => Some(FinishReason::ContentFilter),
            "tool_calls" | "function_call" => Some(FinishReason::ToolCalls),
            "error" => Some(FinishReason::Error),
            _ => None,
        }
    }

    /// Get canonical string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            FinishReason::Stop => "stop",
            FinishReason::Length => "length",
            FinishReason::ContentFilter => "content_filter",
            FinishReason::ToolCalls => "tool_calls",
            FinishReason::Error => "error",
        }
    }
}

impl std::fmt::Display for FinishReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A single step in the inference process.
///
/// Represents discrete actions during inference such as rule firings,
/// tool calls, or thinking blocks. This provides a normalized view
/// across different provider types.
///
/// # Step Types
///
/// Common step types include:
/// - `"rule_firing"` - CLIPS rule activation
/// - `"tool_call"` - LLM function/tool call
/// - `"thinking"` - Model reasoning block
/// - `"fact_assertion"` - CLIPS fact creation
///
/// # Example
///
/// ```
/// use nxuskit_engine::types::InferenceStep;
///
/// // Create a rule firing step
/// let step = InferenceStep::new("rule_firing", "calculate-discount")
///     .with_details(serde_json::json!({
///         "salience": 10,
///         "module": "pricing"
///     }));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InferenceStep {
    /// Type of inference step.
    ///
    /// Standard values: "rule_firing", "tool_call", "thinking", "fact_assertion"
    pub step_type: String,

    /// Identifier for this step (rule name, tool name, etc.).
    pub identifier: String,

    /// Optional details about the step.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl InferenceStep {
    /// Create a new inference step.
    pub fn new(step_type: impl Into<String>, identifier: impl Into<String>) -> Self {
        Self {
            step_type: step_type.into(),
            identifier: identifier.into(),
            details: None,
        }
    }

    /// Add details to this step.
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }

    /// Create a rule firing step (CLIPS).
    pub fn rule_firing(rule_name: impl Into<String>, salience: i32) -> Self {
        Self::new("rule_firing", rule_name).with_details(serde_json::json!({
            "salience": salience
        }))
    }

    /// Create a tool call step (LLM).
    pub fn tool_call(tool_name: impl Into<String>, arguments: serde_json::Value) -> Self {
        Self::new("tool_call", tool_name).with_details(serde_json::json!({
            "arguments": arguments
        }))
    }

    /// Create a thinking step.
    pub fn thinking(content: impl Into<String>) -> Self {
        let content = content.into();
        let snippet = if content.len() > 100 {
            format!("{}...", &content[..100])
        } else {
            content.clone()
        };
        Self::new("thinking", "reasoning").with_details(serde_json::json!({
            "snippet": snippet
        }))
    }
}

/// Unified metadata for inference results across all providers.
///
/// This structure provides a common interface for accessing inference
/// metadata regardless of the underlying provider. Provider-specific
/// details are available in the `provider_metadata` field.
///
/// # Field Availability
///
/// Not all fields are applicable to all providers:
///
/// | Field | CLIPS | LLM Providers |
/// |-------|-------|---------------|
/// | execution_time_ms | ✓ | ✓ |
/// | is_complete | ✓ | ✓ |
/// | finish_reason | ✓ | ✓ |
/// | token_usage | ✓ (mapped) | ✓ |
/// | thinking_trace | ✓ (rules) | ✓ (if enabled) |
/// | inference_steps | ✓ (rules) | ✓ (tool calls) |
/// | provider_metadata | ✓ | ✓ |
///
/// # Example
///
/// ```
/// use nxuskit_engine::types::{InferenceMetadata, FinishReason};
///
/// // Create metadata for a completed inference
/// let metadata = InferenceMetadata::completed(FinishReason::Stop)
///     .with_execution_time(42);
///
/// assert!(metadata.is_complete);
/// assert_eq!(metadata.execution_time_ms, Some(42));
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct InferenceMetadata {
    /// Execution duration in milliseconds.
    ///
    /// For LLM providers, this is the total API call time.
    /// For CLIPS, this is the rule engine execution time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_time_ms: Option<u64>,

    /// Whether the response is complete or was truncated/interrupted.
    ///
    /// - `true`: Response finished normally
    /// - `false`: Response was truncated (max tokens, timeout, error)
    #[serde(default)]
    pub is_complete: bool,

    /// Reason why generation stopped.
    ///
    /// Common values: Stop (normal), Length (max tokens), Error, ToolCalls
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<FinishReason>,

    /// Token usage statistics.
    ///
    /// For CLIPS: prompt_tokens = input facts, completion_tokens = rules fired
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_usage: Option<TokenUsage>,

    /// Reasoning/thinking trace content.
    ///
    /// For thinking-enabled models (Qwen3, etc.), contains the reasoning text.
    /// For CLIPS, contains a summary of rule firings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_trace: Option<String>,

    /// Individual inference steps (rule firings, tool calls, thinking blocks).
    ///
    /// Each step represents a discrete action during inference:
    /// - CLIPS: Rule firings with salience
    /// - LLM: Tool/function calls
    /// - Thinking models: Reasoning blocks
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inference_steps: Option<Vec<InferenceStep>>,

    /// Provider-specific metadata as JSON.
    ///
    /// Contains any additional metadata that doesn't fit the common schema.
    /// Examples:
    /// - CLIPS: conflict_strategy, fact_counts
    /// - OpenAI: system_fingerprint
    /// - Claude: model_version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_metadata: Option<serde_json::Value>,
}

impl InferenceMetadata {
    /// Create metadata indicating successful completion.
    pub fn completed(finish_reason: FinishReason) -> Self {
        Self {
            is_complete: true,
            finish_reason: Some(finish_reason),
            ..Default::default()
        }
    }

    /// Create metadata indicating incomplete/truncated response.
    pub fn incomplete(finish_reason: FinishReason) -> Self {
        Self {
            is_complete: false,
            finish_reason: Some(finish_reason),
            ..Default::default()
        }
    }

    /// Set execution time.
    pub fn with_execution_time(mut self, ms: u64) -> Self {
        self.execution_time_ms = Some(ms);
        self
    }

    /// Set token usage.
    pub fn with_token_usage(mut self, usage: TokenUsage) -> Self {
        self.token_usage = Some(usage);
        self
    }

    /// Add inference steps.
    pub fn with_inference_steps(mut self, steps: Vec<InferenceStep>) -> Self {
        self.inference_steps = Some(steps);
        self
    }

    /// Set provider-specific metadata.
    pub fn with_provider_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.provider_metadata = Some(metadata);
        self
    }

    /// Set thinking trace.
    pub fn with_thinking_trace(mut self, trace: impl Into<String>) -> Self {
        self.thinking_trace = Some(trace.into());
        self
    }
}

/// Response format configuration for structured outputs
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseFormat {
    /// Plain text response (default)
    Text,
    /// JSON response (native support where available)
    Json,
    /// JSON response with schema validation (OpenAI-specific)
    JsonSchema {
        /// JSON schema to validate against
        schema: serde_json::Value,
    },
}

/// Provider-specific options for advanced configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum ProviderOptions {
    /// Ollama-specific options
    Ollama(OllamaOptions),
    /// CLIPS-specific options
    Clips(ClipsOptions),
    /// Local in-process LLM inference options
    #[cfg(any(feature = "provider-local-llama", feature = "provider-local-mistralrs"))]
    Local(crate::providers::local::types::LocalOptions),
}

/// CLIPS-specific configuration options
///
/// These options control the CLIPS expert system provider behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipsOptions {
    /// Conflict resolution strategy for the agenda
    ///
    /// Controls which rule fires when multiple rules are eligible:
    /// - `depth` (default): Depth-first, most recently activated rules fire first
    /// - `breadth`: Breadth-first, oldest activated rules fire first
    /// - `random`: Random selection among eligible rules
    /// - `complexity`: Most specific patterns (complex) fire first
    /// - `simplicity`: Least specific patterns (simple) fire first
    /// - `lex`: Lexicographic ordering by pattern specificity
    /// - `mea`: Means-Ends Analysis, first pattern has highest priority
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy: Option<String>,

    /// Whether to allow duplicate facts
    ///
    /// By default, CLIPS does not allow identical facts to be asserted.
    /// Set to `true` to allow duplicate facts (maps to CLIPS `set-fact-duplication TRUE`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_duplicate_facts: Option<bool>,
}

/// Ollama-specific configuration options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaOptions {
    /// Context window size
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_ctx: Option<u32>,
    /// Number of GPUs to use
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_gpu: Option<u32>,
    /// Mirostat sampling mode (0, 1, or 2)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mirostat: Option<u8>,
    /// Mirostat tau parameter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mirostat_tau: Option<f32>,
    /// Mirostat eta parameter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mirostat_eta: Option<f32>,
}

/// Thinking mode configuration for chain-of-thought reasoning models
///
/// Controls whether models like Qwen3, DeepSeek-R1, etc. produce internal
/// reasoning before their final response. This is a provider-agnostic setting
/// that gets translated to provider-specific parameters (e.g., `think` for Ollama).
///
/// # Behavior
///
/// - **Auto**: Smart default - enables thinking for thinking-capable models, omits for others
/// - **Enabled**: Force thinking mode on - model will reason before responding
/// - **Disabled**: Force thinking mode off - faster responses, no reasoning shown
/// - **Omit**: Don't send thinking parameter at all - use model's raw default behavior
///
/// # Breaking Change (v0.5.0)
///
/// Prior to v0.5.0, `Auto` would omit the thinking parameter entirely. This caused
/// issues with some models (e.g., qwen3-vl) that produce empty content when the
/// parameter is omitted. `Auto` now intelligently enables thinking for known
/// thinking-capable models. Use `Omit` if you need the old behavior.
///
/// # Token Budget Warning
///
/// When thinking is enabled, thinking tokens count toward `max_tokens`.
/// Setting a low `max_tokens` value may cause empty responses as the model
/// exhausts its token budget on thinking before generating content.
///
/// **Recommendation**: Don't set `max_tokens` when using thinking models,
/// or set it very high (e.g., 4000+).
///
/// # Example
///
/// ```
/// use nxuskit_engine::{ChatRequest, Message, ThinkingMode};
///
/// // Smart default: enables thinking for thinking models (recommended)
/// let request = ChatRequest::new("qwen3:8b")
///     .with_message(Message::user("Explain quantum computing"))
///     .with_thinking_mode(ThinkingMode::Auto);
///
/// // Disable thinking for faster responses
/// let fast_request = ChatRequest::new("qwen3:8b")
///     .with_message(Message::user("Say hello"))
///     .with_thinking_mode(ThinkingMode::Disabled);
///
/// // Omit parameter entirely (advanced: let model use raw default)
/// let raw_request = ChatRequest::new("qwen3:8b")
///     .with_message(Message::user("Hello"))
///     .with_thinking_mode(ThinkingMode::Omit);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingMode {
    /// Smart default: enable thinking for thinking-capable models, omit for others
    #[default]
    Auto,
    /// Enable thinking - model will reason before responding (think: true)
    Enabled,
    /// Disable thinking - faster responses, no internal reasoning (think: false)
    Disabled,
    /// Don't send thinking parameter - use model's raw default behavior
    Omit,
}

impl ThinkingMode {
    /// Convert to provider-specific boolean representation (simple mapping)
    ///
    /// Returns `Some(true)` for Enabled, `Some(false)` for Disabled,
    /// `None` for Auto and Omit.
    ///
    /// Note: For smart Auto behavior that considers model capabilities,
    /// use the provider-specific methods instead (e.g., `OllamaProvider::get_think_param`).
    pub fn to_bool_option(self) -> Option<bool> {
        match self {
            ThinkingMode::Auto => None,
            ThinkingMode::Enabled => Some(true),
            ThinkingMode::Disabled => Some(false),
            ThinkingMode::Omit => None,
        }
    }

    /// Check if this mode requires smart/automatic behavior
    pub fn is_auto(self) -> bool {
        matches!(self, ThinkingMode::Auto)
    }

    /// Check if this mode explicitly omits the thinking parameter
    pub fn is_omit(self) -> bool {
        matches!(self, ThinkingMode::Omit)
    }
}

/// Warning about parameter adaptation or unsupported features
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterWarning {
    /// Name of the parameter that triggered the warning
    pub parameter: String,
    /// Human-readable warning message
    pub message: String,
    /// Severity level of the warning
    pub severity: WarningSeverity,
}

/// Severity level for parameter warnings
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WarningSeverity {
    /// Informational (no impact expected)
    Info,
    /// Warning (potential impact on results)
    Warning,
    /// Error (significant issue, fallback used)
    Error,
}

/// Token probability data for logprobs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogprobsData {
    /// Token probabilities for each generated token
    pub content: Vec<TokenLogprob>,
}

/// Probability information for a single token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenLogprob {
    /// The token string
    pub token: String,
    /// Log probability of this token
    pub logprob: f32,
    /// UTF-8 bytes of the token (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<Vec<u8>>,
    /// Top alternative tokens with their probabilities
    #[serde(default)]
    pub top_logprobs: Vec<TopLogprob>,
}

/// Alternative token with probability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopLogprob {
    /// Alternative token string
    pub token: String,
    /// Log probability of this alternative
    pub logprob: f32,
    /// UTF-8 bytes of the token (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<Vec<u8>>,
}

/// Per-chunk logprob delta surfaced on streaming responses.
///
/// `content` carries the [`TokenLogprob`] entries for tokens emitted in
/// *this* chunk only. It MAY be empty (e.g. a finish-reason-only chunk) —
/// empty is not the same as absent. Absence is represented by `None` on
/// [`StreamChunk::logprobs`].
///
/// # Example
/// ```
/// use nxuskit_engine::types::{StreamLogprobsDelta, TokenLogprob};
///
/// let delta = StreamLogprobsDelta {
///     content: vec![TokenLogprob {
///         token: " Hello".into(),
///         logprob: -0.00731,
///         bytes: Some(vec![32, 72, 101, 108, 108, 111]),
///         top_logprobs: vec![],
///     }],
/// };
/// assert_eq!(delta.content[0].token, " Hello");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamLogprobsDelta {
    /// Token logprob entries for the tokens emitted in this chunk.
    pub content: Vec<TokenLogprob>,
}

/// Provider capability information
///
/// Describes what features and parameters a provider supports.
/// Used by the parameter adapter to gracefully handle unsupported parameters.
#[derive(Debug, Clone)]
pub struct ProviderCapabilities {
    /// Whether the provider supports system messages
    pub supports_system_messages: bool,
    /// Whether the provider supports streaming responses
    pub supports_streaming: bool,
    /// Whether the provider supports vision/image inputs
    pub supports_vision: bool,
    /// Maximum number of stop sequences supported (None = unlimited)
    pub max_stop_sequences: Option<usize>,
    /// Whether the provider supports presence_penalty
    pub supports_presence_penalty: bool,
    /// Whether the provider supports frequency_penalty
    pub supports_frequency_penalty: bool,
    /// Whether the provider supports seed for deterministic generation
    pub supports_seed: bool,
    /// Whether the provider supports logprobs
    pub supports_logprobs: bool,
    /// Whether the provider emits per-chunk logprob deltas on streaming responses.
    ///
    /// `true` here implies `supports_logprobs == true` — a provider cannot
    /// stream logprobs it doesn't compute. Construction sites that set this
    /// flag enforce the implication via `debug_assert!`.
    ///
    /// # Example
    /// ```
    /// use nxuskit_engine::types::ProviderCapabilities;
    /// let caps = ProviderCapabilities::default();
    /// assert!(!caps.supports_streaming_logprobs);
    /// ```
    pub supports_streaming_logprobs: bool,
    /// Whether the provider supports native JSON mode
    pub supports_json_mode: bool,
    /// Whether the provider supports JSON schema validation
    pub supports_json_schema: bool,
    /// Valid range for penalties (if supported)
    pub penalty_range: Option<(f32, f32)>,
    /// Maximum number of top_logprobs alternatives (0-20)
    pub max_logprobs: Option<u8>,
}

impl Default for ProviderCapabilities {
    /// Default capabilities (most conservative - minimal support)
    fn default() -> Self {
        Self {
            supports_system_messages: true,
            supports_streaming: false,
            supports_vision: false,
            max_stop_sequences: None,
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

/// Request for a chat completion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    /// List of messages in the conversation
    pub messages: Vec<Message>,
    /// Model to use for completion
    pub model: String,
    /// Sampling temperature (0.0 to 2.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Maximum tokens to generate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Nucleus sampling parameter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Whether to stream the response
    #[serde(skip)]
    pub stream: bool,
    /// Stop sequences - generation stops when these strings are encountered
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    /// Presence penalty (-2.0 to 2.0) - penalize tokens that have appeared
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,
    /// Frequency penalty (-2.0 to 2.0) - penalize tokens based on frequency
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,
    /// Seed for deterministic generation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    /// Whether to return log probabilities
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<bool>,
    /// Number of top alternative tokens to return (0-20)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_logprobs: Option<u8>,
    /// Reasoning effort hint for reasoning-capable models.
    ///
    /// GPT-5.4 compatibility rules require sampling controls such as
    /// `temperature`, `top_p`, and `logprobs` to be dropped when this value is
    /// present and not `"none"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    /// Response format (text, JSON, or JSON with schema)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ResponseFormat>,
    /// Provider-specific options
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_options: Option<ProviderOptions>,
    /// Thinking mode for chain-of-thought reasoning models
    ///
    /// Controls whether models like Qwen3, DeepSeek-R1 produce internal
    /// reasoning before responding. See [`ThinkingMode`] for details.
    #[serde(default, skip_serializing_if = "is_thinking_mode_auto")]
    pub thinking_mode: ThinkingMode,
    /// Tool definitions available for the model to call.
    ///
    /// Each tool describes a function the model can invoke. The model will
    /// return tool_calls in the response when it decides to use a tool.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<serde_json::Value>>,
    /// Controls how the model selects tools.
    ///
    /// String values: `"auto"`, `"none"`, `"required"`.
    /// Object value: `{"type":"function","function":{"name":"fn_name"}}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
    /// Typed structured-output configuration (Phase 4 / US2).
    ///
    /// When set, OpenAI-compatible adapters splice this through
    /// [`crate::capabilities::openai_wire::response_format`] to produce
    /// the wire field, taking precedence over the legacy
    /// [`ResponseFormat`] enum. Adapters that have not been wired through
    /// the typed surface yet ignore it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_output: Option<crate::capabilities::StructuredOutputConfig>,
    /// Typed tool-call configuration (Phase 4 / US2).
    ///
    /// When set, OpenAI-compatible adapters splice this through
    /// [`crate::capabilities::openai_wire::tools`] +
    /// [`crate::capabilities::openai_wire::tool_choice`], taking
    /// precedence over the legacy raw `tools` / `tool_choice` JSON
    /// passthrough. Adapters that have not been wired through the typed
    /// surface yet ignore it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_config: Option<crate::capabilities::ToolCallConfig>,
    /// OpenAI Responses transport options (Phase 5 / US3).
    ///
    /// These fields are Responses-only and must not silently pass through
    /// the Chat Completions request path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openai_responses: Option<crate::capabilities::OpenAIResponsesOptions>,
}

/// Helper function for serde skip_serializing_if
fn is_thinking_mode_auto(mode: &ThinkingMode) -> bool {
    matches!(mode, ThinkingMode::Auto)
}

impl ChatRequest {
    /// Create a new chat request with the specified model
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            messages: Vec::new(),
            model: model.into(),
            temperature: None,
            max_tokens: None,
            top_p: None,
            stream: false,
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            seed: None,
            logprobs: None,
            top_logprobs: None,
            reasoning_effort: None,
            response_format: None,
            provider_options: None,
            thinking_mode: ThinkingMode::Auto,
            tools: None,
            tool_choice: None,
            structured_output: None,
            tool_call_config: None,
            openai_responses: None,
        }
    }

    /// Add a message to the request
    pub fn with_message(mut self, message: Message) -> Self {
        self.messages.push(message);
        self
    }

    /// Add multiple messages to the request
    pub fn with_messages(mut self, messages: Vec<Message>) -> Self {
        self.messages.extend(messages);
        self
    }

    /// Set the temperature
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// Set the maximum tokens
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Set the top_p value
    pub fn with_top_p(mut self, top_p: f32) -> Self {
        self.top_p = Some(top_p);
        self
    }

    /// Set reasoning effort for reasoning-capable models.
    ///
    /// For GPT-5.4 family models, any value other than `"none"` causes the
    /// parameter adapter to warn-and-drop incompatible sampling/logprob fields.
    pub fn with_reasoning_effort(mut self, effort: impl Into<String>) -> Self {
        self.reasoning_effort = Some(effort.into());
        self
    }

    /// Enable streaming
    pub fn with_streaming(mut self, stream: bool) -> Self {
        self.stream = stream;
        self
    }

    /// Set the thinking mode for chain-of-thought reasoning models
    ///
    /// Controls whether models like Qwen3, DeepSeek-R1 produce internal
    /// reasoning before responding. See [`ThinkingMode`] for details.
    ///
    /// # Example
    ///
    /// ```
    /// use nxuskit_engine::{ChatRequest, Message, ThinkingMode};
    ///
    /// // Disable thinking for faster responses
    /// let request = ChatRequest::new("qwen3:8b")
    ///     .with_message(Message::user("Say hello"))
    ///     .with_thinking_mode(ThinkingMode::Disabled);
    /// ```
    pub fn with_thinking_mode(mut self, mode: ThinkingMode) -> Self {
        self.thinking_mode = mode;
        self
    }

    /// Set provider-specific options
    ///
    /// Use this to configure provider-specific behavior like CLIPS strategy
    /// or Ollama context window settings.
    ///
    /// # Example
    ///
    /// ```
    /// use nxuskit_engine::{ChatRequest, Message, ClipsOptions, ProviderOptions};
    ///
    /// // Configure CLIPS with breadth-first strategy
    /// let clips_options = ClipsOptions {
    ///     strategy: Some("breadth".to_string()),
    ///     allow_duplicate_facts: Some(true),
    /// };
    ///
    /// let request = ChatRequest::new("medical-rules.clp")
    ///     .with_message(Message::user(r#"{"facts": [...]}"#))
    ///     .with_provider_options(ProviderOptions::Clips(clips_options));
    /// ```
    pub fn with_provider_options(mut self, options: ProviderOptions) -> Self {
        self.provider_options = Some(options);
        self
    }
}

/// Response from a chat completion request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    /// Generated content
    pub content: String,
    /// Model that generated the response
    pub model: String,
    /// Provider that generated the response (e.g. "claude", "openai", "ollama")
    #[serde(default)]
    pub provider: String,
    /// Token usage information
    pub usage: TokenUsage,
    /// Reason why generation stopped
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<FinishReason>,
    /// Provider-specific metadata (legacy, use inference_metadata for new code)
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
    /// Warnings about parameter adaptations or unsupported features
    #[serde(default)]
    pub warnings: Vec<ParameterWarning>,
    /// Token probability information (if requested)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<LogprobsData>,
    /// Tool calls requested by the model.
    ///
    /// Present when `finish_reason` is `ToolCalls`. Each entry identifies a
    /// function to call with JSON-encoded arguments. The caller should execute
    /// the tools and send results back as `tool` role messages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<serde_json::Value>>,
    /// Unified inference metadata.
    ///
    /// Provides structured access to inference details across all providers.
    /// For backward compatibility, this field has a default value and can
    /// be absent in serialized data.
    #[serde(default)]
    pub inference_metadata: InferenceMetadata,
}

impl ChatResponse {
    /// Create a new chat response
    pub fn new(content: String, model: String, usage: TokenUsage) -> Self {
        Self {
            content,
            model,
            provider: String::new(),
            usage,
            finish_reason: None,
            metadata: HashMap::new(),
            warnings: Vec::new(),
            logprobs: None,
            tool_calls: None,
            inference_metadata: InferenceMetadata::default(),
        }
    }

    /// Parse the response content as CLIPS output.
    ///
    /// This provides typed access to CLIPS inference results without manual JSON parsing.
    /// Returns `None` if the content is not valid CLIPS JSON output.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use nxuskit_engine::providers::ClipsProvider;
    /// use nxuskit_engine::LLMProvider;
    ///
    /// let response = clips_provider.chat(&request).await?;
    ///
    /// // Typed access to CLIPS results
    /// if let Some(output) = response.as_clips_output() {
    ///     for conclusion in &output.conclusions {
    ///         println!("Derived: {} (fact {})",
    ///             conclusion.template,
    ///             conclusion.fact_index
    ///         );
    ///     }
    ///
    ///     if let Some(trace) = &output.trace {
    ///         for rule in &trace.rules_fired {
    ///             println!("Rule fired: {} ({} times)",
    ///                 rule.rule_name,
    ///                 rule.fire_count
    ///             );
    ///         }
    ///     }
    ///
    ///     println!("Execution time: {}ms", output.stats.execution_time_ms);
    /// }
    /// ```
    ///
    /// # When to Use
    ///
    /// Use this method when:
    /// - You know the response came from a `ClipsProvider`
    /// - You want typed access to conclusions, traces, and stats
    /// - You prefer compile-time safety over manual JSON parsing
    ///
    /// For responses from other providers, this will return `None`.
    pub fn as_clips_output(&self) -> Option<crate::providers::clips::ClipsOutput> {
        serde_json::from_str(&self.content).ok()
    }

    /// Set the finish reason
    pub fn with_finish_reason(mut self, finish_reason: FinishReason) -> Self {
        self.finish_reason = Some(finish_reason);
        self
    }

    /// Add metadata to the response
    pub fn with_metadata(mut self, key: String, value: serde_json::Value) -> Self {
        self.metadata.insert(key, value);
        self
    }
}

/// A chunk of a streaming response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    /// Incremental content from the model
    #[serde(default, alias = "content")]
    pub delta: String,
    /// Chain-of-thought reasoning content
    ///
    /// Present when the model provides internal reasoning (e.g., Qwen3 via Ollama).
    /// Clients can:
    /// - Ignore this field (backward compatible)
    /// - Display as "thinking..." indicator
    /// - Log for debugging/analysis
    /// - Show to users for transparency
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    /// Reason for completion (if finished)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<FinishReason>,
    /// Token usage (only in final chunk for some providers)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsage>,
    /// Tool call chunks (incremental tool call data during streaming).
    ///
    /// Present when the model is generating tool calls during a streamed response.
    /// Full tool call assembly from deltas is handled in Phase 9.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<serde_json::Value>>,
    /// Per-chunk logprob delta (v0.9.4+).
    ///
    /// Populated by providers whose `ProviderCapabilities::supports_streaming_logprobs`
    /// is `true` when the request enabled `logprobs`. `None` for providers that
    /// don't support streaming logprobs OR for chunks that carry no logprob
    /// data (e.g. finish-reason-only chunks). Per FR-007, non-supporting
    /// providers MUST NEVER emit phantom data here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<StreamLogprobsDelta>,
}

impl StreamChunk {
    /// Create a new stream chunk with delta content
    pub fn new(delta: String) -> Self {
        Self {
            delta,
            thinking: None,
            finish_reason: None,
            usage: None,
            tool_calls: None,
            logprobs: None,
        }
    }

    /// Attach a streaming logprob delta to this chunk.
    ///
    /// Builder-style helper for tests and provider adapters that have
    /// already constructed a `StreamChunk` and want to enrich it with
    /// logprob data.
    pub fn with_logprobs(mut self, logprobs: StreamLogprobsDelta) -> Self {
        self.logprobs = Some(logprobs);
        self
    }

    /// Create a new stream chunk with thinking content only
    ///
    /// # Example
    /// ```
    /// use nxuskit_engine::StreamChunk;
    /// let chunk = StreamChunk::thinking("analyzing the image...".to_string());
    /// assert!(chunk.delta.is_empty());
    /// assert!(chunk.thinking.is_some());
    /// ```
    pub fn thinking(thinking: String) -> Self {
        Self {
            delta: String::new(),
            thinking: Some(thinking),
            finish_reason: None,
            usage: None,
            tool_calls: None,
            logprobs: None,
        }
    }

    /// Create a chunk with both delta and thinking content
    ///
    /// # Example
    /// ```
    /// use nxuskit_engine::StreamChunk;
    /// let chunk = StreamChunk::with_thinking("Hello".to_string(), "deciding greeting".to_string());
    /// assert_eq!(chunk.delta, "Hello");
    /// assert!(chunk.thinking.is_some());
    /// ```
    pub fn with_thinking(delta: String, thinking: String) -> Self {
        Self {
            delta,
            thinking: Some(thinking),
            finish_reason: None,
            usage: None,
            tool_calls: None,
            logprobs: None,
        }
    }

    /// Create a final chunk with finish reason
    pub fn final_chunk(finish_reason: FinishReason, usage: Option<TokenUsage>) -> Self {
        Self {
            delta: String::new(),
            thinking: None,
            finish_reason: Some(finish_reason),
            usage,
            tool_calls: None,
            logprobs: None,
        }
    }

    /// Check if this is a final chunk
    pub fn is_final(&self) -> bool {
        self.finish_reason.is_some()
    }

    /// Check if this chunk contains thinking content
    ///
    /// Returns `true` if the thinking field is `Some` and non-empty.
    pub fn has_thinking(&self) -> bool {
        self.thinking.as_ref().is_some_and(|t| !t.is_empty())
    }

    /// Check if this chunk has any content (text or thinking)
    ///
    /// Returns `true` if either delta is non-empty or thinking content is present.
    pub fn has_content(&self) -> bool {
        !self.delta.is_empty() || self.has_thinking()
    }

    /// Get all content (thinking + text) combined
    ///
    /// Returns a tuple of (thinking_content, text_content) where both are
    /// Option<&str>. Use this when you want to display both the model's
    /// reasoning and its response.
    ///
    /// # Example
    /// ```
    /// use nxuskit_engine::StreamChunk;
    ///
    /// let chunk = StreamChunk::with_thinking(
    ///     "Hello!".to_string(),
    ///     "Deciding on a friendly greeting...".to_string()
    /// );
    ///
    /// let (thinking, content) = chunk.all_content();
    /// if let Some(t) = thinking {
    ///     println!("[Thinking] {}", t);
    /// }
    /// if let Some(c) = content {
    ///     println!("{}", c);
    /// }
    /// ```
    pub fn all_content(&self) -> (Option<&str>, Option<&str>) {
        let thinking = self
            .thinking
            .as_ref()
            .filter(|t| !t.is_empty())
            .map(|t| t.as_str());
        let content = if self.delta.is_empty() {
            None
        } else {
            Some(self.delta.as_str())
        };
        (thinking, content)
    }

    /// Get full text combining thinking and content with optional separator
    ///
    /// Combines thinking and text content into a single string. Useful for
    /// logging or when you want all output in one place.
    ///
    /// # Arguments
    /// * `separator` - String to place between thinking and content (e.g., "\n\n")
    ///
    /// # Example
    /// ```
    /// use nxuskit_engine::StreamChunk;
    ///
    /// let chunk = StreamChunk::with_thinking(
    ///     "The answer is 4.".to_string(),
    ///     "2 + 2 = 4".to_string()
    /// );
    ///
    /// // Get combined output
    /// let full = chunk.combined_text("\n---\n");
    /// assert!(full.contains("2 + 2 = 4"));
    /// assert!(full.contains("The answer is 4"));
    /// ```
    pub fn combined_text(&self, separator: &str) -> String {
        let (thinking, content) = self.all_content();
        match (thinking, content) {
            (Some(t), Some(c)) => format!("{}{}{}", t, separator, c),
            (Some(t), None) => t.to_string(),
            (None, Some(c)) => c.to_string(),
            (None, None) => String::new(),
        }
    }
}

/// Information about an available model
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ModelInfo {
    /// Model identifier (e.g., "llama3:70b", "claude-3-5-sonnet-20241022")
    pub name: String,

    /// Model size in bytes (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,

    /// Human-readable description (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Model context window size in tokens (if known)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u32>,

    /// Provider-specific metadata
    ///
    /// This allows providers to include additional information without
    /// requiring changes to the ModelInfo struct. All values are stored
    /// as strings for maximum flexibility.
    ///
    /// # Common Metadata Fields
    ///
    /// While each provider may include different metadata, common conventions:
    ///
    /// ## Ollama
    /// - `digest`: SHA256 digest of the model (e.g., "sha256:abc123...")
    /// - `modified_at`: Last modification timestamp (ISO 8601 format)
    /// - `family`: Model family (e.g., "llama", "mistral")
    ///
    /// ## Claude/Anthropic
    /// - `version`: Model version (e.g., "3", "3.5")
    /// - `family`: Model family (e.g., "opus", "sonnet", "haiku")
    ///
    /// ## OpenAI
    /// - `version`: Model version (e.g., "3.5", "4")
    /// - `family`: Model family (e.g., "gpt-3.5", "gpt-4")
    ///
    /// # Forward Compatibility
    ///
    /// This HashMap-based design ensures forward compatibility. New metadata
    /// fields can be added by providers without breaking existing code or
    /// requiring library updates.
    ///
    /// # Example
    ///
    /// ```
    /// use nxuskit_engine::types::ModelInfo;
    ///
    /// let mut info = ModelInfo::new("custom-model");
    /// info.metadata.insert("custom_field".to_string(), "value".to_string());
    ///
    /// // Access metadata
    /// if let Some(value) = info.metadata.get("custom_field") {
    ///     println!("Custom field: {}", value);
    /// }
    /// ```
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl ModelInfo {
    /// Create a basic ModelInfo with just a name
    ///
    /// # Example
    /// ```
    /// use nxuskit_engine::types::ModelInfo;
    /// let info = ModelInfo::new("llama3:7b");
    /// assert_eq!(info.name, "llama3:7b");
    /// ```
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            size_bytes: None,
            description: None,
            context_window: None,
            metadata: HashMap::new(),
        }
    }

    /// Create ModelInfo with size
    ///
    /// # Example
    /// ```
    /// use nxuskit_engine::types::ModelInfo;
    /// let info = ModelInfo::with_size("llama3:70b", 41_503_975_700);
    /// assert!(info.size_bytes.is_some());
    /// ```
    pub fn with_size(name: impl Into<String>, size_bytes: u64) -> Self {
        Self {
            name: name.into(),
            size_bytes: Some(size_bytes),
            description: None,
            context_window: None,
            metadata: HashMap::new(),
        }
    }

    /// Format size in human-readable format (e.g., "3.8 GB")
    ///
    /// Returns None if size_bytes is not set.
    ///
    /// Uses 1024-based units (binary) but displays as KB/MB/GB for familiarity.
    ///
    /// # Example
    /// ```
    /// use nxuskit_engine::types::ModelInfo;
    /// let info = ModelInfo::with_size("model", 4_000_000_000);
    /// assert_eq!(info.formatted_size(), Some("3.7 GB".to_string()));
    /// ```
    pub fn formatted_size(&self) -> Option<String> {
        self.size_bytes.map(|bytes| {
            const KB: u64 = 1024;
            const MB: u64 = KB * 1024;
            const GB: u64 = MB * 1024;
            const TB: u64 = GB * 1024;

            if bytes >= TB {
                format!("{:.1} TB", bytes as f64 / TB as f64)
            } else if bytes >= GB {
                format!("{:.1} GB", bytes as f64 / GB as f64)
            } else if bytes >= MB {
                format!("{:.1} MB", bytes as f64 / MB as f64)
            } else if bytes >= KB {
                format!("{:.1} KB", bytes as f64 / KB as f64)
            } else {
                format!("{} B", bytes)
            }
        })
    }

    /// Format context window in human-readable format (e.g., "200K")
    ///
    /// Returns None if context_window is not set.
    ///
    /// # Example
    /// ```
    /// use nxuskit_engine::types::ModelInfo;
    /// let mut info = ModelInfo::new("model");
    /// info.context_window = Some(200_000);
    /// assert_eq!(info.formatted_context_window(), Some("200K".to_string()));
    /// ```
    pub fn formatted_context_window(&self) -> Option<String> {
        self.context_window.map(|tokens| {
            if tokens >= 1_000_000 {
                format!("{}M", tokens / 1_000_000)
            } else if tokens >= 1_000 {
                format!("{}K", tokens / 1_000)
            } else {
                format!("{}", tokens)
            }
        })
    }

    /// Check if this model supports vision/image inputs
    ///
    /// Returns true if the modalities metadata includes "vision".
    /// Defaults to false if modalities not set or doesn't include vision.
    ///
    /// This helper provides a convenient way to check for multimodal
    /// capabilities without manually parsing the metadata.
    ///
    /// # Example
    /// ```
    /// use nxuskit_engine::types::ModelInfo;
    ///
    /// let mut info = ModelInfo::new("gpt-4o");
    /// info.metadata.insert("modalities".to_string(), "text,vision".to_string());
    /// assert!(info.supports_vision());
    ///
    /// let text_only = ModelInfo::new("gpt-3.5-turbo");
    /// assert!(!text_only.supports_vision());
    /// ```
    pub fn supports_vision(&self) -> bool {
        self.metadata
            .get("modalities")
            .map(|m| m.contains("vision"))
            .unwrap_or(false)
    }

    /// Get list of supported modalities
    ///
    /// Returns a vector of modality strings parsed from the metadata.
    /// Common values include "text" and "vision".
    ///
    /// Defaults to `vec!["text"]` if modalities not specified.
    ///
    /// # Example
    /// ```
    /// use nxuskit_engine::types::ModelInfo;
    ///
    /// let mut info = ModelInfo::new("claude-3-5-sonnet");
    /// info.metadata.insert("modalities".to_string(), "text,vision".to_string());
    /// assert_eq!(info.modalities(), vec!["text", "vision"]);
    ///
    /// let text_only = ModelInfo::new("gpt-3.5-turbo");
    /// assert_eq!(text_only.modalities(), vec!["text"]);
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

    /// Get maximum number of images supported per request
    ///
    /// Returns `Some(n)` if the provider specifies a limit,
    /// or `None` if the limit is unknown or unlimited.
    ///
    /// Different providers have different limits:
    /// - OpenAI: 10 images per request
    /// - Claude: 100 images per request
    /// - Ollama: Varies by model or unlimited
    ///
    /// # Example
    /// ```
    /// use nxuskit_engine::types::ModelInfo;
    ///
    /// let mut info = ModelInfo::new("gpt-4o");
    /// info.metadata.insert("max_images".to_string(), "10".to_string());
    /// assert_eq!(info.max_images(), Some(10));
    ///
    /// let unlimited = ModelInfo::new("ollama-model");
    /// assert_eq!(unlimited.max_images(), None);
    /// ```
    pub fn max_images(&self) -> Option<usize> {
        self.metadata
            .get("max_images")
            .and_then(|s| s.parse::<usize>().ok())
    }
}

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_role_serialization() {
        assert_eq!(serde_json::to_string(&Role::System).unwrap(), "\"system\"");
        assert_eq!(serde_json::to_string(&Role::User).unwrap(), "\"user\"");
        assert_eq!(
            serde_json::to_string(&Role::Assistant).unwrap(),
            "\"assistant\""
        );
    }

    #[test]
    fn test_message_constructors() {
        let sys_msg = Message::system("You are helpful");
        assert_eq!(sys_msg.role, Role::System);
        assert_eq!(
            sys_msg.content,
            MessageContent::Text("You are helpful".to_string())
        );

        let user_msg = Message::user("Hello!");
        assert_eq!(user_msg.role, Role::User);
        assert_eq!(user_msg.content, MessageContent::Text("Hello!".to_string()));

        let asst_msg = Message::assistant("Hi there!");
        assert_eq!(asst_msg.role, Role::Assistant);
        assert_eq!(
            asst_msg.content,
            MessageContent::Text("Hi there!".to_string())
        );
    }

    #[test]
    fn test_token_usage() {
        let usage = TokenUsage::estimated_only(TokenCount::new(100, 50));
        assert_eq!(usage.estimated.prompt_tokens, 100);
        assert_eq!(usage.estimated.completion_tokens, 50);
        assert_eq!(usage.total_tokens(), 150);
    }

    #[test]
    fn test_chat_request_builder() {
        let request = ChatRequest::new("gpt-4")
            .with_message(Message::user("Test"))
            .with_temperature(0.7)
            .with_max_tokens(100);

        assert_eq!(request.model, "gpt-4");
        assert_eq!(request.messages.len(), 1);
        assert_eq!(request.temperature, Some(0.7));
        assert_eq!(request.max_tokens, Some(100));
    }

    #[test]
    fn test_stream_chunk() {
        let chunk = StreamChunk::new("Hello".to_string());
        assert!(!chunk.is_final());
        assert_eq!(chunk.delta, "Hello");
        assert!(chunk.thinking.is_none()); // New: verify thinking is None

        let final_chunk = StreamChunk::final_chunk(FinishReason::Stop, None);
        assert!(final_chunk.is_final());
        assert_eq!(final_chunk.finish_reason, Some(FinishReason::Stop));
        assert!(final_chunk.thinking.is_none()); // New: verify thinking is None
    }

    #[test]
    fn test_stream_chunk_with_thinking() {
        // Test thinking-only chunk
        let thinking_chunk = StreamChunk::thinking("analyzing...".to_string());
        assert!(thinking_chunk.delta.is_empty());
        assert_eq!(thinking_chunk.thinking, Some("analyzing...".to_string()));
        assert!(thinking_chunk.has_thinking());
        assert!(thinking_chunk.has_content());
        assert!(!thinking_chunk.is_final());

        // Test chunk with both content and thinking
        let mixed_chunk =
            StreamChunk::with_thinking("Hello".to_string(), "deciding greeting".to_string());
        assert_eq!(mixed_chunk.delta, "Hello");
        assert_eq!(mixed_chunk.thinking, Some("deciding greeting".to_string()));
        assert!(mixed_chunk.has_thinking());
        assert!(mixed_chunk.has_content());
    }

    #[test]
    fn test_stream_chunk_has_thinking() {
        // No thinking
        let no_thinking = StreamChunk::new("Hello".to_string());
        assert!(!no_thinking.has_thinking());

        // Thinking present
        let with_thinking = StreamChunk::thinking("reasoning...".to_string());
        assert!(with_thinking.has_thinking());

        // Empty string thinking (treated as absent)
        let empty_thinking = StreamChunk {
            delta: String::new(),
            thinking: Some(String::new()),
            finish_reason: None,
            usage: None,
            tool_calls: None,
            logprobs: None,
        };
        assert!(!empty_thinking.has_thinking());
    }

    #[test]
    fn test_stream_chunk_has_content() {
        // Content only
        let content_only = StreamChunk::new("Hello".to_string());
        assert!(content_only.has_content());

        // Thinking only
        let thinking_only = StreamChunk::thinking("reasoning...".to_string());
        assert!(thinking_only.has_content());

        // Both
        let both = StreamChunk::with_thinking("Hello".to_string(), "thinking".to_string());
        assert!(both.has_content());

        // Neither (empty content, no thinking)
        let neither = StreamChunk::new(String::new());
        assert!(!neither.has_content());

        // Empty thinking (treated as absent)
        let empty_thinking = StreamChunk {
            delta: String::new(),
            thinking: Some(String::new()),
            finish_reason: None,
            usage: None,
            tool_calls: None,
            logprobs: None,
        };
        assert!(!empty_thinking.has_content());
    }

    #[test]
    fn test_stream_chunk_serde_with_thinking() {
        // Round-trip with thinking
        let chunk = StreamChunk::with_thinking("Hello".to_string(), "reasoning".to_string());
        let json = serde_json::to_string(&chunk).unwrap();
        let restored: StreamChunk = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.delta, "Hello");
        assert_eq!(restored.thinking, Some("reasoning".to_string()));

        // Deserialize with "delta" key (canonical name)
        let json_delta_key = r#"{"delta":"Hello"}"#;
        let restored: StreamChunk = serde_json::from_str(json_delta_key).unwrap();
        assert_eq!(restored.delta, "Hello");
        assert!(restored.thinking.is_none());

        // Deserialize with "content" key (backward compatibility via serde alias)
        let json_content_key = r#"{"content":"Hello"}"#;
        let restored: StreamChunk = serde_json::from_str(json_content_key).unwrap();
        assert_eq!(restored.delta, "Hello");
        assert!(restored.thinking.is_none());

        // Serialize without thinking (skip_serializing_if)
        let chunk_no_thinking = StreamChunk::new("Hello".to_string());
        let json = serde_json::to_string(&chunk_no_thinking).unwrap();
        assert!(!json.contains("thinking")); // Should be omitted
    }

    #[test]
    fn test_stream_chunk_all_content() {
        // Both thinking and content
        let both = StreamChunk::with_thinking("Response".to_string(), "Thinking...".to_string());
        let (thinking, content) = both.all_content();
        assert_eq!(thinking, Some("Thinking..."));
        assert_eq!(content, Some("Response"));

        // Only content
        let content_only = StreamChunk::new("Hello".to_string());
        let (thinking, content) = content_only.all_content();
        assert_eq!(thinking, None);
        assert_eq!(content, Some("Hello"));

        // Only thinking
        let thinking_only = StreamChunk::thinking("Analyzing...".to_string());
        let (thinking, content) = thinking_only.all_content();
        assert_eq!(thinking, Some("Analyzing..."));
        assert_eq!(content, None);

        // Neither
        let empty = StreamChunk::new(String::new());
        let (thinking, content) = empty.all_content();
        assert_eq!(thinking, None);
        assert_eq!(content, None);

        // Empty thinking string treated as absent
        let empty_thinking = StreamChunk {
            delta: "Hello".to_string(),
            thinking: Some(String::new()),
            finish_reason: None,
            usage: None,
            tool_calls: None,
            logprobs: None,
        };
        let (thinking, content) = empty_thinking.all_content();
        assert_eq!(thinking, None);
        assert_eq!(content, Some("Hello"));
    }

    #[test]
    fn test_stream_chunk_combined_text() {
        // Both thinking and content
        let both = StreamChunk::with_thinking("Response".to_string(), "Thinking...".to_string());
        assert_eq!(both.combined_text("\n---\n"), "Thinking...\n---\nResponse");

        // Only content
        let content_only = StreamChunk::new("Hello".to_string());
        assert_eq!(content_only.combined_text("\n"), "Hello");

        // Only thinking
        let thinking_only = StreamChunk::thinking("Analyzing...".to_string());
        assert_eq!(thinking_only.combined_text("\n"), "Analyzing...");

        // Neither
        let empty = StreamChunk::new(String::new());
        assert_eq!(empty.combined_text("\n"), "");

        // Different separator
        let chunk = StreamChunk::with_thinking("Answer".to_string(), "Reasoning".to_string());
        assert_eq!(chunk.combined_text(" | "), "Reasoning | Answer");
    }

    // ModelInfo tests (T011-T014)
    #[test]
    fn test_model_info_new() {
        let info = ModelInfo::new("test-model");
        assert_eq!(info.name, "test-model");
        assert_eq!(info.size_bytes, None);
        assert_eq!(info.description, None);
        assert_eq!(info.context_window, None);
        assert!(info.metadata.is_empty());
    }

    #[test]
    fn test_model_info_with_size() {
        let info = ModelInfo::with_size("llama3:70b", 41_503_975_700);
        assert_eq!(info.name, "llama3:70b");
        assert_eq!(info.size_bytes, Some(41_503_975_700));
    }

    #[test]
    fn test_formatted_size_gb() {
        let info = ModelInfo::with_size("model", 4_000_000_000);
        assert_eq!(info.formatted_size(), Some("3.7 GB".to_string()));
    }

    #[test]
    fn test_formatted_size_mb() {
        let info = ModelInfo::with_size("model", 50_000_000);
        assert_eq!(info.formatted_size(), Some("47.7 MB".to_string()));
    }

    #[test]
    fn test_formatted_size_kb() {
        let info = ModelInfo::with_size("model", 5_120);
        assert_eq!(info.formatted_size(), Some("5.0 KB".to_string()));
    }

    #[test]
    fn test_formatted_size_bytes() {
        let info = ModelInfo::with_size("model", 512);
        assert_eq!(info.formatted_size(), Some("512 B".to_string()));
    }

    #[test]
    fn test_formatted_size_none() {
        let info = ModelInfo::new("model");
        assert_eq!(info.formatted_size(), None);
    }

    #[test]
    fn test_formatted_context_window_m() {
        let mut info = ModelInfo::new("model");
        info.context_window = Some(2_000_000);
        assert_eq!(info.formatted_context_window(), Some("2M".to_string()));
    }

    #[test]
    fn test_formatted_context_window_k() {
        let mut info = ModelInfo::new("model");
        info.context_window = Some(200_000);
        assert_eq!(info.formatted_context_window(), Some("200K".to_string()));
    }

    #[test]
    fn test_formatted_context_window_plain() {
        let mut info = ModelInfo::new("model");
        info.context_window = Some(512);
        assert_eq!(info.formatted_context_window(), Some("512".to_string()));
    }

    #[test]
    fn test_formatted_context_window_none() {
        let info = ModelInfo::new("model");
        assert_eq!(info.formatted_context_window(), None);
    }

    #[test]
    fn test_model_info_serialization() {
        let info = ModelInfo::with_size("llama3:7b", 4_000_000_000);
        let json = serde_json::to_string(&info).unwrap();
        let deserialized: ModelInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(info, deserialized);
    }

    #[test]
    fn test_model_info_metadata() {
        let mut info = ModelInfo::new("model");
        info.metadata
            .insert("digest".to_string(), "sha256:abc".to_string());
        info.metadata
            .insert("family".to_string(), "llama".to_string());

        assert_eq!(info.metadata.get("digest"), Some(&"sha256:abc".to_string()));
        assert_eq!(info.metadata.get("family"), Some(&"llama".to_string()));
    }

    // Vision tests
    #[test]
    fn test_message_with_image_url() {
        let msg =
            Message::user("What's in this image?").with_image_url("https://example.com/photo.jpg");

        match msg.content {
            MessageContent::Parts(parts) => {
                assert_eq!(parts.len(), 2);
                match &parts[0] {
                    ContentPart::Text { text } => assert_eq!(text, "What's in this image?"),
                    _ => panic!("Expected text part"),
                }
                match &parts[1] {
                    ContentPart::Image { source } => match &source.data {
                        ImageData::Url { url } => assert_eq!(url, "https://example.com/photo.jpg"),
                        _ => panic!("Expected URL image data"),
                    },
                    _ => panic!("Expected image part"),
                }
            }
            _ => panic!("Expected Parts variant"),
        }
    }

    #[test]
    fn test_message_with_multiple_images() {
        let msg = Message::user("Compare these images")
            .with_image_url("https://example.com/img1.jpg")
            .with_image_url("https://example.com/img2.jpg");

        match msg.content {
            MessageContent::Parts(parts) => {
                assert_eq!(parts.len(), 3); // 1 text + 2 images
                match &parts[0] {
                    ContentPart::Text { .. } => {}
                    _ => panic!("Expected text part first"),
                }
                match &parts[1] {
                    ContentPart::Image { source } => match &source.data {
                        ImageData::Url { url } => assert_eq!(url, "https://example.com/img1.jpg"),
                        _ => panic!("Expected first image URL"),
                    },
                    _ => panic!("Expected image part"),
                }
                match &parts[2] {
                    ContentPart::Image { source } => match &source.data {
                        ImageData::Url { url } => assert_eq!(url, "https://example.com/img2.jpg"),
                        _ => panic!("Expected second image URL"),
                    },
                    _ => panic!("Expected image part"),
                }
            }
            _ => panic!("Expected Parts variant"),
        }
    }

    #[test]
    fn test_message_with_base64_image() {
        let msg = Message::user("Describe this").with_image_base64("base64data", "image/jpeg");

        match msg.content {
            MessageContent::Parts(parts) => {
                assert_eq!(parts.len(), 2);
                match &parts[1] {
                    ContentPart::Image { source } => match &source.data {
                        ImageData::Base64 { media_type, data } => {
                            assert_eq!(media_type, "image/jpeg");
                            assert_eq!(data, "base64data");
                        }
                        _ => panic!("Expected Base64 image data"),
                    },
                    _ => panic!("Expected image part"),
                }
            }
            _ => panic!("Expected Parts variant"),
        }
    }

    #[test]
    fn test_message_with_detail() {
        let msg = Message::user("Analyze this diagram")
            .with_image_url("https://example.com/diagram.png")
            .with_detail("high");

        match msg.content {
            MessageContent::Parts(parts) => match &parts[1] {
                ContentPart::Image { source } => {
                    assert_eq!(source.detail, Some("high".to_string()));
                }
                _ => panic!("Expected image part"),
            },
            _ => panic!("Expected Parts variant"),
        }
    }

    #[test]
    fn test_message_content_serialization() {
        // Test Text variant
        let text_content = MessageContent::Text("Hello".to_string());
        let json = serde_json::to_string(&text_content).unwrap();
        assert_eq!(json, "\"Hello\"");

        // Test Parts variant
        let parts_content = MessageContent::Parts(vec![ContentPart::Text {
            text: "Test".to_string(),
        }]);
        let json = serde_json::to_string(&parts_content).unwrap();
        assert!(json.contains("\"type\":\"text\""));
    }

    // Tests for capability metadata helper methods (T010-T013)

    #[test]
    fn test_supports_vision_with_modalities() {
        let mut info = ModelInfo::new("gpt-4o");
        info.metadata
            .insert("modalities".to_string(), "text,vision".to_string());
        assert!(info.supports_vision());
    }

    #[test]
    fn test_supports_vision_text_only() {
        let mut info = ModelInfo::new("gpt-3.5-turbo");
        info.metadata
            .insert("modalities".to_string(), "text".to_string());
        assert!(!info.supports_vision());
    }

    #[test]
    fn test_supports_vision_missing_metadata() {
        let info = ModelInfo::new("unknown-model");
        assert!(!info.supports_vision());
    }

    #[test]
    fn test_supports_vision_empty_modalities() {
        let mut info = ModelInfo::new("model");
        info.metadata
            .insert("modalities".to_string(), "".to_string());
        assert!(!info.supports_vision());
    }

    #[test]
    fn test_modalities_multimodal() {
        let mut info = ModelInfo::new("claude-3-5-sonnet");
        info.metadata
            .insert("modalities".to_string(), "text,vision".to_string());
        assert_eq!(info.modalities(), vec!["text", "vision"]);
    }

    #[test]
    fn test_modalities_text_only() {
        let mut info = ModelInfo::new("gpt-3.5-turbo");
        info.metadata
            .insert("modalities".to_string(), "text".to_string());
        assert_eq!(info.modalities(), vec!["text"]);
    }

    #[test]
    fn test_modalities_default() {
        let info = ModelInfo::new("unknown-model");
        assert_eq!(info.modalities(), vec!["text"]);
    }

    #[test]
    fn test_modalities_with_whitespace() {
        let mut info = ModelInfo::new("model");
        info.metadata
            .insert("modalities".to_string(), " text , vision ".to_string());
        assert_eq!(info.modalities(), vec!["text", "vision"]);
    }

    #[test]
    fn test_modalities_future_extension() {
        let mut info = ModelInfo::new("future-model");
        info.metadata
            .insert("modalities".to_string(), "text,vision,audio".to_string());
        assert_eq!(info.modalities(), vec!["text", "vision", "audio"]);
    }

    #[test]
    fn test_max_images_openai() {
        let mut info = ModelInfo::new("gpt-4o");
        info.metadata
            .insert("max_images".to_string(), "10".to_string());
        assert_eq!(info.max_images(), Some(10));
    }

    #[test]
    fn test_max_images_claude() {
        let mut info = ModelInfo::new("claude-3-5-sonnet");
        info.metadata
            .insert("max_images".to_string(), "100".to_string());
        assert_eq!(info.max_images(), Some(100));
    }

    #[test]
    fn test_max_images_none() {
        let info = ModelInfo::new("ollama-model");
        assert_eq!(info.max_images(), None);
    }

    #[test]
    fn test_max_images_invalid() {
        let mut info = ModelInfo::new("model");
        info.metadata
            .insert("max_images".to_string(), "invalid".to_string());
        assert_eq!(info.max_images(), None);
    }

    #[test]
    fn test_max_images_zero() {
        let mut info = ModelInfo::new("model");
        info.metadata
            .insert("max_images".to_string(), "0".to_string());
        assert_eq!(info.max_images(), Some(0));
    }

    #[test]
    fn test_capability_metadata_integration() {
        // Test full capability metadata workflow
        let mut info = ModelInfo::new("gpt-4o");
        info.context_window = Some(128_000);
        info.description = Some("Multimodal GPT-4 model".to_string());
        info.metadata
            .insert("modalities".to_string(), "text,vision".to_string());
        info.metadata
            .insert("max_images".to_string(), "10".to_string());

        // Verify all helper methods work together
        assert!(info.supports_vision());
        assert_eq!(info.modalities(), vec!["text", "vision"]);
        assert_eq!(info.max_images(), Some(10));
        assert_eq!(info.formatted_context_window(), Some("128K".to_string()));
    }

    #[test]
    fn test_backward_compatibility() {
        // Verify existing code that only uses name and size still works
        let info = ModelInfo::with_size("llama3:70b", 41_503_975_700);
        assert_eq!(info.name, "llama3:70b");
        assert_eq!(info.size_bytes, Some(41_503_975_700));
        assert_eq!(info.formatted_size(), Some("38.7 GB".to_string()));

        // New methods return safe defaults
        assert!(!info.supports_vision());
        assert_eq!(info.modalities(), vec!["text"]);
        assert_eq!(info.max_images(), None);
    }

    // FinishReason tests
    #[test]
    fn test_finish_reason_serialization() {
        assert_eq!(
            serde_json::to_string(&FinishReason::Stop).unwrap(),
            "\"stop\""
        );
        assert_eq!(
            serde_json::to_string(&FinishReason::Length).unwrap(),
            "\"length\""
        );
        assert_eq!(
            serde_json::to_string(&FinishReason::ContentFilter).unwrap(),
            "\"content_filter\""
        );
        assert_eq!(
            serde_json::to_string(&FinishReason::ToolCalls).unwrap(),
            "\"tool_calls\""
        );
    }

    #[test]
    fn test_finish_reason_from_str_flexible() {
        assert_eq!(
            FinishReason::from_str_flexible("stop"),
            Some(FinishReason::Stop)
        );
        assert_eq!(
            FinishReason::from_str_flexible("end_turn"),
            Some(FinishReason::Stop)
        );
        assert_eq!(
            FinishReason::from_str_flexible("STOP_SEQUENCE"),
            Some(FinishReason::Stop)
        );
        assert_eq!(
            FinishReason::from_str_flexible("length"),
            Some(FinishReason::Length)
        );
        assert_eq!(
            FinishReason::from_str_flexible("MAX_TOKENS"),
            Some(FinishReason::Length)
        );
        assert_eq!(
            FinishReason::from_str_flexible("content_filter"),
            Some(FinishReason::ContentFilter)
        );
        assert_eq!(
            FinishReason::from_str_flexible("tool_calls"),
            Some(FinishReason::ToolCalls)
        );
        assert_eq!(FinishReason::from_str_flexible("unknown"), None);
    }

    #[test]
    fn test_finish_reason_display() {
        assert_eq!(FinishReason::Stop.to_string(), "stop");
        assert_eq!(FinishReason::Length.to_string(), "length");
        assert_eq!(FinishReason::ContentFilter.to_string(), "content_filter");
    }

    #[test]
    fn test_chat_response_with_finish_reason() {
        let response = ChatResponse::new(
            "Hello!".to_string(),
            "gpt-4".to_string(),
            TokenUsage::estimated_only(TokenCount::new(10, 5)),
        )
        .with_finish_reason(FinishReason::Stop);

        assert_eq!(response.finish_reason, Some(FinishReason::Stop));
        assert_eq!(response.content, "Hello!");
    }

    #[test]
    fn test_stream_chunk_final() {
        let chunk = StreamChunk::final_chunk(FinishReason::Stop, None);
        assert!(chunk.is_final());
        assert_eq!(chunk.finish_reason, Some(FinishReason::Stop));
        assert_eq!(chunk.delta, "");
    }

    // ThinkingMode tests
    #[test]
    fn test_thinking_mode_default() {
        let mode = ThinkingMode::default();
        assert_eq!(mode, ThinkingMode::Auto);
    }

    #[test]
    fn test_thinking_mode_to_bool_option() {
        assert_eq!(ThinkingMode::Auto.to_bool_option(), None);
        assert_eq!(ThinkingMode::Enabled.to_bool_option(), Some(true));
        assert_eq!(ThinkingMode::Disabled.to_bool_option(), Some(false));
        assert_eq!(ThinkingMode::Omit.to_bool_option(), None);
    }

    #[test]
    fn test_thinking_mode_helper_methods() {
        assert!(ThinkingMode::Auto.is_auto());
        assert!(!ThinkingMode::Enabled.is_auto());
        assert!(!ThinkingMode::Disabled.is_auto());
        assert!(!ThinkingMode::Omit.is_auto());

        assert!(!ThinkingMode::Auto.is_omit());
        assert!(!ThinkingMode::Enabled.is_omit());
        assert!(!ThinkingMode::Disabled.is_omit());
        assert!(ThinkingMode::Omit.is_omit());
    }

    #[test]
    fn test_thinking_mode_serialization() {
        assert_eq!(
            serde_json::to_string(&ThinkingMode::Auto).unwrap(),
            "\"auto\""
        );
        assert_eq!(
            serde_json::to_string(&ThinkingMode::Enabled).unwrap(),
            "\"enabled\""
        );
        assert_eq!(
            serde_json::to_string(&ThinkingMode::Disabled).unwrap(),
            "\"disabled\""
        );
        assert_eq!(
            serde_json::to_string(&ThinkingMode::Omit).unwrap(),
            "\"omit\""
        );
    }

    #[test]
    fn test_thinking_mode_deserialization() {
        assert_eq!(
            serde_json::from_str::<ThinkingMode>("\"auto\"").unwrap(),
            ThinkingMode::Auto
        );
        assert_eq!(
            serde_json::from_str::<ThinkingMode>("\"enabled\"").unwrap(),
            ThinkingMode::Enabled
        );
        assert_eq!(
            serde_json::from_str::<ThinkingMode>("\"disabled\"").unwrap(),
            ThinkingMode::Disabled
        );
        assert_eq!(
            serde_json::from_str::<ThinkingMode>("\"omit\"").unwrap(),
            ThinkingMode::Omit
        );
    }

    #[test]
    fn test_chat_request_with_thinking_mode() {
        let request = ChatRequest::new("qwen3:8b")
            .with_message(Message::user("Hello"))
            .with_thinking_mode(ThinkingMode::Disabled);

        assert_eq!(request.thinking_mode, ThinkingMode::Disabled);
    }

    #[test]
    fn test_chat_request_default_thinking_mode() {
        let request = ChatRequest::new("gpt-4");
        assert_eq!(request.thinking_mode, ThinkingMode::Auto);
    }

    #[test]
    fn test_chat_request_thinking_mode_serialization_skip() {
        // Auto mode should be skipped during serialization
        let request = ChatRequest::new("gpt-4");
        let json = serde_json::to_string(&request).unwrap();
        assert!(!json.contains("thinking_mode"));

        // Non-auto modes should be included
        let request_with_thinking =
            ChatRequest::new("qwen3:8b").with_thinking_mode(ThinkingMode::Disabled);
        let json = serde_json::to_string(&request_with_thinking).unwrap();
        assert!(json.contains("thinking_mode"));
    }
}

#[cfg(test)]
mod stream_logprobs_delta_tests {
    use super::*;

    fn sample_token() -> TokenLogprob {
        TokenLogprob {
            token: " Hello".to_string(),
            logprob: -0.00731,
            bytes: Some(vec![32, 72, 101, 108, 108, 111]),
            top_logprobs: vec![TopLogprob {
                token: " Hi".to_string(),
                logprob: -2.1,
                bytes: Some(vec![32, 72, 105]),
            }],
        }
    }

    #[test]
    fn streamchunk_default_omits_logprobs_field_in_json() {
        let chunk = StreamChunk::new("hello".to_string());
        let v: serde_json::Value = serde_json::to_value(&chunk).unwrap();
        assert!(
            v.get("logprobs").is_none(),
            "absent logprobs must be omitted from JSON, got: {v}"
        );
    }

    #[test]
    fn streamchunk_with_logprobs_roundtrips_through_serde() {
        let delta = StreamLogprobsDelta {
            content: vec![sample_token()],
        };
        let chunk = StreamChunk::new(" Hello".to_string()).with_logprobs(delta);
        let json = serde_json::to_string(&chunk).expect("serialize");
        let restored: StreamChunk = serde_json::from_str(&json).expect("deserialize");
        let lp = restored.logprobs.expect("logprobs preserved");
        assert_eq!(lp.content.len(), 1);
        assert_eq!(lp.content[0].token, " Hello");
        assert!((lp.content[0].logprob - -0.00731).abs() < 1e-5);
        assert_eq!(lp.content[0].top_logprobs.len(), 1);
        assert_eq!(lp.content[0].top_logprobs[0].token, " Hi");
    }

    #[test]
    fn streamchunk_logprobs_field_is_optional_on_deserialize() {
        let json = r#"{"delta":"hi"}"#;
        let chunk: StreamChunk = serde_json::from_str(json).expect("deserialize");
        assert!(chunk.logprobs.is_none());
    }

    #[test]
    fn provider_capabilities_default_streaming_logprobs_is_false() {
        let caps = ProviderCapabilities::default();
        assert!(!caps.supports_streaming_logprobs);
    }

    #[test]
    fn provider_capabilities_streaming_implies_unary_logprobs() {
        let caps = ProviderCapabilities {
            supports_logprobs: true,
            supports_streaming_logprobs: true,
            ..Default::default()
        };
        assert!(!caps.supports_streaming_logprobs || caps.supports_logprobs);
    }
}
