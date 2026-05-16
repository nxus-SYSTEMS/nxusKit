//! Integration tests for CLIPS Expert System Provider
//!
//! These tests verify that ClipsProvider correctly implements the LLMProvider trait
//! and behaves according to the expected CLIPS-as-LLM paradigm:
//!
//! - Model name = CLIPS rules file (.clp)
//! - Chat start = Initialize CLIPS with rules
//! - Prompts = JSON facts → assert → run
//! - Responses = Stream-like chunks from CLIPS
//! - Multi-turn = Facts preserved within session, cleared on model change or reset
//!
//! TDD: These tests are written before implementation fixes per constitution Article III.

#![cfg(feature = "clips")]

use nxuskit_engine::provider::LLMProvider;
use nxuskit_engine::providers::clips::{
    ClipsConfig, ClipsInput, ClipsProvider, FactAssertion, JsonValue,
};
use nxuskit_engine::types::{ChatRequest, ClipsOptions, Message, ProviderOptions};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

// ============================================================================
// Test Fixtures
// ============================================================================

/// Create a temporary directory with test rule files
fn setup_test_rules() -> TempDir {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Create a simple medical triage rule base
    let medical_rules = r#"
;;; Medical Triage Rules
;;; Templates for patient data

(deftemplate patient
    (slot name (type STRING))
    (slot age (type INTEGER))
    (slot temperature (type FLOAT) (default 37.0)))

(deftemplate symptom
    (slot patient-name (type STRING))
    (slot type (type SYMBOL))
    (slot severity (type SYMBOL) (default moderate)))

(deftemplate triage
    (slot patient-name (type STRING))
    (slot level (type SYMBOL))
    (slot reason (type STRING)))

;;; Rules

(defrule fever-check
    "Check if patient has fever"
    (patient (name ?n) (temperature ?t&:(> ?t 38.0)))
    =>
    (assert (triage (patient-name ?n) (level urgent) (reason "Fever detected"))))

(defrule elderly-chest-pain
    "Elderly patient with chest pain is emergency"
    (patient (name ?n) (age ?a&:(>= ?a 60)))
    (symptom (patient-name ?n) (type chest-pain) (severity severe))
    =>
    (assert (triage (patient-name ?n) (level immediate) (reason "Elderly with severe chest pain"))))

(defrule young-mild-symptoms
    "Young patient with mild symptoms is routine"
    (patient (name ?n) (age ?a&:(< ?a 40)))
    (symptom (patient-name ?n) (severity mild))
    =>
    (assert (triage (patient-name ?n) (level routine) (reason "Young patient, mild symptoms"))))
"#;

    // Create a simple loyalty program rule base
    let loyalty_rules = r#"
;;; Customer Loyalty Rules

(deftemplate customer
    (slot id (type STRING))
    (slot years-active (type INTEGER))
    (slot total-purchases (type FLOAT)))

(deftemplate status
    (slot customer-id (type STRING))
    (slot level (type SYMBOL)))

(deftemplate discount
    (slot customer-id (type STRING))
    (slot percentage (type INTEGER)))

(defrule established-customer
    "Customer with 2+ years is established"
    (customer (id ?id) (years-active ?y&:(>= ?y 2)))
    =>
    (assert (status (customer-id ?id) (level established))))

(defrule gold-status
    "Established customer with $5000+ spending gets Gold"
    (customer (id ?id) (total-purchases ?p&:(>= ?p 5000.0)))
    (status (customer-id ?id) (level established))
    =>
    (assert (status (customer-id ?id) (level gold))))

(defrule gold-discount
    "Gold customers get 15% discount"
    (status (customer-id ?id) (level gold))
    =>
    (assert (discount (customer-id ?id) (percentage 15))))
"#;

    fs::write(temp_dir.path().join("medical.clp"), medical_rules)
        .expect("Failed to write medical rules");
    fs::write(temp_dir.path().join("loyalty.clp"), loyalty_rules)
        .expect("Failed to write loyalty rules");

    temp_dir
}

// ============================================================================
// Contract Tests: LLMProvider Trait Implementation
// ============================================================================

