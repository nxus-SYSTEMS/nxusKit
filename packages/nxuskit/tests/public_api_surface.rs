//! T067: Compile-time verification that raw CLIPS types are NOT
//! exported from the nxuskit public API surface.
//!
//! This test verifies that the public API only exposes the encapsulated
//! types (ClipsSession, ClipsValue, SessionInfo, TemplateSlotInfo) and
//! NOT the raw internal types that were removed in v0.9.0.
//!
//! This is a compile-time test: if any of the forbidden types were
//! accidentally re-exported, the assertions here would catch it at
//! test time via absence from the module's public items.

/// Verify that the expected CLIPS session types ARE accessible.
#[test]
fn clips_session_types_are_public() {
    // These should compile — they are intentionally public.
    fn _assert_clips_session_accessible() {
        let _: fn() -> &'static str = || {
            let _ = std::any::type_name::<nxuskit::ClipsSession>();
            let _ = std::any::type_name::<nxuskit::ClipsValue>();
            let _ = std::any::type_name::<nxuskit::SessionInfo>();
            let _ = std::any::type_name::<nxuskit::TemplateSlotInfo>();
            "ok"
        };
    }
}

/// Verify that key public API types are accessible via the crate root.
#[test]
fn core_api_types_are_public() {
    fn _assert_core_types() {
        let _ = std::any::type_name::<nxuskit::NxuskitProvider>();
        let _ = std::any::type_name::<nxuskit::NxuskitError>();
        let _ = std::any::type_name::<nxuskit::ChatRequest>();
        let _ = std::any::type_name::<nxuskit::ChatResponse>();
        let _ = std::any::type_name::<nxuskit::Message>();
        let _ = std::any::type_name::<nxuskit::Role>();
    }
}

/// Verify that Capability Manifest v2 public-preview types are available
/// from the wrapper crate root.
#[test]
fn public_capability_manifest_types_are_public() {
    fn _assert_manifest_types() {
        let _ = std::any::type_name::<nxuskit::CapabilityStatus>();
        let _ = std::any::type_name::<nxuskit::ManifestPublicationPosture>();
        let _ = std::any::type_name::<nxuskit::PublicProviderCapability>();
        let _ = std::any::type_name::<nxuskit::PublicCapabilityManifest>();
    }

    assert_eq!(
        nxuskit::PUBLIC_CAPABILITY_FIELDS,
        &[
            "vision_input",
            "tool_calling",
            "thinking_blocks",
            "streaming_logprobs",
            "json_mode",
            "json_schema_strict",
            "json_schema_best_effort",
            "embeddings",
            "rerank",
        ]
    );
}

/// Verify that the Rust wrapper serializes the public preview projection
/// using the stable public field names and status strings.
#[test]
fn public_capability_manifest_serializes_public_json_keys() {
    let manifest = nxuskit::PublicCapabilityManifest {
        schema_version: "capability-manifest-v2-public-preview/1".to_string(),
        posture: nxuskit::ManifestPublicationPosture::Split,
        providers: vec![nxuskit::PublicProviderCapability {
            name: "openai".to_string(),
            display_name: "OpenAI".to_string(),
            last_reviewed_on: "2026-05-09".to_string(),
            provider_status: "unknown".to_string(),
            capabilities: std::collections::HashMap::from([
                (
                    "json_schema_strict".to_string(),
                    nxuskit::CapabilityStatus::Supported,
                ),
                (
                    "tool_calling".to_string(),
                    nxuskit::CapabilityStatus::ProviderSpecific,
                ),
            ]),
        }],
    };

    let value = serde_json::to_value(manifest).expect("manifest serializes");
    assert_eq!(value["schema_version"], "capability-manifest-v2-public-preview/1");
    assert_eq!(value["posture"], "split");
    assert_eq!(value["providers"][0]["name"], "openai");
    assert_eq!(value["providers"][0]["capabilities"]["json_schema_strict"], "supported");
    assert_eq!(
        value["providers"][0]["capabilities"]["tool_calling"],
        "provider_specific"
    );

    let provider = value["providers"][0].as_object().expect("provider object");
    for internal_key in ["evidence", "model_overrides", "provider_specific", "features"] {
        assert!(
            !provider.contains_key(internal_key),
            "public manifest provider leaked internal key {internal_key}"
        );
    }
}

/// Verify that license types are accessible.
#[test]
fn license_types_are_public() {
    fn _assert_license_types() {
        let _ = std::any::type_name::<nxuskit::LicenseResolution>();
        let _ = std::any::type_name::<nxuskit::TokenInfo>();
        let _ = std::any::type_name::<nxuskit::ActivationResult>();
        let _ = std::any::type_name::<nxuskit::TrialResult>();
    }
}

/// Verify that plugin trust types are accessible.
#[test]
fn plugin_trust_types_are_public() {
    fn _assert_plugin_types() {
        let _ = std::any::type_name::<nxuskit::TrustMode>();
        let _ = std::any::type_name::<nxuskit::PluginInfo>();
    }
}

/// Verify that auth/OAuth types are accessible.
#[test]
fn auth_types_are_public() {
    fn _assert_auth_types() {
        let _ = std::any::type_name::<nxuskit::AuthStatus>();
        let _ = std::any::type_name::<nxuskit::AuthResolution>();
        let _ = std::any::type_name::<nxuskit::OAuthStatus>();
        let _ = std::any::type_name::<nxuskit::OAuthResult>();
    }
}

/// Script-based verification that forbidden types are NOT in public exports.
///
/// Checks that the following internal CLIPS types do NOT appear in `pub use`
/// statements in lib.rs: ClipsEnvironment, FactBuilder, FactRef, FactIterator,
/// TemplateRef.
#[test]
fn raw_clips_types_not_exported() {
    let lib_rs = include_str!("../src/lib.rs");

    let forbidden_types = [
        "ClipsEnvironment",
        "FactBuilder",
        "FactRef",
        "FactIterator",
        "TemplateRef",
    ];

    for forbidden in &forbidden_types {
        // Check that the type name doesn't appear in any `pub use` line
        for line in lib_rs.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("pub use") || trimmed.starts_with("pub mod") {
                assert!(
                    !trimmed.contains(forbidden),
                    "Forbidden type `{forbidden}` found in public export: {trimmed}"
                );
            }
        }
    }
}

/// Verify that the prelude does NOT include raw CLIPS types.
#[test]
fn prelude_is_clean() {
    let lib_rs = include_str!("../src/lib.rs");

    // Find the prelude module block
    let prelude_start = lib_rs
        .find("pub mod prelude")
        .expect("prelude module exists");
    let prelude_section = &lib_rs[prelude_start..];
    // Find closing brace
    let prelude_end = prelude_section
        .find("\n}")
        .expect("prelude module has closing brace");
    let prelude_content = &prelude_section[..prelude_end];

    let forbidden = [
        "ClipsEnvironment",
        "FactBuilder",
        "FactRef",
        "FactIterator",
        "TemplateRef",
        "ClipsSession", // ClipsSession should NOT be in prelude (it's domain-specific)
    ];

    for name in &forbidden {
        assert!(
            !prelude_content.contains(name),
            "Type `{name}` should not be in the prelude module"
        );
    }
}
