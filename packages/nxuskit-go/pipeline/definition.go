// Package pipeline provides pipeline definition types.
package pipeline

import (
	"encoding/json"
	"time"
)

// StageType discriminates the kind of processing a stage performs.
type StageType string

const (
	// StageTypeLLM is an LLM-only stage (calls an LLM provider).
	StageTypeLLM StageType = "llm"
	// StageTypeClipsEval runs CLIPS rules for evaluation.
	StageTypeClipsEval StageType = "clips_eval"
	// StageTypeClipsGen generates facts from CLIPS.
	StageTypeClipsGen StageType = "clips_gen"
	// StageTypeHybrid combines LLM and CLIPS processing.
	StageTypeHybrid StageType = "hybrid"
)

// RulesSource indicates where CLIPS rules come from.
type RulesSource string

const (
	// RulesSourceInline means rules are provided inline in the configuration.
	RulesSourceInline RulesSource = "inline"
	// RulesSourceFile means rules are loaded from a file path.
	RulesSourceFile RulesSource = "file"
	// RulesSourceDynamic means rules are generated at runtime.
	RulesSourceDynamic RulesSource = "dynamic"
)

// ValidationSeverity controls how dangerous CLIPS constructs are handled.
type ValidationSeverity string

const (
	// ValidationSeverityError rejects rules with dangerous constructs (default).
	ValidationSeverityError ValidationSeverity = "error"
	// ValidationSeverityWarning logs a warning but proceeds with loading.
	ValidationSeverityWarning ValidationSeverity = "warning"
	// ValidationSeverityInfo logs an info message only.
	ValidationSeverityInfo ValidationSeverity = "info"
	// ValidationSeverityIgnore skips validation entirely.
	ValidationSeverityIgnore ValidationSeverity = "ignore"
)

// ClipsConfig holds configuration for CLIPS-based stages.
type ClipsConfig struct {
	// RulesSource indicates where rules come from.
	RulesSource RulesSource `json:"rules_source" yaml:"rules_source"`

	// RulesContent contains inline rules or file path (based on RulesSource).
	RulesContent string `json:"rules_content,omitempty" yaml:"rules_content,omitempty"`

	// SaveRules indicates whether to persist rules after stage execution.
	SaveRules bool `json:"save_rules,omitempty" yaml:"save_rules,omitempty"`

	// SavePath is the path for saved rules (required if SaveRules is true).
	SavePath string `json:"save_path,omitempty" yaml:"save_path,omitempty"`

	// BSaveBinary uses CLIPS binary format for save.
	BSaveBinary bool `json:"bsave_binary,omitempty" yaml:"bsave_binary,omitempty"`

	// ValidationSeverity is the security validation severity level.
	ValidationSeverity ValidationSeverity `json:"validation_severity,omitempty" yaml:"validation_severity,omitempty"`

	// TimeoutMs is the CLIPS execution timeout in milliseconds.
	TimeoutMs *uint64 `json:"timeout_ms,omitempty" yaml:"timeout_ms,omitempty"`
}

// LlmStageConfig holds configuration for LLM-based stages.
type LlmStageConfig struct {
	// Provider name (claude, openai, ollama, etc.).
	Provider string `json:"provider" yaml:"provider"`

	// Model identifier.
	Model string `json:"model" yaml:"model"`

	// SystemPrompt is the system prompt template.
	SystemPrompt string `json:"system_prompt,omitempty" yaml:"system_prompt,omitempty"`

	// UserPrompt is the user prompt template with {{variable}} placeholders.
	UserPrompt string `json:"user_prompt" yaml:"user_prompt"`

	// Temperature is the sampling temperature.
	Temperature *float64 `json:"temperature,omitempty" yaml:"temperature,omitempty"`

	// MaxTokens is the maximum response tokens.
	MaxTokens *uint32 `json:"max_tokens,omitempty" yaml:"max_tokens,omitempty"`

	// AdditionalParams contains provider-specific parameters.
	AdditionalParams map[string]any `json:"additional_params,omitempty" yaml:"additional_params,omitempty"`

	// Providers lists multiple providers for parallel execution (optional).
	Providers []string `json:"providers,omitempty" yaml:"providers,omitempty"`
}

