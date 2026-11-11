//! Reference primitives for the v2 preview contract spine.
//!
//! These DTOs identify external resources and small public-safe metadata. They
//! intentionally do not carry row data, file bodies, SQL text, or protected
//! workflow content.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::boundary::{BoundaryViolation, ReferenceOnlyBoundary, validate_reference_only_value};
use super::classification::SurfaceClassification;
use super::ids::{
    ArtifactId, BatchId, CapabilityId, FactSetId, PacketId, PlanId, RouteId, SchemaId,
};

type PublicMetadata = BTreeMap<String, serde_json::Value>;

/// Public-safe locality posture for referenced resources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceLocality {
    Local,
    Remote,
    Packaged,
    External,
}

/// Trust posture for a reference whose resource is stored outside the envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefTrustPosture {
    Trusted,
    Verified,
    Unverified,
    Derived,
}

/// Public-safe artifact categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    File,
    Package,
    ModelArtifact,
    HistoryEntry,
    Report,
    Other,
}

/// Public-safe schema categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchemaKind {
    JsonSchema,
    ArrowSchemaRef,
    OpenApiSchema,
    RustDto,
    Other,
}

/// Public-safe plan categories. Text/body fields must be referenced, not embedded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanKind {
    PortablePlan,
    SqlTextReference,
    EngineSpecificReference,
    Other,
}

/// Compatibility posture for a plan reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanCompatibilityPosture {
    DeclarationOnly,
    ExecutableWhenRuntimePresent,
    Unknown,
}

/// Reference to a schema artifact used by v2 operation inputs or outputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemaRef {
    pub schema_id: SchemaId,
    pub schema_kind: SchemaKind,
    pub schema_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    pub classification: SurfaceClassification,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: PublicMetadata,
}

/// Reference to an artifact whose content is stored outside the public envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactRef {
    pub artifact_id: ArtifactId,
    pub artifact_kind: ArtifactKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_ref: Option<SchemaRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    pub locality: ReferenceLocality,
    pub trust: RefTrustPosture,
    pub classification: SurfaceClassification,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: PublicMetadata,
}

/// Reference to a batch-like data-plane resource. Never embeds rows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchRef {
    pub batch_id: BatchId,
    pub schema_ref: SchemaRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_count_hint: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_size_hint: Option<u64>,
    pub locality: ReferenceLocality,
    pub trust: RefTrustPosture,
    pub classification: SurfaceClassification,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: PublicMetadata,
}

/// Reference to a logical fact set assembled from one or more references.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactSetRef {
    pub fact_set_id: FactSetId,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub schema_refs: Vec<SchemaRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_refs: Vec<OperationRef>,
    pub locality: ReferenceLocality,
    pub trust: RefTrustPosture,
    pub classification: SurfaceClassification,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: PublicMetadata,
}

/// Reference to a declarative or runtime-specific plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanRef {
    pub plan_id: PlanId,
    pub plan_kind: PlanKind,
    pub compatibility_posture: PlanCompatibilityPosture,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_ref: Option<SchemaRef>,
    pub locality: ReferenceLocality,
    pub trust: RefTrustPosture,
    pub classification: SurfaceClassification,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: PublicMetadata,
}

/// Reference to an SDK packet or envelope artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PacketRef {
    pub packet_id: PacketId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_ref: Option<SchemaRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_ref: Option<ArtifactRef>,
    pub locality: ReferenceLocality,
    pub trust: RefTrustPosture,
    pub classification: SurfaceClassification,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: PublicMetadata,
}

/// Lightweight reference to a Capability Catalog entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityRef {
    pub capability_id: CapabilityId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_version: Option<String>,
    pub classification: SurfaceClassification,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: PublicMetadata,
}

/// Reference to a runtime route selected by policy outside the envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteRef {
    pub route_id: RouteId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_label: Option<String>,
    pub locality: ReferenceLocality,
    pub trust: RefTrustPosture,
    pub classification: SurfaceClassification,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: PublicMetadata,
}

/// Discriminated union for operation input/output reference slots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum OperationRef {
    Artifact(ArtifactRef),
    Batch(BatchRef),
    FactSet(FactSetRef),
    Packet(PacketRef),
    Schema(SchemaRef),
}

impl ReferenceOnlyBoundary for SchemaRef {
    fn validate_reference_only(&self) -> Result<(), BoundaryViolation> {
        validate_metadata(&self.metadata)
    }
}

impl ReferenceOnlyBoundary for ArtifactRef {
    fn validate_reference_only(&self) -> Result<(), BoundaryViolation> {
        if let Some(schema_ref) = &self.schema_ref {
            schema_ref.validate_reference_only()?;
        }
        validate_metadata(&self.metadata)
    }
}

impl ReferenceOnlyBoundary for BatchRef {
    fn validate_reference_only(&self) -> Result<(), BoundaryViolation> {
        self.schema_ref.validate_reference_only()?;
        validate_metadata(&self.metadata)
    }
}

impl ReferenceOnlyBoundary for FactSetRef {
    fn validate_reference_only(&self) -> Result<(), BoundaryViolation> {
        for schema_ref in &self.schema_refs {
            schema_ref.validate_reference_only()?;
        }
        for input_ref in &self.input_refs {
            input_ref.validate_reference_only()?;
        }
        validate_metadata(&self.metadata)
    }
}

impl ReferenceOnlyBoundary for PlanRef {
    fn validate_reference_only(&self) -> Result<(), BoundaryViolation> {
        if let Some(schema_ref) = &self.schema_ref {
            schema_ref.validate_reference_only()?;
        }
        validate_metadata(&self.metadata)
    }
}

impl ReferenceOnlyBoundary for PacketRef {
    fn validate_reference_only(&self) -> Result<(), BoundaryViolation> {
        if let Some(schema_ref) = &self.schema_ref {
            schema_ref.validate_reference_only()?;
        }
        if let Some(artifact_ref) = &self.artifact_ref {
            artifact_ref.validate_reference_only()?;
        }
        validate_metadata(&self.metadata)
    }
}

impl ReferenceOnlyBoundary for CapabilityRef {
    fn validate_reference_only(&self) -> Result<(), BoundaryViolation> {
        validate_metadata(&self.metadata)
    }
}

impl ReferenceOnlyBoundary for RouteRef {
    fn validate_reference_only(&self) -> Result<(), BoundaryViolation> {
        validate_metadata(&self.metadata)
    }
}

impl ReferenceOnlyBoundary for OperationRef {
    fn validate_reference_only(&self) -> Result<(), BoundaryViolation> {
        match self {
            Self::Artifact(reference) => reference.validate_reference_only(),
            Self::Batch(reference) => reference.validate_reference_only(),
            Self::FactSet(reference) => reference.validate_reference_only(),
            Self::Packet(reference) => reference.validate_reference_only(),
            Self::Schema(reference) => reference.validate_reference_only(),
        }
    }
}

fn validate_metadata(metadata: &PublicMetadata) -> Result<(), BoundaryViolation> {
    if metadata.is_empty() {
        return Ok(());
    }
    let value = serde_json::to_value(metadata).expect("public metadata serializes");
    validate_reference_only_value(&value)
}
