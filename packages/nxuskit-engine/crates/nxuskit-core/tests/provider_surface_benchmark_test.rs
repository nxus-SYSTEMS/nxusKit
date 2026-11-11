#![allow(clippy::panic)]
#![allow(clippy::print_stdout)]
//! Independent contract tests for the Spec 119 native performance harness.

#[path = "support/provider_surface_harness.rs"]
mod harness;

use std::fs;
use std::path::Path;
use std::process::Command;

use harness::{
    ALLOCATION_SAMPLE_COUNT, ALLOCATION_WARMUPS, DurationMetric, FFI_SAMPLE_COUNT, FFI_TARGET_NS,
    FFI_WARMUPS, FIRST_CHUNK_SAMPLE_COUNT, FIRST_CHUNK_TARGET_NS, FIRST_CHUNK_WARMUPS,
    LOAD_SAMPLE_COUNT, LOAD_TARGET_NS, LOAD_WARMUPS, MetricKind,
};

#[test]
fn nearest_rank_statistics_are_exact_integer_values() {
    let _serial = harness::serial_test_guard().expect("serial test guard");
    let mut values = vec![9, 1, 4, 2, 8, 3, 7, 5, 6, 10];
    let summary = harness::summarize(&mut values).expect("valid sample vector");
    assert_eq!(summary.median, 5);
    assert_eq!(summary.p95, 10);
    assert_eq!(summary.max, 10);
}

#[test]
fn every_threshold_rejects_the_exact_first_value_over() {
    let _serial = harness::serial_test_guard().expect("serial test guard");
    for (kind, threshold) in [
        (MetricKind::FfiCall, FFI_TARGET_NS),
        (MetricKind::ProviderLoad, LOAD_TARGET_NS),
        (MetricKind::FirstChunk, FIRST_CHUNK_TARGET_NS),
    ] {
        assert!(harness::passes_threshold(kind, threshold));
        assert!(!harness::passes_threshold(kind, threshold + 1));
    }
}

#[test]
fn release_sample_policy_is_frozen_and_not_environment_driven() {
    let _serial = harness::serial_test_guard().expect("serial test guard");
    assert_eq!(
        harness::release_policy(MetricKind::FfiCall),
        (FFI_WARMUPS, FFI_SAMPLE_COUNT)
    );
    assert_eq!(
        harness::release_policy(MetricKind::ProviderLoad),
        (LOAD_WARMUPS, LOAD_SAMPLE_COUNT)
    );
    assert_eq!(
        harness::release_policy(MetricKind::FirstChunk),
        (FIRST_CHUNK_WARMUPS, FIRST_CHUNK_SAMPLE_COUNT)
    );
    assert_eq!(
        harness::release_policy(MetricKind::Allocations),
        (ALLOCATION_WARMUPS, ALLOCATION_SAMPLE_COUNT)
    );
}

#[test]
fn direct_chat_crosses_the_real_c_abi_and_strictly_parses_response() {
    let _serial = harness::serial_test_guard().expect("serial test guard");
    let observation = harness::measure_direct_chat_once().expect("loopback C ABI chat");
    assert!(observation.elapsed_ns > 0);
    assert_eq!(observation.content, "spec119-performance");
}

#[test]
fn streaming_crosses_the_real_c_abi_and_observes_one_first_chunk() {
    let _serial = harness::serial_test_guard().expect("serial test guard");
    let observation = harness::measure_first_chunk_once().expect("loopback C ABI stream");
    assert!(observation.elapsed_ns > 0);
    assert_eq!(observation.first_chunk_count, 1);
    assert!(!observation.delta.is_empty());
}

