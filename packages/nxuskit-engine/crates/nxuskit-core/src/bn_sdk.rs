//! Bayesian Network C ABI — opaque handles for cross-language BN inference.
//!
//! Follows the CLIPS SDK pattern: opaque handles, catch_unwind safety,
//! JSON-formatted results, thread-local error reporting.

use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use nxuskit_engine::providers::bayesian::bif::{load_bif_file, save_bif_file};
use nxuskit_engine::providers::bayesian::stream::BayesStreamChunk;
use nxuskit_engine::providers::bayesian::types::{GaussianVariable, VariableName};
use nxuskit_engine::providers::bayesian::{
    BayesianNetwork, Dataset, EliminationHeuristic, Evidence, GibbsSampler, HillClimbConfig,
    HillClimbLearner, InferenceEngine, JunctionTree, K2Config, K2Learner, LoopyBeliefPropagation,
    MleConfig, MleLearner, NUTSConfig, NutsSampler, ParameterLearner, ScoringFunction,
    StructureLearner, VariableElimination,
};

use crate::error;

// ── Opaque handle types ──────────────────────────────────────────

/// Opaque handle to a BayesianNetwork.
pub struct NxuskitBnNet {
    inner: BayesianNetwork,
}

/// Opaque handle to an Evidence set.
pub struct NxuskitBnEvidence {
    inner: Evidence,
}

/// Continuous marginal: mean, variance, 95% credible interval bounds.
struct ContinuousMarginalResult {
    mean: f64,
    variance: f64,
    ci_lower: f64,
    ci_upper: f64,
}

/// Opaque handle to inference results (marginal distributions).
pub struct NxuskitBnResult {
    /// Variable name → state name → probability (discrete marginals)
    marginals: HashMap<String, HashMap<String, f64>>,
    /// Variable name → continuous marginal (mean/variance/CI)
    continuous_marginals: HashMap<String, ContinuousMarginalResult>,
    /// Ordered variable names for iteration
    variables: Vec<String>,
    /// Current iteration index
    cursor: usize,
    /// Algorithm used
    algorithm: String,
    /// Elapsed time in milliseconds
    elapsed_ms: f64,
}

// ── Safe C string helper ─────────────────────────────────────────

unsafe fn c_str_to_str<'a>(ptr: *const c_char, param_name: &str) -> Option<&'a str> {
    if ptr.is_null() {
        error::set_last_error("invalid_argument", &format!("{param_name} is NULL"), None);
        return None;
    }
    let c_str = unsafe { CStr::from_ptr(ptr) };
    match c_str.to_str() {
        Ok(s) => Some(s),
        Err(e) => {
            error::set_last_error(
                "invalid_argument",
                &format!("{param_name} is not valid UTF-8: {e}"),
                None,
            );
            None
        }
    }
}

// ── Network lifecycle ────────────────────────────────────────────

/// Create an empty BayesianNetwork.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_bn_net_create() -> *mut NxuskitBnNet {
    catch_unwind(AssertUnwindSafe(|| {
        error::clear_last_error();
        let lk = crate::entitlement::current_license_key();
        if !crate::entitlement::check_entitlement("bayesian", lk.as_deref()) {
            return std::ptr::null_mut();
        }
        Box::into_raw(Box::new(NxuskitBnNet {
            inner: BayesianNetwork::new(),
        }))
    }))
    .unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in nxuskit_bn_net_create", None);
        std::ptr::null_mut()
    })
}

/// Destroy a BayesianNetwork handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_bn_net_destroy(net: *mut NxuskitBnNet) {
    if !net.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            drop(unsafe { Box::from_raw(net) });
        }));
    }
}

/// Load a BIF file into a new BayesianNetwork. Returns NULL on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_bn_net_load_file(path: *const c_char) -> *mut NxuskitBnNet {
    catch_unwind(AssertUnwindSafe(|| {
        error::clear_last_error();
        let lk = crate::entitlement::current_license_key();
        if !crate::entitlement::check_entitlement("bayesian", lk.as_deref()) {
            return std::ptr::null_mut();
        }
        let path_str = match unsafe { c_str_to_str(path, "path") } {
            Some(s) => s,
            None => return std::ptr::null_mut(),
        };
        match load_bif_file(Path::new(path_str)) {
            Ok(net) => {
                // Enforce max_bayesian_nodes limit
                let limits = crate::entitlement::effective_limits(
                    crate::entitlement::current_license_key().as_deref(),
                );
                let node_count = net.variables().len() as u64;
                if !crate::entitlement::check_limit(
                    limits.max_bayesian_nodes,
                    node_count,
                    "Bayesian network nodes",
                    env!("NXUSKIT_EDITION"),
                ) {
                    return std::ptr::null_mut();
                }
                Box::into_raw(Box::new(NxuskitBnNet { inner: net }))
            }
            Err(e) => {
                error::set_last_error("bn_error", &format!("Failed to load BIF: {e}"), None);
                std::ptr::null_mut()
            }
        }
    }))
    .unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in nxuskit_bn_net_load_file", None);
        std::ptr::null_mut()
    })
}

/// Get the number of variables in the network.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_bn_net_num_variables(net: *const NxuskitBnNet) -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        if net.is_null() {
            error::set_last_error("invalid_argument", "net is NULL", None);
            return -1;
        }
        let net_ref = unsafe { &*net };
        net_ref.inner.num_variables() as i32
    }))
    .unwrap_or(-1)
}

/// Get all variable names as a JSON array string. Caller must free with nxuskit_free_string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_bn_net_variables(net: *const NxuskitBnNet) -> *mut c_char {
    catch_unwind(AssertUnwindSafe(|| {
        error::clear_last_error();
        if net.is_null() {
            error::set_last_error("invalid_argument", "net is NULL", None);
            return std::ptr::null_mut();
        }
        let net_ref = unsafe { &*net };
        let var_names = net_ref.inner.variable_names();
        let names: Vec<&str> = var_names.iter().map(|v| v.as_str()).collect();
        match serde_json::to_string(&names) {
            Ok(json) => match CString::new(json) {
                Ok(cstr) => cstr.into_raw(),
                Err(_) => {
                    error::set_last_error("internal_error", "JSON contains interior NUL", None);
                    std::ptr::null_mut()
                }
            },
            Err(e) => {
                error::set_last_error("internal_error", &format!("Serialization error: {e}"), None);
                std::ptr::null_mut()
            }
        }
    }))
    .unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in nxuskit_bn_net_variables", None);
        std::ptr::null_mut()
    })
}

