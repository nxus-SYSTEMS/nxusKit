#![allow(clippy::panic, clippy::print_stdout, clippy::print_stderr)]
//! Contract tests for session cache (preload, get_cached, cache_remove).
//!
//! These tests verify the LKS pattern: preload → get_cached → independent clone.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use nxuskit_core as _;

// ── FFI declarations ────────────────────────────────────────────────────

unsafe extern "C" {
    fn nxuskit_clips_session_destroy(session: u64);
    fn nxuskit_clips_session_reset(session: u64) -> i32;
    fn nxuskit_clips_session_run(session: u64, limit: i64) -> i64;

    fn nxuskit_clips_fact_assert_string(session: u64, fact_string: *const c_char) -> i64;
    fn nxuskit_clips_facts_by_template(session: u64, template_name: *const c_char) -> *mut c_char;
    fn nxuskit_clips_template_exists(session: u64, name: *const c_char) -> bool;

    fn nxuskit_clips_session_preload(name: *const c_char, rules_json: *const c_char) -> i32;
    fn nxuskit_clips_session_get_cached(name: *const c_char) -> u64;
    fn nxuskit_clips_session_cache_remove(name: *const c_char) -> i32;

    fn nxuskit_free_string(ptr: *mut c_char);
}

fn free_c_str(ptr: *mut c_char) -> String {
    assert!(!ptr.is_null());
    let s = unsafe { CStr::from_ptr(ptr).to_string_lossy().into_owned() };
    unsafe { nxuskit_free_string(ptr) };
    s
}

// ── Tests ────────────────────────────────────────────────────────────────

#[test]
fn preload_and_get_cached_returns_working_session() {
    let rules_json = r#"{
        "templates": [
            {
                "name": "data",
                "slots": [
                    {"name": "key", "type": "STRING"},
                    {"name": "val", "type": "INTEGER"}
                ]
            },
            {
                "name": "result",
                "slots": [
                    {"name": "key", "type": "STRING"},
                    {"name": "val", "type": "INTEGER"}
                ]
            }
        ],
        "rules": [
            {
                "name": "double-val",
                "source": "(defrule double-val (data (key ?k) (val ?v)) => (assert (result (key ?k) (val (* ?v 2)))))"
            }
        ]
    }"#;

    let name = CString::new("test-cache-1").unwrap();
    let c_json = CString::new(rules_json).unwrap();

    // Preload
    let rc = unsafe { nxuskit_clips_session_preload(name.as_ptr(), c_json.as_ptr()) };
    assert_eq!(rc, 0, "preload should succeed");

    // Get cached — returns a new session handle
    let handle = unsafe { nxuskit_clips_session_get_cached(name.as_ptr()) };
    assert_ne!(handle, 0, "get_cached should return valid handle");

    // Reset and use the session
    assert_eq!(unsafe { nxuskit_clips_session_reset(handle) }, 0);

    // Assert a fact and run
    let fact = CString::new(r#"(data (key "input") (val 21))"#).unwrap();
    let idx = unsafe { nxuskit_clips_fact_assert_string(handle, fact.as_ptr()) };
    assert!(idx >= 0);

    let fired = unsafe { nxuskit_clips_session_run(handle, -1) };
    assert_eq!(fired, 1, "double-val rule should fire");

    // Cleanup
    unsafe {
        nxuskit_clips_session_destroy(handle);
        nxuskit_clips_session_cache_remove(name.as_ptr());
    }
}

