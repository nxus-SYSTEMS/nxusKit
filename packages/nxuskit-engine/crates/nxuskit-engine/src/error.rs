//! Error types for LLM operations

use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;

/// Result type alias for LLM operations
pub type Result<T> = std::result::Result<T, NxuskitError>;

// ============================================================================
// CLIPS Error Metadata (for actionable error messages)
// ============================================================================

/// Metadata for enriched CLIPS errors with actionable information.
///
/// This struct provides additional context for CLIPS errors, including
/// available templates, did-you-mean suggestions, and resolution hints.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClipsErrorMetadata {
    /// Names of all templates in the CLIPS environment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_templates: Option<Vec<String>>,

    /// Schema details for relevant templates.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template_schemas: Option<Vec<ClipsTemplateSchemaInfo>>,

    /// Did-you-mean suggestions based on string similarity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestions: Option<Vec<String>>,

    /// Human-readable resolution hint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,

    /// JSON Schema for the template (when applicable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub json_schema: Option<serde_json::Value>,
}

/// Brief schema info for a template (used in error metadata).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipsTemplateSchemaInfo {
    /// Template name.
    pub name: String,
    /// Slot names and types.
    pub slots: Vec<ClipsSlotInfo>,
    /// Optional documentation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation: Option<String>,
}

/// Brief slot info (used in error metadata).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipsSlotInfo {
    /// Slot name.
    pub name: String,
    /// Slot type as string (e.g., "STRING", "INTEGER").
    #[serde(rename = "type")]
    pub slot_type: String,
    /// Whether this is a multislot.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub multislot: bool,
    /// Default value if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    /// Allowed values constraint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_values: Option<Vec<String>>,
}

/// MCP-specific error kinds
#[derive(Debug, Clone)]
pub enum McpErrorKind {
    /// Failed to connect to MCP server
    ConnectionFailed(String),
    /// MCP authentication failed
    AuthenticationFailed(String),
    /// MCP protocol error
    ProtocolError(String),
    /// Request timeout
    Timeout(String),
    /// Invalid or malformed response from MCP server
    InvalidResponse(String),
}

impl std::fmt::Display for McpErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McpErrorKind::ConnectionFailed(msg) => write!(f, "MCP connection failed: {}", msg),
            McpErrorKind::AuthenticationFailed(msg) => {
                write!(f, "MCP authentication failed: {}", msg)
            }
            McpErrorKind::ProtocolError(msg) => write!(f, "MCP protocol error: {}", msg),
            McpErrorKind::Timeout(msg) => write!(f, "MCP timeout: {}", msg),
            McpErrorKind::InvalidResponse(msg) => write!(f, "Invalid MCP response: {}", msg),
        }
    }
}

impl std::error::Error for McpErrorKind {}

/// CLIPS-specific error kinds
#[derive(Debug, Clone)]
pub enum ClipsErrorKind {
    /// Failed to create CLIPS environment
    EnvironmentCreationFailed,
    /// Failed to load rule base file
    LoadFailed {
        /// The file that failed to load
        file: String,
        /// Error message
        message: String,
    },
    /// Failed to parse CLIPS construct
    ParseError {
        /// The construct that failed
        construct: String,
        /// Error message
        message: String,
    },
    /// Failed to assert fact
    AssertFailed {
        /// The fact that failed
        fact: String,
        /// Error message
        message: String,
    },
    /// Runtime error during inference
    RuntimeError {
        /// The rule involved
        rule: String,
        /// Error message
        message: String,
    },
    /// Template not found
    TemplateNotFound {
        /// Template name
        name: String,
    },
    /// Invalid fact schema
    InvalidFactSchema {
        /// Template name
        template: String,
        /// Expected schema
        expected: String,
        /// Actual schema
        got: String,
    },
    /// Rule base file not found
    RuleBaseNotFound {
        /// File path
        path: String,
    },
    /// Generation failed
    GenerationFailed {
        /// Error message
        message: String,
    },
    /// Validation failed
    ValidationFailed {
        /// Validation errors
        errors: Vec<String>,
    },
}