mod contract_tests {
    use super::*;

    #[tokio::test]
    async fn provider_name_returns_clips() {
        let temp_dir = setup_test_rules();
        let provider = ClipsProvider::builder()
            .rules_directory(temp_dir.path())
            .build()
            .expect("Failed to create provider");

        assert_eq!(provider.provider_name(), "clips");
    }

    #[tokio::test]
    async fn list_models_finds_clp_files() {
        let temp_dir = setup_test_rules();
        let provider = ClipsProvider::builder()
            .rules_directory(temp_dir.path())
            .build()
            .expect("Failed to create provider");

        let models = provider.list_models().await.expect("Failed to list models");

        assert!(
            models.len() >= 2,
            "Should find at least medical.clp and loyalty.clp"
        );

        let model_names: Vec<&str> = models.iter().map(|m| m.name.as_str()).collect();
        assert!(model_names.contains(&"medical"), "Should find medical.clp");
        assert!(model_names.contains(&"loyalty"), "Should find loyalty.clp");
    }

    #[tokio::test]
    async fn chat_returns_valid_response() {
        let temp_dir = setup_test_rules();
        let provider = ClipsProvider::builder()
            .rules_directory(temp_dir.path())
            .build()
            .expect("Failed to create provider");

        let input = r#"{
            "facts": [
                {"template": "patient", "values": {"name": "John", "age": 65, "temperature": 38.5}}
            ]
        }"#;

        let request = ChatRequest::new("medical.clp").with_message(Message::user(input));

        let response = provider.chat(&request).await.expect("Chat should succeed");

        // Verify response structure
        assert!(!response.content.is_empty(), "Response should have content");
        assert_eq!(response.model, "medical.clp", "Model should match request");

        // Response should be valid JSON
        let output: serde_json::Value =
            serde_json::from_str(&response.content).expect("Response should be valid JSON");

        // Should have conclusions array
        assert!(
            output.get("conclusions").is_some(),
            "Should have conclusions"
        );
    }

    #[tokio::test]
    async fn chat_stream_returns_chunks() {
        use futures::StreamExt;

        let temp_dir = setup_test_rules();
        let provider = ClipsProvider::builder()
            .rules_directory(temp_dir.path())
            .build()
            .expect("Failed to create provider");

        // Note: "type" and "severity" are SYMBOL typed, so we use {"symbol": "..."} syntax
        let input = r#"{
            "facts": [
                {"template": "patient", "values": {"name": "Jane", "age": 25, "temperature": 37.0}},
                {"template": "symptom", "values": {"patient-name": "Jane", "type": {"symbol": "headache"}, "severity": {"symbol": "mild"}}}
            ]
        }"#;

        let request = ChatRequest::new("medical.clp").with_message(Message::user(input));

        let mut stream = provider
            .chat_stream(&request)
            .await
            .expect("Stream should be created");

        let mut chunks = Vec::new();
        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.expect("Chunk should be valid");
            chunks.push(chunk);
        }

        assert!(!chunks.is_empty(), "Should receive at least one chunk");

        // Last chunk should have finish_reason
        let last_chunk = chunks.last().expect("Should have last chunk");
        assert!(
            last_chunk.finish_reason.is_some(),
            "Last chunk should have finish_reason"
        );
    }
}

// ============================================================================
// Integration Tests: Session Management
// ============================================================================

mod session_tests {
    use super::*;

    #[tokio::test]
    async fn facts_persist_across_multiple_chats_same_model() {
        let temp_dir = setup_test_rules();
        // Enable persistent mode so facts persist between chats
        let provider = ClipsProvider::builder()
            .rules_directory(temp_dir.path())
            .persistent(true)
            .build()
            .expect("Failed to create provider");

        // First chat: assert customer with $2000 (triggers established status only)
        let input1 = r#"{
            "facts": [
                {"template": "customer", "values": {"id": "C001", "years-active": 3, "total-purchases": 2000.0}}
            ]
        }"#;

        let request1 = ChatRequest::new("loyalty.clp").with_message(Message::user(input1));

