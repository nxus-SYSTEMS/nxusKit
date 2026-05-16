//! Synchronous/blocking wrapper for async providers.
//!
//! This module provides `BlockingProvider<P>`, a wrapper that enables
//! synchronous usage of async LLM providers. It's useful for:
//!
//! - Immediate-mode UI applications (egui, imgui)
//! - Simple scripts without async runtime
//! - Testing in synchronous contexts
//!
//! # Feature Flag
//!
//! This module requires the `blocking-api` feature:
//!
//! ```toml
//! [dependencies]
//! nxuskit_engine = { version = "0.7", features = ["blocking-api"] }
//! ```
//!
//! # Example
//!
//! ```no_run
//! use nxuskit_engine::blocking::BlockingProvider;
//! use nxuskit_engine::providers::MockProvider;
//! use nxuskit_engine::types::{ChatRequest, Message};
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create async provider
//!     let mock = MockProvider::builder()
//!         .with_response("Hello from blocking!")
//!         .build()?;
//!
//!     // Wrap for synchronous use
//!     let blocking = BlockingProvider::new(mock)?;
//!
//!     // Synchronous API call
//!     let request = ChatRequest::new("test-model")
//!         .with_message(Message::user("Hello"));
//!     let response = blocking.chat(&request)?;
//!     println!("{}", response.content);
//!
//!     Ok(())
//! }
//! ```

use crate::error::{NxuskitError, Result};
use crate::provider::{LLMProvider, ModelLister};
use crate::types::{ChatRequest, ChatResponse, ModelInfo};
use tokio::runtime::Runtime;

/// Synchronous wrapper for any `LLMProvider`.
///
/// This wrapper maintains an internal tokio runtime and provides
/// blocking versions of async provider methods.
///
/// # Runtime Behavior
///
/// The `BlockingProvider` creates an isolated tokio runtime that doesn't
/// interfere with any existing async context. It's safe to use:
///
/// - From synchronous code (primary use case)
/// - Inside an existing async runtime (creates nested runtime)
/// - On UI threads (won't block other async work)
///
/// # Example
///
/// ```no_run
/// use nxuskit_engine::blocking::BlockingProvider;
/// use nxuskit_engine::providers::MockProvider;
/// use nxuskit_engine::types::{ChatRequest, Message};
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Create async provider
/// let mock = MockProvider::builder()
///     .with_response("Hello!")
///     .build()?;
///
/// // Wrap for synchronous use
/// let blocking = BlockingProvider::new(mock)?;
///
/// // Make synchronous calls
/// let request = ChatRequest::new("test-model")
///     .with_message(Message::user("Hello"));
///
/// let response = blocking.chat(&request)?;  // Blocks until complete
/// println!("{}", response.content);
/// # Ok(())
/// # }
/// ```
pub struct BlockingProvider<P: LLMProvider> {
    inner: P,
    runtime: Runtime,
}

impl<P: LLMProvider> BlockingProvider<P> {
    /// Create a new blocking wrapper with a dedicated runtime.
    ///
    /// # Errors
    ///
    /// Returns an error if the tokio runtime cannot be created.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use nxuskit_engine::blocking::BlockingProvider;
    /// use nxuskit_engine::providers::MockProvider;
    ///
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let provider = MockProvider::builder()
    ///     .with_response("Hello!")
    ///     .build()?;
    /// let blocking = BlockingProvider::new(provider)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(provider: P) -> Result<Self> {
        let runtime = Runtime::new().map_err(|e| {
            NxuskitError::Configuration(format!("Failed to create blocking runtime: {}", e))
        })?;
        Ok(Self {
            inner: provider,
            runtime,
        })
    }

    /// Send a chat request synchronously.
    ///
    /// This method blocks until the provider returns a complete response.
    ///
    /// # Arguments
    ///
    /// * `request` - The chat request to send
    ///
    /// # Returns
    ///
    /// The complete chat response.
    ///
    /// # Errors
    ///
    /// Returns any error from the underlying provider.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use nxuskit_engine::blocking::BlockingProvider;
    /// use nxuskit_engine::providers::MockProvider;
    /// use nxuskit_engine::types::{ChatRequest, Message};
    ///
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let provider = MockProvider::builder()
    ///     .with_response("Hello!")
    ///     .build()?;
    /// let blocking = BlockingProvider::new(provider)?;
    ///
    /// let request = ChatRequest::new("test-model")
    ///     .with_message(Message::user("Hello"));
    /// let response = blocking.chat(&request)?;
    /// println!("{}", response.content);
    /// # Ok(())
    /// # }
    /// ```
    pub fn chat(&self, request: &ChatRequest) -> Result<ChatResponse> {
        self.runtime.block_on(self.inner.chat(request))
    }

    /// Get a reference to the underlying async provider.
    ///
    /// This can be useful if you need to access provider-specific methods
    /// or configuration that isn't exposed through the blocking interface.
    pub fn inner(&self) -> &P {
        &self.inner
    }

    /// Consume the wrapper and return the underlying provider.
    ///
    /// This is useful if you need to switch back to async usage.
    pub fn into_inner(self) -> P {
        self.inner
    }

    /// Get the provider name.
    ///
    /// Delegates to the underlying provider's `provider_name()` method.
    pub fn provider_name(&self) -> &str {
        self.inner.provider_name()
    }
}

