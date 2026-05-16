//! Contract tests for `call` command (T011) — FR-001, FR-002
//!
//! Tests written FIRST per Article III TDD.

use assert_cmd::Command;

fn cli() -> Command {
    Command::cargo_bin("nxuskit-cli").unwrap()
}

// ── (a) Valid JSON input → valid ResponseEnvelope ───────────────────

#[test]
fn call_valid_json_input_returns_response_envelope() {
    let mut cmd = cli();
    cmd.args(["call", "--input", "-", "--format", "json"])
        .write_stdin(r#"{"prompt":"hello world","provider":"loopback","model":"echo"}"#);

    let output = cmd.output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let resp: serde_json::Value = serde_json::from_str(stdout.trim()).expect("valid JSON");

    // ResponseEnvelope required fields
    assert!(resp["trace_id"].is_string(), "missing trace_id");
    assert!(resp["timestamp"].is_string(), "missing timestamp");
    assert!(resp["request_hash"].is_string(), "missing request_hash");
    assert!(
        resp["request_metadata"].is_object(),
        "missing request_metadata"
    );
    assert!(resp["result"].is_object(), "missing result");

    // result fields
    let result = &resp["result"];
    assert!(result["content"].is_string(), "missing content");
    assert!(result["model"].is_string(), "missing model");
    assert!(result["provider"].is_string(), "missing provider");

    // request_metadata
    assert_eq!(resp["request_metadata"]["command"], "call");

    // usage should be present
    assert!(resp["usage"].is_object(), "missing usage");
    assert!(resp["finish_reason"].is_string(), "missing finish_reason");
}

// ── (b) Stdin input works identically ───────────────────────────────

#[test]
fn call_stdin_input_returns_same_envelope() {
    let mut cmd = cli();
    cmd.args(["call", "--input", "-"])
        .write_stdin(r#"{"prompt":"stdin test","provider":"loopback","model":"echo"}"#);

    let output = cmd.output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let resp: serde_json::Value = serde_json::from_str(stdout.trim()).expect("valid JSON");
    assert!(resp["result"]["content"].is_string());
}

// ── (c) Streaming JSONL mode ────────────────────────────────────────

#[test]
fn call_stream_jsonl_emits_chunk_and_summary_events() {
    let mut cmd = cli();
    cmd.args(["call", "--input", "-", "--format", "jsonl", "--stream"])
        .write_stdin(r#"{"prompt":"stream test","provider":"loopback","model":"echo"}"#);

    let output = cmd.output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.trim().lines().collect();
    assert!(!lines.is_empty(), "should have JSONL output");

    // Each line should be valid JSON
    for line in &lines {
        let event: serde_json::Value =
            serde_json::from_str(line).expect("each line should be valid JSON");
        assert!(event["type"].is_string(), "event missing type field");
    }

    // Last event should be summary
    let last: serde_json::Value = serde_json::from_str(lines.last().unwrap()).unwrap();
    assert_eq!(last["type"], "summary");
}

// ── (d) Invalid provider → JSON error, exit 1 ──────────────────────

#[test]
fn call_invalid_provider_returns_provider_error() {
    let mut cmd = cli();
    cmd.args(["call", "--input", "-"])
        .write_stdin(r#"{"prompt":"hi","provider":"nonexistent_provider_xyz"}"#);

    let output = cmd.output().unwrap();
    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8_lossy(&output.stderr);
    let err: serde_json::Value = serde_json::from_str(stderr.trim()).expect("stderr valid JSON");
    assert_eq!(err["code"], "internal");
}

// ── (e) Missing API key → JSON error, exit 1 ───────────────────────

#[test]
fn call_missing_api_key_returns_provider_error() {
    let mut cmd = cli();
    // Claude requires ANTHROPIC_API_KEY which won't be set in test env
    cmd.args(["call", "--input", "-"])
        .write_stdin(r#"{"prompt":"hi","provider":"claude","model":"claude-sonnet-4-5"}"#)
        .env_remove("ANTHROPIC_API_KEY");

    let output = cmd.output().unwrap();
    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8_lossy(&output.stderr);
    let err: serde_json::Value = serde_json::from_str(stderr.trim()).expect("stderr valid JSON");
    assert_eq!(err["code"], "internal");
}

// ── Parse error test ────────────────────────────────────────────────

#[test]
fn call_malformed_json_input_returns_parse_error() {
    let mut cmd = cli();
    cmd.args(["call", "--input", "-"])
        .write_stdin("not json at all{{{");

    let output = cmd.output().unwrap();
    assert_eq!(output.status.code(), Some(5));

    let stderr = String::from_utf8_lossy(&output.stderr);
    let err: serde_json::Value = serde_json::from_str(stderr.trim()).expect("stderr valid JSON");
    assert_eq!(err["code"], "validation");
}

// ═══════════════════════════════════════════════════════════════════
// Phase 5 — Call envelope semantic tests (T024–T025)
// ═══════════════════════════════════════════════════════════════════

// ── T024: Call with tool_definitions propagates them ────────────────

#[test]
fn call_with_tool_definitions_includes_them_in_response() {
    let input = r#"{
        "prompt": "What is 2 + 2?",
        "provider": "loopback",
        "model": "echo",
        "tool_definitions": [
            {
                "type": "function",
                "function": {
                    "name": "calculator",
                    "description": "Perform arithmetic",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "expression": {"type": "string"}
                        }
                    }
                }
            }
        ]
    }"#;

    let mut cmd = cli();
    cmd.args(["call", "--input", "-", "--format", "json"])
        .write_stdin(input);

    let output = cmd.output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let resp: serde_json::Value = serde_json::from_str(stdout.trim()).expect("valid JSON");

    // Response envelope should have result with content
    assert!(resp["result"]["content"].is_string(), "missing content");
    // tool_definitions_count should be present in result
    assert!(
        resp["result"]["tool_definitions_count"].is_number(),
        "result should include tool_definitions_count"
    );
}

// ── T025: Call response has inference_metadata ──────────────────────

#[test]
fn call_response_has_inference_metadata() {
    let input = r#"{"prompt": "hello", "provider": "loopback", "model": "echo"}"#;

    let mut cmd = cli();
    cmd.args(["call", "--input", "-", "--format", "json"])
        .write_stdin(input);

    let output = cmd.output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let resp: serde_json::Value = serde_json::from_str(stdout.trim()).expect("valid JSON");

    // result.inference_metadata should be present with model and provider fields
    let im = &resp["result"]["inference_metadata"];
    assert!(im.is_object(), "result should include inference_metadata");
    assert!(
        im["model"].is_string(),
        "inference_metadata should have model"
    );
    assert!(
        im["provider"].is_string(),
        "inference_metadata should have provider"
    );
}

// ── File input test ─────────────────────────────────────────────────

#[test]
fn call_file_input_works() {
    // Write a temp file
    let dir = std::env::temp_dir().join("nxuskit_test_call");
    std::fs::create_dir_all(&dir).unwrap();
    let input_path = dir.join("prompt.json");
    std::fs::write(
        &input_path,
        r#"{"prompt":"file test","provider":"loopback","model":"echo"}"#,
    )
    .unwrap();

    let output = cli()
        .args(["call", "--input", input_path.to_str().unwrap()])
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
    assert!(resp["result"]["content"].is_string());

    // Cleanup
    let _ = std::fs::remove_dir_all(&dir);
}
