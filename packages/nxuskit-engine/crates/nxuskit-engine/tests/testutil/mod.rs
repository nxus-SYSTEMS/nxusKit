//! Test utilities for nxuskit_engine integration tests.
//!
//! This module provides reusable mock responses and helpers for testing
//! providers without making real API calls.
#![allow(dead_code)]

use nxuskit_engine::{ChatRequest, Message};
use wiremock::ResponseTemplate;

/// Create a simple chat request for testing.
pub fn test_chat_request() -> ChatRequest {
    let mut request = ChatRequest::new("test-model");
    request.messages.push(Message::user("Hello"));
    request
}

/// Create a chat request with a specific model.
pub fn test_chat_request_with_model(model: &str) -> ChatRequest {
    let mut request = ChatRequest::new(model);
    request.messages.push(Message::user("Hello"));
    request
}

// ============================================================================
// Claude API Mock Responses
// ============================================================================

/// Create a successful Claude chat completion response.
pub fn claude_success_response() -> ResponseTemplate {
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

/// Create a Claude response with custom content.
pub fn claude_response_with_content(content: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "id": "msg_test",
        "type": "message",
        "role": "assistant",
        "content": [{"type": "text", "text": content}],
        "model": "claude-sonnet-4-5",
        "stop_reason": "end_turn",
        "usage": {"input_tokens": 10, "output_tokens": 5}
    }))
}

/// Create a Claude response with thinking content.
pub fn claude_response_with_thinking(thinking: &str, content: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "id": "msg_test",
        "type": "message",
        "role": "assistant",
        "content": [
            {"type": "thinking", "thinking": thinking},
            {"type": "text", "text": content}
        ],
        "model": "claude-sonnet-4-5",
        "stop_reason": "end_turn",
        "usage": {"input_tokens": 10, "output_tokens": 15}
    }))
}

/// Create a Claude streaming response in SSE format.
pub fn claude_streaming_response() -> ResponseTemplate {
    let response = r#"event: message_start
data: {"type":"message_start","message":{"id":"msg_test","type":"message","role":"assistant","model":"claude-sonnet-4-20250514","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":50,"output_tokens":0}}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" world"}}

event: content_block_stop
data: {"type":"content_block_stop","index":0}

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":2}}

event: message_stop
data: {"type":"message_stop"}

"#;
    ResponseTemplate::new(200)
        .insert_header("content-type", "text/event-stream")
        .set_body_string(response)
}

/// Create a Claude streaming response with custom chunks.
pub fn claude_streaming_response_with_chunks(chunks: &[&str]) -> ResponseTemplate {
    let mut response = String::from(
        r#"event: message_start
data: {"type":"message_start","message":{"id":"msg_test","type":"message","role":"assistant","model":"claude-sonnet-4-20250514","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":50,"output_tokens":0}}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}

"#,
    );

    for chunk in chunks {
        response.push_str(&format!(
            r#"event: content_block_delta
data: {{"type":"content_block_delta","index":0,"delta":{{"type":"text_delta","text":"{}"}}}}

"#,
            chunk
        ));
    }

    response.push_str(
        r#"event: content_block_stop
data: {"type":"content_block_stop","index":0}

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":5}}

event: message_stop
data: {"type":"message_stop"}

"#,
    );

    ResponseTemplate::new(200)
        .insert_header("content-type", "text/event-stream")
        .set_body_string(response)
}

/// Create a Claude authentication error response.
pub fn claude_auth_error_response() -> ResponseTemplate {
    ResponseTemplate::new(401).set_body_json(serde_json::json!({
        "type": "error",
        "error": {
            "type": "authentication_error",
            "message": "Invalid API key"
        }
    }))
}

/// Create a Claude rate limit error response.
pub fn claude_rate_limit_response() -> ResponseTemplate {
    ResponseTemplate::new(429).set_body_json(serde_json::json!({
        "type": "error",
        "error": {
            "type": "rate_limit_error",
            "message": "Rate limit exceeded"
        }
    }))
}

