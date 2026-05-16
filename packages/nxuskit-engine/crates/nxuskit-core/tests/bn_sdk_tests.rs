//! Integration tests for the Bayesian Network C ABI.
//!
//! These tests exercise the `nxuskit_bn_*` functions through their C ABI
//! interface, verifying network lifecycle, evidence management, inference,
//! result access, and error handling.

use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::PathBuf;

// Import nxuskit_core so Cargo links the library.
use nxuskit_core as _;

// ── FFI declarations for BN SDK ──────────────────────────────────────

// Opaque types — we only deal with pointers.
#[repr(C)]
struct NxuskitBnNet {
    _opaque: [u8; 0],
}
#[repr(C)]
struct NxuskitBnEvidence {
    _opaque: [u8; 0],
}
#[repr(C)]
struct NxuskitBnResult {
    _opaque: [u8; 0],
}

unsafe extern "C" {
    // Network lifecycle
    fn nxuskit_bn_net_create() -> *mut NxuskitBnNet;
    fn nxuskit_bn_net_destroy(net: *mut NxuskitBnNet);
    fn nxuskit_bn_net_load_file(path: *const c_char) -> *mut NxuskitBnNet;
    fn nxuskit_bn_net_num_variables(net: *const NxuskitBnNet) -> i32;
    fn nxuskit_bn_net_variables(net: *const NxuskitBnNet) -> *mut c_char;
    fn nxuskit_bn_net_variable_states(
        net: *const NxuskitBnNet,
        variable: *const c_char,
    ) -> *mut c_char;

    // Evidence
    fn nxuskit_bn_ev_create() -> *mut NxuskitBnEvidence;
    fn nxuskit_bn_ev_destroy(ev: *mut NxuskitBnEvidence);
    fn nxuskit_bn_ev_set_discrete(
        ev: *mut NxuskitBnEvidence,
        net: *const NxuskitBnNet,
        variable: *const c_char,
        state: *const c_char,
    ) -> bool;
    fn nxuskit_bn_ev_retract(ev: *mut NxuskitBnEvidence, variable: *const c_char) -> bool;
    fn nxuskit_bn_ev_clear(ev: *mut NxuskitBnEvidence) -> bool;

    // Inference
    fn nxuskit_bn_infer(
        net: *const NxuskitBnNet,
        ev: *const NxuskitBnEvidence,
        algorithm: *const c_char,
        num_samples: u32,
        burn_in: u32,
        seed: u64,
    ) -> *mut NxuskitBnResult;

    // Result access
    fn nxuskit_bn_result_destroy(result: *mut NxuskitBnResult);
    fn nxuskit_bn_result_json(result: *const NxuskitBnResult) -> *mut c_char;
    fn nxuskit_bn_result_query(
        result: *const NxuskitBnResult,
        variable: *const c_char,
    ) -> *mut c_char;
    fn nxuskit_bn_result_num_variables(result: *const NxuskitBnResult) -> i32;
    fn nxuskit_bn_result_next(result: *mut NxuskitBnResult) -> *mut c_char;
    fn nxuskit_bn_result_reset(result: *mut NxuskitBnResult);

    // Parameter learning
    fn nxuskit_bn_learn_mle(
        net: *mut NxuskitBnNet,
        csv_path: *const c_char,
        pseudocount: f64,
    ) -> bool;
    fn nxuskit_bn_log_likelihood(net: *const NxuskitBnNet, csv_path: *const c_char) -> f64;

    // Structure learning
    fn nxuskit_bn_search_structure(
        net: *const NxuskitBnNet,
        csv_path: *const c_char,
        algorithm: *const c_char,
        scoring: *const c_char,
        max_parents: u32,
        max_steps: u32,
        ess: f64,
        ordering_json: *const c_char,
    ) -> *mut c_char;

    // Shared utilities
    fn nxuskit_last_error() -> *const c_char;
    fn nxuskit_free_string(ptr: *mut c_char);
}

// ── Helper functions ────────────────────────────────────────────────────

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

