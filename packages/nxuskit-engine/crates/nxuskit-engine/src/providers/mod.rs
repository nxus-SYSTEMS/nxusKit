//! Provider implementations for various LLM services
//!
//! This module contains implementations of the `LLMProvider` trait for various
//! backends including cloud APIs, local models, and expert systems.
//!
//! # Available Providers
//!
//! | Provider | Description | Feature |
//! |----------|-------------|---------|
//! | `ClaudeProvider` | Anthropic Claude API | (default) |
//! | `OpenAIProvider` | OpenAI API | (default) |
//! | `OllamaProvider` | Local Ollama models | (default) |
//! | `LmStudioProvider` | LM Studio local server | (default) |
//! | `MockProvider` | Testing mock provider | (default) |
//! | `McpProvider` | Model Context Protocol | (default) |
//! | `ClipsProvider` | CLIPS expert system | (default) |
//! | `LocalRuntimeProvider` | In-process LLM inference | `provider-local-llama` or `provider-local-mistralrs` |

use std::time::Duration;

use crate::{
    capabilities::{ValidationOutcome, registry, validate_typed_request_parts},
    error::{NxuskitError, Result},
    types::{ChatRequest, ParameterWarning, WarningSeverity},
};
use reqwest::header::HeaderMap;

pub mod claude;
pub mod fireworks;
pub mod groq;
pub mod lmstudio;
pub mod loopback;
pub mod mistral;
pub mod mock;
pub mod ollama;
pub mod openai;
pub mod openrouter;
pub mod perplexity;
pub mod together;
pub mod xai;

pub mod bayesian;
pub(crate) mod candidate;
pub mod clips;
pub mod mcp;

// Feature-gated providers: new inference backends
#[cfg(any(feature = "provider-local-llama", feature = "provider-local-mistralrs"))]
pub mod local;


#[path = "zen_stub.rs"]
pub mod zen;

pub use claude::ClaudeProvider;
pub use fireworks::FireworksProvider;
pub use groq::GroqProvider;
pub use lmstudio::LmStudioProvider;
pub use loopback::LoopbackProvider;
pub use mistral::MistralProvider;
pub use mock::MockProvider;
pub use ollama::OllamaProvider;
pub use openai::OpenAIProvider;
pub use openrouter::OpenRouterProvider;
pub use perplexity::PerplexityProvider;
pub use together::TogetherProvider;
pub use xai::XaiProvider;

pub use bayesian::{BayesianProvider, BayesianProviderBuilder};
pub use clips::ClipsProvider;
pub use mcp::{McpContent, McpProvider, McpResourceInfo, McpToolInfo, McpToolResult};

#[cfg(any(feature = "provider-local-llama", feature = "provider-local-mistralrs"))]
pub use local::LocalRuntimeProvider;


/// Validate Phase 4 typed request surfaces before provider request
/// serialization. Blocking outcomes become `InvalidRequest`; warning
/// outcomes are appended to the provider response warning stream.
pub(crate) fn validate_typed_capability_request(
    provider_id: &str,
    request: &ChatRequest,
    warnings: &mut Vec<ParameterWarning>,
) -> Result<()> {
    let record = registry::find(provider_id).ok_or_else(|| {
        NxuskitError::Configuration(format!(
            "provider capability record not found: {provider_id}"
        ))
    })?;

    for outcome in validate_typed_request_parts(
        &record,
        request.structured_output.as_ref(),
        request.tool_call_config.as_ref(),
    ) {
        match outcome {
            ValidationOutcome::Allow => {}
            ValidationOutcome::Warn { feature, reason } => {
                warnings.push(ParameterWarning {
                    parameter: feature.to_string(),
                    message: reason,
                    severity: WarningSeverity::Warning,
                });
            }
            ValidationOutcome::Block { feature, reason } => {
                return Err(NxuskitError::InvalidRequest(format!("{feature}: {reason}")));
            }
        }
    }

    Ok(())
}

