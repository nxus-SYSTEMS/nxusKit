//! Mock provider for test isolation without the SDK binary.
//!
//! [`MockProvider`] implements [`AsyncProvider`](crate::AsyncProvider) with configurable canned
//! responses, request recording, and zero dependency on `libnxuskit`.
//! This enables unit testing of LLM integration logic in CI environments
//! where the SDK binary is not available.
//!
//! # Examples
//!
//! ```
//! use nxuskit::{AsyncProvider, ChatRequest, Message, MockProvider, Role};
//!
//! # tokio_test::block_on(async {
//! let provider = MockProvider::new("The answer is 42");
//! let request = ChatRequest {
//!     model: "mock-model".into(),
//!     messages: vec![Message { role: Role::User, content: "question".into() }],
//!     ..Default::default()
//! };
//! let response = provider.chat(request).await.unwrap();
//! assert_eq!(response.content, "The answer is 42");
//! # });
//! ```

use crate::error::NxuskitError;
use crate::stream::StreamReceiver;
use crate::types::{ChatRequest, ChatResponse, ModelInfo, TokenUsage};

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A test-oriented provider that implements [`AsyncProvider`](crate::AsyncProvider) without
/// requiring the `libnxuskit` binary.
///
/// Records every request for test assertions, returns responses from a
/// configurable sequence, and supports polymorphic dispatch as
/// `Box<dyn AsyncProvider>`.
///
/// # Construction
///
/// Three entry points for different complexity levels:
///
/// - [`MockProvider::new`] — single canned response
/// - [`MockProvider::with_responses`] — ordered response sequence
/// - [`MockProvider::builder`] — full configuration (responses, models, model name)
///
/// # Examples
///
/// ```
/// use nxuskit::MockProvider;
///
/// // Simplest usage: single response
/// let provider = MockProvider::new("Hello!");
///
/// // Sequential responses
/// let provider = MockProvider::with_responses(vec!["First", "Second"]);
///
/// // Full configuration via builder
/// let provider = MockProvider::builder()
///     .with_response("Hello")
///     .with_model_name("gpt-4o")
///     .build();
/// ```
pub struct MockProvider {
    responses: Vec<String>,
    response_index: AtomicUsize,
    models: Vec<ModelInfo>,
    recorded_requests: Mutex<Vec<ChatRequest>>,
    model_name: String,
}

impl MockProvider {
    /// Create a mock provider that always returns the given response.
    ///
    /// # Examples
    ///
    /// ```
    /// use nxuskit::{AsyncProvider, ChatRequest, MockProvider};
    ///
    /// # tokio_test::block_on(async {
    /// let provider = MockProvider::new("Hello");
    /// let request = ChatRequest { model: "m".into(), ..Default::default() };
    /// let response = provider.chat(request).await.unwrap();
    /// assert_eq!(response.content, "Hello");
    /// # });
    /// ```
    pub fn new(response: impl Into<String>) -> Self {
        Self {
            responses: vec![response.into()],
            response_index: AtomicUsize::new(0),
            models: vec![default_mock_model()],
            recorded_requests: Mutex::new(Vec::new()),
            model_name: "mock-model".into(),
        }
    }

    /// Create a mock provider that returns responses in sequence.
    ///
    /// Each `chat()` call returns the next response. After exhaustion,
    /// the last response repeats. An empty vec produces a default empty
    /// response.
    ///
    /// # Examples
    ///
    /// ```
    /// use nxuskit::{AsyncProvider, ChatRequest, MockProvider};
    ///
    /// # tokio_test::block_on(async {
    /// let provider = MockProvider::with_responses(vec!["A", "B", "C"]);
    /// let req = ChatRequest { model: "m".into(), ..Default::default() };
    /// let r1 = provider.chat(req.clone()).await.unwrap();
    /// assert_eq!(r1.content, "A");
    /// # });
    /// ```
    pub fn with_responses(responses: Vec<impl Into<String>>) -> Self {
        Self {
            responses: responses.into_iter().map(Into::into).collect(),
            response_index: AtomicUsize::new(0),
            models: vec![default_mock_model()],
            recorded_requests: Mutex::new(Vec::new()),
            model_name: "mock-model".into(),
        }
    }

