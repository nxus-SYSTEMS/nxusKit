//! T020 (US1) — Provider readiness DTO + canonical serialization test.
//!
//! The CLI-binary `doctor --json` projection test lives in
//! `packages/nxuskit-engine/crates/nxuskit-cli/tests/contract_v11_doctor.rs`
//! to keep CLI binary smoke close to the CLI crate. This test pins the
//! public Rust DTO shape: readiness postures, capability-truth entries,
//! domain coverage, and canonical-hash determinism.

use nxuskit::contract::v1_1::{
    CONTRACT_VERSION, CapabilityDomain, CapabilityId, CapabilityPosture, CapabilityTruthEntry,
    ProviderId, ProviderReadiness, ReadinessPosture, ReadinessSnapshot, TierPosture,
};

#[test]
fn readiness_snapshot_carries_contract_version_and_round_trips() {
    let snapshot = ReadinessSnapshot::from_pure(
        "2026-05-22T17:00:00Z",
        vec![ProviderReadiness {
            provider_id: ProviderId::from("loopback"),
            posture: ReadinessPosture::Ready,
            tier_posture: TierPosture::Ce,
            local_provider: true,
            models: vec![],
            timeouts_ms: Default::default(),
            last_error: None,
            metadata: Default::default(),
        }],
        vec![CapabilityTruthEntry {
            capability_id: CapabilityId::from("llm.text_completion"),
            domain: CapabilityDomain::Llm,
            posture: CapabilityPosture::Native,
            tier: TierPosture::Ce,
            providers: vec![ProviderId::from("loopback")],
        }],
    );
    assert_eq!(snapshot.contract_version, CONTRACT_VERSION);

    let json = serde_json::to_string(&snapshot).unwrap();
    let back: ReadinessSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(back.providers.len(), 1);
    assert_eq!(back.providers[0].provider_id.as_str(), "loopback");
    assert_eq!(back.providers[0].posture, ReadinessPosture::Ready);
    assert_eq!(back.capabilities.len(), 1);
    assert_eq!(back.capabilities[0].domain, CapabilityDomain::Llm);
}

#[test]
fn readiness_posture_enumerates_seven_states_with_snake_case_serialization() {
    // SC-004: ready, auth_missing, auth_invalid, unreachable, timeout,
    // tier_restricted, internal_only are all distinct, machine-readable
    // states.
    use ReadinessPosture::*;
    let all = [
        Ready,
        AuthMissing,
        AuthInvalid,
        Unreachable,
        Timeout,
        TierRestricted,
        InternalOnly,
    ];
    let expected = [
        "\"ready\"",
        "\"auth_missing\"",
        "\"auth_invalid\"",
        "\"unreachable\"",
        "\"timeout\"",
        "\"tier_restricted\"",
        "\"internal_only\"",
    ];
    for (p, want) in all.iter().zip(expected.iter()) {
        assert_eq!(&serde_json::to_string(p).unwrap(), want);
    }
    // Distinctness: a stub collapsing auth_missing/auth_invalid into a single
    // "auth_failure" string would fail the round-trip comparison below.
    let m_json = serde_json::to_string(&AuthMissing).unwrap();
    let i_json = serde_json::to_string(&AuthInvalid).unwrap();
    assert_ne!(
        m_json, i_json,
        "auth_missing must be distinct from auth_invalid"
    );
}

#[test]
fn capability_truth_entries_cover_non_llm_domains() {
    // FR-009 / SC-005: capability truth must cover non-LLM domains, not
    // only LLM booleans.
    let entries = [
        CapabilityTruthEntry {
            capability_id: CapabilityId::from("rule_eval.generic"),
            domain: CapabilityDomain::RuleEval,
            posture: CapabilityPosture::Native,
            tier: TierPosture::Ce,
            providers: vec![ProviderId::from("clips")],
        },
        CapabilityTruthEntry {
            capability_id: CapabilityId::from("solver.smt"),
            domain: CapabilityDomain::Solver,
            posture: CapabilityPosture::Unavailable,
            tier: TierPosture::Pro,
            providers: vec![],
        },
        CapabilityTruthEntry {
            capability_id: CapabilityId::from("data_store_search"),
            domain: CapabilityDomain::DataStoreSearch,
            posture: CapabilityPosture::Unavailable,
            tier: TierPosture::Ce,
            providers: vec![],
        },
    ];
    let json = serde_json::to_string(&entries).unwrap();
    let back: Vec<CapabilityTruthEntry> = serde_json::from_str(&json).unwrap();
    assert_eq!(back[0].domain, CapabilityDomain::RuleEval);
    assert_eq!(back[1].domain, CapabilityDomain::Solver);
    assert_eq!(back[1].posture, CapabilityPosture::Unavailable);
    assert_eq!(back[1].tier, TierPosture::Pro);
    assert_eq!(back[2].domain, CapabilityDomain::DataStoreSearch);
}

#[test]
fn readiness_snapshot_canonical_hash_is_deterministic_across_serializations() {
    let s1 = sample_snapshot();
    let s2 = sample_snapshot();
    let h1 = s1.canonical_hash().expect("hash s1");
    let h2 = s2.canonical_hash().expect("hash s2");
    assert_eq!(h1, h2, "identical content must hash identically");
}

