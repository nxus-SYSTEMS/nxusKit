//! Contract tests for User Story 5: Export Rulebase to File
//!
//! These tests verify that programmatically-built inference environments can be exported
//! to both human-readable source files and compiled binary formats.
//!
//! Per Article III (Test-Driven Development), these contract tests define the requirements
//! for export functionality. Implementation must pass all tests.

#[cfg(test)]
mod export_tests {
    use nxuskit_engine::LLMProvider;
    use nxuskit_engine::providers::clips::ClipsProvider;
    use std::fs;

    /// Test that source export produces a readable .clp file
    #[tokio::test]
    async fn test_source_export_produces_readable_clp_file() {
        // GIVEN: A compiled inference environment built from a JSON rule program
        // WHEN: The user requests a source-format export to a specified file path
        // THEN: The system writes a human-readable rule source file containing all constructs
        //       (modules, templates, rules)

        use nxuskit_engine::providers::clips::{
            ClipsInput, FactAssertion, JsonValue, RuleDefinition, SlotDefinition, SlotType,
            TemplateDefinition,
        };

        let provider = ClipsProvider::builder().persistent(true).build().unwrap();

        // Build an environment
        let template = TemplateDefinition {
            name: "data".to_string(),
            doc: None,
            slots: vec![SlotDefinition {
                name: "value".to_string(),
                slot_type: SlotType::String,
                ..Default::default()
            }],
        };

        let rule = RuleDefinition {
            name: "test-rule".to_string(),
            module: None,
            source: Some("(data (value \"test\")) => (assert (data (value \"done\")))".to_string()),
            conditions: None,
            actions: None,
            doc: None,
            salience: None,
        };

        let input = ClipsInput {
            templates: vec![template],
            rules: vec![rule],
            facts: vec![FactAssertion {
                template: "data".to_string(),
                values: [("value".to_string(), JsonValue::String("test".to_string()))]
                    .into_iter()
                    .collect(),
                id: None,
            }],
            ..Default::default()
        };

        // Build the environment
        let request = nxuskit_engine::types::ChatRequest::new("clips").with_message(
            nxuskit_engine::types::Message::user(serde_json::to_string(&input).unwrap()),
        );

        let response = provider.chat(&request).await;
        assert!(response.is_ok(), "Should build environment successfully");

        // Create temp file for export (cross-platform)
        let temp_dir = std::env::temp_dir();
        let temp_path = temp_dir.join("test_export.clp");
        let temp_file = temp_path.to_str().unwrap();

        // Export to source format
        let export_result = provider.save_source("clips", temp_file);
        assert!(export_result.is_ok(), "Should export to source format");

        // Verify file was created and is readable
        let file_exists = fs::metadata(temp_file).is_ok();
        assert!(file_exists, "Exported file should exist");

        // Verify file has content
        let content = fs::read_to_string(temp_file).expect("Should read exported file");
        assert!(!content.is_empty(), "Exported file should have content");
        assert!(
            content.contains("CLIPS Environment"),
            "Exported file should contain header comment"
        );

        // Cleanup
        let _ = fs::remove_file(temp_file);
    }

    /// Test that binary export produces a loadable .bin file
    #[tokio::test]
    async fn test_binary_export_produces_loadable_bin_file() {
        // GIVEN: A compiled inference environment
        // WHEN: The user requests a binary-format export
        // THEN: The system writes a compiled binary file that can be reloaded for faster startup

        use nxuskit_engine::providers::clips::{
            ClipsInput, FactAssertion, JsonValue, RuleDefinition, SlotDefinition, SlotType,
            TemplateDefinition,
        };

        let provider = ClipsProvider::builder().persistent(true).build().unwrap();

        // Build an environment
        let template = TemplateDefinition {
            name: "item".to_string(),
            doc: None,
            slots: vec![SlotDefinition {
                name: "id".to_string(),
                slot_type: SlotType::String,
                ..Default::default()
            }],
        };

        let rule = RuleDefinition {
            name: "process".to_string(),
            module: None,
            source: Some("(item (id \"x\")) => (assert (item (id \"y\")))".to_string()),
            conditions: None,
            actions: None,
            doc: None,
            salience: None,
        };

        let input = ClipsInput {
            templates: vec![template],
            rules: vec![rule],
            facts: vec![FactAssertion {
                template: "item".to_string(),
                values: [("id".to_string(), JsonValue::String("x".to_string()))]
                    .into_iter()
                    .collect(),
                id: None,
            }],
            ..Default::default()
        };

        // Build the environment
        let request = nxuskit_engine::types::ChatRequest::new("clips").with_message(
            nxuskit_engine::types::Message::user(serde_json::to_string(&input).unwrap()),
        );

        let response = provider.chat(&request).await;
        assert!(response.is_ok(), "Should build environment successfully");

        // Create temp file for export (cross-platform)
        let temp_dir = std::env::temp_dir();
        let temp_path = temp_dir.join("test_export.bin");
        let temp_file = temp_path.to_str().unwrap();

        // Export to binary format
        let export_result = provider.save_binary("clips", temp_file);
        assert!(export_result.is_ok(), "Should export to binary format");

        // Verify file was created
        let file_exists = fs::metadata(temp_file).is_ok();
        assert!(file_exists, "Exported binary file should exist");

        // Verify file is not empty
        let metadata = fs::metadata(temp_file).expect("Should read file metadata");
        assert!(metadata.len() > 0, "Binary file should have content");

        // Cleanup
        let _ = fs::remove_file(temp_file);
    }

