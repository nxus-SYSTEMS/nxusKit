//! License management wrappers for the nxusKit SDK.
//!
//! Safe Rust API over the C ABI license functions: token resolution,
//! validation, and machine fingerprinting.

use crate::error::NxuskitError;
use crate::ffi;

/// Token resolution result from the license precedence chain.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct LicenseResolution {
    /// Where the token was found: "env_var", "file", "api_param", or "none"
    pub source: String,
    /// Token type: "trial", "developer", "deployment", or "none"
    pub token_type: String,
    /// Whether the token passed validation
    pub valid: bool,
    /// Error message if validation failed
    pub error: Option<String>,
}

/// Token validation result.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct TokenInfo {
    /// Whether the token is valid
    pub valid: bool,
    /// Token type: "trial", "developer", "deployment"
    pub token_type: String,
    /// Edition granted by the token
    pub edition: Option<String>,
    /// Days until token expiry (None for deployment tokens)
    pub days_remaining: Option<i64>,
    /// Error message if validation failed
    pub error: Option<String>,
    /// Entitlement result code
    pub result: String,
}

/// Resolve the active license token from all available sources.
///
/// Resolution order:
/// 1. `NXUSKIT_LICENSE_TOKEN` environment variable
/// 2. `~/.nxuskit/license.token` file
/// 3. `explicit_key` parameter (if provided)
///
/// Returns the resolution result including source, token type, and validity.
pub fn license_resolve(explicit_key: Option<&str>) -> Result<LicenseResolution, NxuskitError> {
    use std::ffi::{CStr, CString};

    let ek_cstr = explicit_key.map(|k| CString::new(k).unwrap());
    let ek_ptr = ek_cstr
        .as_ref()
        .map(|c| c.as_ptr())
        .unwrap_or(std::ptr::null());

    let ptr = ffi::ffi_call!(nxuskit_license_resolve, ek_ptr);

    if ptr.is_null() {
        return Err(NxuskitError::Internal {
            message: "nxuskit_license_resolve returned NULL".to_string(),
        });
    }

    let json_str = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|e| NxuskitError::Internal {
            message: format!("license resolve not valid UTF-8: {e}"),
        })?
        .to_string();

    // Free the C string
    ffi::ffi_call!(nxuskit_free_string, ptr);

    serde_json::from_str(&json_str).map_err(|e| NxuskitError::Internal {
        message: format!("Failed to parse license resolve JSON: {e}"),
    })
}

/// Validate a license token JWT string.
///
/// Performs RS384 signature verification, claim parsing, and type-specific
/// validation (expiry, machine binding, version ceiling).
pub fn license_validate(token: &str) -> Result<TokenInfo, NxuskitError> {
    use std::ffi::{CStr, CString};

    let token_cstr = CString::new(token).map_err(|_| NxuskitError::Internal {
        message: "Token contains interior NUL byte".to_string(),
    })?;

    let ptr = ffi::ffi_call!(nxuskit_license_validate, token_cstr.as_ptr());

    if ptr.is_null() {
        return Err(NxuskitError::Internal {
            message: "nxuskit_license_validate returned NULL".to_string(),
        });
    }

    let json_str = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|e| NxuskitError::Internal {
            message: format!("license validate not valid UTF-8: {e}"),
        })?
        .to_string();

    ffi::ffi_call!(nxuskit_free_string, ptr);

    serde_json::from_str(&json_str).map_err(|e| NxuskitError::Internal {
        message: format!("Failed to parse license validate JSON: {e}"),
    })
}

/// Get the machine fingerprint for this device.
///
/// Returns a `sha256:<64-hex-chars>` string derived from the OS machine ID.
///
/// Returns an error if the machine ID cannot be determined (e.g., in Docker
/// containers or minimal environments).
pub fn license_machine_id() -> Result<String, NxuskitError> {
    use std::ffi::CStr;

    let ptr = ffi::ffi_call!(nxuskit_license_machine_id);

    if ptr.is_null() {
        return Err(NxuskitError::Internal {
            message: "Machine ID unavailable (Docker container or minimal environment?)"
                .to_string(),
        });
    }

    let fingerprint = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|e| NxuskitError::Internal {
            message: format!("machine ID not valid UTF-8: {e}"),
        })?
        .to_string();

    ffi::ffi_call!(nxuskit_free_string, ptr);

    Ok(fingerprint)
}

