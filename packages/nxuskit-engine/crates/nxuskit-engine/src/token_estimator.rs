//! Client-side token estimation for streaming responses
//!
//! This module provides token counting using either:
//! - tiktoken-rs BPE tokenizer (95-99% accuracy) when `stream-token-estimation` feature enabled
//! - Character-based heuristic (70-90% accuracy) as fallback
//!
//! The primary use case is streaming responses where token counts need to be estimated
//! in real-time as chunks arrive, without making additional API calls.

use crate::types::{Message, TokenCount, TokenUsage};

/// Indicates which estimation method is being used
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstimationMethod {
    /// Using tiktoken-rs BPE tokenizer (95-99% accuracy)
    Tiktoken,
    /// Using character-based heuristic (~1 token per 3.5 characters, 70-90% accuracy)
    Heuristic,
}

/// Client-side token estimation for streaming responses
///
/// Provides token counting using either tiktoken-rs BPE tokenizer (when available)
/// or character-based heuristic (always available as fallback).
#[derive(Clone)]
pub struct TokenEstimator {
    method: EstimationMethod,
    #[cfg(feature = "stream-token-estimation")]
    bpe: Option<tiktoken_rs::CoreBPE>,
    model: String,
}

impl std::fmt::Debug for TokenEstimator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenEstimator")
            .field("method", &self.method)
            .field("model", &self.model)
            .finish_non_exhaustive()
    }
}

impl TokenEstimator {
    /// Create an estimator optimized for a specific model
    ///
    /// When `stream-token-estimation` feature is enabled:
    /// - Attempts to load tiktoken-rs BPE for model
    /// - Falls back to heuristic if model not supported
    ///
    /// Without the feature:
    /// - Always uses heuristic (chars / 3.5)
    ///
    /// # Arguments
    /// * `model` - Model name (e.g., "gpt-4o", "claude-3-opus")
    pub fn for_model(model: &str) -> Self {
        #[cfg(feature = "stream-token-estimation")]
        {
            match tiktoken_rs::get_bpe_from_model(model) {
                Ok(bpe) => {
                    return Self {
                        method: EstimationMethod::Tiktoken,
                        bpe: Some(bpe),
                        model: model.to_string(),
                    };
                }
                Err(_) => {
                    // Fall through to heuristic
                }
            }
        }

        #[cfg(not(feature = "stream-token-estimation"))]
        {
            let _ = model; // Suppress unused warning
        }

        Self {
            method: EstimationMethod::Heuristic,
            #[cfg(feature = "stream-token-estimation")]
            bpe: None,
            model: model.to_string(),
        }
    }

    /// Count tokens in a text string
    ///
    /// # Arguments
    /// * `text` - The text to tokenize
    ///
    /// # Returns
    /// Estimated token count as u32
    ///
    /// # Example
    /// ```ignore
    /// let estimator = TokenEstimator::for_model("gpt-4o");
    /// let count = estimator.count("Hello, world!");
    /// assert!(count > 0);
    /// ```
    pub fn count(&self, text: &str) -> u32 {
        match self.method {
            #[cfg(feature = "stream-token-estimation")]
            EstimationMethod::Tiktoken => {
                if let Some(ref bpe) = self.bpe {
                    bpe.encode_with_special_tokens(text).len() as u32
                } else {
                    self.heuristic_count(text)
                }
            }
            #[cfg(not(feature = "stream-token-estimation"))]
            EstimationMethod::Tiktoken => {
                // Should not happen since we don't set Tiktoken without the feature,
                // but fallback just in case
                self.heuristic_count(text)
            }
            EstimationMethod::Heuristic => self.heuristic_count(text),
        }
    }

    /// Count tokens in chat messages
    ///
    /// Includes overhead for message formatting (role markers, etc.)
    ///
    /// # Arguments
    /// * `messages` - Slice of chat messages
    ///
    /// # Returns
    /// Total estimated prompt tokens
    pub fn count_messages(&self, messages: &[Message]) -> u32 {
        let mut total = 0u32;

        // Add overhead: ~4 tokens per message for formatting
        total += (messages.len() as u32) * 4;

        // Count tokens in each message
        for msg in messages {
            // Role adds ~1 token
            total += 1;

            // Count content tokens
            match &msg.content {
                crate::types::MessageContent::Text(text) => {
                    total += self.count(text);
                }
                crate::types::MessageContent::Parts(parts) => {
                    for part in parts {
                        match part {
                            crate::types::ContentPart::Text { text } => {
                                total += self.count(text);
                            }
                            crate::types::ContentPart::Image { .. } => {
                                // Rough estimate: images ~100-300 tokens depending on detail
                                total += 150;
                            }
                        }
                    }
                }
            }
        }

        total
    }

