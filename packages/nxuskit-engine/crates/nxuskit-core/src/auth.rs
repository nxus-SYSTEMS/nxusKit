//! Auth helper — credential storage, resolution, and status.
//!
//! Provides set/remove/resolve/status operations for provider API keys.
//! Credential resolution follows deterministic precedence:
//!   explicit > env var > OS credential store > none
//!
//! OS credential store access uses the `keyring` crate. When the store is
//! unavailable (headless Linux, etc.), a file-based fallback with 0600
//! permissions is used.

use serde::Serialize;
use std::path::PathBuf;

use super::auth_metadata;

// ── Types ─────────────────────────────────────────────────────────

/// Source from which a credential was resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialSource {
    Explicit,
    Env,
    Store,
    None,
}

/// Auth status for a provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthStatusKind {
    /// Credential was passed explicitly by the caller (used by resolve + wrapper status).
    #[allow(dead_code)]
    AuthenticatedExplicit,
    AuthenticatedEnv,
    AuthenticatedStore,
    AuthenticatedOAuth,
    NotAuthenticated,
    NotRequired,
}

/// Resolution result returned by `resolve()`.
#[derive(Debug, Clone, Serialize)]
pub struct AuthResolution {
    pub provider_id: String,
    pub source: CredentialSource,
    pub has_credential: bool,
}

/// Full auth status for a provider.
#[derive(Debug, Clone, Serialize)]
pub struct AuthStatus {
    pub provider_id: String,
    pub status: AuthStatusKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub masked_preview: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dashboard_url: Option<String>,
    /// Whether this provider supports OAuth authentication.
    pub oauth_capable: bool,
    /// Auth methods supported by this provider (e.g., ["api_key"], ["api_key", "oauth"]).
    pub auth_methods: Vec<String>,
}

// ── Credential Store Backend ──────────────────────────────────────

/// Attempt to store a credential in the OS keyring.
/// Returns `Ok(())` on success, `Err(msg)` on failure.
fn keyring_set(service: &str, key: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(service, "default")
        .map_err(|e| format!("keyring entry creation failed: {e}"))?;
    entry
        .set_password(key)
        .map_err(|e| format!("keyring set failed: {e}"))
}

/// Attempt to read a credential from the OS keyring.
fn keyring_get(service: &str) -> Option<String> {
    let entry = keyring::Entry::new(service, "default").ok()?;
    entry.get_password().ok()
}

/// Attempt to delete a credential from the OS keyring.
fn keyring_delete(service: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(service, "default")
        .map_err(|e| format!("keyring entry creation failed: {e}"))?;
    entry
        .delete_credential()
        .map_err(|e| format!("keyring delete failed: {e}"))
}

// ── File-based Fallback ───────────────────────────────────────────

/// Get the file-based credential store directory.
fn credential_file_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("NXUSKIT_CREDENTIALS_DIR") {
        PathBuf::from(dir)
    } else if let Some(home) = dirs_path() {
        home.join(".nxuskit").join("credentials")
    } else {
        PathBuf::from("/tmp/.nxuskit-credentials")
    }
}

/// Platform-appropriate home directory.
fn dirs_path() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        std::env::var("HOME").ok().map(PathBuf::from)
    }
    #[cfg(windows)]
    {
        std::env::var("USERPROFILE").ok().map(PathBuf::from)
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}

fn credential_file_path(service: &str) -> PathBuf {
    credential_file_dir().join(format!("{service}.key"))
}

/// Write credential to file with 0600 permissions.
fn file_set(service: &str, key: &str) -> Result<(), String> {
    let dir = credential_file_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("create credential dir: {e}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o700);
        let _ = std::fs::set_permissions(&dir, perms);
    }

    let path = credential_file_path(service);
    std::fs::write(&path, key).map_err(|e| format!("write credential file: {e}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&path, perms).map_err(|e| format!("set file permissions: {e}"))?;
    }

    Ok(())
}

/// Read credential from file.
fn file_get(service: &str) -> Option<String> {
    let path = credential_file_path(service);
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

/// Delete credential file.
fn file_delete(service: &str) -> Result<(), String> {
    let path = credential_file_path(service);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| format!("delete credential file: {e}"))?;
    }
    Ok(())
}

// ── Masked Preview ────────────────────────────────────────────────

/// Generate a masked preview: first 3 chars + "..." + last 4 chars.
/// For short keys (< 10 chars), show "****".
pub fn masked_preview(key: &str) -> String {
    if key.len() < 10 {
        "****".to_string()
    } else {
        let prefix = &key[..3];
        let suffix = &key[key.len() - 4..];
        format!("{prefix}...{suffix}")
    }
}

// ── Public API ────────────────────────────────────────────────────

