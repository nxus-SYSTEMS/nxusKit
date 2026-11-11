//! Spec 119 native provider-surface performance benchmark and receipt writer.

#![allow(clippy::print_stderr, clippy::print_stdout)]

#[path = "../tests/support/provider_surface_harness.rs"]
mod harness;

use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use harness::{AllocationObservation, DurationMetric, MetricKind};

const SCHEMA_VERSION: &str = "nxus-spec119-provider-performance/v2";

#[derive(Serialize)]
struct ToolIdentity {
    cargo: String,
    rustc: String,
    target_triple: String,
    profile: &'static str,
}

#[derive(Serialize)]
struct HardwareIdentity {
    os_version: String,
    architecture: &'static str,
    logical_cpu_count: usize,
    runner_identity: String,
}

#[derive(Serialize)]
struct SamplePolicy {
    clock: &'static str,
    serial: bool,
    percentile: &'static str,
    warmups_excluded: bool,
    raw_duration_unit: &'static str,
}

#[derive(Serialize)]
struct NativeReceipt {
    schema_version: &'static str,
    candidate: String,
    platform: String,
    benchmark_source_sha256: String,
    cargo_lock_sha256: String,
    tool_identity: ToolIdentity,
    hardware: HardwareIdentity,
    library_path: String,
    library_sha256: String,
    sample_policy: SamplePolicy,
    metrics: BTreeMap<&'static str, Value>,
    executed_assertions: u64,
    passed_assertions: u64,
    failed_assertions: u64,
    diagnostic_observations: u64,
}

fn main() -> ExitCode {
    match run() {
        Ok(true) => {
            println!("NXUSKIT_PROVIDER_SURFACE_ASSERTIONS=3");
            ExitCode::SUCCESS
        }
        Ok(false) => {
            eprintln!("ERROR: one or more native performance targets were missed");
            ExitCode::from(2)
        }
        Err(error) => {
            eprintln!("ERROR: {error}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<bool, String> {
    let candidate = required_environment("NXUSKIT_PERF_EXPECTED_CANDIDATE")?;
    if candidate.len() != 40
        || !candidate
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("NXUSKIT_PERF_EXPECTED_CANDIDATE must be lowercase 40-hex".to_string());
    }

    let platform = required_environment("NXUSKIT_PERF_PLATFORM")?;
    validate_native_platform(&platform)?;
    let architecture = receipt_architecture(&platform);
    let runner_identity = required_environment("NXUSKIT_PERF_HARDWARE_ID")?;
    let library_input = PathBuf::from(required_environment("NXUSKIT_PERF_LIBRARY_PATH")?);
    let receipt_input = PathBuf::from(required_environment("NXUSKIT_PERF_RECEIPT_PATH")?);
    let receipt_path = validate_receipt_path(&receipt_input, &platform)?;
    let library_path =
        harness::validate_native_library_path(&library_input, &receipt_path, &platform)?;

    let ffi: DurationMetric = harness::collect_duration_metric(MetricKind::FfiCall, None)?;
    let load: DurationMetric =
        harness::collect_duration_metric(MetricKind::ProviderLoad, Some(&library_path))?;
    let first_chunk: DurationMetric =
        harness::collect_duration_metric(MetricKind::FirstChunk, None)?;
    let allocations: AllocationObservation = harness::collect_allocation_observation()?;

    let mut metrics = BTreeMap::new();
    metrics.insert(
        "ffi_call_p95_us",
        serde_json::to_value(&ffi).map_err(|error| format!("serialize FFI metric: {error}"))?,
    );
    metrics.insert(
        "provider_load_p95_ms",
        serde_json::to_value(&load).map_err(|error| format!("serialize load metric: {error}"))?,
    );
    metrics.insert(
        "first_chunk_p95_ms",
        serde_json::to_value(&first_chunk)
            .map_err(|error| format!("serialize first-chunk metric: {error}"))?,
    );
    metrics.insert(
        "allocations_per_chat",
        serde_json::to_value(&allocations)
            .map_err(|error| format!("serialize allocation metric: {error}"))?,
    );

    let passed = [ffi.passed, load.passed, first_chunk.passed]
        .into_iter()
        .filter(|value| *value)
        .count() as u64;
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cargo_lock = locate_workspace_lock(&manifest)?;
    let receipt = NativeReceipt {
        schema_version: SCHEMA_VERSION,
        candidate,
        platform,
        benchmark_source_sha256: sha256_bytes(include_bytes!("provider_surface.rs")),
        cargo_lock_sha256: sha256_file(&cargo_lock)?,
        tool_identity: tool_identity()?,
        hardware: HardwareIdentity {
            os_version: os_version()?,
            architecture,
            logical_cpu_count: std::thread::available_parallelism()
                .map_err(|error| format!("logical CPU count: {error}"))?
                .get(),
            runner_identity,
        },
        library_path: library_path
            .to_str()
            .ok_or_else(|| "library path is not Unicode".to_string())?
            .to_string(),
        library_sha256: sha256_file(&library_path)?,
        sample_policy: SamplePolicy {
            clock: "std::time::Instant",
            serial: true,
            percentile: "nearest_rank",
            warmups_excluded: true,
            raw_duration_unit: "nanoseconds",
        },
        metrics,
        executed_assertions: 3,
        passed_assertions: passed,
        failed_assertions: 3 - passed,
        diagnostic_observations: 1,
    };
    let mut bytes = serde_json::to_vec_pretty(&receipt)
        .map_err(|error| format!("serialize native receipt: {error}"))?;
    bytes.push(b'\n');
    harness::persist_create_only(&receipt_path, &bytes)?;
    Ok(passed == 3)
}

fn required_environment(name: &str) -> Result<String, String> {
    env::var(name).map_err(|_| format!("required environment variable is missing: {name}"))
}

fn validate_native_platform(platform: &str) -> Result<(), String> {
    let expected = match (env::consts::OS, env::consts::ARCH) {
        ("macos", "aarch64") => "macos_arm64",
        ("linux", "x86_64") => "linux_x86_64",
        ("windows", "x86_64") => "windows_x86_64",
        (os, architecture) => {
            return Err(format!("unsupported native platform: {os}/{architecture}"));
        }
    };
    if platform != expected {
        return Err(format!(
            "NXUSKIT_PERF_PLATFORM mismatch: expected {expected}, received {platform}"
        ));
    }
    Ok(())
}

fn receipt_architecture(platform: &str) -> &'static str {
    match platform {
        "macos_arm64" => "arm64",
        "linux_x86_64" | "windows_x86_64" => "x86_64",
        _ => unreachable!("validated platform"),
    }
}

fn validate_receipt_path(input: &Path, platform: &str) -> Result<PathBuf, String> {
    if !input.is_absolute() {
        return Err("NXUSKIT_PERF_RECEIPT_PATH must be absolute".to_string());
    }
    let expected_name = format!("{platform}.json");
    if input.file_name().and_then(|name| name.to_str()) != Some(expected_name.as_str()) {
        return Err(format!("receipt filename must be {expected_name}"));
    }
    let parent = input
        .parent()
        .ok_or_else(|| "receipt path has no parent".to_string())?;
    if parent.file_name().and_then(|name| name.to_str()) != Some("receipts") {
        return Err("receipt parent directory must be named receipts".to_string());
    }
    let parent_metadata = fs::symlink_metadata(parent)
        .map_err(|error| format!("receipt parent metadata {}: {error}", parent.display()))?;
    if parent_metadata.file_type().is_symlink() || !parent_metadata.is_dir() {
        return Err("receipt parent must be a pre-existing nonsymlink directory".to_string());
    }
    reject_symlink_components(parent)?;
    let physical_parent = parent
        .canonicalize()
        .map_err(|error| format!("canonicalize receipt parent {}: {error}", parent.display()))?;
    let proof_root = physical_parent
        .parent()
        .ok_or_else(|| "receipt path does not identify a proof root".to_string())?;
    let proof_metadata = fs::symlink_metadata(proof_root)
        .map_err(|error| format!("proof root metadata {}: {error}", proof_root.display()))?;
    if proof_metadata.file_type().is_symlink() || !proof_metadata.is_dir() {
        return Err("proof root must be a pre-existing nonsymlink directory".to_string());
    }
    let physical = physical_parent.join(&expected_name);
    #[cfg(not(windows))]
    if physical != input {
        return Err("receipt path must already be canonical and nonsymlinked".to_string());
    }
    Ok(physical)
}

fn reject_symlink_components(path: &Path) -> Result<(), String> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        if matches!(component, std::path::Component::Prefix(_)) {
            // A bare Windows path prefix (e.g. `\\?\D:`) is not a
            // syntactically valid filesystem path on its own; it must be
            // combined with the following root separator before querying
            // metadata, or Windows returns ERROR_INVALID_FUNCTION.
            continue;
        }
        let metadata = fs::symlink_metadata(&current)
            .map_err(|error| format!("receipt path component {}: {error}", current.display()))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "receipt path component is a symlink: {}",
                current.display()
            ));
        }
    }
    Ok(())
}

