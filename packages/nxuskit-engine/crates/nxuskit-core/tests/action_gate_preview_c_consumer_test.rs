#![allow(clippy::panic)]

use sha2::{Digest, Sha256};
use std::ffi::{CStr, CString, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn parse_ce_proof_lock_sha256(value: &str) -> Result<&str, &'static str> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("CE proof lock identity must be lowercase 64-hex SHA-256");
    }
    Ok(value)
}

fn expected_ce_lock_sha256() -> String {
    let value = std::env::var("NXUSKIT_CE_PROOF_LOCK_SHA256")
        .expect("the final CE runner must supply its independently validated lock identity");
    parse_ce_proof_lock_sha256(&value)
        .expect("the final CE runner supplied a malformed lock identity")
        .to_owned()
}

const ALLOW_FIXTURE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/action_gate_preview/allow.json"
));
const DENY_FIXTURE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/action_gate_preview/deny.json"
));
const REVIEW_FIXTURE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/action_gate_preview/review-required.json"
));
const REVIEW_MISSING_HANDOFF_FIXTURE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/action_gate_preview/review-missing-handoff.json"
));
const MISSING_EVIDENCE_FIXTURE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/action_gate_preview/missing-evidence.json"
));
const FORBIDDEN_AUTHORITY_FIXTURE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/action_gate_preview/forbidden-authority.json"
));

#[derive(Debug)]
struct OfflineProofEnv {
    path: OsString,
    tmpdir: PathBuf,
    home: PathBuf,
    cargo_home: PathBuf,
    rustup_home: OsString,
    target_dir: PathBuf,
}

#[derive(Debug, Eq, PartialEq)]
struct WorkspaceSnapshot {
    entries: Vec<SnapshotEntry>,
    lock_sha256: String,
}

#[derive(Debug, Eq, PartialEq)]
struct SnapshotEntry {
    path: Vec<u8>,
    kind: &'static str,
    mode: u32,
    sha256: String,
}