        let response1 = provider
            .chat(&request1)
            .await
            .expect("First chat should succeed");
        let output1: serde_json::Value = serde_json::from_str(&response1.content).unwrap();

        // Should derive "established" status (3 years >= 2)
        let conclusions1 = output1
            .get("conclusions")
            .and_then(|c| c.as_array())
            .unwrap();
        assert!(
            conclusions1
                .iter()
                .any(|c| { c.get("template").and_then(|t| t.as_str()) == Some("status") }),
            "Should derive status from first chat"
        );
        // Should NOT derive discount yet ($2000 < $5000)
        assert!(
            !conclusions1
                .iter()
                .any(|c| { c.get("template").and_then(|t| t.as_str()) == Some("discount") }),
            "Should NOT derive discount from first chat (insufficient purchases)"
        );

        // Second chat: add a second customer with high purchases
        // The first customer's facts should PERSIST and status should still exist
        let input2 = r#"{
            "facts": [
                {"template": "customer", "values": {"id": "C001", "years-active": 3, "total-purchases": 6000.0}}
            ],
            "config": {"derived_only_new": true}
        }"#;

        let request2 = ChatRequest::new("loyalty.clp").with_message(Message::user(input2));

        let response2 = provider
            .chat(&request2)
            .await
            .expect("Second chat should succeed");
        let output2: serde_json::Value = serde_json::from_str(&response2.content).unwrap();

        // Should now derive gold status AND discount (cumulative inference)
        let conclusions2 = output2
            .get("conclusions")
            .and_then(|c| c.as_array())
            .unwrap();

        // With derived_only_new, should only show NEW conclusions from this run
        // Gold status and discount should be new
        assert!(
            conclusions2
                .iter()
                .any(|c| { c.get("template").and_then(|t| t.as_str()) == Some("discount") }),
            "Should derive discount in second chat (gold customer)"
        );
    }

    #[tokio::test]
    async fn facts_cleared_when_model_changes() {
        let temp_dir = setup_test_rules();
        let provider = ClipsProvider::builder()
            .rules_directory(temp_dir.path())
            .build()
            .expect("Failed to create provider");

        // First chat with loyalty rules
        let input1 = r#"{
            "facts": [
                {"template": "customer", "values": {"id": "C001", "years-active": 5, "total-purchases": 10000.0}}
            ]
        }"#;

        let request1 = ChatRequest::new("loyalty.clp").with_message(Message::user(input1));

        provider
            .chat(&request1)
            .await
            .expect("First chat should succeed");

        // Switch to medical rules - should start fresh
        let input2 = r#"{
            "facts": [
                {"template": "patient", "values": {"name": "Alice", "age": 30, "temperature": 37.0}}
            ]
        }"#;

        let request2 = ChatRequest::new("medical.clp").with_message(Message::user(input2));

        let response2 = provider
            .chat(&request2)
            .await
            .expect("Second chat should succeed");
        let output2: serde_json::Value = serde_json::from_str(&response2.content).unwrap();

        // Should only have medical facts, no customer/loyalty data
        let conclusions = output2
            .get("conclusions")
            .and_then(|c| c.as_array())
            .unwrap();

        // Verify no loyalty-related conclusions leaked
        assert!(
            !conclusions.iter().any(|c| {
                let template = c.get("template").and_then(|t| t.as_str()).unwrap_or("");
                template == "status" || template == "discount"
            }),
            "Loyalty facts should not leak into medical session"
        );
    }

    #[tokio::test]
    async fn explicit_reset_clears_facts() {
        let temp_dir = setup_test_rules();
        let provider = ClipsProvider::builder()
            .rules_directory(temp_dir.path())
            .build()
            .expect("Failed to create provider");

        // First chat: assert customer
        let input1 = r#"{
            "facts": [
                {"template": "customer", "values": {"id": "C001", "years-active": 3, "total-purchases": 6000.0}}
            ]
        }"#;

        let request1 = ChatRequest::new("loyalty.clp").with_message(Message::user(input1));

        provider
            .chat(&request1)
            .await
            .expect("First chat should succeed");

        // Second chat: send reset command
        let input2 = r#"{
            "command": "reset"
        }"#;

        let request2 = ChatRequest::new("loyalty.clp").with_message(Message::user(input2));

        provider
            .chat(&request2)
            .await
            .expect("Reset should succeed");

        // Third chat: assert different customer - should have clean slate
        let input3 = r#"{
            "facts": [
                {"template": "customer", "values": {"id": "C002", "years-active": 1, "total-purchases": 100.0}}
            ]
        }"#;

        let request3 = ChatRequest::new("loyalty.clp").with_message(Message::user(input3));

        let response3 = provider
            .chat(&request3)
            .await
            .expect("Third chat should succeed");
        let output3: serde_json::Value = serde_json::from_str(&response3.content).unwrap();

        let conclusions = output3
            .get("conclusions")
            .and_then(|c| c.as_array())
            .unwrap();

        // C002 doesn't qualify for any status (1 year < 2 years required)
        // If facts from C001 leaked, we'd see gold/discount conclusions
        assert!(
            conclusions.is_empty()
                || !conclusions
                    .iter()
                    .any(|c| { c.get("template").and_then(|t| t.as_str()) == Some("status") }),
            "C001 facts should not affect C002 after reset"
        );
    }

    #[tokio::test]
    async fn derived_only_new_returns_only_new_facts() {
        let temp_dir = setup_test_rules();
        let provider = ClipsProvider::builder()
            .rules_directory(temp_dir.path())
            .build()
            .expect("Failed to create provider");

        // First chat: patient triggers fever rule
        let input1 = r#"{
            "facts": [
                {"template": "patient", "values": {"name": "Bob", "age": 45, "temperature": 39.0}}
            ]
        }"#;

        let request1 = ChatRequest::new("medical.clp").with_message(Message::user(input1));

        let response1 = provider
            .chat(&request1)
            .await
            .expect("First chat should succeed");
        let output1: serde_json::Value = serde_json::from_str(&response1.content).unwrap();
        let conclusions1 = output1
            .get("conclusions")
            .and_then(|c| c.as_array())
            .unwrap();

        // Should have fever triage
        assert!(
            conclusions1.iter().any(|c| {
                c.get("values")
                    .and_then(|v| v.get("reason"))
                    .and_then(|r| r.as_str())
                    .map(|s| s.contains("Fever"))
                    .unwrap_or(false)
            }),
            "First chat should derive fever triage"
        );

        let first_count = conclusions1.len();

        // Second chat: add symptom - should only get NEW conclusions
        // Note: "type" and "severity" are SYMBOL typed, so we use {"symbol": "..."} syntax
        let input2 = r#"{
            "facts": [
                {"template": "symptom", "values": {"patient-name": "Bob", "type": {"symbol": "cough"}, "severity": {"symbol": "mild"}}}
            ],
            "config": {"derived_only_new": true}
        }"#;

        let request2 = ChatRequest::new("medical.clp").with_message(Message::user(input2));

        let response2 = provider
            .chat(&request2)
            .await
            .expect("Second chat should succeed");
        let output2: serde_json::Value = serde_json::from_str(&response2.content).unwrap();
        let conclusions2 = output2
            .get("conclusions")
            .and_then(|c| c.as_array())
            .unwrap();

        // Should NOT re-include the fever triage from first run
        // (unless mild cough triggered a new rule)
        for conclusion in conclusions2 {
            let reason = conclusion
                .get("values")
                .and_then(|v| v.get("reason"))
                .and_then(|r| r.as_str())
                .unwrap_or("");

            // Fever triage was already derived, shouldn't appear again
            if reason.contains("Fever") {
                panic!("Fever triage should not be re-reported with derived_only_new");
            }
        }
    }
}

