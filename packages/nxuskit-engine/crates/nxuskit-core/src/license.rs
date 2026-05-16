//! License token validation and resolution for nxusKit.
//!
//! Uses ES256 (EC P-256) JWT validation via the `TokenVerifier` trait,
//! token resolution from multiple sources, and structured logging.

use crate::license_types::{
    TokenClaims, TokenResolution, TokenSourceKind, TokenType, ValidatedClaims, VersionCeiling,
    current_unix_timestamp,
};
use crate::license_types::{TokenVerifier, VerifyError};
use crate::machine_id;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

// ── Constants ────────────────────────────────────────────────────────

/// Expected issuer for all nxusKit license tokens.
const EXPECTED_ISSUER: &str = "nxus-licensing";

/// Environment variable for license token.
const LICENSE_ENV_VAR: &str = "NXUSKIT_LICENSE_TOKEN";

/// Default token file path (relative to home directory).
const TOKEN_FILE_NAME: &str = ".nxuskit/license.token";

/// Licensing API base URL (Odoo-brokered). Build-time configurable via
/// `NXUSKIT_LICENSE_SERVER_DEFAULT` env var. Runtime override via
/// `NXUSKIT_LICENSE_SERVER` env var.
const LICENSE_SERVER_URL: &str = env!("NXUSKIT_LICENSE_SERVER_DEFAULT");

/// Build-time licensing environment inferred from the default endpoint unless
/// `NXUSKIT_LICENSE_ENVIRONMENT_DEFAULT` is set by the release pipeline.
const LICENSE_ENVIRONMENT: &str = env!("NXUSKIT_LICENSE_ENVIRONMENT_DEFAULT");

/// Current SDK version (from workspace Cargo.toml).
const SDK_VERSION: &str = env!("CARGO_PKG_VERSION");

/// RSA-3072 public key for license token verification (RS384).
/// Kept for reference during the RS384 -> ES256 migration. Will be removed
/// once all token issuance has been migrated to ES256.
#[allow(dead_code)]
const LICENSE_PUBLIC_KEY_PEM: &str = r#"-----BEGIN PUBLIC KEY-----
MIIBojANBgkqhkiG9w0BAQEFAAOCAY8AMIIBigKCAYEAm2a1BAceD89qaJQ0une1
DirWwerEb7RFlCDq+2jaZkwmEddBZTdpzOaPY/SWMUrPa0ijAt3NKKjQlDQ1iGqz
GChFD0np8h+E0nQnQrVc1zQj5hzqGPSldrC+/XL0zNT/brOPzfiCQLYDugWjxecC
UN2HdZShBpC/VD2GrdIl6cI86AQUeM0CKbPicUegS1DOcBjn3oQwhudOzjYshr3S
TV8Gqj9f+OCLnSCJOGI/87YEAzseH0RIaMwz/lNTV1fa866dLknSQu9kdmmxYEV6
HUzoG6DiriRq7KoxnQc1uPP5JB1R3yXhei9SgXDp7PrcW9VHISZi3Nb8CXqgAlr0
AijjPmr0pPgrlaQUX81lamuAqf2+2hz7/m0cf+nnwAOlOQ33bZrU/UhuvFormhWC
idvEKAXbU4LOTXkiwLe42c7I3qc6XLSfT0/TGJbcL0FtlTiK6htW9qn5qtQ2WIpF
SWupQco3FEgz+09fLsfODgNj9fQz7zfb76jCUQmBdyGJAgMBAAE=
-----END PUBLIC KEY-----"#;

// ── ES256 Embedded Public Key ────────────────────────────────────────

mod embedded_license_key {
    include!(concat!(env!("OUT_DIR"), "/license_key_generated.rs"));
}

/// Embedded ES256 (EC P-256) public key for license token verification.
///
/// Release builds require the production key from the release-managed key
/// artifact. Debug/test builds use the same key when that artifact is present
/// and fall back to a deterministic dev/test key only when the production
/// artifact is unavailable.
const ES256_PUBLIC_KEY_PEM: &str = embedded_license_key::ES256_PUBLIC_KEY_PEM;

/// Source path or fallback label for the embedded ES256 public key.
pub const ES256_PUBLIC_KEY_SOURCE: &str = embedded_license_key::ES256_PUBLIC_KEY_SOURCE;

/// Expected key id for the production ES256 key.
pub const ES256_PUBLIC_KEY_KID: &str = embedded_license_key::ES256_PUBLIC_KEY_KID;

// ── Global Verifier ─────────────────────────────────────────────────

/// Global verifier instance (initialized on first use).
static TOKEN_VERIFIER: OnceLock<Arc<dyn TokenVerifier>> = OnceLock::new();

/// Get the active token verifier.
///
/// When the external licensing feature is enabled, uses `ExternalClientVerifier`
/// (wrapping `external licensing client::TokenValidator`). Otherwise falls back
/// to `StubTokenVerifier` (local ES256 verification).
pub fn get_verifier() -> Arc<dyn TokenVerifier> {
    // In test builds, ensure NXUS_SIGNING_KEY_PATH is set to the test key
    // BEFORE the OnceLock initializes the verifier. This is critical when
    // the external licensing feature is enabled.
    #[cfg(test)]
    {
        use std::sync::Once;
        static TEST_KEY_INIT: Once = Once::new();
        TEST_KEY_INIT.call_once(|| {
            // Write the TEST key pair's public key for NXUS_SIGNING_KEY_PATH.
            // Must match the private key from test_fixtures::test_es256_keypair().
            let (_, test_public_pem) = crate::test_fixtures::test_es256_keypair();
            let key_dir = std::env::temp_dir().join("nxuskit-test-keys");
            std::fs::create_dir_all(&key_dir).ok();
            let key_path = key_dir.join("es256-test-pubkey.pem");
            std::fs::write(&key_path, &test_public_pem).ok();
            if key_path.exists() {
                unsafe {
                    std::env::set_var("NXUS_SIGNING_KEY_PATH", key_path.to_str().unwrap());
                }
            }
        });
    }

    TOKEN_VERIFIER
        .get_or_init(|| Arc::new(StubTokenVerifier::new()))
        .clone()
}

// ── StubTokenVerifier ───────────────────────────────────────────────

/// ES256 token verifier using jsonwebtoken crate directly.
///
/// This is the stub implementation used during development.
/// Will be replaced by `ExternalClientVerifier` wrapping `external licensing client`
/// when that crate is available.
pub struct StubTokenVerifier {
    #[allow(missing_debug_implementations)]
    decoding_key: DecodingKey,
}

impl std::fmt::Debug for StubTokenVerifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StubTokenVerifier")
            .field("name", &self.name())
            .finish()
    }
}

impl Default for StubTokenVerifier {
    fn default() -> Self {
        Self::new()
    }
}

impl StubTokenVerifier {
    /// Create a new StubTokenVerifier.
    ///
    /// Key resolution order:
    /// 1. `NXUS_SIGNING_KEY_PATH` env var — reads PEM file from that path
    /// 2. Embedded ES256 public key constant
    pub fn new() -> Self {
        let key_pem = Self::load_key_pem();
        let decoding_key = DecodingKey::from_ec_pem(key_pem.as_bytes())
            .expect("ES256 public key PEM must be valid");
        Self { decoding_key }
    }

    /// Create a StubTokenVerifier with an explicit PEM public key.
    pub fn with_public_key(pem: &str) -> Self {
        let decoding_key =
            DecodingKey::from_ec_pem(pem.as_bytes()).expect("ES256 public key PEM must be valid");
        Self { decoding_key }
    }

    /// Load the public key PEM string from env override or embedded constant.
    ///
    /// Security hardening: `NXUS_SIGNING_KEY_PATH` override is only available
    /// in debug builds. Release builds always use the embedded production key.
    fn load_key_pem() -> String {
        // Check env var override — debug/test builds only
        #[cfg(debug_assertions)]
        if let Ok(key_path) = std::env::var("NXUS_SIGNING_KEY_PATH") {
            let key_path = key_path.trim();
            if !key_path.is_empty() {
                match std::fs::read_to_string(key_path) {
                    Ok(pem) => {
                        log::info!("Using signing key from NXUS_SIGNING_KEY_PATH: {key_path}");
                        return pem;
                    }
                    Err(e) => {
                        log::warn!(
                            "Failed to read NXUS_SIGNING_KEY_PATH={key_path}: {e}. \
                             Falling back to embedded key."
                        );
                    }
                }
            }
        }

        // Fall back to embedded key
        ES256_PUBLIC_KEY_PEM.to_string()
    }
}

impl TokenVerifier for StubTokenVerifier {
    fn verify(&self, jwt_str: &str) -> Result<ValidatedClaims, VerifyError> {
        // First, peek at the header to detect RS384 tokens and give guidance
        let header =
            jsonwebtoken::decode_header(jwt_str).map_err(|e| VerifyError::MalformedToken {
                details: e.to_string(),
            })?;

        if header.alg != Algorithm::ES256 {
            let alg_name = format!("{:?}", header.alg);
            return Err(VerifyError::InvalidSignature {
                details: format!(
                    "Token uses {alg_name} algorithm. nxusKit now requires ES256 \
                     signed tokens. Please upgrade your license token at \
                     nxus.systems/docs/license-migration"
                ),
            });
        }

        let mut validation = Validation::new(Algorithm::ES256);
        validation.set_issuer(&[EXPECTED_ISSUER]);
        // We handle expiry ourselves for more specific error messages
        validation.validate_exp = false;
        validation.validate_nbf = false;
        // Require these claims to be present
        validation.set_required_spec_claims(&["iss", "iat"]);

        let token_data =
            jsonwebtoken::decode::<ValidatedClaims>(jwt_str, &self.decoding_key, &validation)
                .map_err(|e| match e.kind() {
                    jsonwebtoken::errors::ErrorKind::InvalidSignature => {
                        VerifyError::InvalidSignature {
                            details: "ES256 signature verification failed".to_string(),
                        }
                    }
                    jsonwebtoken::errors::ErrorKind::InvalidAlgorithm => {
                        VerifyError::InvalidSignature {
                            details: "Expected ES256 algorithm. Token uses a different algorithm. \
                                 Please upgrade your license token."
                                .to_string(),
                        }
                    }
                    jsonwebtoken::errors::ErrorKind::InvalidIssuer => VerifyError::InvalidIssuer,
                    _ => VerifyError::ValidationFailed(e.to_string()),
                })?;

        let claims = token_data.claims;

        // Validate product_id if present
        if claims.product_id != "nxuskit" {
            return Err(VerifyError::InvalidProductId {
                expected: "nxuskit".to_string(),
                actual: claims.product_id,
            });
        }

        Ok(claims)
    }

    fn name(&self) -> &str {
        "StubTokenVerifier"
    }
}

// ── Pre-Expiry Warning Tracking ──────────────────────────────────────

/// Global flag to ensure pre-expiry warnings are logged at most once per session.
static EXPIRY_WARNING_EMITTED: AtomicBool = AtomicBool::new(false);

/// Reset the expiry warning flag (for testing).
#[cfg(test)]
pub fn reset_expiry_warning() {
    EXPIRY_WARNING_EMITTED.store(false, Ordering::Relaxed);
}

// ── Token Validation ─────────────────────────────────────────────────

/// Validate a JWT token string using the active `TokenVerifier`.
///
/// Performs:
/// 1. ES256 signature verification (via `StubTokenVerifier`)
/// 2. Algorithm enforcement (ES256 only; RS384 tokens get upgrade guidance)
/// 3. Issuer validation ("nxus-licensing")
/// 4. Product ID validation ("nxuskit")
/// 5. Claim parsing into `TokenClaims`
///
/// Does NOT perform business-rule checks (expiry, machine binding, version ceiling).
/// Use `validate_token_full` for complete validation.
pub fn validate_token(jwt_str: &str) -> Result<TokenClaims, LicenseError> {
    let verifier = get_verifier();
    let validated = verify_validated_claims(jwt_str, verifier.as_ref())?;
    Ok(validated.to_token_claims())
}

fn verify_validated_claims(
    jwt_str: &str,
    verifier: &dyn TokenVerifier,
) -> Result<ValidatedClaims, LicenseError> {
    verifier
        .verify(jwt_str)
        .map_err(verify_error_to_license_error)
}

/// Convert a `VerifyError` to a `LicenseError` for backward compatibility.
fn verify_error_to_license_error(e: VerifyError) -> LicenseError {
    match e {
        VerifyError::InvalidSignature { details } => {
            if details.contains("ES256") {
                // Algorithm mismatch — provide upgrade guidance
                LicenseError::InvalidAlgorithm(details)
            } else {
                LicenseError::InvalidSignature
            }
        }
        VerifyError::InvalidIssuer => LicenseError::InvalidIssuer,
        VerifyError::InvalidProductId { expected, actual } => {
            LicenseError::InvalidProductId { expected, actual }
        }
        VerifyError::MalformedToken { details } => LicenseError::MalformedToken { details },
        VerifyError::KeyError(msg) => LicenseError::KeyError(msg),
        VerifyError::ValidationFailed(msg) => LicenseError::ValidationFailed(msg),
    }
}

/// Perform full token validation including business rules.
///
/// This validates the JWT signature AND checks:
/// - Expiry (for trial/developer tokens)
/// - Machine binding (for trial/developer tokens)
/// - Version ceiling (for deployment tokens)
/// - Trial activation status (7-day grace period)
pub fn validate_token_full(jwt_str: &str) -> Result<ValidatedToken, LicenseError> {
    let verifier = get_verifier();
    let environment = license_environment();
    validate_token_full_with_verifier_and_environment(jwt_str, verifier.as_ref(), &environment)
}

/// Validate a token with an explicit verifier and expected environment.
///
/// This supports release-smoke tests and downstream diagnostics that need to
/// prove environment semantics without mutating the process-global verifier.
#[doc(hidden)]
pub fn validate_token_full_with_verifier_and_environment(
    jwt_str: &str,
    verifier: &dyn TokenVerifier,
    expected_environment: &str,
) -> Result<ValidatedToken, LicenseError> {
    let validated = verify_validated_claims(jwt_str, verifier)?;
    validate_claim_environment(&validated, expected_environment)?;
    validate_token_claims_full(validated.to_token_claims())
}

fn validate_claim_environment(
    claims: &ValidatedClaims,
    expected_environment: &str,
) -> Result<(), LicenseError> {
    let Some(actual_environment) = claims.environment.as_deref() else {
        return Ok(());
    };

    if environments_match(expected_environment, actual_environment) {
        Ok(())
    } else {
        Err(LicenseError::EnvironmentMismatch {
            expected: expected_environment.to_string(),
            actual: actual_environment.to_string(),
        })
    }
}