// RetryConfig holds retry configuration for stage execution.
type RetryConfig struct {
	// MaxAttempts is the maximum retry attempts (default: 3).
	MaxAttempts uint32 `json:"max_attempts,omitempty" yaml:"max_attempts,omitempty"`

	// BackoffMs is the initial backoff delay in milliseconds (default: 1000).
	BackoffMs uint64 `json:"backoff_ms,omitempty" yaml:"backoff_ms,omitempty"`

	// BackoffMultiplier is the exponential backoff multiplier (default: 2.0).
	BackoffMultiplier float64 `json:"backoff_multiplier,omitempty" yaml:"backoff_multiplier,omitempty"`
}

// DefaultRetryConfig returns a RetryConfig with default values.
func DefaultRetryConfig() RetryConfig {
	return RetryConfig{
		MaxAttempts:       3,
		BackoffMs:         1000,
		BackoffMultiplier: 2.0,
	}
}

// Stage represents a single stage in the pipeline.
type Stage struct {
	// ID is the stage identifier (unique within pipeline).
	ID string `json:"id" yaml:"id"`

	// Name is the human-readable stage name.
	Name string `json:"name" yaml:"name"`

	// Type is the stage type discriminator.
	Type StageType `json:"type" yaml:"type"`

	// UpstreamStageIDs are IDs of stages this depends on (DAG edges).
	UpstreamStageIDs []string `json:"upstream_stage_ids,omitempty" yaml:"upstream_stage_ids,omitempty"`

	// LlmConfig is the LLM configuration (required for llm and hybrid stages).
	LlmConfig *LlmStageConfig `json:"llm_config,omitempty" yaml:"llm_config,omitempty"`

	// ClipsConfig is the CLIPS configuration (required for clips_eval, clips_gen, and hybrid stages).
	ClipsConfig *ClipsConfig `json:"clips_config,omitempty" yaml:"clips_config,omitempty"`

	// TimeoutMs is the stage execution timeout in milliseconds (default: 30000).
	TimeoutMs uint64 `json:"timeout_ms,omitempty" yaml:"timeout_ms,omitempty"`

	// Retry is the retry configuration.
	Retry *RetryConfig `json:"retry,omitempty" yaml:"retry,omitempty"`
}

// PipelineDefinition represents a complete pipeline configuration.
type PipelineDefinition struct {
	// ID is the unique pipeline identifier (UUID).
	ID string `json:"id" yaml:"id"`

	// Name is the human-readable pipeline name.
	Name string `json:"name" yaml:"name"`

	// Description is an optional pipeline description.
	Description string `json:"description,omitempty" yaml:"description,omitempty"`

	// Version is the schema version (default: "1.0").
	Version string `json:"version,omitempty" yaml:"version,omitempty"`

	// Stages is the ordered list of pipeline stages.
	Stages []Stage `json:"stages" yaml:"stages"`

	// CreatedAt is the creation timestamp (ISO 8601).
	CreatedAt time.Time `json:"created_at" yaml:"created_at"`

	// UpdatedAt is the last modification timestamp.
	UpdatedAt time.Time `json:"updated_at" yaml:"updated_at"`

	// Metadata contains arbitrary key-value metadata.
	Metadata map[string]any `json:"metadata,omitempty" yaml:"metadata,omitempty"`
}

// MultiProviderResult holds results from multi-provider stage execution.
type MultiProviderResult struct {
	// Results keyed by provider name.
	Results map[string]json.RawMessage `json:"results"`
}

// GetDefaultTimeoutMs returns the default stage timeout (30000ms).
func GetDefaultTimeoutMs() uint64 {
	return 30000
}

// GetDefaultVersion returns the default schema version ("1.0").
func GetDefaultVersion() string {
	return "1.0"
}
