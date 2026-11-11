mod ce_boundary_helpers;

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use ce_boundary_helpers::{LeakClass, scan_ce_boundary};

fn fixture_root(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time available")
        .as_nanos();
    std::env::temp_dir().join(format!("nxuskit-ce-boundary-{name}-{nonce}"))
}

fn write_fixture(root: &PathBuf, path: &str, contents: &str) {
    let file = root.join(path);
    fs::create_dir_all(file.parent().expect("fixture has parent")).expect("fixture dir created");
    fs::write(file, contents).expect("fixture file written");
}

#[test]
fn clean_ce_boundary_fixture_passes() {
    let root = fixture_root("clean");
    write_fixture(
        &root,
        "src/tools/mod.rs",
        "pub struct ExternalTool; pub enum CeToolPosture { Native, ExternalOnly, Unavailable }",
    );

    let leaks = scan_ce_boundary(&root).expect("boundary scan succeeds");

    assert!(leaks.is_empty(), "clean fixture should not leak: {leaks:?}");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn seeded_leaks_are_classified_by_boundary_helper() {
    let root = fixture_root("seeded");
    write_fixture(&root, "src/lib.rs", "pub use ProviderChannel as Leaked;");
    write_fixture(
        &root,
        "docs/internal.md",
        "This describes Pro internal routing behavior.",
    );
    write_fixture(
        &root,
        "metadata/path.txt",
        "packages/nxuskit-engine/crates/nxuskit-engine/src/pro/private.rs",
    );
    write_fixture(&root, "Cargo.toml", "nxuskit-pro = \"1\"");
    write_fixture(&root, "edition.env", "NXUSKIT_EDITION=pro");

    let leaks = scan_ce_boundary(&root).expect("boundary scan succeeds");
    let classes: Vec<_> = leaks.iter().map(|leak| leak.class.clone()).collect();

    for expected in [
        LeakClass::InternalSymbol,
        LeakClass::ProDoc,
        LeakClass::ProPath,
        LeakClass::ProDependency,
        LeakClass::NonOssEditionAccepted,
    ] {
        assert!(
            classes.contains(&expected),
            "expected seeded leak class {expected:?}, got {leaks:?}"
        );
    }

    let _ = fs::remove_dir_all(root);
}
