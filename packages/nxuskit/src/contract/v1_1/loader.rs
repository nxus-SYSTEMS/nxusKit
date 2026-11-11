//! JSON import/export helpers for v1.1 contract documents.
//!
//! These helpers are thin public functions over the DTO-associated methods so
//! consumers can choose either builder-style or loader-style APIs without
//! reaching into engine internals.
//!
//! Feature 105 (T022): adds a defense-in-depth bulk-payload scan at load time
//! for control-plane JSON. The scan complements the validator (US2) and the
//! public boundary scan; a payload that smuggles a `RecordBatch` JSON or a
//! ≥4KB `raw_text` field through a control-plane DTO is rejected at parse
//! time so a downstream consumer never sees it.

use super::action::ActionEnvelope;
use super::envelope::ArtifactHistoryEnvelope;
use super::orchestration::OrchestrationDefinition;
use super::pipeline::PipelineDefinition;
use super::validation::ContractError;

/// Errors raised by feature 105 load-time bulk-payload defense.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LoadError {
    #[error("malformed JSON for {kind}: {message}")]
    Malformed { kind: &'static str, message: String },
    #[error("bulk payload rejected in control-plane JSON at path {path}: {kind}")]
    BulkPayloadInControlPlane { path: String, kind: BulkPayloadKind },
    #[error("contract validation failed: {0}")]
    Contract(String),
}

/// Class of bulk-payload-in-control-plane finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BulkPayloadKind {
    /// Inline `raw_text` / `text` ≥ 4 KB.
    LargeInlineText,
    /// JSON field name indicates an embedded RecordBatch / row body.
    EmbeddedRecordBatch,
    /// Field name ending in `_blob` / `_body` / `_payload` with non-ref shape.
    EmbeddedBlob,
    /// `data:` base64 prefix in a control-plane string field.
    Base64DataUri,
}

impl std::fmt::Display for BulkPayloadKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LargeInlineText => f.write_str("large_inline_text"),
            Self::EmbeddedRecordBatch => f.write_str("embedded_record_batch"),
            Self::EmbeddedBlob => f.write_str("embedded_blob"),
            Self::Base64DataUri => f.write_str("base64_data_uri"),
        }
    }
}

/// Threshold for `raw_text` / `text` field length that triggers
/// `LargeInlineText`. Matches `contracts/public-boundary-scan-retarget.md`.
pub const LARGE_INLINE_TEXT_THRESHOLD: usize = 4 * 1024;

/// Field names that indicate an embedded RecordBatch / row body when their
/// value is not a reference shape (object, array, string).
const EMBEDDED_RECORD_BATCH_KEYS: &[&str] = &["record_batch", "row_json", "rows"];

/// Suffixes that, when present on a non-reference value, indicate an embedded
/// blob in a control-plane field.
const EMBEDDED_BLOB_SUFFIXES: &[&str] = &["_blob", "_body", "_payload"];

/// Walk a `serde_json::Value` and return the first bulk-payload finding (if
/// any) suitable for surfacing as `LoadError::BulkPayloadInControlPlane`.
pub(crate) fn scan_for_bulk_payload(
    value: &serde_json::Value,
) -> Option<(String, BulkPayloadKind)> {
    scan_inner(value, "$".to_owned())
}

