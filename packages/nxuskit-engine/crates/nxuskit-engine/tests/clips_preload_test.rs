#![allow(clippy::panic, clippy::print_stdout, clippy::print_stderr)]
//! Integration tests for the CLIPS preload API.
//!
//! These tests verify that `preload()` and `preload_all()` correctly
//! cache CLIPS environments ahead of time, reducing first-request latency
//! and validating rule bases at startup.

use nxuskit_engine::LLMProvider;
use nxuskit_engine::providers::ClipsProvider;
use nxuskit_engine::types::{ChatRequest, Message};
use tempfile::TempDir;

// ============================================================================
// Test Rules
// ============================================================================

/// CLIPS rules for a simple patient-triage scenario.
///
/// - deftemplate patient: name (STRING), age (INTEGER)
/// - deftemplate triage: patient-name (STRING), level (SYMBOL), reason (STRING)
/// - defrule fever-check: patients with age > 60 produce a triage fact
const TEST_RULES: &str = r#"
(deftemplate patient
    (slot name (type STRING))
    (slot age (type INTEGER)))

(deftemplate triage
    (slot patient-name (type STRING))
    (slot level (type SYMBOL))
    (slot reason (type STRING)))

(defrule fever-check
    "Flag elderly patients for urgent triage"
    (patient (name ?n) (age ?a&:(> ?a 60)))
    =>
    (assert (triage (patient-name ?n) (level urgent) (reason "Elderly patient requires review"))))
"#;

/// A second, independent rule base for batch-preload testing.
const SECONDARY_RULES: &str = r#"
(deftemplate item
    (slot name (type STRING))
    (slot value (type INTEGER)))

(deftemplate result
    (slot item-name (type STRING))
    (slot computed (type INTEGER)))

(defrule double-value
    (item (name ?n) (value ?v))
    =>
    (assert (result (item-name ?n) (computed (* ?v 2)))))
"#;

// ============================================================================
// Helper
// ============================================================================

/// Write the primary test rules to a `.clp` file inside `dir`.
fn create_preload_rules(dir: &std::path::Path) -> std::io::Result<()> {
    std::fs::write(dir.join("test-rules.clp"), TEST_RULES)
}

/// Write the secondary test rules to a `.clp` file inside `dir`.
fn create_secondary_rules(dir: &std::path::Path) -> std::io::Result<()> {
    std::fs::write(dir.join("secondary-rules.clp"), SECONDARY_RULES)
}

// ============================================================================
// Tests
// ============================================================================

/// Preloading a model caches its CLIPS environment so that a subsequent
/// `chat()` call succeeds without re-parsing the rule file.
#[tokio::test]
async fn test_preload_caches_environment() {
    let temp_dir = TempDir::new().expect("Should create temp dir");
    create_preload_rules(temp_dir.path()).expect("Should write rules");

    let provider = ClipsProvider::builder()
        .rules_directory(temp_dir.path())
        .persistent(true)
        .build()
        .expect("Should build provider");

    // Preload the model -- this parses rules and caches the environment.
    provider
        .preload("test-rules.clp")
        .await
        .expect("Preload should succeed");

    // The model should now be in the cache.
    let cached = provider.cached_models();
    assert!(
        cached.contains(&"test-rules.clp".to_string()),
        "Model should be cached after preload, got: {:?}",
        cached
    );

    // Run a chat request that relies on the preloaded environment.
    let input = r#"{
        "facts": [
            {"template": "patient", "values": {"name": "Alice", "age": 75}}
        ]
    }"#;

    let request = ChatRequest::new("test-rules.clp").with_message(Message::user(input));

    let response = provider
        .chat(&request)
        .await
        .expect("Chat should succeed after preload");

    // Parse the response and verify conclusions were returned.
    let output: serde_json::Value =
        serde_json::from_str(&response.content).expect("Response should be valid JSON");

    let conclusions = output
        .get("conclusions")
        .and_then(|c| c.as_array())
        .expect("Should have conclusions array");

    assert!(
        !conclusions.is_empty(),
        "Should have at least one conclusion (elderly triage)"
    );

    // Verify the triage conclusion references the correct patient.
    let has_alice_triage = conclusions.iter().any(|c| {
        c.get("template").and_then(|t| t.as_str()) == Some("triage")
            && c.get("values")
                .and_then(|v| v.get("patient-name"))
                .and_then(|n| n.as_str())
                == Some("Alice")
    });
    assert!(
        has_alice_triage,
        "Should have a triage conclusion for Alice"
    );
}

/// Calling `preload()` twice on the same model is idempotent -- the second
/// call is a no-op and returns `Ok(())`.
#[tokio::test]
async fn test_preload_idempotent() {
    let temp_dir = TempDir::new().expect("Should create temp dir");
    create_preload_rules(temp_dir.path()).expect("Should write rules");

    let provider = ClipsProvider::builder()
        .rules_directory(temp_dir.path())
        .persistent(true)
        .build()
        .expect("Should build provider");

    // First preload.
    provider
        .preload("test-rules.clp")
        .await
        .expect("First preload should succeed");

    // Second preload -- must not fail.
    provider
        .preload("test-rules.clp")
        .await
        .expect("Second preload should be a no-op and succeed");

    // Cache should still contain exactly one entry for this model.
    let cached = provider.cached_models();
    let count = cached.iter().filter(|m| *m == "test-rules.clp").count();
    assert_eq!(count, 1, "Model should appear exactly once in the cache");
}

