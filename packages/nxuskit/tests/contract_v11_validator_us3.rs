//! US3 (T035-T039) — validator tests for `PlanReferenceUnbound`,
//! `AmbiguousSlot`, retargeted `CapabilityUnavailable`, shared-stage shape,
//! and the feature-104 regression pin.
//!
//! Article III semantic posture: every assertion below would fail against a
//! stub that returned `Ready` or generic `SchemaInvalid` for the inputs.
//! Validator rule order locked: structural shape → boundary
//! (bulk-payload, Zenoh-leak) → ambiguity → reference binding → capability
//! (see `research.md` §R-006 and `plan.md` Sprint Slice US1+US2).

use std::collections::BTreeMap;
use std::path::PathBuf;

use nxuskit::contract::v1_1::{
    BatchId, BatchRef, CapabilityId, CapabilityTruthEntry, EnvelopeRef, PipelineDefinition,
    PipelineValidationOutcome, PlanCompatibilityPosture, PlanId, PlanKind, PlanRef, ProviderId,
    ReadinessSnapshot, StageDecl, StageName, StageOutput, ToolStageMetadata,
    validate_with_readiness,
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
        plan_id: PlanId::from("plan-001"),
        plan_kind: PlanKind::Substrait,
        compatibility_posture: PlanCompatibilityPosture::DeclarationOnly,
        metadata: BTreeMap::new(),
    }
}

// PS-002 / T035 — DataStoreSearch with plan_ref == None must fail with
// the typed `PlanReferenceUnbound { stage_name }` outcome (not the
// generic `SchemaInvalid`).
#[test]
fn t035_plan_reference_unbound_for_data_store_search_without_plan_ref() {
    let dss = StageDecl::data_store_search(StageName::from("dss"))
        .with_capability(CapabilityId::from("data_store.search"));
    let pipeline = PipelineDefinition::new("pl_us3_t035", "1.1.0").with_stage(dss);
    let outcome = pipeline.validate();
    match outcome {
        PipelineValidationOutcome::PlanReferenceUnbound { stage_name } => {
            assert_eq!(stage_name.as_str(), "dss");
        }
        other => panic!(
            "DataStoreSearch with plan_ref=None must produce PlanReferenceUnbound; got {other:?}"
        ),
    }
}

// PS-004 / T036 — A tool stage that declares both an embedded literal slot
// (`tool_input: { "rows": [...] }`) AND `input_refs: [BatchRef]` fails with
// `AmbiguousSlot { path }`.
#[test]
fn t036_ambiguous_slot_when_tool_input_object_coexists_with_input_refs() {
    let mut tool = StageDecl::tool(StageName::from("toolish"))
        .with_provider(ProviderId::from("loopback"))
        .with_capability(CapabilityId::from("tool.emit_artifact"));
    tool.input_refs.push(batch_ref("batch-in-1"));
    // PS-004 names `{ "rows": [...] }` as the canonical embedded-literal
    // example. The bulk-payload scan independently flags `rows` as
    // `EmbeddedRecordBatch`, so to exercise the ambiguity rule cleanly we
    // use an inline literal whose inner keys are not themselves in the
    // bulk-payload key set; the ambiguity rule fires on the slot key
    // (`tool_input`) being object-valued in the presence of refs.
    tool.tool_stage_metadata =
        Some(ToolStageMetadata::new().with("tool_input", json!({"items": [{"id": 1}, {"id": 2}]})));
    let pipeline = PipelineDefinition::new("pl_us3_t036", "1.1.0").with_stage(tool);
    let outcome = pipeline.validate();
    match outcome {
        PipelineValidationOutcome::AmbiguousSlot { path } => {
            assert!(
                path.contains("tool_input"),
                "AmbiguousSlot path must reference the offending key: {path}"
            );
            assert!(
                path.contains("stages[0]"),
                "AmbiguousSlot path must be locatable: {path}"
            );
        }
        other => panic!("tool stage with literal+refs must produce AmbiguousSlot; got {other:?}"),
    }
}