// ============================================================================
// Integration Tests: Streaming Modes
// ============================================================================

mod streaming_tests {
    use super::*;
    use futures::StreamExt;

    #[tokio::test]
    async fn streaming_mode_default_single_chunk() {
        let temp_dir = setup_test_rules();
        let provider = ClipsProvider::builder()
            .rules_directory(temp_dir.path())
            .build()
            .expect("Failed to create provider");

        // Note: "type" and "severity" are SYMBOL typed, so we use {"symbol": "..."} syntax
        let input = r#"{
            "facts": [
                {"template": "patient", "values": {"name": "Test", "age": 70, "temperature": 39.5}},
                {"template": "symptom", "values": {"patient-name": "Test", "type": {"symbol": "chest-pain"}, "severity": {"symbol": "severe"}}}
            ]
        }"#;

        let request = ChatRequest::new("medical.clp").with_message(Message::user(input));

        let mut stream = provider
            .chat_stream(&request)
            .await
            .expect("Stream should be created");

        let mut chunks = Vec::new();
        while let Some(chunk_result) = stream.next().await {
            chunks.push(chunk_result.expect("Chunk should be valid"));
        }

        // Default mode (D): single chunk with all results
        // Multiple rules may fire (fever + elderly chest pain), but single chunk
        assert_eq!(
            chunks.len(),
            1,
            "Default streaming mode should return single chunk"
        );

