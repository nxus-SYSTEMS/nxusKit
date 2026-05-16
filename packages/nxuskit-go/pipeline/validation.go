// Package pipeline provides validation for pipeline definitions.
package pipeline

import (
	"errors"
	"fmt"
)

// ValidationError represents errors that can occur during pipeline validation.
type ValidationError struct {
	Code    string
	Message string
	Stage   string
	Details map[string]string
}

func (e *ValidationError) Error() string {
	if e.Stage != "" {
		return fmt.Sprintf("validation error in stage '%s': %s", e.Stage, e.Message)
	}
	return fmt.Sprintf("validation error: %s", e.Message)
}

var (
	// ErrCycleDetected indicates the pipeline contains a cycle.
	ErrCycleDetected = errors.New("pipeline contains a cycle")
	// ErrUnknownUpstreamStage indicates a reference to a non-existent stage.
	ErrUnknownUpstreamStage = errors.New("unknown upstream stage reference")
	// ErrMissingConfig indicates a stage is missing required configuration.
	ErrMissingConfig = errors.New("missing required configuration")
	// ErrDuplicateStageID indicates duplicate stage IDs.
	ErrDuplicateStageID = errors.New("duplicate stage ID")
)

// ValidateDAG validates that pipeline stages form a valid DAG using Kahn's algorithm.
// This runs in O(V+E) time complexity.
func ValidateDAG(p *PipelineDefinition) error {
	if len(p.Stages) == 0 {
		return nil
	}

	// Build adjacency list and compute in-degrees
	inDegree := make(map[string]int)
	adjacency := make(map[string][]string)
	stageIDs := make(map[string]bool)

	for _, stage := range p.Stages {
		stageIDs[stage.ID] = true
		if _, exists := inDegree[stage.ID]; !exists {
			inDegree[stage.ID] = 0
		}
		if _, exists := adjacency[stage.ID]; !exists {
			adjacency[stage.ID] = []string{}
		}
	}

	// Build edges and compute in-degrees
	for _, stage := range p.Stages {
		for _, upstreamID := range stage.UpstreamStageIDs {
			if !stageIDs[upstreamID] {
				// Skip unknown references (handled by ValidateReferences)
				continue
			}
			adjacency[upstreamID] = append(adjacency[upstreamID], stage.ID)
			inDegree[stage.ID]++
		}
	}

	// Kahn's algorithm: start with nodes having no incoming edges
	queue := []string{}
	for id, degree := range inDegree {
		if degree == 0 {
			queue = append(queue, id)
		}
	}

	processedCount := 0
	for len(queue) > 0 {
		// Dequeue
		current := queue[0]
		queue = queue[1:]
		processedCount++

		// Process neighbors
		for _, neighbor := range adjacency[current] {
			inDegree[neighbor]--
			if inDegree[neighbor] == 0 {
				queue = append(queue, neighbor)
			}
		}
	}

	// If we couldn't process all nodes, there's a cycle
	if processedCount != len(p.Stages) {
		// Find stages that are part of the cycle
		cycleStages := []string{}
		for id, degree := range inDegree {
			if degree > 0 {
				cycleStages = append(cycleStages, id)
			}
		}

		return &ValidationError{
			Code:    "CYCLE_DETECTED",
			Message: fmt.Sprintf("pipeline contains a cycle involving stages: %v", cycleStages),
			Details: map[string]string{"cycle_stages": fmt.Sprintf("%v", cycleStages)},
		}
	}

	return nil
}

// ValidateReferences validates that all upstream stage references exist.
func ValidateReferences(p *PipelineDefinition) error {
	stageIDs := make(map[string]bool)
	for _, stage := range p.Stages {
		stageIDs[stage.ID] = true
	}

	for _, stage := range p.Stages {
		for _, upstreamID := range stage.UpstreamStageIDs {
			if !stageIDs[upstreamID] {
				return &ValidationError{
					Code:    "UNKNOWN_UPSTREAM",
					Message: fmt.Sprintf("references unknown upstream stage '%s'", upstreamID),
					Stage:   stage.ID,
					Details: map[string]string{
						"upstream": upstreamID,
					},
				}
			}
		}
	}

	return nil
}

// ValidateUniqueIDs validates that all stage IDs are unique.
func ValidateUniqueIDs(p *PipelineDefinition) error {
	seen := make(map[string]bool)
	for _, stage := range p.Stages {
		if seen[stage.ID] {
			return &ValidationError{
				Code:    "DUPLICATE_ID",
				Message: fmt.Sprintf("duplicate stage ID: '%s'", stage.ID),
				Stage:   stage.ID,
			}
		}
		seen[stage.ID] = true
	}
	return nil
}

// ValidateStageConfig validates stage configuration based on stage type.
func ValidateStageConfig(stage *Stage) error {
	switch stage.Type {
	case StageTypeLLM:
		if stage.LlmConfig == nil {
			return &ValidationError{
				Code:    "MISSING_CONFIG",
				Message: "llm_config is required for LLM stages",
				Stage:   stage.ID,
			}
		}

	case StageTypeClipsEval, StageTypeClipsGen:
		if stage.ClipsConfig == nil {
			return &ValidationError{
				Code:    "MISSING_CONFIG",
				Message: "clips_config is required for CLIPS stages",
				Stage:   stage.ID,
			}
		}
		if err := validateClipsConfig(stage.ID, stage.ClipsConfig); err != nil {
			return err
		}

	case StageTypeHybrid:
		if stage.LlmConfig == nil {
			return &ValidationError{
				Code:    "MISSING_CONFIG",
				Message: "llm_config is required for hybrid stages",
				Stage:   stage.ID,
			}
		}
		if stage.ClipsConfig == nil {
			return &ValidationError{
				Code:    "MISSING_CONFIG",
				Message: "clips_config is required for hybrid stages",
				Stage:   stage.ID,
			}
		}
		if err := validateClipsConfig(stage.ID, stage.ClipsConfig); err != nil {
			return err
		}
	}

	return nil
}

// validateClipsConfig validates CLIPS configuration requirements.
func validateClipsConfig(stageID string, config *ClipsConfig) error {
	// rules_content required for inline and file sources
	switch config.RulesSource {
	case RulesSourceInline, RulesSourceFile:
		if config.RulesContent == "" {
			return &ValidationError{
				Code:    "CLIPS_CONFIG_ERROR",
				Message: "rules_content is required when rules_source is 'inline' or 'file'",
				Stage:   stageID,
			}
		}
	}

	// save_path required when save_rules is true
	if config.SaveRules && config.SavePath == "" {
		return &ValidationError{
			Code:    "CLIPS_CONFIG_ERROR",
			Message: "save_path is required when save_rules is true",
			Stage:   stageID,
		}
	}

	return nil
}

// Validate performs all validation checks on the pipeline.
func (p *PipelineDefinition) Validate() error {
	if err := ValidateUniqueIDs(p); err != nil {
		return err
	}

	if err := ValidateDAG(p); err != nil {
		return err
	}

	if err := ValidateReferences(p); err != nil {
		return err
	}

	for i := range p.Stages {
		if err := ValidateStageConfig(&p.Stages[i]); err != nil {
			return err
		}
	}

	return nil
}