// T036b — An array-valued embedded literal triggers AmbiguousSlot too.
#[test]
fn t036b_ambiguous_slot_when_query_array_coexists_with_input_refs() {
    let mut tool = StageDecl::tool(StageName::from("toolish"))
        .with_provider(ProviderId::from("loopback"))
        .with_capability(CapabilityId::from("tool.emit_artifact"));
    tool.input_refs.push(batch_ref("batch-in-1"));
    tool.tool_stage_metadata =
        Some(ToolStageMetadata::new().with("query", json!(["SELECT 1", "SELECT 2"])));
    let pipeline = PipelineDefinition::new("pl_us3_t036b", "1.1.0").with_stage(tool);
    let outcome = pipeline.validate();
    assert!(
        matches!(outcome, PipelineValidationOutcome::AmbiguousSlot { .. }),
        "tool stage with `query`-array + input_refs must produce AmbiguousSlot; got {outcome:?}"
    );
}

// T036c — A plain string value for `tool_input` (e.g. a model parameter)
// alongside refs is NOT ambiguous. Only array/object embedded literals
// constitute a bulk-payload-shaped slot.
#[test]
fn t036c_string_tool_input_with_refs_is_not_ambiguous() {
    let mut tool = StageDecl::tool(StageName::from("ok-tool"))
        .with_provider(ProviderId::from("loopback"))
        .with_capability(CapabilityId::from("tool.emit_artifact"));
    tool.input_refs.push(batch_ref("batch-in-1"));
    tool.tool_stage_metadata = Some(ToolStageMetadata::new().with("tool_input", json!("hint")));
    let pipeline = PipelineDefinition::new("pl_us3_t036c", "1.1.0").with_stage(tool);
    let outcome = pipeline.validate();
    assert!(
        matches!(outcome, PipelineValidationOutcome::Ready),
        "string-valued `tool_input` alongside refs must validate Ready; got {outcome:?}"
    );
}

// T037 — `CapabilityUnavailable` fires when a stage declares a capability
// that is not present in the active readiness snapshot. Routed through
// `validate_with_readiness`, which is the public composition surface that
// pairs `PipelineDefinition::validate` with readiness data.
#[test]
fn t037_capability_unavailable_when_readiness_does_not_cover_capability() {
    use nxuskit::contract::v1_1::{
        CapabilityDomain, CapabilityPosture, ProviderReadiness, ReadinessPosture, TierPosture,
    };

    let stage = StageDecl::tool(StageName::from("needs-substrait"))
        .with_provider(ProviderId::from("loopback"))
        .with_capability(CapabilityId::from("substrait.plan_eval"))
        .with_output(StageOutput::new("out"));
    let pipeline = PipelineDefinition::new("pl_us3_t037", "1.1.0").with_stage(stage);

    // The readiness snapshot ONLY advertises `llm.text_completion`. The
    // declared capability `substrait.plan_eval` is absent.
    let snapshot = ReadinessSnapshot::from_pure(
        "2026-05-27T00:00:00Z",
        vec![ProviderReadiness {
            provider_id: ProviderId::from("loopback"),
            posture: ReadinessPosture::Ready,
            tier_posture: TierPosture::Ce,
            local_provider: true,
            models: vec![],
            timeouts_ms: Default::default(),
            last_error: None,
            metadata: Default::default(),
        }],
        vec![CapabilityTruthEntry {
            capability_id: CapabilityId::from("llm.text_completion"),
            domain: CapabilityDomain::Llm,
            posture: CapabilityPosture::Native,
            tier: TierPosture::Ce,
            providers: vec![ProviderId::from("loopback")],
        }],
    );

    let outcome = validate_with_readiness(&pipeline, &snapshot);
    match outcome {
        PipelineValidationOutcome::CapabilityUnavailable {
            stage_name,
            capability_id,
        } => {
            assert_eq!(stage_name.as_str(), "needs-substrait");
            assert_eq!(capability_id.as_str(), "substrait.plan_eval");
        }
        other => panic!("unsatisfied capability must surface CapabilityUnavailable; got {other:?}"),
    }
}

