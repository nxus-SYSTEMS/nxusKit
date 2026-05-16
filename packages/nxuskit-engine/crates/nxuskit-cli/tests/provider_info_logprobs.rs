//! Phase C binary smoke tests (T042–T045) — CLI `provider info` streaming-logprobs disclosure.
//!
//! All tests invoke the compiled `nxuskit-cli` binary via `assert_cmd` (Article II
//! binary-level requirement). No API key or network call is required.

use assert_cmd::Command;

fn cli() -> Command {
    Command::cargo_bin("nxuskit-cli").unwrap()
}

// ── T042: CLI-1 — openai JSON has streaming_logprobs: true ─────────────────

#[test]
fn cli_provider_info_openai_json_has_streaming_logprobs_true() {
    let output = cli()
        .args(["provider", "info", "openai", "--format", "json"])
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

    assert_eq!(
        resp["result"]["capabilities"]["streaming_logprobs"],
        serde_json::Value::Bool(true),
        "openai must report streaming_logprobs: true"
    );
}

// ── T043: CLI-2 — anthropic (claude) JSON has streaming_logprobs: false ────
//
// Note: the CLI provider identifier for Anthropic is "claude" (not "anthropic").
// The test uses "claude" per the known_providers() table. The spec says "anthropic"
// but the CLI canonical name is "claude". This mismatch is documented here.

#[test]
fn cli_provider_info_anthropic_json_has_streaming_logprobs_false() {
    let output = cli()
        .args(["provider", "info", "claude", "--format", "json"])
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

    assert_eq!(
        resp["result"]["capabilities"]["streaming_logprobs"],
        serde_json::Value::Bool(false),
        "claude (anthropic) must report streaming_logprobs: false"
    );
}

// ── T044: CLI-3 — mock JSON has streaming_logprobs: false ──────────────────

#[test]
fn cli_provider_info_mock_json_has_streaming_logprobs_false() {
    let output = cli()
        .args(["provider", "info", "mock", "--format", "json"])
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

    assert_eq!(
        resp["result"]["capabilities"]["streaming_logprobs"],
        serde_json::Value::Bool(false),
        "mock must report streaming_logprobs: false"
    );
}

// ── T045: CLI-4 — openai human format contains "streaming logprobs : yes" ──

#[test]
fn cli_provider_info_openai_human_has_streaming_logprobs_row() {
    let output = cli()
        .args(["provider", "info", "openai", "--format", "human"])
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
        stdout.contains("streaming logprobs : yes")
            || stdout.contains("streaming logprobs : true")
            || stdout.contains("streaming_logprobs\": true"),
        "human output must contain a streaming logprobs row with affirmative value; got:\n{stdout}"
    );
}
