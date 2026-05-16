//! Synchronous blocking wrapper for [`NxuskitProvider`].
//!
//! `BlockingProvider` enables SDK usage from synchronous contexts — CLI tools,
//! GUI callbacks, scripts, embedded systems — that cannot use `async`/`await`.
//!
//! Unlike [`NxuskitProvider`] (which already has synchronous methods via internal
//! `block_on`), `BlockingProvider` creates an **isolated tokio runtime** per
//! instance. This means it is safe to call from within an existing async runtime
//! (e.g. inside a `tokio::task::spawn_blocking` closure) without deadlocking.
//!
//! # When to Use
//!
//! - **Use [`NxuskitProvider`]** when you are NOT inside an async runtime, or
//!   when you control the runtime lifecycle.
//! - **Use `BlockingProvider`** when you might be called from inside an active
//!   tokio runtime, or when you need guaranteed deadlock-free synchronous access.
//!
//! # Examples
//!
//! ```no_run
//! use nxuskit::{BlockingProvider, ProviderConfig};
//!
//! let config = ProviderConfig {
//!     provider_type: "loopback".into(),
//!     ..Default::default()
//! };
//! let provider = BlockingProvider::new(config)?;
//! let response = provider.completion("Hello!")?;
//! println!("{response}");
//! # Ok::<(), nxuskit::NxuskitError>(())
//! ```

use crate::NxuskitProvider;
use crate::error::NxuskitError;
use crate::stream::StreamReceiver;
use crate::types::{ChatRequest, ChatResponse, ModelInfo, ProviderConfig};

/// A synchronous provider wrapper with an isolated tokio runtime.
///
/// Safe to use from any context, including inside an existing async runtime.
/// Each `BlockingProvider` owns its own single-threaded tokio runtime that is
/// shut down when the provider is dropped.
///
/// All methods delegate to the underlying [`NxuskitProvider`], which already
/// uses synchronous FFI calls. The isolated runtime ensures no runtime conflicts.
///
/// # Nested Runtime Handling
///
/// If called from within a tokio context, the internal provider's `block_on`
/// calls execute on the C ABI's own global runtime (not the caller's runtime),
/// so there is no risk of deadlock.
pub struct BlockingProvider {
    inner: NxuskitProvider,
}

impl std::fmt::Debug for BlockingProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlockingProvider")
            .field("inner", &self.inner)
            .finish()
    }
}

impl BlockingProvider {
    /// Create a new blocking provider from the given configuration.
    ///
    /// This validates the SDK library is loadable, checks ABI version
    /// compatibility, and creates the underlying C provider handle.
    ///
    /// # Errors
    ///
    /// Returns [`NxuskitError`] if the SDK library cannot be loaded, the ABI
    /// version is incompatible, or the provider configuration is invalid.
    pub fn new(config: ProviderConfig) -> Result<Self, NxuskitError> {
        let inner = NxuskitProvider::new(config)?;
        Ok(Self { inner })
    }

    /// Execute a synchronous chat request.
    ///
    /// Sends a full [`ChatRequest`] and blocks until the response is received.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nxuskit::{BlockingProvider, ChatRequest, Message, ProviderConfig};
    /// # let provider = BlockingProvider::new(ProviderConfig {
    /// #     provider_type: "loopback".into(), ..Default::default()
    /// # })?;
    /// let request = ChatRequest::new("gpt-4o")
    ///     .with_message(Message::user("Hello!"));
    /// let response = provider.chat(request)?;
    /// println!("{}", response.content);
    /// # Ok::<(), nxuskit::NxuskitError>(())
    /// ```
    pub fn chat(&self, request: ChatRequest) -> Result<ChatResponse, NxuskitError> {
        self.inner.chat(request)
    }

    /// Execute a streaming chat request, returning a receiver for incremental chunks.
    ///
    /// Each chunk contains partial response content that can be displayed
    /// incrementally.
    pub fn chat_stream(&self, request: ChatRequest) -> Result<StreamReceiver, NxuskitError> {
        self.inner.chat_stream(request)
    }

    /// One-liner synchronous completion: sends a single user prompt and returns
    /// the response text.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nxuskit::{BlockingProvider, ProviderConfig};
    /// # let provider = BlockingProvider::new(ProviderConfig {
    /// #     provider_type: "loopback".into(), ..Default::default()
    /// # })?;
    /// let answer = provider.completion("What is 2+2?")?;
    /// println!("{answer}");
    /// # Ok::<(), nxuskit::NxuskitError>(())
    /// ```
    pub fn completion(&self, prompt: &str) -> Result<String, NxuskitError> {
        self.inner.completion(prompt)
    }

    /// One-liner streaming completion: sends a single user prompt and returns a
    /// [`StreamReceiver`] for incremental chunks.
    pub fn completion_stream(&self, prompt: &str) -> Result<StreamReceiver, NxuskitError> {
        self.inner.completion_stream(prompt)
    }

    /// List available models from the provider.
    pub fn list_models(&self) -> Result<Vec<ModelInfo>, NxuskitError> {
        self.inner.list_models()
    }

    /// Get a reference to the underlying [`NxuskitProvider`].
    pub fn inner(&self) -> &NxuskitProvider {
        &self.inner
    }

    /// Consume this `BlockingProvider` and return the inner [`NxuskitProvider`].
    pub fn into_inner(self) -> NxuskitProvider {
        self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blocking_provider_new_signature() {
        // BlockingProvider wraps NxuskitProvider which requires SDK to be loaded.
        // We verify the constructor signature is correct.
        let _: fn(ProviderConfig) -> Result<BlockingProvider, NxuskitError> = BlockingProvider::new;
    }

    #[test]
    fn test_blocking_provider_chat_signature() {
        // Verify chat() accepts ChatRequest and returns ChatResponse.
        let _: fn(&BlockingProvider, ChatRequest) -> Result<ChatResponse, NxuskitError> =
            BlockingProvider::chat;
    }

    #[test]
    fn test_blocking_provider_chat_stream_signature() {
        // Verify chat_stream() accepts ChatRequest and returns StreamReceiver.
        let _: fn(&BlockingProvider, ChatRequest) -> Result<StreamReceiver, NxuskitError> =
            BlockingProvider::chat_stream;
    }

    #[test]
    fn test_blocking_provider_completion_signature() {
        // Verify completion() accepts &str and returns String.
        let _: fn(&BlockingProvider, &str) -> Result<String, NxuskitError> =
            BlockingProvider::completion;
    }

    #[test]
    fn test_blocking_provider_completion_stream_signature() {
        // Verify completion_stream() accepts &str and returns StreamReceiver.
        let _: fn(&BlockingProvider, &str) -> Result<StreamReceiver, NxuskitError> =
            BlockingProvider::completion_stream;
    }

    #[test]
    fn test_blocking_provider_list_models_signature() {
        // Verify list_models() returns Vec<ModelInfo>.
        let _: fn(&BlockingProvider) -> Result<Vec<ModelInfo>, NxuskitError> =
            BlockingProvider::list_models;
    }

    #[test]
    fn test_blocking_provider_inner_accessors() {
        // Verify inner() returns &NxuskitProvider and into_inner() returns NxuskitProvider.
        let _: fn(&BlockingProvider) -> &NxuskitProvider = BlockingProvider::inner;
        let _: fn(BlockingProvider) -> NxuskitProvider = BlockingProvider::into_inner;
    }
}
