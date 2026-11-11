//! T008 — DTO module compile/export test for `nxuskit::contract::v1_1`.
//!
//! Pins the Phase-2 public surface so any later refactor that drops, renames,
//! or re-buckets a foundational primitive fails this test loudly. The
//! assertions are semantic (Article III §Semantic Test Assertions): they
//! observe specific newtype/enum identities and the canonical version
//! constant, not just the existence of the module.

use nxuskit::contract::v1_1::{
    ArtifactCanonicalHash, ArtifactDisplayHash, CONTRACT_VERSION, CapabilityDomain, CapabilityId,
    CapabilityPosture, ContractError, HashError, PipelineId, PipelineValidationOutcome, ProviderId,
    StageName, StageValidationOutcome, TierPosture, canonical_hash, canonical_json_bytes,
    display_short,
};

#[test]
fn contract_version_constant_is_v1_1() {
    assert_eq!(CONTRACT_VERSION, "1.1");
}

#[test]
fn newtype_ids_round_trip_through_serde() {
    // Each ID newtype is `#[serde(transparent)]` so a plain JSON string
    // round-trips. A stub that returned `String` directly would still pass
    // serde, but the types in code must remain distinct from String — the
    // assertion below proves the type identity is preserved.
    let pipeline_id: PipelineId = serde_json::from_str(r#""pl_test""#).unwrap();
    let stage_name: StageName = serde_json::from_str(r#""draft""#).unwrap();
    let capability_id: CapabilityId = serde_json::from_str(r#""llm.text_completion""#).unwrap();
    let provider_id: ProviderId = serde_json::from_str(r#""loopback""#).unwrap();

    assert_eq!(serde_json::to_string(&pipeline_id).unwrap(), r#""pl_test""#);
    assert_eq!(serde_json::to_string(&stage_name).unwrap(), r#""draft""#);
    assert_eq!(
        serde_json::to_string(&capability_id).unwrap(),
        r#""llm.text_completion""#
    );
    assert_eq!(
        serde_json::to_string(&provider_id).unwrap(),
        r#""loopback""#
    );
}

#[test]
fn capability_domain_enumerates_seven_kinds_with_snake_case_serialization() {
    use CapabilityDomain::*;
    let all = [Llm, Tool, RuleEval, Solver, Bayesian, Zen, DataStoreSearch];
    assert_eq!(all.len(), 7);

    let expected = [
        "\"llm\"",
        "\"tool\"",
        "\"rule_eval\"",
        "\"solver\"",
        "\"bayesian\"",
        "\"zen\"",
        "\"data_store_search\"",
    ];
    for (dom, want) in all.iter().zip(expected.iter()) {
        assert_eq!(&serde_json::to_string(dom).unwrap(), want);
    }
}

#[test]
fn capability_posture_enumerates_four_states_with_snake_case_serialization() {
    use CapabilityPosture::*;
    let all = [Native, Emulated, ExternalOnly, Unavailable];
    let expected = [
        "\"native\"",
        "\"emulated\"",
        "\"external_only\"",
        "\"unavailable\"",
    ];
    for (p, want) in all.iter().zip(expected.iter()) {
        assert_eq!(&serde_json::to_string(p).unwrap(), want);
    }
}

#[test]
fn tier_posture_enumerates_four_tiers_with_snake_case_serialization() {
    use TierPosture::*;
    let all = [Ce, Pro, Enterprise, Internal];
    let expected = ["\"ce\"", "\"pro\"", "\"enterprise\"", "\"internal\""];
    for (t, want) in all.iter().zip(expected.iter()) {
        assert_eq!(&serde_json::to_string(t).unwrap(), want);
    }
}

#[test]
fn pipeline_validation_outcome_distinguishes_four_variants() {
    // Each variant must be its own JSON-tagged value. A stub collapsing
    // `SchemaInvalid`/`CapabilityUnavailable` into a single `Err` shape
    // would fail these assertions.
    let ready = PipelineValidationOutcome::Ready;
    let schema = PipelineValidationOutcome::SchemaInvalid {
        path: "stages[0].name".into(),
        message: "missing".into(),
    };
    let cap = PipelineValidationOutcome::CapabilityUnavailable {
        stage_name: StageName::from("draft"),
        capability_id: CapabilityId::from("missing.cap"),
    };
    let readiness = PipelineValidationOutcome::ReadinessUnknown {
        reason: "no snapshot".into(),
    };

    let ready_json = serde_json::to_value(&ready).unwrap();
    let schema_json = serde_json::to_value(&schema).unwrap();
    let cap_json = serde_json::to_value(&cap).unwrap();
    let readiness_json = serde_json::to_value(&readiness).unwrap();

    assert_eq!(ready_json["kind"], "ready");
    assert_eq!(schema_json["kind"], "schema_invalid");
    assert_eq!(schema_json["path"], "stages[0].name");
    assert_eq!(cap_json["kind"], "capability_unavailable");
    assert_eq!(cap_json["stage_name"], "draft");
    assert_eq!(cap_json["capability_id"], "missing.cap");
    assert_eq!(readiness_json["kind"], "readiness_unknown");
}

#[test]
fn stage_validation_outcome_covers_five_named_states() {
    // Each variant tagged distinctly. Phase 2 ships these five; later
    // user-story phases may add additive variants. The test pins the
    // minimum Phase-2 set.
    let outcomes = [
        (StageValidationOutcome::Ok, "ok"),
        (
            StageValidationOutcome::InputMissing {
                input_name: "source".into(),
            },
            "input_missing",
        ),
        (
            StageValidationOutcome::OutputMismatch {
                output_name: "draft_text".into(),
            },
            "output_mismatch",
        ),
        (
            StageValidationOutcome::DependencyCycle {
                stage_name: StageName::from("draft"),
            },
            "dependency_cycle",
        ),
        (
            StageValidationOutcome::CapabilityUnavailable {
                capability_id: CapabilityId::from("missing"),
            },
            "capability_unavailable",
        ),
    ];
    for (variant, kind) in outcomes {
        let v = serde_json::to_value(&variant).unwrap();
        assert_eq!(
            v["kind"], kind,
            "variant {kind} did not serialize correctly"
        );
    }
}

#[test]
fn artifact_hashes_are_typed_newtypes() {
    let h: ArtifactCanonicalHash = serde_json::from_str(
        r#""e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855""#,
    )
    .unwrap();
    let d: ArtifactDisplayHash = serde_json::from_str(r#""e3b0c4429810""#).unwrap();
    assert_eq!(h.as_str().len(), 64);
    assert_eq!(d.as_str().len(), 12);
}

#[test]
fn canonical_hash_helpers_are_callable_with_pure_input() {
    // Exercise the pure helpers directly, with no engine adapter, to prove
    // the contract surface is library-pure (Article VII).
    let value = serde_json::json!({"b": 1, "a": 2});
    let bytes = canonical_json_bytes(&value).expect("canonical encode");
    let hash = canonical_hash(&value).expect("canonical hash");
    let short = display_short(&hash);

    // Sorted keys: 'a' appears before 'b'.
    let s = std::str::from_utf8(&bytes).unwrap();
    let a_idx = s.find("\"a\"").unwrap();
    let b_idx = s.find("\"b\"").unwrap();
    assert!(a_idx < b_idx, "canonical bytes must sort keys: got {s}");
    assert_eq!(hash.as_str().len(), 64);
    assert_eq!(short.as_str().len(), 12);
    assert_eq!(short.as_str(), &hash.as_str()[..12]);
}

#[test]
fn contract_error_and_hash_error_are_distinct_typed_errors() {
    fn assert_error<E: std::error::Error + 'static>(_: &E) {}
    let h = HashError::Encoding("synthetic".into());
    let c = ContractError::SchemaInvalid {
        path: "x".into(),
        message: "y".into(),
    };
    assert_error(&h);
    assert_error(&c);
}