#[test]
fn allocation_window_is_real_and_reports_primary_count() {
    let _serial = harness::serial_test_guard().expect("serial test guard");
    let executable = std::env::current_exe().expect("current test executable");
    let result = Command::new(executable)
        .args([
            "--exact",
            "allocation_probe_runs_in_an_isolated_serial_process",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("NXUSKIT_S1E_ALLOCATION_CHILD", "1")
        .output()
        .expect("isolated allocation child");
    assert!(
        result.status.success(),
        "isolated allocation child failed:\n{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(
        stdout.contains("NXUSKIT_S1E_ALLOCATION_BOUNDARY=PASS")
            || stdout
                .contains("NXUSKIT_S1E_ALLOCATION_BOUNDARY=DIAGNOSTIC_UNAVAILABLE_CROSS_THREAD"),
        "unexpected allocation diagnostic outcome: {stdout}"
    );
}

#[test]
fn allocation_probe_runs_in_an_isolated_serial_process() {
    if std::env::var("NXUSKIT_S1E_ALLOCATION_CHILD").as_deref() != Ok("1") {
        return;
    }
    match harness::measure_allocations_once() {
        Ok(observation) => {
            assert!(observation.allocations > 0);
            assert_eq!(observation.content, "spec119-performance");
            println!("NXUSKIT_S1E_ALLOCATION_BOUNDARY=PASS");
        }
        Err(error) if error == "allocation window observed cross-thread work" => {
            println!("NXUSKIT_S1E_ALLOCATION_BOUNDARY=DIAGNOSTIC_UNAVAILABLE_CROSS_THREAD");
        }
        Err(error) => panic!("counted loopback C ABI chat: {error}"),
    }
}

#[test]
fn allocation_window_fails_closed_on_cross_thread_work() {
    let _serial = harness::serial_test_guard().expect("serial test guard");
    assert_eq!(
        harness::allocation_cross_thread_canary().unwrap_err(),
        "allocation window observed cross-thread work"
    );
}

#[test]
fn dynamic_load_resolves_and_calls_the_real_abi_symbol() {
    let _serial = harness::serial_test_guard().expect("serial test guard");
    let library = harness::locate_test_library().expect("test cdylib path");
    let observation = harness::measure_library_load_once(&library).expect("dynamic ABI load");
    assert!(observation.elapsed_ns > 0);
    assert!(!observation.abi_version.is_empty());
}

#[test]
fn receipt_persistence_is_create_only() {
    let _serial = harness::serial_test_guard().expect("serial test guard");
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("receipt.json");
    harness::persist_create_only(&path, b"first\n").expect("first create");
    let failure = harness::persist_create_only(&path, b"second\n").unwrap_err();
    assert!(failure.contains("already exists"));
    assert_eq!(fs::read(&path).unwrap(), b"first\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn closed_metric_shapes_distinguish_duration_and_allocation() {
    let _serial = harness::serial_test_guard().expect("serial test guard");
    let duration = DurationMetric::new(FFI_TARGET_NS, FFI_WARMUPS, FFI_SAMPLE_COUNT, 1, 2, 3);
    let duration_json = serde_json::to_value(duration).unwrap();
    assert_eq!(duration_json["evidence_role"], "blocking_acceptance");
    assert!(duration_json["allocation_count"].is_null());

    let allocation =
        harness::AllocationObservation::new(ALLOCATION_WARMUPS, ALLOCATION_SAMPLE_COUNT, 1, 2, 3);
    let allocation_json = serde_json::to_value(allocation).unwrap();
    assert_eq!(allocation_json["evidence_role"], "diagnostic_nonblocking");
    assert_eq!(allocation_json["allocation_count"], 2);
    for forbidden in ["target", "target_ns", "comparator", "passed"] {
        assert!(
            allocation_json.get(forbidden).is_none(),
            "forbidden field: {forbidden}"
        );
    }
}

#[test]
fn allocation_values_have_identical_diagnostic_authority() {
    let _serial = harness::serial_test_guard().expect("serial test guard");
    // Illustrative separated magnitudes, not thresholds or privileged policy
    // values: every valid observation has the same diagnostic-only authority.
    for observed in [32, 62, 620] {
        let value = serde_json::to_value(harness::AllocationObservation::new(
            ALLOCATION_WARMUPS,
            ALLOCATION_SAMPLE_COUNT,
            observed,
            observed,
            observed,
        ))
        .unwrap();
        assert_eq!(value["evidence_role"], "diagnostic_nonblocking");
        assert_eq!(value["unit"], "allocation_calls");
        assert_eq!(value["warmups"], ALLOCATION_WARMUPS);
        assert_eq!(value["sample_count"], ALLOCATION_SAMPLE_COUNT);
        assert_eq!(value["median"], observed);
        assert_eq!(value["p95"], observed);
        assert_eq!(value["max"], observed);
        assert_eq!(value["allocation_count"], observed);
        for forbidden in ["target", "target_ns", "comparator", "passed"] {
            assert!(
                value.get(forbidden).is_none(),
                "forbidden field: {forbidden}"
            );
        }
    }
}

#[test]
fn harness_source_contains_no_supplied_timing_or_release_count_override() {
    let _serial = harness::serial_test_guard().expect("serial test guard");
    let source = include_str!("support/provider_surface_harness.rs");
    for forbidden in [
        "NXUSKIT_PERF_SAMPLE_COUNT",
        "NXUSKIT_PERF_WARMUPS",
        "supplied_elapsed",
        "mock_timing",
    ] {
        assert!(
            !source.contains(forbidden),
            "forbidden harness escape: {forbidden}"
        );
    }
    for required in [
        "nxuskit_chat(",
        "nxuskit_chat_stream(",
        "Library::new(",
        "Instant::now(",
    ] {
        assert!(
            source.contains(required),
            "missing real boundary: {required}"
        );
    }
}

#[test]
fn benchmark_owner_exists_at_the_frozen_path() {
    let _serial = harness::serial_test_guard().expect("serial test guard");
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    assert!(manifest.join("benches/provider_surface.rs").is_file());
    let cargo = fs::read_to_string(manifest.join("Cargo.toml")).unwrap();
    assert!(cargo.contains("name = \"provider_surface\""));
    assert!(cargo.contains("harness = false"));
}

#[cfg(unix)]
#[test]
fn native_library_rejects_a_symlinked_target_component() {
    use std::os::unix::fs::symlink;

    let _serial = harness::serial_test_guard().expect("serial test guard");
    let directory = tempfile::tempdir().expect("tempdir");
    let proof = directory.path().join("proof");
    let receipts = proof.join("receipts");
    let outside_target = directory.path().join("outside-target");
    let outside_release = outside_target.join("release");
    fs::create_dir_all(&receipts).expect("proof receipts");
    fs::create_dir_all(&outside_release).expect("outside release");

    let (platform, library_name) = if cfg!(target_os = "macos") {
        ("macos_arm64", "libnxuskit_core.dylib")
    } else {
        ("linux_x86_64", "libnxuskit_core.so")
    };
    let outside_library = outside_release.join(library_name);
    fs::write(&outside_library, b"not-a-real-library").expect("outside library");
    symlink(&outside_target, proof.join("target")).expect("target symlink");

    let receipt = receipts.join(format!("{platform}.json"));
    let supplied_library = proof.join("target").join("release").join(library_name);
    let failure =
        harness::validate_native_library_path(&supplied_library, &receipt, platform).unwrap_err();
    assert!(failure.contains("symlink"), "unexpected failure: {failure}");
}

#[cfg(windows)]
#[test]
fn native_library_accepts_a_verbatim_disk_proof_root() {
    // `Path::canonicalize()` on Windows returns a `\\?\`-prefixed verbatim
    // path (via `GetFinalPathNameByHandleW`/`VOLUME_NAME_DOS`), which is
    // exactly the form the real S1E receipt path takes once the benchmark
    // canonicalizes its proof root. Before the fix, walking that verbatim
    // path's components and calling `fs::symlink_metadata` on the bare
    // `\\?\D:` prefix component (with no trailing separator) fails with
    // `ERROR_INVALID_FUNCTION` ("Incorrect function."), matching the
    // observed native benchmark failure byte-for-byte.
    let _serial = harness::serial_test_guard().expect("serial test guard");
    let directory = tempfile::tempdir().expect("tempdir");
    let physical = directory
        .path()
        .canonicalize()
        .expect("canonicalize tempdir to its verbatim form");
    assert!(
        physical.to_string_lossy().starts_with(r"\\?\"),
        "tempdir canonicalize did not produce a verbatim path: {}",
        physical.display()
    );

    let platform = "windows_x86_64";
    let library_name = "nxuskit_core.dll";
    let proof = physical.join("proof");
    let receipts = proof.join("receipts");
    let release = proof.join("target").join("release");
    fs::create_dir_all(&receipts).expect("proof receipts");
    fs::create_dir_all(&release).expect("proof release");
    let library_path = release.join(library_name);
    fs::write(&library_path, b"not-a-real-library").expect("library bytes");

    let receipt = receipts.join(format!("{platform}.json"));
    let resolved = harness::validate_native_library_path(&library_path, &receipt, platform)
        .unwrap_or_else(|error| panic!("verbatim disk proof root must validate cleanly: {error}"));
    assert_eq!(
        resolved.file_name().and_then(|name| name.to_str()),
        Some(library_name)
    );
}
