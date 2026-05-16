//! Provider trait definition for LLM implementations

use crate::{
    ChatRequest, ChatResponse, StreamChunk, TokenUsage,
    error::Result,
    types::{ModelInfo, ProviderCapabilities},
};
use async_stream::stream;
use async_trait::async_trait;
use futures::Stream;
use futures::StreamExt;
use tokio::sync::oneshot;

/// Trait for providers that support model discovery.
///
/// This trait is separate from `LLMProvider` to ensure correct dispatch
/// when used through trait objects (`Box<dyn ModelLister>`).
///
/// # Why a Separate Trait?
///
/// The `LLMProvider::list_models()` method has a default implementation,
/// which can cause incorrect dispatch through trait objects due to how
/// `async_trait` handles default methods. By using a separate trait with
/// no default implementation, we guarantee correct vtable dispatch.
///
/// # Providers Implementing This Trait
///
/// - `OllamaProvider` - Lists locally installed Ollama models
/// - `LmStudioProvider` - Lists locally available LM Studio models
/// - `ClipsProvider` - Lists discovered .clp rule base files
/// - `MockProvider` - Returns configured mock models
/// - `LoopbackProvider` - Returns a single loopback model
///
/// # Providers NOT Implementing This Trait
///
/// API-based providers (Claude, OpenAI, etc.) do not implement this trait
/// because their APIs do not support model discovery.
///
/// # Example
///
/// ```no_run
/// use nxuskit_engine::provider::ModelLister;
/// use nxuskit_engine::providers::OllamaProvider;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Create multiple providers
/// let ollama = OllamaProvider::builder().build()?;
///
/// // Store as trait object - dispatch works correctly
/// let lister: Box<dyn ModelLister> = Box::new(ollama);
///
/// // Polymorphic call works correctly
/// let models = lister.list_available_models().await?;
/// println!("Found {} models", models.len());
/// # Ok(())
/// # }
/// ```
#[async_trait]
pub trait ModelLister: Send + Sync {
    /// List available models with detailed information.
    ///
    /// Unlike `LLMProvider::list_models()`, this method has no default
    /// implementation and dispatches correctly through trait objects.
    ///
    /// # Returns
    ///
    /// A vector of `ModelInfo` objects. May be empty if no models are available.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The provider's backend is unreachable
    /// - Authentication fails (if applicable)
    /// - The response cannot be parsed
    async fn list_available_models(&self) -> Result<Vec<ModelInfo>>;
}

/// Trait for LLM providers
///
/// This trait defines the interface that all LLM providers must implement.
/// It provides both synchronous (chat) and streaming (chat_stream) methods.
#[async_trait]
pub trait LLMProvider: Send + Sync {
    /// Send a chat request and get complete response
    ///
    /// This method sends a chat completion request and waits for the full response.
    ///
    /// # Arguments
    ///
    /// * `request` - The chat request containing messages and parameters
    ///
    /// # Returns
    ///
    /// Returns a `ChatResponse` containing the generated content and metadata.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Authentication fails
    /// - Network errors occur
    /// - Rate limits are exceeded
    /// - The request is invalid
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse>;

    /// Send a chat request and stream response chunks
    ///
    /// This method sends a chat completion request and returns a stream of
    /// incremental response chunks.
    ///
    /// # Arguments
    ///
    /// * `request` - The chat request containing messages and parameters
    ///
    /// # Returns
    ///
    /// Returns a stream of `StreamChunk` objects. The final chunk will have
    /// `finish_reason` set.
    ///
    /// # Errors
    ///
    /// Returns an error if the request setup fails. Streaming errors are
    /// returned as items in the stream.
    async fn chat_stream(
        &self,
        request: &ChatRequest,
    ) -> Result<Box<dyn Stream<Item = Result<StreamChunk>> + Send + Unpin>>;