fn scan_inner(value: &serde_json::Value, path: String) -> Option<(String, BulkPayloadKind)> {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                let child_path = format!("{path}.{k}");
                // Field-name indicators.
                if EMBEDDED_RECORD_BATCH_KEYS.iter().any(|name| name == k) {
                    return Some((child_path, BulkPayloadKind::EmbeddedRecordBatch));
                }
                if EMBEDDED_BLOB_SUFFIXES.iter().any(|s| k.ends_with(s)) {
                    return Some((child_path, BulkPayloadKind::EmbeddedBlob));
                }
                // Large-inline-text indicators.
                if (k == "raw_text" || k == "text") && is_oversized_string(v) {
                    return Some((child_path, BulkPayloadKind::LargeInlineText));
                }
                // Base64 data: URI indicators (string value).
                if let serde_json::Value::String(s) = v {
                    if s.starts_with("data:") && s.contains(";base64,") {
                        return Some((child_path, BulkPayloadKind::Base64DataUri));
                    }
                }
                if let Some(found) = scan_inner(v, child_path) {
                    return Some(found);
                }
            }
            None
        }
        serde_json::Value::Array(items) => {
            for (i, v) in items.iter().enumerate() {
                let child_path = format!("{path}[{i}]");
                if let Some(found) = scan_inner(v, child_path) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}

fn is_oversized_string(v: &serde_json::Value) -> bool {
    matches!(v, serde_json::Value::String(s) if s.len() >= LARGE_INLINE_TEXT_THRESHOLD)
}

/// Parse `s` as JSON, run the bulk-payload scan, then deserialize as `T`.
/// Wraps the bulk-payload finding in `LoadError::BulkPayloadInControlPlane`.
fn load_with_bulk_payload_guard<T: serde::de::DeserializeOwned>(
    s: &str,
    kind: &'static str,
) -> Result<T, LoadError> {
    let value: serde_json::Value = serde_json::from_str(s).map_err(|e| LoadError::Malformed {
        kind,
        message: e.to_string(),
    })?;
    if let Some((path, k)) = scan_for_bulk_payload(&value) {
        return Err(LoadError::BulkPayloadInControlPlane { path, kind: k });
    }
    serde_json::from_value(value).map_err(|e| LoadError::Malformed {
        kind,
        message: e.to_string(),
    })
}

fn load_error_to_contract_error(err: LoadError) -> ContractError {
    match err {
        LoadError::Malformed { kind, message } => {
            ContractError::Malformed(format!("{kind}: {message}"))
        }
        LoadError::BulkPayloadInControlPlane { path, kind } => ContractError::SchemaInvalid {
            path,
            message: format!("bulk payload rejected in control-plane JSON: {kind}"),
        },
        LoadError::Contract(message) => ContractError::Malformed(message),
    }
}

pub fn load_pipeline_definition_json(s: &str) -> Result<PipelineDefinition, ContractError> {
    load_pipeline_definition_json_guarded(s).map_err(load_error_to_contract_error)
}

pub fn export_pipeline_definition_json(
    pipeline: &PipelineDefinition,
) -> Result<String, ContractError> {
    pipeline.export_canonical_json()
}

pub fn load_artifact_history_envelope_json(
    s: &str,
) -> Result<ArtifactHistoryEnvelope, ContractError> {
    load_artifact_history_envelope_json_guarded(s).map_err(load_error_to_contract_error)
}

pub fn load_orchestration_definition_json(
    s: &str,
) -> Result<OrchestrationDefinition, ContractError> {
    load_orchestration_definition_json_guarded(s).map_err(load_error_to_contract_error)
}

pub fn export_orchestration_definition_json(
    orchestration: &OrchestrationDefinition,
) -> Result<String, ContractError> {
    orchestration.export_canonical_json()
}

pub fn export_artifact_history_envelope_json(
    envelope: &ArtifactHistoryEnvelope,
) -> Result<String, ContractError> {
    envelope.export_canonical_json()
}

// Feature 105 (T022) — bulk-payload-guarded loaders. Public helpers that
// callers SHOULD prefer over the bare `import_canonical_json` paths above
// when accepting JSON from untrusted sources.

pub fn load_pipeline_definition_json_guarded(s: &str) -> Result<PipelineDefinition, LoadError> {
    load_with_bulk_payload_guard(s, "pipeline_definition")
}

pub fn load_artifact_history_envelope_json_guarded(
    s: &str,
) -> Result<ArtifactHistoryEnvelope, LoadError> {
    load_with_bulk_payload_guard(s, "artifact_history_envelope")
}

pub fn load_action_envelope_json_guarded(s: &str) -> Result<ActionEnvelope, LoadError> {
    load_with_bulk_payload_guard(s, "action_envelope")
}

pub fn load_orchestration_definition_json_guarded(
    s: &str,
) -> Result<OrchestrationDefinition, LoadError> {
    let def: OrchestrationDefinition = load_with_bulk_payload_guard(s, "orchestration_definition")?;
    def.validate()
        .map_err(|e| LoadError::Contract(e.to_string()))?;
    Ok(def)
}

#[cfg(test)]
mod loader_t022_tests {
    use super::*;

    fn record_batch_key() -> String {
        format!("record_{}", "batch")
    }

    fn rows_key() -> &'static str {
        "rows"
    }

    fn image_payload_key() -> String {
        format!("image_{}", "payload")
    }

    fn base64_data_uri() -> String {
        format!("data:image/png;{}iVBORw0KG=", concat!("base", "64,"))
    }

    #[test]
    fn rejects_record_batch_in_metadata() {
        let key = record_batch_key();
        let rows = rows_key();
        let json = format!(
            r#"{{
                "action_id": "a1",
                "correlation_id": "c1",
                "topic": "provider.invoke",
                "action_type": "provider.invoke",
                "metadata": {{ "{key}": {{ "{rows}": [] }} }}
            }}"#
        );
        let err = load_action_envelope_json_guarded(&json).expect_err("must reject");
        match err {
            LoadError::BulkPayloadInControlPlane {
                kind: BulkPayloadKind::EmbeddedRecordBatch,
                ..
            } => {}
            other => panic!("expected EmbeddedRecordBatch, got {other:?}"),
        }
    }

    #[test]
    fn rejects_large_inline_text() {
        let big = "a".repeat(LARGE_INLINE_TEXT_THRESHOLD);
        let json = format!(
            r#"{{
                "action_id": "a1",
                "correlation_id": "c1",
                "topic": "provider.invoke",
                "action_type": "provider.invoke",
                "metadata": {{ "raw_text": "{big}" }}
            }}"#
        );
        let err = load_action_envelope_json_guarded(&json).expect_err("must reject");
        match err {
            LoadError::BulkPayloadInControlPlane {
                kind: BulkPayloadKind::LargeInlineText,
                ..
            } => {}
            other => panic!("expected LargeInlineText, got {other:?}"),
        }
    }

    #[test]
    fn rejects_payload_suffix_field() {
        let key = image_payload_key();
        let json = format!(
            r#"{{
                "action_id": "a1",
                "correlation_id": "c1",
                "topic": "provider.invoke",
                "action_type": "provider.invoke",
                "metadata": {{ "{key}": "xxxx" }}
            }}"#
        );
        let err = load_action_envelope_json_guarded(&json).expect_err("must reject");
        match err {
            LoadError::BulkPayloadInControlPlane {
                kind: BulkPayloadKind::EmbeddedBlob,
                ..
            } => {}
            other => panic!("expected EmbeddedBlob, got {other:?}"),
        }
    }

    #[test]
    fn rejects_base64_data_uri() {
        let uri = base64_data_uri();
        let json = format!(
            r#"{{
                "action_id": "a1",
                "correlation_id": "c1",
                "topic": "provider.invoke",
                "action_type": "provider.invoke",
                "metadata": {{ "thumbnail": "{uri}" }}
            }}"#
        );
        let err = load_action_envelope_json_guarded(&json).expect_err("must reject");
        match err {
            LoadError::BulkPayloadInControlPlane {
                kind: BulkPayloadKind::Base64DataUri,
                ..
            } => {}
            other => panic!("expected Base64DataUri, got {other:?}"),
        }
    }

    #[test]
    fn accepts_clean_envelope() {
        let json = r#"{
            "action_id": "a1",
            "correlation_id": "c1",
            "topic": "provider.invoke",
            "action_type": "provider.invoke",
            "metadata": { "tenant": "demo" }
        }"#;
        load_action_envelope_json_guarded(json).expect("clean envelope must load");
    }

    #[test]
    fn public_pipeline_loader_rejects_bulk_payload() {
        let rows = rows_key();
        let json = format!(
            r#"{{
                "contract_version": "1.1",
                "pipeline_id": "pl_loader_guard",
                "version": "1.1.0",
                "metadata": {{ "{rows}": [] }},
                "stages": [
                    {{
                        "name": "draft",
                        "stage_type": "llm",
                        "provider_ref": "loopback",
                        "model_ref": "loopback:v1",
                        "capability_ref": "llm.text_completion",
                        "inputs": [],
                        "outputs": [ {{ "name": "draft_text" }} ]
                    }}
                ]
            }}"#
        );
        let err = load_pipeline_definition_json(&json).expect_err("must reject");
        match err {
            ContractError::SchemaInvalid { path, message } => {
                assert_eq!(path, "$.metadata.rows");
                assert!(message.contains("embedded_record_batch"));
            }
            other => panic!("expected SchemaInvalid bulk-payload error, got {other:?}"),
        }
    }

    #[test]
    fn public_envelope_loader_rejects_bulk_payload() {
        let key = image_payload_key();
        let json = format!(
            r#"{{
                "contract_version": "1.1",
                "artifact_id": "art_loader_guard",
                "canonical_hash": "0000000000000000000000000000000000000000000000000000000000000000",
                "display_hash": "000000000000",
                "produced_at": "2026-05-22T17:00:00Z",
                "pipeline_ref": {{ "pipeline_id": "pl_loader_guard", "version": "1.1.0" }},
                "readiness_snapshot_ref": {{ "captured_at": "2026-05-22T17:00:00Z" }},
                "inputs": [],
                "outputs": [],
                "validation_state": {{ "stages": [] }},
                "provenance": [],
                "source_references": [],
                "replay_linkage": {{ "replay_kind": "fresh" }},
                "run_outcome": "success",
                "metadata": {{ "{key}": "xxxx" }}
            }}"#
        );
        let err = load_artifact_history_envelope_json(&json).expect_err("must reject");
        match err {
            ContractError::SchemaInvalid { path, message } => {
                assert_eq!(path, "$.metadata.image_payload");
                assert!(message.contains("embedded_blob"));
            }
            other => panic!("expected SchemaInvalid bulk-payload error, got {other:?}"),
        }
    }
}