/// Get states for a variable as a JSON array. Caller must free with nxuskit_free_string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_bn_net_variable_states(
    net: *const NxuskitBnNet,
    variable: *const c_char,
) -> *mut c_char {
    catch_unwind(AssertUnwindSafe(|| {
        error::clear_last_error();
        if net.is_null() {
            error::set_last_error("invalid_argument", "net is NULL", None);
            return std::ptr::null_mut();
        }
        let var_str = match unsafe { c_str_to_str(variable, "variable") } {
            Some(s) => s,
            None => return std::ptr::null_mut(),
        };
        let net_ref = unsafe { &*net };
        let vn = match VariableName::new(var_str) {
            Ok(v) => v,
            Err(e) => {
                error::set_last_error("bn_error", &format!("Invalid variable name: {e}"), None);
                return std::ptr::null_mut();
            }
        };
        match net_ref.inner.variable(&vn) {
            Some(var) => {
                let states: Vec<&str> = var.states.iter().map(|s| s.as_str()).collect();
                match serde_json::to_string(&states) {
                    Ok(json) => CString::new(json)
                        .map(|c| c.into_raw())
                        .unwrap_or(std::ptr::null_mut()),
                    Err(_) => std::ptr::null_mut(),
                }
            }
            None => {
                error::set_last_error("bn_error", &format!("Variable '{var_str}' not found"), None);
                std::ptr::null_mut()
            }
        }
    }))
    .unwrap_or_else(|_| {
        error::set_last_error(
            "internal_error",
            "Panic in nxuskit_bn_net_variable_states",
            None,
        );
        std::ptr::null_mut()
    })
}

// ── Evidence lifecycle ───────────────────────────────────────────

/// Create an empty Evidence set.
#[unsafe(no_mangle)]
pub extern "C" fn nxuskit_bn_ev_create() -> *mut NxuskitBnEvidence {
    catch_unwind(AssertUnwindSafe(|| {
        error::clear_last_error();
        Box::into_raw(Box::new(NxuskitBnEvidence {
            inner: Evidence::new(),
        }))
    }))
    .unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in nxuskit_bn_ev_create", None);
        std::ptr::null_mut()
    })
}

/// Destroy an Evidence handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_bn_ev_destroy(ev: *mut NxuskitBnEvidence) {
    if !ev.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            drop(unsafe { Box::from_raw(ev) });
        }));
    }
}

/// Set a discrete observation: variable=state. Requires network for validation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_bn_ev_set_discrete(
    ev: *mut NxuskitBnEvidence,
    net: *const NxuskitBnNet,
    variable: *const c_char,
    state: *const c_char,
) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        error::clear_last_error();
        if ev.is_null() {
            error::set_last_error("invalid_argument", "ev is NULL", None);
            return false;
        }
        if net.is_null() {
            error::set_last_error("invalid_argument", "net is NULL", None);
            return false;
        }
        let var_str = match unsafe { c_str_to_str(variable, "variable") } {
            Some(s) => s,
            None => return false,
        };
        let state_str = match unsafe { c_str_to_str(state, "state") } {
            Some(s) => s,
            None => return false,
        };
        let ev_ref = unsafe { &mut *ev };
        let net_ref = unsafe { &*net };
        let vn = match VariableName::new(var_str) {
            Ok(v) => v,
            Err(e) => {
                error::set_last_error("bn_error", &format!("Invalid variable name: {e}"), None);
                return false;
            }
        };
        match ev_ref.inner.observe(&net_ref.inner, &vn, state_str) {
            Ok(()) => true,
            Err(e) => {
                error::set_last_error("bn_error", &format!("Evidence error: {e}"), None);
                false
            }
        }
    }))
    .unwrap_or_else(|_| {
        error::set_last_error(
            "internal_error",
            "Panic in nxuskit_bn_ev_set_discrete",
            None,
        );
        false
    })
}

/// Retract evidence for a variable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_bn_ev_retract(
    ev: *mut NxuskitBnEvidence,
    variable: *const c_char,
) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        error::clear_last_error();
        if ev.is_null() {
            error::set_last_error("invalid_argument", "ev is NULL", None);
            return false;
        }
        let var_str = match unsafe { c_str_to_str(variable, "variable") } {
            Some(s) => s,
            None => return false,
        };
        let ev_ref = unsafe { &mut *ev };
        let vn = match VariableName::new(var_str) {
            Ok(v) => v,
            Err(e) => {
                error::set_last_error("bn_error", &format!("Invalid variable name: {e}"), None);
                return false;
            }
        };
        ev_ref.inner.retract(&vn);
        true
    }))
    .unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in nxuskit_bn_ev_retract", None);
        false
    })
}

/// Clear all evidence.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_bn_ev_clear(ev: *mut NxuskitBnEvidence) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        error::clear_last_error();
        if ev.is_null() {
            error::set_last_error("invalid_argument", "ev is NULL", None);
            return false;
        }
        let ev_ref = unsafe { &mut *ev };
        ev_ref.inner.clear();
        true
    }))
    .unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in nxuskit_bn_ev_clear", None);
        false
    })
}

// ── Inference ────────────────────────────────────────────────────

/// Run inference. `algorithm` is "ve", "jt", or "gibbs". Returns NULL on error.
/// For Gibbs: `num_samples` and `burn_in` control sampling (0 = default 10000/1000).
/// `seed` of 0 means non-deterministic.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_bn_infer(
    net: *const NxuskitBnNet,
    ev: *const NxuskitBnEvidence,
    algorithm: *const c_char,
    num_samples: u32,
    burn_in: u32,
    seed: u64,
) -> *mut NxuskitBnResult {
    catch_unwind(AssertUnwindSafe(|| {
        error::clear_last_error();
        if net.is_null() {
            error::set_last_error("invalid_argument", "net is NULL", None);
            return std::ptr::null_mut();
        }
        if ev.is_null() {
            error::set_last_error("invalid_argument", "ev is NULL", None);
            return std::ptr::null_mut();
        }
        let algo_str = match unsafe { c_str_to_str(algorithm, "algorithm") } {
            Some(s) => s,
            None => return std::ptr::null_mut(),
        };
        let net_ref = unsafe { &*net };
        let ev_ref = unsafe { &*ev };

        let start = std::time::Instant::now();
        let result = match algo_str {
            "ve" | "variable_elimination" => {
                let ve = VariableElimination::new();
                ve.infer(&net_ref.inner, &ev_ref.inner)
            }
            "jt" | "junction_tree" => {
                let jt = JunctionTree::new();
                jt.infer(&net_ref.inner, &ev_ref.inner)
            }
            "gibbs" => {
                let ns = if num_samples == 0 {
                    10_000
                } else {
                    num_samples as usize
                };
                let bi = if burn_in == 0 {
                    1_000
                } else {
                    burn_in as usize
                };
                let mut gibbs = GibbsSampler::new(ns, bi);
                if seed != 0 {
                    gibbs = gibbs.with_seed(seed);
                }
                gibbs.infer(&net_ref.inner, &ev_ref.inner)
            }
            "lbp" | "loopy_bp" => {
                let lbp = LoopyBeliefPropagation::new();
                lbp.infer(&net_ref.inner, &ev_ref.inner)
            }
            "nuts" | "hmc" => {
                let sampler = NutsSampler::new();
                sampler.infer(&net_ref.inner, &ev_ref.inner)
            }
            "gaussian" | "moment_matching" => {
                use nxuskit_engine::providers::bayesian::MomentMatchingInference;
                let mm = MomentMatchingInference::new();
                mm.infer(&net_ref.inner, &ev_ref.inner)
            }
            _ => {
                error::set_last_error(
                    "bn_error",
                    &format!("Unknown algorithm: '{algo_str}'. Valid: ve, jt, gibbs, lbp, nuts, hmc, gaussian"),
                    None,
                );
                return std::ptr::null_mut();
            }
        };
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;

        match result {
            Ok(inference_result) => {
                convert_inference_result(inference_result, &net_ref.inner, algo_str, elapsed_ms)
            }
            Err(e) => {
                error::set_last_error("bn_error", &format!("Inference failed: {e}"), None);
                std::ptr::null_mut()
            }
        }
    }))
    .unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in nxuskit_bn_infer", None);
        std::ptr::null_mut()
    })
}

