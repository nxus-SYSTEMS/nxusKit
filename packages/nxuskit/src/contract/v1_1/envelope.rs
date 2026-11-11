//! Artifact / history envelope DTOs (US1 floor).
//!
//! US1 ships the envelope shape end-to-end with a minimal `RunOutcome` set
//! (`Success`, `ValidationFailed`) sufficient to drive the Peeler-style
//! direct-consumption smoke and the canonical-hash / export-import contract.
//!
//! TODO(US4 / T057): expand `RunOutcome` to the full nine-variant set
//! (`success | validation_failed | capability_unavailable | auth_failed |
//! unreachable | timeout | cancelled | internal_only_denied |
//! partial_result`) required by SC-006, and add per-variant fixtures plus
//! replay-link helper breadth. Spec contract is unchanged; this is a
//! task-ordering deferral, not a scope reduction.
//!
//! Canonical-hash payload excludes `canonical_hash`, `display_hash`,
//! `legacy_display_aliases`, and `produced_at` (research.md §4,
//! contracts/artifact-history-envelope.md §canonical-payload).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::ids::{ActionId, CorrelationId, ProviderId, StageName};
use super::pipeline::StageArtifactRef;
use super::refs::{EnvelopeRef, PlanRef};
use super::validation::{ContractError, StageValidationOutcome};
use super::{
    ArtifactCanonicalHash, ArtifactDisplayHash, CONTRACT_VERSION, canonical_hash, display_short,
};

/// Top-level artifact / history envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactHistoryEnvelope {
    pub contract_version: String,
    pub artifact_id: String,
    pub canonical_hash: ArtifactCanonicalHash,
    pub display_hash: ArtifactDisplayHash,
    #[serde(default)]
    pub legacy_display_aliases: Vec<String>,
    pub produced_at: String,
    pub pipeline_ref: PipelineDefinitionRef,
    pub readiness_snapshot_ref: ReadinessSnapshotRef,
    #[serde(default)]
    pub inputs: Vec<StageArtifactRef>,
    #[serde(default)]
    pub outputs: Vec<StageArtifactRef>,
    pub validation_state: ValidationState,
    #[serde(default)]
    pub provenance: Vec<ProvenanceEntry>,
    #[serde(default)]
    pub source_references: Vec<SourceReference>,
    pub replay_linkage: ReplayLinkage,
    pub run_outcome: RunOutcome,
    // Feature 105 retargeting (T021): additive optional fields. Absent /
    // default fields contribute nothing to the canonical hash so feature 104
    // envelopes hash identically under feature 105.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_id: Option<ActionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<CorrelationId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_refs: Vec<EnvelopeRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output_refs: Vec<EnvelopeRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_ref: Option<PlanRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_kind: Option<EngineKind>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provenance_class: Vec<ProvenanceEntryClass>,
    #[serde(default)]
    pub metadata: BTreeMap<String, serde_json::Value>,

    // Feature 106 (v1.1.0 RC Peeler CLI update) — additive provenance
    // fields. Absent / default values are skipped on serialize so
    // existing feature 104 / feature 105 envelopes round-trip without
    // emitting new field names. None of these contributes to the
    // canonical hash for empty/None values, preserving the feature
    // 104/105 hash regression invariant.
    /// Per-operation fan-out outcomes for stages that expanded into
    /// parallel sibling operations (FR-026). Each entry's
    /// `operation_id` is stable and matches the runner's JSONL event
    /// stream and the corresponding [`super::FanOutOperationResult`]
    /// in the `PipelineRunReport`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fan_out_outcomes: Vec<super::run_report::FanOutGroupResult>,

    /// Resource-advisory context as it applied to this run, when the
    /// run was launched with `--preflight` (FR-026). Typed in US5 as
    /// the public-safe `ResourceAdvisory` DTO.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_advisory: Option<super::advisory::ResourceAdvisory>,

    /// Public-safe imported-example source reference when the pipeline
    /// originated from an SDK-discoverable nxusKit Examples definition
    /// (FR-026 / FR-046). Carries id, definition type, public
    /// repository alias, path, and ref only; never private repo paths
    /// or PHI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imported_example_ref: Option<ImportedExampleRef>,
}

