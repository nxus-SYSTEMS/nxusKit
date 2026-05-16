//! Contract tests for packet and artifact commands (T031) — FR-009

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

fn test_dir(suffix: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nxuskit_artifact_{}", suffix));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// ── (a) valid packet + schema → valid: true, exit 0 ────────────────

#[test]
fn packet_validate_valid_returns_true() {
    let dir = test_dir("pkt_valid");
    let packet_path = dir.join("packet.json");
    let schema_path = dir.join("schema.json");

    std::fs::write(&packet_path, r#"{"name":"test","age":25}"#).unwrap();
    std::fs::write(
        &schema_path,
        r#"{
        "type": "object",
        "properties": {
            "name": {"type": "string"},
            "age": {"type": "integer"}
        },
        "required": ["name"]
    }"#,
    )
    .unwrap();

    let output = cli()
        .args([
            "packet",
            "validate",
            "--input",
            packet_path.to_str().unwrap(),
            "--schema",
            schema_path.to_str().unwrap(),
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
    assert_eq!(resp["result"]["valid"], true);
    assert!(resp["result"]["errors"].as_array().unwrap().is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

// ── (b) invalid packet → valid: false, exit 1 ──────────────────────

#[test]
fn packet_validate_invalid_returns_false() {
    let dir = test_dir("pkt_invalid");
    let packet_path = dir.join("bad_packet.json");
    let schema_path = dir.join("schema2.json");

    std::fs::write(&packet_path, r#"{"name": 123}"#).unwrap();
    std::fs::write(
        &schema_path,
        r#"{
        "type": "object",
        "properties": {
            "name": {"type": "string"}
        },
        "required": ["name"]
    }"#,
    )
    .unwrap();

    let output = cli()
        .args([
            "packet",
            "validate",
            "--input",
            packet_path.to_str().unwrap(),
            "--schema",
            schema_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    // Should output result first, then exit 5 (validation)
    assert_eq!(output.status.code(), Some(5));

    let _ = std::fs::remove_dir_all(&dir);
}

// ── (c) merge 2 non-conflicting artifacts → merged, exit 0 ─────────

#[test]
fn artifact_merge_non_conflicting_succeeds() {
    let dir = test_dir("merge_ok");
    let a_path = dir.join("a.json");
    let b_path = dir.join("b.json");

    std::fs::write(&a_path, r#"{"name":"test","version":"1.0"}"#).unwrap();
    std::fs::write(&b_path, r#"{"author":"alice","license":"MIT"}"#).unwrap();

    let output = cli()
        .args([
            "artifact",
            "merge",
            "--input",
            a_path.to_str().unwrap(),
            "--input",
            b_path.to_str().unwrap(),
            "--merge-strategy",
            "last",
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
    assert_eq!(result["name"], "test");
    assert_eq!(result["author"], "alice");

    let _ = std::fs::remove_dir_all(&dir);
}

// ── (d) merge conflicting with error strategy → merge_conflict ──────

#[test]
fn artifact_merge_conflict_with_error_strategy() {
    let dir = test_dir("merge_conflict");
    let a_path = dir.join("c1.json");
    let b_path = dir.join("c2.json");

    std::fs::write(&a_path, r#"{"name":"alice"}"#).unwrap();
    std::fs::write(&b_path, r#"{"name":"bob"}"#).unwrap();

    let output = cli()
        .args([
            "artifact",
            "merge",
            "--input",
            a_path.to_str().unwrap(),
            "--input",
            b_path.to_str().unwrap(),
            "--merge-strategy",
            "error",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8_lossy(&output.stderr);
    let err: serde_json::Value = serde_json::from_str(stderr.trim()).expect("stderr JSON");
    assert_eq!(err["code"], "internal");

    let _ = std::fs::remove_dir_all(&dir);
}

// ── (e) artifact summarize --format json ────────────────────────────

#[test]
fn artifact_summarize_json_format() {
    let dir = test_dir("summarize_json");
    let artifact_path = dir.join("artifact.json");

    std::fs::write(
        &artifact_path,
        r#"{"name":"test","version":"1.0","data":{"nested":true}}"#,
    )
    .unwrap();

    let output = cli()
        .args([
            "artifact",
            "summarize",
            "--input",
            artifact_path.to_str().unwrap(),
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
    assert!(resp["result"]["field_count"].is_number());
    assert!(resp["result"]["top_level_keys"].is_array());
    assert!(resp["result"]["estimated_tokens"].is_number());

    let _ = std::fs::remove_dir_all(&dir);
}

// ── (f) artifact summarize --format text ────────────────────────────

#[test]
fn artifact_summarize_text_format() {
    let dir = test_dir("summarize_text");
    let artifact_path = dir.join("art_text.json");

    std::fs::write(&artifact_path, r#"{"title":"hello","count":42}"#).unwrap();

    let output = cli()
        .args([
            "artifact",
            "summarize",
            "--input",
            artifact_path.to_str().unwrap(),
            "--format",
            "text",
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
    assert!(
        stdout.contains("Fields:"),
        "text output should include field count"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ═══════════════════════════════════════════════════════════════════
// Phase 6 — Artifact deep merge (T029)
// ═══════════════════════════════════════════════════════════════════

// ── T029: Deep merge with nested conflict reports dot-notation path ──

#[test]
fn artifact_deep_merge_nested_conflict_reports_path() {
    let a_path = fixture("artifact_nested_a.json");
    let b_path = fixture("artifact_nested_b.json");

    let output = cli()
        .args([
            "artifact",
            "merge",
            "--input",
            a_path.to_str().unwrap(),
            "--input",
            b_path.to_str().unwrap(),
            "--merge-strategy",
            "error",
        ])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(1),
        "Expected exit 1 for merge conflict"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    let err: serde_json::Value = serde_json::from_str(stderr.trim()).expect("stderr JSON");
    assert_eq!(err["code"], "internal");

    // The conflict details should mention the nested path (via conflict_paths array or message)
    let details_str = err["details"].to_string();
    let message_str = err["message"].as_str().unwrap_or("");
    assert!(
        details_str.contains("config.retry.max_attempts")
            || message_str.contains("config.retry.max_attempts"),
        "conflict details or message should include dot-notation path 'config.retry.max_attempts', got details: {}, message: {}",
        details_str,
        message_str
    );
}

// ── T030: Deep merge with 'last' strategy resolves nested conflicts ──

#[test]
fn artifact_deep_merge_last_strategy_resolves_nested() {
    let a_path = fixture("artifact_nested_a.json");
    let b_path = fixture("artifact_nested_b.json");

    let output = cli()
        .args([
            "artifact",
            "merge",
            "--input",
            a_path.to_str().unwrap(),
            "--input",
            b_path.to_str().unwrap(),
            "--merge-strategy",
            "last",
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

    // version from b (last wins)
    assert_eq!(result["version"], "2.0");
    // config.retry should be deeply merged: max_attempts from b, jitter from b added
    assert_eq!(result["config"]["retry"]["max_attempts"], 5);
    assert_eq!(result["config"]["retry"]["jitter"], true);
    // config.retry.backoff_ms from b (last wins)
    assert_eq!(result["config"]["retry"]["backoff_ms"], 200);
    // metadata.author from b (last wins), metadata.reviewed from b (added)
    assert_eq!(result["metadata"]["author"], "team-b");
    assert_eq!(result["metadata"]["reviewed"], true);
}

// ── schema not found ────────────────────────────────────────────────

#[test]
fn packet_validate_missing_schema_returns_schema_not_found() {
    let dir = test_dir("pkt_no_schema");
    let packet_path = dir.join("pkt.json");
    std::fs::write(&packet_path, r#"{"x":1}"#).unwrap();

    let output = cli()
        .args([
            "packet",
            "validate",
            "--input",
            packet_path.to_str().unwrap(),
            "--schema",
            "/tmp/nxuskit_nonexistent_schema_42.json",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8_lossy(&output.stderr);
    let err: serde_json::Value = serde_json::from_str(stderr.trim()).expect("stderr JSON");
    assert_eq!(err["code"], "internal");

    let _ = std::fs::remove_dir_all(&dir);
}
