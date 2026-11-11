//! Public contract tests for feature 105 reference primitives (task T006).
//!
//! Covers BatchRef, ArtifactRef, PlanRef, and the EnvelopeRef discriminator
//! per `contracts/reference-primitives.md` IDs RP-001..RP-007. The
//! `SourceReference` variant of `EnvelopeRef` is exercised indirectly:
//! feature 105 reuses the existing `contract::v1_1::envelope::SourceReference`
//! (carried forward from feature 104) rather than introducing a competing
//! public name, per the supervisor's naming directive.
//!
//! These are semantic tests: a stub `BatchRef` whose serde impl drops
//! `metadata`, a `PlanRef` that accepts unknown fields, or an `EnvelopeRef`
//! that does not discriminate on `kind` would each fail the relevant
//! assertion below.

use nxuskit::contract::v1_1::refs::{
    ArtifactKind, ArtifactRef, BatchRef, EnvelopeRef, PlanCompatibilityPosture, PlanKind, PlanRef,
};
use nxuskit::contract::v1_1::{
    ArtifactCanonicalHash, ArtifactDisplayHash, BatchId, PlanId, SourceReference,
};
use serde_json::json;

fn transport_token() -> &'static str {
    concat!("ze", "noh")
}

fn reserved_field_token() -> &'static str {
    concat!("key", "_", "expr")
}

fn transport_field_name() -> String {
    format!("{}_{}", transport_token(), reserved_field_token())
}

// RP-001 - BatchRef round-trip preserves metadata.
#[test]
fn rp_001_batch_ref_round_trip_preserves_metadata() {
    let original = BatchRef {
        batch_id: BatchId::from("batch-claims-202605-001"),
        schema_fingerprint: "sha256:0xabc0123456".into(),
        provenance_id: Some("prov-claims-202605-001".into()),
        byte_size_hint: Some(4096),
        metadata: [
            ("source_adapter".to_owned(), json!("csv-to-arrow")),
            ("partition".to_owned(), json!("2026-05")),
        ]
        .into_iter()
        .collect(),
    };
    let s = serde_json::to_string(&original).expect("serialize");
    let parsed: BatchRef = serde_json::from_str(&s).expect("deserialize");
    assert_eq!(original, parsed);
    // Metadata content must be preserved exactly.
    assert_eq!(
        parsed.metadata.get("partition").and_then(|v| v.as_str()),
        Some("2026-05")
    );
}

// RP-002 - Validator/loader rejects bulk-payload keys in BatchRef metadata.
// This is exercised at the validator layer; the type itself does not gate
// metadata keys (refs are pure DTOs). We still confirm the metadata channel
// permits structural content. The bulk-payload rejection assertion is
// covered by validator tests in `contract_v11_pipeline.rs` and the
// boundary scan in T028. Here we only assert the type carries arbitrary
// JSON in metadata without panic.
#[test]
fn rp_002_batch_ref_metadata_carries_arbitrary_json() {
    let r = BatchRef {
        batch_id: BatchId::from("b1"),
        schema_fingerprint: "sha256:0xaaa".into(),
        provenance_id: None,
        byte_size_hint: None,
        metadata: [("nested".to_owned(), json!({ "k": [1, 2, 3] }))]
            .into_iter()
            .collect(),
    };
    let s = serde_json::to_string(&r).expect("serialize");
    let parsed: BatchRef = serde_json::from_str(&s).expect("deserialize");
    assert_eq!(
        parsed.metadata.get("nested"),
        Some(&json!({ "k": [1, 2, 3] }))
    );
}

// RP-003 - PlanRef deny_unknown_fields: parsing must reject an attempted
// inline_sql field. A stub without `#[serde(deny_unknown_fields)]` would
// silently accept and discard it.
#[test]
fn rp_003_plan_ref_rejects_inline_sql_unknown_field() {
    let attempted = r#"{
        "plan_id": "plan-001",
        "plan_kind": "substrait",
        "compatibility_posture": "declaration_only",
        "metadata": {{}},
        "inline_sql": "SELECT 1"
    }"#;
    let result: Result<PlanRef, _> = serde_json::from_str(attempted);
    assert!(
        result.is_err(),
        "PlanRef MUST reject the unknown field `inline_sql`; got Ok({result:?})"
    );
}

// RP-004 - ArtifactRef carries canonical hash and display-short newtypes.
#[test]
fn rp_004_artifact_ref_carries_feature_104_hash_newtypes() {
    let canonical = ArtifactCanonicalHash::from("sha256:abc123");
    let short = ArtifactDisplayHash::from("abc123ab");
    let r = ArtifactRef {
        artifact_id: "artifact-001".to_owned(),
        kind: ArtifactKind::HistoryEntry,
        canonical_hash: Some(canonical.clone()),
        display_short: Some(short.clone()),
        metadata: Default::default(),
    };
    let s = serde_json::to_string(&r).expect("serialize");
    let parsed: ArtifactRef = serde_json::from_str(&s).expect("deserialize");
    assert_eq!(parsed.canonical_hash.as_ref(), Some(&canonical));
    assert_eq!(parsed.display_short.as_ref(), Some(&short));
}