    /// Start building a [`MockProvider`] with full configuration.
    ///
    /// # Examples
    ///
    /// ```
    /// use nxuskit::{MockProvider, ModelInfo};
    ///
    /// let provider = MockProvider::builder()
    ///     .with_response("Hello")
    ///     .with_model_name("custom-model")
    ///     .with_models(vec![ModelInfo {
    ///         id: "custom".into(),
    ///         name: "Custom Model".into(),
    ///         description: None,
    ///         size_bytes: None,
    ///         context_window: Some(128000),
    ///         metadata: Default::default(),
    ///     }])
    ///     .build();
    /// ```
    pub fn builder() -> MockProviderBuilder {
        MockProviderBuilder::default()
    }

    /// Return a clone of all recorded requests.
    ///
    /// Each `chat()` call records its [`ChatRequest`]. Use this method
    /// to inspect what was sent to the mock for test assertions.
    ///
    /// # Examples
    ///
    /// ```
    /// use nxuskit::{AsyncProvider, ChatRequest, MockProvider};
    ///
    /// # tokio_test::block_on(async {
    /// let provider = MockProvider::new("OK");
    /// let req = ChatRequest { model: "gpt-4o".into(), ..Default::default() };
    /// provider.chat(req).await.unwrap();
    /// let recorded = provider.requests();
    /// assert_eq!(recorded.len(), 1);
    /// assert_eq!(recorded[0].model, "gpt-4o");
    /// # });
    /// ```
    pub fn requests(&self) -> Vec<ChatRequest> {
        self.recorded_requests
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

#[async_trait::async_trait]
impl crate::async_provider::AsyncProvider for MockProvider {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, NxuskitError> {
        // Record the request.
        self.recorded_requests
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(request);

        // Get the next response, clamping to last on exhaustion.
        let content = if self.responses.is_empty() {
            String::new()
        } else {
            let idx = self.response_index.fetch_add(1, Ordering::Relaxed);
            let clamped = idx.min(self.responses.len() - 1);
            self.responses[clamped].clone()
        };

        let model_name = if self.responses.is_empty() {
            "mock".to_string()
        } else {
            self.model_name.clone()
        };

        Ok(ChatResponse {
            content,
            model: model_name,
            provider: "mock".into(),
            usage: TokenUsage::default(),
            finish_reason: Some(crate::FinishReason::Stop),
            metadata: HashMap::new(),
            warnings: vec![],
            logprobs: None,
            tool_calls: None,
            inference_metadata: None,
        })
    }

    fn chat_stream(&self, _request: ChatRequest) -> Result<StreamReceiver, NxuskitError> {
        Err(NxuskitError::Internal {
            message: "streaming not supported by MockProvider".into(),
        })
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, NxuskitError> {
        Ok(self.models.clone())
    }
}

/// Builder for [`MockProvider`] with full configuration options.
///
/// # Examples
///
/// ```
/// use nxuskit::MockProvider;
///
/// let provider = MockProvider::builder()
///     .with_response("Hello")
///     .with_model_name("my-model")
///     .build();
/// ```
pub struct MockProviderBuilder {
    responses: Vec<String>,
    models: Vec<ModelInfo>,
    model_name: String,
}

impl Default for MockProviderBuilder {
    fn default() -> Self {
        Self {
            responses: vec![String::new()],
            models: vec![default_mock_model()],
            model_name: "mock-model".into(),
        }
    }
}

impl MockProviderBuilder {
    /// Set a single response for the mock provider.
    pub fn with_response(mut self, response: impl Into<String>) -> Self {
        self.responses = vec![response.into()];
        self
    }

    /// Set multiple sequential responses for the mock provider.
    pub fn with_responses(mut self, responses: Vec<impl Into<String>>) -> Self {
        self.responses = responses.into_iter().map(Into::into).collect();
        self
    }

    /// Set the model list returned by `list_models()`.
    pub fn with_models(mut self, models: Vec<ModelInfo>) -> Self {
        self.models = models;
        self
    }

    /// Set the model name returned in chat responses.
    pub fn with_model_name(mut self, name: impl Into<String>) -> Self {
        self.model_name = name.into();
        self
    }

    /// Build the [`MockProvider`].
    pub fn build(self) -> MockProvider {
        MockProvider {
            responses: self.responses,
            response_index: AtomicUsize::new(0),
            models: self.models,
            recorded_requests: Mutex::new(Vec::new()),
            model_name: self.model_name,
        }
    }
}

fn default_mock_model() -> ModelInfo {
    ModelInfo {
        id: "mock".into(),
        name: "Mock Model".into(),
        description: None,
        size_bytes: None,
        context_window: None,
        metadata: Default::default(),
    }
}
