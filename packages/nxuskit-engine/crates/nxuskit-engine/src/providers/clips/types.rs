//! Types for CLIPS provider JSON input/output
//!
//! This module defines the JSON structures used for communicating with the
//! CLIPS provider. All input and output is in JSON format for easy integration.
//! Keep these wire types lightweight; provider behavior belongs outside this module.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// Input Types
// ============================================================================

/// Main input structure for CLIPS provider requests.
///
/// Unknown fields are rejected to prevent silent data loss. If your JSON
/// contains keys not listed here (e.g., bare domain data instead of
/// structured `facts`), deserialization will fail with a descriptive error.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ClipsInput {
    /// Optional command to execute (e.g., "reset", "clear", "retract")
    #[serde(default)]
    pub command: Option<String>,

    /// Optional modules to create programmatically (NEW - Feature 033)
    #[serde(default)]
    pub modules: Vec<ModuleDefinition>,

    /// Optional templates to auto-generate (if not in rule base)
    #[serde(default)]
    pub templates: Vec<TemplateDefinition>,

    /// Optional rules to create programmatically (NEW - Feature 033)
    #[serde(default)]
    pub rules: Vec<RuleDefinition>,

    /// Facts to assert before running inference
    #[serde(default)]
    pub facts: Vec<FactAssertion>,

    /// Optional global variable values to set
    #[serde(default)]
    pub globals: HashMap<String, JsonValue>,

    /// Optional configuration overrides for this request
    #[serde(default)]
    pub config: Option<RequestConfig>,

    /// Optional module focus list for selective rule execution.
    /// When provided, only rules in the specified modules will fire.
    /// Modules are pushed onto the focus stack in reverse order (last = top).
    #[serde(default)]
    pub focus: Option<Vec<String>>,

    /// Single template name to retract (used with command "retract")
    #[serde(default)]
    pub retract_template: Option<String>,

    /// Multiple template names to retract (used with command "retract")
    #[serde(default)]
    pub retract_templates: Option<Vec<String>>,

    /// Optional human-readable policy identifier for caching and diagnostics (NEW - Feature 033)
    /// When provided, this serves as the primary cache key. The system also computes
    /// a content hash internally and verifies policy_id consistency.
    #[serde(default)]
    pub policy_id: Option<String>,

    /// When true, policy_id hash mismatches cause an error instead of a warning (NEW - Feature 033)
    /// Default: false (warnings only). Set to true for strict policy consistency checking.
    #[serde(default)]
    pub strict_policy_id: Option<bool>,
}

/// Streaming mode for CLIPS output
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum StreamMode {
    /// Default: single chunk with all results
    #[default]
    Default,
    /// One chunk per derived fact
    Fact,
    /// One chunk per rule firing (with resulting facts)
    Rule,
}

/// Request-specific configuration overrides
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RequestConfig {
    /// Maximum rules to fire (overrides provider default)
    #[serde(default)]
    pub max_rules: Option<i64>,

    /// Include execution trace in output
    #[serde(default)]
    pub include_trace: Option<bool>,

    /// Only return facts matching these templates
    #[serde(default)]
    pub output_templates: Option<Vec<String>>,

    /// Only return newly derived facts (not input facts or previously derived)
    #[serde(default)]
    pub derived_only_new: Option<bool>,

    /// Streaming mode: "default" (single chunk), "fact" (per fact), "rule" (per rule)
    #[serde(default)]
    pub stream_mode: Option<StreamMode>,
}

/// Template definition for auto-generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateDefinition {
    /// Template name
    pub name: String,

    /// Optional documentation string
    #[serde(default)]
    pub doc: Option<String>,

    /// Slot definitions
    pub slots: Vec<SlotDefinition>,
}

/// Slot definition within a template
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlotDefinition {
    /// Slot name
    pub name: String,

    /// Slot type
    #[serde(default, rename = "type")]
    pub slot_type: SlotType,

    /// Whether this is a multislot
    #[serde(default)]
    pub multislot: bool,

    /// Default value
    #[serde(default)]
    pub default: Option<JsonValue>,

    /// Allowed values (for constrained slots)
    #[serde(default)]
    pub allowed_values: Option<Vec<JsonValue>>,

    /// Numeric range (min, max)
    #[serde(default)]
    pub range: Option<(f64, f64)>,

    /// Cardinality for multislots (min, max)
    #[serde(default)]
    pub cardinality: Option<(usize, usize)>,
}

