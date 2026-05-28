//! Token types and claim structures for nxusKit licensing.
//!
//! Pure data types — no I/O, no side effects. These are the canonical
//! representations of JWT claims for trial, developer, deployment,
//! real-purchase, and leased tokens.

use std::collections::HashMap;
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

// ── Token Type ────────────────────────────────────────────────────────

/// License token types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TokenType {
    Trial,
    Developer,
    Deployment,
    #[serde(rename = "real_purchase")]
    RealPurchase,
    Leased,
}

impl fmt::Display for TokenType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenType::Trial => write!(f, "trial"),
            TokenType::Developer => write!(f, "developer"),
            TokenType::Deployment => write!(f, "deployment"),
            TokenType::RealPurchase => write!(f, "real_purchase"),
            TokenType::Leased => write!(f, "leased"),
        }
    }
}

// ── Token Claims ──────────────────────────────────────────────────────

/// JWT claims structure for all nxusKit license tokens.
///
/// Uses standard JWT registered claim names (`iss`, `iat`, `exp`, `nbf`)
/// plus custom claims for licensing semantics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenClaims {
    // Standard JWT registered claims
    /// Issuer — must be "nxus-licensing"
    pub iss: String,
    /// Issued-at (Unix timestamp)
    pub iat: i64,
    /// Not-before (Unix timestamp). Present for trial and developer tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nbf: Option<i64>,
    /// Expiration (Unix timestamp). Absent for deployment tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp: Option<i64>,

    // Custom claims
    /// Product identifier (e.g., "nxuskit"). Required for v0.9.1+ token format.
    #[serde(default = "default_nxuskit_product_id")]
    pub product_id: String,
    /// Token type: trial, developer, deployment, real_purchase, or leased.
    #[serde(rename = "type")]
    pub token_type: TokenType,
    /// Edition — always "pro" for v0.9.1
    pub edition: String,
    /// Organization identifier (developer/deployment tokens)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    /// Machine fingerprint "sha256:..." (trial/developer tokens)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub machine_id: Option<String>,
    /// Seat index 1-3 (developer tokens)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seat_index: Option<u8>,
    /// Whether trial has been activated (trial tokens)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activated: Option<bool>,
    /// Major.minor version ceiling, e.g. "1.0" (deployment tokens)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sdk_version_ceiling: Option<String>,
    /// Customer email for audit trail (deployment tokens)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_email: Option<String>,
}

impl TokenClaims {
    /// Check whether this token is expired at the current system time.
    pub fn is_expired(&self) -> bool {
        self.is_expired_at(current_unix_timestamp())
    }

    /// Check whether this token is expired at a given Unix timestamp.
    pub fn is_expired_at(&self, now: i64) -> bool {
        match self.exp {
            Some(exp) => now > exp,
            None => false, // Deployment tokens have no expiry
        }
    }

    /// Days remaining until expiry. Returns None for non-expiring tokens.
    pub fn days_remaining(&self) -> Option<i64> {
        self.days_remaining_at(current_unix_timestamp())
    }

    /// Days remaining until expiry at a given Unix timestamp.
    pub fn days_remaining_at(&self, now: i64) -> Option<i64> {
        self.exp.map(|exp| (exp - now) / 86400)
    }

    /// Whether this trial token is suspended (not activated within 7 days).
    pub fn is_trial_suspended(&self) -> bool {
        self.is_trial_suspended_at(current_unix_timestamp())
    }

    /// Whether this trial token is suspended at a given Unix timestamp.
    pub fn is_trial_suspended_at(&self, now: i64) -> bool {
        if self.token_type != TokenType::Trial {
            return false;
        }
        let activated = self.activated.unwrap_or(false);
        if activated {
            return false;
        }
        // Suspended if more than 7 days since issuance without activation
        let grace_deadline = self.iat + (7 * 86400);
        now > grace_deadline
    }
}

// ── Token Source ──────────────────────────────────────────────────────

/// Where a token was resolved from in the resolution chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenSourceKind {
    /// Resolved from `NXUSKIT_LICENSE_TOKEN` environment variable
    EnvironmentVariable,
    /// Resolved from `~/.nxuskit/license.token` file
    TokenFile,
    /// Passed via API parameter at runtime
    ApiParameter,
    /// No token found from any source
    None,
}

impl fmt::Display for TokenSourceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenSourceKind::EnvironmentVariable => write!(f, "env_var"),
            TokenSourceKind::TokenFile => write!(f, "file"),
            TokenSourceKind::ApiParameter => write!(f, "api_param"),
            TokenSourceKind::None => write!(f, "none"),
        }
    }
}

