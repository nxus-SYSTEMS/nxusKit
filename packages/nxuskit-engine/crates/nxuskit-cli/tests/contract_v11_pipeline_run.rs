//! Feature 106 / US2 — `nxuskit-cli pipeline run` v1.1 contract-native
//! binary-level tests.
//!
//! Pins:
//! - FR-006 auto-detection by `contract_version == "1.1"`
//! - FR-007 legacy input compatibility unchanged
//! - FR-008 v1.1 input validated through the public DTO loader
//! - FR-009 LLM stage execution via provider/model refs
//! - FR-022 default `PipelineRunReport` output
//! - FR-027 no bulk payload leak

use assert_cmd::Command;
use serde_json::Value;

#[path = "v11_rc_support.rs"]
mod v11_rc_support;

fn cli() -> Command {
    Command::cargo_bin("nxuskit-cli").unwrap()
}

fn fixture_path(name: &str) -> String {
    format!("tests/fixtures/v11_rc_peeler_cli/{name}")
}

/// T032 — running a v1.1 LLM-only pipeline through `pipeline run` must
/// route to the contract-native runner and emit a `PipelineRunReport`
/// shape (FR-006, FR-008, FR-009, FR-022).
#[test]
fn pipeline_run_v11_llm_only_emits_pipeline_run_report() {
    let output = cli()
        .args([
            "pipeline",
            "run",
            "--input",
            &fixture_path("v11-llm-only.json"),
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).expect("utf-8 stdout");
    let value: Value = serde_json::from_str(&text).expect("valid JSON");

    // PipelineRunReport top-level shape.
    assert_eq!(value["contract_version"], "1.1");
    assert!(value.get("pipeline_ref").is_some());
    assert!(value.get("pipeline_canonical_hash").is_some());

    // Default standard preset emits stage_results + final_outputs.
    assert!(
        value.get("stage_results").is_some(),
        "missing stage_results"
    );
    let stage_results = value["stage_results"]
        .as_array()
        .expect("stage_results is an array");
    assert_eq!(stage_results.len(), 1);
    assert_eq!(stage_results[0]["stage_name"], "summarize");
    assert_eq!(stage_results[0]["outcome"], "success");

    // No public-boundary leaks.
    v11_rc_support::assert_no_public_boundary_leak(&text);
}

/// T033 — a legacy pipeline JSON (the old `name`/`type`/`config`/
/// `output_key` shape) must continue to flow through the legacy runner
/// unchanged (FR-007).
#[test]
fn pipeline_run_legacy_shape_routes_through_legacy_runner() {
    let dir = std::env::temp_dir().join("nxuskit-106-legacy");
    std::fs::create_dir_all(&dir).expect("mkdir tmp");
    let path = dir.join("legacy.json");
    std::fs::write(
        &path,
        r#"{
            "name": "legacy-pipeline",
            "stages": [
                {
                    "name": "first",
                    "type": "llm",
                    "config": {
                        "provider": "loopback",
                        "model": "echo",
                        "prompt": "hi"
                    }
                }
            ]
        }"#,
    )
    .expect("write fixture");

    let output = cli()
        .args([
            "pipeline",
            "run",
            "--input",
            path.to_str().unwrap(),
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).expect("utf-8 stdout");
    let value: Value = serde_json::from_str(&text).expect("valid JSON");

    // Legacy runner wraps its result under a `result` block with
    // `stages` and `summary`, and does NOT emit the v1.1
    // `PipelineRunReport`'s `pipeline_canonical_hash` field.
    let result_block = value
        .get("result")
        .expect("legacy output carries top-level `result`");
    assert!(
        result_block.get("summary").is_some(),
        "legacy summary present under result"
    );
    assert!(
        value.get("pipeline_canonical_hash").is_none()
            && value
                .get("contract_version")
                .map(|v| v.as_str() != Some("1.1"))
                .unwrap_or(true),
        "legacy output must not carry v1.1 PipelineRunReport fields"
    );
}

/// T097 (Polish phase, covered now as part of US2): `pipeline run` help
/// text must mention the new public-safe v1.1 output-selection flags.
#[test]
fn pipeline_run_help_mentions_v11_output_selection_flags() {
    let output = cli()
        .args(["pipeline", "run", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).expect("utf-8 help");
    assert!(
        text.contains("--report-preset") || text.contains("report-preset"),
        "help text must document --report-preset (got:\n{text})"
    );
}

// -----------------------------------------------------------------------
// Feature 106 / US4 — envelope-compatible provenance in the run report.
// T055: assert the embedded `artifact_history_envelope` carries the
// canonical pipeline hash, replay linkage, fan-out per-operation
// outcomes when applicable, and is round-trippable into the public
// v1.1 DTO. FR-026, FR-027.
// -----------------------------------------------------------------------

/// T055 — the default `standard` preset MUST emit an embedded
/// `artifact_history_envelope` whose `pipeline_canonical_hash` field on
/// `pipeline_ref` matches the top-level
/// `pipeline_canonical_hash`. The envelope must round-trip into
/// `nxuskit::contract::v1_1::ArtifactHistoryEnvelope`.
#[test]
fn pipeline_run_v11_embeds_envelope_with_canonical_hash() {
    let output = cli()
        .args([
            "pipeline",
            "run",
            "--input",
            &fixture_path("v11-llm-only.json"),
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).expect("utf-8");
    let value: Value = serde_json::from_str(&text).expect("valid JSON");

    let env = value
        .get("artifact_history_envelope")
        .expect("standard preset must embed an artifact_history_envelope");

    // The embedded envelope MUST round-trip into the public DTO.
    let envelope: nxuskit::contract::v1_1::ArtifactHistoryEnvelope =
        serde_json::from_value(env.clone()).expect("envelope round-trips into public v1.1 DTO");

    // The pipeline canonical hash matches the top-level field.
    let top_hash = value["pipeline_canonical_hash"]
        .as_str()
        .expect("top-level pipeline_canonical_hash is a string");
    let envelope_pipeline_hash = envelope
        .pipeline_ref
        .canonical_hash
        .as_ref()
        .expect("envelope pipeline_ref carries canonical_hash");
    assert_eq!(
        envelope_pipeline_hash.as_str(),
        top_hash,
        "envelope pipeline_ref.canonical_hash must equal top-level pipeline_canonical_hash"
    );

    // Replay linkage present.
    let _ = envelope.replay_linkage.replay_kind;

    v11_rc_support::assert_no_public_boundary_leak(&text);
}

/// T055 — fan-out runs must surface per-operation outcomes inside the
/// embedded envelope's `fan_out_outcomes` array (FR-026).
#[test]
fn pipeline_run_v11_envelope_carries_fan_out_outcomes() {
    let output = cli()
        .args([
            "pipeline",
            "run",
            "--input",
            &fixture_path("v11-fanout.json"),
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).expect("utf-8");
    let value: Value = serde_json::from_str(&text).expect("valid JSON");

    let envelope: nxuskit::contract::v1_1::ArtifactHistoryEnvelope =
        serde_json::from_value(value["artifact_history_envelope"].clone())
            .expect("envelope round-trips");

    assert!(
        !envelope.fan_out_outcomes.is_empty(),
        "fan-out fixture must populate envelope.fan_out_outcomes"
    );
    let group = &envelope.fan_out_outcomes[0];
    assert_eq!(group.group_id, "compare_summaries");
    assert_eq!(group.operations.len(), 2);
    // Operation ids match the documented stable format.
    assert_eq!(group.operations[0].operation_id, "compare_summaries::op-0");
    assert_eq!(group.operations[1].operation_id, "compare_summaries::op-1");

    v11_rc_support::assert_no_public_boundary_leak(&text);
}
