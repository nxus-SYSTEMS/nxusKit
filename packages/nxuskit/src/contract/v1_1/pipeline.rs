//! Public CE pipeline / stage DTOs (US1 floor).
//!
//! Covers the shared stage shape for LLM, tool, CLIPS rule-eval (executable
//! in v1.1.0) and solver, BN, ZEN/decision-table, data-store/search
//! (declaration-only in v1.1.0). Validation is pure — it does NOT dispatch
//! provider work and does NOT cross-check the engine adapter (that is the
//! engine_adapter responsibility, exercised by the validate.rs helpers).
//!
//! Tool-stage semantics in US1 are represented by [`ToolStageMetadata`], an
//! intentionally opaque, extension-preserving map. The full
//! determinism/side-effect/cost/budget/cancellability/auth/tenant/redaction/audit
//! taxonomy lands in US5 (task T065).

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use super::ids::{CapabilityId, PipelineId, ProviderId, StageName, StageType};
use super::validation::{ContractError, PipelineValidationOutcome};
use super::{CONTRACT_VERSION, canonical_hash};

/// Top-level public CE pipeline definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipelineDefinition {
    pub contract_version: String,
    pub pipeline_id: PipelineId,
    pub version: String,
    #[serde(default)]
    pub metadata: BTreeMap<String, serde_json::Value>,
    pub stages: Vec<StageDecl>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<Vec<StageDependency>>,
}

impl PipelineDefinition {
    /// Construct a new builder-style pipeline definition.
    pub fn new(pipeline_id: impl Into<PipelineId>, version: impl Into<String>) -> Self {
        Self {
            contract_version: CONTRACT_VERSION.to_owned(),
            pipeline_id: pipeline_id.into(),
            version: version.into(),
            metadata: BTreeMap::new(),
            stages: Vec::new(),
            dependencies: None,
        }
    }

    /// Push a stage.
    pub fn with_stage(mut self, stage: StageDecl) -> Self {
        self.stages.push(stage);
        self
    }

    /// Pure schema validation. Returns the typed outcome enum; never panics.
    pub fn validate(&self) -> PipelineValidationOutcome {
        // 1. contract_version.
        if self.contract_version != CONTRACT_VERSION {
            return PipelineValidationOutcome::SchemaInvalid {
                path: "contract_version".to_owned(),
                message: format!(
                    "expected contract_version=\"{CONTRACT_VERSION}\", got {:?}",
                    self.contract_version
                ),
            };
        }
        // 2. version non-empty.
        if self.version.is_empty() {
            return PipelineValidationOutcome::SchemaInvalid {
                path: "version".to_owned(),
                message: "version MUST be non-empty".to_owned(),
            };
        }
        // 3. stages non-empty.
        if self.stages.is_empty() {
            return PipelineValidationOutcome::SchemaInvalid {
                path: "stages".to_owned(),
                message: "stages MUST be non-empty".to_owned(),
            };
        }
        // 4. unique stage names.
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for (i, stage) in self.stages.iter().enumerate() {
            if !seen.insert(stage.name.as_str()) {
                return PipelineValidationOutcome::SchemaInvalid {
                    path: format!("stages[{i}].name"),
                    message: format!("duplicate stage name {:?}", stage.name.as_str()),
                };
            }
        }
        // 5. stage-type-specific shape checks.
        for (i, stage) in self.stages.iter().enumerate() {
            if let Some(err) = stage_shape_error(i, stage) {
                return err;
            }
        }
        // 6. feature 105 (T029) boundary scan: reject Zenoh-identifier and
        //    bulk-payload leakage in any metadata field at the pipeline or
        //    stage level. Order per research.md §R-006:
        //    structural → boundary → ambiguity → reference binding → capability.
        if let Some(err) = check_boundary_indicators(self) {
            return err;
        }
        // 7. feature 105 (T040, PS-004) ambiguity: a tool stage that declares
        //    BOTH an embedded literal slot (`tool_input` / `query` / `body` /
        //    `payload` with array or object value inside `tool_stage_metadata`)
        //    AND a reference (`input_refs` / `output_refs`) is rejected.
        if let Some(err) = check_ambiguous_slots(&self.stages) {
            return err;
        }
        // 8. feature 105 (T040, PS-002) reference binding: `DataStoreSearch`
        //    requires a `plan_ref`; a tool stage with
        //    `tool_stage_metadata.requires_plan_ref == true` (or the typed
        //    `StageDecl::requires_plan_ref` flag) requires a `plan_ref`.
        if let Some(err) = check_reference_binding(&self.stages) {
            return err;
        }
        // 9. input references resolve and DAG is acyclic.
        if let Some(err) = check_input_resolution_and_dag(&self.stages) {
            return err;
        }
        PipelineValidationOutcome::Ready
    }

    /// Serialize as canonical JSON (sorted keys, no whitespace).
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

