package pipeline

import (
	"testing"
	"time"
)

func TestLoadPipelineFromJSON(t *testing.T) {
	jsonContent := `{
		"id": "test-id",
		"name": "Test Pipeline",
		"stages": [
			{
				"id": "stage1",
				"name": "Stage 1",
				"type": "llm",
				"llm_config": {
					"provider": "claude",
					"model": "claude-3-sonnet",
					"user_prompt": "Test"
				}
			}
		],
		"created_at": "2026-01-31T00:00:00Z",
		"updated_at": "2026-01-31T00:00:00Z"
	}`

	pipeline, err := LoadPipelineFromJSON(jsonContent)
	if err != nil {
		t.Fatalf("Failed to load JSON: %v", err)
	}

	if pipeline.ID != "test-id" {
		t.Errorf("Expected ID 'test-id', got '%s'", pipeline.ID)
	}
	if pipeline.Name != "Test Pipeline" {
		t.Errorf("Expected Name 'Test Pipeline', got '%s'", pipeline.Name)
	}
	if len(pipeline.Stages) != 1 {
		t.Errorf("Expected 1 stage, got %d", len(pipeline.Stages))
	}
	if pipeline.Stages[0].ID != "stage1" {
		t.Errorf("Expected stage ID 'stage1', got '%s'", pipeline.Stages[0].ID)
	}
	if pipeline.Stages[0].Type != StageTypeLLM {
		t.Errorf("Expected stage type LLM, got '%s'", pipeline.Stages[0].Type)
	}
}

func TestLoadPipelineFromYAML(t *testing.T) {
	yamlContent := `
id: test-id
name: Test Pipeline
stages:
  - id: stage1
    name: Stage 1
    type: llm
    llm_config:
      provider: claude
      model: claude-3-sonnet
      user_prompt: Test
created_at: 2026-01-31T00:00:00Z
updated_at: 2026-01-31T00:00:00Z
`

	pipeline, err := LoadPipelineFromYAML(yamlContent)
	if err != nil {
		t.Fatalf("Failed to load YAML: %v", err)
	}

	if pipeline.ID != "test-id" {
		t.Errorf("Expected ID 'test-id', got '%s'", pipeline.ID)
	}
	if pipeline.Name != "Test Pipeline" {
		t.Errorf("Expected Name 'Test Pipeline', got '%s'", pipeline.Name)
	}
	if len(pipeline.Stages) != 1 {
		t.Errorf("Expected 1 stage, got %d", len(pipeline.Stages))
	}
}

func TestJSONRoundTrip(t *testing.T) {
	original := &PipelineDefinition{
		ID:      "roundtrip-test",
		Name:    "Roundtrip Test",
		Version: "1.0",
		Stages: []Stage{
			{
				ID:   "stage1",
				Name: "Stage 1",
				Type: StageTypeLLM,
				LlmConfig: &LlmStageConfig{
					Provider:   "claude",
					Model:      "claude-3-sonnet",
					UserPrompt: "Test prompt",
				},
			},
		},
		CreatedAt: time.Now().UTC(),
		UpdatedAt: time.Now().UTC(),
	}

	// Convert to JSON
	jsonStr, err := PipelineToJSON(original)
	if err != nil {
		t.Fatalf("Failed to convert to JSON: %v", err)
	}

	// Parse back
	parsed, err := LoadPipelineFromJSON(jsonStr)
	if err != nil {
		t.Fatalf("Failed to parse JSON: %v", err)
	}

	// Verify
	if parsed.ID != original.ID {
		t.Errorf("ID mismatch: expected '%s', got '%s'", original.ID, parsed.ID)
	}
	if parsed.Name != original.Name {
		t.Errorf("Name mismatch: expected '%s', got '%s'", original.Name, parsed.Name)
	}
	if len(parsed.Stages) != len(original.Stages) {
		t.Errorf("Stage count mismatch: expected %d, got %d", len(original.Stages), len(parsed.Stages))
	}
}

