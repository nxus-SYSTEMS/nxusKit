// nxuskit-core: C ABI layer over nxuskit_engine
//
// This crate produces a cdylib (shared library) and staticlib with a stable
// JSON-in/JSON-out C ABI. All internal engine details (CLIPS, etc.) are
// hidden behind opaque handles.

#![allow(clippy::missing_safety_doc)]

pub mod auth;
mod auth_metadata;
pub mod auth_token;
mod bn_sdk;
pub mod catalog;
mod clips_session;
pub mod device_auth;
pub mod entitlement;
mod error;
pub mod eula;
pub mod license;
pub mod license_types;
pub mod machine_id;
pub mod oauth;
#[allow(dead_code)]
mod oauth_prework;
pub mod plugin;
mod provider;
mod runtime;
mod solver_sdk;
mod solver_session;
#[cfg(test)]
pub(crate) mod test_fixtures;
#[allow(dead_code)]
mod tool_types;
mod types;
mod zen_sdk;

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::panic::{AssertUnwindSafe, catch_unwind};

use types::{NxuskitProvider, NxuskitResponse, NxuskitStream};

// ── Version ────────────────────────────────────────────────────────

const VERSION: &CStr =
    match CStr::from_bytes_with_nul(concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes()) {
        Ok(s) => s,
        Err(_) => panic!("version string contains interior NUL"),
    };

#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_version() -> *const c_char {
    VERSION.as_ptr()
}

// ── ABI Introspection ────────────────────────────────────────────

/// ABI compatibility version (major.minor format).
/// Static string — caller MUST NOT free.
const ABI_VERSION: &CStr = c"1.0.0";

/// Edition constant from build script (defaults to "oss").
const EDITION: &CStr =
    match CStr::from_bytes_with_nul(concat!(env!("NXUSKIT_EDITION"), "\0").as_bytes()) {
        Ok(s) => s,
        Err(_) => panic!("edition string contains interior NUL"),
    };

/// Returns the ABI compatibility version as a static string.
///
/// The returned pointer is to static memory and MUST NOT be freed.
/// Safe to call from any thread at any time. No initialization required.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_abi_version() -> *const c_char {
    ABI_VERSION.as_ptr()
}

/// Returns the edition the binary was compiled for.
///
/// One of: `"oss"`, `"pro"`, `"enterprise"`.
/// The returned pointer is to static memory and MUST NOT be freed.
/// Safe to call from any thread at any time. No initialization required.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_edition() -> *const c_char {
    EDITION.as_ptr()
}

/// Capabilities manifest describing compiled-in domains.
#[derive(serde::Serialize)]
struct Capabilities {
    abi_version: &'static str,
    sdk_version: &'static str,
    edition: &'static str,
    domains: CapabilityDomains,
}

#[derive(serde::Serialize)]
struct CapabilityDomains {
    llm: bool,
    clips: bool,
    solver: bool,
    bayesian: bool,
    zen: bool,
    local_llama: bool,
    local_mistralrs: bool,
}

/// Returns a JSON string describing all compiled-in capabilities.
///
/// Caller MUST free the returned pointer with `nxuskit_free_string()`.
/// Returns NULL on allocation failure (sets last error).
/// Safe to call from any thread. Each call allocates a fresh string.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_capabilities() -> *mut c_char {
    error::clear_last_error();

    let caps = Capabilities {
        abi_version: "1.0.0",
        sdk_version: env!("CARGO_PKG_VERSION"),
        edition: env!("NXUSKIT_EDITION"),
        domains: CapabilityDomains {
            llm: true,
            clips: true,
            solver: false,
            bayesian: true,
            zen: false,
            local_llama: cfg!(feature = "provider-local-llama"),
            local_mistralrs: cfg!(feature = "provider-local-mistralrs"),
        },
    };

    match serde_json::to_string(&caps) {
        Ok(json) => match CString::new(json) {
            Ok(cstr) => cstr.into_raw(),
            Err(_) => {
                error::set_last_error("internal_error", "JSON contains interior NUL byte", None);
                std::ptr::null_mut()
            }
        },
        Err(e) => {
            error::set_last_error(
                "internal_error",
                &format!("Failed to serialize capabilities: {e}"),
                None,
            );
            std::ptr::null_mut()
        }
    }
}

/// Returns a JSON string with build fingerprint and signing key info.
///
/// Includes: abi_version, sdk_version, edition, build_target, build_profile.
/// Caller MUST free the returned pointer with `nxuskit_free_string()`.
/// Returns NULL on allocation failure (sets last error).
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_build_info() -> *mut c_char {
    error::clear_last_error();

    let info = serde_json::json!({
        "abi_version": ABI_VERSION.to_str().unwrap_or("0.9.0"),
        "sdk_version": env!("CARGO_PKG_VERSION"),
        "edition": env!("NXUSKIT_EDITION"),
        "build_target": env!("TARGET"),
        "build_profile": if cfg!(debug_assertions) { "debug" } else { "release" },
    });

    match CString::new(info.to_string()) {
        Ok(cstr) => cstr.into_raw(),
        Err(_) => {
            error::set_last_error("internal_error", "JSON contains interior NUL byte", None);
            std::ptr::null_mut()
        }
    }
}

/// Returns entitlement information as a JSON string.
///
/// If `license_key` is non-NULL, it is used to determine the effective edition
/// (compile-time edition upgraded by a valid license key).
///
/// Caller MUST free the returned pointer with `nxuskit_free_string()`.
/// Returns NULL on allocation failure.
///
/// # Safety
///
/// `license_key` must be a valid NUL-terminated C string, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_entitlement_info(license_key: *const c_char) -> *mut c_char {
    error::clear_last_error();

    let lk = if license_key.is_null() {
        None
    } else {
        match unsafe { CStr::from_ptr(license_key) }.to_str() {
            Ok(s) => Some(s),
            Err(_) => {
                error::set_last_error("invalid_argument", "license_key is not valid UTF-8", None);
                return std::ptr::null_mut();
            }
        }
    };

    let info = entitlement::entitlement_info(lk);

    match CString::new(info.to_string()) {
        Ok(cstr) => cstr.into_raw(),
        Err(_) => {
            error::set_last_error("internal_error", "JSON contains interior NUL byte", None);
            std::ptr::null_mut()
        }
    }
}

// ── License ────────────────────────────────────────────────────────

