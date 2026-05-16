//! Binary + semantic contract tests for `bn learn` and `bn evidence` (T031) - FR-006, Article III.
//!
//! BN is a Community-edition surface (no entitlement gate, like `bn infer`),
//! so these run in default CI. The semantic tests assert engine-derived
//! learned CPDs / validated evidence - they must fail against a stub that
//! returns a hardcoded shape or `valid: true` unconditionally.

use assert_cmd::Command;
use std::path::PathBuf;

fn cli() -> Command {
    Command::cargo_bin("nxuskit-cli").unwrap()
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn fixture_str(name: &str) -> String {
    std::fs::read_to_string(fixture(name)).expect("read fixture")
}

/// Build the `bn learn` input by injecting the absolute CSV path into the
/// network-skeleton fixture.
fn learn_input_with_csv() -> String {
    let mut v: serde_json::Value =
        serde_json::from_str(&fixture_str("bn_learn_network.json")).expect("parse learn fixture");
    let csv = fixture("bn_learn_training.csv");
    v["data_file"] = serde_json::Value::String(csv.to_string_lossy().to_string());
    serde_json::to_string(&v).unwrap()
}

// ===================================================================
// `bn learn` - wiring + semantic
// ===================================================================

#[test]
fn bn_learn_help_reachable_and_mentions_data_file() {
    let output = cli().args(["bn", "learn", "--help"]).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--input"),
        "bn learn --help must mention --input; got:\n{stdout}"
    );
    assert!(
        stdout.to_lowercase().contains("csv") || stdout.to_lowercase().contains("data_file"),
        "bn learn --help must describe the CSV data_file; got:\n{stdout}"
    );
}

#[test]
fn bn_learn_invalid_json_returns_parse_error_exit5() {
    let output = cli()
        .args(["bn", "learn", "--input", "-"])
        .write_stdin("{ not json")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(5));
    let stderr = String::from_utf8_lossy(&output.stderr);
    let err: serde_json::Value = serde_json::from_str(stderr.trim()).expect("stderr JSON");
    assert_eq!(err["code"], "parse_error");
}

#[test]
fn bn_learn_learns_engine_derived_cpds_from_csv() {
    // The training CSV has Rain = yes x3, no x7 (10 rows). With MLE +
    // pseudocount 0, the learned CPT for the root variable Rain must be
    // [0.3, 0.7] - an engine-derived value, not a stub default.
    let output = cli()
        .args(["bn", "learn", "--input", "-", "--format", "json"])
        .write_stdin(learn_input_with_csv())
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let resp: serde_json::Value = serde_json::from_str(stdout.trim()).expect("valid JSON");
    let result = &resp["result"];
    assert_eq!(result["num_rows"], 10, "10 training rows consumed");
    assert_eq!(result["learner"], "mle");
    let rain_cpt = result["learned_cpts"]["Rain"]
        .as_array()
        .expect("learned_cpts.Rain must be a probability array");
    assert_eq!(rain_cpt.len(), 2, "Rain has 2 states");
    let p_yes = rain_cpt[0].as_f64().expect("p(Rain=yes)");
    let p_no = rain_cpt[1].as_f64().expect("p(Rain=no)");
    assert!(
        (p_yes - 0.3).abs() < 1e-9,
        "p(Rain=yes) must be 0.3 (3/10), got {p_yes}"
    );
    assert!(
        (p_no - 0.7).abs() < 1e-9,
        "p(Rain=no) must be 0.7 (7/10), got {p_no}"
    );
    // The result must round-trip to BIF (engine-derived export).
    assert!(
        result["bif"]
            .as_str()
            .is_some_and(|s| s.contains("Rain") && s.contains("network")),
        "result.bif must be a BIF-format string mentioning the network and Rain; got {:?}",
        result["bif"]
    );
}

#[test]
fn bn_learn_missing_data_file_returns_exit5() {
    let mut v: serde_json::Value =
        serde_json::from_str(&fixture_str("bn_learn_network.json")).unwrap();
    v["data_file"] = serde_json::Value::String("/nonexistent/path/to/training.csv".into());
    let output = cli()
        .args(["bn", "learn", "--input", "-"])
        .write_stdin(serde_json::to_string(&v).unwrap())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(5), "missing CSV must exit 5");
}

// ===================================================================
// `bn evidence` - wiring + semantic
// ===================================================================

#[test]
fn bn_evidence_help_reachable() {
    let output = cli().args(["bn", "evidence", "--help"]).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--input"),
        "bn evidence --help must mention --input; got:\n{stdout}"
    );
    assert!(
        stdout.to_lowercase().contains("observation") || stdout.to_lowercase().contains("evidence"),
        "bn evidence --help must describe observations; got:\n{stdout}"
    );
}

#[test]
fn bn_evidence_valid_observations_normalize_and_validate() {
    let output = cli()
        .args([
            "bn",
            "evidence",
            "--input",
            fixture("bn_evidence_valid.json").to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let resp: serde_json::Value = serde_json::from_str(stdout.trim()).expect("valid JSON");
    let result = &resp["result"];
    assert_eq!(result["valid"], true);
    // Engine-validated: the observation must be echoed back against the network.
    assert_eq!(result["evidence"]["Rain"], "yes");
    assert_eq!(
        result["observation_count"], 1,
        "one observation was validated"
    );
}

#[test]
fn bn_evidence_unknown_state_returns_validation_exit5() {
    // The observation Rain=maybe references a state that does not exist on the
    // network's Rain variable - this must be caught by the engine, not stubbed.
    let output = cli()
        .args([
            "bn",
            "evidence",
            "--input",
            fixture("bn_evidence_bad_state.json").to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(5), "unknown state must exit 5");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let err: serde_json::Value = serde_json::from_str(stderr.trim()).expect("stderr JSON");
    assert_eq!(err["code"], "validation");
    // The diagnostic must actually mention the offending variable/state.
    let msg = err["message"].as_str().unwrap_or("").to_lowercase();
    assert!(
        msg.contains("rain") || msg.contains("maybe") || msg.contains("state"),
        "error message must name the offending observation; got: {}",
        err["message"]
    );
}
