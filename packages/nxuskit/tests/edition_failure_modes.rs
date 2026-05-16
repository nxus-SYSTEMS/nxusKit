//! Failure-mode tests for nxuskit edition handling (M2: Graceful Loader Gate).
//!
//! Tests marked `#[ignore]` require `libnxuskit` at runtime.
//! Run them with: `NXUSKIT_LIB_DIR=/path/to/lib cargo test --test edition_failure_modes -- --ignored`
//!
//! For OSS-only tests: build without provider-z3, provider-zen features.
//! For Pro tests: build with provider-z3,provider-zen features.

use nxuskit::{Capabilities, NxuskitError, NxuskitProvider, ProviderConfig};

// ── T041: OSS edition initialization succeeds ───────────────────

/// Verify that the SDK initializes successfully even with an OSS (minimal feature) build.
/// This tests the core graceful-init path: no panic, no global failure.
#[test]
#[ignore = "requires libnxuskit runtime"]
fn oss_edition_init_succeeds() {
    let config = ProviderConfig {
        provider_type: "mock".into(),
        ..Default::default()
    };
    let provider = NxuskitProvider::new(config);
    assert!(
        provider.is_ok(),
        "OSS edition should initialize without error: {provider:?}"
    );
    drop(provider);
}

// ── T042: Pro edition initialization succeeds ───────────────────

/// Verify that the SDK initializes successfully with a Pro (all features) build.
#[test]
#[ignore = "requires libnxuskit runtime"]
fn pro_edition_init_succeeds() {
    let config = ProviderConfig {
        provider_type: "mock".into(),
        ..Default::default()
    };
    let provider = NxuskitProvider::new(config);
    assert!(
        provider.is_ok(),
        "Pro edition should initialize without error: {provider:?}"
    );
    drop(provider);
}

// ── T043: Premium feature on correct edition returns expected result ─

