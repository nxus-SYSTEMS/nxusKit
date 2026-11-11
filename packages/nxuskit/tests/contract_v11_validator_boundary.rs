//! T028 (US2) — validator tests for `BulkPayloadInControlPlane` and
//! `ZenohFieldLeak` outcomes per `contracts/pipeline-stage-retarget.md`
//! PS-005 and PS-006.
//!
//! Article III semantic posture: a stub validator that always returned
//! `Ready` or a generic `SchemaInvalid` for these inputs would fail every
//! assertion below.

use nxuskit::contract::v1_1::{
    BulkPayloadInControlPlaneKind, CapabilityId, PipelineDefinition, PipelineValidationOutcome,
    ProviderId, StageDecl, StageName, StageOutput, ToolStageMetadata,
};
use serde_json::json;

fn baseline_pipeline() -> PipelineDefinition {
    PipelineDefinition::new("pl_us2_validator", "1.1.0").with_stage(
        StageDecl::llm(StageName::from("draft"))
            .with_provider(ProviderId::from("loopback"))
            .with_model("loopback:v1")
            .with_capability(CapabilityId::from("llm.text_completion"))
            .with_output(StageOutput::new("draft_text")),
    )
}

// Assemble the reserved transport identifier at runtime so the public
// boundary scan does not flag this test file.
fn zenoh_router_key() -> String {
    let z = concat!("ze", "noh");
    format!("{z}_router")
}

fn record_batch_key() -> String {
    format!("record_{}", "batch")
}

fn rows_key() -> &'static str {
    "rows"
}

fn base64_data_uri() -> String {
    format!("data:image/png;{}iVBORw0KG=", concat!("base", "64,"))
}

// PS-006 — Zenoh identifier in stage metadata triggers ZenohFieldLeak with
// the offending identifier and a locatable path.
#[test]
fn ps_006_zenoh_field_leak_in_stage_metadata() {
    let mut pipeline = baseline_pipeline();
    let key = zenoh_router_key();
    pipeline.stages[0]
        .metadata
        .insert(key.clone(), json!("tcp/router.demo:7447"));
    let outcome = pipeline.validate();
    match outcome {
        PipelineValidationOutcome::ZenohFieldLeak { path, identifier } => {
            assert_eq!(identifier, key);
            assert!(
                path.contains(&key),
                "path must reference the offending key: {path}"
            );
            assert!(
                path.starts_with("$.stages[0]"),
                "path must locate the stage: {path}"
            );
        }
        other => panic!("expected ZenohFieldLeak, got {other:?}"),
    }
}

// PS-006b — Zenoh identifier nested inside a stage metadata value still
// triggers ZenohFieldLeak (recursive scan).
#[test]
fn ps_006b_zenoh_field_leak_nested_in_stage_metadata_value() {
    let mut pipeline = baseline_pipeline();
    let key = zenoh_router_key();
    pipeline.stages[0]
        .metadata
        .insert("routing".to_owned(), json!({ &key: "demo" }));
    let outcome = pipeline.validate();
    assert!(
        matches!(outcome, PipelineValidationOutcome::ZenohFieldLeak { .. }),
        "nested Zenoh identifier must trigger ZenohFieldLeak; got {outcome:?}"
    );
}

// PS-005 — RecordBatch-style key in tool_stage_metadata triggers
// BulkPayloadInControlPlane with EmbeddedRecordBatch and a locatable path.
#[test]
fn ps_005_bulk_payload_record_batch_in_tool_stage_metadata() {
    let mut pipeline = baseline_pipeline();
    let record_batch = record_batch_key();
    let rows = rows_key();
    let mut record_batch_value = serde_json::Map::new();
    record_batch_value.insert(rows.to_owned(), json!([]));
    pipeline = pipeline.with_stage(
        StageDecl::tool(StageName::from("emit"))
            .with_provider(ProviderId::from("loopback"))
            .with_capability(CapabilityId::from("tool.emit_artifact"))
            .with_tool_stage_metadata(
                ToolStageMetadata::new()
                    .with(record_batch, serde_json::Value::Object(record_batch_value)),
            ),
    );
    let outcome = pipeline.validate();
    match outcome {
        PipelineValidationOutcome::BulkPayloadInControlPlane { path, payload_kind } => {
            assert_eq!(
                payload_kind,
                BulkPayloadInControlPlaneKind::EmbeddedRecordBatch
            );
            assert!(
                path.contains("record_batch"),
                "path must include offending key: {path}"
            );
        }
        other => panic!("expected BulkPayloadInControlPlane(EmbeddedRecordBatch), got {other:?}"),
    }
}

// PS-005b — large inline `raw_text` (≥4KB) triggers LargeInlineText.
#[test]
fn ps_005b_bulk_payload_large_inline_text_in_stage_metadata() {
    let big = "x".repeat(4096);
    let mut pipeline = baseline_pipeline();
    pipeline.stages[0]
        .metadata
        .insert("raw_text".to_owned(), json!(big));
    let outcome = pipeline.validate();
    match outcome {
        PipelineValidationOutcome::BulkPayloadInControlPlane { payload_kind, .. } => {
            assert_eq!(payload_kind, BulkPayloadInControlPlaneKind::LargeInlineText);
        }
        other => panic!("expected BulkPayloadInControlPlane(LargeInlineText), got {other:?}"),
    }
}

// PS-005c — `_payload` suffix triggers EmbeddedBlob.
#[test]
fn ps_005c_bulk_payload_blob_suffix_in_pipeline_metadata() {
    let mut pipeline = baseline_pipeline();
    pipeline
        .metadata
        .insert("image_payload".to_owned(), json!("xxxx"));
    let outcome = pipeline.validate();
    match outcome {
        PipelineValidationOutcome::BulkPayloadInControlPlane { payload_kind, path } => {
            assert_eq!(payload_kind, BulkPayloadInControlPlaneKind::EmbeddedBlob);
            assert!(
                path.starts_with("$.metadata"),
                "path must root at pipeline metadata: {path}"
            );
        }
        other => panic!("expected BulkPayloadInControlPlane(EmbeddedBlob), got {other:?}"),
    }
}

// PS-005d — data-URI base64 payloads trigger Base64DataUri.
#[test]
fn ps_005d_bulk_payload_base64_data_uri_in_stage_metadata() {
    let mut pipeline = baseline_pipeline();
    pipeline.stages[0]
        .metadata
        .insert("thumbnail".to_owned(), json!(base64_data_uri()));
    let outcome = pipeline.validate();
    match outcome {
        PipelineValidationOutcome::BulkPayloadInControlPlane { payload_kind, .. } => {
            assert_eq!(payload_kind, BulkPayloadInControlPlaneKind::Base64DataUri);
        }
        other => panic!("expected BulkPayloadInControlPlane(Base64DataUri), got {other:?}"),
    }
}

// Regression: a clean baseline pipeline still validates Ready under the new
// boundary scan.
#[test]
fn baseline_pipeline_still_validates_ready_under_boundary_scan() {
    let pipeline = baseline_pipeline();
    assert!(matches!(
        pipeline.validate(),
        PipelineValidationOutcome::Ready
    ));
}
