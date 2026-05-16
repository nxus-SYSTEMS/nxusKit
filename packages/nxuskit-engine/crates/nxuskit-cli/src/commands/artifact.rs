//! `nxuskit-cli artifact merge|summarize` — Artifact manipulation (FR-009).

use clap::{Args, Subcommand};
use serde::Serialize;

use crate::cli_error::CliError;
use crate::envelope::TraceFields;
use crate::output::{OutputFormat, OutputWriter};

#[derive(Debug, Subcommand)]
pub enum ArtifactAction {
    /// Merge multiple artifact JSON files
    Merge(ArtifactMergeArgs),
    /// Summarize an artifact
    Summarize(ArtifactSummarizeArgs),
}

#[derive(Debug, Args)]
pub struct ArtifactMergeArgs {
    /// Input files (multiple allowed)
    #[arg(short, long, num_args = 1..)]
    pub input: Vec<String>,

    /// Merge strategy for conflicting keys: "error" (default, fail on
    /// conflict), "first" (keep first value), or "last" (keep last value)
    #[arg(long, default_value = "error")]
    pub merge_strategy: String,

    #[arg(short, long, default_value = "json")]
    pub format: String,

    #[arg(short, long, default_value_t = false)]
    pub quiet: bool,

    #[arg(short, long)]
    pub output: Option<String>,
}

#[derive(Debug, Args)]
pub struct ArtifactSummarizeArgs {
    #[arg(short, long)]
    pub input: String,

    #[arg(short, long, default_value = "json")]
    pub format: String,

    #[arg(short, long, default_value_t = false)]
    pub quiet: bool,

    #[arg(short, long)]
    pub output: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ArtifactSummary {
    pub field_count: u32,
    pub top_level_keys: Vec<String>,
    pub estimated_tokens: u32,
}

pub async fn run_artifact_command(action: ArtifactAction) -> Result<(), CliError> {
    match action {
        ArtifactAction::Merge(args) => run_artifact_merge(args).await,
        ArtifactAction::Summarize(args) => run_artifact_summarize(args).await,
    }
}

async fn run_artifact_merge(args: ArtifactMergeArgs) -> Result<(), CliError> {
    let format = OutputFormat::parse(&args.format)?;

    if args.input.len() < 2 {
        return Err(CliError::ParseError {
            message: "artifact merge requires at least 2 input files".to_string(),
        });
    }

    // Read all inputs
    let mut objects: Vec<serde_json::Map<String, serde_json::Value>> = Vec::new();
    let mut raw_inputs = String::new();
    for path in &args.input {
        let raw = OutputWriter::read_input(path)?;
        raw_inputs.push_str(&raw);
        let obj: serde_json::Value =
            serde_json::from_str(&raw).map_err(|e| CliError::ParseError {
                message: format!("Invalid JSON in '{}': {e}", path),
            })?;
        if let serde_json::Value::Object(map) = obj {
            objects.push(map);
        } else {
            return Err(CliError::ParseError {
                message: format!("'{}' is not a JSON object", path),
            });
        }
    }

    let trace = TraceFields::new("artifact_merge", &raw_inputs, None, None);
    let writer = OutputWriter::new(format, args.quiet, args.output);

    // Validate merge strategy
    if !["error", "first", "last"].contains(&args.merge_strategy.as_str()) {
        return Err(CliError::ParseError {
            message: format!("Unknown merge strategy: {}", args.merge_strategy),
        });
    }

    // Recursive deep merge with conflict tracking
    let mut merged = serde_json::Map::new();
    let mut conflicts: Vec<String> = Vec::new();

    for obj in &objects {
        deep_merge_objects(
            &mut merged,
            obj,
            &args.merge_strategy,
            &mut conflicts,
            String::new(),
        );
    }

    if !conflicts.is_empty() {
        return Err(CliError::MergeConflict { paths: conflicts });
    }

    let result = serde_json::Value::Object(merged);
    writer.write_response(result, trace, None, None, None)
}

async fn run_artifact_summarize(args: ArtifactSummarizeArgs) -> Result<(), CliError> {
    let format = OutputFormat::parse(&args.format)?;
    let raw_input = OutputWriter::read_input(&args.input)?;
    let artifact: serde_json::Value =
        serde_json::from_str(&raw_input).map_err(|e| CliError::ParseError {
            message: format!("Invalid artifact JSON: {e}"),
        })?;

    let trace = TraceFields::new("artifact_summarize", &raw_input, None, None);
    let writer = OutputWriter::new(format, args.quiet, args.output);

    let top_level_keys: Vec<String> = artifact
        .as_object()
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();

    let field_count = count_fields(&artifact);
    let estimated_tokens = (raw_input.len() / 4) as u32; // rough estimate

    if format == OutputFormat::Text {
        let mut text = String::new();
        text.push_str(&format!("Fields: {}\n", field_count));
        text.push_str(&format!("Top-level keys: {}\n", top_level_keys.join(", ")));
        text.push_str(&format!("Estimated tokens: {}\n", estimated_tokens));

        if let Some(obj) = artifact.as_object() {
            for (key, value) in obj {
                let preview = match value {
                    serde_json::Value::String(s) => {
                        if s.len() > 80 {
                            format!("\"{}...\"", &s[..77])
                        } else {
                            format!("\"{}\"", s)
                        }
                    }
                    other => {
                        let s = other.to_string();
                        if s.len() > 80 {
                            format!("{}...", &s[..77])
                        } else {
                            s
                        }
                    }
                };
                text.push_str(&format!("  {}: {}\n", key, preview));
            }
        }

        // For text format, write directly
        print!("{text}");
        return Ok(());
    }

    let result = ArtifactSummary {
        field_count,
        top_level_keys,
        estimated_tokens,
    };

    writer.write_response(result, trace, None, None, None)
}

/// Recursively merge `source` into `target`, tracking conflicts at dot-notation paths.
fn deep_merge_objects(
    target: &mut serde_json::Map<String, serde_json::Value>,
    source: &serde_json::Map<String, serde_json::Value>,
    strategy: &str,
    conflicts: &mut Vec<String>,
    prefix: String,
) {
    for (key, value) in source {
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{}.{}", prefix, key)
        };

        if let Some(existing) = target.get_mut(key) {
            // Both sides exist — check for recursive merge opportunity
            if let (serde_json::Value::Object(_), serde_json::Value::Object(source_map)) =
                (existing.clone(), value)
            {
                // Both are objects — recurse
                let existing_obj = existing.as_object_mut().unwrap();
                deep_merge_objects(existing_obj, source_map, strategy, conflicts, path);
            } else if existing != value {
                // Leaf conflict
                match strategy {
                    "error" => conflicts.push(path),
                    "first" => {} // keep existing
                    "last" => {
                        *existing = value.clone();
                    }
                    _ => {} // validated upstream
                }
            }
        } else {
            target.insert(key.clone(), value.clone());
        }
    }
}

fn count_fields(value: &serde_json::Value) -> u32 {
    match value {
        serde_json::Value::Object(map) => {
            map.len() as u32 + map.values().map(count_fields).sum::<u32>()
        }
        serde_json::Value::Array(arr) => arr.iter().map(count_fields).sum(),
        _ => 0,
    }
}
