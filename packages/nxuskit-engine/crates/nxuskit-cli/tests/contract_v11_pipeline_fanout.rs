//! Feature 106 / US3 — fan-out binary tests.
//!
//! Pins:
//! - FR-016 metadata.fan_out normalized into sibling operations
//! - FR-018 truly async dispatch
//! - FR-019 stable operation ids `<stage_id>::op-<pair_index>`
//! - FR-020 stable per-operation result shape
//! - FR-025 JSONL operation event vocabulary

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

/// T043 — running the fan-out fixture must emit two per-operation
/// results with stable operation ids and a `fan_out` block on the
/// parent stage (FR-016, FR-019, FR-020).
#[test]
fn fanout_pipeline_run_emits_stable_operation_ids() {
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
    let value: Value = serde_json::from_str(&text).expect("json");

    let stage_results = value["stage_results"].as_array().expect("array");
    assert_eq!(stage_results.len(), 1);
    let stage = &stage_results[0];
    assert_eq!(stage["stage_name"], "compare_summaries");
    let fan_out = stage.get("fan_out").expect("fan_out block present");
    let operations = fan_out["operations"].as_array().expect("operations array");
    assert_eq!(operations.len(), 2);
    // Stable, pair-index-keyed operation ids.
    assert_eq!(operations[0]["operation_id"], "compare_summaries::op-0");
    assert_eq!(operations[0]["pair_index"], 0);
    assert_eq!(operations[1]["operation_id"], "compare_summaries::op-1");
    assert_eq!(operations[1]["pair_index"], 1);
    // Both operations should succeed against the loopback provider.
    assert_eq!(operations[0]["outcome"], "success");
    assert_eq!(operations[1]["outcome"], "success");

    v11_rc_support::assert_no_public_boundary_leak(&text);
}

/// FR-025 — JSONL mode emits `operation_queued`, `operation_started`,
/// and `operation_completed` events for each fan-out operation.
#[test]
fn fanout_pipeline_run_jsonl_emits_per_operation_events() {
    let output = cli()
        .args([
            "pipeline",
            "run",
            "--input",
            &fixture_path("v11-fanout.json"),
            "--format",
            "jsonl",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).expect("utf-8");
    // Parse each non-empty line as a JSON object.
    let events: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("valid jsonl line"))
        .collect();
    let event_names: Vec<&str> = events
        .iter()
        .filter_map(|e| e.get("event").and_then(|v| v.as_str()))
        .collect();
    // Must include both queued, started, and completed for each op.
    let queued = event_names
        .iter()
        .filter(|n| **n == "operation_queued")
        .count();
    let started = event_names
        .iter()
        .filter(|n| **n == "operation_started")
        .count();
    let completed = event_names
        .iter()
        .filter(|n| **n == "operation_completed")
        .count();
    assert!(
        queued >= 2,
        "expected >=2 operation_queued events, got {queued}: {event_names:?}"
    );
    assert!(
        started >= 2,
        "expected >=2 operation_started events, got {started}: {event_names:?}"
    );
    assert!(
        completed >= 2,
        "expected >=2 operation_completed events, got {completed}: {event_names:?}"
    );

    // Both pipeline_start and pipeline_complete must appear.
    assert!(event_names.contains(&"pipeline_start"));
    assert!(event_names.contains(&"pipeline_complete"));

    v11_rc_support::assert_no_public_boundary_leak(&text);
}
