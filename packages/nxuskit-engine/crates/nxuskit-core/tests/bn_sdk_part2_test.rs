//! Integration tests for the Bayesian Network C ABI — Part 2 extensions.
//!
//! Tests BIF export, Gaussian variables, continuous evidence, LBP/NUTS
//! inference, continuous marginal access, and infer_with_config.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::PathBuf;

// Import nxuskit_core so Cargo links the library.
use nxuskit_core as _;

// ── FFI declarations ────────────────────────────────────────────────

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

#[allow(dead_code)]
unsafe extern "C" {
    // Existing Part 1 functions needed for setup
    fn nxuskit_bn_net_create() -> *mut NxuskitBnNet;
    fn nxuskit_bn_net_destroy(net: *mut NxuskitBnNet);
    fn nxuskit_bn_net_load_file(path: *const c_char) -> *mut NxuskitBnNet;
    fn nxuskit_bn_net_num_variables(net: *const NxuskitBnNet) -> i32;
    fn nxuskit_bn_ev_create() -> *mut NxuskitBnEvidence;
    fn nxuskit_bn_ev_destroy(ev: *mut NxuskitBnEvidence);
    fn nxuskit_bn_ev_set_discrete(
        ev: *mut NxuskitBnEvidence,
        net: *const NxuskitBnNet,
        variable: *const c_char,
        state: *const c_char,
    ) -> bool;
    fn nxuskit_bn_infer(
        net: *const NxuskitBnNet,
        ev: *const NxuskitBnEvidence,
        algorithm: *const c_char,
        num_samples: u32,
        burn_in: u32,
        seed: u64,
    ) -> *mut NxuskitBnResult;
    fn nxuskit_bn_result_destroy(result: *mut NxuskitBnResult);
    fn nxuskit_bn_result_json(result: *const NxuskitBnResult) -> *mut c_char;
    fn nxuskit_bn_result_num_variables(result: *const NxuskitBnResult) -> i32;
    fn nxuskit_free_string(s: *mut c_char);

    // Part 2 extensions
    fn nxuskit_bn_net_save_file(net: *const NxuskitBnNet, path: *const c_char) -> bool;
    fn nxuskit_bn_ev_set_continuous(
        ev: *mut NxuskitBnEvidence,
        net: *const NxuskitBnNet,
        variable: *const c_char,
        value: f64,
    ) -> bool;
    fn nxuskit_bn_net_add_gaussian_variable(
        net: *mut NxuskitBnNet,
        name: *const c_char,
        mean_base: f64,
        variance: f64,
    ) -> bool;
    fn nxuskit_bn_net_set_gaussian_weight(
        net: *mut NxuskitBnNet,
        variable: *const c_char,
        parent: *const c_char,
        weight: f64,
    ) -> bool;
    fn nxuskit_bn_result_mean(result: *const NxuskitBnResult, variable: *const c_char) -> f64;
    fn nxuskit_bn_result_variance(result: *const NxuskitBnResult, variable: *const c_char) -> f64;
    fn nxuskit_bn_result_continuous_marginal(
        result: *const NxuskitBnResult,
        variable: *const c_char,
    ) -> *mut c_char;
    fn nxuskit_bn_infer_with_config(
        net: *const NxuskitBnNet,
        ev: *const NxuskitBnEvidence,
        algorithm: *const c_char,
        config_json: *const c_char,
    ) -> *mut NxuskitBnResult;
}

// ── Helpers ─────────────────────────────────────────────────────────

fn c(s: &str) -> CString {
    CString::new(s).unwrap()
}

unsafe fn read_and_free(ptr: *mut c_char) -> String {
    assert!(!ptr.is_null(), "Expected non-null C string");
    let s = unsafe { CStr::from_ptr(ptr) }.to_str().unwrap().to_string();
    unsafe { nxuskit_free_string(ptr) };
    s
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("nxuskit-engine")
        .join("tests")
        .join("fixtures")
        .join("bn")
        .join(name)
}