impl std::fmt::Display for ClipsErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClipsErrorKind::EnvironmentCreationFailed => {
                write!(f, "Failed to create CLIPS environment")
            }
            ClipsErrorKind::LoadFailed { file, message } => {
                write!(f, "Failed to load rule base '{}': {}", file, message)
            }
            ClipsErrorKind::ParseError { construct, message } => {
                write!(
                    f,
                    "Failed to parse CLIPS construct '{}': {}",
                    construct, message
                )
            }
            ClipsErrorKind::AssertFailed { fact, message } => {
                write!(f, "Failed to assert fact '{}': {}", fact, message)
            }
            ClipsErrorKind::RuntimeError { rule, message } => {
                write!(f, "Runtime error in rule '{}': {}", rule, message)
            }
            ClipsErrorKind::TemplateNotFound { name } => {
                write!(f, "Template '{}' not found in loaded rule base", name)
            }
            ClipsErrorKind::InvalidFactSchema {
                template,
                expected,
                got,
            } => {
                write!(
                    f,
                    "Invalid schema for template '{}': expected {}, got {}",
                    template, expected, got
                )
            }
            ClipsErrorKind::RuleBaseNotFound { path } => {
                write!(f, "Rule base file not found: {}", path)
            }
            ClipsErrorKind::GenerationFailed { message } => {
                write!(f, "CLIPS generation failed: {}", message)
            }
            ClipsErrorKind::ValidationFailed { errors } => {
                write!(f, "CLIPS validation failed: {}", errors.join("; "))
            }
        }
    }
}

impl std::error::Error for ClipsErrorKind {}