/// Resolve the active license token from all available sources.
///
/// Resolution order:
///   1. `NXUSKIT_LICENSE_TOKEN` environment variable
///   2. `~/.nxuskit/license.token` file
///   3. `explicit_key` parameter (if non-NULL)
///
/// Returns JSON with resolution result. Caller MUST free with `nxuskit_free_string()`.
/// Returns NULL on allocation failure.
///
/// # Safety
///
/// `explicit_key` must be a valid NUL-terminated C string, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_license_resolve(explicit_key: *const c_char) -> *mut c_char {
    error::clear_last_error();

    let ek = if explicit_key.is_null() {
        None
    } else {
        match unsafe { CStr::from_ptr(explicit_key) }.to_str() {
            Ok(s) => Some(s),
            Err(_) => {
                error::set_last_error("invalid_argument", "explicit_key is not valid UTF-8", None);
                return std::ptr::null_mut();
            }
        }
    };

    let resolution = license::resolve_token(ek);

    let json = serde_json::json!({
        "source": resolution.source.to_string(),
        "token_type": resolution.claims.as_ref()
            .map(|c| c.token_type.to_string())
            .unwrap_or_else(|| "none".to_string()),
        "valid": resolution.valid,
        "error": resolution.error,
    });

    match CString::new(json.to_string()) {
        Ok(cstr) => cstr.into_raw(),
        Err(_) => {
            error::set_last_error("internal_error", "JSON contains interior NUL byte", None);
            std::ptr::null_mut()
        }
    }
}

/// Validate a license token JWT string.
///
/// Performs RS384 signature verification, claim parsing, and type-specific
/// validation (expiry, machine binding, version ceiling).
///
/// Returns JSON with validation result. Caller MUST free with `nxuskit_free_string()`.
/// Returns NULL on allocation failure.
///
/// # Safety
///
/// `token_jwt` must be a valid NUL-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_license_validate(token_jwt: *const c_char) -> *mut c_char {
    error::clear_last_error();

    if token_jwt.is_null() {
        error::set_last_error("invalid_argument", "token_jwt must not be NULL", None);
        return std::ptr::null_mut();
    }

    let jwt_str = match unsafe { CStr::from_ptr(token_jwt) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            error::set_last_error("invalid_argument", "token_jwt is not valid UTF-8", None);
            return std::ptr::null_mut();
        }
    };

    let json = match license::validate_token_full(jwt_str) {
        Ok(validated) => {
            serde_json::json!({
                "valid": true,
                "token_type": validated.claims.token_type.to_string(),
                "edition": validated.claims.edition,
                "days_remaining": validated.claims.days_remaining(),
                "error": null,
                "result": "granted",
            })
        }
        Err(e) => {
            let result = match &e {
                license::LicenseError::Expired { .. } => "license_expired",
                license::LicenseError::TrialSuspended => "trial_suspended",
                license::LicenseError::MachineMismatch { .. } => "machine_mismatch",
                license::LicenseError::VersionCeilingExceeded { .. } => "version_ceiling_exceeded",
                license::LicenseError::InvalidSignature => "invalid_signature",
                license::LicenseError::InvalidAlgorithm(_) => "invalid_algorithm",
                license::LicenseError::InvalidIssuer => "invalid_issuer",
                _ => "license_invalid",
            };
            serde_json::json!({
                "valid": false,
                "token_type": "unknown",
                "edition": null,
                "days_remaining": null,
                "error": e.to_string(),
                "result": result,
            })
        }
    };

    match CString::new(json.to_string()) {
        Ok(cstr) => cstr.into_raw(),
        Err(_) => {
            error::set_last_error("internal_error", "JSON contains interior NUL byte", None);
            std::ptr::null_mut()
        }
    }
}

/// Get the machine fingerprint for this device.
///
/// Returns `"sha256:<64-hex-chars>"` on success.
/// Returns NULL if machine ID cannot be determined (e.g., Docker container).
/// Caller MUST free with `nxuskit_free_string()`.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_license_machine_id() -> *mut c_char {
    error::clear_last_error();

    match machine_id::get_machine_fingerprint() {
        Ok(fingerprint) => match CString::new(fingerprint) {
            Ok(cstr) => cstr.into_raw(),
            Err(_) => {
                error::set_last_error(
                    "internal_error",
                    "Fingerprint contains interior NUL byte",
                    None,
                );
                std::ptr::null_mut()
            }
        },
        Err(e) => {
            error::set_last_error("machine_id_unavailable", &e.to_string(), None);
            std::ptr::null_mut()
        }
    }
}

/// Activate a Pro license on this machine.
///
/// Calls the licensing microservice to validate the purchase ID, generate
/// a machine-bound developer token, and store it at `~/.nxuskit/license.token`.
///
/// # Safety
///
/// `purchase_id` must be a valid NUL-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_license_activate(purchase_id: *const c_char) -> *mut c_char {
    error::clear_last_error();

    if purchase_id.is_null() {
        error::set_last_error("invalid_argument", "purchase_id must not be NULL", None);
        return std::ptr::null_mut();
    }

    let pid = match unsafe { CStr::from_ptr(purchase_id) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            error::set_last_error("invalid_argument", "purchase_id is not valid UTF-8", None);
            return std::ptr::null_mut();
        }
    };

    let json = match license::activate(pid) {
        Ok(result) => {
            serde_json::to_value(&result).unwrap_or_else(|_| serde_json::json!({"success": false}))
        }
        Err(e) => {
            serde_json::json!({
                "success": false,
                "seats_used": 0,
                "seats_total": 0,
                "message": e.to_string(),
                "error": e.to_string(),
            })
        }
    };

    match CString::new(json.to_string()) {
        Ok(cstr) => cstr.into_raw(),
        Err(_) => {
            error::set_last_error("internal_error", "JSON contains interior NUL byte", None);
            std::ptr::null_mut()
        }
    }
}

/// Deactivate the Pro license on this machine.
///
/// Calls the licensing microservice to release this machine's seat and
/// removes `~/.nxuskit/license.token`.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_license_deactivate() -> *mut c_char {
    error::clear_last_error();

    let json = match license::deactivate() {
        Ok(result) => {
            serde_json::to_value(&result).unwrap_or_else(|_| serde_json::json!({"success": false}))
        }
        Err(e) => {
            serde_json::json!({
                "success": false,
                "seats_used": 0,
                "seats_total": 0,
                "message": e.to_string(),
                "error": e.to_string(),
            })
        }
    };

    match CString::new(json.to_string()) {
        Ok(cstr) => cstr.into_raw(),
        Err(_) => {
            error::set_last_error("internal_error", "JSON contains interior NUL byte", None);
            std::ptr::null_mut()
        }
    }
}

