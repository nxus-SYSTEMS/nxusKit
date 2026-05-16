//! Pipeline DAG validation.
//!
//! This module provides validation for pipeline definitions, ensuring
//! stages form a valid directed acyclic graph (DAG) and that all
//! configuration requirements are met.

use petgraph::algo::toposort;
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::HashMap;

use super::{ClipsConfig, PipelineDefinition, RulesSource, Stage, StageType};

/// Errors that can occur during pipeline validation.
#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    /// Pipeline contains a cycle in stage dependencies.
    #[error("Pipeline contains a cycle involving stages: {stages:?}")]
    CycleDetected { stages: Vec<String> },

    /// A stage references a non-existent upstream stage.
    #[error("Stage '{stage}' references unknown upstream stage '{upstream}'")]
    UnknownUpstreamStage { stage: String, upstream: String },

    /// A stage is missing required configuration.
    #[error("Stage '{stage}' of type '{stage_type}' requires {required} configuration")]
    MissingConfig {
        stage: String,
        stage_type: StageType,
        required: String,
    },

    /// CLIPS config validation error.
    #[error("Stage '{stage}' CLIPS config error: {message}")]
    ClipsConfigError { stage: String, message: String },

    /// Duplicate stage ID found.
    #[error("Duplicate stage ID: '{id}'")]
    DuplicateStageId { id: String },
}

/// Result type for validation operations.
pub type ValidationResult<T> = Result<T, ValidationError>;

/// Build a directed graph from pipeline stages.
///
/// Returns the graph and a mapping from stage IDs to node indices.
pub fn build_dag(stages: &[Stage]) -> (DiGraph<String, ()>, HashMap<String, NodeIndex>) {
    let mut graph = DiGraph::new();
    let mut node_indices: HashMap<String, NodeIndex> = HashMap::new();

    // Add all stages as nodes
    for stage in stages {
        let idx = graph.add_node(stage.id.clone());
        node_indices.insert(stage.id.clone(), idx);
    }

    // Add edges for dependencies
    for stage in stages {
        if let Some(&stage_idx) = node_indices.get(&stage.id) {
            for upstream_id in &stage.upstream_stage_ids {
                if let Some(&upstream_idx) = node_indices.get(upstream_id) {
                    // Edge goes from upstream to downstream (dependency direction)
                    graph.add_edge(upstream_idx, stage_idx, ());
                }
            }
        }
    }

    (graph, node_indices)
}

/// Validate that the pipeline stages form a valid DAG (no cycles).
///
/// Uses topological sort to detect cycles efficiently in O(V+E) time.
pub fn validate_dag(pipeline: &PipelineDefinition) -> ValidationResult<()> {
    let (graph, node_indices) = build_dag(&pipeline.stages);

    match toposort(&graph, None) {
        Ok(_) => Ok(()),
        Err(cycle) => {
            // Find the stage that's part of the cycle
            let cycle_node = cycle.node_id();
            let stage_id = graph[cycle_node].clone();

            // Collect all stages in potential cycle for better error reporting
            let cycle_stages: Vec<String> = node_indices
                .iter()
                .filter_map(|(id, &idx)| {
                    if idx == cycle_node {
                        Some(id.clone())
                    } else {
                        None
                    }
                })
                .collect();

            Err(ValidationError::CycleDetected {
                stages: if cycle_stages.is_empty() {
                    vec![stage_id]
                } else {
                    cycle_stages
                },
            })
        }
    }
}

/// Validate that all upstream stage references exist.
pub fn validate_references(pipeline: &PipelineDefinition) -> ValidationResult<()> {
    let stage_ids: std::collections::HashSet<_> =
        pipeline.stages.iter().map(|s| s.id.as_str()).collect();

    for stage in &pipeline.stages {
        for upstream_id in &stage.upstream_stage_ids {
            if !stage_ids.contains(upstream_id.as_str()) {
                return Err(ValidationError::UnknownUpstreamStage {
                    stage: stage.id.clone(),
                    upstream: upstream_id.clone(),
                });
            }
        }
    }

    Ok(())
}

/// Validate that all stage IDs are unique.
pub fn validate_unique_ids(pipeline: &PipelineDefinition) -> ValidationResult<()> {
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();

    for stage in &pipeline.stages {
        if !seen.insert(&stage.id) {
            return Err(ValidationError::DuplicateStageId {
                id: stage.id.clone(),
            });
        }
    }

    Ok(())
}

