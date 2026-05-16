//! Mock provider for testing

use async_stream::stream;
use async_trait::async_trait;
use futures::Stream;

use crate::{
    ChatRequest, ChatResponse, LLMProvider, ModelInfo, ModelLister, StreamChunk, TokenCount,
    TokenUsage,
    error::Result,
    parameter_adapter::ParameterAdapter,
    types::{FinishReason, InferenceMetadata, ProviderCapabilities, StreamLogprobsDelta},
};

/// Mock provider for testing
///
/// This provider returns predefined responses and is useful for unit testing
/// code that uses the LLM shim without making actual API calls.
///
/// # Example
///
/// ```
/// use nxuskit_engine::providers::MockProvider;
/// use nxuskit_engine::types::{ChatRequest, Message};
/// use nxuskit_engine::LLMProvider;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let provider = MockProvider::builder()
///     .with_response("Hello!")
///     .build()?;
///
/// let request = ChatRequest::new("test-model")
///     .with_message(Message::user("Hello"));
/// let response = provider.chat(&request).await?;
/// assert_eq!(response.content, "Hello!");
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct MockProvider {
    response: String,
    model: String,
    /// Initial response for fresh_session() reset
    initial_response: String,
    /// Per-chunk logprob deltas to inject into `chat_stream`. Index `i`
    /// applies to the `i`-th yielded content chunk; `None` at index `i`
    /// means "no logprobs for this chunk". When the response has more
    /// chunks than entries in this vector, later chunks get `None`.
    streaming_logprobs: Vec<Option<StreamLogprobsDelta>>,
    /// Capability-flag override for `supports_streaming_logprobs`. Default
    /// is `false`; set to `true` when injecting logprob deltas above.
    supports_streaming_logprobs: bool,
}

impl MockProvider {
    /// Create a new mock provider with a predefined response
    pub fn new(response: impl Into<String>) -> Self {
        let response = response.into();
        Self {
            response: response.clone(),
            model: "mock-model".to_string(),
            initial_response: response,
            streaming_logprobs: Vec::new(),
            supports_streaming_logprobs: false,
        }
    }

    /// Inject a sequence of per-chunk logprob deltas. Each entry maps to
    /// one yielded streaming content chunk in order; `None` at any index
    /// means "no logprobs for that chunk". This automatically sets
    /// `supports_streaming_logprobs = true` so the capability flag matches
    /// the observed behavior (FR-007 invariant).
    pub fn with_streaming_logprobs(mut self, deltas: Vec<Option<StreamLogprobsDelta>>) -> Self {
        self.streaming_logprobs = deltas;
        self.supports_streaming_logprobs = true;
        self
    }

    /// Create a builder for configuring the provider
    pub fn builder() -> MockProviderBuilder {
        MockProviderBuilder::default()
    }

    /// Set the model name for responses
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Create a fresh session with no accumulated state.
    ///
    /// For MockProvider, this resets the response queue to its initial configured state.
    ///
    /// # Returns
    ///
    /// A new MockProvider instance with the same configuration but reset state.
    ///
    /// # Example
    ///
    /// ```
    /// use nxuskit_engine::providers::MockProvider;
    ///
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let provider = MockProvider::builder()
    ///     .with_response("Hello!")
    ///     .build()?;
    ///
    /// // Use provider...
    ///
    /// // Reset for next test
    /// let fresh = provider.fresh_session();
    /// # Ok(())
    /// # }
    /// ```
    pub fn fresh_session(&self) -> Self {
        Self {
            response: self.initial_response.clone(),
            model: self.model.clone(),
            initial_response: self.initial_response.clone(),
            streaming_logprobs: self.streaming_logprobs.clone(),
            supports_streaming_logprobs: self.supports_streaming_logprobs,
        }
    }
}

impl Default for MockProvider {
    fn default() -> Self {
        Self::new("This is a mock response")
    }
}

/// Builder for MockProvider configuration
#[derive(Debug, Default)]
pub struct MockProviderBuilder {
    response: Option<String>,
    model: Option<String>,
}

impl MockProviderBuilder {
    /// Set the response to return from chat() calls
    pub fn with_response(mut self, response: impl Into<String>) -> Self {
        self.response = Some(response.into());
        self
    }