impl ArtifactHistoryEnvelope {
    pub fn builder(artifact_id: impl Into<String>) -> ArtifactHistoryEnvelopeBuilder {
        ArtifactHistoryEnvelopeBuilder::new(artifact_id.into())
    }

    /// Export as canonical JSON (sorted keys, no whitespace).
    pub fn export_canonical_json(&self) -> Result<String, ContractError> {
        let value = serde_json::to_value(self)
            .map_err(|e| ContractError::Malformed(format!("serialize: {e}")))?;
        let bytes = super::hash::canonical_json_bytes(&value)
            .map_err(|e| ContractError::Malformed(e.to_string()))?;
        String::from_utf8(bytes).map_err(|e| ContractError::Malformed(e.to_string()))
    }

    /// Deserialize from canonical (or any well-formed) JSON.
    pub fn import_canonical_json(s: &str) -> Result<Self, ContractError> {
        serde_json::from_str(s).map_err(|e| ContractError::Malformed(format!("deserialize: {e}")))
    }
}

/// Builder for envelopes. Computes the canonical hash at build time.
#[derive(Debug, Clone)]
pub struct ArtifactHistoryEnvelopeBuilder {
    artifact_id: String,
    produced_at: Option<String>,
    pipeline_ref: Option<PipelineDefinitionRef>,
    readiness_snapshot_ref: Option<ReadinessSnapshotRef>,
    inputs: Vec<StageArtifactRef>,
    outputs: Vec<StageArtifactRef>,
    validation_state: ValidationState,
    provenance: Vec<ProvenanceEntry>,
    source_references: Vec<SourceReference>,
    replay_linkage: ReplayLinkage,
    run_outcome: RunOutcome,
    metadata: BTreeMap<String, serde_json::Value>,
    legacy_display_aliases: Vec<String>,
    action_id: Option<ActionId>,
    correlation_id: Option<CorrelationId>,
    input_refs: Vec<EnvelopeRef>,
    output_refs: Vec<EnvelopeRef>,
    plan_ref: Option<PlanRef>,
    engine_id: Option<String>,
    engine_kind: Option<EngineKind>,
    provenance_class: Vec<ProvenanceEntryClass>,
}

impl ArtifactHistoryEnvelopeBuilder {
    fn new(artifact_id: String) -> Self {
        Self {
            artifact_id,
            produced_at: None,
            pipeline_ref: None,
            readiness_snapshot_ref: None,
            inputs: Vec::new(),
            outputs: Vec::new(),
            validation_state: ValidationState::default(),
            provenance: Vec::new(),
            source_references: Vec::new(),
            replay_linkage: ReplayLinkage::fresh(),
            run_outcome: RunOutcome::Success,
            metadata: BTreeMap::new(),
            legacy_display_aliases: Vec::new(),
            action_id: None,
            correlation_id: None,
            input_refs: Vec::new(),
            output_refs: Vec::new(),
            plan_ref: None,
            engine_id: None,
            engine_kind: None,
            provenance_class: Vec::new(),
        }
    }

    pub fn pipeline_ref(
        mut self,
        pipeline_id: impl Into<String>,
        version: impl Into<String>,
    ) -> Self {
        self.pipeline_ref = Some(PipelineDefinitionRef {
            pipeline_id: pipeline_id.into(),
            version: version.into(),
            canonical_hash: None,
        });
        self
    }

    pub fn pipeline_ref_with_hash(
        mut self,
        pipeline_id: impl Into<String>,
        version: impl Into<String>,
        canonical_hash: ArtifactCanonicalHash,
    ) -> Self {
        self.pipeline_ref = Some(PipelineDefinitionRef {
            pipeline_id: pipeline_id.into(),
            version: version.into(),
            canonical_hash: Some(canonical_hash),
        });
        self
    }

    pub fn readiness_snapshot_captured_at(mut self, captured_at: impl Into<String>) -> Self {
        self.readiness_snapshot_ref = Some(ReadinessSnapshotRef {
            captured_at: captured_at.into(),
            canonical_hash: None,
        });
        self
    }

    pub fn produced_at(mut self, ts: impl Into<String>) -> Self {
        self.produced_at = Some(ts.into());
        self
    }

