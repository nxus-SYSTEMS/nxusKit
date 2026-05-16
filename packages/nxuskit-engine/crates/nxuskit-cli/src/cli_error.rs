//! Stable CLI error types with exit codes and JSON error envelopes.
//!
//! Exit code scheme (FR-015):
//!   0 = success
//!   1 = internal error
//!   2 = timeout
//!   3 = authentication failure
//!   4 = entitlement denied
//!   5 = validation error
//! 130 = cancelled (SIGINT)
//!
//! All errors are surfaced as structured JSON on stderr (FR-016).

use serde::{Deserialize, Serialize};

use crate::envelope::{new_trace_id, now_rfc3339};

/// All stable error codes from the shell contract.
#[derive(Debug, Clone, thiserror::Error)]
pub enum CliError {
    // --- Exit code 1: Internal errors ---
    #[error("File not found: {path}")]
    FileNotFound { path: String },

    #[error("Empty input from stdin")]
    EmptyInput,

    #[error("Provider error: {message}")]
    ProviderError { message: String },

    #[error("Merge conflict at paths: {paths:?}")]
    MergeConflict { paths: Vec<String> },

    #[error("Pipeline stage failed: {stage}")]
    PipelineStageFailed {
        stage: String,
        detail: Option<String>,
    },

    #[error("Schema not found: {path}")]
    SchemaNotFound { path: String },

    // --- Exit code 2: Timeout ---
    #[error("Timeout: {message}")]
    #[allow(dead_code)]
    Timeout { message: String },

    #[error("Idle timeout: no output for 30 seconds")]
    IdleTimeout,

    // --- Exit code 3: Authentication failure ---
    #[error("Authentication failed: {message}")]
    #[allow(dead_code)]
    AuthFailed { message: String },

    // --- Exit code 4: Entitlement denied ---
    #[error("This command requires the {required_edition} edition")]
    EntitlementRequired {
        required_edition: String,
        current_edition: String,
    },

    // --- Exit code 5: Validation error ---
    #[error("Invalid format: {value}")]
    InvalidFormat { value: String },

    #[allow(dead_code)]
    #[error("Too many inputs: expected {expected}, got {got}")]
    TooManyInputs { expected: usize, got: usize },

    #[error("Parse error: {message}")]
    ParseError { message: String },

    #[error("Validation failed: {message}")]
    ValidationFailed { message: String },

    /// Command-specific validation failure with a stable machine-readable
    /// `code` string and optional structured `details`. Always exit 5.
    ///
    /// Used by commands whose failure modes need a more specific `code` than
    /// the generic `validation` (e.g. `zen validate` -> `zen_validate_error`,
    /// `zen test` -> `zen_test_mismatch` / `zen_test_eval_error`).
    #[error("{message}")]
    CommandValidation {
        code: &'static str,
        message: String,
        details: Option<serde_json::Value>,
    },

    // --- Exit code 130: Cancelled ---
    #[allow(dead_code)]
    #[error("Cancelled")]
    Cancelled,
}

impl CliError {
    /// Stable snake_case error code for JSON output (FR-016).
    pub fn code(&self) -> &'static str {
        match self {
            // Exit 1: internal
            Self::FileNotFound { .. }
            | Self::EmptyInput
            | Self::ProviderError { .. }
            | Self::MergeConflict { .. }
            | Self::PipelineStageFailed { .. }
            | Self::SchemaNotFound { .. } => "internal",

            // Exit 2: timeout
            Self::Timeout { .. } | Self::IdleTimeout => "timeout",

            // Exit 3: auth_failed
            Self::AuthFailed { .. } => "auth_failed",

            // Exit 4: entitlement_denied
            Self::EntitlementRequired { .. } => "entitlement_denied",

            // Exit 5: validation
            Self::InvalidFormat { .. }
            | Self::TooManyInputs { .. }
            | Self::ParseError { .. }
            | Self::ValidationFailed { .. } => "validation",

            // Exit 5: command-specific validation (carries its own code)
            Self::CommandValidation { code, .. } => code,

            // Exit 130: cancelled
            Self::Cancelled => "cancelled",
        }
    }

    /// Exit code per shell contract (FR-015).
    ///
    /// 0=success, 1=internal, 2=timeout, 3=auth, 4=entitlement, 5=validation, 130=cancelled.
    pub fn exit_code(&self) -> i32 {
        match self {
            // Exit 2: timeout
            Self::Timeout { .. } | Self::IdleTimeout => 2,

            // Exit 3: auth
            Self::AuthFailed { .. } => 3,

            // Exit 4: entitlement
            Self::EntitlementRequired { .. } => 4,

            // Exit 5: validation
            Self::InvalidFormat { .. }
            | Self::TooManyInputs { .. }
            | Self::ParseError { .. }
            | Self::ValidationFailed { .. }
            | Self::CommandValidation { .. } => 5,

            // Exit 130: cancelled
            Self::Cancelled => 130,

            // Exit 1: everything else (internal)
            _ => 1,
        }
    }

    /// Convert to a JSON error envelope for stderr output (FR-016).
    pub fn to_error_envelope(&self) -> ErrorEnvelope {
        let details: Option<serde_json::Value> = match self {
            Self::EntitlementRequired {
                required_edition,
                current_edition,
            } => Some(serde_json::json!({
                "feature": required_edition,
                "current_tier": current_edition,
                "upgrade_url": "https://nxus.systems/pricing"
            })),
            Self::MergeConflict { paths } => Some(serde_json::json!({ "conflict_paths": paths })),
            Self::PipelineStageFailed { detail, .. } => detail
                .as_ref()
                .map(|d| serde_json::json!({ "stage_detail": d })),
            Self::CommandValidation { details, .. } => details.clone(),
            _ => None,
        };

        ErrorEnvelope {
            code: self.code().to_string(),
            message: self.to_string(),
            details,
            trace_id: new_trace_id().to_string(),
            timestamp: now_rfc3339(),
        }
    }
}