#[test]
#[ignore = "requires a final public CE projection and proof cache"]
fn executable_c_consumer_matches_shared_preview_fixtures() {
    let core_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let engine_workspace = core_dir
        .parent()
        .and_then(Path::parent)
        .expect("nxuskit-engine workspace must contain the core package");
    let engine_workspace = fs::canonicalize(engine_workspace)
        .expect("nxuskit-engine workspace must resolve as a physical directory");
    assert!(
        engine_workspace.join("Cargo.toml").is_file(),
        "nxuskit-engine workspace manifest must exist"
    );

    let harness = engine_workspace.join("crates/nxuskit-core/tests/c/Makefile");
    let header = engine_workspace.join("crates/nxuskit-core/include/nxuskit.h");
    assert!(
        harness.is_file() && !harness.is_symlink(),
        "package-owned Action Gate C Makefile must exist"
    );
    assert!(
        header.is_file() && !header.is_symlink(),
        "package-owned C ABI header must exist"
    );

    let proof = OfflineProofEnv::from_parent(&engine_workspace);
    let b0 = snapshot_engine_workspace(&engine_workspace);
    assert_workspace_snapshot("B0", &b0);

    let build = offline_cargo_command(&proof)
        .args(["build", "--locked", "--offline", "-p", "nxuskit-core"])
        .current_dir(&engine_workspace)
        .output()
        .expect("the package-local offline Cargo toolchain must be available");
    let b1 = snapshot_engine_workspace(&engine_workspace);
    assert_workspace_snapshot("B1", &b1);
    assert_eq!(
        b0, b1,
        "B0/B1 workspace snapshot mismatch: nested Cargo must not mutate the exported workspace"
    );
    assert!(
        build.status.success(),
        "package-local offline C ABI library build must pass:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    let library = proof
        .target_dir
        .join("debug")
        .join(if cfg!(target_os = "macos") {
            "libnxuskit_core.dylib"
        } else if cfg!(target_os = "windows") {
            "nxuskit_core.dll"
        } else {
            "libnxuskit_core.so"
        });
    assert!(
        library.is_file() && !library.is_symlink(),
        "the fresh debug C ABI library must be built before the C consumer: {}",
        library.display()
    );

    let output = package_c_harness_command(&proof)
        .args(["-C", "crates/nxuskit-core/tests/c", "action-gate-preview"])
        .current_dir(&engine_workspace)
        .env(
            "NXUSKIT_ACTION_GATE_ALLOW_FIXTURE",
            fixture_request(ALLOW_FIXTURE),
        )
        .env(
            "NXUSKIT_ACTION_GATE_DENY_FIXTURE",
            fixture_request(DENY_FIXTURE),
        )
        .env(
            "NXUSKIT_ACTION_GATE_REVIEW_FIXTURE",
            fixture_request(REVIEW_FIXTURE),
        )
        .env(
            "NXUSKIT_ACTION_GATE_REVIEW_MISSING_HANDOFF_FIXTURE",
            fixture_request(REVIEW_MISSING_HANDOFF_FIXTURE),
        )
        .env(
            "NXUSKIT_ACTION_GATE_MISSING_EVIDENCE_FIXTURE",
            fixture_request(MISSING_EVIDENCE_FIXTURE),
        )
        .env(
            "NXUSKIT_ACTION_GATE_FORBIDDEN_AUTHORITY_FIXTURE",
            fixture_request(FORBIDDEN_AUTHORITY_FIXTURE),
        )
        .output()
        .expect("the package-local C compiler and make must be available");
    let b2 = snapshot_engine_workspace(&engine_workspace);
    assert_workspace_snapshot("B2", &b2);
    assert_eq!(
        b1, b2,
        "B1/B2 workspace snapshot mismatch: package Makefile/C consumer must not mutate the exported workspace"
    );
    assert!(
        output.status.success(),
        "package-owned executable C preview consumer must pass:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("PASS: C ABI Action Gate preview consumer")
    );
    println!("PASS: C ABI Action Gate preview consumer");
}

impl OfflineProofEnv {
    fn from_parent(engine_workspace: &Path) -> Self {
        let path = required_env("PATH");
        let rustup_home = required_env("RUSTUP_HOME");
        let tmpdir = proof_owned_external_dir("TMPDIR", engine_workspace);
        let home = proof_owned_external_dir("HOME", engine_workspace);
        let cargo_home = proof_owned_external_dir("CARGO_HOME", engine_workspace);
        let target_dir = proof_owned_external_dir("CARGO_TARGET_DIR", engine_workspace);

        Self {
            path,
            tmpdir,
            home,
            cargo_home,
            rustup_home,
            target_dir,
        }
    }
}

impl OfflineProofEnv {
    fn cargo_envs(&self) -> [(&'static str, OsString); 14] {
        [
            ("PATH", self.path.clone()),
            ("TMPDIR", self.tmpdir.clone().into_os_string()),
            ("HOME", self.home.clone().into_os_string()),
            ("CARGO_HOME", self.cargo_home.clone().into_os_string()),
            ("RUSTUP_HOME", self.rustup_home.clone()),
            ("LC_ALL", OsString::from("C")),
            (
                "CARGO_REGISTRIES_CRATES_IO_PROTOCOL",
                OsString::from("sparse"),
            ),
            (
                "CARGO_REGISTRIES_CRATES_IO_INDEX",
                OsString::from("sparse+https://index.crates.io/"),
            ),
            ("GIT_CONFIG_NOSYSTEM", OsString::from("1")),
            ("GIT_CONFIG_GLOBAL", OsString::from("/dev/null")),
            ("CARGO_NET_OFFLINE", OsString::from("true")),
            ("NXUSKIT_EDITION", OsString::from("oss")),
            ("RUSTFLAGS", OsString::from("-Dunexpected_cfgs")),
            ("CARGO_TARGET_DIR", self.target_dir.clone().into_os_string()),
        ]
    }
}

fn offline_cargo_command(proof: &OfflineProofEnv) -> Command {
    let mut command = Command::new("cargo");
    command.env_clear().envs(proof.cargo_envs());
    command
}

fn package_c_harness_command(proof: &OfflineProofEnv) -> Command {
    let mut command = Command::new("make");
    let make_target_dir = if cfg!(target_os = "windows") {
        OsString::from(proof.target_dir.to_string_lossy().replace('\\', "/"))
    } else {
        proof.target_dir.clone().into_os_string()
    };
    if cfg!(target_os = "windows") {
        command.args(["CC=gcc", "SHELL=bash"]);
    }
    command
        .env_clear()
        .env("PATH", &proof.path)
        .env("CARGO_TARGET_DIR", &make_target_dir);
    command
}

fn required_env(name: &str) -> OsString {
    std::env::var_os(name).unwrap_or_else(|| panic!("{name} must be supplied by the proof harness"))
}

fn proof_owned_external_dir(name: &str, engine_workspace: &Path) -> PathBuf {
    let path = PathBuf::from(required_env(name));
    assert!(
        path.is_absolute(),
        "{name} must be an absolute proof-owned directory"
    );
    let path = fs::canonicalize(&path).unwrap_or_else(|error| {
        panic!("{name} must resolve as an existing physical directory: {error}")
    });
    assert!(
        path.is_dir(),
        "{name} must resolve as a proof-owned directory"
    );
    assert!(
        !path.starts_with(engine_workspace),
        "{name} must be outside the exported nxuskit-engine workspace"
    );
    path
}

fn snapshot_engine_workspace(engine_workspace: &Path) -> WorkspaceSnapshot {
    let lock = engine_workspace.join("Cargo.lock");
    let lock_metadata = fs::symlink_metadata(&lock)
        .unwrap_or_else(|error| panic!("exported Cargo.lock must be present: {error}"));
    assert!(
        lock_metadata.is_file() && !lock_metadata.file_type().is_symlink(),
        "exported Cargo.lock must be a regular non-symlink file"
    );
    let lock_sha256 = sha256_file(&lock);
    assert_eq!(
        lock_sha256,
        expected_ce_lock_sha256(),
        "exported Cargo.lock SHA-256 drifted"
    );

    let mut entries = Vec::new();
    snapshot_children(engine_workspace, engine_workspace, &mut entries);
    WorkspaceSnapshot {
        entries,
        lock_sha256,
    }
}

fn snapshot_children(root: &Path, directory: &Path, entries: &mut Vec<SnapshotEntry>) {
    let mut children = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("cannot snapshot {}: {error}", directory.display()))
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_else(|error| panic!("cannot enumerate {}: {error}", directory.display()));
    children.sort_by(|left, right| {
        left.file_name()
            .as_encoded_bytes()
            .cmp(right.file_name().as_encoded_bytes())
    });

    for child in children {
        let path = child.path();
        let metadata = fs::symlink_metadata(&path)
            .unwrap_or_else(|error| panic!("cannot inspect {}: {error}", path.display()));
        let file_type = metadata.file_type();
        let (kind, bytes) = if file_type.is_dir() {
            ("directory", Vec::new())
        } else if file_type.is_file() {
            (
                "regular",
                fs::read(&path)
                    .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display())),
            )
        } else if file_type.is_symlink() {
            (
                "symlink",
                fs::read_link(&path)
                    .unwrap_or_else(|error| panic!("cannot read link {}: {error}", path.display()))
                    .as_os_str()
                    .as_encoded_bytes()
                    .to_vec(),
            )
        } else {
            panic!("unsupported workspace path type: {}", path.display());
        };
        entries.push(SnapshotEntry {
            path: path
                .strip_prefix(root)
                .expect("workspace child must remain below snapshot root")
                .as_os_str()
                .as_encoded_bytes()
                .to_vec(),
            kind,
            mode: file_mode(&metadata),
            sha256: sha256_bytes(&bytes),
        });
        if file_type.is_dir() {
            snapshot_children(root, &path, entries);
        }
    }
}

