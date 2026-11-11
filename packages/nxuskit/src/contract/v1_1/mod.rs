//! v1.1 public CE contract surface.
//!
//! Pure DTO + validation primitives. No engine-internal re-exports. Every
//! exportable entity carries the constant [`CONTRACT_VERSION`] so consumers
//! can negotiate forward-compatibility.

/// Pinned v1.1 contract version string for serialized payloads.
pub const CONTRACT_VERSION: &str = "1.1";

// Foundational modules from Phase 2 (T012-T017).
mod hash;
mod ids;
mod validation;

// US1 modules - Phase 3 (T024-T029). US5 (`tool` semantics breadth) lands at
// task T065 and is intentionally out of scope here. The US1 pipeline shape
// represents tool-stage metadata as an opaque extension-preserving map so
// the pipeline round-trip works without taking a position on
// determinism/auth/tenant/redaction/cost taxonomies.
pub mod advisory;
mod cli_projection;
mod engine_adapter;
mod envelope;
mod loader;
mod packet;
mod pipeline;
mod readiness;
mod validate;

// Feature 105 retargeting - additive modules.
// `refs` carries reference primitives (BatchRef, ArtifactRef, PlanRef,
// EnvelopeRef). `action` carries the SDK-owned action-envelope DTOs that
// map field-for-field to the retargeted action-envelope contract.
// `tool` carries the public `ToolSemantics` DTO (US4 / T048).
pub mod action;
pub mod graph;
pub mod orchestration;
pub mod refs;
pub mod tool;

// Feature 106 (v1.1.0 RC Peeler CLI update) - additive modules.
// `outcome` carries the typed stage / operation outcome DTOs and
// `RuntimeDomain` enum. `run_report` carries `PipelineRunReport` and
// the output-selection vocabulary (preset + include/omit sections).
pub mod outcome;
pub mod run_report;

// Re-exports - only public DTO primitives and pure helpers. No engine
// types, no I/O orchestration types. Expand additively only.
pub use cli_projection::{
    PipelineValidateCliProjection, doctor_readiness_snapshot, doctor_readiness_snapshot_at,
    pipeline_validate_exit_code, pipeline_validate_outcome_json,
};
pub use hash::{
    ArtifactCanonicalHash, ArtifactDisplayHash, HashError, canonical_hash, canonical_json_bytes,
    display_short,
};
pub use ids::{
    ActionId, ActionType, BatchId, CapabilityDomain, CapabilityId, CapabilityPosture,
    CorrelationId, IdError, PipelineId, PlanId, ProviderId, SourceId, StageName, StageType,
    TierPosture, TopicName,
};
pub use validation::{
    BulkPayloadInControlPlaneKind, ContractError, PipelineValidationOutcome, StageValidationOutcome,
};

pub use envelope::{
    ArtifactHistoryEnvelope, ArtifactHistoryEnvelopeBuilder, EngineKind, ImportedExampleRef,
    ImportedExampleRefError, ImportedExampleType, PipelineDefinitionRef, ProvenanceEntry,
    ProvenanceEntryClass, ReadinessSnapshotRef, ReplayKind, ReplayLinkage, RunOutcome,
    SourceReference, ValidationState,
};
pub use loader::{
    BulkPayloadKind, LoadError, export_artifact_history_envelope_json,
    export_orchestration_definition_json, export_pipeline_definition_json,
    load_action_envelope_json_guarded, load_artifact_history_envelope_json,
    load_artifact_history_envelope_json_guarded, load_orchestration_definition_json,
    load_orchestration_definition_json_guarded, load_pipeline_definition_json,
    load_pipeline_definition_json_guarded,
};
pub use packet::{
    DispositionKind, DraftOutput, EvidenceEntry, EvidenceMap, EvidenceValidationOutcome,
    HumanReviewDisposition, RedactionFlag, RedactionKind, RegulatedWorkflowPacket,
    RegulatedWorkflowPacketBuilder, ReviewStatus, SourceKind, SourcePacket,
};
pub use pipeline::{
    DependencyKind, PipelineDefinition, StageArtifactRef, StageDecl, StageDependency, StageInput,
    StageInputSource, StageOutput, ToolStageMetadata, ValidationSlotDecl,
};
pub use readiness::{
    ArrowReferenceCompatibility, CapabilityTruthEntry, ClipsRulesProviderAvailability,
    ProviderErrorClass, ProviderModel, ProviderModelMetadata, ProviderReadiness,
    ProviderTimeoutsMs, ReadinessPosture, ReadinessSnapshot, SubstraitPlanReferenceCompatibility,
    ZenohAvailability,
};
pub use validate::validate_with_readiness;

pub use action::{ActionEnvelope, ActionErrorSummary, ActionResult, ActionStatus};
pub use advisory::{
    ModelSizeClass, ModelSizeEstimate, ResourceAdvisory, ResourceAdvisoryReason,
    ResourceAdvisoryReasonCode, ResourceAdvisorySeverity, ResourceProbeStatus,
};
pub use graph::{
    GraphEdge, GraphEdgeKind, GraphFanOutGroup, GraphNode, GraphNodeKind, GraphNodePosture,
    GraphProjection, GraphSourceKind, GraphValidationStatus, LayoutHints, LayoutRank,
};
pub use orchestration::{
    LayoutHint, OrchestrationDefinition, OrchestrationEdge, OrchestrationEdgeKind,
    OrchestrationNode, OrchestrationNodeKind, OrchestrationPosture,
};
pub use outcome::{OperationOutcome, RuntimeDomain, StageOutcome};
pub use refs::{
    AdapterPosture, ArtifactKind, ArtifactRef, BatchRef, EnvelopeRef, PlanCompatibilityPosture,
    PlanKind, PlanRef, RefError, SourceReferenceKind,
};
pub use run_report::{
    FanOutGroupResult, FanOutOperationResult, FinalOutputs, PipelineRunReport,
    ReadinessSummaryDigest, ReportPreset, ReportSection, StageFinalOutput, StageRunResult,
    effective_sections,
};
pub use tool::{ToolDataPosture, ToolSemantics};
