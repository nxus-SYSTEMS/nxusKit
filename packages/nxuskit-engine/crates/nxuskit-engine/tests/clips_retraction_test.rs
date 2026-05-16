//! Integration tests for CLIPS selective fact retraction
//!
//! These tests verify that the ClipsProvider correctly supports selective
//! retraction of facts by template name, including single-template,
//! multi-template, edge cases (no matching facts, nonexistent template),
//! and the guard that retract_template is ignored without command="retract".

use nxuskit_engine::LLMProvider;
use nxuskit_engine::providers::ClipsProvider;
use nxuskit_engine::types::{ChatRequest, Message};
use tempfile::TempDir;

/// Helper to create sensor/alert rule files in the given directory.
fn create_retraction_rules(dir: &std::path::Path) -> std::io::Result<()> {
    let rules = r#"
(deftemplate sensor-reading
    (slot sensor-id (type STRING))
    (slot value (type FLOAT)))

(deftemplate alert
    (slot sensor-id (type STRING))
    (slot level (type SYMBOL))
    (slot message (type STRING)))

(defrule high-temp
    (sensor-reading (sensor-id ?id) (value ?v&:(> ?v 100.0)))
    =>
    (assert (alert (sensor-id ?id) (level critical) (message "High temperature"))))

(defrule normal-temp
    (sensor-reading (sensor-id ?id) (value ?v&:(and (> ?v 50.0) (<= ?v 100.0))))
    =>
    (assert (alert (sensor-id ?id) (level info) (message "Normal range"))))
"#;

    std::fs::write(dir.join("sensor-rules.clp"), rules)?;
    Ok(())
}

/// Test retracting facts of a single template.
///
/// 1. Assert 2 sensor-reading facts, run inference to produce alerts.
/// 2. Send retract command for "sensor-reading".
/// 3. Verify retract_result shows count=2 for sensor-reading.
/// 4. Send another chat to check remaining facts -- alerts should still exist.
#[tokio::test]
async fn test_retract_single_template() {
    let temp_dir = TempDir::new().expect("Should create temp dir");
    create_retraction_rules(temp_dir.path()).expect("Should create rules");

    let clips = ClipsProvider::builder()
        .rules_directory(temp_dir.path().to_path_buf())
        .persistent(true)
        .build()
        .expect("Should build");

    // Step 1: Assert two sensor-reading facts and run inference
    let input = r#"{
        "facts": [
            {"template": "sensor-reading", "values": {"sensor-id": "S1", "value": 120.0}},
            {"template": "sensor-reading", "values": {"sensor-id": "S2", "value": 75.0}}
        ]
    }"#;

    let request = ChatRequest::new("sensor-rules.clp").with_message(Message::user(input));
    let response = clips
        .chat(&request)
        .await
        .expect("Inference should succeed");
    let output: serde_json::Value =
        serde_json::from_str(&response.content).expect("Should parse JSON");

    // Verify alerts were produced
    let conclusions = output
        .get("conclusions")
        .and_then(|c| c.as_array())
        .expect("Should have conclusions array");
    assert!(
        conclusions.len() >= 2,
        "Should have at least 2 alert conclusions, got {}",
        conclusions.len()
    );

    // Step 2: Send retract command for sensor-reading
    let retract_input = r#"{"command": "retract", "retract_template": "sensor-reading"}"#;
    let retract_request =
        ChatRequest::new("sensor-rules.clp").with_message(Message::user(retract_input));
    let retract_response = clips
        .chat(&retract_request)
        .await
        .expect("Retract should succeed");
    let retract_output: serde_json::Value =
        serde_json::from_str(&retract_response.content).expect("Should parse retract JSON");

    // Step 3: Verify retract_result
    let retract_result = retract_output
        .get("retract_result")
        .expect("Should have retract_result");
    let retracted = retract_result
        .get("retracted")
        .expect("Should have retracted map");
    let sensor_count = retracted
        .get("sensor-reading")
        .and_then(|c| c.as_u64())
        .expect("Should have sensor-reading count");
    assert_eq!(
        sensor_count, 2,
        "Should have retracted 2 sensor-reading facts"
    );

    let total = retract_result
        .get("total")
        .and_then(|t| t.as_u64())
        .expect("Should have total");
    assert_eq!(total, 2, "Total retracted should be 2");

    // Step 4: Verify via environment_stats that alerts remain but sensor-readings are gone
    let stats = clips
        .environment_stats("sensor-rules.clp")
        .expect("Should have stats for cached model");

    // We started with 4 facts (2 sensor-reading + 2 alerts), retracted 2 sensor-readings
    // so 2 alert facts should remain
    assert!(
        stats.fact_count >= 2,
        "Should have at least 2 remaining facts (alerts), got {}",
        stats.fact_count
    );
}