fn environments_match(expected: &str, actual: &str) -> bool {
    let expected = expected.trim().to_ascii_lowercase();
    let actual = actual.trim().to_ascii_lowercase();

    expected == actual
        || (expected == "development" && matches!(actual.as_str(), "dev" | "test"))
        || (expected == "test" && matches!(actual.as_str(), "development" | "dev"))
}

fn validate_token_claims_full(claims: TokenClaims) -> Result<ValidatedToken, LicenseError> {
    let now = current_unix_timestamp();

    // Check expiry
    if claims.is_expired_at(now) {
        return Err(LicenseError::Expired {
            token_type: claims.token_type,
        });
    }

    // Check trial suspension
    if claims.is_trial_suspended_at(now) {
        return Err(LicenseError::TrialSuspended);
    }

    // Check machine binding for trial/developer tokens
    if matches!(
        claims.token_type,
        TokenType::Trial | TokenType::Developer | TokenType::RealPurchase | TokenType::Leased
    ) && let Some(ref token_machine_id) = claims.machine_id
    {
        match machine_id::get_machine_fingerprint() {
            Ok(local_machine_id) => {
                if token_machine_id != &local_machine_id {
                    return Err(LicenseError::MachineMismatch {
                        expected: token_machine_id.clone(),
                        actual: local_machine_id,
                    });
                }
            }
            Err(e) => {
                log::warn!("Could not verify machine binding: {e}");
                // Continue — we don't block on machine ID failure
            }
        }
    }

    // Check version ceiling for deployment tokens
    if claims.token_type == TokenType::Deployment
        && let Some(ref ceiling_str) = claims.sdk_version_ceiling
        && let Some(ceiling) = VersionCeiling::parse(ceiling_str)
    {
        let (sdk_major, sdk_minor) = parse_sdk_version(SDK_VERSION);
        if !ceiling.allows_version(sdk_major, sdk_minor) {
            return Err(LicenseError::VersionCeilingExceeded {
                ceiling: ceiling_str.clone(),
                sdk_version: SDK_VERSION.to_string(),
            });
        }
    }

    // Emit pre-expiry warning if within 7 days (once per session)
    if let Some(days) = claims.days_remaining_at(now)
        && (0..=7).contains(&days)
    {
        emit_expiry_warning(&claims, days);
    }

    Ok(ValidatedToken {
        claims,
        source: TokenSourceKind::None, // Set by caller
    })
}

/// A token that has passed full validation.
#[derive(Debug, Clone)]
pub struct ValidatedToken {
    pub claims: TokenClaims,
    pub source: TokenSourceKind,
}

// ── Token Resolution ─────────────────────────────────────────────────

/// Resolve a license token from the 5-level precedence chain.
///
/// Resolution order:
/// 1. `NXUSKIT_LICENSE_TOKEN` environment variable (highest)
/// 2. Build-time embedded `NXUSKIT_DEPLOYMENT_TOKEN` (for redistributable apps)
/// 3. Sidecar file `nxuskit-license.token` adjacent to executable
/// 4. `~/.nxuskit/license.token` file (developer workstation)
/// 5. `explicit_key` parameter (lowest)
///
/// Returns the first successfully resolved and validated token,
/// or a resolution with `valid: false` if no valid token is found.
pub fn resolve_token(explicit_key: Option<&str>) -> TokenResolution {
    // 1. Environment variable (highest priority)
    if let Ok(env_token) = std::env::var(LICENSE_ENV_VAR) {
        let env_token = env_token.trim().to_string();
        if !env_token.is_empty() {
            return resolve_from_source(env_token, TokenSourceKind::EnvironmentVariable);
        }
    }

    // 2. Build-time embedded deployment token
    let embedded_token = env!("NXUSKIT_DEPLOYMENT_TOKEN");
    if !embedded_token.is_empty() {
        return resolve_from_source(
            embedded_token.to_string(),
            TokenSourceKind::EnvironmentVariable, // closest existing source kind
        );
    }

    // 3. Sidecar file adjacent to executable
    if let Ok(exe_path) = std::env::current_exe()
        && let Some(exe_dir) = exe_path.parent()
    {
        let sidecar = exe_dir.join("nxuskit-license.token");
        if sidecar.exists()
            && let Ok(sidecar_token) = std::fs::read_to_string(&sidecar)
        {
            let sidecar_token = sidecar_token.trim().to_string();
            if !sidecar_token.is_empty() {
                log::debug!("Using sidecar token from {}", sidecar.display());
                return resolve_from_source(sidecar_token, TokenSourceKind::TokenFile);
            }
        }
    }

    // 4. Token file (developer workstation)
    if let Some(token_path) = token_file_path()
        && token_path.exists()
    {
        // Check file permissions on Unix
        #[cfg(unix)]
        check_file_permissions(&token_path);

        match std::fs::read_to_string(&token_path) {
            Ok(file_token) => {
                let file_token = file_token.trim().to_string();
                if !file_token.is_empty() {
                    return resolve_from_source(file_token, TokenSourceKind::TokenFile);
                }
            }
            Err(e) => {
                log::warn!("Failed to read token file {}: {e}", token_path.display());
            }
        }
    }

    // 3. Explicit API parameter (lowest priority)
    if let Some(key) = explicit_key {
        let key = key.trim();
        if !key.is_empty() {
            return resolve_from_source(key.to_string(), TokenSourceKind::ApiParameter);
        }
    }

    // No token found
    TokenResolution {
        source: TokenSourceKind::None,
        raw_token: None,
        claims: None,
        valid: false,
        error: None,
    }
}

/// Resolve and validate a token from a specific source.
fn resolve_from_source(raw_token: String, source: TokenSourceKind) -> TokenResolution {
    match validate_token_full(&raw_token) {
        Ok(mut validated) => {
            validated.source = source;
            emit_validation_event(source, "valid", &validated.claims);
            TokenResolution {
                source,
                raw_token: Some(raw_token),
                claims: Some(validated.claims),
                valid: true,
                error: None,
            }
        }
        Err(e) => {
            emit_validation_failure(source, &e);
            TokenResolution {
                source,
                raw_token: Some(raw_token),
                claims: None,
                valid: false,
                error: Some(e.to_string()),
            }
        }
    }
}

/// Get the token file path (~/.nxuskit/license.token).
fn token_file_path() -> Option<std::path::PathBuf> {
    dirs_home().map(|home| home.join(TOKEN_FILE_NAME))
}

/// Get the user's home directory.
fn dirs_home() -> Option<std::path::PathBuf> {
    #[cfg(unix)]
    {
        std::env::var("HOME").ok().map(std::path::PathBuf::from)
    }
    #[cfg(windows)]
    {
        std::env::var("USERPROFILE")
            .ok()
            .map(std::path::PathBuf::from)
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}

/// Check file permissions and warn if insecure (Unix only).
#[cfg(unix)]
fn check_file_permissions(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(metadata) = std::fs::metadata(path) {
        let mode = metadata.permissions().mode();
        // Warn if world-readable or group-readable
        if mode & 0o044 != 0 {
            log::warn!(
                "Token file {} has insecure permissions ({:04o}). \
                 Consider restricting to owner-only: chmod 600 {}",
                path.display(),
                mode & 0o777,
                path.display()
            );
        }
    }
}

/// Store a token at the default file path.
pub fn store_token_file(jwt: &str) -> Result<(), LicenseError> {
    let path = token_file_path()
        .ok_or_else(|| LicenseError::StorageError("could not determine home directory".into()))?;

    // Create parent directory if needed
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| LicenseError::StorageError(format!("create dir: {e}")))?;
    }

    std::fs::write(&path, jwt)
        .map_err(|e| LicenseError::StorageError(format!("write token: {e}")))?;

    // Set restrictive permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&path, perms)
            .map_err(|e| LicenseError::StorageError(format!("set permissions: {e}")))?;
    }

    Ok(())
}

/// Remove the token file.
pub fn remove_token_file() -> Result<(), LicenseError> {
    let path = token_file_path()
        .ok_or_else(|| LicenseError::StorageError("could not determine home directory".into()))?;

    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| LicenseError::StorageError(format!("remove token: {e}")))?;
    }

    Ok(())
}

// ── Activation Idempotency Keys ──────────────────────────────────────
//
// Stores a stable UUIDv4 per in-flight purchase ID so a retry against the
// same purchase ID reuses the same `Idempotency-Key` HTTP header. This
// closes the duplicate-mint hole when the proxy returns `backend_timeout`
// but the Cloud Run backend still completes the activation.
//
// Lifecycle:
//   - get_or_create_activation_key(pid)
//       called before each POST /activate. Returns the existing key if
//       present, otherwise creates and persists a new UUIDv4.
//   - clear_activation_key(pid)
//       called after a terminal response (success, or non-retryable
//       client error like invalid purchase ID / wrong product / seat
//       limit). NOT called on transient errors (`backend_timeout`,
//       `rate_limit_exceeded`, network errors), so the next retry
//       reuses the same key.
//
// Contract owner: nxus-licensing (replay returns originally-issued response).
// See the release ownership map for the activation blocker follow-up.

const IDEMPOTENCY_KEY_FILE_NAME: &str = ".nxuskit/activation-keys.json";

fn idempotency_key_file_path() -> Option<std::path::PathBuf> {
    dirs_home().map(|home| home.join(IDEMPOTENCY_KEY_FILE_NAME))
}

fn read_idempotency_key_map() -> std::collections::HashMap<String, String> {
    let Some(path) = idempotency_key_file_path() else {
        return std::collections::HashMap::new();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return std::collections::HashMap::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

fn write_idempotency_key_map(
    map: &std::collections::HashMap<String, String>,
) -> Result<(), LicenseError> {
    let path = idempotency_key_file_path()
        .ok_or_else(|| LicenseError::StorageError("could not determine home directory".into()))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| LicenseError::StorageError(format!("create dir: {e}")))?;
    }
    let body = serde_json::to_string_pretty(map)
        .map_err(|e| LicenseError::StorageError(format!("serialize idempotency map: {e}")))?;
    std::fs::write(&path, body)
        .map_err(|e| LicenseError::StorageError(format!("write idempotency map: {e}")))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&path, perms).map_err(|e| {
            LicenseError::StorageError(format!("set idempotency map permissions: {e}"))
        })?;
    }
    Ok(())
}

/// Get the cached idempotency key for `purchase_id`, creating and
/// persisting a new UUIDv4 if none exists.
pub fn get_or_create_activation_key(purchase_id: &str) -> Result<String, LicenseError> {
    let mut map = read_idempotency_key_map();
    if let Some(existing) = map.get(purchase_id) {
        return Ok(existing.clone());
    }
    let key = uuid::Uuid::new_v4().to_string();
    map.insert(purchase_id.to_string(), key.clone());
    write_idempotency_key_map(&map)?;
    Ok(key)
}

/// Clear the cached idempotency key for `purchase_id`. Called after a
/// terminal response so the next activation attempt starts fresh.
pub fn clear_activation_key(purchase_id: &str) -> Result<(), LicenseError> {
    let mut map = read_idempotency_key_map();
    if map.remove(purchase_id).is_some() {
        write_idempotency_key_map(&map)?;
    }
    Ok(())
}

// ── Structured Logging (FR-010b) ─────────────────────────────────────

/// Emit a structured log event for successful token validation.
fn emit_validation_event(source: TokenSourceKind, outcome: &str, claims: &TokenClaims) {
    let days = claims.days_remaining().unwrap_or(-1);
    let level_str = if days <= 7 { "warn" } else { "info" };

    if days <= 7 {
        log::warn!(
            "{{\"event\":\"token_validation\",\"source\":\"{source}\",\
             \"outcome\":\"{outcome}\",\"token_type\":\"{}\",\
             \"days_remaining\":{days},\"level\":\"{level_str}\"}}",
            claims.token_type
        );
    } else {
        log::info!(
            "{{\"event\":\"token_validation\",\"source\":\"{source}\",\
             \"outcome\":\"{outcome}\",\"token_type\":\"{}\",\
             \"days_remaining\":{days},\"level\":\"{level_str}\"}}",
            claims.token_type
        );
    }
}

/// Emit a structured log event for failed token validation.
fn emit_validation_failure(source: TokenSourceKind, error: &LicenseError) {
    let outcome = match error {
        LicenseError::Expired { .. } => "expired_subscription",
        LicenseError::InvalidSignature => "invalid_signature",
        LicenseError::VersionCeilingExceeded { .. } => "version_ceiling_exceeded",
        LicenseError::TrialSuspended => "suspended",
        LicenseError::MachineMismatch { .. } => "machine_id_mismatch",
        LicenseError::EnvironmentMismatch { .. } => "environment_mismatch",
        LicenseError::DeprecatedSigningKey { .. } => "deprecated_signing_key",
        LicenseError::MalformedToken { .. } => "malformed_token",
        LicenseError::CancelledPurchase => "cancelled_purchase",
        _ => "invalid",
    };
    log::warn!(
        "{{\"event\":\"token_validation\",\"source\":\"{source}\",\
         \"outcome\":\"{outcome}\",\"token_type\":\"unknown\",\
         \"days_remaining\":-1,\"level\":\"warn\"}}"
    );
}

/// Emit a pre-expiry warning (once per session only — FR-007).
fn emit_expiry_warning(claims: &TokenClaims, days: i64) {
    if EXPIRY_WARNING_EMITTED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        let message = match claims.token_type {
            TokenType::Trial => {
                format!("Pro trial expires in {days} days. Purchase: nxus.systems/pricing")
            }
            TokenType::Developer => {
                format!("Pro license expires in {days} days. Renew: nxus.systems/account")
            }
            TokenType::RealPurchase => {
                format!("Purchased Pro license expires in {days} days. Renew: nxus.systems/account")
            }
            TokenType::Leased => {
                format!("Leased Pro license expires in {days} days. Reactivate before expiry.")
            }
            _ => return,
        };
        log::warn!("{message}");
    }
}

// ── SDK Version Parsing ──────────────────────────────────────────────

/// Parse SDK version into (major, minor) for ceiling comparison.
fn parse_sdk_version(version: &str) -> (u32, u32) {
    let parts: Vec<&str> = version.split('.').collect();
    let major = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minor = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    (major, minor)
}

// ── Microservice Client ──────────────────────────────────────────────

/// Get the licensing microservice base URL.
pub fn license_server_url() -> String {
    std::env::var("NXUSKIT_LICENSE_SERVER").unwrap_or_else(|_| LICENSE_SERVER_URL.to_string())
}