// ── Token Resolution ─────────────────────────────────────────────────

/// Result of the token resolution chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenResolution {
    /// Where the token was found (or None)
    pub source: TokenSourceKind,
    /// The raw JWT string, if found
    pub raw_token: Option<String>,
    /// Validated claims, if token was valid
    pub claims: Option<TokenClaims>,
    /// Whether the token passed validation
    pub valid: bool,
    /// Error message if validation failed
    pub error: Option<String>,
}

// ── Version Ceiling ──────────────────────────────────────────────────

/// Major.minor version constraint on deployment tokens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionCeiling {
    pub major: u32,
    pub minor: u32,
}

impl VersionCeiling {
    /// Parse a version ceiling from a "major.minor" string.
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 2 {
            return Option::None;
        }
        let major = parts[0].parse().ok()?;
        let minor = parts[1].parse().ok()?;
        Some(VersionCeiling { major, minor })
    }

    /// Check whether the given SDK version is within this ceiling.
    /// Patch version is ignored — only major.minor is compared.
    pub fn allows_version(&self, sdk_major: u32, sdk_minor: u32) -> bool {
        (sdk_major, sdk_minor) <= (self.major, self.minor)
    }
}

impl fmt::Display for VersionCeiling {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

// ── Activation Result ────────────────────────────────────────────────

/// Result of a license activation call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivationResult {
    pub success: bool,
    pub seats_used: u32,
    pub seats_total: u32,
    pub token: Option<String>,
    pub deployment_token: Option<String>,
    pub message: String,
    pub error: Option<String>,
}

/// Result of a license deactivation call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeactivationResult {
    pub success: bool,
    pub seats_used: u32,
    pub seats_total: u32,
    pub message: String,
    pub error: Option<String>,
}

/// Result of a trial issuance call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrialIssuanceResult {
    pub success: bool,
    pub token: Option<String>,
    pub days_remaining: u32,
    pub message: String,
    pub error: Option<String>,
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Get the current Unix timestamp.
pub(crate) fn current_unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

// ── ValidatedClaims ──────────────────────────────────────────────────

/// Verified token claims produced by a `TokenVerifier` implementation.
///
/// Extends `TokenClaims` with product-aware and entitlement-override fields
/// from the `external licensing client` crate's `ValidatedClaims` shape.
///
/// # Examples
///
/// ```
/// # use nxuskit_core::license_types::{ValidatedClaims, TokenType};
/// let claims = ValidatedClaims {
///     product_id: "nxuskit".to_string(),
///     token_type: TokenType::Developer,
///     edition: "pro".to_string(),
///     iss: "nxus-licensing".to_string(),
///     iat: 1710547200,
///     nbf: None,
///     exp: Some(1742572800),
///     tenant_id: Some("org-123".to_string()),
///     machine_id: None,
///     seat_index: None,
///     activated: None,
///     sdk_version_ceiling: None,
///     customer_email: None,
///     limits_override: None,
///     features_override: None,
///     environment: Some("production".to_string()),
/// };
/// assert_eq!(claims.product_id, "nxuskit");
/// assert_eq!(claims.edition, "pro");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatedClaims {
    /// Product identifier — must be `"nxuskit"` for this SDK.
    #[serde(default = "default_product_id")]
    pub product_id: String,

    /// Token type: trial, developer, deployment, real-purchase, or leased.
    #[serde(rename = "type")]
    pub token_type: TokenType,

    /// Edition: `"community"`, `"pro"`, or `"enterprise"`.
    pub edition: String,

    // Standard JWT registered claims
    pub iss: String,
    pub iat: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nbf: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp: Option<i64>,

    // Custom claims (same as TokenClaims)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub machine_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seat_index: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sdk_version_ceiling: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_email: Option<String>,

    // Entitlement overrides (new for 055)
    /// Per-token numerical limit overrides. Token values take precedence
    /// over catalog defaults for recognized keys.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limits_override: Option<HashMap<String, serde_json::Value>>,

    /// Per-token feature grants, unioned with edition-inherited features.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub features_override: Option<Vec<String>>,

    /// Issuing environment: production, staging, development, or test.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
}

fn default_product_id() -> String {
    "nxuskit".to_string()
}

fn default_nxuskit_product_id() -> String {
    "nxuskit".to_string()
}

impl From<TokenClaims> for ValidatedClaims {
    fn from(claims: TokenClaims) -> Self {
        Self {
            product_id: claims.product_id.clone(),
            token_type: claims.token_type,
            edition: claims.edition,
            iss: claims.iss,
            iat: claims.iat,
            nbf: claims.nbf,
            exp: claims.exp,
            tenant_id: claims.tenant_id,
            machine_id: claims.machine_id,
            seat_index: claims.seat_index,
            activated: claims.activated,
            sdk_version_ceiling: claims.sdk_version_ceiling,
            customer_email: claims.customer_email,
            limits_override: None,
            features_override: None,
            environment: None,
        }
    }
}