#[test]
fn readiness_snapshot_canonical_hash_changes_when_provider_posture_changes() {
    let s1 = sample_snapshot();
    let mut s2 = sample_snapshot();
    s2.providers[0].posture = ReadinessPosture::Unreachable;
    let h1 = s1.canonical_hash().expect("hash s1");
    let h2 = s2.canonical_hash().expect("hash s2");
    assert_ne!(h1, h2, "changing posture must change canonical hash");
}

fn sample_snapshot() -> ReadinessSnapshot {
    ReadinessSnapshot::from_pure(
        "2026-05-22T17:00:00Z",
        vec![ProviderReadiness {
            provider_id: ProviderId::from("loopback"),
            posture: ReadinessPosture::Ready,
            tier_posture: TierPosture::Ce,
            local_provider: true,
            models: vec![],
            timeouts_ms: Default::default(),
            last_error: None,
            metadata: Default::default(),
        }],
        vec![],
    )
}

// -----------------------------------------------------------------------
// Feature 106 / US1 — additive `ProviderModel.metadata.contract_aliases`
// and `alias_only` fields. R7 decision: SHIP in v1.1.0 RC.
// FR-002, FR-004, FR-005, SC-002.
// -----------------------------------------------------------------------

#[test]
fn provider_model_metadata_default_has_empty_contract_aliases() {
    use nxuskit::contract::v1_1::ProviderModelMetadata;
    let m = ProviderModelMetadata::default();
    assert!(m.contract_aliases.is_empty());
    assert!(!m.alias_only);
}

#[test]
fn provider_model_with_empty_metadata_skips_alias_fields_on_serialize() {
    use nxuskit::contract::v1_1::{CapabilityId, ProviderModel, ProviderModelMetadata};
    let model = ProviderModel {
        id: "echo".to_string(),
        capabilities: vec![CapabilityId::from("llm.text_completion")],
        deprecated: false,
        metadata: ProviderModelMetadata::default(),
    };
    let json = serde_json::to_string(&model).expect("serialize");
    assert!(
        !json.contains("contract_aliases"),
        "empty contract_aliases must be skipped: {json}"
    );
    assert!(
        !json.contains("alias_only"),
        "default alias_only=false must be skipped: {json}"
    );
}

#[test]
fn provider_model_round_trips_loopback_v1_alias_metadata() {
    use nxuskit::contract::v1_1::{CapabilityId, ProviderModel, ProviderModelMetadata};
    let model = ProviderModel {
        id: "echo".to_string(),
        capabilities: vec![CapabilityId::from("llm.text_completion")],
        deprecated: false,
        metadata: ProviderModelMetadata {
            contract_aliases: vec!["loopback:v1".to_string()],
            alias_only: false,
        },
    };
    let json = serde_json::to_string(&model).expect("serialize");
    assert!(json.contains("loopback:v1"), "alias missing: {json}");
    let parsed: ProviderModel = serde_json::from_str(&json).expect("round-trip");
    assert_eq!(
        parsed.metadata.contract_aliases,
        vec!["loopback:v1".to_string()]
    );
    assert!(!parsed.metadata.alias_only);
}

#[test]
fn provider_model_supports_alias_only_marker_for_declaration_rows() {
    use nxuskit::contract::v1_1::{ProviderModel, ProviderModelMetadata};
    let model = ProviderModel {
        id: "loopback:v1".to_string(),
        capabilities: Vec::new(),
        deprecated: false,
        metadata: ProviderModelMetadata {
            contract_aliases: Vec::new(),
            alias_only: true,
        },
    };
    let json = serde_json::to_string(&model).expect("serialize");
    assert!(json.contains("\"alias_only\":true"), "missing: {json}");
    let parsed: ProviderModel = serde_json::from_str(&json).expect("round-trip");
    assert!(parsed.metadata.alias_only);
}

#[test]
fn readiness_snapshot_accepts_loopback_alias_metadata_via_loader() {
    // FR-005: the public loader must accept the additive alias metadata.
    let json = r#"{
        "contract_version": "1.1",
        "captured_at": "2026-05-28T12:00:00Z",
        "providers": [
            {
                "provider_id": "loopback",
                "posture": "ready",
                "tier_posture": "ce",
                "local_provider": true,
                "models": [
                    {
                        "id": "echo",
                        "capabilities": [],
                        "metadata": {
                            "contract_aliases": ["loopback:v1"]
                        }
                    }
                ],
                "timeouts_ms": { "connect_ms": 400, "request_ms": 60000 },
                "last_error": null
            }
        ],
        "capabilities": []
    }"#;
    let snapshot: ReadinessSnapshot =
        serde_json::from_str(json).expect("public DTO accepts alias metadata");
    let loopback = snapshot
        .providers
        .iter()
        .find(|p| p.provider_id.as_str() == "loopback")
        .expect("loopback row");
    let echo = loopback
        .models
        .iter()
        .find(|m| m.id == "echo")
        .expect("echo row");
    assert_eq!(
        echo.metadata.contract_aliases,
        vec!["loopback:v1".to_string()]
    );
}