/// CLIPS slot types
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum SlotType {
    /// Any type (no constraint)
    #[default]
    Any,
    /// Integer numbers
    Integer,
    /// Floating-point numbers
    Float,
    /// Any numeric type
    Number,
    /// Quoted strings
    String,
    /// Unquoted symbols
    Symbol,
    /// Fact address reference
    FactAddress,
    /// Instance address reference
    InstanceAddress,
    /// Instance name
    InstanceName,
    /// External address
    ExternalAddress,
}

impl SlotType {
    /// Convert to CLIPS type string
    pub fn to_clips_string(&self) -> Option<&'static str> {
        match self {
            SlotType::Any => None,
            SlotType::Integer => Some("INTEGER"),
            SlotType::Float => Some("FLOAT"),
            SlotType::Number => Some("NUMBER"),
            SlotType::String => Some("STRING"),
            SlotType::Symbol => Some("SYMBOL"),
            SlotType::FactAddress => Some("FACT-ADDRESS"),
            SlotType::InstanceAddress => Some("INSTANCE-ADDRESS"),
            SlotType::InstanceName => Some("INSTANCE-NAME"),
            SlotType::ExternalAddress => Some("EXTERNAL-ADDRESS"),
        }
    }
}

/// Fact assertion from JSON
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactAssertion {
    /// Template name
    pub template: String,

    /// Slot values as JSON object
    pub values: HashMap<String, JsonValue>,

    /// Optional client-side ID for tracking
    #[serde(default)]
    pub id: Option<String>,
}

/// Generic JSON value that maps to CLIPS values
///
/// Uses a custom `Deserialize` impl instead of `#[serde(untagged)]` to handle
/// the `serde_json/arbitrary_precision` feature correctly. When `arbitrary_precision`
/// is enabled by another dependency, serde's generic `ContentDeserializer` used
/// by `#[serde(untagged)]` mishandles `Number` values — they appear as tagged
/// structs (`{"$serde_json::private::Number": "250.0"}`) instead of plain numbers,
/// causing `Float(f64)` to fail and `Object(...)` to match incorrectly.
///
/// By deserializing through `serde_json::Value` first (which knows how to handle
/// its own `Number` type regardless of `arbitrary_precision`), then converting via
/// the `From<serde_json::Value>` impl, we get correct numeric handling in all cases.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(untagged)]
pub enum JsonValue {
    /// Null/nil value
    Null,
    /// Boolean (maps to TRUE/FALSE symbols)
    Bool(bool),
    /// Integer number
    Integer(i64),
    /// Floating-point number
    Float(f64),
    /// String value
    String(String),
    /// Array (maps to multifield)
    Array(Vec<JsonValue>),
    /// Symbol (explicit)
    Symbol(SymbolValue),
    /// Object (for nested structures)
    Object(HashMap<String, JsonValue>),
}

impl<'de> Deserialize<'de> for JsonValue {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Deserialize as serde_json::Value first — it correctly handles
        // arbitrary_precision numbers via its own internal protocol.
        let value = serde_json::Value::deserialize(deserializer)?;
        Ok(JsonValue::from(value))
    }
}

/// Explicit symbol value wrapper
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SymbolValue {
    /// The symbol string
    pub symbol: String,
}

impl JsonValue {
    /// Create a symbol value
    pub fn symbol(s: impl Into<String>) -> Self {
        JsonValue::Symbol(SymbolValue { symbol: s.into() })
    }

    /// Check if this is a null value
    pub fn is_null(&self) -> bool {
        matches!(self, JsonValue::Null)
    }

    /// Try to get as string
    pub fn as_str(&self) -> Option<&str> {
        match self {
            JsonValue::String(s) => Some(s),
            _ => None,
        }
    }

    /// Try to get as integer
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            JsonValue::Integer(i) => Some(*i),
            JsonValue::Float(f) => Some(*f as i64),
            _ => None,
        }
    }

    /// Try to get as float
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            JsonValue::Float(f) => Some(*f),
            JsonValue::Integer(i) => Some(*i as f64),
            _ => None,
        }
    }

    /// Try to get as bool
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            JsonValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Try to get as array
    pub fn as_array(&self) -> Option<&[JsonValue]> {
        match self {
            JsonValue::Array(arr) => Some(arr),
            _ => None,
        }
    }
}

