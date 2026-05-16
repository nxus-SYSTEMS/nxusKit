//! Plugin management wrapper.
//!
//! Wraps the `nxuskit_plugin_*` C ABI functions for idiomatic Rust plugin management.
//! Plugins are shared libraries (`.dylib`/`.so`/`.dll`) with JSON manifests and
//! ed25519 signature verification.

use std::ffi::{CStr, CString, c_char};

use serde::{Deserialize, Serialize};

use crate::NxuskitError;
use crate::ffi;

// ── FFI dispatch helpers ─────────────────────────────────────────

#[cfg(feature = "static-link")]
unsafe fn call_plugin_load_dir(dir: *const c_char) -> i32 {
    unsafe { ffi::nxuskit_plugin_load_dir(dir) }
}

#[cfg(feature = "dynamic-link")]
unsafe fn call_plugin_load_dir(dir: *const c_char) -> i32 {
    let sdk = ffi::dynamic::sdk_unchecked();
    unsafe { (sdk.nxuskit_plugin_load_dir)(dir) }
}

#[cfg(feature = "static-link")]
fn call_plugin_list() -> *mut c_char {
    unsafe { ffi::nxuskit_plugin_list() }
}

#[cfg(feature = "dynamic-link")]
fn call_plugin_list() -> *mut c_char {
    let sdk = ffi::dynamic::sdk_unchecked();
    unsafe { (sdk.nxuskit_plugin_list)() }
}

#[cfg(feature = "static-link")]
unsafe fn call_plugin_info(name: *const c_char) -> *mut c_char {
    unsafe { ffi::nxuskit_plugin_info(name) }
}

#[cfg(feature = "dynamic-link")]
unsafe fn call_plugin_info(name: *const c_char) -> *mut c_char {
    let sdk = ffi::dynamic::sdk_unchecked();
    unsafe { (sdk.nxuskit_plugin_info)(name) }
}

#[cfg(feature = "static-link")]
fn call_plugin_count() -> i32 {
    unsafe { ffi::nxuskit_plugin_count() }
}

#[cfg(feature = "dynamic-link")]
fn call_plugin_count() -> i32 {
    let sdk = ffi::dynamic::sdk_unchecked();
    unsafe { (sdk.nxuskit_plugin_count)() }
}

#[cfg(feature = "static-link")]
unsafe fn call_plugin_loaded(name: *const c_char) -> i32 {
    unsafe { ffi::nxuskit_plugin_loaded(name) }
}

#[cfg(feature = "dynamic-link")]
unsafe fn call_plugin_loaded(name: *const c_char) -> i32 {
    let sdk = ffi::dynamic::sdk_unchecked();
    unsafe { (sdk.nxuskit_plugin_loaded)(name) }
}

#[cfg(feature = "static-link")]
fn call_plugin_unload_all() {
    unsafe { ffi::nxuskit_plugin_unload_all() }
}

#[cfg(feature = "dynamic-link")]
fn call_plugin_unload_all() {
    let sdk = ffi::dynamic::sdk_unchecked();
    unsafe { (sdk.nxuskit_plugin_unload_all)() }
}

#[cfg(feature = "static-link")]
unsafe fn call_free_string(ptr: *mut c_char) {
    unsafe { ffi::nxuskit_free_string(ptr) }
}

#[cfg(feature = "dynamic-link")]
unsafe fn call_free_string(ptr: *mut c_char) {
    let sdk = ffi::dynamic::sdk_unchecked();
    unsafe { (sdk.nxuskit_free_string)(ptr) }
}

#[cfg(feature = "static-link")]
unsafe fn last_error_ptr() -> *const c_char {
    unsafe { ffi::nxuskit_last_error() }
}

#[cfg(feature = "dynamic-link")]
unsafe fn last_error_ptr() -> *const c_char {
    let sdk = ffi::dynamic::sdk_unchecked();
    unsafe { (sdk.nxuskit_last_error)() }
}

fn last_error_or(fallback: &str) -> NxuskitError {
    let ptr = unsafe { last_error_ptr() };
    if ptr.is_null() {
        return NxuskitError::Internal {
            message: fallback.to_string(),
        };
    }
    let err_str = match unsafe { CStr::from_ptr(ptr) }.to_str() {
        Ok(s) if !s.is_empty() => s,
        _ => {
            return NxuskitError::Internal {
                message: fallback.to_string(),
            };
        }
    };
    NxuskitError::from_json_str(err_str)
}

// ── Plugin Info ─────────────────────────────────────────────────

/// Metadata for a loaded plugin, deserialized from the C ABI JSON output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    /// Unique plugin identifier (kebab-case).
    pub name: String,

    /// Plugin version (semver).
    pub version: String,

    /// ABI version the plugin was built for.
    pub abi_version: String,

    /// Capabilities this plugin provides.
    pub capabilities: Vec<String>,

    /// Human-readable description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Plugin author.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,

    /// Minimum required SDK edition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_edition: Option<String>,

    /// Required entitlement domains.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_entitlements: Vec<String>,

    /// Path to the loaded shared library.
    pub library_path: String,

    /// JSON metadata returned by the plugin's init function.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub init_metadata: Option<String>,
}

