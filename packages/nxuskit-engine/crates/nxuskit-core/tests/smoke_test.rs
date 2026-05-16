//! Smoke tests for nxuskit-core C ABI.
//!
//! These tests call the FFI functions directly to verify basic lifecycle
//! operations: version retrieval, provider creation/destruction, error
//! handling, and null safety.

use std::ffi::{CStr, CString};

// Link against our crate's staticlib to resolve the `nxuskit_*` symbols.
// The integration test binary is separate from the cdylib, so we need
// an explicit extern block. Cargo links the rlib for us, but the
// `#[unsafe(no_mangle)] pub extern "C"` functions are exported
// from the rlib too.
use nxuskit_core::*;

#[test]
fn test_version_returns_valid_string() {
    let ptr = nxuskit_version();
    assert!(!ptr.is_null(), "nxuskit_version() returned NULL");

    let version = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .expect("version is not valid UTF-8");

    // Should match the workspace version (semver format)
    assert!(
        version.contains('.'),
        "version string '{version}' doesn't look like a semver"
    );
}

#[test]
fn test_create_mock_provider() {
    let config = CString::new(r#"{"provider_type": "mock"}"#).unwrap();
    let provider = unsafe { nxuskit_create_provider(config.as_ptr()) };
    assert!(
        !provider.is_null(),
        "Failed to create mock provider. Error: {}",
        get_last_error()
    );

    unsafe { nxuskit_free_provider(provider) };
}

#[test]
fn test_create_mock_provider_with_model() {
    let config = CString::new(r#"{"provider_type": "mock", "model": "test-model"}"#).unwrap();
    let provider = unsafe { nxuskit_create_provider(config.as_ptr()) };
    assert!(
        !provider.is_null(),
        "Failed to create mock provider with model. Error: {}",
        get_last_error()
    );

    unsafe { nxuskit_free_provider(provider) };
}

#[test]
fn test_create_loopback_provider() {
    let config = CString::new(r#"{"provider_type": "loopback"}"#).unwrap();
    let provider = unsafe { nxuskit_create_provider(config.as_ptr()) };
    assert!(
        !provider.is_null(),
        "Failed to create loopback provider. Error: {}",
        get_last_error()
    );

    unsafe { nxuskit_free_provider(provider) };
}

#[test]
fn test_create_provider_unknown_type_returns_null() {
    let config = CString::new(r#"{"provider_type": "nonexistent"}"#).unwrap();
    let provider = unsafe { nxuskit_create_provider(config.as_ptr()) };
    assert!(
        provider.is_null(),
        "Expected NULL for unknown provider type"
    );

    let err = get_last_error();
    assert!(
        err.contains("nonexistent"),
        "Error should mention the unknown type, got: {err}"
    );
}

#[test]
fn test_create_provider_invalid_json_returns_null() {
    let config = CString::new(r#"not valid json"#).unwrap();
    let provider = unsafe { nxuskit_create_provider(config.as_ptr()) };
    assert!(provider.is_null(), "Expected NULL for invalid JSON");

    let err = get_last_error();
    assert!(
        err.contains("parse") || err.contains("JSON"),
        "Error should mention parse failure, got: {err}"
    );
}

#[test]
fn test_create_provider_missing_type_returns_null() {
    let config = CString::new(r#"{"api_key": "test"}"#).unwrap();
    let provider = unsafe { nxuskit_create_provider(config.as_ptr()) };
    assert!(
        provider.is_null(),
        "Expected NULL for missing provider_type"
    );

    let err = get_last_error();
    assert!(
        err.contains("provider_type"),
        "Error should mention missing provider_type, got: {err}"
    );
}

#[test]
fn test_create_provider_null_config_returns_null() {
    let provider = unsafe { nxuskit_create_provider(std::ptr::null()) };
    assert!(provider.is_null(), "Expected NULL for NULL config");
}

#[test]
fn test_free_null_provider_is_safe() {
    // Should not crash or panic
    unsafe { nxuskit_free_provider(std::ptr::null_mut()) };
}

#[test]
fn test_free_null_response_is_safe() {
    unsafe { nxuskit_free_response(std::ptr::null_mut()) };
}

#[test]
fn test_free_null_string_is_safe() {
    unsafe { nxuskit_free_string(std::ptr::null_mut()) };
}

#[test]
fn test_chat_null_provider_returns_null() {
    let request = CString::new(r#"{"messages":[]}"#).unwrap();
    let response = unsafe { nxuskit_chat(std::ptr::null_mut(), request.as_ptr()) };
    assert!(response.is_null(), "Expected NULL for NULL provider");
}

#[test]
fn test_chat_null_request_returns_null() {
    let config = CString::new(r#"{"provider_type": "mock"}"#).unwrap();
    let provider = unsafe { nxuskit_create_provider(config.as_ptr()) };
    assert!(!provider.is_null());

    let response = unsafe { nxuskit_chat(provider, std::ptr::null()) };
    assert!(response.is_null(), "Expected NULL for NULL request");

    unsafe { nxuskit_free_provider(provider) };
}

#[test]
fn test_response_json_null_is_null() {
    let ptr = unsafe { nxuskit_response_json(std::ptr::null()) };
    assert!(ptr.is_null());
}

#[test]
fn test_list_models_null_provider_returns_null() {
    let ptr = unsafe { nxuskit_list_models(std::ptr::null_mut()) };
    assert!(ptr.is_null(), "Expected NULL for NULL provider");
}

#[test]
fn test_mock_provider_chat_returns_response() {
    let config = CString::new(r#"{"provider_type": "mock"}"#).unwrap();
    let provider = unsafe { nxuskit_create_provider(config.as_ptr()) };
    assert!(!provider.is_null(), "Error: {}", get_last_error());

    let request =
        CString::new(r#"{"model":"test","messages":[{"role":"user","content":"hello"}]}"#).unwrap();
    let response = unsafe { nxuskit_chat(provider, request.as_ptr()) };
    assert!(
        !response.is_null(),
        "Chat returned NULL. Error: {}",
        get_last_error()
    );

    let json_ptr = unsafe { nxuskit_response_json(response) };
    assert!(!json_ptr.is_null(), "Response JSON pointer is NULL");

    let json_str = unsafe { CStr::from_ptr(json_ptr) }
        .to_str()
        .expect("response JSON is not valid UTF-8");

    // Verify it parses as valid JSON
    let parsed: serde_json::Value =
        serde_json::from_str(json_str).expect("response is not valid JSON");

    // MockProvider should return some content
    assert!(
        parsed.get("content").is_some() || parsed.get("error").is_some(),
        "Response JSON should have 'content' or 'error' field: {json_str}"
    );

    unsafe {
        nxuskit_free_response(response);
        nxuskit_free_provider(provider);
    };
}

// Helper to read the last error message
fn get_last_error() -> String {
    let ptr = nxuskit_last_error();
    if ptr.is_null() {
        return "(no error)".to_string();
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .unwrap_or("(invalid UTF-8)")
        .to_string()
}