/// Convert an InferenceResult to a NxuskitBnResult, handling both discrete and continuous marginals.
fn convert_inference_result(
    inference_result: nxuskit_engine::providers::bayesian::InferenceResult,
    network: &BayesianNetwork,
    algorithm: &str,
    elapsed_ms: f64,
) -> *mut NxuskitBnResult {
    let mut marginals = HashMap::new();
    let mut variables = Vec::new();
    let mut continuous_marginals = HashMap::new();

    // Discrete marginals
    for (var_name, probs) in &inference_result.marginals {
        if let Some(var) = network.variable(var_name) {
            let mut state_map = HashMap::new();
            for (i, state) in var.states.iter().enumerate() {
                state_map.insert(state.to_string(), probs[i]);
            }
            variables.push(var_name.to_string());
            marginals.insert(var_name.to_string(), state_map);
        }
    }

    // Continuous marginals
    for (var_name, cm) in &inference_result.continuous_marginals {
        variables.push(var_name.to_string());
        continuous_marginals.insert(
            var_name.to_string(),
            ContinuousMarginalResult {
                mean: cm.mean,
                variance: cm.variance,
                ci_lower: cm.ci_lower,
                ci_upper: cm.ci_upper,
            },
        );
    }

    variables.sort();
    variables.dedup();

    Box::into_raw(Box::new(NxuskitBnResult {
        marginals,
        continuous_marginals,
        variables,
        cursor: 0,
        algorithm: algorithm.to_string(),
        elapsed_ms,
    }))
}

// ── Result access ────────────────────────────────────────────────

/// Destroy a result handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_bn_result_destroy(result: *mut NxuskitBnResult) {
    if !result.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            drop(unsafe { Box::from_raw(result) });
        }));
    }
}

/// Get full result as JSON. Caller must free with nxuskit_free_string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_bn_result_json(result: *const NxuskitBnResult) -> *mut c_char {
    catch_unwind(AssertUnwindSafe(|| {
        error::clear_last_error();
        if result.is_null() {
            error::set_last_error("invalid_argument", "result is NULL", None);
            return std::ptr::null_mut();
        }
        let r = unsafe { &*result };
        // Build continuous marginals for JSON output
        let cm_json: HashMap<String, serde_json::Value> = r
            .continuous_marginals
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    serde_json::json!({
                        "mean": v.mean,
                        "variance": v.variance,
                        "ci_lower": v.ci_lower,
                        "ci_upper": v.ci_upper,
                    }),
                )
            })
            .collect();
        let output = serde_json::json!({
            "marginals": r.marginals,
            "continuous_marginals": cm_json,
            "algorithm": r.algorithm,
            "elapsed_ms": r.elapsed_ms,
            "num_variables": r.variables.len(),
        });
        match serde_json::to_string(&output) {
            Ok(json) => CString::new(json)
                .map(|c| c.into_raw())
                .unwrap_or(std::ptr::null_mut()),
            Err(e) => {
                error::set_last_error("internal_error", &format!("Serialization error: {e}"), None);
                std::ptr::null_mut()
            }
        }
    }))
    .unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in nxuskit_bn_result_json", None);
        std::ptr::null_mut()
    })
}

/// Query a single variable's posterior. Returns JSON object {state: probability}.
/// Caller must free with nxuskit_free_string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_bn_result_query(
    result: *const NxuskitBnResult,
    variable: *const c_char,
) -> *mut c_char {
    catch_unwind(AssertUnwindSafe(|| {
        error::clear_last_error();
        if result.is_null() {
            error::set_last_error("invalid_argument", "result is NULL", None);
            return std::ptr::null_mut();
        }
        let var_str = match unsafe { c_str_to_str(variable, "variable") } {
            Some(s) => s,
            None => return std::ptr::null_mut(),
        };
        let r = unsafe { &*result };
        match r.marginals.get(var_str) {
            Some(dist) => match serde_json::to_string(dist) {
                Ok(json) => CString::new(json)
                    .map(|c| c.into_raw())
                    .unwrap_or(std::ptr::null_mut()),
                Err(_) => std::ptr::null_mut(),
            },
            None => {
                error::set_last_error(
                    "bn_error",
                    &format!("Variable '{var_str}' not in results (may be observed)"),
                    None,
                );
                std::ptr::null_mut()
            }
        }
    }))
    .unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in nxuskit_bn_result_query", None);
        std::ptr::null_mut()
    })
}

/// Get the number of variables in the result.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_bn_result_num_variables(result: *const NxuskitBnResult) -> i32 {
    catch_unwind(AssertUnwindSafe(|| {
        if result.is_null() {
            return -1;
        }
        let r = unsafe { &*result };
        r.variables.len() as i32
    }))
    .unwrap_or(-1)
}

/// Iterate: get the next variable name. Returns NULL when done.
/// Caller must free with nxuskit_free_string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_bn_result_next(result: *mut NxuskitBnResult) -> *mut c_char {
    catch_unwind(AssertUnwindSafe(|| {
        if result.is_null() {
            return std::ptr::null_mut();
        }
        let r = unsafe { &mut *result };
        if r.cursor >= r.variables.len() {
            return std::ptr::null_mut();
        }
        let name = &r.variables[r.cursor];
        r.cursor += 1;
        CString::new(name.as_str())
            .map(|c| c.into_raw())
            .unwrap_or(std::ptr::null_mut())
    }))
    .unwrap_or(std::ptr::null_mut())
}