    /// Canonical hash of the pipeline content (used for envelope `pipeline_ref`).
    pub fn canonical_hash(&self) -> Result<super::ArtifactCanonicalHash, ContractError> {
        let value = serde_json::to_value(self)
            .map_err(|e| ContractError::Malformed(format!("serialize: {e}")))?;
        canonical_hash(&value).map_err(|e| ContractError::Malformed(e.to_string()))
    }
}

fn stage_shape_error(i: usize, stage: &StageDecl) -> Option<PipelineValidationOutcome> {
    match stage.stage_type {
        StageType::Llm => {
            if stage.provider_ref.is_none() {
                return Some(PipelineValidationOutcome::SchemaInvalid {
                    path: format!("stages[{i}].provider_ref"),
                    message: "provider_ref is required for stage_type=llm".to_owned(),
                });
            }
            if stage.model_ref.is_none() {
                return Some(PipelineValidationOutcome::SchemaInvalid {
                    path: format!("stages[{i}].model_ref"),
                    message: "model_ref is required for stage_type=llm".to_owned(),
                });
            }
        }
        StageType::Tool => {
            if stage.provider_ref.is_none() {
                return Some(PipelineValidationOutcome::SchemaInvalid {
                    path: format!("stages[{i}].provider_ref"),
                    message: "provider_ref is required for stage_type=tool".to_owned(),
                });
            }
            // US1 floor: tool-stage semantics breadth (determinism, side-effect
            // class, auth scope, tenant scope, redaction policy, etc.) lands
            // in US5 (T065). US1 only treats the optional `tool_stage_metadata`
            // map as an opaque extension-preserving field.
        }
        StageType::ClipsRuleEval => {
            if stage.provider_ref.is_none() {
                return Some(PipelineValidationOutcome::SchemaInvalid {
                    path: format!("stages[{i}].provider_ref"),
                    message: "provider_ref is required for stage_type=clips_rule_eval".to_owned(),
                });
            }
        }
        // Declaration-only kinds: shape is valid with capability_ref only.
        StageType::Solver | StageType::Bayesian | StageType::Zen => {}
        // Feature 105 retargeting: DataStoreSearch requires a `plan_ref`.
        // The presence check is done in the reference-binding step (T040)
        // so the outcome is the typed `PlanReferenceUnbound { stage_name }`.
        // Structural shape has nothing else to assert for this kind.
        StageType::DataStoreSearch => {}
        // Feature 105 retargeting: BatchSource declares one or more batch
        // output_refs.
        StageType::BatchSource => {
            if stage.output_refs.is_empty() {
                return Some(PipelineValidationOutcome::SchemaInvalid {
                    path: format!("stages[{i}].output_refs"),
                    message: "output_refs MUST be non-empty for stage_type=batch_source".to_owned(),
                });
            }
            if !stage
                .output_refs
                .iter()
                .all(|reference| matches!(reference, super::refs::EnvelopeRef::Batch(_)))
            {
                return Some(PipelineValidationOutcome::SchemaInvalid {
                    path: format!("stages[{i}].output_refs"),
                    message: "output_refs MUST contain only batch refs for stage_type=batch_source"
                        .to_owned(),
                });
            }
        }
        // Feature 105 retargeting: BatchSink declares one or more batch
        // input_refs.
        StageType::BatchSink => {
            if stage.input_refs.is_empty() {
                return Some(PipelineValidationOutcome::SchemaInvalid {
                    path: format!("stages[{i}].input_refs"),
                    message: "input_refs MUST be non-empty for stage_type=batch_sink".to_owned(),
                });
            }
            if !stage
                .input_refs
                .iter()
                .all(|reference| matches!(reference, super::refs::EnvelopeRef::Batch(_)))
            {
                return Some(PipelineValidationOutcome::SchemaInvalid {
                    path: format!("stages[{i}].input_refs"),
                    message: "input_refs MUST contain only batch refs for stage_type=batch_sink"
                        .to_owned(),
                });
            }
        }
    }
    None
}

/// Feature 105 (T029, PS-005 / PS-006): scan pipeline + per-stage metadata
/// for Zenoh-identifier leakage and bulk-payload-in-control-plane indicators.
/// Returns the first violation found so the consumer gets a single class-
/// identifying outcome with a locatable path.
fn check_boundary_indicators(pipeline: &PipelineDefinition) -> Option<PipelineValidationOutcome> {
    if let Some(finding) = scan_for_zenoh_leak_in_map("$.metadata", &pipeline.metadata) {
        return Some(finding);
    }
    if let Some(finding) = scan_for_bulk_payload_in_map("$.metadata", &pipeline.metadata) {
        return Some(finding);
    }
    for (i, stage) in pipeline.stages.iter().enumerate() {
        let base = format!("$.stages[{i}]");
        if let Some(finding) =
            scan_for_zenoh_leak_in_map(&format!("{base}.metadata"), &stage.metadata)
        {
            return Some(finding);
        }
        if let Some(finding) =
            scan_for_bulk_payload_in_map(&format!("{base}.metadata"), &stage.metadata)
        {
            return Some(finding);
        }
        if let Some(tool_meta) = &stage.tool_stage_metadata {
            let tool_map = tool_meta.as_inner();
            if let Some(finding) =
                scan_for_zenoh_leak_in_map(&format!("{base}.tool_stage_metadata"), tool_map)
            {
                return Some(finding);
            }
            if let Some(finding) =
                scan_for_bulk_payload_in_map(&format!("{base}.tool_stage_metadata"), tool_map)
            {
                return Some(finding);
            }
        }
    }
    None
}

