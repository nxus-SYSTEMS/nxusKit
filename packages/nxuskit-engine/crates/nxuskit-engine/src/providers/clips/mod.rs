//! CLIPS Expert System Provider
//!
//! This module provides integration with CLIPS (C Language Integrated Production System),
//! enabling rule-based inference as a "model" within nxusKit.
//!
//! # Overview
//!
//! CLIPS is a forward-chaining rule-based expert system shell. This provider allows you to:
//!
//! - Use CLIPS rule bases as "models"
//! - Pass facts as JSON in message content
//! - Receive inference conclusions as JSON output
//! - Generate CLIPS rules from natural language using LLMs
//!
//! # Example
//!
//! ```no_run
//! use nxuskit_engine::providers::clips::ClipsProvider;
//! use nxuskit_engine::provider::LLMProvider;
//! use nxuskit_engine::types::{ChatRequest, Message};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let provider = ClipsProvider::builder()
//!         .rules_directory("./rules")
//!         .persistent(true)
//!         .build()?;
//!
//!     let input = r#"{
//!         "facts": [
//!             {"template": "patient", "values": {"name": "John", "age": 65}},
//!             {"template": "symptom", "values": {"patient": "John", "type": "chest-pain"}}
//!         ]
//!     }"#;
//!
//!     let request = ChatRequest::new("cardiac-rules.clp")
//!         .with_message(Message::user(input));
//!
//!     let response = provider.chat(&request).await?;
//!     println!("Conclusions: {}", response.content);
//!     Ok(())
//! }
//! ```
//!
//! # Input Format
//!
//! The provider expects JSON input in the user message:
//!
//! ```json
//! {
//!     "templates": [                    // Optional: auto-generate templates
//!         {
//!             "name": "patient",
//!             "slots": [
//!                 {"name": "name", "type": "STRING"},
//!                 {"name": "age", "type": "INTEGER"}
//!             ]
//!         }
//!     ],
//!     "facts": [                        // Required: facts to assert
//!         {
//!             "template": "patient",
//!             "values": {"name": "John", "age": 65}
//!         }
//!     ],
//!     "config": {                       // Optional: request overrides
//!         "include_trace": true,
//!         "max_rules": 100
//!     }
//! }
//! ```
//!
//! # Output Format
//!
//! The provider returns JSON output:
//!
//! ```json
//! {
//!     "conclusions": [                  // Derived facts from inference
//!         {
//!             "template": "diagnosis",
//!             "values": {"patient": "John", "condition": "cardiac-risk"},
//!             "fact_index": 5,
//!             "derived": true
//!         }
//!     ],
//!     "stats": {
//!         "total_rules_fired": 3,
//!         "conclusions_count": 1,
//!         "execution_time_ms": 15
//!     },
//!     "trace": {                        // Optional: execution trace
//!         "rules_fired": [
//!             {"rule_name": "elderly-check", "fire_count": 1}
//!         ]
//!     }
//! }
//! ```
//!
//! # Rulebase Generation
//!
//! Use the `generation` module to create CLIPS rules from natural language:
//!
//! ```no_run
//! use nxuskit_engine::providers::clips::generation::RulebaseGenerator;
//! use nxuskit_engine::providers::ClaudeProvider;
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let claude = Arc::new(ClaudeProvider::builder()
//!         .api_key("your-api-key")
//!         .build()?);
//!
//!     let generator = RulebaseGenerator::new(claude, "claude-sonnet-4-5-20250514");
//!
//!     let policy = r#"
//!         Customer Loyalty Rules:
//!         1. Customers with 2+ years membership are "established"
//!         2. Established customers with $5000+ spending get Gold status
//!         3. Gold customers receive 15% discount
//!     "#;
//!
//!     let rulebase = generator.from_policy(policy).await?;
//!     println!("Generated CLIPS code:\n{}", rulebase.code);
//!     Ok(())
//! }
//! ```
//!
//! # Features
//!
//! - `clips` - Enable CLIPS integration (requires clips-sys crate)
//!
//! Without the `clips` feature, the provider will return an error when used,
//! but the types and generation modules remain available.

pub mod converter;
pub mod generation;
pub mod provider;
pub mod schema;
pub mod security;
pub mod similarity;
pub mod types;

// Re-export main types
pub use converter::{ClipsCodeBuilder, ClipsToJsonConverter, JsonToClipsConverter};
pub use generation::{
    CLIPS_SYSTEM_PROMPT, DECISION_TABLE_PROMPT_TEMPLATE, EXAMPLE_PROMPT_TEMPLATE,
    JSON_SPEC_PROMPT_TEMPLATE, POLICY_PROMPT_TEMPLATE, RulebaseGenerator,
    TEST_GENERATION_PROMPT_TEMPLATE, generate_from_policy, generate_from_spec,
};
pub use provider::{
    ClipsConfig, ClipsProvider, ClipsProviderBuilder, LoadType, ModelPathResolver, ResolvedModel,
};
pub use schema::{
    ClipsSchema, SchemaError, SlotSchema, SlotTypeSchemaExt, TemplateSchema,
    deftemplate_to_json_schema, describe_all_templates, describe_template,
    extract_schemas_from_environment, json_schema_to_deftemplate, templates_to_json_schema,
};
pub use security::{SecurityIssue, SecuritySeverity, SecurityValidationResult, SecurityValidator};
pub use similarity::{
    DEFAULT_MAX_SUGGESTIONS, DEFAULT_SIMILARITY_THRESHOLD, find_similar, find_similar_strings,
};
pub use types::{
    ActivationInfo, ClipsInput, ClipsOutput, ExecutionStats, ExecutionTrace, ExpectedFact,
    FactAssertion, FactEvent, FactOutput, GeneratedRulebase, GeneratedTestCase, GenerationMetadata,
    GenerationOptions, GenerationStyle, JsonValue, ModuleDefinition, RequestConfig, RuleAction,
    RuleChunkOutput, RuleCondition, RuleDefinition, RuleFiring, SlotDefinition, SlotType,
    StreamMode, SymbolValue, TemplateDefinition, ValidationError, ValidationLevel,
    ValidationResult, ValidationWarning,
};
