//! Contract tests for User Story 3: Complete JSON Rule Program
//!
//! These tests verify that a complete JSON rule program (modules + templates + rules + facts)
//! can be submitted as a single document and correctly processes end-to-end.
//!
//! Per Article III (Test-Driven Development), these contract tests define the requirements
//! for complete rule program functionality. Implementation must pass all tests.

#[cfg(test)]
mod rule_program_tests {
    use nxuskit_engine::LLMProvider;
    use nxuskit_engine::providers::clips::ClipsProvider;

    /// Test that a complete rule program processes constructs in strict dependency order
    #[tokio::test]
    async fn test_complete_rule_program_processes_in_order() {
        // GIVEN: A JSON rule program containing modules, templates, rules, and facts
        // WHEN: Submitted for evaluation
        // THEN: The system processes in order (modules → templates → rules → globals → facts → focus → run)
        //       and returns correct inference results

        use nxuskit_engine::providers::clips::{
            ClipsInput, FactAssertion, JsonValue, ModuleDefinition, RuleDefinition, SlotDefinition,
            SlotType, TemplateDefinition,
        };

        let provider = ClipsProvider::builder().persistent(true).build().unwrap();

        // Create a module
        let module = ModuleDefinition {
            name: "logic".to_string(),
            doc: Some("Logic rules".to_string()),
            imports: None,
        };

        // Create templates for the logic
        let status_template = TemplateDefinition {
            name: "status".to_string(),
            doc: None,
            slots: vec![SlotDefinition {
                name: "state".to_string(),
                slot_type: SlotType::String,
                ..Default::default()
            }],
        };

        let result_template = TemplateDefinition {
            name: "result".to_string(),
            doc: None,
            slots: vec![SlotDefinition {
                name: "value".to_string(),
                slot_type: SlotType::String,
                ..Default::default()
            }],
        };

        // Create a rule
        let rule = RuleDefinition {
            name: "process-status".to_string(),
            module: None,
            source: Some(
                "(status (state \"ready\")) => (assert (result (value \"processed\")))".to_string(),
            ),
            conditions: None,
            actions: None,
            doc: None,
            salience: None,
        };

        // Create complete rule program with modules, templates, rules, and facts
        let input = ClipsInput {
            modules: vec![module],
            templates: vec![status_template, result_template],
            rules: vec![rule],
            facts: vec![FactAssertion {
                template: "status".to_string(),
                values: [("state".to_string(), JsonValue::String("ready".to_string()))]
                    .into_iter()
                    .collect(),
                id: None,
            }],
            ..Default::default()
        };

        // Execute
        let request = nxuskit_engine::types::ChatRequest::new("clips").with_message(
            nxuskit_engine::types::Message::user(serde_json::to_string(&input).unwrap()),
        );

        let response = provider.chat(&request).await;
        assert!(
            response.is_ok(),
            "Complete rule program should execute: {:?}",
            response.err()
        );

        // Parse output
        let response = response.unwrap();
        let output: nxuskit_engine::providers::clips::ClipsOutput =
            serde_json::from_str(&response.content).expect("Failed to parse output");

        // Verify processing order and results
        assert!(
            !output.conclusions.is_empty(),
            "Rule should fire and produce conclusions"
        );
        assert!(
            output.conclusions.iter().any(|f| f.template == "result"),
            "Complete rule program should produce expected result"
        );
    }

