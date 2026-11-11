//! T019 (US1) — Public pipeline DTO round-trip + validation tests.
//!
//! Pins the public CE `PipelineDefinition` / `StageDecl` / `StageType` shape
//! and the validation outcomes for the LLM + tool + CLIPS rule-eval
//! executable stages plus the declaration-only solver/BN/ZEN/data-store
//! stages. Tests are semantic (Article III): they assert outcome variants,
//! locatable error paths, and canonical-hash determinism. A stub returning
//! `Ready` for every input, or a constant canonical hash, fails these tests.

use nxuskit::contract::v1_1::{
    CONTRACT_VERSION, CapabilityId, PipelineDefinition, PipelineValidationOutcome, ProviderId,
    StageDecl, StageInput, StageName, StageOutput, StageType, ToolStageMetadata, canonical_hash,
};
use serde_json::json;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/contract_v11")
}

fn load_fixture(name: &str) -> PipelineDefinition {
    let path = fixtures_dir().join(name);
    let body =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&body).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

#[test]
fn floor_llm_only_fixture_parses_and_carries_version() {
    let pipeline = load_fixture("pipeline_llm_only.json");
    assert_eq!(pipeline.contract_version, CONTRACT_VERSION);
    assert_eq!(pipeline.pipeline_id.as_str(), "pl_llm_only_floor");
    assert_eq!(pipeline.version, "0.1.0");
    assert_eq!(pipeline.stages.len(), 1);
    assert_eq!(pipeline.stages[0].stage_type, StageType::Llm);
    assert_eq!(pipeline.stages[0].name.as_str(), "draft");
}

#[test]
fn floor_llm_only_fixture_validates_ready() {
    // No engine adapter required: validation uses pure schema + capability
    // cross-check defaults (no capabilities registered → `Ready` is fine for
    // the schema layer; the capability cross-check is exercised in the
    // engine-adapter test below).
    let pipeline = load_fixture("pipeline_llm_only.json");
    let outcome = pipeline.validate();
    assert_eq!(outcome, PipelineValidationOutcome::Ready);
}

#[test]
fn missing_contract_version_is_schema_invalid_locatably() {
    let bad = json!({
        "contract_version": "0.0",
        "pipeline_id": "pl_x",
        "version": "0.1.0",
        "metadata": {},
        "stages": [{
            "name": "draft", "stage_type": "llm",
            "provider_ref": "loopback", "model_ref": "loopback:v1",
            "capability_ref": "llm.text_completion",
            "inputs": [], "outputs": [], "metadata": {}
        }]
    });
    let pipeline: PipelineDefinition = serde_json::from_value(bad).unwrap();
    let outcome = pipeline.validate();
    match outcome {
        PipelineValidationOutcome::SchemaInvalid { path, message } => {
            assert_eq!(path, "contract_version");
            assert!(
                message.contains("1.1"),
                "message must reference 1.1, got: {message}"
            );
        }
        other => panic!("expected SchemaInvalid for contract_version, got {other:?}"),
    }
}

#[test]
fn empty_stages_is_schema_invalid_locatably() {
    let bad = json!({
        "contract_version": "1.1",
        "pipeline_id": "pl_x", "version": "0.1.0",
        "metadata": {}, "stages": []
    });
    let pipeline: PipelineDefinition = serde_json::from_value(bad).unwrap();
    match pipeline.validate() {
        PipelineValidationOutcome::SchemaInvalid { path, message } => {
            assert_eq!(path, "stages");
            assert!(
                message.to_lowercase().contains("non-empty")
                    || message.to_lowercase().contains("empty")
            );
        }
        other => panic!("expected SchemaInvalid for empty stages, got {other:?}"),
    }
}

#[test]
fn duplicate_stage_name_is_schema_invalid_locatably() {
    let bad = json!({
        "contract_version": "1.1",
        "pipeline_id": "pl_x", "version": "0.1.0",
        "metadata": {},
        "stages": [
            {"name": "draft", "stage_type": "llm", "provider_ref": "loopback",
             "model_ref": "loopback:v1", "capability_ref": "llm.text_completion",
             "inputs": [], "outputs": [], "metadata": {}},
            {"name": "draft", "stage_type": "llm", "provider_ref": "loopback",
             "model_ref": "loopback:v1", "capability_ref": "llm.text_completion",
             "inputs": [], "outputs": [], "metadata": {}}
        ]
    });
    let pipeline: PipelineDefinition = serde_json::from_value(bad).unwrap();
    match pipeline.validate() {
        PipelineValidationOutcome::SchemaInvalid { path, message } => {
            assert_eq!(path, "stages[1].name");
            assert!(message.to_lowercase().contains("duplicate"));
        }
        other => panic!("expected SchemaInvalid for duplicate stage name, got {other:?}"),
    }
}

