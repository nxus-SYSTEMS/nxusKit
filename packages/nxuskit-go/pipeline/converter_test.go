package pipeline

import (
	"testing"
	"time"
)

func TestToPeelerFormat(t *testing.T) {
	temp := 0.7
	maxTokens := uint32(1000)

	pipeline := &PipelineDefinition{
		ID:          "test-id",
		Name:        "Test Pipeline",
		Description: "Test description",
		Version:     "1.0",
		Stages: []Stage{
			{
				ID:   "stage1",
				Name: "Stage 1",
				Type: StageTypeLLM,
				LlmConfig: &LlmStageConfig{
					Provider:     "claude",
					Model:        "claude-3-sonnet",
					SystemPrompt: "System prompt",
					UserPrompt:   "User prompt",
					Temperature:  &temp,
					MaxTokens:    &maxTokens,
				},
			},
		},
		CreatedAt: time.Now().UTC(),
		UpdatedAt: time.Now().UTC(),
	}

	peeler := ToPeelerFormat(pipeline)

	if peeler.ID != pipeline.ID {
		t.Errorf("Expected ID '%s', got '%s'", pipeline.ID, peeler.ID)
	}
	if peeler.Name != pipeline.Name {
		t.Errorf("Expected Name '%s', got '%s'", pipeline.Name, peeler.Name)
	}
	if len(peeler.Stages) != 1 {
		t.Errorf("Expected 1 stage, got %d", len(peeler.Stages))
	}

	stage := peeler.Stages[0]
	if stage.ID != "stage1" {
		t.Errorf("Expected stage ID 'stage1', got '%s'", stage.ID)
	}
	if stage.Type != "llm" {
		t.Errorf("Expected stage type 'llm', got '%s'", stage.Type)
	}
	if stage.Provider != "claude" {
		t.Errorf("Expected provider 'claude', got '%s'", stage.Provider)
	}
	if stage.Model != "claude-3-sonnet" {
		t.Errorf("Expected model 'claude-3-sonnet', got '%s'", stage.Model)
	}
}

func TestToPeelerFormat_SkipsClipsStages(t *testing.T) {
	pipeline := &PipelineDefinition{
		ID:   "test",
		Name: "Test",
		Stages: []Stage{
			{
				ID:   "llm-stage",
				Name: "LLM Stage",
				Type: StageTypeLLM,
				LlmConfig: &LlmStageConfig{
					Provider:   "claude",
					Model:      "claude-3-sonnet",
					UserPrompt: "Test",
				},
			},
			{
				ID:   "clips-stage",
				Name: "CLIPS Stage",
				Type: StageTypeClipsEval,
				ClipsConfig: &ClipsConfig{
					RulesSource:  RulesSourceInline,
					RulesContent: "(defrule test)",
				},
			},
		},
		CreatedAt: time.Now().UTC(),
		UpdatedAt: time.Now().UTC(),
	}

	peeler := ToPeelerFormat(pipeline)

	// Should only have the LLM stage
	if len(peeler.Stages) != 1 {
		t.Errorf("Expected 1 stage (CLIPS filtered out), got %d", len(peeler.Stages))
	}
	if peeler.Stages[0].ID != "llm-stage" {
		t.Errorf("Expected LLM stage, got '%s'", peeler.Stages[0].ID)
	}
}

func TestFromPeelerFormat(t *testing.T) {
	temp := 0.7
	peeler := &PeelerPipeline{
		ID:   "peeler-id",
		Name: "Peeler Pipeline",
		Stages: []PeelerStage{
			{
				ID:          "stage1",
				Name:        "Stage 1",
				Type:        "llm",
				Provider:    "openai",
				Model:       "gpt-4",
				UserPrompt:  "Prompt",
				Temperature: &temp,
			},
		},
	}

	pipeline := FromPeelerFormat(peeler)

	if pipeline.ID != peeler.ID {
		t.Errorf("Expected ID '%s', got '%s'", peeler.ID, pipeline.ID)
	}
	if pipeline.Name != peeler.Name {
		t.Errorf("Expected Name '%s', got '%s'", peeler.Name, pipeline.Name)
	}
	if len(pipeline.Stages) != 1 {
		t.Errorf("Expected 1 stage, got %d", len(pipeline.Stages))
	}

	stage := pipeline.Stages[0]
	if stage.Type != StageTypeLLM {
		t.Errorf("Expected LLM type, got '%s'", stage.Type)
	}
	if stage.LlmConfig == nil {
		t.Fatal("Expected LlmConfig, got nil")
	}
	if stage.LlmConfig.Provider != "openai" {
		t.Errorf("Expected provider 'openai', got '%s'", stage.LlmConfig.Provider)
	}
	if stage.LlmConfig.Model != "gpt-4" {
		t.Errorf("Expected model 'gpt-4', got '%s'", stage.LlmConfig.Model)
	}
}

func TestFromPeelerFormat_SkipsUnknownTypes(t *testing.T) {
	peeler := &PeelerPipeline{
		ID:   "test",
		Name: "Test",
		Stages: []PeelerStage{
			{
				ID:         "llm-stage",
				Name:       "LLM Stage",
				Type:       "llm",
				Provider:   "claude",
				Model:      "claude-3-sonnet",
				UserPrompt: "Test",
			},
			{
				ID:   "unknown-stage",
				Name: "Unknown Stage",
				Type: "custom_type",
			},
		},
	}

	pipeline := FromPeelerFormat(peeler)

	// Should only have the LLM stage
	if len(pipeline.Stages) != 1 {
		t.Errorf("Expected 1 stage (unknown filtered out), got %d", len(pipeline.Stages))
	}
	if pipeline.Stages[0].ID != "llm-stage" {
		t.Errorf("Expected LLM stage, got '%s'", pipeline.Stages[0].ID)
	}
}

func TestRoundTrip(t *testing.T) {
	temp := 0.7
	maxTokens := uint32(1000)

	original := &PipelineDefinition{
		ID:          "roundtrip-test",
		Name:        "Roundtrip Test",
		Description: "Test description",
		Version:     "1.0",
		Stages: []Stage{
			{
				ID:   "stage1",
				Name: "Stage 1",
				Type: StageTypeLLM,
				LlmConfig: &LlmStageConfig{
					Provider:     "claude",
					Model:        "claude-3-sonnet",
					SystemPrompt: "System",
					UserPrompt:   "User",
					Temperature:  &temp,
					MaxTokens:    &maxTokens,
				},
			},
		},
		CreatedAt: time.Now().UTC(),
		UpdatedAt: time.Now().UTC(),
	}

	// Convert to Peeler and back
	peeler := ToPeelerFormat(original)
	converted := FromPeelerFormat(peeler)

	if converted.ID != original.ID {
		t.Errorf("ID mismatch: expected '%s', got '%s'", original.ID, converted.ID)
	}
	if converted.Name != original.Name {
		t.Errorf("Name mismatch: expected '%s', got '%s'", original.Name, converted.Name)
	}
	if len(converted.Stages) != len(original.Stages) {
		t.Errorf("Stage count mismatch: expected %d, got %d", len(original.Stages), len(converted.Stages))
	}

	origStage := original.Stages[0]
	convStage := converted.Stages[0]
	if origStage.ID != convStage.ID {
		t.Errorf("Stage ID mismatch: expected '%s', got '%s'", origStage.ID, convStage.ID)
	}
	if origStage.Type != convStage.Type {
		t.Errorf("Stage type mismatch: expected '%s', got '%s'", origStage.Type, convStage.Type)
	}
}
