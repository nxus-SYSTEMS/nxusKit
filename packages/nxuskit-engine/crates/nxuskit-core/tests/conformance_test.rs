//! Cross-language conformance tests (T079).
//!
//! Verify the JSON response structures from the C ABI match the exact
//! field names and types expected by Go (`unmarshalChatResponse`) and
//! Python (`ChatResponse.from_dict`, `StreamChunk.from_dict`).

use std::ffi::{CStr, CString};
use std::sync::{Arc, Mutex};

use nxuskit_core::*;

/// Shared state for streaming callback pair.
struct CallbackState {
    chunks: Mutex<Vec<serde_json::Value>>,
    final_json: Mutex<Option<serde_json::Value>>,
}

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

// ── Chat response conformance ─────────────────────────────────

#[test]
fn test_chat_response_has_required_fields_for_go_and_python() {
    let config = CString::new(r#"{"provider_type": "mock"}"#).unwrap();
    let provider = unsafe { nxuskit_create_provider(config.as_ptr()) };
    assert!(!provider.is_null(), "Error: {}", get_last_error());

    let request =
        CString::new(r#"{"model":"test","messages":[{"role":"user","content":"hello"}]}"#).unwrap();
    let response = unsafe { nxuskit_chat(provider, request.as_ptr()) };
    assert!(
        !response.is_null(),
        "Chat returned NULL: {}",
        get_last_error()
    );

    let json_ptr = unsafe { nxuskit_response_json(response) };
    let json_str = unsafe { CStr::from_ptr(json_ptr) }.to_str().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();

    // Go's unmarshalChatResponse expects these top-level string fields:
    assert!(
        parsed.get("content").is_some(),
        "Missing 'content' field (needed by Go + Python)"
    );
    assert!(
        parsed["content"].is_string(),
        "'content' must be a string, got: {}",
        parsed["content"]
    );

    assert!(
        parsed.get("model").is_some(),
        "Missing 'model' field (needed by Go + Python)"
    );
    assert!(parsed["model"].is_string(), "'model' must be a string");

    // Python's ChatResponse.from_dict also reads "provider"
    if let Some(provider_val) = parsed.get("provider") {
        assert!(
            provider_val.is_string(),
            "'provider' must be a string if present"
        );
    }

    unsafe {
        nxuskit_free_response(response);
        nxuskit_free_provider(provider);
    };
}

#[test]
fn test_chat_response_usage_structure() {
    let config = CString::new(r#"{"provider_type": "loopback"}"#).unwrap();
    let provider = unsafe { nxuskit_create_provider(config.as_ptr()) };
    assert!(!provider.is_null(), "Error: {}", get_last_error());

    let request =
        CString::new(r#"{"model":"echo","messages":[{"role":"user","content":"test usage"}]}"#)
            .unwrap();
    let response = unsafe { nxuskit_chat(provider, request.as_ptr()) };
    assert!(
        !response.is_null(),
        "Chat returned NULL: {}",
        get_last_error()
    );

    let json_ptr = unsafe { nxuskit_response_json(response) };
    let json_str = unsafe { CStr::from_ptr(json_ptr) }.to_str().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();

    // Verify usage field is present and structured correctly.
    // ChatResponse serializes usage as TokenUsage { estimated: TokenCount, actual?: TokenCount }
    // Wrappers (Go/Python) must read from estimated.prompt_tokens / estimated.completion_tokens,
    // or from actual.prompt_tokens / actual.completion_tokens if present.
    let usage = parsed.get("usage").expect("usage field must be present");
    assert!(usage.is_object(), "usage must be an object");

    // estimated is always present
    let estimated = usage
        .get("estimated")
        .expect("usage.estimated must be present");
    assert!(
        estimated.get("prompt_tokens").is_some(),
        "estimated missing 'prompt_tokens'"
    );
    assert!(
        estimated["prompt_tokens"].is_number(),
        "estimated.prompt_tokens must be a number"
    );
    assert!(
        estimated.get("completion_tokens").is_some(),
        "estimated missing 'completion_tokens'"
    );
    assert!(
        estimated["completion_tokens"].is_number(),
        "estimated.completion_tokens must be a number"
    );

    // actual may or may not be present (skip_serializing_if = "Option::is_none")
    if let Some(actual) = usage.get("actual") {
        assert!(
            actual.is_object(),
            "usage.actual must be an object if present"
        );
        assert!(
            actual.get("prompt_tokens").is_some(),
            "actual missing 'prompt_tokens'"
        );
        assert!(
            actual.get("completion_tokens").is_some(),
            "actual missing 'completion_tokens'"
        );
    }

    unsafe {
        nxuskit_free_response(response);
        nxuskit_free_provider(provider);
    };
}

// ── Stream chunk conformance ──────────────────────────────────

unsafe extern "C" fn on_chunk_cb(
    chunk_json: *const std::os::raw::c_char,
    user_data: *mut std::ffi::c_void,
) -> i32 {
    let state = unsafe { &*(user_data as *const CallbackState) };
    let s = unsafe { CStr::from_ptr(chunk_json) }.to_str().unwrap();
    let v: serde_json::Value = serde_json::from_str(s).unwrap();
    state.chunks.lock().unwrap().push(v);
    0
}

unsafe extern "C" fn on_done_cb(
    done_json: *const std::os::raw::c_char,
    user_data: *mut std::ffi::c_void,
) {
    let state = unsafe { &*(user_data as *const CallbackState) };
    let s = unsafe { CStr::from_ptr(done_json) }.to_str().unwrap();
    let v: serde_json::Value = serde_json::from_str(s).unwrap();
    *state.final_json.lock().unwrap() = Some(v);
}

#[test]
fn test_stream_chunks_match_go_and_python_structure() {
    let config = CString::new(r#"{"provider_type": "mock"}"#).unwrap();
    let provider = unsafe { nxuskit_create_provider(config.as_ptr()) };
    assert!(!provider.is_null(), "Error: {}", get_last_error());

    let request = CString::new(
        r#"{"model":"test","messages":[{"role":"user","content":"hi"}],"stream":true}"#,
    )
    .unwrap();

    let state = Arc::new(CallbackState {
        chunks: Mutex::new(Vec::new()),
        final_json: Mutex::new(None),
    });

    let state_ptr = Arc::into_raw(Arc::clone(&state));
    let stream = unsafe {
        nxuskit_chat_stream(
            provider,
            request.as_ptr(),
            on_chunk_cb,
            on_done_cb,
            state_ptr as *mut std::ffi::c_void,
        )
    };
    assert!(
        !stream.is_null(),
        "Stream returned NULL: {}",
        get_last_error()
    );

    // Wait for stream to complete
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Reclaim the Arc reference
    unsafe { Arc::from_raw(state_ptr) };

    let collected_chunks = state.chunks.lock().unwrap();
    let done = state.final_json.lock().unwrap();

    // Verify chunk structure matches Go + Python expectations:
    // Go: chunk.Delta (reads "delta" field via JSON tag)
    // Python: stream_chunk_from_ffi reads "delta" (with "content" fallback)
    // Canonical field name is "delta" per leadership ruling (060).
    for (i, chunk) in collected_chunks.iter().enumerate() {
        assert!(
            chunk.get("delta").is_some(),
            "Chunk {i} missing 'delta' field (canonical streaming content key)"
        );
        assert!(
            chunk["delta"].is_string(),
            "Chunk {i} 'delta' must be a string"
        );

        assert!(
            chunk.get("index").is_some(),
            "Chunk {i} missing 'index' field (needed by Python)"
        );
        assert!(
            chunk["index"].is_number(),
            "Chunk {i} 'index' must be a number"
        );
    }

    // Verify final/done JSON structure
    if let Some(ref done_val) = *done {
        assert!(
            done_val.get("content").is_some(),
            "Done JSON missing 'content' field"
        );
    }

    unsafe {
        nxuskit_free_stream(stream);
        nxuskit_free_provider(provider);
    };
}

// ── List models conformance ───────────────────────────────────

#[test]
fn test_list_models_matches_go_and_python_structure() {
    let config = CString::new(r#"{"provider_type": "mock"}"#).unwrap();
    let provider = unsafe { nxuskit_create_provider(config.as_ptr()) };
    assert!(!provider.is_null(), "Error: {}", get_last_error());

    let models_ptr = unsafe { nxuskit_list_models(provider) };
    assert!(
        !models_ptr.is_null(),
        "list_models returned NULL: {}",
        get_last_error()
    );

    let models_str = unsafe { CStr::from_ptr(models_ptr) }.to_str().unwrap();
    let models: serde_json::Value = serde_json::from_str(models_str).unwrap();

    assert!(models.is_array(), "list_models should return a JSON array");

    // Go reads: m["id"].(string), m["name"].(string)
    // Python reads: ModelInfo.from_dict reads "id", "name", "provider"
    if let Some(arr) = models.as_array() {
        for (i, model) in arr.iter().enumerate() {
            assert!(model.is_object(), "Model {i} should be an object");
            if let Some(id) = model.get("id") {
                assert!(id.is_string(), "Model {i} 'id' must be a string");
            }
            if let Some(name) = model.get("name") {
                assert!(name.is_string(), "Model {i} 'name' must be a string");
            }
        }
    }

    unsafe {
        nxuskit_free_string(models_ptr);
        nxuskit_free_provider(provider);
    };
}

// ── Error response conformance ────────────────────────────────

#[test]
fn test_error_response_structure_matches_wrappers() {
    let config = CString::new(r#"{"provider_type": "mock"}"#).unwrap();
    let provider = unsafe { nxuskit_create_provider(config.as_ptr()) };
    assert!(!provider.is_null());

    // Missing "model" should trigger an error
    let request = CString::new(r#"{"messages":[{"role":"user","content":"test"}]}"#).unwrap();
    let response = unsafe { nxuskit_chat(provider, request.as_ptr()) };

    if response.is_null() {
        // Error in thread-local — verify it's set
        let err_ptr = nxuskit_last_error();
        assert!(!err_ptr.is_null(), "Both response and last_error are NULL");
        let err_str = unsafe { CStr::from_ptr(err_ptr) }.to_str().unwrap();
        // Should be a non-empty error string
        assert!(!err_str.is_empty(), "last_error should not be empty");
    } else {
        // Error embedded in response JSON
        let json_ptr = unsafe { nxuskit_response_json(response) };
        let json_str = unsafe { CStr::from_ptr(json_ptr) }.to_str().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();

        // Go + Python check for "error" object with "error_type" and "message"
        if let Some(error) = parsed.get("error") {
            assert!(
                error.get("error_type").is_some(),
                "Error object missing 'error_type'"
            );
            assert!(
                error.get("message").is_some(),
                "Error object missing 'message'"
            );
        }

        unsafe { nxuskit_free_response(response) };
    }

    unsafe { nxuskit_free_provider(provider) };
}

// ── Zero-wrapper-code provider propagation (T082) ─────────────

/// Demonstrates that any provider added to the Rust core automatically
/// works through the C ABI without wrapper changes. The "loopback"
/// provider is used as the test vehicle — Go/Python wrappers don't have
/// special-purpose code for it, yet it works through the generic FFI path.
#[test]
fn test_provider_propagation_through_ffi_without_wrapper_changes() {
    // Loopback is in the Rust core's provider.rs match table.
    // Go and Python wrappers have no loopback-specific code — only the
    // generic ffiProvider/FFIProvider handles it.
    let config = CString::new(r#"{"provider_type": "loopback"}"#).unwrap();
    let provider = unsafe { nxuskit_create_provider(config.as_ptr()) };
    assert!(!provider.is_null(), "Error: {}", get_last_error());

    // 1. Chat works through generic path
    let request = CString::new(
        r#"{"model":"echo","messages":[{"role":"user","content":"propagation test"}]}"#,
    )
    .unwrap();
    let response = unsafe { nxuskit_chat(provider, request.as_ptr()) };
    assert!(!response.is_null(), "Chat failed: {}", get_last_error());

    let json_ptr = unsafe { nxuskit_response_json(response) };
    let json_str = unsafe { CStr::from_ptr(json_ptr) }.to_str().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();
    assert!(
        parsed["content"]
            .as_str()
            .unwrap_or("")
            .contains("propagation test"),
        "Loopback should echo content through generic FFI path"
    );

    // 2. list_models works through generic path
    let models_ptr = unsafe { nxuskit_list_models(provider) };
    assert!(
        !models_ptr.is_null(),
        "list_models failed: {}",
        get_last_error()
    );
    let models_str = unsafe { CStr::from_ptr(models_ptr) }.to_str().unwrap();
    let models: serde_json::Value = serde_json::from_str(models_str).unwrap();
    assert!(models.is_array(), "list_models must return array");

    unsafe {
        nxuskit_free_string(models_ptr);
        nxuskit_free_response(response);
        nxuskit_free_provider(provider);
    };
}
