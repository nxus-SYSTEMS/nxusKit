#![allow(
    clippy::approx_constant,
    clippy::panic,
    clippy::print_stderr,
    clippy::print_stdout
)]
//! Integration tests for ClipsValue JSON round-trip fidelity.
//!
//! Every CLIPS value type (INTEGER, FLOAT, STRING, SYMBOL) is asserted into a
//! typed template slot, queried back via the Session API, and verified to
//! preserve its type tag in the returned JSON.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use nxuskit_core as _;

// ── FFI declarations ────────────────────────────────────────────────────

unsafe extern "C" {
    fn nxuskit_clips_session_create() -> u64;
    fn nxuskit_clips_session_destroy(session: u64);
    fn nxuskit_clips_session_load_string(session: u64, constructs: *const c_char) -> i32;
    fn nxuskit_clips_session_reset(session: u64) -> i32;
    fn nxuskit_clips_fact_assert_string(session: u64, fact_string: *const c_char) -> i64;
    fn nxuskit_clips_fact_get_slot(
        session: u64,
        fact_index: i64,
        slot_name: *const c_char,
    ) -> *mut c_char;
    fn nxuskit_clips_fact_slot_values(session: u64, fact_index: i64) -> *mut c_char;
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

/// Retrieve the last FFI error message (if any).
#[allow(dead_code)]
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
fn test_integer_roundtrip() {
    let session = unsafe { nxuskit_clips_session_create() };
    assert_ne!(session, 0, "session_create should return a non-zero handle");

    // Define a template with a single INTEGER slot.
    let tmpl = c("(deftemplate int-holder (slot val (type INTEGER)))");
    let rc = unsafe { nxuskit_clips_session_load_string(session, tmpl.as_ptr()) };
    assert_eq!(rc, 0, "load_string should succeed");

    assert_eq!(unsafe { nxuskit_clips_session_reset(session) }, 0);

    // Assert a fact with integer value 42.
    let fact = c("(int-holder (val 42))");
    let idx = unsafe { nxuskit_clips_fact_assert_string(session, fact.as_ptr()) };
    assert!(idx >= 0, "assert should return a valid index, got {idx}");

    // Query the slot back.
    let slot = c("val");
    let json_str =
        read_and_free(unsafe { nxuskit_clips_fact_get_slot(session, idx, slot.as_ptr()) });
    assert!(json_str.is_some(), "get_slot should return JSON");
    let val = json_parse(&json_str.unwrap());

    assert_eq!(
        val["type"].as_str(),
        Some("integer"),
        "type tag must be \"integer\", got: {val}"
    );
    assert_eq!(
        val["value"].as_i64(),
        Some(42),
        "value must be 42, got: {val}"
    );

    unsafe { nxuskit_clips_session_destroy(session) };
}

#[test]
fn test_float_roundtrip() {
    let session = unsafe { nxuskit_clips_session_create() };
    assert_ne!(session, 0);

    let tmpl = c("(deftemplate float-holder (slot val (type FLOAT)))");
    let rc = unsafe { nxuskit_clips_session_load_string(session, tmpl.as_ptr()) };
    assert_eq!(rc, 0, "load_string should succeed");

    assert_eq!(unsafe { nxuskit_clips_session_reset(session) }, 0);

    // Use "3.14" with explicit decimal point so CLIPS treats it as FLOAT.
    let fact = c("(float-holder (val 3.14))");
    let idx = unsafe { nxuskit_clips_fact_assert_string(session, fact.as_ptr()) };
    assert!(idx >= 0, "assert should return a valid index, got {idx}");

    let slot = c("val");
    let json_str =
        read_and_free(unsafe { nxuskit_clips_fact_get_slot(session, idx, slot.as_ptr()) });
    assert!(json_str.is_some(), "get_slot should return JSON");
    let val = json_parse(&json_str.unwrap());

    assert_eq!(
        val["type"].as_str(),
        Some("float"),
        "type tag must be \"float\", got: {val}"
    );

    let float_val = val["value"]
        .as_f64()
        .unwrap_or_else(|| panic!("value should be a number, got: {val}"));
    assert!(
        (float_val - 3.14).abs() < 1e-9,
        "value must be close to 3.14, got {float_val}"
    );

    unsafe { nxuskit_clips_session_destroy(session) };
}

#[test]
fn test_string_roundtrip() {
    let session = unsafe { nxuskit_clips_session_create() };
    assert_ne!(session, 0);

    let tmpl = c("(deftemplate string-holder (slot val (type STRING)))");
    let rc = unsafe { nxuskit_clips_session_load_string(session, tmpl.as_ptr()) };
    assert_eq!(rc, 0, "load_string should succeed");

    assert_eq!(unsafe { nxuskit_clips_session_reset(session) }, 0);

    let fact = c(r#"(string-holder (val "hello world"))"#);
    let idx = unsafe { nxuskit_clips_fact_assert_string(session, fact.as_ptr()) };
    assert!(idx >= 0, "assert should return a valid index, got {idx}");

    let slot = c("val");
    let json_str =
        read_and_free(unsafe { nxuskit_clips_fact_get_slot(session, idx, slot.as_ptr()) });
    assert!(json_str.is_some(), "get_slot should return JSON");
    let val = json_parse(&json_str.unwrap());

    assert_eq!(
        val["type"].as_str(),
        Some("string"),
        "type tag must be \"string\", got: {val}"
    );
    assert_eq!(
        val["value"].as_str(),
        Some("hello world"),
        "value must be \"hello world\", got: {val}"
    );

    unsafe { nxuskit_clips_session_destroy(session) };
}

#[test]
fn test_symbol_roundtrip() {
    let session = unsafe { nxuskit_clips_session_create() };
    assert_ne!(session, 0);

    let tmpl = c("(deftemplate symbol-holder (slot val (type SYMBOL)))");
    let rc = unsafe { nxuskit_clips_session_load_string(session, tmpl.as_ptr()) };
    assert_eq!(rc, 0, "load_string should succeed");

    assert_eq!(unsafe { nxuskit_clips_session_reset(session) }, 0);

    // Symbols are unquoted tokens in CLIPS.
    let fact = c("(symbol-holder (val active))");
    let idx = unsafe { nxuskit_clips_fact_assert_string(session, fact.as_ptr()) };
    assert!(idx >= 0, "assert should return a valid index, got {idx}");

    let slot = c("val");
    let json_str =
        read_and_free(unsafe { nxuskit_clips_fact_get_slot(session, idx, slot.as_ptr()) });
    assert!(json_str.is_some(), "get_slot should return JSON");
    let val = json_parse(&json_str.unwrap());

    assert_eq!(
        val["type"].as_str(),
        Some("symbol"),
        "type tag must be \"symbol\", got: {val}"
    );
    assert_eq!(
        val["value"].as_str(),
        Some("active"),
        "value must be \"active\", got: {val}"
    );

    unsafe { nxuskit_clips_session_destroy(session) };
}

#[test]
fn test_all_types_single_template() {
    let session = unsafe { nxuskit_clips_session_create() };
    assert_ne!(session, 0);

    // Template with one slot per value type.
    let tmpl = c("(deftemplate multi-type \
            (slot int-val  (type INTEGER)) \
            (slot flt-val  (type FLOAT))   \
            (slot str-val  (type STRING))  \
            (slot sym-val  (type SYMBOL)))");
    let rc = unsafe { nxuskit_clips_session_load_string(session, tmpl.as_ptr()) };
    assert_eq!(rc, 0, "load_string should succeed");

    assert_eq!(unsafe { nxuskit_clips_session_reset(session) }, 0);

    // Assert a fact populating every slot.
    // Float value uses decimal point to ensure CLIPS stores it as FLOAT.
    let fact =
        c(r#"(multi-type (int-val 99) (flt-val 2.718) (str-val "round-trip") (sym-val ready))"#);
    let idx = unsafe { nxuskit_clips_fact_assert_string(session, fact.as_ptr()) };
    assert!(idx >= 0, "assert should return a valid index, got {idx}");

    // Query all slot values at once via fact_slot_values.
    let all_json = read_and_free(unsafe { nxuskit_clips_fact_slot_values(session, idx) });
    assert!(all_json.is_some(), "fact_slot_values should return JSON");
    let all = json_parse(&all_json.unwrap());
    assert!(all.is_object(), "slot_values should return a JSON object");

    // Verify integer slot
    let int_slot = &all["int-val"];
    assert_eq!(
        int_slot["type"].as_str(),
        Some("integer"),
        "int-val type must be \"integer\", got: {int_slot}"
    );
    assert_eq!(
        int_slot["value"].as_i64(),
        Some(99),
        "int-val value must be 99, got: {int_slot}"
    );

    // Verify float slot
    let flt_slot = &all["flt-val"];
    assert_eq!(
        flt_slot["type"].as_str(),
        Some("float"),
        "flt-val type must be \"float\", got: {flt_slot}"
    );
    let flt = flt_slot["value"]
        .as_f64()
        .unwrap_or_else(|| panic!("flt-val value should be a number, got: {flt_slot}"));
    assert!(
        (flt - 2.718).abs() < 1e-9,
        "flt-val must be close to 2.718, got {flt}"
    );

    // Verify string slot
    let str_slot = &all["str-val"];
    assert_eq!(
        str_slot["type"].as_str(),
        Some("string"),
        "str-val type must be \"string\", got: {str_slot}"
    );
    assert_eq!(
        str_slot["value"].as_str(),
        Some("round-trip"),
        "str-val value must be \"round-trip\", got: {str_slot}"
    );

    // Verify symbol slot
    let sym_slot = &all["sym-val"];
    assert_eq!(
        sym_slot["type"].as_str(),
        Some("symbol"),
        "sym-val type must be \"symbol\", got: {sym_slot}"
    );
    assert_eq!(
        sym_slot["value"].as_str(),
        Some("ready"),
        "sym-val value must be \"ready\", got: {sym_slot}"
    );

    unsafe { nxuskit_clips_session_destroy(session) };
}

#[test]
fn test_eval_integer_and_float() {
    let session = unsafe { nxuskit_clips_session_create() };
    assert_ne!(session, 0);

    // ── eval integer arithmetic ─────────────────────────────────────────
    let expr = c("(+ 10 20)");
    let result = read_and_free(unsafe { nxuskit_clips_eval(session, expr.as_ptr()) });
    assert!(result.is_some(), "eval should return JSON");
    let val = json_parse(&result.unwrap());
    assert_eq!(
        val["type"].as_str(),
        Some("integer"),
        "(+ 10 20) type must be \"integer\", got: {val}"
    );
    assert_eq!(
        val["value"].as_i64(),
        Some(30),
        "(+ 10 20) value must be 30, got: {val}"
    );

    // ── eval float arithmetic ───────────────────────────────────────────
    // Both operands have decimal points so CLIPS produces a FLOAT result.
    let expr = c("(+ 1.5 2.5)");
    let result = read_and_free(unsafe { nxuskit_clips_eval(session, expr.as_ptr()) });
    assert!(result.is_some(), "eval should return JSON");
    let val = json_parse(&result.unwrap());
    assert_eq!(
        val["type"].as_str(),
        Some("float"),
        "(+ 1.5 2.5) type must be \"float\", got: {val}"
    );
    let flt = val["value"]
        .as_f64()
        .unwrap_or_else(|| panic!("(+ 1.5 2.5) value should be a number, got: {val}"));
    assert!(
        (flt - 4.0).abs() < 1e-9,
        "(+ 1.5 2.5) value must be 4.0, got {flt}"
    );

    // ── eval string concatenation ───────────────────────────────────────
    let expr = c(r#"(str-cat "a" "b")"#);
    let result = read_and_free(unsafe { nxuskit_clips_eval(session, expr.as_ptr()) });
    assert!(result.is_some(), "eval should return JSON");
    let val = json_parse(&result.unwrap());
    assert_eq!(
        val["type"].as_str(),
        Some("string"),
        "(str-cat \"a\" \"b\") type must be \"string\", got: {val}"
    );
    assert_eq!(
        val["value"].as_str(),
        Some("ab"),
        "(str-cat \"a\" \"b\") value must be \"ab\", got: {val}"
    );

    unsafe { nxuskit_clips_session_destroy(session) };
}