/// Create a Claude invalid request error response.
pub fn claude_invalid_request_response(message: &str) -> ResponseTemplate {
    ResponseTemplate::new(400).set_body_json(serde_json::json!({
        "type": "error",
        "error": {
            "type": "invalid_request_error",
            "message": message
        }
    }))
}

// ============================================================================
// OpenAI API Mock Responses
// ============================================================================

/// Create a successful OpenAI chat completion response.
pub fn openai_success_response() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "id": "chatcmpl-test",
        "object": "chat.completion",
        "created": 1234567890,
        "model": "gpt-4o",
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": "Hello!"},
            "finish_reason": "stop"
        }],
        "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
    }))
}

/// Create an OpenAI response with custom content.
pub fn openai_response_with_content(content: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "id": "chatcmpl-test",
        "object": "chat.completion",
        "created": 1234567890,
        "model": "gpt-4o",
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": content},
            "finish_reason": "stop"
        }],
        "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
    }))
}

/// Create an OpenAI streaming response in SSE format.
pub fn openai_streaming_response() -> ResponseTemplate {
    let response = r#"data: {"id":"chatcmpl-test","object":"chat.completion.chunk","model":"gpt-4o","choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null}]}

data: {"id":"chatcmpl-test","object":"chat.completion.chunk","model":"gpt-4o","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}

data: {"id":"chatcmpl-test","object":"chat.completion.chunk","model":"gpt-4o","choices":[{"index":0,"delta":{"content":" world"},"finish_reason":null}]}

data: {"id":"chatcmpl-test","object":"chat.completion.chunk","model":"gpt-4o","choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":2,"total_tokens":12}}

data: [DONE]

"#;
    ResponseTemplate::new(200)
        .insert_header("content-type", "text/event-stream")
        .set_body_string(response)
}

/// Create an OpenAI streaming response with custom chunks.
pub fn openai_streaming_response_with_chunks(chunks: &[&str]) -> ResponseTemplate {
    let mut response = String::from(
        r#"data: {"id":"chatcmpl-test","object":"chat.completion.chunk","model":"gpt-4o","choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null}]}

"#,
    );

    for chunk in chunks {
        response.push_str(&format!(
            r#"data: {{"id":"chatcmpl-test","object":"chat.completion.chunk","model":"gpt-4o","choices":[{{"index":0,"delta":{{"content":"{}"}},"finish_reason":null}}]}}

"#,
            chunk
        ));
    }

    response.push_str(r#"data: {"id":"chatcmpl-test","object":"chat.completion.chunk","model":"gpt-4o","choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}}

data: [DONE]

"#);

    ResponseTemplate::new(200)
        .insert_header("content-type", "text/event-stream")
        .set_body_string(response)
}

/// Create an OpenAI authentication error response.
pub fn openai_auth_error_response() -> ResponseTemplate {
    ResponseTemplate::new(401).set_body_json(serde_json::json!({
        "error": {
            "message": "Invalid API key",
            "type": "invalid_request_error",
            "code": "invalid_api_key"
        }
    }))
}

/// Create an OpenAI rate limit error response.
pub fn openai_rate_limit_response() -> ResponseTemplate {
    ResponseTemplate::new(429)
        .insert_header("Retry-After", "30")
        .set_body_json(serde_json::json!({
            "error": {
                "message": "Rate limit exceeded",
                "type": "rate_limit_error"
            }
        }))
}

/// Create an OpenAI invalid request error response.
pub fn openai_invalid_request_response(message: &str) -> ResponseTemplate {
    ResponseTemplate::new(400).set_body_json(serde_json::json!({
        "error": {
            "message": message,
            "type": "invalid_request_error"
        }
    }))
}

/// Create an OpenAI models list response.
pub fn openai_models_response() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "object": "list",
        "data": [
            {"id": "gpt-4o", "object": "model", "owned_by": "openai"},
            {"id": "gpt-4o-mini", "object": "model", "owned_by": "openai"},
            {"id": "gpt-4-turbo", "object": "model", "owned_by": "openai"}
        ]
    }))
}