/// Issue a 30-day trial token for this machine.
///
/// Calls the licensing microservice trial endpoint.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_license_trial_issue() -> *mut c_char {
    error::clear_last_error();

    let json = match license::trial_issue() {
        Ok(result) => {
            serde_json::to_value(&result).unwrap_or_else(|_| serde_json::json!({"success": false}))
        }
        Err(e) => {
            serde_json::json!({
                "success": false,
                "days_remaining": 0,
                "message": e.to_string(),
                "error": e.to_string(),
            })
        }
    };

    match CString::new(json.to_string()) {
        Ok(cstr) => cstr.into_raw(),
        Err(_) => {
            error::set_last_error("internal_error", "JSON contains interior NUL byte", None);
            std::ptr::null_mut()
        }
    }
}

/// Activate a trial token (complete email verification).
///
/// # Safety
///
/// `activation_code` must be a valid NUL-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_license_trial_activate(
    activation_code: *const c_char,
) -> *mut c_char {
    error::clear_last_error();

    if activation_code.is_null() {
        error::set_last_error("invalid_argument", "activation_code must not be NULL", None);
        return std::ptr::null_mut();
    }

    let code = match unsafe { CStr::from_ptr(activation_code) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            error::set_last_error(
                "invalid_argument",
                "activation_code is not valid UTF-8",
                None,
            );
            return std::ptr::null_mut();
        }
    };

    let json = match license::trial_activate(code) {
        Ok(result) => {
            serde_json::to_value(&result).unwrap_or_else(|_| serde_json::json!({"success": false}))
        }
        Err(e) => {
            serde_json::json!({
                "success": false,
                "days_remaining": 0,
                "message": e.to_string(),
                "error": e.to_string(),
            })
        }
    };

    match CString::new(json.to_string()) {
        Ok(cstr) => cstr.into_raw(),
        Err(_) => {
            error::set_last_error("internal_error", "JSON contains interior NUL byte", None);
            std::ptr::null_mut()
        }
    }
}

// ── Auth Helper ────────────────────────────────────────────────────

/// Store a credential for a provider in the OS credential store.
///
/// Returns 0 on success, non-zero on failure (sets last error).
/// Falls back to file-based storage if OS store is unavailable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_auth_set_credential(
    provider_id: *const c_char,
    api_key: *const c_char,
) -> i32 {
    error::clear_last_error();

    let pid = match c_str_to_str(provider_id, "provider_id") {
        Some(s) => s,
        None => return 1,
    };
    let key = match c_str_to_str(api_key, "api_key") {
        Some(s) => s,
        None => return 1,
    };

    match auth::set_credential(pid, key) {
        Ok(()) => 0,
        Err(e) => {
            error::set_last_error("auth_error", &e, None);
            1
        }
    }
}

/// Remove a stored credential for a provider.
///
/// Returns 0 on success, non-zero if no credential existed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_auth_remove_credential(provider_id: *const c_char) -> i32 {
    error::clear_last_error();

    let pid = match c_str_to_str(provider_id, "provider_id") {
        Some(s) => s,
        None => return 1,
    };

    match auth::remove_credential(pid) {
        Ok(()) => 0,
        Err(e) => {
            error::set_last_error("auth_error", &e, None);
            1
        }
    }
}

/// Resolve a credential for a provider using deterministic precedence.
///
/// Returns JSON: `{"provider_id":"...","source":"...","has_credential":bool}`
/// Caller MUST free the returned pointer with `nxuskit_free_string()`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_auth_resolve(
    provider_id: *const c_char,
    explicit_key: *const c_char,
) -> *mut c_char {
    error::clear_last_error();

    let pid = match c_str_to_str(provider_id, "provider_id") {
        Some(s) => s,
        None => return std::ptr::null_mut(),
    };
    let ek = if explicit_key.is_null() {
        None
    } else {
        match c_str_to_str(explicit_key, "explicit_key") {
            Some(s) => Some(s),
            None => return std::ptr::null_mut(),
        }
    };

    match auth::resolve(pid, ek) {
        Ok(res) => json_to_c_string(&res),
        Err(e) => {
            error::set_last_error("auth_error", &e, None);
            std::ptr::null_mut()
        }
    }
}

/// Get auth status for a single provider.
///
/// Returns JSON with status, masked preview, source, dashboard URL.
/// Caller MUST free the returned pointer with `nxuskit_free_string()`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_auth_status(provider_id: *const c_char) -> *mut c_char {
    error::clear_last_error();

    let pid = match c_str_to_str(provider_id, "provider_id") {
        Some(s) => s,
        None => return std::ptr::null_mut(),
    };

    match auth::status(pid) {
        Ok(s) => json_to_c_string(&s),
        Err(e) => {
            error::set_last_error("auth_error", &e, None);
            std::ptr::null_mut()
        }
    }
}

/// Get auth status for all known providers.
///
/// Returns JSON array. Caller MUST free with `nxuskit_free_string()`.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_auth_status_all() -> *mut c_char {
    error::clear_last_error();
    let statuses = auth::status_all();
    json_to_c_string(&statuses)
}

/// Get metadata for all known providers.
///
/// Returns JSON array. Caller MUST free with `nxuskit_free_string()`.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_auth_providers() -> *mut c_char {
    error::clear_last_error();
    let providers = auth_metadata::all_providers();
    json_to_c_string(&providers)
}

/// Helper: convert a C string pointer to a Rust &str, setting error on failure.
fn c_str_to_str<'a>(ptr: *const c_char, param_name: &str) -> Option<&'a str> {
    if ptr.is_null() {
        error::set_last_error("invalid_argument", &format!("{param_name} is NULL"), None);
        return None;
    }
    match unsafe { CStr::from_ptr(ptr) }.to_str() {
        Ok(s) => Some(s),
        Err(_) => {
            error::set_last_error(
                "invalid_argument",
                &format!("{param_name} is not valid UTF-8"),
                None,
            );
            None
        }
    }
}

