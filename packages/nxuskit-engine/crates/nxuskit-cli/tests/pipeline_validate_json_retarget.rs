//! US6 (T062, DJ-102 / DJ-103) - binary-level smoke for
//! `nxuskit-cli pipeline validate --json`.
//!
//! Article II CLI Verification Requirements: binary-level invocation via
//! `assert_cmd`. Asserts that:
//!   - positive fixtures yield `{"outcome":"valid"}` and exit code 0.
//!   - negative fixtures yield class-identifying outcome shapes and the
//!     locked non-zero exit codes from
//!     `nxuskit::contract::v1_1::pipeline_validate_exit_code`.
//!
//! Fixtures live under the retarget contract fixture directory.

use std::path::PathBuf;

use assert_cmd::Command;
use serde_json::Value;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .expect("workspace root")
        .join("specs")
        .join("105-v110-arrow-zenoh-substrait-retarget")
        .join("contracts")
        .join("fixtures")
}

fn cli() -> Command {
    Command::cargo_bin("nxuskit-cli").unwrap()
}

fn run_validate(fixture: &str) -> std::process::Output {
    let path = fixtures_dir().join(fixture);
    assert!(path.exists(), "fixture missing: {path:?}");
    cli()
        .args(["pipeline", "validate", path.to_str().unwrap(), "--json"])
        .output()
        .expect("invoke nxuskit-cli pipeline validate --json")
}

fn parse_outcome(out: &std::process::Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "non-JSON output: {e}\nstdout: {}",
            String::from_utf8_lossy(&out.stdout)
        )
    })
}

// DJ-102 - Positive mixed-kind fixture: `{"outcome":"valid"}`, exit 0.
#[test]
fn dj_102_positive_pipeline_data_store_search_plan_ref_yields_valid_exit_zero() {
    let out = run_validate("positive_pipeline_data_store_search_plan_ref.json");
    assert!(
        out.status.success(),
        "positive fixture must exit 0; got {:?}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let outcome = parse_outcome(&out);
    assert_eq!(outcome["outcome"], "valid");
}

#[test]
fn dj_102_positive_pipeline_llm_plus_referenced_data_tool_yields_valid_exit_zero() {
    let out = run_validate("positive_pipeline_llm_plus_referenced_data_tool.json");
    assert!(out.status.success(), "positive fixture must exit 0");
    let outcome = parse_outcome(&out);
    assert_eq!(outcome["outcome"], "valid");
}

// DJ-103 - Negative bulk-payload fixture: outcome=bulk_payload_in_control_plane,
// non-zero exit code.
#[test]
fn dj_103_negative_boundary_bulk_payload_yields_bulk_payload_outcome_nonzero_exit() {
    let out = run_validate("negative_boundary_bulk_payload.json");
    assert!(
        !out.status.success(),
        "bulk-payload negative fixture must exit non-zero"
    );
    let outcome = parse_outcome(&out);
    assert_eq!(outcome["outcome"], "bulk_payload_in_control_plane");
    assert!(outcome["path"].is_string());
}

#[test]
fn dj_103_negative_ambiguous_slot_yields_ambiguous_slot_outcome_nonzero_exit() {
    let out = run_validate("negative_ambiguous_slot.json");
    assert!(!out.status.success());
    let outcome = parse_outcome(&out);
    assert_eq!(outcome["outcome"], "ambiguous_slot");
    assert!(
        outcome["path"]
            .as_str()
            .unwrap_or("")
            .contains("tool_input"),
        "ambiguous_slot path must locate offending key: {outcome:?}"
    );
}

#[test]
fn dj_103_negative_plan_reference_unbound_yields_plan_reference_unbound_outcome_nonzero_exit() {
    let out = run_validate("negative_plan_reference_unbound.json");
    assert!(!out.status.success());
    let outcome = parse_outcome(&out);
    assert_eq!(outcome["outcome"], "plan_reference_unbound");
    assert_eq!(outcome["stage_id"], "search");
}

// Class-distinguishable exit codes per the locked policy in
// `nxuskit::contract::v1_1::pipeline_validate_exit_code`. The exit codes
// MUST be stable across runs; they are documented in the contract.
#[test]
fn dj_103_exit_codes_are_class_distinguishable() {
    let bulk = run_validate("negative_boundary_bulk_payload.json");
    let amb = run_validate("negative_ambiguous_slot.json");
    let plan = run_validate("negative_plan_reference_unbound.json");
    let bulk_code = bulk.status.code().expect("exit code");
    let amb_code = amb.status.code().expect("exit code");
    let plan_code = plan.status.code().expect("exit code");
    assert_ne!(bulk_code, 0);
    assert_ne!(amb_code, 0);
    assert_ne!(plan_code, 0);
    assert_ne!(bulk_code, amb_code, "exit codes must distinguish classes");
    assert_ne!(bulk_code, plan_code, "exit codes must distinguish classes");
    assert_ne!(amb_code, plan_code, "exit codes must distinguish classes");
}
