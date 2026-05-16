#![allow(clippy::panic, clippy::print_stdout, clippy::print_stderr)]
//! Integration tests for CLIPS Session API lifecycle operations.
//!
//! Tests exercise the `nxuskit_clips_session_*` C ABI functions:
//! create/destroy, reset/clear, info, stale handle detection,
//! and max sessions enforcement.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

// Link the nxuskit_core library.
use nxuskit_core as _;

// ── FFI declarations for Session API ────────────────────────────────────

unsafe extern "C" {
    // Session lifecycle
    fn nxuskit_clips_session_create() -> u64;
    fn nxuskit_clips_session_destroy(session: u64);
    fn nxuskit_clips_session_reset(session: u64) -> i32;
    fn nxuskit_clips_session_clear(session: u64) -> i32;
    fn nxuskit_clips_session_info(session: u64) -> *mut c_char;

    // Construct loading
    fn nxuskit_clips_session_load_string(session: u64, constructs: *const c_char) -> i32;
    fn nxuskit_clips_session_build(session: u64, construct: *const c_char) -> i32;
    fn nxuskit_clips_session_run(session: u64, limit: i64) -> i64;

    // Fact operations
    fn nxuskit_clips_fact_assert_string(session: u64, fact_string: *const c_char) -> i64;
    fn nxuskit_clips_fact_exists(session: u64, fact_index: i64) -> bool;
    fn nxuskit_clips_facts_list(session: u64) -> *mut c_char;

    // Error handling
    fn nxuskit_last_error() -> *const c_char;
    fn nxuskit_free_string(ptr: *mut c_char);
}

// ── Helpers ─────────────────────────────────────────────────────────────

fn c_str(s: &str) -> CString {
    CString::new(s).expect("CString::new failed")
}

/// Read and free a heap-allocated C string returned by session API.
fn read_and_free(ptr: *mut c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let s = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .unwrap_or("")
        .to_string();
    unsafe { nxuskit_free_string(ptr) };
    Some(s)
}

