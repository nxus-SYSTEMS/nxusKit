//! Tests for CLIPS ordering guarantees
//!
//! These tests verify that CLIPS provider returns conclusions and rule firings
//! in deterministic, reproducible order.
#![allow(clippy::panic, clippy::print_stdout, clippy::print_stderr)]
#![cfg(feature = "clips")]

use nxuskit_engine::LLMProvider;
use nxuskit_engine::providers::ClipsProvider;
use nxuskit_engine::types::{ChatRequest, ClipsOptions, Message, ProviderOptions};
use tempfile::TempDir;

/// Helper to create a test rule base
fn create_test_rules(dir: &std::path::Path) -> std::io::Result<()> {
    let rules = r#"
(deftemplate item
    (slot name (type STRING))
    (slot value (type INTEGER)))

(deftemplate result
    (slot item-name (type STRING))
    (slot computed (type INTEGER)))

(defrule process-item-a
    (item (name "a") (value ?v))
    =>
    (assert (result (item-name "a") (computed (* ?v 2)))))

(defrule process-item-b
    (item (name "b") (value ?v))
    =>
    (assert (result (item-name "b") (computed (* ?v 3)))))

(defrule process-item-c
    (item (name "c") (value ?v))
    =>
    (assert (result (item-name "c") (computed (* ?v 4)))))
"#;

    // Write rules directly to the rules directory (not a subdirectory)
    std::fs::write(dir.join("ordering-rules.clp"), rules)?;
    Ok(())
}

/// T056: Test CLIPS conclusions are sorted by fact_index
#[tokio::test]
async fn test_clips_conclusions_sorted_by_fact_index() {
    let temp_dir = TempDir::new().expect("Should create temp dir");
    create_test_rules(temp_dir.path()).expect("Should create rules");

    let clips = ClipsProvider::builder()
        .rules_directory(temp_dir.path().to_path_buf())
        .include_trace(true)
        .build()
        .expect("Should build");

    // Input multiple items to generate multiple conclusions
    let input = r#"{
        "facts": [
            {"template": "item", "values": {"name": "c", "value": 10}},
            {"template": "item", "values": {"name": "a", "value": 5}},
            {"template": "item", "values": {"name": "b", "value": 7}}
        ]
    }"#;

    // Use the .clp file name as the model
    let request = ChatRequest::new("ordering-rules.clp").with_message(Message::user(input));

    let response = clips.chat(&request).await.expect("Should succeed");

    // Parse the output to check conclusions
    let output: serde_json::Value =
        serde_json::from_str(&response.content).expect("Should parse JSON");

    if let Some(conclusions) = output.get("conclusions").and_then(|c| c.as_array()) {
        if conclusions.len() > 1 {
            // Verify conclusions are sorted by fact_index
            let indices: Vec<i64> = conclusions
                .iter()
                .filter_map(|c| c.get("fact_index").and_then(|i| i.as_i64()))
                .collect();

            for i in 1..indices.len() {
                assert!(
                    indices[i] >= indices[i - 1],
                    "Conclusions should be sorted by fact_index: {:?}",
                    indices
                );
            }
        }
    }
}

/// T057: Test CLIPS fired rules are in deterministic order
#[tokio::test]
async fn test_clips_rules_fired_deterministic_order() {
    let temp_dir = TempDir::new().expect("Should create temp dir");
    create_test_rules(temp_dir.path()).expect("Should create rules");

    let clips = ClipsProvider::builder()
        .rules_directory(temp_dir.path().to_path_buf())
        .include_trace(true)
        .persistent(false) // Fresh environment each time
        .build()
        .expect("Should build");

    let input = r#"{
        "facts": [
            {"template": "item", "values": {"name": "a", "value": 1}},
            {"template": "item", "values": {"name": "b", "value": 2}}
        ]
    }"#;

    let request = ChatRequest::new("ordering-rules.clp").with_message(Message::user(input));

    // Run twice and compare (fresh_session returns Result for CLIPS)
    let fresh1 = clips
        .fresh_session()
        .expect("Should create fresh session 1");
    let fresh2 = clips
        .fresh_session()
        .expect("Should create fresh session 2");

    let response1 = fresh1.chat(&request).await.expect("Run 1");
    let response2 = fresh2.chat(&request).await.expect("Run 2");

    let output1: serde_json::Value = serde_json::from_str(&response1.content).expect("Parse 1");
    let output2: serde_json::Value = serde_json::from_str(&response2.content).expect("Parse 2");

    // Extract rule names from trace
    let get_rule_names = |output: &serde_json::Value| -> Vec<String> {
        output
            .get("trace")
            .and_then(|t| t.get("rules_fired"))
            .and_then(|r| r.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|r| r.get("rule_name").and_then(|n| n.as_str()))
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default()
    };

    let rules1 = get_rule_names(&output1);
    let rules2 = get_rule_names(&output2);

    assert_eq!(
        rules1, rules2,
        "Rule firing order should be deterministic between runs"
    );
}

/// T058: Test conflict strategy is recorded in metadata
#[tokio::test]
async fn test_clips_conflict_strategy_in_metadata() {
    let temp_dir = TempDir::new().expect("Should create temp dir");
    create_test_rules(temp_dir.path()).expect("Should create rules");

    let clips = ClipsProvider::builder()
        .rules_directory(temp_dir.path().to_path_buf())
        .build()
        .expect("Should build");

    let input = r#"{"facts": [{"template": "item", "values": {"name": "a", "value": 1}}]}"#;

    // Test with explicit strategy
    let request = ChatRequest::new("ordering-rules.clp")
        .with_message(Message::user(input))
        .with_provider_options(ProviderOptions::Clips(ClipsOptions {
            strategy: Some("breadth".to_string()),
            allow_duplicate_facts: None,
        }));

    let response = clips.chat(&request).await.expect("Should succeed");

    // Check provider_metadata contains conflict_strategy
    let provider_meta = response
        .inference_metadata
        .provider_metadata
        .expect("Should have provider_metadata");

    let strategy = provider_meta
        .get("conflict_strategy")
        .expect("Should have conflict_strategy");

    assert_eq!(
        strategy.as_str(),
        Some("breadth"),
        "Should record the conflict strategy used"
    );
}

/// Test default conflict strategy is "depth"
#[tokio::test]
async fn test_clips_default_conflict_strategy() {
    let temp_dir = TempDir::new().expect("Should create temp dir");
    create_test_rules(temp_dir.path()).expect("Should create rules");

    let clips = ClipsProvider::builder()
        .rules_directory(temp_dir.path().to_path_buf())
        .build()
        .expect("Should build");

    let input = r#"{"facts": [{"template": "item", "values": {"name": "a", "value": 1}}]}"#;

    // No explicit strategy - should default to "depth"
    let request = ChatRequest::new("ordering-rules.clp").with_message(Message::user(input));

    let response = clips.chat(&request).await.expect("Should succeed");

    let provider_meta = response
        .inference_metadata
        .provider_metadata
        .expect("Should have provider_metadata");

    let strategy = provider_meta
        .get("conflict_strategy")
        .and_then(|s| s.as_str());

    assert_eq!(strategy, Some("depth"), "Default strategy should be depth");
}
