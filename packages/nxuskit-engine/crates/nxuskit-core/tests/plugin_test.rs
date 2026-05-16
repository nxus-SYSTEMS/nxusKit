#![allow(clippy::panic)]
//! Integration tests for plugin loading.
//!
//! Tests the plugin module through its public Rust API (PluginRegistry).
//! C ABI symbol presence is verified by the ABI conformance test.
//!
//! IMPORTANT: These tests share a global PluginRegistry singleton and modify
//! environment variables, so they MUST run sequentially.

use std::path::Path;
use std::sync::Mutex;

/// Global mutex to serialize plugin tests (shared registry + env vars).
static TEST_MUTEX: Mutex<()> = Mutex::new(());

fn host_abi_version() -> String {
    let ptr = nxuskit_core::nxuskit_abi_version();
    assert!(!ptr.is_null(), "host ABI version pointer must not be null");
    unsafe { std::ffi::CStr::from_ptr(ptr) }
        .to_str()
        .expect("host ABI version must be valid UTF-8")
        .to_string()
}

// ── US1 Tests: Plugin Discovery and Loading ────────────────────────

#[test]
fn test_load_dir_nonexistent() {
    let _lock = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    nxuskit_core::plugin::PluginRegistry::unload_all();
    let count = nxuskit_core::plugin::PluginRegistry::load_dir(Path::new("/nonexistent/dir"));
    assert_eq!(count, 0, "Nonexistent dir should return 0");
}

#[test]
fn test_load_dir_empty() {
    let _lock = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    nxuskit_core::plugin::PluginRegistry::unload_all();
    let tmp = tempfile::tempdir().unwrap();
    let count = nxuskit_core::plugin::PluginRegistry::load_dir(tmp.path());
    assert_eq!(count, 0, "Empty dir should return 0");
}

#[test]
fn test_load_dir_file_instead_of_dir() {
    let _lock = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    nxuskit_core::plugin::PluginRegistry::unload_all();
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let count = nxuskit_core::plugin::PluginRegistry::load_dir(tmp.path());
    assert_eq!(count, 0, "File path should return 0");
}

#[test]
fn test_list_empty() {
    let _lock = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    nxuskit_core::plugin::PluginRegistry::unload_all();
    let list = nxuskit_core::plugin::PluginRegistry::list();
    assert!(list.is_empty(), "Empty registry should return empty list");
}

#[test]
fn test_count_empty() {
    let _lock = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    nxuskit_core::plugin::PluginRegistry::unload_all();
    assert_eq!(nxuskit_core::plugin::PluginRegistry::count(), 0);
}

#[test]
fn test_is_loaded_nonexistent() {
    let _lock = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    nxuskit_core::plugin::PluginRegistry::unload_all();
    assert!(!nxuskit_core::plugin::PluginRegistry::is_loaded(
        "nonexistent"
    ));
}

#[test]
fn test_info_nonexistent() {
    let _lock = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    nxuskit_core::plugin::PluginRegistry::unload_all();
    assert!(nxuskit_core::plugin::PluginRegistry::info("nonexistent").is_none());
}

#[test]
fn test_unload_all_idempotent() {
    let _lock = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    nxuskit_core::plugin::PluginRegistry::unload_all();
    // Call again — must be idempotent (no double-lock; same mutex guard held)
    nxuskit_core::plugin::PluginRegistry::unload_all();
    assert_eq!(nxuskit_core::plugin::PluginRegistry::count(), 0);
}

// ── Tests requiring mock plugin binary ─────────────────────────────

/// Get the pre-built mock plugin library path.
///
/// The mock plugin must be pre-built before running these tests:
///   cd tests/plugin_fixtures/mock_plugin && CARGO_TARGET_DIR=./target cargo build --release
///
/// Tests that need the mock plugin are skipped if it's not built.
fn mock_plugin_lib() -> Option<std::path::PathBuf> {
    let ws_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent() // nxuskit-core -> crates
        .and_then(|p| p.parent()) // crates -> nxuskit-engine
        .and_then(|p| p.parent()) // nxuskit-engine -> packages
        .and_then(|p| p.parent()) // packages -> workspace root
        .unwrap();

    let mock_dir = ws_root.join("tests/plugin_fixtures/mock_plugin");

    let (prefix, ext) = if cfg!(target_os = "macos") {
        ("lib", "dylib")
    } else if cfg!(target_os = "windows") {
        ("", "dll") // Windows cdylib has no lib prefix
    } else {
        ("lib", "so")
    };

    let lib_path = mock_dir.join(format!("target/release/{prefix}mock_plugin.{ext}"));
    if lib_path.exists() {
        Some(lib_path)
    } else {
        None
    }
}

