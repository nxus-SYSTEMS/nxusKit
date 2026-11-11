use std::cell::RefCell;
use std::ffi::CString;
use std::os::raw::c_char;

pub(crate) struct AsyncCallbackError {
    status: i32,
    json: CString,
}

impl AsyncCallbackError {
    pub(crate) fn from_provider(error: &nxuskit_engine::NxuskitError) -> Self {
        let (status, code) = callback_status_and_code(error);
        let retry_after_ms = error
            .retry_after()
            .and_then(|duration| u64::try_from(duration.as_millis()).ok());
        Self::new(status, code, &error.to_string(), retry_after_ms)
    }

    pub(crate) fn internal(message: &str) -> Self {
        Self::new(6, "internal_error", message, None)
    }

    pub(crate) fn cancelled() -> Self {
        Self::new(5, "cancelled", "SDK-owned chat operation cancelled", None)
    }

    pub(crate) fn deadline() -> Self {
        Self::new(5, "timeout", "SDK chat operation deadline exceeded", None)
    }

    fn new(status: i32, code: &str, message: &str, retry_after_ms: Option<u64>) -> Self {
        let json = match retry_after_ms {
            Some(retry_after_ms) => serde_json::json!({
                "code": code,
                "message": message,
                "retry_after_ms": retry_after_ms,
            }),
            None => serde_json::json!({"code": code, "message": message}),
        }
        .to_string();
        let json = CString::new(json).unwrap_or_else(|_| {
            c"{\"code\":\"internal_error\",\"message\":\"error encoding failed\"}".to_owned()
        });
        Self { status, json }
    }

    pub(crate) fn status(&self) -> i32 {
        self.status
    }