/// Helper: serialize a value to a JSON C string.
fn json_to_c_string<T: serde::Serialize>(value: &T) -> *mut c_char {
    match serde_json::to_string(value) {
        Ok(json) => match CString::new(json) {
            Ok(cstr) => cstr.into_raw(),
            Err(_) => {
                error::set_last_error("internal_error", "JSON contains interior NUL byte", None);
                std::ptr::null_mut()
            }
        },
        Err(e) => {
            error::set_last_error(
                "internal_error",
                &format!("Failed to serialize JSON: {e}"),
                None,
            );
            std::ptr::null_mut()
        }
    }
}

// ── Provider lifecycle ─────────────────────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_create_provider(
    config_json: *const c_char,
) -> *mut NxuskitProvider {
    catch_unwind(AssertUnwindSafe(|| {
        error::clear_last_error();

        if config_json.is_null() {
            error::set_last_error("invalid_config", "config_json is NULL", None);
            return std::ptr::null_mut();
        }

        let c_str = unsafe { CStr::from_ptr(config_json) };
        let json_str = match c_str.to_str() {
            Ok(s) => s,
            Err(e) => {
                error::set_last_error(
                    "invalid_config",
                    &format!("config_json is not valid UTF-8: {e}"),
                    None,
                );
                return std::ptr::null_mut();
            }
        };

        match provider::create_provider_from_json(json_str) {
            Some(p) => Box::into_raw(Box::new(p)),
            None => std::ptr::null_mut(),
        }
    }))
    .unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in nxuskit_create_provider", None);
        std::ptr::null_mut()
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_free_provider(provider: *mut NxuskitProvider) {
    if !provider.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            drop(unsafe { Box::from_raw(provider) });
        }));
    }
}

// ── Synchronous chat ───────────────────────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_chat(
    provider: *mut NxuskitProvider,
    request_json: *const c_char,
) -> *mut NxuskitResponse {
    catch_unwind(AssertUnwindSafe(|| {
        error::clear_last_error();

        if provider.is_null() {
            error::set_last_error("invalid_config", "provider is NULL", None);
            return std::ptr::null_mut();
        }
        if request_json.is_null() {
            error::set_last_error("invalid_config", "request_json is NULL", None);
            return std::ptr::null_mut();
        }

        let provider_ref = unsafe { &*provider };
        let c_str = unsafe { CStr::from_ptr(request_json) };
        let json_str = match c_str.to_str() {
            Ok(s) => s,
            Err(e) => {
                error::set_last_error(
                    "invalid_config",
                    &format!("request_json is not valid UTF-8: {e}"),
                    None,
                );
                return std::ptr::null_mut();
            }
        };

        let request: nxuskit_engine::ChatRequest = match serde_json::from_str(json_str) {
            Ok(r) => r,
            Err(e) => {
                error::set_last_error(
                    "invalid_config",
                    &format!("Failed to parse ChatRequest JSON: {e}"),
                    None,
                );
                return std::ptr::null_mut();
            }
        };

        let rt = runtime::get_runtime();
        let chat_result = rt.block_on(provider_ref.inner().chat(&request));

        match chat_result {
            Ok(response) => {
                let json = match serde_json::to_string(&response) {
                    Ok(j) => j,
                    Err(e) => {
                        error::set_last_error(
                            "internal_error",
                            &format!("Failed to serialize response: {e}"),
                            None,
                        );
                        return std::ptr::null_mut();
                    }
                };
                match NxuskitResponse::from_json(json) {
                    Some(resp) => Box::into_raw(Box::new(resp)),
                    None => {
                        error::set_last_error(
                            "internal_error",
                            "Response JSON contains interior NUL byte",
                            None,
                        );
                        std::ptr::null_mut()
                    }
                }
            }
            Err(e) => {
                let error_json = format!(
                    r#"{{"content":"","model":"","provider":"","error":{{"error_type":"{}","message":"{}"}}}}"#,
                    error_type_for(&e),
                    error::escape_json_pub(&format!("{e}")),
                );
                match NxuskitResponse::from_json(error_json) {
                    Some(resp) => Box::into_raw(Box::new(resp)),
                    None => {
                        error::set_from_nxuskit_error(&e);
                        std::ptr::null_mut()
                    }
                }
            }
        }
    }))
    .unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in nxuskit_chat", None);
        std::ptr::null_mut()
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_response_json(response: *const NxuskitResponse) -> *const c_char {
    if response.is_null() {
        return std::ptr::null();
    }
    let resp = unsafe { &*response };
    resp.as_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_free_response(response: *mut NxuskitResponse) {
    if !response.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            drop(unsafe { Box::from_raw(response) });
        }));
    }
}

// ── Streaming chat ─────────────────────────────────────────────────

pub type NxuskitStreamCallback =
    unsafe extern "C" fn(chunk_json: *const c_char, user_data: *mut std::ffi::c_void) -> i32;

