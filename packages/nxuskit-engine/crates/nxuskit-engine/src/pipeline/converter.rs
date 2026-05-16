//! Pipeline format conversion for Peeler compatibility.
//!
//! This module provides functions to convert between nxusKit and Peeler
//! pipeline formats, enabling interoperability between the two systems.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::{LlmStageConfig, PipelineDefinition, Stage, StageType};

/// Peeler-compatible pipeline format.
///
/// This is a simplified format that strips nxusKit-specific fields
/// for compatibility with the Peeler system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeelerPipeline {
    /// Unique pipeline identifier
    pub id: String,

    /// Human-readable pipeline name
    pub name: String,

    /// Optional description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Pipeline stages
    pub stages: Vec<PeelerStage>,

    /// Arbitrary metadata
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Peeler-compatible stage format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeelerStage {
    /// Stage identifier
    pub id: String,

    /// Stage name
    pub name: String,

    /// Stage type (only "llm" supported in Peeler)
    #[serde(rename = "type")]
    pub stage_type: String,

    /// Upstream dependencies
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub upstream_stage_ids: Vec<String>,

    /// Provider name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,

    /// Model name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// System prompt
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,

    /// User prompt
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_prompt: Option<String>,

    /// Temperature
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,

    /// Max tokens
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
}

/// Convert a nxusKit pipeline to Peeler format.
///
/// This strips nxusKit-specific fields (CLIPS config, retry config, etc.)
/// and flattens the LLM config into the stage level.
///
/// # Arguments
///
/// * `pipeline` - The nxusKit pipeline definition
///
/// # Returns
///
/// A Peeler-compatible pipeline, or None for stages that can't be converted.
/// CLIPS-only stages are filtered out.
pub fn to_peeler_format(pipeline: &PipelineDefinition) -> PeelerPipeline {
    let stages: Vec<PeelerStage> = pipeline
        .stages
        .iter()
        .filter_map(convert_stage_to_peeler)
        .collect();

    PeelerPipeline {
        id: pipeline.id.clone(),
        name: pipeline.name.clone(),
        description: pipeline.description.clone(),
        stages,
        metadata: pipeline.metadata.clone(),
    }
}

/// Convert a single stage to Peeler format.
fn convert_stage_to_peeler(stage: &Stage) -> Option<PeelerStage> {
    // Only convert stages that have LLM config
    // Pure CLIPS stages are not supported in Peeler
    match stage.stage_type {
        StageType::ClipsEval | StageType::ClipsGen => {
            // Skipping CLIPS-only stage in Peeler conversion
            return None;
        }
        StageType::Llm | StageType::Hybrid => {}
    }

    let llm_config = stage.llm_config.as_ref()?;

    Some(PeelerStage {
        id: stage.id.clone(),
        name: stage.name.clone(),
        stage_type: "llm".to_string(), // Peeler only supports "llm"
        upstream_stage_ids: stage.upstream_stage_ids.clone(),
        provider: Some(llm_config.provider.clone()),
        model: Some(llm_config.model.clone()),
        system_prompt: llm_config.system_prompt.clone(),
        user_prompt: Some(llm_config.user_prompt.clone()),
        temperature: llm_config.temperature,
        max_tokens: llm_config.max_tokens,
    })
}

/// Convert a Peeler pipeline to nxusKit format.
///
/// This adds nxusKit-specific structure while preserving the core
/// pipeline definition. Unknown stage types are skipped with a warning.
///
/// # Arguments
///
/// * `peeler` - The Peeler pipeline definition
pub fn from_peeler_format(peeler: &PeelerPipeline) -> PipelineDefinition {
    let stages: Vec<Stage> = peeler
        .stages
        .iter()
        .filter_map(convert_stage_from_peeler)
        .collect();

    // Generate ISO 8601 timestamp using std::time
    let now = {
        use std::time::{SystemTime, UNIX_EPOCH};
        let duration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        // Simple ISO 8601 format (approximation without chrono)
        format!("{}", duration.as_secs())
    };

    PipelineDefinition {
        id: peeler.id.clone(),
        name: peeler.name.clone(),
        description: peeler.description.clone(),
        version: "1.0".to_string(),
        stages,
        created_at: now.clone(),
        updated_at: now,
        metadata: peeler.metadata.clone(),
    }
}

