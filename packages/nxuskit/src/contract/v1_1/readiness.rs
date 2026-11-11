//! Public CE provider-readiness and capability-truth DTOs (US1 floor).
//!
//! The canonical surface is the Rust DTO here; the `nxuskit-cli doctor --json`
//! CLI projection is a `serde_json` projection of [`ReadinessSnapshot`]
//! (research.md §2, CB-6).
//!
//! US1 provides a pure constructor [`ReadinessSnapshot::from_pure`] used by
//! tests, the example smoke, and the CLI projection. The FFI-backed
//! ``query()`` constructor that reads the live `auth_providers` /
//! `auth_status_all` surfaces lands as a future Integration follow-up and
//! MUST go through the narrow engine adapter; it is not part of the US1
//! deliverable.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::ids::{CapabilityDomain, CapabilityId, CapabilityPosture, ProviderId, TierPosture};
use super::validation::ContractError;
use super::{CONTRACT_VERSION, canonical_hash};

/// Snapshot of provider/capability readiness.
///
/// Feature 105 (US5 / T055) adds four named architectural-posture rows so
/// consumers can programmatically test whether the CE build supports Arrow
/// references, Substrait plan references, Zenoh transport, and the CLIPS
/// rules provider — independently of provider configuration.
/// `#[serde(deny_unknown_fields)]` is load-bearing: it rejects the banned
/// `clips_orchestrator_availability` field name at parse time (PR-103).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadinessSnapshot {
    pub contract_version: String,
    pub captured_at: String,
    pub providers: Vec<ProviderReadiness>,
    pub capabilities: Vec<CapabilityTruthEntry>,
    /// Feature 105 (US5): Arrow reference compatibility posture.
    #[serde(default = "default_arrow_reference_compatibility")]
    pub arrow_reference_compatibility: ArrowReferenceCompatibility,
    /// Feature 105 (US5): Substrait plan-reference compatibility posture.
    #[serde(default = "default_substrait_plan_reference_compatibility")]
    pub substrait_plan_reference_compatibility: SubstraitPlanReferenceCompatibility,
    /// Feature 105 (US5): Zenoh availability posture. CE default is
    /// `Deferred` — CE never ships a Zenoh transport.
    #[serde(default = "default_zenoh_availability")]
    pub zenoh_availability: ZenohAvailability,
    /// Feature 105 (US5): CLIPS rules-provider availability. Independent of
    /// any future CLIPS orchestrator capability (FR-009).
    #[serde(default = "default_clips_rules_provider_availability")]
    pub clips_rules_provider_availability: ClipsRulesProviderAvailability,
}

fn default_arrow_reference_compatibility() -> ArrowReferenceCompatibility {
    ArrowReferenceCompatibility::DeclarationOnly
}
fn default_substrait_plan_reference_compatibility() -> SubstraitPlanReferenceCompatibility {
    SubstraitPlanReferenceCompatibility::DeclarationOnly
}
fn default_zenoh_availability() -> ZenohAvailability {
    ZenohAvailability::Deferred
}
fn default_clips_rules_provider_availability() -> ClipsRulesProviderAvailability {
    ClipsRulesProviderAvailability::Unavailable
}

impl ReadinessSnapshot {
    /// Construct a snapshot from explicit data. Used by tests, the example
    /// smoke harness, and the CLI projection. Does NOT touch the FFI.
    ///
    /// CE defaults for the four feature-105 posture rows (`DeclarationOnly`
    /// / `DeclarationOnly` / `Deferred` / `Unavailable`) are applied here
    /// per `contracts/provider-readiness-retarget.md`. Use
    /// `with_clips_rules_provider_availability` to override after probing.
    pub fn from_pure(
        captured_at: impl Into<String>,
        providers: Vec<ProviderReadiness>,
        capabilities: Vec<CapabilityTruthEntry>,
    ) -> Self {
        Self {
            contract_version: CONTRACT_VERSION.to_owned(),
            captured_at: captured_at.into(),
            providers,
            capabilities,
            arrow_reference_compatibility: default_arrow_reference_compatibility(),
            substrait_plan_reference_compatibility: default_substrait_plan_reference_compatibility(
            ),
            zenoh_availability: default_zenoh_availability(),
            clips_rules_provider_availability: default_clips_rules_provider_availability(),
        }
    }

    /// Builder-style override for the CLIPS rules-provider posture. The
    /// other three feature-105 rows have no runtime probe in CE (no Arrow /
    /// Substrait / Zenoh runtime is linked) so they remain at the CE
    /// defaults.
    pub fn with_clips_rules_provider_availability(
        mut self,
        availability: ClipsRulesProviderAvailability,
    ) -> Self {
        self.clips_rules_provider_availability = availability;
        self
    }

    /// Canonical hash of the snapshot content.
    pub fn canonical_hash(&self) -> Result<super::ArtifactCanonicalHash, ContractError> {
        let value = serde_json::to_value(self)
            .map_err(|e| ContractError::Malformed(format!("serialize: {e}")))?;
        canonical_hash(&value).map_err(|e| ContractError::Malformed(e.to_string()))
    }
}

