//! Integration tests for `provider ping` command (US3, T039-T042).

use assert_cmd::Command;

fn cli() -> Command {
    Command::cargo_bin("nxuskit-cli").unwrap()
}

// ── T039: ping loopback reachable ───────────────────────────────────

#[test]
fn test_provider_ping_loopback_reachable() {
    let output = cli()
        .args(["provider", "ping", "--provider", "loopback", "--json"])
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
    assert_eq!(resp["reachable"], true);
    assert!(resp["latency_ms"].is_number(), "should have latency_ms");
    let latency = resp["latency_ms"].as_f64().unwrap();
    assert!(latency >= 0.0, "latency must be non-negative");
}

// ── T040: ping unknown provider ─────────────────────────────────────

#[test]
fn test_provider_ping_unknown_provider() {
    let output = cli()
        .args(["provider", "ping", "--provider", "nonexistent", "--json"])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(1),
        "unknown provider should exit 1"
    );
}

// ── T041: ping text format ──────────────────────────────────────────

#[test]
fn test_provider_ping_text_format() {
    let output = cli()
        .args([
            "provider",
            "ping",
            "--provider",
            "loopback",
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
        stdout.contains("loopback"),
        "text output should contain provider name"
    );
    assert!(
        stdout.contains("reachable"),
        "text output should mention reachability"
    );
    // Should NOT be valid JSON (it's text format)
    assert!(
        serde_json::from_str::<serde_json::Value>(stdout.trim()).is_err(),
        "text format should not be JSON"
    );
}

// ── T042: ping JSON format ──────────────────────────────────────────

#[test]
fn test_provider_ping_json_format() {
    let output = cli()
        .args(["provider", "ping", "--provider", "loopback", "--json"])
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
    assert!(resp["provider"].is_string(), "should have provider field");
    assert!(
        resp["reachable"].is_boolean(),
        "should have reachable field"
    );
    assert!(
        resp["latency_ms"].is_number(),
        "should have latency_ms field"
    );
}