// ============================================================================
// Output Types
// ============================================================================

/// Main output structure from CLIPS provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipsOutput {
    /// Derived facts (conclusions from inference)
    pub conclusions: Vec<FactOutput>,

    /// Input facts echoed back (if requested)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_facts: Vec<FactOutput>,

    /// Execution trace (if requested)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace: Option<ExecutionTrace>,

    /// Execution statistics
    pub stats: ExecutionStats,

    /// Retraction result (when command is "retract")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retract_result: Option<RetractResult>,
}

/// Result of selective fact retraction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetractResult {
    /// Number of facts retracted per template name
    pub retracted: HashMap<String, usize>,

    /// Total number of facts retracted across all templates
    pub total: usize,
}

/// Environment statistics for health introspection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentStats {
    /// Model key (cache key)
    pub model: String,

    /// Number of facts in working memory
    pub fact_count: usize,

    /// Number of rules defined
    pub rule_count: usize,

    /// Number of templates defined
    pub template_count: usize,

    /// Number of activations on the agenda
    pub agenda_size: usize,

    /// List of module names
    pub modules: Vec<String>,

    /// Current conflict resolution strategy
    pub strategy: String,

    /// Whether fact duplication is allowed
    pub fact_duplication: bool,
}

/// Output representation of a fact
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactOutput {
    /// Template name
    pub template: String,

    /// Slot values
    pub values: HashMap<String, JsonValue>,

    /// CLIPS fact index
    pub fact_index: i64,

    /// Whether this fact was derived (vs input)
    pub derived: bool,

    /// Client-provided ID (if was in input)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

/// Execution trace for debugging/auditing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionTrace {
    /// Rules that fired during execution
    pub rules_fired: Vec<RuleFiring>,

    /// Facts asserted during execution (excluding initial input)
    pub facts_asserted: Vec<FactEvent>,

    /// Facts retracted during execution
    pub facts_retracted: Vec<FactEvent>,

    /// Agenda state at end of execution
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remaining_activations: Vec<ActivationInfo>,
}

/// Record of a rule firing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleFiring {
    /// Rule name
    pub rule_name: String,

    /// Module containing the rule
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,

    /// Number of times this rule fired
    pub fire_count: u64,

    /// Salience value
    #[serde(default)]
    pub salience: i32,
}

/// Output for rule-per-chunk streaming mode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleChunkOutput {
    /// Rule name that fired
    pub rule_name: String,

    /// Module containing the rule
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,

    /// Facts asserted by this rule firing
    pub facts: Vec<FactOutput>,
}

/// Record of a fact assertion or retraction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactEvent {
    /// Template name
    pub template: String,

    /// Fact index
    pub fact_index: i64,

    /// Slot values (for assertions)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<HashMap<String, JsonValue>>,
}

/// Information about an activation on the agenda
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivationInfo {
    /// Rule name
    pub rule_name: String,

    /// Salience value
    pub salience: i32,
}

/// Execution statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExecutionStats {
    /// Total number of rules that fired
    pub total_rules_fired: u64,

    /// Number of input facts (for usage tracking as "prompt tokens")
    pub input_facts_count: u64,

    /// Number of facts asserted (including input)
    pub facts_asserted: u64,

    /// Number of facts retracted
    pub facts_retracted: u64,

    /// Number of derived conclusions
    pub conclusions_count: u64,

    /// Execution time in milliseconds
    pub execution_time_ms: u64,

    /// Number of rule bases loaded
    pub rule_bases_loaded: u64,
}

// ============================================================================
// Rulebase Generation Types
// ============================================================================

/// Options for rulebase generation
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GenerationOptions {
    /// Model to use for generation (e.g., "claude-sonnet-4-5-20250514")
    #[serde(default)]
    pub model: Option<String>,

    /// Generation style
    #[serde(default)]
    pub style: GenerationStyle,

    /// Include test cases in output
    #[serde(default)]
    pub include_tests: bool,

    /// Include documentation in generated code
    #[serde(default = "default_true")]
    pub include_docs: bool,

    /// Validation level
    #[serde(default)]
    pub validation: ValidationLevel,
}