/// Store a credential for a provider.
///
/// Tries OS keyring first; falls back to file-based storage.
/// Never logs `api_key` in plain text.
pub fn set_credential(provider_id: &str, api_key: &str) -> Result<(), String> {
    let meta = auth_metadata::lookup(provider_id)
        .ok_or_else(|| format!("unknown provider: {provider_id}"))?;

    if !meta.auth_required {
        return Err(format!(
            "provider '{provider_id}' does not require authentication"
        ));
    }

    log::debug!("Setting credential for provider '{provider_id}'");

    match keyring_set(meta.credential_service_name, api_key) {
        Ok(()) => {
            log::debug!("Credential stored in OS keyring for '{provider_id}'");
            Ok(())
        }
        Err(e) => {
            log::warn!("OS keyring unavailable for '{provider_id}': {e}; using file fallback");
            file_set(meta.credential_service_name, api_key)
        }
    }
}

/// Remove a stored credential for a provider.
pub fn remove_credential(provider_id: &str) -> Result<(), String> {
    let meta = auth_metadata::lookup(provider_id)
        .ok_or_else(|| format!("unknown provider: {provider_id}"))?;

    log::debug!("Removing credential for provider '{provider_id}'");

    // Try both keyring and file — remove from wherever it exists
    let kr_result = keyring_delete(meta.credential_service_name);
    let file_result = file_delete(meta.credential_service_name);

    if kr_result.is_err() && file_result.is_err() {
        Err(format!("no credential found for '{provider_id}'"))
    } else {
        Ok(())
    }
}

/// Resolve a credential using deterministic precedence:
/// explicit > env > store > none.
pub fn resolve(provider_id: &str, explicit_key: Option<&str>) -> Result<AuthResolution, String> {
    let meta = auth_metadata::lookup(provider_id)
        .ok_or_else(|| format!("unknown provider: {provider_id}"))?;

    // Local providers don't need credentials
    if !meta.auth_required {
        return Ok(AuthResolution {
            provider_id: provider_id.to_string(),
            source: CredentialSource::None,
            has_credential: false,
        });
    }

    // 1. Explicit key
    if explicit_key.is_some() {
        return Ok(AuthResolution {
            provider_id: provider_id.to_string(),
            source: CredentialSource::Explicit,
            has_credential: true,
        });
    }

    // 2. Environment variable
    if std::env::var(meta.env_var_name).is_ok() {
        return Ok(AuthResolution {
            provider_id: provider_id.to_string(),
            source: CredentialSource::Env,
            has_credential: true,
        });
    }

    // 3. OS credential store (keyring or file fallback)
    if keyring_get(meta.credential_service_name).is_some()
        || file_get(meta.credential_service_name).is_some()
    {
        return Ok(AuthResolution {
            provider_id: provider_id.to_string(),
            source: CredentialSource::Store,
            has_credential: true,
        });
    }

    // 4. None
    Ok(AuthResolution {
        provider_id: provider_id.to_string(),
        source: CredentialSource::None,
        has_credential: false,
    })
}

/// Get the actual credential value (for internal use by provider creation).
/// Never exposed via C ABI — used internally to bridge auth resolution to
/// provider initialization.
#[allow(dead_code)]
pub(crate) fn resolve_credential_value(
    provider_id: &str,
    explicit_key: Option<&str>,
) -> Option<String> {
    let meta = auth_metadata::lookup(provider_id)?;

    if !meta.auth_required {
        return None;
    }

    // 1. Explicit
    if let Some(key) = explicit_key {
        return Some(key.to_string());
    }

    // 2. Env
    if let Ok(val) = std::env::var(meta.env_var_name) {
        return Some(val);
    }

    // 3. Store (keyring then file)
    if let Some(val) = keyring_get(meta.credential_service_name) {
        return Some(val);
    }
    if let Some(val) = file_get(meta.credential_service_name) {
        return Some(val);
    }

    None
}

