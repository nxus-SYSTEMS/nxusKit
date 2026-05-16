//! `nxuskit-cli pipeline run` — Pipeline execution (FR-008).

use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};

use std::collections::HashMap;

use crate::cli_error::CliError;
use crate::envelope::TraceFields;
use crate::output::{JsonlEvent, OutputFormat, OutputWriter};

#[derive(Debug, Subcommand)]
pub enum PipelineRunAction {
    /// Execute a pipeline definition
    Run(PipelineRunArgs),
}

#[derive(Debug, Args)]
pub struct PipelineRunArgs {
    /// Input file path or `-` for stdin.
    ///
    /// YAML/JSON pipeline definition with stages, each having a name,
    /// type (llm, clips_eval, zen_eval, solver_solve, bn_infer), and config.
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
pub struct PipelineResult {
    pub stages: Vec<StageResult>,
    pub summary: PipelineSummary,
    pub final_output: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct StageResult {
    pub name: String,
    pub status: String,
    pub result: serde_json::Value,
    pub elapsed_ms: f64,
}

#[derive(Debug, Serialize)]
pub struct PipelineSummary {
    pub total_stages: u32,
    pub completed: u32,
    pub failed: u32,
    pub skipped: u32,
}

/// Pipeline definition parsed from YAML/JSON input.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct PipelineDefinition {
    pub name: String,
    pub stages: Vec<Stage>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct Stage {
    pub name: String,
    #[serde(rename = "type")]
    pub stage_type: String,
    #[serde(default)]
    pub config: serde_json::Value,
    pub output_key: Option<String>,
}

pub async fn run_pipeline_command(action: PipelineRunAction) -> Result<(), CliError> {
    match action {
        PipelineRunAction::Run(args) => run_pipeline(args).await,
    }
}

async fn run_pipeline(args: PipelineRunArgs) -> Result<(), CliError> {
    let format = OutputFormat::parse(&args.format)?;
    let raw_input = OutputWriter::read_input(&args.input)?;

    // Try YAML first, then JSON
    let pipeline: PipelineDefinition = serde_yaml_ng::from_str(&raw_input)
        .or_else(|_| {
            serde_json::from_str(&raw_input).map_err(|e| {
                // Convert serde_json error to serde_yaml_ng error type
                use serde::de::Error;
                serde_yaml_ng::Error::custom(e.to_string())
            })
        })
        .map_err(|e| CliError::ParseError {
            message: format!("Invalid pipeline definition: {e}"),
        })?;

    let trace = TraceFields::new("pipeline_run", &raw_input, None, None);
    let writer = OutputWriter::new(format, args.quiet, args.output);
    let is_jsonl = format == OutputFormat::Jsonl;

    let mut stage_results = Vec::new();
    let mut last_output = serde_json::Value::Null;
    let mut output_bindings: HashMap<String, serde_json::Value> = HashMap::new();
    let mut failed_stage: Option<(String, String)> = None;

    for (i, stage) in pipeline.stages.iter().enumerate() {
        let start = std::time::Instant::now();

        match execute_stage(stage, &last_output, &output_bindings).await {
            Ok(output) => {
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                last_output = output.clone();

                // Store output_key binding if specified
                if let Some(key) = &stage.output_key {
                    output_bindings.insert(key.clone(), output.clone());
                }

                let sr = StageResult {
                    name: stage.name.clone(),
                    status: "completed".to_string(),
                    result: output.clone(),
                    elapsed_ms: elapsed,
                };

                if is_jsonl {
                    let event = JsonlEvent {
                        event_type: "stage_complete".to_string(),
                        data: serde_json::json!({
                            "stage": stage.name,
                            "result": output,
                        }),
                        trace_id: Some(trace.trace_id.clone()),
                    };
                    writer.write_jsonl_event(&event)?;
                }

                stage_results.push(sr);
            }
            Err(e) => {
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;

                stage_results.push(StageResult {
                    name: stage.name.clone(),
                    status: "failed".to_string(),
                    result: serde_json::json!({
                        "code": e.code(),
                        "message": e.to_string(),
                    }),
                    elapsed_ms: elapsed,
                });

                // Record failure and mark remaining stages as skipped
                failed_stage = Some((stage.name.clone(), e.to_string()));

                for remaining in pipeline.stages.iter().skip(i + 1) {
                    stage_results.push(StageResult {
                        name: remaining.name.clone(),
                        status: "skipped".to_string(),
                        result: serde_json::json!(null),
                        elapsed_ms: 0.0,
                    });
                }

                break;
            }
        }
    }

    let completed = stage_results
        .iter()
        .filter(|s| s.status == "completed")
        .count() as u32;
    let failed_count = stage_results
        .iter()
        .filter(|s| s.status == "failed")
        .count() as u32;
    let skipped = stage_results
        .iter()
        .filter(|s| s.status == "skipped")
        .count() as u32;

    let result = PipelineResult {
        stages: stage_results,
        summary: PipelineSummary {
            total_stages: pipeline.stages.len() as u32,
            completed,
            failed: failed_count,
            skipped,
        },
        final_output: last_output,
    };

    // Always write partial results to stdout
    writer.write_response(result, trace, None, None, None)?;

    // If a stage failed, return the error (which writes to stderr and exits 1)
    if let Some((stage, detail)) = failed_stage {
        return Err(CliError::PipelineStageFailed {
            stage,
            detail: Some(detail),
        });
    }

    Ok(())
}

/// Interpolate `{{key}}` placeholders in a string with values from output bindings.
fn interpolate_bindings(s: &str, bindings: &HashMap<String, serde_json::Value>) -> String {
    let mut result = s.to_string();
    for (key, value) in bindings {
        let placeholder = format!("{{{{{}}}}}", key);
        let replacement = match value {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        result = result.replace(&placeholder, &replacement);
    }
    result
}

async fn execute_stage(
    stage: &Stage,
    previous_output: &serde_json::Value,
    output_bindings: &HashMap<String, serde_json::Value>,
) -> Result<serde_json::Value, CliError> {
    match stage.stage_type.as_str() {
        "llm" => {
            let raw_prompt = stage
                .config
                .get("prompt")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let prompt = interpolate_bindings(raw_prompt, output_bindings);

            // Enrich prompt with previous output context if available
            let enriched_prompt = if !previous_output.is_null() {
                format!(
                    "{}\n\nPrevious stage output: {}",
                    prompt,
                    serde_json::to_string(previous_output).unwrap_or_default()
                )
            } else {
                prompt
            };

            let provider = stage
                .config
                .get("provider")
                .and_then(|v| v.as_str())
                .unwrap_or("loopback");

            let provider_impl =
                crate::create_provider(provider).map_err(|e| CliError::ProviderError {
                    message: format!("{e}"),
                })?;

            let model = stage
                .config
                .get("model")
                .and_then(|v| v.as_str())
                .unwrap_or("default");

            let request = nxuskit_engine::prelude::ChatRequest::new(model)
                .with_message(nxuskit_engine::prelude::Message::user(&enriched_prompt));

            let response =
                provider_impl
                    .chat(&request)
                    .await
                    .map_err(|e| CliError::ProviderError {
                        message: format!("{e}"),
                    })?;

            Ok(serde_json::json!({
                "content": response.content,
                "model": response.model,
            }))
        }
        "clips_eval" => {
            // Real CLIPS evaluation
            let rules = stage
                .config
                .get("rules")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let env = clips_sys::ClipsEnvironment::new().map_err(|e| CliError::ProviderError {
                message: format!("Failed to create CLIPS environment: {e}"),
            })?;

            env.load_from_string(rules)
                .map_err(|e| CliError::ProviderError {
                    message: format!("Failed to load CLIPS rules: {e}"),
                })?;

            // Assert facts from config
            if let Some(facts) = stage.config.get("facts").and_then(|v| v.as_array()) {
                for fact in facts {
                    if let Some(fact_str) = fact.as_str() {
                        env.assert_string(fact_str)
                            .map_err(|e| CliError::ProviderError {
                                message: format!("Failed to assert fact: {e}"),
                            })?;
                    }
                }
            }

            let run_result = env.run(None).map_err(|e| CliError::ProviderError {
                message: format!("Failed to run CLIPS agenda: {e}"),
            })?;

            let matched_rules: Vec<serde_json::Value> = env
                .rules()
                .filter_map(|r| r.ok())
                .filter_map(|r| {
                    let name = r.name().ok()?;
                    Some(serde_json::json!({
                        "name": name,
                        "times_fired": r.times_fired(),
                    }))
                })
                .collect();

            let derived_facts: Vec<serde_json::Value> = env
                .facts()
                .filter_map(|f| f.ok())
                .filter_map(|f| {
                    let template = f.template_name().ok()?;
                    let slots = f
                        .slot_values()
                        .ok()
                        .map(|sv| {
                            let map: serde_json::Map<String, serde_json::Value> = sv
                                .into_iter()
                                .map(|(k, v)| (k, super::clips::clips_value_to_json(&v)))
                                .collect();
                            serde_json::Value::Object(map)
                        })
                        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                    Some(serde_json::json!({
                        "template": template,
                        "slots": slots,
                    }))
                })
                .collect();

            Ok(serde_json::json!({
                "matched_rules": matched_rules,
                "derived_facts": derived_facts,
                "fired_rules": run_result.rules_fired,
                "agenda_count": env.agenda_size(),
            }))
        }
        "zen_eval" => super::zen::run_zen_stage_eval(&stage.config).await,
        "solver_solve" => super::solver::run_solver_stage_solve(&stage.config),
        "bn_infer" => {
            // Parse BN input from stage config
            let bn_input: super::bn::BnInferInput = serde_json::from_value(stage.config.clone())
                .map_err(|e| CliError::ParseError {
                    message: format!("Invalid BN stage config: {e}"),
                })?;

            let posteriors = super::bn::run_inference(&bn_input)?;

            Ok(serde_json::json!({
                "posteriors": posteriors,
                "algorithm": bn_input.algorithm,
            }))
        }
        other => Err(CliError::ParseError {
            message: format!("Unknown stage type: {other}"),
        }),
    }
}
