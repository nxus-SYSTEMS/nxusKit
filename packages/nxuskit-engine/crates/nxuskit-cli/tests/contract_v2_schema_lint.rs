//! S03 Gate E binary-level smokes for v2 schema and fixture lint.

use assert_cmd::Command;
use serde_json::Value;
use std::path::PathBuf;

fn cli() -> Command {
    Command::cargo_bin("nxuskit-cli").unwrap()
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../nxuskit/tests/fixtures/contract_v2")
}

#[test]
fn contract_v2_schema_emit_is_stable_json_manifest() {
    let output = cli()
        .args(["contract-v2", "schema", "emit"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let manifest: Value = serde_json::from_slice(&output).expect("schema emit outputs JSON");

    assert_eq!(
        manifest["manifest_kind"],
        "nxuskit.contract.v2.schema_manifest"
    );
    assert_eq!(manifest["contract_version"], "2.0-alpha");
    assert_eq!(manifest["vocabulary"]["product"], "Capability Catalog");
    assert!(
        manifest["dto_schemas"]
            .as_array()
            .unwrap()
            .iter()
            .any(|schema| schema["name"] == "OperationEnvelope")
    );
    assert!(
        manifest["reference_schemas"]
            .as_array()
            .unwrap()
            .iter()
            .any(|schema| schema == "BatchRef")
    );
}

#[test]
fn contract_v2_validate_accepts_positive_fixture_corpus() {
    let output = cli()
        .args([
            "contract-v2",
            "validate",
            fixture_root().join("positive").to_str().unwrap(),
            "--posture",
            "positive",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: Value = serde_json::from_slice(&output).expect("validate outputs JSON");

    assert_eq!(report["status"], "pass");
    assert_eq!(report["checked"], 6);
    assert!(
        report["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .all(|diagnostic| diagnostic["status"] == "pass")
    );
}

#[test]
fn contract_v2_lint_classifies_negative_fixture_corpus() {
    let output = cli()
        .args([
            "contract-v2",
            "lint",
            fixture_root().join("negative").to_str().unwrap(),
            "--posture",
            "negative",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: Value = serde_json::from_slice(&output).expect("lint outputs JSON");

    assert_eq!(report["status"], "pass");
    assert_eq!(report["checked"], 7);
    let keys: Vec<&str> = report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|diagnostic| diagnostic["key"].as_str())
        .collect();
    assert!(keys.contains(&"rows"));
    assert!(keys.contains(&"record_batch"));
    assert!(keys.contains(&"inline_sql"));
    assert!(keys.contains(&"file_body"));
    assert!(keys.contains(&"blob_payload"));
    assert!(keys.contains(&"private_persona_ref"));
    assert!(keys.contains(&"workflow_pack_content_ref"));
}

#[test]
fn contract_v2_validate_rejects_negative_fixture_as_positive() {
    let output = cli()
        .args([
            "contract-v2",
            "validate",
            fixture_root()
                .join("negative/bulk-rows.json")
                .to_str()
                .unwrap(),
            "--posture",
            "positive",
        ])
        .assert()
        .failure()
        .code(5)
        .get_output()
        .stderr
        .clone();
    let error: Value = serde_json::from_slice(&output).expect("failure emits JSON error");

    assert_eq!(error["code"], "contract_v2_validation_failed");
    assert_eq!(error["details"]["status"], "fail");
}

#[test]
fn contract_v2_schema_diff_passes_against_emitted_manifest() {
    let emit = cli()
        .args(["contract-v2", "schema", "emit"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let expected = std::env::temp_dir().join(format!(
        "nxuskit-contract-v2-schema-{}.json",
        std::process::id()
    ));
    std::fs::write(&expected, emit).unwrap();

    let output = cli()
        .args(["contract-v2", "diff", expected.to_str().unwrap()])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: Value = serde_json::from_slice(&output).expect("diff outputs JSON");
    let _ = std::fs::remove_file(expected);

    assert_eq!(report["status"], "pass");
    assert_eq!(report["diagnostics"][0]["artifact_type"], "schema_manifest");
}
