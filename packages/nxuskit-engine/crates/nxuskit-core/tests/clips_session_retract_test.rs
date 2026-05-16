#![allow(clippy::panic, clippy::print_stdout, clippy::print_stderr, dead_code)]
//! Contract test: CLIPS Session API retraction operations (T019).
//!
//! Tests retract-by-index, retract-by-template, invalid retraction handling,
//! re-run after retraction, and preservation of unrelated facts.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use nxuskit_core as _;

// ── FFI declarations ────────────────────────────────────────────────────

unsafe extern "C" {
    fn nxuskit_clips_session_create() -> u64;
    fn nxuskit_clips_session_destroy(session: u64);
    fn nxuskit_clips_session_reset(session: u64) -> i32;
    fn nxuskit_clips_session_load_string(session: u64, constructs: *const c_char) -> i32;
    fn nxuskit_clips_session_run(session: u64, limit: i64) -> i64;

    fn nxuskit_clips_fact_assert_string(session: u64, fact_string: *const c_char) -> i64;
    fn nxuskit_clips_fact_retract(session: u64, fact_index: i64) -> i32;
    fn nxuskit_clips_fact_retract_by_template(session: u64, template_name: *const c_char) -> i32;
    fn nxuskit_clips_fact_exists(session: u64, fact_index: i64) -> bool;
    fn nxuskit_clips_facts_list(session: u64) -> *mut c_char;
    fn nxuskit_clips_facts_by_template(session: u64, template_name: *const c_char) -> *mut c_char;
    fn nxuskit_clips_session_info(session: u64) -> *mut c_char;

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

// ── Medical domain constructs ───────────────────────────────────────────

const MEDICAL_RULES: &str = r#"
(deftemplate patient
    (slot name (type STRING))
    (slot age (type INTEGER)))

(deftemplate medication
    (slot patient-name (type STRING))
    (slot drug (type STRING))
    (slot dosage (type FLOAT)))

(defrule prescribe-elderly
    (patient (name ?n) (age ?a&:(> ?a 65)))
    =>
    (assert (medication (patient-name ?n) (drug "aspirin") (dosage 81.0))))
"#;

// ── Tests ───────────────────────────────────────────────────────────────

#[test]
fn test_retract_by_index() {
    let session = unsafe { nxuskit_clips_session_create() };
    assert_ne!(session, 0);

    let rules = c(MEDICAL_RULES);
    assert_eq!(
        unsafe { nxuskit_clips_session_load_string(session, rules.as_ptr()) },
        0
    );
    assert_eq!(unsafe { nxuskit_clips_session_reset(session) }, 0);

    // Assert a patient fact
    let fact = c(r#"(patient (name "Alice") (age 50))"#);
    let idx = unsafe { nxuskit_clips_fact_assert_string(session, fact.as_ptr()) };
    assert!(idx >= 0, "assert should succeed");

    // Verify fact exists
    assert!(unsafe { nxuskit_clips_fact_exists(session, idx) });

    // Retract it by index
    let result = unsafe { nxuskit_clips_fact_retract(session, idx) };
    assert_eq!(result, 0, "retract should return 0 on success");

    // Verify fact no longer exists
    assert!(
        !unsafe { nxuskit_clips_fact_exists(session, idx) },
        "fact should not exist after retraction"
    );

    unsafe { nxuskit_clips_session_destroy(session) };
}

#[test]
fn test_retract_invalid_index() {
    let session = unsafe { nxuskit_clips_session_create() };
    assert_ne!(session, 0);

    let rules = c(MEDICAL_RULES);
    assert_eq!(
        unsafe { nxuskit_clips_session_load_string(session, rules.as_ptr()) },
        0
    );
    assert_eq!(unsafe { nxuskit_clips_session_reset(session) }, 0);

    // Attempt to retract a nonexistent fact index
    let result = unsafe { nxuskit_clips_fact_retract(session, 99999) };
    assert_eq!(result, -1, "retract of nonexistent index should return -1");

    unsafe { nxuskit_clips_session_destroy(session) };
}

#[test]
fn test_retract_by_template() {
    let session = unsafe { nxuskit_clips_session_create() };
    assert_ne!(session, 0);

    let rules = c(MEDICAL_RULES);
    assert_eq!(
        unsafe { nxuskit_clips_session_load_string(session, rules.as_ptr()) },
        0
    );
    assert_eq!(unsafe { nxuskit_clips_session_reset(session) }, 0);

    // Assert 5 patient facts
    for i in 0..5 {
        let fact = c(&format!(
            r#"(patient (name "Patient{i}") (age {}))"#,
            30 + i
        ));
        let idx = unsafe { nxuskit_clips_fact_assert_string(session, fact.as_ptr()) };
        assert!(idx >= 0, "assert should succeed for Patient{i}");
    }

    // Assert 3 medication facts (from a different template)
    for i in 0..3 {
        let fact = c(&format!(
            r#"(medication (patient-name "Patient{i}") (drug "ibuprofen") (dosage {:.1}))"#,
            200.0 + (i as f64) * 100.0
        ));
        let idx = unsafe { nxuskit_clips_fact_assert_string(session, fact.as_ptr()) };
        assert!(idx >= 0, "assert should succeed for medication {i}");
    }

    // Verify both templates have facts
    let tmpl = c("patient");
    let patients_json =
        read_and_free(unsafe { nxuskit_clips_facts_by_template(session, tmpl.as_ptr()) });
    let patients = json_parse(&patients_json.unwrap());
    assert_eq!(
        patients.as_array().map(|a| a.len()).unwrap_or(0),
        5,
        "should have 5 patient facts before retract"
    );

    let tmpl = c("medication");
    let meds_json =
        read_and_free(unsafe { nxuskit_clips_facts_by_template(session, tmpl.as_ptr()) });
    let meds = json_parse(&meds_json.unwrap());
    assert_eq!(
        meds.as_array().map(|a| a.len()).unwrap_or(0),
        3,
        "should have 3 medication facts before retract"
    );

    // Retract all patient facts by template
    let tmpl = c("patient");
    let retracted = unsafe { nxuskit_clips_fact_retract_by_template(session, tmpl.as_ptr()) };
    assert_eq!(retracted, 5, "should retract exactly 5 patient facts");

    // Verify patient facts are gone
    let tmpl = c("patient");
    let patients_json =
        read_and_free(unsafe { nxuskit_clips_facts_by_template(session, tmpl.as_ptr()) });
    let patients = json_parse(&patients_json.unwrap());
    assert_eq!(
        patients.as_array().map(|a| a.len()).unwrap_or(0),
        0,
        "should have 0 patient facts after retract"
    );

    // Verify medication facts still exist
    let tmpl = c("medication");
    let meds_json =
        read_and_free(unsafe { nxuskit_clips_facts_by_template(session, tmpl.as_ptr()) });
    let meds = json_parse(&meds_json.unwrap());
    assert_eq!(
        meds.as_array().map(|a| a.len()).unwrap_or(0),
        3,
        "medication facts should be preserved after patient retract"
    );

    unsafe { nxuskit_clips_session_destroy(session) };
}

#[test]
fn test_retract_and_rerun() {
    let session = unsafe { nxuskit_clips_session_create() };
    assert_ne!(session, 0);

    let rules = c(MEDICAL_RULES);
    assert_eq!(
        unsafe { nxuskit_clips_session_load_string(session, rules.as_ptr()) },
        0
    );
    assert_eq!(unsafe { nxuskit_clips_session_reset(session) }, 0);

    // Assert an elderly patient — triggers the prescribe-elderly rule
    let fact = c(r#"(patient (name "Grandma") (age 80))"#);
    let patient_idx = unsafe { nxuskit_clips_fact_assert_string(session, fact.as_ptr()) };
    assert!(patient_idx >= 0, "assert should succeed");

    // Run inference — prescribe-elderly should fire
    let fired = unsafe { nxuskit_clips_session_run(session, -1) };
    assert!(fired >= 1, "prescribe-elderly should fire, got {fired}");

    // Verify medication was derived
    let tmpl = c("medication");
    let meds_json =
        read_and_free(unsafe { nxuskit_clips_facts_by_template(session, tmpl.as_ptr()) });
    let meds = json_parse(&meds_json.unwrap());
    let med_count_after_first_run = meds.as_array().map(|a| a.len()).unwrap_or(0);
    assert_eq!(
        med_count_after_first_run, 1,
        "should have 1 derived medication"
    );

    // Retract the triggering patient fact
    let result = unsafe { nxuskit_clips_fact_retract(session, patient_idx) };
    assert_eq!(result, 0, "retract should succeed");

    // Re-run inference — no new rules should fire (trigger is gone)
    let fired_again = unsafe { nxuskit_clips_session_run(session, -1) };
    assert_eq!(
        fired_again, 0,
        "no rules should fire after trigger retracted"
    );

    // Verify medication count has NOT increased (no duplication)
    let tmpl = c("medication");
    let meds_json =
        read_and_free(unsafe { nxuskit_clips_facts_by_template(session, tmpl.as_ptr()) });
    let meds = json_parse(&meds_json.unwrap());
    let med_count_after_rerun = meds.as_array().map(|a| a.len()).unwrap_or(0);
    assert_eq!(
        med_count_after_rerun, med_count_after_first_run,
        "medication count should not increase after re-run with trigger retracted"
    );

    unsafe { nxuskit_clips_session_destroy(session) };
}

#[test]
fn test_retract_preserves_unrelated_facts() {
    let session = unsafe { nxuskit_clips_session_create() };
    assert_ne!(session, 0);

    let rules = c(MEDICAL_RULES);
    assert_eq!(
        unsafe { nxuskit_clips_session_load_string(session, rules.as_ptr()) },
        0
    );
    assert_eq!(unsafe { nxuskit_clips_session_reset(session) }, 0);

    // Assert patient facts
    let patient_facts = [
        r#"(patient (name "Alice") (age 40))"#,
        r#"(patient (name "Bob") (age 55))"#,
    ];
    let mut patient_indices = Vec::new();
    for p in &patient_facts {
        let fact = c(p);
        let idx = unsafe { nxuskit_clips_fact_assert_string(session, fact.as_ptr()) };
        assert!(idx >= 0, "assert should succeed for {p}");
        patient_indices.push(idx);
    }

    // Assert medication facts
    let med_facts = [
        r#"(medication (patient-name "Alice") (drug "metformin") (dosage 500.0))"#,
        r#"(medication (patient-name "Bob") (drug "lisinopril") (dosage 10.0))"#,
        r#"(medication (patient-name "Alice") (drug "atorvastatin") (dosage 20.0))"#,
    ];
    let mut med_indices = Vec::new();
    for m in &med_facts {
        let fact = c(m);
        let idx = unsafe { nxuskit_clips_fact_assert_string(session, fact.as_ptr()) };
        assert!(idx >= 0, "assert should succeed for {m}");
        med_indices.push(idx);
    }

    // Retract all patient facts by template
    let tmpl = c("patient");
    let retracted = unsafe { nxuskit_clips_fact_retract_by_template(session, tmpl.as_ptr()) };
    assert_eq!(retracted, 2, "should retract 2 patient facts");

    // Verify patient facts are gone
    for &idx in &patient_indices {
        assert!(
            !unsafe { nxuskit_clips_fact_exists(session, idx) },
            "patient fact {idx} should not exist after retraction"
        );
    }

    // Verify all medication facts still exist and are valid
    for &idx in &med_indices {
        assert!(
            unsafe { nxuskit_clips_fact_exists(session, idx) },
            "medication fact {idx} should still exist"
        );
    }

    // Double-check via template query
    let tmpl = c("medication");
    let meds_json =
        read_and_free(unsafe { nxuskit_clips_facts_by_template(session, tmpl.as_ptr()) });
    let meds = json_parse(&meds_json.unwrap());
    assert_eq!(
        meds.as_array().map(|a| a.len()).unwrap_or(0),
        3,
        "all 3 medication facts should remain"
    );

    // Verify full facts list shows only medication facts (initial-fact may or may not be included)
    let all_facts_json = read_and_free(unsafe { nxuskit_clips_facts_list(session) });
    let all_facts = json_parse(&all_facts_json.unwrap());
    let all_count = all_facts.as_array().map(|a| a.len()).unwrap_or(0);
    assert!(
        all_count >= 3,
        "should have at least 3 facts (3 medications), got {all_count}"
    );

    unsafe { nxuskit_clips_session_destroy(session) };
}