fn default_true() -> bool {
    true
}

/// Style of generated CLIPS code
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GenerationStyle {
    /// Minimal, compact code
    Minimal,
    /// Standard readable code
    #[default]
    Standard,
    /// Verbose with extensive comments
    Verbose,
}

/// Validation level for generated code
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ValidationLevel {
    /// No validation
    None,
    /// Syntax check only
    #[default]
    Syntax,
    /// Syntax + semantic checks
    Semantic,
    /// Full validation with test execution
    Full,
}

/// Result of rulebase generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedRulebase {
    /// The generated CLIPS code
    pub code: String,

    /// Validation result
    pub validation: ValidationResult,

    /// Generated test cases (if requested)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tests: Vec<GeneratedTestCase>,

    /// Generation metadata
    pub metadata: GenerationMetadata,
}

/// Validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    /// Whether validation passed
    pub passed: bool,

    /// Validation errors (if any)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<ValidationError>,

    /// Validation warnings
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<ValidationWarning>,
}

impl ValidationResult {
    /// Create a passed result
    pub fn passed() -> Self {
        Self {
            passed: true,
            errors: vec![],
            warnings: vec![],
        }
    }

    /// Create a skipped result
    pub fn skipped() -> Self {
        Self {
            passed: true,
            errors: vec![],
            warnings: vec![ValidationWarning {
                code: "SKIPPED".to_string(),
                message: "Validation was skipped".to_string(),
                location: None,
            }],
        }
    }

    /// Create a failed result
    pub fn failed(errors: Vec<ValidationError>) -> Self {
        Self {
            passed: false,
            errors,
            warnings: vec![],
        }
    }
}

/// Validation error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    /// Error code
    pub code: String,

    /// Error message
    pub message: String,

    /// Location in code (line number)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,

    /// Problematic construct
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub construct: Option<String>,
}

/// Validation warning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationWarning {
    /// Warning code
    pub code: String,

    /// Warning message
    pub message: String,

    /// Location in code
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
}

/// Generated test case
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedTestCase {
    /// Test name
    pub name: String,

    /// Test description
    pub description: String,

    /// Input facts for the test
    pub input_facts: Vec<FactAssertion>,

    /// Expected conclusions
    pub expected_conclusions: Vec<ExpectedFact>,

    /// Expected rules to fire
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expected_rules: Vec<String>,
}

/// Expected fact in test case
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectedFact {
    /// Template name
    pub template: String,

    /// Expected slot values (partial match OK)
    pub values: HashMap<String, JsonValue>,
}

/// Generation metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationMetadata {
    /// Source description/document
    pub source_type: String,

    /// Generator model used
    pub generator_model: String,

    /// Generation timestamp (ISO 8601)
    pub generated_at: String,

    /// Number of templates generated
    pub template_count: usize,

    /// Number of rules generated
    pub rule_count: usize,

    /// Generation duration in milliseconds
    pub generation_time_ms: u64,
}

// ============================================================================
// Conversion Helpers
// ============================================================================

impl From<serde_json::Value> for JsonValue {
    fn from(value: serde_json::Value) -> Self {
        match value {
            serde_json::Value::Null => JsonValue::Null,
            serde_json::Value::Bool(b) => JsonValue::Bool(b),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    JsonValue::Integer(i)
                } else if let Some(f) = n.as_f64() {
                    JsonValue::Float(f)
                } else {
                    JsonValue::Null
                }
            }
            serde_json::Value::String(s) => JsonValue::String(s),
            serde_json::Value::Array(arr) => {
                JsonValue::Array(arr.into_iter().map(Into::into).collect())
            }
            serde_json::Value::Object(obj) => {
                JsonValue::Object(obj.into_iter().map(|(k, v)| (k, v.into())).collect())
            }
        }
    }
}