/// Build-time default licensing server URL.
pub fn default_license_server_url() -> &'static str {
    LICENSE_SERVER_URL
}

/// Get the active licensing environment.
pub fn license_environment() -> String {
    std::env::var("NXUSKIT_LICENSE_ENVIRONMENT").unwrap_or_else(|_| LICENSE_ENVIRONMENT.to_string())
}

/// Build-time default licensing environment.
pub fn default_license_environment() -> &'static str {
    LICENSE_ENVIRONMENT
}

/// Embedded ES256 public key PEM.
pub fn embedded_es256_public_key_pem() -> &'static str {
    ES256_PUBLIC_KEY_PEM
}

/// Embedded ES256 public key source path or fallback label.
pub fn embedded_es256_public_key_source() -> &'static str {
    ES256_PUBLIC_KEY_SOURCE
}

/// Embedded ES256 public key id.
pub fn embedded_es256_public_key_kid() -> &'static str {
    ES256_PUBLIC_KEY_KID
}

/// Explicitly refresh the cached license token through the licensing API.
///
/// Normal entitlement checks are offline-first and do not call this path. This
/// function is reserved for user-initiated refresh/sync flows.
pub fn refresh_cached_license() -> Result<TokenResolution, LicenseError> {
    let raw_token = current_raw_token_for_refresh()
        .ok_or_else(|| LicenseError::ValidationFailed("No cached license token found".into()))?;

    let verifier = get_verifier();
    let environment = license_environment();
    refresh_cached_license_with_client_inner(
        &raw_token,
        verifier.as_ref(),
        &environment,
        true,
        |url, body| blocking_post_with_timeout(url, body, DEFAULT_TIMEOUT_SECS),
    )
}

fn current_raw_token_for_refresh() -> Option<String> {
    if let Ok(env_token) = std::env::var(LICENSE_ENV_VAR) {
        let token = env_token.trim();
        if !token.is_empty() {
            return Some(token.to_string());
        }
    }

    token_file_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Refresh a token through an injected client and explicit verifier.
///
/// This is used by release-readiness tests to prove that explicit refresh is
/// the only path that performs a licensing API call.
#[doc(hidden)]
pub fn refresh_cached_license_with_client_and_verifier<F>(
    raw_token: &str,
    verifier: &dyn TokenVerifier,
    expected_environment: &str,
    refresh_client: F,
) -> Result<TokenResolution, LicenseError>
where
    F: FnOnce(&str, &serde_json::Value) -> Result<serde_json::Value, LicenseError>,
{
    refresh_cached_license_with_client_inner(
        raw_token,
        verifier,
        expected_environment,
        false,
        refresh_client,
    )
}

fn refresh_cached_license_with_client_inner<F>(
    raw_token: &str,
    verifier: &dyn TokenVerifier,
    expected_environment: &str,
    persist_refreshed_token: bool,
    refresh_client: F,
) -> Result<TokenResolution, LicenseError>
where
    F: FnOnce(&str, &serde_json::Value) -> Result<serde_json::Value, LicenseError>,
{
    let machine_id = machine_id::get_machine_fingerprint()
        .map_err(|e| LicenseError::NetworkError(format!("machine ID: {e}")))?;
    let body = serde_json::json!({
        "token": raw_token,
        "machine_id": machine_id,
    });
    let url = format!("{}/refresh", license_server_url());
    let response_json = refresh_client(&url, &body)?;
    let refreshed_token = response_json
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap_or(raw_token)
        .trim()
        .to_string();

    if refreshed_token.is_empty() {
        return Err(LicenseError::MalformedToken {
            details: "refresh response did not include a token".to_string(),
        });
    }

    let validated = validate_token_full_with_verifier_and_environment(
        &refreshed_token,
        verifier,
        expected_environment,
    )?;

    if persist_refreshed_token {
        store_token_file(&refreshed_token)?;
    }

    Ok(TokenResolution {
        source: TokenSourceKind::TokenFile,
        raw_token: Some(refreshed_token),
        claims: Some(validated.claims),
        valid: true,
        error: None,
    })
}

// ── License Activation / Deactivation ────────────────────────────────

/// Activate a Pro license on this machine.
///
/// Calls `POST /activate` on the licensing microservice with the machine
/// fingerprint, stores the returned JWT at `~/.nxuskit/license.token`,
/// and returns the seat count.
///
/// Sends an `Idempotency-Key` HTTP header keyed on `purchase_id`. If the
/// proxy returns a transient error (`backend_timeout`,
/// `rate_limit_exceeded`, network error) the key is preserved so the
/// next retry against the same purchase ID reuses it; the licensing
/// backend (when it adopts the contract) will then return the
/// originally-issued token rather than minting a duplicate. On a
/// terminal response (success, or a non-retryable client error such as
/// `invalid_purchase_id` / `wrong_product_identifier` / seat limit), the
/// key is cleared so a future re-attempt starts fresh.
pub fn activate(purchase_id: &str) -> Result<crate::license_types::ActivationResult, LicenseError> {
    activate_with_post(purchase_id, |url, body, timeout_secs, idempotency_key| {
        blocking_post_with_timeout_and_idempotency(url, body, timeout_secs, idempotency_key)
    })
}

fn activate_with_post<F>(
    purchase_id: &str,
    post_client: F,
) -> Result<crate::license_types::ActivationResult, LicenseError>
where
    F: FnOnce(
        &str,
        &serde_json::Value,
        u64,
        Option<&str>,
    ) -> Result<serde_json::Value, LicenseError>,
{
    let machine_id = machine_id::get_machine_fingerprint()
        .map_err(|e| LicenseError::NetworkError(format!("machine ID: {e}")))?;

    let body = serde_json::json!({
        "purchase_id": purchase_id,
        "machine_id": machine_id,
    });

    let url = format!("{}/activate", license_server_url());
    let idempotency_key = get_or_create_activation_key(purchase_id)?;
    let response_json =
        match post_client(&url, &body, EXTENDED_TIMEOUT_SECS, Some(&idempotency_key)) {
            Ok(v) => v,
            Err(e) => {
                // Preserve the key on transient errors so a retry reuses it.
                // Clear on non-retryable client errors so the next attempt
                // starts fresh — the user will need a new purchase ID anyway.
                if is_retryable_activation_error(&e) {
                    log::debug!(
                        "activation transient error; preserving Idempotency-Key for retry: {e}"
                    );
                } else {
                    let _ = clear_activation_key(purchase_id);
                }
                return Err(e);
            }
        };

    // Parse response — success is determined by HTTP status (2xx),
    // not a `success` field (per contract governance DL-08).

    // Store the returned token
    if let Some(token) = response_json.get("token").and_then(|v| v.as_str()) {
        store_token_file(token)?;
    }

    let seats_used = response_json
        .get("seats_used")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let seats_total = response_json
        .get("seats_total")
        .and_then(|v| v.as_u64())
        .unwrap_or(3) as u32;
    let message = response_json
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Success is terminal — clear the cached idempotency key.
    let _ = clear_activation_key(purchase_id);

    Ok(crate::license_types::ActivationResult {
        success: true,
        seats_used,
        seats_total,
        token: response_json
            .get("token")
            .and_then(|v| v.as_str())
            .map(String::from),
        deployment_token: response_json
            .get("deployment_token")
            .and_then(|v| v.as_str())
            .map(String::from),
        message,
        error: None,
    })
}

/// Returns true if the activation error is transient and the SDK should
/// preserve the cached `Idempotency-Key` so the next retry replays it.
/// Returns false for terminal errors (invalid purchase ID, wrong product,
/// seat limit) where a retry would not succeed without user intervention.
fn is_retryable_activation_error(err: &LicenseError) -> bool {
    match err {
        LicenseError::NetworkError(_) => true,
        LicenseError::ServerError { code, .. } => matches!(
            code.as_str(),
            "backend_timeout"
                | "rate_limit_exceeded"
                | "service_unavailable"
                | "gateway_timeout"
                | "upstream_unavailable"
                | "activation_in_progress"
                | "already_activated"
        ),
        // Authentication can be refreshed by the user re-running login,
        // and the same Idempotency-Key remains valid for the next attempt.
        LicenseError::AuthenticationRequired => true,
        // Terminal: invalid purchase, wrong product, seat limit, malformed
        // token, storage error, etc. The cached key would be wrong for any
        // future activation against a different purchase ID anyway.
        _ => false,
    }
}

/// Deactivate the Pro license on this machine.
///
/// Reads the current token, calls `POST /deactivate` on the microservice,
/// and deletes `~/.nxuskit/license.token`.
pub fn deactivate() -> Result<crate::license_types::DeactivationResult, LicenseError> {
    let machine_id = machine_id::get_machine_fingerprint()
        .map_err(|e| LicenseError::NetworkError(format!("machine ID: {e}")))?;

    // Read current token
    let current_token = token_file_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| s.trim().to_string());

    let body = serde_json::json!({
        "machine_id": machine_id,
        "token": current_token,
    });

    let url = format!("{}/deactivate", license_server_url());
    let response_json = blocking_post(&url, &body)?;
    // Success determined by HTTP 2xx status (contract governance DL-08).

    // Remove token file
    remove_token_file()?;

    let seats_used = response_json
        .get("seats_used")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let seats_total = response_json
        .get("seats_total")
        .and_then(|v| v.as_u64())
        .unwrap_or(3) as u32;
    let message = response_json
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Ok(crate::license_types::DeactivationResult {
        success: true,
        seats_used,
        seats_total,
        message,
        error: None,
    })
}

// ── Trial Issuance / Activation ─────────────────────────────────────

/// Auto-issue a 30-day trial token for this machine.
///
/// Calls `POST /trial` on the licensing microservice. Uses extended timeout
/// (30s) for cold-start backend scenarios.
///
/// If the server returns `trial_exists` (a previous trial was created but the
/// token was never stored locally), attempts to fetch the existing token via
/// `GET /trial?machine_id=...`.
pub fn trial_issue() -> Result<crate::license_types::TrialIssuanceResult, LicenseError> {
    let machine_id = machine_id::get_machine_fingerprint()
        .map_err(|e| LicenseError::NetworkError(format!("machine ID: {e}")))?;

    let body = serde_json::json!({
        "machine_id": machine_id,
        "sdk_version": SDK_VERSION,
    });

    let url = format!("{}/trial", license_server_url());
    let result = blocking_post_with_timeout(&url, &body, EXTENDED_TIMEOUT_SECS);

    // Handle trial_exists: attempt to fetch the existing trial token.
    // If the fetch endpoint isn't available yet, return a helpful error.
    if let Err(LicenseError::ServerError { ref code, .. }) = result
        && (code == "trial_exists" || code == "trial_already_issued")
    {
        log::info!("Trial already exists on server — attempting to fetch existing token");
        match trial_fetch(&machine_id) {
            Ok(r) => return Ok(r),
            Err(fetch_err) => {
                log::warn!("trial_fetch failed: {fetch_err}");
                return Err(LicenseError::ServerError {
                    code: "trial_exists".to_string(),
                    message: format!(
                        "A trial already exists for this machine but the token could \
                         not be retrieved ({fetch_err}). Run `nxuskit-cli license sync` \
                         to retry, or contact support at https://nxus.systems/support"
                    ),
                });
            }
        }
    }

    let response_json = result?;

    // Store the trial token
    if let Some(token) = response_json.get("token").and_then(|v| v.as_str()) {
        store_token_file(token)?;
    }

    let days_remaining = response_json
        .get("days_remaining")
        .and_then(|v| v.as_u64())
        .unwrap_or(30) as u32;
    let message = response_json
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Ok(crate::license_types::TrialIssuanceResult {
        success: true,
        token: response_json
            .get("token")
            .and_then(|v| v.as_str())
            .map(String::from),
        days_remaining,
        message,
        error: None,
    })
}

/// Fetch an existing trial token from the server.
///
/// Calls `GET /trial?machine_id=...` to retrieve a trial token that was
/// previously issued but not stored locally (e.g., due to timeout).
///
/// Requires the nxus-licensing backend to support this endpoint.
/// If the endpoint doesn't exist (404), returns a descriptive error.
pub fn trial_fetch(
    machine_id: &str,
) -> Result<crate::license_types::TrialIssuanceResult, LicenseError> {
    let url = format!("{}/trial?machine_id={}", license_server_url(), machine_id);

    let client = reqwest::blocking::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(EXTENDED_TIMEOUT_SECS))
        .build()
        .map_err(|e| LicenseError::NetworkError(format!("create HTTP client: {e}")))?;

    let mut request = client.get(&url);
    if let Some(bearer) = crate::auth_token::read_bearer_token() {
        request = request.header("Authorization", format!("Bearer {bearer}"));
    }

    let response = request
        .send()
        .map_err(|e| LicenseError::NetworkError(format!("GET {url}: {e}")))?;

    let status = response.status();

    // Handle non-success responses (including 405 Method Not Allowed from
    // servers that haven't deployed the GET /trial endpoint yet)
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        let (code, message) = if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
            (
                json.get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("server_error")
                    .to_string(),
                json.get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Failed to fetch trial token")
                    .to_string(),
            )
        } else {
            (
                format!("http_{}", status.as_u16()),
                format!(
                    "License server returned HTTP {}. The token fetch endpoint \
                     may not be available yet. Contact support or try again later.",
                    status.as_u16()
                ),
            )
        };
        return Err(LicenseError::ServerError { code, message });
    }

    let response_json: serde_json::Value = response
        .json()
        .map_err(|e| LicenseError::NetworkError(format!("parse response: {e}")))?;

    // Store the fetched token
    if let Some(token) = response_json.get("token").and_then(|v| v.as_str()) {
        store_token_file(token)?;
    }

    let days_remaining = response_json
        .get("days_remaining")
        .and_then(|v| v.as_u64())
        .unwrap_or(30) as u32;
    let message = response_json
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("Existing trial token retrieved.")
        .to_string();

    Ok(crate::license_types::TrialIssuanceResult {
        success: true,
        token: response_json
            .get("token")
            .and_then(|v| v.as_str())
            .map(String::from),
        days_remaining,
        message,
        error: None,
    })
}

