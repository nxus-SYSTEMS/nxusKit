//! T011 — Engine-internal import boundary test.
//!
//! Pins the Phase-2 invariant that the public `nxuskit::contract::v1_1`
//! surface does not import from `nxuskit_engine`, `clips_sys`, or any
//! engine-internal crate. The narrow `engine_adapter.rs` chokepoint
//! (Phase 3, T029) is the only file allowed to do so; today the chokepoint
//! does not yet exist, so the rule is the simpler "no engine imports
//! anywhere under `src/contract/v1_1/`."
//!
//! This is a *file-content* boundary test, not a runtime one. A stub or a
//! drive-by refactor that grafted an engine-internal type into the public
//! contract surface would trip this assertion immediately.

use std::path::PathBuf;

fn contract_v11_src_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/contract/v1_1")
}

fn iter_rust_files(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).expect("read contract dir") {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            out.extend(iter_rust_files(&path));
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    out
}

#[test]
fn no_engine_internal_imports_in_contract_v11() {
    let banned_substrings = [
        "use nxuskit_engine",
        "nxuskit_engine::",
        "use clips_sys",
        "clips_sys::",
    ];
    // Phase-2 carve-out: `engine_adapter.rs` is the planned single
    // chokepoint (Phase 3, T029). The file does not exist yet; if it
    // appears later, this test still flags it because the carve-out is
    // explicit in Phase 3, not Phase 2.
    let phase_2_allowlist: [&str; 0] = [];

    let files = iter_rust_files(&contract_v11_src_dir());
    assert!(
        !files.is_empty(),
        "contract/v1_1 source dir must exist with at least mod.rs"
    );

    for path in files {
        let body = std::fs::read_to_string(&path).expect("read contract source");
        let fname = path.file_name().unwrap().to_string_lossy().to_string();
        if phase_2_allowlist.contains(&fname.as_str()) {
            continue;
        }
        for needle in banned_substrings {
            assert!(
                !body.contains(needle),
                "engine-internal import {needle:?} found in {} — public contract \
                 must talk to the engine only through the narrow adapter \
                 (engine_adapter.rs, Phase 3 T029)",
                path.display()
            );
        }
    }
}

#[test]
fn contract_mod_declares_versioned_namespace() {
    let mod_rs = std::fs::read_to_string(contract_v11_src_dir().parent().unwrap().join("mod.rs"))
        .expect("read contract/mod.rs");
    assert!(
        mod_rs.contains("pub mod v1_1"),
        "contract/mod.rs must publish the v1_1 namespace"
    );
}

#[test]
fn contract_v1_1_mod_pins_contract_version_constant() {
    let mod_rs = std::fs::read_to_string(contract_v11_src_dir().join("mod.rs"))
        .expect("read contract/v1_1/mod.rs");
    assert!(
        mod_rs.contains("pub const CONTRACT_VERSION: &str = \"1.1\""),
        "contract/v1_1/mod.rs must pin CONTRACT_VERSION = \"1.1\""
    );
}

#[test]
fn peeler_consumer_smoke_example_has_no_engine_internal_dependency() {
    // T023 — Direct-consumption smoke MUST depend only on the public
    // `nxuskit` crate. Declaring `nxuskit-engine`, `clips-sys`, `nxuskit-core`,
    // or `nxuskit-cli` in the example's manifest is a contract violation
    // (FR-027 / contracts/direct-consumption-smoke.md).
    let example_manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent() // packages
        .and_then(|p| p.parent()) // repo root
        .expect("repo root")
        .join("examples/peeler-consumer-smoke/Cargo.toml");
    assert!(
        example_manifest.exists(),
        "expected example crate at {}",
        example_manifest.display()
    );
    let body = std::fs::read_to_string(&example_manifest).expect("read example manifest");
    let banned = ["nxuskit-engine", "clips-sys", "nxuskit-core", "nxuskit-cli"];
    for needle in banned {
        assert!(
            !body.contains(needle),
            "examples/peeler-consumer-smoke/Cargo.toml MUST NOT declare {needle} \
             as a dependency (FR-027 direct-consumption smoke boundary)"
        );
    }
    // Positive assertion: the public `nxuskit` crate IS the dependency.
    assert!(
        body.contains("nxuskit = "),
        "examples/peeler-consumer-smoke/Cargo.toml must depend on the public `nxuskit` crate"
    );
}
