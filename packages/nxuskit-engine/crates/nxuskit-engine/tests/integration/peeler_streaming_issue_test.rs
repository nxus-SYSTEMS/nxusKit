//! Tests investigating the peeler-alpha streaming error scenario
//!
//! This module reproduces the streaming error observed in peeler-alpha where
//! the stream fails mid-response with "Network error: error decoding response body"
//! after receiving ~132 chunks in ~6 seconds.
//!
//! The error is NOT a timeout issue (the v0.4.3 fix is working correctly).
//! It appears to be related to HTTP/2 stream resets or network issues.
//!
//! Related issue: peeler-alpha logs showing:
//! - Stream started at 19:11:42
//! - Error at 19:11:48 on chunk #132
//! - Error message: "Network error: error decoding response body"
#![allow(clippy::panic, clippy::print_stdout, clippy::print_stderr)]

use futures::StreamExt;
use nxuskit_engine::providers::ClaudeProvider;
use nxuskit_engine::{ChatRequest, LLMProvider, Message};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Respond, ResponseTemplate};

/// Helper to create a chat request configured for streaming
fn streaming_chat_request() -> ChatRequest {
    let mut request = ChatRequest::new("claude-sonnet-4-20250514");
    request.messages.push(Message::user("Write a detailed essay about the history of computing, covering at least 10 major milestones."));
    request.stream = true;
    request
}

/// Generate a Claude SSE streaming response with multiple chunks
/// This simulates the Claude API's Server-Sent Events format
fn generate_claude_sse_chunks(num_chunks: usize, include_error_at: Option<usize>) -> String {
    let mut response = String::new();

    // Message start event
    response.push_str("event: message_start\n");
    response.push_str("data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_test\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-sonnet-4-20250514\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":50,\"output_tokens\":0}}}\n\n");

    // Content block start
    response.push_str("event: content_block_start\n");
    response.push_str("data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n");

    // Generate content_block_delta events
    for i in 0..num_chunks {
        if include_error_at == Some(i) {
            // Simulate an error mid-stream (malformed event)
            response.push_str("event: error\n");
            response.push_str("data: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}\n\n");
            break;
        }

        let text = format!("Chunk {} of the response. ", i);
        response.push_str("event: content_block_delta\n");
        response.push_str(&format!(
            "data: {{\"type\":\"content_block_delta\",\"index\":0,\"delta\":{{\"type\":\"text_delta\",\"text\":\"{}\"}}}}\n\n",
            text
        ));
    }

    // Only add stop events if we didn't have an error
    if include_error_at.is_none() {
        // Content block stop
        response.push_str("event: content_block_stop\n");
        response.push_str("data: {\"type\":\"content_block_stop\",\"index\":0}\n\n");

        // Message delta with stop reason
        response.push_str("event: message_delta\n");
        response.push_str("data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":100}}\n\n");

        // Message stop
        response.push_str("event: message_stop\n");
        response.push_str("data: {\"type\":\"message_stop\"}\n\n");
    }

    response
}

/// Custom responder that streams chunks with configurable delays and can simulate errors
struct StreamingResponder {
    chunks: Vec<String>,
    #[allow(dead_code)]
    delay_between_chunks: Duration,
    fail_at_chunk: Option<usize>,
}

impl StreamingResponder {
    fn new(num_chunks: usize, delay_ms: u64) -> Self {
        let full_response = generate_claude_sse_chunks(num_chunks, None);
        // Split into individual SSE events
        let chunks: Vec<String> = full_response
            .split("\n\n")
            .filter(|s| !s.is_empty())
            .map(|s| format!("{}\n\n", s))
            .collect();

        Self {
            chunks,
            delay_between_chunks: Duration::from_millis(delay_ms),
            fail_at_chunk: None,
        }
    }

    fn with_failure_at(mut self, chunk_index: usize) -> Self {
        self.fail_at_chunk = Some(chunk_index);
        self
    }
}

