//! Public contract tests for feature 105 action envelopes (task T007).
//!
//! Covers ActionEnvelope, ActionResult, ActionStatus, topic/action-type
//! grammar, metadata determinism, and the union-with-EnvelopeRef behavior
//! per `contracts/action-envelope.md` IDs AE-001..AE-008.
//!
//! Semantic tests: a stub ActionEnvelope that drops metadata, accepts a
//! transport-shaped action_type, or fails to round-trip canonical-hash-equal
//! across map-order permutations would each fail the relevant assertion.

use nxuskit::contract::v1_1::action::{ActionEnvelope, ActionResult, ActionStatus};
use nxuskit::contract::v1_1::refs::{
    BatchRef, EnvelopeRef, PlanCompatibilityPosture, PlanKind, PlanRef,
};
use nxuskit::contract::v1_1::{
    ActionId, ActionType, BatchId, CorrelationId, IdError, PlanId, TopicName,
};
use serde_json::json;
use std::collections::BTreeMap;

fn transport_token() -> &'static str {
    concat!("ze", "noh")
}

fn reserved_field_token() -> &'static str {
    concat!("key", "_", "expr")
}

fn internal_crate_prefix() -> &'static str {
    concat!("nxus", "_", "factory", "_")
}

fn transport_field_name() -> String {
    format!("{}_{}", transport_token(), reserved_field_token())
}

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
        metadata: [
            ("workflow".to_owned(), json!("peeler-smoke")),
            ("tier".to_owned(), json!("ce")),
        ]
        .into_iter()
        .collect(),
    }
}

// AE-001 - ActionEnvelope round-trip preserves all fields including
// EnvelopeRef union variants and PlanRef.
#[test]
fn ae_001_round_trip_preserves_all_fields() {
    let original = sample_envelope();
    let s = serde_json::to_string(&original).expect("serialize");
    let parsed: ActionEnvelope = serde_json::from_str(&s).expect("deserialize");
    assert_eq!(original, parsed);
}

// AE-002 - Topic grammar: rejects uppercase, leading digit, embedded
// whitespace, embedded slash; accepts public-safe values.
#[test]
fn ae_002_topic_name_grammar() {
    assert!(TopicName::try_new("provider.invoke").is_ok());
    assert!(TopicName::try_new("pipeline.stage.start").is_ok());
    assert!(TopicName::try_new("tool.invoke").is_ok());
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

// AE-003 - ActionType: accepts public-safe set; rejects reserved
// transport/internal tokens.
#[test]
fn ae_003_action_type_accepts_public_safe_and_rejects_banned_tokens() {
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
    let transport = ActionType::try_new(format!("{}.publish", transport_token()))
        .expect_err("transport token must be rejected");
    assert!(matches!(
        transport,
        IdError::InternalTokenLeak { token, .. } if token == transport_token()
    ));
    let reserved = ActionType::try_new(format!("router.{}.match", reserved_field_token()))
        .expect_err("reserved field token must be rejected");
    assert!(matches!(
        reserved,
        IdError::InternalTokenLeak { token, .. } if token == reserved_field_token()
    ));
    let internal = ActionType::try_new(format!("{}arrow.dispatch", internal_crate_prefix()))
        .expect_err("internal crate prefix must be rejected");
    assert!(matches!(
        internal,
        IdError::InternalTokenLeak { token, .. } if token == internal_crate_prefix()
    ));
}

// AE-004 - Metadata bulk-payload-key rejection is enforced by the validator
// layer (T027 in US2). At the type level, ActionEnvelope simply carries
// metadata as a JSON map; this is asserted by confirming arbitrary JSON
// values are accepted. The validator-level test lives in
// `contract_v11_pipeline.rs` once T027 lands.
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

// AE-005 - Validator-level transport-leak rejection on `metadata` values is
// enforced by T027. Type level: confirm the ActionEnvelope struct itself
// rejects an unknown top-level field via deny_unknown_fields.
#[test]
fn ae_005_action_envelope_rejects_unknown_top_level_field() {
    let field_name = transport_field_name();
    let bogus = format!(
        r#"{{
        "action_id": "a1",
        "correlation_id": "c1",
        "topic": "provider.invoke",
        "action_type": "provider.invoke",
        "input_refs": [],
        "output_refs": [],
        "metadata": {{}},
        "{field_name}": "demo"
    }}"#
    );
    let result: Result<ActionEnvelope, _> = serde_json::from_str(&bogus);
    assert!(
        result.is_err(),
        "ActionEnvelope MUST reject unknown top-level field; got Ok({result:?})"
    );
}

// AE-006 - Duplicate identities within input_refs / output_refs would be
// caught by a higher-level validator in US2. At the type level we confirm
// the Vec preserves order and length.
#[test]
fn ae_006_envelope_ref_vec_preserves_order() {
    let env = sample_envelope();
    assert_eq!(env.input_refs.len(), 2);
    let s = serde_json::to_string(&env).expect("serialize");
    let parsed: ActionEnvelope = serde_json::from_str(&s).expect("deserialize");
    let in_ids: Vec<String> = parsed
        .input_refs
        .iter()
        .filter_map(|r| match r {
            EnvelopeRef::Batch(b) => Some(b.batch_id.as_str().to_owned()),
            _ => None,
        })
        .collect();
    assert_eq!(
        in_ids,
        vec!["batch-in-1".to_owned(), "batch-in-2".to_owned()]
    );
}

// AE-007 - ActionResult shape and ActionStatus enum values.
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

    // Status discriminator round-trips as snake_case.
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

    // Snake_case wire format for ActionStatus::Succeeded.
    let s = serde_json::to_string(&ActionStatus::Succeeded).unwrap();
    assert_eq!(s, "\"succeeded\"");
}

// AE-008 - Metadata-order-insensitive canonical-equality. Build the same
// envelope two ways with metadata keys inserted in different orders; the
// serialized JSON must match because metadata is a BTreeMap (sorted keys).
#[test]
fn ae_008_metadata_btreemap_order_insensitive_canonical_form() {
    let env_a = ActionEnvelope {
        metadata: [
            ("alpha".to_owned(), json!(1)),
            ("zulu".to_owned(), json!(2)),
            ("mike".to_owned(), json!(3)),
        ]
        .into_iter()
        .collect(),
        ..sample_envelope()
    };
    let env_b = ActionEnvelope {
        metadata: [
            ("zulu".to_owned(), json!(2)),
            ("alpha".to_owned(), json!(1)),
            ("mike".to_owned(), json!(3)),
        ]
        .into_iter()
        .collect(),
        ..sample_envelope()
    };
    let s_a = serde_json::to_string(&env_a).expect("serialize a");
    let s_b = serde_json::to_string(&env_b).expect("serialize b");
    assert_eq!(s_a, s_b, "BTreeMap metadata must yield identical wire form");
}
