// Package pipeline provides format conversion for Peeler compatibility.
package pipeline

import (
	"log/slog"
	"time"
)

// PeelerPipeline is the Peeler-compatible pipeline format.
// This is a simplified format that strips nxusKit-specific fields.
type PeelerPipeline struct {
	// ID is the unique pipeline identifier.
	ID string `json:"id" yaml:"id"`

	// Name is the human-readable pipeline name.
	Name string `json:"name" yaml:"name"`

	// Description is an optional description.
	Description string `json:"description,omitempty" yaml:"description,omitempty"`

	// Stages contains the pipeline stages.
	Stages []PeelerStage `json:"stages" yaml:"stages"`

	// Metadata contains arbitrary key-value metadata.
	Metadata map[string]any `json:"metadata,omitempty" yaml:"metadata,omitempty"`
}

// PeelerStage is the Peeler-compatible stage format.
type PeelerStage struct {
	// ID is the stage identifier.
	ID string `json:"id" yaml:"id"`

	// Name is the stage name.
	Name string `json:"name" yaml:"name"`

	// Type is the stage type (only "llm" supported in Peeler).
	Type string `json:"type" yaml:"type"`

	// UpstreamStageIDs are the upstream dependencies.
	UpstreamStageIDs []string `json:"upstream_stage_ids,omitempty" yaml:"upstream_stage_ids,omitempty"`

	// Provider is the provider name.
	Provider string `json:"provider,omitempty" yaml:"provider,omitempty"`

	// Model is the model name.
	Model string `json:"model,omitempty" yaml:"model,omitempty"`

	// SystemPrompt is the system prompt.
	SystemPrompt string `json:"system_prompt,omitempty" yaml:"system_prompt,omitempty"`

	// UserPrompt is the user prompt.
	UserPrompt string `json:"user_prompt,omitempty" yaml:"user_prompt,omitempty"`

	// Temperature is the sampling temperature.
	Temperature *float64 `json:"temperature,omitempty" yaml:"temperature,omitempty"`

	// MaxTokens is the maximum response tokens.
	MaxTokens *uint32 `json:"max_tokens,omitempty" yaml:"max_tokens,omitempty"`
}

// ToPeelerFormat converts a nxusKit pipeline to Peeler format.
// This strips nxusKit-specific fields (CLIPS config, retry config, etc.)
// and flattens the LLM config into the stage level.
// CLIPS-only stages are filtered out.
func ToPeelerFormat(pipeline *PipelineDefinition) *PeelerPipeline {
	stages := make([]PeelerStage, 0, len(pipeline.Stages))

	for _, stage := range pipeline.Stages {
		if peelerStage := convertStageToPeeler(&stage); peelerStage != nil {
			stages = append(stages, *peelerStage)
		}
	}

	return &PeelerPipeline{
		ID:          pipeline.ID,
		Name:        pipeline.Name,
		Description: pipeline.Description,
		Stages:      stages,
		Metadata:    pipeline.Metadata,
	}
}

// convertStageToPeeler converts a single stage to Peeler format.
func convertStageToPeeler(stage *Stage) *PeelerStage {
	// Only convert stages that have LLM config
	// Pure CLIPS stages are not supported in Peeler
	switch stage.Type {
	case StageTypeClipsEval, StageTypeClipsGen:
		slog.Debug("Skipping CLIPS-only stage in Peeler conversion",
			"stage_id", stage.ID)
		return nil
	}

	if stage.LlmConfig == nil {
		return nil
	}

	return &PeelerStage{
		ID:               stage.ID,
		Name:             stage.Name,
		Type:             "llm", // Peeler only supports "llm"
		UpstreamStageIDs: stage.UpstreamStageIDs,
		Provider:         stage.LlmConfig.Provider,
		Model:            stage.LlmConfig.Model,
		SystemPrompt:     stage.LlmConfig.SystemPrompt,
		UserPrompt:       stage.LlmConfig.UserPrompt,
		Temperature:      stage.LlmConfig.Temperature,
		MaxTokens:        stage.LlmConfig.MaxTokens,
	}
}

// FromPeelerFormat converts a Peeler pipeline to nxusKit format.
// This adds nxusKit-specific structure while preserving the core
// pipeline definition. Unknown stage types are skipped with a warning.
func FromPeelerFormat(peeler *PeelerPipeline) *PipelineDefinition {
	stages := make([]Stage, 0, len(peeler.Stages))

	for _, stage := range peeler.Stages {
		if nxusStage := convertStageFromPeeler(&stage); nxusStage != nil {
			stages = append(stages, *nxusStage)
		}
	}

	now := time.Now().UTC()

	return &PipelineDefinition{
		ID:          peeler.ID,
		Name:        peeler.Name,
		Description: peeler.Description,
		Version:     "1.0",
		Stages:      stages,
		CreatedAt:   now,
		UpdatedAt:   now,
		Metadata:    peeler.Metadata,
	}
}

// convertStageFromPeeler converts a single Peeler stage to nxusKit format.
func convertStageFromPeeler(stage *PeelerStage) *Stage {
	// Only convert "llm" stage type
	if stage.Type != "llm" {
		slog.Warn("Skipping unknown stage type in Peeler conversion",
			"stage_id", stage.ID,
			"stage_type", stage.Type)
		return nil
	}

	return &Stage{
		ID:               stage.ID,
		Name:             stage.Name,
		Type:             StageTypeLLM,
		UpstreamStageIDs: stage.UpstreamStageIDs,
		LlmConfig: &LlmStageConfig{
			Provider:     stage.Provider,
			Model:        stage.Model,
			SystemPrompt: stage.SystemPrompt,
			UserPrompt:   stage.UserPrompt,
			Temperature:  stage.Temperature,
			MaxTokens:    stage.MaxTokens,
		},
		TimeoutMs: GetDefaultTimeoutMs(),
	}
}