impl Respond for StreamingResponder {
    fn respond(&self, _request: &wiremock::Request) -> ResponseTemplate {
        // For wiremock, we need to return the full body
        // Note: wiremock doesn't support true streaming simulation with delays between chunks
        // so this test focuses on the response format parsing

        let mut body = String::new();
        for (i, chunk) in self.chunks.iter().enumerate() {
            if self.fail_at_chunk == Some(i) {
                // Simulate a truncated/corrupted response
                body.push_str("event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}\n\n");
                break;
            }
            body.push_str(chunk);
        }

        ResponseTemplate::new(200)
            .insert_header("content-type", "text/event-stream")
            .insert_header("cache-control", "no-cache")
            .set_body_string(body)
    }
}

#[cfg(test)]
mod peeler_scenario_tests {
    use super::*;

    /// Test 1: Verify streaming works correctly with many chunks (baseline)
    ///
    /// This establishes that streaming works when there are no errors.
    /// Peeler was receiving ~132 chunks before failure.
    #[tokio::test]
    async fn test_streaming_many_chunks_succeeds() {
        let mock_server = MockServer::start().await;

        // Simulate 200 chunks (more than peeler's 132)
        let responder = StreamingResponder::new(200, 0);

        Mock::given(method("POST"))
            .and(path("/messages"))
            .respond_with(responder)
            .mount(&mock_server)
            .await;

        let provider = ClaudeProvider::builder()
            .api_key("test-key")
            .base_url(mock_server.uri())
            .connection_timeout(Duration::from_secs(10))
            .stream_read_timeout(Duration::from_secs(120))
            .total_timeout(Duration::from_secs(600))
            .build()
            .expect("Failed to build provider");

        let chunk_count = Arc::new(AtomicUsize::new(0));
        let chunk_count_clone = chunk_count.clone();

        let result = provider.chat_stream(&streaming_chat_request()).await;
        assert!(result.is_ok(), "Stream should start successfully");

        let mut stream = result.unwrap();
        let mut error_occurred = false;

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(_chunk) => {
                    chunk_count_clone.fetch_add(1, Ordering::SeqCst);
                }
                Err(e) => {
                    eprintln!(
                        "Stream error at chunk {}: {:?}",
                        chunk_count.load(Ordering::SeqCst),
                        e
                    );
                    error_occurred = true;
                    break;
                }
            }
        }

        let final_count = chunk_count.load(Ordering::SeqCst);
        println!("Received {} chunks", final_count);

        assert!(!error_occurred, "Stream should complete without errors");
        assert!(
            final_count > 100,
            "Should receive many chunks, got {}",
            final_count
        );
    }

    /// Test 2: Simulate the peeler scenario - error occurs mid-stream
    ///
    /// This simulates what peeler experienced: the stream was working fine,
    /// then suddenly failed at chunk ~132.
    #[tokio::test]
    async fn test_streaming_error_midstream_like_peeler() {
        let mock_server = MockServer::start().await;

        // Simulate failure at chunk 132 (like peeler)
        let responder = StreamingResponder::new(200, 0).with_failure_at(132);

        Mock::given(method("POST"))
            .and(path("/messages"))
            .respond_with(responder)
            .mount(&mock_server)
            .await;

        let provider = ClaudeProvider::builder()
            .api_key("test-key")
            .base_url(mock_server.uri())
            .connection_timeout(Duration::from_secs(10))
            .stream_read_timeout(Duration::from_secs(120))
            .total_timeout(Duration::from_secs(600))
            .build()
            .expect("Failed to build provider");

        let chunk_count = Arc::new(AtomicUsize::new(0));
        let chunk_count_clone = chunk_count.clone();

        let result = provider.chat_stream(&streaming_chat_request()).await;
        assert!(result.is_ok(), "Stream should start successfully");

        let mut stream = result.unwrap();
        let mut error_at_chunk: Option<(usize, String)> = None;

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(_chunk) => {
                    chunk_count_clone.fetch_add(1, Ordering::SeqCst);
                }
                Err(e) => {
                    let count = chunk_count.load(Ordering::SeqCst);
                    eprintln!("Stream error at chunk {}: {:?}", count, e);
                    error_at_chunk = Some((count, format!("{:?}", e)));
                    break;
                }
            }
        }

        let final_count = chunk_count.load(Ordering::SeqCst);
        println!("Received {} chunks before error/completion", final_count);

        if let Some((chunk_num, err)) = &error_at_chunk {
            println!("Error occurred at chunk {}: {}", chunk_num, err);
        }

        // We expect chunks to be received before the error event
        // The error event from the server should be parsed and handled
        assert!(final_count > 0, "Should receive some chunks before error");
    }

    /// Test 3: Test with peeler's exact timeout configuration
    ///
    /// Peeler uses: connection=10s, stream_read=120s, total=600s
    #[tokio::test]
    async fn test_with_peeler_timeout_config() {
        let mock_server = MockServer::start().await;

        // Simulate a successful long stream
        let responder = StreamingResponder::new(150, 0);

        Mock::given(method("POST"))
            .and(path("/messages"))
            .respond_with(responder)
            .mount(&mock_server)
            .await;

        // Use exact peeler timeout configuration
        let provider = ClaudeProvider::builder()
            .api_key("test-key")
            .base_url(mock_server.uri())
            .connection_timeout(Duration::from_secs(10))
            .stream_read_timeout(Duration::from_secs(120))
            .total_timeout(Duration::from_secs(600))
            .build()
            .expect("Failed to build provider");

        let start = Instant::now();
        let result = provider.chat_stream(&streaming_chat_request()).await;
        assert!(result.is_ok(), "Stream should start successfully");

        let mut stream = result.unwrap();
        let mut chunk_count = 0;
        let mut error_message: Option<String> = None;

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(_) => chunk_count += 1,
                Err(e) => {
                    error_message = Some(format!("{:?}", e));
                    break;
                }
            }
        }

        let elapsed = start.elapsed();

        println!("Streaming completed in {:?}", elapsed);
        println!("Received {} chunks", chunk_count);

        if let Some(err) = &error_message {
            println!("Error: {}", err);
        }

        assert!(chunk_count > 0, "Should receive chunks");
        assert!(
            error_message.is_none(),
            "Should not have errors with peeler config: {:?}",
            error_message
        );
    }

    /// Test 4: Test HTTP/2 vs HTTP/1.1 behavior
    ///
    /// The peeler error might be related to HTTP/2 stream handling.
    /// This test documents the current behavior.
    #[tokio::test]
    async fn test_streaming_documents_http_version() {
        let mock_server = MockServer::start().await;

        let responder = StreamingResponder::new(50, 0);

        Mock::given(method("POST"))
            .and(path("/messages"))
            .respond_with(responder)
            .mount(&mock_server)
            .await;

        let provider = ClaudeProvider::builder()
            .api_key("test-key")
            .base_url(mock_server.uri())
            .build()
            .expect("Failed to build provider");

        let result = provider.chat_stream(&streaming_chat_request()).await;

        // This test just documents that HTTP/2 is the default in reqwest 0.12
        // If issues occur, we may need to add http1_only() option
        assert!(result.is_ok(), "Stream should start");

        let mut stream = result.unwrap();
        let mut count = 0;
        while let Some(Ok(_)) = stream.next().await {
            count += 1;
        }

        println!("HTTP streaming test received {} chunks", count);
        assert!(count > 0, "Should receive chunks");
    }

    /// Test 5: Stress test with rapid chunk delivery
    ///
    /// Tests if the issue is related to chunk processing speed
    #[tokio::test]
    async fn test_streaming_rapid_chunks() {
        let mock_server = MockServer::start().await;

        // Generate a large number of chunks
        let responder = StreamingResponder::new(500, 0);

        Mock::given(method("POST"))
            .and(path("/messages"))
            .respond_with(responder)
            .mount(&mock_server)
            .await;

        let provider = ClaudeProvider::builder()
            .api_key("test-key")
            .base_url(mock_server.uri())
            .connection_timeout(Duration::from_secs(10))
            .stream_read_timeout(Duration::from_secs(120))
            .total_timeout(Duration::from_secs(600))
            .build()
            .expect("Failed to build provider");

        let start = Instant::now();
        let result = provider.chat_stream(&streaming_chat_request()).await;
        assert!(result.is_ok(), "Stream should start");

        let mut stream = result.unwrap();
        let mut chunk_count = 0;
        let mut total_bytes = 0;

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    chunk_count += 1;
                    total_bytes += chunk.delta.len();
                }
                Err(e) => {
                    eprintln!("Error at chunk {}: {:?}", chunk_count, e);
                    break;
                }
            }
        }

        let elapsed = start.elapsed();
        let chunks_per_sec = chunk_count as f64 / elapsed.as_secs_f64();

        println!(
            "Rapid streaming: {} chunks, {} bytes in {:?}",
            chunk_count, total_bytes, elapsed
        );
        println!("Rate: {:.1} chunks/sec", chunks_per_sec);

        assert!(
            chunk_count > 400,
            "Should receive most chunks, got {}",
            chunk_count
        );
    }

    /// Test 6: Test with ping events (Claude API sends these)
    ///
    /// The Claude API may send ping events to keep connections alive.
    /// This tests that they're handled correctly.
    #[tokio::test]
    async fn test_streaming_handles_ping_events() {
        let mock_server = MockServer::start().await;

        // Create a response with ping events interspersed
        let mut response = String::new();

        // Message start
        response.push_str("event: message_start\n");
        response.push_str("data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_test\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-sonnet-4-20250514\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":50,\"output_tokens\":0}}}\n\n");

        // Content block start
        response.push_str("event: content_block_start\n");
        response.push_str("data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n");

        // Add some chunks with ping events interspersed
        for i in 0..20 {
            // Every 5 chunks, send a ping
            if i % 5 == 0 && i > 0 {
                response.push_str("event: ping\n");
                response.push_str("data: {\"type\":\"ping\"}\n\n");
            }

            response.push_str("event: content_block_delta\n");
            response.push_str(&format!(
                "data: {{\"type\":\"content_block_delta\",\"index\":0,\"delta\":{{\"type\":\"text_delta\",\"text\":\"Chunk {}. \"}}}}\n\n",
                i
            ));
        }

        // Message stop
        response.push_str("event: message_stop\n");
        response.push_str("data: {\"type\":\"message_stop\"}\n\n");

        Mock::given(method("POST"))
            .and(path("/messages"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(response),
            )
            .mount(&mock_server)
            .await;

        let provider = ClaudeProvider::builder()
            .api_key("test-key")
            .base_url(mock_server.uri())
            .build()
            .expect("Failed to build provider");

        let result = provider.chat_stream(&streaming_chat_request()).await;
        assert!(result.is_ok(), "Stream should start");

        let mut stream = result.unwrap();
        let mut chunk_count = 0;

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(_) => chunk_count += 1,
                Err(e) => {
                    panic!("Stream error with ping events: {:?}", e);
                }
            }
        }

        println!(
            "Received {} text chunks (ping events should be ignored)",
            chunk_count
        );
        assert!(
            chunk_count >= 20,
            "Should receive all text chunks, got {}",
            chunk_count
        );
    }
}