impl From<JsonValue> for serde_json::Value {
    fn from(value: JsonValue) -> Self {
        match value {
            JsonValue::Null => serde_json::Value::Null,
            JsonValue::Bool(b) => serde_json::Value::Bool(b),
            JsonValue::Integer(i) => serde_json::Value::Number(i.into()),
            JsonValue::Float(f) => serde_json::Number::from_f64(f)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
            JsonValue::String(s) => serde_json::Value::String(s),
            JsonValue::Symbol(s) => serde_json::Value::String(s.symbol),
            JsonValue::Array(arr) => {
                serde_json::Value::Array(arr.into_iter().map(Into::into).collect())
            }
            JsonValue::Object(obj) => {
                serde_json::Value::Object(obj.into_iter().map(|(k, v)| (k, v.into())).collect())
            }
        }
    }
}

// ============================================================================
// Programmatic Rule Loading Types (Feature 033)
// ============================================================================

/// Module definition for programmatic creation via JSON
///
/// Represents a CLIPS defmodule that will be created from JSON input.
/// Modules provide namespace isolation for templates and rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleDefinition {
    /// Module name (typically uppercase by CLIPS convention, e.g., "SCREEN-SIZE")
    pub name: String,

    /// Optional documentation string describing the module's purpose
    #[serde(default)]
    pub doc: Option<String>,

    /// Optional list of module names from which to import constructs.
    /// Each import generates `(import MODULE deftemplate ?ALL)` in CLIPS.
    #[serde(default)]
    pub imports: Option<Vec<String>>,
}

impl ModuleDefinition {
    /// Validate the module definition
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `name` is empty
    /// - `name` conflicts with reserved module name "MAIN"
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() {
            return Err("Module name cannot be empty".to_string());
        }

        if self.name.to_uppercase() == "MAIN" {
            return Err("Module name 'MAIN' is reserved and cannot be redefined".to_string());
        }

        Ok(())
    }
}

/// Rule condition for structured JSON rule definitions
///
/// Represents a single pattern in the left-hand side (LHS) of a rule.
/// Used in structured rules as an alternative to raw CLIPS source strings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleCondition {
    /// Template name to match against (required)
    pub template: String,

    /// Slot-to-variable bindings (e.g., `{"width": "?w"}`).
    /// Keys are slot names, values are CLIPS variables.
    #[serde(default)]
    pub bindings: Option<HashMap<String, String>>,

    /// CLIPS constraint expressions applied to bound variables.
    /// Each string should be a valid CLIPS expression (e.g., `"(< ?w 768)"`).
    #[serde(default)]
    pub constraints: Option<Vec<String>>,
}

impl RuleCondition {
    /// Validate the rule condition
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `template` is empty
    pub fn validate(&self) -> Result<(), String> {
        if self.template.is_empty() {
            return Err("Condition template name cannot be empty".to_string());
        }

        Ok(())
    }
}

/// Rule action for structured JSON rule definitions
///
/// Represents a single action in the right-hand side (RHS) of a rule.
/// Used in structured rules as an alternative to raw CLIPS source strings.
/// Exactly one of `assert`, `retract`, or `modify` must be provided per action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleAction {
    /// Assert a new fact (creates `(assert ...)` action).
    /// Mutually exclusive with `retract` and `modify`.
    #[serde(default)]
    pub assert: Option<FactAssertion>,

    /// Retract facts matching a pattern string (creates `(retract ...)` action).
    /// Mutually exclusive with `assert` and `modify`.
    #[serde(default)]
    pub retract: Option<String>,

    /// Modify slot values of a matched fact (creates `(modify ...)` action).
    /// Keys are slot names, values are new values or variable references.
    /// Mutually exclusive with `assert` and `retract`.
    #[serde(default)]
    pub modify: Option<HashMap<String, String>>,
}

impl RuleAction {
    /// Validate the rule action
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - More than one of `assert`, `retract`, `modify` is provided
    /// - None of `assert`, `retract`, `modify` is provided
    pub fn validate(&self) -> Result<(), String> {
        let action_count = [
            self.assert.is_some(),
            self.retract.is_some(),
            self.modify.is_some(),
        ]
        .iter()
        .filter(|&&x| x)
        .count();

        if action_count == 0 {
            return Err(
                "Action must specify exactly one of: assert, retract, or modify".to_string(),
            );
        }

        if action_count > 1 {
            return Err("Action must specify only one of: assert, retract, or modify".to_string());
        }

        Ok(())
    }
}