/// Reset the iteration cursor to the beginning.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_bn_result_reset(result: *mut NxuskitBnResult) {
    if !result.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            let r = unsafe { &mut *result };
            r.cursor = 0;
        }));
    }
}

// ── Part 2: BIF export ──────────────────────────────────────────

/// Save a BayesianNetwork to a BIF file. Returns true on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_bn_net_save_file(
    net: *const NxuskitBnNet,
    path: *const c_char,
) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        error::clear_last_error();
        if net.is_null() {
            error::set_last_error("invalid_argument", "net is NULL", None);
            return false;
        }
        let path_str = match unsafe { c_str_to_str(path, "path") } {
            Some(s) => s,
            None => return false,
        };
        let net_ref = unsafe { &*net };
        match save_bif_file(&net_ref.inner, Path::new(path_str)) {
            Ok(()) => true,
            Err(e) => {
                error::set_last_error("bn_error", &format!("Failed to save BIF: {e}"), None);
                false
            }
        }
    }))
    .unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in nxuskit_bn_net_save_file", None);
        false
    })
}

// ── Part 2: Continuous evidence ─────────────────────────────────

/// Set a continuous observation for a Gaussian variable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_bn_ev_set_continuous(
    ev: *mut NxuskitBnEvidence,
    net: *const NxuskitBnNet,
    variable: *const c_char,
    value: f64,
) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        error::clear_last_error();
        if ev.is_null() {
            error::set_last_error("invalid_argument", "ev is NULL", None);
            return false;
        }
        if net.is_null() {
            error::set_last_error("invalid_argument", "net is NULL", None);
            return false;
        }
        let var_str = match unsafe { c_str_to_str(variable, "variable") } {
            Some(s) => s,
            None => return false,
        };
        let ev_ref = unsafe { &mut *ev };
        let net_ref = unsafe { &*net };
        let vn = match VariableName::new(var_str) {
            Ok(v) => v,
            Err(e) => {
                error::set_last_error("bn_error", &format!("Invalid variable name: {e}"), None);
                return false;
            }
        };
        match ev_ref.inner.observe_continuous(&net_ref.inner, &vn, value) {
            Ok(()) => true,
            Err(e) => {
                error::set_last_error("bn_error", &format!("Continuous evidence error: {e}"), None);
                false
            }
        }
    }))
    .unwrap_or_else(|_| {
        error::set_last_error(
            "internal_error",
            "Panic in nxuskit_bn_ev_set_continuous",
            None,
        );
        false
    })
}

// ── Part 2: Gaussian network construction ───────────────────────

/// Add a Gaussian variable to the network.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_bn_net_add_gaussian_variable(
    net: *mut NxuskitBnNet,
    name: *const c_char,
    mean_base: f64,
    variance: f64,
) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        error::clear_last_error();
        if net.is_null() {
            error::set_last_error("invalid_argument", "net is NULL", None);
            return false;
        }
        let name_str = match unsafe { c_str_to_str(name, "name") } {
            Some(s) => s,
            None => return false,
        };
        let net_ref = unsafe { &mut *net };
        let gv = match GaussianVariable::new(name_str, mean_base, variance) {
            Ok(v) => v,
            Err(e) => {
                error::set_last_error("bn_error", &format!("Invalid Gaussian variable: {e}"), None);
                return false;
            }
        };
        match net_ref.inner.add_gaussian_variable(gv) {
            Ok(()) => true,
            Err(e) => {
                error::set_last_error(
                    "bn_error",
                    &format!("Failed to add Gaussian variable: {e}"),
                    None,
                );
                false
            }
        }
    }))
    .unwrap_or_else(|_| {
        error::set_last_error(
            "internal_error",
            "Panic in nxuskit_bn_net_add_gaussian_variable",
            None,
        );
        false
    })
}

/// Set a parent weight for a Gaussian variable's conditional distribution.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_bn_net_set_gaussian_weight(
    net: *mut NxuskitBnNet,
    variable: *const c_char,
    parent: *const c_char,
    weight: f64,
) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        error::clear_last_error();
        if net.is_null() {
            error::set_last_error("invalid_argument", "net is NULL", None);
            return false;
        }
        let var_str = match unsafe { c_str_to_str(variable, "variable") } {
            Some(s) => s,
            None => return false,
        };
        let parent_str = match unsafe { c_str_to_str(parent, "parent") } {
            Some(s) => s,
            None => return false,
        };
        let net_ref = unsafe { &mut *net };
        let vn = match VariableName::new(var_str) {
            Ok(v) => v,
            Err(e) => {
                error::set_last_error("bn_error", &format!("Invalid variable name: {e}"), None);
                return false;
            }
        };
        // Look up the existing Gaussian variable, add the weight, and re-add with edge.
        let gaussian_vars = net_ref.inner.gaussian_variables();
        let gv = match gaussian_vars.get(&vn) {
            Some(existing) => existing.clone(),
            None => {
                error::set_last_error(
                    "bn_error",
                    &format!("Variable '{var_str}' is not a Gaussian variable"),
                    None,
                );
                return false;
            }
        };
        let updated = match gv.with_weight(parent_str, weight) {
            Ok(v) => v,
            Err(e) => {
                error::set_last_error("bn_error", &format!("Failed to set weight: {e}"), None);
                return false;
            }
        };
        // Update the Gaussian variable in the network.
        match net_ref.inner.update_gaussian_variable(updated) {
            Ok(()) => {
                // Also add the edge.
                let parent_vn = match VariableName::new(parent_str) {
                    Ok(v) => v,
                    Err(e) => {
                        error::set_last_error(
                            "bn_error",
                            &format!("Invalid parent name: {e}"),
                            None,
                        );
                        return false;
                    }
                };
                match net_ref.inner.add_edge(&parent_vn, &vn) {
                    Ok(()) | Err(_) => true, // Edge may already exist
                }
            }
            Err(e) => {
                error::set_last_error(
                    "bn_error",
                    &format!("Failed to update Gaussian variable: {e}"),
                    None,
                );
                false
            }
        }
    }))
    .unwrap_or_else(|_| {
        error::set_last_error(
            "internal_error",
            "Panic in nxuskit_bn_net_set_gaussian_weight",
            None,
        );
        false
    })
}

// ── Part 2: Continuous marginal access ──────────────────────────

/// Get the posterior mean for a continuous variable. Returns NaN on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_bn_result_mean(
    result: *const NxuskitBnResult,
    variable: *const c_char,
) -> f64 {
    catch_unwind(AssertUnwindSafe(|| {
        if result.is_null() {
            error::set_last_error("invalid_argument", "result is NULL", None);
            return f64::NAN;
        }
        let var_str = match unsafe { c_str_to_str(variable, "variable") } {
            Some(s) => s,
            None => return f64::NAN,
        };
        let r = unsafe { &*result };
        match r.continuous_marginals.get(var_str) {
            Some(cm) => cm.mean,
            None => {
                error::set_last_error(
                    "bn_error",
                    &format!("No continuous marginal for '{var_str}'"),
                    None,
                );
                f64::NAN
            }
        }
    }))
    .unwrap_or(f64::NAN)
}

