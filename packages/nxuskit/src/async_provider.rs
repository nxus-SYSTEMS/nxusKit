//! Async provider trait for polymorphic dynamic dispatch.
//!
//! The [`AsyncProvider`] trait enables `Box<dyn AsyncProvider>` for runtime
//! polymorphism — consumers can swap between provider implementations (e.g.,
//! `NxuskitProvider`, test mocks) without changing calling code.
//!
//! # Object safety
//!
//! The trait uses [`async_trait`] to make `async fn` methods object-safe
//! (compatible with `dyn AsyncProvider`). Native `async fn` in traits is not
//! object-safe in Rust as of 1.92.
//!
//! # Examples
//!
//! ```no_run
//! use nxuskit::{AsyncProvider, ChatRequest, Message, NxuskitProvider, ProviderConfig, Role};
//!
//! async fn ask(provider: &dyn AsyncProvider, question: &str) -> String {
//!     let request = ChatRequest {
//!         model: "gpt-4o".into(),
//!         messages: vec![Message { role: Role::User, content: question.into() }],
//!         ..Default::default()
//!     };
//!     provider.chat(request).await.unwrap().content
//! }
//! ```

use crate::error::NxuskitError;
use crate::provider::NxuskitProvider;
use crate::stream::StreamReceiver;
use crate::types::{ChatRequest, ChatResponse, ModelInfo};

/// Trait for async provider operations, enabling `Box<dyn AsyncProvider>`.
///
/// All implementors must be `Send + Sync` so that trait objects can be shared
/// across async tasks and threads.
#[async_trait::async_trait]
pub trait AsyncProvider: Send + Sync {
    /// Send an async chat request and return the full response.
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, NxuskitError>;

    /// Start a streaming chat request, returning a receiver for incremental chunks.
    ///
    /// This method is synchronous because the async part is consuming the
    /// returned [`StreamReceiver`] (which implements [`futures_core::Stream`]).
    fn chat_stream(&self, request: ChatRequest) -> Result<StreamReceiver, NxuskitError>;

    /// List models available from this provider asynchronously.
    async fn list_models(&self) -> Result<Vec<ModelInfo>, NxuskitError>;
}

#[async_trait::async_trait]
impl AsyncProvider for NxuskitProvider {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, NxuskitError> {
        self.chat_async(request).await
    }

    fn chat_stream(&self, request: ChatRequest) -> Result<StreamReceiver, NxuskitError> {
        NxuskitProvider::chat_stream(self, request)
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, NxuskitError> {
        self.list_models_async().await
    }
}