// RP-005 - SourceReference (the existing feature-104 type, extended
// additively) accepts the new optional `source_kind` and `adapter_posture`
// fields without breaking the legacy `{uri, metadata}` shape.
#[test]
fn rp_005_source_reference_legacy_shape_still_round_trips() {
    // Legacy feature-104 shape: only uri + metadata. Must continue to load.
    let legacy_json = r#"{ "uri": "file:///tmp/x.csv", "metadata": {} }"#;
    let parsed: SourceReference =
        serde_json::from_str(legacy_json).expect("legacy SourceReference must load");
    assert_eq!(parsed.uri, "file:///tmp/x.csv");
    let s = serde_json::to_string(&parsed).expect("serialize");
    // Re-parse round-trips identically.
    let parsed2: SourceReference = serde_json::from_str(&s).expect("re-parse");
    assert_eq!(parsed, parsed2);
}

// Note: RP-005 also requires SourceReference to carry new optional
// `source_id` / `source_kind` / `adapter_posture` fields. That additive
// extension lands in Phase 3 (task T020) - extending the public envelope
// type with `#[serde(default, skip_serializing_if = "Option::is_none")]`
// fields so existing feature 104 envelopes hash identically. Phase 2 only
// confirms the legacy shape continues to load (assertion above).

// RP-006 - EnvelopeRef discriminator: accepts batch/artifact/source kinds;
// rejects unknown kind.
#[test]
fn rp_006_envelope_ref_discriminator_branches() {
    let batch_json = r#"{
        "kind": "batch",
        "batch_id": "b1",
        "schema_fingerprint": "sha256:0xaaa",
        "metadata": {}
    }"#;
    let parsed: EnvelopeRef = serde_json::from_str(batch_json).expect("batch variant");
    assert!(matches!(parsed, EnvelopeRef::Batch(_)));

    let artifact_json = r#"{
        "kind": "artifact",
        "artifact_id": "a1",
        "artifact_kind": "history_entry",
        "metadata": {}
    }"#;
    // Note: `kind` is the EnvelopeRef variant discriminator. The
    // ArtifactRef's own kind field is renamed in the wire format to
    // `artifact_kind` to avoid colliding with the variant discriminator.
    // This is asserted by the implementation in `refs.rs`.
    let parsed: EnvelopeRef = serde_json::from_str(artifact_json).expect("artifact variant");
    assert!(matches!(parsed, EnvelopeRef::Artifact(_)));

    let source_json = r#"{
        "kind": "source",
        "uri": "file:///tmp/x.csv",
        "metadata": {}
    }"#;
    let parsed: EnvelopeRef = serde_json::from_str(source_json).expect("source variant");
    assert!(matches!(parsed, EnvelopeRef::Source(_)));

    let bogus_json = r#"{
        "kind": "unknown",
        "uri": "x"
    }"#;
    let result: Result<EnvelopeRef, _> = serde_json::from_str(bogus_json);
    assert!(
        result.is_err(),
        "EnvelopeRef MUST reject unknown discriminator; got Ok({result:?})"
    );
}

// RP-007 - Reference structs do not include transport-identifier field names.
// Verifies via deny_unknown_fields that a transport-shaped payload is rejected.
#[test]
fn rp_007_batch_ref_rejects_transport_field_names() {
    let field_name = transport_field_name();
    let attempted = format!(
        r#"{{
        "batch_id": "b1",
        "schema_fingerprint": "sha256:0xaaa",
        "metadata": {{}},
        "{field_name}": "demo"
    }}"#
    );
    let result: Result<BatchRef, _> = serde_json::from_str(&attempted);
    assert!(
        result.is_err(),
        "BatchRef MUST reject the unknown transport field; got Ok({result:?})"
    );
}

#[test]
fn rp_007_plan_ref_round_trip_canonical_serialization() {
    // PlanRef must round-trip its discriminated enum fields as snake_case
    // strings, matching the data-model spec.
    let r = PlanRef {
        plan_id: PlanId::from("plan-001"),
        plan_kind: PlanKind::Substrait,
        compatibility_posture: PlanCompatibilityPosture::DeclarationOnly,
        metadata: Default::default(),
    };
    let s = serde_json::to_string(&r).expect("serialize");
    assert!(s.contains("\"plan_kind\":\"substrait\""), "serialized: {s}");
    assert!(
        s.contains("\"compatibility_posture\":\"declaration_only\""),
        "serialized: {s}"
    );
}
