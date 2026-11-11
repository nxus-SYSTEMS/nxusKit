//! US5 (T054, DJ-101) — binary-level smoke for `nxuskit-cli doctor --json`.
//!
//! Article II §CLI Verification Requirements: binary-level invocation via
//! `assert_cmd`. Asserts the four feature 105 posture rows appear at their
//! CE defaults and that no `clips_orchestrator_availability` key is
//! present anywhere in the JSON output (FR-009 / PR-103).

use assert_cmd::Command;
use serde_json::Value;

fn cli() -> Command {
    Command::cargo_bin("nxuskit-cli").unwrap()
}

#[test]
fn dj_101_doctor_json_emits_four_feature_105_posture_rows_at_ce_defaults() {
    let output = cli()
        .args(["doctor", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: Value = serde_json::from_slice(&output).expect("doctor --json must emit valid JSON");

    // Arrow reference compatibility: CE default DeclarationOnly.
    assert_eq!(
        value["arrow_reference_compatibility"], "declaration_only",
        "doctor --json arrow_reference_compatibility CE default must be declaration_only; got {value}"
    );
    // Substrait plan-reference compatibility: CE default DeclarationOnly.
    assert_eq!(
        value["substrait_plan_reference_compatibility"], "declaration_only",
        "doctor --json substrait_plan_reference_compatibility CE default must be declaration_only"
    );
    // Zenoh availability: CE default Deferred.
    assert_eq!(
        value["zenoh_availability"], "deferred",
        "doctor --json zenoh_availability CE default must be deferred"
    );
    // CLIPS rules-provider availability: present in capability truth so
    // `available`. Note: under a CE build without the CLIPS provider
    // wired, this would be `unavailable`. The default CLI projection
    // includes the `rule_eval.generic` capability bound to provider
    // `clips`, so we expect `available` here.
    assert_eq!(
        value["clips_rules_provider_availability"], "available",
        "doctor --json clips_rules_provider_availability must be `available` when \
         rule_eval.generic is wired to provider `clips`"
    );
}

#[test]
fn dj_101_doctor_json_does_not_contain_clips_orchestrator_availability_key() {
    let output = cli()
        .args(["doctor", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let raw = std::str::from_utf8(&output).expect("doctor --json output is UTF-8");
    assert!(
        !raw.contains("clips_orchestrator_availability"),
        "doctor --json output MUST NOT contain `clips_orchestrator_availability` anywhere; \
         CLIPS orchestration is not a CE concept (FR-009). Output was: {raw}"
    );
}

#[test]
fn dj_101_doctor_json_feature_104_fields_remain_present() {
    // Regression: feature 104 readiness fields (contract_version,
    // captured_at, providers, capabilities) MUST remain reachable in the
    // projected JSON output after the feature 105 extension.
    let output = cli()
        .args(["doctor", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: Value = serde_json::from_slice(&output).expect("doctor --json must emit valid JSON");
    for key in [
        "contract_version",
        "captured_at",
        "providers",
        "capabilities",
    ] {
        assert!(
            value.get(key).is_some(),
            "doctor --json output missing feature 104 field `{key}`"
        );
    }
    assert_eq!(value["contract_version"], "1.1");
}