// ── BIF Export Tests ────────────────────────────────────────────────

#[test]
fn part2_save_bif_roundtrip() {
    let path = c(fixture("asia.bif").to_str().unwrap());
    let net = unsafe { nxuskit_bn_net_load_file(path.as_ptr()) };
    assert!(!net.is_null());

    let tmp = std::env::temp_dir().join("nxuskit_part2_save_test.bif");
    let tmp_c = c(tmp.to_str().unwrap());
    let ok = unsafe { nxuskit_bn_net_save_file(net, tmp_c.as_ptr()) };
    assert!(ok, "save_file should succeed");

    // Verify round-trip
    let reloaded = unsafe { nxuskit_bn_net_load_file(tmp_c.as_ptr()) };
    assert!(!reloaded.is_null());
    assert_eq!(unsafe { nxuskit_bn_net_num_variables(reloaded) }, 8);

    unsafe {
        nxuskit_bn_net_destroy(reloaded);
        nxuskit_bn_net_destroy(net);
    }
    let _ = std::fs::remove_file(&tmp);
}

// ── Gaussian Variable Tests ─────────────────────────────────────────

#[test]
fn part2_add_gaussian_variable() {
    let net = unsafe { nxuskit_bn_net_create() };
    assert!(!net.is_null());

    let name = c("Temperature");
    let ok = unsafe { nxuskit_bn_net_add_gaussian_variable(net, name.as_ptr(), 20.0, 5.0) };
    assert!(ok, "add_gaussian_variable should succeed");

    unsafe { nxuskit_bn_net_destroy(net) };
}

#[test]
fn part2_set_gaussian_weight() {
    let net = unsafe { nxuskit_bn_net_create() };
    assert!(!net.is_null());

    let x = c("X");
    let y = c("Y");
    unsafe {
        assert!(nxuskit_bn_net_add_gaussian_variable(
            net,
            x.as_ptr(),
            0.0,
            1.0
        ));
        assert!(nxuskit_bn_net_add_gaussian_variable(
            net,
            y.as_ptr(),
            0.0,
            1.0
        ));
        assert!(nxuskit_bn_net_set_gaussian_weight(
            net,
            y.as_ptr(),
            x.as_ptr(),
            0.5
        ));
        nxuskit_bn_net_destroy(net);
    }
}

// ── Continuous Evidence Tests ───────────────────────────────────────

#[test]
fn part2_set_continuous_evidence() {
    let net = unsafe { nxuskit_bn_net_create() };
    assert!(!net.is_null());

    let x = c("X");
    unsafe {
        assert!(nxuskit_bn_net_add_gaussian_variable(
            net,
            x.as_ptr(),
            0.0,
            1.0
        ));
    }

    let ev = unsafe { nxuskit_bn_ev_create() };
    assert!(!ev.is_null());

    let ok = unsafe { nxuskit_bn_ev_set_continuous(ev, net, x.as_ptr(), 2.5) };
    assert!(ok, "set_continuous should succeed");

    unsafe {
        nxuskit_bn_ev_destroy(ev);
        nxuskit_bn_net_destroy(net);
    }
}

// ── LBP Inference Tests ────────────────────────────────────────────

#[test]
fn part2_infer_lbp() {
    let path = c(fixture("asia.bif").to_str().unwrap());
    let net = unsafe { nxuskit_bn_net_load_file(path.as_ptr()) };
    assert!(!net.is_null());

    let ev = unsafe { nxuskit_bn_ev_create() };
    let algo = c("lbp");
    let result = unsafe { nxuskit_bn_infer(net, ev, algo.as_ptr(), 0, 0, 0) };
    assert!(!result.is_null(), "LBP inference should succeed");
    assert_eq!(unsafe { nxuskit_bn_result_num_variables(result) }, 8);

    unsafe {
        nxuskit_bn_result_destroy(result);
        nxuskit_bn_ev_destroy(ev);
        nxuskit_bn_net_destroy(net);
    }
}