/// Get the posterior variance for a continuous variable. Returns NaN on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_bn_result_variance(
    result: *const NxuskitBnResult,
    variable: *const c_char,
) -> f64 {
    catch_unwind(AssertUnwindSafe(|| {
        if result.is_null() {
            error::set_last_error("invalid_argument", "result is NULL", None);
            return f64::NAN;
        }
        let var_str = match unsafe { c_str_to_str(variable, "variable") } {
            Some(s) => s,
            None => return f64::NAN,
        };
        let r = unsafe { &*result };
        match r.continuous_marginals.get(var_str) {
            Some(cm) => cm.variance,
            None => {
                error::set_last_error(
                    "bn_error",
                    &format!("No continuous marginal for '{var_str}'"),
                    None,
                );
                f64::NAN
            }
        }
    }))
    .unwrap_or(f64::NAN)
}

/// Get the full continuous marginal as JSON: {"mean": ..., "variance": ..., "ci_lower": ..., "ci_upper": ...}.
/// Caller must free with nxuskit_free_string. Returns NULL on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_bn_result_continuous_marginal(
    result: *const NxuskitBnResult,
    variable: *const c_char,
) -> *mut c_char {
    catch_unwind(AssertUnwindSafe(|| {
        error::clear_last_error();
        if result.is_null() {
            error::set_last_error("invalid_argument", "result is NULL", None);
            return std::ptr::null_mut();
        }
        let var_str = match unsafe { c_str_to_str(variable, "variable") } {
            Some(s) => s,
            None => return std::ptr::null_mut(),
        };
        let r = unsafe { &*result };
        match r.continuous_marginals.get(var_str) {
            Some(cm) => {
                let json = serde_json::json!({
                    "mean": cm.mean,
                    "variance": cm.variance,
                    "ci_lower": cm.ci_lower,
                    "ci_upper": cm.ci_upper,
                });
                match serde_json::to_string(&json) {
                    Ok(s) => CString::new(s)
                        .map(|c| c.into_raw())
                        .unwrap_or(std::ptr::null_mut()),
                    Err(_) => std::ptr::null_mut(),
                }
            }
            None => {
                error::set_last_error(
                    "bn_error",
                    &format!("No continuous marginal for '{var_str}'"),
                    None,
                );
                std::ptr::null_mut()
            }
        }
    }))
    .unwrap_or_else(|_| {
        error::set_last_error(
            "internal_error",
            "Panic in nxuskit_bn_result_continuous_marginal",
            None,
        );
        std::ptr::null_mut()
    })
}

// ── Part 2: Algorithm-specific inference ────────────────────────

/// Run inference with a JSON configuration object for algorithm-specific parameters.
///
/// `algorithm`: "ve", "jt", "gibbs", "lbp", "nuts", "hmc", "gaussian".
/// `config_json`: JSON string with algorithm-specific parameters. NULL for defaults.
///
/// VE config: {"elimination_heuristic": "min_fill"} or {"elimination_heuristic": "min_weight"}
/// LBP config: {"max_iterations": 100, "convergence_threshold": 1e-4, "damping_factor": 0.5}
/// NUTS config: {"num_samples": 1000, "num_warmup": 500, "max_tree_depth": 10, "seed": 42, "num_chains": 4}
/// Gibbs config: {"num_samples": 10000, "burn_in": 1000, "seed": 42}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_bn_infer_with_config(
    net: *const NxuskitBnNet,
    ev: *const NxuskitBnEvidence,
    algorithm: *const c_char,
    config_json: *const c_char,
) -> *mut NxuskitBnResult {
    catch_unwind(AssertUnwindSafe(|| {
        error::clear_last_error();
        if net.is_null() {
            error::set_last_error("invalid_argument", "net is NULL", None);
            return std::ptr::null_mut();
        }
        if ev.is_null() {
            error::set_last_error("invalid_argument", "ev is NULL", None);
            return std::ptr::null_mut();
        }
        let algo_str = match unsafe { c_str_to_str(algorithm, "algorithm") } {
            Some(s) => s,
            None => return std::ptr::null_mut(),
        };
        let config_str = if config_json.is_null() {
            None
        } else {
            unsafe { c_str_to_str(config_json, "config_json") }
        };
        let net_ref = unsafe { &*net };
        let ev_ref = unsafe { &*ev };

        let start = std::time::Instant::now();
        let result = match algo_str {
            "ve" | "variable_elimination" => {
                let ve = parse_ve_heuristic(config_str);
                ve.infer(&net_ref.inner, &ev_ref.inner)
            }
            "jt" | "junction_tree" => {
                JunctionTree::new().infer(&net_ref.inner, &ev_ref.inner)
            }
            "gibbs" => {
                let (ns, bi, seed) = parse_gibbs_config(config_str);
                let mut gibbs = GibbsSampler::new(ns, bi);
                if seed != 0 {
                    gibbs = gibbs.with_seed(seed);
                }
                gibbs.infer(&net_ref.inner, &ev_ref.inner)
            }
            "lbp" | "loopy_bp" => {
                let lbp = build_lbp_from_config(config_str);
                lbp.infer(&net_ref.inner, &ev_ref.inner)
            }
            "nuts" | "hmc" => {
                let nuts_config = parse_nuts_config(config_str);
                NutsSampler::with_config(nuts_config).infer(&net_ref.inner, &ev_ref.inner)
            }
            "gaussian" | "moment_matching" => {
                use nxuskit_engine::providers::bayesian::MomentMatchingInference;
                MomentMatchingInference::new().infer(&net_ref.inner, &ev_ref.inner)
            }
            _ => {
                error::set_last_error(
                    "bn_error",
                    &format!("Unknown algorithm: '{algo_str}'. Valid: ve, jt, gibbs, lbp, nuts, hmc, gaussian"),
                    None,
                );
                return std::ptr::null_mut();
            }
        };
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;

        match result {
            Ok(inference_result) => {
                convert_inference_result(inference_result, &net_ref.inner, algo_str, elapsed_ms)
            }
            Err(e) => {
                error::set_last_error("bn_error", &format!("Inference failed: {e}"), None);
                std::ptr::null_mut()
            }
        }
    }))
    .unwrap_or_else(|_| {
        error::set_last_error(
            "internal_error",
            "Panic in nxuskit_bn_infer_with_config",
            None,
        );
        std::ptr::null_mut()
    })
}