// ============================================================================
// Ollama API Mock Responses
// ============================================================================

/// Create a successful Ollama chat response.
pub fn ollama_success_response() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "model": "llama3.2",
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

/// Create an Ollama response with custom content.
pub fn ollama_response_with_content(content: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "model": "llama3.2",
        "created_at": "2024-01-01T00:00:00Z",
        "message": {"role": "assistant", "content": content},
        "done": true,
        "total_duration": 1000000000,
        "load_duration": 100000000,
        "prompt_eval_count": 10,
        "eval_count": 5,
        "eval_duration": 500000000
    }))
}

/// Create an Ollama response with thinking content.
pub fn ollama_response_with_thinking(thinking: &str, content: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "model": "qwen3",
        "created_at": "2024-01-01T00:00:00Z",
        "message": {"role": "assistant", "content": content},
        "thinking": thinking,
        "done": true,
        "total_duration": 1000000000,
        "load_duration": 100000000,
        "prompt_eval_count": 10,
        "eval_count": 5,
        "eval_duration": 500000000
    }))
}

/// Create an Ollama streaming response (NDJSON format).
pub fn ollama_streaming_response() -> ResponseTemplate {
    let response = r#"{"model":"llama3.2","message":{"role":"assistant","content":"Hello"},"done":false}
{"model":"llama3.2","message":{"role":"assistant","content":" world"},"done":false}
{"model":"llama3.2","message":{"role":"assistant","content":"!"},"done":true,"eval_count":3,"prompt_eval_count":5}
"#;
    ResponseTemplate::new(200).set_body_string(response)
}

/// Create an Ollama streaming response with custom chunks.
pub fn ollama_streaming_response_with_chunks(chunks: &[&str]) -> ResponseTemplate {
    let mut response = String::new();
    for (i, chunk) in chunks.iter().enumerate() {
        let done = i == chunks.len() - 1;
        if done {
            response.push_str(&format!(
                r#"{{"model":"llama3.2","message":{{"role":"assistant","content":"{}"}},"done":true,"eval_count":{},"prompt_eval_count":5}}
"#,
                chunk,
                chunks.len()
            ));
        } else {
            response.push_str(&format!(
                r#"{{"model":"llama3.2","message":{{"role":"assistant","content":"{}"}},"done":false}}
"#,
                chunk
            ));
        }
    }
    ResponseTemplate::new(200).set_body_string(response)
}

/// Create an Ollama model not found error response.
pub fn ollama_not_found_response(model: &str) -> ResponseTemplate {
    ResponseTemplate::new(404).set_body_json(serde_json::json!({
        "error": format!("model '{}' not found", model)
    }))
}

/// Create an Ollama tags (models list) response.
pub fn ollama_tags_response() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "models": [
            {
                "name": "llama3.2:latest",
                "modified_at": "2024-01-15T10:30:00Z",
                "size": 4109853696_i64,
                "digest": "sha256:abc123"
            },
            {
                "name": "mistral:7b",
                "modified_at": "2024-01-14T09:00:00Z",
                "size": 4100000000_i64,
                "digest": "sha256:def456"
            }
        ]
    }))
}

// ============================================================================
// Generic Error Responses
// ============================================================================

/// Create a server error response (500).
pub fn server_error_response() -> ResponseTemplate {
    ResponseTemplate::new(500).set_body_json(serde_json::json!({
        "error": {
            "message": "Internal server error"
        }
    }))
}

/// Create a service unavailable response (503).
pub fn service_unavailable_response() -> ResponseTemplate {
    ResponseTemplate::new(503).set_body_json(serde_json::json!({
        "error": {
            "message": "Service temporarily unavailable"
        }
    }))
}

/// Create a custom error response.
pub fn error_response(status: u16, message: &str) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(serde_json::json!({
        "error": {
            "message": message
        }
    }))
}