/// Get mock plugin path, panicking with instructions if not built.
fn require_mock_plugin() -> std::path::PathBuf {
    mock_plugin_lib().unwrap_or_else(|| {
        panic!(
            "Mock plugin not pre-built. Run:\n  \
             cd tests/plugin_fixtures/mock_plugin && CARGO_TARGET_DIR=./target cargo build --release"
        )
    })
}

/// Set up a signed mock plugin in a temp directory.
/// Returns (temp_dir, trust_keys_path).
fn setup_signed_plugin(name: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let mock_lib = require_mock_plugin();
    let tmp = tempfile::tempdir().unwrap();

    let ext = if cfg!(target_os = "macos") {
        "dylib"
    } else if cfg!(target_os = "windows") {
        "dll"
    } else {
        "so"
    };

    // Copy mock library
    let lib_dest = tmp.path().join(format!("{name}.{ext}"));
    std::fs::copy(&mock_lib, &lib_dest).unwrap();

    // Read binary for signing
    let binary = std::fs::read(&lib_dest).unwrap();

    // Generate deterministic test keypair and sign
    use ed25519_dalek::{Signer, SigningKey};
    let seed: [u8; 32] = [
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
        0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e,
        0x1f, 0x20,
    ];
    let signing_key = SigningKey::from_bytes(&seed);
    let verifying_key = signing_key.verifying_key();
    let signature = signing_key.sign(&binary);

    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD;
    let sig_b64 = b64.encode(signature.to_bytes());
    let pub_key_b64 = b64.encode(verifying_key.to_bytes());

    // Write manifest
    let manifest = serde_json::json!({
        "schema_version": "1.0",
        "name": name,
        "version": "0.1.0",
        "abi_version": host_abi_version(),
        "capabilities": [format!("{name}-cap")],
        "signature": sig_b64,
    });
    std::fs::write(
        tmp.path().join(format!("{name}.json")),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();

    // Write trust keys
    let trust = serde_json::json!({
        "schema_version": "1.0",
        "keys": [{
            "id": "test-key-001",
            "public_key": pub_key_b64,
            "description": "Test signing key"
        }]
    });
    let trust_path = tmp.path().join("trust_keys.json");
    std::fs::write(&trust_path, serde_json::to_string_pretty(&trust).unwrap()).unwrap();

    (tmp, trust_path)
}

#[test]
fn test_load_signed_plugin() {
    let _lock = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    nxuskit_core::plugin::PluginRegistry::unload_all();

    let (tmp, trust_path) = setup_signed_plugin("test-plugin");

    // Set trust keys env var (unsafe in Rust 2024 edition)
    unsafe { std::env::set_var("NXUSKIT_TRUSTED_KEYS_FILE", trust_path.to_str().unwrap()) };

    let count = nxuskit_core::plugin::PluginRegistry::load_dir(tmp.path());
    assert_eq!(count, 1, "Should load 1 plugin");

    // Verify count
    assert_eq!(nxuskit_core::plugin::PluginRegistry::count(), 1);

    // Verify is_loaded
    assert!(nxuskit_core::plugin::PluginRegistry::is_loaded(
        "test-plugin"
    ));
    assert!(!nxuskit_core::plugin::PluginRegistry::is_loaded(
        "nonexistent"
    ));

    // Verify list
    let list = nxuskit_core::plugin::PluginRegistry::list();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "test-plugin");
    assert_eq!(list[0].version, "0.1.0");
    assert_eq!(list[0].abi_version, host_abi_version());
    assert_eq!(list[0].capabilities, vec!["test-plugin-cap"]);

    // Verify info
    let info = nxuskit_core::plugin::PluginRegistry::info("test-plugin").unwrap();
    assert_eq!(info.name, "test-plugin");
    assert_eq!(info.abi_version, host_abi_version());
    assert!(info.init_metadata.is_some());
    // The mock plugin returns JSON metadata
    let meta: serde_json::Value =
        serde_json::from_str(info.init_metadata.as_ref().unwrap()).unwrap();
    assert_eq!(meta["plugin"], "mock-plugin");

    // Cleanup (lock already held from start of test)
    nxuskit_core::plugin::PluginRegistry::unload_all();
    assert_eq!(nxuskit_core::plugin::PluginRegistry::count(), 0);

    unsafe { std::env::remove_var("NXUSKIT_TRUSTED_KEYS_FILE") };
}