// ── Public API ───────────────────────────────────────────────────

/// Load plugins from a directory.
///
/// Scans for `*.json` manifest files paired with shared libraries, verifies
/// ABI compatibility and ed25519 signatures, and loads valid plugins into the
/// global registry.
///
/// Plugin load failures are non-fatal — invalid plugins are logged and skipped.
///
/// # Errors
///
/// Returns [`NxuskitError::Configuration`] if `dir` contains an interior NUL byte.
/// Returns [`NxuskitError::Internal`] if the C ABI call fails unexpectedly.
pub fn load_plugins(dir: &str) -> Result<i32, NxuskitError> {
    let c_dir = CString::new(dir).map_err(|_| NxuskitError::Configuration {
        message: "plugin directory path contains interior NUL byte".to_string(),
    })?;

    let count = unsafe { call_plugin_load_dir(c_dir.as_ptr()) };
    if count < 0 {
        return Err(last_error_or("nxuskit_plugin_load_dir returned -1"));
    }
    Ok(count)
}

/// List all loaded plugins.
///
/// Returns metadata for every plugin currently loaded in the global registry.
pub fn list_plugins() -> Result<Vec<PluginInfo>, NxuskitError> {
    let ptr = call_plugin_list();
    if ptr.is_null() {
        return Err(last_error_or("nxuskit_plugin_list returned NULL"));
    }

    let json_str = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|e| {
            unsafe { call_free_string(ptr) };
            NxuskitError::InvalidResponse {
                message: format!("plugin list is not valid UTF-8: {e}"),
            }
        })?
        .to_owned();

    unsafe { call_free_string(ptr) };

    serde_json::from_str(&json_str).map_err(|e| NxuskitError::InvalidResponse {
        message: format!("failed to parse plugin list JSON: {e}"),
    })
}

/// Get metadata for a specific plugin by name.
///
/// Returns `None` if the plugin is not loaded.
pub fn plugin_info(name: &str) -> Result<Option<PluginInfo>, NxuskitError> {
    let c_name = CString::new(name).map_err(|_| NxuskitError::Configuration {
        message: "plugin name contains interior NUL byte".to_string(),
    })?;

    let ptr = unsafe { call_plugin_info(c_name.as_ptr()) };
    if ptr.is_null() {
        return Ok(None);
    }

    let json_str = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|e| {
            unsafe { call_free_string(ptr) };
            NxuskitError::InvalidResponse {
                message: format!("plugin info is not valid UTF-8: {e}"),
            }
        })?
        .to_owned();

    unsafe { call_free_string(ptr) };

    let info: PluginInfo =
        serde_json::from_str(&json_str).map_err(|e| NxuskitError::InvalidResponse {
            message: format!("failed to parse plugin info JSON: {e}"),
        })?;
    Ok(Some(info))
}

/// Return the number of loaded plugins.
pub fn plugin_count() -> i32 {
    call_plugin_count()
}

/// Check if a specific plugin is loaded by name.
pub fn is_plugin_loaded(name: &str) -> bool {
    let c_name = match CString::new(name) {
        Ok(c) => c,
        Err(_) => return false,
    };
    let result = unsafe { call_plugin_loaded(c_name.as_ptr()) };
    result == 1
}

/// Unload all plugins, drop library handles, and clear the registry.
///
/// Safe to call even if no plugins are loaded (idempotent).
pub fn unload_all_plugins() {
    call_plugin_unload_all();
}

// ── Trust Mode ──────────────────────────────────────────────────

/// Plugin trust mode controlling whether unsigned plugins are allowed.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustMode {
    /// Only cryptographically signed plugins are loaded (default).
    SignedOnly = 0,
    /// Unsigned plugins are loaded but emit audit events.
    AllowUnsigned = 1,
}

/// Set the plugin trust mode.
///
/// Controls whether unsigned plugins are allowed to load. When set to
/// `AllowUnsigned`, unsigned plugins will be loaded but structured audit
/// events are emitted for every unsigned load attempt.
pub fn set_plugin_trust_mode(mode: TrustMode) -> Result<(), NxuskitError> {
    let result = ffi::ffi_call!(nxuskit_plugin_set_trust_mode, mode as i32);
    if result < 0 {
        return Err(last_error_or("nxuskit_plugin_set_trust_mode failed"));
    }
    Ok(())
}

/// Get the current plugin trust mode.
pub fn get_plugin_trust_mode() -> TrustMode {
    let mode = ffi::ffi_call!(nxuskit_plugin_get_trust_mode);
    match mode {
        1 => TrustMode::AllowUnsigned,
        _ => TrustMode::SignedOnly,
    }
}