        let content = &chunks[0].delta;
        let output: serde_json::Value =
            serde_json::from_str(content).expect("Chunk content should be valid JSON");

        // Should contain all conclusions
        let conclusions = output
            .get("conclusions")
            .and_then(|c| c.as_array())
            .unwrap();
        assert!(
            conclusions.len() >= 2,
            "Should have multiple conclusions in single chunk"
        );
    }

    #[tokio::test]
    async fn streaming_mode_fact_per_chunk() {
        let temp_dir = setup_test_rules();
        let provider = ClipsProvider::builder()
            .rules_directory(temp_dir.path())
            .build()
            .expect("Failed to create provider");

        let input = r#"{
            "facts": [
                {"template": "customer", "values": {"id": "C001", "years-active": 5, "total-purchases": 10000.0}}
            ],
            "config": {"stream_mode": "fact"}
        }"#;

        let request = ChatRequest::new("loyalty.clp").with_message(Message::user(input));

        let mut stream = provider
            .chat_stream(&request)
            .await
            .expect("Stream should be created");

        let mut chunks = Vec::new();
        while let Some(chunk_result) = stream.next().await {
            chunks.push(chunk_result.expect("Chunk should be valid"));
        }

        // Mode A: one derived fact per chunk
        // Customer with 5 years and $10000 should derive:
        // 1. established status (2+ years)
        // 2. gold status (established + $5000+)
        // 3. discount (gold status)
        // = 3 derived facts = 3 chunks (plus possible final empty chunk)

        assert!(
            chunks.len() >= 3,
            "Fact-per-chunk mode should return multiple chunks"
        );

        // Each chunk (except possibly last) should contain a single fact
        for (i, chunk) in chunks.iter().enumerate() {
            if chunk.finish_reason.is_some() && chunk.delta.is_empty() {
                continue; // Final marker chunk
            }

            let fact: serde_json::Value = serde_json::from_str(&chunk.delta)
                .expect(&format!("Chunk {} should be valid JSON", i));

            // Each chunk is a single fact object, not an array
            assert!(
                fact.get("template").is_some(),
                "Each chunk should be a single fact with template field"
            );
        }
    }

    #[tokio::test]
    async fn streaming_mode_rule_per_chunk() {
        let temp_dir = setup_test_rules();
        let provider = ClipsProvider::builder()
            .rules_directory(temp_dir.path())
            .build()
            .expect("Failed to create provider");

        let input = r#"{
            "facts": [
                {"template": "customer", "values": {"id": "C001", "years-active": 5, "total-purchases": 10000.0}}
            ],
            "config": {"stream_mode": "rule"}
        }"#;

        let request = ChatRequest::new("loyalty.clp").with_message(Message::user(input));

        let mut stream = provider
            .chat_stream(&request)
            .await
            .expect("Stream should be created");

        let mut chunks = Vec::new();
        while let Some(chunk_result) = stream.next().await {
            chunks.push(chunk_result.expect("Chunk should be valid"));
        }

        // Mode B: one rule firing per chunk (with resulting facts)
        // Should have one chunk per rule that fired

        for chunk in &chunks {
            if chunk.finish_reason.is_some() && chunk.delta.is_empty() {
                continue; // Final marker chunk
            }

            let rule_output: serde_json::Value =
                serde_json::from_str(&chunk.delta).expect("Chunk should be valid JSON");

            // Each chunk should have rule_name and resulting facts
            assert!(
                rule_output.get("rule_name").is_some() || rule_output.get("facts").is_some(),
                "Rule-per-chunk should include rule info or facts"
            );
        }
    }
}