pub type NxuskitStreamDoneCallback =
    unsafe extern "C" fn(final_json: *const c_char, user_data: *mut std::ffi::c_void);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_chat_stream(
    provider: *mut NxuskitProvider,
    request_json: *const c_char,
    on_chunk: NxuskitStreamCallback,
    on_done: NxuskitStreamDoneCallback,
    user_data: *mut std::ffi::c_void,
) -> *mut NxuskitStream {
    catch_unwind(AssertUnwindSafe(|| {
        error::clear_last_error();

        if provider.is_null() {
            error::set_last_error("invalid_config", "provider is NULL", None);
            return std::ptr::null_mut();
        }
        if request_json.is_null() {
            error::set_last_error("invalid_config", "request_json is NULL", None);
            return std::ptr::null_mut();
        }

        let provider_ref = unsafe { &*provider };
        let c_str = unsafe { CStr::from_ptr(request_json) };
        let json_str = match c_str.to_str() {
            Ok(s) => s,
            Err(e) => {
                error::set_last_error(
                    "invalid_config",
                    &format!("request_json is not valid UTF-8: {e}"),
                    None,
                );
                return std::ptr::null_mut();
            }
        };

        let request: nxuskit_engine::ChatRequest = match serde_json::from_str(json_str) {
            Ok(r) => r,
            Err(e) => {
                error::set_last_error(
                    "invalid_config",
                    &format!("Failed to parse ChatRequest JSON: {e}"),
                    None,
                );
                return std::ptr::null_mut();
            }
        };

        let ud = UserDataSend(user_data);
        let chunk_cb = ChunkCallbackSend(on_chunk);
        let done_cb = DoneCallbackSend(on_done);
        let cancel_token = tokio_util::sync::CancellationToken::new();
        let cancel_child = cancel_token.clone();
        let rt = runtime::get_runtime();

        let stream_result = rt.block_on(provider_ref.inner().chat_stream(&request));
        let mut stream = match stream_result {
            Ok(s) => s,
            Err(e) => {
                error::set_from_nxuskit_error(&e);
                return std::ptr::null_mut();
            }
        };

        // SAFETY: user_data, on_chunk, on_done are guaranteed by the caller to be
        // valid until on_done fires. The Send wrappers assert this contract.
        let handle = rt.spawn(unsafe {
            send_future(async move {
                use futures::StreamExt;

                let mut aggregated_content = String::new();
                let mut chunk_index: u64 = 0;

                loop {
                    tokio::select! {
                        _ = cancel_child.cancelled() => {
                            break;
                        }
                        item = stream.next() => {
                            match item {
                                Some(Ok(chunk)) => {
                                    aggregated_content.push_str(&chunk.delta);
                                    let mut chunk_json = serde_json::json!({
                                        "delta": chunk.delta,
                                        "index": chunk_index,
                                    });
                                    if let Some(thinking) = &chunk.thinking {
                                        chunk_json["thinking"] = serde_json::Value::String(thinking.clone());
                                    }
                                    if let Some(fr) = &chunk.finish_reason {
                                        chunk_json["finish_reason"] = serde_json::to_value(fr).unwrap_or_default();
                                    }
                                    if let Some(usage) = &chunk.usage {
                                        chunk_json["usage"] = serde_json::to_value(usage).unwrap_or_default();
                                    }
                                    if let Some(tc) = &chunk.tool_calls {
                                        chunk_json["tool_calls"] = serde_json::Value::Array(tc.clone());
                                    }
                                    chunk_index += 1;
                                    if let Ok(json_cstr) = CString::new(chunk_json.to_string()) {
                                        let should_cancel =
                                            (chunk_cb.0)(json_cstr.as_ptr(), ud.0);
                                        if should_cancel != 0 {
                                            break;
                                        }
                                    }
                                }
                                Some(Err(e)) => {
                                    let error_json = serde_json::json!({
                                        "content": aggregated_content,
                                        "model": "",
                                        "provider": "",
                                        "error": {
                                            "error_type": "provider_error",
                                            "message": format!("{e}"),
                                        },
                                    });
                                    if let Ok(json_cstr) = CString::new(error_json.to_string()) {
                                        (done_cb.0)(json_cstr.as_ptr(), ud.0);
                                    }
                                    return;
                                }
                                None => break,
                            }
                        }
                    }
                }

                let final_json = serde_json::json!({
                    "content": aggregated_content,
                    "model": "",
                    "provider": "",
                });
                if let Ok(json_cstr) = CString::new(final_json.to_string()) {
                    (done_cb.0)(json_cstr.as_ptr(), ud.0);
                }
            })
        });

        Box::into_raw(Box::new(NxuskitStream::new(cancel_token, handle)))
    }))
    .unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in nxuskit_chat_stream", None);
        std::ptr::null_mut()
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_cancel_stream(stream: *mut NxuskitStream) {
    if stream.is_null() {
        return;
    }
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let stream_ref = unsafe { &mut *stream };
        stream_ref.cancel_token().cancel();
        if let Some(handle) = stream_ref.take_handle() {
            let rt = runtime::get_runtime();
            let _ = rt.block_on(handle);
        }
    }));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_free_stream(stream: *mut NxuskitStream) {
    if !stream.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            drop(unsafe { Box::from_raw(stream) });
        }));
    }
}

// ── Model discovery ────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_list_models(provider: *mut NxuskitProvider) -> *mut c_char {
    catch_unwind(AssertUnwindSafe(|| {
        error::clear_last_error();

        if provider.is_null() {
            error::set_last_error("invalid_config", "provider is NULL", None);
            return std::ptr::null_mut();
        }

        let provider_ref = unsafe { &*provider };
        let rt = runtime::get_runtime();
        let models_result = rt.block_on(provider_ref.inner().list_models());

        match models_result {
            Ok(models) => {
                let json = match serde_json::to_string(&models) {
                    Ok(j) => j,
                    Err(e) => {
                        error::set_last_error(
                            "internal_error",
                            &format!("Failed to serialize models: {e}"),
                            None,
                        );
                        return std::ptr::null_mut();
                    }
                };
                match CString::new(json) {
                    Ok(cstr) => cstr.into_raw(),
                    Err(_) => {
                        error::set_last_error(
                            "internal_error",
                            "Models JSON contains interior NUL byte",
                            None,
                        );
                        std::ptr::null_mut()
                    }
                }
            }
            Err(e) => {
                error::set_from_nxuskit_error(&e);
                std::ptr::null_mut()
            }
        }
    }))
    .unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in nxuskit_list_models", None);
        std::ptr::null_mut()
    })
}

// ── Convenience completions ───────────────────────────────────────

/// One-shot completion convenience function.
///
/// Auto-detects provider from model name and environment variables.
/// Returns response text as a JSON string `{"content":"...","model":"..."}`.
/// Caller MUST free the returned pointer with `nxuskit_free_string()`.
///
/// `config_json` is a JSON object:
///   `{"model": "gpt-4o", "prompt": "Hello", "temperature": 0.7, "max_tokens": 1024}`
///
/// Only `model` and `prompt` are required.
///
/// Returns NULL on error (check `nxuskit_last_error()`).
///
/// # Safety
/// `config_json` must be a valid, NUL-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_completion(config_json: *const c_char) -> *mut c_char {
    catch_unwind(AssertUnwindSafe(|| {
        error::clear_last_error();

        if config_json.is_null() {
            error::set_last_error("invalid_config", "config_json is NULL", None);
            return std::ptr::null_mut();
        }

        let c_str = unsafe { CStr::from_ptr(config_json) };
        let json_str = match c_str.to_str() {
            Ok(s) => s,
            Err(e) => {
                error::set_last_error(
                    "invalid_config",
                    &format!("config_json is not valid UTF-8: {e}"),
                    None,
                );
                return std::ptr::null_mut();
            }
        };

        let config: serde_json::Value = match serde_json::from_str(json_str) {
            Ok(v) => v,
            Err(e) => {
                error::set_last_error(
                    "invalid_config",
                    &format!("Failed to parse config JSON: {e}"),
                    None,
                );
                return std::ptr::null_mut();
            }
        };

        let model = match config.get("model").and_then(|v| v.as_str()) {
            Some(m) if !m.is_empty() => m,
            _ => {
                error::set_last_error(
                    "invalid_config",
                    "config_json must contain a non-empty \"model\" field",
                    None,
                );
                return std::ptr::null_mut();
            }
        };

        let prompt = match config.get("prompt").and_then(|v| v.as_str()) {
            Some(p) if !p.is_empty() => p,
            _ => {
                error::set_last_error(
                    "invalid_config",
                    "config_json must contain a non-empty \"prompt\" field",
                    None,
                );
                return std::ptr::null_mut();
            }
        };

        let rt = runtime::get_runtime();
        let result = rt.block_on(nxuskit_engine::convenience::completion(model, prompt));

        match result {
            Ok(content) => {
                let response_json = serde_json::json!({
                    "content": content,
                    "model": model,
                });
                match CString::new(response_json.to_string()) {
                    Ok(cstr) => cstr.into_raw(),
                    Err(_) => {
                        error::set_last_error(
                            "internal_error",
                            "Response JSON contains interior NUL byte",
                            None,
                        );
                        std::ptr::null_mut()
                    }
                }
            }
            Err(e) => {
                error::set_last_error("provider_error", &format!("{e}"), None);
                std::ptr::null_mut()
            }
        }
    }))
    .unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in nxuskit_completion", None);
        std::ptr::null_mut()
    })
}

