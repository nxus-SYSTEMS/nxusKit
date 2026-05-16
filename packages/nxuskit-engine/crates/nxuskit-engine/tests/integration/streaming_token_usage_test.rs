//! Integration tests for stream_with_usage() convenience method
//!
//! Tests the stream_with_usage() method across different provider implementations
//! to ensure token usage is properly captured and returned after stream completion.
#![allow(clippy::panic, clippy::print_stdout, clippy::print_stderr)]

use futures::StreamExt;
use nxuskit_engine::prelude::*;

#[tokio::test]
async fn test_stream_with_usage_mock_provider() {
    // Test with Mock provider which has predictable token usage
    let provider = MockProvider::default();
    let request = ChatRequest::new("Hello, world!");

    let result = provider.stream_with_usage(&request).await;
    assert!(
        result.is_ok(),
        "stream_with_usage should succeed with mock provider"
    );

    let (stream, usage_rx) = result.unwrap();

    // Consume the stream
    let mut stream = Box::pin(stream);
    let mut chunks = Vec::new();

    while let Some(result) = stream.next().await {
        match result {
            Ok(chunk) => chunks.push(chunk),
            Err(e) => panic!("Stream error: {}", e),
        }
    }

    // Verify we got chunks
    assert!(!chunks.is_empty(), "Should have received stream chunks");

    // Verify we got the final usage from the channel
    let usage = usage_rx
        .await
        .expect("usage_rx should provide token usage after stream completes");

    // Verify usage structure
    assert!(
        usage.estimated.prompt_tokens > 0,
        "Mock provider should estimate prompt tokens"
    );
    assert!(
        usage.estimated.completion_tokens > 0,
        "Mock provider should estimate completion tokens"
    );

    // Mock provider returns actual counts
    assert!(
        usage.actual.is_some(),
        "Mock provider should return actual token counts"
    );

    let actual = usage.actual.expect("actual should be Some");
    assert_eq!(
        actual.prompt_tokens, 10,
        "Mock provider uses fixed 10 prompt tokens"
    );
    assert_eq!(
        actual.completion_tokens, 20,
        "Mock provider uses fixed 20 completion tokens"
    );
}

#[tokio::test]
async fn test_stream_with_usage_final_chunk_has_usage() {
    // Test that the final chunk in the stream has usage information
    let provider = MockProvider::default();
    let request = ChatRequest::new("Test message");

    let (stream, _usage_rx) = provider
        .stream_with_usage(&request)
        .await
        .expect("stream_with_usage should succeed");

    let mut stream = Box::pin(stream);
    let mut last_chunk: Option<StreamChunk> = None;

    while let Some(result) = stream.next().await {
        if let Ok(chunk) = result {
            last_chunk = Some(chunk);
        }
    }

    // Verify the last chunk had usage
    let last_chunk = last_chunk.expect("Should have received at least one chunk");
    assert!(
        last_chunk.usage.is_some(),
        "Final chunk should have usage information"
    );
    assert!(
        last_chunk.is_final(),
        "Last chunk should be marked as final"
    );
}

#[tokio::test]
async fn test_stream_with_usage_running_totals() {
    // Test that usage grows monotonically as we consume the stream
    let provider = MockProvider::default();
    let request = ChatRequest::new("Stream with growing tokens");

    let (stream, _usage_rx) = provider
        .stream_with_usage(&request)
        .await
        .expect("stream_with_usage should succeed");

    let mut stream = Box::pin(stream);
    let mut last_completion_tokens: u32 = 0;
    let mut chunks_with_usage = 0;

    while let Some(result) = stream.next().await {
        if let Ok(chunk) = result
            && let Some(usage) = &chunk.usage
        {
            let current_completion = usage.estimated.completion_tokens;
            // Completion tokens should be non-decreasing (or first chunk)
            assert!(
                current_completion >= last_completion_tokens,
                "Completion tokens should not decrease in running totals"
            );
            last_completion_tokens = current_completion;
            chunks_with_usage += 1;
        }
    }

    // Verify we got usage in multiple chunks (running totals)
    assert!(
        chunks_with_usage > 0,
        "Should have received chunks with usage information"
    );
}

#[tokio::test]
async fn test_stream_with_usage_has_actual_for_mock() {
    // Test that Mock provider (which supports actual counts) populates actual field
    let provider = MockProvider::default();
    let request = ChatRequest::new("Check actual counts");

    let (stream, usage_rx) = provider
        .stream_with_usage(&request)
        .await
        .expect("stream_with_usage should succeed");

    let mut stream = Box::pin(stream);
    while let Some(result) = stream.next().await {
        match result {
            Ok(_chunk) => {
                // Consume stream
            }
            Err(e) => panic!("Stream error: {}", e),
        }
    }

    let usage = usage_rx
        .await
        .expect("usage_rx should provide usage after stream");

    // Mock provider supports actual counts (native support)
    assert!(
        usage.has_actual(),
        "Mock provider should have actual token counts"
    );

    let actual = usage.actual.expect("actual should be populated");
    let estimated = usage.estimated;

    // For mock provider, verify we have both actual and estimated
    assert_eq!(
        actual.prompt_tokens, estimated.prompt_tokens,
        "For mock provider, actual and estimated should match"
    );
}

#[tokio::test]
async fn test_stream_with_usage_multiple_streams() {
    // Test that multiple concurrent streams work independently
    let provider = MockProvider::default();

    let request1 = ChatRequest::new("First message");
    let request2 = ChatRequest::new("Second message");

    let (stream1, usage_rx1) = provider
        .stream_with_usage(&request1)
        .await
        .expect("First stream should succeed");

    let (stream2, usage_rx2) = provider
        .stream_with_usage(&request2)
        .await
        .expect("Second stream should succeed");

    // Consume both streams
    let mut stream1 = Box::pin(stream1);
    let mut stream2 = Box::pin(stream2);

    while let Some(result) = stream1.next().await {
        if let Err(e) = result {
            panic!("Stream 1 error: {}", e);
        }
    }

    while let Some(result) = stream2.next().await {
        if let Err(e) = result {
            panic!("Stream 2 error: {}", e);
        }
    }

    // Get both usages
    let usage1 = usage_rx1
        .await
        .expect("usage_rx1 should provide usage after stream 1");
    let usage2 = usage_rx2
        .await
        .expect("usage_rx2 should provide usage after stream 2");

    // Both should have valid usage
    assert!(
        usage1.estimated.total() > 0,
        "First stream should have usage"
    );
    assert!(
        usage2.estimated.total() > 0,
        "Second stream should have usage"
    );
}

#[tokio::test]
async fn test_stream_with_usage_best_available() {
    // Test that best_available() works correctly with actual + estimated
    let provider = MockProvider::default();
    let request = ChatRequest::new("Best available test");

    let (stream, usage_rx) = provider
        .stream_with_usage(&request)
        .await
        .expect("stream_with_usage should succeed");

    let mut stream = Box::pin(stream);
    while let Some(result) = stream.next().await {
        if let Err(e) = result {
            panic!("Stream error: {}", e);
        }
    }

    let usage = usage_rx
        .await
        .expect("usage_rx should provide usage after stream");

    // Test best_available() method
    let best = usage.best_available();

    // For mock provider with actual counts, best_available should return actual
    if usage.has_actual() {
        let actual = usage.actual.expect("actual should exist");
        assert_eq!(
            best.prompt_tokens, actual.prompt_tokens,
            "best_available should return actual when available"
        );
    }

    // total_tokens() convenience method should work
    assert!(
        best.total() > 0,
        "best_available().total() should return positive token count"
    );
}