/// Get auth status for a single provider.
pub fn status(provider_id: &str) -> Result<AuthStatus, String> {
    let meta = auth_metadata::lookup(provider_id)
        .ok_or_else(|| format!("unknown provider: {provider_id}"))?;

    let auth_methods: Vec<String> = meta.auth_methods.iter().map(|s| s.to_string()).collect();

    if !meta.auth_required {
        return Ok(AuthStatus {
            provider_id: provider_id.to_string(),
            status: AuthStatusKind::NotRequired,
            masked_preview: None,
            source: None,
            dashboard_url: meta.dashboard_url.map(String::from),
            oauth_capable: meta.oauth_capable,
            auth_methods,
        });
    }

    // Check env first
    if let Ok(val) = std::env::var(meta.env_var_name) {
        return Ok(AuthStatus {
            provider_id: provider_id.to_string(),
            status: AuthStatusKind::AuthenticatedEnv,
            masked_preview: Some(masked_preview(&val)),
            source: Some("env".to_string()),
            dashboard_url: meta.dashboard_url.map(String::from),
            oauth_capable: meta.oauth_capable,
            auth_methods,
        });
    }

    // Check store (keyring then file)
    if let Some(val) =
        keyring_get(meta.credential_service_name).or_else(|| file_get(meta.credential_service_name))
    {
        return Ok(AuthStatus {
            provider_id: provider_id.to_string(),
            status: AuthStatusKind::AuthenticatedStore,
            masked_preview: Some(masked_preview(&val)),
            source: Some("store".to_string()),
            dashboard_url: meta.dashboard_url.map(String::from),
            oauth_capable: meta.oauth_capable,
            auth_methods,
        });
    }

    Ok(AuthStatus {
        provider_id: provider_id.to_string(),
        status: AuthStatusKind::NotAuthenticated,
        masked_preview: None,
        source: None,
        dashboard_url: meta.dashboard_url.map(String::from),
        oauth_capable: meta.oauth_capable,
        auth_methods,
    })
}

/// Get auth status for all known providers.
pub fn status_all() -> Vec<AuthStatus> {
    auth_metadata::all_providers()
        .iter()
        .filter_map(|meta| status(meta.provider_id).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_masked_preview_normal() {
        assert_eq!(masked_preview("sk-proj-abc123xyz789"), "sk-...z789");
    }

    #[test]
    fn test_masked_preview_short() {
        assert_eq!(masked_preview("abc"), "****");
    }

    #[test]
    fn test_masked_preview_exact_boundary() {
        // 10 chars is the threshold
        assert_eq!(masked_preview("1234567890"), "123...7890");
    }

    #[test]
    fn test_resolve_local_provider() {
        let res = resolve("ollama", None).unwrap();
        assert_eq!(res.source, CredentialSource::None);
        assert!(!res.has_credential);
    }

    #[test]
    fn test_resolve_explicit_wins() {
        let res = resolve("openai", Some("sk-test-key")).unwrap();
        assert_eq!(res.source, CredentialSource::Explicit);
        assert!(res.has_credential);
    }

    #[test]
    fn test_resolve_unknown_provider() {
        assert!(resolve("nonexistent", None).is_err());
    }

    #[test]
    fn test_status_local_provider() {
        let s = status("ollama").unwrap();
        assert_eq!(s.status, AuthStatusKind::NotRequired);
        assert!(s.masked_preview.is_none());
    }

    #[test]
    fn test_status_unknown_provider() {
        assert!(status("nonexistent").is_err());
    }

    #[test]
    fn test_set_credential_local_provider_rejected() {
        let result = set_credential("ollama", "some-key");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not require"));
    }

    #[test]
    fn test_set_credential_unknown_rejected() {
        let result = set_credential("nonexistent", "key");
        assert!(result.is_err());
    }

    #[test]
    fn test_file_fallback_roundtrip() {
        let _lock = ENV_MUTEX.lock().unwrap();
        // Use a unique temp dir to avoid interfering with real credentials
        let tmp = tempfile::tempdir().unwrap();
        let dir_str = tmp.path().to_str().unwrap().to_string();

        // Override credential dir
        unsafe { std::env::set_var("NXUSKIT_CREDENTIALS_DIR", &dir_str) };

        let service = "nxuskit-test-roundtrip";
        file_set(service, "test-api-key-12345").unwrap();
        let val = file_get(service);
        assert_eq!(val, Some("test-api-key-12345".to_string()));

        file_delete(service).unwrap();
        assert!(file_get(service).is_none());

        unsafe { std::env::remove_var("NXUSKIT_CREDENTIALS_DIR") };
    }

    /// Mutex to serialize tests that mutate environment variables.
    /// Env vars are process-global; concurrent mutation causes flaky failures.
    static ENV_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn test_resolve_env_wins() {
        let _lock = ENV_MUTEX.lock().unwrap();
        // Set env var for groq
        unsafe { std::env::set_var("GROQ_API_KEY", "gsk_test12345678") };
        let res = resolve("groq", None).unwrap();
        assert_eq!(res.source, CredentialSource::Env);
        assert!(res.has_credential);
        unsafe { std::env::remove_var("GROQ_API_KEY") };
    }

    #[test]
    fn test_status_env_has_masked_preview() {
        let _lock = ENV_MUTEX.lock().unwrap();
        unsafe { std::env::set_var("GROQ_API_KEY", "gsk_test12345678") };
        let s = status("groq").unwrap();
        assert_eq!(s.status, AuthStatusKind::AuthenticatedEnv);
        assert_eq!(s.masked_preview, Some("gsk...5678".to_string()));
        assert_eq!(s.source, Some("env".to_string()));
        unsafe { std::env::remove_var("GROQ_API_KEY") };
    }
}
