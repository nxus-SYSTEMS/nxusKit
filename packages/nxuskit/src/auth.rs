//! Auth helper — safe Rust wrapper over the nxusKit auth C ABI.
//!
//! Provides credential lifecycle operations (set/remove/resolve/status)
//! with deterministic precedence: explicit > env > store > none.

use crate::error::NxuskitError;
use crate::ffi;
use serde::Deserialize;
use std::ffi::{CStr, CString};

/// Credential resolution result.
#[derive(Debug, Clone, Deserialize)]
pub struct AuthResolution {
    pub provider_id: String,
    pub source: String,
    pub has_credential: bool,
}

/// Auth status for a provider.
#[derive(Debug, Clone, Deserialize)]
pub struct AuthStatus {
    pub provider_id: String,
    pub status: String,
    pub masked_preview: Option<String>,
    pub source: Option<String>,
    pub dashboard_url: Option<String>,
    #[serde(default)]
    pub oauth_capable: bool,
    #[serde(default)]
    pub auth_methods: Vec<String>,
}

/// OAuth flow result.
#[derive(Debug, Clone, Deserialize)]
pub struct OAuthResult {
    pub success: bool,
    pub provider_id: String,
    pub message: String,
    pub error: Option<String>,
}

/// OAuth token status.
#[derive(Debug, Clone, Deserialize)]
pub struct OAuthStatus {
    pub authenticated: bool,
    pub provider_id: String,
    pub expires_at: Option<i64>,
    pub scopes: Option<Vec<String>>,
}

/// Provider auth metadata.
#[derive(Debug, Clone, Deserialize)]
pub struct ProviderAuthMetadata {
    pub provider_id: String,
    pub display_name: String,
    pub env_var_name: String,
    pub auth_required: bool,
    pub dashboard_url: Option<String>,
    pub oauth_capable: bool,
    pub auth_methods: Vec<String>,
    pub credential_service_name: String,
}

/// Store a credential for a provider in the OS credential store.
///
/// Falls back to file-based storage if OS store is unavailable.
/// The `api_key` is never logged in plain text.
pub fn auth_set_credential(provider_id: &str, api_key: &str) -> Result<(), NxuskitError> {
    let pid = CString::new(provider_id).map_err(|_| NxuskitError::Internal {
        message: "provider_id contains NUL byte".to_string(),
    })?;
    let key = CString::new(api_key).map_err(|_| NxuskitError::Internal {
        message: "api_key contains NUL byte".to_string(),
    })?;

    #[cfg(feature = "static-link")]
    let result = unsafe { ffi::nxuskit_auth_set_credential(pid.as_ptr(), key.as_ptr()) };
    #[cfg(feature = "dynamic-link")]
    let result = {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_auth_set_credential)(pid.as_ptr(), key.as_ptr()) }
    };

    if result == 0 {
        Ok(())
    } else {
        Err(NxuskitError::Internal {
            message: format!("Failed to set credential for '{provider_id}'"),
        })
    }
}

/// Remove a stored credential for a provider.
pub fn auth_remove_credential(provider_id: &str) -> Result<(), NxuskitError> {
    let pid = CString::new(provider_id).map_err(|_| NxuskitError::Internal {
        message: "provider_id contains NUL byte".to_string(),
    })?;

    #[cfg(feature = "static-link")]
    let result = unsafe { ffi::nxuskit_auth_remove_credential(pid.as_ptr()) };
    #[cfg(feature = "dynamic-link")]
    let result = {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_auth_remove_credential)(pid.as_ptr()) }
    };

    if result == 0 {
        Ok(())
    } else {
        Err(NxuskitError::Internal {
            message: format!("No credential found for '{provider_id}'"),
        })
    }
}

/// Resolve a credential using deterministic precedence.
///
/// Precedence: explicit > env var > OS store > none.
pub fn auth_resolve(
    provider_id: &str,
    explicit_key: Option<&str>,
) -> Result<AuthResolution, NxuskitError> {
    let pid = CString::new(provider_id).map_err(|_| NxuskitError::Internal {
        message: "provider_id contains NUL byte".to_string(),
    })?;
    let ek_cstr = explicit_key.map(|k| CString::new(k).unwrap());
    let ek_ptr = ek_cstr
        .as_ref()
        .map(|c| c.as_ptr())
        .unwrap_or(std::ptr::null());

    #[cfg(feature = "static-link")]
    let ptr = unsafe { ffi::nxuskit_auth_resolve(pid.as_ptr(), ek_ptr) };
    #[cfg(feature = "dynamic-link")]
    let ptr = {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_auth_resolve)(pid.as_ptr(), ek_ptr) }
    };

    parse_json_ptr(ptr, "auth_resolve")
}