    /// Set the model name for responses
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Build the MockProvider
    pub fn build(self) -> Result<MockProvider> {
        let response = self
            .response
            .unwrap_or_else(|| "This is a mock response".to_string());
        let model = self.model.unwrap_or_else(|| "mock-model".to_string());

        Ok(MockProvider {
            response: response.clone(),
            model,
            initial_response: response,
            streaming_logprobs: Vec::new(),
            supports_streaming_logprobs: false,
        })
    }
}

#[async_trait]
impl LLMProvider for MockProvider {
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse> {
        let start_time = std::time::Instant::now();

        // Adapt request to provider capabilities
        let capabilities = self.get_capabilities();
        let adapted = ParameterAdapter::adapt(request, &capabilities);

        let usage = TokenUsage::estimated_only(TokenCount::new(10, 20));
        let mut response = ChatResponse::new(self.response.clone(), self.model.clone(), usage);
        response.provider = self.provider_name().to_string();

        // Add parameter adaptation warnings
        response.warnings = adapted.warnings;
        response.finish_reason = Some(FinishReason::Stop);

        // Populate inference metadata
        let execution_time = start_time.elapsed().as_millis() as u64;
        response.inference_metadata = InferenceMetadata::completed(FinishReason::Stop)
            .with_execution_time(execution_time)
            .with_token_usage(response.usage.clone())
            .with_provider_metadata(serde_json::json!({
                "provider": "mock"
            }));

        Ok(response)
    }

    async fn chat_stream(
        &self,
        _request: &ChatRequest,
    ) -> Result<Box<dyn Stream<Item = Result<StreamChunk>> + Send + Unpin>> {
        let response = self.response.clone();
        let injected = self.streaming_logprobs.clone();

        let output_stream = stream! {
            use crate::token_estimator::StreamingTokenAccumulator;

            // Create token accumulator for estimation
            let estimator = crate::token_estimator::TokenEstimator::for_model("mock-model");
            let mut accumulator = StreamingTokenAccumulator::new(estimator, 10);

            // Split response into words and yield them one by one
            for (i, word) in response.split_whitespace().enumerate() {
                let delta = format!("{} ", word);
                accumulator.add_chunk(&delta);

                let mut chunk = StreamChunk::new(delta);
                chunk.usage = Some(accumulator.running_total());
                chunk.logprobs = injected.get(i).cloned().unwrap_or(None);
                yield Ok(chunk);
            }

            // Final chunk with complete usage
            let final_usage = TokenUsage::with_actual(
                crate::types::TokenCount::new(10, 20),
                crate::types::TokenCount::new(10, 20),
            );
            yield Ok(StreamChunk::final_chunk(crate::types::FinishReason::Stop, Some(final_usage)));
        };

        Ok(Box::new(Box::pin(output_stream)))
    }

