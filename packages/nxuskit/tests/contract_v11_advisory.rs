//! Feature 106 / US5 — public DTO tests for `ResourceAdvisory`.
//!
//! Pins FR-030, FR-032 vocabularies and the stable fallback shape.

use nxuskit::contract::v1_1::{
    ModelSizeClass, ModelSizeEstimate, ResourceAdvisory, ResourceAdvisoryReason,
    ResourceAdvisoryReasonCode, ResourceAdvisorySeverity, ResourceProbeStatus,
};

#[test]
fn resource_advisory_severity_serializes_to_documented_strings() {
    for (variant, expected) in [
        (ResourceAdvisorySeverity::Ok, "ok"),
        (ResourceAdvisorySeverity::Caution, "caution"),
        (ResourceAdvisorySeverity::HighRisk, "high_risk"),
        (ResourceAdvisorySeverity::Blocked, "blocked"),
    ] {
        let json = serde_json::to_value(variant).expect("ser");
        assert_eq!(json, expected, "{variant:?}");
    }
}

#[test]
fn resource_advisory_reason_code_vocabulary_is_complete() {
    for (variant, expected) in [
        (
            ResourceAdvisoryReasonCode::InsufficientAvailableMemory,
            "insufficient_available_memory",
        ),
        (
            ResourceAdvisoryReasonCode::UnknownModelSize,
            "unknown_model_size",
        ),
        (
            ResourceAdvisoryReasonCode::ProviderAlreadyBusy,
            "provider_already_busy",
        ),
        (
            ResourceAdvisoryReasonCode::MultipleLargeModels,
            "multiple_large_models",
        ),
        (
            ResourceAdvisoryReasonCode::ResourceProbeUnavailable,
            "resource_probe_unavailable",
        ),
    ] {
        let json = serde_json::to_value(variant).expect("ser");
        assert_eq!(json, expected, "{variant:?}");
    }
}

#[test]
fn resource_advisory_probe_status_vocabulary_is_complete() {
    for (variant, expected) in [
        (ResourceProbeStatus::Available, "available"),
        (ResourceProbeStatus::Unavailable, "unavailable"),
        (ResourceProbeStatus::Partial, "partial"),
    ] {
        let json = serde_json::to_value(variant).expect("ser");
        assert_eq!(json, expected, "{variant:?}");
    }
}

#[test]
fn resource_advisory_fallback_helper_returns_stable_unavailable_shape() {
    let adv = ResourceAdvisory::probe_unavailable();
    assert_eq!(adv.contract_version, "1.1");
    assert_eq!(adv.severity, ResourceAdvisorySeverity::Caution);
    assert_eq!(adv.probe_status, ResourceProbeStatus::Unavailable);
    assert_eq!(adv.reasons.len(), 1);
    assert_eq!(
        adv.reasons[0].code,
        ResourceAdvisoryReasonCode::ResourceProbeUnavailable
    );
    let json = serde_json::to_string(&adv).expect("ser");
    assert!(json.contains("\"resource_probe_unavailable\""));
}

#[test]
fn resource_advisory_round_trips_with_full_reason_payload() {
    let adv = ResourceAdvisory {
        contract_version: "1.1".to_string(),
        severity: ResourceAdvisorySeverity::HighRisk,
        reasons: vec![ResourceAdvisoryReason {
            code: ResourceAdvisoryReasonCode::MultipleLargeModels,
            detail: "two operations request large local models".to_string(),
            operations: vec!["compare::op-0".to_string(), "compare::op-1".to_string()],
        }],
        recommended_max_concurrency: Some(1),
        model_size_estimates: vec![ModelSizeEstimate {
            provider_ref: "loopback".to_string(),
            model_ref: "echo".to_string(),
            size_class: ModelSizeClass::Unknown,
            estimated_bytes: None,
        }],
        memory_estimate_mb: Some(16_384),
        probe_status: ResourceProbeStatus::Partial,
    };
    let json = serde_json::to_string(&adv).expect("ser");
    let parsed: ResourceAdvisory = serde_json::from_str(&json).expect("de");
    assert_eq!(parsed, adv);
}