/// Rule definition for programmatic creation via JSON
///
/// Represents a CLIPS defrule that will be created from JSON input.
/// Supports two mutually exclusive modes:
/// 1. Raw source string (full CLIPS expressiveness, escape hatch)
/// 2. Structured JSON (conditions + actions for common patterns)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleDefinition {
    /// Rule name (e.g., "classify-mobile"), without module prefix
    pub name: String,

    /// Optional module this rule belongs to.
    /// If specified, the rule fires in that module's namespace.
    #[serde(default)]
    pub module: Option<String>,

    /// Raw CLIPS defrule source string (escape hatch for full expressiveness).
    /// When provided, `conditions` and `actions` are ignored.
    /// Example: `"(defrule NAME condition => action)"`
    #[serde(default)]
    pub source: Option<String>,

    /// Structured conditions (alternative to `source`).
    /// For simple template-match + constraint patterns.
    /// Mutually exclusive with `source`.
    #[serde(default)]
    pub conditions: Option<Vec<RuleCondition>>,

    /// Structured actions (alternative to `source`).
    /// For simple assert/retract/modify actions.
    /// Mutually exclusive with `source`.
    #[serde(default)]
    pub actions: Option<Vec<RuleAction>>,

    /// Optional documentation string
    #[serde(default)]
    pub doc: Option<String>,

    /// Rule priority (salience). Higher values fire first. Default: 0.
    #[serde(default)]
    pub salience: Option<i32>,
}

impl RuleDefinition {
    /// Validate the rule definition
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `name` is empty
    /// - Both `source` and (`conditions` + `actions`) are provided
    /// - Neither `source` nor (`conditions` + `actions`) are provided
    /// - Any condition fails validation
    /// - Any action fails validation
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() {
            return Err("Rule name cannot be empty".to_string());
        }

        let has_source = self.source.is_some();
        let has_structured = self.conditions.is_some() || self.actions.is_some();

        if has_source && has_structured {
            return Err(
                "Rule must use either 'source' OR ('conditions' + 'actions'), not both".to_string(),
            );
        }

        if !has_source && !has_structured {
            return Err(
                "Rule must specify either 'source' OR ('conditions' + 'actions')".to_string(),
            );
        }

        // Validate conditions
        if let Some(conditions) = &self.conditions {
            for condition in conditions {
                condition.validate()?;
            }
        }

        // Validate actions
        if let Some(actions) = &self.actions {
            for action in actions {
                action.validate()?;
            }
        }

        Ok(())
    }
}

// ============================================================================
// Notes on ClipsInput Extensions
// ============================================================================
//
// The following fields were added to the ClipsInput struct for Feature 033:
//   pub modules: Vec<ModuleDefinition>,     // NEW (Phase 2.1)
//   pub rules: Vec<RuleDefinition>,         // NEW (Phase 2.1)
//   pub policy_id: Option<String>,          // NEW (Phase 2.1)
//   pub strict_policy_id: Option<bool>,     // NEW (Phase 2.1)
//
// All new fields are optional and maintain full backward compatibility.