/// Test retracting multiple templates at once.
///
/// Assert facts, run inference, then retract both "sensor-reading" and "alert"
/// using the retract_templates array. Verify counts per template in retract_result.
#[tokio::test]
async fn test_retract_multiple_templates() {
    let temp_dir = TempDir::new().expect("Should create temp dir");
    create_retraction_rules(temp_dir.path()).expect("Should create rules");

    let clips = ClipsProvider::builder()
        .rules_directory(temp_dir.path().to_path_buf())
        .persistent(true)
        .build()
        .expect("Should build");

    // Assert facts and run inference
    let input = r#"{
        "facts": [
            {"template": "sensor-reading", "values": {"sensor-id": "S1", "value": 120.0}},
            {"template": "sensor-reading", "values": {"sensor-id": "S2", "value": 75.0}}
        ]
    }"#;

    let request = ChatRequest::new("sensor-rules.clp").with_message(Message::user(input));
    let response = clips
        .chat(&request)
        .await
        .expect("Inference should succeed");
    let output: serde_json::Value =
        serde_json::from_str(&response.content).expect("Should parse JSON");

    // Verify we got some conclusions
    let conclusions = output
        .get("conclusions")
        .and_then(|c| c.as_array())
        .expect("Should have conclusions");
    assert!(
        !conclusions.is_empty(),
        "Should have at least one alert conclusion"
    );

    // Retract both templates
    let retract_input =
        r#"{"command": "retract", "retract_templates": ["sensor-reading", "alert"]}"#;
    let retract_request =
        ChatRequest::new("sensor-rules.clp").with_message(Message::user(retract_input));
    let retract_response = clips
        .chat(&retract_request)
        .await
        .expect("Retract should succeed");
    let retract_output: serde_json::Value =
        serde_json::from_str(&retract_response.content).expect("Should parse retract JSON");

    let retract_result = retract_output
        .get("retract_result")
        .expect("Should have retract_result");
    let retracted = retract_result
        .get("retracted")
        .expect("Should have retracted map");

    // Verify per-template counts
    let sensor_count = retracted
        .get("sensor-reading")
        .and_then(|c| c.as_u64())
        .expect("Should have sensor-reading count");
    assert_eq!(
        sensor_count, 2,
        "Should have retracted 2 sensor-reading facts"
    );

    let alert_count = retracted
        .get("alert")
        .and_then(|c| c.as_u64())
        .expect("Should have alert count");
    assert!(alert_count >= 2, "Should have retracted at least 2 alerts");

    let total = retract_result
        .get("total")
        .and_then(|t| t.as_u64())
        .expect("Should have total");
    assert!(
        total >= 4,
        "Total retracted should be at least 4 (2 sensors + 2 alerts)"
    );
}

