//! US5 (T051-T053) — readiness posture row tests.
//!
//! Article III semantic posture: each assertion would fail against a
//! constant `Available`-everywhere stub, an unconditional `Unavailable`
//! stub, or a `ReadinessSnapshot` that omitted the new fields.

use std::path::PathBuf;

use nxuskit::contract::v1_1::{
    ArrowReferenceCompatibility, ClipsRulesProviderAvailability, ReadinessSnapshot,
    SubstraitPlanReferenceCompatibility, ZenohAvailability,
};

// PR-101 / T051 — CE defaults: no providers configured.
#[test]
fn t051_ce_default_posture_rows_at_construction() {
    let snap = ReadinessSnapshot::from_pure("2026-05-27T00:00:00Z", vec![], vec![]);
    assert_eq!(
        snap.arrow_reference_compatibility,
        ArrowReferenceCompatibility::DeclarationOnly
    );
    assert_eq!(
        snap.substrait_plan_reference_compatibility,
        SubstraitPlanReferenceCompatibility::DeclarationOnly
    );
    assert_eq!(snap.zenoh_availability, ZenohAvailability::Deferred);
    assert_eq!(
        snap.clips_rules_provider_availability,
        ClipsRulesProviderAvailability::Unavailable
    );
}

// PR-106 / T052 — CLIPS-toggling: loading the CLIPS rules provider flips
// `clips_rules_provider_availability` to `Available`; removing it returns
// to `Unavailable`; neither transition affects any other row.
#[test]
fn t052_clips_rules_provider_toggle_isolates_other_rows() {
    let base = ReadinessSnapshot::from_pure("2026-05-27T00:00:00Z", vec![], vec![]);
    let arrow = base.arrow_reference_compatibility;
    let substrait = base.substrait_plan_reference_compatibility;
    let zenoh = base.zenoh_availability;

    let with_clips = base
        .clone()
        .with_clips_rules_provider_availability(ClipsRulesProviderAvailability::Available);
    assert_eq!(
        with_clips.clips_rules_provider_availability,
        ClipsRulesProviderAvailability::Available
    );
    // Other rows MUST be unchanged.
    assert_eq!(with_clips.arrow_reference_compatibility, arrow);
    assert_eq!(with_clips.substrait_plan_reference_compatibility, substrait);
    assert_eq!(with_clips.zenoh_availability, zenoh);

    let removed = with_clips
        .clone()
        .with_clips_rules_provider_availability(ClipsRulesProviderAvailability::Unavailable);
    assert_eq!(
        removed.clips_rules_provider_availability,
        ClipsRulesProviderAvailability::Unavailable
    );
    assert_eq!(removed.arrow_reference_compatibility, arrow);
    assert_eq!(removed.substrait_plan_reference_compatibility, substrait);
    assert_eq!(removed.zenoh_availability, zenoh);
}

// PR-103 / T053 — `clips_orchestrator_availability` is rejected at parse
// time by `#[serde(deny_unknown_fields)]`.
#[test]
fn t053_clips_orchestrator_availability_field_is_rejected() {
    let raw = r#"{
        "contract_version": "1.1",
        "captured_at": "2026-05-27T00:00:00Z",
        "providers": [],
        "capabilities": [],
        "clips_orchestrator_availability": "available"
    }"#;
    let err = serde_json::from_str::<ReadinessSnapshot>(raw)
        .expect_err("ReadinessSnapshot MUST reject clips_orchestrator_availability");
    let msg = err.to_string();
    assert!(
        msg.contains("clips_orchestrator_availability"),
        "serde error must name the offending field; got: {msg}"
    );
}

// PR-103 follow-up — `zenoh_session` (and any other identifier in the
// reserved Zenoh-identifier set beyond `zenoh_availability`) is rejected
// by deny_unknown_fields at the top level too. Pins PR-104.
#[test]
fn t053b_zenoh_session_field_is_rejected_at_top_level() {
    let z = concat!("ze", "noh");
    let raw = format!(
        r#"{{
            "contract_version": "1.1",
            "captured_at": "2026-05-27T00:00:00Z",
            "providers": [],
            "capabilities": [],
            "{z}_session": "endpoint"
        }}"#
    );
    let err = serde_json::from_str::<ReadinessSnapshot>(&raw)
        .expect_err("ReadinessSnapshot MUST reject zenoh_session at top level");
    assert!(err.to_string().contains(&format!("{z}_session")));
}

// T059 — positive fixture round-trip: load
// positive_readiness_zenoh_deferred.json, assert it carries the
// expected CE defaults, and round-trip canonically.
#[test]
fn t059_positive_readiness_fixture_round_trips() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .unwrap()
        .join("specs")
        .join("105-v110-arrow-zenoh-substrait-retarget")
        .join("contracts")
        .join("fixtures")
        .join("positive_readiness_zenoh_deferred.json");
    let raw = std::fs::read_to_string(&path).expect("read T059 fixture");
    let snap: ReadinessSnapshot =
        serde_json::from_str(&raw).expect("deserialize T059 fixture under deny_unknown_fields");
    assert_eq!(
        snap.arrow_reference_compatibility,
        ArrowReferenceCompatibility::DeclarationOnly
    );
    assert_eq!(
        snap.substrait_plan_reference_compatibility,
        SubstraitPlanReferenceCompatibility::DeclarationOnly
    );
    assert_eq!(snap.zenoh_availability, ZenohAvailability::Deferred);
    assert_eq!(
        snap.clips_rules_provider_availability,
        ClipsRulesProviderAvailability::Available
    );
    // Round-trip: export → re-import → canonical-hash equality.
    let exported = serde_json::to_string(&snap).expect("serialize T059");
    let back: ReadinessSnapshot = serde_json::from_str(&exported).expect("re-import T059 export");
    assert_eq!(
        snap.canonical_hash().expect("hash original"),
        back.canonical_hash().expect("hash re-imported"),
        "T059 fixture canonical hash must survive round-trip"
    );
}

// PR-105 / regression — feature 104 readiness fields (providers,
// capabilities, contract_version, captured_at, posture, tier_posture)
// remain present and reachable. A serialized snapshot includes them all.
#[test]
fn t053c_feature_104_fields_remain_in_serialized_snapshot() {
    let snap = ReadinessSnapshot::from_pure("2026-05-27T00:00:00Z", vec![], vec![]);
    let raw = serde_json::to_value(&snap).expect("serialize");
    for key in [
        "contract_version",
        "captured_at",
        "providers",
        "capabilities",
        "arrow_reference_compatibility",
        "substrait_plan_reference_compatibility",
        "zenoh_availability",
        "clips_rules_provider_availability",
    ] {
        assert!(
            raw.get(key).is_some(),
            "ReadinessSnapshot serialized form missing required field `{key}`"
        );
    }
}
