//! Level 2 integration tests for `call` command — v0.9.2 features.
//!
//! Tests written FIRST per Article III TDD.

use assert_cmd::Command;

fn cli() -> Command {
    Command::cargo_bin("nxuskit-cli").unwrap()
}

/// Helper: send call input JSON and return parsed response envelope.
fn call_loopback(input: &str) -> serde_json::Value {
    let output = cli()
        .args(["call", "--input", "-", "--format", "json"])
        .write_stdin(input)
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(stdout.trim()).expect("valid JSON response envelope")
}

// ── T014: call with tool_choice ────────────────────────────��────────

#[test]
fn test_call_with_tool_choice() {
    let input = r#"{
        "prompt": "What is 2+2?",
        "provider": "loopback",
        "model": "echo",
        "tool_choice": "required",
        "tool_definitions": [{"type":"function","function":{"name":"calc","description":"calc"}}]
    }"#;
    let resp = call_loopback(input);
    assert!(
        resp["result"]["content"].is_string(),
        "response should have content"
    );
    assert!(
        resp["result"]["inference_metadata"].is_object(),
        "response should have inference_metadata"
    );
}

// ── T015: call with response_format ─────────────────────────────────

#[test]
fn test_call_with_response_format() {
    let input = r#"{
        "prompt": "return JSON",
        "provider": "loopback",
        "model": "echo",
        "response_format": {"type": "json_object"}
    }"#;
    let resp = call_loopback(input);
    assert!(
        resp["result"]["content"].is_string(),
        "call with response_format should succeed"
    );
}

// ── T016: call with thinking_mode ───────────────────────────────────

#[test]
fn test_call_with_thinking_mode() {
    let input = r#"{
        "prompt": "think about this",
        "provider": "loopback",
        "model": "echo",
        "thinking_mode": "enabled"
    }"#;
    let resp = call_loopback(input);
    assert!(
        resp["result"]["content"].is_string(),
        "call with thinking_mode should succeed"
    );
}

// ── T017: unknown fields ignored ────────────────────────────────────

#[test]
fn test_call_unknown_fields_ignored() {
    let input = r#"{
        "prompt": "test",
        "provider": "loopback",
        "model": "echo",
        "future_field": 42,
        "another_unknown": "value"
    }"#;
    let resp = call_loopback(input);
    assert!(
        resp["result"]["content"].is_string(),
        "unknown fields should be ignored"
    );
}

// ── T018: parameter warning in envelope ─────────────────────────────

#[test]
fn test_call_parameter_warning_in_envelope() {
    // Loopback provider may or may not emit warnings for response_format.
    // We verify the call succeeds and the envelope structure is valid.
    // If warnings are present, they should be an array.
    let input = r#"{
        "prompt": "test",
        "provider": "loopback",
        "model": "echo",
        "response_format": {"type": "json_object"}
    }"#;
    let resp = call_loopback(input);
    assert!(resp["result"]["content"].is_string());
    // warnings field may or may not be present — if present, must be an array
    if let Some(warnings) = resp["result"].get("warnings") {
        assert!(warnings.is_array(), "warnings must be an array if present");
    }
}

// ══════════════════════════════════════════════════════════════════════
// Phase 4 — User Story 2: Vision/Image Input
// ══════════════════════════════════════════════════════════════════════

// ── T028: call with multimodal messages ─────────────────────────────

#[test]
fn test_call_with_multimodal_messages() {
    let input = r#"{
        "provider": "loopback",
        "model": "echo",
        "messages": [
            {
                "role": "user",
                "content": [
                    {"type": "text", "text": "What is in this image?"},
                    {"type": "image", "url": "https://example.com/photo.jpg"}
                ]
            }
        ]
    }"#;
    let resp = call_loopback(input);
    assert!(
        resp["result"]["content"].is_string(),
        "multimodal call should succeed"
    );
}

// ── T029: call with --image-url flag ────────────────────────────────

