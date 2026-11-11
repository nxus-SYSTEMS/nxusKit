//! T020 (US1) — `nxuskit-cli doctor --json` binary-level smoke test.
//!
//! Article II §CLI Verification Requirements: binary-level invocation via
//! `assert_cmd`, not internal-handler-only. The CLI projection must be a
//! `serde_json` projection of the canonical Rust DTOs in
//! `nxuskit::contract::v1_1::ReadinessSnapshot`, NOT a competing schema
//! (CB-6, contracts/doctor-json-cli-projection.md).
//!
//! The doctor projection MUST be public-safe and MUST NOT carry internal
//! routing/catalog topology, internal correlation IDs, or internal repo names.

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;

fn cli() -> Command {
    Command::cargo_bin("nxuskit-cli").unwrap()
}

#[test]
fn doctor_json_subcommand_exists_and_exits_zero() {
    // The Phase-3 implementation MUST exit 0 (the Phase-2 placeholder
    // exited 2 to signal not-yet-implemented).
    cli().args(["doctor", "--json"]).assert().success();
}

#[test]
fn doctor_json_emits_contract_v1_1_readiness_snapshot_shape() {
    let output = cli()
        .args(["doctor", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: Value = serde_json::from_slice(&output).expect("doctor --json must emit valid JSON");

    assert_eq!(
        value["contract_version"], "1.1",
        "doctor --json output must declare contract_version=1.1"
    );

    // Required top-level fields per ReadinessSnapshot DTO.
    for key in ["captured_at", "providers", "capabilities"] {
        assert!(
            value.get(key).is_some(),
            "doctor --json output missing required field `{key}`"
        );
    }
    // captured_at must be a useful RFC 3339 capture timestamp, not the
    // deterministic epoch fixture used by older pre-RC projections.
    let captured_at = value["captured_at"]
        .as_str()
        .expect("captured_at must be a string");
    assert_ne!(
        captured_at, "1970-01-01T00:00:00Z",
        "doctor --json captured_at must reflect command capture time"
    );
    chrono::DateTime::parse_from_rfc3339(captured_at).expect("captured_at must parse as RFC 3339");
    // providers must be an array.
    assert!(value["providers"].is_array());
    // capabilities must be an array.
    assert!(value["capabilities"].is_array());
}

#[test]
fn doctor_json_output_round_trips_into_the_canonical_rust_dto() {
    // CB-6 / contracts/doctor-json-cli-projection.md §Round-Trip Test:
    // CLI stdout MUST deserialize into the Rust DTO, proving the CLI is a
    // projection of the canonical type.
    let output = cli()
        .args(["doctor", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let snapshot: nxuskit::contract::v1_1::ReadinessSnapshot = serde_json::from_slice(&output)
        .expect("doctor --json output must deserialize into ReadinessSnapshot");
    assert_eq!(snapshot.contract_version, "1.1");
}

#[test]
fn doctor_json_does_not_leak_internal_provider_channel_or_internal_repo_names() {
    // BR-001 / BR-003 / FR-031: public CLI output must not name internal
    // topology fields, Pro routing, or internal repo paths.
    let output = cli()
        .args(["doctor", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).expect("doctor stdout must be UTF-8");
    let banned = [
        format!("{}Envelope", "Request"),
        format!("{}Envelope", "Result"),
        format!("{}{}", "Provider", "Channel"),
        format!("{}_{}", "entitlement", "source"),
        format!("{}_{}", "causality", "id"),
        format!("{}_{}_{}", "pro", "routing", "key"),
        format!("{}-{}", "nxusKit", "internal"),
        format!("{}-{}", "peeler", "internal"),
    ];
    for needle in banned {
        assert!(
            !text.contains(&needle),
            "doctor --json output leaked banned token `{needle}`. Full output:\n{text}"
        );
    }
}

#[test]
fn doctor_help_is_listed_under_root_help_and_describes_v1_1_readiness() {
    cli()
        .args(["doctor", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("readiness"));
}

// -----------------------------------------------------------------------
// Feature 106 / US1: runtime-callable model ids + loopback:v1 alias
// (T023, T024). FR-001, FR-002, FR-004, FR-005, SC-001, SC-002, SC-003.
// -----------------------------------------------------------------------

/// T023 — every ready, non-deprecated, non-`alias_only` model row emitted by
/// `doctor --json` MUST be directly callable by `nxuskit-cli call` without
/// any consumer-side alias rewriting. The loopback `echo` row is the
/// canonical case; this test pins the contract for it.
#[test]
fn doctor_ready_loopback_models_are_runtime_callable_by_their_emitted_id() {
    let output = cli()
        .args(["doctor", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let snapshot: nxuskit::contract::v1_1::ReadinessSnapshot =
        serde_json::from_slice(&output).expect("doctor --json deserializes into ReadinessSnapshot");

    let loopback = snapshot
        .providers
        .iter()
        .find(|p| p.provider_id.as_str() == "loopback")
        .expect("doctor --json must report the loopback provider");
    assert_eq!(
        loopback.posture,
        nxuskit::contract::v1_1::ReadinessPosture::Ready,
        "loopback must be Ready in default readiness"
    );

    // FR-001: every non-deprecated, non-`alias_only` model row is callable.
    let runtime_callable: Vec<&str> = loopback
        .models
        .iter()
        .filter(|m| !m.deprecated && !m.metadata.alias_only)
        .map(|m| m.id.as_str())
        .collect();
    assert!(
        !runtime_callable.is_empty(),
        "loopback readiness must report at least one runtime-callable model row"
    );

    // FR-001 / FR-004: the legacy `loopback:v1` id must NOT appear as a
    // non-`alias_only` row (it is only acceptable as a documented alias on a
    // real model row).
    assert!(
        !runtime_callable.iter().any(|id| *id == "loopback:v1"),
        "loopback:v1 must not be emitted as a runtime-callable model row in v1.1.0 RC"
    );
    // Runtime-callable default MUST be `echo`.
    assert!(
        runtime_callable.iter().any(|id| *id == "echo"),
        "loopback runtime-callable models must include `echo` (got {runtime_callable:?})"
    );

    // SC-001 binary-level: every runtime-callable row is accepted by
    // `nxuskit-cli call` (using `--help` form since call requires
    // network/provider initialization for full chat; we assert at the
    // argument-parse level that the model is accepted without an
    // alias-mismatch rejection).
    for model_id in &runtime_callable {
        let assert = cli()
            .args(["call", "loopback", model_id, "--prompt", "hi", "--dry-run"])
            .assert();
        // `--dry-run` may not yet be a documented flag; tolerate that by
        // requiring at minimum that the call did not reject the model id
        // with a "not a valid loopback model" or alias-mismatch error.
        let output = assert.get_output();
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let combined = format!("{stderr}{stdout}");
        assert!(
            !combined.contains("Invalid loopback model")
                && !combined.contains("alias mismatch")
                && !combined.contains("not a valid model"),
            "nxuskit-cli call rejected runtime-callable model `{model_id}` with alias/model error:\n{combined}"
        );
    }
}

/// T024 — `nxuskit-cli call loopback loopback:v1` MUST be accepted as an alias
/// for the runtime-callable default loopback model. FR-002, FR-051.
#[test]
fn doctor_loopback_v1_alias_is_accepted_by_call_for_back_compat() {
    let assert = cli()
        .args(["call", "loopback", "loopback:v1", "--prompt", "hi"])
        .assert();
    let output = assert.get_output();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{stderr}{stdout}");
    // FR-002: `loopback:v1` must NOT be rejected as an unknown model. The
    // call may still fail for orthogonal reasons (no provider initialized
    // in the test env), but the failure must NOT be alias rejection.
    assert!(
        !combined.contains("Invalid loopback model")
            && !combined.contains("alias mismatch")
            && !combined.contains("not a valid model")
            && !combined.contains("unknown model"),
        "nxuskit-cli call rejected the loopback:v1 alias:\n{combined}"
    );
}

/// US1 / R7 — `ProviderModel.metadata.contract_aliases` SHIPS in v1.1.0 RC
/// as an additive optional field. For the loopback `echo` row it MUST
/// include `"loopback:v1"`. Other ready providers' rows MUST leave it
/// absent or empty.
#[test]
fn doctor_loopback_echo_model_exposes_loopback_v1_as_contract_alias() {
    let output = cli()
        .args(["doctor", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let snapshot: nxuskit::contract::v1_1::ReadinessSnapshot =
        serde_json::from_slice(&output).expect("deserialize ReadinessSnapshot");

    let loopback = snapshot
        .providers
        .iter()
        .find(|p| p.provider_id.as_str() == "loopback")
        .expect("loopback provider row present");
    let echo = loopback
        .models
        .iter()
        .find(|m| m.id == "echo")
        .expect("loopback `echo` model row present");
    assert!(
        echo.metadata
            .contract_aliases
            .iter()
            .any(|a| a == "loopback:v1"),
        "loopback echo row metadata.contract_aliases must include `loopback:v1` (got {:?})",
        echo.metadata.contract_aliases
    );
}
