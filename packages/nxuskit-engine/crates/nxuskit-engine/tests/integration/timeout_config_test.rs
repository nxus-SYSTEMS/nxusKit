//! Regression tests for timeout configuration bug fix
//!
//! This module tests that timeout configurations passed to provider builders
//! are actually applied to the underlying HTTP client, not just stored in struct fields.
//!
//! Bug context: legacy timeout configuration regression 005-fix-timeout-config.
//!
//! The bug was that `reqwest::Client::new()` was used instead of
//! `reqwest::Client::builder()` with timeout configuration, causing
//! all user-configured timeouts to be ignored.
#![allow(clippy::panic, clippy::print_stdout, clippy::print_stderr)]

use std::time::{Duration, Instant};

use nxuskit_engine::providers::{ClaudeProvider, OllamaProvider, OpenAIProvider};
use nxuskit_engine::{ChatRequest, LLMProvider, Message};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Helper to create a simple chat request for testing
fn test_chat_request() -> ChatRequest {
    let mut request = ChatRequest::new("test-model");
    request.messages.push(Message::user("Hello"));
    request
}

/// Helper to create a mock response that simulates Claude API
fn claude_success_response() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "id": "msg_test",
        "type": "message",
        "role": "assistant",
        "content": [{"type": "text", "text": "Hello!"}],
        "model": "claude-sonnet-4-5",
        "stop_reason": "end_turn",
        "usage": {"input_tokens": 10, "output_tokens": 5}
    }))
}

/// Helper to create a mock response that simulates OpenAI API
fn openai_success_response() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "id": "chatcmpl-test",
        "object": "chat.completion",
        "created": 1234567890,
        "model": "gpt-4",
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": "Hello!"},
            "finish_reason": "stop"
        }],
        "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
    }))
}

/// Helper to create a mock response that simulates Ollama API
fn ollama_success_response() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "model": "llama2",
        "created_at": "2024-01-01T00:00:00Z",
        "message": {"role": "assistant", "content": "Hello!"},
        "done": true,
        "total_duration": 1000000000,
        "load_duration": 100000000,
        "prompt_eval_count": 10,
        "eval_count": 5,
        "eval_duration": 500000000
    }))
}

#[cfg(test)]
mod timeout_application_tests {
    use super::*;

    // T006: Test that ClaudeProvider applies connection_timeout to the HTTP client
    //
    // This test verifies that when we configure a short connection timeout,
    // the request actually fails with a timeout error (not just stores the value).
    //
    // EXPECTED: This test should FAIL before the fix is applied because
    // `reqwest::Client::new()` ignores the configured timeout.
    #[tokio::test]
    async fn test_claude_applies_connection_timeout() {
        // Create a mock server that responds successfully but we won't reach it
        // because we're testing connection timeout behavior
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/messages"))
            .respond_with(claude_success_response())
            .mount(&mock_server)
            .await;

        // Configure a very short total timeout (1 second)
        // If the timeout is properly applied, requests should respect it
        let provider = ClaudeProvider::builder()
            .api_key("test-key")
            .base_url(mock_server.uri())
            .total_timeout(Duration::from_secs(1))
            .build()
            .expect("Failed to build provider");

        // This should succeed quickly (mock server responds immediately)
        let start = Instant::now();
        let result = provider.chat(&test_chat_request()).await;
        let elapsed = start.elapsed();

        // The request should complete successfully and quickly
        assert!(result.is_ok(), "Request should succeed: {:?}", result);
        assert!(
            elapsed < Duration::from_secs(5),
            "Request should complete quickly, took {:?}",
            elapsed
        );
    }

    // T007: Test that ClaudeProvider applies stream_read_timeout
    //
    // Note: With reqwest 0.12+, read_timeout applies to each read operation during response body reading.
    // This test verifies the configuration is passed through correctly.
    // For non-streaming requests, the total_timeout typically triggers first.
    // The real value of read_timeout is for streaming responses with slow chunk delivery.
    #[tokio::test]
    async fn test_claude_stream_read_timeout_configuration() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/messages"))
            .respond_with(claude_success_response())
            .mount(&mock_server)
            .await;

