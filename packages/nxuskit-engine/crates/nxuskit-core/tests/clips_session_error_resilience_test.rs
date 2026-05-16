#![allow(clippy::panic, clippy::print_stdout, clippy::print_stderr)]
//! T018b — Error resilience tests for CLIPS Session API.
//!
//! Each test verifies that the session remains fully usable after
//! encountering a specific type of error (invalid template, bad syntax,
//! retract of non-existent fact, etc.).

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

// Link the nxuskit_core library.
use nxuskit_core as _;

// ── FFI declarations for Session API ────────────────────────────────────

#[allow(dead_code)]
unsafe extern "C" {
    fn nxuskit_clips_session_create() -> u64;
    fn nxuskit_clips_session_destroy(session: u64);
    fn nxuskit_clips_session_reset(session: u64) -> i32;
    fn nxuskit_clips_session_load_string(session: u64, constructs: *const c_char) -> i32;
    fn nxuskit_clips_session_build(session: u64, construct: *const c_char) -> i32;
    fn nxuskit_clips_session_run(session: u64, limit: i64) -> i64;

    fn nxuskit_clips_fact_assert_string(session: u64, fact_string: *const c_char) -> i64;
    fn nxuskit_clips_fact_retract(session: u64, fact_index: i64) -> i32;
    fn nxuskit_clips_fact_exists(session: u64, fact_index: i64) -> bool;
    fn nxuskit_clips_facts_list(session: u64) -> *mut c_char;

    fn nxuskit_clips_eval(session: u64, expression: *const c_char) -> *mut c_char;

    fn nxuskit_last_error() -> *const c_char;
    fn nxuskit_free_string(ptr: *mut c_char);
}

// ── Helpers ─────────────────────────────────────────────────────────────

fn c(s: &str) -> CString {
    CString::new(s).expect("CString::new failed")
}

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

fn json_parse(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap_or_else(|e| panic!("Invalid JSON: {e}\nGot: {s}"))
}

// ── Tests ───────────────────────────────────────────────────────────────

/// Assert a fact using a nonexistent template (should fail), then load the
/// template and assert again (should succeed).
#[test]
fn test_assert_with_invalid_template_then_recover() {
    let session = unsafe { nxuskit_clips_session_create() };
    assert_ne!(session, 0);

    // Attempt to assert a fact with a template that does not exist yet.
    let bad_fact = c("(widget (name \"gear\") (weight 3.5))");
    let idx = unsafe { nxuskit_clips_fact_assert_string(session, bad_fact.as_ptr()) };
    assert_eq!(idx, -1, "assert with nonexistent template should return -1");

    // Now load the template that was missing.
    let constructs = c("(deftemplate widget (slot name) (slot weight))");
    let rc = unsafe { nxuskit_clips_session_load_string(session, constructs.as_ptr()) };
    assert_eq!(rc, 0, "load_string should succeed after prior error");

    // Re-attempt the assert — should succeed now.
    let good_fact = c("(widget (name \"gear\") (weight 3.5))");
    let idx2 = unsafe { nxuskit_clips_fact_assert_string(session, good_fact.as_ptr()) };
    assert!(
        idx2 >= 0,
        "assert should succeed after template is loaded, got {idx2}"
    );

    // Verify the fact actually exists.
    assert!(unsafe { nxuskit_clips_fact_exists(session, idx2) });

    unsafe { nxuskit_clips_session_destroy(session) };
}

