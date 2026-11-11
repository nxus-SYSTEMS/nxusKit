//! Reference-only boundary validation for v2 public control-plane JSON.

use serde_json::Value;

/// Trait implemented by DTOs whose public control-plane metadata must remain
/// reference-only.
pub trait ReferenceOnlyBoundary {
    fn validate_reference_only(&self) -> Result<(), BoundaryViolation>;
}

/// Reference-only boundary violation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{kind:?} at {path}: {key}")]
pub struct BoundaryViolation {
    pub path: String,
    pub key: String,
    pub kind: BoundaryViolationKind,
}

/// Class-identifying violation kinds for Gate B alpha.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BoundaryViolationKind {
    BulkDataKey,
    InlineBodyKey,
    ProtectedContentKey,
    DataUrlValue,
}

/// Validate arbitrary metadata/control-plane JSON for reference-only posture.
pub fn validate_reference_only_value(value: &Value) -> Result<(), BoundaryViolation> {
    validate_value_at(value, "$")
}

fn validate_value_at(value: &Value, path: &str) -> Result<(), BoundaryViolation> {
    match value {
        Value::Object(map) => {
            for (key, nested) in map {
                let normalized_key = key.to_ascii_lowercase();
                if is_bulk_key(&normalized_key) {
                    return Err(BoundaryViolation {
                        path: path.to_owned(),
                        key: key.to_owned(),
                        kind: BoundaryViolationKind::BulkDataKey,
                    });
                }
                if is_inline_body_key(&normalized_key) {
                    return Err(BoundaryViolation {
                        path: path.to_owned(),
                        key: key.to_owned(),
                        kind: BoundaryViolationKind::InlineBodyKey,
                    });
                }
                if normalized_key.contains("persona") || normalized_key.contains("workflow_pack") {
                    return Err(BoundaryViolation {
                        path: path.to_owned(),
                        key: key.to_owned(),
                        kind: BoundaryViolationKind::ProtectedContentKey,
                    });
                }
                let next_path = format!("{path}.{key}");
                validate_value_at(nested, &next_path)?;
            }
        }
        Value::Array(items) => {
            for (idx, nested) in items.iter().enumerate() {
                validate_value_at(nested, &format!("{path}[{idx}]"))?;
            }
        }
        Value::String(s) if s.starts_with("data:") && s.contains(";base64,") => {
            return Err(BoundaryViolation {
                path: path.to_owned(),
                key: "data_url".to_owned(),
                kind: BoundaryViolationKind::DataUrlValue,
            });
        }
        _ => {}
    }
    Ok(())
}

fn is_bulk_key(key: &str) -> bool {
    matches!(
        key,
        "record_batch"
            | "record_batches"
            | "row_json"
            | "rows"
            | "inline_sql"
            | "sql_text"
            | "sql"
            | "file_contents"
            | "file_content"
            | "file_bytes"
            | "blob"
            | "payload"
            | "body"
    )
}

fn is_inline_body_key(key: &str) -> bool {
    key.ends_with("_body")
        || key.ends_with("_blob")
        || key.ends_with("_payload")
        || key.ends_with("_bytes")
        || key.ends_with("_contents")
}
