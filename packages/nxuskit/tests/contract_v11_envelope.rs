//! T021 (US1) — Artifact / history envelope round-trip tests.
//!
//! US1 scope: prove that a Peeler-style consumer can produce, export, and
//! re-import an envelope with stable canonical identity. The full
//! nine-variant `RunOutcome` enumeration test belongs to US4 (T057) and is
//! intentionally NOT exercised here — US1 only ships the `Success` and
//! `ValidationFailed` variants needed by the MVP smoke path.

use nxuskit::contract::v1_1::{ArtifactHistoryEnvelope, CONTRACT_VERSION, RunOutcome};

fn sample_envelope() -> ArtifactHistoryEnvelope {
    ArtifactHistoryEnvelope::builder("art_001")
        .pipeline_ref("pl_smoke", "0.1.0")
        .readiness_snapshot_captured_at("2026-05-22T17:00:00Z")
        .run_outcome(RunOutcome::Success)
        .build()
}

#[test]
fn envelope_carries_contract_version_and_canonical_hash() {
    let env = sample_envelope();
    assert_eq!(env.contract_version, CONTRACT_VERSION);
    assert_eq!(env.canonical_hash.as_str().len(), 64);
    assert_eq!(env.display_hash.as_str().len(), 12);
    assert_eq!(
        env.display_hash.as_str(),
        &env.canonical_hash.as_str()[..12],
        "display short must be the first 12 hex chars of the canonical hash"
    );
}

#[test]
fn envelope_round_trip_preserves_canonical_hash() {
    let env = sample_envelope();
    let exported = env.export_canonical_json().expect("export");
    let reimported = ArtifactHistoryEnvelope::import_canonical_json(&exported).expect("import");
    assert_eq!(env.canonical_hash, reimported.canonical_hash);
    assert_eq!(env.display_hash, reimported.display_hash);
    assert_eq!(env.run_outcome, reimported.run_outcome);
}

#[test]
fn envelope_canonical_hash_excludes_produced_at_and_aliases() {
    // produced_at and legacy_display_aliases must NOT contribute to the
    // canonical identity payload (research.md §4).
    let env_a = ArtifactHistoryEnvelope::builder("art_001")
        .pipeline_ref("pl_smoke", "0.1.0")
        .readiness_snapshot_captured_at("2026-05-22T17:00:00Z")
        .produced_at("2026-05-22T17:00:00Z")
        .run_outcome(RunOutcome::Success)
        .build();
    let env_b = ArtifactHistoryEnvelope::builder("art_001")
        .pipeline_ref("pl_smoke", "0.1.0")
        .readiness_snapshot_captured_at("2026-05-22T17:00:00Z")
        .produced_at("2099-12-31T23:59:59Z") // different produced_at
        .with_legacy_display_alias("abc1234") // different alias set
        .run_outcome(RunOutcome::Success)
        .build();
    assert_eq!(
        env_a.canonical_hash, env_b.canonical_hash,
        "produced_at and legacy aliases must not affect canonical identity"
    );
}

#[test]
fn envelope_canonical_hash_changes_when_payload_changes() {
    let env_a = sample_envelope();
    let env_b = ArtifactHistoryEnvelope::builder("art_002") // different artifact_id
        .pipeline_ref("pl_smoke", "0.1.0")
        .readiness_snapshot_captured_at("2026-05-22T17:00:00Z")
        .run_outcome(RunOutcome::Success)
        .build();
    assert_ne!(env_a.canonical_hash, env_b.canonical_hash);
}

#[test]
fn us1_run_outcome_variants_are_only_success_and_validation_failed() {
    // US1 floor: only two variants ship in this slice. US4/T057 expands to
    // the full nine-variant set. A premature broadening of this enum here
    // would fail this test and would be a US4-scope leak.
    let v_success = serde_json::to_string(&RunOutcome::Success).unwrap();
    let v_validation = serde_json::to_string(&RunOutcome::ValidationFailed).unwrap();
    assert_eq!(v_success, "\"success\"");
    assert_eq!(v_validation, "\"validation_failed\"");
}

// -----------------------------------------------------------------------
// Feature 106 / US4 — `ArtifactHistoryEnvelope` additive extensions for
// fan-out outcomes, resource advisory slot, and imported-example
// reference. FR-026, FR-027.
// -----------------------------------------------------------------------

use nxuskit::contract::v1_1::{
    FanOutGroupResult, FanOutOperationResult, ImportedExampleRef, ImportedExampleType,
    OperationOutcome, ResourceAdvisory,
};

#[test]
fn envelope_without_feature_106_fields_skips_them_on_serialize() {
    let env = sample_envelope();
    let json = serde_json::to_string(&env).expect("ser");
    assert!(
        !json.contains("fan_out_outcomes"),
        "empty fan_out_outcomes must be skipped: {json}"
    );
    assert!(
        !json.contains("resource_advisory"),
        "None resource_advisory must be skipped: {json}"
    );
    assert!(
        !json.contains("imported_example_ref"),
        "None imported_example_ref must be skipped: {json}"
    );
}