#[test]
fn test_reject_abi_mismatch() {
    let _lock = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    nxuskit_core::plugin::PluginRegistry::unload_all();

    let mock_lib = require_mock_plugin();
    let tmp = tempfile::tempdir().unwrap();

    let ext = if cfg!(target_os = "macos") {
        "dylib"
    } else if cfg!(target_os = "windows") {
        "dll"
    } else {
        "so"
    };

    let lib_dest = tmp.path().join(format!("bad-abi.{ext}"));
    std::fs::copy(&mock_lib, &lib_dest).unwrap();
    let binary = std::fs::read(&lib_dest).unwrap();

    use base64::Engine;
    use ed25519_dalek::{Signer, SigningKey};
    let seed: [u8; 32] = [
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
        0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e,
        0x1f, 0x20,
    ];
    let signing_key = SigningKey::from_bytes(&seed);
    let verifying_key = signing_key.verifying_key();
    let signature = signing_key.sign(&binary);
    let b64 = base64::engine::general_purpose::STANDARD;

    // Manifest with wrong ABI version
    let manifest = serde_json::json!({
        "schema_version": "1.0",
        "name": "bad-abi",
        "version": "0.1.0",
        "abi_version": "0.7",
        "capabilities": ["bad-cap"],
        "signature": b64.encode(signature.to_bytes()),
    });
    std::fs::write(
        tmp.path().join("bad-abi.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let trust = serde_json::json!({
        "schema_version": "1.0",
        "keys": [{"id": "t", "public_key": b64.encode(verifying_key.to_bytes())}]
    });
    let trust_path = tmp.path().join("keys.json");
    std::fs::write(&trust_path, serde_json::to_string_pretty(&trust).unwrap()).unwrap();
    unsafe { std::env::set_var("NXUSKIT_TRUSTED_KEYS_FILE", trust_path.to_str().unwrap()) };

    let count = nxuskit_core::plugin::PluginRegistry::load_dir(tmp.path());
    assert_eq!(count, 0, "ABI mismatch should reject plugin");

    // Cleanup (lock already held from start of test)
    nxuskit_core::plugin::PluginRegistry::unload_all();
    unsafe { std::env::remove_var("NXUSKIT_TRUSTED_KEYS_FILE") };
}

#[test]
fn test_reject_malformed_manifest() {
    let _lock = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    nxuskit_core::plugin::PluginRegistry::unload_all();

    let mock_lib = require_mock_plugin();
    let tmp = tempfile::tempdir().unwrap();

    let ext = if cfg!(target_os = "macos") {
        "dylib"
    } else if cfg!(target_os = "windows") {
        "dll"
    } else {
        "so"
    };

    let lib_dest = tmp.path().join(format!("malformed.{ext}"));
    std::fs::copy(&mock_lib, &lib_dest).unwrap();

    // Malformed manifest (missing required fields)
    std::fs::write(
        tmp.path().join("malformed.json"),
        r#"{"schema_version": "1.0", "name": "malformed"}"#,
    )
    .unwrap();

    let count = nxuskit_core::plugin::PluginRegistry::load_dir(tmp.path());
    assert_eq!(count, 0, "Malformed manifest should reject plugin");

    // Cleanup (lock already held from start of test)
    nxuskit_core::plugin::PluginRegistry::unload_all();
}

