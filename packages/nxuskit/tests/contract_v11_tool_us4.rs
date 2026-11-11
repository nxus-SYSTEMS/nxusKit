//! US4 (T045-T049) — `ToolSemantics` data-posture binding tests.
//!
//! Article III semantic posture: each assertion would fail against a stub
//! that returned `Ready` regardless of `data_posture` / `requires_plan_ref`
//! / `input_refs` shape, or that ignored the parent stage's reference slots.

use std::collections::BTreeMap;

use nxuskit::contract::v1_1::{
    BatchId, BatchRef, CapabilityId, EnvelopeRef, PipelineDefinition, PipelineValidationOutcome,
    PlanCompatibilityPosture, PlanId, PlanKind, PlanRef, ProviderId, StageDecl, StageName,
    ToolDataPosture, ToolSemantics, ToolStageMetadata,
};
use serde_json::json;

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
        plan_id: PlanId::from("plan-us4"),
        plan_kind: PlanKind::Substrait,
        compatibility_posture: PlanCompatibilityPosture::DeclarationOnly,
        metadata: BTreeMap::new(),
    }
}

fn tool_meta_from_semantics(s: &ToolSemantics) -> ToolStageMetadata {
    let v = serde_json::to_value(s).expect("ToolSemantics is serde-clean");
    let mut m = ToolStageMetadata::new();
    if let serde_json::Value::Object(obj) = v {
        for (k, val) in obj {
            m = m.with(k, val);
        }
    }
    m
}

// TS-001 / T045 — A tool stage that declares `data_posture =
// ConsumesReferencedData` MUST NOT validate Ready when `input_refs` is
// empty.
#[test]
fn t045_consumes_referenced_data_requires_input_refs() {
    let semantics = ToolSemantics::new(ToolDataPosture::ConsumesReferencedData);
    let tool = StageDecl::tool(StageName::from("eats"))
        .with_provider(ProviderId::from("loopback"))
        .with_capability(CapabilityId::from("tool.eat_refs"))
        .with_tool_stage_metadata(tool_meta_from_semantics(&semantics));
    // input_refs intentionally empty.
    let pipeline = PipelineDefinition::new("pl_us4_t045", "1.1.0").with_stage(tool);
    let outcome = pipeline.validate();
    match outcome {
        PipelineValidationOutcome::SchemaInvalid { path, message } => {
            assert!(
                path.contains("data_posture"),
                "expected data_posture-rooted path; got {path}"
            );
            assert!(
                message.contains("input_refs"),
                "expected input_refs-rooted message; got {message}"
            );
        }
        other => panic!("ConsumesReferencedData + empty input_refs must fail; got {other:?}"),
    }
}

#[test]
fn t045b_consumes_referenced_data_with_input_refs_validates_ready() {
    let semantics = ToolSemantics::new(ToolDataPosture::ConsumesReferencedData);
    let mut tool = StageDecl::tool(StageName::from("eats"))
        .with_provider(ProviderId::from("loopback"))
        .with_capability(CapabilityId::from("tool.eat_refs"))
        .with_tool_stage_metadata(tool_meta_from_semantics(&semantics));
    tool.input_refs.push(batch_ref("batch-in-1"));
    let pipeline = PipelineDefinition::new("pl_us4_t045b", "1.1.0").with_stage(tool);
    let outcome = pipeline.validate();
    assert!(
        matches!(outcome, PipelineValidationOutcome::Ready),
        "ConsumesReferencedData + non-empty input_refs must validate Ready; got {outcome:?}"
    );
}

// TS-002 / T046 — A tool stage that declares `requires_plan_ref = true`
// MUST NOT validate Ready when `plan_ref` is None.
#[test]
fn t046_requires_plan_ref_true_requires_plan_ref_on_stage() {
    let mut semantics = ToolSemantics::new(ToolDataPosture::RequiresPlanRef);
    semantics.requires_plan_ref = true;
    let tool = StageDecl::tool(StageName::from("planned"))
        .with_provider(ProviderId::from("loopback"))
        .with_capability(CapabilityId::from("tool.needs_plan"))
        .with_tool_stage_metadata(tool_meta_from_semantics(&semantics));
    let pipeline = PipelineDefinition::new("pl_us4_t046", "1.1.0").with_stage(tool);
    let outcome = pipeline.validate();
    match outcome {
        PipelineValidationOutcome::PlanReferenceUnbound { stage_name } => {
            assert_eq!(stage_name.as_str(), "planned");
        }
        other => panic!(
            "ToolSemantics.requires_plan_ref=true with no plan_ref must yield \
             PlanReferenceUnbound; got {other:?}"
        ),
    }
}