impl ValidatedClaims {
    /// Convert to `TokenClaims` for backward compatibility with existing code.
    pub fn to_token_claims(&self) -> TokenClaims {
        TokenClaims {
            product_id: self.product_id.clone(),
            iss: self.iss.clone(),
            iat: self.iat,
            nbf: self.nbf,
            exp: self.exp,
            token_type: self.token_type,
            edition: self.edition.clone(),
            tenant_id: self.tenant_id.clone(),
            machine_id: self.machine_id.clone(),
            seat_index: self.seat_index,
            activated: self.activated,
            sdk_version_ceiling: self.sdk_version_ceiling.clone(),
            customer_email: self.customer_email.clone(),
        }
    }
}

// ── VerifyError ──────────────────────────────────────────────────────

/// Token verification errors.
///
/// Produced by `TokenVerifier` implementations. Maps to `LicenseError`
/// at the license.rs boundary for backward compatibility.
#[derive(Debug, Clone)]
pub enum VerifyError {
    /// JWT signature is invalid or algorithm is unsupported.
    InvalidSignature { details: String },
    /// Issuer claim does not match `"nxus-licensing"`.
    InvalidIssuer,
    /// Product ID claim does not match `"nxuskit"`.
    InvalidProductId { expected: String, actual: String },
    /// JWT structure is malformed.
    MalformedToken { details: String },
    /// Public key loading or decoding failure.
    KeyError(String),
    /// General validation failure.
    ValidationFailed(String),
}

impl fmt::Display for VerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VerifyError::InvalidSignature { details } => {
                write!(f, "Invalid token signature: {details}")
            }
            VerifyError::InvalidIssuer => write!(f, "Invalid token issuer"),
            VerifyError::InvalidProductId { expected, actual } => {
                write!(
                    f,
                    "Token issued for product '{actual}', expected '{expected}'"
                )
            }
            VerifyError::MalformedToken { details } => {
                write!(f, "Malformed JWT: {details}")
            }
            VerifyError::KeyError(msg) => write!(f, "Key error: {msg}"),
            VerifyError::ValidationFailed(msg) => {
                write!(f, "Validation failed: {msg}")
            }
        }
    }
}

impl std::error::Error for VerifyError {}

// ── TokenVerifier Trait ──────────────────────────────────────────────

/// Abstracted token verification interface.
///
/// Separates signature verification from business logic (expiry, machine
/// binding, version ceiling). Implementations:
/// - `StubTokenVerifier`: Local ES256 verification via `jsonwebtoken` crate
/// - `ClientCrateVerifier`: Wraps `external licensing client::TokenValidator` (future)
pub trait TokenVerifier: Send + Sync {
    /// Verify a JWT string and extract validated claims.
    ///
    /// Only performs signature verification and claim extraction.
    /// Does NOT check expiry, machine binding, or version ceiling.
    fn verify(&self, jwt_str: &str) -> Result<ValidatedClaims, VerifyError>;

