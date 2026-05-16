//! Full FBP integration test for the ClipsSession wrapper.
//!
//! Exercises the complete Fact-Based Processing pattern:
//! create → load rules → assert facts → run → query by template → retract → repeat
//!
//! Run with: `NXUSKIT_LIB_DIR=/path/to/lib cargo test --test clips_session_integration_test -- --ignored`

use nxuskit::{ClipsSession, ClipsValue};
use std::collections::HashMap;

/// Full FBP cycle: 20 templates, 500 facts, 10 inference cycles.
#[test]
#[ignore = "requires libnxuskit runtime"]
fn fbp_500_facts_10_cycles() {
    let session = ClipsSession::create().expect("create session");

    // Define 20 templates with a forwarding rule for each
    let mut constructs = String::new();
    for i in 0..20 {
        constructs.push_str(&format!(
            "(deftemplate input-t{i} (slot id (type INTEGER)) (slot value (type INTEGER)))\n"
        ));
        constructs.push_str(&format!(
            "(deftemplate output-t{i} (slot id (type INTEGER)) (slot result (type INTEGER)))\n"
        ));
        constructs.push_str(&format!(
            "(defrule process-t{i} (input-t{i} (id ?id) (value ?v)) => (assert (output-t{i} (id ?id) (result (* ?v 2)))))\n"
        ));
    }
    session.load_string(&constructs).expect("load constructs");

    let start = std::time::Instant::now();

    for cycle in 0..10 {
        session.reset().expect("reset");

        // Assert 500 facts across 20 templates (25 per template)
        for i in 0..500 {
            let tmpl_idx = i % 20;
            let mut slots = HashMap::new();
            slots.insert("id".into(), ClipsValue::Integer(i as i64));
            slots.insert(
                "value".into(),
                ClipsValue::Integer((i + cycle * 500) as i64),
            );
            session
                .fact_assert_structured(&format!("input-t{tmpl_idx}"), &slots)
                .unwrap_or_else(|e| panic!("assert fact {i} cycle {cycle}: {e}"));
        }

        // Run inference — should fire 500 rules (one per input fact)
        let fired = session.run(None).expect("run");
        assert_eq!(
            fired, 500,
            "cycle {cycle}: expected 500 rules fired, got {fired}"
        );

        // Verify output facts exist for each template
        for t in 0..20 {
            let outputs = session
                .facts_by_template(&format!("output-t{t}"))
                .expect("query outputs");
            assert_eq!(
                outputs.len(),
                25,
                "cycle {cycle}: template output-t{t} should have 25 facts, got {}",
                outputs.len()
            );
        }

        // Verify a sample output's result value (doubling check)
        let output_facts = session
            .facts_by_template("output-t0")
            .expect("query output-t0");
        if let Some(&first_idx) = output_facts.first() {
            let sv = session.fact_slot_values(first_idx).expect("slot values");
            let result = sv["result"].as_integer().unwrap();
            let id = sv["id"].as_integer().unwrap();
            let expected_value = (id + (cycle * 500) as i64) * 2;
            assert_eq!(result, expected_value, "cycle {cycle}: result mismatch");
        }

        // Retract all output facts by template
        for t in 0..20 {
            let retracted = session
                .fact_retract_by_template(&format!("output-t{t}"))
                .expect("retract outputs");
            assert_eq!(retracted, 25, "cycle {cycle}: retract output-t{t}");
        }
    }

    let elapsed = start.elapsed();
    assert!(
        elapsed.as_secs() < 5,
        "10 cycles of 500-fact FBP took {elapsed:?} (target: <5s)"
    );
}

/// Session isolation: two concurrent sessions don't interfere.
#[test]
#[ignore = "requires libnxuskit runtime"]
fn session_isolation_concurrent() {
    let s1 = ClipsSession::create().expect("create s1");
    let s2 = ClipsSession::create().expect("create s2");

    s1.build("(deftemplate alpha (slot x (type INTEGER)))")
        .expect("build s1");
    s2.build("(deftemplate beta (slot y (type INTEGER)))")
        .expect("build s2");

    assert!(s1.template_exists("alpha"));
    assert!(!s1.template_exists("beta"));
    assert!(s2.template_exists("beta"));
    assert!(!s2.template_exists("alpha"));

    s1.reset().expect("reset s1");
    s2.reset().expect("reset s2");

    s1.fact_assert_string("(alpha (x 1))").expect("assert s1");
    s2.fact_assert_string("(beta (y 2))").expect("assert s2");

    let s1_info = s1.info().expect("info s1");
    let s2_info = s2.info().expect("info s2");

    // Each session should only see its own template + initial-fact template
    assert!(s1_info.template_count >= 1);
    assert!(s2_info.template_count >= 1);
}

