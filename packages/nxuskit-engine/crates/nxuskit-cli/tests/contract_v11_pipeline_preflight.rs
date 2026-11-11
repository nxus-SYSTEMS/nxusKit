//! Feature 106 / US5 — `pipeline preflight` and `pipeline run
//! --preflight` binary tests.
//!
//! Pins:
//! - FR-028 `pipeline preflight --input <file> --format json` exists
//!   and emits a public `ResourceAdvisory` JSON object.
//! - FR-029 `pipeline run --preflight` includes the same advisory in
//!   the run report (and in the embedded envelope, per FR-026).
//! - FR-030 severity vocabulary `ok|caution|high_risk|blocked`.
//! - FR-030 reason vocabulary includes `resource_probe_unavailable`.
//! - FR-032 when probing is unavailable the CLI MUST return the
//!   stable typed advisory rather than omitting the field.
//! - FR-033 advisory output passes the public-boundary deny-list.

use assert_cmd::Command;
use serde_json::Value;

#[path = "v11_rc_support.rs"]
mod v11_rc_support;

fn cli() -> Command {
    Command::cargo_bin("nxuskit-cli").unwrap()
}

fn fixture_path(name: &str) -> String {
    format!("tests/fixtures/v11_rc_peeler_cli/{name}")
}

/// T062 — `pipeline preflight` returns a public-safe `ResourceAdvisory`
/// document on a v1.1 fan-out fixture. Severity and reason codes come
/// from the documented vocabularies.
#[test]
fn pipeline_preflight_emits_resource_advisory_for_fanout_fixture() {
    let output = cli()
        .args([
            "pipeline",
            "preflight",
            "--input",
            &fixture_path("v11-fanout.json"),
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).expect("utf-8");
    let value: Value = serde_json::from_str(&text).expect("json");

    assert_eq!(value["contract_version"], "1.1");
    let severity = value["severity"].as_str().expect("severity is a string");
    assert!(
        matches!(severity, "ok" | "caution" | "high_risk" | "blocked"),
        "severity {severity:?} is not in the documented vocabulary"
    );

    // Reasons array always present (FR-032: stable fallback when
    // probing is unavailable means the array must still carry the
    // `resource_probe_unavailable` reason rather than being omitted).
    let reasons = value["reasons"].as_array().expect("reasons array");
    assert!(!reasons.is_empty(), "reasons must not be empty");
    let known_reasons = [
        "insufficient_available_memory",
        "multiple_large_models",
        "provider_already_busy",
        "unknown_model_size",
        "resource_probe_unavailable",
    ];
    for reason in reasons {
        let code = reason["code"].as_str().expect("code is a string");
        assert!(
            known_reasons.contains(&code),
            "reason code {code:?} is not in the documented vocabulary"
        );
    }

    // `probe_status` is `available | unavailable | partial`.
    let probe_status = value["probe_status"]
        .as_str()
        .expect("probe_status is a string");
    assert!(matches!(
        probe_status,
        "available" | "unavailable" | "partial"
    ));

    v11_rc_support::assert_no_public_boundary_leak(&text);
}

/// T062 — when the SDK cannot probe resources, the CLI MUST return the
/// stable `resource_probe_unavailable` fallback advisory rather than
/// omitting the field (FR-032).
#[test]
fn pipeline_preflight_returns_stable_fallback_when_probe_unavailable() {
    // The RC implementation reports `probe_status: "unavailable"` when
    // it has no local resource heuristics. The simplest way to assert
    // the contract is to require the fallback reason code to appear
    // whenever `probe_status == "unavailable"`.
    let output = cli()
        .args([
            "pipeline",
            "preflight",
            "--input",
            &fixture_path("v11-llm-only.json"),
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).expect("utf-8");
    let value: Value = serde_json::from_str(&text).expect("json");
    if value["probe_status"] == "unavailable" {
        let reasons = value["reasons"].as_array().expect("reasons array");
        assert!(
            reasons
                .iter()
                .any(|r| r["code"] == "resource_probe_unavailable"),
            "probe_status=unavailable must include resource_probe_unavailable reason: {text}"
        );
        // Severity for fallback must be `caution`.
        assert_eq!(value["severity"], "caution");
    }
    v11_rc_support::assert_no_public_boundary_leak(&text);
}

/// T063 — `pipeline run --preflight` embeds the advisory in the report
/// and in the embedded envelope (FR-029, FR-026).
#[test]
fn pipeline_run_preflight_embeds_advisory_in_report_and_envelope() {
    let output = cli()
        .args([
            "pipeline",
            "run",
            "--input",
            &fixture_path("v11-fanout.json"),
            "--format",
            "json",
            "--preflight",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).expect("utf-8");
    let value: Value = serde_json::from_str(&text).expect("json");

    // The `resource_advisory` section MUST be present in the report.
    let report_adv = value
        .get("resource_advisory")
        .expect("preflight populates report.resource_advisory");

    // And ALSO inside the embedded envelope.
    let envelope_adv = value["artifact_history_envelope"]
        .get("resource_advisory")
        .expect("preflight populates envelope.resource_advisory");

    // The two MUST match.
    assert_eq!(
        report_adv, envelope_adv,
        "report and envelope advisories must match"
    );

    v11_rc_support::assert_no_public_boundary_leak(&text);
}