fn assert_workspace_snapshot(boundary: &str, snapshot: &WorkspaceSnapshot) {
    assert!(
        !snapshot.entries.is_empty(),
        "{boundary} workspace snapshot must not be empty"
    );
    assert_eq!(
        snapshot.lock_sha256,
        expected_ce_lock_sha256(),
        "{boundary} Cargo.lock identity must remain exact"
    );
}

#[cfg(unix)]
fn file_mode(metadata: &fs::Metadata) -> u32 {
    use std::os::unix::fs::MetadataExt;
    metadata.mode() & 0o7777
}

#[cfg(not(unix))]
fn file_mode(metadata: &fs::Metadata) -> u32 {
    if metadata.permissions().readonly() {
        0o444
    } else {
        0o666
    }
}

fn sha256_file(path: &Path) -> String {
    sha256_bytes(
        &fs::read(path).unwrap_or_else(|error| panic!("cannot hash {}: {error}", path.display())),
    )
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[test]
fn c_abi_rejects_null_and_invalid_utf8_without_returning_allow() {
    let null_result = unsafe {
        nxuskit_core::action_gate_preview::nxuskit_action_gate_preview_evaluate_json(
            std::ptr::null(),
        )
    };
    assert!(null_result.is_null(), "null input must be rejected safely");

    let invalid_utf8 =
        CStr::from_bytes_with_nul(b"\xff\0").expect("fixture must be NUL terminated");
    let invalid_utf8_result = unsafe {
        nxuskit_core::action_gate_preview::nxuskit_action_gate_preview_evaluate_json(
            invalid_utf8.as_ptr(),
        )
    };
    assert!(
        invalid_utf8_result.is_null(),
        "invalid UTF-8 must be rejected safely"
    );

    let malformed = CString::new("{").expect("fixture has no NUL");
    let result_ptr = unsafe {
        nxuskit_core::action_gate_preview::nxuskit_action_gate_preview_evaluate_json(
            malformed.as_ptr(),
        )
    };
    assert!(
        !result_ptr.is_null(),
        "malformed JSON returns structured deny"
    );
    let result = unsafe { CStr::from_ptr(result_ptr) }
        .to_str()
        .expect("structured result is valid UTF-8");
    assert!(result.contains("\"status\":\"invalid_request\""));
    assert!(!result.contains("\"decision\":\"allow\""));
    unsafe { nxuskit_core::nxuskit_free_string(result_ptr) };
}

fn fixture_request(fixture: &str) -> String {
    serde_json::from_str::<serde_json::Value>(fixture).expect("shared fixture must be valid JSON")
        ["request"]
        .to_string()
}
