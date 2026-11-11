//! Feature 106 / US3 — boundary/non-execution outcome binary tests.
//!
//! Pins:
//! - FR-011 CLIPS execution only via `rule_eval.generic`
//! - FR-012 `tool.emit_artifact` is external-only
//! - FR-014 typed non-execution outcomes carry `RuntimeDomain`
//! - FR-015 no plugin path / repo name / Zenoh route leaks
//! - FR-049 public-boundary scans pass on every new outcome

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

fn run_pipeline_run_json(fixture: &str) -> Value {
    let output = cli()
        .args([
            "pipeline",
            "run",
            "--input",
            &fixture_path(fixture),
            "--format",
            "json",
        ])
        .assert()
        // pipeline run may exit non-zero if any stage failed; we want
        // to inspect the report either way. Allow non-zero exit.
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).expect("utf-8");
    serde_json::from_str::<Value>(&text).unwrap_or_else(|e| panic!("invalid JSON ({e}):\n{text}"))
}

/// T044 (US3) / FR-012 — `tool.emit_artifact` returns
/// `external_action_required` with domain `tool_emit_artifact`.
#[test]
fn tool_emit_artifact_returns_external_action_required() {
    let value = run_pipeline_run_json("v11-tool-external.json");
    let stage = &value["stage_results"][0];
    assert_eq!(stage["outcome"], "external_action_required");
    assert_eq!(stage["domain"], "tool_emit_artifact");

    let text = serde_json::to_string(&value).unwrap();
    v11_rc_support::assert_no_public_boundary_leak(&text);
}

/// T044 (US3) / FR-014 — data-plane (Substrait-style) stages return
/// `declaration_only` with documented runtime domain.
#[test]
fn data_store_search_returns_declaration_only() {
    let value = run_pipeline_run_json("v11-substrait-only.json");
    let stage = &value["stage_results"][0];
    assert_eq!(stage["outcome"], "declaration_only");
    // The DataStoreSearch stage type maps to the duckdb_adapter
    // runtime domain in the v1.1.0 RC vocabulary.
    assert_eq!(stage["domain"], "duckdb_adapter");

    let text = serde_json::to_string(&value).unwrap();
    v11_rc_support::assert_no_public_boundary_leak(&text);
}

/// T044 (US3) / FR-011 — CLIPS `rule_eval.generic` stages route through
/// the rules-provider boundary and are accepted (no CLIPS
/// orchestration request emerges from this fixture).
#[test]
fn clips_rule_eval_generic_is_accepted() {
    let value = run_pipeline_run_json("v11-clips-rule-eval.json");
    let stage = &value["stage_results"][0];
    // The v1.1.0 RC runner records `rule_eval.generic` as `success`
    // (the boundary is acknowledged; the actual rules-provider
    // invocation lives in the existing CLIPS adapter and is
    // integrated as Integration follow-up).
    assert_eq!(stage["outcome"], "success");

    let text = serde_json::to_string(&value).unwrap();
    v11_rc_support::assert_no_public_boundary_leak(&text);
}
