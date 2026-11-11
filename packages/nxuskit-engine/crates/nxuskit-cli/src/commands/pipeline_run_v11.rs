//! v1.1 contract-native `pipeline run` driver (Feature 106 / US2).
//!
//! Invoked by `commands/pipeline.rs` when the input file declares
//! `contract_version == "1.1"`. Routes execution through the engine-
//! internal `PipelineCapabilityRunner` trait (FR-006, FR-008, FR-009,
//! FR-022). The legacy runner remains unchanged for non-v1.1 inputs
//! (FR-007).

use std::sync::Arc;

use nxuskit::contract::v1_1::{
    PipelineDefinition, ReportPreset, ReportSection, load_pipeline_definition_json,
};
use nxuskit_engine::runner::{
    DirectProviderRunner, JsonlEventSink, PipelineCapabilityRunner, PipelineRunOptions,
    VecEventSink,
};

use crate::cli_error::CliError;

/// Detect whether `raw` is a v1.1 `PipelineDefinition` by peeking at
/// the top-level `contract_version` field. Returns `true` when the
/// field equals `"1.1"`.
pub fn detect_v1_1(raw: &str) -> bool {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) {
        return value.get("contract_version").and_then(|v| v.as_str()) == Some("1.1");
    }
    // Also try YAML, since the CLI accepts both.
    if let Ok(value) = serde_yaml_ng::from_str::<serde_json::Value>(raw) {
        return value.get("contract_version").and_then(|v| v.as_str()) == Some("1.1");
    }
    false
}

/// Execute a v1.1 pipeline. `raw_input` is the input file contents.
///
/// `format` is `"json"` (default) or `"jsonl"`. `report_preset_raw`,
/// `report_include`, `report_omit` are the CLI strings; this function
/// parses them through the public-DTO vocabulary.
pub async fn run_v1_1(
    raw_input: &str,
    format: &str,
    report_preset_raw: Option<&str>,
    report_include: &[String],
    report_omit: &[String],
    preflight: bool,
) -> Result<(), CliError> {
    // Validate through the same loader/validator used by
    // `pipeline validate --json` (FR-008).
    let def: PipelineDefinition =
        load_pipeline_definition_json(raw_input).map_err(|e| CliError::ParseError {
            message: format!("v1.1 PipelineDefinition load error: {e}"),
        })?;

    let report_preset = report_preset_raw
        .map(ReportPreset::parse)
        .unwrap_or_default();

    let report_include: Vec<ReportSection> = report_include
        .iter()
        .flat_map(|s| s.split(','))
        .filter_map(|s| ReportSection::parse(s.trim()))
        .collect();
    let report_omit: Vec<ReportSection> = report_omit
        .iter()
        .flat_map(|s| s.split(','))
        .filter_map(|s| ReportSection::parse(s.trim()))
        .collect();

    let event_sink_capture: Option<Arc<VecEventSink>> = if format == "jsonl" {
        Some(Arc::new(VecEventSink::new()))
    } else {
        None
    };
    let event_sink: Option<Arc<dyn JsonlEventSink>> = event_sink_capture
        .clone()
        .map(|s| s as Arc<dyn JsonlEventSink>);

    let opts = PipelineRunOptions {
        report_preset,
        report_include,
        report_omit,
        fan_out_concurrency: None,
        event_sink,
        preflight,
    };

    let runner = DirectProviderRunner::new();
    let report = runner
        .run(&def, &opts)
        .await
        .map_err(|e| CliError::ProviderError {
            message: format!("{e}"),
        })?;

    match format {
        "jsonl" => {
            if let Some(sink) = event_sink_capture {
                // Emit captured events as JSONL on stdout.
                for evt in sink.events() {
                    println!("{}", evt.render());
                }
            }
            // Final synthesis: also emit the full report as a single
            // line so consumers that only watch JSONL still receive
            // structured final results.
            let line = serde_json::to_string(&report).map_err(|e| CliError::ProviderError {
                message: format!("serialize report: {e}"),
            })?;
            println!("{line}");
        }
        _ => {
            let body = serde_json::to_string(&report).map_err(|e| CliError::ProviderError {
                message: format!("serialize report: {e}"),
            })?;
            println!("{body}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_v1_1_recognizes_documented_field() {
        let json =
            r#"{"contract_version": "1.1", "pipeline_id": "x", "version": "0.0.0", "stages": []}"#;
        assert!(detect_v1_1(json));
    }

    #[test]
    fn detect_v1_1_rejects_legacy_shape() {
        let json = r#"{"name": "legacy", "stages": []}"#;
        assert!(!detect_v1_1(json));
    }

    #[test]
    fn detect_v1_1_rejects_unknown_contract_version() {
        let json =
            r#"{"contract_version": "9.9", "pipeline_id": "x", "version": "0.0.0", "stages": []}"#;
        assert!(!detect_v1_1(json));
    }
}
