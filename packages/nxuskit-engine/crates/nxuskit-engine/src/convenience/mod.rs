//! Convenience API for simplified LLM access
//!
//! This module provides LiteLLM-style convenience functions that automatically
//! detect providers from environment variables and model names.
//!
//! # Quick Start
//!
//! ```no_run
//! use nxuskit_engine::convenience::completion;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Set OPENAI_API_KEY environment variable first
//! let response = completion("gpt-4o", "What is Rust?").await?;
//! println!("{}", response);
//! # Ok(())
//! # }
//! ```

pub mod env_detector;
pub mod parser;
pub mod router;

// Re-export commonly used types
pub use env_detector::EnvConfig;
pub use parser::{ModelSpecifier, ProviderName};
pub use router::ProviderRouter;

use crate::error::Result;
use crate::types::{ChatRequest, Message};
use futures::{Stream, StreamExt};

/// Execute a single-turn chat completion with automatic provider detection
///
/// This is the simplest way to interact with LLMs across different providers.
/// The provider is automatically selected based on the model name and available
/// environment variables.
///
/// # Environment Variables
///
/// - `OPENAI_API_KEY` - For OpenAI models (gpt-4o, gpt-3.5-turbo, etc.)
/// - `ANTHROPIC_API_KEY` - For Anthropic models (claude-*)
/// - `OLLAMA_API_URL` - For Ollama models (default: http://localhost:11434)
///
/// # Examples
///
/// ```no_run
/// use nxuskit_engine::convenience::completion;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // OpenAI (inferred from model name)
/// let response = completion("gpt-4o", "Explain async Rust").await?;
///
/// // Anthropic (explicit provider)
/// let response = completion("anthropic/claude-sonnet-4-5", "What is Rust?").await?;
///
/// // Ollama (local)
/// let response = completion("llama2", "Hello!").await?;
/// # Ok(())
/// # }
/// ```
pub async fn completion(model: &str, prompt: &str) -> Result<String> {
    // Parse model specifier
    let spec = ModelSpecifier::parse(model)?;

    // Route to appropriate provider
    let router = ProviderRouter::new();
    let provider = router.route(&spec).await?;

    // Create request
    let request = ChatRequest::new(&spec.model).with_message(Message::user(prompt));

    // Execute chat completion
    let response = provider.chat(&request).await?;

    Ok(response.content)
}

/// Execute a streaming chat completion with automatic provider detection
///
/// Returns a stream of text chunks as the model generates the response.
///
/// # Examples
///
/// ```no_run
/// use nxuskit_engine::convenience::completion_stream;
/// use futures::StreamExt;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let mut stream = completion_stream("gpt-4o", "Write a haiku about Rust").await?;
///
/// while let Some(chunk) = stream.next().await {
///     match chunk {
///         Ok(text) => print!("{}", text),
///         Err(e) => eprintln!("Error: {}", e),
///     }
/// }
/// # Ok(())
/// # }
/// ```
pub async fn completion_stream(
    model: &str,
    prompt: &str,
) -> Result<impl Stream<Item = Result<String>>> {
    // Parse model specifier
    let spec = ModelSpecifier::parse(model)?;

    // Route to appropriate provider
    let router = ProviderRouter::new();
    let provider = router.route(&spec).await?;

    // Create request
    let request = ChatRequest::new(&spec.model).with_message(Message::user(prompt));

    // Execute streaming chat completion
    let stream = provider.chat_stream(&request).await?;

    // Map StreamChunk to just the text content
    Ok(stream.map(|chunk_result| chunk_result.map(|chunk| chunk.delta)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_specifier_parse() {
        let spec = ModelSpecifier::parse("gpt-4o").unwrap();
        assert_eq!(spec.model, "gpt-4o");
        assert_eq!(spec.provider, None);

        let spec = ModelSpecifier::parse("openai/gpt-4o").unwrap();
        assert_eq!(spec.model, "gpt-4o");
        assert_eq!(spec.provider, Some(ProviderName::OpenAI));
    }

    #[test]
    fn test_env_config_detection() {
        // Test EnvConfig construction directly to avoid env var race conditions
        // in parallel tests (set_var/remove_var are not thread-safe in Rust).
        let config = EnvConfig {
            provider: ProviderName::OpenAI,
            api_key: Some("test-key".to_string()),
            base_url: None,
        };
        assert!(config.is_valid());

        let config_no_key = EnvConfig {
            provider: ProviderName::OpenAI,
            api_key: None,
            base_url: None,
        };
        assert!(!config_no_key.is_valid());
    }
}