/// Convert clips_sys::ClipsError to NxuskitError
impl From<clips_sys::ClipsError> for NxuskitError {
    fn from(err: clips_sys::ClipsError) -> Self {
        use clips_sys::ClipsError as CE;
        match err {
            CE::EnvironmentCreationFailed => {
                NxuskitError::Clips(ClipsErrorKind::EnvironmentCreationFailed)
            }
            CE::LoadFailed { file, message } => {
                NxuskitError::Clips(ClipsErrorKind::LoadFailed { file, message })
            }
            CE::ParseError { construct, message } => {
                NxuskitError::Clips(ClipsErrorKind::ParseError { construct, message })
            }
            CE::BuildFailed { construct } => NxuskitError::Clips(ClipsErrorKind::ParseError {
                construct: construct.clone(),
                message: format!("Failed to build construct: {}", construct),
            }),
            CE::AssertFailed { fact, message } => {
                NxuskitError::Clips(ClipsErrorKind::AssertFailed { fact, message })
            }
            CE::RetractFailed { fact_index } => NxuskitError::Clips(ClipsErrorKind::RuntimeError {
                rule: String::new(),
                message: format!("Failed to retract fact with index {}", fact_index),
            }),
            CE::EvaluationFailed { expression } => {
                NxuskitError::Clips(ClipsErrorKind::RuntimeError {
                    rule: String::new(),
                    message: format!("Evaluation failed: {}", expression),
                })
            }
            CE::TemplateNotFound { name } => {
                NxuskitError::Clips(ClipsErrorKind::TemplateNotFound { name })
            }
            CE::RuleNotFound { name } => NxuskitError::Clips(ClipsErrorKind::RuntimeError {
                rule: name.clone(),
                message: format!("Rule '{}' not found", name),
            }),
            CE::ModuleNotFound { name } => NxuskitError::Clips(ClipsErrorKind::RuntimeError {
                rule: String::new(),
                message: format!("Module '{}' not found", name),
            }),
            CE::GlobalNotFound { name } => NxuskitError::Clips(ClipsErrorKind::RuntimeError {
                rule: String::new(),
                message: format!("Global '{}' not found", name),
            }),
            CE::FunctionNotFound { name } => NxuskitError::Clips(ClipsErrorKind::RuntimeError {
                rule: String::new(),
                message: format!("Function '{}' not found", name),
            }),
            CE::ClassNotFound { name } => NxuskitError::Clips(ClipsErrorKind::RuntimeError {
                rule: String::new(),
                message: format!("Class '{}' not found", name),
            }),
            CE::InstanceNotFound { name } => NxuskitError::Clips(ClipsErrorKind::RuntimeError {
                rule: String::new(),
                message: format!("Instance '{}' not found", name),
            }),
            CE::SlotNotFound { template, slot } => {
                NxuskitError::Clips(ClipsErrorKind::InvalidFactSchema {
                    template,
                    expected: slot.clone(),
                    got: format!("slot '{}' not found", slot),
                })
            }
            CE::InvalidSlotType {
                slot,
                expected,
                got,
            } => NxuskitError::Clips(ClipsErrorKind::InvalidFactSchema {
                template: String::new(),
                expected: format!("{} for slot '{}'", expected, slot),
                got,
            }),
            CE::FactBuilderCreationFailed { template } => {
                NxuskitError::Clips(ClipsErrorKind::RuntimeError {
                    rule: String::new(),
                    message: format!("Failed to create fact builder for template '{}'", template),
                })
            }
            CE::FactBuilderError { code, message } => {
                NxuskitError::Clips(ClipsErrorKind::RuntimeError {
                    rule: String::new(),
                    message: format!("Fact builder error (code {}): {}", code, message),
                })
            }
            CE::ResetFailed => NxuskitError::Clips(ClipsErrorKind::RuntimeError {
                rule: String::new(),
                message: "Failed to reset CLIPS environment".to_string(),
            }),
            CE::ClearFailed => NxuskitError::Clips(ClipsErrorKind::RuntimeError {
                rule: String::new(),
                message: "Failed to clear CLIPS environment".to_string(),
            }),
            CE::WatchFailed | CE::UnwatchFailed => {
                NxuskitError::Clips(ClipsErrorKind::RuntimeError {
                    rule: String::new(),
                    message: "Watch operation failed".to_string(),
                })
            }
            CE::RouterFailed { message } => NxuskitError::Clips(ClipsErrorKind::RuntimeError {
                rule: String::new(),
                message: format!("Router operation failed: {}", message),
            }),
            CE::InstanceCreationFailed { message } => {
                NxuskitError::Clips(ClipsErrorKind::RuntimeError {
                    rule: String::new(),
                    message: format!("Instance creation failed: {}", message),
                })
            }
            CE::MessageSendFailed { message } => {
                NxuskitError::Clips(ClipsErrorKind::RuntimeError {
                    rule: String::new(),
                    message: format!("Message send failed: {}", message),
                })
            }
            CE::NullPointer { operation } => NxuskitError::Clips(ClipsErrorKind::RuntimeError {
                rule: String::new(),
                message: format!("Null pointer from CLIPS operation: {}", operation),
            }),
            CE::NulError(e) => NxuskitError::Clips(ClipsErrorKind::RuntimeError {
                rule: String::new(),
                message: format!("String contains null byte: {}", e),
            }),
            CE::Utf8Error(e) => NxuskitError::Clips(ClipsErrorKind::RuntimeError {
                rule: String::new(),
                message: format!("UTF-8 conversion error: {}", e),
            }),
            CE::InvalidValueType { type_code } => {
                NxuskitError::Clips(ClipsErrorKind::RuntimeError {
                    rule: String::new(),
                    message: format!("Invalid CLIPS value type: {}", type_code),
                })
            }
            CE::MultifieldIndexOutOfBounds { index, length } => {
                NxuskitError::Clips(ClipsErrorKind::RuntimeError {
                    rule: String::new(),
                    message: format!(
                        "Multifield index {} out of bounds (length: {})",
                        index, length
                    ),
                })
            }
            CE::EnvironmentDestroyed => NxuskitError::Clips(ClipsErrorKind::RuntimeError {
                rule: String::new(),
                message: "CLIPS environment has been destroyed".to_string(),
            }),
            CE::Timeout { duration_ms } => NxuskitError::Clips(ClipsErrorKind::RuntimeError {
                rule: String::new(),
                message: format!("Operation timed out after {}ms", duration_ms),
            }),
            CE::ExecutionHalted => NxuskitError::Clips(ClipsErrorKind::RuntimeError {
                rule: String::new(),
                message: "CLIPS execution was halted".to_string(),
            }),
            CE::BinaryError { operation, file } => {
                NxuskitError::Clips(ClipsErrorKind::LoadFailed {
                    file,
                    message: format!("Binary {} failed", operation),
                })
            }
        }
    }
}