/// Parse VE elimination heuristic from JSON config.
///
/// Recognizes `{"elimination_heuristic": "min_fill"}` or `{"elimination_heuristic": "min_weight"}`.
/// Falls back to `MinFill` (default) when not specified or unrecognized.
fn parse_ve_heuristic(config_str: Option<&str>) -> VariableElimination {
    if let Some(s) = config_str
        && let Ok(v) = serde_json::from_str::<serde_json::Value>(s)
        && let Some(h) = v.get("elimination_heuristic").and_then(|x| x.as_str())
        && h == "min_weight"
    {
        return VariableElimination::with_heuristic(EliminationHeuristic::MinWeight);
    }
    // "min_fill" or anything else → default
    VariableElimination::new()
}

/// Parse Gibbs config from JSON.
fn parse_gibbs_config(config_str: Option<&str>) -> (usize, usize, u64) {
    let mut ns = 10_000usize;
    let mut bi = 1_000usize;
    let mut seed = 0u64;
    if let Some(s) = config_str
        && let Ok(v) = serde_json::from_str::<serde_json::Value>(s)
    {
        if let Some(n) = v.get("num_samples").and_then(|x| x.as_u64()) {
            ns = n as usize;
        }
        if let Some(n) = v.get("burn_in").and_then(|x| x.as_u64()) {
            bi = n as usize;
        }
        if let Some(n) = v.get("seed").and_then(|x| x.as_u64()) {
            seed = n;
        }
    }
    (ns, bi, seed)
}

/// Build LBP engine from JSON config using builder pattern.
fn build_lbp_from_config(config_str: Option<&str>) -> LoopyBeliefPropagation {
    let mut lbp = LoopyBeliefPropagation::new();
    if let Some(s) = config_str
        && let Ok(v) = serde_json::from_str::<serde_json::Value>(s)
    {
        if let Some(n) = v.get("max_iterations").and_then(|x| x.as_u64()) {
            lbp = lbp.max_iterations(n as usize);
        }
        if let Some(n) = v.get("convergence_threshold").and_then(|x| x.as_f64()) {
            lbp = lbp.convergence_threshold(n);
        }
        if let Some(n) = v.get("damping_factor").and_then(|x| x.as_f64()) {
            lbp = lbp.damping(n);
        }
    }
    lbp
}

/// Parse NUTS config from JSON.
fn parse_nuts_config(config_str: Option<&str>) -> NUTSConfig {
    let mut config = NUTSConfig::default();
    if let Some(s) = config_str
        && let Ok(v) = serde_json::from_str::<serde_json::Value>(s)
    {
        if let Some(n) = v.get("num_samples").and_then(|x| x.as_u64()) {
            config.num_samples = n;
        }
        if let Some(n) = v.get("num_warmup").and_then(|x| x.as_u64()) {
            config.num_warmup = n;
        }
        if let Some(n) = v.get("max_tree_depth").and_then(|x| x.as_u64()) {
            config.max_tree_depth = n;
        }
        if let Some(n) = v.get("seed").and_then(|x| x.as_u64()) {
            config.seed = n;
        }
        if let Some(n) = v.get("num_chains").and_then(|x| x.as_u64()) {
            config.num_chains = n as usize;
        }
    }
    config
}

// ── Streaming inference ─────────────────────────────────────────

/// Callback for each streaming inference chunk.
/// `chunk_json` is a NUL-terminated JSON string (valid for the duration of the call).
/// `iteration` is the current sample count, `total` is the target.
/// `is_final` is true for the last chunk.
/// Return `true` to continue, `false` to cancel.
pub type NxuskitBnStreamCallback = unsafe extern "C" fn(
    chunk_json: *const c_char,
    iteration: u32,
    total: u32,
    is_final: bool,
    user_data: *mut std::ffi::c_void,
) -> bool;

/// Run streaming Gibbs inference with callback-based delivery.
///
/// Calls `on_chunk` for each progressive inference result. The chunk JSON has the
/// same format as `nxuskit_bn_result_json` plus `iteration`, `total_iterations`,
/// `convergence_metric`, and `is_final` fields.
///
/// Returns `true` on success (all chunks delivered or cancelled by callback).
/// Returns `false` on error (check `nxuskit_last_error`).
///
/// `chunk_size` controls how many Gibbs samples between callbacks (0 = default 1000).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_bn_infer_stream(
    net: *const NxuskitBnNet,
    ev: *const NxuskitBnEvidence,
    num_samples: u32,
    burn_in: u32,
    seed: u64,
    chunk_size: u32,
    on_chunk: NxuskitBnStreamCallback,
    user_data: *mut std::ffi::c_void,
) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        error::clear_last_error();
        if net.is_null() {
            error::set_last_error("invalid_argument", "net is NULL", None);
            return false;
        }
        if ev.is_null() {
            error::set_last_error("invalid_argument", "ev is NULL", None);
            return false;
        }

        let net_ref = unsafe { &*net };
        let ev_ref = unsafe { &*ev };
        let ns = if num_samples == 0 {
            10_000
        } else {
            num_samples as usize
        };
        let bi = if burn_in == 0 {
            1_000
        } else {
            burn_in as usize
        };
        let cs = if chunk_size == 0 {
            1_000
        } else {
            chunk_size as usize
        };

        let mut gibbs = GibbsSampler::new(ns, bi);
        if seed != 0 {
            gibbs = gibbs.with_seed(seed);
        }

        // sample_stream uses tokio::spawn internally, so we need a runtime.
        // Create a multi-threaded runtime so the background sampling task can
        // run on a worker thread while we consume chunks on this thread.
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build();
        let rt = match rt {
            Ok(r) => r,
            Err(e) => {
                error::set_last_error(
                    "internal_error",
                    &format!("Failed to create tokio runtime: {e}"),
                    None,
                );
                return false;
            }
        };

        // Enter the runtime so tokio::spawn works inside sample_stream.
        let _guard = rt.enter();

        let stream = match gibbs.sample_stream(&net_ref.inner, &ev_ref.inner, cs) {
            Ok(s) => s,
            Err(e) => {
                error::set_last_error("bn_error", &format!("Gibbs streaming failed: {e}"), None);
                return false;
            }
        };

        // Consume chunks via blocking_iter (uses the runtime we entered).
        let user_data_ptr = user_data;
        for chunk in stream.blocking_iter() {
            let chunk_json = build_stream_chunk_json(&chunk, &net_ref.inner);
            let json_cstr = match CString::new(chunk_json) {
                Ok(c) => c,
                Err(_) => {
                    error::set_last_error("internal_error", "NUL byte in chunk JSON", None);
                    return false;
                }
            };

            let should_continue = unsafe {
                on_chunk(
                    json_cstr.as_ptr(),
                    chunk.iteration as u32,
                    chunk.total_iterations as u32,
                    chunk.is_final,
                    user_data_ptr,
                )
            };

            if !should_continue {
                return true; // cancelled by callback — not an error
            }
        }

        true
    }))
    .unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in nxuskit_bn_infer_stream", None);
        false
    })
}