#[test]
fn part2_infer_lbp_with_config() {
    let path = c(fixture("asia.bif").to_str().unwrap());
    let net = unsafe { nxuskit_bn_net_load_file(path.as_ptr()) };
    assert!(!net.is_null());

    let ev = unsafe { nxuskit_bn_ev_create() };
    let algo = c("lbp");
    let config = c(r#"{"max_iterations": 200, "damping": 0.3}"#);
    let result = unsafe { nxuskit_bn_infer_with_config(net, ev, algo.as_ptr(), config.as_ptr()) };
    assert!(!result.is_null(), "LBP infer_with_config should succeed");
    assert_eq!(unsafe { nxuskit_bn_result_num_variables(result) }, 8);

    unsafe {
        nxuskit_bn_result_destroy(result);
        nxuskit_bn_ev_destroy(ev);
        nxuskit_bn_net_destroy(net);
    }
}

// ── NUTS Inference Tests ────────────────────────────────────────────

#[test]
fn part2_infer_nuts() {
    let net = unsafe { nxuskit_bn_net_create() };
    assert!(!net.is_null());

    let x = c("X");
    let y = c("Y");
    unsafe {
        assert!(nxuskit_bn_net_add_gaussian_variable(
            net,
            x.as_ptr(),
            0.0,
            1.0
        ));
        assert!(nxuskit_bn_net_add_gaussian_variable(
            net,
            y.as_ptr(),
            0.0,
            1.0
        ));
        assert!(nxuskit_bn_net_set_gaussian_weight(
            net,
            y.as_ptr(),
            x.as_ptr(),
            0.5
        ));
    }

    let ev = unsafe { nxuskit_bn_ev_create() };
    let algo = c("nuts");
    let config = c(r#"{"num_samples": 500, "num_tune": 200, "seed": 42}"#);
    let result = unsafe { nxuskit_bn_infer_with_config(net, ev, algo.as_ptr(), config.as_ptr()) };
    assert!(!result.is_null(), "NUTS inference should succeed");

    // Verify JSON contains continuous_marginals
    let json_ptr = unsafe { nxuskit_bn_result_json(result) };
    let json = unsafe { read_and_free(json_ptr) };
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(
        parsed.get("continuous_marginals").is_some(),
        "NUTS result should contain continuous_marginals"
    );

    unsafe {
        nxuskit_bn_result_destroy(result);
        nxuskit_bn_ev_destroy(ev);
        nxuskit_bn_net_destroy(net);
    }
}

// ── Continuous Marginal Access Tests ────────────────────────────────

#[test]
fn part2_result_mean_variance() {
    let net = unsafe { nxuskit_bn_net_create() };
    let x = c("X");
    unsafe {
        assert!(nxuskit_bn_net_add_gaussian_variable(
            net,
            x.as_ptr(),
            5.0,
            2.0
        ));
    }

    let ev = unsafe { nxuskit_bn_ev_create() };
    let algo = c("nuts");
    let config = c(r#"{"num_samples": 1000, "num_tune": 500, "seed": 42}"#);
    let result = unsafe { nxuskit_bn_infer_with_config(net, ev, algo.as_ptr(), config.as_ptr()) };
    assert!(!result.is_null());

    let mean = unsafe { nxuskit_bn_result_mean(result, x.as_ptr()) };
    assert!(!mean.is_nan(), "Mean should not be NaN");
    assert!(
        (mean - 5.0).abs() < 2.0,
        "Posterior mean should be near prior 5.0, got {}",
        mean
    );

    let var = unsafe { nxuskit_bn_result_variance(result, x.as_ptr()) };
    assert!(!var.is_nan(), "Variance should not be NaN");
    assert!(var > 0.0, "Variance should be positive, got {}", var);

    unsafe {
        nxuskit_bn_result_destroy(result);
        nxuskit_bn_ev_destroy(ev);
        nxuskit_bn_net_destroy(net);
    }
}

#[test]
fn part2_result_continuous_marginal_json() {
    let net = unsafe { nxuskit_bn_net_create() };
    let x = c("X");
    unsafe {
        assert!(nxuskit_bn_net_add_gaussian_variable(
            net,
            x.as_ptr(),
            0.0,
            1.0
        ));
    }

    let ev = unsafe { nxuskit_bn_ev_create() };
    let algo = c("nuts");
    let config = c(r#"{"num_samples": 500, "num_tune": 200, "seed": 42}"#);
    let result = unsafe { nxuskit_bn_infer_with_config(net, ev, algo.as_ptr(), config.as_ptr()) };
    assert!(!result.is_null());

    let json_ptr = unsafe { nxuskit_bn_result_continuous_marginal(result, x.as_ptr()) };
    let json = unsafe { read_and_free(json_ptr) };
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert!(parsed.get("mean").is_some(), "Should have mean field");
    assert!(
        parsed.get("variance").is_some(),
        "Should have variance field"
    );
    assert!(
        parsed.get("ci_lower").is_some(),
        "Should have ci_lower field"
    );
    assert!(
        parsed.get("ci_upper").is_some(),
        "Should have ci_upper field"
    );

    let ci_lower = parsed["ci_lower"].as_f64().unwrap();
    let ci_upper = parsed["ci_upper"].as_f64().unwrap();
    let mean = parsed["mean"].as_f64().unwrap();
    assert!(ci_lower < mean, "CI lower should be below mean");
    assert!(ci_upper > mean, "CI upper should be above mean");

    unsafe {
        nxuskit_bn_result_destroy(result);
        nxuskit_bn_ev_destroy(ev);
        nxuskit_bn_net_destroy(net);
    }
}

// ── Gibbs with Config Tests ─────────────────────────────────────────

#[test]
fn part2_infer_gibbs_with_config() {
    let path = c(fixture("asia.bif").to_str().unwrap());
    let net = unsafe { nxuskit_bn_net_load_file(path.as_ptr()) };
    assert!(!net.is_null());

    let ev = unsafe { nxuskit_bn_ev_create() };
    let algo = c("gibbs");
    let config = c(r#"{"num_samples": 5000, "burn_in": 500, "seed": 42}"#);
    let result = unsafe { nxuskit_bn_infer_with_config(net, ev, algo.as_ptr(), config.as_ptr()) };
    assert!(!result.is_null(), "Gibbs infer_with_config should succeed");
    assert_eq!(unsafe { nxuskit_bn_result_num_variables(result) }, 8);

    unsafe {
        nxuskit_bn_result_destroy(result);
        nxuskit_bn_ev_destroy(ev);
        nxuskit_bn_net_destroy(net);
    }
}

// ── Error Case: Mean of Nonexistent Variable ────────────────────────

#[test]
fn part2_mean_nonexistent_returns_nan() {
    let path = c(fixture("asia.bif").to_str().unwrap());
    let net = unsafe { nxuskit_bn_net_load_file(path.as_ptr()) };
    let ev = unsafe { nxuskit_bn_ev_create() };
    let algo = c("ve");
    let result = unsafe { nxuskit_bn_infer(net, ev, algo.as_ptr(), 0, 0, 0) };
    assert!(!result.is_null());

    let bad_var = c("NonexistentVar");
    let mean = unsafe { nxuskit_bn_result_mean(result, bad_var.as_ptr()) };
    assert!(mean.is_nan(), "Mean of nonexistent variable should be NaN");

    unsafe {
        nxuskit_bn_result_destroy(result);
        nxuskit_bn_ev_destroy(ev);
        nxuskit_bn_net_destroy(net);
    }
}