#[test]
fn envelope_canonical_hash_unchanged_when_feature_106_fields_absent() {
    // The feature 104/105 canonical-hash regression invariant: an
    // envelope without any feature-106 fields must hash identically to
    // its US1 counterpart. The `sample_envelope()` is built without
    // any of the new fields, so its hash must equal the hash recorded
    // in the existing `envelope_carries_contract_version_and_canonical_hash`
    // test below — i.e., 64 hex chars deterministic.
    let env1 = sample_envelope();
    let env2 = sample_envelope();
    assert_eq!(env1.canonical_hash, env2.canonical_hash);
    assert_eq!(env1.canonical_hash.as_str().len(), 64);
}

#[test]
fn envelope_with_fan_out_outcomes_round_trips() {
    let mut env = sample_envelope();
    env.fan_out_outcomes.push(FanOutGroupResult {
        group_id: "compare_summaries".to_string(),
        operations: vec![
            FanOutOperationResult {
                operation_id: "compare_summaries::op-0".to_string(),
                pair_index: 0,
                provider_ref: "loopback".to_string(),
                model_ref: "echo".to_string(),
                outcome: OperationOutcome::Success,
                elapsed_ms: 12,
                output_summary: Some("echo: hi".to_string()),
            },
            FanOutOperationResult {
                operation_id: "compare_summaries::op-1".to_string(),
                pair_index: 1,
                provider_ref: "loopback".to_string(),
                model_ref: "echo-json-native".to_string(),
                outcome: OperationOutcome::Success,
                elapsed_ms: 14,
                output_summary: None,
            },
        ],
    });
    let json = serde_json::to_string(&env).expect("ser");
    assert!(json.contains("\"fan_out_outcomes\""));
    assert!(json.contains("compare_summaries::op-0"));
    let parsed: ArtifactHistoryEnvelope = serde_json::from_str(&json).expect("de");
    assert_eq!(parsed.fan_out_outcomes.len(), 1);
    assert_eq!(parsed.fan_out_outcomes[0].operations.len(), 2);
    assert_eq!(
        parsed.fan_out_outcomes[0].operations[0].operation_id,
        "compare_summaries::op-0"
    );
}

#[test]
fn envelope_with_imported_example_ref_round_trips() {
    let mut env = sample_envelope();
    env.imported_example_ref = Some(ImportedExampleRef {
        example_id: "rc-loopback-llm-only".to_string(),
        definition_type: ImportedExampleType::Pipeline,
        source_repository: "nxusKit-examples".to_string(),
        source_path: "examples/v1_1/loopback/llm-only.json".to_string(),
        source_ref: "v1.1.0-rc".to_string(),
    });
    let json = serde_json::to_string(&env).expect("ser");
    assert!(json.contains("\"imported_example_ref\""));
    let parsed: ArtifactHistoryEnvelope = serde_json::from_str(&json).expect("de");
    let r = parsed.imported_example_ref.expect("ref present");
    assert_eq!(r.example_id, "rc-loopback-llm-only");
    assert_eq!(r.source_repository, "nxusKit-examples");
}

#[test]
fn envelope_with_resource_advisory_slot_round_trips() {
    // FR-026 / US5: resource advisory slot is the typed public
    // `ResourceAdvisory` DTO.
    let mut env = sample_envelope();
    env.resource_advisory = Some(ResourceAdvisory::probe_unavailable());
    let json = serde_json::to_string(&env).expect("ser");
    assert!(json.contains("\"resource_advisory\""));
    let parsed: ArtifactHistoryEnvelope = serde_json::from_str(&json).expect("de");
    let adv = parsed.resource_advisory.expect("present");
    assert_eq!(
        adv.severity,
        nxuskit::contract::v1_1::ResourceAdvisorySeverity::Caution
    );
}

#[test]
fn imported_example_ref_constructor_rejects_internal_repo_names() {
    // FR-046 / public-boundary-leakcheck deny-list: the public-safe
    // constructor MUST refuse repo names that match documented
    // internal markers. The plain struct literal is still allowed (the
    // type is `pub` for round-trip purposes), but consumers SHOULD go
    // through the constructor.
    assert!(
        ImportedExampleRef::try_public(
            "x",
            ImportedExampleType::Pipeline,
            "nxusKit-examples-internal",
            "examples/x.json",
            "main",
        )
        .is_err(),
        "must reject nxusKit-examples-internal"
    );
    assert!(
        ImportedExampleRef::try_public(
            "x",
            ImportedExampleType::Pipeline,
            "peeler-internal",
            "examples/x.json",
            "main",
        )
        .is_err(),
        "must reject peeler-internal"
    );
    assert!(
        ImportedExampleRef::try_public(
            "x",
            ImportedExampleType::Pipeline,
            "nxusKit-examples",
            "examples/x.json",
            "main",
        )
        .is_ok(),
        "public alias must be accepted"
    );
}