// T037b — Positive cross-check: when the capability IS in readiness, the
// validator returns `Ready`, not `CapabilityUnavailable`.
#[test]
fn t037b_capability_unavailable_does_not_fire_when_capability_is_in_readiness() {
    use nxuskit::contract::v1_1::{
        CapabilityDomain, CapabilityPosture, ProviderReadiness, ReadinessPosture, TierPosture,
    };

    let stage = StageDecl::llm(StageName::from("llm"))
        .with_provider(ProviderId::from("loopback"))
        .with_model("loopback:v1")
        .with_capability(CapabilityId::from("llm.text_completion"))
        .with_output(StageOutput::new("text"));
    let pipeline = PipelineDefinition::new("pl_us3_t037b", "1.1.0").with_stage(stage);
    let snapshot = ReadinessSnapshot::from_pure(
        "2026-05-27T00:00:00Z",
        vec![ProviderReadiness {
            provider_id: ProviderId::from("loopback"),
            posture: ReadinessPosture::Ready,
            tier_posture: TierPosture::Ce,
            local_provider: true,
            models: vec![],
            timeouts_ms: Default::default(),
            last_error: None,
            metadata: Default::default(),
        }],
        vec![CapabilityTruthEntry {
            capability_id: CapabilityId::from("llm.text_completion"),
            domain: CapabilityDomain::Llm,
            posture: CapabilityPosture::Native,
            tier: TierPosture::Ce,
            providers: vec![ProviderId::from("loopback")],
        }],
    );
    let outcome = validate_with_readiness(&pipeline, &snapshot);
    assert!(
        matches!(outcome, PipelineValidationOutcome::Ready),
        "satisfied capability must validate Ready; got {outcome:?}"
    );
}

// T038 — Shared-shape integration: a pipeline with one stage of every
// existing 104 variant plus every new 105 variant validates as Ready
// when each new-variant stage is correctly populated.
//
// This duplicates the in-source `ps_001_mixed_kind_pipeline_with_batch_variants_validates_ready`
// unit test but at the public-surface integration level so the public
// re-export set is exercised end-to-end.
#[test]
fn t038_shared_shape_pipeline_with_every_stage_kind_validates_ready() {
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
    let solver = StageDecl::solver(StageName::from("solver"))
        .with_capability(CapabilityId::from("solver.declare_only"));
    let bn = StageDecl::bayesian(StageName::from("bn"))
        .with_capability(CapabilityId::from("bn.declare_only"));
    let zen = StageDecl::zen(StageName::from("zen"))
        .with_capability(CapabilityId::from("zen.declare_only"));
    let mut dss = StageDecl::data_store_search(StageName::from("dss"))
        .with_capability(CapabilityId::from("data_store.search"));
    dss.plan_ref = Some(plan_ref());
    dss.requires_plan_ref = true;
    let mut src = StageDecl::batch_source(StageName::from("src"))
        .with_capability(CapabilityId::from("batch.source"));
    src.output_refs.push(batch_ref("batch-out-1"));
    let mut sink = StageDecl::batch_sink(StageName::from("sink"))
        .with_capability(CapabilityId::from("batch.sink"));
    sink.input_refs.push(batch_ref("batch-in-1"));

    let mut p = PipelineDefinition::new("pl_us3_t038", "1.1.0");
    for s in [llm, tool, clips, solver, bn, zen, dss, src, sink] {
        p = p.with_stage(s);
    }
    let outcome = p.validate();
    assert!(
        matches!(outcome, PipelineValidationOutcome::Ready),
        "mixed-kind pipeline must validate Ready under feature 105; got {outcome:?}"
    );
}

// T039 — Feature 104 regression: a feature-104-shaped fixture (no new 105
// fields populated) loaded through the feature 105 surface still validates
// `Ready` and round-trips canonically.
#[test]
fn t039_feature_104_pipeline_fixture_still_validates_ready() {
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("contract_v11")
        .join("pipeline_llm_only.json");
    let raw = std::fs::read_to_string(&fixture_path)
        .unwrap_or_else(|e| panic!("read fixture {fixture_path:?}: {e}"));
    let pipeline = nxuskit::contract::v1_1::load_pipeline_definition_json(&raw)
        .expect("feature 104 fixture must load under feature 105 loader");
    let outcome = pipeline.validate();
    assert!(
        matches!(outcome, PipelineValidationOutcome::Ready),
        "feature 104 pipeline_llm_only fixture must validate Ready under feature 105; got {outcome:?}"
    );

    // Round-trip canonical equality regression: feature 104 fixture must
    // canonical-hash identically before/after round trip through 105.
    let exported = pipeline
        .export_canonical_json()
        .expect("export canonical JSON");
    let re_imported = nxuskit::contract::v1_1::load_pipeline_definition_json(&exported)
        .expect("re-import exported JSON");
    assert_eq!(
        pipeline.canonical_hash().expect("hash original"),
        re_imported.canonical_hash().expect("hash re-imported"),
        "feature 104 pipeline canonical hash must survive feature 105 round-trip"
    );
}