fn last_error() -> Option<String> {
    let ptr = unsafe { nxuskit_last_error() };
    if ptr.is_null() {
        return None;
    }
    let s = unsafe { CStr::from_ptr(ptr) }.to_str().ok()?;
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

fn read_c_string(ptr: *mut c_char) -> String {
    assert!(
        !ptr.is_null(),
        "C string pointer is null; last error: {:?}",
        last_error()
    );
    let s = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .expect("not valid UTF-8")
        .to_owned();
    unsafe { nxuskit_free_string(ptr) };
    s
}

// ── Network Lifecycle Tests ──────────────────────────────────────────

#[test]
fn test_bn_net_create_and_destroy() {
    let net = unsafe { nxuskit_bn_net_create() };
    assert!(
        !net.is_null(),
        "net_create returned NULL: {:?}",
        last_error()
    );
    assert_eq!(unsafe { nxuskit_bn_net_num_variables(net) }, 0);
    unsafe { nxuskit_bn_net_destroy(net) };
}

#[test]
fn test_bn_net_load_file_asia() {
    let path = fixture_path("asia.bif");
    let net = unsafe { nxuskit_bn_net_load_file(path.as_ptr()) };
    assert!(
        !net.is_null(),
        "load_file returned NULL: {:?}",
        last_error()
    );

    let num_vars = unsafe { nxuskit_bn_net_num_variables(net) };
    assert_eq!(num_vars, 8, "Asia network should have 8 variables");

    unsafe { nxuskit_bn_net_destroy(net) };
}

#[test]
fn test_bn_net_load_file_nonexistent() {
    let path = c("nonexistent-network.bif");
    let net = unsafe { nxuskit_bn_net_load_file(path.as_ptr()) };
    assert!(net.is_null(), "loading nonexistent file should return NULL");
    assert!(last_error().is_some(), "should set an error message");
}

#[test]
fn test_bn_net_variables_json() {
    let path = fixture_path("asia.bif");
    let net = unsafe { nxuskit_bn_net_load_file(path.as_ptr()) };
    assert!(!net.is_null());

    let json_ptr = unsafe { nxuskit_bn_net_variables(net) };
    let json_str = read_c_string(json_ptr);
    let vars: Vec<String> = serde_json::from_str(&json_str).unwrap();
    assert_eq!(vars.len(), 8);

    // Check some expected variable names
    assert!(vars.contains(&"Smoking".to_string()));
    assert!(vars.contains(&"Bronchitis".to_string()));

    unsafe { nxuskit_bn_net_destroy(net) };
}

#[test]
fn test_bn_net_variable_states() {
    let path = fixture_path("asia.bif");
    let net = unsafe { nxuskit_bn_net_load_file(path.as_ptr()) };
    assert!(!net.is_null());

    let var_name = c("Smoking");
    let states_ptr = unsafe { nxuskit_bn_net_variable_states(net, var_name.as_ptr()) };
    let states_str = read_c_string(states_ptr);
    let states: Vec<String> = serde_json::from_str(&states_str).unwrap();
    assert_eq!(states.len(), 2, "Smoking should have 2 states");
    assert!(states.contains(&"yes".to_string()));
    assert!(states.contains(&"no".to_string()));

    unsafe { nxuskit_bn_net_destroy(net) };
}

// ── Evidence Tests ──────────────────────────────────────────────────────

#[test]
fn test_bn_ev_create_and_destroy() {
    let ev = unsafe { nxuskit_bn_ev_create() };
    assert!(!ev.is_null(), "ev_create returned NULL: {:?}", last_error());
    unsafe { nxuskit_bn_ev_destroy(ev) };
}

#[test]
fn test_bn_ev_set_discrete() {
    let path = fixture_path("asia.bif");
    let net = unsafe { nxuskit_bn_net_load_file(path.as_ptr()) };
    assert!(!net.is_null());

    let ev = unsafe { nxuskit_bn_ev_create() };
    assert!(!ev.is_null());

    let var_name = c("Smoking");
    let state = c("yes");
    let ok = unsafe { nxuskit_bn_ev_set_discrete(ev, net, var_name.as_ptr(), state.as_ptr()) };
    assert!(ok, "set_discrete should succeed; error: {:?}", last_error());

    unsafe { nxuskit_bn_ev_destroy(ev) };
    unsafe { nxuskit_bn_net_destroy(net) };
}

#[test]
fn test_bn_ev_retract() {
    let path = fixture_path("asia.bif");
    let net = unsafe { nxuskit_bn_net_load_file(path.as_ptr()) };
    let ev = unsafe { nxuskit_bn_ev_create() };

    let var_name = c("Smoking");
    let state = c("yes");
    unsafe { nxuskit_bn_ev_set_discrete(ev, net, var_name.as_ptr(), state.as_ptr()) };

    let ok = unsafe { nxuskit_bn_ev_retract(ev, var_name.as_ptr()) };
    assert!(ok, "retract should succeed");

    unsafe { nxuskit_bn_ev_destroy(ev) };
    unsafe { nxuskit_bn_net_destroy(net) };
}

#[test]
fn test_bn_ev_clear() {
    let path = fixture_path("asia.bif");
    let net = unsafe { nxuskit_bn_net_load_file(path.as_ptr()) };
    let ev = unsafe { nxuskit_bn_ev_create() };

    let var_name = c("Smoking");
    let state = c("yes");
    unsafe { nxuskit_bn_ev_set_discrete(ev, net, var_name.as_ptr(), state.as_ptr()) };

    let ok = unsafe { nxuskit_bn_ev_clear(ev) };
    assert!(ok, "clear should succeed");

    unsafe { nxuskit_bn_ev_destroy(ev) };
    unsafe { nxuskit_bn_net_destroy(net) };
}

// ── Inference Tests (VE) ──────────────────────────────────────────────

#[test]
fn test_bn_infer_ve_no_evidence() {
    let path = fixture_path("asia.bif");
    let net = unsafe { nxuskit_bn_net_load_file(path.as_ptr()) };
    assert!(!net.is_null());

    let ev = unsafe { nxuskit_bn_ev_create() };
    let algo = c("ve");

    let result = unsafe { nxuskit_bn_infer(net, ev, algo.as_ptr(), 0, 0, 0) };
    assert!(!result.is_null(), "infer returned NULL: {:?}", last_error());

    let num = unsafe { nxuskit_bn_result_num_variables(result) };
    assert_eq!(num, 8, "Should have 8 variables in result");

    unsafe { nxuskit_bn_result_destroy(result) };
    unsafe { nxuskit_bn_ev_destroy(ev) };
    unsafe { nxuskit_bn_net_destroy(net) };
}

#[test]
fn test_bn_infer_ve_with_evidence() {
    let path = fixture_path("asia.bif");
    let net = unsafe { nxuskit_bn_net_load_file(path.as_ptr()) };
    let ev = unsafe { nxuskit_bn_ev_create() };

    let var_name = c("Smoking");
    let state = c("yes");
    unsafe { nxuskit_bn_ev_set_discrete(ev, net, var_name.as_ptr(), state.as_ptr()) };

    let algo = c("ve");
    let result = unsafe { nxuskit_bn_infer(net, ev, algo.as_ptr(), 0, 0, 0) };
    assert!(
        !result.is_null(),
        "infer with evidence returned NULL: {:?}",
        last_error()
    );

    // Query Bronchitis posterior
    let query_var = c("Bronchitis");
    let query_ptr = unsafe { nxuskit_bn_result_query(result, query_var.as_ptr()) };
    let query_str = read_c_string(query_ptr);
    let dist: HashMap<String, f64> = serde_json::from_str(&query_str).unwrap();

    // With Smoking=yes, P(Bronchitis=present) should be > 0.5
    let p_present = dist.get("present").unwrap();
    assert!(
        *p_present > 0.5,
        "P(Bronchitis=present|Smoking=yes) should be > 0.5, got {}",
        p_present
    );

    unsafe { nxuskit_bn_result_destroy(result) };
    unsafe { nxuskit_bn_ev_destroy(ev) };
    unsafe { nxuskit_bn_net_destroy(net) };
}

// ── Inference Tests (JT) ──────────────────────────────────────────────

#[test]
fn test_bn_infer_jt() {
    let path = fixture_path("asia.bif");
    let net = unsafe { nxuskit_bn_net_load_file(path.as_ptr()) };
    let ev = unsafe { nxuskit_bn_ev_create() };

    let algo = c("jt");
    let result = unsafe { nxuskit_bn_infer(net, ev, algo.as_ptr(), 0, 0, 0) };
    assert!(
        !result.is_null(),
        "JT infer returned NULL: {:?}",
        last_error()
    );

    assert_eq!(unsafe { nxuskit_bn_result_num_variables(result) }, 8);

    unsafe { nxuskit_bn_result_destroy(result) };
    unsafe { nxuskit_bn_ev_destroy(ev) };
    unsafe { nxuskit_bn_net_destroy(net) };
}

// ── Inference Tests (Gibbs) ──────────────────────────────────────────────

#[test]
fn test_bn_infer_gibbs() {
    let path = fixture_path("asia.bif");
    let net = unsafe { nxuskit_bn_net_load_file(path.as_ptr()) };
    let ev = unsafe { nxuskit_bn_ev_create() };

    let algo = c("gibbs");
    let result = unsafe { nxuskit_bn_infer(net, ev, algo.as_ptr(), 5000, 500, 42) };
    assert!(
        !result.is_null(),
        "Gibbs infer returned NULL: {:?}",
        last_error()
    );

    assert_eq!(unsafe { nxuskit_bn_result_num_variables(result) }, 8);

    unsafe { nxuskit_bn_result_destroy(result) };
    unsafe { nxuskit_bn_ev_destroy(ev) };
    unsafe { nxuskit_bn_net_destroy(net) };
}

// ── Result JSON Output ──────────────────────────────────────────────────

#[test]
fn test_bn_result_json() {
    let path = fixture_path("asia.bif");
    let net = unsafe { nxuskit_bn_net_load_file(path.as_ptr()) };
    let ev = unsafe { nxuskit_bn_ev_create() };
    let algo = c("ve");

    let result = unsafe { nxuskit_bn_infer(net, ev, algo.as_ptr(), 0, 0, 0) };
    assert!(!result.is_null());

    let json_ptr = unsafe { nxuskit_bn_result_json(result) };
    let json_str = read_c_string(json_ptr);

    // Should be valid JSON with expected structure
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    assert!(
        parsed.get("marginals").is_some(),
        "JSON should have 'marginals' key"
    );
    assert!(
        parsed.get("algorithm").is_some(),
        "JSON should have 'algorithm' key"
    );
    assert_eq!(parsed["algorithm"], "ve");

    unsafe { nxuskit_bn_result_destroy(result) };
    unsafe { nxuskit_bn_ev_destroy(ev) };
    unsafe { nxuskit_bn_net_destroy(net) };
}

#[test]
fn test_bn_result_query_single_variable() {
    let path = fixture_path("asia.bif");
    let net = unsafe { nxuskit_bn_net_load_file(path.as_ptr()) };
    let ev = unsafe { nxuskit_bn_ev_create() };
    let algo = c("ve");

    let result = unsafe { nxuskit_bn_infer(net, ev, algo.as_ptr(), 0, 0, 0) };
    assert!(!result.is_null());

    let var = c("Smoking");
    let query_ptr = unsafe { nxuskit_bn_result_query(result, var.as_ptr()) };
    let query_str = read_c_string(query_ptr);
    let dist: HashMap<String, f64> = serde_json::from_str(&query_str).unwrap();

    // Smoking prior: 0.5/0.5
    assert_eq!(dist.len(), 2);
    assert!((dist["yes"] - 0.5).abs() < 1e-6);
    assert!((dist["no"] - 0.5).abs() < 1e-6);

    unsafe { nxuskit_bn_result_destroy(result) };
    unsafe { nxuskit_bn_ev_destroy(ev) };
    unsafe { nxuskit_bn_net_destroy(net) };
}

// ── Result Iteration ──────────────────────────────────────────────────────

#[test]
fn test_bn_result_iteration() {
    let path = fixture_path("asia.bif");
    let net = unsafe { nxuskit_bn_net_load_file(path.as_ptr()) };
    let ev = unsafe { nxuskit_bn_ev_create() };
    let algo = c("ve");

    let result = unsafe { nxuskit_bn_infer(net, ev, algo.as_ptr(), 0, 0, 0) };
    assert!(!result.is_null());

    // Iterate through all variables
    let mut var_names = Vec::new();
    loop {
        let name_ptr = unsafe { nxuskit_bn_result_next(result) };
        if name_ptr.is_null() {
            break;
        }
        var_names.push(read_c_string(name_ptr));
    }
    assert_eq!(var_names.len(), 8, "Should iterate 8 variables");

    // Reset and iterate again
    unsafe { nxuskit_bn_result_reset(result) };
    let mut var_names2 = Vec::new();
    loop {
        let name_ptr = unsafe { nxuskit_bn_result_next(result) };
        if name_ptr.is_null() {
            break;
        }
        var_names2.push(read_c_string(name_ptr));
    }
    assert_eq!(
        var_names, var_names2,
        "Reset should produce same iteration order"
    );

    unsafe { nxuskit_bn_result_destroy(result) };
    unsafe { nxuskit_bn_ev_destroy(ev) };
    unsafe { nxuskit_bn_net_destroy(net) };
}

// ── NULL Handle Safety ──────────────────────────────────────────────────

#[test]
fn test_bn_null_net_num_variables() {
    let result = unsafe { nxuskit_bn_net_num_variables(std::ptr::null()) };
    assert_eq!(result, -1, "NULL net should return -1");
}

#[test]
fn test_bn_null_net_variables() {
    let result = unsafe { nxuskit_bn_net_variables(std::ptr::null()) };
    assert!(result.is_null(), "NULL net variables should return NULL");
}

#[test]
fn test_bn_null_result_num_variables() {
    let result = unsafe { nxuskit_bn_result_num_variables(std::ptr::null()) };
    assert_eq!(result, -1, "NULL result should return -1");
}

#[test]
fn test_bn_null_result_json() {
    let result = unsafe { nxuskit_bn_result_json(std::ptr::null()) };
    assert!(result.is_null(), "NULL result json should return NULL");
}

// ── VE vs JT Cross-Validation ──────────────────────────────────────────

#[test]
fn test_bn_ve_jt_agreement() {
    let path = fixture_path("asia.bif");
    let net = unsafe { nxuskit_bn_net_load_file(path.as_ptr()) };
    let ev = unsafe { nxuskit_bn_ev_create() };

    // Set evidence
    let var_name = c("Smoking");
    let state = c("yes");
    unsafe { nxuskit_bn_ev_set_discrete(ev, net, var_name.as_ptr(), state.as_ptr()) };

    // VE inference
    let algo_ve = c("ve");
    let result_ve = unsafe { nxuskit_bn_infer(net, ev, algo_ve.as_ptr(), 0, 0, 0) };
    assert!(!result_ve.is_null());

    // JT inference
    let algo_jt = c("jt");
    let result_jt = unsafe { nxuskit_bn_infer(net, ev, algo_jt.as_ptr(), 0, 0, 0) };
    assert!(!result_jt.is_null());

    // Compare Bronchitis posteriors
    let query_var = c("Bronchitis");
    let ve_ptr = unsafe { nxuskit_bn_result_query(result_ve, query_var.as_ptr()) };
    let jt_ptr = unsafe { nxuskit_bn_result_query(result_jt, query_var.as_ptr()) };
    let ve_str = read_c_string(ve_ptr);
    let jt_str = read_c_string(jt_ptr);
    let ve_dist: HashMap<String, f64> = serde_json::from_str(&ve_str).unwrap();
    let jt_dist: HashMap<String, f64> = serde_json::from_str(&jt_str).unwrap();

    for (state_name, &p_ve) in &ve_dist {
        let p_jt = jt_dist[state_name];
        assert!(
            (p_ve - p_jt).abs() < 1e-6,
            "VE vs JT mismatch for Bronchitis[{}]: {} vs {}",
            state_name,
            p_ve,
            p_jt
        );
    }

    unsafe { nxuskit_bn_result_destroy(result_ve) };
    unsafe { nxuskit_bn_result_destroy(result_jt) };
    unsafe { nxuskit_bn_ev_destroy(ev) };
    unsafe { nxuskit_bn_net_destroy(net) };
}

// ── Alarm Network (37 nodes) ──────────────────────────────────────────

#[test]
fn test_bn_alarm_network() {
    let path = fixture_path("alarm.bif");
    let net = unsafe { nxuskit_bn_net_load_file(path.as_ptr()) };
    assert!(
        !net.is_null(),
        "loading alarm.bif failed: {:?}",
        last_error()
    );
    assert_eq!(unsafe { nxuskit_bn_net_num_variables(net) }, 37);

    let ev = unsafe { nxuskit_bn_ev_create() };
    let algo = c("ve");
    let result = unsafe { nxuskit_bn_infer(net, ev, algo.as_ptr(), 0, 0, 0) };
    assert!(
        !result.is_null(),
        "Alarm VE infer failed: {:?}",
        last_error()
    );
    assert_eq!(unsafe { nxuskit_bn_result_num_variables(result) }, 37);

    unsafe { nxuskit_bn_result_destroy(result) };
    unsafe { nxuskit_bn_ev_destroy(ev) };
    unsafe { nxuskit_bn_net_destroy(net) };
}

// ── MLE Parameter Learning Tests ─────────────────────────────────────

fn csv_fixture_path(name: &str) -> CString {
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

#[test]
fn test_bn_learn_mle_cancer() {
    // Load cancer network
    let bif_path = fixture_path("cancer.bif");
    let net = unsafe { nxuskit_bn_net_load_file(bif_path.as_ptr()) };
    assert!(!net.is_null(), "load cancer.bif failed: {:?}", last_error());

    // Learn from CSV data
    let csv_path = csv_fixture_path("cancer_data.csv");
    let ok = unsafe { nxuskit_bn_learn_mle(net, csv_path.as_ptr(), 1.0) };
    assert!(ok, "learn_mle failed: {:?}", last_error());

    // Verify learned CPTs by running inference — should not crash and
    // should produce valid posteriors
    let ev = unsafe { nxuskit_bn_ev_create() };
    let algo = c("ve");
    let result = unsafe { nxuskit_bn_infer(net, ev, algo.as_ptr(), 0, 0, 0) };
    assert!(
        !result.is_null(),
        "infer after learn failed: {:?}",
        last_error()
    );
    assert_eq!(unsafe { nxuskit_bn_result_num_variables(result) }, 5);

    // Query Pollution posterior (learned from data)
    let var = c("Pollution");
    let query_ptr = unsafe { nxuskit_bn_result_query(result, var.as_ptr()) };
    let query_str = read_c_string(query_ptr);
    let dist: HashMap<String, f64> = serde_json::from_str(&query_str).unwrap();
    let sum: f64 = dist.values().sum();
    assert!(
        (sum - 1.0).abs() < 1e-6,
        "Pollution posterior should sum to 1.0, got {}",
        sum
    );

    unsafe { nxuskit_bn_result_destroy(result) };
    unsafe { nxuskit_bn_ev_destroy(ev) };
    unsafe { nxuskit_bn_net_destroy(net) };
}

#[test]
fn test_bn_learn_mle_null_net() {
    let csv_path = csv_fixture_path("cancer_data.csv");
    let ok = unsafe { nxuskit_bn_learn_mle(std::ptr::null_mut(), csv_path.as_ptr(), 1.0) };
    assert!(!ok, "learn_mle with NULL net should return false");
}

#[test]
fn test_bn_learn_mle_invalid_csv() {
    let bif_path = fixture_path("cancer.bif");
    let net = unsafe { nxuskit_bn_net_load_file(bif_path.as_ptr()) };
    assert!(!net.is_null());

    let csv_path = c("nonexistent.csv");
    let ok = unsafe { nxuskit_bn_learn_mle(net, csv_path.as_ptr(), 1.0) };
    assert!(!ok, "learn_mle with invalid CSV should return false");
    assert!(last_error().is_some());

    unsafe { nxuskit_bn_net_destroy(net) };
}

#[test]
fn test_bn_log_likelihood_cancer() {
    let bif_path = fixture_path("cancer.bif");
    let net = unsafe { nxuskit_bn_net_load_file(bif_path.as_ptr()) };
    assert!(!net.is_null());

    let csv_path = csv_fixture_path("cancer_data.csv");
    let ll = unsafe { nxuskit_bn_log_likelihood(net, csv_path.as_ptr()) };

    // Log-likelihood should be negative (probability < 1)
    assert!(
        ll.is_finite(),
        "log-likelihood should be finite, got {}",
        ll
    );
    assert!(ll < 0.0, "log-likelihood should be negative, got {}", ll);

    unsafe { nxuskit_bn_net_destroy(net) };
}

#[test]
fn test_bn_log_likelihood_null_net() {
    let csv_path = csv_fixture_path("cancer_data.csv");
    let ll = unsafe { nxuskit_bn_log_likelihood(std::ptr::null(), csv_path.as_ptr()) };
    assert_eq!(ll, f64::NEG_INFINITY, "NULL net should return NEG_INFINITY");
}

#[test]
fn test_bn_learn_then_log_likelihood() {
    // Learn parameters, then compute log-likelihood — should be higher than
    // the log-likelihood of the original (BIF-defined) parameters
    let bif_path = fixture_path("cancer.bif");
    let csv_path = csv_fixture_path("cancer_data.csv");

    // Compute LL with original CPTs
    let net_orig = unsafe { nxuskit_bn_net_load_file(bif_path.as_ptr()) };
    assert!(!net_orig.is_null());
    let ll_orig = unsafe { nxuskit_bn_log_likelihood(net_orig, csv_path.as_ptr()) };
    assert!(ll_orig.is_finite());

    // Learn CPTs from same data
    let net_learned = unsafe { nxuskit_bn_net_load_file(bif_path.as_ptr()) };
    assert!(!net_learned.is_null());
    let ok = unsafe { nxuskit_bn_learn_mle(net_learned, csv_path.as_ptr(), 1.0) };
    assert!(ok, "learn_mle failed: {:?}", last_error());
    let ll_learned = unsafe { nxuskit_bn_log_likelihood(net_learned, csv_path.as_ptr()) };
    assert!(ll_learned.is_finite());

    // MLE should produce higher (or equal) log-likelihood than original params
    assert!(
        ll_learned >= ll_orig - 1e-6,
        "Learned LL ({}) should be >= original LL ({})",
        ll_learned,
        ll_orig,
    );

    unsafe { nxuskit_bn_net_destroy(net_orig) };
    unsafe { nxuskit_bn_net_destroy(net_learned) };
}

// ── Structure Learning Tests ─────────────────────────────────────

#[test]
fn test_bn_search_hill_climb_bic() {
    let bif_path = fixture_path("cancer.bif");
    let net = unsafe { nxuskit_bn_net_load_file(bif_path.as_ptr()) };
    assert!(!net.is_null(), "load cancer.bif failed: {:?}", last_error());

    let csv_path = csv_fixture_path("cancer_data.csv");
    let algo = c("hill_climb");
    let scoring = c("bic");

    let result_ptr = unsafe {
        nxuskit_bn_search_structure(
            net,
            csv_path.as_ptr(),
            algo.as_ptr(),
            scoring.as_ptr(),
            0,                // default max_parents
            0,                // default max_steps
            0.0,              // ignored for BIC
            std::ptr::null(), // no ordering needed for hill_climb
        )
    };
    assert!(
        !result_ptr.is_null(),
        "search returned NULL: {:?}",
        last_error()
    );

    let result_str = read_c_string(result_ptr);
    let result: serde_json::Value = serde_json::from_str(&result_str).unwrap();
    assert_eq!(result["algorithm"], "hill_climb");
    assert_eq!(result["scoring"], "bic");
    assert!(result["score"].as_f64().unwrap().is_finite());
    assert!(result["num_variables"].as_u64().unwrap() == 5);

    unsafe { nxuskit_bn_net_destroy(net) };
}

#[test]
fn test_bn_search_k2() {
    let bif_path = fixture_path("cancer.bif");
    let net = unsafe { nxuskit_bn_net_load_file(bif_path.as_ptr()) };
    assert!(!net.is_null());

    let csv_path = csv_fixture_path("cancer_data.csv");
    let algo = c("k2");
    let scoring = c("bic");
    let ordering = c(r#"["Pollution","Smoker","Cancer","Xray","Dyspnea"]"#);

    let result_ptr = unsafe {
        nxuskit_bn_search_structure(
            net,
            csv_path.as_ptr(),
            algo.as_ptr(),
            scoring.as_ptr(),
            3, // max_parents
            0, // ignored for k2
            0.0,
            ordering.as_ptr(),
        )
    };
    assert!(
        !result_ptr.is_null(),
        "K2 search returned NULL: {:?}",
        last_error()
    );

    let result_str = read_c_string(result_ptr);
    let result: serde_json::Value = serde_json::from_str(&result_str).unwrap();
    assert_eq!(result["algorithm"], "k2");
    assert!(result["score"].as_f64().unwrap().is_finite());

    unsafe { nxuskit_bn_net_destroy(net) };
}

#[test]
fn test_bn_search_bdeu() {
    let bif_path = fixture_path("cancer.bif");
    let net = unsafe { nxuskit_bn_net_load_file(bif_path.as_ptr()) };
    assert!(!net.is_null());

    let csv_path = csv_fixture_path("cancer_data.csv");
    let algo = c("hill_climb");
    let scoring = c("bdeu");

    let result_ptr = unsafe {
        nxuskit_bn_search_structure(
            net,
            csv_path.as_ptr(),
            algo.as_ptr(),
            scoring.as_ptr(),
            0,
            0,
            10.0, // ESS for BDeu
            std::ptr::null(),
        )
    };
    assert!(
        !result_ptr.is_null(),
        "BDeu search returned NULL: {:?}",
        last_error()
    );

    let result_str = read_c_string(result_ptr);
    let result: serde_json::Value = serde_json::from_str(&result_str).unwrap();
    assert_eq!(result["scoring"], "bdeu");
    assert!(result["score"].as_f64().unwrap().is_finite());

    unsafe { nxuskit_bn_net_destroy(net) };
}

#[test]
fn test_bn_search_null_net() {
    let csv_path = csv_fixture_path("cancer_data.csv");
    let algo = c("hill_climb");
    let scoring = c("bic");

    let result_ptr = unsafe {
        nxuskit_bn_search_structure(
            std::ptr::null(),
            csv_path.as_ptr(),
            algo.as_ptr(),
            scoring.as_ptr(),
            0,
            0,
            0.0,
            std::ptr::null(),
        )
    };
    assert!(result_ptr.is_null(), "NULL net should return NULL");
}

#[test]
fn test_bn_search_invalid_algorithm() {
    let bif_path = fixture_path("cancer.bif");
    let net = unsafe { nxuskit_bn_net_load_file(bif_path.as_ptr()) };
    assert!(!net.is_null());

    let csv_path = csv_fixture_path("cancer_data.csv");
    let algo = c("invalid_algo");
    let scoring = c("bic");

    let result_ptr = unsafe {
        nxuskit_bn_search_structure(
            net,
            csv_path.as_ptr(),
            algo.as_ptr(),
            scoring.as_ptr(),
            0,
            0,
            0.0,
            std::ptr::null(),
        )
    };
    assert!(result_ptr.is_null(), "Invalid algorithm should return NULL");
    assert!(last_error().is_some());

    unsafe { nxuskit_bn_net_destroy(net) };
}