/// Comprehensive error type for all LLM operations
#[derive(Debug, Error)]
pub enum NxuskitError {
    /// Authentication failure (invalid API key, etc.)
    #[error("Authentication failed: {0}")]
    Authentication(String),

    /// Rate limit exceeded
    #[error("Rate limit exceeded{}", .retry_after.map(|d| format!(", retry after {:?}", d)).unwrap_or_default())]
    RateLimit {
        /// Time to wait before retrying
        retry_after: Option<Duration>,
    },

    /// Network error
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    /// Invalid request
    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    /// Provider-specific error
    #[error("Provider error (status {status}): {message}")]
    Provider {
        /// HTTP status code
        status: u16,
        /// Error message from provider
        message: String,
    },

    /// Streaming error
    #[error("Stream error: {0}")]
    Stream(String),

    /// Serialization/deserialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Configuration(String),

    /// Invalid image format
    #[error("Invalid image format: {0}")]
    InvalidImageFormat(String),

    /// Image file not found
    #[error("Image file not found: {0}")]
    ImageFileNotFound(String),

    /// Image too large for provider
    #[error("Image too large: {size} bytes exceeds {limit} bytes limit for {provider}")]
    ImageTooLarge {
        size: u64,
        limit: u64,
        provider: String,
    },

    /// Image encoding failed
    #[error("Failed to encode image: {0}")]
    ImageEncodingFailed(String),

    /// Invalid image detail level
    #[error("Invalid detail level: {0}. Must be 'low', 'high', or 'auto'")]
    InvalidDetailLevel(String),

    /// I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// MCP-specific error
    #[error("{0}")]
    Mcp(McpErrorKind),

    /// CLIPS-specific error
    #[error("{0}")]
    Clips(ClipsErrorKind),

    /// Feature not yet implemented
    #[error("{feature} is not yet implemented")]
    NotImplemented {
        /// The feature that is not implemented
        feature: String,
    },

    /// Could not detect provider for model (convenience API)
    #[error(
        "Could not detect provider for model '{0}'. Try using 'provider/model' format. Supported providers: {1}"
    )]
    ProviderDetectionFailed(String, String),

    /// Model not recognized (convenience API)
    #[error("Model '{0}' not recognized. Supported models: {1}")]
    ModelNotRecognized(String, String),

    /// Missing API credentials (convenience API)
    #[error("Missing API credentials for {0}. Set environment variable: {1}")]
    MissingCredentials(String, String),

    /// Invalid model specifier format (convenience API)
    #[error("Invalid model specifier '{0}'. Expected format: 'model' or 'provider/model'")]
    InvalidModelSpecifier(String),

    /// Feature requires a paid license.
    ///
    /// Enforcement deferred to v0.9.0 — in v0.8.1 `check_entitlement()` always
    /// returns `true`.  The variant exists so that downstream code can
    /// pattern-match against it once enforcement is enabled.
    #[error("{feature} requires a nxusKit Pro license. Visit https://nxuskit.dev/pro for details.")]
    LicenseRequired {
        /// The feature that requires a license
        feature: String,
    },

    /// Feature is not available in the current edition/build.
    ///
    /// Enforcement deferred to v0.9.0.
    #[error("{feature} is not available in this edition")]
    FeatureUnavailable {
        /// The feature that is unavailable
        feature: String,
    },

    /// License key has expired.
    ///
    /// Enforcement deferred to v0.9.0.
    #[error("License for {feature} has expired")]
    LicenseExpired {
        /// The feature whose license expired
        feature: String,
    },

    /// Current edition does not include the requested feature.
    ///
    /// Enforcement deferred to v0.9.0.
    #[error("{feature} requires {required_edition} edition")]
    EditionInsufficient {
        /// The feature that requires a higher edition
        feature: String,
        /// The edition required (e.g. "pro", "enterprise")
        required_edition: String,
    },
}

