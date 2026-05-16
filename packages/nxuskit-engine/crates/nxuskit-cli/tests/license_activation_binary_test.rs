use assert_cmd::Command;
use serde_json::Value;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn license_status_json_exposes_release_diagnostics_against_temp_token_path() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time must be after Unix epoch")
        .as_nanos();
    let home = std::env::temp_dir().join(format!(
        "nxuskit-cli-license-status-{}-{unique}",
        std::process::id()
    ));
    let token_dir = home.join(".nxuskit");
    fs::create_dir_all(&token_dir).expect("create temp token dir");
    fs::write(token_dir.join("license.token"), "not-a-jwt").expect("write temp token");

    let output = Command::cargo_bin("nxuskit-cli")
        .expect("nxuskit-cli binary should be available")
        .args(["license", "status", "--json"])
        .env("HOME", &home)
        .env("NXUSKIT_LICENSE_ENVIRONMENT", "test")
        .output()
        .expect("run license status");

    assert!(
        output.status.success(),
        "status command should emit diagnostics even for invalid token"
    );

    let json: Value = serde_json::from_slice(&output.stdout).expect("status output is JSON");
    assert_eq!(json["licensing"]["environment"], "test");
    assert_eq!(
        json["licensing"]["endpoint"]["default"],
        "https://nxus.systems/licensing-api/v1"
    );
    assert_eq!(json["licensing"]["signing_key"]["kid"], "es256-v1");
    let signing_key_source = json["licensing"]["signing_key"]["source"]
        .as_str()
        .expect("signing key source should be a string");
    assert!(
        matches!(
            signing_key_source,
            "embedded-production" | "dev-test-fallback" | "embedded-key"
        ),
        "unexpected signing key source label: {signing_key_source}"
    );
    assert!(
        !signing_key_source.contains('/') && !signing_key_source.contains('\\'),
        "signing key source label must not expose a local filesystem path"
    );

    assert!(json["license"]["edition"].is_string());
    assert!(json["license"]["features"].is_array());
    assert!(json["license"]["effective_limits"].is_object());

    fs::remove_dir_all(home).ok();
}

#[test]
fn license_status_human_output_redacts_signing_key_path() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time must be after Unix epoch")
        .as_nanos();
    let home = std::env::temp_dir().join(format!(
        "nxuskit-cli-license-status-human-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&home).expect("create temp home");

    let output = Command::cargo_bin("nxuskit-cli")
        .expect("nxuskit-cli binary should be available")
        .args(["license", "status"])
        .env("HOME", &home)
        .output()
        .expect("run license status");

    assert!(output.status.success(), "status command should succeed");

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains("Signing key: es256-v1 ("));
    assert!(
        !stdout.contains("DevOps/sharedData/keys")
            && !stdout.contains("es256-production-pubkey.pem")
            && !stdout.contains("/Users/"),
        "human status output must not expose local signing key paths:\n{stdout}"
    );

    fs::remove_dir_all(home).ok();
}