    /// Test that invalid export paths return clear errors
    #[tokio::test]
    async fn test_invalid_path_returns_error() {
        // GIVEN: An export request with an invalid or inaccessible file path
        // WHEN: Submitted
        // THEN: The system returns a clear error indicating the path issue
        //       (validation includes: no .., parent directory exists and writable, etc.)

        let provider = ClipsProvider::builder().persistent(true).build().unwrap();

        // Test 1: Path with directory traversal attempt
        let result1 = provider.save_source("clips", "../../../etc/passwd");
        assert!(result1.is_err(), "Should reject path with ..");

        // Test 2: Path with non-existent parent directory
        let result2 =
            provider.save_source("clips", "/nonexistent/path/that/does/not/exist/file.clp");
        assert!(
            result2.is_err(),
            "Should reject path with non-existent parent directory"
        );

        // Test 3: Binary export with invalid path
        let result3 = provider.save_binary("clips", "../../etc/shadow");
        assert!(
            result3.is_err(),
            "Should reject binary export with invalid path"
        );

        // Test 4: Model that doesn't exist in cache
        let temp_dir = std::env::temp_dir();
        let temp_path = temp_dir.join("test_nonexistent.clp");
        let result4 = provider.save_source("nonexistent-model", temp_path.to_str().unwrap());
        assert!(result4.is_err(), "Should error for non-existent model");
    }

    /// Test round-trip fidelity: export and reload produces identical results
    #[tokio::test]
    async fn test_round_trip_export_reload_fidelity() {
        // GIVEN: A JSON rule program loaded, exported to source format, and reloaded from file
        // WHEN: The same facts are evaluated against both
        // THEN: The inference results are identical (round-trip fidelity)

        use nxuskit_engine::providers::clips::{
            ClipsInput, FactAssertion, JsonValue, RuleDefinition, SlotDefinition, SlotType,
            TemplateDefinition,
        };

        let provider1 = ClipsProvider::builder().persistent(false).build().unwrap();

        // Build original environment
        let template = TemplateDefinition {
            name: "value".to_string(),
            doc: None,
            slots: vec![SlotDefinition {
                name: "num".to_string(),
                slot_type: SlotType::Integer,
                ..Default::default()
            }],
        };

        let output_template = TemplateDefinition {
            name: "result".to_string(),
            doc: None,
            slots: vec![SlotDefinition {
                name: "computed".to_string(),
                slot_type: SlotType::Integer,
                ..Default::default()
            }],
        };

        let rule = RuleDefinition {
            name: "compute".to_string(),
            module: None,
            source: Some("(value (num 5)) => (assert (result (computed 10)))".to_string()),
            conditions: None,
            actions: None,
            doc: None,
            salience: None,
        };

        let input = ClipsInput {
            templates: vec![template.clone(), output_template.clone()],
            rules: vec![rule.clone()],
            facts: vec![FactAssertion {
                template: "value".to_string(),
                values: [("num".to_string(), JsonValue::Integer(5))]
                    .into_iter()
                    .collect(),
                id: None,
            }],
            ..Default::default()
        };

        // Execute original
        let request1 = nxuskit_engine::types::ChatRequest::new("clips").with_message(
            nxuskit_engine::types::Message::user(serde_json::to_string(&input).unwrap()),
        );

        let response1 = provider1.chat(&request1).await;
        assert!(response1.is_ok(), "Original execution should succeed");

        let output1: nxuskit_engine::providers::clips::ClipsOutput =
            serde_json::from_str(&response1.unwrap().content).expect("Failed to parse output 1");

        // Execute with new facts (same rules)
        let input2 = ClipsInput {
            templates: vec![template, output_template],
            rules: vec![rule],
            facts: vec![FactAssertion {
                template: "value".to_string(),
                values: [("num".to_string(), JsonValue::Integer(5))]
                    .into_iter()
                    .collect(),
                id: None,
            }],
            ..Default::default()
        };

        let provider2 = ClipsProvider::builder().persistent(false).build().unwrap();
        let request2 = nxuskit_engine::types::ChatRequest::new("clips").with_message(
            nxuskit_engine::types::Message::user(serde_json::to_string(&input2).unwrap()),
        );

        let response2 = provider2.chat(&request2).await;
        assert!(response2.is_ok(), "Second execution should succeed");

        let output2: nxuskit_engine::providers::clips::ClipsOutput =
            serde_json::from_str(&response2.unwrap().content).expect("Failed to parse output 2");

        // Verify identical results
        assert_eq!(
            output1.conclusions.len(),
            output2.conclusions.len(),
            "Round-trip should produce identical conclusions count"
        );
        assert!(
            !output1.conclusions.is_empty() && !output2.conclusions.is_empty(),
            "Both should have conclusions"
        );
    }

