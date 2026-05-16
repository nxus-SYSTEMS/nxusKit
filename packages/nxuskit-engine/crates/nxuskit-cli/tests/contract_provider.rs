//! Contract tests for models and provider commands (T049) — FR-013, FR-014
//!
//! Note: The existing `models` and `provider` commands in main.rs handle
//! some of this functionality. These tests verify the L1 wiring works.

use assert_cmd::Command;

fn cli() -> Command {
    Command::cargo_bin("nxuskit-cli").unwrap()
}

// ── Existing models command still works ─────────────────────────────

#[test]
fn existing_models_command_works_with_json() {
    let output = cli()
        .args(["models", "--provider", "loopback", "--format", "json"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));

    let stdout = String::from_utf8_lossy(&output.stdout);
    let resp: serde_json::Value = serde_json::from_str(stdout.trim()).expect("valid JSON");
    assert!(
        resp["result"]["models"].is_array(),
        "result.models should be an array"
    );
}

// ── Existing provider status works ──────────────────────────────────

#[test]
fn existing_provider_status_works() {
    let output = cli()
        .args(["provider", "status", "--json"])
        .output()
        .unwrap();

    // Should not crash
    assert!(output.status.code().unwrap() <= 1);
}

// ═══════════════════════════════════════════════════════════════════
// Phase 7 — Models capability inference (T031)
// ═══════════════════════════════════════════════════════════════════

// ── T031: models with loopback returns non-empty supports arrays ────

#[test]
fn models_loopback_has_non_empty_supports() {
    let output = cli()
        .args(["models", "--provider", "loopback", "--format", "json"])
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

    let models = resp["result"]["models"].as_array().expect("models array");
    assert!(!models.is_empty(), "should have at least one model");

    for model in models {
        let supports = model["supports"].as_array().expect("supports array");
        assert!(
            !supports.is_empty(),
            "supports for model '{}' should be non-empty",
            model["id"]
        );
    }
}

// ── T032b: provider status returns structured auth fields ───────────

#[test]
fn provider_status_returns_structured_auth_fields() {
    // Query all providers (no specific provider arg) to get structured output
    let output = cli()
        .args(["provider", "status", "--json"])
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

    // Status output is an array of provider auth entries
    let entries = resp.as_array().expect("response should be a JSON array");
    assert!(
        !entries.is_empty(),
        "should have at least one provider entry"
    );

    // Each entry should have structured auth fields
    let entry = &entries[0];
    assert!(
        entry["provider_id"].is_string(),
        "entry should have provider_id"
    );
    assert!(
        entry["status"].is_string(),
        "entry should have status string"
    );
}

// ── Invalid provider for models returns error ───────────────────────

#[test]
fn models_invalid_provider_returns_error() {
    let output = cli()
        .args(["models", "--provider", "nonexistent_xyz"])
        .output()
        .unwrap();

    assert_ne!(output.status.code(), Some(0));
}

// ═══════════════════════════════════════════════════════════════════
// Level 2: `provider list` / `provider info` (T021) - FR-001, SC-001
// ═══════════════════════════════════════════════════════════════════

// ── provider list success path ──────────────────────────────────────

#[test]
fn provider_list_succeeds_and_returns_provider_array() {
    let output = cli()
        .args(["provider", "list", "--format", "json"])
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
    let providers = resp["result"]["providers"]
        .as_array()
        .expect("providers array");
    assert!(
        !providers.is_empty(),
        "provider list must return at least one provider"
    );
    assert!(
        providers
            .iter()
            .any(|p| p["name"] == "xai" && p["display_name"] == "xAI Grok"),
        "provider list must expose xAI Grok under canonical id xai"
    );
    assert!(
        !providers.iter().any(|p| p["name"] == "grok"),
        "provider list must not expose confusing grok alias"
    );
}

// ── provider list failure path (bad --format) ───────────────────────

#[test]
fn provider_list_bad_format_returns_validation_exit5() {
    let output = cli()
        .args(["provider", "list", "--format", "xml"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(5), "bad --format must exit 5");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let err: serde_json::Value = serde_json::from_str(stderr.trim()).expect("valid JSON");
    assert_eq!(err["code"], "validation");
}

// ── provider info success path ──────────────────────────────────────

#[test]
fn provider_info_known_provider_succeeds() {
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
    assert!(
        resp["result"].is_object(),
        "provider info must return a structured result object"
    );
}

// ── provider info failure path (unknown provider) ───────────────────

#[test]
fn provider_info_unknown_provider_returns_validation_exit5() {
    let output = cli()
        .args(["provider", "info", "definitely_not_a_provider", "--json"])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(5),
        "unknown provider must exit 5"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    let err: serde_json::Value = serde_json::from_str(stderr.trim()).expect("valid JSON");
    assert_eq!(err["code"], "validation");
    assert!(
        err["trace_id"].is_string(),
        "error envelope must carry trace_id"
    );
}