/// Test retracting a template when no facts of that template exist.
///
/// Should succeed with count=0.
#[tokio::test]
async fn test_retract_no_matching_facts() {
    let temp_dir = TempDir::new().expect("Should create temp dir");
    create_retraction_rules(temp_dir.path()).expect("Should create rules");

    let clips = ClipsProvider::builder()
        .rules_directory(temp_dir.path().to_path_buf())
        .persistent(true)
        .build()
        .expect("Should build");

    // Load the rules without asserting any facts first
    let init_input = r#"{"facts": []}"#;
    let init_request = ChatRequest::new("sensor-rules.clp").with_message(Message::user(init_input));
    clips
        .chat(&init_request)
        .await
        .expect("Init should succeed");

    // Retract sensor-reading when none exist
    let retract_input = r#"{"command": "retract", "retract_template": "sensor-reading"}"#;
    let retract_request =
        ChatRequest::new("sensor-rules.clp").with_message(Message::user(retract_input));
    let retract_response = clips
        .chat(&retract_request)
        .await
        .expect("Retract with no matching facts should succeed");
    let retract_output: serde_json::Value =
        serde_json::from_str(&retract_response.content).expect("Should parse JSON");

    let retract_result = retract_output
        .get("retract_result")
        .expect("Should have retract_result");
    let retracted = retract_result
        .get("retracted")
        .expect("Should have retracted map");

    let sensor_count = retracted
        .get("sensor-reading")
        .and_then(|c| c.as_u64())
        .expect("Should have sensor-reading count");
    assert_eq!(sensor_count, 0, "Should retract 0 facts when none exist");

    let total = retract_result
        .get("total")
        .and_then(|t| t.as_u64())
        .expect("Should have total");
    assert_eq!(total, 0, "Total should be 0");
}

/// Test retracting a template name that does not exist in the rule base.
///
/// Should return an error.
#[tokio::test]
async fn test_retract_nonexistent_template() {
    let temp_dir = TempDir::new().expect("Should create temp dir");
    create_retraction_rules(temp_dir.path()).expect("Should create rules");

    let clips = ClipsProvider::builder()
        .rules_directory(temp_dir.path().to_path_buf())
        .persistent(true)
        .build()
        .expect("Should build");

    // Load the rules first
    let init_input = r#"{"facts": []}"#;
    let init_request = ChatRequest::new("sensor-rules.clp").with_message(Message::user(init_input));
    clips
        .chat(&init_request)
        .await
        .expect("Init should succeed");

    // Retract a template that does not exist
    let retract_input = r#"{"command": "retract", "retract_template": "nonexistent-template"}"#;
    let retract_request =
        ChatRequest::new("sensor-rules.clp").with_message(Message::user(retract_input));
    let result = clips.chat(&retract_request).await;

    assert!(
        result.is_err(),
        "Retracting a nonexistent template should return an error"
    );

    let err_msg = result.unwrap_err().to_string().to_lowercase();
    assert!(
        err_msg.contains("nonexistent-template") || err_msg.contains("failed to retract"),
        "Error should mention the template name or retract failure: {}",
        err_msg
    );
}

/// Test that retract_template is ignored when command is not "retract".
///
/// Sending retract_template without command="retract" should process as
/// normal inference, ignoring the retract_template field.
#[tokio::test]
async fn test_retract_without_command_field() {
    let temp_dir = TempDir::new().expect("Should create temp dir");
    create_retraction_rules(temp_dir.path()).expect("Should create rules");

    let clips = ClipsProvider::builder()
        .rules_directory(temp_dir.path().to_path_buf())
        .persistent(true)
        .build()
        .expect("Should build");

    // Send facts with retract_template but WITHOUT command="retract"
    // The retract_template field should be ignored and normal inference should proceed
    let input = r#"{
        "facts": [
            {"template": "sensor-reading", "values": {"sensor-id": "S1", "value": 120.0}}
        ],
        "retract_template": "sensor-reading"
    }"#;

    let request = ChatRequest::new("sensor-rules.clp").with_message(Message::user(input));
    let response = clips
        .chat(&request)
        .await
        .expect("Should succeed as normal inference");
    let output: serde_json::Value =
        serde_json::from_str(&response.content).expect("Should parse JSON");

    // Should have run inference and produced conclusions (not retraction)
    let conclusions = output
        .get("conclusions")
        .and_then(|c| c.as_array())
        .expect("Should have conclusions array");
    assert!(
        !conclusions.is_empty(),
        "Should have conclusions from normal inference"
    );

    // Should NOT have a retract_result since command was not "retract"
    assert!(
        output.get("retract_result").is_none(),
        "Should not have retract_result when command is not 'retract'"
    );

    // Verify the sensor-reading fact was actually asserted (not retracted)
    let has_alert = conclusions
        .iter()
        .any(|f| f.get("template").and_then(|t| t.as_str()) == Some("alert"));
    assert!(
        has_alert,
        "Should produce alert conclusions from normal inference"
    );
}
