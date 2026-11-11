//! Shared implementation for the Spec 119 native provider-surface benchmark.

#![allow(dead_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::ffi::{CStr, CString, c_void};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::raw::c_char;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use libloading::{Library, Symbol};
use nxuskit_core::{
    nxuskit_cancel_stream, nxuskit_chat, nxuskit_chat_stream, nxuskit_create_provider,
    nxuskit_free_provider, nxuskit_free_response, nxuskit_free_stream, nxuskit_response_json,
};
use serde::Serialize;
use serde_json::{Value, json};

pub const FFI_TARGET_NS: u64 = 250_000;
pub const FFI_WARMUPS: usize = 50;
pub const FFI_SAMPLE_COUNT: usize = 1_000;
pub const LOAD_TARGET_NS: u64 = 50_000_000;
pub const LOAD_WARMUPS: usize = 10;
pub const LOAD_SAMPLE_COUNT: usize = 100;
pub const FIRST_CHUNK_TARGET_NS: u64 = 100_000_000;
pub const FIRST_CHUNK_WARMUPS: usize = 10;
pub const FIRST_CHUNK_SAMPLE_COUNT: usize = 100;
pub const ALLOCATION_WARMUPS: usize = 10;
pub const ALLOCATION_SAMPLE_COUNT: usize = 100;

const EXPECTED_CONTENT: &str = "spec119-performance";
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(5);

static TRACK_ALLOCATIONS: AtomicBool = AtomicBool::new(false);
static ALLOCATION_CALLS: AtomicU64 = AtomicU64::new(0);
static DEALLOCATION_CALLS: AtomicU64 = AtomicU64::new(0);
static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);
static DEALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);
static CROSS_THREAD_ALLOCATION: AtomicBool = AtomicBool::new(false);
static ALLOCATION_MEASUREMENT_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static SERIAL_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

thread_local! {
    static ALLOCATION_OWNER: Cell<bool> = const { Cell::new(false) };
}

struct CountingAllocator;

#[global_allocator]
static GLOBAL_ALLOCATOR: CountingAllocator = CountingAllocator;

fn record_allocation(size: usize) {
    if !TRACK_ALLOCATIONS.load(Ordering::Relaxed) {
        return;
    }
    ALLOCATION_OWNER.with(|owner| {
        if owner.get() {
            ALLOCATION_CALLS.fetch_add(1, Ordering::Relaxed);
            ALLOCATED_BYTES.fetch_add(size as u64, Ordering::Relaxed);
        } else {
            CROSS_THREAD_ALLOCATION.store(true, Ordering::Relaxed);
        }
    });
}