#[test]
fn get_cached_returns_independent_clones() {
    let rules_json = r#"{
        "templates": [
            {
                "name": "counter",
                "slots": [{"name": "n", "type": "INTEGER"}]
            }
        ]
    }"#;

    let name = CString::new("test-cache-2").unwrap();
    let c_json = CString::new(rules_json).unwrap();

    unsafe { nxuskit_clips_session_preload(name.as_ptr(), c_json.as_ptr()) };

    // Get two clones
    let h1 = unsafe { nxuskit_clips_session_get_cached(name.as_ptr()) };
    let h2 = unsafe { nxuskit_clips_session_get_cached(name.as_ptr()) };
    assert_ne!(h1, 0);
    assert_ne!(h2, 0);
    assert_ne!(h1, h2, "clones should have different handles");

    // Modify h1
    unsafe { nxuskit_clips_session_reset(h1) };
    let fact = CString::new("(counter (n 42))").unwrap();
    unsafe { nxuskit_clips_fact_assert_string(h1, fact.as_ptr()) };

    // h2 should have no counter facts (after reset)
    unsafe { nxuskit_clips_session_reset(h2) };
    let tmpl = CString::new("counter").unwrap();
    let ptr = unsafe { nxuskit_clips_facts_by_template(h2, tmpl.as_ptr()) };
    let json = free_c_str(ptr);
    let facts: Vec<i64> = serde_json::from_str(&json).unwrap();
    assert_eq!(facts.len(), 0, "h2 should have no counter facts");

    unsafe {
        nxuskit_clips_session_destroy(h1);
        nxuskit_clips_session_destroy(h2);
        nxuskit_clips_session_cache_remove(name.as_ptr());
    }
}

#[test]
fn get_cached_nonexistent_returns_zero() {
    let name = CString::new("nonexistent-cache-entry").unwrap();
    let handle = unsafe { nxuskit_clips_session_get_cached(name.as_ptr()) };
    assert_eq!(handle, 0, "nonexistent cache entry should return 0");
}

#[test]
fn cache_remove_nonexistent_returns_error() {
    let name = CString::new("no-such-cache").unwrap();
    let rc = unsafe { nxuskit_clips_session_cache_remove(name.as_ptr()) };
    assert_eq!(rc, -1, "removing nonexistent cache should fail");
}

#[test]
fn preload_dedup_same_content_different_name() {
    let rules = r#"{"templates": [{"name": "dup_test", "slots": [{"name": "x"}]}]}"#;

    let name_a = CString::new("dedup-a").unwrap();
    let name_b = CString::new("dedup-b").unwrap();
    let c_json = CString::new(rules).unwrap();

    // Preload same content under two names
    unsafe {
        nxuskit_clips_session_preload(name_a.as_ptr(), c_json.as_ptr());
        nxuskit_clips_session_preload(name_b.as_ptr(), c_json.as_ptr());
    }

    // Both should be retrievable
    let h1 = unsafe { nxuskit_clips_session_get_cached(name_a.as_ptr()) };
    let h2 = unsafe { nxuskit_clips_session_get_cached(name_b.as_ptr()) };
    assert_ne!(h1, 0, "dedup-a should be retrievable");
    assert_ne!(h2, 0, "dedup-b should be retrievable");

    unsafe {
        nxuskit_clips_session_destroy(h1);
        nxuskit_clips_session_destroy(h2);
        nxuskit_clips_session_cache_remove(name_a.as_ptr());
        nxuskit_clips_session_cache_remove(name_b.as_ptr());
    }
}

#[test]
fn preload_same_name_replaces_existing() {
    let rules_v1 = r#"{"templates": [{"name": "v1item", "slots": [{"name": "x"}]}]}"#;
    let rules_v2 = r#"{"templates": [{"name": "v2item", "slots": [{"name": "y"}]}]}"#;

    let name = CString::new("replace-test").unwrap();
    let c_v1 = CString::new(rules_v1).unwrap();
    let c_v2 = CString::new(rules_v2).unwrap();

    unsafe {
        nxuskit_clips_session_preload(name.as_ptr(), c_v1.as_ptr());
        nxuskit_clips_session_preload(name.as_ptr(), c_v2.as_ptr());
    }

    // Get cached should return v2's template
    let handle = unsafe { nxuskit_clips_session_get_cached(name.as_ptr()) };
    assert_ne!(handle, 0);

    let tmpl_v2 = CString::new("v2item").unwrap();
    let exists = unsafe { nxuskit_clips_template_exists(handle, tmpl_v2.as_ptr()) };
    assert!(exists, "v2item template should exist (replaced v1)");

    unsafe {
        nxuskit_clips_session_destroy(handle);
        nxuskit_clips_session_cache_remove(name.as_ptr());
    }
}
