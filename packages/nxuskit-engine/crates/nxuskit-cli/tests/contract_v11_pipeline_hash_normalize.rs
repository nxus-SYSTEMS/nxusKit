//! Feature 106 / US6 — `pipeline normalize` and `pipeline hash`.

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

#[test]
fn pipeline_normalize_is_deterministic_and_preserves_public_metadata() {
    let output_a = cli()
        .args([
            "pipeline",
            "normalize",
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
    let text_a = String::from_utf8(output_a).expect("utf-8");
    let value_a: Value = serde_json::from_str(&text_a).expect("json");

    assert_eq!(value_a["contract_version"], "1.1");
    assert!(
        value_a["stages"][0]["metadata"].get("fan_out").is_some(),
        "normalize must preserve public extension metadata"
    );

    let mut second = cli();
    second.write_stdin(text_a.clone()).args([
        "pipeline",
        "normalize",
        "--input",
        "-",
        "--format",
        "json",
    ]);
    let output_b = second.assert().success().get_output().stdout.clone();
    let text_b = String::from_utf8(output_b).expect("utf-8");

    assert_eq!(text_a, text_b, "normalize must be idempotent");
    v11_rc_support::assert_no_public_boundary_leak(&text_a);
}

#[test]
fn pipeline_hash_matches_normalized_input_and_run_report_hash() {
    let hash_output = cli()
        .args([
            "pipeline",
            "hash",
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
    let hash_text = String::from_utf8(hash_output).expect("utf-8");
    let hash_value: Value = serde_json::from_str(&hash_text).expect("json");
    assert_eq!(hash_value["contract_version"], "1.1");
    assert_eq!(hash_value["kind"], "pipeline_definition");
    let cli_hash = hash_value["hash"].as_str().expect("hash string");
    assert_eq!(cli_hash.len(), 64);

    let normalize_output = cli()
        .args([
            "pipeline",
            "normalize",
            "--input",
            &fixture_path("v11-llm-only.json"),
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let normalized = String::from_utf8(normalize_output).expect("utf-8");
    let mut hash_normalized = cli();
    hash_normalized
        .write_stdin(normalized)
        .args(["pipeline", "hash", "--input", "-", "--format", "json"]);
    let normalized_hash_output = hash_normalized
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let normalized_hash_text = String::from_utf8(normalized_hash_output).expect("utf-8");
    let normalized_hash_value: Value = serde_json::from_str(&normalized_hash_text).expect("json");
    assert_eq!(normalized_hash_value["hash"], cli_hash);

    let run_output = cli()
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
    let run_text = String::from_utf8(run_output).expect("utf-8");
    let run_value: Value = serde_json::from_str(&run_text).expect("json");
    assert_eq!(run_value["pipeline_canonical_hash"], cli_hash);

    v11_rc_support::assert_no_public_boundary_leak(&hash_text);
}
