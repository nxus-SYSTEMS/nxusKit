//! US6 (T063 / T064) fixture generation helper.
//!
//! Builds the canonical positive fixtures for `ActionEnvelope` and the
//! retargeted `ArtifactHistoryEnvelope` via the public builder surface and
//! asserts the authored on-disk fixtures match those canonical outputs.
//!
//! Article III posture: every assertion would fail against a builder that
//! dropped retargeted fields silently, or a loader that did not preserve
//! `canonical_hash` across export/import.

use std::collections::BTreeMap;
use std::path::PathBuf;

use nxuskit::contract::v1_1::refs::{
    BatchRef, EnvelopeRef, PlanCompatibilityPosture, PlanKind, PlanRef,
};
use nxuskit::contract::v1_1::{
    ActionEnvelope, ActionId, ArtifactHistoryEnvelope, BatchId, CorrelationId, EngineKind, PlanId,
    ProvenanceEntryClass, ReplayKind, ReplayLinkage, RunOutcome, TopicName,
    load_artifact_history_envelope_json,
};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .join("specs")
        .join("105-v110-arrow-zenoh-substrait-retarget")
        .join("contracts")
        .join("fixtures")
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

fn artifact_ref(id: &str) -> EnvelopeRef {
    use nxuskit::contract::v1_1::refs::{ArtifactKind, ArtifactRef};
    EnvelopeRef::Artifact(ArtifactRef {
        artifact_id: id.to_owned(),
        kind: ArtifactKind::HistoryEntry,
        canonical_hash: None,
        display_short: None,
        metadata: BTreeMap::new(),
    })
}

fn plan_ref() -> PlanRef {
    PlanRef {
        plan_id: PlanId::from("plan-us6"),
        plan_kind: PlanKind::Substrait,
        compatibility_posture: PlanCompatibilityPosture::DeclarationOnly,
        metadata: BTreeMap::new(),
    }
}

fn build_positive_action_envelope() -> ActionEnvelope {
    ActionEnvelope {
        action_id: ActionId::from("action-us6-001"),
        correlation_id: CorrelationId::from("corr-us6-001"),
        topic: TopicName::from("nxuskit.demo.action"),
        action_type: TopicName::from("demo.invoke").into_action_type(),
        input_refs: vec![
            batch_ref("batch-input-1", "sha256:0xa1"),
            batch_ref("batch-input-2", "sha256:0xa2"),
        ],
        output_refs: vec![artifact_ref("art-out-1")],
        plan_ref: Some(plan_ref()),
        metadata: BTreeMap::new(),
    }
}

trait TopicNameIntoActionType {
    fn into_action_type(self) -> nxuskit::contract::v1_1::ActionType;
}

impl TopicNameIntoActionType for TopicName {
    fn into_action_type(self) -> nxuskit::contract::v1_1::ActionType {
        nxuskit::contract::v1_1::ActionType::from(self.as_str())
    }
}

fn build_positive_history_envelope() -> ArtifactHistoryEnvelope {
    ArtifactHistoryEnvelope::builder("art_us6_replay_001")
        .pipeline_ref("pl_us3_data_store_search_plan_ref", "1.1.0")
        .readiness_snapshot_captured_at("2026-05-27T00:00:00Z")
        .run_outcome(RunOutcome::Success)
        .replay_linkage(ReplayLinkage {
            replay_kind: ReplayKind::ReplayOf,
            source_envelope_id: Some("pl_us3_data_store_search_plan_ref".to_owned()),
            source_canonical_hash: None,
        })
        .action_id(ActionId::from("action-us6-002"))
        .correlation_id(CorrelationId::from("corr-us6-002"))
        .add_input_ref(batch_ref("batch-in-a", "sha256:0xb1"))
        .add_input_ref(batch_ref("batch-in-b", "sha256:0xb2"))
        .add_output_ref(batch_ref("batch-out-a", "sha256:0xc1"))
        .plan_ref(plan_ref())
        .engine_id("clips-engine-default")
        .engine_kind(EngineKind::ClipsRules)
        .add_provenance_class(ProvenanceEntryClass::DataPlaneReference)
        .add_provenance_class(ProvenanceEntryClass::ControlPlaneAction)
        .build()
}

#[test]
fn t063_positive_action_envelope_refs_fixture_matches_builder_output() {
    let path = fixtures_dir().join("positive_action_envelope_refs.json");
    let envelope = build_positive_action_envelope();
    let body = serde_json::to_string_pretty(&envelope).expect("serialize");
    assert_fixture_matches(&path, &body);

    let on_disk = std::fs::read_to_string(&path).expect("read fixture");
    let parsed: ActionEnvelope = serde_json::from_str(&on_disk).expect("parse on-disk fixture");
    assert_eq!(parsed.action_id.as_str(), "action-us6-001");
    assert_eq!(parsed.input_refs.len(), 2);
    assert_eq!(parsed.output_refs.len(), 1);
    assert!(parsed.plan_ref.is_some());
}

#[test]
fn t064_positive_envelope_input_output_batch_refs_replay_fixture_round_trips() {
    let path = fixtures_dir().join("positive_envelope_input_output_batch_refs_replay.json");
    let envelope = build_positive_history_envelope();
    let body = envelope
        .export_canonical_json()
        .expect("export canonical JSON");
    let pretty = pretty_print(&body);
    assert_fixture_matches(&path, &pretty);

    let on_disk = std::fs::read_to_string(&path).expect("read fixture");
    let parsed = load_artifact_history_envelope_json(&on_disk).expect("load on-disk fixture");
    let canonical_again = parsed
        .export_canonical_json()
        .expect("re-export canonical JSON");
    let canonical_pretty = pretty_print(&canonical_again);
    assert_eq!(
        canonical_pretty.trim(),
        on_disk.trim(),
        "T064 fixture must equal its canonical round-trip representation"
    );
    assert_eq!(parsed.canonical_hash.as_str().len(), 64);
    assert!(parsed.action_id.is_some());
    assert!(parsed.plan_ref.is_some());
}

fn assert_fixture_matches(path: &std::path::Path, content: &str) {
    let mut expected = content.to_owned();
    if !expected.ends_with('\n') {
        expected.push('\n');
    }
    let actual = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    assert_eq!(
        actual, expected,
        "authored fixture must match canonical builder output: {path:?}"
    );
}

fn pretty_print(raw: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(raw).expect("parse canonical JSON");
    let mut out = serde_json::to_string_pretty(&v).expect("pretty-print canonical JSON");
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}
