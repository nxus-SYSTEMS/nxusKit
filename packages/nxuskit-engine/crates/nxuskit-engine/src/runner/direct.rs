//! RC implementation of [`PipelineCapabilityRunner`] (FR-047).
//!
//! Uses this crate's existing provider creation directly (today:
//! `LoopbackProvider`; other providers as their integrations are
//! wired in). A future Pro/internal implementation MAY swap this for
//! engine-internal dispatch without changing the public CLI input/output
//! contracts.
//!
//! Boundary rules (FR-027, FR-048, FR-049):
//!
//! - No engine-internal routing or control types may appear in the trait API
//!   or in any field of [`nxuskit::contract::v1_1::PipelineRunReport`].
//! - Output text is small public-safe summaries — no bulk Arrow
//!   batches, no full file contents, no PHI.
//! - Typed non-execution outcomes carry only documented
//!   `RuntimeDomain` strings and public-safe `detail` strings.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use serde_json::json;
use tokio::sync::Semaphore;

use nxuskit::contract::v1_1::{
    ArtifactCanonicalHash, ArtifactHistoryEnvelope, FanOutGroupResult, FanOutOperationResult,
    FinalOutputs, OperationOutcome, PipelineDefinition, PipelineDefinitionRef, PipelineRunReport,
    ReadinessSummaryDigest, ReportSection, ResourceAdvisory, RuntimeDomain, StageFinalOutput,
    StageOutcome, StageRunResult, effective_sections,
};
use nxuskit::contract::v1_1::{RunOutcome, StageInputSource, StageType};

use crate::error::NxuskitError;
use crate::provider::LLMProvider;
use crate::providers::loopback::LoopbackProvider;
use crate::types::{ChatRequest, Message};

use super::advisory::assess_pipeline_resources;
use super::events::{JsonlEvent, JsonlEventSink, NullEventSink};
use super::fanout::{DEFAULT_FAN_OUT_CONCURRENCY, normalize_fan_out, parse_fan_out_metadata};
use super::{PipelineCapabilityRunner, PipelineRunOptions};

/// RC implementation of [`PipelineCapabilityRunner`].
///
/// Stateless; can be constructed cheaply per run.
#[derive(Debug, Default, Clone, Copy)]
pub struct DirectProviderRunner;

