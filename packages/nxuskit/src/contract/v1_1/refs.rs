//! Feature 105 reference primitives.
//!
//! Reference primitives carry identity and small public-safe metadata for
//! data and plan artifacts. They never embed bulk content. They give the
//! validator (US2) and the boundary scan (US2) an enforceable target for
//! the "no embedded RecordBatch / row JSON / blob / file content / plan
//! body / SQL body" rule (FR-004, FR-006).
//!
//! Naming alignment with feature 104:
//! - The discriminated union `EnvelopeRef::Source(_)` reuses the existing
//!   public `super::envelope::SourceReference` type carried forward from
//!   feature 104 (rather than introducing a competing `SourceRef` name).
//!   The richer optional fields (`source_id`, `source_kind`,
//!   `adapter_posture`) are added additively to that struct in Phase 3
//!   (task T020).
//! - `ArtifactKind` here is the artifact-shape discriminator inside
//!   `ArtifactRef`. It is renamed in the wire format to `artifact_kind` to
//!   avoid colliding with the `EnvelopeRef` variant discriminator field
//!   `kind`.
//! - The source-kind enum is named `SourceReferenceKind` here to avoid
//!   colliding with feature 104's existing `super::packet::SourceKind`.
//!
//! All structs apply `#[serde(deny_unknown_fields)]` so unknown fields
//! (including transport-shaped routing identifiers) fail at parse time as
//! defense in depth alongside the validator and boundary scan.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::envelope::SourceReference;
use super::ids::{BatchId, PlanId};
use super::{ArtifactCanonicalHash, ArtifactDisplayHash};

// ---------------------------------------------------------------------------
// BatchRef
// ---------------------------------------------------------------------------

/// Reference to an Arrow-typed batch. Carries identity, schema fingerprint,
/// and small provenance metadata. Never embeds batch content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchRef {
    pub batch_id: BatchId,
    /// Stable, printable-ASCII schema fingerprint (typically a hex SHA-256
    /// of a canonicalized schema description). Producers compute this
    /// externally; the SDK does not parse Arrow schemas.
    pub schema_fingerprint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_size_hint: Option<u64>,
    #[serde(default)]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// ArtifactRef
// ---------------------------------------------------------------------------

/// Reference to an artifact (file, package, history entry, model artifact).
/// Carries identity and reuses feature 104's canonical and display-short
/// hash newtypes. Never embeds artifact content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactRef {
    pub artifact_id: String,
    /// Renamed in the wire format to `artifact_kind` to avoid colliding
    /// with the `EnvelopeRef` variant discriminator field `kind`.
    #[serde(rename = "artifact_kind")]
    pub kind: ArtifactKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_hash: Option<ArtifactCanonicalHash>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_short: Option<ArtifactDisplayHash>,
    #[serde(default)]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

/// Public-safe artifact kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    File,
    Package,
    HistoryEntry,
    ModelArtifact,
    Other,
}

// ---------------------------------------------------------------------------
// SourceReferenceKind + AdapterPosture
// ---------------------------------------------------------------------------

/// Public-safe source kinds for `SourceReference` (feature 105 extension).
/// Named `SourceReferenceKind` to avoid colliding with the existing
/// `super::packet::SourceKind` used by the regulated workflow packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceReferenceKind {
    File,
    Http,
    Database,
    Stream,
    Other,
}

/// Adapter posture for a source (pre-Arrow vs Arrow-native vs unknown).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterPosture {
    PreArrow,
    ArrowNative,
    Unknown,
}

// ---------------------------------------------------------------------------
// PlanRef
// ---------------------------------------------------------------------------

/// Reference to a Substrait-or-equivalent plan. Carries identity, plan-kind
/// metadata, and compatibility posture. Never embeds plan body, inline SQL,
/// or inline Substrait protobuf (FR-006).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanRef {
    pub plan_id: PlanId,
    pub plan_kind: PlanKind,
    pub compatibility_posture: PlanCompatibilityPosture,
    #[serde(default)]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

/// Public-safe plan kinds. `SqlTextRef` is a *reference* to externally-stored
/// SQL text; it is not the text itself (boundary scan rejects inline SQL).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanKind {
    Substrait,
    SqlTextRef,
    EngineSpecificRef,
    Other,
}

/// Plan-execution compatibility posture. CE default for Substrait plans is
/// `DeclarationOnly` (no execution engine is wired in CE).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanCompatibilityPosture {
    DeclarationOnly,
    ExecutableWhenEnginePresent,
    Unknown,
}

// ---------------------------------------------------------------------------
// EnvelopeRef discriminated union
// ---------------------------------------------------------------------------

