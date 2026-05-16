//! Comprehensive integration tests for the CLIPS lifecycle-control features.
//!
//! These tests exercise a realistic end-to-end lifecycle flow spanning all four
//! features of clips-lifecycle-control:
//!
//! 1. **Preload / warm-up** -- `preload()` eagerly caches environments.
//! 2. **Focus-stack control** -- `focus` field restricts which modules fire.
//! 3. **Selective fact retraction** -- `command: "retract"` removes facts by template.
//! 4. **Environment introspection** -- `environment_stats()` and `cached_models()`.
//!
//! The tests use a multi-module rule base (sensor pipeline with CLASSIFY and ALERT
//! stages) to validate that all four features compose correctly within a single
//! persistent session.

use nxuskit_engine::LLMProvider;
use nxuskit_engine::providers::ClipsProvider;
use nxuskit_engine::types::{ChatRequest, Message};
use tempfile::TempDir;

// ============================================================================
// Test Rule Bases
// ============================================================================

/// Multi-module sensor pipeline rules.
///
/// Defines two modules that form a staged evaluation pipeline:
///
///   - **CLASSIFY**: Reads `sensor-reading` facts and derives `classification`
///     facts (e.g., "critical" when value > 100, "normal" when 50 < value <= 100).
///
///   - **ALERT**: Imports from CLASSIFY and derives `alert` facts for critical
///     classifications.
///
/// This rule base is designed to exercise focus-stack control (run CLASSIFY
/// without ALERT) and selective retraction (retract sensor-reading facts while
/// keeping derived classifications and alerts).
const SENSOR_PIPELINE_RULES: &str = r#"
;;; ---------------------------------------------------------------
;;; Module: CLASSIFY
;;; Classifies raw sensor readings into severity levels.
;;; ---------------------------------------------------------------

(defmodule CLASSIFY (export ?ALL))

(deftemplate CLASSIFY::sensor-reading
    (slot sensor-id (type STRING))
    (slot value (type FLOAT)))

(deftemplate CLASSIFY::classification
    (slot sensor-id (type STRING))
    (slot level (type SYMBOL))
    (slot threshold (type FLOAT)))

(defrule CLASSIFY::classify-critical
    "Values above 100.0 are critical"
    (sensor-reading (sensor-id ?id) (value ?v&:(> ?v 100.0)))
    =>
    (assert (classification (sensor-id ?id) (level critical) (threshold 100.0))))

(defrule CLASSIFY::classify-normal
    "Values between 50.0 and 100.0 are normal"
    (sensor-reading (sensor-id ?id) (value ?v&:(and (> ?v 50.0) (<= ?v 100.0))))
    =>
    (assert (classification (sensor-id ?id) (level normal) (threshold 50.0))))

;;; ---------------------------------------------------------------
;;; Module: ALERT
;;; Derives alerts from critical classifications.
;;; ---------------------------------------------------------------

(defmodule ALERT (import CLASSIFY ?ALL) (export ?ALL))

(deftemplate ALERT::alert
    (slot sensor-id (type STRING))
    (slot severity (type SYMBOL))
    (slot message (type STRING)))

(defrule ALERT::critical-alert
    "Generate alert for critical classifications"
    (classification (sensor-id ?id) (level critical))
    =>
    (assert (alert (sensor-id ?id) (severity high) (message "Sensor exceeded critical threshold"))))
"#;

// ============================================================================
// Helpers
// ============================================================================

/// Write the multi-module sensor pipeline rules to the given directory.
fn create_sensor_pipeline_rules(dir: &std::path::Path) -> std::io::Result<()> {
    std::fs::write(dir.join("sensor-rules.clp"), SENSOR_PIPELINE_RULES)
}

/// Helper: find a conclusion by template name in a parsed response.
fn find_conclusion<'a>(
    conclusions: &'a [serde_json::Value],
    template: &str,
) -> Option<&'a serde_json::Value> {
    conclusions
        .iter()
        .find(|c| c.get("template").and_then(|t| t.as_str()) == Some(template))
}

/// Helper: count conclusions matching a template name.
fn count_conclusions(conclusions: &[serde_json::Value], template: &str) -> usize {
    conclusions
        .iter()
        .filter(|c| c.get("template").and_then(|t| t.as_str()) == Some(template))
        .count()
}

// ============================================================================
// Test 1: Full Lifecycle -- preload, introspect, infer, retract, focus
// ============================================================================