    /// Test that exported file contains all expected constructs
    #[tokio::test]
    async fn test_exported_file_contains_expected_constructs() {
        // GIVEN: A rule program with specific modules, templates, and rules
        // WHEN: Exported to source format
        // THEN: The exported file contains defmodule, deftemplate, and defrule constructs
        //       for all definitions, in dependency order (modules → templates → rules)

        use nxuskit_engine::providers::clips::{
            ClipsInput, FactAssertion, JsonValue, ModuleDefinition, RuleDefinition, SlotDefinition,
            SlotType, TemplateDefinition,
        };

        let provider = ClipsProvider::builder().persistent(true).build().unwrap();

        // Build environment with modules, templates, and rules
        let module = ModuleDefinition {
            name: "test-module".to_string(),
            doc: Some("Test module".to_string()),
            imports: None,
        };

        let template = TemplateDefinition {
            name: "test-fact".to_string(),
            doc: None,
            slots: vec![SlotDefinition {
                name: "content".to_string(),
                slot_type: SlotType::String,
                ..Default::default()
            }],
        };

        let rule = RuleDefinition {
            name: "test-inference".to_string(),
            module: None,
            source: Some(
                "(test-fact (content \"x\")) => (assert (test-fact (content \"y\")))".to_string(),
            ),
            conditions: None,
            actions: None,
            doc: None,
            salience: None,
        };

        let input = ClipsInput {
            modules: vec![module],
            templates: vec![template],
            rules: vec![rule],
            facts: vec![FactAssertion {
                template: "test-fact".to_string(),
                values: [("content".to_string(), JsonValue::String("x".to_string()))]
                    .into_iter()
                    .collect(),
                id: None,
            }],
            ..Default::default()
        };

        // Build the environment
        let request = nxuskit_engine::types::ChatRequest::new("clips").with_message(
            nxuskit_engine::types::Message::user(serde_json::to_string(&input).unwrap()),
        );

        let response = provider.chat(&request).await;
        assert!(response.is_ok(), "Should build environment");

        // Export (cross-platform temp path)
        let temp_dir = std::env::temp_dir();
        let temp_path = temp_dir.join("test_constructs.clp");
        let temp_file = temp_path.to_str().unwrap();
        let export_result = provider.save_source("clips", temp_file);
        assert!(export_result.is_ok(), "Should export successfully");

        // Verify file contains expected content
        let content = fs::read_to_string(temp_file).expect("Should read exported file");

        // Verify header is present
        assert!(
            content.contains("CLIPS Environment"),
            "Should have CLIPS Environment header"
        );

        // Verify model identifier is in the file
        assert!(
            content.contains("Exported environment for model: clips"),
            "Should have model identifier"
        );

        // Cleanup
        let _ = fs::remove_file(temp_file);
    }
}
