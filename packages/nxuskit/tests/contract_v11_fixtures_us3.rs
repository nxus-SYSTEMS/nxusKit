//! US3 (T042-T044) — fixture-driven validator pins.
//!
//! Each fixture is the on-disk file that the contract authors named in the
//! retarget fixture index and pipeline-stage contract. The tests load the
//! fixture through the public loader and assert the exact validator outcome.

use std::path::PathBuf;

use nxuskit::contract::v1_1::{PipelineValidationOutcome, load_pipeline_definition_json};

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .join("specs")
        .join("105-v110-arrow-zenoh-substrait-retarget")
        .join("contracts")
        .join("fixtures")
        .join(name)
}

fn load(name: &str) -> nxuskit::contract::v1_1::PipelineDefinition {
    let path = fixture_path(name);
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    load_pipeline_definition_json(&raw).unwrap_or_else(|e| panic!("load {path:?}: {e}"))
}

#[test]
fn t043_positive_pipeline_data_store_search_plan_ref_validates_ready() {
    let pipeline = load("positive_pipeline_data_store_search_plan_ref.json");
    let outcome = pipeline.validate();
    assert!(
        matches!(outcome, PipelineValidationOutcome::Ready),
        "positive DataStoreSearch+PlanRef fixture must validate Ready; got {outcome:?}"
    );
}

#[test]
fn t042_positive_pipeline_llm_plus_referenced_data_tool_validates_ready() {
    let pipeline = load("positive_pipeline_llm_plus_referenced_data_tool.json");
    let outcome = pipeline.validate();
    assert!(
        matches!(outcome, PipelineValidationOutcome::Ready),
        "positive LLM+ReferencedDataTool fixture must validate Ready; got {outcome:?}"
    );
}

#[test]
fn t044a_negative_plan_reference_unbound_fixture_yields_plan_reference_unbound() {
    let pipeline = load("negative_plan_reference_unbound.json");
    let outcome = pipeline.validate();
    match outcome {
        PipelineValidationOutcome::PlanReferenceUnbound { stage_name } => {
            assert_eq!(stage_name.as_str(), "search");
        }
        other => panic!(
            "negative_plan_reference_unbound fixture must yield PlanReferenceUnbound; got {other:?}"
        ),
    }
}

#[test]
fn t044b_negative_ambiguous_slot_fixture_yields_ambiguous_slot() {
    let pipeline = load("negative_ambiguous_slot.json");
    let outcome = pipeline.validate();
    match outcome {
        PipelineValidationOutcome::AmbiguousSlot { path } => {
            assert!(
                path.contains("tool_input"),
                "AmbiguousSlot path must reference offending slot key: {path}"
            );
        }
        other => panic!("negative_ambiguous_slot fixture must yield AmbiguousSlot; got {other:?}"),
    }
}
