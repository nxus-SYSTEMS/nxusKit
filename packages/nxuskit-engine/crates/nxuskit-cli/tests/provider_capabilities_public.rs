//! Binary smoke tests for Capability Manifest v2 public preview provider output.
//!
//! These tests do not use API keys or live provider calls. Model lookup errors
//! are swallowed by the CLI and only static capability metadata is asserted.

use assert_cmd::Command;

fn cli() -> Command {
    Command::cargo_bin("nxuskit-cli").unwrap()
}

#[test]
fn provider_list_json_includes_public_capability_status() {
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
    let openai = providers
        .iter()
        .find(|provider| provider["name"] == "openai")
        .expect("openai provider entry");

    assert_eq!(openai["last_reviewed_on"], "2026-05-09");
    assert_eq!(openai["provider_status"], "unknown");
    assert_eq!(
        openai["capability_status"]["json_schema_strict"],
        "supported"
    );
}

#[test]
fn provider_info_openai_json_uses_public_projection_only() {
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
    let result = &resp["result"];
    let result_obj = result.as_object().expect("result object");

    assert_eq!(result["last_reviewed_on"], "2026-05-09");
    assert_eq!(
        result["capability_status"]["json_schema_strict"],
        "supported"
    );
    assert_eq!(result["capabilities"]["streaming_logprobs"], true);

    for internal_key in [
        "evidence",
        "model_overrides",
        "provider_specific",
        "features",
    ] {
        assert!(
            !result_obj.contains_key(internal_key),
            "provider info leaked internal key {internal_key}"
        );
        assert!(
            result["capability_status"].get(internal_key).is_none(),
            "capability_status leaked internal key {internal_key}"
        );
    }
}

#[test]
fn provider_info_groq_json_includes_status_map() {
    let output = cli()
        .args(["provider", "info", "groq", "--format", "json"])
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
    let status = resp["result"]["capability_status"]
        .as_object()
        .expect("capability_status object");

    assert!(status.contains_key("json_schema_strict"));
    assert!(status.contains_key("tool_calling"));
    assert!(status.contains_key("streaming_logprobs"));
}

#[test]
fn provider_info_xai_json_is_runtime_provider_without_grok_alias() {
    let output = cli()
        .args(["provider", "info", "xai", "--format", "json"])
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
    assert_eq!(resp["result"]["name"], "xai");
    assert_eq!(resp["result"]["display_name"], "xAI Grok");
    assert_eq!(resp["result"]["capabilities"]["streaming"], true);

    let alias = cli()
        .args(["provider", "info", "grok", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(
        alias.status.code(),
        Some(5),
        "`grok` must not be registered as an alias for xAI Grok"
    );
}

#[test]
fn provider_info_human_preserves_recognized_status() {
    let output = cli()
        .args(["provider", "info", "openrouter", "--format", "human"])
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
        stdout.contains("recognized"),
        "human output must preserve recognized status, got:\n{stdout}"
    );
}