#[cfg(test)]
mod error_investigation_tests {
    use super::*;

    /// Test: Capture detailed error information
    ///
    /// This test captures the full error chain to help diagnose issues.
    #[tokio::test]
    async fn test_capture_detailed_error_info() {
        let mock_server = MockServer::start().await;

        // Create a response that will cause a parsing error
        let malformed_response = r#"event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}

event: content_block_delta
data: {MALFORMED JSON HERE}

"#;

        Mock::given(method("POST"))
            .and(path("/messages"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(malformed_response),
            )
            .mount(&mock_server)
            .await;

        let provider = ClaudeProvider::builder()
            .api_key("test-key")
            .base_url(mock_server.uri())
            .build()
            .expect("Failed to build provider");

        let result = provider.chat_stream(&streaming_chat_request()).await;
        assert!(result.is_ok(), "Stream should start");

        let mut stream = result.unwrap();
        let mut errors = Vec::new();
        let mut chunks_received = 0;

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(_) => chunks_received += 1,
                Err(e) => {
                    // Capture the full error chain
                    let error_debug = format!("{:?}", e);
                    let error_display = format!("{}", e);

                    println!("=== Error Details ===");
                    println!("Display: {}", error_display);
                    println!("Debug: {}", error_debug);

                    // Check error type
                    let error_type = match &e {
                        nxuskit_engine::error::NxuskitError::Network(_) => "Network",
                        nxuskit_engine::error::NxuskitError::Stream(_) => "Stream",
                        nxuskit_engine::error::NxuskitError::Serialization(_) => "Serialization",
                        _ => "Other",
                    };
                    println!("Error type: {}", error_type);

                    errors.push((error_type.to_string(), error_display));
                }
            }
        }

