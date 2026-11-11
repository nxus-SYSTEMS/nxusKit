#![allow(clippy::panic)]

mod license_fixture_support;

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

use license_fixture_support::sign_license_fixture_claims_with_kid;
use serde_json::Value;

const FROZEN_CLAIMS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/licensing/deployment-claims-v2.json"
));
const CURRENT_KID: &str = "projects/nxus-licensing-prod/locations/us-central1/keyRings/licensing-signing/cryptoKeys/license-token-signing/cryptoKeyVersions/1";

fn frozen_claims() -> Value {
    serde_json::from_str(FROZEN_CLAIMS)
        .expect("the accepted deterministic v2 deployment fixture must be valid JSON")
}

#[derive(Debug, Eq, PartialEq)]
struct DebugArtifactLayout {
    target_dir: PathBuf,
    lib_dir: PathBuf,
    library: PathBuf,
}

fn debug_artifact_layout(test_executable: &Path) -> Result<DebugArtifactLayout, &'static str> {
    let lib_dir = test_executable
        .parent()
        .filter(|path| path.file_name() == Some(OsStr::new("deps")))
        .ok_or("integration-test executable must run from a deps directory")?;
    let profile_dir = lib_dir
        .parent()
        .filter(|path| path.file_name() == Some(OsStr::new("debug")))
        .ok_or("C ABI license proof requires a debug integration-test executable")?;
    let target_dir = profile_dir
        .parent()
        .ok_or("debug profile directory must have a target parent")?;
    let library = lib_dir.join(if cfg!(target_os = "macos") {
        "libnxuskit_core.dylib"
    } else if cfg!(target_os = "windows") {
        "nxuskit_core.dll"
    } else {
        "libnxuskit_core.so"
    });

    Ok(DebugArtifactLayout {
        target_dir: target_dir.to_owned(),
        lib_dir: lib_dir.to_owned(),
        library,
    })
}

#[test]
fn executable_c_consumer_validates_v2_and_rejects_v3_without_echoing_sensitive_data() {
    let core_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let engine_workspace = core_dir
        .parent()
        .and_then(std::path::Path::parent)
        .expect("nxuskit-engine workspace must contain the core package");
    let current_exe = std::env::current_exe()
        .expect("the debug C ABI proof must resolve its integration-test executable");
    let layout = debug_artifact_layout(&current_exe)
        .unwrap_or_else(|error| panic!("cannot resolve debug C ABI artifacts: {error}"));
    let target_dir = layout.target_dir.to_string_lossy().into_owned();
    let make_target_dir = target_dir.replace('\\', "/");
    let make_lib_dir = layout.lib_dir.to_string_lossy().replace('\\', "/");
    let library = layout.library;
    assert!(
        library.is_file(),
        "the debug C ABI library must be built before this executable consumer: {}",
        library.display()
    );

    let claims = frozen_claims();
    let (v2_token, public_key_pem) = sign_license_fixture_claims_with_kid(&claims, CURRENT_KID);
    let mut v3_claims = claims;
    v3_claims["sdk_release_line"] = serde_json::json!(3);
    let (v3_token, _) = sign_license_fixture_claims_with_kid(&v3_claims, CURRENT_KID);
    let key_dir = tempfile::tempdir().expect("temporary debug verifier key directory");
    let key_path = key_dir.path().join("deterministic-c-fixture-public.pem");
    std::fs::write(&key_path, public_key_pem)
        .expect("deterministic debug verifier PEM must be writable");

    let mut command = Command::new("make");
    if cfg!(target_os = "windows") {
        command.args(["CC=gcc", "SHELL=bash"]);
    }
    let output = command
        .arg(format!("LIB_DIR={make_lib_dir}"))
        .args(["-C", "crates/nxuskit-core/tests/c", "deployment-token-v2"])
        .current_dir(engine_workspace)
        .env("CARGO_TARGET_DIR", &make_target_dir)
        .env("NXUS_SIGNING_KEY_PATH", &key_path)
        .env("NXUSKIT_DEPLOYMENT_TOKEN_V2", v2_token)
        .env("NXUSKIT_DEPLOYMENT_TOKEN_V3", v3_token)
        .output()
        .expect("the local C compiler and make must be available for C ABI proof");
    assert!(
        output.status.success(),
        "executable C consumer must pass ({}):\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("PASS: C ABI DeploymentClaimsV2 fixture consumer")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expected_library_name() -> &'static str {
        if cfg!(target_os = "macos") {
            "libnxuskit_core.dylib"
        } else if cfg!(target_os = "windows") {
            "nxuskit_core.dll"
        } else {
            "libnxuskit_core.so"
        }
    }

    #[test]
    fn debug_artifact_layout_resolves_host_and_explicit_targets() {
        let root = std::env::temp_dir().join("nxuskit-artifact-layout");
        let cases = [
            ("target", "license_v2_c_consumer_test-0123456789abcdef"),
            (
                "target/x86_64-pc-windows-msvc",
                "license_v2_c_consumer_test-0123456789abcdef.exe",
            ),
        ];

        for (target_suffix, executable_name) in cases {
            let target_dir = root.join(target_suffix);
            let lib_dir = target_dir.join("debug/deps");
            let layout = debug_artifact_layout(&lib_dir.join(executable_name))
                .expect("host and explicit-target debug layouts must resolve");

            assert_eq!(layout.target_dir, target_dir);
            assert_eq!(layout.lib_dir, lib_dir);
            assert_eq!(layout.library, lib_dir.join(expected_library_name()));
        }
    }

    #[test]
    fn debug_artifact_layout_rejects_non_deps_parent() {
        let executable = std::env::temp_dir()
            .join("nxuskit-invalid-artifact-layout/target/debug/examples/license_test");

        let error = debug_artifact_layout(&executable)
            .expect_err("non-deps executable parent must be rejected");

        assert!(error.contains("deps"), "unexpected error: {error}");
    }

    #[test]
    fn debug_artifact_layout_rejects_release_profile() {
        let executable = std::env::temp_dir()
            .join("nxuskit-release-artifact-layout/target/release/deps/license_test");

        let error = debug_artifact_layout(&executable)
            .expect_err("release executable layout must be rejected");

        assert!(error.contains("debug"), "unexpected error: {error}");
    }
}
