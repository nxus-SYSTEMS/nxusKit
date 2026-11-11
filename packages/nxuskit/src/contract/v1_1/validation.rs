//! Validation outcome primitives for the v1.1 contract surface.
//!
//! Outcomes are typed enums with `#[serde(tag = "kind", rename_all =
//! "snake_case")]` so JSON consumers can branch on a single discriminator
//! field. This shape is shared by the Rust API and the `nxuskit-cli`
//! pipeline-validation projection (Phase 3, T051).

use serde::{Deserialize, Serialize};

use super::ids::{CapabilityId, StageName};

/// Pipeline-level validation outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PipelineValidationOutcome {
    /// Pipeline parsed and validated cleanly against the contract schema.
    Ready,
    /// Schema-level failure with a locatable JSON Pointer-ish `path`.
    SchemaInvalid { path: String, message: String },
    /// Stage declared a capability the active registry does not provide.
    CapabilityUnavailable {
        stage_name: StageName,
        capability_id: CapabilityId,
    },
    /// Readiness has not been queried yet; the consumer should resolve
    /// readiness before treating any downstream Ready outcome as actionable.
    ReadinessUnknown { reason: String },
    /// Feature 105 (T029, PS-005): a control-plane DTO carries an embedded
    /// bulk payload (RecordBatch JSON, base64 blob, ≥4KB inline text, or a
    /// field whose name matches `_blob` / `_body` / `_payload`). The
    /// public contract is reference-only on the control plane.
    BulkPayloadInControlPlane {
        path: String,
        payload_kind: BulkPayloadInControlPlaneKind,
    },
    /// Feature 105 (T029, PS-006): a control-plane DTO carries a Zenoh API
    /// or routing identifier. Public CE must not surface transport topology
    /// — Zenoh wiring is a Pro/Enterprise concern.
    ZenohFieldLeak { path: String, identifier: String },
    /// Feature 105 (T040, PS-002): a stage that requires a plan reference
    /// (`DataStoreSearch`, or a tool stage whose `ToolSemantics` sets
    /// `requires_plan_ref = true`) has no `plan_ref` set.
    PlanReferenceUnbound { stage_name: StageName },
    /// Feature 105 (T040, PS-004): a slot is bound twice — once via an
    /// embedded literal (`tool_input` / `query` / `body` / `payload` with
    /// array or object value) and once via a reference (`input_refs` /
    /// `output_refs`). The control plane is reference-only when refs are
    /// present.
    AmbiguousSlot { path: String },
}

/// Class of bulk-payload-in-control-plane validator finding. Mirrors the
/// loader-side `BulkPayloadKind` so consumers can branch on a single typed
/// classification regardless of the layer that caught the violation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BulkPayloadInControlPlaneKind {
    LargeInlineText,
    EmbeddedRecordBatch,
    EmbeddedBlob,
    Base64DataUri,
}

/// Stage-level validation outcome — the minimum Phase-2 set. User-story
/// phases may add additive variants; never remove or rename.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StageValidationOutcome {
    /// Stage passed its declared shape checks.
    Ok,
    /// Required input slot missing.
    InputMissing { input_name: String },
    /// Declared output does not match what the stage produced.
    OutputMismatch { output_name: String },
    /// DAG cycle detected at this stage.
    DependencyCycle { stage_name: StageName },
    /// Required capability unresolved on the active registry.
    CapabilityUnavailable { capability_id: CapabilityId },
}

/// Errors raised by the public contract surface that are not themselves
/// validation outcomes (those use the `*Outcome` enums above).
#[derive(Debug, thiserror::Error)]
pub enum ContractError {
    /// Schema-level failure with a locatable JSON path and message.
    #[error("schema invalid at {path}: {message}")]
    SchemaInvalid { path: String, message: String },

    /// Generic "the input shape is wrong" — used by parsers when the JSON
    /// payload itself is malformed (i.e. not just contract-invalid).
    #[error("malformed contract input: {0}")]
    Malformed(String),

    /// A required engine surface is unreachable from the public adapter.
    /// Reserved for the Phase-3 engine adapter (T029); Phase-2 helpers never
    /// return this variant.
    #[error("engine adapter unavailable: {0}")]
    EngineUnavailable(String),
}