// ── Parameter learning ───────────────────────────────────────────

/// Learn CPT parameters from a CSV dataset using Maximum Likelihood Estimation.
///
/// Loads the CSV at `csv_path`, fits CPTs on the network using MLE with
/// Laplace smoothing (`pseudocount`; 0.0 = no smoothing, 1.0 = default).
/// The network is modified in-place with learned parameters.
///
/// Returns `true` on success, `false` on error (check `nxuskit_last_error`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_bn_learn_mle(
    net: *mut NxuskitBnNet,
    csv_path: *const c_char,
    pseudocount: f64,
) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        error::clear_last_error();
        if net.is_null() {
            error::set_last_error("invalid_argument", "net is NULL", None);
            return false;
        }
        let path_str = match unsafe { c_str_to_str(csv_path, "csv_path") } {
            Some(s) => s,
            None => return false,
        };

        let net_ref = unsafe { &mut *net };
        let data = match Dataset::from_csv(Path::new(path_str), &net_ref.inner) {
            Ok(d) => d,
            Err(e) => {
                error::set_last_error(
                    "bn_error",
                    &format!("Failed to load CSV '{}': {}", path_str, e),
                    None,
                );
                return false;
            }
        };

        let pc = if pseudocount == 0.0 { 0.0 } else { pseudocount };
        let learner = MleLearner::new(MleConfig {
            pseudocount: pc,
            ..Default::default()
        });

        match learner.fit(&mut net_ref.inner, &data) {
            Ok(()) => true,
            Err(e) => {
                error::set_last_error("bn_error", &format!("MLE learning failed: {}", e), None);
                false
            }
        }
    }))
    .unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in nxuskit_bn_learn_mle", None);
        false
    })
}

/// Compute log-likelihood of data given the current network CPTs.
///
/// Returns the log-likelihood value, or `f64::NEG_INFINITY` on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_bn_log_likelihood(
    net: *const NxuskitBnNet,
    csv_path: *const c_char,
) -> f64 {
    catch_unwind(AssertUnwindSafe(|| {
        error::clear_last_error();
        if net.is_null() {
            error::set_last_error("invalid_argument", "net is NULL", None);
            return f64::NEG_INFINITY;
        }
        let path_str = match unsafe { c_str_to_str(csv_path, "csv_path") } {
            Some(s) => s,
            None => return f64::NEG_INFINITY,
        };

        let net_ref = unsafe { &*net };
        let data = match Dataset::from_csv(Path::new(path_str), &net_ref.inner) {
            Ok(d) => d,
            Err(e) => {
                error::set_last_error(
                    "bn_error",
                    &format!("Failed to load CSV '{}': {}", path_str, e),
                    None,
                );
                return f64::NEG_INFINITY;
            }
        };

        let learner = MleLearner::with_defaults();
        match learner.log_likelihood(&net_ref.inner, &data) {
            Ok(ll) => ll,
            Err(e) => {
                error::set_last_error("bn_error", &format!("Log-likelihood failed: {}", e), None);
                f64::NEG_INFINITY
            }
        }
    }))
    .unwrap_or_else(|_| {
        error::set_last_error("internal_error", "Panic in nxuskit_bn_log_likelihood", None);
        f64::NEG_INFINITY
    })
}

// ── Structure learning helpers ────────────────────────────────────

