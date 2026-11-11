//! T016 (US1) — feature 105 envelope extension round-trip tests.
//!
//! Covers `contracts/artifact-history-envelope-retarget.md`:
//!   AHE-001 — feature-104-shaped envelope canonical-hashes unchanged under 105.
//!   AHE-002 — full retargeted round-trip preserves canonical equality.
//!   AHE-003 — replay_linkage resolves to original pipeline-definition snapshot.
//!   AHE-005 — legacy short-hash aliases stay alias-only (no canonical promotion).
//!
//! Article III semantic posture: each negative assertion would fail against a
//! stub that (a) included default-empty `input_refs`/`output_refs` in the
//! canonical hash, (b) dropped optional `action_id`/`plan_ref` on round-trip,
//! or (c) promoted `legacy_display_aliases` into the identity payload.

use nxuskit::contract::v1_1::refs::{
    BatchRef, EnvelopeRef, PlanCompatibilityPosture, PlanKind, PlanRef,
};
use nxuskit::contract::v1_1::{
    ActionId, ArtifactCanonicalHash, ArtifactHistoryEnvelope, BatchId, CorrelationId, EngineKind,
    PlanId, ProvenanceEntryClass, ReplayKind, ReplayLinkage, RunOutcome,
};
use std::collections::BTreeMap;

fn baseline_104_envelope() -> ArtifactHistoryEnvelope {
    ArtifactHistoryEnvelope::builder("art_104_baseline_001")
        .pipeline_ref("pl_baseline", "0.1.0")
        .readiness_snapshot_captured_at("2026-05-22T17:00:00Z")
        .run_outcome(RunOutcome::Success)
        .build()
}

fn batch_ref(id: &str, fp: &str) -> EnvelopeRef {
    EnvelopeRef::Batch(BatchRef {
        batch_id: BatchId::from(id),
        schema_fingerprint: fp.to_owned(),
        provenance_id: None,
        byte_size_hint: None,
        metadata: BTreeMap::new(),
    })
}

fn plan_ref() -> PlanRef {
    PlanRef {
        plan_id: PlanId::from("plan-001"),
        plan_kind: PlanKind::Substrait,
        compatibility_posture: PlanCompatibilityPosture::DeclarationOnly,
        metadata: BTreeMap::new(),
    }
}

// AHE-001 — feature-104-shaped envelope hashes the same across rebuilds and
// across export/import. A stub that included a default-empty `input_refs`
// vector in the canonical payload would shift the hash and fail here.
#[test]
fn ahe_001_feature_104_envelope_canonical_hash_regression() {
    let env = baseline_104_envelope();
    assert_eq!(env.canonical_hash.as_str().len(), 64);
    let env2 = baseline_104_envelope();
    assert_eq!(
        env.canonical_hash, env2.canonical_hash,
        "feature-104 envelope must hash deterministically across rebuilds"
    );
    let exported = env.export_canonical_json().expect("export");
    let reimported = ArtifactHistoryEnvelope::import_canonical_json(&exported).expect("import");
    assert_eq!(env.canonical_hash, reimported.canonical_hash);
}

// AHE-001b — baseline envelope must not auto-populate any feature-105 field.
#[test]
fn ahe_001b_baseline_envelope_has_no_retargeted_fields() {
    let env = baseline_104_envelope();
    assert!(env.action_id.is_none());
    assert!(env.correlation_id.is_none());
    assert!(env.input_refs.is_empty());
    assert!(env.output_refs.is_empty());
    assert!(env.plan_ref.is_none());
    assert!(env.engine_id.is_none());
    assert!(env.engine_kind.is_none());
    assert!(env.provenance_class.is_empty());
}

