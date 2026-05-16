//! Integration tests for CLIPS environment introspection
//!
//! These tests verify that the `environment_stats()` and `cached_models()`
//! methods on ClipsProvider return accurate information about cached
//! CLIPS environments.

use nxuskit_engine::LLMProvider;
use nxuskit_engine::providers::ClipsProvider;
use nxuskit_engine::types::{ChatRequest, Message};
use tempfile::TempDir;

/// Helper function to create introspection test rules.
///
/// Writes a `.clp` file containing:
/// - deftemplate `order` (slot id INTEGER, slot customer STRING, slot total FLOAT)
/// - deftemplate `discount` (slot order-id INTEGER, slot amount FLOAT)
/// - defrule `apply-discount`: when an order's total > 100.0, assert a discount
///   with amount = total * 0.1
fn create_introspection_rules(dir: &std::path::Path) -> std::io::Result<()> {
    let rules = r#"
(deftemplate order
    (slot id (type INTEGER))
    (slot customer (type STRING))
    (slot total (type FLOAT)))

(deftemplate discount
    (slot order-id (type INTEGER))
    (slot amount (type FLOAT)))

(defrule apply-discount
    (order (id ?oid) (total ?t&:(> ?t 100.0)))
    =>
    (assert (discount (order-id ?oid) (amount (* ?t 0.1)))))
"#;

    std::fs::write(dir.join("introspection-rules.clp"), rules)?;
    Ok(())
}

/// Test that `environment_stats` returns valid data after preloading a model.
///
/// After preloading (no facts asserted yet), we expect:
/// - fact_count == 0 (no user facts asserted)
/// - rule_count >= 1 (at least `apply-discount`)
/// - template_count >= 1 (at least `order` and `discount`, excluding system templates)
/// - strategy == "depth" (CLIPS default)
/// - modules list is non-empty (at least "MAIN")
#[tokio::test]
async fn test_environment_stats_after_preload() {
    let temp_dir = TempDir::new().expect("Should create temp dir");
    create_introspection_rules(temp_dir.path()).expect("Should create rules");

    let provider = ClipsProvider::builder()
        .rules_directory(temp_dir.path().to_path_buf())
        .persistent(true)
        .build()
        .expect("Should build provider");

    provider
        .preload("introspection-rules.clp")
        .await
        .expect("Should preload model");

    let stats = provider
        .environment_stats("introspection-rules.clp")
        .expect("Should return stats for cached model");

    assert_eq!(
        stats.fact_count, 0,
        "No facts should exist immediately after preload"
    );
    assert!(
        stats.rule_count >= 1,
        "Should have at least 1 rule (apply-discount), got {}",
        stats.rule_count
    );
    assert!(
        stats.template_count >= 1,
        "Should have at least 1 user-defined template, got {}",
        stats.template_count
    );
    assert_eq!(stats.strategy, "depth", "Default strategy should be depth");
    assert!(
        !stats.modules.is_empty(),
        "Modules list should be non-empty (at least MAIN)"
    );
}

/// Test that `environment_stats` reflects increased fact_count after asserting facts via chat.
///
/// Preloads the model, sends a chat request to assert an order fact that triggers
/// the discount rule, then checks that fact_count has increased.
#[tokio::test]
async fn test_environment_stats_after_facts() {
    let temp_dir = TempDir::new().expect("Should create temp dir");
    create_introspection_rules(temp_dir.path()).expect("Should create rules");

    let provider = ClipsProvider::builder()
        .rules_directory(temp_dir.path().to_path_buf())
        .persistent(true)
        .build()
        .expect("Should build provider");

    provider
        .preload("introspection-rules.clp")
        .await
        .expect("Should preload model");

    // Verify no facts before chat
    let stats_before = provider
        .environment_stats("introspection-rules.clp")
        .expect("Should return stats");
    assert_eq!(stats_before.fact_count, 0, "No facts before chat");

    // Assert a fact via chat (order with total > 100 to trigger discount rule)
    let input = r#"{
        "facts": [
            {"template": "order", "values": {"id": 1, "customer": "Alice", "total": 250.0}}
        ]
    }"#;

    let request = ChatRequest::new("introspection-rules.clp").with_message(Message::user(input));

    provider.chat(&request).await.expect("Chat should succeed");

    // Now check stats again -- fact_count should have increased
    let stats_after = provider
        .environment_stats("introspection-rules.clp")
        .expect("Should return stats after chat");

    assert!(
        stats_after.fact_count > stats_before.fact_count,
        "fact_count should increase after asserting facts; before={}, after={}",
        stats_before.fact_count,
        stats_after.fact_count
    );
}

/// Test that `environment_stats` returns `None` for a model that is not cached.
#[tokio::test]
async fn test_environment_stats_uncached() {
    let temp_dir = TempDir::new().expect("Should create temp dir");
    create_introspection_rules(temp_dir.path()).expect("Should create rules");

    let provider = ClipsProvider::builder()
        .rules_directory(temp_dir.path().to_path_buf())
        .persistent(true)
        .build()
        .expect("Should build provider");

    // Do NOT preload anything -- query for a model that is not in the cache
    let stats = provider.environment_stats("nonexistent-model.clp");

    assert!(
        stats.is_none(),
        "environment_stats should return None for an uncached model"
    );
}

/// Test that `cached_models` lists all preloaded models.
///
/// Preloads two different models and verifies both appear in the cached_models list.
#[tokio::test]
async fn test_cached_models_lists_all() {
    let temp_dir = TempDir::new().expect("Should create temp dir");
    create_introspection_rules(temp_dir.path()).expect("Should create rules");

    // Create a second rule file
    let rules2 = r#"
(deftemplate widget
    (slot name (type STRING))
    (slot weight (type FLOAT)))

(defrule heavy-widget
    (widget (name ?n) (weight ?w&:(> ?w 50.0)))
    =>
    (printout t "Heavy widget: " ?n crlf))
"#;
    std::fs::write(temp_dir.path().join("widget-rules.clp"), rules2)
        .expect("Should write second rule file");

    let provider = ClipsProvider::builder()
        .rules_directory(temp_dir.path().to_path_buf())
        .persistent(true)
        .build()
        .expect("Should build provider");

    provider
        .preload("introspection-rules.clp")
        .await
        .expect("Should preload first model");
    provider
        .preload("widget-rules.clp")
        .await
        .expect("Should preload second model");

    let cached = provider.cached_models();

    assert!(
        cached.contains(&"introspection-rules.clp".to_string()),
        "cached_models should contain 'introspection-rules.clp', got {:?}",
        cached
    );
    assert!(
        cached.contains(&"widget-rules.clp".to_string()),
        "cached_models should contain 'widget-rules.clp', got {:?}",
        cached
    );
    assert_eq!(
        cached.len(),
        2,
        "Should have exactly 2 cached models, got {}",
        cached.len()
    );
}

/// Test that `cached_models` returns an empty Vec when no models have been preloaded.
#[tokio::test]
async fn test_cached_models_empty() {
    let temp_dir = TempDir::new().expect("Should create temp dir");

    let provider = ClipsProvider::builder()
        .rules_directory(temp_dir.path().to_path_buf())
        .persistent(true)
        .build()
        .expect("Should build provider");

    let cached = provider.cached_models();

    assert!(
        cached.is_empty(),
        "cached_models should return empty Vec when nothing is preloaded, got {:?}",
        cached
    );
}