        println!("\nReceived {} chunks before error(s)", chunks_received);
        println!("Total errors: {}", errors.len());

        for (i, (error_type, msg)) in errors.iter().enumerate() {
            println!("Error {}: [{}] {}", i + 1, error_type, msg);
        }

        // We expect at least one parsing error from the malformed JSON
        assert!(chunks_received >= 1, "Should receive at least one chunk");
        assert!(!errors.is_empty(), "Should capture the parsing error");
    }

    /// Test: Verify error categorization for "error decoding response body"
    ///
    /// This specifically tests what error type we get for decode errors.
    #[tokio::test]
    async fn test_decode_error_categorization() {
        // Note: It's difficult to simulate the exact "error decoding response body"
        // from reqwest without actually causing a network-level error.
        // This test documents the expected behavior.

        let mock_server = MockServer::start().await;

        // Normal response to verify baseline
        let responder = StreamingResponder::new(10, 0);

        Mock::given(method("POST"))
            .and(path("/messages"))
            .respond_with(responder)
            .mount(&mock_server)
            .await;

        let provider = ClaudeProvider::builder()
            .api_key("test-key")
            .base_url(mock_server.uri())
            .build()
            .expect("Failed to build provider");

        let result = provider.chat_stream(&streaming_chat_request()).await;
        assert!(result.is_ok(), "Stream should start");

        let mut stream = result.unwrap();
        let mut chunk_count = 0;

        while let Some(Ok(_)) = stream.next().await {
            chunk_count += 1;
        }

        assert!(chunk_count > 0, "Should receive chunks");

        // Document: "error decoding response body" errors from reqwest are mapped to
        // NxuskitError::Network(reqwest::Error) in the streaming code.
        // See nxuskit_engine/src/providers/claude.rs lines 353-354
        println!("Note: 'error decoding response body' errors are mapped to NxuskitError::Network");
    }
}