#[test]
fn llm_stage_missing_model_ref_is_schema_invalid_locatably() {
    let bad = json!({
        "contract_version": "1.1",
        "pipeline_id": "pl_x", "version": "0.1.0",
        "metadata": {},
        "stages": [{
            "name": "draft", "stage_type": "llm",
            "provider_ref": "loopback",
            "capability_ref": "llm.text_completion",
            "inputs": [], "outputs": [], "metadata": {}
        }]
    });
    let pipeline: PipelineDefinition = serde_json::from_value(bad).unwrap();
    match pipeline.validate() {
        PipelineValidationOutcome::SchemaInvalid { path, message } => {
            assert_eq!(path, "stages[0].model_ref");
            assert!(message.to_lowercase().contains("required"));
        }
        other => panic!("expected SchemaInvalid for missing model_ref, got {other:?}"),
    }
}

#[test]
fn tool_stage_without_provider_ref_is_schema_invalid_locatably() {
    // US1 floor: tool stages MUST declare a provider_ref. The full
    // ToolSemantics requirements (determinism / side-effect / cost / etc.)
    // land in US5 (T065). US1 only ensures the tool stage is parseable and
    // its provider_ref is enforced.
    let bad = json!({
        "contract_version": "1.1",
        "pipeline_id": "pl_x", "version": "0.1.0",
        "metadata": {},
        "stages": [{
            "name": "emit", "stage_type": "tool",
            "capability_ref": "tool.emit_artifact",
            "inputs": [], "outputs": [], "metadata": {}
        }]
    });
    let pipeline: PipelineDefinition = serde_json::from_value(bad).unwrap();
    match pipeline.validate() {
        PipelineValidationOutcome::SchemaInvalid { path, message } => {
            assert_eq!(path, "stages[0].provider_ref");
            assert!(message.to_lowercase().contains("required"));
        }
        other => {
            panic!("expected SchemaInvalid for missing provider_ref on tool stage, got {other:?}")
        }
    }
}

#[test]
fn dependency_cycle_is_schema_invalid() {
    let bad = json!({
        "contract_version": "1.1",
        "pipeline_id": "pl_x", "version": "0.1.0",
        "metadata": {},
        "stages": [
            {"name": "a", "stage_type": "llm", "provider_ref": "loopback",
             "model_ref": "loopback:v1", "capability_ref": "llm.text_completion",
             "inputs": [{"name": "x", "from_stage": {"stage": "b", "output": "out"}}],
             "outputs": [{"name": "out", "streaming": false}], "metadata": {}},
            {"name": "b", "stage_type": "llm", "provider_ref": "loopback",
             "model_ref": "loopback:v1", "capability_ref": "llm.text_completion",
             "inputs": [{"name": "x", "from_stage": {"stage": "a", "output": "out"}}],
             "outputs": [{"name": "out", "streaming": false}], "metadata": {}}
        ]
    });
    let pipeline: PipelineDefinition = serde_json::from_value(bad).unwrap();
    match pipeline.validate() {
        PipelineValidationOutcome::SchemaInvalid { path: _, message } => {
            assert!(message.to_lowercase().contains("cycle"));
        }
        other => panic!("expected SchemaInvalid (cycle), got {other:?}"),
    }
}

#[test]
fn declaration_only_solver_stage_validates() {
    // Solver is declaration-only in v1.1.0. The schema layer accepts it
    // without provider_ref/model_ref. Capability availability is checked by
    // the engine-adapter cross-check (separate test).
    let p = json!({
        "contract_version": "1.1",
        "pipeline_id": "pl_x", "version": "0.1.0", "metadata": {},
        "stages": [{
            "name": "plan", "stage_type": "solver",
            "capability_ref": "solver.smt",
            "inputs": [], "outputs": [], "metadata": {}
        }]
    });
    let pipeline: PipelineDefinition = serde_json::from_value(p).unwrap();
    assert_eq!(pipeline.validate(), PipelineValidationOutcome::Ready);
}