    pub fn with_legacy_display_alias(mut self, alias: impl Into<String>) -> Self {
        self.legacy_display_aliases.push(alias.into());
        self
    }

    pub fn run_outcome(mut self, o: RunOutcome) -> Self {
        self.run_outcome = o;
        self
    }

    pub fn validation_state(mut self, v: ValidationState) -> Self {
        self.validation_state = v;
        self
    }

    pub fn add_provenance(mut self, p: ProvenanceEntry) -> Self {
        self.provenance.push(p);
        self
    }

    pub fn add_output(mut self, r: StageArtifactRef) -> Self {
        self.outputs.push(r);
        self
    }

    pub fn replay_linkage(mut self, r: ReplayLinkage) -> Self {
        self.replay_linkage = r;
        self
    }

    // Feature 105 retargeting (T021) builder methods.
    pub fn action_id(mut self, id: ActionId) -> Self {
        self.action_id = Some(id);
        self
    }
    pub fn correlation_id(mut self, id: CorrelationId) -> Self {
        self.correlation_id = Some(id);
        self
    }
    pub fn add_input_ref(mut self, r: EnvelopeRef) -> Self {
        self.input_refs.push(r);
        self
    }
    pub fn add_output_ref(mut self, r: EnvelopeRef) -> Self {
        self.output_refs.push(r);
        self
    }
    pub fn plan_ref(mut self, p: PlanRef) -> Self {
        self.plan_ref = Some(p);
        self
    }
    pub fn engine_id(mut self, id: impl Into<String>) -> Self {
        self.engine_id = Some(id.into());
        self
    }
    pub fn engine_kind(mut self, k: EngineKind) -> Self {
        self.engine_kind = Some(k);
        self
    }
    pub fn add_provenance_class(mut self, c: ProvenanceEntryClass) -> Self {
        self.provenance_class.push(c);
        self
    }

    /// Build the envelope and compute the canonical hash.
    pub fn build(self) -> ArtifactHistoryEnvelope {
        let pipeline_ref = self.pipeline_ref.unwrap_or_default();
        let readiness_snapshot_ref = self.readiness_snapshot_ref.unwrap_or_default();
        let produced_at = self
            .produced_at
            .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_owned());

        // Compute canonical hash over the identity-bearing payload. Feature
        // 104 fields land in fixed order. Feature 105 additive fields are
        // appended only when present / non-default, so a feature-104 envelope
        // (none of the new fields set) hashes identically (AHE-001).
        let mut identity_payload = serde_json::Map::new();
        identity_payload.insert(
            "contract_version".to_owned(),
            serde_json::json!(CONTRACT_VERSION),
        );
        identity_payload.insert(
            "artifact_id".to_owned(),
            serde_json::json!(self.artifact_id),
        );
        identity_payload.insert(
            "pipeline_ref".to_owned(),
            serde_json::to_value(&pipeline_ref).unwrap(),
        );
        identity_payload.insert(
            "readiness_snapshot_ref".to_owned(),
            serde_json::to_value(&readiness_snapshot_ref).unwrap(),
        );
        identity_payload.insert(
            "inputs".to_owned(),
            serde_json::to_value(&self.inputs).unwrap(),
        );
        identity_payload.insert(
            "outputs".to_owned(),
            serde_json::to_value(&self.outputs).unwrap(),
        );
        identity_payload.insert(
            "validation_state".to_owned(),
            serde_json::to_value(&self.validation_state).unwrap(),
        );
        identity_payload.insert(
            "provenance".to_owned(),
            serde_json::to_value(&self.provenance).unwrap(),
        );
        identity_payload.insert(
            "source_references".to_owned(),
            serde_json::to_value(&self.source_references).unwrap(),
        );
        identity_payload.insert(
            "replay_linkage".to_owned(),
            serde_json::to_value(&self.replay_linkage).unwrap(),
        );
        identity_payload.insert(
            "run_outcome".to_owned(),
            serde_json::to_value(self.run_outcome).unwrap(),
        );
        identity_payload.insert(
            "metadata".to_owned(),
            serde_json::to_value(&self.metadata).unwrap(),
        );
        // Feature 105 additive contributions — only when non-default.
        if let Some(ref id) = self.action_id {
            identity_payload.insert("action_id".to_owned(), serde_json::to_value(id).unwrap());
        }
        if let Some(ref id) = self.correlation_id {
            identity_payload.insert(
                "correlation_id".to_owned(),
                serde_json::to_value(id).unwrap(),
            );
        }
        if !self.input_refs.is_empty() {
            identity_payload.insert(
                "input_refs".to_owned(),
                serde_json::to_value(&self.input_refs).unwrap(),
            );
        }
        if !self.output_refs.is_empty() {
            identity_payload.insert(
                "output_refs".to_owned(),
                serde_json::to_value(&self.output_refs).unwrap(),
            );
        }
        if let Some(ref p) = self.plan_ref {
            identity_payload.insert("plan_ref".to_owned(), serde_json::to_value(p).unwrap());
        }
        if let Some(ref e) = self.engine_id {
            identity_payload.insert("engine_id".to_owned(), serde_json::to_value(e).unwrap());
        }
        if let Some(k) = self.engine_kind {
            identity_payload.insert("engine_kind".to_owned(), serde_json::to_value(k).unwrap());
        }
        if !self.provenance_class.is_empty() {
            identity_payload.insert(
                "provenance_class".to_owned(),
                serde_json::to_value(&self.provenance_class).unwrap(),
            );
        }
        let identity_value = serde_json::Value::Object(identity_payload);
        let canonical = canonical_hash(&identity_value)
            .expect("canonical hash of in-memory envelope payload must succeed");
        let display = display_short(&canonical);

