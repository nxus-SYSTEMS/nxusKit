//! Integration tests for the CLIPS Session API safe Rust wrapper.
//!
//! Tests marked `#[ignore]` require `libnxuskit` at runtime.
//! Run them with: `NXUSKIT_LIB_DIR=/path/to/lib cargo test --test clips_sdk_test -- --ignored`

// --- Tests that run without libnxuskit (pure Rust types) ---

use nxuskit::ClipsValue;

// ── ClipsValue type tests ────────────────────────────────────────────

#[test]
fn clips_value_integer_round_trip() {
    let v = ClipsValue::Integer(42);
    let json = serde_json::to_string(&v).unwrap();
    let parsed: ClipsValue = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.as_integer().unwrap(), 42);
}

#[test]
fn clips_value_float_round_trip() {
    let v = ClipsValue::Float(3.14);
    let json = serde_json::to_string(&v).unwrap();
    let parsed: ClipsValue = serde_json::from_str(&json).unwrap();
    assert!((parsed.as_float().unwrap() - 3.14).abs() < 0.001);
}

#[test]
fn clips_value_string_round_trip() {
    let v = ClipsValue::String("hello world".to_string());
    let json = serde_json::to_string(&v).unwrap();
    let parsed: ClipsValue = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.as_string().unwrap(), "hello world");
}

#[test]
fn clips_value_symbol_round_trip() {
    let v = ClipsValue::Symbol("active".to_string());
    let json = serde_json::to_string(&v).unwrap();
    let parsed: ClipsValue = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.as_symbol().unwrap(), "active");
}

#[test]
fn clips_value_string_vs_symbol_distinction() {
    let s = ClipsValue::String("active".to_string());
    let sym = ClipsValue::Symbol("active".to_string());
    assert_ne!(
        s, sym,
        "string and symbol with same content must be distinct"
    );
    assert!(s.as_string().is_ok());
    assert!(s.as_symbol().is_err());
    assert!(sym.as_symbol().is_ok());
    assert!(sym.as_string().is_err());
}

#[test]
fn clips_value_multifield_round_trip() {
    let v = ClipsValue::Multifield(vec![
        ClipsValue::Integer(1),
        ClipsValue::String("two".to_string()),
        ClipsValue::Float(3.0),
        ClipsValue::Symbol("four".to_string()),
    ]);
    let json = serde_json::to_string(&v).unwrap();
    let parsed: ClipsValue = serde_json::from_str(&json).unwrap();
    let items = parsed.as_multifield().unwrap();
    assert_eq!(items.len(), 4);
    assert_eq!(items[0].as_integer().unwrap(), 1);
    assert_eq!(items[1].as_string().unwrap(), "two");
}

#[test]
fn clips_value_void() {
    let v = ClipsValue::Void;
    assert!(v.is_void());
    let json = serde_json::to_string(&v).unwrap();
    let parsed: ClipsValue = serde_json::from_str(&json).unwrap();
    assert!(parsed.is_void());
}

#[test]
fn clips_value_wrong_type_accessors() {
    let v = ClipsValue::Integer(42);
    assert!(v.as_float().is_err());
    assert!(v.as_string().is_err());
    assert!(v.as_symbol().is_err());
    assert!(v.as_multifield().is_err());
    assert!(!v.is_void());
}

#[test]
fn clips_value_c_abi_json_format() {
    // Test parsing JSON in the C ABI format: {"type":"integer","value":42}
    let json = r#"{"type":"integer","value":42}"#;
    let v: ClipsValue = serde_json::from_str(json).unwrap();
    assert_eq!(v.as_integer().unwrap(), 42);

    let json = r#"{"type":"float","value":3.14}"#;
    let v: ClipsValue = serde_json::from_str(json).unwrap();
    assert!((v.as_float().unwrap() - 3.14).abs() < 0.001);

    let json = r#"{"type":"string","value":"hello"}"#;
    let v: ClipsValue = serde_json::from_str(json).unwrap();
    assert_eq!(v.as_string().unwrap(), "hello");

    let json = r#"{"type":"symbol","value":"active"}"#;
    let v: ClipsValue = serde_json::from_str(json).unwrap();
    assert_eq!(v.as_symbol().unwrap(), "active");

    let json = r#"{"type":"void"}"#;
    let v: ClipsValue = serde_json::from_str(json).unwrap();
    assert!(v.is_void());
}

#[test]
fn clips_value_fact_address_round_trip() {
    let v = ClipsValue::FactAddress(42);
    let json = serde_json::to_string(&v).unwrap();
    let parsed: ClipsValue = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, ClipsValue::FactAddress(42));
}

#[test]
fn clips_value_instance_address_round_trip() {
    let v = ClipsValue::InstanceAddress("my-instance".to_string());
    let json = serde_json::to_string(&v).unwrap();
    let parsed: ClipsValue = serde_json::from_str(&json).unwrap();
    assert_eq!(
        parsed,
        ClipsValue::InstanceAddress("my-instance".to_string())
    );
}