/// Build an HTTP client with properly configured timeouts.
///
/// This helper function ensures that timeout configurations are actually applied
/// to the `reqwest::Client`, not just stored in struct fields.
///
/// # Arguments
///
/// * `connection_timeout` - Maximum time to establish a TCP connection
/// * `read_timeout` - Maximum time between receiving data chunks (critical for streaming)
/// * `total_timeout` - Maximum total time for the entire request (includes connection + response)
///
/// # Timeout Behavior
///
/// - `connection_timeout`: Applied during TCP handshake only
/// - `read_timeout`: Applied per-chunk during response body reading (resets after each successful read)
/// - `total_timeout`: Applied to the entire request lifecycle
///
/// For streaming responses, `read_timeout` is particularly important as it allows
/// time between chunks while the LLM is "thinking".
///
/// # Errors
///
/// Returns `NxuskitError::Configuration` if the client cannot be built (e.g., invalid TLS config)
///
/// # Example
///
/// ```rust,ignore
/// use std::time::Duration;
/// use nxuskit_engine::providers::build_http_client;
///
/// let client = build_http_client(
///     Duration::from_secs(10),   // connection timeout
///     Duration::from_secs(120),  // read timeout for streaming chunks
///     Duration::from_secs(600),  // total request timeout
/// )?;
/// ```
pub fn build_http_client(
    connection_timeout: Duration,
    read_timeout: Duration,
    total_timeout: Duration,
) -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(connection_timeout)
        .read_timeout(read_timeout)
        .timeout(total_timeout)
        .build()
        .map_err(|e| NxuskitError::Configuration(format!("Failed to build HTTP client: {}", e)))
}

/// Parse the `Retry-After` header from HTTP response headers.
///
/// The `Retry-After` header can be in two formats:
/// 1. Delay-seconds: A non-negative integer representing seconds to wait (e.g., "120")
/// 2. HTTP-date: A date/time after which to retry (e.g., "Wed, 21 Oct 2025 07:28:00 GMT")
///
/// This implementation currently only supports the delay-seconds format, which is what
/// most LLM providers (OpenAI, Anthropic, etc.) use. HTTP-date format will return None.
///
/// # Arguments
///
/// * `headers` - The HTTP response headers to parse
///
/// # Returns
///
/// * `Some(Duration)` - If a valid `retry-after` header with seconds was found
/// * `None` - If the header is missing, empty, or in an unsupported format
///
/// # Example
///
/// ```rust,ignore
/// let retry_after = parse_retry_after(response.headers());
/// if let Some(duration) = retry_after {
///     println!("Retry after {} seconds", duration.as_secs());
/// }
/// ```
pub fn parse_retry_after(headers: &HeaderMap) -> Option<Duration> {
    headers
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
        .map(Duration::from_secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_http_client_success() {
        let client = build_http_client(
            Duration::from_secs(10),
            Duration::from_secs(120),
            Duration::from_secs(60),
        );
        assert!(client.is_ok(), "Client should build successfully");
    }

    #[test]
    fn test_build_http_client_with_zero_timeouts() {
        // Zero timeouts should still work (reqwest accepts them)
        let client = build_http_client(
            Duration::from_secs(0),
            Duration::from_secs(0),
            Duration::from_secs(0),
        );
        assert!(client.is_ok(), "Client should build with zero timeouts");
    }

    #[test]
    fn test_build_http_client_with_long_timeouts() {
        // Very long timeouts should work
        let client = build_http_client(
            Duration::from_secs(3600),
            Duration::from_secs(3600),
            Duration::from_secs(3600),
        );
        assert!(client.is_ok(), "Client should build with long timeouts");
    }

    #[test]
    fn test_parse_retry_after_with_seconds() {
        let mut headers = HeaderMap::new();
        headers.insert("retry-after", "30".parse().unwrap());

        let result = parse_retry_after(&headers);
        assert_eq!(result, Some(Duration::from_secs(30)));
    }

    #[test]
    fn test_parse_retry_after_with_whitespace() {
        let mut headers = HeaderMap::new();
        headers.insert("retry-after", "  60  ".parse().unwrap());

        let result = parse_retry_after(&headers);
        assert_eq!(result, Some(Duration::from_secs(60)));
    }

    #[test]
    fn test_parse_retry_after_missing_header() {
        let headers = HeaderMap::new();

        let result = parse_retry_after(&headers);
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_retry_after_invalid_value() {
        let mut headers = HeaderMap::new();
        headers.insert("retry-after", "not-a-number".parse().unwrap());

        let result = parse_retry_after(&headers);
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_retry_after_http_date_unsupported() {
        // HTTP-date format is not currently supported, should return None
        let mut headers = HeaderMap::new();
        headers.insert(
            "retry-after",
            "Wed, 21 Oct 2025 07:28:00 GMT".parse().unwrap(),
        );

        let result = parse_retry_after(&headers);
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_retry_after_zero() {
        let mut headers = HeaderMap::new();
        headers.insert("retry-after", "0".parse().unwrap());

        let result = parse_retry_after(&headers);
        assert_eq!(result, Some(Duration::from_secs(0)));
    }

    #[test]
    fn test_parse_retry_after_large_value() {
        let mut headers = HeaderMap::new();
        headers.insert("retry-after", "3600".parse().unwrap());

        let result = parse_retry_after(&headers);
        assert_eq!(result, Some(Duration::from_secs(3600)));
    }
}
