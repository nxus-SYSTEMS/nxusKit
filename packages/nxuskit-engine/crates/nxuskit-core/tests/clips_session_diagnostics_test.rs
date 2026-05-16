#![allow(clippy::panic, clippy::print_stdout, clippy::print_stderr)]
//! Contract tests for CLIPS Session API — globals, watch, and dribble.
//!
//! Exercises: global_exists/list/get_value/set_value, watch/unwatch,
//! dribble_on/off.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use nxuskit_core as _;

// ── FFI declarations ────────────────────────────────────────────────────

unsafe extern "C" {
    fn nxuskit_clips_session_create() -> u64;
    fn nxuskit_clips_session_destroy(session: u64);
    fn nxuskit_clips_session_reset(session: u64) -> i32;
    fn nxuskit_clips_session_load_string(session: u64, constructs: *const c_char) -> i32;

    // Global variables
    fn nxuskit_clips_global_exists(session: u64, name: *const c_char) -> bool;
    fn nxuskit_clips_global_list(session: u64) -> *mut c_char;
    fn nxuskit_clips_global_get_value(session: u64, name: *const c_char) -> *mut c_char;
    fn nxuskit_clips_global_set_value(
        session: u64,
        name: *const c_char,
        value_json: *const c_char,
    ) -> i32;

    // Watch & diagnostics
    fn nxuskit_clips_watch(session: u64, item: *const c_char) -> i32;
    fn nxuskit_clips_unwatch(session: u64, item: *const c_char) -> i32;
    fn nxuskit_clips_dribble_on(session: u64, file_path: *const c_char) -> i32;
    fn nxuskit_clips_dribble_off(session: u64) -> i32;

    fn nxuskit_last_error() -> *const c_char;
    fn nxuskit_free_string(ptr: *mut c_char);
}

// ── Helpers ─────────────────────────────────────────────────────────────

fn c_str(s: &str) -> CString {
    CString::new(s).expect("CString::new failed")
}

fn read_and_free(ptr: *mut c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let s = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .ok()
        .map(String::from);
    unsafe { nxuskit_free_string(ptr) };
    s
}

fn last_error() -> String {
    let ptr = unsafe { nxuskit_last_error() };
    if ptr.is_null() {
        "(none)".to_string()
    } else {
        unsafe { CStr::from_ptr(ptr) }
            .to_str()
            .unwrap_or("(invalid)")
            .to_string()
    }
}

/// Create session with globals loaded.
fn setup_diag_session() -> u64 {
    let session = unsafe { nxuskit_clips_session_create() };
    assert_ne!(session, 0, "session_create failed: {}", last_error());

    let constructs = c_str(
        r#"
(defglobal ?*counter* = 0)
(defglobal ?*label* = "test")
"#,
    );
    let rc = unsafe { nxuskit_clips_session_load_string(session, constructs.as_ptr()) };
    assert_eq!(rc, 0, "load_string failed: {}", last_error());

    let rc = unsafe { nxuskit_clips_session_reset(session) };
    assert_eq!(rc, 0, "reset failed: {}", last_error());

    session
}

// ── Global variable tests ───────────────────────────────────────────────

#[test]
fn test_global_exists() {
    let session = setup_diag_session();

    assert!(unsafe { nxuskit_clips_global_exists(session, c_str("counter").as_ptr()) });
    assert!(unsafe { nxuskit_clips_global_exists(session, c_str("label").as_ptr()) });
    assert!(!unsafe { nxuskit_clips_global_exists(session, c_str("nonexistent").as_ptr()) });

    unsafe { nxuskit_clips_session_destroy(session) };
}

#[test]
fn test_global_list() {
    let session = setup_diag_session();

    let json = read_and_free(unsafe { nxuskit_clips_global_list(session) });
    assert!(
        json.is_some(),
        "global_list returned NULL: {}",
        last_error()
    );
    let json = json.unwrap();
    assert!(
        json.contains("counter"),
        "global_list should contain counter: {json}"
    );
    assert!(
        json.contains("label"),
        "global_list should contain label: {json}"
    );

    unsafe { nxuskit_clips_session_destroy(session) };
}

#[test]
fn test_global_get_value() {
    let session = setup_diag_session();

    let val = read_and_free(unsafe {
        nxuskit_clips_global_get_value(session, c_str("counter").as_ptr())
    });
    assert!(
        val.is_some(),
        "global_get_value returned NULL: {}",
        last_error()
    );
    let val = val.unwrap();
    assert!(val.contains("0"), "counter should be 0: {val}");

    unsafe { nxuskit_clips_session_destroy(session) };
}

#[test]
fn test_global_set_value() {
    let session = setup_diag_session();

    let new_val = c_str(r#"{"type": "integer", "value": 42}"#);
    let rc = unsafe {
        nxuskit_clips_global_set_value(session, c_str("counter").as_ptr(), new_val.as_ptr())
    };
    assert_eq!(rc, 0, "global_set_value failed: {}", last_error());

    let val = read_and_free(unsafe {
        nxuskit_clips_global_get_value(session, c_str("counter").as_ptr())
    });
    assert!(val.is_some(), "global_get_value after set returned NULL");
    let val = val.unwrap();
    assert!(val.contains("42"), "counter should be 42 after set: {val}");

    unsafe { nxuskit_clips_session_destroy(session) };
}

// ── Watch & diagnostics tests ───────────────────────────────────────────

#[test]
#[cfg_attr(
    target_os = "windows",
    ignore = "CLIPS Watch API requires terminal stdout on Windows"
)]
fn test_watch_unwatch() {
    let session = setup_diag_session();

    let rc = unsafe { nxuskit_clips_watch(session, c_str("facts").as_ptr()) };
    assert_eq!(rc, 0, "watch(facts) failed: {}", last_error());

    let rc = unsafe { nxuskit_clips_unwatch(session, c_str("facts").as_ptr()) };
    assert_eq!(rc, 0, "unwatch(facts) failed: {}", last_error());

    // Watch multiple items
    let rc = unsafe { nxuskit_clips_watch(session, c_str("rules").as_ptr()) };
    assert_eq!(rc, 0, "watch(rules) failed: {}", last_error());

    let rc = unsafe { nxuskit_clips_watch(session, c_str("activations").as_ptr()) };
    assert_eq!(rc, 0, "watch(activations) failed: {}", last_error());

    let rc = unsafe { nxuskit_clips_unwatch(session, c_str("all").as_ptr()) };
    assert_eq!(rc, 0, "unwatch(all) failed: {}", last_error());

    unsafe { nxuskit_clips_session_destroy(session) };
}

#[test]
fn test_watch_invalid_item() {
    let session = setup_diag_session();

    let rc = unsafe { nxuskit_clips_watch(session, c_str("invalid-watch-item").as_ptr()) };
    assert_ne!(rc, 0, "watch(invalid) should fail");

    unsafe { nxuskit_clips_session_destroy(session) };
}

#[test]
fn test_dribble_on_off() {
    let session = setup_diag_session();

    let tmp_path = std::env::temp_dir().join("nxuskit_dribble_test.log");
    let path_str = c_str(tmp_path.to_str().unwrap());

    let rc = unsafe { nxuskit_clips_dribble_on(session, path_str.as_ptr()) };
    assert_eq!(rc, 0, "dribble_on failed: {}", last_error());

    let rc = unsafe { nxuskit_clips_dribble_off(session) };
    assert_eq!(rc, 0, "dribble_off failed: {}", last_error());

    // Clean up temp file
    let _ = std::fs::remove_file(&tmp_path);

    unsafe { nxuskit_clips_session_destroy(session) };
}
