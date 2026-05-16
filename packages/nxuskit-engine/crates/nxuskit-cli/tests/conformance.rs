//! Shell contract conformance tests (T016, T036) — FR-003, FR-010
//!
//! Verifies all commands honor --input -, --format json|yaml, --quiet,
//! stable exit codes, and trace envelope fields.

use assert_cmd::Command;

fn cli() -> Command {
    Command::cargo_bin("nxuskit-cli").unwrap()
}

// ── Any command --format json → valid jq-parseable output ───────────

#[test]
fn call_format_json_is_valid_json() {
    let mut cmd = cli();
    cmd.args(["call", "--input", "-", "--format", "json"])
        .write_stdin(r#"{"prompt":"conformance","provider":"loopback","model":"echo"}"#);

    let output = cmd.output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let _: serde_json::Value = serde_json::from_str(stdout.trim()).expect("must be valid JSON");
}

// ── Any command failure → stderr JSON with code, message ────────────

#[test]
fn failure_produces_stderr_json_with_code_and_message() {
    let output = cli()
        .args([
            "call",
            "--input",
            "/tmp/nxuskit_conformance_nonexistent.json",
        ])
        .output()
        .unwrap();

    assert_ne!(output.status.code(), Some(0));
    let stderr = String::from_utf8_lossy(&output.stderr);
    let err: serde_json::Value = serde_json::from_str(stderr.trim()).expect("stderr must be JSON");
    assert!(err["code"].is_string());
    assert!(err["message"].is_string());
}

// ── --quiet suppresses extra output ─────────────────────────────────

#[test]
fn quiet_flag_suppresses_non_result_output() {
    let mut cmd = cli();
    cmd.args(["call", "--input", "-", "--format", "json", "--quiet"])
        .write_stdin(r#"{"prompt":"quiet test","provider":"loopback","model":"echo"}"#);

    let output = cmd.output().unwrap();
    assert_eq!(output.status.code(), Some(0));

    // stderr should be empty in quiet mode
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.trim().is_empty(),
        "stderr should be empty in quiet mode, got: {}",
        stderr
    );

    // stdout should still have the result
    let stdout = String::from_utf8_lossy(&output.stdout);
    let _: serde_json::Value = serde_json::from_str(stdout.trim()).expect("stdout should be JSON");
}

// ── --input - works identically to file input for multiple commands ──

#[test]
fn stdin_and_file_input_produce_equivalent_results() {
    let dir = std::env::temp_dir().join("nxuskit_conformance_test");
    std::fs::create_dir_all(&dir).unwrap();
    let input_path = dir.join("input.json");
    let input_json = r#"{"prompt":"equivalence test","provider":"loopback","model":"echo"}"#;
    std::fs::write(&input_path, input_json).unwrap();

    // Via file
    let file_output = cli()
        .args([
            "call",
            "--input",
            input_path.to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    // Via stdin
    let mut stdin_cmd = cli();
    stdin_cmd
        .args(["call", "--input", "-", "--format", "json"])
        .write_stdin(input_json);
    let stdin_output = stdin_cmd.output().unwrap();

    assert_eq!(file_output.status.code(), Some(0));
    assert_eq!(stdin_output.status.code(), Some(0));

    // Both should produce valid JSON with same structure
    let file_resp: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&file_output.stdout).trim()).unwrap();
    let stdin_resp: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&stdin_output.stdout).trim()).unwrap();

    // Same result structure (content may differ due to trace_id/timestamp)
    assert!(file_resp["result"]["content"].is_string());
    assert!(stdin_resp["result"]["content"].is_string());
    assert_eq!(
        file_resp["request_metadata"]["command"],
        stdin_resp["request_metadata"]["command"]
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ── --format yaml → valid YAML ──────────────────────────────────────

#[test]
fn format_yaml_produces_valid_yaml() {
    let mut cmd = cli();
    cmd.args(["call", "--input", "-", "--format", "yaml"])
        .write_stdin(r#"{"prompt":"yaml test","provider":"loopback","model":"echo"}"#);

    let output = cmd.output().unwrap();
    assert_eq!(output.status.code(), Some(0));

    let stdout = String::from_utf8_lossy(&output.stdout);
    let _: serde_json::Value =
        serde_yaml_ng::from_str(stdout.trim()).expect("stdout should be valid YAML");
}

// ── --output to file works ──────────────────────────────────────────

#[test]
fn output_to_file_writes_result() {
    let dir = std::env::temp_dir().join("nxuskit_conformance_output");
    std::fs::create_dir_all(&dir).unwrap();
    let output_path = dir.join("result.json");

    let mut cmd = cli();
    cmd.args([
        "call",
        "--input",
        "-",
        "--format",
        "json",
        "--output",
        output_path.to_str().unwrap(),
    ])
    .write_stdin(r#"{"prompt":"output test","provider":"loopback","model":"echo"}"#);

    let result = cmd.output().unwrap();
    assert_eq!(
        result.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    // File should exist and contain valid JSON
    let content = std::fs::read_to_string(&output_path).expect("output file should exist");
    let _: serde_json::Value =
        serde_json::from_str(content.trim()).expect("file should be valid JSON");

    let _ = std::fs::remove_dir_all(&dir);
}

// ── T036: Trace envelope fields present in all JSON commands ────────

fn assert_trace_fields(resp: &serde_json::Value, expected_command: &str) {
    let trace_id = resp["trace_id"].as_str().expect("trace_id must be string");
    assert!(!trace_id.is_empty(), "trace_id must be non-empty");
    assert!(trace_id.contains('-'), "trace_id should be UUID format");

    let timestamp = resp["timestamp"]
        .as_str()
        .expect("timestamp must be string");
    assert!(timestamp.contains('T'), "timestamp must be RFC 3339");
    assert!(timestamp.ends_with('Z'), "timestamp must be UTC");

    assert!(
        resp["request_hash"].is_string(),
        "request_hash must be string"
    );
    assert_eq!(resp["request_metadata"]["command"], expected_command);
}

#[test]
fn trace_fields_present_in_call() {
    let mut cmd = cli();
    cmd.args(["call", "--input", "-", "--format", "json"])
        .write_stdin(r#"{"prompt":"trace","provider":"loopback","model":"echo"}"#);
    let output = cmd.output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    let resp: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim()).unwrap();
    assert_trace_fields(&resp, "call");
}

#[test]
fn trace_fields_present_in_clips_eval() {
    let mut cmd = cli();
    cmd.args(["clips", "eval", "--input", "-", "--format", "json"])
        .write_stdin(r#"{"rules":"(defrule x (initial-fact) =>)","facts":[],"queries":[]}"#);
    let output = cmd.output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    let resp: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim()).unwrap();
    assert_trace_fields(&resp, "clips_eval");
}

#[test]
fn trace_fields_present_in_bn_infer() {
    let input = r#"{
        "network": {
            "nodes": [{"name": "A", "states": ["t", "f"]}],
            "edges": [],
            "cpds": {"A": {"probabilities": [0.3, 0.7]}}
        },
        "evidence": {},
        "query_nodes": ["A"]
    }"#;
    let mut cmd = cli();
    cmd.args(["bn", "infer", "--input", "-", "--format", "json"])
        .write_stdin(input);
    let output = cmd.output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    let resp: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim()).unwrap();
    assert_trace_fields(&resp, "bn_infer");
}