// ============================================================================
// Integration Tests: Error Handling
// ============================================================================

mod error_tests {
    use super::*;

    #[tokio::test]
    async fn missing_rules_file_returns_error() {
        let temp_dir = setup_test_rules();
        let provider = ClipsProvider::builder()
            .rules_directory(temp_dir.path())
            .build()
            .expect("Failed to create provider");

        let input = r#"{"facts": []}"#;

        let request = ChatRequest::new("nonexistent.clp").with_message(Message::user(input));

        let result = provider.chat(&request).await;

        assert!(result.is_err(), "Should fail for missing rules file");
        let err = result.unwrap_err();
        let err_msg = err.to_string().to_lowercase();
        assert!(
            err_msg.contains("not found") || err_msg.contains("failed to load"),
            "Error should mention file not found: {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn invalid_json_input_returns_error() {
        let temp_dir = setup_test_rules();
        let provider = ClipsProvider::builder()
            .rules_directory(temp_dir.path())
            .build()
            .expect("Failed to create provider");

        let request = ChatRequest::new("medical.clp").with_message(Message::user("not valid json"));

        let result = provider.chat(&request).await;

        assert!(result.is_err(), "Should fail for invalid JSON input");
        let err = result.unwrap_err();
        let err_msg = err.to_string().to_lowercase();
        assert!(
            err_msg.contains("json") || err_msg.contains("parse"),
            "Error should mention JSON parsing: {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn unknown_template_returns_error() {
        let temp_dir = setup_test_rules();
        let provider = ClipsProvider::builder()
            .rules_directory(temp_dir.path())
            .auto_generate_templates(false) // Disable auto-generation
            .build()
            .expect("Failed to create provider");

        let input = r#"{
            "facts": [
                {"template": "nonexistent_template", "values": {"foo": "bar"}}
            ]
        }"#;

        let request = ChatRequest::new("medical.clp").with_message(Message::user(input));

        let result = provider.chat(&request).await;

        assert!(result.is_err(), "Should fail for unknown template");
        let err = result.unwrap_err();
        let err_msg = err.to_string().to_lowercase();
        assert!(
            err_msg.contains("template") && err_msg.contains("not found"),
            "Error should mention template not found: {}",
            err_msg
        );
    }
}

// ============================================================================
// Integration Tests: Edge Cases
// ============================================================================

mod edge_case_tests {
    use super::*;

    #[tokio::test]
    async fn empty_facts_runs_without_error() {
        let temp_dir = setup_test_rules();
        let provider = ClipsProvider::builder()
            .rules_directory(temp_dir.path())
            .build()
            .expect("Failed to create provider");

        let input = r#"{"facts": []}"#;

        let request = ChatRequest::new("medical.clp").with_message(Message::user(input));

        let response = provider
            .chat(&request)
            .await
            .expect("Empty facts should succeed");
        let output: serde_json::Value = serde_json::from_str(&response.content).unwrap();

        let conclusions = output
            .get("conclusions")
            .and_then(|c| c.as_array())
            .unwrap();
        assert!(
            conclusions.is_empty(),
            "No facts should produce no conclusions"
        );
    }

    #[tokio::test]
    async fn multiple_rule_files_can_be_loaded() {
        let temp_dir = setup_test_rules();
        let provider = ClipsProvider::builder()
            .rules_directory(temp_dir.path())
            .build()
            .expect("Failed to create provider");

        // Load both rule files (comma-separated model name)
        let input = r#"{
            "facts": [
                {"template": "patient", "values": {"name": "Multi", "age": 50, "temperature": 38.5}},
                {"template": "customer", "values": {"id": "Multi", "years-active": 3, "total-purchases": 6000.0}}
            ]
        }"#;