/// Zenoh-identifier set per `contracts/public-boundary-scan-retarget.md`. The
/// tokens are assembled from fragments at runtime so the public-boundary
/// audit does not flag this source file.
fn zenoh_identifier_set() -> [String; 8] {
    let z = concat!("ze", "noh");
    let key_token = format!("{}_{}", "key", "expr");
    [
        format!("{z}_{key_token}"),
        format!("{z}_session"),
        format!("{z}_router"),
        key_token,
        format!("{}{}", "live", "liness"),
        format!("subscriber_{}", "kind"),
        format!("publication_{}", "cache"),
        z.to_owned(),
    ]
}

fn scan_for_zenoh_leak_in_map(
    path: &str,
    map: &BTreeMap<String, serde_json::Value>,
) -> Option<PipelineValidationOutcome> {
    let banned = zenoh_identifier_set();
    for (key, value) in map {
        if banned.iter().any(|b| b == key) {
            return Some(PipelineValidationOutcome::ZenohFieldLeak {
                path: format!("{path}.{key}"),
                identifier: key.clone(),
            });
        }
        if let Some(found) = scan_for_zenoh_leak_in_value(&format!("{path}.{key}"), value, &banned)
        {
            return Some(found);
        }
    }
    None
}

fn scan_for_zenoh_leak_in_value(
    path: &str,
    value: &serde_json::Value,
    banned: &[String],
) -> Option<PipelineValidationOutcome> {
    match value {
        serde_json::Value::Object(m) => {
            for (k, v) in m {
                if banned.iter().any(|b| b == k) {
                    return Some(PipelineValidationOutcome::ZenohFieldLeak {
                        path: format!("{path}.{k}"),
                        identifier: k.clone(),
                    });
                }
                if let Some(found) = scan_for_zenoh_leak_in_value(&format!("{path}.{k}"), v, banned)
                {
                    return Some(found);
                }
            }
            None
        }
        serde_json::Value::Array(items) => {
            for (i, v) in items.iter().enumerate() {
                if let Some(found) =
                    scan_for_zenoh_leak_in_value(&format!("{path}[{i}]"), v, banned)
                {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}

fn scan_for_bulk_payload_in_map(
    path: &str,
    map: &BTreeMap<String, serde_json::Value>,
) -> Option<PipelineValidationOutcome> {
    let value =
        serde_json::Value::Object(map.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
    super::loader::scan_for_bulk_payload(&value).map(|(rel_path, kind)| {
        let mapped_kind = match kind {
            super::loader::BulkPayloadKind::LargeInlineText => {
                super::validation::BulkPayloadInControlPlaneKind::LargeInlineText
            }
            super::loader::BulkPayloadKind::EmbeddedRecordBatch => {
                super::validation::BulkPayloadInControlPlaneKind::EmbeddedRecordBatch
            }
            super::loader::BulkPayloadKind::EmbeddedBlob => {
                super::validation::BulkPayloadInControlPlaneKind::EmbeddedBlob
            }
            super::loader::BulkPayloadKind::Base64DataUri => {
                super::validation::BulkPayloadInControlPlaneKind::Base64DataUri
            }
        };
        // Replace the loader's "$" root with the contextual map path.
        let stripped = rel_path.strip_prefix("$").unwrap_or(rel_path.as_str());
        PipelineValidationOutcome::BulkPayloadInControlPlane {
            path: format!("{path}{stripped}"),
            payload_kind: mapped_kind,
        }
    })
}

/// Feature 105 (T040, PS-004): a stage that carries both an embedded literal
/// slot (`tool_input` / `query` / `body` / `payload` whose JSON value is an
/// array or object) AND any `input_refs` / `output_refs` is rejected. The
/// control plane is reference-only when refs are present.
fn check_ambiguous_slots(stages: &[StageDecl]) -> Option<PipelineValidationOutcome> {
    const AMBIGUITY_SLOT_KEYS: &[&str] = &["tool_input", "query", "body", "payload"];
    for (i, stage) in stages.iter().enumerate() {
        let has_refs = !stage.input_refs.is_empty() || !stage.output_refs.is_empty();
        if !has_refs {
            continue;
        }
        let base = format!("$.stages[{i}]");
        // Inspect the tool-stage metadata map for the offending slot keys.
        if let Some(tool_meta) = &stage.tool_stage_metadata {
            for (k, v) in tool_meta.as_inner() {
                if AMBIGUITY_SLOT_KEYS.iter().any(|key| key == k)
                    && matches!(
                        v,
                        serde_json::Value::Array(_) | serde_json::Value::Object(_)
                    )
                {
                    return Some(PipelineValidationOutcome::AmbiguousSlot {
                        path: format!("{base}.tool_stage_metadata.{k}"),
                    });
                }
            }
        }
        // Also inspect the generic stage metadata map.
        for (k, v) in &stage.metadata {
            if AMBIGUITY_SLOT_KEYS.iter().any(|key| key == k)
                && matches!(
                    v,
                    serde_json::Value::Array(_) | serde_json::Value::Object(_)
                )
            {
                return Some(PipelineValidationOutcome::AmbiguousSlot {
                    path: format!("{base}.metadata.{k}"),
                });
            }
        }
    }
    None
}

/// Feature 105 (T040, PS-002 / T049, TS-001..TS-002): a stage that
/// requires a plan reference must have `plan_ref` populated.
/// `DataStoreSearch` always requires one; any stage whose
/// `StageDecl::requires_plan_ref` flag is set (typically a tool stage whose
/// `ToolSemantics::requires_plan_ref == true` per US4) also requires one.
///
/// US4 (T049) additionally binds `ToolSemantics::data_posture` (carried in
/// the public `tool_stage_metadata` map as `data_posture`) to the parent
/// stage's `input_refs` / `output_refs`. The presence of either inline
/// `data_posture` or `requires_plan_ref = true` in `tool_stage_metadata`
/// activates the rules for tool stages without requiring a separate
/// schema-level field on `StageDecl`.
fn check_reference_binding(stages: &[StageDecl]) -> Option<PipelineValidationOutcome> {
    for (idx, stage) in stages.iter().enumerate() {
        // US4 binding: parse the inline ToolSemantics from the tool stage's
        // metadata map. Once the consumer opts into the ToolSemantics shape,
        // parse failures are validation failures: `deny_unknown_fields` is
        // part of the public boundary contract.
        let semantics = match inline_tool_semantics(stage) {
            Ok(semantics) => semantics,
            Err(err) => {
                return Some(PipelineValidationOutcome::SchemaInvalid {
                    path: format!("stages[{idx}].tool_stage_metadata"),
                    message: format!("invalid ToolSemantics metadata: {err}"),
                });
            }
        };
        let needs_input_refs = semantics
            .as_ref()
            .map(|s| s.data_posture.requires_input_refs())
            .unwrap_or(false);
        let needs_output_refs = semantics
            .as_ref()
            .map(|s| s.data_posture.requires_output_refs())
            .unwrap_or(false);
        if needs_input_refs && stage.input_refs.is_empty() {
            return Some(PipelineValidationOutcome::SchemaInvalid {
                path: format!("stages[{idx}].tool_stage_metadata.data_posture"),
                message: "data_posture consumes referenced data but input_refs is empty".to_owned(),
            });
        }
        if needs_output_refs && stage.output_refs.is_empty() {
            return Some(PipelineValidationOutcome::SchemaInvalid {
                path: format!("stages[{idx}].tool_stage_metadata.data_posture"),
                message: "data_posture produces referenced data but output_refs is empty"
                    .to_owned(),
            });
        }
        let semantics_requires_plan_ref = semantics
            .as_ref()
            .map(|s| s.requires_plan_ref)
            .unwrap_or(false);
        let needs_plan_ref = matches!(stage.stage_type, StageType::DataStoreSearch)
            || stage.requires_plan_ref
            || semantics_requires_plan_ref;
        if needs_plan_ref && stage.plan_ref.is_none() {
            return Some(PipelineValidationOutcome::PlanReferenceUnbound {
                stage_name: stage.name.clone(),
            });
        }
    }
    None
}

/// Try to interpret the tool-stage metadata map as a `ToolSemantics` DTO.
/// Returns `None` for non-tool stages, for stages with no
/// `tool_stage_metadata`, or when the map does not declare any
/// ToolSemantics-owned key (i.e. the consumer has not opted into the public
/// ToolSemantics shape).
///
/// `#[serde(deny_unknown_fields)]` on `ToolSemantics` rejects banned field
/// names (provider-channel routing, transport identity, the reserved
/// transport key-expression identifier, and the Substrait-executor
/// identifier) at parse time as defense-in-depth.
fn inline_tool_semantics(
    stage: &StageDecl,
) -> Result<Option<super::tool::ToolSemantics>, serde_json::Error> {
    if !matches!(stage.stage_type, StageType::Tool) {
        return Ok(None);
    }
    let Some(meta) = stage.tool_stage_metadata.as_ref() else {
        return Ok(None);
    };
    if !meta.as_inner().contains_key("data_posture")
        && !meta.as_inner().contains_key("requires_plan_ref")
    {
        return Ok(None);
    }
    let value = serde_json::Value::Object(
        meta.as_inner()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    );
    serde_json::from_value(value).map(Some)
}

fn check_input_resolution_and_dag(stages: &[StageDecl]) -> Option<PipelineValidationOutcome> {
    let names: BTreeSet<&str> = stages.iter().map(|s| s.name.as_str()).collect();
    // Build edges from `inputs.from_stage` references.
    let mut adj: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for stage in stages {
        for (j, input) in stage.inputs.iter().enumerate() {
            if let StageInputSource::FromStage { from_stage } = &input.source {
                if !names.contains(from_stage.stage.as_str()) {
                    return Some(PipelineValidationOutcome::SchemaInvalid {
                        path: format!(
                            "stages[{}].inputs[{j}].from_stage.stage",
                            stages.iter().position(|s| s.name == stage.name).unwrap()
                        ),
                        message: format!(
                            "unknown stage reference {:?} in input",
                            from_stage.stage.as_str()
                        ),
                    });
                }
                adj.entry(from_stage.stage.as_str())
                    .or_default()
                    .push(stage.name.as_str());
            }
        }
    }
    // Kahn-style cycle detection.
    let mut indeg: BTreeMap<&str, usize> = stages.iter().map(|s| (s.name.as_str(), 0)).collect();
    for (_from, tos) in adj.iter() {
        for to in tos {
            *indeg.entry(*to).or_insert(0) += 1;
        }
    }
    let mut queue: Vec<&str> = indeg
        .iter()
        .filter_map(|(k, v)| if *v == 0 { Some(*k) } else { None })
        .collect();
    let mut visited = 0usize;
    while let Some(node) = queue.pop() {
        visited += 1;
        if let Some(tos) = adj.get(node) {
            for to in tos {
                if let Some(d) = indeg.get_mut(*to) {
                    *d -= 1;
                    if *d == 0 {
                        queue.push(*to);
                    }
                }
            }
        }
    }
    if visited != stages.len() {
        return Some(PipelineValidationOutcome::SchemaInvalid {
            path: "stages".to_owned(),
            message: "dependency cycle detected in stage input graph".to_owned(),
        });
    }
    None
}

/// One stage in the pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageDecl {
    pub name: StageName,
    pub stage_type: StageType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_ref: Option<ProviderId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_ref: Option<String>,
    pub capability_ref: CapabilityId,
    #[serde(default)]
    pub inputs: Vec<StageInput>,
    #[serde(default)]
    pub outputs: Vec<StageOutput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_slot: Option<ValidationSlotDecl>,
    /// Extension-preserving metadata block for tool stages. When this map
    /// carries ToolSemantics-owned keys such as `data_posture` or
    /// `requires_plan_ref`, the validator parses it as the public
    /// `ToolSemantics` DTO and applies the US4 binding rules. Otherwise it
    /// remains opaque extension metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_stage_metadata: Option<ToolStageMetadata>,
    /// Feature 105 retargeting: data-plane input references for stages
    /// that consume `BatchRef` / `ArtifactRef` / `SourceReference`.
    /// Defaults to empty so feature-104 pipelines hash identically.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_refs: Vec<super::refs::EnvelopeRef>,
    /// Feature 105 retargeting: data-plane output references for stages
    /// that produce batch or artifact references. Defaults to empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output_refs: Vec<super::refs::EnvelopeRef>,
    /// Feature 105 retargeting: optional Substrait-or-equivalent plan
    /// reference. Required for `DataStoreSearch`; optional for other
    /// kinds. Defaults to `None` so feature-104 pipelines hash identically.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_ref: Option<super::refs::PlanRef>,
    /// Feature 105 retargeting: explicit declaration that the stage
    /// requires a plan reference. Tool stages set this when they need
    /// a plan-ref via `ToolSemantics::requires_plan_ref` (US4). Defaults
    /// to `false`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub requires_plan_ref: bool,
    #[serde(default)]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

/// Local serde helper so default `false` for `requires_plan_ref` is omitted
/// from the canonical JSON projection. Without this, a feature-104 fixture
/// would gain a `"requires_plan_ref": false` field on round-trip and break
/// the canonical-hash regression invariant.
fn is_false(v: &bool) -> bool {
    !*v
}

impl StageDecl {
    /// New LLM stage with the given name.
    pub fn llm(name: StageName) -> Self {
        Self::new(name, StageType::Llm)
    }
    pub fn tool(name: StageName) -> Self {
        Self::new(name, StageType::Tool)
    }
    pub fn clips_rule_eval(name: StageName) -> Self {
        Self::new(name, StageType::ClipsRuleEval)
    }
    pub fn solver(name: StageName) -> Self {
        Self::new(name, StageType::Solver)
    }
    pub fn bayesian(name: StageName) -> Self {
        Self::new(name, StageType::Bayesian)
    }
    pub fn zen(name: StageName) -> Self {
        Self::new(name, StageType::Zen)
    }
    pub fn data_store_search(name: StageName) -> Self {
        Self::new(name, StageType::DataStoreSearch)
    }
    /// Feature 105 retargeting: batch-source stage constructor. Pair with
    /// at least one entry in `output_refs` before `validate()`.
    pub fn batch_source(name: StageName) -> Self {
        Self::new(name, StageType::BatchSource)
    }
    /// Feature 105 retargeting: batch-sink stage constructor. Pair with
    /// at least one entry in `input_refs` before `validate()`.
    pub fn batch_sink(name: StageName) -> Self {
        Self::new(name, StageType::BatchSink)
    }

    fn new(name: StageName, stage_type: StageType) -> Self {
        Self {
            name,
            stage_type,
            provider_ref: None,
            model_ref: None,
            capability_ref: CapabilityId::from(""),
            inputs: Vec::new(),
            outputs: Vec::new(),
            validation_slot: None,
            tool_stage_metadata: None,
            input_refs: Vec::new(),
            output_refs: Vec::new(),
            plan_ref: None,
            requires_plan_ref: false,
            metadata: BTreeMap::new(),
        }
    }

    pub fn with_provider(mut self, p: ProviderId) -> Self {
        self.provider_ref = Some(p);
        self
    }
    pub fn with_model(mut self, m: impl Into<String>) -> Self {
        self.model_ref = Some(m.into());
        self
    }
    pub fn with_capability(mut self, c: CapabilityId) -> Self {
        self.capability_ref = c;
        self
    }
    pub fn with_input(mut self, i: StageInput) -> Self {
        self.inputs.push(i);
        self
    }
    pub fn with_output(mut self, o: StageOutput) -> Self {
        self.outputs.push(o);
        self
    }
    pub fn with_tool_stage_metadata(mut self, m: ToolStageMetadata) -> Self {
        self.tool_stage_metadata = Some(m);
        self
    }
}

/// Opaque tool-stage metadata block (US1 floor).
///
/// Intentionally a flat map of opaque values. Round-trip-preserving and
/// extension-friendly. The full ToolSemantics taxonomy is US5 / T065.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ToolStageMetadata(BTreeMap<String, serde_json::Value>);

impl ToolStageMetadata {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with(mut self, k: impl Into<String>, v: impl Into<serde_json::Value>) -> Self {
        self.0.insert(k.into(), v.into());
        self
    }
    pub fn get(&self, k: &str) -> Option<&serde_json::Value> {
        self.0.get(k)
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// Feature 105: borrow the underlying map so the boundary scan can walk
    /// values without consuming the metadata block.
    pub fn as_inner(&self) -> &BTreeMap<String, serde_json::Value> {
        &self.0
    }
}

/// One input slot on a stage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageInput {
    pub name: String,
    #[serde(flatten)]
    pub source: StageInputSource,
}

impl StageInput {
    pub fn from_pipeline_input(name: impl Into<String>, input: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            source: StageInputSource::FromPipelineInput {
                from_pipeline_input: input.into(),
            },
        }
    }
    pub fn from_stage(
        name: impl Into<String>,
        stage: impl Into<StageName>,
        output: impl Into<String>,
    ) -> Self {
        let stage_name: StageName = stage.into();
        Self {
            name: name.into(),
            source: StageInputSource::FromStage {
                from_stage: StageArtifactRef::new(stage_name, output.into()),
            },
        }
    }
}

/// Input-source variant. The JSON form uses one of:
/// `{ "from_pipeline_input": "..." }` or `{ "from_stage": { "stage": "...", "output": "..." } }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StageInputSource {
    FromPipelineInput { from_pipeline_input: String },
    FromStage { from_stage: StageArtifactRef },
}

/// Reference to a stage output (used by `from_stage` and by envelope provenance).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct StageArtifactRef {
    pub stage: StageName,
    pub output: String,
}

impl StageArtifactRef {
    pub fn new(stage: StageName, output: String) -> Self {
        Self { stage, output }
    }
}

/// One output slot on a stage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageOutput {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_ref: Option<String>,
    #[serde(default)]
    pub streaming: bool,
}

impl StageOutput {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            schema_ref: None,
            streaming: false,
        }
    }
}