/// Retract a fact with an invalid index (should fail), then assert a real
/// fact and retract it (should succeed).
#[test]
fn test_retract_invalid_index_then_recover() {
    let session = unsafe { nxuskit_clips_session_create() };
    assert_ne!(session, 0);

    // Retract a fact index that does not exist.
    let rc = unsafe { nxuskit_clips_fact_retract(session, 9999) };
    assert_ne!(rc, 0, "retract of non-existent index should fail");

    // Load a template and assert a fact.
    let constructs = c("(deftemplate item (slot id))");
    let rc = unsafe { nxuskit_clips_session_load_string(session, constructs.as_ptr()) };
    assert_eq!(rc, 0, "load_string should succeed");

    let fact = c("(item (id 42))");
    let idx = unsafe { nxuskit_clips_fact_assert_string(session, fact.as_ptr()) };
    assert!(idx >= 0, "assert should succeed, got {idx}");
    assert!(unsafe { nxuskit_clips_fact_exists(session, idx) });

    // Retract the real fact — should succeed.
    let rc = unsafe { nxuskit_clips_fact_retract(session, idx) };
    assert_eq!(rc, 0, "retract of valid fact should return 0");
    assert!(
        !unsafe { nxuskit_clips_fact_exists(session, idx) },
        "fact should no longer exist after retract"
    );

    unsafe { nxuskit_clips_session_destroy(session) };
}

/// Load malformed CLIPS constructs (should fail), then load valid
/// constructs (should succeed).
#[test]
fn test_load_invalid_constructs_then_recover() {
    let session = unsafe { nxuskit_clips_session_create() };
    assert_ne!(session, 0);

    // Attempt to load garbage.
    let bad = c("(deftemplate broken (slot )))");
    let rc = unsafe { nxuskit_clips_session_load_string(session, bad.as_ptr()) };
    assert_ne!(rc, 0, "load_string with malformed input should fail");

    // Now load valid constructs.
    let good = c("(deftemplate sensor (slot type) (slot value))");
    let rc = unsafe { nxuskit_clips_session_load_string(session, good.as_ptr()) };
    assert_eq!(rc, 0, "load_string should succeed after prior failure");

    // Verify the template works by asserting a fact.
    let fact = c("(sensor (type \"temp\") (value 72))");
    let idx = unsafe { nxuskit_clips_fact_assert_string(session, fact.as_ptr()) };
    assert!(
        idx >= 0,
        "assert should succeed with valid template, got {idx}"
    );

    unsafe { nxuskit_clips_session_destroy(session) };
}

/// Evaluate an expression with a syntax error (should return null), then
/// evaluate a valid expression (should return correct JSON).
#[test]
fn test_eval_syntax_error_then_recover() {
    let session = unsafe { nxuskit_clips_session_create() };
    assert_ne!(session, 0);

    // Evaluate a malformed expression (missing closing paren).
    let bad_expr = c("(+ 2");
    let ptr = unsafe { nxuskit_clips_eval(session, bad_expr.as_ptr()) };
    assert!(
        ptr.is_null(),
        "eval of malformed expression should return null"
    );

    // Now evaluate a valid expression.
    let good_expr = c("(+ 2 3)");
    let result = read_and_free(unsafe { nxuskit_clips_eval(session, good_expr.as_ptr()) });
    assert!(
        result.is_some(),
        "eval of valid expression should return a result"
    );

    let val = json_parse(&result.unwrap());
    assert_eq!(
        val["type"].as_str(),
        Some("integer"),
        "result type should be integer"
    );
    assert_eq!(val["value"].as_i64(), Some(5), "result value should be 5");

    unsafe { nxuskit_clips_session_destroy(session) };
}

/// Build an invalid construct (should fail), then build a valid deftemplate
/// (should succeed), then assert a fact using it (should succeed).
#[test]
fn test_build_invalid_then_recover() {
    let session = unsafe { nxuskit_clips_session_create() };
    assert_ne!(session, 0);

    // Attempt to build garbage.
    let bad = c("(this is not a valid construct)");
    let rc = unsafe { nxuskit_clips_session_build(session, bad.as_ptr()) };
    assert_eq!(rc, -1, "build of garbage should return -1");

    // Build a valid deftemplate.
    let good = c("(deftemplate device (slot name) (slot status))");
    let rc = unsafe { nxuskit_clips_session_build(session, good.as_ptr()) };
    assert_eq!(rc, 0, "build of valid deftemplate should return 0");

    // Assert a fact using the built template.
    let fact = c("(device (name \"router\") (status \"online\"))");
    let idx = unsafe { nxuskit_clips_fact_assert_string(session, fact.as_ptr()) };
    assert!(
        idx >= 0,
        "assert should succeed with built template, got {idx}"
    );
    assert!(unsafe { nxuskit_clips_fact_exists(session, idx) });

    unsafe { nxuskit_clips_session_destroy(session) };
}