/// Get auth status for a single provider.
pub fn auth_status(provider_id: &str) -> Result<AuthStatus, NxuskitError> {
    let pid = CString::new(provider_id).map_err(|_| NxuskitError::Internal {
        message: "provider_id contains NUL byte".to_string(),
    })?;

    #[cfg(feature = "static-link")]
    let ptr = unsafe { ffi::nxuskit_auth_status(pid.as_ptr()) };
    #[cfg(feature = "dynamic-link")]
    let ptr = {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_auth_status)(pid.as_ptr()) }
    };

    parse_json_ptr(ptr, "auth_status")
}

/// Get auth status for all known providers.
pub fn auth_status_all() -> Result<Vec<AuthStatus>, NxuskitError> {
    #[cfg(feature = "static-link")]
    let ptr = unsafe { ffi::nxuskit_auth_status_all() };
    #[cfg(feature = "dynamic-link")]
    let ptr = {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_auth_status_all)() }
    };

    parse_json_ptr(ptr, "auth_status_all")
}

/// Get metadata for all known providers.
pub fn auth_providers() -> Result<Vec<ProviderAuthMetadata>, NxuskitError> {
    #[cfg(feature = "static-link")]
    let ptr = unsafe { ffi::nxuskit_auth_providers() };
    #[cfg(feature = "dynamic-link")]
    let ptr = {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_auth_providers)() }
    };

    parse_json_ptr(ptr, "auth_providers")
}

// ── OAuth Functions ──────────────────────────────────────────────

/// Start an OAuth authentication flow for a provider.
///
/// This is a **blocking** call — it launches a browser, starts a localhost
/// callback server, and waits for the authorization code.
pub fn oauth_start(provider_id: &str, timeout_secs: u32) -> Result<OAuthResult, NxuskitError> {
    let pid = CString::new(provider_id).map_err(|_| NxuskitError::Internal {
        message: "provider_id contains NUL byte".to_string(),
    })?;

    let ptr = ffi::ffi_call!(nxuskit_oauth_start, pid.as_ptr(), timeout_secs);
    parse_json_ptr(ptr, "oauth_start")
}

/// Start an OAuth authentication flow asynchronously.
///
/// Runs the blocking OAuth flow on a spawned thread via `tokio::task::spawn_blocking`.
pub async fn oauth_start_async(
    provider_id: String,
    timeout_secs: u32,
) -> Result<OAuthResult, NxuskitError> {
    tokio::task::spawn_blocking(move || oauth_start(&provider_id, timeout_secs))
        .await
        .map_err(|e| NxuskitError::Internal {
            message: format!("OAuth spawn_blocking join failed: {e}"),
        })?
}

/// Check OAuth authentication status for a provider.
pub fn oauth_status(provider_id: &str) -> Result<OAuthStatus, NxuskitError> {
    let pid = CString::new(provider_id).map_err(|_| NxuskitError::Internal {
        message: "provider_id contains NUL byte".to_string(),
    })?;

    let ptr = ffi::ffi_call!(nxuskit_oauth_status, pid.as_ptr());
    parse_json_ptr(ptr, "oauth_status")
}

/// Revoke/remove the stored OAuth token for a provider.
pub fn oauth_revoke(provider_id: &str) -> Result<(), NxuskitError> {
    let pid = CString::new(provider_id).map_err(|_| NxuskitError::Internal {
        message: "provider_id contains NUL byte".to_string(),
    })?;

    let result = ffi::ffi_call!(nxuskit_oauth_revoke, pid.as_ptr());

    if result == 0 {
        Ok(())
    } else {
        Err(NxuskitError::Internal {
            message: format!("Failed to revoke OAuth token for '{provider_id}'"),
        })
    }
}

/// Parse a JSON C string pointer into a Rust type, freeing the pointer.
fn parse_json_ptr<T: serde::de::DeserializeOwned>(
    ptr: *mut std::ffi::c_char,
    fn_name: &str,
) -> Result<T, NxuskitError> {
    if ptr.is_null() {
        return Err(NxuskitError::Internal {
            message: format!("{fn_name} returned NULL"),
        });
    }

    let json_str = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|e| NxuskitError::Internal {
            message: format!("{fn_name} returned invalid UTF-8: {e}"),
        })?
        .to_string();

    #[cfg(feature = "static-link")]
    unsafe {
        ffi::nxuskit_free_string(ptr);
    }
    #[cfg(feature = "dynamic-link")]
    {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_free_string)(ptr) };
    }

    serde_json::from_str(&json_str).map_err(|e| NxuskitError::Internal {
        message: format!("Failed to parse {fn_name} JSON: {e}"),
    })
}