/// Auto-populate a network's variables from CSV column headers and
/// unique cell values. Used when search_structure is called on an empty
/// network so the user doesn't need to pre-define variables.
fn auto_populate_from_csv(csv_path: &Path, network: &mut BayesianNetwork) -> Result<(), String> {
    use std::collections::BTreeSet;
    use std::io::BufRead;

    let file = std::fs::File::open(csv_path)
        .map_err(|e| format!("Failed to open CSV '{}': {}", csv_path.display(), e))?;
    let reader = std::io::BufReader::new(file);
    let mut lines = reader.lines();

    // Read header line
    let header_line = lines
        .next()
        .ok_or_else(|| "CSV is empty".to_string())?
        .map_err(|e| format!("Failed to read CSV header: {}", e))?;
    let headers: Vec<String> = header_line
        .split(',')
        .map(|h| h.trim().to_string())
        .collect();

    if headers.is_empty() {
        return Err("CSV has no columns".to_string());
    }

    // Discover unique states per column (sorted for determinism)
    let mut states_per_col: Vec<BTreeSet<String>> = vec![BTreeSet::new(); headers.len()];

    for line_result in lines {
        let line = line_result.map_err(|e| format!("CSV row error: {}", e))?;
        for (i, cell) in line.split(',').enumerate() {
            let val = cell.trim();
            if !val.is_empty() && val != "?" && i < states_per_col.len() {
                states_per_col[i].insert(val.to_string());
            }
        }
    }

    // Add each column as a discrete variable with discovered states
    for (i, col_name) in headers.iter().enumerate() {
        let states: Vec<nxuskit_engine::providers::bayesian::types::StateName> = states_per_col[i]
            .iter()
            .map(|s| nxuskit_engine::providers::bayesian::types::StateName::new(s.as_str()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Invalid state name in column '{}': {}", col_name, e))?;
        if states.is_empty() {
            return Err(format!("Column '{}' has no non-empty values", col_name));
        }
        let var_name = VariableName::new(col_name)
            .map_err(|e| format!("Invalid variable name '{}': {}", col_name, e))?;
        let var =
            nxuskit_engine::providers::bayesian::types::DiscreteVariable::new(var_name, states)
                .map_err(|e| format!("Failed to create variable '{}': {}", col_name, e))?;
        network
            .add_variable(var)
            .map_err(|e| format!("Failed to add variable '{}': {}", col_name, e))?;
    }

    log::info!(
        "Auto-populated {} variables from CSV headers: {:?}",
        headers.len(),
        headers
    );
    Ok(())
}

// ── Structure learning ────────────────────────────────────────────

/// Run structure learning on a network given CSV data.
///
/// `algorithm`: "hill_climb" or "k2".
/// `scoring`: "bic" or "bdeu".
/// `csv_path`: path to CSV data file.
/// `max_parents`: maximum parents per node (0 = default: 5 for hill_climb, 3 for k2).
/// `max_steps`: maximum search steps for hill_climb (0 = default 1000). Ignored for k2.
/// `ess`: equivalent sample size for BDeu (0.0 = default 10.0). Ignored for BIC.
/// `ordering_json`: JSON array of variable names for K2 ordering. NULL for hill_climb.
///
/// Returns a JSON string with the search results (edges, score, iterations).
/// Caller must free with `nxuskit_free_string`. Returns NULL on error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_bn_search_structure(
    net: *const NxuskitBnNet,
    csv_path: *const c_char,
    algorithm: *const c_char,
    scoring: *const c_char,
    max_parents: u32,
    max_steps: u32,
    ess: f64,
    ordering_json: *const c_char,
) -> *mut c_char {
    catch_unwind(AssertUnwindSafe(|| {
        error::clear_last_error();
        if net.is_null() {
            error::set_last_error("invalid_argument", "net is NULL", None);
            return std::ptr::null_mut();
        }
        let path_str = match unsafe { c_str_to_str(csv_path, "csv_path") } {
            Some(s) => s,
            None => return std::ptr::null_mut(),
        };
        let algo_str = match unsafe { c_str_to_str(algorithm, "algorithm") } {
            Some(s) => s,
            None => return std::ptr::null_mut(),
        };
        let scoring_str = match unsafe { c_str_to_str(scoring, "scoring") } {
            Some(s) => s,
            None => return std::ptr::null_mut(),
        };

        let net_ref = unsafe { &mut *net.cast_mut() };

        // If the network is empty (no variables), auto-populate variables
        // and states from CSV headers and unique cell values. This enables
        // structure learning from scratch without requiring a pre-defined
        // network (the common use case for search_structure).
        if net_ref.inner.variables().is_empty()
            && let Err(e) = auto_populate_from_csv(Path::new(path_str), &mut net_ref.inner)
        {
            error::set_last_error(
                "bn_error",
                &format!("Failed to auto-populate variables from CSV: {}", e),
                None,
            );
            return std::ptr::null_mut();
        }

        // Load dataset
        let data = match Dataset::from_csv(Path::new(path_str), &net_ref.inner) {
            Ok(d) => d,
            Err(e) => {
                error::set_last_error(
                    "bn_error",
                    &format!("Failed to load CSV '{}': {}", path_str, e),
                    None,
                );
                return std::ptr::null_mut();
            }
        };

        // Parse scoring function
        let scoring_fn = match scoring_str {
            "bic" => ScoringFunction::BIC,
            "bdeu" => {
                let effective_ess = if ess <= 0.0 { 10.0 } else { ess };
                ScoringFunction::bdeu_with_ess(effective_ess)
            }
            _ => {
                error::set_last_error(
                    "bn_error",
                    &format!("Unknown scoring: '{}'. Valid: bic, bdeu", scoring_str),
                    None,
                );
                return std::ptr::null_mut();
            }
        };

        // Run structure learning
        let search_result = match algo_str {
            "hill_climb" => {
                let mp = if max_parents == 0 {
                    5
                } else {
                    max_parents as usize
                };
                let ms = if max_steps == 0 {
                    1000
                } else {
                    max_steps as usize
                };
                let config = HillClimbConfig {
                    scoring: scoring_fn,
                    max_steps: ms,
                    max_parents: mp,
                    ..Default::default()
                };
                let learner = HillClimbLearner::new(config);
                learner.search(&net_ref.inner, &data)
            }
            "k2" => {
                // Parse ordering from JSON
                let ordering_str = match unsafe { c_str_to_str(ordering_json, "ordering_json") } {
                    Some(s) => s,
                    None => {
                        error::set_last_error(
                            "bn_error",
                            "K2 requires ordering_json parameter",
                            None,
                        );
                        return std::ptr::null_mut();
                    }
                };
                let ordering: Vec<String> = match serde_json::from_str(ordering_str) {
                    Ok(o) => o,
                    Err(e) => {
                        error::set_last_error(
                            "bn_error",
                            &format!("Invalid ordering JSON: {}", e),
                            None,
                        );
                        return std::ptr::null_mut();
                    }
                };
                let mp = if max_parents == 0 {
                    3
                } else {
                    max_parents as usize
                };
                let config = K2Config {
                    ordering,
                    max_parents: mp,
                    scoring: scoring_fn,
                };
                let learner = K2Learner::new(config);
                learner.search(&net_ref.inner, &data)
            }
            _ => {
                error::set_last_error(
                    "bn_error",
                    &format!("Unknown algorithm: '{}'. Valid: hill_climb, k2", algo_str),
                    None,
                );
                return std::ptr::null_mut();
            }
        };

        match search_result {
            Ok(result) => {
                // Build edges list
                let mut edges: Vec<(String, String)> = Vec::new();
                for vn in result.network.variable_names() {
                    for parent in result.network.parents(&vn) {
                        edges.push((parent.to_string(), vn.to_string()));
                    }
                }

                let output = serde_json::json!({
                    "algorithm": algo_str,
                    "scoring": scoring_str,
                    "score": result.score,
                    "iterations": result.iterations,
                    "num_edges": edges.len(),
                    "edges": edges,
                    "num_variables": result.network.num_variables(),
                });

                match serde_json::to_string(&output) {
                    Ok(json) => CString::new(json)
                        .map(|c| c.into_raw())
                        .unwrap_or(std::ptr::null_mut()),
                    Err(e) => {
                        error::set_last_error(
                            "internal_error",
                            &format!("Serialization error: {}", e),
                            None,
                        );
                        std::ptr::null_mut()
                    }
                }
            }
            Err(e) => {
                error::set_last_error(
                    "bn_error",
                    &format!("Structure learning failed: {}", e),
                    None,
                );
                std::ptr::null_mut()
            }
        }
    }))
    .unwrap_or_else(|_| {
        error::set_last_error(
            "internal_error",
            "Panic in nxuskit_bn_search_structure",
            None,
        );
        std::ptr::null_mut()
    })
}

/// Build JSON for a streaming chunk.
fn build_stream_chunk_json(
    chunk: &BayesStreamChunk<nxuskit_engine::providers::bayesian::InferenceResult>,
    network: &BayesianNetwork,
) -> String {
    let mut marginals = HashMap::new();
    for (var_name, probs) in &chunk.data.marginals {
        if let Some(var) = network.variable(var_name) {
            let mut state_map = HashMap::new();
            for (i, state) in var.states.iter().enumerate() {
                state_map.insert(state.to_string(), probs[i]);
            }
            marginals.insert(var_name.to_string(), state_map);
        }
    }

    serde_json::json!({
        "marginals": marginals,
        "algorithm": chunk.data.algorithm,
        "elapsed_ms": chunk.data.elapsed.as_secs_f64() * 1000.0,
        "num_variables": marginals.len(),
        "iteration": chunk.iteration,
        "total_iterations": chunk.total_iterations,
        "convergence_metric": chunk.convergence_metric,
        "is_final": chunk.is_final,
    })
    .to_string()
}
