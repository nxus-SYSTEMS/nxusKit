//! Contract foundation tests (T007) — FR-003
//!
//! These tests verify the shell contract foundation:
//! - `--format invalid_value` → stderr JSON error, exit 5
//! - `--input nonexistent.json` → `internal`, exit 1
//! - `--input -` with empty stdin → `internal`, exit 1
//! - Successful commands exit 0 with valid JSON

use assert_cmd::Command;

fn cli() -> Command {
    Command::cargo_bin("nxuskit-cli").unwrap()
}

// ── Invalid format tests ────────────────────────────────────────────

#[test]
fn call_invalid_format_returns_exit_5_with_json_error() {
    let mut cmd = cli();
    cmd.args(["call", "--input", "-", "--format", "invalid_value"])
        .write_stdin(r#"{"prompt":"hi","provider":"loopback"}"#);

    let output = cmd.output().unwrap();
    assert_eq!(output.status.code(), Some(5));

    let stderr = String::from_utf8_lossy(&output.stderr);
    let err: serde_json::Value =
        serde_json::from_str(stderr.trim()).expect("stderr should be valid JSON");
    assert_eq!(err["code"], "validation");
}

#[test]
fn clips_eval_invalid_format_returns_exit_5() {
    let mut cmd = cli();
    cmd.args(["clips", "eval", "--input", "-", "--format", "bad_format"])
        .write_stdin(r#"{"rules":"","facts":[],"queries":[]}"#);

    let output = cmd.output().unwrap();
    assert_eq!(output.status.code(), Some(5));

    let stderr = String::from_utf8_lossy(&output.stderr);
    let err: serde_json::Value =
        serde_json::from_str(stderr.trim()).expect("stderr should be valid JSON");
    assert_eq!(err["code"], "validation");
}

#[test]
fn bn_infer_invalid_format_returns_exit_5() {
    let mut cmd = cli();
    cmd.args(["bn", "infer", "--input", "-", "--format", "xml"])
        .write_stdin(r#"{"network":{"nodes":[],"edges":[],"cpds":{}},"query_nodes":[]}"#);

    let output = cmd.output().unwrap();
    assert_eq!(output.status.code(), Some(5));

    let stderr = String::from_utf8_lossy(&output.stderr);
    let err: serde_json::Value =
        serde_json::from_str(stderr.trim()).expect("stderr should be valid JSON");
    assert_eq!(err["code"], "validation");
}

// ── File not found tests ────────────────────────────────────────────

#[test]
fn call_nonexistent_input_returns_exit_1_file_not_found() {
    let output = cli()
        .args([
            "call",
            "--input",
            "/tmp/nxuskit_test_nonexistent_file_42.json",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8_lossy(&output.stderr);
    let err: serde_json::Value =
        serde_json::from_str(stderr.trim()).expect("stderr should be valid JSON");
    assert_eq!(err["code"], "internal");
}

#[test]
fn clips_eval_nonexistent_input_returns_exit_1() {
    let output = cli()
        .args([
            "clips",
            "eval",
            "--input",
            "/tmp/nxuskit_test_nonexistent_42.json",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8_lossy(&output.stderr);
    let err: serde_json::Value =
        serde_json::from_str(stderr.trim()).expect("stderr should be valid JSON");
    assert_eq!(err["code"], "internal");
}

#[test]
fn packet_validate_nonexistent_input_returns_exit_1() {
    let output = cli()
        .args([
            "packet",
            "validate",
            "--input",
            "/tmp/nxuskit_test_nonexistent_42.json",
            "--schema",
            "/tmp/schema.json",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8_lossy(&output.stderr);
    let err: serde_json::Value =
        serde_json::from_str(stderr.trim()).expect("stderr should be valid JSON");
    assert_eq!(err["code"], "internal");
}

// ── Empty stdin tests ───────────────────────────────────────────────

#[test]
fn call_empty_stdin_returns_exit_1_empty_input() {
    let mut cmd = cli();
    cmd.args(["call", "--input", "-"]).write_stdin("");

    let output = cmd.output().unwrap();
    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8_lossy(&output.stderr);
    let err: serde_json::Value =
        serde_json::from_str(stderr.trim()).expect("stderr should be valid JSON");
    assert_eq!(err["code"], "internal");
}

#[test]
fn clips_eval_empty_stdin_returns_exit_1_empty_input() {
    let mut cmd = cli();
    cmd.args(["clips", "eval", "--input", "-"])
        .write_stdin("   "); // whitespace-only = empty

    let output = cmd.output().unwrap();
    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8_lossy(&output.stderr);
    let err: serde_json::Value =
        serde_json::from_str(stderr.trim()).expect("stderr should be valid JSON");
    assert_eq!(err["code"], "internal");
}

// ── Error envelope structure tests ──────────────────────────────────

#[test]
fn error_envelope_has_required_fields() {
    let output = cli()
        .args(["call", "--input", "/tmp/nxuskit_test_nonexistent_42.json"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    let err: serde_json::Value =
        serde_json::from_str(stderr.trim()).expect("stderr should be valid JSON");

    // All error envelopes must have these fields per data-model.md
    assert!(err.get("code").is_some(), "Missing code");
    assert!(err.get("message").is_some(), "Missing message");
    assert!(err.get("trace_id").is_some(), "Missing trace_id");
    assert!(err.get("timestamp").is_some(), "Missing timestamp");

    // trace_id should be a non-empty string
    let trace_id = err["trace_id"].as_str().unwrap();
    assert!(!trace_id.is_empty(), "trace_id should be non-empty");

    // timestamp should be RFC 3339 (contains T and Z)
    let timestamp = err["timestamp"].as_str().unwrap();
    assert!(timestamp.contains('T'), "timestamp should be RFC 3339");
    assert!(timestamp.ends_with('Z'), "timestamp should be UTC");
}

// ── Successful command exit 0 test ──────────────────────────────────

#[test]
fn call_valid_loopback_returns_exit_0_with_json() {
    let mut cmd = cli();
    cmd.args(["call", "--input", "-", "--format", "json"])
        .write_stdin(r#"{"prompt":"hello","provider":"loopback","model":"echo"}"#);

    let output = cmd.output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "Expected exit 0, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let response: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("stdout should be valid JSON");

    // ResponseEnvelope fields
    assert!(response.get("trace_id").is_some(), "Missing trace_id");
    assert!(response.get("timestamp").is_some(), "Missing timestamp");
    assert!(
        response.get("request_hash").is_some(),
        "Missing request_hash"
    );
    assert!(
        response.get("request_metadata").is_some(),
        "Missing request_metadata"
    );
    assert!(response.get("result").is_some(), "Missing result");

    // result should have content
    let result = &response["result"];
    assert!(result.get("content").is_some(), "result missing content");
}

// ── YAML format test ────────────────────────────────────────────────

#[test]
fn call_yaml_format_returns_valid_yaml() {
    let mut cmd = cli();
    cmd.args(["call", "--input", "-", "--format", "yaml"])
        .write_stdin(r#"{"prompt":"hello","provider":"loopback","model":"echo"}"#);

    let output = cmd.output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "Expected exit 0, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should parse as valid YAML
    let _yaml: serde_json::Value =
        serde_yaml_ng::from_str(stdout.trim()).expect("stdout should be valid YAML");
}