    /// Get the estimation method being used
    pub fn method(&self) -> EstimationMethod {
        self.method
    }

    /// Heuristic token counting: ~1 token per 3.5 characters
    fn heuristic_count(&self, text: &str) -> u32 {
        ((text.len() as f32) / 3.5).ceil() as u32
    }
}

/// Accumulates streaming content for progressive token estimation
///
/// Used internally by providers to track token usage during streaming.
/// Tracks both response content and thinking content separately for accurate
/// token counting when models provide chain-of-thought reasoning.
pub(crate) struct StreamingTokenAccumulator {
    estimator: TokenEstimator,
    prompt_tokens: u32,
    accumulated_response: String,
    /// Accumulated thinking/reasoning content from thinking-enabled models
    accumulated_thinking: String,
    actual_usage: Option<TokenCount>,
    is_complete: bool,
}

impl StreamingTokenAccumulator {
    /// Create accumulator with known prompt token count
    pub fn new(estimator: TokenEstimator, prompt_tokens: u32) -> Self {
        Self {
            estimator,
            prompt_tokens,
            accumulated_response: String::new(),
            accumulated_thinking: String::new(),
            actual_usage: None,
            is_complete: true,
        }
    }

    /// Add content from a streaming chunk
    ///
    /// # Arguments
    /// * `delta` - The content from a StreamChunk
    pub fn add_chunk(&mut self, delta: &str) {
        self.accumulated_response.push_str(delta);
    }

    /// Add thinking content from a streaming chunk
    ///
    /// Thinking tokens are counted as part of completion tokens since they
    /// represent model computation and affect billing/resource usage.
    ///
    /// # Arguments
    /// * `thinking` - The thinking content from StreamChunk
    pub fn add_thinking_chunk(&mut self, thinking: &str) {
        self.accumulated_thinking.push_str(thinking);
    }

    /// Set actual usage from provider (when received)
    pub fn set_actual(&mut self, actual: TokenCount) {
        self.actual_usage = Some(actual);
    }

    /// Get current running total as TokenUsage
    ///
    /// Completion tokens include both response content and thinking content.
    pub fn running_total(&self) -> TokenUsage {
        let response_tokens = self.estimator.count(&self.accumulated_response);
        let thinking_tokens = self.estimator.count(&self.accumulated_thinking);
        let completion_tokens = response_tokens + thinking_tokens;
        let estimated = TokenCount::new(self.prompt_tokens, completion_tokens);

        if let Some(actual) = self.actual_usage {
            TokenUsage::with_actual(actual, estimated)
        } else {
            TokenUsage::estimated_only(estimated)
        }
    }

    /// Mark stream as interrupted/partial
    pub fn mark_interrupted(&mut self) {
        self.is_complete = false;
    }

    /// Finalize and get complete TokenUsage
    ///
    /// Completion tokens include both response content and thinking content.
    pub fn finalize(self) -> TokenUsage {
        let response_tokens = self.estimator.count(&self.accumulated_response);
        let thinking_tokens = self.estimator.count(&self.accumulated_thinking);
        let completion_tokens = response_tokens + thinking_tokens;
        let estimated = TokenCount::new(self.prompt_tokens, completion_tokens);

        if let Some(actual) = self.actual_usage {
            if self.is_complete {
                TokenUsage::with_actual(actual, estimated)
            } else {
                TokenUsage::partial(Some(actual), estimated)
            }
        } else if self.is_complete {
            TokenUsage::estimated_only(estimated)
        } else {
            TokenUsage::partial(None, estimated)
        }
    }

    /// Get accumulated text (for debugging)
    #[allow(dead_code)]
    pub fn accumulated_text(&self) -> &str {
        &self.accumulated_response
    }