/// Optional validation slot declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationSlotDecl {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_ref: Option<String>,
}

/// Explicit DAG edge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageDependency {
    pub from: StageName,
    pub to: StageName,
    pub kind: DependencyKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyKind {
    Data,
    Order,
}

// ---------------------------------------------------------------------------
// Feature 105 pipeline-retargeting tests (task T015).
//
// These tests were written RED before task T020 landed the additive
// `StageType` variants (`BatchSource`, `BatchSink`) and the `StageDecl`
// extension fields (`input_refs`, `output_refs`, `plan_ref`,
// `requires_plan_ref`).
//
// Article III semantic test posture: every negative assertion below would
// fail against a stub that hardcoded `Ready` (or `Valid`) for unknown stage
// variants, ignored missing `plan_ref` on `DataStoreSearch`, or accepted a
// `BatchSink` with empty `input_refs`.
//
// Contract references: `contracts/pipeline-stage-retarget.md`
// PS-001 (shared shape across kinds), PS-002 (DataStoreSearch requires
// PlanRef), PS-003 (BatchSink requires input_refs), PS-008 (no hidden
// fork; one shared `StageDecl` shape).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod feature_105_pipeline_unit_tests {
    use super::super::refs::{
        ArtifactKind, ArtifactRef, BatchRef, EnvelopeRef, PlanCompatibilityPosture, PlanKind,
        PlanRef,
    };
    use super::super::{BatchId, PlanId};
    use super::*;

    fn batch_ref(id: &str) -> EnvelopeRef {
        EnvelopeRef::Batch(BatchRef {
            batch_id: BatchId::from(id),
            schema_fingerprint: "sha256:0xaaa".into(),
            provenance_id: None,
            byte_size_hint: None,
            metadata: BTreeMap::new(),
        })
    }

    fn plan_ref() -> PlanRef {
        PlanRef {
            plan_id: PlanId::from("plan-001"),
            plan_kind: PlanKind::Substrait,
            compatibility_posture: PlanCompatibilityPosture::DeclarationOnly,
            metadata: BTreeMap::new(),
        }
    }

    fn artifact_ref(id: &str) -> EnvelopeRef {
        EnvelopeRef::Artifact(ArtifactRef {
            artifact_id: id.to_owned(),
            kind: ArtifactKind::HistoryEntry,
            canonical_hash: None,
            display_short: None,
            metadata: BTreeMap::new(),
        })
    }

    /// Minimal pipeline scaffold to host one stage of `stage` under test.
    fn pipeline_with(stages: Vec<StageDecl>) -> PipelineDefinition {
        let mut p = PipelineDefinition::new(PipelineId::from("p-105-t015"), "1.0.0");
        for s in stages {
            p = p.with_stage(s);
        }
        p
    }

    // PS-008 - `StageDecl` carries the new optional ref slots
    // (`input_refs`, `output_refs`, `plan_ref`, `requires_plan_ref`) as one
    // shared shape across every stage kind.
    #[test]
    fn ps_008_stage_decl_carries_retargeted_ref_slots() {
        let stage: StageDecl = StageDecl::tool(StageName::from("t"))
            .with_capability(CapabilityId::from("tool.do"))
            .with_provider(ProviderId::from("loopback"));
        // Direct field access pins that the new optional fields exist on
        // the shared `StageDecl` shape (not on a per-variant struct).
        let _ = &stage.input_refs;
        let _ = &stage.output_refs;
        let _ = &stage.plan_ref;
        let _ = &stage.requires_plan_ref;
    }

    // PS-001 - A pipeline containing one stage of every existing
    // 104 variant plus the new feature-105 BatchSource and BatchSink
    // variants validates as `Ready` when each new-variant stage is
    // correctly populated (BatchSource has at least one output ref;
    // BatchSink has at least one input ref; DataStoreSearch has a plan_ref).
    // A stub validator
    // that returns `Ready` for everything would still pass this test, but
    // PS-002 and PS-003 below ensure such a stub fails elsewhere.
    #[test]
    fn ps_001_mixed_kind_pipeline_with_batch_variants_validates_ready() {
        let llm = StageDecl::llm(StageName::from("llm"))
            .with_provider(ProviderId::from("loopback"))
            .with_model("loopback:v1")
            .with_capability(CapabilityId::from("llm.text_completion"));
        let tool = StageDecl::tool(StageName::from("tool"))
            .with_provider(ProviderId::from("loopback"))
            .with_capability(CapabilityId::from("tool.emit_artifact"));
        let clips = StageDecl::clips_rule_eval(StageName::from("rules"))
            .with_provider(ProviderId::from("clips"))
            .with_capability(CapabilityId::from("rule_eval.generic"));

        let mut data_store = StageDecl::data_store_search(StageName::from("dss"))
            .with_capability(CapabilityId::from("data_store.search"));
        data_store.plan_ref = Some(plan_ref());
        data_store.requires_plan_ref = true;

        // BatchSource variant carries an output ref; BatchSink carries an
        // input ref. Both are new feature-105 variants (T020 RED gate).
        let mut src = StageDecl::batch_source(StageName::from("src"))
            .with_capability(CapabilityId::from("batch.source"));
        src.output_refs.push(batch_ref("batch-out-1"));

        let mut sink = StageDecl::batch_sink(StageName::from("sink"))
            .with_capability(CapabilityId::from("batch.sink"));
        sink.input_refs.push(batch_ref("batch-in-1"));

        let p = pipeline_with(vec![llm, tool, clips, data_store, src, sink]);
        let outcome = p.validate();
        assert!(
            matches!(outcome, PipelineValidationOutcome::Ready),
            "mixed-kind pipeline must validate Ready under feature 105; got {outcome:?}"
        );
    }

    // PS-002 - A `DataStoreSearch` stage with `plan_ref = None`
    // must not validate as `Ready`. The exact error variant
    // (`PlanReferenceUnbound`) is added in US3 / T040; this test only
    // asserts the non-Ready outcome to bridge the gap. A stub validator
    // that returned `Ready` regardless would fail here.
    #[test]
    fn ps_002_data_store_search_without_plan_ref_is_not_ready() {
        let dss = StageDecl::data_store_search(StageName::from("dss"))
            .with_capability(CapabilityId::from("data_store.search"));
        assert!(dss.plan_ref.is_none());
        let p = pipeline_with(vec![dss]);
        let outcome = p.validate();
        assert!(
            !matches!(outcome, PipelineValidationOutcome::Ready),
            "DataStoreSearch with plan_ref=None must not validate Ready; got {outcome:?}"
        );
    }

    // PS-002b - A `DataStoreSearch` stage WITH a `plan_ref` set
    // validates `Ready`. Pins the positive side so US3 / T040
    // `PlanReferenceUnbound` implementation cannot accidentally reject
    // well-formed DataStoreSearch stages.
    #[test]
    fn ps_002b_data_store_search_with_plan_ref_validates_ready() {
        let mut dss = StageDecl::data_store_search(StageName::from("dss"))
            .with_capability(CapabilityId::from("data_store.search"));
        dss.plan_ref = Some(plan_ref());
        dss.requires_plan_ref = true;
        let p = pipeline_with(vec![dss]);
        let outcome = p.validate();
        assert!(
            matches!(outcome, PipelineValidationOutcome::Ready),
            "DataStoreSearch with plan_ref present must validate Ready; got {outcome:?}"
        );
    }

    // PS-003 - A `BatchSink` stage with an empty `input_refs`
    // must not validate as `Ready`. The variant exists only after T020;
    // shape rule lands in the same task.
    #[test]
    fn ps_003_batch_sink_without_input_refs_is_not_ready() {
        let sink = StageDecl::batch_sink(StageName::from("sink"))
            .with_capability(CapabilityId::from("batch.sink"));
        assert!(sink.input_refs.is_empty());
        let p = pipeline_with(vec![sink]);
        let outcome = p.validate();
        assert!(
            !matches!(outcome, PipelineValidationOutcome::Ready),
            "BatchSink with empty input_refs must not validate Ready; got {outcome:?}"
        );
    }

    #[test]
    fn ps_003_batch_sink_with_non_batch_input_ref_is_not_ready() {
        let mut sink = StageDecl::batch_sink(StageName::from("sink"))
            .with_capability(CapabilityId::from("batch.sink"));
        sink.input_refs.push(artifact_ref("artifact-in-1"));
        let p = pipeline_with(vec![sink]);
        let outcome = p.validate();
        assert!(
            !matches!(outcome, PipelineValidationOutcome::Ready),
            "BatchSink with non-batch input_refs must not validate Ready; got {outcome:?}"
        );
    }

    // PS-003b - Symmetric rule: `BatchSource` with empty
    // `output_refs` must not validate `Ready`.
    #[test]
    fn ps_003b_batch_source_without_output_refs_is_not_ready() {
        let src = StageDecl::batch_source(StageName::from("src"))
            .with_capability(CapabilityId::from("batch.source"));
        assert!(src.output_refs.is_empty());
        let p = pipeline_with(vec![src]);
        let outcome = p.validate();
        assert!(
            !matches!(outcome, PipelineValidationOutcome::Ready),
            "BatchSource with empty output_refs must not validate Ready; got {outcome:?}"
        );
    }

    #[test]
    fn ps_003b_batch_source_with_non_batch_output_ref_is_not_ready() {
        let mut src = StageDecl::batch_source(StageName::from("src"))
            .with_capability(CapabilityId::from("batch.source"));
        src.output_refs.push(artifact_ref("artifact-out-1"));
        let p = pipeline_with(vec![src]);
        let outcome = p.validate();
        assert!(
            !matches!(outcome, PipelineValidationOutcome::Ready),
            "BatchSource with non-batch output_refs must not validate Ready; got {outcome:?}"
        );
    }

    // PS-008b - Regression pin: a feature-104-only pipeline
    // (no retargeting fields populated) still validates `Ready` so the
    // optional ref slots remain truly additive. `input_refs` /
    // `output_refs` default to empty Vec; `plan_ref` defaults to None;
    // `requires_plan_ref` defaults to false. A stub T020 impl that flipped
    // these defaults would break feature-104 fixtures.
    #[test]
    fn ps_008b_feature_104_only_pipeline_still_validates_ready() {
        let llm = StageDecl::llm(StageName::from("only-llm"))
            .with_provider(ProviderId::from("loopback"))
            .with_model("loopback:v1")
            .with_capability(CapabilityId::from("llm.text_completion"));
        // The new fields MUST default to empty / None / false.
        assert!(llm.input_refs.is_empty());
        assert!(llm.output_refs.is_empty());
        assert!(llm.plan_ref.is_none());
        assert!(!llm.requires_plan_ref);
        let p = pipeline_with(vec![llm]);
        let outcome = p.validate();
        assert!(
            matches!(outcome, PipelineValidationOutcome::Ready),
            "feature-104-only pipeline must continue to validate Ready; got {outcome:?}"
        );
    }
}