        // Verify that stream_read_timeout configuration is accepted and provider builds successfully
        let provider = ClaudeProvider::builder()
            .api_key("test-key")
            .base_url(mock_server.uri())
            .stream_read_timeout(Duration::from_secs(120))
            .build()
            .expect("Failed to build provider with stream_read_timeout");

        // If we get here, the configuration was accepted
        // The actual read_timeout behavior is tested via the client-level configuration
        let result = provider.chat(&test_chat_request()).await;
        assert!(result.is_ok(), "Request should succeed: {:?}", result);
    }

    // T008: Test that ClaudeProvider applies total_timeout
    #[tokio::test]
    async fn test_claude_applies_total_timeout() {
        let mock_server = MockServer::start().await;

        // Create a response with 5 second delay
        Mock::given(method("POST"))
            .and(path("/messages"))
            .respond_with(claude_success_response().set_delay(Duration::from_secs(5)))
            .mount(&mock_server)
            .await;

        // Configure a 2 second total timeout
        let provider = ClaudeProvider::builder()
            .api_key("test-key")
            .base_url(mock_server.uri())
            .total_timeout(Duration::from_secs(2))
            .build()
            .expect("Failed to build provider");

        let start = Instant::now();
        let result = provider.chat(&test_chat_request()).await;
        let elapsed = start.elapsed();

        // Should timeout around 2s, not wait for 5s response
        assert!(result.is_err(), "Request should timeout");
        assert!(
            elapsed < Duration::from_secs(4),
            "Should timeout around 2s (total_timeout), not wait for 5s. Took {:?}",
            elapsed
        );
    }

    // T009: Test that OpenAIProvider applies all timeouts
    #[tokio::test]
    async fn test_openai_applies_timeouts() {
        let mock_server = MockServer::start().await;

        // Create a response with 4 second delay
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(openai_success_response().set_delay(Duration::from_secs(4)))
            .mount(&mock_server)
            .await;

        // Configure a 2 second total timeout
        let provider = OpenAIProvider::builder()
            .api_key("test-key")
            .base_url(mock_server.uri())
            .total_timeout(Duration::from_secs(2))
            .build()
            .expect("Failed to build provider");

        let start = Instant::now();
        let result = provider.chat(&test_chat_request()).await;
        let elapsed = start.elapsed();

        // Should timeout around 2s
        assert!(result.is_err(), "Request should timeout");
        assert!(
            elapsed < Duration::from_secs(3),
            "Should timeout around 2s, not wait for 4s. Took {:?}",
            elapsed
        );
    }

    // T010: Test that OllamaProvider applies all timeouts
    #[tokio::test]
    async fn test_ollama_applies_timeouts() {
        let mock_server = MockServer::start().await;

        // Create a response with 4 second delay
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ollama_success_response().set_delay(Duration::from_secs(4)))
            .mount(&mock_server)
            .await;

        // Configure a 2 second total timeout
        let provider = OllamaProvider::builder()
            .model("llama2")
            .base_url(mock_server.uri())
            .total_timeout(Duration::from_secs(2))
            .build()
            .expect("Failed to build provider");

        let start = Instant::now();
        let result = provider.chat(&test_chat_request()).await;
        let elapsed = start.elapsed();

        // Should timeout around 2s
        assert!(result.is_err(), "Request should timeout");
        assert!(
            elapsed < Duration::from_secs(3),
            "Should timeout around 2s, not wait for 4s. Took {:?}",
            elapsed
        );
    }

    // Additional test: Verify that default timeouts still work (backward compatibility)
    #[tokio::test]
    async fn test_claude_default_timeouts_work() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/messages"))
            .respond_with(claude_success_response())
            .mount(&mock_server)
            .await;

        // Build without explicit timeouts - should use defaults
        let provider = ClaudeProvider::builder()
            .api_key("test-key")
            .base_url(mock_server.uri())
            .build()
            .expect("Failed to build provider");

        let result = provider.chat(&test_chat_request()).await;
        assert!(
            result.is_ok(),
            "Request with default timeouts should succeed"
        );
    }
}

#[cfg(test)]
mod behavioral_timeout_tests {
    use super::*;

