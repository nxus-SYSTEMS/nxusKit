#![allow(clippy::panic, clippy::print_stdout, clippy::print_stderr)]
//! Contract test: FBP (Fact-Based Processing) pattern through Session API.
//!
//! Tests the core customer workflow: create session → load rules → assert facts
//! → run inference → query results by template → verify fact counts.

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
    fn nxuskit_clips_session_info(session: u64) -> *mut c_char;

    fn nxuskit_clips_fact_assert_string(session: u64, fact_string: *const c_char) -> i64;
    fn nxuskit_clips_fact_exists(session: u64, fact_index: i64) -> bool;
    fn nxuskit_clips_fact_get_slot(
        session: u64,
        fact_index: i64,
        slot_name: *const c_char,
    ) -> *mut c_char;
    fn nxuskit_clips_fact_slot_values(session: u64, fact_index: i64) -> *mut c_char;
    fn nxuskit_clips_facts_by_template(session: u64, template_name: *const c_char) -> *mut c_char;

    fn nxuskit_clips_template_exists(session: u64, name: *const c_char) -> bool;
    fn nxuskit_clips_template_list(session: u64) -> *mut c_char;
    fn nxuskit_clips_template_slot_names(session: u64, template_name: *const c_char)
    -> *mut c_char;

    fn nxuskit_clips_rule_exists(session: u64, name: *const c_char) -> bool;
    fn nxuskit_clips_rule_list(session: u64) -> *mut c_char;

    fn nxuskit_clips_eval(session: u64, expression: *const c_char) -> *mut c_char;

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

// ── FBP Pattern Rules ───────────────────────────────────────────────────

const FBP_RULES: &str = r#"
(deftemplate patient
    (slot name (type STRING))
    (slot age (type INTEGER))
    (slot temperature (type FLOAT)))

(deftemplate alert
    (slot patient-name (type STRING))
    (slot alert-type (type SYMBOL))
    (slot severity (type SYMBOL)))

(deftemplate vitals-summary
    (slot patient-name (type STRING))
    (slot status (type SYMBOL)))

(defrule high-fever
    (patient (name ?n) (temperature ?t&:(> ?t 38.5)))
    =>
    (assert (alert (patient-name ?n) (alert-type fever) (severity high))))

(defrule elderly-patient
    (patient (name ?n) (age ?a&:(> ?a 65)))
    =>
    (assert (vitals-summary (patient-name ?n) (status at-risk))))

(defrule fever-in-elderly
    (patient (name ?n) (age ?a&:(> ?a 65)) (temperature ?t&:(> ?t 38.0)))
    =>
    (assert (alert (patient-name ?n) (alert-type critical) (severity critical))))
"#;

// ── Tests ───────────────────────────────────────────────────────────────

#[test]
fn test_fbp_full_cycle() {
    let session = unsafe { nxuskit_clips_session_create() };
    assert_ne!(session, 0);

    // Step 1: Load rules
    let rules = c(FBP_RULES);
    let result = unsafe { nxuskit_clips_session_load_string(session, rules.as_ptr()) };
    assert_eq!(result, 0, "load_string should succeed");

    // Verify templates were loaded
    let tmpl = c("patient");
    assert!(unsafe { nxuskit_clips_template_exists(session, tmpl.as_ptr()) });
    let tmpl = c("alert");
    assert!(unsafe { nxuskit_clips_template_exists(session, tmpl.as_ptr()) });

    // Verify rules were loaded
    let rule = c("high-fever");
    assert!(unsafe { nxuskit_clips_rule_exists(session, rule.as_ptr()) });

    // Step 2: Reset to initialize
    assert_eq!(unsafe { nxuskit_clips_session_reset(session) }, 0);

    // Step 3: Assert patient facts
    let patients = [
        "(patient (name \"Alice\") (age 70) (temperature 39.0))",
        "(patient (name \"Bob\") (age 30) (temperature 37.0))",
        "(patient (name \"Carol\") (age 80) (temperature 38.2))",
        "(patient (name \"Dave\") (age 45) (temperature 40.1))",
        "(patient (name \"Eve\") (age 68) (temperature 36.5))",
    ];

    let mut patient_indices = Vec::new();
    for p in &patients {
        let fact = c(p);
        let idx = unsafe { nxuskit_clips_fact_assert_string(session, fact.as_ptr()) };
        assert!(idx >= 0, "assert should succeed for {p}");
        patient_indices.push(idx);
    }

    // Verify all patients exist
    for &idx in &patient_indices {
        assert!(unsafe { nxuskit_clips_fact_exists(session, idx) });
    }

    // Step 4: Run inference
    let fired = unsafe { nxuskit_clips_session_run(session, -1) };
    assert!(fired >= 1, "at least one rule should fire, got {fired}");

    // Step 5: Query results by template
    let tmpl_name = c("alert");
    let alerts_json =
        read_and_free(unsafe { nxuskit_clips_facts_by_template(session, tmpl_name.as_ptr()) });
    assert!(alerts_json.is_some(), "should get alerts list");
    let alerts: serde_json::Value = json_parse(&alerts_json.unwrap());
    let alert_count = alerts.as_array().map(|a| a.len()).unwrap_or(0);
    // Alice: high-fever + critical (elderly + fever)
    // Carol: critical (elderly + fever above 38.0)
    // Dave: high-fever
    assert!(
        alert_count >= 3,
        "expected at least 3 alerts, got {alert_count}"
    );

    // Step 6: Verify session info reflects the state
    let info = read_and_free(unsafe { nxuskit_clips_session_info(session) });
    assert!(info.is_some());
    let info_val = json_parse(&info.unwrap());
    let fact_count = info_val["fact_count"].as_u64().unwrap_or(0);
    // 5 patients + 3+ alerts + 2+ vitals-summaries + initial-fact
    assert!(
        fact_count >= 10,
        "expected at least 10 facts, got {fact_count}"
    );

    unsafe { nxuskit_clips_session_destroy(session) };
}