impl NxuskitError {
    /// Check if this error is retryable
    ///
    /// Retryable errors include:
    /// - Network failures
    /// - Rate limits
    /// - 5xx server errors
    /// - MCP connection failures and timeouts
    pub fn is_retryable(&self) -> bool {
        match self {
            NxuskitError::Network(_) => true,
            NxuskitError::RateLimit { .. } => true,
            NxuskitError::Provider { status, .. } => *status >= 500 && *status < 600,
            NxuskitError::Mcp(kind) => matches!(
                kind,
                McpErrorKind::ConnectionFailed(_) | McpErrorKind::Timeout(_)
            ),
            _ => false,
        }
    }

    /// Get the HTTP status code, if applicable
    pub fn status_code(&self) -> Option<u16> {
        match self {
            NxuskitError::Provider { status, .. } => Some(*status),
            NxuskitError::Authentication(_) => Some(401),
            NxuskitError::RateLimit { .. } => Some(429),
            _ => None,
        }
    }

    /// Get the retry-after duration for rate limit errors
    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            NxuskitError::RateLimit { retry_after } => *retry_after,
            _ => None,
        }
    }

    /// Create a rate limit error with retry-after duration
    pub fn rate_limit(retry_after: Option<Duration>) -> Self {
        NxuskitError::RateLimit { retry_after }
    }

    /// Create a provider error
    pub fn provider(status: u16, message: impl Into<String>) -> Self {
        NxuskitError::Provider {
            status,
            message: message.into(),
        }
    }

    /// Create an MCP connection failed error
    pub fn mcp_connection_failed(message: impl Into<String>) -> Self {
        NxuskitError::Mcp(McpErrorKind::ConnectionFailed(message.into()))
    }

    /// Create an MCP authentication failed error
    pub fn mcp_authentication_failed(message: impl Into<String>) -> Self {
        NxuskitError::Mcp(McpErrorKind::AuthenticationFailed(message.into()))
    }

    /// Create an MCP protocol error
    pub fn mcp_protocol_error(message: impl Into<String>) -> Self {
        NxuskitError::Mcp(McpErrorKind::ProtocolError(message.into()))
    }

    /// Create an MCP timeout error
    pub fn mcp_timeout(message: impl Into<String>) -> Self {
        NxuskitError::Mcp(McpErrorKind::Timeout(message.into()))
    }

    /// Create an MCP invalid response error
    pub fn mcp_invalid_response(message: impl Into<String>) -> Self {
        NxuskitError::Mcp(McpErrorKind::InvalidResponse(message.into()))
    }

    /// Create a license required error
    pub fn license_required(feature: impl Into<String>) -> Self {
        NxuskitError::LicenseRequired {
            feature: feature.into(),
        }
    }

    /// Create a feature unavailable error
    pub fn feature_unavailable(feature: impl Into<String>) -> Self {
        NxuskitError::FeatureUnavailable {
            feature: feature.into(),
        }
    }

    /// Create a license expired error
    pub fn license_expired(feature: impl Into<String>) -> Self {
        NxuskitError::LicenseExpired {
            feature: feature.into(),
        }
    }

    /// Create an edition insufficient error
    pub fn edition_insufficient(
        feature: impl Into<String>,
        required_edition: impl Into<String>,
    ) -> Self {
        NxuskitError::EditionInsufficient {
            feature: feature.into(),
            required_edition: required_edition.into(),
        }
    }

    /// Create a not implemented error
    pub fn not_implemented(feature: impl Into<String>) -> Self {
        NxuskitError::NotImplemented {
            feature: feature.into(),
        }
    }

    /// Create a CLIPS environment creation failed error
    pub fn clips_environment_failed() -> Self {
        NxuskitError::Clips(ClipsErrorKind::EnvironmentCreationFailed)
    }

    /// Create a CLIPS load failed error
    pub fn clips_load_failed(file: impl Into<String>, message: impl Into<String>) -> Self {
        NxuskitError::Clips(ClipsErrorKind::LoadFailed {
            file: file.into(),
            message: message.into(),
        })
    }

    /// Create a CLIPS parse error
    pub fn clips_parse_error(construct: impl Into<String>, message: impl Into<String>) -> Self {
        NxuskitError::Clips(ClipsErrorKind::ParseError {
            construct: construct.into(),
            message: message.into(),
        })
    }

    /// Create a CLIPS assert failed error
    pub fn clips_assert_failed(fact: impl Into<String>, message: impl Into<String>) -> Self {
        NxuskitError::Clips(ClipsErrorKind::AssertFailed {
            fact: fact.into(),
            message: message.into(),
        })
    }

    /// Create a CLIPS template not found error
    pub fn clips_template_not_found(name: impl Into<String>) -> Self {
        NxuskitError::Clips(ClipsErrorKind::TemplateNotFound { name: name.into() })
    }

    /// Create a CLIPS generation failed error
    pub fn clips_generation_failed(message: impl Into<String>) -> Self {
        NxuskitError::Clips(ClipsErrorKind::GenerationFailed {
            message: message.into(),
        })
    }

    /// Create a CLIPS validation failed error
    pub fn clips_validation_failed(errors: Vec<String>) -> Self {
        NxuskitError::Clips(ClipsErrorKind::ValidationFailed { errors })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_retryable() {
        assert!(NxuskitError::RateLimit { retry_after: None }.is_retryable());
        assert!(NxuskitError::provider(500, "Server error").is_retryable());
        assert!(NxuskitError::provider(503, "Service unavailable").is_retryable());
        assert!(!NxuskitError::Authentication("Invalid key".to_string()).is_retryable());
        assert!(!NxuskitError::provider(400, "Bad request").is_retryable());
        assert!(!NxuskitError::Configuration("Missing config".to_string()).is_retryable());
    }

    #[test]
    fn test_status_code() {
        assert_eq!(
            NxuskitError::provider(404, "Not found").status_code(),
            Some(404)
        );
        assert_eq!(
            NxuskitError::Authentication("Bad key".to_string()).status_code(),
            Some(401)
        );
        assert_eq!(
            NxuskitError::RateLimit { retry_after: None }.status_code(),
            Some(429)
        );
        assert_eq!(
            NxuskitError::Configuration("Missing".to_string()).status_code(),
            None
        );
    }

    #[test]
    fn test_retry_after() {
        let duration = Duration::from_secs(60);
        let err = NxuskitError::rate_limit(Some(duration));
        assert_eq!(err.retry_after(), Some(duration));

        let err = NxuskitError::Authentication("Bad key".to_string());
        assert_eq!(err.retry_after(), None);
    }

    #[test]
    fn test_error_messages() {
        let err = NxuskitError::Authentication("Invalid API key".to_string());
        assert_eq!(err.to_string(), "Authentication failed: Invalid API key");

        let err = NxuskitError::provider(500, "Internal server error");
        assert_eq!(
            err.to_string(),
            "Provider error (status 500): Internal server error"
        );

        let err = NxuskitError::rate_limit(Some(Duration::from_secs(30)));
        assert!(err.to_string().contains("Rate limit exceeded"));
    }

    #[test]
    fn test_clips_error_metadata() {
        let metadata = ClipsErrorMetadata {
            available_templates: Some(vec!["user".to_string(), "order".to_string()]),
            suggestions: Some(vec!["person".to_string()]),
            hint: Some("Did you mean 'user'?".to_string()),
            ..Default::default()
        };

        assert_eq!(metadata.available_templates.as_ref().unwrap().len(), 2);
        assert_eq!(metadata.suggestions.as_ref().unwrap()[0], "person");
        assert_eq!(metadata.hint.as_ref().unwrap(), "Did you mean 'user'?");
    }

    #[test]
    fn test_clips_template_schema_info() {
        let schema = ClipsTemplateSchemaInfo {
            name: "order".to_string(),
            slots: vec![
                ClipsSlotInfo {
                    name: "id".to_string(),
                    slot_type: "INTEGER".to_string(),
                    multislot: false,
                    default: None,
                    allowed_values: None,
                },
                ClipsSlotInfo {
                    name: "status".to_string(),
                    slot_type: "SYMBOL".to_string(),
                    multislot: false,
                    default: Some(serde_json::json!("pending")),
                    allowed_values: Some(vec![
                        "pending".to_string(),
                        "shipped".to_string(),
                        "delivered".to_string(),
                    ]),
                },
            ],
            documentation: Some("An order record".to_string()),
        };

        assert_eq!(schema.name, "order");
        assert_eq!(schema.slots.len(), 2);
        assert_eq!(schema.slots[1].allowed_values.as_ref().unwrap().len(), 3);
    }

    #[test]
    fn test_image_error_messages() {
        let err = NxuskitError::ImageTooLarge {
            size: 25_000_000,
            limit: 20_000_000,
            provider: "openai".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("25000000 bytes"));
        assert!(msg.contains("20000000 bytes"));
        assert!(msg.contains("openai"));

        let err = NxuskitError::InvalidImageFormat("unknown".to_string());
        assert!(err.to_string().contains("Invalid image format"));

        let err = NxuskitError::ImageFileNotFound("/path/to/image.png".to_string());
        assert!(err.to_string().contains("Image file not found"));
    }

    #[test]
    fn test_convenience_errors() {
        let err = NxuskitError::ProviderDetectionFailed(
            "llama".to_string(),
            "openai, claude, ollama".to_string(),
        );
        assert!(err.to_string().contains("Could not detect provider"));
        assert!(err.to_string().contains("llama"));

        let err =
            NxuskitError::ModelNotRecognized("gpt-5".to_string(), "gpt-4o, claude-3".to_string());
        assert!(err.to_string().contains("not recognized"));

        let err =
            NxuskitError::MissingCredentials("openai".to_string(), "OPENAI_API_KEY".to_string());
        assert!(err.to_string().contains("Missing API credentials"));
        assert!(err.to_string().contains("OPENAI_API_KEY"));

        let err = NxuskitError::InvalidModelSpecifier("invalid//format".to_string());
        assert!(err.to_string().contains("Invalid model specifier"));
    }

    #[test]
    fn test_license_required_error() {
        let err = NxuskitError::license_required("MCP provider");
        assert!(err.to_string().contains("requires a nxusKit Pro license"));
        assert!(err.to_string().contains("MCP provider"));
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_stream_error() {
        let err = NxuskitError::Stream("connection reset".to_string());
        assert_eq!(err.to_string(), "Stream error: connection reset");
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_configuration_error() {
        let err = NxuskitError::Configuration("Missing required field".to_string());
        assert!(err.to_string().contains("Configuration error"));
        assert_eq!(err.status_code(), None);
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_invalid_request_error() {
        let err = NxuskitError::InvalidRequest("messages cannot be empty".to_string());
        assert!(err.to_string().contains("Invalid request"));
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_clips_metadata_serialization() {
        let metadata = ClipsErrorMetadata {
            available_templates: Some(vec!["test".to_string()]),
            suggestions: None,
            hint: Some("Try this".to_string()),
            template_schemas: None,
            json_schema: Some(serde_json::json!({"type": "object"})),
        };

        let json = serde_json::to_string(&metadata).unwrap();
        assert!(json.contains("available_templates"));
        assert!(json.contains("hint"));
        // None fields should be skipped
        assert!(!json.contains("suggestions"));
        assert!(!json.contains("template_schemas"));
    }
}