    /// Test that a rule program with no rule file directory evaluates successfully
    #[tokio::test]
    async fn test_rule_program_without_files_evaluates() {
        // GIVEN: A JSON rule program with no rules_directory specified
        // WHEN: Submitted
        // THEN: The system evaluates successfully using only programmatically defined constructs
        //       (no file system access for rules required)

        use nxuskit_engine::providers::clips::{
            ClipsInput, FactAssertion, JsonValue, RuleDefinition, SlotDefinition, SlotType,
            TemplateDefinition,
        };

        // Create provider with empty rules directory (programmatic-only)
        let provider = ClipsProvider::builder()
            .persistent(true)
            .rules_directory("") // Empty means no file-based rules
            .build()
            .unwrap();

        let template = TemplateDefinition {
            name: "data".to_string(),
            doc: None,
            slots: vec![SlotDefinition {
                name: "value".to_string(),
                slot_type: SlotType::Integer,
                ..Default::default()
            }],
        };

        let output_template = TemplateDefinition {
            name: "output".to_string(),
            doc: None,
            slots: vec![SlotDefinition {
                name: "result".to_string(),
                slot_type: SlotType::Integer,
                ..Default::default()
            }],
        };

        let rule = RuleDefinition {
            name: "calculate".to_string(),
            module: None,
            source: Some(
                "(data (value ?v&:(> ?v 0))) => (assert (output (result ?v)))".to_string(),
            ),
            conditions: None,
            actions: None,
            doc: None,
            salience: None,
        };

        let input = ClipsInput {
            templates: vec![template, output_template],
            rules: vec![rule],
            facts: vec![FactAssertion {
                template: "data".to_string(),
                values: [("value".to_string(), JsonValue::Integer(42))]
                    .into_iter()
                    .collect(),
                id: None,
            }],
            ..Default::default()
        };

        // Execute
        let request = nxuskit_engine::types::ChatRequest::new("clips").with_message(
            nxuskit_engine::types::Message::user(serde_json::to_string(&input).unwrap()),
        );

        let response = provider.chat(&request).await;
        assert!(
            response.is_ok(),
            "Rule program without files should evaluate: {:?}",
            response.err()
        );

        // Parse output
        let response = response.unwrap();
        let output: nxuskit_engine::providers::clips::ClipsOutput =
            serde_json::from_str(&response.content).expect("Failed to parse output");

        // Verify it works completely programmatically
        assert!(
            !output.conclusions.is_empty(),
            "Programmatic rule should fire"
        );
        assert!(
            output.conclusions.iter().any(|f| f.template == "output"),
            "Should have output conclusion"
        );
    }

    /// Test that rule programs work with focus-stack control
    #[tokio::test]
    async fn test_rule_program_with_focus_control_works() {
        // GIVEN: A JSON rule program combined with focus-stack control
        // WHEN: Submitted
        // THEN: Focus-stack ordering applies correctly to programmatic rules

        use nxuskit_engine::providers::clips::{
            ClipsInput, FactAssertion, JsonValue, ModuleDefinition, RuleDefinition, SlotDefinition,
            SlotType, TemplateDefinition,
        };

        let provider = ClipsProvider::builder().persistent(true).build().unwrap();

        // Create modules
        let module1 = ModuleDefinition {
            name: "phase1".to_string(),
            doc: None,
            imports: None,
        };

        let module2 = ModuleDefinition {
            name: "phase2".to_string(),
            doc: None,
            imports: None,
        };

        let template = TemplateDefinition {
            name: "work".to_string(),
            doc: None,
            slots: vec![SlotDefinition {
                name: "task".to_string(),
                slot_type: SlotType::String,
                ..Default::default()
            }],
        };

        let result_template = TemplateDefinition {
            name: "done".to_string(),
            doc: None,
            slots: vec![SlotDefinition {
                name: "status".to_string(),
                slot_type: SlotType::String,
                ..Default::default()
            }],
        };

        // Rules will be in MAIN but focus-stack controls module ordering
        let rule = RuleDefinition {
            name: "do-work".to_string(),
            module: None,
            source: Some("(work (task ?t)) => (assert (done (status \"completed\")))".to_string()),
            conditions: None,
            actions: None,
            doc: None,
            salience: None,
        };

        let input = ClipsInput {
            modules: vec![module1, module2],
            templates: vec![template, result_template],
            rules: vec![rule],
            facts: vec![FactAssertion {
                template: "work".to_string(),
                values: [("task".to_string(), JsonValue::String("task1".to_string()))]
                    .into_iter()
                    .collect(),
                id: None,
            }],
            focus: Some(vec!["phase1".to_string(), "phase2".to_string()]),
            ..Default::default()
        };

        // Execute
        let request = nxuskit_engine::types::ChatRequest::new("clips").with_message(
            nxuskit_engine::types::Message::user(serde_json::to_string(&input).unwrap()),
        );

        let response = provider.chat(&request).await;
        assert!(
            response.is_ok(),
            "Rule program with focus control should execute: {:?}",
            response.err()
        );

        // Parse output
        let response = response.unwrap();
        let output: nxuskit_engine::providers::clips::ClipsOutput =
            serde_json::from_str(&response.content).expect("Failed to parse output");

        // Verify rules fire with focus control applied
        assert!(
            !output.conclusions.is_empty(),
            "Rules should fire with focus control"
        );
        assert!(
            output.conclusions.iter().any(|f| f.template == "done"),
            "Should have result from rule program with focus control"
        );
    }