/// Convert a single Peeler stage to nxusKit format.
fn convert_stage_from_peeler(stage: &PeelerStage) -> Option<Stage> {
    // Only convert "llm" stage type
    if stage.stage_type != "llm" {
        // Skipping unknown stage type in Peeler conversion
        return None;
    }

    // Build LLM config from flattened fields
    let llm_config = LlmStageConfig {
        provider: stage.provider.clone().unwrap_or_default(),
        model: stage.model.clone().unwrap_or_default(),
        system_prompt: stage.system_prompt.clone(),
        user_prompt: stage.user_prompt.clone().unwrap_or_default(),
        temperature: stage.temperature,
        max_tokens: stage.max_tokens,
        additional_params: HashMap::new(),
        providers: None,
    };

    Some(Stage {
        id: stage.id.clone(),
        name: stage.name.clone(),
        stage_type: StageType::Llm,
        upstream_stage_ids: stage.upstream_stage_ids.clone(),
        llm_config: Some(llm_config),
        clips_config: None,
        timeout_ms: 30000,
        retry: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_nxuskit_pipeline() -> PipelineDefinition {
        PipelineDefinition {
            id: "test-id".to_string(),
            name: "Test Pipeline".to_string(),
            description: Some("Test description".to_string()),
            version: "1.0".to_string(),
            stages: vec![Stage {
                id: "stage1".to_string(),
                name: "Stage 1".to_string(),
                stage_type: StageType::Llm,
                upstream_stage_ids: vec![],
                llm_config: Some(LlmStageConfig {
                    provider: "claude".to_string(),
                    model: "claude-3-sonnet".to_string(),
                    system_prompt: Some("System prompt".to_string()),
                    user_prompt: "User prompt".to_string(),
                    temperature: Some(0.7),
                    max_tokens: Some(1000),
                    additional_params: HashMap::new(),
                    providers: None,
                }),
                clips_config: None,
                timeout_ms: 30000,
                retry: None,
            }],
            created_at: "2026-01-31T00:00:00Z".to_string(),
            updated_at: "2026-01-31T00:00:00Z".to_string(),
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn test_to_peeler_format() {
        let nxuskit = create_test_nxuskit_pipeline();
        let peeler = to_peeler_format(&nxuskit);

        assert_eq!(peeler.id, nxuskit.id);
        assert_eq!(peeler.name, nxuskit.name);
        assert_eq!(peeler.stages.len(), 1);

        let stage = &peeler.stages[0];
        assert_eq!(stage.id, "stage1");
        assert_eq!(stage.stage_type, "llm");
        assert_eq!(stage.provider, Some("claude".to_string()));
        assert_eq!(stage.model, Some("claude-3-sonnet".to_string()));
    }

    #[test]
    fn test_from_peeler_format() {
        let peeler = PeelerPipeline {
            id: "peeler-id".to_string(),
            name: "Peeler Pipeline".to_string(),
            description: None,
            stages: vec![PeelerStage {
                id: "stage1".to_string(),
                name: "Stage 1".to_string(),
                stage_type: "llm".to_string(),
                upstream_stage_ids: vec![],
                provider: Some("openai".to_string()),
                model: Some("gpt-4".to_string()),
                system_prompt: None,
                user_prompt: Some("Prompt".to_string()),
                temperature: None,
                max_tokens: None,
            }],
            metadata: HashMap::new(),
        };

        let nxuskit = from_peeler_format(&peeler);

        assert_eq!(nxuskit.id, peeler.id);
        assert_eq!(nxuskit.name, peeler.name);
        assert_eq!(nxuskit.stages.len(), 1);

        let stage = &nxuskit.stages[0];
        assert_eq!(stage.stage_type, StageType::Llm);
        assert!(stage.llm_config.is_some());

        let llm = stage.llm_config.as_ref().unwrap();
        assert_eq!(llm.provider, "openai");
        assert_eq!(llm.model, "gpt-4");
    }

    #[test]
    fn test_round_trip() {
        let original = create_test_nxuskit_pipeline();
        let peeler = to_peeler_format(&original);
        let converted = from_peeler_format(&peeler);

        assert_eq!(original.id, converted.id);
        assert_eq!(original.name, converted.name);
        assert_eq!(original.stages.len(), converted.stages.len());

        let orig_stage = &original.stages[0];
        let conv_stage = &converted.stages[0];
        assert_eq!(orig_stage.id, conv_stage.id);
        assert_eq!(orig_stage.stage_type, conv_stage.stage_type);
    }
}