/// Streaming completion convenience function.
///
/// Auto-detects provider from model name and environment variables.
/// Calls `on_chunk` for each streaming chunk, `on_done` when complete.
///
/// `config_json` is a JSON object:
///   `{"model": "gpt-4o", "prompt": "Hello", "temperature": 0.7, "max_tokens": 1024}`
///
/// Only `model` and `prompt` are required.
///
/// Returns a stream handle (free with `nxuskit_free_stream()`), or NULL on error.
///
/// # Safety
/// `config_json` must be a valid, NUL-terminated C string.
/// Callback function pointers must remain valid until `on_done` fires.
/// `user_data` is forwarded opaquely to callbacks.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_completion_stream(
    config_json: *const c_char,
    on_chunk: NxuskitStreamCallback,
    on_done: NxuskitStreamDoneCallback,
    user_data: *mut std::ffi::c_void,
) -> *mut NxuskitStream {
    catch_unwind(AssertUnwindSafe(|| {
        error::clear_last_error();

        if config_json.is_null() {
            error::set_last_error("invalid_config", "config_json is NULL", None);
            return std::ptr::null_mut();
        }

        let c_str = unsafe { CStr::from_ptr(config_json) };
        let json_str = match c_str.to_str() {
            Ok(s) => s,
            Err(e) => {
                error::set_last_error(
                    "invalid_config",
                    &format!("config_json is not valid UTF-8: {e}"),
                    None,
                );
                return std::ptr::null_mut();
            }
        };

        let config: serde_json::Value = match serde_json::from_str(json_str) {
            Ok(v) => v,
            Err(e) => {
                error::set_last_error(
                    "invalid_config",
                    &format!("Failed to parse config JSON: {e}"),
                    None,
                );
                return std::ptr::null_mut();
            }
        };

        let model = match config.get("model").and_then(|v| v.as_str()) {
            Some(m) if !m.is_empty() => m.to_string(),
            _ => {
                error::set_last_error(
                    "invalid_config",
                    "config_json must contain a non-empty \"model\" field",
                    None,
                );
                return std::ptr::null_mut();
            }
        };

        let prompt = match config.get("prompt").and_then(|v| v.as_str()) {
            Some(p) if !p.is_empty() => p.to_string(),
            _ => {
                error::set_last_error(
                    "invalid_config",
                    "config_json must contain a non-empty \"prompt\" field",
                    None,
                );
                return std::ptr::null_mut();
            }
        };

        let ud = UserDataSend(user_data);
        let chunk_cb = ChunkCallbackSend(on_chunk);
        let done_cb = DoneCallbackSend(on_done);
        let cancel_token = tokio_util::sync::CancellationToken::new();
        let cancel_child = cancel_token.clone();
        let rt = runtime::get_runtime();

        let model_owned = model.clone();
        // Create the stream inside block_on and pin it to erase lifetimes.
        // The engine's completion_stream returns `impl Stream` which is opaque;
        // we collect it into a Pin<Box<dyn Stream + Send>> for the spawned task.
        let stream_result = rt.block_on(async {
            use futures::StreamExt;
            let spec = nxuskit_engine::convenience::ModelSpecifier::parse(&model)?;
            let router = nxuskit_engine::convenience::ProviderRouter::new();
            let provider = router.route(&spec).await?;
            let request = nxuskit_engine::ChatRequest::new(&spec.model)
                .with_message(nxuskit_engine::Message::user(&prompt));
            let stream = provider.chat_stream(&request).await?;
            // Map to text-only stream and box-pin to make it 'static + Send
            let mapped: std::pin::Pin<
                Box<dyn futures::Stream<Item = nxuskit_engine::error::Result<String>> + Send>,
            > = Box::pin(stream.map(|r| r.map(|c| c.delta)));
            Ok::<_, nxuskit_engine::NxuskitError>(mapped)
        });
        let mut stream = match stream_result {
            Ok(s) => s,
            Err(e) => {
                error::set_last_error("provider_error", &format!("{e}"), None);
                return std::ptr::null_mut();
            }
        };

        let handle = rt.spawn(unsafe {
            send_future(async move {
                use futures::StreamExt;

                let mut aggregated_content = String::new();
                let mut chunk_index: u64 = 0;

                loop {
                    tokio::select! {
                        _ = cancel_child.cancelled() => {
                            break;
                        }
                        item = stream.next() => {
                            match item {
                                Some(Ok(text)) => {
                                    aggregated_content.push_str(&text);
                                    let chunk_json = serde_json::json!({
                                        "content": text,
                                        "index": chunk_index,
                                    });
                                    chunk_index += 1;
                                    if let Ok(json_cstr) = CString::new(chunk_json.to_string()) {
                                        let should_cancel =
                                            (chunk_cb.0)(json_cstr.as_ptr(), ud.0);
                                        if should_cancel != 0 {
                                            break;
                                        }
                                    }
                                }
                                Some(Err(e)) => {
                                    let error_json = serde_json::json!({
                                        "content": aggregated_content,
                                        "model": model_owned,
                                        "error": {
                                            "error_type": "provider_error",
                                            "message": format!("{e}"),
                                        },
                                    });
                                    if let Ok(json_cstr) = CString::new(error_json.to_string()) {
                                        (done_cb.0)(json_cstr.as_ptr(), ud.0);
                                    }
                                    return;
                                }
                                None => break,
                            }
                        }
                    }
                }

                let final_json = serde_json::json!({
                    "content": aggregated_content,
                    "model": model_owned,
                });
                if let Ok(json_cstr) = CString::new(final_json.to_string()) {
                    (done_cb.0)(json_cstr.as_ptr(), ud.0);
                }
            })
        });

        Box::into_raw(Box::new(NxuskitStream::new(cancel_token, handle)))
    }))
    .unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in nxuskit_completion_stream", None);
        std::ptr::null_mut()
    })
}