#[test]
fn test_reject_invalid_signature() {
    let _lock = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    nxuskit_core::plugin::PluginRegistry::unload_all();

    let mock_lib = require_mock_plugin();
    let tmp = tempfile::tempdir().unwrap();

    let ext = if cfg!(target_os = "macos") {
        "dylib"
    } else if cfg!(target_os = "windows") {
        "dll"
    } else {
        "so"
    };

    let lib_dest = tmp.path().join(format!("bad-sig.{ext}"));
    std::fs::copy(&mock_lib, &lib_dest).unwrap();

    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD;

    // Manifest with an invalid signature (wrong bytes)
    let fake_sig = b64.encode([0xAA_u8; 64]);
    let manifest = serde_json::json!({
        "schema_version": "1.0",
        "name": "bad-sig",
        "version": "0.1.0",
        "abi_version": host_abi_version(),
        "capabilities": ["bad-cap"],
        "signature": fake_sig,
    });
    std::fs::write(
        tmp.path().join("bad-sig.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();

    // Provide trust keys so verification runs
    use ed25519_dalek::SigningKey;
    let seed: [u8; 32] = [
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
        0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e,
        0x1f, 0x20,
    ];
    let signing_key = SigningKey::from_bytes(&seed);
    let verifying_key = signing_key.verifying_key();
    let trust = serde_json::json!({
        "schema_version": "1.0",
        "keys": [{"id": "t", "public_key": b64.encode(verifying_key.to_bytes())}]
    });
    let trust_path = tmp.path().join("keys.json");
    std::fs::write(&trust_path, serde_json::to_string_pretty(&trust).unwrap()).unwrap();
    unsafe { std::env::set_var("NXUSKIT_TRUSTED_KEYS_FILE", trust_path.to_str().unwrap()) };

    let count = nxuskit_core::plugin::PluginRegistry::load_dir(tmp.path());
    assert_eq!(count, 0, "Invalid signature should reject plugin");

    // Cleanup (lock already held from start of test)
    nxuskit_core::plugin::PluginRegistry::unload_all();
    unsafe { std::env::remove_var("NXUSKIT_TRUSTED_KEYS_FILE") };
}

#[test]
fn test_capability_conflict() {
    let _lock = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    nxuskit_core::plugin::PluginRegistry::unload_all();

    let mock_lib = require_mock_plugin();
    let tmp = tempfile::tempdir().unwrap();

    let ext = if cfg!(target_os = "macos") {
        "dylib"
    } else if cfg!(target_os = "windows") {
        "dll"
    } else {
        "so"
    };

    use base64::Engine;
    use ed25519_dalek::{Signer, SigningKey};
    let seed: [u8; 32] = [
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
        0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e,
        0x1f, 0x20,
    ];
    let signing_key = SigningKey::from_bytes(&seed);
    let verifying_key = signing_key.verifying_key();
    let b64 = base64::engine::general_purpose::STANDARD;

    // Create two plugins with the same capability
    for name in &["aaa-plugin", "bbb-plugin"] {
        let lib_dest = tmp.path().join(format!("{name}.{ext}"));
        std::fs::copy(&mock_lib, &lib_dest).unwrap();
        let binary = std::fs::read(&lib_dest).unwrap();
        let signature = signing_key.sign(&binary);

        let manifest = serde_json::json!({
            "schema_version": "1.0",
            "name": name,
            "version": "0.1.0",
            "abi_version": host_abi_version(),
            "capabilities": ["shared-cap"],
            "signature": b64.encode(signature.to_bytes()),
        });
        std::fs::write(
            tmp.path().join(format!("{name}.json")),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    let trust = serde_json::json!({
        "schema_version": "1.0",
        "keys": [{"id": "t", "public_key": b64.encode(verifying_key.to_bytes())}]
    });
    let trust_path = tmp.path().join("keys.json");
    std::fs::write(&trust_path, serde_json::to_string_pretty(&trust).unwrap()).unwrap();
    unsafe { std::env::set_var("NXUSKIT_TRUSTED_KEYS_FILE", trust_path.to_str().unwrap()) };

    let count = nxuskit_core::plugin::PluginRegistry::load_dir(tmp.path());
    // aaa-plugin loads first (alphabetical), bbb-plugin rejected for capability conflict
    assert_eq!(
        count, 1,
        "Only first plugin should load (capability conflict)"
    );
    assert!(nxuskit_core::plugin::PluginRegistry::is_loaded(
        "aaa-plugin"
    ));
    assert!(!nxuskit_core::plugin::PluginRegistry::is_loaded(
        "bbb-plugin"
    ));

    // Cleanup (lock already held from start of test)
    nxuskit_core::plugin::PluginRegistry::unload_all();
    unsafe { std::env::remove_var("NXUSKIT_TRUSTED_KEYS_FILE") };
}