// ── Runtime integration tests (require libnxuskit) ───────────────────
// These tests use the new ClipsSession API.

#[test]
#[ignore = "requires libnxuskit runtime"]
fn session_create_build_run_drop() {
    use nxuskit::ClipsSession;

    let session = ClipsSession::create().expect("create failed");

    session
        .build(
            r#"(deftemplate sensor (slot name) (slot value))
               (deftemplate alert (slot alert-name) (slot alert-value))
               (defrule check
                   (sensor (name ?n) (value ?v))
                   =>
                   (assert (alert (alert-name ?n) (alert-value ?v))))"#,
        )
        .expect("build failed");

    session.reset().expect("reset failed");
    let rules_fired = session.run(Some(100)).expect("run failed");
    assert_eq!(rules_fired, 0, "no facts asserted yet");
}

#[test]
#[ignore = "requires libnxuskit runtime"]
fn session_two_independent_sessions() {
    use nxuskit::ClipsSession;

    let s1 = ClipsSession::create().expect("create s1");
    let s2 = ClipsSession::create().expect("create s2");

    s1.build("(deftemplate sensor (slot name))")
        .expect("build into s1");

    // s2 should not see s1's template
    assert!(
        !s2.template_exists("sensor"),
        "s2 should not have s1's template"
    );
    // s1 should have it
    assert!(s1.template_exists("sensor"), "s1 should have its template");
}

#[test]
#[ignore = "requires libnxuskit runtime"]
fn session_structured_fact_all_types() {
    use nxuskit::ClipsSession;
    use std::collections::HashMap;

    let session = ClipsSession::create().expect("create");
    session
        .build(
            r#"(deftemplate data
                (slot i (type INTEGER))
                (slot f (type FLOAT))
                (slot s (type STRING))
                (slot sym (type SYMBOL)))"#,
        )
        .expect("build");
    session.reset().expect("reset");

    let mut slots = HashMap::new();
    slots.insert("i".into(), ClipsValue::Integer(42));
    slots.insert("f".into(), ClipsValue::Float(3.14));
    slots.insert("s".into(), ClipsValue::String("hello".into()));
    slots.insert("sym".into(), ClipsValue::Symbol("active".into()));

    let idx = session
        .fact_assert_structured("data", &slots)
        .expect("assert");
    assert!(idx >= 0);

    // Read back
    let sv = session.fact_slot_values(idx).expect("slot_values");
    assert_eq!(sv["i"].as_integer().unwrap(), 42);
    assert!((sv["f"].as_float().unwrap() - 3.14).abs() < 0.001);
    assert_eq!(sv["s"].as_string().unwrap(), "hello");
    assert_eq!(sv["sym"].as_symbol().unwrap(), "active");
}

#[test]
#[ignore = "requires libnxuskit runtime"]
fn session_multi_phase_inference() {
    use nxuskit::ClipsSession;

    let session = ClipsSession::create().expect("create");
    session
        .build(
            r#"(deftemplate input (slot v (type INTEGER)))
               (deftemplate output (slot v (type INTEGER)))
               (defrule double (input (v ?x)) => (assert (output (v (* ?x 2)))))"#,
        )
        .expect("build");

    // Phase 1
    session.reset().expect("reset");
    session.fact_assert_string("(input (v 5))").expect("assert");
    let fired = session.run(Some(100)).expect("run");
    assert_eq!(fired, 1);

    // Phase 2 — reset preserves rules, clears facts
    session.reset().expect("reset2");
    session
        .fact_assert_string("(input (v 10))")
        .expect("assert2");
    let fired = session.run(None).expect("run2");
    assert_eq!(fired, 1);
}

#[test]
#[ignore = "requires libnxuskit runtime"]
fn session_500_facts_performance() {
    use nxuskit::ClipsSession;
    use std::collections::HashMap;

    let session = ClipsSession::create().expect("create");
    let mut templates = String::new();
    for i in 0..20 {
        templates.push_str(&format!("(deftemplate t{i} (slot v (type INTEGER)))\n"));
    }
    session.load_string(&templates).expect("load");
    session.reset().expect("reset");

    let start = std::time::Instant::now();
    for i in 0..500 {
        let tmpl_idx = i % 20;
        let mut slots = HashMap::new();
        slots.insert("v".into(), ClipsValue::Integer(i as i64));
        session
            .fact_assert_structured(&format!("t{tmpl_idx}"), &slots)
            .expect("assert");
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_secs() < 2,
        "500 facts took {:?} (should be under 2s)",
        elapsed
    );
}