#[cfg(test)]
mod real_api_tests {
    use super::*;
    use std::env;

    /// MANUAL TEST: Run against real Claude API to reproduce peeler issue
    ///
    /// This test requires ANTHROPIC_API_KEY to be set.
    /// Run with: cargo test --test peeler_streaming_issue_test real_api -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "Requires ANTHROPIC_API_KEY and makes real API calls"]
    async fn test_real_api_streaming_like_peeler() {
        let api_key = env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY not set");

        // Use exact peeler configuration
        let provider = ClaudeProvider::builder()
            .api_key(&api_key)
            .connection_timeout(Duration::from_secs(10))
            .stream_read_timeout(Duration::from_secs(120))
            .total_timeout(Duration::from_secs(600))
            .build()
            .expect("Failed to build provider");

        // Create a request that will generate a long response
        let mut request = ChatRequest::new("claude-sonnet-4-20250514");
        request.messages.push(Message::user(
            "Write a comprehensive essay about the history of computing from the 1940s to present day. \
             Cover at least 15 major milestones including ENIAC, transistors, integrated circuits, \
             personal computers, the internet, smartphones, cloud computing, and AI. \
             For each milestone, explain its significance and impact on society. \
             This should be a detailed, multi-paragraph response."
        ));
        request.stream = true;
        request.max_tokens = Some(4000); // Request a long response

        println!("Starting streaming request to Claude API...");
        let start = Instant::now();

        let result = provider.chat_stream(&request).await;

        match result {
            Ok(mut stream) => {
                let mut chunk_count = 0;
                let mut total_chars = 0;
                let mut last_chunk_time = Instant::now();

                while let Some(chunk_result) = stream.next().await {
                    match chunk_result {
                        Ok(chunk) => {
                            chunk_count += 1;
                            total_chars += chunk.delta.len();

                            let chunk_interval = last_chunk_time.elapsed();
                            last_chunk_time = Instant::now();

                            // Log every 50 chunks
                            if chunk_count % 50 == 0 {
                                println!(
                                    "Chunk {}: {} total chars, {:.1}ms since last chunk",
                                    chunk_count,
                                    total_chars,
                                    chunk_interval.as_secs_f64() * 1000.0
                                );
                            }
                        }
                        Err(e) => {
                            let elapsed = start.elapsed();
                            println!("\n=== STREAMING ERROR ===");
                            println!("Failed at chunk #{}", chunk_count);
                            println!("Total chars received: {}", total_chars);
                            println!("Time elapsed: {:?}", elapsed);
                            println!("Error type: {:?}", e);
                            println!("Error display: {}", e);

                            // Check if it's a network error (like peeler experienced)
                            if let nxuskit_engine::error::NxuskitError::Network(ref reqwest_err) = e
                            {
                                println!("Reqwest error details: {:?}", reqwest_err);
                                if reqwest_err.is_timeout() {
                                    println!(">>> This is a TIMEOUT error");
                                }
                                if reqwest_err.is_connect() {
                                    println!(">>> This is a CONNECT error");
                                }
                                if reqwest_err.is_body() {
                                    println!(">>> This is a BODY error (likely decode issue)");
                                }
                            }

                            panic!("Stream failed: {:?}", e);
                        }
                    }
                }

                let elapsed = start.elapsed();
                println!("\n=== STREAMING COMPLETED SUCCESSFULLY ===");
                println!("Total chunks: {}", chunk_count);
                println!("Total chars: {}", total_chars);
                println!("Time elapsed: {:?}", elapsed);
                println!(
                    "Average chunks/sec: {:.1}",
                    chunk_count as f64 / elapsed.as_secs_f64()
                );
            }
            Err(e) => {
                println!("Failed to start stream: {:?}", e);
                panic!("Stream failed to start: {:?}", e);
            }
        }
    }

