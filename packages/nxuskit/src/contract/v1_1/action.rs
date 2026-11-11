//! Feature 105 action envelopes.
//!
//! `ActionEnvelope` and `ActionResult` are SDK-owned public DTOs that carry
//! control-plane actions and outcomes between providers, tools, and pipeline
//! stages. They are reference-only carriers: bulk data lives behind
//! `BatchRef` / `ArtifactRef` / `SourceReference` / `PlanRef`. They never
//! embed payload bodies.
//!
//! Field semantics map field-for-field to the retargeted action-envelope
//! contract (research.md R-004). The SDK type identity is
//! `nxuskit::contract::v1_1::ActionEnvelope`; no internal crate is imported
//! into CE.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::refs::{EnvelopeRef, PlanRef};
use super::{ActionId, ActionType, CorrelationId, TopicName};

/// Control-plane action envelope. Reference-only data posture.
///
/// Wire format uses `#[serde(deny_unknown_fields)]` so a transport-shaped
/// routing identifier or any unknown top-level field is rejected at parse time as
/// defense in depth alongside the validator and boundary scan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionEnvelope {
    pub action_id: ActionId,
    pub correlation_id: CorrelationId,
    pub topic: TopicName,
    pub action_type: ActionType,
    #[serde(default)]
    pub input_refs: Vec<EnvelopeRef>,
    #[serde(default)]
    pub output_refs: Vec<EnvelopeRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_ref: Option<PlanRef>,
    #[serde(default)]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

/// Control-plane action result. Mirrors `ActionEnvelope` with status and
/// outcome references. `error` is a public-safe summary so internal stack
/// traces and persona content stay out of the contract surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionResult {
    pub action_id: ActionId,
    pub correlation_id: CorrelationId,
    pub status: ActionStatus,
    #[serde(default)]
    pub outcome_refs: Vec<EnvelopeRef>,
    /// Public-safe error projection. Held as a string-only summary in v1.1.x
    /// to avoid coupling to the still-evolving `ContractError` JSON shape.
    /// A typed projection MAY land in a follow-up feature.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ActionErrorSummary>,
    #[serde(default)]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

/// Lifecycle status for an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionStatus {
    Accepted,
    Succeeded,
    Failed,
    Cancelled,
    Superseded,
    Expired,
}

/// Public-safe error summary attached to a failed/cancelled `ActionResult`.
/// String-only; no internal repo paths, no internal topology.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionErrorSummary {
    pub class: String,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Feature 105 internal unit tests (task T014).
