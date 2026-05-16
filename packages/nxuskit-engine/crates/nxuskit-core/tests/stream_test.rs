#![allow(clippy::panic)]
//! Integration tests for streaming chat via the C ABI.

use std::ffi::{CStr, CString, c_void};
use std::os::raw::c_char;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use nxuskit_core::*;

/// Callback state shared between the test thread and the callback invocations.
struct StreamState {
    chunks: Mutex<Vec<String>>,
    chunk_count: AtomicU32,
    done_called: AtomicBool,
    final_json: Mutex<Option<String>>,
}

unsafe extern "C" fn test_on_chunk(chunk_json: *const c_char, user_data: *mut c_void) -> i32 {
    let state = unsafe { &*(user_data as *const StreamState) };
    if !chunk_json.is_null() {
        let json_str = unsafe { CStr::from_ptr(chunk_json) }
            .to_str()
            .unwrap_or("")
            .to_string();
        state.chunks.lock().unwrap().push(json_str);
    }
    state.chunk_count.fetch_add(1, Ordering::SeqCst);
    0 // continue streaming
}

unsafe extern "C" fn test_on_done(final_json: *const c_char, user_data: *mut c_void) {
    let state = unsafe { &*(user_data as *const StreamState) };
    state.done_called.store(true, Ordering::SeqCst);
    if !final_json.is_null() {
        let json_str = unsafe { CStr::from_ptr(final_json) }
            .to_str()
            .unwrap_or("")
            .to_string();
        *state.final_json.lock().unwrap() = Some(json_str);
    }
}

#[test]
fn test_stream_with_mock_delivers_chunks_and_done() {
    let config = CString::new(r#"{"provider_type": "mock"}"#).unwrap();
    let provider = unsafe { nxuskit_create_provider(config.as_ptr()) };
    assert!(!provider.is_null());

    let state = StreamState {
        chunks: Mutex::new(Vec::new()),
        chunk_count: AtomicU32::new(0),
        done_called: AtomicBool::new(false),
        final_json: Mutex::new(None),
    };

    let request =
        CString::new(r#"{"model":"test","messages":[{"role":"user","content":"stream test"}]}"#)
            .unwrap();

    let stream = unsafe {
        nxuskit_chat_stream(
            provider,
            request.as_ptr(),
            test_on_chunk,
            test_on_done,
            &state as *const StreamState as *mut c_void,
        )
    };
    assert!(!stream.is_null(), "Stream creation failed");

    // Wait for the stream to complete by freeing it (which joins the task)
    unsafe { nxuskit_free_stream(stream) };

    // Give the async task a moment to complete (it may have already finished)
    std::thread::sleep(std::time::Duration::from_millis(100));

    // on_done must have been called
    assert!(
        state.done_called.load(Ordering::SeqCst),
        "on_done was not called"
    );

    // Final JSON should be valid
    let final_json = state.final_json.lock().unwrap();
    assert!(final_json.is_some(), "Final JSON is None");
    let parsed: serde_json::Value = serde_json::from_str(final_json.as_ref().unwrap()).unwrap();
    assert!(
        parsed.get("content").is_some(), // ChatResponse.content (not StreamChunk)
        "Final response should have 'content' field"
    );

    unsafe { nxuskit_free_provider(provider) };
}

#[test]
fn test_stream_null_provider_returns_null() {
    let request =
        CString::new(r#"{"model":"test","messages":[{"role":"user","content":"test"}]}"#).unwrap();

    let state = StreamState {
        chunks: Mutex::new(Vec::new()),
        chunk_count: AtomicU32::new(0),
        done_called: AtomicBool::new(false),
        final_json: Mutex::new(None),
    };

    let stream = unsafe {
        nxuskit_chat_stream(
            std::ptr::null_mut(),
            request.as_ptr(),
            test_on_chunk,
            test_on_done,
            &state as *const StreamState as *mut c_void,
        )
    };
    assert!(stream.is_null(), "Expected NULL for NULL provider");
}

#[test]
fn test_stream_null_request_returns_null() {
    let config = CString::new(r#"{"provider_type": "mock"}"#).unwrap();
    let provider = unsafe { nxuskit_create_provider(config.as_ptr()) };
    assert!(!provider.is_null());

    let state = StreamState {
        chunks: Mutex::new(Vec::new()),
        chunk_count: AtomicU32::new(0),
        done_called: AtomicBool::new(false),
        final_json: Mutex::new(None),
    };

    let stream = unsafe {
        nxuskit_chat_stream(
            provider,
            std::ptr::null(),
            test_on_chunk,
            test_on_done,
            &state as *const StreamState as *mut c_void,
        )
    };
    assert!(stream.is_null(), "Expected NULL for NULL request");

    unsafe { nxuskit_free_provider(provider) };
}

#[test]
fn test_stream_chunks_contain_index() {
    let config = CString::new(r#"{"provider_type": "mock"}"#).unwrap();
    let provider = unsafe { nxuskit_create_provider(config.as_ptr()) };
    assert!(!provider.is_null());

    let state = StreamState {
        chunks: Mutex::new(Vec::new()),
        chunk_count: AtomicU32::new(0),
        done_called: AtomicBool::new(false),
        final_json: Mutex::new(None),
    };

    let request =
        CString::new(r#"{"model":"test","messages":[{"role":"user","content":"index test"}]}"#)
            .unwrap();

    let stream = unsafe {
        nxuskit_chat_stream(
            provider,
            request.as_ptr(),
            test_on_chunk,
            test_on_done,
            &state as *const StreamState as *mut c_void,
        )
    };

    if !stream.is_null() {
        unsafe { nxuskit_free_stream(stream) };
        std::thread::sleep(std::time::Duration::from_millis(100));

        let chunks = state.chunks.lock().unwrap();
        for (i, chunk_json) in chunks.iter().enumerate() {
            let parsed: serde_json::Value = serde_json::from_str(chunk_json)
                .unwrap_or_else(|_| panic!("Chunk {i} is not valid JSON: {chunk_json}"));
            assert!(
                parsed.get("index").is_some(),
                "Chunk {i} should have 'index' field"
            );
            assert!(
                parsed.get("delta").is_some(),
                "Chunk {i} should have 'delta' field"
            );
        }
    }

    unsafe { nxuskit_free_provider(provider) };
}