    /// Send a chat request and stream response chunks with final token usage
    ///
    /// This convenience method combines `chat_stream()` with automatic token usage
    /// aggregation. It returns both the streaming chunks and a channel that will
    /// receive the final `TokenUsage` after the stream completes.
    ///
    /// # Arguments
    ///
    /// * `request` - The chat request containing messages and parameters
    ///
    /// # Returns
    ///
    /// Returns a tuple of:
    /// - A stream of `StreamChunk` objects
    /// - A `oneshot::Receiver<TokenUsage>` that receives the final usage
    ///
    /// The receiver will be signaled after the stream is exhausted or on error.
    /// If an error occurs during streaming, the receiver will contain usage with
    /// `is_complete = false` to indicate a partial stream.
    ///
    /// # Errors
    ///
    /// Returns an error if the request setup fails. The receiver itself will
    /// always be available even if streaming fails.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use nxuskit_engine::prelude::*;
    /// use futures::StreamExt;
    ///
    /// # async fn example() -> std::result::Result<(), Box<dyn std::error::Error>> {
    /// let provider = OllamaProvider::builder().build()?;
    /// let request = ChatRequest::new("Hello, world!");
    ///
    /// let (stream, usage_rx) = provider.stream_with_usage(&request).await?;
    ///
    /// // Consume the stream
    /// let mut stream = Box::pin(stream);
    /// while let Some(result) = stream.next().await {
    ///     match result {
    ///         Ok(chunk) => print!("{}", chunk.delta),
    ///         Err(e) => eprintln!("Error: {}", e),
    ///     }
    /// }
    ///
    /// // Get the final usage
    /// let usage = usage_rx.await?;
    /// println!("Total tokens: {}", usage.best_available().total());
    /// # Ok(())
    /// # }
    /// ```
    async fn stream_with_usage(
        &self,
        request: &ChatRequest,
    ) -> Result<(
        Box<dyn Stream<Item = Result<StreamChunk>> + Send + Unpin>,
        oneshot::Receiver<TokenUsage>,
    )> {
        let (tx, rx) = oneshot::channel();
        let stream = self.chat_stream(request).await?;

        // Create a wrapping stream that captures final usage
        let output_stream = stream! {
            let mut stream = stream;
            let mut final_usage: Option<TokenUsage> = None;

            while let Some(result) = stream.next().await {
                match result {
                    Ok(chunk) => {
                        // Capture usage from this chunk for later
                        if let Some(usage) = &chunk.usage {
                            final_usage = Some(usage.clone());
                        }
                        yield Ok(chunk);
                    }
                    Err(e) => {
                        yield Err(e);
                    }
                }
            }

            // Send final usage through channel
            if let Some(usage) = final_usage {
                let _ = tx.send(usage);
            }
        };

        Ok((Box::new(Box::pin(output_stream)), rx))
    }

    /// Get the provider name (for logging/debugging)
    ///
    /// # Returns
    ///
    /// A string identifying the provider (e.g., "claude", "openai", "ollama")
    fn provider_name(&self) -> &str;

    /// List available models with detailed information
    ///
    /// Returns structured information about each available model including
    /// size, capabilities, and provider-specific metadata.
    ///
    /// Not all providers support listing models. The default implementation
    /// returns an empty vector.
    ///
    /// # Returns
    ///
    /// A vector of `ModelInfo` objects containing model details.
    ///
    /// # Errors
    ///
    /// Returns an error if the provider API is unreachable or returns invalid data.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use nxuskit_engine::prelude::*;
    ///
    /// # async fn example() -> std::result::Result<(), Box<dyn std::error::Error>> {
    /// let provider = OllamaProvider::builder().build()
    ///     .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
    /// let models = provider.list_models().await
    ///     .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
    /// for model in models {
    ///     println!("{} - {}",
    ///         model.name,
    ///         model.formatted_size().unwrap_or_default()
    ///     );
    /// }
    /// # Ok(())
    /// # }
    /// ```
    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        Ok(Vec::new())
    }

    /// Get the capabilities of this provider
    ///
    /// Returns information about what features and parameters this provider supports.
    /// This is used by the parameter adapter to gracefully handle unsupported parameters.
    ///
    /// # Returns
    ///
    /// A `ProviderCapabilities` struct describing supported features.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use nxuskit_engine::prelude::*;
    ///
    /// # fn example() -> std::result::Result<(), Box<dyn std::error::Error>> {
    /// let provider = OpenAIProvider::builder()
    ///     .api_key("api-key")
    ///     .build()?;
    /// let caps = provider.get_capabilities();
    /// if caps.supports_json_mode {
    ///     println!("Provider supports native JSON mode");
    /// }
    /// if let Some(max_stop) = caps.max_stop_sequences {
    ///     println!("Provider supports up to {} stop sequences", max_stop);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    fn get_capabilities(&self) -> ProviderCapabilities {
        // Default implementation returns minimal capabilities
        ProviderCapabilities::default()
    }

    /// Returns this provider as a `CapabilityDetector` if it supports per-model
    /// capability detection. Providers that implement `CapabilityDetector` should
    /// override this to return `Some(self)`.
    fn as_capability_detector(&self) -> Option<&dyn crate::capability::CapabilityDetector> {
        None
    }
}