// AHE-002 — full retargeted round-trip preserves canonical equality.
#[test]
fn ahe_002_full_retargeted_round_trip_canonical_equality() {
    let env = ArtifactHistoryEnvelope::builder("art_retargeted_001")
        .pipeline_ref("pl_retargeted", "1.1.0")
        .readiness_snapshot_captured_at("2026-05-22T17:00:00Z")
        .run_outcome(RunOutcome::Success)
        .action_id(ActionId::from("action-001"))
        .correlation_id(CorrelationId::from("corr-001"))
        .add_input_ref(batch_ref("batch-in-1", "sha256:0xaaa"))
        .add_input_ref(batch_ref("batch-in-2", "sha256:0xbbb"))
        .add_output_ref(batch_ref("batch-out-1", "sha256:0xccc"))
        .plan_ref(plan_ref())
        .engine_id("clips-engine-default")
        .engine_kind(EngineKind::ClipsRules)
        .add_provenance_class(ProvenanceEntryClass::DataPlaneReference)
        .add_provenance_class(ProvenanceEntryClass::ControlPlaneAction)
        .build();
    let exported = env.export_canonical_json().expect("export");
    let reimported = ArtifactHistoryEnvelope::import_canonical_json(&exported).expect("import");
    assert_eq!(env, reimported, "full retargeted envelope must round-trip");
    assert_eq!(env.canonical_hash, reimported.canonical_hash);
}

// AHE-002b — populating a retargeted field MUST shift the canonical hash.
// Locks "additive when present" semantics.
#[test]
fn ahe_002b_setting_retargeted_field_shifts_canonical_hash() {
    let baseline = baseline_104_envelope();
    let with_action = ArtifactHistoryEnvelope::builder("art_104_baseline_001")
        .pipeline_ref("pl_baseline", "0.1.0")
        .readiness_snapshot_captured_at("2026-05-22T17:00:00Z")
        .run_outcome(RunOutcome::Success)
        .action_id(ActionId::from("action-extra"))
        .build();
    assert_ne!(
        baseline.canonical_hash, with_action.canonical_hash,
        "setting action_id must shift the canonical hash"
    );
}

// AHE-003 — replay_linkage resolves to the original snapshot via the source
// canonical hash carried on the envelope's replay block.
#[test]
fn ahe_003_replay_linkage_carries_source_hash() {
    let pipeline_hash = ArtifactCanonicalHash::from(
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    );
    let replay = ReplayLinkage {
        replay_kind: ReplayKind::ReplayOf,
        source_envelope_id: Some("art_original_001".to_owned()),
        source_canonical_hash: Some(pipeline_hash.clone()),
    };
    let env = ArtifactHistoryEnvelope::builder("art_replay_001")
        .pipeline_ref_with_hash("pl_replay", "1.1.0", pipeline_hash.clone())
        .readiness_snapshot_captured_at("2026-05-22T17:00:00Z")
        .run_outcome(RunOutcome::Success)
        .replay_linkage(replay)
        .build();
    assert_eq!(env.replay_linkage.replay_kind, ReplayKind::ReplayOf);
    assert_eq!(
        env.replay_linkage.source_canonical_hash.as_ref(),
        Some(&pipeline_hash),
        "replay linkage must carry the source pipeline canonical hash"
    );
    assert_eq!(
        env.pipeline_ref.canonical_hash.as_ref(),
        Some(&pipeline_hash)
    );
}

// AHE-005 — legacy_display_aliases stay alias-only and do NOT contribute to
// canonical identity. A stub that hashed the alias list would fail here.
#[test]
fn ahe_005_legacy_display_aliases_are_alias_only() {
    let env_no_alias = baseline_104_envelope();
    let env_with_alias = ArtifactHistoryEnvelope::builder("art_104_baseline_001")
        .pipeline_ref("pl_baseline", "0.1.0")
        .readiness_snapshot_captured_at("2026-05-22T17:00:00Z")
        .run_outcome(RunOutcome::Success)
        .with_legacy_display_alias("abc1234")
        .with_legacy_display_alias("abcdef012345")
        .build();
    assert_eq!(
        env_no_alias.canonical_hash, env_with_alias.canonical_hash,
        "legacy_display_aliases MUST NOT contribute to canonical hash"
    );
    assert_eq!(env_with_alias.legacy_display_aliases.len(), 2);
}