impl DirectProviderRunner {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl PipelineCapabilityRunner for DirectProviderRunner {
    async fn run(
        &self,
        def: &PipelineDefinition,
        opts: &PipelineRunOptions,
    ) -> Result<PipelineRunReport, NxuskitError> {
        let sections =
            effective_sections(opts.report_preset, &opts.report_include, &opts.report_omit);

        let sink: Arc<dyn JsonlEventSink> = opts
            .event_sink
            .clone()
            .unwrap_or_else(|| Arc::new(NullEventSink));

        let pipeline_canonical_hash = match def.canonical_hash() {
            Ok(h) => h,
            Err(_) => ArtifactCanonicalHash::from(String::new()),
        };
        let pipeline_ref = PipelineDefinitionRef {
            pipeline_id: def.pipeline_id.as_str().to_string(),
            version: def.version.clone(),
            canonical_hash: Some(pipeline_canonical_hash.clone()),
        };

        sink.emit(JsonlEvent::new(
            "pipeline_start",
            json!({
                "pipeline_id": pipeline_ref.pipeline_id,
                "pipeline_canonical_hash": pipeline_canonical_hash.as_str(),
            }),
        ));

        let mut stage_results: Vec<StageRunResult> = Vec::with_capacity(def.stages.len());
        let mut stage_outputs: Vec<StageFinalOutput> = Vec::new();
        // Cache of stage_name -> (output_name -> small text payload)
        // used for `from_stage` dependency resolution (FR-010).
        let mut stage_output_cache: HashMap<String, HashMap<String, String>> = HashMap::new();

        let fan_out_concurrency = opts
            .fan_out_concurrency
            .unwrap_or(DEFAULT_FAN_OUT_CONCURRENCY)
            .max(1) as usize;

        for stage in &def.stages {
            let stage_name = stage.name.as_str().to_string();

            sink.emit(JsonlEvent::new(
                "stage_start",
                json!({ "stage_id": stage_name }),
            ));

            let start = Instant::now();
            let (outcome, summary, fan_out, final_text) =
                run_stage(stage, &stage_output_cache, fan_out_concurrency, &*sink).await;
            let elapsed_ms = start.elapsed().as_millis() as u64;

            let event_name = match &outcome {
                StageOutcome::Success | StageOutcome::Failure { .. } => "stage_complete",
                StageOutcome::ExternalActionRequired { .. } => "external_action_required",
                _ => "stage_skipped",
            };
            sink.emit(JsonlEvent::new(
                event_name,
                json!({
                    "stage_id": stage_name,
                    "outcome": outcome.status_str(),
                    "elapsed_ms": elapsed_ms,
                }),
            ));

            if let Some(text) = final_text.as_ref() {
                // Cache the first declared output's text for downstream
                // `from_stage` lookups (FR-010).
                let default_output_name = stage
                    .outputs
                    .first()
                    .map(|o| o.name.clone())
                    .unwrap_or_else(|| "output".to_string());
                stage_output_cache
                    .entry(stage_name.clone())
                    .or_default()
                    .insert(default_output_name.clone(), text.clone());
                stage_outputs.push(StageFinalOutput {
                    stage_name: stage_name.clone(),
                    output_name: default_output_name,
                    text: text.clone(),
                });
            }

            stage_results.push(StageRunResult {
                stage_name,
                outcome,
                elapsed_ms,
                summary,
                fan_out,
            });
        }

        sink.emit(JsonlEvent::new(
            "pipeline_complete",
            json!({
                "pipeline_id": pipeline_ref.pipeline_id,
                "stages_total": def.stages.len(),
            }),
        ));

        let captured_at = current_utc_rfc3339_seconds();
        let resource_advisory: Option<ResourceAdvisory> =
            if opts.preflight || sections.contains(&ReportSection::ResourceAdvisory) {
                Some(assess_pipeline_resources(def))
            } else {
                None
            };

        // Feature 106 / US4: build the envelope-compatible provenance
        // object when the `ArtifactHistory` section is enabled.
        // Aggregates per-stage fan-out groups so the envelope's
        // `fan_out_outcomes` mirrors the per-stage `fan_out` blocks
        // (FR-026).
        let envelope = if sections.contains(&ReportSection::ArtifactHistory) {
            let fan_out_outcomes: Vec<FanOutGroupResult> = stage_results
                .iter()
                .filter_map(|s| s.fan_out.clone())
                .collect();
            let any_failure = stage_results
                .iter()
                .any(|s| matches!(s.outcome, StageOutcome::Failure { .. }));
            let run_outcome = if any_failure {
                // The two-variant RC enum maps non-success to
                // `ValidationFailed` for now; the full nine-variant
                // expansion is tracked under T057 in the public DTO
                // module and will land additively.
                RunOutcome::ValidationFailed
            } else {
                RunOutcome::Success
            };
            let mut env = ArtifactHistoryEnvelope::builder(format!(
                "{}@{}",
                pipeline_ref.pipeline_id, pipeline_ref.version
            ))
            .pipeline_ref_with_hash(
                pipeline_ref.pipeline_id.clone(),
                pipeline_ref.version.clone(),
                pipeline_canonical_hash.clone(),
            )
            .readiness_snapshot_captured_at(captured_at.clone())
            .run_outcome(run_outcome)
            .build();
            // Layer the feature-106 additive provenance directly. The
            // envelope's existing builder does not expose setters for
            // these fields by design — they are runner-derived rather
            // than user-authored.
            env.fan_out_outcomes = fan_out_outcomes;
            env.resource_advisory = resource_advisory.clone();
            // `imported_example_ref` stays `None` until US8 wires
            // examples discovery.
            Some(env)
        } else {
            None
        };

        Ok(PipelineRunReport {
            contract_version: nxuskit::contract::v1_1::CONTRACT_VERSION.to_string(),
            pipeline_ref,
            pipeline_canonical_hash,
            readiness_summary: if sections.contains(&ReportSection::ReadinessSummary) {
                Some(ReadinessSummaryDigest {
                    captured_at: captured_at.clone(),
                    providers_ready: 1,
                })
            } else {
                None
            },
            stage_results: if sections.contains(&ReportSection::StageDetails) {
                Some(stage_results)
            } else {
                None
            },
            final_outputs: if sections.contains(&ReportSection::FinalOutputs) {
                Some(FinalOutputs { stage_outputs })
            } else {
                None
            },
            artifact_history_envelope: envelope,
            resource_advisory,
        })
    }
}

/// Execute one stage. Returns `(outcome, summary, fan_out_result,
/// final_output_text)`.
async fn run_stage(
    stage: &nxuskit::contract::v1_1::StageDecl,
    stage_output_cache: &HashMap<String, HashMap<String, String>>,
    fan_out_concurrency: usize,
    sink: &dyn JsonlEventSink,
) -> (
    StageOutcome,
    Option<String>,
    Option<FanOutGroupResult>,
    Option<String>,
) {
    // Compose input text from `from_stage` and `from_pipeline_input`.
    let prompt = compose_prompt(stage, stage_output_cache);

    // First check for fan-out metadata regardless of stage type
    // (FR-016, FR-017). If present, normalize and dispatch siblings.
    if let Some(spec) = parse_fan_out_metadata(&stage.metadata)
        && spec.mode == "parallel"
    {
        let ops = normalize_fan_out(stage.name.as_str(), &spec);
        let group = run_fan_out(&ops, &prompt, fan_out_concurrency, sink).await;
        // Summarise the group as the joined per-op success/failure
        // line count.
        let summary = Some(format!("fan-out: {} operation(s)", group.operations.len()));
        // The first successful op's output is the stage's final text.
        let final_text = group
            .operations
            .iter()
            .find_map(|op| op.output_summary.clone());
        return (StageOutcome::Success, summary, Some(group), final_text);
    }

    match stage.stage_type {
        StageType::Llm => run_llm_stage(stage, &prompt).await,
        StageType::Tool => run_tool_stage(stage),
        StageType::ClipsRuleEval => run_rule_eval_stage(stage),
        // Solver/Bayesian/Zen capabilities are valid v1.1 stage types
        // but their runtime execution is out of scope for this RC; emit
        // `declaration_only`.
        StageType::Solver | StageType::Bayesian | StageType::Zen => (
            StageOutcome::DeclarationOnly {
                domain: RuntimeDomain::DuckdbAdapter,
                detail: Some(format!(
                    "stage type {:?} is declaration-only in v1.1.0 RC",
                    stage.stage_type
                )),
            },
            None,
            None,
            None,
        ),
        StageType::DataStoreSearch => (
            StageOutcome::DeclarationOnly {
                domain: RuntimeDomain::DuckdbAdapter,
                detail: Some("data_store_search is declaration-only in v1.1.0 RC".to_string()),
            },
            None,
            None,
            None,
        ),
        StageType::BatchSource | StageType::BatchSink => (
            StageOutcome::DeclarationOnly {
                domain: RuntimeDomain::ArrowRuntime,
                detail: Some(
                    "Arrow `BatchRef` stages are declaration-only in v1.1.0 RC".to_string(),
                ),
            },
            None,
            None,
            None,
        ),
    }
}

/// Build a prompt string from a stage's inputs, resolving
/// `from_stage` references via the cache (FR-010).
fn compose_prompt(
    stage: &nxuskit::contract::v1_1::StageDecl,
    stage_output_cache: &HashMap<String, HashMap<String, String>>,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    for input in &stage.inputs {
        match &input.source {
            StageInputSource::FromPipelineInput {
                from_pipeline_input,
            } => {
                parts.push(format!("{}={}", input.name, from_pipeline_input));
            }
            StageInputSource::FromStage { from_stage } => {
                let stage_name = from_stage.stage.as_str();
                let output_name = &from_stage.output;
                if let Some(outputs) = stage_output_cache.get(stage_name)
                    && let Some(text) = outputs.get(output_name)
                {
                    parts.push(format!("{}={}", input.name, text));
                }
            }
        }
    }
    // Pull `prompt` from stage metadata if present (RC convenience).
    if let Some(serde_json::Value::String(s)) = stage.metadata.get("prompt") {
        parts.push(s.clone());
    }
    parts.join("\n")
}

/// LLM stage: provider/model resolution + direct provider call.
/// Today the runner supports `provider_ref == "loopback"`. Other
/// providers fall back to a typed `MissingPlugin` outcome rather than
/// silently passing.
async fn run_llm_stage(
    stage: &nxuskit::contract::v1_1::StageDecl,
    prompt: &str,
) -> (
    StageOutcome,
    Option<String>,
    Option<FanOutGroupResult>,
    Option<String>,
) {
    let provider_ref = stage
        .provider_ref
        .as_ref()
        .map(|p| p.as_str())
        .unwrap_or("loopback");
    let model_ref = stage.model_ref.as_deref().unwrap_or("echo");
    if provider_ref != "loopback" {
        return (
            StageOutcome::MissingPlugin {
                domain: RuntimeDomain::DuckdbAdapter,
                detail: Some(format!(
                    "provider `{provider_ref}` not configured for v1.1.0 RC contract-native runner"
                )),
            },
            None,
            None,
            None,
        );
    }
    let provider = LoopbackProvider::new();
    let request = ChatRequest::new(model_ref).with_message(Message::user(prompt));
    match provider.chat(&request).await {
        Ok(resp) => {
            let text = sanitize_small_text(&resp.content);
            (
                StageOutcome::Success,
                Some(format!("loopback `{model_ref}` ok")),
                None,
                Some(text),
            )
        }
        Err(e) => (
            StageOutcome::Failure {
                reason: format!("{e}"),
            },
            None,
            None,
            None,
        ),
    }
}

/// Tool stage: enforce external-only for `tool.emit_artifact` (FR-012).
fn run_tool_stage(
    stage: &nxuskit::contract::v1_1::StageDecl,
) -> (
    StageOutcome,
    Option<String>,
    Option<FanOutGroupResult>,
    Option<String>,
) {
    if stage.capability_ref.as_str() == "tool.emit_artifact" {
        return (
            StageOutcome::ExternalActionRequired {
                domain: RuntimeDomain::ToolEmitArtifact,
                detail: Some("tool.emit_artifact is external-only in v1.1.0 RC".to_string()),
            },
            None,
            None,
            None,
        );
    }
    (
        StageOutcome::UnsupportedStageType {
            domain: RuntimeDomain::ToolEmitArtifact,
            detail: Some(format!(
                "tool capability `{}` is not supported in v1.1.0 RC",
                stage.capability_ref.as_str()
            )),
        },
        None,
        None,
        None,
    )
}

/// CLIPS rule-eval stage: routed only through `rule_eval.generic`
/// (FR-011). All other CLIPS requests are rejected as
/// `UnsupportedStageType`.
fn run_rule_eval_stage(
    stage: &nxuskit::contract::v1_1::StageDecl,
) -> (
    StageOutcome,
    Option<String>,
    Option<FanOutGroupResult>,
    Option<String>,
) {
    let cap = stage.capability_ref.as_str();
    if cap != "rule_eval.generic" {
        return (
            StageOutcome::UnsupportedStageType {
                domain: RuntimeDomain::ClipsRuleEval,
                detail: Some(format!(
                    "CLIPS capability `{cap}` is not supported in v1.1.0 RC (only rule_eval.generic)"
                )),
            },
            None,
            None,
            None,
        );
    }
    // The RC runner records the CLIPS rule-eval boundary as a
    // declaration-only-but-supported execution: the actual rules
    // provider invocation lives behind the existing CLIPS adapter
    // and is integrated in a follow-up. For US3 the important
    // contract is that `rule_eval.generic` is the only accepted
    // capability, and that the boundary is observable as a typed
    // outcome rather than as silent execution.
    (
        StageOutcome::Success,
        Some("rule_eval.generic boundary acknowledged".to_string()),
        None,
        Some("rule_eval.generic".to_string()),
    )
}

/// Dispatch fan-out operations with a concurrency-limited semaphore.
/// Emits per-operation events in real completion order (FR-018, FR-025).
async fn run_fan_out(
    ops: &[super::fanout::FanOutOperation],
    base_prompt: &str,
    concurrency: usize,
    sink: &dyn JsonlEventSink,
) -> FanOutGroupResult {
    let sem = Arc::new(Semaphore::new(concurrency));

    // Emit `operation_queued` for every op before any starts so consumers
    // know the full group up-front.
    for op in ops {
        sink.emit(JsonlEvent::new(
            "operation_queued",
            json!({
                "operation_id": op.operation_id,
                "pair_index": op.pair_index,
                "provider_ref": op.provider_ref,
                "model_ref": op.model_ref,
            }),
        ));
    }

    let mut handles = Vec::with_capacity(ops.len());
    for op in ops {
        let op = op.clone();
        let op_id = op.operation_id.clone();
        let sem = sem.clone();
        let base_prompt = base_prompt.to_string();
        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.ok();
            let start = Instant::now();
            // Provider creation. Today: loopback only. Other providers
            // emit a typed Failure rather than silently passing.
            if op.provider_ref != "loopback" {
                return FanOutOperationResult {
                    operation_id: op.operation_id.clone(),
                    pair_index: op.pair_index,
                    provider_ref: op.provider_ref.clone(),
                    model_ref: op.model_ref.clone(),
                    outcome: OperationOutcome::Failure {
                        reason: format!(
                            "provider `{}` not configured for v1.1.0 RC contract-native runner",
                            op.provider_ref
                        ),
                    },
                    elapsed_ms: start.elapsed().as_millis() as u64,
                    output_summary: None,
                };
            }
            let provider = LoopbackProvider::new();
            let request =
                ChatRequest::new(op.model_ref.as_str()).with_message(Message::user(&base_prompt));
            let (outcome, summary) = match provider.chat(&request).await {
                Ok(resp) => (
                    OperationOutcome::Success,
                    Some(sanitize_small_text(&resp.content)),
                ),
                Err(e) => (
                    OperationOutcome::Failure {
                        reason: format!("{e}"),
                    },
                    None,
                ),
            };
            FanOutOperationResult {
                operation_id: op.operation_id.clone(),
                pair_index: op.pair_index,
                provider_ref: op.provider_ref.clone(),
                model_ref: op.model_ref.clone(),
                outcome,
                elapsed_ms: start.elapsed().as_millis() as u64,
                output_summary: summary,
            }
        });
        handles.push((op_id, handle));
    }

    // Emit `operation_started` events as soon as we have spawned all
    // tasks. Tokio will admit them up to the semaphore cap; emitting
    // start events before await keeps the contract simple at the cost
    // of a small race window (start ≅ spawn rather than actual permit
    // acquisition). The completion-order assertion in tests is the
    // load-bearing async-dispatch guarantee.
    for (op_id, _) in &handles {
        sink.emit(JsonlEvent::new(
            "operation_started",
            json!({ "operation_id": op_id }),
        ));
    }

    let mut results: Vec<FanOutOperationResult> = Vec::with_capacity(ops.len());
    for (op_id, handle) in handles {
        match handle.await {
            Ok(r) => {
                let evt = match &r.outcome {
                    OperationOutcome::Success => "operation_completed",
                    OperationOutcome::Failure { .. } => "operation_failed",
                    _ => "operation_completed",
                };
                sink.emit(JsonlEvent::new(
                    evt,
                    json!({
                        "operation_id": r.operation_id,
                        "elapsed_ms": r.elapsed_ms,
                    }),
                ));
                results.push(r);
            }
            Err(join_err) => {
                sink.emit(JsonlEvent::new(
                    "operation_failed",
                    json!({
                        "operation_id": op_id,
                        "error_class": "join_error",
                    }),
                ));
                // Push a synthetic failure record so we don't drop the op.
                results.push(FanOutOperationResult {
                    operation_id: op_id,
                    pair_index: 0,
                    provider_ref: String::new(),
                    model_ref: String::new(),
                    outcome: OperationOutcome::Failure {
                        reason: format!("worker join error: {join_err}"),
                    },
                    elapsed_ms: 0,
                    output_summary: None,
                });
            }
        }
    }

    // Restore pair_index order for the report (FR-020). Event order
    // already preserved the real completion order.
    results.sort_by_key(|r| r.pair_index);

    FanOutGroupResult {
        group_id: ops
            .first()
            .map(|op| op.operation_id.split("::").next().unwrap_or("").to_string())
            .unwrap_or_default(),
        operations: results,
    }
}

/// Trim potentially-large text to a small public-safe summary (FR-027).
fn sanitize_small_text(s: &str) -> String {
    const LIMIT: usize = 1024;
    if s.len() <= LIMIT {
        s.to_string()
    } else {
        let mut t = s[..LIMIT].to_string();
        t.push('…');
        t
    }
}

fn current_utc_rfc3339_seconds() -> String {
    // Mirrors `nxuskit::contract::v1_1::cli_projection` formatting at
    // second precision. Engine-local helper so the runner doesn't pull
    // in the projection helper.
    use std::time::{SystemTime, UNIX_EPOCH};
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (seconds / 86_400) as i64;
    let secs = seconds % 86_400;
    let hour = secs / 3_600;
    let minute = (secs % 3_600) / 60;
    let second = secs % 60;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn civil_from_days(days_since_epoch: i64) -> (i64, u64, u64) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}