        let request =
            ChatRequest::new("medical.clp,loyalty.clp").with_message(Message::user(input));

        let response = provider
            .chat(&request)
            .await
            .expect("Multiple rule files should load");
        let output: serde_json::Value = serde_json::from_str(&response.content).unwrap();

        let conclusions = output
            .get("conclusions")
            .and_then(|c| c.as_array())
            .unwrap();

        // Should have conclusions from both rule bases
        let has_triage = conclusions
            .iter()
            .any(|c| c.get("template").and_then(|t| t.as_str()) == Some("triage"));
        let has_status = conclusions
            .iter()
            .any(|c| c.get("template").and_then(|t| t.as_str()) == Some("status"));

        assert!(has_triage, "Should have medical triage conclusions");
        assert!(has_status, "Should have loyalty status conclusions");
    }

    #[tokio::test]
    async fn include_trace_shows_rule_firings() {
        let temp_dir = setup_test_rules();
        let provider = ClipsProvider::builder()
            .rules_directory(temp_dir.path())
            .include_trace(true)
            .build()
            .expect("Failed to create provider");

        let input = r#"{
            "facts": [
                {"template": "patient", "values": {"name": "Trace", "age": 70, "temperature": 39.0}}
            ]
        }"#;

        let request = ChatRequest::new("medical.clp").with_message(Message::user(input));

        let response = provider.chat(&request).await.expect("Chat should succeed");
        let output: serde_json::Value = serde_json::from_str(&response.content).unwrap();

        let trace = output
            .get("trace")
            .expect("Should have trace when include_trace=true");
        let rules_fired = trace.get("rules_fired").and_then(|r| r.as_array()).unwrap();

        assert!(!rules_fired.is_empty(), "Trace should show fired rules");

        // Should have rule name in trace
        let has_fever_rule = rules_fired.iter().any(|r| {
            r.get("rule_name")
                .and_then(|n| n.as_str())
                .map(|s| s.contains("fever"))
                .unwrap_or(false)
        });

        assert!(has_fever_rule, "Trace should include fever-check rule");
    }
}

// ============================================================================
// Integration Tests: Provider Options (Strategy and allow_duplicate_facts)
// ============================================================================

mod provider_options_tests {
    use super::*;

    #[tokio::test]
    async fn test_clips_options_strategy_depth() {
        let temp_dir = setup_test_rules();
        let provider = ClipsProvider::builder()
            .rules_directory(temp_dir.path())
            .build()
            .expect("Failed to create provider");

        let input = r#"{
            "facts": [
                {"template": "patient", "values": {"name": "Alice", "age": 65, "temperature": 39.0}}
            ]
        }"#;

        let clips_options = ClipsOptions {
            strategy: Some("depth".to_string()),
            allow_duplicate_facts: None,
        };

        let request = ChatRequest::new("medical.clp")
            .with_message(Message::user(input))
            .with_provider_options(ProviderOptions::Clips(clips_options));

        let response = provider.chat(&request).await.expect("Chat should succeed");
        let output: serde_json::Value = serde_json::from_str(&response.content).unwrap();

