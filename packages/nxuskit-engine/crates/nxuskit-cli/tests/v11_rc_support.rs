//! Shared CLI integration-test helpers for feature 106 (v1.1.0 RC).
//!
//! These helpers are imported by the binary-level integration tests under
//! `tests/contract_v11_*.rs`. They centralize:
//!
//! - Locating the compiled `nxuskit-cli` binary via `cargo_bin`.
//! - Public-boundary deny-list scanning so every test asserts the new
//!   surfaces stay public-CE-safe.
//!
//! This file MUST stay test-only. It is referenced via `#[path = "..."]`
//! includes (or via the integration-test convention where every `.rs`
//! under `tests/` compiles as its own binary) — see each test file.

#![allow(dead_code)]

use std::path::PathBuf;
use std::process::Command;

/// Locate the compiled `nxuskit-cli` binary built by Cargo for tests.
///
/// Mirrors what `assert_cmd::cargo_bin` does without taking a dependency
/// on `assert_cmd` (we already use `Command` directly in this crate).
pub fn cli_binary() -> PathBuf {
    let mut path = std::env::current_exe().expect("current_exe");
    // `target/debug/deps/contract_v11_*-<hash>` → `target/debug/`
    path.pop();
    if path.ends_with("deps") {
        path.pop();
    }
    path.push("nxuskit-cli");
    path
}

/// Build a `Command` for the compiled CLI.
pub fn cli_cmd() -> Command {
    Command::new(cli_binary())
}

/// Public-boundary deny-list shared by feature 106 binary tests.
///
/// Source of truth: the public-boundary deny-list contract.
///
/// The list is intentionally narrow — substring search over CLI stdout —
/// so that legitimate documented vocabulary (e.g., `clips_rule_eval`,
/// `arrow_runtime`, `loopback:v1`) is not in this list. The
/// allow-list / documented exceptions are enforced by inverting the
/// match in each test where they apply.
pub const PUBLIC_BOUNDARY_DENYLIST: &[&str] = &[
    // Internal types and routing vocabulary
    concat!("Provider", "Channel"),
    concat!("Tool", "Catalog"),
    concat!("Request", "Envelope"),
    concat!("Result", "Envelope"),
    "internal-channel",
    "provider-routing",
    "provider_selection_internals",
    // Internal repo / persona identifiers
    concat!("nxusKit", "-internal"),
    "nxusKit-examples-internal",
    "peeler-internal",
    "peeler-internals",
    "peeler-personas",
    // Library filename hints
    "libnxuskit_",
    "libclips_",
    // Zenoh internals
    "zenoh://",
    "key_expr",
    "zenoh_session",
    "zenoh_route_id",
    // Pro internals heuristics
    "pro_only",
    "pro_internal",
    "pro_entitlement_secret",
];

/// Assert that the given `output` (stdout) does not contain any
/// deny-list substring. Fails the test with the matched substring on
/// any hit.
pub fn assert_no_public_boundary_leak(output: &str) {
    for needle in PUBLIC_BOUNDARY_DENYLIST {
        assert!(
            !output.contains(needle),
            "public-boundary leak: output contained deny-list substring {needle:?}",
        );
    }
}
