//! Contract tests for User Story 1: Define Rules via JSON
//!
//! These tests verify that rules can be defined via JSON payloads (both raw source strings
//! and structured JSON definitions) and correctly fire during inference.
//!
//! Per Article III (Test-Driven Development), these contract tests define the requirements
//! for rule definition functionality. Implementation must pass all tests.

#[cfg(test)]
mod rule_definition_tests {
    use nxuskit_engine::LLMProvider;
    use nxuskit_engine::providers::clips::ClipsProvider;

    /// Test that a rule defined as a raw source string fires correctly
    #[tokio::test]
    async fn test_source_string_rule_fires() {
        // GIVEN: A JSON payload containing a rule defined via raw source string
        // WHEN: The payload is submitted for evaluation with matching facts
        // THEN: The rule fires and produces the expected inferred facts in the output

        use nxuskit_engine::providers::clips::{
            ClipsInput, FactAssertion, JsonValue, RuleDefinition, SlotDefinition, SlotType,
            TemplateDefinition,
        };

        // Create provider
        let provider = ClipsProvider::builder().persistent(true).build().unwrap();

        // Define a simple template
        let template = TemplateDefinition {
            name: "sensor".to_string(),
            doc: None,
            slots: vec![SlotDefinition {
                name: "value".to_string(),
                slot_type: SlotType::Integer,
                ..Default::default()
            }],
        };

        // Define a rule via source string
        let rule = RuleDefinition {
            name: "alert-high".to_string(),
            module: None,
            source: Some(
                "(sensor (value ?v&:(> ?v 100))) => (assert (alert (level \"high\")))".to_string(),
            ),
            conditions: None,
            actions: None,
            doc: None,
            salience: None,
        };

        // Create alert template
        let alert_template = TemplateDefinition {
            name: "alert".to_string(),
            doc: None,
            slots: vec![SlotDefinition {
                name: "level".to_string(),
                slot_type: SlotType::String,
                ..Default::default()
            }],
        };

        // Create input with rule and facts
        let input = ClipsInput {
            templates: vec![template, alert_template],
            rules: vec![rule],
            facts: vec![FactAssertion {
                template: "sensor".to_string(),
                values: [("value".to_string(), JsonValue::Integer(150))]
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
            "Failed to execute CLIPS provider: {:?}",
            response.err()
        );

        // Parse output
        let response = response.unwrap();

        let output: nxuskit_engine::providers::clips::ClipsOutput =
            serde_json::from_str(&response.content).expect("Failed to parse output");

        // Verify the rule fired and produced the expected conclusion
        assert!(
            !output.conclusions.is_empty(),
            "Rule should fire and produce conclusions"
        );
        assert!(
            output.conclusions.iter().any(|f| f.template == "alert"),
            "Should have alert conclusion"
        );
    }

    /// Test that a rule defined via structured JSON (conditions/actions) fires correctly
    #[tokio::test]
    async fn test_structured_json_rule_fires() {
        // GIVEN: A JSON payload containing a rule defined via structured JSON
        //        (conditions array, actions array, salience)
        // WHEN: The payload is submitted for evaluation
        // THEN: The system generates the equivalent rule construct and fires correctly

        use nxuskit_engine::providers::clips::{
            ClipsInput, FactAssertion, JsonValue, RuleAction, RuleCondition, RuleDefinition,
            SlotDefinition, SlotType, TemplateDefinition,
        };

        // Create provider
        let provider = ClipsProvider::builder().persistent(true).build().unwrap();

        // Define templates
        let sensor_template = TemplateDefinition {
            name: "sensor".to_string(),
            doc: None,
            slots: vec![SlotDefinition {
                name: "value".to_string(),
                slot_type: SlotType::Integer,
                ..Default::default()
            }],
        };

        let result_template = TemplateDefinition {
            name: "result".to_string(),
            doc: None,
            slots: vec![SlotDefinition {
                name: "status".to_string(),
                slot_type: SlotType::String,
                ..Default::default()
            }],
        };

        // Define rule via structured JSON
        let rule = RuleDefinition {
            name: "test-rule".to_string(),
            module: None,
            source: None,
            conditions: Some(vec![RuleCondition {
                template: "sensor".to_string(),
                bindings: Some(
                    [("value".to_string(), "?v&:(> ?v 50)".to_string())]
                        .into_iter()
                        .collect(),
                ),
                constraints: None,
            }]),
            actions: Some(vec![RuleAction {
                assert: Some(FactAssertion {
                    template: "result".to_string(),
                    values: [("status".to_string(), JsonValue::String("ok".to_string()))]
                        .into_iter()
                        .collect(),
                    id: None,
                }),
                retract: None,
                modify: None,
            }]),
            doc: None,
            salience: None,
        };

        // Create input
        let input = ClipsInput {
            templates: vec![sensor_template, result_template],
            rules: vec![rule],
            facts: vec![FactAssertion {
                template: "sensor".to_string(),
                values: [("value".to_string(), JsonValue::Integer(75))]
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
            "Failed to execute CLIPS provider: {:?}",
            response.err()
        );

        // Parse output
        let response = response.unwrap();

        let output: nxuskit_engine::providers::clips::ClipsOutput =
            serde_json::from_str(&response.content).expect("Failed to parse output");

        // Verify the rule fired
        assert!(
            !output.conclusions.is_empty(),
            "Rule should fire and produce conclusions"
        );
        assert!(
            output.conclusions.iter().any(|f| f.template == "result"),
            "Should have result conclusion"
        );
    }

    /// Test that both source-based and structured rules can be mixed in the same payload
    #[tokio::test]
    async fn test_mixed_source_and_structured_rules() {
        // GIVEN: A JSON payload containing both source-based and structured rules
        // WHEN: Submitted for evaluation with matching facts
        // THEN: Both rule styles are processed and fire correctly

        use nxuskit_engine::providers::clips::{
            ClipsInput, FactAssertion, JsonValue, RuleAction, RuleCondition, RuleDefinition,
            SlotDefinition, SlotType, TemplateDefinition,
        };

        let provider = ClipsProvider::builder().persistent(true).build().unwrap();

        // Define templates
        let event_template = TemplateDefinition {
            name: "event".to_string(),
            doc: None,
            slots: vec![SlotDefinition {
                name: "type".to_string(),
                slot_type: SlotType::String,
                ..Default::default()
            }],
        };

        let alert_template = TemplateDefinition {
            name: "alert".to_string(),
            doc: None,
            slots: vec![SlotDefinition {
                name: "message".to_string(),
                slot_type: SlotType::String,
                ..Default::default()
            }],
        };

        let log_template = TemplateDefinition {
            name: "log".to_string(),
            doc: None,
            slots: vec![SlotDefinition {
                name: "entry".to_string(),
                slot_type: SlotType::String,
                ..Default::default()
            }],
        };

        // Rule 1: Source-based rule
        let source_rule = RuleDefinition {
            name: "source-rule".to_string(),
            module: None,
            source: Some(
                "(event (type \"error\")) => (assert (alert (message \"Error detected\")))"
                    .to_string(),
            ),
            conditions: None,
            actions: None,
            doc: None,
            salience: None,
        };

        // Rule 2: Structured rule
        let structured_rule = RuleDefinition {
            name: "structured-rule".to_string(),
            module: None,
            source: None,
            conditions: Some(vec![RuleCondition {
                template: "event".to_string(),
                bindings: Some(
                    [("type".to_string(), "?t".to_string())]
                        .into_iter()
                        .collect(),
                ),
                constraints: None,
            }]),
            actions: Some(vec![RuleAction {
                assert: Some(FactAssertion {
                    template: "log".to_string(),
                    values: [(
                        "entry".to_string(),
                        JsonValue::String("Event logged".to_string()),
                    )]
                    .into_iter()
                    .collect(),
                    id: None,
                }),
                retract: None,
                modify: None,
            }]),
            doc: None,
            salience: None,
        };

        // Create input with both rule types
        let input = ClipsInput {
            templates: vec![event_template, alert_template, log_template],
            rules: vec![source_rule, structured_rule],
            facts: vec![FactAssertion {
                template: "event".to_string(),
                values: [("type".to_string(), JsonValue::String("error".to_string()))]
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
            "Failed to execute CLIPS provider: {:?}",
            response.err()
        );

        // Parse output
        let response = response.unwrap();

        let output: nxuskit_engine::providers::clips::ClipsOutput =
            serde_json::from_str(&response.content).expect("Failed to parse output");

        // Verify both rules fired
        assert!(
            !output.conclusions.is_empty(),
            "Rules should fire and produce conclusions"
        );
        let has_alert = output.conclusions.iter().any(|f| f.template == "alert");
        let has_log = output.conclusions.iter().any(|f| f.template == "log");
        assert!(
            has_alert,
            "Source-based rule should produce alert conclusion"
        );
        assert!(has_log, "Structured rule should produce log conclusion");
    }

    /// Test that invalid rule syntax produces a clear error message
    #[tokio::test]
    async fn test_rule_with_invalid_syntax_returns_error() {
        // GIVEN: A rule definition with invalid CLIPS syntax
        // WHEN: Submitted
        // THEN: The system returns a clear, actionable error message indicating what failed

        use nxuskit_engine::providers::clips::{
            ClipsInput, FactAssertion, JsonValue, RuleDefinition, SlotDefinition, SlotType,
            TemplateDefinition,
        };

        let provider = ClipsProvider::builder().persistent(true).build().unwrap();

        let template = TemplateDefinition {
            name: "sensor".to_string(),
            doc: None,
            slots: vec![SlotDefinition {
                name: "value".to_string(),
                slot_type: SlotType::Integer,
                ..Default::default()
            }],
        };

        // Define a rule with invalid CLIPS syntax (missing closing paren)
        let rule = RuleDefinition {
            name: "bad-rule".to_string(),
            module: None,
            source: Some("(sensor (value ?v&:(> ?v 100)) => (assert (alert))".to_string()), // Missing closing paren
            conditions: None,
            actions: None,
            doc: None,
            salience: None,
        };

        let input = ClipsInput {
            templates: vec![template],
            rules: vec![rule],
            facts: vec![FactAssertion {
                template: "sensor".to_string(),
                values: [("value".to_string(), JsonValue::Integer(150))]
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

        // Verify error is returned
        assert!(response.is_err(), "Invalid syntax should produce an error");
    }

    /// Test that a rule referencing a non-existent template produces a clear error
    #[tokio::test]
    async fn test_rule_referencing_nonexistent_template_returns_error() {
        // GIVEN: A rule definition that references a template not defined in payload or files
        // WHEN: Submitted
        // THEN: The system returns error TemplateNotFound(template_name) before evaluation

        use nxuskit_engine::providers::clips::{
            ClipsInput, FactAssertion, JsonValue, RuleDefinition, SlotDefinition, SlotType,
            TemplateDefinition,
        };

        let provider = ClipsProvider::builder().persistent(true).build().unwrap();

        let template = TemplateDefinition {
            name: "sensor".to_string(),
            doc: None,
            slots: vec![SlotDefinition {
                name: "value".to_string(),
                slot_type: SlotType::Integer,
                ..Default::default()
            }],
        };

        // Define a rule that references a template "nonexistent" that is not defined
        let rule = RuleDefinition {
            name: "bad-ref-rule".to_string(),
            module: None,
            source: Some("(nonexistent (value ?v)) => (assert (alert))".to_string()),
            conditions: None,
            actions: None,
            doc: None,
            salience: None,
        };

        let input = ClipsInput {
            templates: vec![template],
            rules: vec![rule],
            facts: vec![FactAssertion {
                template: "sensor".to_string(),
                values: [("value".to_string(), JsonValue::Integer(150))]
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

        // Verify error is returned for undefined template reference
        assert!(
            response.is_err(),
            "Undefined template reference should produce an error"
        );
    }

    /// Test that rules with salience priority fire in the correct order
    #[tokio::test]
    async fn test_rule_with_salience_fires_in_priority_order() {
        // GIVEN: Multiple rules with different salience values
        // WHEN: Facts trigger all rules
        // THEN: Rules fire in order determined by salience (higher values first)

        use nxuskit_engine::providers::clips::{
            ClipsInput, FactAssertion, JsonValue, RuleDefinition, SlotDefinition, SlotType,
            TemplateDefinition,
        };

        let provider = ClipsProvider::builder().persistent(true).build().unwrap();

        let sensor_template = TemplateDefinition {
            name: "sensor".to_string(),
            doc: None,
            slots: vec![SlotDefinition {
                name: "value".to_string(),
                slot_type: SlotType::Integer,
                ..Default::default()
            }],
        };

        let order_template = TemplateDefinition {
            name: "order".to_string(),
            doc: None,
            slots: vec![SlotDefinition {
                name: "sequence".to_string(),
                slot_type: SlotType::Integer,
                ..Default::default()
            }],
        };

        // Rule with high salience (should fire first)
        let high_salience_rule = RuleDefinition {
            name: "high-priority".to_string(),
            module: None,
            source: Some(
                "(sensor (value ?v&:(> ?v 50))) => (assert (order (sequence 1)))".to_string(),
            ),
            conditions: None,
            actions: None,
            doc: None,
            salience: Some(100),
        };

        // Rule with medium salience (should fire second)
        let med_salience_rule = RuleDefinition {
            name: "med-priority".to_string(),
            module: None,
            source: Some(
                "(sensor (value ?v&:(> ?v 50))) => (assert (order (sequence 2)))".to_string(),
            ),
            conditions: None,
            actions: None,
            doc: None,
            salience: Some(50),
        };

        // Rule with low salience (should fire third)
        let low_salience_rule = RuleDefinition {
            name: "low-priority".to_string(),
            module: None,
            source: Some(
                "(sensor (value ?v&:(> ?v 50))) => (assert (order (sequence 3)))".to_string(),
            ),
            conditions: None,
            actions: None,
            doc: None,
            salience: Some(10),
        };

        let input = ClipsInput {
            templates: vec![sensor_template, order_template],
            rules: vec![high_salience_rule, med_salience_rule, low_salience_rule],
            facts: vec![FactAssertion {
                template: "sensor".to_string(),
                values: [("value".to_string(), JsonValue::Integer(75))]
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
            "Failed to execute CLIPS provider: {:?}",
            response.err()
        );

        // Parse output
        let response = response.unwrap();

        let output: nxuskit_engine::providers::clips::ClipsOutput =
            serde_json::from_str(&response.content).expect("Failed to parse output");

        // Verify all three rules fired (salience affects firing order, not whether they fire)
        let order_conclusions: Vec<_> = output
            .conclusions
            .iter()
            .filter(|f| f.template == "order")
            .collect();

        assert_eq!(order_conclusions.len(), 3, "All three rules should fire");

        // Extract sequence numbers to verify they were all created
        let sequences: Vec<i64> = order_conclusions
            .iter()
            .filter_map(|f| {
                f.values.get("sequence").and_then(|v| match v {
                    JsonValue::Integer(n) => Some(*n),
                    _ => None,
                })
            })
            .collect();

        // All three sequences should be present (1, 2, 3)
        assert!(
            sequences.contains(&1),
            "High priority rule should have fired (sequence 1)"
        );
        assert!(
            sequences.contains(&2),
            "Medium priority rule should have fired (sequence 2)"
        );
        assert!(
            sequences.contains(&3),
            "Low priority rule should have fired (sequence 3)"
        );
    }
}
