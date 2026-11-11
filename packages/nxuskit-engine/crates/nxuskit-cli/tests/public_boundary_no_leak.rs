//! Feature 106 / US4 (T056) — consolidated public-boundary scan.
//!
//! Runs every feature-106 binary fixture through the compiled
//! `nxuskit-cli` and asserts that stdout contains no string from the
//! documented public-boundary deny-list (FR-049).
//!
//! Per-test deny-list scans live in the individual contract test files
//! (`contract_v11_doctor.rs`, `contract_v11_pipeline_run.rs`,
//! `contract_v11_pipeline_fanout.rs`, `contract_v11_pipeline_boundaries.rs`).
//! This test consolidates them in one place so that a release-gate
//! sweep can verify the entire feature-106 surface in a single
//! invocation.

use assert_cmd::Command;

#[path = "v11_rc_support.rs"]
mod v11_rc_support;

fn cli() -> Command {
    Command::cargo_bin("nxuskit-cli").unwrap()
}

fn fixture_path(name: &str) -> String {
    format!("tests/fixtures/v11_rc_peeler_cli/{name}")
}

const FIXTURES: &[&str] = &[
    "v11-llm-only.json",
    "v11-fanout.json",
    "v11-tool-external.json",
    "v11-substrait-only.json",
    "v11-clips-rule-eval.json",
];

/// `doctor --json` stdout passes the deny-list.
#[test]
fn doctor_json_passes_public_boundary_scan() {
    let output = cli()
        .args(["doctor", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).expect("utf-8");
    v11_rc_support::assert_no_public_boundary_leak(&text);
}

/// Every feature-106 v1.1 fixture's `pipeline run --format json`
/// stdout passes the deny-list.
#[test]
fn pipeline_run_json_passes_public_boundary_scan_for_every_fixture() {
    for fixture in FIXTURES {
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
            .get_output()
            .stdout
            .clone();
        let text = String::from_utf8(output).expect("utf-8");
        v11_rc_support::assert_no_public_boundary_leak(&text);
    }
}

/// Every fan-out fixture's `pipeline run --format jsonl` stdout
/// (events + final report line) passes the deny-list.
#[test]
fn pipeline_run_jsonl_passes_public_boundary_scan() {
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
    v11_rc_support::assert_no_public_boundary_leak(&text);
}

/// US6 declaration-only orchestration outputs pass the deny-list.
#[test]
fn orchestration_outputs_pass_public_boundary_scan() {
    let fixture = fixture_path("v11-orchestration.json");
    let commands: Vec<Vec<String>> = vec![
        vec![
            "orchestration".into(),
            "validate".into(),
            "--input".into(),
            fixture.clone(),
            "--format".into(),
            "json".into(),
        ],
        vec![
            "orchestration".into(),
            "normalize".into(),
            "--input".into(),
            fixture.clone(),
            "--format".into(),
            "json".into(),
        ],
        vec![
            "orchestration".into(),
            "hash".into(),
            "--input".into(),
            fixture.clone(),
            "--format".into(),
            "json".into(),
        ],
        vec![
            "orchestration".into(),
            "graph".into(),
            "--input".into(),
            fixture.clone(),
            "--format".into(),
            "json".into(),
        ],
        vec![
            "orchestration".into(),
            "graph".into(),
            "--input".into(),
            fixture,
            "--mermaid".into(),
        ],
    ];
    for args in commands {
        let output = cli()
            .args(args)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let text = String::from_utf8(output).expect("utf-8");
        v11_rc_support::assert_no_public_boundary_leak(&text);
    }
}