    /// MANUAL TEST: Run multiple streaming requests to reproduce intermittent issue
    ///
    /// Run with: cargo test --test peeler_streaming_issue_test real_api_multiple -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "Requires ANTHROPIC_API_KEY and makes real API calls"]
    async fn test_real_api_multiple_streams() {
        let api_key = env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY not set");

        let provider = ClaudeProvider::builder()
            .api_key(&api_key)
            .connection_timeout(Duration::from_secs(10))
            .stream_read_timeout(Duration::from_secs(120))
            .total_timeout(Duration::from_secs(600))
            .build()
            .expect("Failed to build provider");

        let num_requests = 5;
        let mut successes = 0;
        let mut failures = Vec::new();

        for i in 0..num_requests {
            println!("\n=== Request {} of {} ===", i + 1, num_requests);

            let mut request = ChatRequest::new("claude-sonnet-4-20250514");
            request.messages.push(Message::user(format!(
                "Request {}: Write a detailed paragraph about a random topic. Make it at least 200 words.",
                i + 1
            )));
            request.stream = true;

            match provider.chat_stream(&request).await {
                Ok(mut stream) => {
                    let mut chunk_count = 0;
                    let mut error: Option<String> = None;

                    while let Some(chunk_result) = stream.next().await {
                        match chunk_result {
                            Ok(_) => chunk_count += 1,
                            Err(e) => {
                                error = Some(format!("Chunk {}: {:?}", chunk_count, e));
                                break;
                            }
                        }
                    }

                    if let Some(err) = error {
                        println!("Request {} FAILED: {}", i + 1, err);
                        failures.push((i + 1, err));
                    } else {
                        println!("Request {} SUCCESS: {} chunks", i + 1, chunk_count);
                        successes += 1;
                    }
                }
                Err(e) => {
                    println!("Request {} FAILED to start: {:?}", i + 1, e);
                    failures.push((i + 1, format!("{:?}", e)));
                }
            }

            // Small delay between requests
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        println!("\n=== SUMMARY ===");
        println!("Successes: {}/{}", successes, num_requests);
        println!("Failures: {}", failures.len());
        for (req_num, err) in &failures {
            println!("  Request {}: {}", req_num, err);
        }

        assert!(successes > 0, "At least some requests should succeed");
    }
}