/// Validate stage configuration based on stage type.
///
/// Ensures that:
/// - LLM stages have llm_config
/// - CLIPS stages have clips_config
/// - Hybrid stages have both
pub fn validate_stage_config(stage: &Stage) -> ValidationResult<()> {
    match stage.stage_type {
        StageType::Llm => {
            if stage.llm_config.is_none() {
                return Err(ValidationError::MissingConfig {
                    stage: stage.id.clone(),
                    stage_type: stage.stage_type,
                    required: "llm_config".to_string(),
                });
            }
        }
        StageType::ClipsEval | StageType::ClipsGen => {
            if stage.clips_config.is_none() {
                return Err(ValidationError::MissingConfig {
                    stage: stage.id.clone(),
                    stage_type: stage.stage_type,
                    required: "clips_config".to_string(),
                });
            }
            if let Some(ref config) = stage.clips_config {
                validate_clips_config(&stage.id, config)?;
            }
        }
        StageType::Hybrid => {
            if stage.llm_config.is_none() {
                return Err(ValidationError::MissingConfig {
                    stage: stage.id.clone(),
                    stage_type: stage.stage_type,
                    required: "llm_config".to_string(),
                });
            }
            if stage.clips_config.is_none() {
                return Err(ValidationError::MissingConfig {
                    stage: stage.id.clone(),
                    stage_type: stage.stage_type,
                    required: "clips_config".to_string(),
                });
            }
            if let Some(ref config) = stage.clips_config {
                validate_clips_config(&stage.id, config)?;
            }
        }
    }

    Ok(())
}

/// Validate CLIPS configuration requirements.
fn validate_clips_config(stage_id: &str, config: &ClipsConfig) -> ValidationResult<()> {
    // rules_content required for inline and file sources
    match config.rules_source {
        RulesSource::Inline | RulesSource::File => {
            if config.rules_content.is_none() {
                return Err(ValidationError::ClipsConfigError {
                    stage: stage_id.to_string(),
                    message: "rules_content is required when rules_source is 'inline' or 'file'"
                        .to_string(),
                });
            }
        }
        RulesSource::Dynamic => {}
    }

    // save_path required when save_rules is true
    if config.save_rules && config.save_path.is_none() {
        return Err(ValidationError::ClipsConfigError {
            stage: stage_id.to_string(),
            message: "save_path is required when save_rules is true".to_string(),
        });
    }

    Ok(())
}

impl PipelineDefinition {
    /// Validate the entire pipeline.
    ///
    /// Performs all validation checks:
    /// - Unique stage IDs
    /// - Valid DAG (no cycles)
    /// - All upstream references exist
    /// - All stage configurations are valid
    pub fn validate(&self) -> ValidationResult<()> {
        validate_unique_ids(self)?;
        validate_dag(self)?;
        validate_references(self)?;

        for stage in &self.stages {
            validate_stage_config(stage)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_pipeline(stages: Vec<Stage>) -> PipelineDefinition {
        PipelineDefinition {
            id: "test-pipeline".to_string(),
            name: "Test Pipeline".to_string(),
            description: None,
            version: "1.0".to_string(),
            stages,
            created_at: "2026-01-31T00:00:00Z".to_string(),
            updated_at: "2026-01-31T00:00:00Z".to_string(),
            metadata: Default::default(),
        }
    }

    fn create_llm_stage(id: &str, upstream: Vec<&str>) -> Stage {
        use super::super::LlmStageConfig;

        Stage {
            id: id.to_string(),
            name: format!("Stage {}", id),
            stage_type: StageType::Llm,
            upstream_stage_ids: upstream.into_iter().map(String::from).collect(),
            llm_config: Some(LlmStageConfig {
                provider: "claude".to_string(),
                model: "claude-3-sonnet".to_string(),
                system_prompt: None,
                user_prompt: "Test prompt".to_string(),
                temperature: None,
                max_tokens: None,
                additional_params: Default::default(),
                providers: None,
            }),
            clips_config: None,
            timeout_ms: 30000,
            retry: None,
        }
    }

    #[test]
    fn test_valid_dag() {
        let pipeline = create_test_pipeline(vec![
            create_llm_stage("a", vec![]),
            create_llm_stage("b", vec!["a"]),
            create_llm_stage("c", vec!["a", "b"]),
        ]);

        assert!(validate_dag(&pipeline).is_ok());
    }

    #[test]
    fn test_cycle_detection() {
        let pipeline = create_test_pipeline(vec![
            create_llm_stage("a", vec!["c"]),
            create_llm_stage("b", vec!["a"]),
            create_llm_stage("c", vec!["b"]),
        ]);

        let result = validate_dag(&pipeline);
        assert!(result.is_err());
        assert!(matches!(result, Err(ValidationError::CycleDetected { .. })));
    }

    #[test]
    fn test_missing_reference_detection() {
        let pipeline = create_test_pipeline(vec![
            create_llm_stage("a", vec![]),
            create_llm_stage("b", vec!["nonexistent"]),
        ]);

        let result = validate_references(&pipeline);
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(ValidationError::UnknownUpstreamStage { .. })
        ));
    }

    #[test]
    fn test_duplicate_id_detection() {
        let pipeline = create_test_pipeline(vec![
            create_llm_stage("a", vec![]),
            create_llm_stage("a", vec![]),
        ]);

        let result = validate_unique_ids(&pipeline);
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(ValidationError::DuplicateStageId { .. })
        ));
    }

    #[test]
    fn test_stage_config_validation() {
        // LLM stage without config should fail
        let stage = Stage {
            id: "test".to_string(),
            name: "Test".to_string(),
            stage_type: StageType::Llm,
            upstream_stage_ids: vec![],
            llm_config: None,
            clips_config: None,
            timeout_ms: 30000,
            retry: None,
        };

        let result = validate_stage_config(&stage);
        assert!(result.is_err());
        assert!(matches!(result, Err(ValidationError::MissingConfig { .. })));
    }
}