        // Should still produce conclusions with depth strategy
        let conclusions = output
            .get("conclusions")
            .and_then(|c| c.as_array())
            .unwrap();
        assert!(
            !conclusions.is_empty(),
            "Should have conclusions with depth strategy"
        );
    }

    #[tokio::test]
    async fn test_clips_options_strategy_breadth() {
        let temp_dir = setup_test_rules();
        let provider = ClipsProvider::builder()
            .rules_directory(temp_dir.path())
            .build()
            .expect("Failed to create provider");

        let input = r#"{
            "facts": [
                {"template": "patient", "values": {"name": "Bob", "age": 70, "temperature": 39.5}}
            ]
        }"#;

        let clips_options = ClipsOptions {
            strategy: Some("breadth".to_string()),
            allow_duplicate_facts: None,
        };

        let request = ChatRequest::new("medical.clp")
            .with_message(Message::user(input))
            .with_provider_options(ProviderOptions::Clips(clips_options));

        let response = provider.chat(&request).await.expect("Chat should succeed");
        let output: serde_json::Value = serde_json::from_str(&response.content).unwrap();

        // Should still produce conclusions with breadth strategy
        let conclusions = output
            .get("conclusions")
            .and_then(|c| c.as_array())
            .unwrap();
        assert!(
            !conclusions.is_empty(),
            "Should have conclusions with breadth strategy"
        );
    }

    #[tokio::test]
    async fn test_clips_options_invalid_strategy() {
        let temp_dir = setup_test_rules();
        let provider = ClipsProvider::builder()
            .rules_directory(temp_dir.path())
            .build()
            .expect("Failed to create provider");

        let input = r#"{
            "facts": [
                {"template": "patient", "values": {"name": "Test", "age": 30, "temperature": 37.0}}
            ]
        }"#;

        let clips_options = ClipsOptions {
            strategy: Some("invalid_strategy".to_string()),
            allow_duplicate_facts: None,
        };

        let request = ChatRequest::new("medical.clp")
            .with_message(Message::user(input))
            .with_provider_options(ProviderOptions::Clips(clips_options));

        let result = provider.chat(&request).await;
        assert!(result.is_err(), "Invalid strategy should return an error");

        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Unknown strategy"),
            "Error should mention unknown strategy: {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn test_clips_options_allow_duplicate_facts_false() {
        let temp_dir = setup_test_rules();

        // Use persistent mode to test duplicate facts across calls
        let provider = ClipsProvider::builder()
            .rules_directory(temp_dir.path())
            .persistent(true)
            .build()
            .expect("Failed to create provider");

        let input = r#"{
            "facts": [
                {"template": "patient", "values": {"name": "Charlie", "age": 45, "temperature": 37.0}}
            ]
        }"#;

        let clips_options = ClipsOptions {
            strategy: None,
            allow_duplicate_facts: Some(false),
        };

        let request = ChatRequest::new("medical.clp")
            .with_message(Message::user(input))
            .with_provider_options(ProviderOptions::Clips(clips_options.clone()));

        // First assertion should succeed
        let response1 = provider
            .chat(&request)
            .await
            .expect("First chat should succeed");
        let output1: serde_json::Value = serde_json::from_str(&response1.content).unwrap();
        let input_count1 = output1
            .get("stats")
            .and_then(|s| s.get("input_facts_count"))
            .and_then(|c| c.as_u64())
            .unwrap_or(0);
        assert_eq!(input_count1, 1, "First assertion should have 1 input fact");

        // Second assertion of same fact - with allow_duplicate_facts=false,
        // CLIPS should reject the duplicate (default behavior)
        // The fact won't be asserted again but we won't get an error
        let response2 = provider
            .chat(&request)
            .await
            .expect("Second chat should succeed");
        let output2: serde_json::Value = serde_json::from_str(&response2.content).unwrap();

        // The stats should show we attempted to add a fact
        // (behavior depends on CLIPS - duplicate assertion may silently fail)
        assert!(
            output2.get("conclusions").is_some(),
            "Should still return valid output"
        );
    }

    #[tokio::test]
    async fn test_clips_options_all_strategies() {
        let temp_dir = setup_test_rules();

        let strategies = vec![
            "depth",
            "breadth",
            "random",
            "complexity",
            "simplicity",
            "lex",
            "mea",
        ];

        for strategy in strategies {
            let provider = ClipsProvider::builder()
                .rules_directory(temp_dir.path())
                .build()
                .expect("Failed to create provider");

            let input = r#"{
                "facts": [
                    {"template": "patient", "values": {"name": "Test", "age": 65, "temperature": 39.0}}
                ]
            }"#;

            let clips_options = ClipsOptions {
                strategy: Some(strategy.to_string()),
                allow_duplicate_facts: None,
            };

            let request = ChatRequest::new("medical.clp")
                .with_message(Message::user(input))
                .with_provider_options(ProviderOptions::Clips(clips_options));

            let result = provider.chat(&request).await;
            assert!(
                result.is_ok(),
                "Strategy '{}' should be valid: {:?}",
                strategy,
                result.err()
            );
        }
    }
}