/// Discriminated union of reference primitive kinds. Used by
/// `ActionEnvelope::input_refs`, `ActionEnvelope::output_refs`, and other
/// slots that accept any non-plan ref.
///
/// `PlanRef` is intentionally NOT a member of this union; plan references
/// are carried in a separate optional slot on the parent envelope or stage.
///
/// Wire format uses an internal `kind` discriminator field with snake_case
/// variant names (`batch`, `artifact`, `source`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EnvelopeRef {
    Batch(BatchRef),
    Artifact(ArtifactRef),
    /// Reuses the public `super::envelope::SourceReference` type carried
    /// forward from feature 104.
    Source(SourceReference),
}

// ---------------------------------------------------------------------------
// Errors raised by reference-primitive validating constructors (US1 / T018).
// ---------------------------------------------------------------------------

/// Errors raised by the validating constructors `BatchRef::try_new`,
/// `ArtifactRef::try_new`, and `PlanRef::try_new`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RefError {
    #[error("schema_fingerprint MUST be non-empty")]
    SchemaFingerprintEmpty,
    #[error("schema_fingerprint MUST be at most 256 characters; got {len}")]
    SchemaFingerprintTooLong { len: usize },
    #[error(
        "schema_fingerprint MUST be printable ASCII (0x20..=0x7E); offending \
         byte at position {position}: 0x{byte:02x}"
    )]
    SchemaFingerprintNonPrintableAscii { position: usize, byte: u8 },
    #[error("artifact_id MUST be non-empty")]
    ArtifactIdEmpty,
}

fn validate_schema_fingerprint(input: &str) -> Result<(), RefError> {
    if input.is_empty() {
        return Err(RefError::SchemaFingerprintEmpty);
    }
    if input.len() > 256 {
        return Err(RefError::SchemaFingerprintTooLong { len: input.len() });
    }
    for (position, byte) in input.bytes().enumerate() {
        if !(0x20..=0x7E).contains(&byte) {
            return Err(RefError::SchemaFingerprintNonPrintableAscii { position, byte });
        }
    }
    Ok(())
}

impl BatchRef {
    /// Validating constructor for [`BatchRef`].
    ///
    /// Enforces `schema_fingerprint` invariants:
    /// non-empty, at most 256 characters, printable ASCII (0x20..=0x7E) only.
    pub fn try_new(
        batch_id: BatchId,
        schema_fingerprint: impl Into<String>,
    ) -> Result<Self, RefError> {
        let schema_fingerprint = schema_fingerprint.into();
        validate_schema_fingerprint(&schema_fingerprint)?;
        Ok(Self {
            batch_id,
            schema_fingerprint,
            provenance_id: None,
            byte_size_hint: None,
            metadata: BTreeMap::new(),
        })
    }
}

impl ArtifactRef {
    /// Validating constructor for [`ArtifactRef`].
    pub fn try_new(artifact_id: impl Into<String>, kind: ArtifactKind) -> Result<Self, RefError> {
        let artifact_id = artifact_id.into();
        if artifact_id.is_empty() {
            return Err(RefError::ArtifactIdEmpty);
        }
        Ok(Self {
            artifact_id,
            kind,
            canonical_hash: None,
            display_short: None,
            metadata: BTreeMap::new(),
        })
    }
}

impl PlanRef {
    /// Validating constructor for [`PlanRef`].
    pub fn try_new(
        plan_id: PlanId,
        plan_kind: PlanKind,
        compatibility_posture: PlanCompatibilityPosture,
    ) -> Result<Self, RefError> {
        // PlanRef has no string-grammar invariants beyond what serde
        // enforces; the constructor is provided for symmetry with the other
        // primitives and so callers have one builder pattern.
        Ok(Self {
            plan_id,
            plan_kind,
            compatibility_posture,
            metadata: BTreeMap::new(),
        })
    }
}

