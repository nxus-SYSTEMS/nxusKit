package pipeline

import (
	"testing"
)

func TestValidateDAG_ValidPipeline(t *testing.T) {
	pipeline := &PipelineDefinition{
		ID:   "test",
		Name: "Test",
		Stages: []Stage{
			{ID: "a", Name: "A", Type: StageTypeLLM},
			{ID: "b", Name: "B", Type: StageTypeLLM, UpstreamStageIDs: []string{"a"}},
			{ID: "c", Name: "C", Type: StageTypeLLM, UpstreamStageIDs: []string{"a", "b"}},
		},
	}

	err := ValidateDAG(pipeline)
	if err != nil {
		t.Errorf("Expected valid DAG, got error: %v", err)
	}
}

func TestValidateDAG_CycleDetection(t *testing.T) {
	pipeline := &PipelineDefinition{
		ID:   "test",
		Name: "Test",
		Stages: []Stage{
			{ID: "a", Name: "A", Type: StageTypeLLM, UpstreamStageIDs: []string{"c"}},
			{ID: "b", Name: "B", Type: StageTypeLLM, UpstreamStageIDs: []string{"a"}},
			{ID: "c", Name: "C", Type: StageTypeLLM, UpstreamStageIDs: []string{"b"}},
		},
	}

	err := ValidateDAG(pipeline)
	if err == nil {
		t.Error("Expected cycle detection error, got nil")
	}

	valErr, ok := err.(*ValidationError)
	if !ok {
		t.Errorf("Expected ValidationError, got %T", err)
	}
	if valErr.Code != "CYCLE_DETECTED" {
		t.Errorf("Expected CYCLE_DETECTED code, got %s", valErr.Code)
	}
}

func TestValidateReferences_MissingReference(t *testing.T) {
	pipeline := &PipelineDefinition{
		ID:   "test",
		Name: "Test",
		Stages: []Stage{
			{ID: "a", Name: "A", Type: StageTypeLLM},
			{ID: "b", Name: "B", Type: StageTypeLLM, UpstreamStageIDs: []string{"nonexistent"}},
		},
	}

	err := ValidateReferences(pipeline)
	if err == nil {
		t.Error("Expected missing reference error, got nil")
	}

	valErr, ok := err.(*ValidationError)
	if !ok {
		t.Errorf("Expected ValidationError, got %T", err)
	}
	if valErr.Code != "UNKNOWN_UPSTREAM" {
		t.Errorf("Expected UNKNOWN_UPSTREAM code, got %s", valErr.Code)
	}
}

func TestValidateUniqueIDs_DuplicateID(t *testing.T) {
	pipeline := &PipelineDefinition{
		ID:   "test",
		Name: "Test",
		Stages: []Stage{
			{ID: "a", Name: "A", Type: StageTypeLLM},
			{ID: "a", Name: "A Duplicate", Type: StageTypeLLM},
		},
	}

	err := ValidateUniqueIDs(pipeline)
	if err == nil {
		t.Error("Expected duplicate ID error, got nil")
	}

	valErr, ok := err.(*ValidationError)
	if !ok {
		t.Errorf("Expected ValidationError, got %T", err)
	}
	if valErr.Code != "DUPLICATE_ID" {
		t.Errorf("Expected DUPLICATE_ID code, got %s", valErr.Code)
	}
}

func TestValidateStageConfig_MissingLlmConfig(t *testing.T) {
	stage := &Stage{
		ID:   "test",
		Name: "Test",
		Type: StageTypeLLM,
		// Missing LlmConfig
	}

	err := ValidateStageConfig(stage)
	if err == nil {
		t.Error("Expected missing config error, got nil")
	}

	valErr, ok := err.(*ValidationError)
	if !ok {
		t.Errorf("Expected ValidationError, got %T", err)
	}
	if valErr.Code != "MISSING_CONFIG" {
		t.Errorf("Expected MISSING_CONFIG code, got %s", valErr.Code)
	}
}

func TestValidateStageConfig_MissingClipsConfig(t *testing.T) {
	stage := &Stage{
		ID:   "test",
		Name: "Test",
		Type: StageTypeClipsEval,
		// Missing ClipsConfig
	}

	err := ValidateStageConfig(stage)
	if err == nil {
		t.Error("Expected missing config error, got nil")
	}

	valErr, ok := err.(*ValidationError)
	if !ok {
		t.Errorf("Expected ValidationError, got %T", err)
	}
	if valErr.Code != "MISSING_CONFIG" {
		t.Errorf("Expected MISSING_CONFIG code, got %s", valErr.Code)
	}
}

func TestValidateStageConfig_ClipsConfigMissingRulesContent(t *testing.T) {
	stage := &Stage{
		ID:   "test",
		Name: "Test",
		Type: StageTypeClipsEval,
		ClipsConfig: &ClipsConfig{
			RulesSource: RulesSourceInline,
			// Missing RulesContent
		},
	}

	err := ValidateStageConfig(stage)
	if err == nil {
		t.Error("Expected clips config error, got nil")
	}

	valErr, ok := err.(*ValidationError)
	if !ok {
		t.Errorf("Expected ValidationError, got %T", err)
	}
	if valErr.Code != "CLIPS_CONFIG_ERROR" {
		t.Errorf("Expected CLIPS_CONFIG_ERROR code, got %s", valErr.Code)
	}
}

func TestValidateStageConfig_ClipsConfigMissingSavePath(t *testing.T) {
	stage := &Stage{
		ID:   "test",
		Name: "Test",
		Type: StageTypeClipsEval,
		ClipsConfig: &ClipsConfig{
			RulesSource:  RulesSourceInline,
			RulesContent: "(defrule test)",
			SaveRules:    true,
			// Missing SavePath
		},
	}

	err := ValidateStageConfig(stage)
	if err == nil {
		t.Error("Expected clips config error, got nil")
	}

	valErr, ok := err.(*ValidationError)
	if !ok {
		t.Errorf("Expected ValidationError, got %T", err)
	}
	if valErr.Code != "CLIPS_CONFIG_ERROR" {
		t.Errorf("Expected CLIPS_CONFIG_ERROR code, got %s", valErr.Code)
	}
}

func TestPipelineDefinition_Validate(t *testing.T) {
	pipeline := &PipelineDefinition{
		ID:   "test",
		Name: "Test",
		Stages: []Stage{
			{
				ID:   "a",
				Name: "A",
				Type: StageTypeLLM,
				LlmConfig: &LlmStageConfig{
					Provider:   "claude",
					Model:      "claude-3-sonnet",
					UserPrompt: "Test",
				},
			},
			{
				ID:               "b",
				Name:             "B",
				Type:             StageTypeLLM,
				UpstreamStageIDs: []string{"a"},
				LlmConfig: &LlmStageConfig{
					Provider:   "claude",
					Model:      "claude-3-sonnet",
					UserPrompt: "Test",
				},
			},
		},
	}

	err := pipeline.Validate()
	if err != nil {
		t.Errorf("Expected valid pipeline, got error: %v", err)
	}
}