#[test]
fn test_call_image_url_flag() {
    let output = cli()
        .args([
            "call",
            "--input",
            "-",
            "--image-url",
            "https://example.com/photo.jpg",
        ])
        .write_stdin(r#"{"prompt":"describe this","provider":"loopback","model":"echo"}"#)
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
}

// ── T030: call with --image-file flag ───────────────────────────────

#[test]
fn test_call_image_file_flag() {
    let dir = std::env::temp_dir().join("nxuskit_test_imgfile");
    std::fs::create_dir_all(&dir).unwrap();
    let img_path = dir.join("test.png");
    // Write a minimal PNG header (just needs to be a valid file for the path/extension check)
    std::fs::write(&img_path, [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]).unwrap();

    let output = cli()
        .args([
            "call",
            "--input",
            "-",
            "--image-file",
            img_path.to_str().unwrap(),
        ])
        .write_stdin(r#"{"prompt":"describe","provider":"loopback","model":"echo"}"#)
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

    let _ = std::fs::remove_dir_all(&dir);
}

// ── T031: --image-file too large ────────────────────────────────────

#[test]
fn test_call_image_file_too_large() {
    let dir = std::env::temp_dir().join("nxuskit_test_imglarge");
    std::fs::create_dir_all(&dir).unwrap();
    let img_path = dir.join("huge.png");
    // Create a file > 20MB
    let data = vec![0u8; 21 * 1024 * 1024];
    std::fs::write(&img_path, &data).unwrap();

    let output = cli()
        .args([
            "call",
            "--input",
            "-",
            "--image-file",
            img_path.to_str().unwrap(),
        ])
        .write_stdin(r#"{"prompt":"test","provider":"loopback","model":"echo"}"#)
        .output()
        .unwrap();
    assert_ne!(
        output.status.code(),
        Some(0),
        "should fail for oversized image"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("20MB") || stderr.contains("20 MB"),
        "error should mention 20MB limit: {stderr}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ── T032: --image-file stdin rejected ───────────────────────────────

#[test]
fn test_call_image_file_stdin_rejected() {
    let output = cli()
        .args(["call", "--input", "-", "--image-file", "-"])
        .write_stdin(r#"{"prompt":"test","provider":"loopback","model":"echo"}"#)
        .output()
        .unwrap();
    assert_ne!(
        output.status.code(),
        Some(0),
        "stdin should be rejected for --image-file"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("stdin"),
        "error should mention stdin: {stderr}"
    );
}

// ── T033: --image-url and --image-file mutually exclusive ───────────

#[test]
fn test_call_image_url_and_file_mutually_exclusive() {
    let output = cli()
        .args([
            "call",
            "--input",
            "-",
            "--image-url",
            "https://example.com/photo.jpg",
            "--image-file",
            "/tmp/test.png",
        ])
        .write_stdin(r#"{"prompt":"test","provider":"loopback","model":"echo"}"#)
        .output()
        .unwrap();
    assert_ne!(output.status.code(), Some(0), "should reject both flags");
}

// ── T033a: --image-file unknown extension ───────────────────────────

// ══════════════════════════════════════════════════════════════════════
// Phase 9 — User Story 7: CLI Conformance Validation
// ══════════════════════════════════════════════════════════════════════

// ── T067: Level 1 call unchanged after Level 2 additions ────────────
#[test]
fn test_level1_call_unchanged_after_level2() {
    // Pure L1 call: prompt, model, provider only — no L2 fields
    let input = r#"{"prompt":"hello","provider":"loopback","model":"echo"}"#;
    let resp = call_loopback(input);
    let result = &resp["result"];

    // Required L1 fields
    assert!(result["content"].is_string(), "missing content");
    assert!(result["model"].is_string(), "missing model");
    assert!(result["provider"].is_string(), "missing provider");
    assert!(
        result["inference_metadata"].is_object(),
        "missing inference_metadata"
    );
    assert!(
        result["inference_metadata"]["model"].is_string(),
        "inference_metadata missing model"
    );
    assert!(
        result["inference_metadata"]["provider"].is_string(),
        "inference_metadata missing provider"
    );

    // L2 fields should NOT appear in a pure L1 call
    assert!(
        result.get("warnings").is_none() || result["warnings"].is_null(),
        "warnings should not appear in L1-only call"
    );
}

// ── T068: exit codes for error cases ────────────────────────────────

#[test]
fn test_call_exit_codes() {
    // Invalid JSON → exit code 5 (validation) with code "validation"
    let output = cli()
        .args(["call", "--input", "-"])
        .write_stdin("{not valid json")
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(5),
        "invalid JSON should exit 5 (validation)"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    let err: serde_json::Value = serde_json::from_str(stderr.trim()).expect("stderr valid JSON");
    assert_eq!(err["code"], "validation");

    // Unknown provider → exit 1 (internal) with code "internal"
    let output2 = cli()
        .args(["call", "--input", "-"])
        .write_stdin(r#"{"prompt":"hi","provider":"nonexistent_xyz"}"#)
        .output()
        .unwrap();
    assert_eq!(
        output2.status.code(),
        Some(1),
        "unknown provider should exit 1 (internal)"
    );
    let stderr2 = String::from_utf8_lossy(&output2.stderr);
    let err2: serde_json::Value = serde_json::from_str(stderr2.trim()).expect("stderr valid JSON");
    assert_eq!(err2["code"], "internal");
}

// ── T033a: --image-file unknown extension ───────────────────────────

#[test]
fn test_call_image_file_unknown_extension() {
    let dir = std::env::temp_dir().join("nxuskit_test_imgext");
    std::fs::create_dir_all(&dir).unwrap();
    let txt_path = dir.join("data.txt");
    std::fs::write(&txt_path, b"not an image").unwrap();

    let output = cli()
        .args([
            "call",
            "--input",
            "-",
            "--image-file",
            txt_path.to_str().unwrap(),
        ])
        .write_stdin(r#"{"prompt":"test","provider":"loopback","model":"echo"}"#)
        .output()
        .unwrap();
    assert_ne!(
        output.status.code(),
        Some(0),
        "should reject unknown extension"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unrecognized") || stderr.contains("extension"),
        "error should mention unrecognized extension: {stderr}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
