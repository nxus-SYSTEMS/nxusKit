//! Operation DTOs for the v2 preview contract spine.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::boundary::{BoundaryViolation, ReferenceOnlyBoundary, validate_reference_only_value};
use super::classification::SurfaceClassification;
use super::ids::{ActorId, CausalityId, CorrelationId, OperationEventId, OperationId, TenantId};
use super::refs::{
    ArtifactRef, CapabilityRef, OperationRef, PacketRef, PlanRef, RouteRef, SchemaRef,
};

type PublicMetadata = BTreeMap<String, serde_json::Value>;

/// Expected side-effect posture for an operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SideEffectClass {
    ReadOnly,
    WritesMetadata,
    WritesArtifact,
    InvokesExternalService,
    ProtectedRuntime,
}

/// Determinism posture for planning, replay, and fixture classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeterminismClass {
    Deterministic,
    BoundedNonDeterministic,
    ModelNonDeterministic,
    Unknown,
}

/// Public projection posture for a contract artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionPosture {
    NotProjected,
    ProjectionReady,
    Projected,
    ProjectionBlocked,
}

/// Dispatch acknowledgement status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcceptanceStatus {
    Accepted,
    RejectedBeforeDispatch,
    ValidationFailed,
}

/// Material operation status used by results and status events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    TimedOut,
    RetryScheduled,
    BudgetExceeded,
    ValidationFailed,
}

/// Event stream discriminator for operation status and progress updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationEventKind {
    Accepted,
    Started,
    Progress,
    ResultAvailable,
    RetryScheduled,
    Cancelled,
    TimedOut,
    BudgetUpdated,
    BudgetExceeded,
    ValidationFailed,
    Failed,
}

/// Actor and tenant context used for policy and audit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActorContext {
    pub actor_id: ActorId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<TenantId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_kind: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: PublicMetadata,
}

/// Retry policy used by accepted operations and rejected-before-dispatch guidance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backoff_hint: Option<String>,
}

/// Budget policy used to bound runtime and result fan-out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetPolicy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_runtime_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_refs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_cost_units: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_tag: Option<String>,
}

/// Dispatch policy carried by an operation intent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPolicy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    pub cancellable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancellation_token: Option<String>,
    pub retry: RetryPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dedupe_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub causality_id: Option<CausalityId>,
    pub budget: BudgetPolicy,
}

/// Projection classification and provenance for generated contract artifacts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionMetadata {
    pub posture: ProjectionPosture,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_schema_refs: Vec<SchemaRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub projected_schema_refs: Vec<SchemaRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub generated_artifact_refs: Vec<ArtifactRef>,
    pub classification: SurfaceClassification,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: PublicMetadata,
}

/// Audit metadata attached to operation intents, acknowledgements, and results.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submitted_at: Option<String>,
    pub actor: ActorContext,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit_ref: Option<ArtifactRef>,
    pub classification: SurfaceClassification,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: PublicMetadata,
}

/// Compact, public-safe error summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationErrorSummary {
    pub error_code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    pub retryable: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: PublicMetadata,
}

/// Canonical v2 operation intent envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationEnvelope {
    pub contract_version: String,
    pub operation_id: OperationId,
    pub correlation_id: CorrelationId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub causality_id: Option<CausalityId>,
    pub actor: ActorContext,
    pub capability_ref: CapabilityRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_ref: Option<RouteRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_refs: Vec<OperationRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plan_refs: Vec<PlanRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expected_output_schema_refs: Vec<SchemaRef>,
    pub side_effect: SideEffectClass,
    pub determinism: DeterminismClass,
    pub execution_policy: ExecutionPolicy,
    pub projection: ProjectionMetadata,
    pub audit: AuditMetadata,
    pub classification: SurfaceClassification,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: PublicMetadata,
}

/// Operation dispatch acknowledgement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationAccepted {
    pub contract_version: String,
    pub operation_id: OperationId,
    pub correlation_id: CorrelationId,
    pub status: AcceptanceStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepted_at: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub validation_errors: Vec<OperationErrorSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_stream_ref: Option<PacketRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_ref: Option<PacketRef>,
    pub classification: SurfaceClassification,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: PublicMetadata,
}

