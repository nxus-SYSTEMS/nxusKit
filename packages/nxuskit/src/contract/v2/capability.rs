//! Capability Catalog DTOs for the v2 preview contract spine.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::boundary::{BoundaryViolation, ReferenceOnlyBoundary, validate_reference_only_value};
use super::classification::{SurfaceClassification, SurfaceTier};
use super::operation::{AuditMetadata, DeterminismClass, ProjectionMetadata, SideEffectClass};
use super::refs::{CapabilityRef, RouteRef, SchemaRef};

type PublicMetadata = BTreeMap<String, serde_json::Value>;

/// Product vocabulary marker for catalog entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityCatalogVocabulary {
    CapabilityCatalog,
    SdkImplementationCatalog,
    ReservedGraphModel,
}

/// Runtime route family used for capability planning without exposing internals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeRouteKind {
    LocalAdapter,
    RemoteService,
    PackagedRuntime,
    ProtectedRuntime,
    Disabled,
}

/// Public locality posture for a capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityLocality {
    Local,
    Remote,
    Hybrid,
    Packaged,
}

/// Resource/cost posture for planning and user-facing diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CostResourceClass {
    Free,
    Metered,
    ComputeBound,
    MemoryBound,
    DataPlaneBound,
    Unknown,
}

/// Entitlement posture for public contract planning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntitlementPosture {
    CommunityAllowed,
    ProRequired,
    EnterpriseRequired,
    Disabled,
    Unknown,
}

/// Release-readiness posture for a capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadinessPosture {
    Ready,
    Preview,
    Blocked,
    Experimental,
}

/// Operational health posture for a capability entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityHealth {
    Healthy,
    Degraded,
    Unavailable,
    Unknown,
}

/// Trust/classification posture used by catalog consumers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustPosture {
    PublicContract,
    InternalOnly,
    ProPrivate,
    EnterprisePrivate,
}

/// Reference categories a capability may accept or produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AllowedRefKind {
    Artifact,
    Batch,
    FactSet,
    Packet,
    Plan,
    Schema,
}

/// Product-level capability catalog entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityCatalogEntry {
    pub contract_version: String,
    pub capability_ref: CapabilityRef,
    pub title: String,
    pub summary: String,
    pub vocabulary: CapabilityCatalogVocabulary,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_schema_refs: Vec<SchemaRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output_schema_refs: Vec<SchemaRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub route_refs: Vec<RouteRef>,
    pub route_kind: RuntimeRouteKind,
    pub locality: CapabilityLocality,
    pub determinism: DeterminismClass,
    pub side_effect: SideEffectClass,
    pub cancellable: bool,
    pub cost: CostResourceClass,
    pub tier: SurfaceTier,
    pub entitlement: EntitlementPosture,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor_scope: Option<String>,
    pub tenant_scoped: bool,
    pub trust: TrustPosture,
    pub readiness: ReadinessPosture,
    pub health: CapabilityHealth,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_input_ref_kinds: Vec<AllowedRefKind>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_output_ref_kinds: Vec<AllowedRefKind>,
    pub projection: ProjectionMetadata,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit: Option<AuditMetadata>,
    pub classification: SurfaceClassification,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: PublicMetadata,
}

impl ReferenceOnlyBoundary for CapabilityCatalogEntry {
    fn validate_reference_only(&self) -> Result<(), BoundaryViolation> {
        self.capability_ref.validate_reference_only()?;
        for reference in &self.input_schema_refs {
            reference.validate_reference_only()?;
        }
        for reference in &self.output_schema_refs {
            reference.validate_reference_only()?;
        }
        for reference in &self.route_refs {
            reference.validate_reference_only()?;
        }
        self.projection.validate_reference_only()?;
        if let Some(audit) = &self.audit {
            audit.validate_reference_only()?;
        }
        validate_metadata(&self.metadata)
    }
}

fn validate_metadata(metadata: &PublicMetadata) -> Result<(), BoundaryViolation> {
    if metadata.is_empty() {
        return Ok(());
    }
    let value = serde_json::to_value(metadata).expect("public metadata serializes");
    validate_reference_only_value(&value)
}