        ArtifactHistoryEnvelope {
            contract_version: CONTRACT_VERSION.to_owned(),
            artifact_id: self.artifact_id,
            canonical_hash: canonical,
            display_hash: display,
            legacy_display_aliases: self.legacy_display_aliases,
            produced_at,
            pipeline_ref,
            readiness_snapshot_ref,
            inputs: self.inputs,
            outputs: self.outputs,
            validation_state: self.validation_state,
            provenance: self.provenance,
            source_references: self.source_references,
            replay_linkage: self.replay_linkage,
            run_outcome: self.run_outcome,
            action_id: self.action_id,
            correlation_id: self.correlation_id,
            input_refs: self.input_refs,
            output_refs: self.output_refs,
            plan_ref: self.plan_ref,
            engine_id: self.engine_id,
            engine_kind: self.engine_kind,
            provenance_class: self.provenance_class,
            metadata: self.metadata,
            // Feature 106 additive fields default to empty / None; the
            // builder does not yet expose setters for them, so US4
            // runner code that needs to populate them does so by direct
            // mutation after the build.
            fan_out_outcomes: Vec::new(),
            resource_advisory: None,
            imported_example_ref: None,
        }
    }
}

/// Snapshot reference to a pipeline definition.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PipelineDefinitionRef {
    pub pipeline_id: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_hash: Option<ArtifactCanonicalHash>,
}

/// Snapshot reference to a readiness snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ReadinessSnapshotRef {
    pub captured_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_hash: Option<ArtifactCanonicalHash>,
}

/// Validation-state block carried in the envelope.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ValidationState {
    #[serde(default)]
    pub stages: Vec<StageValidationOutcome>,
}

/// One provenance row (per-stage execution record).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvenanceEntry {
    pub stage_name: StageName,
    pub capability_id: super::ids::CapabilityId,
    pub provider_id: ProviderId,
    pub timestamp: String,
    pub outcome: StageValidationOutcome,
}

/// External-source reference (document URI, dataset, etc.).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceReference {
    pub uri: String,
    #[serde(default)]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

/// Replay linkage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayLinkage {
    pub replay_kind: ReplayKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_envelope_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_canonical_hash: Option<ArtifactCanonicalHash>,
}