#[test]
#[ignore = "requires libnxuskit runtime"]
fn session_fact_duplication() {
    use nxuskit::ClipsSession;

    let session = ClipsSession::create().expect("create");
    session
        .build("(deftemplate item (slot id (type INTEGER)))")
        .expect("build");
    session.reset().expect("reset");

    session
        .fact_assert_string("(item (id 1))")
        .expect("first assert");
    // Default: duplicates are silently ignored
    session
        .fact_assert_string("(item (id 1))")
        .expect("dup assert");

    // Enable duplication
    session
        .fact_duplication_set(true)
        .expect("set_fact_duplication");
    session
        .fact_assert_string("(item (id 1))")
        .expect("dup with flag");
}

#[test]
#[ignore = "requires libnxuskit runtime"]
fn session_rule_listing() {
    use nxuskit::ClipsSession;

    let session = ClipsSession::create().expect("create");
    session
        .build(
            r#"(deftemplate a (slot x))
               (deftemplate b (slot y))
               (defrule rule-alpha (a (x 1)) => (assert (b (y 1))))
               (defrule rule-beta (a (x 2)) => (assert (b (y 2))))"#,
        )
        .expect("build");

    let rules = session.rule_list().expect("rule_list");
    assert!(
        rules.contains(&"rule-alpha".to_string()),
        "missing rule-alpha"
    );
    assert!(
        rules.contains(&"rule-beta".to_string()),
        "missing rule-beta"
    );
}

#[test]
#[ignore = "requires libnxuskit runtime"]
fn session_full_fbp_pattern() {
    use nxuskit::ClipsSession;

    let session = ClipsSession::create().expect("create");
    session
        .build(
            r#"(deftemplate sensor (slot name (type STRING)) (slot value (type INTEGER)))
               (deftemplate alert (slot sensor (type STRING)) (slot level (type SYMBOL)))
               (defrule check-high
                   (sensor (name ?n) (value ?v&:(> ?v 100)))
                   =>
                   (assert (alert (sensor ?n) (level high))))"#,
        )
        .expect("build");

    // Phase 1: assert 10 high-value sensors
    session.reset().expect("reset");
    for i in 0..10 {
        session
            .fact_assert_string(&format!(r#"(sensor (name "s-{i}") (value 200))"#))
            .expect("assert");
    }
    let fired = session.run(Some(100)).expect("run");
    assert_eq!(fired, 10, "10 sensors should fire 10 rules");

    // Query alerts
    let alerts = session
        .facts_by_template("alert")
        .expect("facts_by_template");
    assert_eq!(alerts.len(), 10, "should have 10 alerts");

    // Phase 2: reset and run with different data
    session.reset().expect("reset2");
    for i in 0..5 {
        session
            .fact_assert_string(&format!(r#"(sensor (name "s-{i}") (value 50))"#))
            .expect("assert2");
    }
    let fired = session.run(None).expect("run2");
    assert_eq!(fired, 0, "values <= 100 should not fire check-high");
}

#[test]
#[ignore = "requires libnxuskit runtime"]
fn session_template_and_info() {
    use nxuskit::ClipsSession;

    let session = ClipsSession::create().expect("create");
    session
        .build(
            r#"(deftemplate sensor (slot name (type STRING)) (slot value (type INTEGER)))
               (defrule noop (sensor) => )"#,
        )
        .expect("build");
    session.reset().expect("reset");

    // Template list
    let templates = session.template_list().expect("template_list");
    assert!(templates.contains(&"sensor".to_string()));

    // Template slot names
    let slots = session.template_slot_names("sensor").expect("slot_names");
    assert!(slots.contains(&"name".to_string()));
    assert!(slots.contains(&"value".to_string()));

    // Session info
    let info = session.info().expect("info");
    assert!(info.template_count >= 1);
    assert!(info.rule_count >= 1);
}

#[test]
#[ignore = "requires libnxuskit runtime"]
fn session_retract_and_verify() {
    use nxuskit::ClipsSession;

    let session = ClipsSession::create().expect("create");
    session
        .build("(deftemplate item (slot id (type INTEGER)))")
        .expect("build");
    session.reset().expect("reset");

    let idx1 = session
        .fact_assert_string("(item (id 1))")
        .expect("assert1");
    let idx2 = session
        .fact_assert_string("(item (id 2))")
        .expect("assert2");
    let idx3 = session
        .fact_assert_string("(item (id 3))")
        .expect("assert3");

    assert!(session.fact_exists(idx1));
    assert!(session.fact_exists(idx2));
    assert!(session.fact_exists(idx3));

    // Retract by index
    session.fact_retract(idx2).expect("retract");
    assert!(session.fact_exists(idx1));
    assert!(!session.fact_exists(idx2));
    assert!(session.fact_exists(idx3));

    // Retract by template
    let count = session
        .fact_retract_by_template("item")
        .expect("retract_by_template");
    assert!(count >= 2);
    assert!(!session.fact_exists(idx1));
    assert!(!session.fact_exists(idx3));
}