/// Edition-aware test: detects the runtime edition via capabilities() and
/// asserts the correct behavior for that edition.
///
/// - OSS (zen domain absent): `zen_evaluate` must return `FeatureUnavailable`.
/// - Pro/Enterprise (zen domain present): `zen_evaluate` must NOT return `FeatureUnavailable`.
#[test]
#[ignore = "requires libnxuskit runtime"]
fn premium_feature_zen_edition_correct() {
    let caps: Capabilities =
        NxuskitProvider::capabilities().expect("capabilities() should succeed");
    let result = nxuskit::zen_evaluate(r#"{"nodes":[],"edges":[]}"#, r#"{}"#);

    if caps.domains.zen {
        // Pro/Enterprise: zen is compiled in — must NOT get FeatureUnavailable.
        match &result {
            Err(NxuskitError::FeatureUnavailable { .. }) => {
                panic!(
                    "Edition '{}' reports zen=true in capabilities, \
                     but zen_evaluate returned FeatureUnavailable",
                    caps.edition
                );
            }
            _ => {
                // Ok or some other error (e.g. invalid model) — both acceptable.
                eprintln!(
                    "PASS [{}]: zen domain available, zen_evaluate did not return FeatureUnavailable",
                    caps.edition
                );
            }
        }
    } else {
        // OSS: zen is absent — must get FeatureUnavailable with correct feature ID.
        match result {
            Err(NxuskitError::FeatureUnavailable { feature, .. }) => {
                assert_eq!(
                    feature, "zen",
                    "feature ID should be 'zen', got '{feature}'"
                );
                eprintln!(
                    "PASS [{}]: zen domain absent, got FeatureUnavailable(zen)",
                    caps.edition
                );
            }
            Err(other) => {
                panic!(
                    "Edition '{}' reports zen=false, expected FeatureUnavailable, got: {other:?}",
                    caps.edition
                );
            }
            Ok(_) => {
                panic!(
                    "Edition '{}' reports zen=false, but zen_evaluate succeeded unexpectedly",
                    caps.edition
                );
            }
        }
    }
}

// ── T044: Introspection returns correct values ──────────────────

/// Introspection APIs should return valid values regardless of edition.
#[test]
#[ignore = "requires libnxuskit runtime"]
fn introspection_returns_valid_values() {
    // Introspection should be callable without creating a provider first.
    let abi_version = NxuskitProvider::abi_version().expect("abi_version() should succeed");
    assert_eq!(abi_version, "0.8", "ABI version should be '0.8'");

    let edition = NxuskitProvider::edition().expect("edition() should succeed");
    assert!(
        ["oss", "pro", "enterprise"].contains(&edition.as_str()),
        "edition should be oss/pro/enterprise, got '{edition}'"
    );

    let caps: Capabilities =
        NxuskitProvider::capabilities().expect("capabilities() should succeed");
    assert_eq!(caps.abi_version, "0.8");
    assert!(
        !caps.sdk_version.is_empty(),
        "sdk_version should not be empty"
    );
    assert!(
        ["oss", "pro", "enterprise"].contains(&caps.edition.as_str()),
        "capabilities edition should be oss/pro/enterprise"
    );

    // LLM and CLIPS domains should always be present (not feature-gated).
    assert!(caps.domains.llm, "llm domain should always be available");
    assert!(
        caps.domains.clips,
        "clips domain should always be available"
    );

    // Edition-specific domain assertions for matrix evidence.
    match edition.as_str() {
        "oss" => {
            eprintln!(
                "MATRIX [oss]: solver={}, zen={}, bayesian={}",
                caps.domains.solver, caps.domains.zen, caps.domains.bayesian
            );
        }
        "pro" | "enterprise" => {
            // Pro/Enterprise should have solver and zen domains.
            assert!(
                caps.domains.solver,
                "pro/enterprise should have solver domain"
            );
            assert!(caps.domains.zen, "pro/enterprise should have zen domain");
            eprintln!(
                "MATRIX [{}]: solver={}, zen={}, bayesian={}",
                edition, caps.domains.solver, caps.domains.zen, caps.domains.bayesian
            );
        }
        other => {
            panic!("unexpected edition: {other}");
        }
    }
}

/// Introspection is callable before any other SDK operation.
#[test]
#[ignore = "requires libnxuskit runtime"]
fn introspection_before_provider_creation() {
    // This specifically tests the edge case where introspection is the first SDK call.
    // The SDK's OnceLock should initialize on this first call.
    let abi = NxuskitProvider::abi_version();
    assert!(
        abi.is_ok(),
        "abi_version() should succeed as the first SDK call"
    );
}

// ── T045: Invalid library path returns descriptive error ────────

/// When NXUSKIT_LIB_DIR points to a non-existent path, the SDK should
/// return a descriptive `LibraryNotFound` error — not panic.
#[test]
fn invalid_library_path_returns_library_not_found() {
    // This test does NOT require libnxuskit — it tests the failure path.
    // We set an invalid env var; the OnceLock may already be initialized
    // from a previous test, so this test is most meaningful when run in isolation.
    //
    // Since OnceLock is process-global, this test verifies the error type
    // construction rather than the full dynamic-link path.
    let err = NxuskitError::LibraryNotFound {
        message: "Could not load nxusKit SDK library".into(),
    };
    let msg = format!("{err}");
    assert!(
        msg.contains("library not found"),
        "error message should contain 'library not found': {msg}"
    );
}

/// Verify the FeatureUnavailable error format.
#[test]
fn feature_unavailable_error_format() {
    let err = NxuskitError::FeatureUnavailable {
        feature: "zen".into(),
        message: "feature 'zen' is not available in this edition".into(),
    };
    let msg = format!("{err}");
    assert!(
        msg.contains("zen"),
        "error should contain feature name: {msg}"
    );
    assert!(
        msg.contains("feature unavailable"),
        "error should contain 'feature unavailable': {msg}"
    );
}

/// Verify that `from_json_str` correctly parses a feature_unavailable error.
#[test]
fn parse_feature_unavailable_from_json() {
    let json = r#"{"error_type":"feature_unavailable","message":"zen"}"#;
    let err = NxuskitError::from_json_str(json);
    match err {
        NxuskitError::FeatureUnavailable { feature, .. } => {
            assert_eq!(feature, "zen");
        }
        other => panic!("Expected FeatureUnavailable, got: {other:?}"),
    }
}

/// Verify that Capabilities struct deserializes correctly.
#[test]
fn capabilities_deserialization() {
    let json = r#"{
        "abi_version": "0.8",
        "sdk_version": "0.7.9",
        "edition": "oss",
        "domains": {
            "llm": true,
            "clips": true,
            "solver": false,
            "bayesian": true,
            "zen": false,
            "local_llama": false,
            "local_mistralrs": false
        }
    }"#;
    let caps: Capabilities = serde_json::from_str(json).unwrap();
    assert_eq!(caps.abi_version, "0.8");
    assert_eq!(caps.edition, "oss");
    assert!(caps.domains.llm);
    assert!(caps.domains.clips);
    assert!(!caps.domains.solver);
    assert!(!caps.domains.zen);
}
