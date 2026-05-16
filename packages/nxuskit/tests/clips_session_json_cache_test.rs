//! Integration tests for JSON rule loading and session caching.
//!
//! Run with: `NXUSKIT_LIB_DIR=/path/to/lib cargo test --test clips_session_json_cache_test -- --ignored`

use nxuskit::ClipsSession;

#[test]
#[ignore = "requires libnxuskit runtime"]
fn json_load_and_run_inference() {
    let session = ClipsSession::create().expect("create");

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

    session.load_json(json).expect("load_json");
    session.reset().expect("reset");

    session
        .fact_assert_string(r#"(sensor (name "temp-1") (value 200))"#)
        .expect("assert");
    let fired = session.run(None).expect("run");
    assert_eq!(fired, 1);

    let alerts = session.facts_by_template("alert").expect("query alerts");
    assert_eq!(alerts.len(), 1);
}

#[test]
#[ignore = "requires libnxuskit runtime"]
fn cache_preload_get_cached_workflow() {
    let rules_json = r#"{
        "templates": [
            {
                "name": "item",
                "slots": [
                    {"name": "id", "type": "INTEGER"},
                    {"name": "name", "type": "STRING"}
                ]
            }
        ],
        "rules": [
            {
                "name": "noop",
                "source": "(defrule noop (item) => )"
            }
        ]
    }"#;

    // Preload
    ClipsSession::preload("json-cache-test", rules_json).expect("preload");

    // Get a cached clone
    let s1 = ClipsSession::get_cached("json-cache-test").expect("get_cached");
    s1.reset().expect("reset s1");

    // Templates should be available
    assert!(s1.template_exists("item"));

    // Get a second clone — independent
    let s2 = ClipsSession::get_cached("json-cache-test").expect("get_cached 2");
    s2.reset().expect("reset s2");

    // Modify s1, s2 should be unaffected
    s1.fact_assert_string(r#"(item (id 1) (name "a"))"#)
        .expect("assert s1");

    let s2_facts = s2.facts_by_template("item").expect("s2 facts");
    assert_eq!(s2_facts.len(), 0, "s2 should have no facts");

    // Cleanup
    ClipsSession::cache_remove("json-cache-test").expect("cache_remove");
}

#[test]
#[ignore = "requires libnxuskit runtime"]
fn cache_hit_verification() {
    let rules = r#"{"templates": [{"name": "cached_t", "slots": [{"name": "x"}]}]}"#;

    // First preload
    ClipsSession::preload("cache-hit-test", rules).expect("preload 1");

    // Same content, same name — should be a no-op (dedup)
    ClipsSession::preload("cache-hit-test", rules).expect("preload 2 (dedup)");

    // Verify it works
    let s = ClipsSession::get_cached("cache-hit-test").expect("get_cached");
    assert!(s.template_exists("cached_t"));

    ClipsSession::cache_remove("cache-hit-test").expect("cleanup");
}

#[test]
#[ignore = "requires libnxuskit runtime"]
fn module_and_focus_operations() {
    let session = ClipsSession::create().expect("create");

    // Default module is MAIN
    let current = session.module_current_get().expect("current module");
    assert_eq!(current, "MAIN");

    // Create a new module via JSON
    session
        .load_json(r#"{"modules": [{"name": "analysis"}]}"#)
        .expect("load module");

    assert!(session.module_exists("analysis"));
    assert!(session.module_exists("MAIN"));

    let modules = session.module_list().expect("module list");
    assert!(modules.contains(&"MAIN".to_string()));
    assert!(modules.contains(&"analysis".to_string()));

    // Set current module
    session.module_current_set("analysis").expect("set module");
    let current = session.module_current_get().expect("current after set");
    assert_eq!(current, "analysis");

    // Focus stack operations
    session.focus_push("MAIN").expect("push MAIN");
    let top = session.focus_get();
    assert_eq!(top.as_deref(), Some("MAIN"));

    session.focus_pop().expect("pop");
    session.focus_clear().expect("clear focus");
}