// ── Plugin management ─────────────────────────────────────────────

/// Load plugins from a directory.
///
/// Scans the directory for `*.json` manifest files paired with shared libraries,
/// verifies ABI compatibility and signatures, loads valid plugins into the registry.
/// Returns the number of successfully loaded plugins, or -1 on argument error.
///
/// Plugin load failures are non-fatal — rejected plugins are logged via `log::warn!`.
/// The string pointed to by `dir_path` is borrowed only for the duration of the call.
///
/// # Safety
/// `dir_path` must be a valid, NUL-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_plugin_load_dir(dir_path: *const c_char) -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        error::clear_last_error();

        if dir_path.is_null() {
            error::set_last_error("invalid_config", "dir_path is NULL", None);
            return -1;
        }

        let c_str = unsafe { CStr::from_ptr(dir_path) };
        let path_str = match c_str.to_str() {
            Ok(s) => s,
            Err(e) => {
                error::set_last_error(
                    "invalid_config",
                    &format!("dir_path is not valid UTF-8: {e}"),
                    None,
                );
                return -1;
            }
        };

        plugin::PluginRegistry::load_dir(std::path::Path::new(path_str))
    }))
    .unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in nxuskit_plugin_load_dir", None);
        -1
    })
}

/// Return a JSON array describing all loaded plugins.
///
/// Caller MUST free the returned pointer with `nxuskit_free_string()`.
/// Returns `"[]"` if no plugins are loaded.
/// Returns NULL on internal error (sets last error).
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_plugin_list() -> *mut c_char {
    catch_unwind(AssertUnwindSafe(|| {
        error::clear_last_error();

        let plugins = plugin::PluginRegistry::list();
        match serde_json::to_string(&plugins) {
            Ok(json) => match CString::new(json) {
                Ok(cstr) => cstr.into_raw(),
                Err(_) => {
                    error::set_last_error(
                        "internal_error",
                        "Plugin list JSON contains interior NUL byte",
                        None,
                    );
                    std::ptr::null_mut()
                }
            },
            Err(e) => {
                error::set_last_error(
                    "internal_error",
                    &format!("Failed to serialize plugin list: {e}"),
                    None,
                );
                std::ptr::null_mut()
            }
        }
    }))
    .unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in nxuskit_plugin_list", None);
        std::ptr::null_mut()
    })
}

/// Return JSON metadata for a specific plugin by name.
///
/// Caller MUST free the returned pointer with `nxuskit_free_string()`.
/// Returns NULL if the plugin is not found (sets last error).
///
/// # Safety
/// `name` must be a valid, NUL-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_plugin_info(name: *const c_char) -> *mut c_char {
    catch_unwind(AssertUnwindSafe(|| {
        error::clear_last_error();

        if name.is_null() {
            error::set_last_error("invalid_config", "plugin name is NULL", None);
            return std::ptr::null_mut();
        }

        let c_str = unsafe { CStr::from_ptr(name) };
        let name_str = match c_str.to_str() {
            Ok(s) => s,
            Err(e) => {
                error::set_last_error(
                    "invalid_config",
                    &format!("plugin name is not valid UTF-8: {e}"),
                    None,
                );
                return std::ptr::null_mut();
            }
        };

        match plugin::PluginRegistry::info(name_str) {
            Some(info) => match serde_json::to_string(&info) {
                Ok(json) => match CString::new(json) {
                    Ok(cstr) => cstr.into_raw(),
                    Err(_) => {
                        error::set_last_error(
                            "internal_error",
                            "Plugin info JSON contains interior NUL byte",
                            None,
                        );
                        std::ptr::null_mut()
                    }
                },
                Err(e) => {
                    error::set_last_error(
                        "internal_error",
                        &format!("Failed to serialize plugin info: {e}"),
                        None,
                    );
                    std::ptr::null_mut()
                }
            },
            None => {
                error::set_last_error(
                    "plugin_not_found",
                    &format!("Plugin '{name_str}' not found"),
                    None,
                );
                std::ptr::null_mut()
            }
        }
    }))
    .unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in nxuskit_plugin_info", None);
        std::ptr::null_mut()
    })
}

/// Return the number of loaded plugins.
///
/// Always succeeds. Returns 0 if no plugins are loaded.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_plugin_count() -> i32 {
    catch_unwind(AssertUnwindSafe(|| plugin::PluginRegistry::count() as i32)).unwrap_or(0)
}

/// Check if a specific plugin is loaded by name.
///
/// Returns 1 if loaded, 0 if not loaded or on error.
///
/// # Safety
/// `name` must be a valid, NUL-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_plugin_loaded(name: *const c_char) -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        if name.is_null() {
            return 0;
        }
        let c_str = unsafe { CStr::from_ptr(name) };
        match c_str.to_str() {
            Ok(s) => {
                if plugin::PluginRegistry::is_loaded(s) {
                    1
                } else {
                    0
                }
            }
            Err(_) => 0,
        }
    }))
    .unwrap_or(0)
}

/// Unload all plugins, drop library handles, and clear the registry.
///
/// Safe to call even if no plugins are loaded.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_plugin_unload_all() {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        plugin::PluginRegistry::unload_all();
    }));
}

// ── Plugin Trust Mode ─────────────────────────────────────────────

/// Set the plugin trust mode.
///
/// # Arguments
/// * `mode` — 0 = SignedOnly (default), 1 = AllowUnsigned
///
/// Returns 0 on success, -1 on invalid mode.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_plugin_set_trust_mode(mode: i32) -> i32 {
    match mode {
        0 => {
            plugin::set_trust_mode(plugin::TrustMode::SignedOnly);
            0
        }
        1 => {
            plugin::set_trust_mode(plugin::TrustMode::AllowUnsigned);
            0
        }
        _ => {
            error::set_last_error(
                "invalid_argument",
                "trust_mode must be 0 (signed-only) or 1 (allow-unsigned)",
                None,
            );
            -1
        }
    }
}