/// Trigger three different errors in sequence, then verify a full workflow
/// (load → reset → assert → run → query) all succeeds.
#[test]
fn test_multiple_errors_then_full_workflow() {
    let session = unsafe { nxuskit_clips_session_create() };
    assert_ne!(session, 0);

    // ── Error 1: assert with nonexistent template ───────────────────────
    let bad_fact = c("(ghost (phantom true))");
    let idx = unsafe { nxuskit_clips_fact_assert_string(session, bad_fact.as_ptr()) };
    assert_eq!(idx, -1, "error 1: assert should fail");

    // ── Error 2: load malformed constructs ──────────────────────────────
    let bad_load = c("(defrule broken (???) =>)");
    let rc = unsafe { nxuskit_clips_session_load_string(session, bad_load.as_ptr()) };
    assert_ne!(rc, 0, "error 2: load should fail");

    // ── Error 3: eval syntax error ──────────────────────────────────────
    let bad_eval = c("(* 1 2 +)");
    let ptr = unsafe { nxuskit_clips_eval(session, bad_eval.as_ptr()) };
    // Eval of malformed expression may return null or an error value;
    // we just need the session to still be healthy afterwards.
    if !ptr.is_null() {
        unsafe { nxuskit_free_string(ptr) };
    }

    // ── Full workflow: load → reset → assert → run → query ──────────────

    // Load valid constructs.
    let constructs = c(
        "(deftemplate order (slot product (type STRING)) (slot qty (type INTEGER)) (slot price (type FLOAT)))
         (deftemplate invoice (slot product (type STRING)) (slot total (type FLOAT)))
         (defrule compute-total
             (order (product ?p) (qty ?q) (price ?pr))
             =>
             (assert (invoice (product ?p) (total (* ?q ?pr)))))"
    );
    let rc = unsafe { nxuskit_clips_session_load_string(session, constructs.as_ptr()) };
    assert_eq!(rc, 0, "load should succeed after errors");

    // Reset.
    let rc = unsafe { nxuskit_clips_session_reset(session) };
    assert_eq!(rc, 0, "reset should succeed after errors");

    // Assert facts.
    let fact1 = c("(order (product \"Widget\") (qty 10) (price 2.5))");
    let idx1 = unsafe { nxuskit_clips_fact_assert_string(session, fact1.as_ptr()) };
    assert!(idx1 >= 0, "assert should succeed, got {idx1}");

    let fact2 = c("(order (product \"Gadget\") (qty 5) (price 9.0))");
    let idx2 = unsafe { nxuskit_clips_fact_assert_string(session, fact2.as_ptr()) };
    assert!(idx2 >= 0, "assert should succeed, got {idx2}");

    // Run inference.
    let fired = unsafe { nxuskit_clips_session_run(session, -1) };
    assert!(fired >= 2, "at least 2 rules should fire, got {fired}");

    // Query — list all facts and verify invoices were created.
    let list_ptr = unsafe { nxuskit_clips_facts_list(session) };
    let list = read_and_free(list_ptr);
    assert!(list.is_some(), "facts_list should return a result");

    let facts = json_parse(&list.unwrap());
    let arr = facts
        .as_array()
        .expect("facts_list should return a JSON array");
    // We should have at least: 2 orders + 2 invoices = 4 facts
    // (initial-fact may or may not be included depending on session state)
    assert!(
        arr.len() >= 4,
        "expected at least 4 facts (2 orders + 2 invoices), got {}",
        arr.len()
    );

    unsafe { nxuskit_clips_session_destroy(session) };
}