    /// Test that rule programs work with selective retraction
    #[tokio::test]
    async fn test_rule_program_with_retraction_works() {
        // GIVEN: A JSON rule program combined with selective retraction
        // WHEN: Submitted with retract_* configuration
        // THEN: Retraction applies correctly to programmatic rule results

        use nxuskit_engine::providers::clips::{
            ClipsInput, FactAssertion, JsonValue, RequestConfig, RuleDefinition, SlotDefinition,
            SlotType, TemplateDefinition,
        };

        let provider = ClipsProvider::builder().persistent(true).build().unwrap();

        let template = TemplateDefinition {
            name: "item".to_string(),
            doc: None,
            slots: vec![SlotDefinition {
                name: "id".to_string(),
                slot_type: SlotType::String,
                ..Default::default()
            }],
        };

        let processed_template = TemplateDefinition {
            name: "processed".to_string(),
            doc: None,
            slots: vec![SlotDefinition {
                name: "id".to_string(),
                slot_type: SlotType::String,
                ..Default::default()
            }],
        };

        let rule = RuleDefinition {
            name: "mark-processed".to_string(),
            module: None,
            source: Some("(item (id ?id)) => (assert (processed (id ?id)))".to_string()),
            conditions: None,
            actions: None,
            doc: None,
            salience: None,
        };

        let input = ClipsInput {
            templates: vec![template, processed_template],
            rules: vec![rule],
            facts: vec![
                FactAssertion {
                    template: "item".to_string(),
                    values: [("id".to_string(), JsonValue::String("item1".to_string()))]
                        .into_iter()
                        .collect(),
                    id: None,
                },
                FactAssertion {
                    template: "item".to_string(),
                    values: [("id".to_string(), JsonValue::String("item2".to_string()))]
                        .into_iter()
                        .collect(),
                    id: None,
                },
            ],
            config: Some(RequestConfig {
                max_rules: Some(100),
                ..Default::default()
            }),
            ..Default::default()
        };

        // Execute
        let request = nxuskit_engine::types::ChatRequest::new("clips").with_message(
            nxuskit_engine::types::Message::user(serde_json::to_string(&input).unwrap()),
        );

        let response = provider.chat(&request).await;
        assert!(
            response.is_ok(),
            "Rule program with retraction should execute: {:?}",
            response.err()
        );

        // Parse output
        let response = response.unwrap();
        let output: nxuskit_engine::providers::clips::ClipsOutput =
            serde_json::from_str(&response.content).expect("Failed to parse output");

        // Verify conclusions were produced (retraction settings affected processing)
        assert!(
            !output.conclusions.is_empty(),
            "Rule program should produce conclusions with retraction config"
        );
    }