/// Exercises the complete CLIPS lifecycle across all four features in a
/// single persistent session.
///
/// Flow:
///   1. Build a persistent provider and preload the sensor-rules model.
///   2. Verify `cached_models()` lists the preloaded model.
///   3. Verify `environment_stats()` returns valid baseline stats.
///   4. Assert sensor-reading facts via `chat()` and verify conclusions.
///   5. Verify `environment_stats()` reflects the increased fact count.
///   6. Retract sensor-reading facts and verify the retract result.
///   7. Verify `environment_stats()` shows a decreased fact count.
///   8. Retract remaining derived facts to reach a clean slate, then use
///      focus to run only the CLASSIFY module (no alerts generated).
#[tokio::test]
async fn test_full_lifecycle_preload_focus_retract_introspect() {
    // ------------------------------------------------------------------
    // Setup: create temp dir with rules, build persistent provider
    // ------------------------------------------------------------------
    let temp_dir = TempDir::new().expect("Should create temp dir");
    create_sensor_pipeline_rules(temp_dir.path()).expect("Should write sensor pipeline rules");

    let provider = ClipsProvider::builder()
        .rules_directory(temp_dir.path().to_path_buf())
        .persistent(true)
        .build()
        .expect("Should build persistent provider");

    // ------------------------------------------------------------------
    // Step 1: Preload the model (warm-up)
    // ------------------------------------------------------------------
    provider
        .preload("sensor-rules.clp")
        .await
        .expect("Preload should succeed for sensor-rules.clp");

    // ------------------------------------------------------------------
    // Step 2: Verify cached_models() lists the preloaded model
    // ------------------------------------------------------------------
    let cached = provider.cached_models();
    assert!(
        cached.contains(&"sensor-rules.clp".to_string()),
        "cached_models() should include 'sensor-rules.clp' after preload, got: {:?}",
        cached
    );

    // ------------------------------------------------------------------
    // Step 3: Verify environment_stats() returns valid baseline stats
    // ------------------------------------------------------------------
    let baseline_stats = provider
        .environment_stats("sensor-rules.clp")
        .expect("environment_stats() should return Some for a preloaded model");

    assert!(
        baseline_stats.rule_count >= 1,
        "Baseline rule_count should be >= 1 (at least classify-critical), got {}",
        baseline_stats.rule_count
    );
    assert_eq!(
        baseline_stats.strategy, "depth",
        "Default conflict resolution strategy should be 'depth', got '{}'",
        baseline_stats.strategy
    );
    assert_eq!(
        baseline_stats.fact_count, 0,
        "No facts should exist immediately after preload, got {}",
        baseline_stats.fact_count
    );
    // Template count depends on how the CLIPS template iterator handles
    // multi-module templates. We require at least 1 user-defined template.
    assert!(
        baseline_stats.template_count >= 1,
        "Should have at least 1 user-defined template, got {}",
        baseline_stats.template_count
    );
    assert!(
        !baseline_stats.modules.is_empty(),
        "Modules list should be non-empty (at least MAIN, CLASSIFY, ALERT)"
    );

    // ------------------------------------------------------------------
    // Step 4: Assert facts via chat, verify conclusions
    // ------------------------------------------------------------------
    let assert_input = r#"{
        "facts": [
            {"template": "sensor-reading", "values": {"sensor-id": "T1", "value": 120.0}},
            {"template": "sensor-reading", "values": {"sensor-id": "T2", "value": 75.0}}
        ]
    }"#;

    let request = ChatRequest::new("sensor-rules.clp").with_message(Message::user(assert_input));
    let response = provider
        .chat(&request)
        .await
        .expect("Chat should succeed after asserting sensor facts");

    let output: serde_json::Value =
        serde_json::from_str(&response.content).expect("Response should be valid JSON");

    let conclusions = output
        .get("conclusions")
        .and_then(|c| c.as_array())
        .expect("Response should have a conclusions array");

    // T1 (120.0 > 100.0) should produce a critical classification and an alert.
    let critical_classification = find_conclusion(conclusions, "classification");
    assert!(
        critical_classification.is_some(),
        "Should have at least one classification conclusion, got conclusions: {:?}",
        conclusions
    );

    // Verify T1 got a critical classification
    let has_t1_critical = conclusions.iter().any(|c| {
        c.get("template").and_then(|t| t.as_str()) == Some("classification")
            && c.get("values")
                .and_then(|v| v.get("sensor-id"))
                .and_then(|s| s.as_str())
                == Some("T1")
            && (c.get("values").and_then(|v| v.get("level")).and_then(|l| {
                l.get("symbol")
                    .and_then(|s| s.as_str())
                    .or_else(|| l.as_str())
            }) == Some("critical"))
    });
    assert!(
        has_t1_critical,
        "Sensor T1 (value 120.0) should produce a critical classification"
    );

    // T2 (75.0, 50 < v <= 100) should produce a normal classification
    let has_t2_normal = conclusions.iter().any(|c| {
        c.get("template").and_then(|t| t.as_str()) == Some("classification")
            && c.get("values")
                .and_then(|v| v.get("sensor-id"))
                .and_then(|s| s.as_str())
                == Some("T2")
    });
    assert!(
        has_t2_normal,
        "Sensor T2 (value 75.0) should produce a normal classification"
    );

    // T1 critical classification should also trigger an alert from the ALERT module
    let has_alert = find_conclusion(conclusions, "alert").is_some();
    assert!(
        has_alert,
        "Critical classification for T1 should produce an alert conclusion"
    );

    // ------------------------------------------------------------------
    // Step 5: Verify environment_stats() shows increased fact_count
    // ------------------------------------------------------------------
    let post_assert_stats = provider
        .environment_stats("sensor-rules.clp")
        .expect("environment_stats() should return Some for a cached model");

    assert!(
        post_assert_stats.fact_count > baseline_stats.fact_count,
        "fact_count should increase after asserting facts; baseline={}, after={}",
        baseline_stats.fact_count,
        post_assert_stats.fact_count
    );

    // We asserted 2 sensor-readings, which should produce 2 classifications + 1 alert = 5 facts minimum
    assert!(
        post_assert_stats.fact_count >= 5,
        "Should have at least 5 facts (2 sensor-reading + 2 classification + 1 alert), got {}",
        post_assert_stats.fact_count
    );

    let fact_count_before_retract = post_assert_stats.fact_count;

    // ------------------------------------------------------------------
    // Step 6: Retract sensor-reading facts, verify retract_result
    // ------------------------------------------------------------------
    let retract_input = r#"{"command": "retract", "retract_template": "sensor-reading"}"#;
    let retract_request =
        ChatRequest::new("sensor-rules.clp").with_message(Message::user(retract_input));
    let retract_response = provider
        .chat(&retract_request)
        .await
        .expect("Retract command should succeed");

    let retract_output: serde_json::Value = serde_json::from_str(&retract_response.content)
        .expect("Retract response should be valid JSON");

    let retract_result = retract_output
        .get("retract_result")
        .expect("Response should contain retract_result");

    let retracted_map = retract_result
        .get("retracted")
        .expect("retract_result should contain retracted map");

    let sensor_retract_count = retracted_map
        .get("sensor-reading")
        .and_then(|c| c.as_u64())
        .expect("retracted map should have sensor-reading count");

    assert_eq!(
        sensor_retract_count, 2,
        "Should have retracted exactly 2 sensor-reading facts, got {}",
        sensor_retract_count
    );

    let retract_total = retract_result
        .get("total")
        .and_then(|t| t.as_u64())
        .expect("retract_result should have total");
    assert_eq!(
        retract_total, 2,
        "Total retracted should be 2, got {}",
        retract_total
    );

    // ------------------------------------------------------------------
    // Step 7: Verify environment_stats() shows decreased fact_count
    // ------------------------------------------------------------------
    let post_retract_stats = provider
        .environment_stats("sensor-rules.clp")
        .expect("environment_stats() should return Some after retraction");

    assert!(
        post_retract_stats.fact_count < fact_count_before_retract,
        "fact_count should decrease after retraction; before={}, after={}",
        fact_count_before_retract,
        post_retract_stats.fact_count
    );

    // We retracted 2 sensor-readings from 5+ facts, so should have 3+ remaining
    // (2 classifications + 1 alert)
    assert!(
        post_retract_stats.fact_count >= 3,
        "Should have at least 3 remaining facts (classifications + alert), got {}",
        post_retract_stats.fact_count
    );

    // ------------------------------------------------------------------
    // Step 8: Use focus to run only the CLASSIFY module
    // ------------------------------------------------------------------
    // Retract the remaining derived facts (classification, alert) so that
    // we start the focus step with a clean working memory. This avoids using
    // reset (which changes the CLIPS module context) and demonstrates
    // multi-template retraction as a lifecycle cleanup pattern.
    let cleanup_input =
        r#"{"command": "retract", "retract_templates": ["classification", "alert"]}"#;
    let cleanup_request =
        ChatRequest::new("sensor-rules.clp").with_message(Message::user(cleanup_input));
    provider
        .chat(&cleanup_request)
        .await
        .expect("Cleanup retraction should succeed");

    // Verify fact_count is now 0 after full retraction
    let clean_stats = provider
        .environment_stats("sensor-rules.clp")
        .expect("environment_stats() should return Some after cleanup");
    assert_eq!(
        clean_stats.fact_count, 0,
        "fact_count should be 0 after retracting all facts, got {}",
        clean_stats.fact_count
    );

    // Now assert a new sensor fact with focus restricted to CLASSIFY only.
    // Only the CLASSIFY module should fire -- the ALERT module should NOT execute.
    let focus_input = r#"{
        "focus": ["CLASSIFY"],
        "facts": [
            {"template": "sensor-reading", "values": {"sensor-id": "F1", "value": 150.0}}
        ]
    }"#;

    let focus_request =
        ChatRequest::new("sensor-rules.clp").with_message(Message::user(focus_input));
    let focus_response = provider
        .chat(&focus_request)
        .await
        .expect("Chat with focus on CLASSIFY should succeed");

    let focus_output: serde_json::Value =
        serde_json::from_str(&focus_response.content).expect("Focus response should be valid JSON");

    let focus_conclusions = focus_output
        .get("conclusions")
        .and_then(|c| c.as_array())
        .expect("Focus response should have conclusions array");

    // CLASSIFY should fire: we should see a classification conclusion
    let classification_count = count_conclusions(focus_conclusions, "classification");
    assert!(
        classification_count >= 1,
        "Focusing on CLASSIFY should produce at least 1 classification conclusion, got {}",
        classification_count
    );

    // ALERT should NOT fire: no alert conclusions should appear
    let alert_count = count_conclusions(focus_conclusions, "alert");
    assert_eq!(
        alert_count, 0,
        "Focusing on CLASSIFY only should NOT produce alert conclusions, got {}",
        alert_count
    );
}

