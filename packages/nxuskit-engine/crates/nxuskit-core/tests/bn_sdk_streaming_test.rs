//! C ABI streaming test — nxuskit_bn_infer_stream callback verification.
//!
//! T044: Tests callback-based streaming inference through the C ABI.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

// Force linkage of nxuskit-core symbols.
use nxuskit_core as _;

// ── Opaque types (matches bn_sdk.rs) ────────────────────────────

#[derive(Debug)]
#[repr(C)]
pub struct NxuskitBnNet {
    _opaque: [u8; 0],
}

#[derive(Debug)]
#[repr(C)]
pub struct NxuskitBnEvidence {
    _opaque: [u8; 0],
}

// ── Callback type ───────────────────────────────────────────────

type NxuskitBnStreamCallback = unsafe extern "C" fn(
    chunk_json: *const c_char,
    iteration: u32,
    total: u32,
    is_final: bool,
    user_data: *mut std::ffi::c_void,
) -> bool;

// ── FFI declarations ────────────────────────────────────────────

unsafe extern "C" {
    fn nxuskit_bn_net_destroy(net: *mut NxuskitBnNet);
    fn nxuskit_bn_net_load_file(path: *const c_char) -> *mut NxuskitBnNet;
    fn nxuskit_bn_ev_create() -> *mut NxuskitBnEvidence;
    fn nxuskit_bn_ev_destroy(ev: *mut NxuskitBnEvidence);
    fn nxuskit_bn_ev_set_discrete(
        ev: *mut NxuskitBnEvidence,
        net: *const NxuskitBnNet,
        variable: *const c_char,
        state: *const c_char,
    ) -> bool;
    fn nxuskit_bn_infer_stream(
        net: *const NxuskitBnNet,
        ev: *const NxuskitBnEvidence,
        num_samples: u32,
        burn_in: u32,
        seed: u64,
        chunk_size: u32,
        on_chunk: NxuskitBnStreamCallback,
        user_data: *mut std::ffi::c_void,
    ) -> bool;
}

// ── Helper ──────────────────────────────────────────────────────

fn c(s: &str) -> CString {
    CString::new(s).unwrap()
}

fn fixture_path(name: &str) -> CString {
    let path: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "..",
        "nxuskit-engine",
        "tests",
        "fixtures",
        "bn",
        name,
    ]
    .iter()
    .collect();
    c(path.to_str().unwrap())
}

/// Load the Asia network via C ABI. Panics on failure.
unsafe fn load_asia_net() -> *mut NxuskitBnNet {
    let path = fixture_path("asia.bif");
    let net = unsafe { nxuskit_bn_net_load_file(path.as_ptr()) };
    assert!(!net.is_null(), "Failed to load asia.bif");
    net
}

/// Callback state for collecting streaming results.
struct StreamState {
    chunk_count: usize,
    iterations: Vec<u32>,
    totals: Vec<u32>,
    had_final: bool,
    json_valid: bool,
}

/// Simple callback that records chunk metadata.
unsafe extern "C" fn collect_callback(
    chunk_json: *const c_char,
    iteration: u32,
    total: u32,
    is_final: bool,
    user_data: *mut std::ffi::c_void,
) -> bool {
    let state = unsafe { &mut *(user_data as *mut StreamState) };
    state.chunk_count += 1;
    state.iterations.push(iteration);
    state.totals.push(total);
    if is_final {
        state.had_final = true;
    }

    // Validate JSON is parseable
    let json_str = unsafe { CStr::from_ptr(chunk_json) }.to_str().unwrap_or("");
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(json_str);
    if parsed.is_err() {
        state.json_valid = false;
    }

    true // continue streaming
}

/// Callback that cancels after N chunks.
unsafe extern "C" fn cancel_after_n_callback(
    _chunk_json: *const c_char,
    _iteration: u32,
    _total: u32,
    _is_final: bool,
    user_data: *mut std::ffi::c_void,
) -> bool {
    let count = unsafe { &*(user_data as *mut AtomicUsize) };
    let current = count.fetch_add(1, Ordering::SeqCst);
    current < 2 // cancel after 3rd chunk (0, 1, 2 → false on 2)
}

// ── Tests ───────────────────────────────────────────────────────

#[test]
fn stream_delivers_chunks() {
    let mut state = StreamState {
        chunk_count: 0,
        iterations: Vec::new(),
        totals: Vec::new(),
        had_final: false,
        json_valid: true,
    };

    unsafe {
        let net = load_asia_net();
        let ev = nxuskit_bn_ev_create();

        let result = nxuskit_bn_infer_stream(
            net,
            ev,
            5_000, // num_samples
            100,   // burn_in
            42,    // seed
            1_000, // chunk_size
            collect_callback,
            &mut state as *mut StreamState as *mut std::ffi::c_void,
        );

        assert!(result, "nxuskit_bn_infer_stream should succeed");
        assert!(
            state.chunk_count >= 5,
            "Expected ≥5 chunks, got {}",
            state.chunk_count
        );
        assert!(state.had_final, "Should have received a final chunk");
        assert!(state.json_valid, "All chunk JSON should be valid");

        nxuskit_bn_ev_destroy(ev);
        nxuskit_bn_net_destroy(net);
    }
}