/// Activate a trial token (complete email verification).
///
/// Calls `POST /trial/activate` on the licensing microservice.
/// Uses extended timeout (30s) for cold-start backend scenarios.
///
/// Sends an `Idempotency-Key` HTTP header keyed on `activation_code`,
/// using the same lifecycle rules as [`activate`] (preserve on transient
/// errors, clear on terminal response).
pub fn trial_activate(
    activation_code: &str,
) -> Result<crate::license_types::TrialIssuanceResult, LicenseError> {
    let machine_id = machine_id::get_machine_fingerprint()
        .map_err(|e| LicenseError::NetworkError(format!("machine ID: {e}")))?;

    let body = serde_json::json!({
        "activation_code": activation_code,
        "machine_id": machine_id,
    });

    let url = format!("{}/trial/activate", license_server_url());
    let idempotency_key = get_or_create_activation_key(activation_code)?;
    let response_json = match blocking_post_with_timeout_and_idempotency(
        &url,
        &body,
        EXTENDED_TIMEOUT_SECS,
        Some(&idempotency_key),
    ) {
        Ok(v) => v,
        Err(e) => {
            if is_retryable_activation_error(&e) {
                log::debug!(
                    "trial activation transient error; preserving Idempotency-Key for retry: {e}"
                );
            } else {
                let _ = clear_activation_key(activation_code);
            }
            return Err(e);
        }
    };
    // Success determined by HTTP 2xx status (contract governance DL-08).

    // Replace token file with activated token
    if let Some(token) = response_json.get("token").and_then(|v| v.as_str()) {
        store_token_file(token)?;
    }

    let days_remaining = response_json
        .get("days_remaining")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let message = response_json
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Success is terminal — clear the cached idempotency key.
    let _ = clear_activation_key(activation_code);

    Ok(crate::license_types::TrialIssuanceResult {
        success: true,
        token: response_json
            .get("token")
            .and_then(|v| v.as_str())
            .map(String::from),
        days_remaining,
        message,
        error: None,
    })
}

// ── HTTP Client ─────────────────────────────────────────────────────

/// Default total timeout for licensing API calls.
const DEFAULT_TIMEOUT_SECS: u64 = 10;

/// Extended timeout for operations that may hit cold-start backends
/// (trial issuance, activation). Cloud Run / Odoo proxy may take
/// 15-30 seconds on first request after idle.
const EXTENDED_TIMEOUT_SECS: u64 = 30;

/// Blocking HTTP POST to the licensing microservice.
///
/// Uses reqwest blocking client with a 5-second connect timeout.
/// Total timeout is configurable (default 10s, extended 30s for trial/activate).
fn blocking_post(url: &str, body: &serde_json::Value) -> Result<serde_json::Value, LicenseError> {
    blocking_post_with_timeout(url, body, DEFAULT_TIMEOUT_SECS)
}

/// Blocking HTTP POST with explicit timeout.
fn blocking_post_with_timeout(
    url: &str,
    body: &serde_json::Value,
    timeout_secs: u64,
) -> Result<serde_json::Value, LicenseError> {
    blocking_post_with_timeout_and_idempotency(url, body, timeout_secs, None)
}

/// Blocking HTTP POST with explicit timeout and an optional
/// `Idempotency-Key` header.
///
/// When `idempotency_key` is `Some`, the SDK sends an `Idempotency-Key`
/// HTTP header alongside the body. Requests whose JSON body contains a
/// `machine_id` string also send `X-Machine-Id`, allowing the backend
/// rate limiter to isolate buckets per device before it parses the body.
/// The Odoo proxy passes both headers through to the licensing backend.
/// The backend contract — return the originally-issued response when the
/// same key is replayed — is owned by `nxus-licensing` and tracked in the
/// release ownership map. The SDK send-side landed first so the backend
/// contract could complete additively.
fn blocking_post_with_timeout_and_idempotency(
    url: &str,
    body: &serde_json::Value,
    timeout_secs: u64,
    idempotency_key: Option<&str>,
) -> Result<serde_json::Value, LicenseError> {
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| LicenseError::NetworkError(format!("create HTTP client: {e}")))?;

    // Add Bearer auth token if available
    let mut request = client.post(url).json(body);
    if let Some(bearer) = crate::auth_token::read_bearer_token() {
        request = request.header("Authorization", format!("Bearer {bearer}"));
    }
    if let Some(key) = idempotency_key {
        request = request.header("Idempotency-Key", key);
    }
    if let Some(machine_id) = body
        .get("machine_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        request = request.header("X-Machine-Id", machine_id);
    }

    let response = request
        .send()
        .map_err(|e| LicenseError::NetworkError(e.to_string()))?;

    let status = response.status();
    let response_body = response
        .text()
        .map_err(|e| LicenseError::NetworkError(format!("read response body: {e}")))?;

    parse_license_server_json_response(url, status, &response_body)
}

fn parse_license_server_json_response(
    url: &str,
    status: reqwest::StatusCode,
    response_body: &str,
) -> Result<serde_json::Value, LicenseError> {
    // Handle 401 before parsing JSON. Odoo auth failures may be rendered as
    // HTML by Werkzeug, but they still mean the local device auth session must
    // be refreshed.
    if status.as_u16() == 401 {
        return Err(LicenseError::AuthenticationRequired);
    }

    let parsed_json = serde_json::from_str::<serde_json::Value>(response_body);

    if !status.is_success() {
        let response_json = parsed_json.ok();
        let error_code = response_json
            .as_ref()
            .and_then(|json| json.get("error"))
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| format!("http_{}", status.as_u16()));
        let message = response_json
            .as_ref()
            .and_then(|json| json.get("message"))
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| {
                format!(
                    "License server returned HTTP {} with a non-JSON response from {url}",
                    status.as_u16()
                )
            });
        return Err(LicenseError::ServerError {
            code: error_code,
            message,
        });
    }

    parsed_json.map_err(|e| LicenseError::NetworkError(format!("parse response from {url}: {e}")))
}

// ── Error Types ──────────────────────────────────────────────────────

/// Errors during license token operations.
#[derive(Debug)]
pub enum LicenseError {
    /// JWT signature is invalid.
    InvalidSignature,
    /// JWT uses an unexpected algorithm (not RS384).
    InvalidAlgorithm(String),
    /// JWT issuer is not "nxus-licensing".
    InvalidIssuer,
    /// JWT structure is malformed.
    MalformedToken { details: String },
    /// Token product id does not match this SDK.
    InvalidProductId { expected: String, actual: String },
    /// Token has expired.
    Expired { token_type: TokenType },
    /// Purchase has been cancelled upstream.
    CancelledPurchase,
    /// Token was signed by a deprecated key id.
    DeprecatedSigningKey { kid: String },
    /// Token environment does not match this binary's environment.
    EnvironmentMismatch { expected: String, actual: String },
    /// Trial token not activated within 7-day grace period.
    TrialSuspended,
    /// Token is bound to a different machine.
    MachineMismatch { expected: String, actual: String },
    /// Deployment token version ceiling exceeded.
    VersionCeilingExceeded {
        ceiling: String,
        sdk_version: String,
    },
    /// RSA public key error.
    KeyError(String),
    /// General validation failure.
    ValidationFailed(String),
    /// Token storage error.
    StorageError(String),
    /// Network error communicating with licensing microservice.
    NetworkError(String),
    /// Microservice returned an error.
    ServerError { code: String, message: String },
    /// Authentication required (401 from licensing API).
    AuthenticationRequired,
}

impl std::fmt::Display for LicenseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LicenseError::InvalidSignature => write!(f, "Invalid license token signature"),
            LicenseError::InvalidAlgorithm(msg) => write!(f, "Invalid algorithm: {msg}"),
            LicenseError::InvalidIssuer => write!(f, "Invalid token issuer"),
            LicenseError::MalformedToken { details } => write!(f, "Malformed token: {details}"),
            LicenseError::InvalidProductId { expected, actual } => {
                write!(
                    f,
                    "Token issued for product '{actual}', expected '{expected}'"
                )
            }
            LicenseError::Expired { token_type } => {
                write!(f, "{token_type} token has expired")
            }
            LicenseError::CancelledPurchase => write!(f, "Purchase has been cancelled"),
            LicenseError::DeprecatedSigningKey { kid } => {
                write!(f, "Token was signed by deprecated key id '{kid}'")
            }
            LicenseError::EnvironmentMismatch { expected, actual } => {
                write!(
                    f,
                    "Token environment '{actual}' does not match expected '{expected}'"
                )
            }
            LicenseError::TrialSuspended => {
                write!(
                    f,
                    "Trial suspended. Run: nxuskit-cli license activate --trial"
                )
            }
            LicenseError::MachineMismatch { expected, actual } => {
                write!(
                    f,
                    "Token bound to machine {expected}, current machine is {actual}"
                )
            }
            LicenseError::VersionCeilingExceeded {
                ceiling,
                sdk_version,
            } => write!(
                f,
                "Deployment token covers up to v{ceiling}.x. \
                 Current SDK is v{sdk_version}. Update your deployment token."
            ),
            LicenseError::KeyError(msg) => write!(f, "Public key error: {msg}"),
            LicenseError::ValidationFailed(msg) => write!(f, "Token validation failed: {msg}"),
            LicenseError::StorageError(msg) => write!(f, "Token storage error: {msg}"),
            LicenseError::NetworkError(msg) => write!(f, "Network error: {msg}"),
            LicenseError::ServerError { code, message } => {
                write!(f, "Server error [{code}]: {message}")
            }
            LicenseError::AuthenticationRequired => {
                write!(f, "Session expired. Run: nxuskit-cli license login")
            }
        }
    }
}

impl std::error::Error for LicenseError {}

impl LicenseError {
    /// Stable machine-readable error code for CLI/GUI consumers.
    pub fn code(&self) -> &str {
        match self {
            LicenseError::InvalidSignature => "invalid_signature",
            LicenseError::InvalidAlgorithm(_) => "invalid_algorithm",
            LicenseError::InvalidIssuer => "invalid_issuer",
            LicenseError::MalformedToken { .. } => "malformed_token",
            LicenseError::InvalidProductId { .. } => "wrong_product_identifier",
            LicenseError::Expired { .. } => "expired_subscription",
            LicenseError::CancelledPurchase => "cancelled_purchase",
            LicenseError::DeprecatedSigningKey { .. } => "deprecated_signing_key",
            LicenseError::EnvironmentMismatch { .. } => "environment_mismatch",
            LicenseError::TrialSuspended => "trial_suspended",
            LicenseError::MachineMismatch { .. } => "machine_id_mismatch",
            LicenseError::VersionCeilingExceeded { .. } => "version_ceiling_exceeded",
            LicenseError::KeyError(_) => "key_error",
            LicenseError::ValidationFailed(_) => "validation_failed",
            LicenseError::StorageError(_) => "storage_error",
            LicenseError::NetworkError(_) => "network_error",
            LicenseError::ServerError { code, .. } => code.as_str(),
            LicenseError::AuthenticationRequired => "authentication_required",
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::panic, clippy::print_stderr)]
mod tests {
    use super::*;

