//! Integration tests for stream cancellation via the C ABI.

use std::ffi::{CString, c_void};
use std::os::raw::c_char;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use nxuskit_core::*;

struct CancelState {
    chunk_count: AtomicU32,
    done_called: AtomicBool,
}

unsafe extern "C" fn cancel_on_chunk(_chunk_json: *const c_char, user_data: *mut c_void) -> i32 {
    let state = unsafe { &*(user_data as *const CancelState) };
    state.chunk_count.fetch_add(1, Ordering::SeqCst);
    0
}

unsafe extern "C" fn cancel_on_done(_final_json: *const c_char, user_data: *mut c_void) {
    let state = unsafe { &*(user_data as *const CancelState) };
    state.done_called.store(true, Ordering::SeqCst);
}

#[test]
fn test_cancel_stream_is_safe() {
    let config = CString::new(r#"{"provider_type": "mock"}"#).unwrap();
    let provider = unsafe { nxuskit_create_provider(config.as_ptr()) };
    assert!(!provider.is_null());

    let state = CancelState {
        chunk_count: AtomicU32::new(0),
        done_called: AtomicBool::new(false),
    };

    let request =
        CString::new(r#"{"model":"test","messages":[{"role":"user","content":"cancel me"}]}"#)
            .unwrap();

    let stream = unsafe {
        nxuskit_chat_stream(
            provider,
            request.as_ptr(),
            cancel_on_chunk,
            cancel_on_done,
            &state as *const CancelState as *mut c_void,
        )
    };

    if !stream.is_null() {
        // Cancel immediately
        unsafe { nxuskit_cancel_stream(stream) };

        // Record chunk count at cancellation
        let count_at_cancel = state.chunk_count.load(Ordering::SeqCst);

        // Wait briefly
        std::thread::sleep(std::time::Duration::from_millis(200));

        // No additional chunks should arrive after cancel returns
        let count_after_wait = state.chunk_count.load(Ordering::SeqCst);
        assert_eq!(
            count_at_cancel, count_after_wait,
            "No chunks should arrive after cancel returns"
        );

        unsafe { nxuskit_free_stream(stream) };
    }

    unsafe { nxuskit_free_provider(provider) };
}

#[test]
fn test_cancel_null_stream_is_safe() {
    // Should not crash or panic
    unsafe { nxuskit_cancel_stream(std::ptr::null_mut()) };
}

#[test]
fn test_free_stream_null_is_safe() {
    unsafe { nxuskit_free_stream(std::ptr::null_mut()) };
}

#[test]
fn test_cancel_then_free_is_safe() {
    let config = CString::new(r#"{"provider_type": "mock"}"#).unwrap();
    let provider = unsafe { nxuskit_create_provider(config.as_ptr()) };
    assert!(!provider.is_null());

    let state = CancelState {
        chunk_count: AtomicU32::new(0),
        done_called: AtomicBool::new(false),
    };

    let request =
        CString::new(r#"{"model":"test","messages":[{"role":"user","content":"cancel test"}]}"#)
            .unwrap();

    let stream = unsafe {
        nxuskit_chat_stream(
            provider,
            request.as_ptr(),
            cancel_on_chunk,
            cancel_on_done,
            &state as *const CancelState as *mut c_void,
        )
    };

    if !stream.is_null() {
        // Cancel and then free — should not double-free or panic
        unsafe { nxuskit_cancel_stream(stream) };
        unsafe { nxuskit_free_stream(stream) };
    }

    unsafe { nxuskit_free_provider(provider) };
}