#[test]
fn test_fbp_multi_cycle_inference() {
    let session = unsafe { nxuskit_clips_session_create() };
    assert_ne!(session, 0);

    let rules = c(FBP_RULES);
    assert_eq!(
        unsafe { nxuskit_clips_session_load_string(session, rules.as_ptr()) },
        0
    );

    // Run 5 cycles: reset → assert → run → query → verify
    for cycle in 0..5 {
        assert_eq!(unsafe { nxuskit_clips_session_reset(session) }, 0);

        // Assert facts for this cycle
        let temp = 37.0 + (cycle as f64) * 0.5;
        let fact_str = format!(
            "(patient (name \"Cycle{cycle}\") (age {}) (temperature {temp:.1}))",
            50 + cycle * 5
        );
        let fact = c(&fact_str);
        let idx = unsafe { nxuskit_clips_fact_assert_string(session, fact.as_ptr()) };
        assert!(idx >= 0, "assert should succeed in cycle {cycle}");

        let fired = unsafe { nxuskit_clips_session_run(session, -1) };
        assert!(fired >= 0, "run should succeed in cycle {cycle}");

        // Verify the patient fact still exists
        assert!(unsafe { nxuskit_clips_fact_exists(session, idx) });
    }

    unsafe { nxuskit_clips_session_destroy(session) };
}

#[test]
fn test_fbp_query_slot_values() {
    let session = unsafe { nxuskit_clips_session_create() };
    assert_ne!(session, 0);

    let rules = c(FBP_RULES);
    assert_eq!(
        unsafe { nxuskit_clips_session_load_string(session, rules.as_ptr()) },
        0
    );
    assert_eq!(unsafe { nxuskit_clips_session_reset(session) }, 0);

    let fact = c("(patient (name \"TestPatient\") (age 75) (temperature 39.5))");
    let idx = unsafe { nxuskit_clips_fact_assert_string(session, fact.as_ptr()) };
    assert!(idx >= 0);

    // Query individual slots
    let slot_name = c("name");
    let name_json =
        read_and_free(unsafe { nxuskit_clips_fact_get_slot(session, idx, slot_name.as_ptr()) });
    assert!(name_json.is_some(), "should get name slot");
    let name_val = json_parse(&name_json.unwrap());
    assert_eq!(name_val["type"].as_str(), Some("string"));
    assert_eq!(name_val["value"].as_str(), Some("TestPatient"));

    let slot_age = c("age");
    let age_json =
        read_and_free(unsafe { nxuskit_clips_fact_get_slot(session, idx, slot_age.as_ptr()) });
    assert!(age_json.is_some());
    let age_val = json_parse(&age_json.unwrap());
    assert_eq!(age_val["type"].as_str(), Some("integer"));
    assert_eq!(age_val["value"].as_i64(), Some(75));

    let slot_temp = c("temperature");
    let temp_json =
        read_and_free(unsafe { nxuskit_clips_fact_get_slot(session, idx, slot_temp.as_ptr()) });
    assert!(temp_json.is_some());
    let temp_val = json_parse(&temp_json.unwrap());
    assert_eq!(temp_val["type"].as_str(), Some("float"));

    // Query all slot values at once
    let all_slots = read_and_free(unsafe { nxuskit_clips_fact_slot_values(session, idx) });
    assert!(all_slots.is_some());
    let all_val = json_parse(&all_slots.unwrap());
    assert!(
        all_val.is_object(),
        "slot_values should return a JSON object"
    );
    assert!(all_val.get("name").is_some());
    assert!(all_val.get("age").is_some());
    assert!(all_val.get("temperature").is_some());

    unsafe { nxuskit_clips_session_destroy(session) };
}