/// Activation result from the licensing microservice.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ActivationResult {
    pub success: bool,
    pub seats_used: u32,
    pub seats_total: u32,
    pub message: String,
    pub error: Option<String>,
}

/// Activate a Pro license on this machine.
///
/// Calls the licensing microservice to validate the purchase ID, generate
/// a machine-bound developer token, and store it locally.
pub fn license_activate(purchase_id: &str) -> Result<ActivationResult, NxuskitError> {
    use std::ffi::{CStr, CString};

    let pid_cstr = CString::new(purchase_id).map_err(|_| NxuskitError::Internal {
        message: "Purchase ID contains interior NUL byte".to_string(),
    })?;

    let ptr = ffi::ffi_call!(nxuskit_license_activate, pid_cstr.as_ptr());

    if ptr.is_null() {
        return Err(NxuskitError::Internal {
            message: "nxuskit_license_activate returned NULL".to_string(),
        });
    }

    let json_str = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|e| NxuskitError::Internal {
            message: format!("activate result not valid UTF-8: {e}"),
        })?
        .to_string();

    ffi::ffi_call!(nxuskit_free_string, ptr);

    serde_json::from_str(&json_str).map_err(|e| NxuskitError::Internal {
        message: format!("Failed to parse activation JSON: {e}"),
    })
}

/// Deactivate the Pro license on this machine.
///
/// Releases this machine's seat and removes the stored token.
pub fn license_deactivate() -> Result<ActivationResult, NxuskitError> {
    use std::ffi::CStr;

    let ptr = ffi::ffi_call!(nxuskit_license_deactivate);

    if ptr.is_null() {
        return Err(NxuskitError::Internal {
            message: "nxuskit_license_deactivate returned NULL".to_string(),
        });
    }

    let json_str = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|e| NxuskitError::Internal {
            message: format!("deactivate result not valid UTF-8: {e}"),
        })?
        .to_string();

    ffi::ffi_call!(nxuskit_free_string, ptr);

    serde_json::from_str(&json_str).map_err(|e| NxuskitError::Internal {
        message: format!("Failed to parse deactivation JSON: {e}"),
    })
}

/// Trial issuance result.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct TrialResult {
    pub success: bool,
    pub days_remaining: u32,
    pub message: String,
    pub error: Option<String>,
}

/// Issue a 30-day trial token for this machine.
pub fn license_trial_issue() -> Result<TrialResult, NxuskitError> {
    use std::ffi::CStr;

    let ptr = ffi::ffi_call!(nxuskit_license_trial_issue);

    if ptr.is_null() {
        return Err(NxuskitError::Internal {
            message: "nxuskit_license_trial_issue returned NULL".to_string(),
        });
    }

    let json_str = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|e| NxuskitError::Internal {
            message: format!("trial issue result not valid UTF-8: {e}"),
        })?
        .to_string();

    ffi::ffi_call!(nxuskit_free_string, ptr);

    serde_json::from_str(&json_str).map_err(|e| NxuskitError::Internal {
        message: format!("Failed to parse trial issue JSON: {e}"),
    })
}

/// Activate a trial token (complete email verification).
pub fn license_trial_activate(code: &str) -> Result<TrialResult, NxuskitError> {
    use std::ffi::{CStr, CString};

    let code_cstr = CString::new(code).map_err(|_| NxuskitError::Internal {
        message: "Activation code contains interior NUL byte".to_string(),
    })?;

    let ptr = ffi::ffi_call!(nxuskit_license_trial_activate, code_cstr.as_ptr());

    if ptr.is_null() {
        return Err(NxuskitError::Internal {
            message: "nxuskit_license_trial_activate returned NULL".to_string(),
        });
    }

    let json_str = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|e| NxuskitError::Internal {
            message: format!("trial activate result not valid UTF-8: {e}"),
        })?
        .to_string();

    ffi::ffi_call!(nxuskit_free_string, ptr);

    serde_json::from_str(&json_str).map_err(|e| NxuskitError::Internal {
        message: format!("Failed to parse trial activate JSON: {e}"),
    })
}