    /// Get accumulated thinking text (for debugging)
    #[allow(dead_code)]
    pub fn accumulated_thinking(&self) -> &str {
        &self.accumulated_thinking
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heuristic_count_empty() {
        let estimator = TokenEstimator::for_model("unknown-model");
        assert_eq!(estimator.count(""), 0);
    }

    #[test]
    fn test_heuristic_count_simple() {
        let estimator = TokenEstimator::for_model("unknown-model");
        // "Hello, world!" is ~13 characters / 3.5 ≈ 4 tokens
        let count = estimator.count("Hello, world!");
        assert!(count > 0 && count <= 6, "Got {} tokens", count);
    }

    #[test]
    fn test_accumulator_running_total() {
        let estimator = TokenEstimator::for_model("unknown-model");
        let mut acc = StreamingTokenAccumulator::new(estimator, 10);

        acc.add_chunk("Hello ");
        let usage1 = acc.running_total();
        assert_eq!(usage1.estimated.prompt_tokens, 10);
        assert!(usage1.best_available().completion_tokens > 0);

        acc.add_chunk("world!");
        let usage2 = acc.running_total();
        assert!(
            usage2.best_available().completion_tokens >= usage1.best_available().completion_tokens
        );
    }

    #[test]
    fn test_accumulator_partial_marking() {
        let estimator = TokenEstimator::for_model("unknown-model");
        let mut acc = StreamingTokenAccumulator::new(estimator, 10);
        acc.add_chunk("Hello");
        acc.mark_interrupted();

        let usage = acc.finalize();
        assert!(!usage.is_complete);
    }

    #[test]
    fn test_accumulator_with_actual() {
        let estimator = TokenEstimator::for_model("unknown-model");
        let mut acc = StreamingTokenAccumulator::new(estimator, 10);
        acc.add_chunk("Hello world");

        let actual = TokenCount::new(10, 2);
        acc.set_actual(actual);

        let usage = acc.finalize();
        assert!(usage.has_actual());
        assert_eq!(usage.actual.unwrap(), actual);
    }

    #[test]
    fn test_token_count_total() {
        let tc = TokenCount::new(5, 3);
        assert_eq!(tc.total(), 8);
    }

    #[test]
    fn test_token_count_is_zero() {
        assert!(TokenCount::new(0, 0).is_zero());
        assert!(!TokenCount::new(1, 0).is_zero());
        assert!(!TokenCount::new(0, 1).is_zero());
    }

    // Tests for thinking token accumulation (T034-T036)

    #[test]
    fn test_accumulator_add_thinking_chunk() {
        let estimator = TokenEstimator::for_model("unknown-model");
        let mut acc = StreamingTokenAccumulator::new(estimator, 10);

        // Add thinking content
        acc.add_thinking_chunk("analyzing ");
        acc.add_thinking_chunk("the problem");

        assert_eq!(acc.accumulated_thinking(), "analyzing the problem");
    }

    #[test]
    fn test_accumulator_thinking_tokens_in_running_total() {
        let estimator = TokenEstimator::for_model("unknown-model");
        let mut acc = StreamingTokenAccumulator::new(estimator, 10);

        // Add only thinking (no content)
        acc.add_thinking_chunk("reasoning step one");

        let usage = acc.running_total();
        // Completion tokens should include thinking tokens
        assert!(usage.estimated.completion_tokens > 0);
    }

    #[test]
    fn test_accumulator_combined_content_and_thinking() {
        let estimator = TokenEstimator::for_model("unknown-model");
        let mut acc = StreamingTokenAccumulator::new(estimator, 10);

        // Add thinking
        acc.add_thinking_chunk("thinking about it");

        // Add content
        acc.add_chunk("Hello world");

        let usage = acc.finalize();
        // Completion tokens should include both thinking and response tokens
        let thinking_only_estimate =
            TokenEstimator::for_model("unknown-model").count("thinking about it");
        let response_only_estimate =
            TokenEstimator::for_model("unknown-model").count("Hello world");

        assert_eq!(
            usage.estimated.completion_tokens,
            thinking_only_estimate + response_only_estimate
        );
    }

    #[test]
    fn test_accumulator_finalize_includes_thinking() {
        let estimator = TokenEstimator::for_model("unknown-model");
        let mut acc = StreamingTokenAccumulator::new(estimator, 10);

        acc.add_thinking_chunk("step one step two");
        acc.add_chunk("final answer");

        let usage = acc.finalize();

        // Both thinking and response should be counted
        assert!(usage.estimated.completion_tokens > 0);
        assert_eq!(usage.estimated.prompt_tokens, 10);
    }
}
