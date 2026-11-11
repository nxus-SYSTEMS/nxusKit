//! T010 — Golden fixture inventory pairing test.
//!
//! Asserts that the Phase-2 minimum subset of golden fixtures exists on disk
//! and is parseable as well-formed JSON. The full inventory lands across the
//! user-story phases (US1–US5); Phase 2 only requires the floor set declared
//! in `packages/nxuskit/tests/fixtures/contract_v11/README.md`.
//!
//! A stub that returned empty fixtures or constant content would fail the
//! JSON parse + content assertions below (Article III §Stub and Placeholder
//! Prohibition).

use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/contract_v11")
}

#[test]
fn fixture_index_readme_is_present() {
    let path = fixtures_dir().join("README.md");
    assert!(
        path.exists(),
        "Phase-2 requires fixture index README at {}",
        path.display()
    );
    let body = std::fs::read_to_string(&path).expect("read fixture README");
    // Anchor: the README MUST reference the authoritative planning inventory
    // so reviewers can trace the on-disk inventory back to the contract.
    assert!(
        body.contains("authoritative golden fixture index"),
        "fixture README must reference the authoritative fixture inventory"
    );
    // Anchor: the README MUST enumerate the Phase-2 floor names.
    for must_have in [
        "pipeline_llm_only.json",
        "boundary_positive.json",
        "boundary_negative/internal_repo_name.json",
    ] {
        assert!(
            body.contains(must_have),
            "fixture README must list Phase-2 floor entry {must_have}"
        );
    }
}

#[test]
fn phase_2_floor_fixtures_exist_and_are_well_formed_json() {
    let floor = [
        "pipeline_llm_only.json",
        "boundary_positive.json",
        "boundary_negative/internal_repo_name.json",
    ];
    for name in floor {
        let path = fixtures_dir().join(name);
        assert!(
            path.exists(),
            "Phase-2 floor fixture missing: {}",
            path.display()
        );
        let body = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let value: serde_json::Value =
            serde_json::from_str(&body).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
        // Phase-2 floor fixtures MUST carry the contract_version anchor when
        // they represent a contract DTO. The boundary_positive fixture is a
        // scan-target document and still carries the version anchor so the
        // boundary scan can route on it.
        match value.get("contract_version") {
            Some(v) => assert_eq!(
                v.as_str(),
                Some("1.1"),
                "{} must declare contract_version=1.1",
                path.display()
            ),
            None => panic!(
                "{} must declare a contract_version field (Phase-2 floor rule)",
                path.display()
            ),
        }
    }
}

#[test]
fn negative_control_fixture_carries_seed_class_marker() {
    // The Phase-2 floor negative-control is the `internal_repo_name` class
    // seed. The fixture must explicitly self-identify the seeded class so
    // the boundary scan test (T011 / T035) can route deterministically
    // without re-running the (Phase-4) rule engine.
    let path = fixtures_dir().join("boundary_negative/internal_repo_name.json");
    let body = std::fs::read_to_string(&path).expect("read negative fixture");
    let value: serde_json::Value = serde_json::from_str(&body).expect("parse negative fixture");
    assert_eq!(
        value["seeded_class"].as_str(),
        Some("internal_repo_name"),
        "negative-control fixture must self-identify its seeded class"
    );
    // The fixture itself MUST NOT embed the literal banned token (that would
    // trip the existing leak gate); the seeded token is reconstructed in
    // the Phase-4 scoped negative-control test (T035). The fixture declares
    // the seeded *kind*.
    assert_eq!(
        value["seeded_token_kind"].as_str(),
        Some("internal_repo_name_marker"),
        "negative-control fixture must declare its seeded token kind"
    );
}
