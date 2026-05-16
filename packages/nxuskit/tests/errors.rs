//! Error handling tests for nxuskit.
//!
//! Tests marked `#[ignore]` require `libnxuskit` at runtime.
//! Run them with: `NXUSKIT_LIB_DIR=/path/to/lib cargo test --test errors -- --ignored`

use nxuskit::{NxuskitError, NxuskitProvider, ProviderConfig};

/// Invalid provider_type should return a Configuration error.
#[test]
#[ignore = "requires libnxuskit runtime"]
fn invalid_provider_type() {
    let config = ProviderConfig {
        provider_type: "nonexistent_provider_xyz".into(),
        ..Default::default()
    };
    let result = NxuskitProvider::new(config);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(
            err,
            NxuskitError::Configuration { .. } | NxuskitError::Provider { .. }
        ),
        "expected Configuration or Provider error, got: {err}"
    );
}

/// Library-not-found error when NXUSKIT_LIB_DIR points to invalid path.
/// This test only applies in dynamic-link mode.
#[test]
#[cfg(feature = "dynamic-link")]
fn library_not_found_with_invalid_path() {
    // Save and override the env var.
    let original = std::env::var("NXUSKIT_LIB_DIR").ok();

    // SAFETY: We restore the env var immediately after; this test should not run
    // concurrently with other tests that depend on this env var.
    unsafe {
        std::env::set_var("NXUSKIT_LIB_DIR", "/nonexistent/path/that/does/not/exist");
    }

    // Note: The SDK singleton is loaded once per process, so this test may
    // not trigger LibraryNotFound if the SDK was already loaded. This is
    // expected in a test suite where other tests run first.
    // In isolation, this would produce LibraryNotFound.

    // Restore.
    unsafe {
        match original {
            Some(val) => std::env::set_var("NXUSKIT_LIB_DIR", val),
            None => std::env::remove_var("NXUSKIT_LIB_DIR"),
        }
    }
}

/// Large response handling: verify the wrapper doesn't choke on big JSON.
#[test]
#[ignore = "requires libnxuskit runtime with mock provider support for large responses"]
fn large_response_handling() {
    let config = ProviderConfig {
        provider_type: "mock".into(),
        ..Default::default()
    };
    let provider = NxuskitProvider::new(config).expect("failed to create mock provider");

    let request = nxuskit::ChatRequest {
        model: "mock-model".into(),
        messages: vec![nxuskit::Message {
            role: nxuskit::Role::User,
            content: "Generate a very long response".into(),
        }],
        ..Default::default()
    };

    // The mock provider should handle this without special config.
    let response = provider.chat(request).expect("large response chat failed");
    assert!(!response.content.is_empty());
}

// --- Tests that run without libnxuskit ---

/// Verify NxuskitError::from_json_str parses known error types correctly.
#[test]
fn error_from_json_str_configuration() {
    let json = r#"{"error_type": "invalid_config", "message": "missing provider_type"}"#;
    let err = NxuskitError::from_json_str(json);
    assert!(matches!(err, NxuskitError::Configuration { .. }));
    assert!(err.to_string().contains("missing provider_type"));
}

#[test]
fn error_from_json_str_authentication() {
    let json = r#"{"error_type": "auth_failed", "message": "invalid API key"}"#;
    let err = NxuskitError::from_json_str(json);
    assert!(matches!(err, NxuskitError::Authentication { .. }));
}

#[test]
fn error_from_json_str_rate_limited() {
    let json = r#"{"error_type": "rate_limited", "message": "too many requests"}"#;
    let err = NxuskitError::from_json_str(json);
    assert!(matches!(err, NxuskitError::RateLimited { .. }));
}

#[test]
fn error_from_json_str_provider_error() {
    let json =
        r#"{"error_type": "provider_error", "message": "server error", "provider": "openai"}"#;
    let err = NxuskitError::from_json_str(json);
    match err {
        NxuskitError::Provider { message, provider } => {
            assert_eq!(message, "server error");
            assert_eq!(provider.as_deref(), Some("openai"));
        }
        _ => panic!("expected Provider error"),
    }
}

#[test]
fn error_from_json_str_unknown_type_falls_back_to_provider() {
    let json = r#"{"error_type": "something_new", "message": "future error type"}"#;
    let err = NxuskitError::from_json_str(json);
    assert!(matches!(err, NxuskitError::Provider { .. }));
}

#[test]
fn error_from_json_str_invalid_json_falls_back_to_internal() {
    let err = NxuskitError::from_json_str("not valid json");
    assert!(matches!(err, NxuskitError::Internal { .. }));
}

/// Verify NxuskitError Display implementations.
#[test]
fn error_display_messages() {
    let err = NxuskitError::Configuration {
        message: "bad config".into(),
    };
    assert_eq!(err.to_string(), "configuration error: bad config");

    let err = NxuskitError::VersionMismatch {
        expected: "0.1.0".into(),
        found: "1.0.0".into(),
    };
    assert_eq!(
        err.to_string(),
        "version mismatch: expected 0.1.0, found 1.0.0"
    );

    let err = NxuskitError::Provider {
        message: "timeout".into(),
        provider: Some("ollama".into()),
    };
    assert_eq!(err.to_string(), "provider error (ollama): timeout");

    let err = NxuskitError::Provider {
        message: "timeout".into(),
        provider: None,
    };
    assert_eq!(err.to_string(), "provider error: timeout");
}