impl<P: LLMProvider + ModelLister> BlockingProvider<P> {
    /// List available models synchronously.
    ///
    /// This method is only available when the wrapped provider
    /// implements the `ModelLister` trait.
    ///
    /// # Returns
    ///
    /// A vector of available models.
    ///
    /// # Errors
    ///
    /// Returns any error from the underlying provider.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use nxuskit_engine::blocking::BlockingProvider;
    /// use nxuskit_engine::providers::MockProvider;
    ///
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let provider = MockProvider::builder()
    ///     .with_response("Hello!")
    ///     .build()?;
    /// let blocking = BlockingProvider::new(provider)?;
    ///
    /// let models = blocking.list_models()?;
    /// for model in models {
    ///     println!("{}", model.name);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn list_models(&self) -> Result<Vec<ModelInfo>> {
        self.runtime.block_on(self.inner.list_available_models())
    }
}

impl<P: LLMProvider + std::fmt::Debug> std::fmt::Debug for BlockingProvider<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlockingProvider")
            .field("inner", &self.inner)
            .field("runtime", &"<tokio::Runtime>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::MockProvider;
    use crate::types::Message;

    #[test]
    fn test_blocking_provider_creation() {
        let mock = MockProvider::builder()
            .with_response("Hello!")
            .build()
            .unwrap();
        let blocking = BlockingProvider::new(mock);
        assert!(blocking.is_ok());
    }

    #[test]
    fn test_blocking_provider_chat() {
        let mock = MockProvider::builder()
            .with_response("Hello from blocking!")
            .build()
            .unwrap();
        let blocking = BlockingProvider::new(mock).unwrap();

        let request = ChatRequest::new("test-model").with_message(Message::user("Hello"));

        let response = blocking.chat(&request);
        assert!(response.is_ok());
        assert_eq!(response.unwrap().content, "Hello from blocking!");
    }

    #[test]
    fn test_blocking_provider_name() {
        let mock = MockProvider::builder()
            .with_response("Hello!")
            .build()
            .unwrap();
        let blocking = BlockingProvider::new(mock).unwrap();
        assert_eq!(blocking.provider_name(), "mock");
    }

    #[test]
    fn test_blocking_provider_inner() {
        let mock = MockProvider::builder()
            .with_response("Hello!")
            .build()
            .unwrap();
        let blocking = BlockingProvider::new(mock).unwrap();

        // Can access inner provider
        assert_eq!(blocking.inner().provider_name(), "mock");
    }

    #[test]
    fn test_blocking_provider_into_inner() {
        let mock = MockProvider::builder()
            .with_response("Hello!")
            .build()
            .unwrap();
        let blocking = BlockingProvider::new(mock).unwrap();

        let inner = blocking.into_inner();
        assert_eq!(inner.provider_name(), "mock");
    }

    #[test]
    fn test_blocking_list_models() {
        let mock = MockProvider::builder()
            .with_response("Hello!")
            .build()
            .unwrap();
        let blocking = BlockingProvider::new(mock).unwrap();

        let models = blocking.list_models();
        assert!(models.is_ok());
        // MockProvider returns one model named "mock-model"
        let models = models.unwrap();
        assert!(!models.is_empty());
    }
}
