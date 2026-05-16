//! Integration tests for nxuskit_completion() and nxuskit_completion_stream() C ABI functions

use std::ffi::{CStr, CString};
use std::os::raw::c_void;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use nxuskit_core::*;

fn last_error() -> Option<String> {
    unsafe {
        let ptr = nxuskit_last_error();
        if ptr.is_null() {
            None
        } else {
            Some(CStr::from_ptr(ptr).to_string_lossy().into_owned())
        }
    }
}

// ── nxuskit_completion() tests ────────────────────────────────────

#[test]
fn test_completion_null_config() {
    unsafe {
        let result = nxuskit_completion(std::ptr::null());
        assert!(result.is_null(), "Should return NULL for NULL config");
        let err = last_error().expect("Should set last error");
        assert!(err.contains("NULL"), "Error should mention NULL: {err}");
    }
}

#[test]
fn test_completion_invalid_json() {
    let config = CString::new("not json").unwrap();
    unsafe {
        let result = nxuskit_completion(config.as_ptr());
        assert!(result.is_null(), "Should return NULL for invalid JSON");
        let err = last_error().expect("Should set last error");
        assert!(
            err.contains("parse") || err.contains("JSON"),
            "Error should mention parse failure: {err}"
        );
    }
}

#[test]
fn test_completion_missing_model() {
    let config = CString::new(r#"{"prompt": "hello"}"#).unwrap();
    unsafe {
        let result = nxuskit_completion(config.as_ptr());
        assert!(result.is_null(), "Should return NULL for missing model");
        let err = last_error().expect("Should set last error");
        assert!(
            err.contains("model"),
            "Error should mention model field: {err}"
        );
    }
}

#[test]
fn test_completion_missing_prompt() {
    let config = CString::new(r#"{"model": "gpt-4o"}"#).unwrap();
    unsafe {
        let result = nxuskit_completion(config.as_ptr());
        assert!(result.is_null(), "Should return NULL for missing prompt");
        let err = last_error().expect("Should set last error");
        assert!(
            err.contains("prompt"),
            "Error should mention prompt field: {err}"
        );
    }
}

#[test]
fn test_completion_empty_model() {
    let config = CString::new(r#"{"model": "", "prompt": "hello"}"#).unwrap();
    unsafe {
        let result = nxuskit_completion(config.as_ptr());
        assert!(result.is_null(), "Should return NULL for empty model");
        let err = last_error().expect("Should set last error");
        assert!(
            err.contains("model"),
            "Error should mention model field: {err}"
        );
    }
}

#[test]
fn test_completion_empty_prompt() {
    let config = CString::new(r#"{"model": "gpt-4o", "prompt": ""}"#).unwrap();
    unsafe {
        let result = nxuskit_completion(config.as_ptr());
        assert!(result.is_null(), "Should return NULL for empty prompt");
        let err = last_error().expect("Should set last error");
        assert!(
            err.contains("prompt"),
            "Error should mention prompt field: {err}"
        );
    }
}

// ── nxuskit_completion_stream() tests ──────────────────────────────

unsafe extern "C" fn test_chunk_cb(
    _chunk_json: *const std::os::raw::c_char,
    user_data: *mut c_void,
) -> i32 {
    let counter = unsafe { &*(user_data as *const AtomicI32) };
    counter.fetch_add(1, Ordering::SeqCst);
    0 // continue
}

unsafe extern "C" fn test_done_cb(
    _final_json: *const std::os::raw::c_char,
    user_data: *mut c_void,
) {
    let done_flag = unsafe { &*(user_data as *const AtomicBool) };
    done_flag.store(true, Ordering::SeqCst);
}

#[test]
fn test_completion_stream_null_config() {
    let chunk_count = AtomicI32::new(0);
    unsafe {
        let result = nxuskit_completion_stream(
            std::ptr::null(),
            test_chunk_cb,
            test_done_cb,
            &chunk_count as *const _ as *mut c_void,
        );
        assert!(result.is_null(), "Should return NULL for NULL config");
        let err = last_error().expect("Should set last error");
        assert!(err.contains("NULL"), "Error should mention NULL: {err}");
    }
}

#[test]
fn test_completion_stream_invalid_json() {
    let chunk_count = AtomicI32::new(0);
    let config = CString::new("not json").unwrap();
    unsafe {
        let result = nxuskit_completion_stream(
            config.as_ptr(),
            test_chunk_cb,
            test_done_cb,
            &chunk_count as *const _ as *mut c_void,
        );
        assert!(result.is_null(), "Should return NULL for invalid JSON");
        let err = last_error().expect("Should set last error");
        assert!(
            err.contains("parse") || err.contains("JSON"),
            "Error should mention parse failure: {err}"
        );
    }
}

#[test]
fn test_completion_stream_missing_model() {
    let chunk_count = AtomicI32::new(0);
    let config = CString::new(r#"{"prompt": "hello"}"#).unwrap();
    unsafe {
        let result = nxuskit_completion_stream(
            config.as_ptr(),
            test_chunk_cb,
            test_done_cb,
            &chunk_count as *const _ as *mut c_void,
        );
        assert!(result.is_null(), "Should return NULL for missing model");
        let err = last_error().expect("Should set last error");
        assert!(
            err.contains("model"),
            "Error should mention model field: {err}"
        );
    }
}

#[test]
fn test_completion_stream_missing_prompt() {
    let chunk_count = AtomicI32::new(0);
    let config = CString::new(r#"{"model": "gpt-4o"}"#).unwrap();
    unsafe {
        let result = nxuskit_completion_stream(
            config.as_ptr(),
            test_chunk_cb,
            test_done_cb,
            &chunk_count as *const _ as *mut c_void,
        );
        assert!(result.is_null(), "Should return NULL for missing prompt");
        let err = last_error().expect("Should set last error");
        assert!(
            err.contains("prompt"),
            "Error should mention prompt field: {err}"
        );
    }
}