func TestYAMLRoundTrip(t *testing.T) {
	original := &PipelineDefinition{
		ID:      "roundtrip-test",
		Name:    "Roundtrip Test",
		Version: "1.0",
		Stages: []Stage{
			{
				ID:   "stage1",
				Name: "Stage 1",
				Type: StageTypeLLM,
				LlmConfig: &LlmStageConfig{
					Provider:   "claude",
					Model:      "claude-3-sonnet",
					UserPrompt: "Test prompt",
				},
			},
		},
		CreatedAt: time.Now().UTC(),
		UpdatedAt: time.Now().UTC(),
	}

	// Convert to YAML
	yamlStr, err := PipelineToYAML(original)
	if err != nil {
		t.Fatalf("Failed to convert to YAML: %v", err)
	}

	// Parse back
	parsed, err := LoadPipelineFromYAML(yamlStr)
	if err != nil {
		t.Fatalf("Failed to parse YAML: %v", err)
	}

	// Verify
	if parsed.ID != original.ID {
		t.Errorf("ID mismatch: expected '%s', got '%s'", original.ID, parsed.ID)
	}
	if parsed.Name != original.Name {
		t.Errorf("Name mismatch: expected '%s', got '%s'", original.Name, parsed.Name)
	}
}

func TestFormatDetection(t *testing.T) {
	tests := []struct {
		path        string
		expected    fileFormat
		shouldError bool
	}{
		{"test.json", formatJSON, false},
		{"test.yaml", formatYAML, false},
		{"test.yml", formatYAML, false},
		{"test.JSON", formatJSON, false},
		{"test.YAML", formatYAML, false},
		{"test.txt", 0, true},
		{"test", 0, true},
	}

	for _, tt := range tests {
		t.Run(tt.path, func(t *testing.T) {
			format, err := detectFormat(tt.path)
			if tt.shouldError {
				if err == nil {
					t.Errorf("Expected error for path '%s', got nil", tt.path)
				}
			} else {
				if err != nil {
					t.Errorf("Unexpected error for path '%s': %v", tt.path, err)
				}
				if format != tt.expected {
					t.Errorf("Expected format %v, got %v", tt.expected, format)
				}
			}
		})
	}
}

func TestClipsStagesParsing(t *testing.T) {
	jsonContent := `{
		"id": "clips-test",
		"name": "CLIPS Test Pipeline",
		"stages": [
			{
				"id": "classify",
				"name": "LLM Classification",
				"type": "llm",
				"llm_config": {
					"provider": "claude",
					"model": "claude-3-sonnet",
					"user_prompt": "Classify: {{input}}"
				}
			},
			{
				"id": "validate",
				"name": "CLIPS Validation",
				"type": "clips_eval",
				"upstream_stage_ids": ["classify"],
				"clips_config": {
					"rules_source": "inline",
					"rules_content": "(defrule check ...)",
					"validation_severity": "error"
				}
			}
		],
		"created_at": "2026-01-31T00:00:00Z",
		"updated_at": "2026-01-31T00:00:00Z"
	}`

	pipeline, err := LoadPipelineFromJSON(jsonContent)
	if err != nil {
		t.Fatalf("Failed to load JSON: %v", err)
	}

	if len(pipeline.Stages) != 2 {
		t.Errorf("Expected 2 stages, got %d", len(pipeline.Stages))
	}

	clipsStage := pipeline.Stages[1]
	if clipsStage.Type != StageTypeClipsEval {
		t.Errorf("Expected clips_eval type, got '%s'", clipsStage.Type)
	}
	if clipsStage.ClipsConfig == nil {
		t.Fatal("Expected ClipsConfig, got nil")
	}
	if clipsStage.ClipsConfig.RulesSource != RulesSourceInline {
		t.Errorf("Expected inline rules source, got '%s'", clipsStage.ClipsConfig.RulesSource)
	}
}

func TestHybridStageParsing(t *testing.T) {
	jsonContent := `{
		"id": "hybrid-test",
		"name": "Hybrid Test",
		"stages": [
			{
				"id": "hybrid1",
				"name": "Hybrid Stage",
				"type": "hybrid",
				"llm_config": {
					"provider": "claude",
					"model": "claude-3-sonnet",
					"user_prompt": "Test"
				},
				"clips_config": {
					"rules_source": "file",
					"rules_content": "rules.clp"
				}
			}
		],
		"created_at": "2026-01-31T00:00:00Z",
		"updated_at": "2026-01-31T00:00:00Z"
	}`

	pipeline, err := LoadPipelineFromJSON(jsonContent)
	if err != nil {
		t.Fatalf("Failed to load JSON: %v", err)
	}

	stage := pipeline.Stages[0]
	if stage.Type != StageTypeHybrid {
		t.Errorf("Expected hybrid type, got '%s'", stage.Type)
	}
	if stage.LlmConfig == nil {
		t.Error("Expected LlmConfig, got nil")
	}
	if stage.ClipsConfig == nil {
		t.Error("Expected ClipsConfig, got nil")
	}
}