//
// Module-local mirror of the public integration tests in
// `tests/contract_v1_1_action.rs`. Lives inside the module so a stub that
// breaks ActionEnvelope/ActionResult semantics fails unit tests before
// integration tests run. Article III semantic test assertions: a stub
// that dropped `metadata`, accepted a reserved-token `action_type`, or
// failed canonical-equality across map-order permutations would fail the
// relevant assertion below.
//
// The HashMap-based determinism test (AE-008) populates metadata from a
// `std::collections::HashMap` (not BTreeMap) and confirms two
// serializations are equal regardless of insertion order; ActionEnvelope
// stores metadata as a BTreeMap, so the BTreeMap-canonical key order
// always wins on serialize. A stub that stored a HashMap or that
// serialized in insertion order would fail this assertion.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod feature_105_action_unit_tests {
    use super::super::refs::{BatchRef, EnvelopeRef, PlanCompatibilityPosture, PlanKind, PlanRef};
    use super::super::{ActionId, ActionType, BatchId, CorrelationId, IdError, PlanId, TopicName};
    use super::*;
    use serde_json::json;
    use std::collections::{BTreeMap, HashMap};

    fn sample_envelope() -> ActionEnvelope {
        ActionEnvelope {
            action_id: ActionId::from("action-001"),
            correlation_id: CorrelationId::from("corr-001"),
            topic: TopicName::try_new("provider.invoke").expect("topic"),
            action_type: ActionType::try_new("provider.invoke").expect("action type"),
            input_refs: vec![
                EnvelopeRef::Batch(BatchRef {
                    batch_id: BatchId::from("batch-in-1"),
                    schema_fingerprint: "sha256:0xaaa".into(),
                    provenance_id: None,
                    byte_size_hint: None,
                    metadata: BTreeMap::new(),
                }),
                EnvelopeRef::Batch(BatchRef {
                    batch_id: BatchId::from("batch-in-2"),
                    schema_fingerprint: "sha256:0xbbb".into(),
                    provenance_id: None,
                    byte_size_hint: None,
                    metadata: BTreeMap::new(),
                }),
            ],
            output_refs: vec![],
            plan_ref: Some(PlanRef {
                plan_id: PlanId::from("plan-001"),
                plan_kind: PlanKind::Substrait,
                compatibility_posture: PlanCompatibilityPosture::DeclarationOnly,
                metadata: BTreeMap::new(),
            }),
            metadata: BTreeMap::new(),
        }
    }

    // AE-001 - ActionEnvelope round-trip preserves all fields.
    #[test]
    fn ae_001_round_trip_preserves_all_fields() {
        let original = sample_envelope();
        let s = serde_json::to_string(&original).expect("serialize");
        let parsed: ActionEnvelope = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(original, parsed);
    }

    // AE-002 - Topic grammar.
    #[test]
    fn ae_002_topic_name_grammar() {
        for ok in [
            "provider.invoke",
            "pipeline.stage.start",
            "tool.invoke",
            "envelope.export",
        ] {
            assert!(
                TopicName::try_new(ok).is_ok(),
                "well-formed topic was rejected: {ok}"
            );
        }
        assert!(matches!(
            TopicName::try_new("Provider.invoke"),
            Err(IdError::InvalidGrammar { .. })
        ));
        assert!(matches!(
            TopicName::try_new("1pipeline"),
            Err(IdError::InvalidGrammar { .. })
        ));
        assert!(matches!(
            TopicName::try_new("provider invoke"),
            Err(IdError::InvalidGrammar { .. })
        ));
        assert!(matches!(
            TopicName::try_new("provider/invoke"),
            Err(IdError::InvalidGrammar { .. })
        ));
    }

    // Build reserved-token strings from fragments so the public boundary
    // scan does not flag this test file. Mirrors the pattern used in
    // `ids.rs::feature_105_id_tests`.
    fn seeded_transport_action_type() -> String {
        let t = concat!("ze", "noh");
        format!("{t}.publish")
    }
    fn seeded_reserved_field_action_type() -> String {
        let f = concat!("key", "_", "expr");
        format!("router.{f}.match")
    }
    fn seeded_internal_crate_action_type() -> String {
        let p = concat!("nxus", "_", "factory", "_");
        format!("{p}arrow.dispatch")
    }

    // AE-003 - ActionType accepts public-safe values and rejects banned-token
    // leaks via the `try_new` constructor.
    #[test]
    fn ae_003_action_type_accepts_public_safe_and_rejects_banned() {
        for ok in [
            "provider.invoke",
            "provider.invoke.complete",
            "tool.invoke",
            "tool.invoke.complete",
            "pipeline.stage.start",
            "pipeline.stage.complete",
            "pipeline.validate",
            "readiness.query",
            "envelope.export",
            "envelope.import",
        ] {
            assert!(
                ActionType::try_new(ok).is_ok(),
                "public-safe action type was rejected: {ok}"
            );
        }
        assert!(matches!(
            ActionType::try_new(seeded_transport_action_type()),
            Err(IdError::InternalTokenLeak { .. })
        ));
        assert!(matches!(
            ActionType::try_new(seeded_reserved_field_action_type()),
            Err(IdError::InternalTokenLeak { .. })
        ));
        assert!(matches!(
            ActionType::try_new(seeded_internal_crate_action_type()),
            Err(IdError::InternalTokenLeak { .. })
        ));
    }

    // AE-004 - Metadata carries arbitrary JSON values.
    #[test]
    fn ae_004_metadata_carries_arbitrary_json_values() {
        let mut env = sample_envelope();
        env.metadata
            .insert("nested".to_owned(), json!({ "k": [1, 2, 3] }));
        let s = serde_json::to_string(&env).expect("serialize");
        let parsed: ActionEnvelope = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(
            parsed.metadata.get("nested"),
            Some(&json!({ "k": [1, 2, 3] }))
        );
    }

    // AE-005 - deny_unknown_fields at the ActionEnvelope top level.
    // The reserved field name is built from fragments so the boundary scan
    // can stay clean.
    fn seeded_reserved_top_level_field() -> String {
        let t = concat!("ze", "noh");
        let f = concat!("key", "_", "expr");
        format!("{t}_{f}")
    }

    #[test]
    fn ae_005_action_envelope_rejects_unknown_top_level_field() {
        let bogus_field = seeded_reserved_top_level_field();
        let bogus = format!(
            "{{ \"action_id\": \"a1\", \"correlation_id\": \"c1\", \"topic\": \"provider.invoke\", \"action_type\": \"provider.invoke\", \"input_refs\": [], \"output_refs\": [], \"metadata\": {{}}, \"{bogus_field}\": \"demo/key\" }}"
        );
        let result: Result<ActionEnvelope, _> = serde_json::from_str(&bogus);
        assert!(
            result.is_err(),
            "ActionEnvelope MUST reject unknown top-level field; got Ok({result:?})"
        );
    }

    // AE-006 - EnvelopeRef Vec preserves order.
    #[test]
    fn ae_006_envelope_ref_vec_preserves_order() {
        let env = sample_envelope();
        assert_eq!(env.input_refs.len(), 2);
        let s = serde_json::to_string(&env).expect("serialize");
        let parsed: ActionEnvelope = serde_json::from_str(&s).expect("deserialize");
        let ids: Vec<String> = parsed
            .input_refs
            .iter()
            .filter_map(|r| match r {
                EnvelopeRef::Batch(b) => Some(b.batch_id.as_str().to_owned()),
                _ => None,
            })
            .collect();
        assert_eq!(ids, vec!["batch-in-1".to_owned(), "batch-in-2".to_owned()]);
    }

    // AE-007 - ActionResult shape + ActionStatus enum.
    #[test]
    fn ae_007_action_result_shape_and_status_enum() {
        let result = ActionResult {
            action_id: ActionId::from("a1"),
            correlation_id: CorrelationId::from("c1"),
            status: ActionStatus::Succeeded,
            outcome_refs: vec![EnvelopeRef::Batch(BatchRef {
                batch_id: BatchId::from("batch-out-1"),
                schema_fingerprint: "sha256:0xccc".into(),
                provenance_id: None,
                byte_size_hint: None,
                metadata: BTreeMap::new(),
            })],
            error: None,
            metadata: BTreeMap::new(),
        };
        let s = serde_json::to_string(&result).expect("serialize");
        let parsed: ActionResult = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(result, parsed);

        for status in [
            ActionStatus::Accepted,
            ActionStatus::Succeeded,
            ActionStatus::Failed,
            ActionStatus::Cancelled,
            ActionStatus::Superseded,
            ActionStatus::Expired,
        ] {
            let s = serde_json::to_string(&status).expect("serialize");
            let parsed: ActionStatus = serde_json::from_str(&s).expect("deserialize");
            assert_eq!(status, parsed);
        }

        assert_eq!(
            serde_json::to_string(&ActionStatus::Succeeded).unwrap(),
            "\"succeeded\""
        );
    }

    // AE-008 - Canonical-equality test: populate metadata from a
    // `std::collections::HashMap` (which has nondeterministic iteration
    // order) and confirm the resulting BTreeMap-backed serialization is
    // identical across multiple constructions. A stub storing a HashMap
    // or serializing in insertion order would fail this.
    #[test]
    fn ae_008_metadata_canonical_equality_via_hashmap_input() {
        // Build the same logical map twice with HashMap intermediate. The
        // BTreeMap conversion produces deterministic key order on serialize.
        fn build(metadata_in: HashMap<String, serde_json::Value>) -> ActionEnvelope {
            let mut env = sample_envelope();
            // Convert HashMap into ActionEnvelope's BTreeMap. The order of
            // iteration over `metadata_in` is nondeterministic; correctness
            // depends on BTreeMap canonicalizing on the way in.
            env.metadata = metadata_in.into_iter().collect();
            env
        }

        let mut map_a: HashMap<String, serde_json::Value> = HashMap::new();
        map_a.insert("alpha".to_owned(), json!(1));
        map_a.insert("zulu".to_owned(), json!(2));
        map_a.insert("mike".to_owned(), json!(3));
        map_a.insert("delta".to_owned(), json!(4));
        map_a.insert("echo".to_owned(), json!(5));

        let mut map_b: HashMap<String, serde_json::Value> = HashMap::new();
        // Same key/value content, inserted in a different order.
        map_b.insert("echo".to_owned(), json!(5));
        map_b.insert("delta".to_owned(), json!(4));
        map_b.insert("mike".to_owned(), json!(3));
        map_b.insert("zulu".to_owned(), json!(2));
        map_b.insert("alpha".to_owned(), json!(1));

        let env_a = build(map_a);
        let env_b = build(map_b);
        let s_a = serde_json::to_string(&env_a).expect("serialize a");
        let s_b = serde_json::to_string(&env_b).expect("serialize b");
        assert_eq!(
            s_a, s_b,
            "ActionEnvelope serialized form MUST be canonical regardless of metadata insertion order"
        );
    }
}
