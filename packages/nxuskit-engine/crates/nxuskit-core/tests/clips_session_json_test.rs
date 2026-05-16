#![allow(clippy::panic, clippy::print_stdout, clippy::print_stderr)]
//! Contract tests for session_load_json with modules/templates/rules JSON.
//!
//! These tests verify the JSON rule definition loading path (FR-059/FR-060).

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use nxuskit_core as _;

// ── FFI declarations ────────────────────────────────────────────────────

unsafe extern "C" {
    fn nxuskit_clips_session_create() -> u64;
    fn nxuskit_clips_session_destroy(session: u64);
    fn nxuskit_clips_session_reset(session: u64) -> i32;
    fn nxuskit_clips_session_run(session: u64, limit: i64) -> i64;

    fn nxuskit_clips_session_load_json(session: u64, json: *const c_char) -> i32;

    fn nxuskit_clips_fact_assert_string(session: u64, fact_string: *const c_char) -> i64;
    fn nxuskit_clips_facts_by_template(session: u64, template_name: *const c_char) -> *mut c_char;
    fn nxuskit_clips_template_exists(session: u64, name: *const c_char) -> bool;
    fn nxuskit_clips_module_exists(session: u64, name: *const c_char) -> bool;
    fn nxuskit_free_string(ptr: *mut c_char);
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn create_session() -> u64 {
    let h = unsafe { nxuskit_clips_session_create() };
    assert_ne!(h, 0, "session creation must succeed");
    h
}

fn free_c_str(ptr: *mut c_char) -> String {
    assert!(!ptr.is_null());
    let s = unsafe { CStr::from_ptr(ptr).to_string_lossy().into_owned() };
    unsafe { nxuskit_free_string(ptr) };
    s
}

// ── Tests ────────────────────────────────────────────────────────────────

#[test]
fn json_load_templates_and_rules_run_inference() {
    let session = create_session();

    let json = r#"{
        "templates": [
            {
                "name": "sensor",
                "slots": [
                    {"name": "name", "type": "STRING"},
                    {"name": "value", "type": "INTEGER"}
                ]
            },
            {
                "name": "alert",
                "slots": [
                    {"name": "sensor-name", "type": "STRING"},
                    {"name": "level", "type": "SYMBOL"}
                ]
            }
        ],
        "rules": [
            {
                "name": "check-high",
                "source": "(defrule check-high (sensor (name ?n) (value ?v&:(> ?v 100))) => (assert (alert (sensor-name ?n) (level high))))"
            }
        ]
    }"#;

    let c_json = CString::new(json).unwrap();
    let rc = unsafe { nxuskit_clips_session_load_json(session, c_json.as_ptr()) };
    assert_eq!(rc, 0, "load_json should succeed");

    // Reset and assert facts
    assert_eq!(unsafe { nxuskit_clips_session_reset(session) }, 0);

    let fact = CString::new(r#"(sensor (name "temp-1") (value 200))"#).unwrap();
    let idx = unsafe { nxuskit_clips_fact_assert_string(session, fact.as_ptr()) };
    assert!(idx >= 0);

    let fired = unsafe { nxuskit_clips_session_run(session, -1) };
    assert_eq!(fired, 1, "one rule should fire");

    // Query alert facts
    let tmpl = CString::new("alert").unwrap();
    let alerts_ptr = unsafe { nxuskit_clips_facts_by_template(session, tmpl.as_ptr()) };
    let alerts_json = free_c_str(alerts_ptr);
    let alerts: Vec<i64> = serde_json::from_str(&alerts_json).unwrap();
    assert_eq!(alerts.len(), 1, "should have 1 alert");

    unsafe { nxuskit_clips_session_destroy(session) };
}

#[test]
fn json_load_with_modules() {
    let session = create_session();

    let json = r#"{
        "modules": [
            {"name": "sensors"}
        ],
        "templates": [
            {
                "name": "reading",
                "module": "sensors",
                "slots": [
                    {"name": "value", "type": "FLOAT"}
                ]
            }
        ]
    }"#;

    let c_json = CString::new(json).unwrap();
    let rc = unsafe { nxuskit_clips_session_load_json(session, c_json.as_ptr()) };
    assert_eq!(rc, 0, "load_json with modules should succeed");

    let mod_name = CString::new("sensors").unwrap();
    let exists = unsafe { nxuskit_clips_module_exists(session, mod_name.as_ptr()) };
    assert!(exists, "sensors module should exist");

    unsafe { nxuskit_clips_session_destroy(session) };
}

#[test]
fn json_load_with_facts() {
    let session = create_session();

    let json = r#"{
        "templates": [
            {
                "name": "item",
                "slots": [
                    {"name": "id", "type": "INTEGER"}
                ]
            }
        ],
        "facts": [
            "(item (id 1))",
            "(item (id 2))",
            "(item (id 3))"
        ]
    }"#;

    let c_json = CString::new(json).unwrap();
    let rc = unsafe { nxuskit_clips_session_load_json(session, c_json.as_ptr()) };
    assert_eq!(rc, 0);

    let tmpl = CString::new("item").unwrap();
    let exists = unsafe { nxuskit_clips_template_exists(session, tmpl.as_ptr()) };
    assert!(exists, "item template should exist after load_json");

    unsafe { nxuskit_clips_session_destroy(session) };
}

#[test]
fn json_load_invalid_json_returns_error() {
    let session = create_session();

    let bad_json = CString::new("not valid json {{{").unwrap();
    let rc = unsafe { nxuskit_clips_session_load_json(session, bad_json.as_ptr()) };
    assert_eq!(rc, -1, "invalid JSON should fail");

    unsafe { nxuskit_clips_session_destroy(session) };
}

#[test]
fn json_load_empty_object_succeeds() {
    let session = create_session();

    let empty = CString::new("{}").unwrap();
    let rc = unsafe { nxuskit_clips_session_load_json(session, empty.as_ptr()) };
    assert_eq!(rc, 0, "empty JSON object should succeed (no-op)");

    unsafe { nxuskit_clips_session_destroy(session) };
}

#[test]
fn json_load_template_with_slot_constraints() {
    let session = create_session();

    let json = r#"{
        "templates": [
            {
                "name": "measurement",
                "slots": [
                    {"name": "sensor", "type": "STRING"},
                    {"name": "value", "type": "FLOAT"},
                    {"name": "unit", "type": "SYMBOL", "default": {"type":"symbol","value":"celsius"}}
                ]
            }
        ]
    }"#;

    let c_json = CString::new(json).unwrap();
    let rc = unsafe { nxuskit_clips_session_load_json(session, c_json.as_ptr()) };
    assert_eq!(rc, 0, "template with constraints should load");

    let tmpl = CString::new("measurement").unwrap();
    let exists = unsafe { nxuskit_clips_template_exists(session, tmpl.as_ptr()) };
    assert!(exists);

    unsafe { nxuskit_clips_session_destroy(session) };
}