/// Feature 105 (US5) Arrow reference compatibility posture. CE default is
/// `DeclarationOnly` — the contract carries Arrow `BatchRef` shapes but
/// CE does not link an Arrow runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArrowReferenceCompatibility {
    Available,
    DeclarationOnly,
    Unavailable,
    Unknown,
}

/// Feature 105 (US5) Substrait plan-reference compatibility posture. CE
/// default is `DeclarationOnly` — `PlanRef` shapes are part of the public
/// contract but CE does not execute Substrait plans.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubstraitPlanReferenceCompatibility {
    Available,
    DeclarationOnly,
    Unavailable,
    Unknown,
}

/// Feature 105 (US5) Zenoh availability posture. CE default is `Deferred`
/// — Zenoh transport is reserved for Pro/Enterprise. The presence of this
/// row in the public schema is intentional (FR-007 / Session 2026-05-26
/// clarification): downstream consumers can see the deferral.
///
/// # Example
///
/// ```
/// use nxuskit::contract::v1_1::{ReadinessSnapshot, ZenohAvailability};
/// let snap = ReadinessSnapshot::from_pure("2026-05-27T00:00:00Z", vec![], vec![]);
/// assert_eq!(snap.zenoh_availability, ZenohAvailability::Deferred);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZenohAvailability {
    Available,
    Deferred,
    Unavailable,
    Unknown,
}

/// Feature 105 (US5) CLIPS rules-provider availability. Reported
/// independently of any future CLIPS orchestrator capability (FR-009). The
/// schema MUST NOT carry a `clips_orchestrator_availability` field;
/// `#[serde(deny_unknown_fields)]` on `ReadinessSnapshot` enforces this at
/// parse time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClipsRulesProviderAvailability {
    Available,
    Unavailable,
    Unknown,
}

/// One provider's readiness posture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderReadiness {
    pub provider_id: ProviderId,
    pub posture: ReadinessPosture,
    pub tier_posture: TierPosture,
    pub local_provider: bool,
    #[serde(default)]
    pub models: Vec<ProviderModel>,
    pub timeouts_ms: ProviderTimeoutsMs,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<ProviderErrorClass>,
    #[serde(default)]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

/// Readiness posture (FR-007, SC-004).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadinessPosture {
    Ready,
    AuthMissing,
    AuthInvalid,
    Unreachable,
    Timeout,
    TierRestricted,
    InternalOnly,
}

/// Provider error class. Class-only — no raw provider error strings (BR-001).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderErrorClass {
    AuthMissing,
    AuthInvalid,
    Unreachable,
    Timeout,
    TierRestricted,
    InternalOnly,
    Other,
}

/// Listed model. Capability list per-model prevents LLM-shaped flattening (FR-011).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderModel {
    pub id: String,
    #[serde(default)]
    pub capabilities: Vec<CapabilityId>,
    #[serde(default)]
    pub deprecated: bool,
    /// Additive public-safe model metadata. Carries the optional
    /// `contract_aliases` list (R7, FR-002) and the `alias_only` marker
    /// (FR-004). Default is an empty metadata block; existing fixtures
    /// without the field continue to deserialize because every field is
    /// `#[serde(default)]`.
    #[serde(default, skip_serializing_if = "ProviderModelMetadata::is_empty")]
    pub metadata: ProviderModelMetadata,
}

/// Additive public-safe metadata for [`ProviderModel`] introduced in
/// feature 106 (v1.1.0 RC Peeler CLI update).
///
/// All fields are optional with `#[serde(default)]` and
/// `skip_serializing_if` so the field is silently absent in existing
/// fixtures.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderModelMetadata {
    /// Backward-compat alias strings this model is also addressable as.
    /// For the loopback `echo` row this carries `["loopback:v1"]`
    /// (FR-002, FR-051). Empty when no aliases apply.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contract_aliases: Vec<String>,
    /// `true` when this row is only a declaration alias and NOT directly
    /// runtime-callable (FR-004). Preferred v1.1.0 RC behavior is to
    /// avoid emitting such rows for ready providers, but downstream
    /// consumers may still encounter them and need to filter.
    #[serde(default, skip_serializing_if = "is_false")]
    pub alias_only: bool,
}

impl ProviderModelMetadata {
    fn is_empty(&self) -> bool {
        self.contract_aliases.is_empty() && !self.alias_only
    }
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// Timeout posture as machine-readable data (not raw config). Default values
/// match the existing workspace defaults; consumers MAY override.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProviderTimeoutsMs {
    pub connect_ms: u64,
    pub request_ms: u64,
}

impl Default for ProviderTimeoutsMs {
    fn default() -> Self {
        Self {
            connect_ms: 5_000,
            request_ms: 60_000,
        }
    }
}

/// Capability-truth entry. Domain-aware; posture and tier are independent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityTruthEntry {
    pub capability_id: CapabilityId,
    pub domain: CapabilityDomain,
    pub posture: CapabilityPosture,
    pub tier: TierPosture,
    #[serde(default)]
    pub providers: Vec<ProviderId>,
}