/// Terminal or material operation result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationResult {
    pub contract_version: String,
    pub operation_id: OperationId,
    pub correlation_id: CorrelationId,
    pub status: OperationStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output_refs: Vec<OperationRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_ref: Option<PacketRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<OperationErrorSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    pub classification: SurfaceClassification,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: PublicMetadata,
}

/// Operation event stream item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationEvent {
    pub contract_version: String,
    pub event_id: OperationEventId,
    pub operation_id: OperationId,
    pub correlation_id: CorrelationId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub causality_id: Option<CausalityId>,
    pub event_kind: OperationEventKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<OperationStatus>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refs: Vec<OperationRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occurred_at: Option<String>,
    pub classification: SurfaceClassification,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: PublicMetadata,
}

impl ReferenceOnlyBoundary for ActorContext {
    fn validate_reference_only(&self) -> Result<(), BoundaryViolation> {
        validate_metadata(&self.metadata)
    }
}

impl ReferenceOnlyBoundary for ProjectionMetadata {
    fn validate_reference_only(&self) -> Result<(), BoundaryViolation> {
        for reference in &self.source_schema_refs {
            reference.validate_reference_only()?;
        }
        for reference in &self.projected_schema_refs {
            reference.validate_reference_only()?;
        }
        for reference in &self.generated_artifact_refs {
            reference.validate_reference_only()?;
        }
        validate_metadata(&self.metadata)
    }
}

impl ReferenceOnlyBoundary for AuditMetadata {
    fn validate_reference_only(&self) -> Result<(), BoundaryViolation> {
        self.actor.validate_reference_only()?;
        if let Some(audit_ref) = &self.audit_ref {
            audit_ref.validate_reference_only()?;
        }
        validate_metadata(&self.metadata)
    }
}

impl ReferenceOnlyBoundary for OperationErrorSummary {
    fn validate_reference_only(&self) -> Result<(), BoundaryViolation> {
        validate_metadata(&self.metadata)
    }
}

impl ReferenceOnlyBoundary for OperationEnvelope {
    fn validate_reference_only(&self) -> Result<(), BoundaryViolation> {
        self.actor.validate_reference_only()?;
        self.capability_ref.validate_reference_only()?;
        if let Some(route_ref) = &self.route_ref {
            route_ref.validate_reference_only()?;
        }
        for reference in &self.input_refs {
            reference.validate_reference_only()?;
        }
        for reference in &self.plan_refs {
            reference.validate_reference_only()?;
        }
        for reference in &self.expected_output_schema_refs {
            reference.validate_reference_only()?;
        }
        self.projection.validate_reference_only()?;
        self.audit.validate_reference_only()?;
        validate_metadata(&self.metadata)
    }
}

impl ReferenceOnlyBoundary for OperationAccepted {
    fn validate_reference_only(&self) -> Result<(), BoundaryViolation> {
        for error in &self.validation_errors {
            error.validate_reference_only()?;
        }
        if let Some(event_stream_ref) = &self.event_stream_ref {
            event_stream_ref.validate_reference_only()?;
        }
        if let Some(result_ref) = &self.result_ref {
            result_ref.validate_reference_only()?;
        }
        validate_metadata(&self.metadata)
    }
}

impl ReferenceOnlyBoundary for OperationResult {
    fn validate_reference_only(&self) -> Result<(), BoundaryViolation> {
        for reference in &self.output_refs {
            reference.validate_reference_only()?;
        }
        if let Some(result_ref) = &self.result_ref {
            result_ref.validate_reference_only()?;
        }
        if let Some(error) = &self.error {
            error.validate_reference_only()?;
        }
        validate_metadata(&self.metadata)
    }
}

impl ReferenceOnlyBoundary for OperationEvent {
    fn validate_reference_only(&self) -> Result<(), BoundaryViolation> {
        for reference in &self.refs {
            reference.validate_reference_only()?;
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