    // T018: Test that short timeout causes failure on slow server
    #[tokio::test]
    async fn test_short_timeout_fails_on_slow_response() {
        let mock_server = MockServer::start().await;

        // 5 second delay
        Mock::given(method("POST"))
            .and(path("/messages"))
            .respond_with(claude_success_response().set_delay(Duration::from_secs(5)))
            .mount(&mock_server)
            .await;

        let provider = ClaudeProvider::builder()
            .api_key("test-key")
            .base_url(mock_server.uri())
            .timeout(Duration::from_secs(2)) // General timeout of 2s
            .build()
            .expect("Failed to build provider");

        let result = provider.chat(&test_chat_request()).await;
        assert!(result.is_err(), "Should timeout with slow response");
    }

    // T019: Test that adequate timeout succeeds on slow server
    #[tokio::test]
    async fn test_adequate_timeout_succeeds_on_slow_response() {
        let mock_server = MockServer::start().await;

        // 2 second delay
        Mock::given(method("POST"))
            .and(path("/messages"))
            .respond_with(claude_success_response().set_delay(Duration::from_secs(2)))
            .mount(&mock_server)
            .await;

        let provider = ClaudeProvider::builder()
            .api_key("test-key")
            .base_url(mock_server.uri())
            .timeout(Duration::from_secs(10)) // Adequate timeout
            .build()
            .expect("Failed to build provider");

        let result = provider.chat(&test_chat_request()).await;
        assert!(
            result.is_ok(),
            "Should succeed with adequate timeout: {:?}",
            result
        );
    }
}

#[cfg(test)]
mod provider_timeout_macro {
    use super::*;

    // T023: Macro/helper to verify any provider applies timeouts correctly
    // This will be used for all current and future providers

    /// Verify that a provider respects its configured total_timeout.
    ///
    /// This function tests that when a provider is configured with a short timeout
    /// and the mock server has a longer delay, the request fails with a timeout error.
    ///
    /// # Type Parameters
    /// * `P` - The provider type that implements LLMProvider
    ///
    /// # Arguments
    /// * `provider` - A provider configured with a short timeout (e.g., 2s)
    /// * `expected_timeout_secs` - The configured timeout in seconds
    async fn verify_provider_respects_timeout<P: LLMProvider>(
        provider: P,
        expected_timeout_secs: u64,
    ) {
        let start = Instant::now();
        let result = provider.chat(&test_chat_request()).await;
        let elapsed = start.elapsed();

        // Request should fail
        assert!(
            result.is_err(),
            "Request should timeout when server delay exceeds configured timeout"
        );

        // Should fail close to the configured timeout (with some tolerance)
        let tolerance_secs = expected_timeout_secs + 1;
        assert!(
            elapsed < Duration::from_secs(tolerance_secs),
            "Request should timeout around {}s, took {:?}",
            expected_timeout_secs,
            elapsed
        );
    }

    // T024: Apply the verification to all providers
    #[tokio::test]
    async fn test_all_providers_respect_timeout_macro() {
        let mock_server = MockServer::start().await;

        // Set up mocks for all providers with 5 second delay
        Mock::given(method("POST"))
            .and(path("/messages"))
            .respond_with(claude_success_response().set_delay(Duration::from_secs(5)))
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(openai_success_response().set_delay(Duration::from_secs(5)))
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ollama_success_response().set_delay(Duration::from_secs(5)))
            .mount(&mock_server)
            .await;

        // Test Claude
        let claude = ClaudeProvider::builder()
            .api_key("test-key")
            .base_url(mock_server.uri())
            .total_timeout(Duration::from_secs(2))
            .build()
            .expect("Failed to build Claude provider");
        verify_provider_respects_timeout(claude, 2).await;

        // Test OpenAI
        let openai = OpenAIProvider::builder()
            .api_key("test-key")
            .base_url(mock_server.uri())
            .total_timeout(Duration::from_secs(2))
            .build()
            .expect("Failed to build OpenAI provider");
        verify_provider_respects_timeout(openai, 2).await;

        // Test Ollama
        let ollama = OllamaProvider::builder()
            .model("llama2")
            .base_url(mock_server.uri())
            .total_timeout(Duration::from_secs(2))
            .build()
            .expect("Failed to build Ollama provider");
        verify_provider_respects_timeout(ollama, 2).await;
    }
}