// ============================================================================
// Test 2: Lifecycle Without Explicit Preload (on-demand caching)
// ============================================================================

/// Verifies that all four lifecycle features work correctly without an explicit
/// `preload()` call. The environment is created on-demand by the first `chat()`
/// and cached for subsequent requests.
///
/// This test confirms that preload is an optimization, not a requirement.
#[tokio::test]
async fn test_lifecycle_without_preload() {
    // ------------------------------------------------------------------
    // Setup: create temp dir with rules, build persistent provider
    // ------------------------------------------------------------------
    let temp_dir = TempDir::new().expect("Should create temp dir");
    create_sensor_pipeline_rules(temp_dir.path()).expect("Should write sensor pipeline rules");

    let provider = ClipsProvider::builder()
        .rules_directory(temp_dir.path().to_path_buf())
        .persistent(true)
        .build()
        .expect("Should build persistent provider");

    // ------------------------------------------------------------------
    // Verify: no models cached initially (no preload)
    // ------------------------------------------------------------------
    let cached_before = provider.cached_models();
    assert!(
        cached_before.is_empty(),
        "No models should be cached before any interaction, got: {:?}",
        cached_before
    );

    // environment_stats should return None for an uncached model
    let stats_before = provider.environment_stats("sensor-rules.clp");
    assert!(
        stats_before.is_none(),
        "environment_stats() should return None before the model is loaded"
    );

    // ------------------------------------------------------------------
    // Step 1: First chat triggers on-demand caching
    // ------------------------------------------------------------------
    let input = r#"{
        "facts": [
            {"template": "sensor-reading", "values": {"sensor-id": "OD1", "value": 110.0}}
        ]
    }"#;

    let request = ChatRequest::new("sensor-rules.clp").with_message(Message::user(input));
    let response = provider
        .chat(&request)
        .await
        .expect("First chat (on-demand load) should succeed");

    let output: serde_json::Value =
        serde_json::from_str(&response.content).expect("Response should be valid JSON");

    let conclusions = output
        .get("conclusions")
        .and_then(|c| c.as_array())
        .expect("Should have conclusions array");

    // Should have classification and alert (all modules fire by default)
    assert!(
        !conclusions.is_empty(),
        "On-demand chat should produce conclusions"
    );

    let has_classification = find_conclusion(conclusions, "classification").is_some();
    assert!(
        has_classification,
        "On-demand chat should produce classification conclusions"
    );

    // ------------------------------------------------------------------
    // Step 2: Verify model is now cached after on-demand load
    // ------------------------------------------------------------------
    let cached_after = provider.cached_models();
    assert!(
        cached_after.contains(&"sensor-rules.clp".to_string()),
        "Model should be cached after on-demand chat, got: {:?}",
        cached_after
    );

    // ------------------------------------------------------------------
    // Step 3: environment_stats should now work
    // ------------------------------------------------------------------
    let stats = provider
        .environment_stats("sensor-rules.clp")
        .expect("environment_stats() should return Some after on-demand cache");

    assert!(
        stats.rule_count >= 1,
        "rule_count should be >= 1 after on-demand load, got {}",
        stats.rule_count
    );
    assert_eq!(
        stats.strategy, "depth",
        "Strategy should be 'depth' (default), got '{}'",
        stats.strategy
    );
    assert!(
        stats.fact_count > 0,
        "fact_count should be > 0 after asserting facts via chat, got {}",
        stats.fact_count
    );

    let fact_count_before_retract = stats.fact_count;

    // ------------------------------------------------------------------
    // Step 4: Retract sensor-reading facts
    // ------------------------------------------------------------------
    let retract_input = r#"{"command": "retract", "retract_template": "sensor-reading"}"#;
    let retract_request =
        ChatRequest::new("sensor-rules.clp").with_message(Message::user(retract_input));
    let retract_response = provider
        .chat(&retract_request)
        .await
        .expect("Retract should succeed without preload");

    let retract_output: serde_json::Value =
        serde_json::from_str(&retract_response.content).expect("Should parse retract JSON");

    let retract_result = retract_output
        .get("retract_result")
        .expect("Should have retract_result");

    let retract_total = retract_result
        .get("total")
        .and_then(|t| t.as_u64())
        .expect("retract_result should have total");

    assert_eq!(
        retract_total, 1,
        "Should have retracted 1 sensor-reading fact, got {}",
        retract_total
    );

    // ------------------------------------------------------------------
    // Step 5: Verify decreased fact_count after retraction
    // ------------------------------------------------------------------
    let stats_after_retract = provider
        .environment_stats("sensor-rules.clp")
        .expect("environment_stats() should return Some after retraction");

    assert!(
        stats_after_retract.fact_count < fact_count_before_retract,
        "fact_count should decrease after retraction; before={}, after={}",
        fact_count_before_retract,
        stats_after_retract.fact_count
    );

    // ------------------------------------------------------------------
    // Step 6: Focus-stack control works on cached (non-preloaded) model
    // ------------------------------------------------------------------
    // Retract remaining derived facts to get a clean slate, rather than
    // using reset (which can change module context).
    let cleanup_input =
        r#"{"command": "retract", "retract_templates": ["classification", "alert"]}"#;
    let cleanup_request =
        ChatRequest::new("sensor-rules.clp").with_message(Message::user(cleanup_input));
    provider
        .chat(&cleanup_request)
        .await
        .expect("Cleanup retraction should succeed");

    let focus_input = r#"{
        "focus": ["CLASSIFY"],
        "facts": [
            {"template": "sensor-reading", "values": {"sensor-id": "OD2", "value": 200.0}}
        ]
    }"#;

    let focus_request =
        ChatRequest::new("sensor-rules.clp").with_message(Message::user(focus_input));
    let focus_response = provider
        .chat(&focus_request)
        .await
        .expect("Focused chat should succeed on on-demand cached model");

    let focus_output: serde_json::Value =
        serde_json::from_str(&focus_response.content).expect("Should parse focus JSON");

    let focus_conclusions = focus_output
        .get("conclusions")
        .and_then(|c| c.as_array())
        .expect("Focus response should have conclusions");

    // CLASSIFY should fire
    let has_classification = count_conclusions(focus_conclusions, "classification") >= 1;
    assert!(
        has_classification,
        "Focus on CLASSIFY should produce classification, got: {:?}",
        focus_conclusions
    );

    // ALERT should NOT fire
    let has_alert = count_conclusions(focus_conclusions, "alert") > 0;
    assert!(
        !has_alert,
        "Focus on CLASSIFY should NOT produce alerts, got: {:?}",
        focus_conclusions
    );
}
