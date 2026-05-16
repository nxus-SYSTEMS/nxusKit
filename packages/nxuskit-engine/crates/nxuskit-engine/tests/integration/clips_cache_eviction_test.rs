//! Contract tests for cache eviction functionality
//!
//! These tests verify that manual cache eviction methods work correctly.
//! Per Article III (Test-Driven Development), these contract tests define the requirements.

#[cfg(test)]
mod cache_eviction_tests {
    use nxuskit_engine::LLMProvider;
    use nxuskit_engine::providers::clips::ClipsProvider;

    /// Test that evict_environment removes a specific cache entry
    #[tokio::test]
    async fn test_evict_environment_removes_entry() {
        // GIVEN: A compiled environment cached under a specific key
        // WHEN: evict_environment(key) is called
        // THEN: The environment is removed from the cache

        use nxuskit_engine::providers::clips::{
            ClipsInput, FactAssertion, JsonValue, RuleDefinition, SlotDefinition, SlotType,
            TemplateDefinition,
        };

        let provider = ClipsProvider::builder().persistent(true).build().unwrap();

        // Create a simple rule program
        let template = TemplateDefinition {
            name: "fact".to_string(),
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
            source: Some("(fact (value \"test\")) => (assert (fact (value \"done\")))".to_string()),
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
                values: [("value".to_string(), JsonValue::String("test".to_string()))]
                    .into_iter()
                    .collect(),
                id: None,
            }],
            ..Default::default()
        };

        // Execute to cache the environment
        let request = nxuskit_engine::types::ChatRequest::new("clips").with_message(
            nxuskit_engine::types::Message::user(serde_json::to_string(&input).unwrap()),
        );

        let response1 = provider.chat(&request).await;
        assert!(
            response1.is_ok(),
            "First execution should cache environment"
        );

        // Evict the cached environment by model key
        let evicted = provider.evict_environment("clips");
        assert!(evicted, "Environment should be evicted");

        // Execute again - should recompile (cache miss)
        let response2 = provider.chat(&request).await;
        assert!(
            response2.is_ok(),
            "Second execution should recompile after eviction"
        );
    }

    /// Test that clear_cache removes all cached environments
    #[tokio::test]
    async fn test_clear_cache_removes_all_entries() {
        // GIVEN: Multiple compiled environments cached
        // WHEN: clear_cache() is called
        // THEN: All cached environments are removed

        use nxuskit_engine::providers::clips::{
            ClipsInput, FactAssertion, JsonValue, RuleDefinition, SlotDefinition, SlotType,
            TemplateDefinition,
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

        let rule = RuleDefinition {
            name: "process-item".to_string(),
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

        // Cache the environment by executing
        let request = nxuskit_engine::types::ChatRequest::new("clips").with_message(
            nxuskit_engine::types::Message::user(serde_json::to_string(&input).unwrap()),
        );

        let response1 = provider.chat(&request).await;
        assert!(
            response1.is_ok(),
            "First execution should cache environment"
        );

        // Clear all cached environments
        provider.clear_cache();

        // Execute again - should recompile (all cache cleared)
        let response2 = provider.chat(&request).await;
        assert!(
            response2.is_ok(),
            "Second execution should recompile after clearing cache"
        );
    }

    /// Test that resubmitting after eviction recompiles the environment
    #[tokio::test]
    async fn test_resubmit_after_eviction_recompiles() {
        // GIVEN: An environment that was evicted from the cache
        // WHEN: The same rule program is submitted again
        // THEN: The environment is recompiled (cache miss) and reinserted into cache

        use nxuskit_engine::providers::clips::{
            ClipsInput, FactAssertion, JsonValue, RuleDefinition, SlotDefinition, SlotType,
            TemplateDefinition,
        };

        let provider = ClipsProvider::builder().persistent(true).build().unwrap();

        let template = TemplateDefinition {
            name: "data".to_string(),
            doc: None,
            slots: vec![SlotDefinition {
                name: "value".to_string(),
                slot_type: SlotType::Integer,
                ..Default::default()
            }],
        };

        let rule = RuleDefinition {
            name: "compute".to_string(),
            module: None,
            source: Some("(data (value 1)) => (assert (data (value 2)))".to_string()),
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
                values: [("value".to_string(), JsonValue::Integer(1))]
                    .into_iter()
                    .collect(),
                id: None,
            }],
            ..Default::default()
        };

        // First execution - caches environment
        let request = nxuskit_engine::types::ChatRequest::new("clips").with_message(
            nxuskit_engine::types::Message::user(serde_json::to_string(&input).unwrap()),
        );

        let response1 = provider.chat(&request).await;
        assert!(response1.is_ok(), "First execution should cache");

        // Evict from cache
        provider.evict_environment("clips");

        // Submit identical program again - should recompile and recache
        let response2 = provider.chat(&request).await;
        assert!(
            response2.is_ok(),
            "Second submission should recompile after eviction"
        );

        // Parse both outputs to verify they're identical (deterministic recompilation)
        let output1: nxuskit_engine::providers::clips::ClipsOutput =
            serde_json::from_str(&response1.unwrap().content).expect("Failed to parse output 1");
        let output2: nxuskit_engine::providers::clips::ClipsOutput =
            serde_json::from_str(&response2.unwrap().content).expect("Failed to parse output 2");

        // Results should be identical since rules are the same
        assert_eq!(
            output1.conclusions.len(),
            output2.conclusions.len(),
            "Recompiled environment should produce identical results"
        );
    }
}