#[cfg(test)]
#[allow(clippy::panic, clippy::approx_constant)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_clips_input() {
        let json = r#"{
            "facts": [
                {"template": "person", "values": {"name": "Alice", "age": 30}}
            ]
        }"#;

        let input: ClipsInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.facts.len(), 1);
        assert_eq!(input.facts[0].template, "person");
    }

    #[test]
    fn test_parse_template_definition() {
        let json = r#"{
            "name": "customer",
            "slots": [
                {"name": "id", "type": "STRING"},
                {"name": "balance", "type": "FLOAT", "default": 0.0}
            ]
        }"#;

        let template: TemplateDefinition = serde_json::from_str(json).unwrap();
        assert_eq!(template.name, "customer");
        assert_eq!(template.slots.len(), 2);
        assert_eq!(template.slots[0].slot_type, SlotType::String);
    }

    #[test]
    fn test_json_value_conversions() {
        let json = serde_json::json!({
            "int": 42,
            "float": 3.14,
            "string": "hello",
            "bool": true,
            "array": [1, 2, 3],
            "null": null
        });

        let value: JsonValue = json.into();
        if let JsonValue::Object(obj) = value {
            assert!(matches!(obj.get("int"), Some(JsonValue::Integer(42))));
            assert!(matches!(obj.get("bool"), Some(JsonValue::Bool(true))));
        } else {
            panic!("Expected object");
        }
    }

    #[test]
    fn test_module_definition_parsing() {
        let json = r#"{
            "name": "SCREEN-SIZE",
            "doc": "Screen classification module",
            "imports": ["MAIN"]
        }"#;

        let module: ModuleDefinition = serde_json::from_str(json).unwrap();
        assert_eq!(module.name, "SCREEN-SIZE");
        assert_eq!(module.doc, Some("Screen classification module".to_string()));
        assert_eq!(module.imports, Some(vec!["MAIN".to_string()]));
    }

    #[test]
    fn test_module_definition_validation() {
        let mut module = ModuleDefinition {
            name: String::new(),
            doc: None,
            imports: None,
        };

        assert!(module.validate().is_err());

        module.name = "MAIN".to_string();
        assert!(module.validate().is_err());

        module.name = "VALID-MODULE".to_string();
        assert!(module.validate().is_ok());
    }

    #[test]
    fn test_rule_condition_parsing() {
        let json = r#"{
            "template": "screen-config",
            "bindings": {"width": "?w"},
            "constraints": ["(< ?w 768)"]
        }"#;

        let condition: RuleCondition = serde_json::from_str(json).unwrap();
        assert_eq!(condition.template, "screen-config");
        assert_eq!(
            condition.bindings.as_ref().unwrap().get("width").unwrap(),
            "?w"
        );
    }

    #[test]
    fn test_rule_action_validation() {
        let mut action = RuleAction {
            assert: None,
            retract: None,
            modify: None,
        };

        assert!(action.validate().is_err());

        action.assert = Some(FactAssertion {
            template: "fact".to_string(),
            values: HashMap::new(),
            id: None,
        });

        assert!(action.validate().is_ok());

        action.retract = Some("pattern".to_string());
        assert!(action.validate().is_err());
    }

    #[test]
    fn test_rule_definition_source_mode() {
        let json = r#"{
            "name": "classify-mobile",
            "module": "SCREEN-SIZE",
            "source": "(defrule classify-mobile (screen (width ?w&:(< ?w 768))) => (assert (device mobile)))",
            "salience": 10
        }"#;

        let rule: RuleDefinition = serde_json::from_str(json).unwrap();
        assert_eq!(rule.name, "classify-mobile");
        assert!(rule.source.is_some());
        assert!(rule.conditions.is_none());
        assert!(rule.validate().is_ok());
    }

    #[test]
    fn test_rule_definition_structured_mode() {
        let json = r#"{
            "name": "classify-mobile",
            "module": "SCREEN-SIZE",
            "conditions": [
                {
                    "template": "screen-config",
                    "bindings": {"width": "?w"},
                    "constraints": ["(< ?w 768)"]
                }
            ],
            "actions": [
                {
                    "assert": {
                        "template": "device-class",
                        "values": {"type": "mobile"}
                    }
                }
            ],
            "salience": 10
        }"#;

        let rule: RuleDefinition = serde_json::from_str(json).unwrap();
        assert_eq!(rule.name, "classify-mobile");
        assert!(rule.source.is_none());
        assert!(rule.conditions.is_some());
        assert!(rule.validate().is_ok());
    }

    #[test]
    fn test_rule_definition_validation_mixed_modes() {
        let rule = RuleDefinition {
            name: "test-rule".to_string(),
            module: None,
            source: Some("(defrule test (fact) => (assert (result)))".to_string()),
            conditions: Some(vec![RuleCondition {
                template: "fact".to_string(),
                bindings: None,
                constraints: None,
            }]),
            actions: None,
            doc: None,
            salience: None,
        };

        assert!(rule.validate().is_err());
    }

    #[test]
    fn test_clips_input_with_modules_and_rules() {
        let json = r#"{
            "modules": [
                {"name": "SCREEN-SIZE"}
            ],
            "templates": [
                {
                    "name": "screen-config",
                    "slots": [{"name": "width", "type": "INTEGER"}]
                }
            ],
            "rules": [
                {
                    "name": "classify-mobile",
                    "module": "SCREEN-SIZE",
                    "source": "(defrule classify-mobile (screen-config (width ?w&:(< ?w 768))) => (assert (device-class (type mobile))))"
                }
            ],
            "facts": [
                {"template": "screen-config", "values": {"width": 375}}
            ],
            "policy_id": "screen-policy-v2.3"
        }"#;

        let input: ClipsInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.modules.len(), 1);
        assert_eq!(input.rules.len(), 1);
        assert_eq!(input.facts.len(), 1);
        assert_eq!(input.policy_id, Some("screen-policy-v2.3".to_string()));
    }
}