fn locate_workspace_lock(manifest: &Path) -> Result<PathBuf, String> {
    for ancestor in manifest.ancestors() {
        let candidate = ancestor.join("Cargo.lock");
        if candidate.exists() {
            let metadata = fs::symlink_metadata(&candidate)
                .map_err(|error| format!("Cargo.lock metadata: {error}"))?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err("workspace Cargo.lock must be a regular nonsymlink file".to_string());
            }
            return candidate
                .canonicalize()
                .map_err(|error| format!("canonicalize Cargo.lock: {error}"));
        }
    }
    Err("workspace Cargo.lock was not found".to_string())
}

fn command_output(program: OsString, arguments: &[&str]) -> Result<String, String> {
    let output = Command::new(&program)
        .args(arguments)
        .output()
        .map_err(|error| format!("run {}: {error}", PathBuf::from(&program).display()))?;
    if !output.status.success() {
        return Err(format!(
            "{} exited with {}",
            PathBuf::from(&program).display(),
            output.status
        ));
    }
    String::from_utf8(output.stdout)
        .map(|value| value.trim_end().to_string())
        .map_err(|error| format!("tool output is not UTF-8: {error}"))
}

fn tool_identity() -> Result<ToolIdentity, String> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let rustc = env::var_os("RUSTC").unwrap_or_else(|| OsString::from("rustc"));
    let cargo_output = command_output(cargo, &["-V"])?;
    let rustc_output = command_output(rustc, &["-Vv"])?;
    let target_triple = rustc_output
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .ok_or_else(|| "rustc -Vv did not report a host target".to_string())?
        .to_string();
    Ok(ToolIdentity {
        cargo: cargo_output,
        rustc: rustc_output,
        target_triple,
        profile: "bench",
    })
}

fn os_version() -> Result<String, String> {
    #[cfg(target_os = "macos")]
    let result = command_output(OsString::from("/usr/bin/sw_vers"), &["-productVersion"]);
    #[cfg(target_os = "linux")]
    let result = command_output(OsString::from("/usr/bin/uname"), &["-srv"]);
    #[cfg(target_os = "windows")]
    let result = command_output(OsString::from("cmd.exe"), &["/C", "ver"]);
    result
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    Ok(sha256_bytes(&bytes))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