/// Structured error written to stderr as JSON (FR-016).
///
/// Fields: `code`, `message`, `details`, `trace_id`, `timestamp`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorEnvelope {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    pub trace_id: String,
    pub timestamp: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_exit_code_is_2() {
        let err = CliError::Timeout {
            message: "network timed out".into(),
        };
        assert_eq!(err.exit_code(), 2);
        assert_eq!(err.code(), "timeout");
    }

    #[test]
    fn idle_timeout_exit_code_is_2() {
        assert_eq!(CliError::IdleTimeout.exit_code(), 2);
        assert_eq!(CliError::IdleTimeout.code(), "timeout");
    }

    #[test]
    fn auth_failed_exit_code_is_3() {
        let err = CliError::AuthFailed {
            message: "invalid credentials".into(),
        };
        assert_eq!(err.exit_code(), 3);
        assert_eq!(err.code(), "auth_failed");
    }

    #[test]
    fn entitlement_exit_code_is_4() {
        let err = CliError::EntitlementRequired {
            required_edition: "pro".into(),
            current_edition: "community".into(),
        };
        assert_eq!(err.exit_code(), 4);
        assert_eq!(err.code(), "entitlement_denied");
    }

    #[test]
    fn entitlement_envelope_has_structured_details() {
        let err = CliError::EntitlementRequired {
            required_edition: "pro".into(),
            current_edition: "community".into(),
        };
        let envelope = err.to_error_envelope();
        assert_eq!(envelope.code, "entitlement_denied");
        let details = envelope.details.expect("details must be present");
        assert_eq!(details["feature"], "pro");
        assert_eq!(details["current_tier"], "community");
        assert!(
            details["upgrade_url"]
                .as_str()
                .unwrap()
                .starts_with("https://")
        );
    }

    #[test]
    fn validation_errors_exit_code_is_5() {
        let cases: Vec<CliError> = vec![
            CliError::InvalidFormat {
                value: "xml".into(),
            },
            CliError::TooManyInputs {
                expected: 1,
                got: 3,
            },
            CliError::ParseError {
                message: "bad json".into(),
            },
            CliError::ValidationFailed {
                message: "missing field".into(),
            },
        ];
        for err in cases {
            assert_eq!(err.exit_code(), 5, "Expected exit 5 for {:?}", err);
            assert_eq!(
                err.code(),
                "validation",
                "Expected code 'validation' for {:?}",
                err
            );
        }
    }

    #[test]
    fn cancelled_exit_code_is_130() {
        assert_eq!(CliError::Cancelled.exit_code(), 130);
        assert_eq!(CliError::Cancelled.code(), "cancelled");
    }

    #[test]
    fn internal_errors_exit_code_is_1() {
        let cases: Vec<CliError> = vec![
            CliError::FileNotFound {
                path: "/tmp/x".into(),
            },
            CliError::EmptyInput,
            CliError::ProviderError {
                message: "fail".into(),
            },
            CliError::SchemaNotFound {
                path: "x.json".into(),
            },
        ];
        for err in cases {
            assert_eq!(err.exit_code(), 1, "Expected exit 1 for {:?}", err);
            assert_eq!(
                err.code(),
                "internal",
                "Expected code 'internal' for {:?}",
                err
            );
        }
    }

    #[test]
    fn error_envelope_has_required_fields() {
        let err = CliError::ValidationFailed {
            message: "bad input".into(),
        };
        let envelope = err.to_error_envelope();
        assert_eq!(envelope.code, "validation");
        assert_eq!(envelope.message, "Validation failed: bad input");
        assert!(!envelope.trace_id.is_empty());
        assert!(!envelope.timestamp.is_empty());
    }

    #[test]
    fn error_envelope_serializes_to_spec_schema() {
        let err = CliError::ValidationFailed {
            message: "bad input".into(),
        };
        let envelope = err.to_error_envelope();
        let json = serde_json::to_value(&envelope).unwrap();
        // FR-016: must have code, message, trace_id, timestamp
        assert!(json.get("code").is_some());
        assert!(json.get("message").is_some());
        assert!(json.get("trace_id").is_some());
        assert!(json.get("timestamp").is_some());
        // Must NOT have old field names
        assert!(json.get("error_code").is_none());
        assert!(json.get("detail").is_none());
    }
}