#[test]
fn stream_iterations_increase() {
    let mut state = StreamState {
        chunk_count: 0,
        iterations: Vec::new(),
        totals: Vec::new(),
        had_final: false,
        json_valid: true,
    };

    unsafe {
        let net = load_asia_net();
        let ev = nxuskit_bn_ev_create();

        nxuskit_bn_infer_stream(
            net,
            ev,
            3_000,
            100,
            42,
            500,
            collect_callback,
            &mut state as *mut StreamState as *mut std::ffi::c_void,
        );

        // Iteration counts should be strictly increasing
        for window in state.iterations.windows(2) {
            assert!(
                window[1] > window[0],
                "Iterations should increase: {} -> {}",
                window[0],
                window[1]
            );
        }

        // All totals should be the same (3000)
        for t in &state.totals {
            assert_eq!(*t, 3_000);
        }

        nxuskit_bn_ev_destroy(ev);
        nxuskit_bn_net_destroy(net);
    }
}

#[test]
fn stream_chunk_json_format() {
    /// State to capture the first chunk's JSON.
    struct CaptureState {
        first_json: Option<String>,
    }

    unsafe extern "C" fn capture_first_json(
        chunk_json: *const c_char,
        _iteration: u32,
        _total: u32,
        _is_final: bool,
        user_data: *mut std::ffi::c_void,
    ) -> bool {
        let state = unsafe { &mut *(user_data as *mut CaptureState) };
        if state.first_json.is_none() {
            let json_str = unsafe { CStr::from_ptr(chunk_json) }.to_str().unwrap();
            state.first_json = Some(json_str.to_string());
        }
        true
    }

    let mut capture = CaptureState { first_json: None };

    unsafe {
        let net = load_asia_net();
        let ev = nxuskit_bn_ev_create();

        nxuskit_bn_infer_stream(
            net,
            ev,
            2_000,
            100,
            42,
            500,
            capture_first_json,
            &mut capture as *mut CaptureState as *mut std::ffi::c_void,
        );

        let json_str = capture.first_json.as_ref().unwrap();
        let v: serde_json::Value = serde_json::from_str(json_str).unwrap();

        // Required fields in chunk JSON
        assert!(v.get("marginals").is_some(), "missing marginals");
        assert!(v.get("algorithm").is_some(), "missing algorithm");
        assert!(v.get("elapsed_ms").is_some(), "missing elapsed_ms");
        assert!(v.get("iteration").is_some(), "missing iteration");
        assert!(
            v.get("total_iterations").is_some(),
            "missing total_iterations"
        );
        assert!(
            v.get("convergence_metric").is_some(),
            "missing convergence_metric"
        );
        assert!(v.get("is_final").is_some(), "missing is_final");

        assert_eq!(v["algorithm"], "gibbs");
        assert_eq!(v["total_iterations"], 2_000);

        nxuskit_bn_ev_destroy(ev);
        nxuskit_bn_net_destroy(net);
    }
}

#[test]
fn stream_cancellation_via_callback() {
    let count = AtomicUsize::new(0);

    unsafe {
        let net = load_asia_net();
        let ev = nxuskit_bn_ev_create();

        // Request 100K samples but cancel after 3 chunks
        let result = nxuskit_bn_infer_stream(
            net,
            ev,
            100_000, // very large
            100,
            42,
            1_000,
            cancel_after_n_callback,
            &count as *const AtomicUsize as *mut std::ffi::c_void,
        );

        assert!(result, "Cancelled stream should still return true");
        let final_count = count.load(Ordering::SeqCst);
        assert_eq!(
            final_count, 3,
            "Should have received exactly 3 chunks before cancel"
        );

        nxuskit_bn_ev_destroy(ev);
        nxuskit_bn_net_destroy(net);
    }
}

#[test]
fn stream_with_evidence() {
    let mut state = StreamState {
        chunk_count: 0,
        iterations: Vec::new(),
        totals: Vec::new(),
        had_final: false,
        json_valid: true,
    };

    unsafe {
        let net = load_asia_net();
        let ev = nxuskit_bn_ev_create();

        let smoking = c("Smoking");
        let yes = c("yes");
        nxuskit_bn_ev_set_discrete(ev, net, smoking.as_ptr(), yes.as_ptr());

        let result = nxuskit_bn_infer_stream(
            net,
            ev,
            3_000,
            100,
            42,
            500,
            collect_callback,
            &mut state as *mut StreamState as *mut std::ffi::c_void,
        );

        assert!(result);
        assert!(
            state.chunk_count >= 6,
            "Expected ≥6 chunks, got {}",
            state.chunk_count
        );
        assert!(state.had_final);

        nxuskit_bn_ev_destroy(ev);
        nxuskit_bn_net_destroy(net);
    }
}

#[test]
fn stream_null_net_returns_false() {
    let mut state = StreamState {
        chunk_count: 0,
        iterations: Vec::new(),
        totals: Vec::new(),
        had_final: false,
        json_valid: true,
    };

    unsafe {
        let result = nxuskit_bn_infer_stream(
            std::ptr::null(),
            std::ptr::null(),
            1_000,
            100,
            42,
            500,
            collect_callback,
            &mut state as *mut StreamState as *mut std::ffi::c_void,
        );

        assert!(!result, "NULL net should fail");
        assert_eq!(state.chunk_count, 0, "No chunks should have been delivered");
    }
}