// ============================================================================
// Content Hash & Cache Tests (Feature 033)
// ============================================================================

#[cfg(test)]
mod content_hash_tests {
    use super::*;

    #[test]
    fn test_content_hash_deterministic() {
        // Create rule program
        let module = ModuleDefinition {
            name: "TEST".to_string(),
            doc: Some("Test module".to_string()),
            imports: None,
        };

        let template = TemplateDefinition {
            name: "fact".to_string(),
            doc: None,
            slots: vec![SlotDefinition {
                name: "value".to_string(),
                slot_type: SlotType::Integer,
                multislot: false,
                default: None,
                allowed_values: None,
                range: None,
                cardinality: None,
            }],
        };

        let rule = RuleDefinition {
            name: "test-rule".to_string(),
            module: Some("TEST".to_string()),
            source: Some("(defrule test-rule (fact) => (assert (result)))".to_string()),
            conditions: None,
            actions: None,
            doc: None,
            salience: None,
        };

        // Compute hash twice
        let hash1 = crate::providers::clips::provider::ClipsProvider::compute_content_hash(
            std::slice::from_ref(&module),
            std::slice::from_ref(&template),
            std::slice::from_ref(&rule),
        );

        let hash2 = crate::providers::clips::provider::ClipsProvider::compute_content_hash(
            &[module],
            &[template],
            &[rule],
        );

        // Should be identical
        assert_eq!(hash1, hash2);
        assert!(hash1.starts_with("sha256:"));
    }

    #[test]
    fn test_content_hash_different_rules() {
        let rule1 = RuleDefinition {
            name: "rule1".to_string(),
            module: None,
            source: Some("(defrule rule1 (fact) => (assert (result)))".to_string()),
            conditions: None,
            actions: None,
            doc: None,
            salience: None,
        };

        let rule2 = RuleDefinition {
            name: "rule2".to_string(),
            module: None,
            source: Some("(defrule rule2 (fact) => (assert (result)))".to_string()),
            conditions: None,
            actions: None,
            doc: None,
            salience: None,
        };

        let hash1 = crate::providers::clips::provider::ClipsProvider::compute_content_hash(
            &[],
            &[],
            &[rule1],
        );

        let hash2 = crate::providers::clips::provider::ClipsProvider::compute_content_hash(
            &[],
            &[],
            &[rule2],
        );

        // Different rules should produce different hashes
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_content_hash_empty_program() {
        let hash =
            crate::providers::clips::provider::ClipsProvider::compute_content_hash(&[], &[], &[]);
        assert!(hash.starts_with("sha256:"));
    }

    #[test]
    fn test_resolve_cache_key_with_policy_id() {
        let key = crate::providers::clips::provider::ClipsProvider::resolve_cache_key(
            Some("my-policy"),
            "sha256:abc123",
            None,
        );
        assert_eq!(key, "my-policy");
    }

    #[test]
    fn test_resolve_cache_key_without_policy_id() {
        let key = crate::providers::clips::provider::ClipsProvider::resolve_cache_key(
            None,
            "sha256:abc123",
            None,
        );
        assert_eq!(key, "sha256:abc123");
    }

    #[test]
    fn test_resolve_cache_key_with_model_name() {
        let key = crate::providers::clips::provider::ClipsProvider::resolve_cache_key(
            None,
            "sha256:abc123",
            Some("my-model"),
        );
        assert_eq!(key, "my-model+sha256:abc123");
    }

    #[test]
    fn test_resolve_cache_key_priority() {
        // policy_id should take priority over model_name
        let key = crate::providers::clips::provider::ClipsProvider::resolve_cache_key(
            Some("policy-1"),
            "sha256:abc123",
            Some("my-model"),
        );
        assert_eq!(key, "policy-1");
    }
}