impl ReplayLinkage {
    pub fn fresh() -> Self {
        Self {
            replay_kind: ReplayKind::Fresh,
            source_envelope_id: None,
            source_canonical_hash: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayKind {
    Fresh,
    ReplayOf,
    PartialReplayOf,
}

/// Run-outcome enumeration. US1 ships only the two variants the
/// direct-consumption smoke and US1 tests need (`Success`,
/// `ValidationFailed`).
///
/// TODO(US4 / T057): expand to the full nine-variant set required by
/// SC-006 — `capability_unavailable | auth_failed | unreachable | timeout
/// | cancelled | internal_only_denied | partial_result`. US1 deliberately
/// avoids claiming those variants because their semantics are tied to
/// outcomes US1 does not produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunOutcome {
    Success,
    ValidationFailed,
}

/// Feature 105 retargeting (T021): public-safe engine kind. Identifies the
/// engine class behind an envelope without leaking engine-internal topology.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineKind {
    Llm,
    ClipsRules,
    Solver,
    Bayesian,
    ZenDecisionTable,
    External,
    Other,
}

/// Feature 105 retargeting (T021): provenance entry classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceEntryClass {
    DataPlaneReference,
    ControlPlaneAction,
    ValidationOutcome,
    ReadinessSnapshot,
    Other,
}

// ---------------------------------------------------------------------------
// Feature 106 (v1.1.0 RC Peeler CLI update) — ImportedExampleRef DTO.
// FR-026, FR-046. Public-safe reference: id, definition type, public
// repo alias, path, ref. Constructor `try_public` enforces the
// `contracts/public-boundary-leakcheck.md` deny-list at the DTO layer.
// ---------------------------------------------------------------------------

/// Public-safe reference to a pipeline / declaration-only orchestration
/// definition that originated from an SDK-discoverable nxusKit Examples
/// manifest entry.
///
/// The fields are intentionally narrow: id, definition type, public
/// repository alias, path, and ref. No private repo paths, no
/// authentication tokens, no PHI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportedExampleRef {
    pub example_id: String,
    pub definition_type: ImportedExampleType,
    /// Public repository alias (e.g., `nxusKit-examples`). Internal
    /// repo names from the public-boundary deny-list MUST NOT appear
    /// here; the [`try_public`] constructor enforces this.
    ///
    /// [`try_public`]: ImportedExampleRef::try_public
    pub source_repository: String,
    pub source_path: String,
    pub source_ref: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportedExampleType {
    Pipeline,
    Orchestration,
}

/// Error returned by [`ImportedExampleRef::try_public`] when the
/// repository name matches a documented internal marker.
#[derive(Debug, thiserror::Error)]
pub enum ImportedExampleRefError {
    #[error("source_repository `{0}` matches an internal-only marker; use the public alias")]
    PrivateRepository(String),
    #[error("field `{0}` must not be empty")]
    EmptyField(&'static str),
}

impl ImportedExampleRef {
    /// Public-safe constructor that refuses internal-only repo names.
    /// Use this rather than the struct literal in CLI / runner code.
    pub fn try_public(
        example_id: impl Into<String>,
        definition_type: ImportedExampleType,
        source_repository: impl Into<String>,
        source_path: impl Into<String>,
        source_ref: impl Into<String>,
    ) -> Result<Self, ImportedExampleRefError> {
        let example_id = example_id.into();
        let source_repository = source_repository.into();
        let source_path = source_path.into();
        let source_ref = source_ref.into();
        if example_id.is_empty() {
            return Err(ImportedExampleRefError::EmptyField("example_id"));
        }
        if source_repository.is_empty() {
            return Err(ImportedExampleRefError::EmptyField("source_repository"));
        }
        if source_path.is_empty() {
            return Err(ImportedExampleRefError::EmptyField("source_path"));
        }
        if source_ref.is_empty() {
            return Err(ImportedExampleRefError::EmptyField("source_ref"));
        }
        const INTERNAL_MARKERS: &[&str] = &[
            concat!("nxusKit", "-internal"),
            "nxusKit-examples-internal",
            "peeler-internal",
            "peeler-internals",
            "peeler-personas",
        ];
        for needle in INTERNAL_MARKERS {
            if source_repository.contains(needle) {
                return Err(ImportedExampleRefError::PrivateRepository(
                    source_repository,
                ));
            }
        }
        Ok(Self {
            example_id,
            definition_type,
            source_repository,
            source_path,
            source_ref,
        })
    }
}
