//! Pipeline definition types.
//!
//! This module defines the core types for pipeline configurations including
//! stages, configs, and the pipeline definition itself.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Stage type discriminator.
///
/// Determines what kind of processing a stage performs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageType {
    /// LLM-only stage (calls an LLM provider)
    Llm,
    /// CLIPS evaluation stage (runs CLIPS rules)
    ClipsEval,
    /// CLIPS generation stage (generates facts from CLIPS)
    ClipsGen,
    /// Hybrid stage (combines LLM and CLIPS)
    Hybrid,
}

impl std::fmt::Display for StageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StageType::Llm => write!(f, "llm"),
            StageType::ClipsEval => write!(f, "clips_eval"),
            StageType::ClipsGen => write!(f, "clips_gen"),
            StageType::Hybrid => write!(f, "hybrid"),
        }
    }
}

/// Source of CLIPS rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RulesSource {
    /// Rules provided inline in the configuration
    Inline,
    /// Rules loaded from a file path
    File,
    /// Rules generated dynamically at runtime
    Dynamic,
}

/// Security validation severity level.
///
/// Controls how dangerous CLIPS constructs are handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ValidationSeverity {
    /// Reject rules with dangerous constructs (default)
    #[default]
    Error,
    /// Log warning but proceed with loading
    Warning,
    /// Log info message only
    Info,
    /// Skip validation entirely
    Ignore,
}

/// Configuration for CLIPS-based stages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipsConfig {
    /// Source of CLIPS rules
    pub rules_source: RulesSource,

    /// Inline rules or file path (based on rules_source)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rules_content: Option<String>,

    /// Persist rules after stage execution
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub save_rules: bool,

    /// Path for saved rules (required if save_rules is true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub save_path: Option<String>,

    /// Use CLIPS binary format for save
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bsave_binary: bool,

    /// Security validation severity level
    #[serde(default, skip_serializing_if = "is_default_severity")]
    pub validation_severity: ValidationSeverity,

    /// CLIPS execution timeout in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

fn is_default_severity(severity: &ValidationSeverity) -> bool {
    *severity == ValidationSeverity::Error
}

/// Configuration for LLM-based stages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmStageConfig {
    /// Provider name (claude, openai, ollama, etc.)
    pub provider: String,

    /// Model identifier
    pub model: String,

    /// System prompt template
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,

    /// User prompt template with {{variable}} placeholders
    pub user_prompt: String,

    /// Sampling temperature
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,

    /// Maximum response tokens
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,

    /// Provider-specific parameters
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub additional_params: HashMap<String, serde_json::Value>,

    /// Multiple providers for parallel execution (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub providers: Option<Vec<String>>,
}

/// Retry configuration for stage execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum retry attempts
    #[serde(default = "default_max_attempts")]
    pub max_attempts: u32,

    /// Initial backoff delay in milliseconds
    #[serde(default = "default_backoff_ms")]
    pub backoff_ms: u64,

    /// Exponential backoff multiplier
    #[serde(default = "default_backoff_multiplier")]
    pub backoff_multiplier: f64,
}

fn default_max_attempts() -> u32 {
    3
}

fn default_backoff_ms() -> u64 {
    1000
}

fn default_backoff_multiplier() -> f64 {
    2.0
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: default_max_attempts(),
            backoff_ms: default_backoff_ms(),
            backoff_multiplier: default_backoff_multiplier(),
        }
    }
}

/// A single stage in the pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stage {
    /// Stage identifier (unique within pipeline)
    pub id: String,

    /// Human-readable stage name
    pub name: String,

    /// Stage type discriminator
    #[serde(rename = "type")]
    pub stage_type: StageType,

    /// IDs of stages this depends on (DAG edges)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub upstream_stage_ids: Vec<String>,

    /// LLM configuration (required for llm and hybrid stages)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_config: Option<LlmStageConfig>,

    /// CLIPS configuration (required for clips_eval, clips_gen, and hybrid stages)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clips_config: Option<ClipsConfig>,

    /// Stage execution timeout in milliseconds
    #[serde(
        default = "default_timeout_ms",
        skip_serializing_if = "is_default_timeout"
    )]
    pub timeout_ms: u64,

    /// Retry configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry: Option<RetryConfig>,
}

fn default_timeout_ms() -> u64 {
    30000
}

fn is_default_timeout(timeout: &u64) -> bool {
    *timeout == 30000
}

/// A complete pipeline definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineDefinition {
    /// Unique pipeline identifier (UUID)
    pub id: String,

    /// Human-readable pipeline name
    pub name: String,

    /// Optional pipeline description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Schema version
    #[serde(default = "default_version")]
    pub version: String,

    /// Ordered list of pipeline stages
    pub stages: Vec<Stage>,

    /// Creation timestamp (ISO 8601)
    pub created_at: String,

    /// Last modification timestamp
    pub updated_at: String,

    /// Arbitrary key-value metadata
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
}

fn default_version() -> String {
    "1.0".to_string()
}

/// Result from multi-provider stage execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiProviderResult {
    /// Results keyed by provider name
    pub results: HashMap<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stage_type_display() {
        assert_eq!(StageType::Llm.to_string(), "llm");
        assert_eq!(StageType::ClipsEval.to_string(), "clips_eval");
        assert_eq!(StageType::ClipsGen.to_string(), "clips_gen");
        assert_eq!(StageType::Hybrid.to_string(), "hybrid");
    }

    #[test]
    fn test_validation_severity_default() {
        assert_eq!(ValidationSeverity::default(), ValidationSeverity::Error);
    }

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.backoff_ms, 1000);
        assert_eq!(config.backoff_multiplier, 2.0);
    }
}
