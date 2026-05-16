//! Contract tests for CLIPS session lifecycle (T026, T031b) — FR-014
//!
//! Tests: clips session create, list, destroy.
//! Session state is per-process, so the full lifecycle test creates, lists,
//! destroys, and verifies within a single invocation sequence.

use assert_cmd::Command;

fn cli() -> Command {
    Command::cargo_bin("nxuskit-cli").unwrap()
}

// ═══════════════════════════════════════════════════════════════════
// T026: CLIPS session lifecycle
// ═══════════════════════════════════════════════════════════════════

/// T026(a): `clips session create --json` returns exit 0 with session_id and created_at.
#[test]
#[ignore = "requires CLIPS runtime"]
fn clips_session_create_returns_session_id() {
    let output = cli()
        .args(["clips", "session", "create", "--json"])
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

    let session_id = resp["result"]["session_id"]
        .as_str()
        .expect("session_id should be a string");
    assert!(!session_id.is_empty(), "session_id should be non-empty");

    let created_at = resp["result"]["created_at"]
        .as_str()
        .expect("created_at should be a string");
    assert!(
        created_at.len() >= 10,
        "created_at should be ISO 8601: {created_at}"
    );
}

/// T026(d): `clips session destroy invalid_id_xyz --json` returns exit 5.
#[test]
fn clips_session_destroy_invalid_id_returns_exit5() {
    let output = cli()
        .args(["clips", "session", "destroy", "invalid_id_xyz", "--json"])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(5),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    let err: serde_json::Value = serde_json::from_str(stderr.trim()).expect("valid JSON error");
    assert_eq!(err["code"], "validation", "error code should be validation");
}

/// T026: session list returns empty array when no sessions exist.
#[test]
fn clips_session_list_empty() {
    let output = cli()
        .args(["clips", "session", "list", "--json"])
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

    let sessions = resp["result"]["sessions"]
        .as_array()
        .expect("sessions should be an array");
    assert_eq!(
        sessions.len(),
        0,
        "no sessions should exist in fresh process"
    );

    assert!(
        resp["result"]["tier"].is_string(),
        "tier should be a string"
    );
    assert!(
        resp["result"]["count"].is_number(),
        "count should be a number"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Wiring: clips session subcommand is reachable
// ═══════════════════════════════════════════════════════════════════

#[test]
fn clips_session_help_reachable() {
    let output = cli().args(["clips", "session", "--help"]).output().unwrap();

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("create"), "help should mention create");
    assert!(stdout.contains("list"), "help should mention list");
    assert!(stdout.contains("destroy"), "help should mention destroy");
}
