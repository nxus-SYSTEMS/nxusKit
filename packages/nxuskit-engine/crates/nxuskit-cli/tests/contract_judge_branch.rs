//! Contract tests for judge and branch commands (T045) — FR-012

use assert_cmd::Command;

fn cli() -> Command {
    Command::cargo_bin("nxuskit-cli").unwrap()
}

// ── (a) branch fork with 2 loopback models → 2 results ─────────────

#[test]
fn branch_fork_returns_two_results() {
    let input = r#"{
        "prompt": "hello",
        "models": ["echo", "echo"],
        "provider": "loopback"
    }"#;

    let mut cmd = cli();
    cmd.args(["branch", "fork", "--input", "-", "--format", "json"])
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

    let results = resp["result"]["results"].as_array().expect("results array");
    assert_eq!(results.len(), 2);
    for r in results {
        assert!(r["model"].is_string());
        assert!(r["content"].is_string());
        assert!(r["elapsed_ms"].is_number());
    }
}

// ── (b) branch compare with fork output ─────────────────────────────

#[test]
fn branch_compare_with_fork_output() {
    // First fork
    let fork_input = r#"{
        "prompt": "test",
        "models": ["echo", "echo"],
        "provider": "loopback"
    }"#;

    let mut fork_cmd = cli();
    fork_cmd
        .args(["branch", "fork", "--input", "-", "--format", "json"])
        .write_stdin(fork_input);
    let fork_output = fork_cmd.output().unwrap();
    assert_eq!(fork_output.status.code(), Some(0));

    let fork_stdout = String::from_utf8_lossy(&fork_output.stdout);
    let fork_resp: serde_json::Value = serde_json::from_str(fork_stdout.trim()).unwrap();

    // Extract just the result for compare input
    let compare_input = serde_json::json!({
        "results": fork_resp["result"]["results"]
    });

    let mut compare_cmd = cli();
    compare_cmd
        .args(["branch", "compare", "--input", "-", "--format", "json"])
        .write_stdin(compare_input.to_string());

    let compare_output = compare_cmd.output().unwrap();
    assert_eq!(
        compare_output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&compare_output.stderr)
    );

    let compare_stdout = String::from_utf8_lossy(&compare_output.stdout);
    let resp: serde_json::Value = serde_json::from_str(compare_stdout.trim()).expect("valid JSON");
    assert!(resp["result"]["comparison"].is_array());
    assert!(resp["result"]["diffs"].is_array());
}

// ═══════════════════════════════════════════════════════════════════
// Phase 10 — Judge/Branch semantic tests (T037–T038)
// ═══════════════════════════════════════════════════════════════════

// ── T038: Branch compare returns structural diffs, not just length ──

#[test]
fn branch_compare_returns_structural_diffs() {
    // First fork
    let fork_input = r#"{
        "prompt": "test structural diff",
        "models": ["echo", "echo"],
        "provider": "loopback"
    }"#;

    let mut fork_cmd = cli();
    fork_cmd
        .args(["branch", "fork", "--input", "-", "--format", "json"])
        .write_stdin(fork_input);
    let fork_output = fork_cmd.output().unwrap();
    assert_eq!(fork_output.status.code(), Some(0));

    let fork_stdout = String::from_utf8_lossy(&fork_output.stdout);
    let fork_resp: serde_json::Value = serde_json::from_str(fork_stdout.trim()).unwrap();

    let compare_input = serde_json::json!({
        "results": fork_resp["result"]["results"]
    });

    let mut compare_cmd = cli();
    compare_cmd
        .args(["branch", "compare", "--input", "-", "--format", "json"])
        .write_stdin(compare_input.to_string());

    let compare_output = compare_cmd.output().unwrap();
    assert_eq!(
        compare_output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&compare_output.stderr)
    );

    let compare_stdout = String::from_utf8_lossy(&compare_output.stdout);
    let resp: serde_json::Value = serde_json::from_str(compare_stdout.trim()).expect("valid JSON");

    let diffs = resp["result"]["diffs"].as_array().expect("diffs array");
    // Should have multiple structural diff fields, not just content_length
    let diff_fields: Vec<&str> = diffs.iter().filter_map(|d| d["field"].as_str()).collect();
    assert!(
        diff_fields.contains(&"content_length"),
        "diffs should include content_length"
    );
    assert!(
        diff_fields.contains(&"word_count"),
        "diffs should include word_count"
    );
    assert!(
        diff_fields.contains(&"sentence_count"),
        "diffs should include sentence_count"
    );
    assert!(
        diff_fields.contains(&"elapsed_ms"),
        "diffs should include elapsed_ms"
    );
    assert!(
        diff_fields.contains(&"content_similarity"),
        "diffs should include content_similarity"
    );
}

// ── (c) judge select with candidates (loopback echoes prompt, parse fails) ──

#[test]
fn judge_select_parse_failure_returns_error() {
    // T037: Judge parse failure returns CliError::ParseError instead of silent fallback
    let input = r#"{
        "candidates": [
            {"id": "a", "content": "The sky is blue."},
            {"id": "b", "content": "Water is wet."}
        ],
        "provider": "loopback",
        "model": "echo"
    }"#;

    let mut cmd = cli();
    cmd.args(["judge", "select", "--input", "-", "--format", "json"])
        .write_stdin(input);

    let output = cmd.output().unwrap();
    // Loopback echoes the prompt which is not valid judge JSON, so parse error
    assert_eq!(
        output.status.code(),
        Some(5),
        "Expected exit 5 for validation error"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    let err: serde_json::Value = serde_json::from_str(stderr.trim()).expect("stderr JSON");
    assert_eq!(err["code"], "validation");
}