    pub(crate) fn as_ptr(&self) -> *const c_char {
        self.json.as_ptr()
    }
}

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
    if let nxuskit_engine::NxuskitError::UnsupportedCapability {
        provider,
        capability,
    } = err
    {
        set_last_error("unsupported_capability", capability, Some(provider));
        return;
    }
    let (error_type, message) = match err {
        nxuskit_engine::NxuskitError::Network(error) if error.is_timeout() => {
            ("timeout", "Provider request timed out")
        }
        nxuskit_engine::NxuskitError::Configuration(msg) => ("invalid_config", msg.as_str()),
        nxuskit_engine::NxuskitError::Authentication(msg) => {
            ("authentication_failed", msg.as_str())
        }
        nxuskit_engine::NxuskitError::InvalidRequest(msg) => ("invalid_request", msg.as_str()),
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

fn callback_status_and_code(error: &nxuskit_engine::NxuskitError) -> (i32, &'static str) {
    use nxuskit_engine::NxuskitError;
    use nxuskit_engine::error::McpErrorKind;

    match error {
        NxuskitError::Configuration(_)
        | NxuskitError::InvalidRequest(_)
        | NxuskitError::InvalidImageFormat(_)
        | NxuskitError::ImageFileNotFound(_)
        | NxuskitError::ImageTooLarge { .. }
        | NxuskitError::InvalidDetailLevel(_)
        | NxuskitError::ProviderDetectionFailed(_, _)
        | NxuskitError::ModelNotRecognized(_, _)
        | NxuskitError::InvalidModelSpecifier(_) => (2, "invalid_argument"),
        NxuskitError::Provider { status, .. } if (400..500).contains(status) => match status {
            401 | 403 => (4, "authentication"),
            429 => (4, "rate_limited"),
            _ => (2, "invalid_argument"),
        },
        NxuskitError::Authentication(_)
        | NxuskitError::MissingCredentials(_, _)
        | NxuskitError::Mcp(McpErrorKind::AuthenticationFailed(_)) => (4, "authentication"),
        NxuskitError::RateLimit { .. } => (4, "rate_limited"),
        NxuskitError::Network(_)
        | NxuskitError::Provider { .. }
        | NxuskitError::Stream(_)
        | NxuskitError::Mcp(McpErrorKind::ConnectionFailed(_) | McpErrorKind::Timeout(_)) => {
            (4, "unavailable")
        }
        NxuskitError::UnsupportedCapability { .. } => (4, "unsupported_capability"),
        NxuskitError::LicenseRequired { .. } => (4, "license_required"),
        NxuskitError::FeatureUnavailable { .. } | NxuskitError::NotImplemented { .. } => {
            (4, "feature_unavailable")
        }
        NxuskitError::LicenseExpired { .. } => (4, "license_expired"),
        NxuskitError::EditionInsufficient { .. } => (4, "edition_insufficient"),
        NxuskitError::Serialization(_)
        | NxuskitError::ImageEncodingFailed(_)
        | NxuskitError::Io(_)
        | NxuskitError::Mcp(McpErrorKind::ProtocolError(_) | McpErrorKind::InvalidResponse(_))
        | NxuskitError::Clips(_) => (6, "internal_error"),
    }
}

#[cfg(test)]
mod async_callback_tests {
    use super::AsyncCallbackError;
    use nxuskit_engine::NxuskitError;
    use nxuskit_engine::error::McpErrorKind;
    use serde_json::Value;
    use std::{ffi::CStr, time::Duration};

    fn callback_status_and_code(error: NxuskitError) -> (i32, String) {
        let callback_error = AsyncCallbackError::from_provider(&error);
        let json = unsafe { CStr::from_ptr(callback_error.as_ptr()) };
        let value: Value = serde_json::from_slice(json.to_bytes()).expect("strict error JSON");
        (
            callback_error.status(),
            value["code"]
                .as_str()
                .expect("string error code")
                .to_owned(),
        )
    }

    #[test]
    fn rate_limit_callback_preserves_retry_after_ms() {
        let error = NxuskitError::rate_limit(Some(Duration::from_secs(2)));
        let callback_error = AsyncCallbackError::from_provider(&error);
        let json = unsafe { CStr::from_ptr(callback_error.as_ptr()) };
        let value: Value = serde_json::from_slice(json.to_bytes()).expect("strict error JSON");
        assert_eq!(value["code"], "rate_limited");
        assert_eq!(value["retry_after_ms"], 2000);
    }

    #[test]
    fn callback_status_and_code_are_derived_from_one_consistent_taxonomy() {
        assert_eq!(
            callback_status_and_code(NxuskitError::InvalidImageFormat("bmp".to_owned())),
            (2, "invalid_argument".to_owned())
        );
        assert_eq!(
            callback_status_and_code(NxuskitError::MissingCredentials(
                "openai".to_owned(),
                "OPENAI_API_KEY".to_owned(),
            )),
            (4, "authentication".to_owned())
        );
        assert_eq!(
            callback_status_and_code(NxuskitError::Provider {
                status: 401,
                message: "unauthorized".to_owned(),
            }),
            (4, "authentication".to_owned())
        );
        assert_eq!(
            callback_status_and_code(NxuskitError::Provider {
                status: 403,
                message: "forbidden".to_owned(),
            }),
            (4, "authentication".to_owned())
        );
        assert_eq!(
            callback_status_and_code(NxuskitError::Provider {
                status: 429,
                message: "limited".to_owned(),
            }),
            (4, "rate_limited".to_owned())
        );
        assert_eq!(
            callback_status_and_code(NxuskitError::Provider {
                status: 503,
                message: "offline".to_owned(),
            }),
            (4, "unavailable".to_owned())
        );
        assert_eq!(
            callback_status_and_code(NxuskitError::Mcp(McpErrorKind::AuthenticationFailed(
                "denied".to_owned(),
            ))),
            (4, "authentication".to_owned())
        );
        for error in [
            NxuskitError::Mcp(McpErrorKind::ConnectionFailed("offline".to_owned())),
            NxuskitError::Mcp(McpErrorKind::Timeout("late".to_owned())),
        ] {
            assert_eq!(
                callback_status_and_code(error),
                (4, "unavailable".to_owned())
            );
        }
        for error in [
            NxuskitError::Mcp(McpErrorKind::ProtocolError("invalid frame".to_owned())),
            NxuskitError::Mcp(McpErrorKind::InvalidResponse("invalid JSON".to_owned())),
        ] {
            assert_eq!(
                callback_status_and_code(error),
                (6, "internal_error".to_owned())
            );
        }
        assert_eq!(
            callback_status_and_code(NxuskitError::LicenseRequired {
                feature: "premium-model".to_owned(),
            }),
            (4, "license_required".to_owned())
        );
        assert_eq!(
            callback_status_and_code(NxuskitError::FeatureUnavailable {
                feature: "premium-model".to_owned(),
            }),
            (4, "feature_unavailable".to_owned())
        );
        assert_eq!(
            callback_status_and_code(NxuskitError::UnsupportedCapability {
                provider: "perplexity".to_owned(),
                capability: "stop".to_owned(),
            }),
            (4, "unsupported_capability".to_owned())
        );
        assert_eq!(
            callback_status_and_code(NxuskitError::LicenseExpired {
                feature: "premium-model".to_owned(),
            }),
            (4, "license_expired".to_owned())
        );
        assert_eq!(
            callback_status_and_code(NxuskitError::EditionInsufficient {
                feature: "premium-model".to_owned(),
                required_edition: "pro".to_owned(),
            }),
            (4, "edition_insufficient".to_owned())
        );
    }

    #[test]
    fn invalid_request_preserves_distinct_ffi_error_type() {
        use super::{clear_last_error, last_error_ptr, set_from_nxuskit_error};

        clear_last_error();
        set_from_nxuskit_error(&NxuskitError::InvalidRequest("bad request".to_owned()));

        let ptr = last_error_ptr();
        assert!(!ptr.is_null());
        let json = unsafe { CStr::from_ptr(ptr) };
        let value: Value = serde_json::from_slice(json.to_bytes()).expect("valid error JSON");
        assert_eq!(value["error_type"], "invalid_request");
        assert_eq!(value["message"], "bad request");
    }
}