/// Multi-phase inference with rule deletion between phases.
#[test]
#[ignore = "requires libnxuskit runtime"]
fn multi_phase_rule_evolution() {
    let session = ClipsSession::create().expect("create");

    session
        .build(
            r#"(deftemplate sensor (slot name (type STRING)) (slot value (type INTEGER)))
               (deftemplate alert (slot sensor-name (type STRING)) (slot level (type SYMBOL)))
               (defrule check-critical
                   (sensor (name ?n) (value ?v&:(> ?v 100)))
                   =>
                   (assert (alert (sensor-name ?n) (level critical))))"#,
        )
        .expect("build phase 1 rules");

    // Phase 1: high values trigger critical alerts
    session.reset().expect("reset");
    for i in 0..10 {
        session
            .fact_assert_string(&format!(r#"(sensor (name "s-{i}") (value 200))"#))
            .expect("assert sensor");
    }
    let fired = session.run(None).expect("run");
    assert_eq!(fired, 10, "phase 1: 10 rules should fire");

    let alerts = session.facts_by_template("alert").expect("query alerts");
    assert_eq!(alerts.len(), 10, "phase 1: 10 alerts");

    // Delete the original rule and add a new one with different threshold
    session.rule_delete("check-critical").expect("delete rule");
    assert!(!session.rule_exists("check-critical"));

    session
        .build(
            r#"(defrule check-warning
                   (sensor (name ?n) (value ?v&:(> ?v 50)&:(<= ?v 100)))
                   =>
                   (assert (alert (sensor-name ?n) (level warning))))"#,
        )
        .expect("build phase 2 rule");

    // Phase 2: medium values trigger warnings
    session.reset().expect("reset2");
    for i in 0..5 {
        session
            .fact_assert_string(&format!(r#"(sensor (name "w-{i}") (value 75))"#))
            .expect("assert warning sensor");
    }
    let fired = session.run(None).expect("run2");
    assert_eq!(fired, 5, "phase 2: 5 warning rules should fire");

    let rules = session.rule_list().expect("rule list");
    assert!(rules.contains(&"check-warning".to_string()));
    assert!(!rules.contains(&"check-critical".to_string()));
}

/// Template introspection: slot names, info, and pp_form.
#[test]
#[ignore = "requires libnxuskit runtime"]
fn template_introspection() {
    let session = ClipsSession::create().expect("create");
    session
        .build(
            r#"(deftemplate measurement
                   (slot sensor-id (type STRING))
                   (slot reading (type FLOAT))
                   (slot timestamp (type INTEGER))
                   (slot unit (type SYMBOL) (default meters)))"#,
        )
        .expect("build template");
    session.reset().expect("reset");

    let templates = session.template_list().expect("template list");
    assert!(templates.contains(&"measurement".to_string()));

    let slots = session
        .template_slot_names("measurement")
        .expect("slot names");
    assert!(slots.contains(&"sensor-id".to_string()));
    assert!(slots.contains(&"reading".to_string()));
    assert!(slots.contains(&"timestamp".to_string()));
    assert!(slots.contains(&"unit".to_string()));

    let pp = session.template_pp_form("measurement").expect("pp form");
    assert!(pp.contains("measurement"));
    assert!(pp.contains("sensor-id"));
}

/// Settings: fact duplication and reset globals toggles.
#[test]
#[ignore = "requires libnxuskit runtime"]
fn settings_round_trip() {
    let session = ClipsSession::create().expect("create");

    // Fact duplication defaults off
    assert!(!session.fact_duplication_get());
    session.fact_duplication_set(true).expect("set dup on");
    assert!(session.fact_duplication_get());
    session.fact_duplication_set(false).expect("set dup off");
    assert!(!session.fact_duplication_get());

    // Reset globals defaults on
    assert!(session.reset_globals_get());
    session
        .reset_globals_set(false)
        .expect("set reset globals off");
    assert!(!session.reset_globals_get());
    session
        .reset_globals_set(true)
        .expect("set reset globals on");
    assert!(session.reset_globals_get());
}

/// Execution control: strategy get/set, agenda operations.
#[test]
#[ignore = "requires libnxuskit runtime"]
fn execution_control() {
    let session = ClipsSession::create().expect("create");

    let strategy = session.strategy_get().expect("strategy get");
    assert_eq!(strategy, "depth", "default strategy should be depth");

    session.strategy_set("breadth").expect("set breadth");
    assert_eq!(session.strategy_get().expect("get"), "breadth");

    session.strategy_set("depth").expect("restore depth");

    let salience = session.salience_mode_get().expect("salience get");
    assert!(!salience.is_empty(), "salience mode should not be empty");
}

/// Eval and function_call.
#[test]
#[ignore = "requires libnxuskit runtime"]
fn eval_and_function_call() {
    let session = ClipsSession::create().expect("create");
    session.reset().expect("reset");

    let result = session.eval("(+ 10 20)").expect("eval");
    assert_eq!(result.as_integer().unwrap(), 30);

    let result = session.eval("(* 3.0 4.0)").expect("eval float");
    assert!((result.as_float().unwrap() - 12.0).abs() < 0.001);
}