/// Get the last error string, if any.
fn last_error() -> Option<String> {
    let ptr = unsafe { nxuskit_last_error() };
    if ptr.is_null() {
        return None;
    }
    let s = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .unwrap_or("")
        .to_string();
    if s.is_empty() { None } else { Some(s) }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[test]
fn test_session_create_returns_nonzero_handle() {
    let handle = unsafe { nxuskit_clips_session_create() };
    assert_ne!(handle, 0, "session_create should return a non-zero handle");
    unsafe { nxuskit_clips_session_destroy(handle) };
}

#[test]
fn test_session_create_multiple_independent() {
    let h1 = unsafe { nxuskit_clips_session_create() };
    let h2 = unsafe { nxuskit_clips_session_create() };
    assert_ne!(h1, 0);
    assert_ne!(h2, 0);
    assert_ne!(h1, h2, "each session should get a unique handle");
    unsafe {
        nxuskit_clips_session_destroy(h1);
        nxuskit_clips_session_destroy(h2);
    };
}

#[test]
fn test_session_destroy_invalidates_handle() {
    let handle = unsafe { nxuskit_clips_session_create() };
    assert_ne!(handle, 0);

    // Destroy the session
    unsafe { nxuskit_clips_session_destroy(handle) };

    // Now reset should fail — stale handle
    let result = unsafe { nxuskit_clips_session_reset(handle) };
    assert_eq!(result, -1, "reset on destroyed session should return -1");

    let err = last_error();
    assert!(
        err.is_some(),
        "last_error should be set after stale handle use"
    );
    let err_str = err.unwrap();
    assert!(
        err_str.contains("Invalid") || err_str.contains("invalid") || err_str.contains("destroyed"),
        "error should mention invalid/destroyed handle, got: {err_str}"
    );
}

#[test]
fn test_session_reset_preserves_rules() {
    let handle = unsafe { nxuskit_clips_session_create() };
    assert_ne!(handle, 0);

    // Load a template and rule
    let constructs = c_str(
        "(deftemplate person (slot name) (slot age))
         (defrule greet (person (name ?n)) => )",
    );
    let result = unsafe { nxuskit_clips_session_load_string(handle, constructs.as_ptr()) };
    assert_eq!(result, 0, "load_string should succeed");

    // Assert a fact
    let fact = c_str("(person (name \"Alice\") (age 30))");
    let idx = unsafe { nxuskit_clips_fact_assert_string(handle, fact.as_ptr()) };
    assert!(idx >= 0, "assert should return a valid index");

    // Reset — should retract facts but keep rules
    let result = unsafe { nxuskit_clips_session_reset(handle) };
    assert_eq!(result, 0, "reset should succeed");

    // The old fact should no longer exist
    let exists = unsafe { nxuskit_clips_fact_exists(handle, idx) };
    assert!(!exists, "fact should not exist after reset");

    // But we should be able to assert new facts (template still exists)
    let fact2 = c_str("(person (name \"Bob\") (age 25))");
    let idx2 = unsafe { nxuskit_clips_fact_assert_string(handle, fact2.as_ptr()) };
    assert!(
        idx2 >= 0,
        "should be able to assert facts after reset (template preserved)"
    );

    unsafe { nxuskit_clips_session_destroy(handle) };
}

#[test]
fn test_session_clear_removes_everything() {
    let handle = unsafe { nxuskit_clips_session_create() };
    assert_ne!(handle, 0);

    // Load a template
    let constructs = c_str("(deftemplate item (slot id))");
    let result = unsafe { nxuskit_clips_session_load_string(handle, constructs.as_ptr()) };
    assert_eq!(result, 0);

    // Assert a fact
    let fact = c_str("(item (id 1))");
    let idx = unsafe { nxuskit_clips_fact_assert_string(handle, fact.as_ptr()) };
    assert!(idx >= 0);

    // Clear — should remove everything including templates
    let result = unsafe { nxuskit_clips_session_clear(handle) };
    assert_eq!(result, 0, "clear should succeed");

    // Trying to assert with the old template should fail (template is gone)
    let fact2 = c_str("(item (id 2))");
    let idx2 = unsafe { nxuskit_clips_fact_assert_string(handle, fact2.as_ptr()) };
    assert_eq!(
        idx2, -1,
        "assert should fail after clear (template removed)"
    );

    unsafe { nxuskit_clips_session_destroy(handle) };
}

#[test]
fn test_session_info_returns_valid_json() {
    let handle = unsafe { nxuskit_clips_session_create() };
    assert_ne!(handle, 0);

    // Load some constructs
    let constructs = c_str(
        "(deftemplate sensor (slot type) (slot value))
         (defrule check-sensor (sensor (type \"temp\") (value ?v&:(> ?v 100))) => )",
    );
    let result = unsafe { nxuskit_clips_session_load_string(handle, constructs.as_ptr()) };
    assert_eq!(result, 0);

    // Assert facts
    let fact1 = c_str("(sensor (type \"temp\") (value 105))");
    let fact2 = c_str("(sensor (type \"pressure\") (value 50))");
    unsafe {
        nxuskit_clips_fact_assert_string(handle, fact1.as_ptr());
        nxuskit_clips_fact_assert_string(handle, fact2.as_ptr());
    };

    // Get session info
    let info_ptr = unsafe { nxuskit_clips_session_info(handle) };
    let info = read_and_free(info_ptr);
    assert!(info.is_some(), "session_info should return a JSON string");

    let info_str = info.unwrap();
    // Parse as JSON to validate structure
    let parsed: serde_json::Value = serde_json::from_str(&info_str)
        .unwrap_or_else(|e| panic!("session_info returned invalid JSON: {e}\nGot: {info_str}"));

    // Verify expected fields
    assert!(parsed["fact_count"].is_number(), "should have fact_count");
    assert!(parsed["rule_count"].is_number(), "should have rule_count");
    assert!(
        parsed["template_count"].is_number(),
        "should have template_count"
    );
    assert!(
        parsed["module_names"].is_array(),
        "should have module_names"
    );
    assert!(parsed["agenda_size"].is_number(), "should have agenda_size");

    // Verify counts are reasonable
    let fact_count = parsed["fact_count"].as_u64().unwrap();
    assert!(
        fact_count >= 2,
        "should have at least 2 facts, got {fact_count}"
    );

    let rule_count = parsed["rule_count"].as_u64().unwrap();
    assert!(
        rule_count >= 1,
        "should have at least 1 rule, got {rule_count}"
    );

    unsafe { nxuskit_clips_session_destroy(handle) };
}

#[test]
fn test_session_info_on_invalid_handle_returns_null() {
    let info_ptr = unsafe { nxuskit_clips_session_info(0xDEADBEEF) };
    assert!(
        info_ptr.is_null(),
        "session_info on invalid handle should return NULL"
    );
    assert!(last_error().is_some(), "last_error should be set");
}

#[test]
fn test_stale_handle_errors_on_all_operations() {
    let handle = unsafe { nxuskit_clips_session_create() };
    assert_ne!(handle, 0);
    unsafe { nxuskit_clips_session_destroy(handle) };

    // All operations on the stale handle should fail gracefully
    assert_eq!(unsafe { nxuskit_clips_session_reset(handle) }, -1);
    assert_eq!(unsafe { nxuskit_clips_session_clear(handle) }, -1);
    assert!(unsafe { nxuskit_clips_session_info(handle) }.is_null());

    let constructs = c_str("(deftemplate x (slot y))");
    assert_eq!(
        unsafe { nxuskit_clips_session_load_string(handle, constructs.as_ptr()) },
        -1
    );

    let fact = c_str("(x (y 1))");
    assert_eq!(
        unsafe { nxuskit_clips_fact_assert_string(handle, fact.as_ptr()) },
        -1
    );

    assert_eq!(unsafe { nxuskit_clips_session_run(handle, -1) }, -1);
}

#[test]
fn test_session_destroy_is_idempotent() {
    let handle = unsafe { nxuskit_clips_session_create() };
    assert_ne!(handle, 0);

    // First destroy succeeds
    unsafe { nxuskit_clips_session_destroy(handle) };

    // Second destroy should not crash (sets error but doesn't panic)
    unsafe { nxuskit_clips_session_destroy(handle) };

    // Verify error was set
    let err = last_error();
    assert!(err.is_some(), "double destroy should set an error");
}

#[test]
fn test_session_run_basic() {
    let handle = unsafe { nxuskit_clips_session_create() };
    assert_ne!(handle, 0);

    let constructs = c_str(
        "(deftemplate data (slot value))
         (defrule process-data (data (value ?v)) => )",
    );
    let result = unsafe { nxuskit_clips_session_load_string(handle, constructs.as_ptr()) };
    assert_eq!(result, 0);

    // Reset to put initial-fact on the fact list
    unsafe { nxuskit_clips_session_reset(handle) };

    let fact = c_str("(data (value 42))");
    let idx = unsafe { nxuskit_clips_fact_assert_string(handle, fact.as_ptr()) };
    assert!(idx >= 0);

    let fired = unsafe { nxuskit_clips_session_run(handle, -1) };
    assert!(
        fired >= 0,
        "run should return rules fired count, got {fired}"
    );
    assert!(fired >= 1, "at least one rule should fire, got {fired}");

    unsafe { nxuskit_clips_session_destroy(handle) };
}

#[test]
fn test_session_build_single_construct() {
    let handle = unsafe { nxuskit_clips_session_create() };
    assert_ne!(handle, 0);

    // Build a template
    let tmpl = c_str("(deftemplate widget (slot name) (slot weight))");
    let result = unsafe { nxuskit_clips_session_build(handle, tmpl.as_ptr()) };
    assert_eq!(result, 0, "build should succeed for a valid construct");

    // Now we should be able to assert facts using the template
    let fact = c_str("(widget (name \"gear\") (weight 3.5))");
    let idx = unsafe { nxuskit_clips_fact_assert_string(handle, fact.as_ptr()) };
    assert!(idx >= 0, "should be able to assert with built template");

    unsafe { nxuskit_clips_session_destroy(handle) };
}

#[test]
fn test_session_build_invalid_construct() {
    let handle = unsafe { nxuskit_clips_session_create() };
    assert_ne!(handle, 0);

    let bad = c_str("(this is not a valid construct)");
    let result = unsafe { nxuskit_clips_session_build(handle, bad.as_ptr()) };
    assert_eq!(result, -1, "build should fail for invalid construct");
    assert!(last_error().is_some(), "last_error should be set");

    unsafe { nxuskit_clips_session_destroy(handle) };
}

#[test]
fn test_facts_list_empty_session() {
    let handle = unsafe { nxuskit_clips_session_create() };
    assert_ne!(handle, 0);

    let list_ptr = unsafe { nxuskit_clips_facts_list(handle) };
    let list = read_and_free(list_ptr);
    assert!(
        list.is_some(),
        "facts_list should return a string even when empty"
    );

    let list_str = list.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&list_str)
        .unwrap_or_else(|e| panic!("Invalid JSON: {e}\nGot: {list_str}"));
    assert!(parsed.is_array(), "facts_list should return a JSON array");

    unsafe { nxuskit_clips_session_destroy(handle) };
}