    fn provider_name(&self) -> &str {
        "mock"
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        Ok(vec![
            {
                // Text-only model
                let mut info = ModelInfo::with_size("mock-text-model", 1_000_000_000); // 1 GB
                info.context_window = Some(4_096);
                info.description = Some("Mock text-only model for testing".to_string());
                info.metadata
                    .insert("modalities".to_string(), "text".to_string());
                info
            },
            {
                // Vision-capable model
                let mut info = ModelInfo::with_size("mock-vision-model", 3_500_000_000); // 3.5 GB
                info.context_window = Some(8_192);
                info.description = Some("Mock multimodal model for testing".to_string());
                info.metadata
                    .insert("modalities".to_string(), "text,vision".to_string());
                info.metadata
                    .insert("max_images".to_string(), "5".to_string());
                info
            },
            {
                // Model with unknown capabilities
                ModelInfo::new("mock-minimal-model")
            },
        ])
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
            supports_streaming_logprobs: self.supports_streaming_logprobs,
            supports_json_mode: true,
            supports_json_schema: false,
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

#[async_trait]
impl ModelLister for MockProvider {
    async fn list_available_models(&self) -> Result<Vec<ModelInfo>> {
        // Return the same models as list_models() to ensure consistent behavior
        self.list_models().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Message;
    use futures::StreamExt;

    #[tokio::test]
    async fn test_mock_provider_chat() {
        let provider = MockProvider::new("Test response");
        let request = ChatRequest::new("mock-model").with_message(Message::user("Hello"));

        let response = provider.chat(&request).await.unwrap();
        assert_eq!(response.content, "Test response");
        assert_eq!(response.model, "mock-model");
    }

    #[tokio::test]
    async fn test_mock_provider_stream() {
        let provider = MockProvider::new("Hello world");
        let request = ChatRequest::new("mock-model").with_message(Message::user("Test"));

        let mut stream = provider.chat_stream(&request).await.unwrap();
        let mut chunks = Vec::new();

        while let Some(chunk) = stream.next().await {
            chunks.push(chunk.unwrap());
        }

        assert!(chunks.len() >= 2); // At least "Hello " and final chunk
        assert!(chunks.last().unwrap().is_final());
    }

    #[tokio::test]
    async fn test_mock_provider_list_models() {
        let provider = MockProvider::default();
        let models = provider.list_models().await.unwrap();
        assert_eq!(models.len(), 3);

        // Test text-only model
        assert_eq!(models[0].name, "mock-text-model");
        assert_eq!(models[0].size_bytes, Some(1_000_000_000));
        assert!(!models[0].supports_vision());
        assert_eq!(models[0].modalities(), vec!["text"]);

        // Test vision-capable model
        assert_eq!(models[1].name, "mock-vision-model");
        assert_eq!(models[1].size_bytes, Some(3_500_000_000));
        assert!(models[1].supports_vision());
        assert_eq!(models[1].modalities(), vec!["text", "vision"]);
        assert_eq!(models[1].max_images(), Some(5));

        // Test minimal model with defaults
        assert_eq!(models[2].name, "mock-minimal-model");
        assert!(!models[2].supports_vision());
        assert_eq!(models[2].modalities(), vec!["text"]);
        assert_eq!(models[2].max_images(), None);
    }

    #[tokio::test]
    async fn mock_provider_with_streaming_logprobs_emits_injected_deltas() {
        use crate::types::{TokenLogprob, TopLogprob};

        let token = |s: &str, lp: f32| TokenLogprob {
            token: s.to_string(),
            logprob: lp,
            bytes: Some(s.as_bytes().to_vec()),
            top_logprobs: vec![TopLogprob {
                token: format!("{s}_alt"),
                logprob: lp - 1.0,
                bytes: None,
            }],
        };
        let deltas = vec![
            Some(StreamLogprobsDelta {
                content: vec![token("Hello", -0.01)],
            }),
            Some(StreamLogprobsDelta {
                content: vec![token("world", -0.05)],
            }),
        ];

        let provider = MockProvider::new("Hello world").with_streaming_logprobs(deltas);
        let caps = provider.get_capabilities();
        assert!(
            caps.supports_streaming_logprobs,
            "with_streaming_logprobs must flip the capability flag"
        );

        let req = ChatRequest::new("mock-model").with_message(Message::user("hi"));
        let mut stream = provider.chat_stream(&req).await.unwrap();
        let mut content_chunks: Vec<StreamChunk> = Vec::new();
        while let Some(chunk) = stream.next().await {
            let c = chunk.unwrap();
            if !c.is_final() {
                content_chunks.push(c);
            }
        }
        assert_eq!(content_chunks.len(), 2);
        let lp0 = content_chunks[0]
            .logprobs
            .as_ref()
            .expect("first chunk has injected logprobs");
        assert_eq!(lp0.content[0].token, "Hello");
        let lp1 = content_chunks[1]
            .logprobs
            .as_ref()
            .expect("second chunk has injected logprobs");
        assert_eq!(lp1.content[0].token, "world");
    }

    #[tokio::test]
    async fn mock_provider_default_streaming_emits_no_logprobs() {
        let provider = MockProvider::new("Hello world");
        let caps = provider.get_capabilities();
        assert!(!caps.supports_streaming_logprobs);

        let req = ChatRequest::new("mock-model").with_message(Message::user("hi"));
        let mut stream = provider.chat_stream(&req).await.unwrap();
        while let Some(chunk) = stream.next().await {
            let c = chunk.unwrap();
            assert!(c.logprobs.is_none(), "default mock must not emit logprobs");
        }
    }
}
