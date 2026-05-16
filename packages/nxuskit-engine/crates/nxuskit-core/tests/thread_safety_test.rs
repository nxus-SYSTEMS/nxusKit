//! Thread safety tests for the C ABI.
//!
//! Verifies that concurrent operations (provider creation, chat, free)
//! from multiple threads don't cause panics, races, or undefined behavior.

use std::ffi::{CStr, CString};
use std::sync::Arc;
use std::thread;

use nxuskit_core::*;

#[test]
fn test_concurrent_provider_creation() {
    let handles: Vec<_> = (0..8)
        .map(|i| {
            thread::spawn(move || {
                let config = CString::new(format!(
                    r#"{{"provider_type": "mock", "model": "thread-{i}"}}"#
                ))
                .unwrap();
                let provider = unsafe { nxuskit_create_provider(config.as_ptr()) };
                assert!(!provider.is_null(), "Thread {i} failed to create provider");
                unsafe { nxuskit_free_provider(provider) };
            })
        })
        .collect();

    for h in handles {
        h.join().expect("Thread panicked");
    }
}

#[test]
fn test_concurrent_chat_on_shared_provider() {
    let config = CString::new(r#"{"provider_type": "mock"}"#).unwrap();
    let provider = unsafe { nxuskit_create_provider(config.as_ptr()) };
    assert!(!provider.is_null());

    // Wrap the raw pointer in Arc for sharing across threads
    let provider_ptr = Arc::new(provider as usize);

    let handles: Vec<_> = (0..8)
        .map(|i| {
            let ptr = Arc::clone(&provider_ptr);
            thread::spawn(move || {
                let provider = *ptr as *mut std::ffi::c_void;
                let request = CString::new(format!(
                    r#"{{"model":"test","messages":[{{"role":"user","content":"thread {i}"}}]}}"#
                ))
                .unwrap();
                let response = unsafe { nxuskit_chat(provider as *mut _, request.as_ptr()) };
                assert!(!response.is_null(), "Thread {i} chat returned NULL");

                let json_ptr = unsafe { nxuskit_response_json(response) };
                assert!(!json_ptr.is_null());

                let json_str = unsafe { CStr::from_ptr(json_ptr) }.to_str().unwrap();
                let _: serde_json::Value = serde_json::from_str(json_str).unwrap();

                unsafe { nxuskit_free_response(response) };
            })
        })
        .collect();

    for h in handles {
        h.join().expect("Thread panicked");
    }

    unsafe { nxuskit_free_provider(provider) };
}

#[test]
fn test_concurrent_version_calls() {
    let handles: Vec<_> = (0..16)
        .map(|_| {
            thread::spawn(|| {
                let ptr = nxuskit_version();
                assert!(!ptr.is_null());
                let version = unsafe { CStr::from_ptr(ptr) }.to_str().unwrap();
                assert!(version.contains('.'));
            })
        })
        .collect();

    for h in handles {
        h.join().expect("Thread panicked");
    }
}

#[test]
fn test_concurrent_error_calls_are_thread_local() {
    // Verify that error state is thread-local (one thread's error
    // doesn't clobber another's).
    let handles: Vec<_> = (0..8)
        .map(|i| {
            thread::spawn(move || {
                // Trigger an error on this thread
                let config =
                    CString::new(format!(r#"{{"provider_type": "nonexistent_{i}"}}"#)).unwrap();
                let provider = unsafe { nxuskit_create_provider(config.as_ptr()) };
                assert!(provider.is_null());

                // Read the error on this thread — should mention our specific type
                let err_ptr = nxuskit_last_error();
                assert!(!err_ptr.is_null());
                let err = unsafe { CStr::from_ptr(err_ptr) }
                    .to_str()
                    .unwrap()
                    .to_string();
                assert!(
                    err.contains(&format!("nonexistent_{i}")),
                    "Thread {i} expected its own error, got: {err}"
                );
            })
        })
        .collect();

    for h in handles {
        h.join().expect("Thread panicked");
    }
}

#[test]
fn test_concurrent_create_and_free_different_providers() {
    let handles: Vec<_> = (0..8)
        .map(|_| {
            thread::spawn(|| {
                for _ in 0..10 {
                    let config = CString::new(r#"{"provider_type": "mock"}"#).unwrap();
                    let provider = unsafe { nxuskit_create_provider(config.as_ptr()) };
                    assert!(!provider.is_null());

                    let request = CString::new(
                        r#"{"model":"test","messages":[{"role":"user","content":"hi"}]}"#,
                    )
                    .unwrap();
                    let response = unsafe { nxuskit_chat(provider, request.as_ptr()) };
                    if !response.is_null() {
                        unsafe { nxuskit_free_response(response) };
                    }

                    unsafe { nxuskit_free_provider(provider) };
                }
            })
        })
        .collect();

    for h in handles {
        h.join().expect("Thread panicked");
    }
}