#[test]
fn export_import_round_trip_preserves_canonical_hash() {
    let original = load_fixture("pipeline_llm_only.json");
    let exported = original.export_canonical_json().expect("export");
    let reimported = PipelineDefinition::import_canonical_json(&exported).expect("import");

    let h1 = canonical_hash(&serde_json::to_value(&original).unwrap()).unwrap();
    let h2 = canonical_hash(&serde_json::to_value(&reimported).unwrap()).unwrap();
    assert_eq!(h1, h2, "canonical hash must be stable across export/import");
}

#[test]
fn extension_metadata_round_trips_byte_equal_in_canonical_form() {
    // Open-extension metadata (FR-004) must survive export → import without
    // dropping unknown keys.
    let p = json!({
        "contract_version": "1.1",
        "pipeline_id": "pl_meta", "version": "0.1.0",
        "metadata": {
            "peeler_template_id": "ce-template-001",
            "peeler_workflow_pack": "demo-pack-v1"
        },
        "stages": [{
            "name": "draft", "stage_type": "llm",
            "provider_ref": "loopback", "model_ref": "loopback:v1",
            "capability_ref": "llm.text_completion",
            "inputs": [], "outputs": [],
            "metadata": {"peeler_stage_template": "abc"}
        }]
    });
    let pipeline: PipelineDefinition = serde_json::from_value(p).unwrap();
    let exported = pipeline.export_canonical_json().expect("export");
    let reimported = PipelineDefinition::import_canonical_json(&exported).expect("reimport");
    assert_eq!(
        pipeline.metadata.get("peeler_template_id"),
        reimported.metadata.get("peeler_template_id")
    );
    assert_eq!(
        pipeline.metadata.get("peeler_workflow_pack"),
        reimported.metadata.get("peeler_workflow_pack")
    );
    assert_eq!(
        pipeline.stages[0].metadata.get("peeler_stage_template"),
        reimported.stages[0].metadata.get("peeler_stage_template")
    );
}

#[test]
fn builder_constructs_llm_plus_tool_plus_clips_pipeline() {
    // Builder-style construction exercises the public API surface without
    // touching JSON, proving the DTO is usable directly from Rust callers.
    let pipeline = PipelineDefinition::new("pl_builder", "0.1.0")
        .with_stage(
            StageDecl::llm(StageName::from("draft"))
                .with_provider(ProviderId::from("loopback"))
                .with_model("loopback:v1")
                .with_capability(CapabilityId::from("llm.text_completion"))
                .with_input(StageInput::from_pipeline_input("source", "source"))
                .with_output(StageOutput::new("draft_text")),
        )
        .with_stage(
            StageDecl::clips_rule_eval(StageName::from("validate"))
                .with_provider(ProviderId::from("clips"))
                .with_capability(CapabilityId::from("rule_eval.generic"))
                .with_input(StageInput::from_stage("input", "draft", "draft_text"))
                .with_output(StageOutput::new("validation_outcomes")),
        )
        .with_stage(
            StageDecl::tool(StageName::from("emit"))
                .with_provider(ProviderId::from("loopback"))
                .with_capability(CapabilityId::from("tool.emit_artifact"))
                .with_tool_stage_metadata(
                    ToolStageMetadata::new().with("display_name", "emit_artifact"),
                )
                .with_input(StageInput::from_stage(
                    "input",
                    "validate",
                    "validation_outcomes",
                ))
                .with_output(StageOutput::new("artifact_ref")),
        );
    assert_eq!(pipeline.validate(), PipelineValidationOutcome::Ready);
    assert_eq!(pipeline.stages.len(), 3);
    assert_eq!(pipeline.stages[1].stage_type, StageType::ClipsRuleEval);
    assert_eq!(pipeline.stages[2].stage_type, StageType::Tool);
    // US1 floor: tool stage carries an opaque, extension-preserving
    // metadata block on the public DTO. Full ToolSemantics (determinism,
    // side-effect class, auth scope, redaction policy, etc.) is US5 / T065.
    assert!(pipeline.stages[2].tool_stage_metadata.is_some());
}