/// Get the current plugin trust mode.
///
/// Returns 0 = SignedOnly, 1 = AllowUnsigned.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_plugin_get_trust_mode() -> i32 {
    plugin::get_trust_mode() as i32
}

/// Load plugins from a directory with explicit trust mode awareness.
///
/// Equivalent to `nxuskit_plugin_load_dir` but documents that the current
/// trust mode setting is respected during loading.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_plugin_load_dir_trusted(dir_path: *const c_char) -> i32 {
    // Delegates to the standard load_dir which now respects trust mode
    unsafe { nxuskit_plugin_load_dir(dir_path) }
}

// ── OAuth ──────────────────────────────────────────────────────────

/// Start an OAuth authentication flow for a provider.
///
/// This is a BLOCKING call — it launches a browser, starts a localhost
/// callback server, and waits for the authorization code.
///
/// provider_id: Provider to authenticate (e.g., "azure-openai").
/// timeout_secs: Max seconds to wait for callback (0 = default 120s).
///
/// Returns JSON: {"success": true|false, "provider_id": "...",
///                "message": "...", "error": null|"message"}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_oauth_start(
    provider_id: *const c_char,
    timeout_secs: u32,
) -> *mut c_char {
    let provider = match unsafe { CStr::from_ptr(provider_id) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            error::set_last_error("invalid_argument", "invalid provider_id", None);
            return std::ptr::null_mut();
        }
    };

    let result = catch_unwind(AssertUnwindSafe(|| {
        oauth::oauth_start(provider, timeout_secs)
    }));

    match result {
        Ok(Ok(oauth_result)) => {
            let json = serde_json::to_string(&oauth_result).unwrap_or_default();
            CString::new(json)
                .map(CString::into_raw)
                .unwrap_or(std::ptr::null_mut())
        }
        Ok(Err(e)) => {
            let err_result = oauth::OAuthResult {
                success: false,
                provider_id: provider.to_string(),
                message: e.to_string(),
                error: Some(e.to_string()),
            };
            let json = serde_json::to_string(&err_result).unwrap_or_default();
            CString::new(json)
                .map(CString::into_raw)
                .unwrap_or(std::ptr::null_mut())
        }
        Err(_) => {
            error::set_last_error("panic", "oauth_start panicked", None);
            std::ptr::null_mut()
        }
    }
}

/// Check OAuth token status for a provider.
///
/// Returns JSON: {"authenticated": true|false, "provider_id": "...",
///                "expires_at": null, "scopes": null}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_oauth_status(provider_id: *const c_char) -> *mut c_char {
    let provider = match unsafe { CStr::from_ptr(provider_id) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            error::set_last_error("invalid_argument", "invalid provider_id", None);
            return std::ptr::null_mut();
        }
    };

    let result = catch_unwind(AssertUnwindSafe(|| oauth::oauth_status(provider)));

    match result {
        Ok(Ok(status)) => {
            let json = serde_json::to_string(&status).unwrap_or_default();
            CString::new(json)
                .map(CString::into_raw)
                .unwrap_or(std::ptr::null_mut())
        }
        Ok(Err(e)) => {
            error::set_last_error("oauth_error", &e.to_string(), None);
            std::ptr::null_mut()
        }
        Err(_) => {
            error::set_last_error("panic", "oauth_status panicked", None);
            std::ptr::null_mut()
        }
    }
}

/// Remove the stored OAuth token for a provider.
///
/// Returns 0 on success (or if no token was stored), -1 on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_oauth_revoke(provider_id: *const c_char) -> i32 {
    let provider = match unsafe { CStr::from_ptr(provider_id) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            error::set_last_error("invalid_argument", "invalid provider_id", None);
            return -1;
        }
    };

    match oauth::oauth_revoke(provider) {
        Ok(()) => 0,
        Err(e) => {
            error::set_last_error("oauth_error", &e.to_string(), None);
            -1
        }
    }
}

// ── Error handling ─────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_last_error() -> *const c_char {
    error::last_error_ptr()
}

// ── Memory management ──────────────────────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            drop(unsafe { CString::from_raw(ptr) });
        }));
    }
}

// ── Internal helpers ───────────────────────────────────────────────

/// Wrapper to make `*mut c_void` Send for async tasks.
/// Safety: The caller guarantees the pointer is valid for the task's lifetime.
struct UserDataSend(*mut std::ffi::c_void);
unsafe impl Send for UserDataSend {}

/// Wrapper for callback function pointers to assert Send.
/// Function pointers are Send in Rust but the compiler can't prove it through type aliases.
struct ChunkCallbackSend(NxuskitStreamCallback);
unsafe impl Send for ChunkCallbackSend {}

struct DoneCallbackSend(NxuskitStreamDoneCallback);
unsafe impl Send for DoneCallbackSend {}

/// Wrap a future in a Send-asserting wrapper.
/// SAFETY: The caller must guarantee the future's captures are safe to send
/// across threads (e.g., raw pointers are valid for the task's lifetime).
unsafe fn send_future<F: std::future::Future>(
    f: F,
) -> impl std::future::Future<Output = F::Output> + Send {
    struct AssertSend<F>(F);
    unsafe impl<F> Send for AssertSend<F> {}
    impl<F: std::future::Future> std::future::Future for AssertSend<F> {
        type Output = F::Output;
        fn poll(
            self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Self::Output> {
            // SAFETY: structural pinning — we only access the inner field through a pin
            let inner = unsafe { self.map_unchecked_mut(|s| &mut s.0) };
            inner.poll(cx)
        }
    }
    AssertSend(f)
}

fn error_type_for(err: &nxuskit_engine::NxuskitError) -> &'static str {
    match err {
        nxuskit_engine::NxuskitError::Configuration(_)
        | nxuskit_engine::NxuskitError::InvalidRequest(_) => "invalid_config",
        nxuskit_engine::NxuskitError::Authentication(_) => "authentication_failed",
        nxuskit_engine::NxuskitError::RateLimit { .. } => "rate_limited",
        nxuskit_engine::NxuskitError::Provider { .. } => "provider_error",
        nxuskit_engine::NxuskitError::Stream(_) => "provider_error",
        _ => "internal_error",
    }
}