/// Attempting to preload a model whose rule file does not exist should
/// return an error.
#[tokio::test]
async fn test_preload_missing_file() {
    let temp_dir = TempDir::new().expect("Should create temp dir");
    // Intentionally do NOT write any rules.

    let provider = ClipsProvider::builder()
        .rules_directory(temp_dir.path())
        .persistent(true)
        .build()
        .expect("Should build provider");

    let result = provider.preload("nonexistent-rules.clp").await;

    assert!(
        result.is_err(),
        "Preloading a missing model should return an error"
    );

    let err_msg = result.unwrap_err().to_string().to_lowercase();
    assert!(
        err_msg.contains("not found") || err_msg.contains("failed to load"),
        "Error should mention the missing file: {}",
        err_msg
    );
}

/// `preload()` requires `persistent(true)`.  When the provider is built
/// in non-persistent (stateless) mode, preload should return an error
/// indicating that persistent mode is required.
#[tokio::test]
async fn test_preload_requires_persistent_mode() {
    let temp_dir = TempDir::new().expect("Should create temp dir");
    create_preload_rules(temp_dir.path()).expect("Should write rules");

    let provider = ClipsProvider::builder()
        .rules_directory(temp_dir.path())
        .persistent(false) // explicitly non-persistent
        .build()
        .expect("Should build provider");

    let result = provider.preload("test-rules.clp").await;

    assert!(
        result.is_err(),
        "Preload should fail in non-persistent mode"
    );

    let err_msg = result.unwrap_err().to_string().to_lowercase();
    assert!(
        err_msg.contains("persistent"),
        "Error should mention persistent mode requirement: {}",
        err_msg
    );
}

/// `preload_all()` loads multiple models in one call.  Both should be
/// cached and available for subsequent chat requests.
#[tokio::test]
async fn test_preload_all_batch() {
    let temp_dir = TempDir::new().expect("Should create temp dir");
    create_preload_rules(temp_dir.path()).expect("Should write primary rules");
    create_secondary_rules(temp_dir.path()).expect("Should write secondary rules");

    let provider = ClipsProvider::builder()
        .rules_directory(temp_dir.path())
        .persistent(true)
        .build()
        .expect("Should build provider");

    provider
        .preload_all(&["test-rules.clp", "secondary-rules.clp"])
        .await
        .expect("preload_all should succeed for both models");

    let cached = provider.cached_models();
    assert!(
        cached.contains(&"test-rules.clp".to_string()),
        "Primary model should be cached, got: {:?}",
        cached
    );
    assert!(
        cached.contains(&"secondary-rules.clp".to_string()),
        "Secondary model should be cached, got: {:?}",
        cached
    );
}

/// `preload_all()` fails fast: if any model in the batch does not exist,
/// the call returns an error without necessarily loading subsequent models.
#[tokio::test]
async fn test_preload_all_fail_fast() {
    let temp_dir = TempDir::new().expect("Should create temp dir");
    create_preload_rules(temp_dir.path()).expect("Should write primary rules");
    // Intentionally do NOT write secondary rules.

    let provider = ClipsProvider::builder()
        .rules_directory(temp_dir.path())
        .persistent(true)
        .build()
        .expect("Should build provider");

    let result = provider
        .preload_all(&["test-rules.clp", "missing-rules.clp"])
        .await;

    assert!(
        result.is_err(),
        "preload_all should fail when a model is missing"
    );

    let err_msg = result.unwrap_err().to_string().to_lowercase();
    assert!(
        err_msg.contains("not found") || err_msg.contains("failed to load"),
        "Error should reference the missing model: {}",
        err_msg
    );
}

/// After a single `preload()`, multiple independent chat requests with
/// different facts should all succeed and return valid conclusions.
#[tokio::test]
async fn test_preload_then_multiple_chats() {
    let temp_dir = TempDir::new().expect("Should create temp dir");
    create_preload_rules(temp_dir.path()).expect("Should write rules");

    let provider = ClipsProvider::builder()
        .rules_directory(temp_dir.path())
        .persistent(true)
        .build()
        .expect("Should build provider");

    provider
        .preload("test-rules.clp")
        .await
        .expect("Preload should succeed");

    // Run 5 chat requests with different patient facts.
    let patients = [
        ("Patient-A", 65),
        ("Patient-B", 72),
        ("Patient-C", 80),
        ("Patient-D", 90),
        ("Patient-E", 61),
    ];

    for (name, age) in &patients {
        let input = format!(
            r#"{{
                "facts": [
                    {{"template": "patient", "values": {{"name": "{}", "age": {}}}}}
                ]
            }}"#,
            name, age
        );

        let request = ChatRequest::new("test-rules.clp").with_message(Message::user(&input));

        let response = provider
            .chat(&request)
            .await
            .unwrap_or_else(|e| panic!("Chat for {} should succeed: {}", name, e));

        let output: serde_json::Value = serde_json::from_str(&response.content)
            .unwrap_or_else(|e| panic!("Response for {} should be valid JSON: {}", name, e));

        let conclusions = output
            .get("conclusions")
            .and_then(|c| c.as_array())
            .unwrap_or_else(|| panic!("Response for {} should have conclusions array", name));

        // Every patient is over 60, so each should trigger the fever-check rule.
        assert!(
            !conclusions.is_empty(),
            "Chat for {} (age {}) should produce at least one conclusion",
            name,
            age
        );
    }
}