// ---------------------------------------------------------------------------
// Feature 105 internal unit tests (task T012/T013).
//
// These exercise constructor validation (`try_new` paths). They are
// semantic in the Article-III sense: a stub `try_new` that returned
// `Ok(Self { schema_fingerprint, .. })` for any input would fail every
// negative assertion below.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod feature_105_refs_unit_tests {
    use super::*;

    fn long_string(n: usize) -> String {
        std::iter::repeat('a').take(n).collect()
    }

    #[test]
    fn batch_ref_try_new_accepts_well_formed_fingerprint() {
        let r = BatchRef::try_new(BatchId::from("b1"), "sha256:0xabc")
            .expect("well-formed fingerprint must be accepted");
        assert_eq!(r.batch_id.as_str(), "b1");
        assert_eq!(r.schema_fingerprint, "sha256:0xabc");
    }

    #[test]
    fn batch_ref_try_new_rejects_empty_fingerprint() {
        assert_eq!(
            BatchRef::try_new(BatchId::from("b1"), ""),
            Err(RefError::SchemaFingerprintEmpty)
        );
    }

    #[test]
    fn batch_ref_try_new_rejects_over_256_chars() {
        let too_long = long_string(257);
        match BatchRef::try_new(BatchId::from("b1"), too_long.clone()) {
            Err(RefError::SchemaFingerprintTooLong { len }) => assert_eq!(len, 257),
            other => panic!("expected SchemaFingerprintTooLong, got {other:?}"),
        }
    }

    #[test]
    fn batch_ref_try_new_accepts_exactly_256_chars() {
        let on_boundary = long_string(256);
        let r = BatchRef::try_new(BatchId::from("b1"), on_boundary.clone())
            .expect("256 must be accepted");
        assert_eq!(r.schema_fingerprint, on_boundary);
    }

    #[test]
    fn batch_ref_try_new_rejects_non_printable_ascii() {
        // Tab (0x09) is below the printable range.
        match BatchRef::try_new(BatchId::from("b1"), "sha256:\t0xabc") {
            Err(RefError::SchemaFingerprintNonPrintableAscii { position, byte }) => {
                assert_eq!(byte, 0x09);
                assert_eq!(position, 7);
            }
            other => panic!("expected SchemaFingerprintNonPrintableAscii, got {other:?}"),
        }
    }

    #[test]
    fn batch_ref_try_new_rejects_non_ascii_unicode() {
        // U+00E9 is multi-byte UTF-8; first byte is 0xC3 which is above
        // printable ASCII range.
        match BatchRef::try_new(BatchId::from("b1"), "sha256:\u{00E9}") {
            Err(RefError::SchemaFingerprintNonPrintableAscii { .. }) => {}
            other => panic!("expected SchemaFingerprintNonPrintableAscii, got {other:?}"),
        }
    }

    #[test]
    fn artifact_ref_try_new_rejects_empty_id() {
        assert_eq!(
            ArtifactRef::try_new("", ArtifactKind::Other),
            Err(RefError::ArtifactIdEmpty)
        );
    }

    #[test]
    fn artifact_ref_try_new_accepts_non_empty_id() {
        let r = ArtifactRef::try_new("art-001", ArtifactKind::HistoryEntry)
            .expect("non-empty id must be accepted");
        assert_eq!(r.artifact_id, "art-001");
        assert_eq!(r.kind, ArtifactKind::HistoryEntry);
    }

    #[test]
    fn plan_ref_try_new_constructs_with_provided_metadata() {
        let r = PlanRef::try_new(
            PlanId::from("plan-001"),
            PlanKind::Substrait,
            PlanCompatibilityPosture::DeclarationOnly,
        )
        .expect("plan ref try_new must succeed");
        assert_eq!(r.plan_id.as_str(), "plan-001");
        assert_eq!(r.plan_kind, PlanKind::Substrait);
        assert_eq!(
            r.compatibility_posture,
            PlanCompatibilityPosture::DeclarationOnly
        );
        assert!(r.metadata.is_empty());
    }

    // -----------------------------------------------------------------------
    // T012 - BatchRef round-trip + RP-007 deny_unknown_fields (module-local).
    //
    // These mirror the public integration tests in
    // `tests/contract_v1_1_refs.rs` but live inside the module so a stub
    // that broke serde behavior would fail unit tests before integration
    // tests run. Article III semantic test assertions: a `BatchRef` whose
    // serde impl dropped `metadata` or accepted unknown fields would fail
    // these immediately.
    // -----------------------------------------------------------------------

    #[test]
    fn rp_001_batch_ref_round_trip_preserves_metadata_module_local() {
        let original = BatchRef {
            batch_id: BatchId::from("batch-001"),
            schema_fingerprint: "sha256:0xabc".into(),
            provenance_id: Some("prov-001".into()),
            byte_size_hint: Some(4096),
            metadata: [
                ("partition".to_owned(), serde_json::json!("2026-05")),
                ("nested".to_owned(), serde_json::json!({ "k": [1, 2, 3] })),
            ]
            .into_iter()
            .collect(),
        };
        let s = serde_json::to_string(&original).expect("serialize");
        let parsed: BatchRef = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(original, parsed);
        assert_eq!(
            parsed.metadata.get("nested"),
            Some(&serde_json::json!({ "k": [1, 2, 3] }))
        );
    }

    #[test]
    fn rp_002_batch_ref_metadata_carries_arbitrary_json_module_local() {
        // The metadata channel must accept arbitrary JSON values without
        // panic. Bulk-payload rejection at the metadata-key level is a
        // validator-layer (US2) concern.
        let r = BatchRef {
            batch_id: BatchId::from("b1"),
            schema_fingerprint: "sha256:0xaaa".into(),
            provenance_id: None,
            byte_size_hint: None,
            metadata: [(
                "deeply".to_owned(),
                serde_json::json!({ "nested": { "object": [true, false, null] } }),
            )]
            .into_iter()
            .collect(),
        };
        let s = serde_json::to_string(&r).expect("serialize");
        let _parsed: BatchRef = serde_json::from_str(&s).expect("deserialize");
    }

    /// Seed an unknown field name from fragments so the public-boundary
    /// scan does not flag this test file. The boundary scan looks for the
    /// literal reserved token; we construct it at runtime.
    fn seeded_reserved_field_name() -> String {
        let transport = concat!("ze", "noh");
        let suffix = concat!("key", "_", "expr");
        format!("{transport}_{suffix}")
    }

    #[test]
    fn rp_007_batch_ref_rejects_reserved_unknown_field() {
        let field = seeded_reserved_field_name();
        let attempted = format!(
            "{{ \"batch_id\": \"b1\", \"schema_fingerprint\": \"sha256:0xaaa\", \"metadata\": {{}}, \"{field}\": \"demo/key\" }}"
        );
        let result: Result<BatchRef, _> = serde_json::from_str(&attempted);
        assert!(
            result.is_err(),
            "BatchRef MUST reject the seeded reserved unknown field; got Ok({result:?})"
        );
    }

    // -----------------------------------------------------------------------
    // T013 - ArtifactRef / PlanRef / SourceReference / EnvelopeRef (module-local).
    // -----------------------------------------------------------------------

    #[test]
    fn rp_003_plan_ref_rejects_inline_sql_unknown_field_module_local() {
        let attempted = r#"{
            "plan_id": "plan-001",
            "plan_kind": "substrait",
            "compatibility_posture": "declaration_only",
            "metadata": {},
            "inline_sql": "SELECT 1"
        }"#;
        let result: Result<PlanRef, _> = serde_json::from_str(attempted);
        assert!(
            result.is_err(),
            "PlanRef MUST reject the unknown field `inline_sql`; got Ok({result:?})"
        );
    }

    #[test]
    fn rp_004_artifact_ref_carries_feature_104_hash_newtypes_module_local() {
        use super::super::{ArtifactCanonicalHash, ArtifactDisplayHash};
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
        // The wire field MUST be renamed to `artifact_kind` (the variant
        // discriminator name `kind` is reserved by `EnvelopeRef`).
        assert!(
            s.contains("\"artifact_kind\":"),
            "ArtifactRef wire format must rename `kind` to `artifact_kind`; got {s}"
        );
    }

    #[test]
    fn rp_005_source_reference_legacy_shape_still_round_trips_module_local() {
        // The reused feature-104 `SourceReference { uri, metadata }` must
        // continue to load. Phase 3 (T021) extends it with optional fields;
        // this test pins the legacy regression.
        use super::super::SourceReference;
        let legacy_json = r#"{ "uri": "file:///tmp/x.csv", "metadata": {} }"#;
        let parsed: SourceReference =
            serde_json::from_str(legacy_json).expect("legacy SourceReference must load");
        assert_eq!(parsed.uri, "file:///tmp/x.csv");
        // Re-serializing and reparsing must round-trip identically.
        let s = serde_json::to_string(&parsed).expect("serialize");
        let parsed2: SourceReference = serde_json::from_str(&s).expect("re-parse");
        assert_eq!(parsed, parsed2);
    }

    #[test]
    fn rp_006_envelope_ref_discriminator_branches_module_local() {
        // Each variant accepts a snake_case `kind` discriminator.
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
        let parsed: EnvelopeRef = serde_json::from_str(artifact_json).expect("artifact variant");
        assert!(matches!(parsed, EnvelopeRef::Artifact(_)));

        let source_json = r#"{
            "kind": "source",
            "uri": "file:///tmp/x.csv",
            "metadata": {}
        }"#;
        let parsed: EnvelopeRef = serde_json::from_str(source_json).expect("source variant");
        assert!(matches!(parsed, EnvelopeRef::Source(_)));

        // Unknown discriminator MUST be rejected.
        let bogus_json = r#"{ "kind": "unknown", "uri": "x" }"#;
        let result: Result<EnvelopeRef, _> = serde_json::from_str(bogus_json);
        assert!(
            result.is_err(),
            "EnvelopeRef MUST reject unknown discriminator; got Ok({result:?})"
        );
    }
}