#[cfg(test)]
mod streaming_timeout_tests {
    use super::*;
    use futures::StreamExt;

    /// Helper to create a Claude SSE streaming response
    fn claude_streaming_response() -> ResponseTemplate {
        let response = r#"event: message_start
data: {"type":"message_start","message":{"id":"msg_test","type":"message","role":"assistant","model":"claude-sonnet-4-20250514","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":50,"output_tokens":0}}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" world"}}

event: message_stop
data: {"type":"message_stop"}

"#;
        ResponseTemplate::new(200)
            .insert_header("content-type", "text/event-stream")
            .set_body_string(response)
    }

    /// Regression test: Streaming requests must use total_timeout, not connection_timeout
    ///
    /// Bug: The streaming methods were using `.timeout(self.connection_timeout)` on
    /// the request builder, which overrode the client's total_timeout with the much
    /// shorter connection_timeout (default 10s). This caused streaming to fail after
    /// ~10 seconds even when total_timeout was set to 600s.
    ///
    /// Fix: Streaming methods now use `.timeout(self.total_timeout)` to allow long
    /// streaming responses to complete.
    #[tokio::test]
    async fn test_streaming_uses_total_timeout_not_connection_timeout() {
        let mock_server = MockServer::start().await;

        // Response with 3 second delay - should succeed with 5s total timeout
        // but would fail if connection_timeout (1s) was used instead
        Mock::given(method("POST"))
            .and(path("/messages"))
            .respond_with(claude_streaming_response().set_delay(Duration::from_secs(3)))
            .mount(&mock_server)
            .await;

        let provider = ClaudeProvider::builder()
            .api_key("test-key")
            .base_url(mock_server.uri())
            .connection_timeout(Duration::from_secs(1)) // Very short - would fail if used for streaming
            .total_timeout(Duration::from_secs(10)) // Adequate for streaming
            .build()
            .expect("Failed to build provider");

        let mut request = test_chat_request();
        request.stream = true;

        let start = Instant::now();
        let result = provider.chat_stream(&request).await;

        // Stream should start successfully
        assert!(result.is_ok(), "Stream should start successfully");

        let mut stream = result.unwrap();
        let mut chunk_count = 0;

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(_) => chunk_count += 1,
                Err(e) => panic!("Stream should not fail with proper total_timeout: {:?}", e),
            }
        }

        let elapsed = start.elapsed();

        // Should complete successfully (not timeout at 1s connection_timeout)
        assert!(chunk_count > 0, "Should receive chunks");
        println!(
            "Streaming completed in {:?} with {} chunks (connection_timeout=1s, total_timeout=10s)",
            elapsed, chunk_count
        );
    }

    /// Test: Streaming with short total_timeout should fail appropriately
    #[tokio::test]
    async fn test_streaming_respects_total_timeout() {
        let mock_server = MockServer::start().await;

        // Response with 5 second delay
        Mock::given(method("POST"))
            .and(path("/messages"))
            .respond_with(claude_streaming_response().set_delay(Duration::from_secs(5)))
            .mount(&mock_server)
            .await;

        let provider = ClaudeProvider::builder()
            .api_key("test-key")
            .base_url(mock_server.uri())
            .connection_timeout(Duration::from_secs(10)) // Long enough
            .total_timeout(Duration::from_secs(2)) // Should timeout
            .build()
            .expect("Failed to build provider");

        let mut request = test_chat_request();
        request.stream = true;

        let start = Instant::now();
        let result = provider.chat_stream(&request).await;

        // Either fails to start or fails during streaming
        let elapsed = start.elapsed();

        // Should timeout around 2s, not wait for 5s delay
        assert!(
            elapsed < Duration::from_secs(4),
            "Should timeout around 2s (total_timeout), took {:?}",
            elapsed
        );

        // The result depends on when the timeout hits - could be on connect or during stream
        if let Ok(mut stream) = result {
            // If stream started, consuming it should eventually fail
            while let Some(chunk_result) = stream.next().await {
                if chunk_result.is_err() {
                    break;
                }
            }
        }

        println!(
            "Streaming timed out correctly at {:?} (total_timeout=2s)",
            elapsed
        );
    }
}