#[test]
fn test_fbp_template_introspection() {
    let session = unsafe { nxuskit_clips_session_create() };
    assert_ne!(session, 0);

    let rules = c(FBP_RULES);
    assert_eq!(
        unsafe { nxuskit_clips_session_load_string(session, rules.as_ptr()) },
        0
    );

    // List all templates
    let list = read_and_free(unsafe { nxuskit_clips_template_list(session) });
    assert!(list.is_some());
    let templates: serde_json::Value = json_parse(&list.unwrap());
    let template_names: Vec<&str> = templates
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(
        template_names.contains(&"patient"),
        "should have patient template"
    );
    assert!(
        template_names.contains(&"alert"),
        "should have alert template"
    );
    assert!(
        template_names.contains(&"vitals-summary"),
        "should have vitals-summary template"
    );

    // Check slot names for patient template
    let tmpl = c("patient");
    let slots_json =
        read_and_free(unsafe { nxuskit_clips_template_slot_names(session, tmpl.as_ptr()) });
    assert!(slots_json.is_some());
    let slots: serde_json::Value = json_parse(&slots_json.unwrap());
    let slot_names: Vec<&str> = slots
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(
        slot_names.contains(&"name"),
        "patient should have name slot"
    );
    assert!(slot_names.contains(&"age"), "patient should have age slot");
    assert!(
        slot_names.contains(&"temperature"),
        "patient should have temperature slot"
    );

    unsafe { nxuskit_clips_session_destroy(session) };
}

#[test]
fn test_fbp_rule_introspection() {
    let session = unsafe { nxuskit_clips_session_create() };
    assert_ne!(session, 0);

    let rules = c(FBP_RULES);
    assert_eq!(
        unsafe { nxuskit_clips_session_load_string(session, rules.as_ptr()) },
        0
    );

    // List all rules
    let list = read_and_free(unsafe { nxuskit_clips_rule_list(session) });
    assert!(list.is_some());
    let rules_val: serde_json::Value = json_parse(&list.unwrap());
    let rule_names: Vec<&str> = rules_val
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(
        rule_names.contains(&"high-fever"),
        "should have high-fever rule"
    );
    assert!(
        rule_names.contains(&"elderly-patient"),
        "should have elderly-patient rule"
    );
    assert!(
        rule_names.contains(&"fever-in-elderly"),
        "should have fever-in-elderly rule"
    );

    // Check specific rules exist
    let rule = c("high-fever");
    assert!(unsafe { nxuskit_clips_rule_exists(session, rule.as_ptr()) });
    let rule = c("nonexistent-rule");
    assert!(!unsafe { nxuskit_clips_rule_exists(session, rule.as_ptr()) });

    unsafe { nxuskit_clips_session_destroy(session) };
}

#[test]
fn test_fbp_eval_expression() {
    let session = unsafe { nxuskit_clips_session_create() };
    assert_ne!(session, 0);

    // Evaluate a simple expression
    let expr = c("(+ 2 3)");
    let result = read_and_free(unsafe { nxuskit_clips_eval(session, expr.as_ptr()) });
    assert!(result.is_some());
    let val = json_parse(&result.unwrap());
    assert_eq!(val["type"].as_str(), Some("integer"));
    assert_eq!(val["value"].as_i64(), Some(5));

    // Evaluate string expression
    let expr = c("(str-cat \"hello\" \" \" \"world\")");
    let result = read_and_free(unsafe { nxuskit_clips_eval(session, expr.as_ptr()) });
    assert!(result.is_some());
    let val = json_parse(&result.unwrap());
    assert_eq!(val["type"].as_str(), Some("string"));
    assert_eq!(val["value"].as_str(), Some("hello world"));

    unsafe { nxuskit_clips_session_destroy(session) };
}

#[test]
fn test_fbp_50_facts_bulk_assert() {
    let session = unsafe { nxuskit_clips_session_create() };
    assert_ne!(session, 0);

    let rules = c(FBP_RULES);
    assert_eq!(
        unsafe { nxuskit_clips_session_load_string(session, rules.as_ptr()) },
        0
    );
    assert_eq!(unsafe { nxuskit_clips_session_reset(session) }, 0);

    // Assert 50 patient facts
    let mut indices = Vec::new();
    for i in 0..50 {
        let fact = c(&format!(
            "(patient (name \"Patient{i}\") (age {}) (temperature {:.1}))",
            20 + (i % 60),
            36.0 + (i as f64) * 0.1
        ));
        let idx = unsafe { nxuskit_clips_fact_assert_string(session, fact.as_ptr()) };
        assert!(idx >= 0, "assert should succeed for patient {i}");
        indices.push(idx);
    }

    // Run inference
    let fired = unsafe { nxuskit_clips_session_run(session, -1) };
    assert!(fired >= 0, "run should succeed");

    // Verify all patient facts still exist
    for &idx in &indices {
        assert!(unsafe { nxuskit_clips_fact_exists(session, idx) });
    }

    // Verify total fact count increased (patients + derived alerts/summaries + initial-fact)
    let info = read_and_free(unsafe { nxuskit_clips_session_info(session) });
    let info_val = json_parse(&info.unwrap());
    let total = info_val["fact_count"].as_u64().unwrap_or(0);
    assert!(
        total > 50,
        "total facts should exceed 50 patients, got {total}"
    );

    unsafe { nxuskit_clips_session_destroy(session) };
}
