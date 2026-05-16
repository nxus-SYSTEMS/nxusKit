//! Integration tests for synchronous chat via the C ABI.

use std::ffi::{CStr, CString};

use nxuskit_core::*;

#[test]
fn test_chat_with_mock_returns_valid_json_response() {
    let config = CString::new(r#"{"provider_type": "mock"}"#).unwrap();
    let provider = unsafe { nxuskit_create_provider(config.as_ptr()) };
    assert!(!provider.is_null());

    let request =
        CString::new(r#"{"model":"test","messages":[{"role":"user","content":"Say hello"}]}"#)
            .unwrap();
    let response = unsafe { nxuskit_chat(provider, request.as_ptr()) };
    assert!(!response.is_null());

    let json_ptr = unsafe { nxuskit_response_json(response) };
    assert!(!json_ptr.is_null());

    let json_str = unsafe { CStr::from_ptr(json_ptr) }.to_str().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();

    // Response should have content field
    assert!(
        parsed.get("content").is_some(),
        "Expected 'content' in response: {json_str}"
    );

    unsafe {
        nxuskit_free_response(response);
        nxuskit_free_provider(provider);
    };
}

#[test]
fn test_chat_with_loopback_echoes_content() {
    let config = CString::new(r#"{"provider_type": "loopback"}"#).unwrap();
    let provider = unsafe { nxuskit_create_provider(config.as_ptr()) };
    assert!(!provider.is_null());

    // LoopbackProvider requires model="echo" to echo back the last user message
    let request =
        CString::new(r#"{"model":"echo","messages":[{"role":"user","content":"echo this"}]}"#)
            .unwrap();
    let response = unsafe { nxuskit_chat(provider, request.as_ptr()) };
    assert!(!response.is_null());

    let json_ptr = unsafe { nxuskit_response_json(response) };
    let json_str = unsafe { CStr::from_ptr(json_ptr) }.to_str().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();

    // Loopback "echo" model echoes back the last message content
    let content = parsed["content"].as_str().unwrap_or("");
    assert!(
        content.contains("echo this"),
        "Expected loopback to echo content, got: {content}"
    );

    unsafe {
        nxuskit_free_response(response);
        nxuskit_free_provider(provider);
    };
}

#[test]
fn test_chat_with_loopback_returns_synthetic_logprobs_when_requested() {
    let config = CString::new(r#"{"provider_type": "loopback"}"#).unwrap();
    let provider = unsafe { nxuskit_create_provider(config.as_ptr()) };
    assert!(!provider.is_null());

    let request = CString::new(
        r#"{"model":"echo","messages":[{"role":"user","content":"Paris."}],"logprobs":true,"top_logprobs":5}"#,
    )
    .unwrap();
    let response = unsafe { nxuskit_chat(provider, request.as_ptr()) };
    assert!(!response.is_null());

    let json_ptr = unsafe { nxuskit_response_json(response) };
    let json_str = unsafe { CStr::from_ptr(json_ptr) }.to_str().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();

    assert_eq!(parsed["content"], "Paris.");
    assert_eq!(parsed["logprobs"]["content"][0]["token"], "Paris");
    assert_eq!(parsed["logprobs"]["content"][0]["logprob"], -0.01);
    assert_eq!(
        parsed["logprobs"]["content"][0]["top_logprobs"][0]["token"],
        "Lyon"
    );
    assert_eq!(
        parsed["logprobs"]["content"][0]["top_logprobs"][0]["logprob"],
        -3.2
    );

    unsafe {
        nxuskit_free_response(response);
        nxuskit_free_provider(provider);
    };
}

#[test]
fn test_chat_invalid_request_returns_null_with_error() {
    let config = CString::new(r#"{"provider_type": "mock"}"#).unwrap();
    let provider = unsafe { nxuskit_create_provider(config.as_ptr()) };
    assert!(!provider.is_null());

    // Missing required "model" field
    let request = CString::new(r#"{"messages":[]}"#).unwrap();
    let response = unsafe { nxuskit_chat(provider, request.as_ptr()) };
    assert!(response.is_null(), "Expected NULL for invalid request");

    let err_ptr = nxuskit_last_error();
    assert!(!err_ptr.is_null());
    let err = unsafe { CStr::from_ptr(err_ptr) }.to_str().unwrap();
    assert!(
        err.contains("model") || err.contains("parse"),
        "Error should mention missing model: {err}"
    );

    unsafe { nxuskit_free_provider(provider) };
}

#[test]
fn test_chat_response_json_is_rereadable() {
    let config = CString::new(r#"{"provider_type": "mock"}"#).unwrap();
    let provider = unsafe { nxuskit_create_provider(config.as_ptr()) };
    assert!(!provider.is_null());

    let request =
        CString::new(r#"{"model":"test","messages":[{"role":"user","content":"hi"}]}"#).unwrap();
    let response = unsafe { nxuskit_chat(provider, request.as_ptr()) };
    assert!(!response.is_null());

    // Read response JSON twice — pointer should be stable
    let ptr1 = unsafe { nxuskit_response_json(response) };
    let ptr2 = unsafe { nxuskit_response_json(response) };
    assert_eq!(ptr1, ptr2, "response_json should return a stable pointer");

    let json1 = unsafe { CStr::from_ptr(ptr1) }.to_str().unwrap();
    let json2 = unsafe { CStr::from_ptr(ptr2) }.to_str().unwrap();
    assert_eq!(json1, json2);

    unsafe {
        nxuskit_free_response(response);
        nxuskit_free_provider(provider);
    };
}

#[test]
fn test_chat_multiple_messages() {
    let config = CString::new(r#"{"provider_type": "mock"}"#).unwrap();
    let provider = unsafe { nxuskit_create_provider(config.as_ptr()) };
    assert!(!provider.is_null());

    // Multi-turn conversation
    let request = CString::new(
        r#"{"model":"test","messages":[{"role":"system","content":"You are helpful"},{"role":"user","content":"hello"},{"role":"assistant","content":"Hi there"},{"role":"user","content":"thanks"}]}"#,
    )
    .unwrap();
    let response = unsafe { nxuskit_chat(provider, request.as_ptr()) };
    assert!(!response.is_null());

    let json_ptr = unsafe { nxuskit_response_json(response) };
    let json_str = unsafe { CStr::from_ptr(json_ptr) }.to_str().unwrap();
    let _parsed: serde_json::Value =
        serde_json::from_str(json_str).expect("Multi-message response should be valid JSON");

    unsafe {
        nxuskit_free_response(response);
        nxuskit_free_provider(provider);
    };
}
