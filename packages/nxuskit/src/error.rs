//! Error types for the nxuskit wrapper.

/// All errors that can occur when using the nxusKit SDK wrapper.
#[derive(Debug, thiserror::Error)]
pub enum NxuskitError {
    /// Invalid provider configuration (missing fields, unknown provider type).
    #[error("configuration error: {message}")]
    Configuration { message: String },

    /// Authentication failure (invalid or expired API key).
    #[error("authentication failed: {message}")]
    Authentication { message: String },

    /// Provider rate limit exceeded.
    #[error("rate limited: {message}")]
    RateLimited { message: String },

    /// Generic provider error.
    #[error("provider error{}: {message}", provider.as_ref().map(|p| format!(" ({p})")).unwrap_or_default())]
    Provider {
        message: String,
        provider: Option<String>,
    },

    /// Internal SDK error (bug in the SDK).
    #[error("internal error: {message}")]
    Internal { message: String },

    /// Streaming error (mid-stream failure or cancellation).
    #[error("stream error: {message}")]
    Stream { message: String },

    /// SDK version does not match the wrapper's expected version range.
    #[error("version mismatch: expected {expected}, found {found}")]
    VersionMismatch { expected: String, found: String },

    /// SDK shared library could not be found or loaded (dynamic-link mode).
    #[error("library not found: {message}")]
    LibraryNotFound { message: String },

    /// Request data is invalid (e.g. NUL byte in string argument).
    #[error("invalid request: {message}")]
    InvalidRequest { message: String },

    /// Response JSON could not be parsed into the expected type.
    #[error("invalid response: {message}")]
    InvalidResponse { message: String },

    /// CLIPS session error.
    #[error("CLIPS error: {message}")]
    ClipsError { message: String },

    /// Requested feature is not available in the current edition/build.
    ///
    /// Active in v0.9.0 — returned when a feature domain requires a higher edition.
    #[error("feature unavailable: {feature} — {message}")]
    FeatureUnavailable { feature: String, message: String },

    /// License key required but not provided.
    ///
    /// Active in v0.9.0.
    #[error("license required: {feature} — {message}")]
    LicenseRequired { feature: String, message: String },

    /// License key expired.
    ///
    /// Active in v0.9.0.
    #[error("license expired: {feature} — {message}")]
    LicenseExpired { feature: String, message: String },

    /// Current edition does not include feature.
    ///
    /// Active in v0.9.0.
    #[error("edition insufficient: {feature} requires {required_edition} — {message}")]
    EditionInsufficient {
        feature: String,
        required_edition: String,
        message: String,
    },
}

impl NxuskitError {
    /// Parse an error from the C ABI's JSON error format.
    ///
    /// Expected format: `{"error_type": "...", "message": "...", "provider": "..."}`
    pub fn from_json(value: &serde_json::Value) -> Self {
        let error_type = value
            .get("error_type")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let message = value
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error")
            .to_string();
        let provider = value
            .get("provider")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        match error_type {
            "invalid_config" => Self::Configuration { message },
            "auth_failed" | "authentication_failed" => Self::Authentication { message },
            "rate_limited" => Self::RateLimited { message },
            "provider_error" => Self::Provider { message, provider },
            "internal_error" => Self::Internal { message },
            "feature_unavailable" => Self::FeatureUnavailable {
                feature: message.clone(),
                message: format!("feature '{message}' is not available in this edition"),
            },
            "license_required" => Self::LicenseRequired {
                feature: message.clone(),
                message: format!("feature '{message}' requires a license key"),
            },
            "license_invalid" => Self::LicenseRequired {
                feature: message.clone(),
                message: format!("invalid license key for feature '{message}'"),
            },
            "license_expired" => Self::LicenseExpired {
                feature: message.clone(),
                message: format!("license for feature '{message}' has expired"),
            },
            "edition_insufficient" => Self::EditionInsufficient {
                feature: message.clone(),
                required_edition: value
                    .get("required_edition")
                    .and_then(|v| v.as_str())
                    .unwrap_or("pro")
                    .to_string(),
                message: format!("feature '{message}' requires a higher edition"),
            },
            _ => Self::Provider { message, provider },
        }
    }

    /// Parse an error from a raw JSON string (as returned by `nxuskit_last_error`).
    pub fn from_json_str(json: &str) -> Self {
        match serde_json::from_str::<serde_json::Value>(json) {
            Ok(val) => Self::from_json(&val),
            Err(_) => Self::Internal {
                message: json.to_string(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entitlement_error_roundtrip_feature_unavailable() {
        let json = r#"{"error_type":"feature_unavailable","message":"zen"}"#;
        let err = NxuskitError::from_json_str(json);
        match err {
            NxuskitError::FeatureUnavailable { feature, .. } => {
                assert_eq!(feature, "zen");
            }
            other => panic!("Expected FeatureUnavailable, got: {other}"),
        }
    }

    #[test]
    fn test_entitlement_error_roundtrip_license_required() {
        let json = r#"{"error_type":"license_required","message":"zen"}"#;
        let err = NxuskitError::from_json_str(json);
        match err {
            NxuskitError::LicenseRequired { feature, message } => {
                assert_eq!(feature, "zen");
                assert!(message.contains("license key"));
            }
            other => panic!("Expected LicenseRequired, got: {other}"),
        }
    }

    #[test]
    fn test_entitlement_error_roundtrip_license_expired() {
        let json = r#"{"error_type":"license_expired","message":"solver"}"#;
        let err = NxuskitError::from_json_str(json);
        match err {
            NxuskitError::LicenseExpired { feature, message } => {
                assert_eq!(feature, "solver");
                assert!(message.contains("expired"));
            }
            other => panic!("Expected LicenseExpired, got: {other}"),
        }
    }

    #[test]
    fn test_entitlement_error_roundtrip_edition_insufficient() {
        let json =
            r#"{"error_type":"edition_insufficient","message":"zen","required_edition":"pro"}"#;
        let err = NxuskitError::from_json_str(json);
        match err {
            NxuskitError::EditionInsufficient {
                feature,
                required_edition,
                message,
            } => {
                assert_eq!(feature, "zen");
                assert_eq!(required_edition, "pro");
                assert!(message.contains("higher edition"));
            }
            other => panic!("Expected EditionInsufficient, got: {other}"),
        }
    }

    #[test]
    fn test_entitlement_error_edition_insufficient_default_edition() {
        let json = r#"{"error_type":"edition_insufficient","message":"zen"}"#;
        let err = NxuskitError::from_json_str(json);
        match err {
            NxuskitError::EditionInsufficient {
                required_edition, ..
            } => {
                assert_eq!(required_edition, "pro");
            }
            other => panic!("Expected EditionInsufficient, got: {other}"),
        }
    }
}