#[test]
fn t046b_requires_plan_ref_true_with_plan_ref_validates_ready() {
    let mut semantics = ToolSemantics::new(ToolDataPosture::RequiresPlanRef);
    semantics.requires_plan_ref = true;
    let mut tool = StageDecl::tool(StageName::from("planned"))
        .with_provider(ProviderId::from("loopback"))
        .with_capability(CapabilityId::from("tool.needs_plan"))
        .with_tool_stage_metadata(tool_meta_from_semantics(&semantics));
    tool.plan_ref = Some(plan_ref());
    let pipeline = PipelineDefinition::new("pl_us4_t046b", "1.1.0").with_stage(tool);
    let outcome = pipeline.validate();
    assert!(
        matches!(outcome, PipelineValidationOutcome::Ready),
        "requires_plan_ref=true with plan_ref present must validate Ready; got {outcome:?}"
    );
}

// TS-003 / T047 — JSON deserialization of `ToolSemantics` that includes a
// banned field name is rejected by serde with a class-identifying error.
// Covered in the in-source unit tests under `tool.rs`; this integration
// test pins that the public re-export is reachable from a consumer and the
// same deny_unknown_fields semantic applies.
#[test]
fn t047_deny_unknown_fields_on_public_tool_semantics() {
    let raw = json!({
        "data_posture": "inline_metadata_only",
        "provider_channel_route": "router-1"
    });
    let err = serde_json::from_value::<ToolSemantics>(raw)
        .expect_err("ToolSemantics must reject provider_channel_route at the public surface");
    assert!(err.to_string().contains("provider_channel_route"));
}

#[test]
fn t047b_tool_stage_metadata_parse_errors_are_validation_errors() {
    let mut meta =
        tool_meta_from_semantics(&ToolSemantics::new(ToolDataPosture::InlineMetadataOnly));
    meta = meta.with("provider_channel_route", json!("router-1"));

    let tool = StageDecl::tool(StageName::from("public-safe-tool"))
        .with_provider(ProviderId::from("loopback"))
        .with_capability(CapabilityId::from("tool.public_safe"))
        .with_tool_stage_metadata(meta);
    let pipeline = PipelineDefinition::new("pl_us4_t047b", "1.1.0").with_stage(tool);

    match pipeline.validate() {
        PipelineValidationOutcome::SchemaInvalid { path, message } => {
            assert_eq!(path, "stages[0].tool_stage_metadata");
            assert!(
                message.contains("provider_channel_route"),
                "serde error must name the offending field; got: {message}"
            );
        }
        other => panic!("invalid ToolSemantics-shaped metadata must fail; got {other:?}"),
    }
}

// T050-component / positive fixture — Confirm the
// `positive_pipeline_llm_plus_referenced_data_tool.json` fixture carries
// the full retargeted `ToolSemantics` field set and validates Ready.
#[test]
fn t050_positive_fixture_has_full_tool_semantics_field_set_and_validates_ready() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .unwrap()
        .join("specs")
        .join("105-v110-arrow-zenoh-substrait-retarget")
        .join("contracts")
        .join("fixtures")
        .join("positive_pipeline_llm_plus_referenced_data_tool.json");
    let raw = std::fs::read_to_string(&path).expect("read T050 fixture");
    let pipeline = nxuskit::contract::v1_1::load_pipeline_definition_json(&raw)
        .expect("load T050 fixture through public loader");
    let outcome = pipeline.validate();
    assert!(
        matches!(outcome, PipelineValidationOutcome::Ready),
        "T050 fixture must validate Ready; got {outcome:?}"
    );

    // Pin: the tool stage carries data_posture + the full posture field
    // set so that downstream consumers know the fixture covers the
    // retargeted shape.
    let tool_stage = pipeline
        .stages
        .iter()
        .find(|s| s.name.as_str() == "emit")
        .expect("emit tool stage present");
    let meta = tool_stage
        .tool_stage_metadata
        .as_ref()
        .expect("tool_stage_metadata present");
    let raw_meta = meta.as_inner();
    for required in [
        "data_posture",
        "requires_plan_ref",
        "determinism",
        "side_effect",
        "cost_budget_posture",
        "cancellability",
        "auth_posture",
        "tenant_scope",
        "redaction_posture",
        "audit_posture",
    ] {
        assert!(
            raw_meta.contains_key(required),
            "T050 fixture tool stage missing {required} field"
        );
    }
}