fn record_deallocation(size: usize) {
    if !TRACK_ALLOCATIONS.load(Ordering::Relaxed) {
        return;
    }
    ALLOCATION_OWNER.with(|owner| {
        if owner.get() {
            DEALLOCATION_CALLS.fetch_add(1, Ordering::Relaxed);
            DEALLOCATED_BYTES.fetch_add(size as u64, Ordering::Relaxed);
        } else {
            CROSS_THREAD_ALLOCATION.store(true, Ordering::Relaxed);
        }
    });
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            record_allocation(layout.size());
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            record_allocation(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        record_deallocation(layout.size());
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        let replacement = unsafe { System.realloc(pointer, old, new_size) };
        if !replacement.is_null() {
            record_deallocation(old.size());
            record_allocation(new_size);
        }
        replacement
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MetricKind {
    FfiCall,
    ProviderLoad,
    FirstChunk,
    Allocations,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Summary {
    pub median: u64,
    pub p95: u64,
    pub max: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DurationMetric {
    pub evidence_role: &'static str,
    pub unit: &'static str,
    pub target_ns: u64,
    pub comparator: &'static str,
    pub warmups: usize,
    pub sample_count: usize,
    pub median_ns: u64,
    pub p95_ns: u64,
    pub max_ns: u64,
    pub allocation_count: Option<u64>,
    pub passed: bool,
}

impl DurationMetric {
    pub fn new(
        target_ns: u64,
        warmups: usize,
        sample_count: usize,
        median_ns: u64,
        p95_ns: u64,
        max_ns: u64,
    ) -> Self {
        Self {
            evidence_role: "blocking_acceptance",
            unit: "nanoseconds",
            target_ns,
            comparator: "less_than_or_equal",
            warmups,
            sample_count,
            median_ns,
            p95_ns,
            max_ns,
            allocation_count: None,
            passed: p95_ns <= target_ns,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AllocationObservation {
    pub evidence_role: &'static str,
    pub unit: &'static str,
    pub warmups: usize,
    pub sample_count: usize,
    pub median: u64,
    pub p95: u64,
    pub max: u64,
    pub allocation_count: u64,
}

impl AllocationObservation {
    pub fn new(warmups: usize, sample_count: usize, median: u64, p95: u64, max: u64) -> Self {
        Self {
            evidence_role: "diagnostic_nonblocking",
            unit: "allocation_calls",
            warmups,
            sample_count,
            median,
            p95,
            max,
            allocation_count: p95,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DirectChatObservation {
    pub elapsed_ns: u64,
    pub content: String,
}

#[derive(Clone, Debug)]
pub struct FirstChunkObservation {
    pub elapsed_ns: u64,
    pub first_chunk_count: usize,
    pub delta: String,
}

#[derive(Clone, Debug)]
pub struct AllocationWindowObservation {
    pub allocations: u64,
    #[allow(dead_code)]
    pub deallocations: u64,
    #[allow(dead_code)]
    pub allocated_bytes: u64,
    #[allow(dead_code)]
    pub deallocated_bytes: u64,
    pub content: String,
}

#[derive(Clone, Debug)]
pub struct LibraryLoadObservation {
    pub elapsed_ns: u64,
    pub abi_version: String,
}

struct ProviderHandle(*mut c_void);

impl ProviderHandle {
    fn loopback() -> Result<Self, String> {
        let config = CString::new(r#"{"provider_type":"loopback"}"#)
            .map_err(|error| format!("loopback config: {error}"))?;
        let provider = unsafe { nxuskit_create_provider(config.as_ptr()) };
        if provider.is_null() {
            return Err("nxuskit_create_provider returned NULL".to_string());
        }
        Ok(Self(provider.cast()))
    }

    fn as_ffi(&self) -> *mut c_void {
        self.0
    }
}

impl Drop for ProviderHandle {
    fn drop(&mut self) {
        unsafe { nxuskit_free_provider(self.0.cast()) };
    }
}

struct ResponseHandle(*mut c_void);

impl Drop for ResponseHandle {
    fn drop(&mut self) {
        unsafe { nxuskit_free_response(self.0.cast()) };
    }
}

fn canonical_request() -> Value {
    json!({
        "model": "echo",
        "messages": [{"role": "user", "content": EXPECTED_CONTENT}]
    })
}

fn elapsed_ns(started: Instant, ended: Instant) -> Result<u64, String> {
    u64::try_from(ended.duration_since(started).as_nanos())
        .map_err(|_| "duration exceeds u64 nanoseconds".to_string())
}

fn direct_chat_sample(
    provider: &ProviderHandle,
    request: &Value,
) -> Result<DirectChatObservation, String> {
    let started = Instant::now();
    let request_json = serde_json::to_string(request)
        .map_err(|error| format!("serialize canonical request: {error}"))?;
    let request_c = CString::new(request_json)
        .map_err(|error| format!("canonical request contains NUL: {error}"))?;
    let response = unsafe { nxuskit_chat(provider.as_ffi().cast(), request_c.as_ptr()) };
    if response.is_null() {
        return Err("nxuskit_chat returned NULL".to_string());
    }
    let response = ResponseHandle(response.cast());
    let json_pointer = unsafe { nxuskit_response_json(response.0.cast()) };
    if json_pointer.is_null() {
        return Err("nxuskit_response_json returned NULL".to_string());
    }
    let response_text = unsafe { CStr::from_ptr(json_pointer) }
        .to_str()
        .map_err(|error| format!("response is not UTF-8: {error}"))?;
    let response_value: Value = serde_json::from_str(response_text)
        .map_err(|error| format!("response is not strict JSON: {error}"))?;
    let content = response_value
        .get("content")
        .and_then(Value::as_str)
        .ok_or_else(|| "response content is missing or not a string".to_string())?
        .to_string();
    if content != EXPECTED_CONTENT {
        return Err(format!("unexpected loopback content: {content:?}"));
    }
    drop(response);
    let ended = Instant::now();
    Ok(DirectChatObservation {
        elapsed_ns: elapsed_ns(started, ended)?,
        content,
    })
}

pub fn measure_direct_chat_once() -> Result<DirectChatObservation, String> {
    let provider = ProviderHandle::loopback()?;
    direct_chat_sample(&provider, &canonical_request())
}

struct FirstChunkState {
    sender: SyncSender<(Instant, String)>,
    count: AtomicUsize,
}

unsafe extern "C" fn first_chunk_callback(
    chunk_json: *const c_char,
    user_data: *mut c_void,
) -> i32 {
    if chunk_json.is_null() || user_data.is_null() {
        return 1;
    }
    let state = unsafe { &*(user_data.cast::<FirstChunkState>()) };
    let parsed = unsafe { CStr::from_ptr(chunk_json) }
        .to_str()
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(text).ok());
    let Some(delta) = parsed
        .as_ref()
        .and_then(|value| value.get("delta"))
        .and_then(Value::as_str)
        .filter(|delta| !delta.is_empty())
    else {
        return 0;
    };
    state.count.fetch_add(1, Ordering::SeqCst);
    let _ = state.sender.try_send((Instant::now(), delta.to_string()));
    1
}

unsafe extern "C" fn stream_done_callback(_final_json: *const c_char, _user_data: *mut c_void) {}

fn first_chunk_sample(
    provider: &ProviderHandle,
    request: &Value,
) -> Result<FirstChunkObservation, String> {
    let (sender, receiver) = sync_channel(1);
    let state = FirstChunkState {
        sender,
        count: AtomicUsize::new(0),
    };
    let started = Instant::now();
    let request_json = serde_json::to_string(request)
        .map_err(|error| format!("serialize streaming request: {error}"))?;
    let request_c = CString::new(request_json)
        .map_err(|error| format!("streaming request contains NUL: {error}"))?;
    let stream = unsafe {
        nxuskit_chat_stream(
            provider.as_ffi().cast(),
            request_c.as_ptr(),
            first_chunk_callback,
            stream_done_callback,
            (&state as *const FirstChunkState).cast_mut().cast(),
        )
    };
    if stream.is_null() {
        return Err("nxuskit_chat_stream returned NULL".to_string());
    }
    let received = receiver
        .recv_timeout(CALLBACK_TIMEOUT)
        .map_err(|error| format!("first chunk was not observed within five seconds: {error}"));
    unsafe {
        nxuskit_cancel_stream(stream);
        nxuskit_free_stream(stream);
    }
    let (observed_at, delta) = received?;
    let count = state.count.load(Ordering::SeqCst);
    if count != 1 {
        return Err(format!(
            "expected exactly one first chunk, observed {count}"
        ));
    }
    Ok(FirstChunkObservation {
        elapsed_ns: elapsed_ns(started, observed_at)?,
        first_chunk_count: count,
        delta,
    })
}

pub fn measure_first_chunk_once() -> Result<FirstChunkObservation, String> {
    let provider = ProviderHandle::loopback()?;
    first_chunk_sample(&provider, &canonical_request())
}

struct AllocationWindow;

impl AllocationWindow {
    fn start() -> Self {
        ALLOCATION_CALLS.store(0, Ordering::SeqCst);
        DEALLOCATION_CALLS.store(0, Ordering::SeqCst);
        ALLOCATED_BYTES.store(0, Ordering::SeqCst);
        DEALLOCATED_BYTES.store(0, Ordering::SeqCst);
        CROSS_THREAD_ALLOCATION.store(false, Ordering::SeqCst);
        ALLOCATION_OWNER.with(|owner| owner.set(true));
        TRACK_ALLOCATIONS.store(true, Ordering::SeqCst);
        Self
    }
}

impl Drop for AllocationWindow {
    fn drop(&mut self) {
        TRACK_ALLOCATIONS.store(false, Ordering::SeqCst);
        ALLOCATION_OWNER.with(|owner| owner.set(false));
    }
}

fn allocation_sample(
    provider: &ProviderHandle,
    request: &Value,
) -> Result<AllocationWindowObservation, String> {
    let lock = ALLOCATION_MEASUREMENT_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock
        .lock()
        .map_err(|_| "allocation measurement lock poisoned".to_string())?;
    let window = AllocationWindow::start();
    let observation = direct_chat_sample(provider, request);
    let allocations = ALLOCATION_CALLS.load(Ordering::SeqCst);
    let deallocations = DEALLOCATION_CALLS.load(Ordering::SeqCst);
    let allocated_bytes = ALLOCATED_BYTES.load(Ordering::SeqCst);
    let deallocated_bytes = DEALLOCATED_BYTES.load(Ordering::SeqCst);
    let cross_thread = CROSS_THREAD_ALLOCATION.load(Ordering::SeqCst);
    drop(window);
    if cross_thread {
        return Err("allocation window observed cross-thread work".to_string());
    }
    let observation = observation?;
    if allocations == 0 {
        return Err("allocation window observed zero allocation calls".to_string());
    }
    Ok(AllocationWindowObservation {
        allocations,
        deallocations,
        allocated_bytes,
        deallocated_bytes,
        content: observation.content,
    })
}

pub fn measure_allocations_once() -> Result<AllocationWindowObservation, String> {
    let provider = ProviderHandle::loopback()?;
    let request = canonical_request();
    direct_chat_sample(&provider, &request)?;
    allocation_sample(&provider, &request)
}

pub fn allocation_cross_thread_canary() -> Result<(), String> {
    let lock = ALLOCATION_MEASUREMENT_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock
        .lock()
        .map_err(|_| "allocation measurement lock poisoned".to_string())?;
    let window = AllocationWindow::start();
    std::thread::spawn(|| {
        let allocation = Box::new([0_u8; 128]);
        std::hint::black_box(allocation);
    })
    .join()
    .map_err(|_| "cross-thread allocation canary panicked".to_string())?;
    let cross_thread = CROSS_THREAD_ALLOCATION.load(Ordering::SeqCst);
    drop(window);
    if cross_thread {
        Err("allocation window observed cross-thread work".to_string())
    } else {
        Err("cross-thread allocation canary was not observed".to_string())
    }
}

fn validate_library_path(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("library path must be absolute".to_string());
    }
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("library metadata {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("library must be a regular nonsymlink file".to_string());
    }
    path.canonicalize()
        .map_err(|error| format!("canonicalize library {}: {error}", path.display()))
}

fn reject_symlink_components(path: &Path, label: &str) -> Result<(), String> {
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
            .map_err(|error| format!("{label} component {}: {error}", current.display()))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "{label} component is a symlink: {}",
                current.display()
            ));
        }
    }
    Ok(())
}

fn expected_native_library_name(platform: &str) -> Result<&'static str, String> {
    match platform {
        "macos_arm64" => Ok("libnxuskit_core.dylib"),
        "linux_x86_64" => Ok("libnxuskit_core.so"),
        "windows_x86_64" => Ok("nxuskit_core.dll"),
        _ => Err("unsupported native performance platform".to_string()),
    }
}

pub fn validate_native_library_path(
    input: &Path,
    receipt_path: &Path,
    platform: &str,
) -> Result<PathBuf, String> {
    let expected_name = expected_native_library_name(platform)?;
    if !input.is_absolute() {
        return Err("native library path must be absolute".to_string());
    }
    if input.file_name().and_then(|name| name.to_str()) != Some(expected_name) {
        return Err("native library filename does not match the platform".to_string());
    }
    let proof_root = receipt_path
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| "receipt path does not identify a proof root".to_string())?;
    reject_symlink_components(proof_root, "proof root")?;
    let physical_proof_root = proof_root
        .canonicalize()
        .map_err(|error| format!("canonicalize proof root {}: {error}", proof_root.display()))?;
    let expected_input = proof_root
        .join("target")
        .join("release")
        .join(expected_name);
    reject_symlink_components(input, "native library path")?;
    reject_symlink_components(&expected_input, "native library path")?;
    let metadata = fs::symlink_metadata(input)
        .map_err(|error| format!("native library metadata {}: {error}", input.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("native library must be a regular nonsymlink file".to_string());
    }
    let physical = input
        .canonicalize()
        .map_err(|error| format!("canonicalize native library {}: {error}", input.display()))?;
    let expected = expected_input
        .canonicalize()
        .map_err(|error| format!("canonicalize expected native library: {error}"))?;
    if !expected.starts_with(&physical_proof_root) || physical != expected {
        return Err("native library must be the exact proof-owned release artifact".to_string());
    }
    Ok(physical)
}

pub fn measure_library_load_once(path: &Path) -> Result<LibraryLoadObservation, String> {
    let physical = validate_library_path(path)?;
    let started = Instant::now();
    let library = unsafe { Library::new(&physical) }
        .map_err(|error| format!("load {}: {error}", physical.display()))?;
    let abi_version = {
        let symbol: Symbol<'_, unsafe extern "C" fn() -> *const c_char> = unsafe {
            library
                .get(b"nxuskit_abi_version\0")
                .map_err(|error| format!("resolve nxuskit_abi_version: {error}"))?
        };
        let pointer = unsafe { symbol() };
        if pointer.is_null() {
            return Err("nxuskit_abi_version returned NULL".to_string());
        }
        unsafe { CStr::from_ptr(pointer) }
            .to_str()
            .map_err(|error| format!("ABI version is not UTF-8: {error}"))?
            .to_string()
    };
    if abi_version.is_empty() {
        return Err("ABI version is empty".to_string());
    }
    drop(library);
    let ended = Instant::now();
    Ok(LibraryLoadObservation {
        elapsed_ns: elapsed_ns(started, ended)?,
        abi_version,
    })
}

pub fn locate_test_library() -> Result<PathBuf, String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("resolve current test executable: {error}"))?;
    let dependencies = executable
        .parent()
        .ok_or_else(|| "test executable has no parent".to_string())?;
    #[cfg(target_os = "macos")]
    let filename = "libnxuskit_core.dylib";
    #[cfg(target_os = "linux")]
    let filename = "libnxuskit_core.so";
    #[cfg(target_os = "windows")]
    let filename = "nxuskit_core.dll";
    for directory in [Some(dependencies), dependencies.parent()]
        .into_iter()
        .flatten()
    {
        let candidate = directory.join(filename);
        if candidate.is_file() && !candidate.is_symlink() {
            return validate_library_path(&candidate);
        }
    }
    Err(format!(
        "test cdylib {filename} was not found next to the test executable"
    ))
}

pub fn release_policy(kind: MetricKind) -> (usize, usize) {
    match kind {
        MetricKind::FfiCall => (FFI_WARMUPS, FFI_SAMPLE_COUNT),
        MetricKind::ProviderLoad => (LOAD_WARMUPS, LOAD_SAMPLE_COUNT),
        MetricKind::FirstChunk => (FIRST_CHUNK_WARMUPS, FIRST_CHUNK_SAMPLE_COUNT),
        MetricKind::Allocations => (ALLOCATION_WARMUPS, ALLOCATION_SAMPLE_COUNT),
    }
}

pub fn passes_threshold(kind: MetricKind, value: u64) -> bool {
    let target = match kind {
        MetricKind::FfiCall => FFI_TARGET_NS,
        MetricKind::ProviderLoad => LOAD_TARGET_NS,
        MetricKind::FirstChunk => FIRST_CHUNK_TARGET_NS,
        MetricKind::Allocations => return false,
    };
    value <= target
}

pub fn summarize(values: &mut [u64]) -> Result<Summary, String> {
    if values.is_empty() {
        return Err("sample vector must not be empty".to_string());
    }
    values.sort_unstable();
    let nearest_rank = |numerator: usize, denominator: usize| -> usize {
        let rank = values.len().saturating_mul(numerator).div_ceil(denominator);
        rank.saturating_sub(1).min(values.len() - 1)
    };
    Ok(Summary {
        median: values[nearest_rank(1, 2)],
        p95: values[nearest_rank(95, 100)],
        max: *values.last().expect("nonempty samples"),
    })
}

#[allow(dead_code)]
pub fn collect_duration_metric(
    kind: MetricKind,
    library_path: Option<&Path>,
) -> Result<DurationMetric, String> {
    let (warmups, sample_count) = release_policy(kind);
    let target = match kind {
        MetricKind::FfiCall => FFI_TARGET_NS,
        MetricKind::ProviderLoad => LOAD_TARGET_NS,
        MetricKind::FirstChunk => FIRST_CHUNK_TARGET_NS,
        MetricKind::Allocations => return Err("allocation metric is not a duration".to_string()),
    };
    let provider = match kind {
        MetricKind::FfiCall | MetricKind::FirstChunk => Some(ProviderHandle::loopback()?),
        MetricKind::ProviderLoad => None,
        MetricKind::Allocations => unreachable!(),
    };
    let request = canonical_request();
    let sample = || -> Result<u64, String> {
        match kind {
            MetricKind::FfiCall => {
                direct_chat_sample(provider.as_ref().expect("provider"), &request)
                    .map(|observation| observation.elapsed_ns)
            }
            MetricKind::ProviderLoad => measure_library_load_once(
                library_path.ok_or_else(|| "library path is required".to_string())?,
            )
            .map(|observation| observation.elapsed_ns),
            MetricKind::FirstChunk => {
                first_chunk_sample(provider.as_ref().expect("provider"), &request)
                    .map(|observation| observation.elapsed_ns)
            }
            MetricKind::Allocations => unreachable!(),
        }
    };
    for _ in 0..warmups {
        sample()?;
    }
    let mut values = Vec::with_capacity(sample_count);
    for _ in 0..sample_count {
        values.push(sample()?);
    }
    let summary = summarize(&mut values)?;
    Ok(DurationMetric::new(
        target,
        warmups,
        sample_count,
        summary.median,
        summary.p95,
        summary.max,
    ))
}

#[allow(dead_code)]
pub fn collect_allocation_observation() -> Result<AllocationObservation, String> {
    let provider = ProviderHandle::loopback()?;
    let request = canonical_request();
    direct_chat_sample(&provider, &request)?;
    for _ in 0..ALLOCATION_WARMUPS {
        allocation_sample(&provider, &request)?;
    }
    let mut values = Vec::with_capacity(ALLOCATION_SAMPLE_COUNT);
    for _ in 0..ALLOCATION_SAMPLE_COUNT {
        values.push(allocation_sample(&provider, &request)?.allocations);
    }
    let summary = summarize(&mut values)?;
    Ok(AllocationObservation::new(
        ALLOCATION_WARMUPS,
        ALLOCATION_SAMPLE_COUNT,
        summary.median,
        summary.p95,
        summary.max,
    ))
}

pub fn persist_create_only(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("receipt path must be absolute".to_string());
    }
    let parent = path
        .parent()
        .ok_or_else(|| "receipt path has no parent".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("create receipt directory {}: {error}", parent.display()))?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            "receipt already exists".to_string()
        } else {
            format!("create receipt {}: {error}", path.display())
        }
    })?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("persist receipt {}: {error}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("set receipt mode {}: {error}", path.display()))?;
    }
    Ok(())
}

pub fn serial_test_guard() -> Result<std::sync::MutexGuard<'static, ()>, String> {
    Ok(SERIAL_TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner))
}