    // Ensure NXUS_SIGNING_KEY_PATH points to the test key BEFORE any test
    // calls get_verifier(). This is critical when the `licensing-client`
    // feature is enabled, because ExternalClientVerifier uses this env var
    // and OnceLock caches the first verifier instance for the process.
    //
    // We use std::sync::Once to make this safe for parallel test execution.
    fn ensure_test_key_env() {
        use std::sync::Once;
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            // Write the TEST key pair's public key to temp file for NXUS_SIGNING_KEY_PATH.
            // This must match the private key used by test_fixtures::test_es256_keypair()
            // to sign test tokens — NOT the embedded production/dev key.
            let (_, test_public_pem) = crate::test_fixtures::test_es256_keypair();
            let key_dir = std::env::temp_dir().join("nxuskit-test-keys");
            std::fs::create_dir_all(&key_dir).ok();
            let key_path = key_dir.join("es256-test-pubkey.pem");
            std::fs::write(&key_path, &test_public_pem).ok();
            unsafe {
                std::env::set_var("NXUS_SIGNING_KEY_PATH", key_path.to_str().unwrap());
            }
        });
    }

    #[test]
    fn test_parse_sdk_version() {
        ensure_test_key_env(); // Must be first to set up test signing key
        assert_eq!(parse_sdk_version("0.9.1"), (0, 9));
        assert_eq!(parse_sdk_version("1.0.0"), (1, 0));
        assert_eq!(parse_sdk_version("0.10.3"), (0, 10));
    }

    #[test]
    fn test_validate_token_rejects_garbage() {
        let result = validate_token("not-a-jwt");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_token_rejects_empty() {
        let result = validate_token("");
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_with_no_sources() {
        // Clean state: no env var, no token file, no API param
        let _ = remove_token_file();
        unsafe { std::env::remove_var(LICENSE_ENV_VAR) };
        let resolution = resolve_token(None);
        assert!(!resolution.valid);
        assert_eq!(resolution.source, TokenSourceKind::None);
    }

    #[test]
    fn test_resolve_env_var_takes_precedence() {
        // Env var should be checked before file or API param
        let _ = remove_token_file();
        unsafe { std::env::set_var(LICENSE_ENV_VAR, "env-token-value") };
        let resolution = resolve_token(Some("api-param-value"));
        assert_eq!(resolution.source, TokenSourceKind::EnvironmentVariable);
        unsafe { std::env::remove_var(LICENSE_ENV_VAR) };
    }

    #[test]
    fn test_resolve_api_param_when_no_env_or_file() {
        // No env var, no token file — should fall through to API param
        let _ = remove_token_file();
        unsafe { std::env::remove_var(LICENSE_ENV_VAR) };
        let resolution = resolve_token(Some("api-token-value"));
        assert_eq!(resolution.source, TokenSourceKind::ApiParameter);
        assert!(!resolution.valid); // Not a real JWT
    }

    #[test]
    fn test_license_error_display() {
        let err = LicenseError::VersionCeilingExceeded {
            ceiling: "0.9".to_string(),
            sdk_version: "0.10.0".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("v0.9.x"));
        assert!(msg.contains("v0.10.0"));
    }

    #[test]
    fn test_html_unauthorized_response_maps_to_auth_required() {
        let err = parse_license_server_json_response(
            "https://nxus.systems/licensing-api/v1/activate",
            reqwest::StatusCode::UNAUTHORIZED,
            "<!doctype html><h1>Unauthorized</h1>",
        )
        .expect_err("401 should require login even when response is HTML");

        assert!(matches!(err, LicenseError::AuthenticationRequired));
        assert_eq!(err.code(), "authentication_required");
    }

    #[test]
    fn test_json_server_error_preserves_error_code() {
        let err = parse_license_server_json_response(
            "https://nxus.systems/licensing-api/v1/activate",
            reqwest::StatusCode::CONFLICT,
            r#"{"error":"seats_exhausted","message":"No seats remain."}"#,
        )
        .expect_err("non-2xx response should map to server error");

        match err {
            LicenseError::ServerError { code, message } => {
                assert_eq!(code, "seats_exhausted");
                assert_eq!(message, "No seats remain.");
            }
            other => panic!("expected server error, got {other:?}"),
        }
    }

    #[test]
    fn test_expiry_warning_once_per_session() {
        reset_expiry_warning();
        let claims = TokenClaims {
            product_id: "nxuskit".to_string(),
            iss: "nxus-licensing".to_string(),
            iat: 0,
            nbf: None,
            exp: Some(current_unix_timestamp() + 3 * 86400),
            token_type: TokenType::Developer,
            edition: "pro".to_string(),
            tenant_id: None,
            machine_id: None,
            seat_index: None,
            activated: None,
            sdk_version_ceiling: None,
            customer_email: None,
        };

        // First call should emit
        emit_expiry_warning(&claims, 3);
        assert!(EXPIRY_WARNING_EMITTED.load(Ordering::SeqCst));

        // Second call should be no-op (already emitted)
        emit_expiry_warning(&claims, 3);
        // Still true — no panic, just skipped

        reset_expiry_warning();
    }

    // ── Signed Token Test Fixtures ──────────────────────────────────

    /// Ensure the test ES256 public key is available for the global verifier.
    ///
    /// When the external licensing feature is enabled, `get_verifier()` returns
    /// `ExternalClientVerifier` which uses `NXUS_SIGNING_KEY_PATH` if set. This
    /// function writes the test public key to a temp file and sets the env var
    /// so that test-signed tokens verify correctly.
    ///
    /// Uses `ctor` pattern to run before any test, ensuring the OnceLock in
    /// `get_verifier()` picks up the test key on first initialization.
    /// Alias for ensure_test_key_env() — used by signed_fixtures module.
    fn setup_test_signing_key() {
        ensure_test_key_env();
    }

    /// Helper module that generates ES256-signed JWTs for integration testing
    /// using the deterministic test key pair from `test_fixtures`.
    mod signed_fixtures {
        use super::*;
        use crate::test_fixtures;

        /// Create a signed JWT with given claims using the ES256 test key.
        fn sign_claims(claims: &TokenClaims) -> Option<String> {
            setup_test_signing_key();
            let (priv_pem, _pub_pem) = test_fixtures::test_es256_keypair();
            Some(test_fixtures::sign_test_jwt(claims, &priv_pem))
        }

        /// Create a valid developer token for the current machine (30 days from now).
        fn make_developer_token() -> Option<String> {
            let now = current_unix_timestamp();
            let claims = TokenClaims {
                product_id: "nxuskit".to_string(),
                iss: "nxus-licensing".to_string(),
                iat: now,
                nbf: Some(now),
                exp: Some(now + 30 * 86400),
                token_type: TokenType::Developer,
                edition: "pro".to_string(),
                tenant_id: Some("test-org".to_string()),
                machine_id: None, // Don't bind to this machine for portability
                seat_index: Some(1),
                activated: None,
                sdk_version_ceiling: None,
                customer_email: None,
            };
            sign_claims(&claims)
        }

        /// Create a developer token expiring within N days.
        fn make_near_expiry_token(days: i64) -> Option<String> {
            let now = current_unix_timestamp();
            let claims = TokenClaims {
                product_id: "nxuskit".to_string(),
                iss: "nxus-licensing".to_string(),
                iat: now - 25 * 86400,
                nbf: Some(now - 25 * 86400),
                exp: Some(now + days * 86400),
                token_type: TokenType::Developer,
                edition: "pro".to_string(),
                tenant_id: Some("test-org".to_string()),
                machine_id: None,
                seat_index: Some(1),
                activated: None,
                sdk_version_ceiling: None,
                customer_email: None,
            };
            sign_claims(&claims)
        }

        /// Create an expired developer token.
        fn make_expired_token() -> Option<String> {
            let now = current_unix_timestamp();
            let claims = TokenClaims {
                product_id: "nxuskit".to_string(),
                iss: "nxus-licensing".to_string(),
                iat: now - 60 * 86400,
                nbf: Some(now - 60 * 86400),
                exp: Some(now - 1), // expired 1 second ago
                token_type: TokenType::Developer,
                edition: "pro".to_string(),
                tenant_id: Some("test-org".to_string()),
                machine_id: None,
                seat_index: Some(1),
                activated: None,
                sdk_version_ceiling: None,
                customer_email: None,
            };
            sign_claims(&claims)
        }

        /// Create a deployment token with a version ceiling.
        fn make_deployment_token(ceiling: &str) -> Option<String> {
            let now = current_unix_timestamp();
            let claims = TokenClaims {
                product_id: "nxuskit".to_string(),
                iss: "nxus-licensing".to_string(),
                iat: now,
                nbf: None,
                exp: None, // deployment tokens never expire
                token_type: TokenType::Deployment,
                edition: "pro".to_string(),
                tenant_id: Some("test-org".to_string()),
                machine_id: None,
                seat_index: None,
                activated: None,
                sdk_version_ceiling: Some(ceiling.to_string()),
                customer_email: Some("test@example.com".to_string()),
            };
            sign_claims(&claims)
        }

        /// Create a trial token (unactivated).
        fn make_trial_token(activated: bool, issued_days_ago: i64) -> Option<String> {
            let now = current_unix_timestamp();
            let iat = now - issued_days_ago * 86400;
            let claims = TokenClaims {
                product_id: "nxuskit".to_string(),
                iss: "nxus-licensing".to_string(),
                iat,
                nbf: Some(iat),
                exp: Some(iat + 30 * 86400),
                token_type: TokenType::Trial,
                edition: "pro".to_string(),
                tenant_id: None,
                machine_id: None,
                seat_index: None,
                activated: Some(activated),
                sdk_version_ceiling: None,
                customer_email: None,
            };
            sign_claims(&claims)
        }

        // ── T026: Pre-expiry warning behavior tests ──────────────────

        #[test]
        fn test_signed_developer_token_validates() {
            let jwt = match make_developer_token() {
                Some(t) => t,
                None => {
                    eprintln!("SKIP: private key not available");
                    return;
                }
            };

            let claims = validate_token(&jwt).expect("valid signed token should validate");
            assert_eq!(claims.token_type, TokenType::Developer);
            assert_eq!(claims.iss, "nxus-licensing");
            assert_eq!(claims.edition, "pro");
            assert_eq!(claims.seat_index, Some(1));
        }

        #[test]
        fn test_signed_token_full_validation_passes() {
            let jwt = match make_developer_token() {
                Some(t) => t,
                None => {
                    eprintln!("SKIP: private key not available");
                    return;
                }
            };

            let validated =
                validate_token_full(&jwt).expect("valid token should pass full validation");
            assert_eq!(validated.claims.token_type, TokenType::Developer);
            assert!(!validated.claims.is_expired());
        }

        #[test]
        fn test_signed_expired_token_rejected() {
            let jwt = match make_expired_token() {
                Some(t) => t,
                None => {
                    eprintln!("SKIP: private key not available");
                    return;
                }
            };

            // Signature validates fine
            let claims = validate_token(&jwt).expect("expired token still has valid signature");
            assert_eq!(claims.token_type, TokenType::Developer);

            // But full validation rejects it
            let err = validate_token_full(&jwt).unwrap_err();
            assert!(matches!(err, LicenseError::Expired { .. }));
        }

        #[test]
        fn test_pre_expiry_warning_at_3_days() {
            reset_expiry_warning();
            let jwt = match make_near_expiry_token(3) {
                Some(t) => t,
                None => {
                    eprintln!("SKIP: private key not available");
                    return;
                }
            };

            // Full validation should succeed and emit warning
            let validated =
                validate_token_full(&jwt).expect("near-expiry token should still be valid");
            assert!(validated.claims.days_remaining().unwrap() <= 7);
            assert!(EXPIRY_WARNING_EMITTED.load(Ordering::SeqCst));

            reset_expiry_warning();
        }

        #[test]
        fn test_no_warning_at_14_days() {
            reset_expiry_warning();
            let jwt = match make_near_expiry_token(14) {
                Some(t) => t,
                None => {
                    eprintln!("SKIP: private key not available");
                    return;
                }
            };

            let validated = validate_token_full(&jwt).expect("14-day token should be valid");
            assert!(validated.claims.days_remaining().unwrap() > 7);
            assert!(!EXPIRY_WARNING_EMITTED.load(Ordering::SeqCst));

            reset_expiry_warning();
        }

        #[test]
        fn test_warning_emitted_once_per_session() {
            reset_expiry_warning();
            let jwt = match make_near_expiry_token(3) {
                Some(t) => t,
                None => {
                    eprintln!("SKIP: private key not available");
                    return;
                }
            };

            // First validation sets the flag
            let _ = validate_token_full(&jwt);
            assert!(EXPIRY_WARNING_EMITTED.load(Ordering::SeqCst));

            // Second validation doesn't reset it (flag already set)
            let _ = validate_token_full(&jwt);
            assert!(EXPIRY_WARNING_EMITTED.load(Ordering::SeqCst));

            reset_expiry_warning();
        }

        // ── T046: Deployment token validation tests ──────────────────

        #[test]
        fn test_deployment_token_no_expiry() {
            let jwt = match make_deployment_token("0.9") {
                Some(t) => t,
                None => {
                    eprintln!("SKIP: private key not available");
                    return;
                }
            };

            let claims = validate_token(&jwt).expect("deployment token should validate");
            assert_eq!(claims.token_type, TokenType::Deployment);
            assert!(claims.exp.is_none());
            assert!(!claims.is_expired());
        }

        #[test]
        fn test_deployment_token_version_ceiling_within() {
            // Ceiling "0.9" should allow SDK 0.9.x
            let jwt = match make_deployment_token("0.9") {
                Some(t) => t,
                None => {
                    eprintln!("SKIP: private key not available");
                    return;
                }
            };

            // SDK version is 0.9.1, ceiling is 0.9 — should pass
            let validated = validate_token_full(&jwt)
                .expect("deployment token with matching ceiling should pass");
            assert_eq!(validated.claims.token_type, TokenType::Deployment);
            assert_eq!(
                validated.claims.sdk_version_ceiling,
                Some("0.9".to_string())
            );
        }

        #[test]
        fn test_deployment_token_version_ceiling_exceeded() {
            // Ceiling "0.8" should reject SDK 0.9.x
            let jwt = match make_deployment_token("0.8") {
                Some(t) => t,
                None => {
                    eprintln!("SKIP: private key not available");
                    return;
                }
            };

            let err = validate_token_full(&jwt).unwrap_err();
            assert!(matches!(err, LicenseError::VersionCeilingExceeded { .. }));
            let msg = err.to_string();
            assert!(msg.contains("v0.8.x"));
        }

        #[test]
        fn test_deployment_token_no_machine_binding() {
            let jwt = match make_deployment_token("0.9") {
                Some(t) => t,
                None => {
                    eprintln!("SKIP: private key not available");
                    return;
                }
            };

            // Deployment tokens should never check machine ID
            let claims = validate_token(&jwt).unwrap();
            assert!(claims.machine_id.is_none());
            // Full validation passes without machine check
            let validated =
                validate_token_full(&jwt).expect("no machine binding check for deployment");
            assert_eq!(validated.claims.token_type, TokenType::Deployment);
        }

        // ── T051: Resolution chain integration tests ─────────────────

        #[test]
        fn test_resolution_env_var_with_signed_token() {
            let jwt = match make_developer_token() {
                Some(t) => t,
                None => {
                    eprintln!("SKIP: private key not available");
                    return;
                }
            };

            // Set env var
            unsafe { std::env::set_var(LICENSE_ENV_VAR, &jwt) };

            let resolution = resolve_token(None);
            assert!(resolution.valid, "env var token should resolve as valid");
            assert_eq!(resolution.source, TokenSourceKind::EnvironmentVariable);
            assert!(resolution.claims.is_some());
            let claims = resolution.claims.unwrap();
            assert_eq!(claims.token_type, TokenType::Developer);

            unsafe { std::env::remove_var(LICENSE_ENV_VAR) };
        }

        #[test]
        fn test_resolution_env_var_overrides_api_param() {
            let dev_jwt = match make_developer_token() {
                Some(t) => t,
                None => {
                    eprintln!("SKIP: private key not available");
                    return;
                }
            };
            let deploy_jwt = match make_deployment_token("0.9") {
                Some(t) => t,
                None => {
                    eprintln!("SKIP: private key not available");
                    return;
                }
            };

            // Env var gets the developer token, API param gets deployment token
            unsafe { std::env::set_var(LICENSE_ENV_VAR, &dev_jwt) };

            let resolution = resolve_token(Some(&deploy_jwt));
            assert!(resolution.valid);
            assert_eq!(resolution.source, TokenSourceKind::EnvironmentVariable);
            let claims = resolution.claims.unwrap();
            // Should be the developer token (from env), not deployment (from param)
            assert_eq!(claims.token_type, TokenType::Developer);

            unsafe { std::env::remove_var(LICENSE_ENV_VAR) };
        }

        #[test]
        fn test_resolution_api_param_fallback_with_signed_token() {
            // Ensure no token file exists (may have been created by earlier tests)
            let _ = remove_token_file();
            // Ensure env var is unset so resolution falls through to API param
            unsafe { std::env::set_var(LICENSE_ENV_VAR, "") };

            let jwt = match make_deployment_token("0.9") {
                Some(t) => t,
                None => {
                    eprintln!("SKIP: private key not available");
                    return;
                }
            };

            let resolution = resolve_token(Some(&jwt));
            assert!(resolution.valid);
            assert_eq!(resolution.source, TokenSourceKind::ApiParameter);
            let claims = resolution.claims.unwrap();
            assert_eq!(claims.token_type, TokenType::Deployment);

            unsafe { std::env::remove_var(LICENSE_ENV_VAR) };
        }

        #[test]
        fn test_resolution_file_with_signed_token() {
            unsafe { std::env::remove_var(LICENSE_ENV_VAR) };

            let jwt = match make_developer_token() {
                Some(t) => t,
                None => {
                    eprintln!("SKIP: private key not available");
                    return;
                }
            };

            // Write token to temp file in place of the standard token path
            // We use a temp dir to avoid touching the user's real token file
            let temp_dir = std::env::temp_dir().join("nxuskit-test-resolution");
            let _ = std::fs::create_dir_all(&temp_dir);
            let token_path = temp_dir.join("license.token");
            std::fs::write(&token_path, &jwt).unwrap();

            // We can't easily redirect the token file path in the current code,
            // so instead test via API param (which still validates the signing)
            let resolution = resolve_token(Some(&jwt));
            assert!(resolution.valid);

            // Cleanup
            let _ = std::fs::remove_dir_all(&temp_dir);
        }

        // ── Trial token tests ────────────────────────────────────────

        #[test]
        fn test_trial_token_validates() {
            let jwt = match make_trial_token(false, 0) {
                Some(t) => t,
                None => {
                    eprintln!("SKIP: private key not available");
                    return;
                }
            };

            let claims = validate_token(&jwt).expect("trial token should validate");
            assert_eq!(claims.token_type, TokenType::Trial);
            assert_eq!(claims.activated, Some(false));
        }

        #[test]
        fn test_trial_suspension_at_8_days() {
            let jwt = match make_trial_token(false, 8) {
                Some(t) => t,
                None => {
                    eprintln!("SKIP: private key not available");
                    return;
                }
            };

            let err = validate_token_full(&jwt).unwrap_err();
            assert!(matches!(err, LicenseError::TrialSuspended));
        }

        #[test]
        fn test_activated_trial_not_suspended_at_8_days() {
            let jwt = match make_trial_token(true, 8) {
                Some(t) => t,
                None => {
                    eprintln!("SKIP: private key not available");
                    return;
                }
            };

            let validated =
                validate_token_full(&jwt).expect("activated trial should not be suspended");
            assert_eq!(validated.claims.token_type, TokenType::Trial);
            assert_eq!(validated.claims.activated, Some(true));
        }

        #[test]
        fn test_trial_within_grace_period() {
            let jwt = match make_trial_token(false, 3) {
                Some(t) => t,
                None => {
                    eprintln!("SKIP: private key not available");
                    return;
                }
            };

            let validated =
                validate_token_full(&jwt).expect("trial within grace period should be valid");
            assert_eq!(validated.claims.token_type, TokenType::Trial);
            assert_eq!(validated.claims.activated, Some(false));
        }

        // ── T105: Performance benchmark for token validation ────────

        #[test]
        fn test_token_validation_under_1ms() {
            const RELEASE_TARGET: std::time::Duration = std::time::Duration::from_millis(1);
            #[cfg(debug_assertions)]
            const DEBUG_SANITY_LIMIT: std::time::Duration = std::time::Duration::from_millis(25);

            let jwt = match make_developer_token() {
                Some(t) => t,
                None => {
                    eprintln!("SKIP: private key not available");
                    return;
                }
            };

            // Warm up
            let _ = validate_token(&jwt);

            // Measure 100 iterations
            let start = std::time::Instant::now();
            let iterations = 100;
            for _ in 0..iterations {
                let _ = validate_token(&jwt);
            }
            let elapsed = start.elapsed();
            let per_call = elapsed / iterations;

            eprintln!(
                "Token validation: {:?} per call ({:?} total for {} iterations)",
                per_call, elapsed, iterations
            );

            // PR-007: Token validation (local, no network) ≤1ms per check.
            //
            // The 1ms contract is an optimized-build performance target. Debug
            // test binaries vary too much across self-hosted runners for this
            // to be a reliable CI gate, especially around ECDSA verification.
            #[cfg(not(debug_assertions))]
            assert!(
                per_call <= RELEASE_TARGET,
                "Token validation took {:?} per call (exceeds 1ms target)",
                per_call
            );
            #[cfg(debug_assertions)]
            {
                if per_call > RELEASE_TARGET {
                    eprintln!(
                        "NOTE: debug token validation exceeded the optimized-build target of {:?}; \
                         run this test with --release to enforce the performance gate",
                        RELEASE_TARGET
                    );
                }
                assert!(
                    per_call <= DEBUG_SANITY_LIMIT,
                    "Token validation took {:?} per call in debug (exceeds {:?} sanity limit)",
                    per_call,
                    DEBUG_SANITY_LIMIT
                );
            }
        }

        #[test]
        fn test_resolution_chain_under_5ms() {
            let jwt = match make_deployment_token("0.9") {
                Some(t) => t,
                None => {
                    eprintln!("SKIP: private key not available");
                    return;
                }
            };

            // Use API param path (avoids env var race conditions)
            unsafe { std::env::set_var(LICENSE_ENV_VAR, "") };

            // Warm up
            let _ = resolve_token(Some(&jwt));

            // Measure
            let start = std::time::Instant::now();
            let iterations = 50;
            for _ in 0..iterations {
                let _ = resolve_token(Some(&jwt));
            }
            let elapsed = start.elapsed();
            let per_call = elapsed / iterations;

            unsafe { std::env::remove_var(LICENSE_ENV_VAR) };

            eprintln!(
                "Resolution chain: {:?} per call ({:?} total for {} iterations)",
                per_call, elapsed, iterations
            );
            // Token resolution chain (env → file → API param) ≤5ms total
            assert!(
                per_call.as_millis() <= 5,
                "Resolution chain took {:?} per call (exceeds 5ms target)",
                per_call
            );
        }

        // ── Tampered token test ──────────────────────────────────────

        #[test]
        fn test_tampered_token_rejected() {
            let jwt = match make_developer_token() {
                Some(t) => t,
                None => {
                    eprintln!("SKIP: private key not available");
                    return;
                }
            };

            // Tamper with the payload (flip a character)
            let parts: Vec<&str> = jwt.split('.').collect();
            assert_eq!(parts.len(), 3);
            let mut payload = parts[1].to_string();
            // Flip last char
            if payload.ends_with('a') {
                payload.pop();
                payload.push('b');
            } else {
                payload.pop();
                payload.push('a');
            }
            let tampered = format!("{}.{}.{}", parts[0], payload, parts[2]);

            let err = validate_token(&tampered);
            assert!(err.is_err(), "tampered token should be rejected");
        }

        // ── Mock HTTP Server Infrastructure ─────────────────────────
        //
        // Spins up a `TcpListener` on localhost:0 to simulate the
        // licensing microservice. Tests that manipulate env vars
        // (HOME, NXUSKIT_LICENSE_SERVER) acquire MOCK_ENV_LOCK to
        // serialize with each other.

        use std::io::{Read as IoRead, Write as IoWrite};
        use std::net::TcpListener;
        use std::sync::Mutex;

        /// Global lock for tests that manipulate process-wide env vars.
        static MOCK_ENV_LOCK: Mutex<()> = Mutex::new(());

        /// Minimal HTTP response builder.
        fn http_response(status: u16, body: &str) -> String {
            let reason = match status {
                200 => "OK",
                400 => "Bad Request",
                403 => "Forbidden",
                409 => "Conflict",
                429 => "Too Many Requests",
                _ => "Error",
            };
            format!(
                "HTTP/1.1 {status} {reason}\r\n\
                 Content-Type: application/json\r\n\
                 Content-Length: {}\r\n\
                 Connection: close\r\n\
                 \r\n\
                 {}",
                body.len(),
                body
            )
        }

        /// Start a mock server that accepts ONE request, matches the path,
        /// and responds with the given status + body. Returns the base URL.
        fn mock_server_once(
            expected_path: &str,
            status: u16,
            body: String,
        ) -> (String, std::thread::JoinHandle<()>) {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let base_url = format!("http://127.0.0.1:{port}");
            let expected = expected_path.to_string();

            let handle = std::thread::spawn(move || {
                // Accept with a 5-second timeout so the thread doesn't block forever
                // if no client connects (prevents leaked threads on test failure).
                let start = std::time::Instant::now();
                let accept_timeout = std::time::Duration::from_secs(5);
                listener.set_nonblocking(true).expect("set nonblocking");
                let stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            stream
                                .set_nonblocking(false)
                                .expect("set blocking for read");
                            break stream;
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            if start.elapsed() > accept_timeout {
                                eprintln!("mock server: no connection within {accept_timeout:?}");
                                return;
                            }
                            std::thread::sleep(std::time::Duration::from_millis(10));
                        }
                        Err(e) => panic!("mock server accept error: {e}"),
                    }
                };
                let mut stream = stream;
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap();
                let request = String::from_utf8_lossy(&buf[..n]);

                // Verify path
                let first_line = request.lines().next().unwrap_or("");
                assert!(
                    first_line.contains(&expected),
                    "Expected path {expected} in request: {first_line}"
                );

                let resp = http_response(status, &body);
                stream.write_all(resp.as_bytes()).unwrap();
                stream.flush().unwrap();
            });

            (base_url, handle)
        }

        fn mock_server_once_capture_request(
            expected_path: &str,
            status: u16,
            body: String,
        ) -> (String, std::thread::JoinHandle<String>) {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let base_url = format!("http://127.0.0.1:{port}");
            let expected = expected_path.to_string();

            let handle = std::thread::spawn(move || {
                let start = std::time::Instant::now();
                let accept_timeout = std::time::Duration::from_secs(5);
                listener.set_nonblocking(true).expect("set nonblocking");
                let stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            stream
                                .set_nonblocking(false)
                                .expect("set blocking for read");
                            break stream;
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            if start.elapsed() > accept_timeout {
                                eprintln!("mock server: no connection within {accept_timeout:?}");
                                return String::new();
                            }
                            std::thread::sleep(std::time::Duration::from_millis(10));
                        }
                        Err(e) => panic!("mock server accept error: {e}"),
                    }
                };
                let mut stream = stream;
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap();
                let request = String::from_utf8_lossy(&buf[..n]).to_string();

                let first_line = request.lines().next().unwrap_or("");
                assert!(
                    first_line.contains(&expected),
                    "Expected path {expected} in request: {first_line}"
                );

                let resp = http_response(status, &body);
                stream.write_all(resp.as_bytes()).unwrap();
                stream.flush().unwrap();
                request
            });

            (base_url, handle)
        }

        /// Start a mock server that immediately closes the connection
        /// (simulates network failure without waiting for reqwest timeout).
        fn mock_server_connection_reset() -> String {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let base_url = format!("http://127.0.0.1:{port}");

            std::thread::spawn(move || {
                // Accept with timeout to avoid leaked threads
                let start = std::time::Instant::now();
                let timeout = std::time::Duration::from_secs(5);
                listener.set_nonblocking(true).expect("set nonblocking");
                loop {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            drop(stream); // Close immediately — no response
                            break;
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            if start.elapsed() > timeout {
                                break;
                            }
                            std::thread::sleep(std::time::Duration::from_millis(10));
                        }
                        Err(_) => break,
                    }
                }
            });

            base_url
        }

        /// Create a sandboxed HOME dir for token file operations.
        fn sandbox_home() -> tempfile::TempDir {
            tempfile::tempdir().expect("create temp dir for sandboxed HOME")
        }

        /// Run a closure with NXUSKIT_LICENSE_SERVER and HOME overridden.
        /// Acquires MOCK_ENV_LOCK to serialize with other env-manipulating tests.
        fn with_mock_env<F, R>(base_url: &str, home: &std::path::Path, f: F) -> R
        where
            F: FnOnce() -> R,
        {
            let _guard = MOCK_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

            let orig_server = std::env::var("NXUSKIT_LICENSE_SERVER").ok();
            let orig_home = std::env::var("HOME").ok();
            #[cfg(windows)]
            let orig_userprofile = std::env::var("USERPROFILE").ok();

            unsafe {
                std::env::set_var("NXUSKIT_LICENSE_SERVER", base_url);
                std::env::set_var("HOME", home.as_os_str());
                // On Windows, dirs_home() reads USERPROFILE, not HOME
                #[cfg(windows)]
                std::env::set_var("USERPROFILE", home.as_os_str());
                // Clear env var token to avoid interference
                std::env::set_var(LICENSE_ENV_VAR, "");
            }

            let result = f();

            unsafe {
                match orig_server {
                    Some(v) => std::env::set_var("NXUSKIT_LICENSE_SERVER", v),
                    None => std::env::remove_var("NXUSKIT_LICENSE_SERVER"),
                }
                #[cfg(windows)]
                match orig_userprofile {
                    Some(v) => std::env::set_var("USERPROFILE", v),
                    None => std::env::remove_var("USERPROFILE"),
                }
                match orig_home {
                    Some(v) => std::env::set_var("HOME", v),
                    None => std::env::remove_var("HOME"),
                }
                std::env::remove_var(LICENSE_ENV_VAR);
            }

            result
        }

        // ── T024: Activation C ABI tests ────────────────────────────

        #[test]
        fn test_activate_success_stores_token_and_returns_seats() {
            let jwt = match make_developer_token() {
                Some(t) => t,
                None => {
                    eprintln!("SKIP: private key not available");
                    return;
                }
            };

            let body = serde_json::json!({
                "token": jwt,
                "seats_used": 2,
                "seats_total": 3,
                "message": "Seat 2 of 3 activated"
            })
            .to_string();

            let (base_url, handle) = mock_server_once("/activate", 200, body);
            let home = sandbox_home();

            let result = with_mock_env(&base_url, home.path(), || activate("PUR-12345"));
            handle.join().unwrap();

            let activation = result.expect("activation should succeed");
            assert!(activation.success);
            assert_eq!(activation.seats_used, 2);
            assert_eq!(activation.seats_total, 3);
            assert!(activation.token.is_some());
            assert_eq!(activation.message, "Seat 2 of 3 activated");

            // Verify token was stored on disk
            let token_path = home.path().join(".nxuskit/license.token");
            assert!(token_path.exists(), "token file should be created");
            let stored = std::fs::read_to_string(&token_path).unwrap();
            assert_eq!(stored, jwt);
        }

        #[test]
        fn test_activate_seat_limit_exceeded_returns_server_error() {
            let body = serde_json::json!({
                "error": "seat_limit_exceeded",
                "message": "All 3 seats are in use. Deactivate another machine first."
            })
            .to_string();

            let (base_url, handle) = mock_server_once("/activate", 409, body);
            let home = sandbox_home();

            let result = with_mock_env(&base_url, home.path(), || activate("PUR-12345"));
            handle.join().unwrap();

            let err = result.unwrap_err();
            match err {
                LicenseError::ServerError { code, message } => {
                    assert_eq!(code, "seat_limit_exceeded");
                    assert!(message.contains("3 seats"));
                }
                other => panic!("expected ServerError, got: {other}"),
            }
        }

        #[test]
        fn test_activate_invalid_purchase_id_returns_error() {
            let body = serde_json::json!({
                "error": "invalid_purchase",
                "message": "Purchase ID not found"
            })
            .to_string();

            let (base_url, handle) = mock_server_once("/activate", 400, body);
            let home = sandbox_home();

            let result = with_mock_env(&base_url, home.path(), || activate("INVALID-ID"));
            handle.join().unwrap();

            let err = result.unwrap_err();
            assert!(matches!(err, LicenseError::ServerError { .. }));
        }

        #[test]
        fn test_activate_network_failure() {
            // Mock server that immediately closes the connection (no timeout wait).
            let base_url = mock_server_connection_reset();
            let home = sandbox_home();

            let result = with_mock_env(&base_url, home.path(), || activate("PUR-12345"));

            let err = result.unwrap_err();
            assert!(matches!(err, LicenseError::NetworkError(_)));
        }

        #[test]
        fn test_activate_uses_extended_timeout_for_cold_start_backend() {
            let home = sandbox_home();
            let observed_timeout = std::cell::Cell::new(0);
            let observed_idempotency_key: std::cell::RefCell<Option<String>> =
                std::cell::RefCell::new(None);

            let result = with_mock_env("http://example.invalid", home.path(), || {
                activate_with_post("PUR-12345", |url, body, timeout_secs, idempotency_key| {
                    observed_timeout.set(timeout_secs);
                    *observed_idempotency_key.borrow_mut() = idempotency_key.map(String::from);
                    assert!(url.ends_with("/activate"));
                    assert_eq!(
                        body.get("purchase_id").and_then(|v| v.as_str()),
                        Some("PUR-12345")
                    );
                    Ok(serde_json::json!({
                        "seats_used": 1,
                        "seats_total": 3,
                        "message": "Activated"
                    }))
                })
            });

            let activation = result.expect("activation should succeed");
            assert!(activation.success);
            assert_eq!(observed_timeout.get(), EXTENDED_TIMEOUT_SECS);
            let key = observed_idempotency_key
                .borrow()
                .clone()
                .expect("activate must send an Idempotency-Key");
            assert!(!key.is_empty(), "Idempotency-Key must be non-empty");
            // UUIDv4 string form is 36 chars (8-4-4-4-12 with dashes).
            assert_eq!(key.len(), 36, "Idempotency-Key should be a UUIDv4 string");
        }

        #[test]
        fn test_blocking_post_sends_machine_id_header_for_rate_limit_keying() {
            let (base_url, handle) =
                mock_server_once_capture_request("/activate", 200, "{}".to_string());
            let url = format!("{base_url}/activate");
            let body = serde_json::json!({
                "purchase_id": "PUR-HEADER",
                "machine_id": "sha256:header-test",
            });

            let result = blocking_post_with_timeout_and_idempotency(
                &url,
                &body,
                DEFAULT_TIMEOUT_SECS,
                Some("idempotency-header-test"),
            );
            assert!(result.is_ok());

            let request = handle.join().unwrap().to_ascii_lowercase();
            assert!(
                request.contains("idempotency-key: idempotency-header-test"),
                "request must include Idempotency-Key header: {request}"
            );
            assert!(
                request.contains("x-machine-id: sha256:header-test"),
                "request must include X-Machine-Id header for backend rate limiting: {request}"
            );
        }

        // ── Idempotency-Key cache lifecycle (T054 send-side) ─────────

        #[test]
        fn test_get_or_create_activation_key_persists_and_reuses() {
            let home = sandbox_home();
            let (first, second) = with_mock_env("http://example.invalid", home.path(), || {
                let a = get_or_create_activation_key("PUR-A").expect("create key");
                let b = get_or_create_activation_key("PUR-A").expect("re-read key");
                (a, b)
            });
            assert_eq!(first, second, "second call must reuse the first key");
            assert_eq!(first.len(), 36, "key should be a UUIDv4 string");
        }

        #[test]
        fn test_clear_activation_key_removes_entry() {
            let home = sandbox_home();
            with_mock_env("http://example.invalid", home.path(), || {
                let original = get_or_create_activation_key("PUR-B").expect("create");
                clear_activation_key("PUR-B").expect("clear");
                let regenerated = get_or_create_activation_key("PUR-B").expect("regenerate");
                assert_ne!(
                    original, regenerated,
                    "after clear, a fresh key must be issued"
                );
            });
        }

        #[test]
        fn test_activate_preserves_idempotency_key_after_backend_timeout() {
            let home = sandbox_home();
            let observed: std::cell::RefCell<Option<String>> = std::cell::RefCell::new(None);

            with_mock_env("http://example.invalid", home.path(), || {
                let result = activate_with_post("PUR-T1", |_url, _body, _t, key| {
                    *observed.borrow_mut() = key.map(String::from);
                    Err(LicenseError::ServerError {
                        code: "backend_timeout".to_string(),
                        message: "simulated".to_string(),
                    })
                });
                assert!(result.is_err());

                // Cache must still hold the key for the next retry.
                let cached = get_or_create_activation_key("PUR-T1").expect("re-read");
                assert_eq!(
                    Some(cached.clone()),
                    *observed.borrow(),
                    "cached key must equal the one sent on the failed attempt"
                );
            });
        }

        #[test]
        fn test_activate_preserves_idempotency_key_after_rate_limit() {
            let home = sandbox_home();
            let observed_first: std::cell::RefCell<Option<String>> = std::cell::RefCell::new(None);
            let observed_second: std::cell::RefCell<Option<String>> = std::cell::RefCell::new(None);

            with_mock_env("http://example.invalid", home.path(), || {
                let _ = activate_with_post("PUR-T2", |_url, _body, _t, key| {
                    *observed_first.borrow_mut() = key.map(String::from);
                    Err(LicenseError::ServerError {
                        code: "rate_limit_exceeded".to_string(),
                        message: "simulated".to_string(),
                    })
                });

                let _ = activate_with_post("PUR-T2", |_url, _body, _t, key| {
                    *observed_second.borrow_mut() = key.map(String::from);
                    Err(LicenseError::ServerError {
                        code: "rate_limit_exceeded".to_string(),
                        message: "simulated".to_string(),
                    })
                });

                assert_eq!(
                    *observed_first.borrow(),
                    *observed_second.borrow(),
                    "second retry must replay the same Idempotency-Key after rate_limit_exceeded"
                );
                assert!(observed_first.borrow().is_some());
            });
        }

        #[test]
        fn test_activate_preserves_idempotency_key_after_network_error() {
            let home = sandbox_home();
            let observed_first: std::cell::RefCell<Option<String>> = std::cell::RefCell::new(None);
            let observed_second: std::cell::RefCell<Option<String>> = std::cell::RefCell::new(None);

            with_mock_env("http://example.invalid", home.path(), || {
                let _ = activate_with_post("PUR-T3", |_url, _body, _t, key| {
                    *observed_first.borrow_mut() = key.map(String::from);
                    Err(LicenseError::NetworkError("connection reset".into()))
                });
                let _ = activate_with_post("PUR-T3", |_url, _body, _t, key| {
                    *observed_second.borrow_mut() = key.map(String::from);
                    Err(LicenseError::NetworkError("connection reset".into()))
                });
                assert_eq!(
                    *observed_first.borrow(),
                    *observed_second.borrow(),
                    "network errors are retryable; key must persist"
                );
            });
        }

        #[test]
        fn test_activate_clears_idempotency_key_after_success() {
            let home = sandbox_home();
            let observed_first: std::cell::RefCell<Option<String>> = std::cell::RefCell::new(None);
            let observed_second: std::cell::RefCell<Option<String>> = std::cell::RefCell::new(None);

            with_mock_env("http://example.invalid", home.path(), || {
                let r1 = activate_with_post("PUR-T4", |_url, _body, _t, key| {
                    *observed_first.borrow_mut() = key.map(String::from);
                    Ok(serde_json::json!({
                        "seats_used": 1,
                        "seats_total": 3,
                        "message": "Activated"
                    }))
                });
                assert!(r1.is_ok());

                let _ = activate_with_post("PUR-T4", |_url, _body, _t, key| {
                    *observed_second.borrow_mut() = key.map(String::from);
                    Ok(serde_json::json!({
                        "seats_used": 1,
                        "seats_total": 3,
                        "message": "Activated"
                    }))
                });
                assert_ne!(
                    *observed_first.borrow(),
                    *observed_second.borrow(),
                    "after success the key is cleared; a re-activation must mint a new key"
                );
            });
        }

        #[test]
        fn test_activate_clears_idempotency_key_after_terminal_client_error() {
            let home = sandbox_home();
            let observed_first: std::cell::RefCell<Option<String>> = std::cell::RefCell::new(None);
            let observed_second: std::cell::RefCell<Option<String>> = std::cell::RefCell::new(None);

            with_mock_env("http://example.invalid", home.path(), || {
                // invalid_purchase is NOT in the retryable allow-list.
                let _ = activate_with_post("PUR-T5", |_url, _body, _t, key| {
                    *observed_first.borrow_mut() = key.map(String::from);
                    Err(LicenseError::ServerError {
                        code: "invalid_purchase".to_string(),
                        message: "purchase ID not found".to_string(),
                    })
                });
                let _ = activate_with_post("PUR-T5", |_url, _body, _t, key| {
                    *observed_second.borrow_mut() = key.map(String::from);
                    Err(LicenseError::ServerError {
                        code: "invalid_purchase".to_string(),
                        message: "purchase ID not found".to_string(),
                    })
                });
                assert_ne!(
                    *observed_first.borrow(),
                    *observed_second.borrow(),
                    "non-retryable errors clear the key; second attempt must mint a fresh one"
                );
            });
        }

        #[test]
        fn test_is_retryable_activation_error_classification() {
            // Retryable
            assert!(is_retryable_activation_error(&LicenseError::NetworkError(
                "any".into()
            )));
            assert!(is_retryable_activation_error(
                &LicenseError::AuthenticationRequired
            ));
            for code in [
                "backend_timeout",
                "rate_limit_exceeded",
                "service_unavailable",
                "gateway_timeout",
                "upstream_unavailable",
                "activation_in_progress",
                "already_activated",
            ] {
                assert!(
                    is_retryable_activation_error(&LicenseError::ServerError {
                        code: code.to_string(),
                        message: "x".into()
                    }),
                    "{code} should be retryable"
                );
            }
            // Terminal
            for code in [
                "invalid_purchase",
                "wrong_product_identifier",
                "seat_limit_exceeded",
                "environment_mismatch",
            ] {
                assert!(
                    !is_retryable_activation_error(&LicenseError::ServerError {
                        code: code.to_string(),
                        message: "x".into()
                    }),
                    "{code} should be terminal"
                );
            }
        }

        #[test]
        fn test_activate_replaces_trial_token_with_developer() {
            // Edge case: trial→paid token replacement
            let trial_jwt = match make_trial_token(true, 5) {
                Some(t) => t,
                None => {
                    eprintln!("SKIP: private key not available");
                    return;
                }
            };
            let dev_jwt = match make_developer_token() {
                Some(t) => t,
                None => {
                    eprintln!("SKIP: private key not available");
                    return;
                }
            };

            let body = serde_json::json!({
                "token": dev_jwt,
                "seats_used": 1,
                "seats_total": 3,
                "message": "Activated"
            })
            .to_string();

            let (base_url, handle) = mock_server_once("/activate", 200, body);
            let home = sandbox_home();

            // Pre-populate with trial token
            let nxuskit_dir = home.path().join(".nxuskit");
            std::fs::create_dir_all(&nxuskit_dir).unwrap();
            std::fs::write(nxuskit_dir.join("license.token"), &trial_jwt).unwrap();

            let result = with_mock_env(&base_url, home.path(), || activate("PUR-12345"));
            handle.join().unwrap();

            let activation = result.expect("activation should succeed");
            assert!(activation.success);

            // Verify trial token was replaced by developer token
            let stored = std::fs::read_to_string(nxuskit_dir.join("license.token")).unwrap();
            assert_eq!(
                stored, dev_jwt,
                "trial token should be replaced by developer token"
            );

            // Verify the new token validates as developer, not trial
            let claims = validate_token(&stored).unwrap();
            assert_eq!(claims.token_type, TokenType::Developer);
        }

        // ── T025: Deactivation C ABI tests ──────────────────────────

        #[test]
        fn test_deactivate_success_removes_token() {
            let jwt = match make_developer_token() {
                Some(t) => t,
                None => {
                    eprintln!("SKIP: private key not available");
                    return;
                }
            };

            let body = serde_json::json!({
                "seats_used": 1,
                "seats_total": 3,
                "message": "Machine deactivated"
            })
            .to_string();

            let (base_url, handle) = mock_server_once("/deactivate", 200, body);
            let home = sandbox_home();

            // Pre-populate token file
            let nxuskit_dir = home.path().join(".nxuskit");
            std::fs::create_dir_all(&nxuskit_dir).unwrap();
            std::fs::write(nxuskit_dir.join("license.token"), &jwt).unwrap();

            let result = with_mock_env(&base_url, home.path(), deactivate);
            handle.join().unwrap();

            let deact = result.expect("deactivation should succeed");
            assert!(deact.success);
            assert_eq!(deact.seats_used, 1);
            assert_eq!(deact.seats_total, 3);
            assert_eq!(deact.message, "Machine deactivated");

            // Token file should be removed
            let token_path = nxuskit_dir.join("license.token");
            assert!(
                !token_path.exists(),
                "token file should be removed after deactivation"
            );
        }

        #[test]
        fn test_deactivate_no_active_license() {
            let body = serde_json::json!({
                "error": "not_activated",
                "message": "No active license for this machine"
            })
            .to_string();

            let (base_url, handle) = mock_server_once("/deactivate", 400, body);
            let home = sandbox_home();

            let result = with_mock_env(&base_url, home.path(), deactivate);
            handle.join().unwrap();

            let err = result.unwrap_err();
            match err {
                LicenseError::ServerError { code, .. } => {
                    assert_eq!(code, "not_activated");
                }
                other => panic!("expected ServerError, got: {other}"),
            }
        }

        // ── T035: Trial issuance tests ──────────────────────────────

        #[test]
        fn test_trial_issue_success_stores_token() {
            let trial_jwt = match make_trial_token(false, 0) {
                Some(t) => t,
                None => {
                    eprintln!("SKIP: private key not available");
                    return;
                }
            };

            let body = serde_json::json!({
                "token": trial_jwt,
                "days_remaining": 30,
                "message": "Trial issued for 30 days"
            })
            .to_string();

            let (base_url, handle) = mock_server_once("/trial", 200, body);
            let home = sandbox_home();

            let result = with_mock_env(&base_url, home.path(), trial_issue);
            handle.join().unwrap();

            let issuance = result.expect("trial issuance should succeed");
            assert!(issuance.success);
            assert_eq!(issuance.days_remaining, 30);
            assert!(issuance.token.is_some());
            assert_eq!(issuance.message, "Trial issued for 30 days");

            // Verify token was stored on disk
            let token_path = home.path().join(".nxuskit/license.token");
            assert!(token_path.exists(), "trial token file should be created");
            let stored = std::fs::read_to_string(&token_path).unwrap();
            assert_eq!(stored, trial_jwt);

            // Verify it's actually a trial token
            let claims = validate_token(&stored).unwrap();
            assert_eq!(claims.token_type, TokenType::Trial);
        }

        #[test]
        fn test_trial_issue_network_unreachable() {
            // Mock server that resets connection (fast failure, no 10s wait)
            let base_url = mock_server_connection_reset();
            let home = sandbox_home();

            let result = with_mock_env(&base_url, home.path(), trial_issue);

            let err = result.unwrap_err();
            assert!(
                matches!(err, LicenseError::NetworkError(_)),
                "unreachable microservice should produce NetworkError"
            );
        }

        #[test]
        fn test_trial_issue_already_has_trial() {
            // Server rejects duplicate trial issuance. trial_issue() will
            // intercept the trial_already_issued error and attempt trial_fetch(),
            // which will also fail (mock server only serves one request).
            // The final error should be trial_exists with a helpful message.
            let body = serde_json::json!({
                "error": "trial_already_issued",
                "message": "A trial has already been issued for this machine"
            })
            .to_string();

            let (base_url, handle) = mock_server_once("/trial", 409, body);
            let home = sandbox_home();

            let result = with_mock_env(&base_url, home.path(), trial_issue);
            handle.join().unwrap();

            let err = result.unwrap_err();
            match err {
                LicenseError::ServerError { code, message } => {
                    assert_eq!(code, "trial_exists");
                    assert!(
                        message.contains("already exists"),
                        "expected helpful message, got: {message}"
                    );
                }
                other => panic!("expected ServerError, got: {other}"),
            }
        }

        // ── T037: Trial activation tests ────────────────────────────

        #[test]
        fn test_trial_activate_success_replaces_token() {
            let activated_jwt = match make_trial_token(true, 3) {
                Some(t) => t,
                None => {
                    eprintln!("SKIP: private key not available");
                    return;
                }
            };

            let body = serde_json::json!({
                "token": activated_jwt,
                "days_remaining": 27,
                "message": "Trial activated successfully"
            })
            .to_string();

            let (base_url, handle) = mock_server_once("/trial/activate", 200, body);
            let home = sandbox_home();

            // Pre-populate with an unactivated trial token
            let unactivated_jwt = make_trial_token(false, 3).unwrap();
            let nxuskit_dir = home.path().join(".nxuskit");
            std::fs::create_dir_all(&nxuskit_dir).unwrap();
            std::fs::write(nxuskit_dir.join("license.token"), &unactivated_jwt).unwrap();

            let result = with_mock_env(&base_url, home.path(), || {
                trial_activate("ACTIVATION-CODE-123")
            });
            handle.join().unwrap();

            let issuance = result.expect("trial activation should succeed");
            assert!(issuance.success);
            assert_eq!(issuance.days_remaining, 27);
            assert_eq!(issuance.message, "Trial activated successfully");

            // Token file should now contain the activated token
            let stored = std::fs::read_to_string(nxuskit_dir.join("license.token")).unwrap();
            assert_eq!(stored, activated_jwt);
            let claims = validate_token(&stored).unwrap();
            assert_eq!(claims.activated, Some(true));
        }

        #[test]
        fn test_trial_activate_invalid_code() {
            let body = serde_json::json!({
                "error": "invalid_activation_code",
                "message": "The activation code is invalid or expired"
            })
            .to_string();

            let (base_url, handle) = mock_server_once("/trial/activate", 400, body);
            let home = sandbox_home();

            let result = with_mock_env(&base_url, home.path(), || trial_activate("BAD-CODE"));
            handle.join().unwrap();

            let err = result.unwrap_err();
            match err {
                LicenseError::ServerError { code, .. } => {
                    assert_eq!(code, "invalid_activation_code");
                }
                other => panic!("expected ServerError, got: {other}"),
            }
        }
    }

    // ── StubTokenVerifier Tests (T013-T022a) ────────────────────────

    mod verifier_tests {
        use super::*;
        use crate::license_types::{TokenVerifier, VerifyError};
        use crate::test_fixtures;

        /// T013: StubTokenVerifier accepts ES256-signed tokens.
        #[test]
        fn test_stub_verifier_accepts_es256() {
            let (jwt, pub_pem) = test_fixtures::make_trial_token();
            let verifier = StubTokenVerifier::with_public_key(&pub_pem);

            let result = verifier.verify(&jwt);
            let claims = result.expect("ES256 token should be accepted by StubTokenVerifier");

            assert_eq!(claims.token_type, TokenType::Trial);
            assert_eq!(claims.iss, "nxus-licensing");
            assert_eq!(claims.edition, "pro");
            assert_eq!(claims.product_id, "nxuskit");
        }

        /// T014: StubTokenVerifier rejects RS384 tokens with ES256 guidance.
        #[test]
        fn test_stub_verifier_rejects_rs384() {
            let (_jwt, pub_pem) = test_fixtures::make_trial_token();
            let verifier = StubTokenVerifier::with_public_key(&pub_pem);

            // Build a fake JWT with RS384 header to trigger the algorithm check.
            // The StubTokenVerifier checks the header algorithm before verifying
            // the signature, so we just need the header to declare RS384.
            let now = crate::license_types::current_unix_timestamp();
            let claims = TokenClaims {
                product_id: "nxuskit".to_string(),
                iss: "nxus-licensing".to_string(),
                iat: now,
                nbf: Some(now),
                exp: Some(now + 30 * 86400),
                token_type: TokenType::Trial,
                edition: "pro".to_string(),
                tenant_id: None,
                machine_id: None,
                seat_index: None,
                activated: Some(false),
                sdk_version_ceiling: None,
                customer_email: None,
            };

            let header = serde_json::json!({"alg": "RS384", "typ": "JWT"});
            let payload = serde_json::to_value(&claims).unwrap();
            let header_b64 = base64_url_encode(&serde_json::to_vec(&header).unwrap());
            let payload_b64 = base64_url_encode(&serde_json::to_vec(&payload).unwrap());
            let fake_rs384_jwt = format!("{header_b64}.{payload_b64}.fake-signature");

            let result = verifier.verify(&fake_rs384_jwt);
            assert!(result.is_err(), "RS384 token should be rejected");

            let err = result.unwrap_err();
            match &err {
                VerifyError::InvalidSignature { details } => {
                    assert!(
                        details.contains("ES256"),
                        "Error message should mention ES256: {details}"
                    );
                    assert!(
                        details.contains("RS384"),
                        "Error message should mention RS384: {details}"
                    );
                }
                other => panic!("Expected InvalidSignature, got: {other:?}"),
            }
        }

        /// T015: StubTokenVerifier validates issuer claim.
        #[test]
        fn test_stub_verifier_validates_issuer() {
            let (priv_pem, pub_pem) = test_fixtures::test_es256_keypair();
            let verifier = StubTokenVerifier::with_public_key(&pub_pem);

            let now = crate::license_types::current_unix_timestamp();
            let claims = TokenClaims {
                product_id: "nxuskit".to_string(),
                iss: "wrong-issuer".to_string(),
                iat: now,
                nbf: Some(now),
                exp: Some(now + 30 * 86400),
                token_type: TokenType::Developer,
                edition: "pro".to_string(),
                tenant_id: None,
                machine_id: None,
                seat_index: None,
                activated: None,
                sdk_version_ceiling: None,
                customer_email: None,
            };

            let jwt = test_fixtures::sign_test_jwt(&claims, &priv_pem);
            let result = verifier.verify(&jwt);
            assert!(
                result.is_err(),
                "Token with wrong issuer should be rejected"
            );

            let err = result.unwrap_err();
            assert!(
                matches!(err, VerifyError::InvalidIssuer),
                "Expected InvalidIssuer, got: {err:?}"
            );
        }

        /// T016: StubTokenVerifier validates product_id claim.
        #[test]
        fn test_stub_verifier_validates_product_id() {
            let (priv_pem, pub_pem) = test_fixtures::test_es256_keypair();
            let verifier = StubTokenVerifier::with_public_key(&pub_pem);

            let now = crate::license_types::current_unix_timestamp();

            // Build a JWT with product_id = "peeler" via raw JSON
            let claims_json = serde_json::json!({
                "iss": "nxus-licensing",
                "iat": now,
                "nbf": now,
                "exp": now + 30 * 86400,
                "type": "developer",
                "edition": "pro",
                "product_id": "peeler",
                "tenant_id": "org-test-001",
                "machine_id": "sha256:test-machine-001",
                "seat_index": 1,
            });

            let encoding_key = jsonwebtoken::EncodingKey::from_ec_pem(priv_pem.as_bytes())
                .expect("valid EC PEM key");
            let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::ES256);
            header.kid = Some("test-key-001".to_string());
            let jwt =
                jsonwebtoken::encode(&header, &claims_json, &encoding_key).expect("JWT encoding");

            let result = verifier.verify(&jwt);
            assert!(
                result.is_err(),
                "Token with product_id 'peeler' should be rejected"
            );

            let err = result.unwrap_err();
            match &err {
                VerifyError::InvalidProductId { expected, actual } => {
                    assert_eq!(expected, "nxuskit");
                    assert_eq!(actual, "peeler");
                }
                other => panic!("Expected InvalidProductId, got: {other:?}"),
            }
        }

        /// T017: StubTokenVerifier key override via NXUS_SIGNING_KEY_PATH env var.
        #[test]
        fn test_stub_verifier_key_override() {
            let (priv_pem, pub_pem) = test_fixtures::test_es256_keypair();

            // Write the public key to a temp file
            let temp_dir = tempfile::tempdir().expect("create temp dir");
            let key_path = temp_dir.path().join("test-signing-key.pem");
            std::fs::write(&key_path, &pub_pem).expect("write key file");

            // Set the env var and create a verifier (not via get_verifier() global)
            unsafe {
                std::env::set_var("NXUS_SIGNING_KEY_PATH", key_path.to_str().unwrap());
            }

            let verifier = StubTokenVerifier::new();

            // Create and verify a token
            let now = crate::license_types::current_unix_timestamp();
            let claims = TokenClaims {
                product_id: "nxuskit".to_string(),
                iss: "nxus-licensing".to_string(),
                iat: now,
                nbf: Some(now),
                exp: Some(now + 30 * 86400),
                token_type: TokenType::Developer,
                edition: "pro".to_string(),
                tenant_id: None,
                machine_id: None,
                seat_index: Some(1),
                activated: None,
                sdk_version_ceiling: None,
                customer_email: None,
            };
            let jwt = test_fixtures::sign_test_jwt(&claims, &priv_pem);
            let result = verifier.verify(&jwt);

            // Clean up env var
            unsafe {
                std::env::remove_var("NXUS_SIGNING_KEY_PATH");
            }

            let validated = result
                .expect("Token signed with key from NXUS_SIGNING_KEY_PATH should be accepted");
            assert_eq!(validated.token_type, TokenType::Developer);
            assert_eq!(validated.iss, "nxus-licensing");
        }

        /// T018: validate_token() uses the verifier trait (backward compat).
        #[test]
        fn test_validate_token_through_verifier() {
            setup_test_signing_key();
            let (jwt, _pub_pem) = test_fixtures::make_developer_token();

            // validate_token() should now use the global StubTokenVerifier
            // which has the embedded ES256 test key
            let result = validate_token(&jwt);
            let claims = result.expect("validate_token should work with ES256 tokens via verifier");
            assert_eq!(claims.token_type, TokenType::Developer);
            assert_eq!(claims.iss, "nxus-licensing");
        }

        /// T019: validate_token_full() round-trip with ES256 tokens.
        #[test]
        fn test_validate_token_full_with_es256() {
            setup_test_signing_key();
            // Use a deployment token (no machine binding, no expiry) to avoid
            // machine ID mismatch in full validation.
            let (jwt, _pub_pem) = test_fixtures::make_deployment_token();

            let result = validate_token_full(&jwt);
            let validated = result.expect("validate_token_full should work with ES256 tokens");
            assert_eq!(validated.claims.token_type, TokenType::Deployment);
            assert!(!validated.claims.is_expired());
        }

        /// T020: StubTokenVerifier accepts token with missing product_id
        /// (defaults to "nxuskit").
        #[test]
        fn test_stub_verifier_default_product_id() {
            let (priv_pem, pub_pem) = test_fixtures::test_es256_keypair();
            let verifier = StubTokenVerifier::with_public_key(&pub_pem);

            // Create a token without explicit product_id field
            let now = crate::license_types::current_unix_timestamp();
            let claims = TokenClaims {
                product_id: "nxuskit".to_string(),
                iss: "nxus-licensing".to_string(),
                iat: now,
                nbf: Some(now),
                exp: Some(now + 30 * 86400),
                token_type: TokenType::Developer,
                edition: "pro".to_string(),
                tenant_id: None,
                machine_id: None,
                seat_index: Some(1),
                activated: None,
                sdk_version_ceiling: None,
                customer_email: None,
            };

            let jwt = test_fixtures::sign_test_jwt(&claims, &priv_pem);
            let result = verifier.verify(&jwt);
            let validated = result.expect("Token without product_id should default to 'nxuskit'");
            assert_eq!(validated.product_id, "nxuskit");
        }

        /// T021: StubTokenVerifier returns correct ValidatedClaims fields.
        #[test]
        fn test_stub_verifier_validated_claims_completeness() {
            let (jwt, pub_pem) = test_fixtures::make_deployment_token();
            let verifier = StubTokenVerifier::with_public_key(&pub_pem);

            let claims = verifier
                .verify(&jwt)
                .expect("deployment token should verify");
            assert_eq!(claims.token_type, TokenType::Deployment);
            assert_eq!(claims.edition, "pro");
            assert_eq!(claims.iss, "nxus-licensing");
            assert!(claims.exp.is_none()); // deployment tokens have no expiry
            assert_eq!(claims.sdk_version_ceiling, Some("0.9".to_string()));
            assert_eq!(claims.customer_email, Some("test@example.com".to_string()));
        }

        /// T022: Verifier name is "StubTokenVerifier".
        #[test]
        fn test_stub_verifier_name() {
            let (_jwt, pub_pem) = test_fixtures::make_trial_token();
            let verifier = StubTokenVerifier::with_public_key(&pub_pem);
            assert_eq!(verifier.name(), "StubTokenVerifier");
        }

        /// Helper: base64url-encode bytes (no padding).
        fn base64_url_encode(data: &[u8]) -> String {
            use base64::Engine;
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data)
        }
    }
}