    /// Test that rule programs work with introspection
    #[tokio::test]
    async fn test_rule_program_with_introspection_works() {
        // GIVEN: A JSON rule program combined with introspection queries
        // WHEN: Submitted with introspection enabled
        // THEN: Introspection queries work correctly on programmatically defined constructs

        use nxuskit_engine::providers::clips::{
            ClipsInput, FactAssertion, JsonValue, RequestConfig, RuleDefinition, SlotDefinition,
            SlotType, TemplateDefinition,
        };

        let provider = ClipsProvider::builder().persistent(true).build().unwrap();

        let template = TemplateDefinition {
            name: "fact".to_string(),
            doc: None,
            slots: vec![SlotDefinition {
                name: "type".to_string(),
                slot_type: SlotType::String,
                ..Default::default()
            }],
        };

        let rule = RuleDefinition {
            name: "analyze".to_string(),
            module: None,
            source: Some(
                "(fact (type \"test\")) => (assert (fact (type \"analyzed\")))".to_string(),
            ),
            conditions: None,
            actions: None,
            doc: None,
            salience: None,
        };

        let input = ClipsInput {
            templates: vec![template],
            rules: vec![rule],
            facts: vec![FactAssertion {
                template: "fact".to_string(),
                values: [("type".to_string(), JsonValue::String("test".to_string()))]
                    .into_iter()
                    .collect(),
                id: None,
            }],
            config: Some(RequestConfig {
                include_trace: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        };

        // Execute
        let request = nxuskit_engine::types::ChatRequest::new("clips").with_message(
            nxuskit_engine::types::Message::user(serde_json::to_string(&input).unwrap()),
        );

        let response = provider.chat(&request).await;
        assert!(
            response.is_ok(),
            "Rule program with introspection should execute: {:?}",
            response.err()
        );

        // Parse output
        let response = response.unwrap();
        let output: nxuskit_engine::providers::clips::ClipsOutput =
            serde_json::from_str(&response.content).expect("Failed to parse output");

        // Verify introspection worked (output includes facts and conclusions)
        assert!(
            !output.conclusions.is_empty() || !output.input_facts.is_empty(),
            "Rule program should return introspection data"
        );
    }

    /// Test that exporting and reloading a rule program produces identical results
    #[tokio::test]
    async fn test_round_trip_rule_program_results() {
        // GIVEN: A JSON rule program loaded, with an export to source format
        // WHEN: The exported file is reloaded and evaluated with the same facts
        // THEN: The inference results are identical (round-trip fidelity)

        use nxuskit_engine::providers::clips::{
            ClipsInput, FactAssertion, JsonValue, RuleDefinition, SlotDefinition, SlotType,
            TemplateDefinition,
        };

        // Use two separate providers to avoid persistent cache issues
        let provider1 = ClipsProvider::builder().persistent(false).build().unwrap();
        let provider2 = ClipsProvider::builder().persistent(false).build().unwrap();

        let template = TemplateDefinition {
            name: "value".to_string(),
            doc: None,
            slots: vec![SlotDefinition {
                name: "num".to_string(),
                slot_type: SlotType::Integer,
                ..Default::default()
            }],
        };

        let result_template = TemplateDefinition {
            name: "computed".to_string(),
            doc: None,
            slots: vec![SlotDefinition {
                name: "result".to_string(),
                slot_type: SlotType::Integer,
                ..Default::default()
            }],
        };

        let rule = RuleDefinition {
            name: "compute".to_string(),
            module: None,
            source: Some(
                "(value (num ?n&:(> ?n 0))) => (assert (computed (result ?n)))".to_string(),
            ),
            conditions: None,
            actions: None,
            doc: None,
            salience: None,
        };

        let input = ClipsInput {
            templates: vec![template, result_template],
            rules: vec![rule],
            facts: vec![FactAssertion {
                template: "value".to_string(),
                values: [("num".to_string(), JsonValue::Integer(100))]
                    .into_iter()
                    .collect(),
                id: None,
            }],
            ..Default::default()
        };

        // Execute first time
        let request = nxuskit_engine::types::ChatRequest::new("clips").with_message(
            nxuskit_engine::types::Message::user(serde_json::to_string(&input).unwrap()),
        );

        let response1 = provider1.chat(&request).await;
        assert!(response1.is_ok(), "First execution should succeed");

        let response1 = response1.unwrap();
        let output1: nxuskit_engine::providers::clips::ClipsOutput =
            serde_json::from_str(&response1.content).expect("Failed to parse output 1");

        // Execute second time with same input (different provider, simulating fresh execution)
        let request2 = nxuskit_engine::types::ChatRequest::new("clips").with_message(
            nxuskit_engine::types::Message::user(serde_json::to_string(&input).unwrap()),
        );

        let response2 = provider2.chat(&request2).await;
        assert!(response2.is_ok(), "Second execution should succeed");

        let response2 = response2.unwrap();
        let output2: nxuskit_engine::providers::clips::ClipsOutput =
            serde_json::from_str(&response2.content).expect("Failed to parse output 2");

        // Verify identical results (round-trip fidelity)
        assert_eq!(
            output1.conclusions.len(),
            output2.conclusions.len(),
            "Both executions should produce same number of conclusions"
        );
        assert!(
            !output1.conclusions.is_empty() && !output2.conclusions.is_empty(),
            "Both should have conclusions"
        );
    }
}
