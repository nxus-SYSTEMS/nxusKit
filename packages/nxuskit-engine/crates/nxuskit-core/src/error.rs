use std::cell::RefCell;
use std::ffi::CString;
use std::os::raw::c_char;

// Thread-local storage for the last error message (JSON-encoded ErrorInfo).
thread_local! {
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

/// Store an error for the current thread. The error is serialized as JSON.
pub(crate) fn set_last_error(error_type: &str, message: &str, provider: Option<&str>) {
    let json = if let Some(p) = provider {
        format!(
            r#"{{"error_type":"{}","message":"{}","provider":"{}"}}"#,
            escape_json(error_type),
            escape_json(message),
            escape_json(p),
        )
    } else {
        format!(
            r#"{{"error_type":"{}","message":"{}"}}"#,
            escape_json(error_type),
            escape_json(message),
        )
    };

    if let Ok(cstr) = CString::new(json) {
        LAST_ERROR.with(|cell| {
            *cell.borrow_mut() = Some(cstr);
        });
    }
}

/// Clear the last error for the current thread.
pub(crate) fn clear_last_error() {
    LAST_ERROR.with(|cell| {
        *cell.borrow_mut() = None;
    });
}

/// Get the last error as a C string pointer. Returns null if no error.
/// The pointer is valid until the next `nxuskit_*` call on this thread.
pub(crate) fn last_error_ptr() -> *const c_char {
    LAST_ERROR.with(|cell| {
        cell.borrow()
            .as_ref()
            .map(|s| s.as_ptr())
            .unwrap_or(std::ptr::null())
    })
}

/// Map a nxuskit_engine error to our thread-local error storage.
pub(crate) fn set_from_nxuskit_error(err: &nxuskit_engine::NxuskitError) {
    let (error_type, message) = match err {
        nxuskit_engine::NxuskitError::Configuration(msg) => ("invalid_config", msg.as_str()),
        nxuskit_engine::NxuskitError::Authentication(msg) => {
            ("authentication_failed", msg.as_str())
        }
        nxuskit_engine::NxuskitError::InvalidRequest(msg) => ("invalid_config", msg.as_str()),
        nxuskit_engine::NxuskitError::RateLimit { .. } => ("rate_limited", "Rate limit exceeded"),
        nxuskit_engine::NxuskitError::Provider { message, .. } => {
            ("provider_error", message.as_str())
        }
        nxuskit_engine::NxuskitError::Stream(msg) => ("provider_error", msg.as_str()),
        // Entitlement errors — enforcement deferred to v0.9.0 but types are
        // fully wired through the C ABI JSON error taxonomy.
        nxuskit_engine::NxuskitError::FeatureUnavailable { feature } => {
            ("feature_unavailable", feature.as_str())
        }
        nxuskit_engine::NxuskitError::LicenseRequired { feature } => {
            ("license_required", feature.as_str())
        }
        nxuskit_engine::NxuskitError::LicenseExpired { feature } => {
            ("license_expired", feature.as_str())
        }
        nxuskit_engine::NxuskitError::EditionInsufficient { feature, .. } => {
            ("edition_insufficient", feature.as_str())
        }
        _ => ("internal_error", "An internal error occurred"),
    };
    set_last_error(error_type, message, None);
}

/// Minimal JSON string escaping for error messages (crate-internal).
pub(crate) fn escape_json_pub(s: &str) -> String {
    escape_json(s)
}

/// Set a tool-calling error for the current thread.
#[allow(dead_code)]
pub(crate) fn set_tool_error(error_type: &str, message: &str) {
    set_last_error(error_type, message, None);
}

fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}
