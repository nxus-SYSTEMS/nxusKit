//! Feature 106 / US6 — declaration-only orchestration CLI tests.

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
fn orchestration_validate_normalize_and_hash_are_stable() {
    let validate = cli()
        .args([
            "orchestration",
            "validate",
            "--input",
            &fixture_path("v11-orchestration.json"),
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let validate_text = String::from_utf8(validate).expect("utf-8");
    let validate_value: Value = serde_json::from_str(&validate_text).expect("json");
    assert_eq!(validate_value["contract_version"], "1.1");
    assert_eq!(validate_value["outcome"], "valid");

    let normalize = cli()
        .args([
            "orchestration",
            "normalize",
            "--input",
            &fixture_path("v11-orchestration.json"),
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let normalize_text = String::from_utf8(normalize).expect("utf-8");
    let normalized_value: Value = serde_json::from_str(&normalize_text).expect("json");
    assert_eq!(normalized_value["id"], "v11-orchestration");

    let mut normalize_again = cli();
    normalize_again.write_stdin(normalize_text.clone()).args([
        "orchestration",
        "normalize",
        "--input",
        "-",
        "--format",
        "json",
    ]);
    let normalize_again_text = String::from_utf8(
        normalize_again
            .assert()
            .success()
            .get_output()
            .stdout
            .clone(),
    )
    .expect("utf-8");
    assert_eq!(normalize_text, normalize_again_text);

    let hash = cli()
        .args([
            "orchestration",
            "hash",
            "--input",
            &fixture_path("v11-orchestration.json"),
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let hash_text = String::from_utf8(hash).expect("utf-8");
    let hash_value: Value = serde_json::from_str(&hash_text).expect("json");
    assert_eq!(hash_value["kind"], "orchestration_definition");
    assert_eq!(hash_value["hash"].as_str().expect("hash").len(), 64);

    v11_rc_support::assert_no_public_boundary_leak(&validate_text);
    v11_rc_support::assert_no_public_boundary_leak(&normalize_text);
    v11_rc_support::assert_no_public_boundary_leak(&hash_text);
}

#[test]
fn orchestration_graph_json_and_mermaid_agree_on_nodes_and_edges() {
    let json_output = cli()
        .args([
            "orchestration",
            "graph",
            "--input",
            &fixture_path("v11-orchestration.json"),
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json_text = String::from_utf8(json_output).expect("utf-8");
    let graph: Value = serde_json::from_str(&json_text).expect("json");
    assert_eq!(graph["source_kind"], "orchestration");
    assert_eq!(graph["source_id"], "v11-orchestration");
    assert_eq!(graph["validation_status"], "valid");
    assert_eq!(graph["nodes"].as_array().expect("nodes").len(), 3);
    assert_eq!(graph["edges"].as_array().expect("edges").len(), 2);

    let mermaid_output = cli()
        .args([
            "orchestration",
            "graph",
            "--input",
            &fixture_path("v11-orchestration.json"),
            "--mermaid",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mermaid = String::from_utf8(mermaid_output).expect("utf-8");
    assert!(mermaid.starts_with("flowchart TD\n"));
    for node_id in ["collect", "rules", "review"] {
        assert!(
            mermaid.contains(node_id),
            "missing node {node_id}: {mermaid}"
        );
    }
    assert!(mermaid.contains("-->|data|"));
    assert!(mermaid.contains("-.->|control|"));

    v11_rc_support::assert_no_public_boundary_leak(&json_text);
    v11_rc_support::assert_no_public_boundary_leak(&mermaid);
}

#[test]
fn orchestration_run_is_not_available_in_v11_rc() {
    cli()
        .args([
            "orchestration",
            "run",
            "--input",
            &fixture_path("v11-orchestration.json"),
        ])
        .assert()
        .failure();
}
