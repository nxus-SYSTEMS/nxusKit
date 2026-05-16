//! Pipeline loading from JSON and YAML files.
//!
//! This module provides functions to load pipeline definitions from
//! JSON or YAML files with automatic format detection.

use std::fs;
use std::path::Path;

use super::PipelineDefinition;

/// Errors that can occur during pipeline loading.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// File I/O error.
    #[error("Failed to read file: {0}")]
    IoError(#[from] std::io::Error),

    /// JSON parsing error.
    #[error("Failed to parse JSON: {0}")]
    JsonError(#[from] serde_json::Error),

    /// YAML parsing error.
    #[error("Failed to parse YAML: {0}")]
    YamlError(#[from] serde_yaml::Error),

    /// Unsupported file format.
    #[error("Unsupported file format: {extension}. Expected .json or .yaml/.yml")]
    UnsupportedFormat { extension: String },

    /// License required for CLIPS features.
    #[error("Pro license required for CLIPS features")]
    LicenseRequired,
}

/// Result type for load operations.
pub type LoadResult<T> = Result<T, LoadError>;

/// Detect file format from extension.
fn detect_format(path: &Path) -> LoadResult<FileFormat> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match extension.as_str() {
        "json" => Ok(FileFormat::Json),
        "yaml" | "yml" => Ok(FileFormat::Yaml),
        _ => Err(LoadError::UnsupportedFormat { extension }),
    }
}

#[derive(Debug, Clone, Copy)]
enum FileFormat {
    Json,
    Yaml,
}

/// Load a pipeline definition from a file.
///
/// Automatically detects JSON or YAML format based on file extension.
///
/// # Arguments
///
/// * `path` - Path to the pipeline definition file (.json, .yaml, or .yml)
///
/// # Example
///
/// ```no_run
/// use nxuskit_engine::pipeline::load_pipeline;
///
/// let pipeline = load_pipeline("my-pipeline.json").unwrap();
/// println!("Loaded pipeline: {}", pipeline.name);
/// ```
pub fn load_pipeline<P: AsRef<Path>>(path: P) -> LoadResult<PipelineDefinition> {
    let path = path.as_ref();
    // Note: Consider adding tracing when available in workspace dependencies

    let format = detect_format(path)?;
    let content = fs::read_to_string(path)?;

    let pipeline = load_pipeline_from_str(&content, format)?;

    Ok(pipeline)
}

/// Load a pipeline definition from a string.
///
/// # Arguments
///
/// * `content` - The pipeline definition content
/// * `format` - The format of the content (JSON or YAML)
fn load_pipeline_from_str(content: &str, format: FileFormat) -> LoadResult<PipelineDefinition> {
    let pipeline: PipelineDefinition = match format {
        FileFormat::Json => serde_json::from_str(content)?,
        FileFormat::Yaml => serde_yaml::from_str(content)?,
    };

    Ok(pipeline)
}

/// Load a pipeline definition from a JSON string.
///
/// # Example
///
/// ```
/// use nxuskit_engine::pipeline::load_pipeline_from_json;
///
/// let json = r#"{
///     "id": "test",
///     "name": "Test Pipeline",
///     "stages": [],
///     "created_at": "2026-01-31T00:00:00Z",
///     "updated_at": "2026-01-31T00:00:00Z"
/// }"#;
///
/// let pipeline = load_pipeline_from_json(json).unwrap();
/// assert_eq!(pipeline.name, "Test Pipeline");
/// ```
pub fn load_pipeline_from_json(content: &str) -> LoadResult<PipelineDefinition> {
    load_pipeline_from_str(content, FileFormat::Json)
}

/// Load a pipeline definition from a YAML string.
///
/// # Example
///
/// ```
/// use nxuskit_engine::pipeline::load_pipeline_from_yaml;
///
/// let yaml = r#"
/// id: test
/// name: Test Pipeline
/// stages: []
/// created_at: "2026-01-31T00:00:00Z"
/// updated_at: "2026-01-31T00:00:00Z"
/// "#;
///
/// let pipeline = load_pipeline_from_yaml(yaml).unwrap();
/// assert_eq!(pipeline.name, "Test Pipeline");
/// ```
pub fn load_pipeline_from_yaml(content: &str) -> LoadResult<PipelineDefinition> {
    load_pipeline_from_str(content, FileFormat::Yaml)
}

/// Save a pipeline definition to a file.
///
/// Automatically detects JSON or YAML format based on file extension.
///
/// # Arguments
///
/// * `pipeline` - The pipeline definition to save
/// * `path` - Path to save the file (.json, .yaml, or .yml)
pub fn save_pipeline<P: AsRef<Path>>(pipeline: &PipelineDefinition, path: P) -> LoadResult<()> {
    let path = path.as_ref();

    let format = detect_format(path)?;

    let content = match format {
        FileFormat::Json => serde_json::to_string_pretty(pipeline)?,
        FileFormat::Yaml => serde_yaml::to_string(pipeline)?,
    };

    fs::write(path, content)?;

    Ok(())
}

/// Serialize a pipeline definition to JSON.
pub fn pipeline_to_json(pipeline: &PipelineDefinition) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(pipeline)
}

/// Serialize a pipeline definition to YAML.
pub fn pipeline_to_yaml(pipeline: &PipelineDefinition) -> Result<String, serde_yaml::Error> {
    serde_yaml::to_string(pipeline)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_round_trip() {
        let json = r#"{
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
        }"#;

        let pipeline = load_pipeline_from_json(json).unwrap();
        assert_eq!(pipeline.id, "test-id");
        assert_eq!(pipeline.name, "Test Pipeline");
        assert_eq!(pipeline.stages.len(), 1);

        // Round-trip
        let json_out = pipeline_to_json(&pipeline).unwrap();
        let pipeline2 = load_pipeline_from_json(&json_out).unwrap();
        assert_eq!(pipeline.id, pipeline2.id);
        assert_eq!(pipeline.name, pipeline2.name);
    }

    #[test]
    fn test_yaml_round_trip() {
        let yaml = r#"
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
created_at: "2026-01-31T00:00:00Z"
updated_at: "2026-01-31T00:00:00Z"
"#;

        let pipeline = load_pipeline_from_yaml(yaml).unwrap();
        assert_eq!(pipeline.id, "test-id");
        assert_eq!(pipeline.name, "Test Pipeline");
        assert_eq!(pipeline.stages.len(), 1);

        // Round-trip
        let yaml_out = pipeline_to_yaml(&pipeline).unwrap();
        let pipeline2 = load_pipeline_from_yaml(&yaml_out).unwrap();
        assert_eq!(pipeline.id, pipeline2.id);
        assert_eq!(pipeline.name, pipeline2.name);
    }

    #[test]
    fn test_format_detection() {
        assert!(matches!(
            detect_format(Path::new("test.json")),
            Ok(FileFormat::Json)
        ));
        assert!(matches!(
            detect_format(Path::new("test.yaml")),
            Ok(FileFormat::Yaml)
        ));
        assert!(matches!(
            detect_format(Path::new("test.yml")),
            Ok(FileFormat::Yaml)
        ));
        assert!(matches!(
            detect_format(Path::new("test.txt")),
            Err(LoadError::UnsupportedFormat { .. })
        ));
    }
}
