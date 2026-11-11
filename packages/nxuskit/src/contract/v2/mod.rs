//! v2 preview operation/capability/ref contract spine.
//!
//! This module is source-path contract alpha material. It defines pure DTOs,
//! reference primitives, surface classification, and reference-only boundary
//! validation. It does not implement projection generation, CLI schema
//! commands, transport, protected execution, or engine runtime dispatch.

pub mod boundary;
pub mod capability;
pub mod classification;
pub mod ids;
pub mod operation;
pub mod refs;

/// Pinned v2 preview contract version string for serialized payloads.
pub const CONTRACT_VERSION: &str = "2.0-alpha";

pub use boundary::{
    BoundaryViolation, BoundaryViolationKind, ReferenceOnlyBoundary, validate_reference_only_value,
};
pub use capability::{
    AllowedRefKind, CapabilityCatalogEntry, CapabilityCatalogVocabulary, CapabilityHealth,
    CapabilityLocality, CostResourceClass, EntitlementPosture, ReadinessPosture, RuntimeRouteKind,
    TrustPosture,
};
pub use classification::{SurfaceClassification, SurfaceTier};
pub use ids::{
    ActorId, ArtifactId, BatchId, CapabilityId, CausalityId, CorrelationId, FactSetId,
    OperationEventId, OperationId, PacketId, PlanId, RouteId, SchemaId, TenantId,
};
pub use operation::{
    AcceptanceStatus, ActorContext, AuditMetadata, BudgetPolicy, DeterminismClass, ExecutionPolicy,
    OperationAccepted, OperationEnvelope, OperationErrorSummary, OperationEvent,
    OperationEventKind, OperationResult, OperationStatus, ProjectionMetadata, ProjectionPosture,
    RetryPolicy, SideEffectClass,
};
pub use refs::{
    ArtifactKind, ArtifactRef, BatchRef, CapabilityRef, FactSetRef, OperationRef, PacketRef,
    PlanCompatibilityPosture, PlanKind, PlanRef, RefTrustPosture, ReferenceLocality, RouteRef,
    SchemaKind, SchemaRef,
};