    /// Human-readable name of this verifier implementation.
    fn name(&self) -> &str;
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_token_type_serde() {
        let tt = TokenType::Trial;
        let json = serde_json::to_string(&tt).unwrap();
        assert_eq!(json, r#""trial""#);

        let parsed: TokenType = serde_json::from_str(r#""deployment""#).unwrap();
        assert_eq!(parsed, TokenType::Deployment);
    }

    #[test]
    fn test_token_type_display() {
        assert_eq!(TokenType::Trial.to_string(), "trial");
        assert_eq!(TokenType::Developer.to_string(), "developer");
        assert_eq!(TokenType::Deployment.to_string(), "deployment");
        assert_eq!(TokenType::RealPurchase.to_string(), "real_purchase");
        assert_eq!(TokenType::Leased.to_string(), "leased");
    }

    #[test]
    fn test_version_ceiling_parse() {
        let vc = VersionCeiling::parse("0.9").unwrap();
        assert_eq!(vc.major, 0);
        assert_eq!(vc.minor, 9);

        assert!(VersionCeiling::parse("bad").is_none());
        assert!(VersionCeiling::parse("1.2.3").is_none());
        assert!(VersionCeiling::parse("").is_none());
    }

    #[test]
    fn test_version_ceiling_allows() {
        let vc = VersionCeiling { major: 0, minor: 9 };
        assert!(vc.allows_version(0, 9)); // exact match
        assert!(vc.allows_version(0, 8)); // lower minor
        assert!(!vc.allows_version(0, 10)); // higher minor
        assert!(!vc.allows_version(1, 0)); // higher major
    }

    #[test]
    fn test_token_claims_expiry() {
        let claims = TokenClaims {
            product_id: "nxuskit".to_string(),
            iss: "nxus-licensing".to_string(),
            iat: 1000000,
            nbf: Some(1000000),
            exp: Some(2000000),
            token_type: TokenType::Developer,
            edition: "pro".to_string(),
            tenant_id: Some("org-1".to_string()),
            machine_id: Some("sha256:abc".to_string()),
            seat_index: Some(1),
            activated: None,
            sdk_version_ceiling: None,
            customer_email: None,
        };

        assert!(!claims.is_expired_at(1500000)); // before expiry
        assert!(claims.is_expired_at(2500000)); // after expiry
        assert_eq!(claims.days_remaining_at(1000000), Some(11)); // ~11 days
    }

    #[test]
    fn test_deployment_token_never_expires() {
        let claims = TokenClaims {
            product_id: "nxuskit".to_string(),
            iss: "nxus-licensing".to_string(),
            iat: 1000000,
            nbf: None,
            exp: None, // No expiry
            token_type: TokenType::Deployment,
            edition: "pro".to_string(),
            tenant_id: Some("org-1".to_string()),
            machine_id: None,
            seat_index: None,
            activated: None,
            sdk_version_ceiling: Some("1.0".to_string()),
            customer_email: Some("dev@co.com".to_string()),
        };

        assert!(!claims.is_expired_at(i64::MAX));
        assert_eq!(claims.days_remaining_at(1000000), None);
    }

    #[test]
    fn test_trial_suspension() {
        let claims = TokenClaims {
            product_id: "nxuskit".to_string(),
            iss: "nxus-licensing".to_string(),
            iat: 1000000,
            nbf: Some(1000000),
            exp: Some(1000000 + 30 * 86400),
            token_type: TokenType::Trial,
            edition: "pro".to_string(),
            tenant_id: None,
            machine_id: Some("sha256:abc".to_string()),
            seat_index: None,
            activated: Some(false),
            sdk_version_ceiling: None,
            customer_email: None,
        };

        // Within 7-day grace: not suspended
        assert!(!claims.is_trial_suspended_at(1000000 + 3 * 86400));
        // After 7-day grace: suspended
        assert!(claims.is_trial_suspended_at(1000000 + 8 * 86400));
    }

    #[test]
    fn test_activated_trial_not_suspended() {
        let claims = TokenClaims {
            product_id: "nxuskit".to_string(),
            iss: "nxus-licensing".to_string(),
            iat: 1000000,
            nbf: Some(1000000),
            exp: Some(1000000 + 30 * 86400),
            token_type: TokenType::Trial,
            edition: "pro".to_string(),
            tenant_id: None,
            machine_id: Some("sha256:abc".to_string()),
            seat_index: None,
            activated: Some(true),
            sdk_version_ceiling: None,
            customer_email: None,
        };

        // Even after 7 days, activated trial is not suspended
        assert!(!claims.is_trial_suspended_at(1000000 + 8 * 86400));
    }

    #[test]
    fn test_token_source_display() {
        assert_eq!(TokenSourceKind::EnvironmentVariable.to_string(), "env_var");
        assert_eq!(TokenSourceKind::TokenFile.to_string(), "file");
        assert_eq!(TokenSourceKind::ApiParameter.to_string(), "api_param");
        assert_eq!(TokenSourceKind::None.to_string(), "none");
    }

    #[test]
    fn test_token_claims_json_roundtrip() {
        let claims = TokenClaims {
            product_id: "nxuskit".to_string(),
            iss: "nxus-licensing".to_string(),
            iat: 1710547200,
            nbf: Some(1710547200),
            exp: Some(1713139200),
            token_type: TokenType::Developer,
            edition: "pro".to_string(),
            tenant_id: Some("org-123".to_string()),
            machine_id: Some("sha256:a1b2c3d4".to_string()),
            seat_index: Some(2),
            activated: None,
            sdk_version_ceiling: None,
            customer_email: None,
        };

        let json = serde_json::to_string(&claims).unwrap();
        assert!(json.contains(r#""type":"developer""#));
        assert!(json.contains(r#""iss":"nxus-licensing""#));
        assert!(!json.contains("sdk_version_ceiling")); // skipped when None

        let parsed: TokenClaims = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.token_type, TokenType::Developer);
        assert_eq!(parsed.seat_index, Some(2));
    }
}
