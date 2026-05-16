//! Safe Rust wrapper for the Bayesian Network C ABI.
//!
//! Provides RAII types: `BnNetwork`, `BnEvidence`, `BnResult`.

use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char};

use serde::Deserialize;

use crate::NxuskitError;
use crate::ffi;

/// Continuous marginal result with mean, variance, and confidence interval.
#[derive(Debug, Clone, Deserialize)]
pub struct ContinuousMarginal {
    pub mean: f64,
    pub variance: f64,
    pub ci_lower: f64,
    pub ci_upper: f64,
}

// ── Helper functions ─────────────────────────────────────────────

fn to_cstring(s: &str, param_name: &str) -> Result<CString, NxuskitError> {
    CString::new(s).map_err(|_| NxuskitError::Configuration {
        message: format!("{param_name} contains interior NUL byte"),
    })
}

#[cfg(feature = "static-link")]
unsafe fn last_error_ptr() -> *const c_char {
    unsafe { ffi::nxuskit_last_error() }
}

#[cfg(feature = "dynamic-link")]
unsafe fn last_error_ptr() -> *const c_char {
    let sdk = ffi::dynamic::sdk_unchecked();
    unsafe { (sdk.nxuskit_last_error)() }
}

#[cfg(feature = "static-link")]
unsafe fn free_string(ptr: *mut c_char) {
    unsafe { ffi::nxuskit_free_string(ptr) }
}

#[cfg(feature = "dynamic-link")]
unsafe fn free_string(ptr: *mut c_char) {
    let sdk = ffi::dynamic::sdk_unchecked();
    unsafe { (sdk.nxuskit_free_string)(ptr) }
}

fn last_error_or(fallback: &str) -> NxuskitError {
    let ptr = unsafe { last_error_ptr() };
    if ptr.is_null() {
        return NxuskitError::Internal {
            message: fallback.to_string(),
        };
    }
    let err_str = match unsafe { CStr::from_ptr(ptr) }.to_str() {
        Ok(s) if !s.is_empty() => s,
        _ => {
            return NxuskitError::Internal {
                message: fallback.to_string(),
            };
        }
    };
    NxuskitError::from_json_str(err_str)
}

/// Read a C string returned by the SDK, convert to owned String, free the C memory.
unsafe fn read_and_free_string(ptr: *mut c_char) -> Result<String, NxuskitError> {
    if ptr.is_null() {
        return Err(last_error_or("NULL string returned"));
    }
    let s = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|e| NxuskitError::Internal {
            message: format!("Invalid UTF-8: {e}"),
        })?
        .to_string();
    unsafe { free_string(ptr) };
    Ok(s)
}

// ── BnNetwork ────────────────────────────────────────────────────

/// A discrete Bayesian Network. Wraps the C ABI handle with RAII.
pub struct BnNetwork {
    handle: *mut ffi::NxuskitBnNet,
}

unsafe impl Send for BnNetwork {}

impl BnNetwork {
    // -- dispatch helpers --

    #[cfg(feature = "static-link")]
    unsafe fn call_create() -> *mut ffi::NxuskitBnNet {
        unsafe { ffi::nxuskit_bn_net_create() }
    }
    #[cfg(feature = "dynamic-link")]
    unsafe fn call_create() -> *mut ffi::NxuskitBnNet {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_bn_net_create)() }
    }

    #[cfg(feature = "static-link")]
    unsafe fn call_destroy(h: *mut ffi::NxuskitBnNet) {
        unsafe { ffi::nxuskit_bn_net_destroy(h) }
    }
    #[cfg(feature = "dynamic-link")]
    unsafe fn call_destroy(h: *mut ffi::NxuskitBnNet) {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_bn_net_destroy)(h) }
    }

    #[cfg(feature = "static-link")]
    unsafe fn call_load_file(path: *const c_char) -> *mut ffi::NxuskitBnNet {
        unsafe { ffi::nxuskit_bn_net_load_file(path) }
    }
    #[cfg(feature = "dynamic-link")]
    unsafe fn call_load_file(path: *const c_char) -> *mut ffi::NxuskitBnNet {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_bn_net_load_file)(path) }
    }

    #[cfg(feature = "static-link")]
    unsafe fn call_num_variables(h: *const ffi::NxuskitBnNet) -> i32 {
        unsafe { ffi::nxuskit_bn_net_num_variables(h) }
    }
    #[cfg(feature = "dynamic-link")]
    unsafe fn call_num_variables(h: *const ffi::NxuskitBnNet) -> i32 {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_bn_net_num_variables)(h) }
    }

    #[cfg(feature = "static-link")]
    unsafe fn call_variables(h: *const ffi::NxuskitBnNet) -> *mut c_char {
        unsafe { ffi::nxuskit_bn_net_variables(h) }
    }
    #[cfg(feature = "dynamic-link")]
    unsafe fn call_variables(h: *const ffi::NxuskitBnNet) -> *mut c_char {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_bn_net_variables)(h) }
    }

    #[cfg(feature = "static-link")]
    unsafe fn call_variable_states(h: *const ffi::NxuskitBnNet, v: *const c_char) -> *mut c_char {
        unsafe { ffi::nxuskit_bn_net_variable_states(h, v) }
    }
    #[cfg(feature = "dynamic-link")]
    unsafe fn call_variable_states(h: *const ffi::NxuskitBnNet, v: *const c_char) -> *mut c_char {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_bn_net_variable_states)(h, v) }
    }

    // -- public API --

    /// Create an empty Bayesian Network.
    pub fn create() -> Result<Self, NxuskitError> {
        let handle = unsafe { Self::call_create() };
        if handle.is_null() {
            return Err(last_error_or("failed to create BN"));
        }
        Ok(Self { handle })
    }

    /// Load a BIF file into a new network.
    pub fn load_file(path: &str) -> Result<Self, NxuskitError> {
        let c_path = to_cstring(path, "path")?;
        let handle = unsafe { Self::call_load_file(c_path.as_ptr()) };
        if handle.is_null() {
            return Err(last_error_or("failed to load BIF file"));
        }
        Ok(Self { handle })
    }

    /// Number of variables in the network.
    pub fn num_variables(&self) -> i32 {
        unsafe { Self::call_num_variables(self.handle) }
    }

    /// Get all variable names.
    pub fn variables(&self) -> Result<Vec<String>, NxuskitError> {
        let ptr = unsafe { Self::call_variables(self.handle) };
        let json = unsafe { read_and_free_string(ptr)? };
        serde_json::from_str(&json).map_err(|e| NxuskitError::Internal {
            message: format!("Failed to parse variables JSON: {e}"),
        })
    }

    /// Get states for a specific variable.
    pub fn variable_states(&self, variable: &str) -> Result<Vec<String>, NxuskitError> {
        let c_var = to_cstring(variable, "variable")?;
        let ptr = unsafe { Self::call_variable_states(self.handle, c_var.as_ptr()) };
        let json = unsafe { read_and_free_string(ptr)? };
        serde_json::from_str(&json).map_err(|e| NxuskitError::Internal {
            message: format!("Failed to parse states JSON: {e}"),
        })
    }

    /// Run inference with the given evidence and algorithm.
    pub fn infer(&self, evidence: &BnEvidence, algorithm: &str) -> Result<BnResult, NxuskitError> {
        self.infer_with_options(evidence, algorithm, 0, 0, 0)
    }

    /// Run inference with full options (for Gibbs sampling).
    pub fn infer_with_options(
        &self,
        evidence: &BnEvidence,
        algorithm: &str,
        num_samples: u32,
        burn_in: u32,
        seed: u64,
    ) -> Result<BnResult, NxuskitError> {
        let c_algo = to_cstring(algorithm, "algorithm")?;
        let handle = unsafe {
            Self::call_infer(
                self.handle,
                evidence.handle,
                c_algo.as_ptr(),
                num_samples,
                burn_in,
                seed,
            )
        };
        if handle.is_null() {
            return Err(last_error_or("inference failed"));
        }
        Ok(BnResult { handle })
    }

    /// Internal handle accessor for evidence binding.
    pub(crate) fn raw_handle(&self) -> *const ffi::NxuskitBnNet {
        self.handle
    }

    #[cfg(feature = "static-link")]
    unsafe fn call_infer(
        net: *const ffi::NxuskitBnNet,
        ev: *const ffi::NxuskitBnEvidence,
        algo: *const c_char,
        num_samples: u32,
        burn_in: u32,
        seed: u64,
    ) -> *mut ffi::NxuskitBnResult {
        unsafe { ffi::nxuskit_bn_infer(net, ev, algo, num_samples, burn_in, seed) }
    }
    #[cfg(feature = "dynamic-link")]
    unsafe fn call_infer(
        net: *const ffi::NxuskitBnNet,
        ev: *const ffi::NxuskitBnEvidence,
        algo: *const c_char,
        num_samples: u32,
        burn_in: u32,
        seed: u64,
    ) -> *mut ffi::NxuskitBnResult {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_bn_infer)(net, ev, algo, num_samples, burn_in, seed) }
    }

    // -- Part 2 dispatch helpers --

    #[cfg(feature = "static-link")]
    unsafe fn call_save_file(net: *const ffi::NxuskitBnNet, path: *const c_char) -> bool {
        unsafe { ffi::nxuskit_bn_net_save_file(net, path) }
    }
    #[cfg(feature = "dynamic-link")]
    unsafe fn call_save_file(net: *const ffi::NxuskitBnNet, path: *const c_char) -> bool {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_bn_net_save_file)(net, path) }
    }

    #[cfg(feature = "static-link")]
    unsafe fn call_add_gaussian_variable(
        net: *mut ffi::NxuskitBnNet,
        name: *const c_char,
        mean_base: f64,
        variance: f64,
    ) -> bool {
        unsafe { ffi::nxuskit_bn_net_add_gaussian_variable(net, name, mean_base, variance) }
    }
    #[cfg(feature = "dynamic-link")]
    unsafe fn call_add_gaussian_variable(
        net: *mut ffi::NxuskitBnNet,
        name: *const c_char,
        mean_base: f64,
        variance: f64,
    ) -> bool {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_bn_net_add_gaussian_variable)(net, name, mean_base, variance) }
    }

    #[cfg(feature = "static-link")]
    unsafe fn call_set_gaussian_weight(
        net: *mut ffi::NxuskitBnNet,
        variable: *const c_char,
        parent: *const c_char,
        weight: f64,
    ) -> bool {
        unsafe { ffi::nxuskit_bn_net_set_gaussian_weight(net, variable, parent, weight) }
    }
    #[cfg(feature = "dynamic-link")]
    unsafe fn call_set_gaussian_weight(
        net: *mut ffi::NxuskitBnNet,
        variable: *const c_char,
        parent: *const c_char,
        weight: f64,
    ) -> bool {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_bn_net_set_gaussian_weight)(net, variable, parent, weight) }
    }

    #[cfg(feature = "static-link")]
    unsafe fn call_infer_with_config(
        net: *const ffi::NxuskitBnNet,
        ev: *const ffi::NxuskitBnEvidence,
        algorithm: *const c_char,
        config_json: *const c_char,
    ) -> *mut ffi::NxuskitBnResult {
        unsafe { ffi::nxuskit_bn_infer_with_config(net, ev, algorithm, config_json) }
    }
    #[cfg(feature = "dynamic-link")]
    unsafe fn call_infer_with_config(
        net: *const ffi::NxuskitBnNet,
        ev: *const ffi::NxuskitBnEvidence,
        algorithm: *const c_char,
        config_json: *const c_char,
    ) -> *mut ffi::NxuskitBnResult {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_bn_infer_with_config)(net, ev, algorithm, config_json) }
    }

    // -- Part 2 public API --

    /// Save the network to a BIF file.
    pub fn save_file(&self, path: &str) -> Result<(), NxuskitError> {
        let c_path = to_cstring(path, "path")?;
        let ok = unsafe { Self::call_save_file(self.handle, c_path.as_ptr()) };
        if !ok {
            return Err(last_error_or("failed to save BIF file"));
        }
        Ok(())
    }

    /// Add a Gaussian (continuous) variable to the network.
    pub fn add_gaussian_variable(
        &mut self,
        name: &str,
        mean_base: f64,
        variance: f64,
    ) -> Result<(), NxuskitError> {
        let c_name = to_cstring(name, "name")?;
        let ok = unsafe {
            Self::call_add_gaussian_variable(self.handle, c_name.as_ptr(), mean_base, variance)
        };
        if !ok {
            return Err(last_error_or("failed to add Gaussian variable"));
        }
        Ok(())
    }

    /// Set the weight from a parent variable to a Gaussian child variable.
    pub fn set_gaussian_weight(
        &mut self,
        variable: &str,
        parent: &str,
        weight: f64,
    ) -> Result<(), NxuskitError> {
        let c_var = to_cstring(variable, "variable")?;
        let c_parent = to_cstring(parent, "parent")?;
        let ok = unsafe {
            Self::call_set_gaussian_weight(self.handle, c_var.as_ptr(), c_parent.as_ptr(), weight)
        };
        if !ok {
            return Err(last_error_or("failed to set Gaussian weight"));
        }
        Ok(())
    }

    // -- Streaming inference dispatch helpers --

    #[cfg(feature = "static-link")]
    unsafe fn call_infer_stream(
        net: *const ffi::NxuskitBnNet,
        ev: *const ffi::NxuskitBnEvidence,
        num_samples: u32,
        burn_in: u32,
        seed: u64,
        chunk_size: u32,
        on_chunk: Option<
            unsafe extern "C" fn(*const c_char, u32, u32, bool, *mut std::ffi::c_void) -> bool,
        >,
        user_data: *mut std::ffi::c_void,
    ) -> bool {
        unsafe {
            ffi::nxuskit_bn_infer_stream(
                net,
                ev,
                num_samples,
                burn_in,
                seed,
                chunk_size,
                on_chunk,
                user_data,
            )
        }
    }
    #[cfg(feature = "dynamic-link")]
    unsafe fn call_infer_stream(
        net: *const ffi::NxuskitBnNet,
        ev: *const ffi::NxuskitBnEvidence,
        num_samples: u32,
        burn_in: u32,
        seed: u64,
        chunk_size: u32,
        on_chunk: Option<
            unsafe extern "C" fn(*const c_char, u32, u32, bool, *mut std::ffi::c_void) -> bool,
        >,
        user_data: *mut std::ffi::c_void,
    ) -> bool {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe {
            (sdk.nxuskit_bn_infer_stream)(
                net,
                ev,
                num_samples,
                burn_in,
                seed,
                chunk_size,
                on_chunk,
                user_data,
            )
        }
    }

    // -- Streaming inference public API --

    /// Run streaming Gibbs inference with a callback for progressive results.
    ///
    /// The callback receives JSON chunks of partial results at intervals of
    /// `chunk_size` Gibbs samples (0 = default 1000). Return `true` from the
    /// callback to continue, `false` to cancel.
    ///
    /// # Safety
    ///
    /// The callback is invoked from the C runtime. The `chunk_json` parameter
    /// is only valid for the duration of each callback invocation.
    pub fn infer_stream<F>(
        &self,
        evidence: &BnEvidence,
        num_samples: u32,
        burn_in: u32,
        seed: u64,
        chunk_size: u32,
        mut callback: F,
    ) -> Result<(), NxuskitError>
    where
        F: FnMut(&str, u32, u32, bool) -> bool,
    {
        unsafe extern "C" fn trampoline<F>(
            chunk_json: *const c_char,
            iteration: u32,
            total: u32,
            is_final: bool,
            user_data: *mut std::ffi::c_void,
        ) -> bool
        where
            F: FnMut(&str, u32, u32, bool) -> bool,
        {
            let cb = unsafe { &mut *(user_data as *mut F) };
            let json_str = if chunk_json.is_null() {
                ""
            } else {
                unsafe { CStr::from_ptr(chunk_json) }.to_str().unwrap_or("")
            };
            cb(json_str, iteration, total, is_final)
        }

        let user_data = &mut callback as *mut F as *mut std::ffi::c_void;
        let ok = unsafe {
            Self::call_infer_stream(
                self.handle,
                evidence.handle,
                num_samples,
                burn_in,
                seed,
                chunk_size,
                Some(trampoline::<F>),
                user_data,
            )
        };
        if !ok {
            return Err(last_error_or("streaming inference failed"));
        }
        Ok(())
    }

    // -- Structure & Parameter Learning dispatch helpers --

    #[cfg(feature = "static-link")]
    unsafe fn call_search_structure(
        net: *const ffi::NxuskitBnNet,
        csv_path: *const c_char,
        algorithm: *const c_char,
        scoring: *const c_char,
        max_parents: u32,
        max_steps: u32,
        ess: f64,
        ordering_json: *const c_char,
    ) -> *mut c_char {
        unsafe {
            ffi::nxuskit_bn_search_structure(
                net,
                csv_path,
                algorithm,
                scoring,
                max_parents,
                max_steps,
                ess,
                ordering_json,
            )
        }
    }
    #[cfg(feature = "dynamic-link")]
    unsafe fn call_search_structure(
        net: *const ffi::NxuskitBnNet,
        csv_path: *const c_char,
        algorithm: *const c_char,
        scoring: *const c_char,
        max_parents: u32,
        max_steps: u32,
        ess: f64,
        ordering_json: *const c_char,
    ) -> *mut c_char {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe {
            (sdk.nxuskit_bn_search_structure)(
                net,
                csv_path,
                algorithm,
                scoring,
                max_parents,
                max_steps,
                ess,
                ordering_json,
            )
        }
    }

    #[cfg(feature = "static-link")]
    unsafe fn call_learn_mle(
        net: *mut ffi::NxuskitBnNet,
        csv_path: *const c_char,
        pseudocount: f64,
    ) -> bool {
        unsafe { ffi::nxuskit_bn_learn_mle(net, csv_path, pseudocount) }
    }
    #[cfg(feature = "dynamic-link")]
    unsafe fn call_learn_mle(
        net: *mut ffi::NxuskitBnNet,
        csv_path: *const c_char,
        pseudocount: f64,
    ) -> bool {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_bn_learn_mle)(net, csv_path, pseudocount) }
    }

    #[cfg(feature = "static-link")]
    unsafe fn call_log_likelihood(net: *const ffi::NxuskitBnNet, csv_path: *const c_char) -> f64 {
        unsafe { ffi::nxuskit_bn_log_likelihood(net, csv_path) }
    }
    #[cfg(feature = "dynamic-link")]
    unsafe fn call_log_likelihood(net: *const ffi::NxuskitBnNet, csv_path: *const c_char) -> f64 {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_bn_log_likelihood)(net, csv_path) }
    }

    // -- Structure & Parameter Learning public API --

    /// Run structure learning on CSV data.
    ///
    /// `algorithm`: `"hill_climb"` or `"k2"`.
    /// `scoring`: `"bic"` or `"bdeu"`.
    /// `ordering`: optional variable ordering for K2 (JSON array of names).
    ///
    /// Returns the result as a JSON value containing discovered edges and score.
    pub fn search_structure(
        &self,
        csv_path: &str,
        algorithm: &str,
        scoring: &str,
        max_parents: u32,
        max_steps: u32,
        ess: f64,
        ordering: Option<&str>,
    ) -> Result<serde_json::Value, NxuskitError> {
        let c_csv = to_cstring(csv_path, "csv_path")?;
        let c_algo = to_cstring(algorithm, "algorithm")?;
        let c_scoring = to_cstring(scoring, "scoring")?;
        let ordering_cstr = ordering.map(|o| to_cstring(o, "ordering")).transpose()?;
        let ordering_ptr = ordering_cstr
            .as_ref()
            .map_or(std::ptr::null(), |c| c.as_ptr());

        let ptr = unsafe {
            Self::call_search_structure(
                self.handle,
                c_csv.as_ptr(),
                c_algo.as_ptr(),
                c_scoring.as_ptr(),
                max_parents,
                max_steps,
                ess,
                ordering_ptr,
            )
        };
        let json_str = unsafe { read_and_free_string(ptr)? };
        serde_json::from_str(&json_str).map_err(|e| NxuskitError::Internal {
            message: format!("Failed to parse structure learning result JSON: {e}"),
        })
    }

    /// Learn CPT parameters via Maximum Likelihood Estimation from CSV data.
    ///
    /// `pseudocount`: Laplace smoothing parameter (e.g. 1.0).
    pub fn learn_mle(&mut self, csv_path: &str, pseudocount: f64) -> Result<(), NxuskitError> {
        let c_csv = to_cstring(csv_path, "csv_path")?;
        let ok = unsafe { Self::call_learn_mle(self.handle, c_csv.as_ptr(), pseudocount) };
        if !ok {
            return Err(last_error_or("MLE parameter learning failed"));
        }
        Ok(())
    }

    /// Compute log-likelihood of data given the current network CPTs.
    pub fn log_likelihood(&self, csv_path: &str) -> Result<f64, NxuskitError> {
        let c_csv = to_cstring(csv_path, "csv_path")?;
        let ll = unsafe { Self::call_log_likelihood(self.handle, c_csv.as_ptr()) };
        if ll.is_infinite() && ll.is_sign_negative() {
            return Err(last_error_or("log-likelihood computation failed"));
        }
        Ok(ll)
    }

    /// Run inference with algorithm-specific configuration (JSON).
    pub fn infer_with_config(
        &self,
        evidence: &BnEvidence,
        algorithm: &str,
        config_json: &str,
    ) -> Result<BnResult, NxuskitError> {
        let c_algo = to_cstring(algorithm, "algorithm")?;
        let c_config = to_cstring(config_json, "config_json")?;
        let handle = unsafe {
            Self::call_infer_with_config(
                self.handle,
                evidence.handle,
                c_algo.as_ptr(),
                c_config.as_ptr(),
            )
        };
        if handle.is_null() {
            return Err(last_error_or("inference with config failed"));
        }
        Ok(BnResult { handle })
    }
}

impl Drop for BnNetwork {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { Self::call_destroy(self.handle) };
        }
    }
}

// ── BnEvidence ───────────────────────────────────────────────────

/// Evidence (observations) for Bayesian Network inference. RAII wrapper.
pub struct BnEvidence {
    handle: *mut ffi::NxuskitBnEvidence,
}

unsafe impl Send for BnEvidence {}

impl BnEvidence {
    #[cfg(feature = "static-link")]
    unsafe fn call_create() -> *mut ffi::NxuskitBnEvidence {
        unsafe { ffi::nxuskit_bn_ev_create() }
    }
    #[cfg(feature = "dynamic-link")]
    unsafe fn call_create() -> *mut ffi::NxuskitBnEvidence {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_bn_ev_create)() }
    }

    #[cfg(feature = "static-link")]
    unsafe fn call_destroy(h: *mut ffi::NxuskitBnEvidence) {
        unsafe { ffi::nxuskit_bn_ev_destroy(h) }
    }
    #[cfg(feature = "dynamic-link")]
    unsafe fn call_destroy(h: *mut ffi::NxuskitBnEvidence) {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_bn_ev_destroy)(h) }
    }

    #[cfg(feature = "static-link")]
    unsafe fn call_set_discrete(
        ev: *mut ffi::NxuskitBnEvidence,
        net: *const ffi::NxuskitBnNet,
        var: *const c_char,
        state: *const c_char,
    ) -> bool {
        unsafe { ffi::nxuskit_bn_ev_set_discrete(ev, net, var, state) }
    }
    #[cfg(feature = "dynamic-link")]
    unsafe fn call_set_discrete(
        ev: *mut ffi::NxuskitBnEvidence,
        net: *const ffi::NxuskitBnNet,
        var: *const c_char,
        state: *const c_char,
    ) -> bool {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_bn_ev_set_discrete)(ev, net, var, state) }
    }

    #[cfg(feature = "static-link")]
    unsafe fn call_retract(ev: *mut ffi::NxuskitBnEvidence, var: *const c_char) -> bool {
        unsafe { ffi::nxuskit_bn_ev_retract(ev, var) }
    }
    #[cfg(feature = "dynamic-link")]
    unsafe fn call_retract(ev: *mut ffi::NxuskitBnEvidence, var: *const c_char) -> bool {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_bn_ev_retract)(ev, var) }
    }

    #[cfg(feature = "static-link")]
    unsafe fn call_clear(ev: *mut ffi::NxuskitBnEvidence) -> bool {
        unsafe { ffi::nxuskit_bn_ev_clear(ev) }
    }
    #[cfg(feature = "dynamic-link")]
    unsafe fn call_clear(ev: *mut ffi::NxuskitBnEvidence) -> bool {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_bn_ev_clear)(ev) }
    }

    #[cfg(feature = "static-link")]
    unsafe fn call_set_continuous(
        ev: *mut ffi::NxuskitBnEvidence,
        net: *const ffi::NxuskitBnNet,
        variable: *const c_char,
        value: f64,
    ) -> bool {
        unsafe { ffi::nxuskit_bn_ev_set_continuous(ev, net, variable, value) }
    }
    #[cfg(feature = "dynamic-link")]
    unsafe fn call_set_continuous(
        ev: *mut ffi::NxuskitBnEvidence,
        net: *const ffi::NxuskitBnNet,
        variable: *const c_char,
        value: f64,
    ) -> bool {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_bn_ev_set_continuous)(ev, net, variable, value) }
    }

    /// Create an empty evidence set.
    pub fn create() -> Result<Self, NxuskitError> {
        let handle = unsafe { Self::call_create() };
        if handle.is_null() {
            return Err(last_error_or("failed to create evidence"));
        }
        Ok(Self { handle })
    }

    /// Set a continuous observation for a Gaussian variable.
    pub fn set_continuous(
        &mut self,
        network: &BnNetwork,
        variable: &str,
        value: f64,
    ) -> Result<(), NxuskitError> {
        let c_var = to_cstring(variable, "variable")?;
        let ok = unsafe {
            Self::call_set_continuous(self.handle, network.raw_handle(), c_var.as_ptr(), value)
        };
        if !ok {
            return Err(last_error_or("failed to set continuous evidence"));
        }
        Ok(())
    }

    /// Set a discrete observation: variable=state.
    pub fn set_discrete(
        &mut self,
        network: &BnNetwork,
        variable: &str,
        state: &str,
    ) -> Result<(), NxuskitError> {
        let c_var = to_cstring(variable, "variable")?;
        let c_state = to_cstring(state, "state")?;
        let ok = unsafe {
            Self::call_set_discrete(
                self.handle,
                network.raw_handle(),
                c_var.as_ptr(),
                c_state.as_ptr(),
            )
        };
        if !ok {
            return Err(last_error_or("failed to set evidence"));
        }
        Ok(())
    }

    /// Retract evidence for a variable.
    pub fn retract(&mut self, variable: &str) -> Result<(), NxuskitError> {
        let c_var = to_cstring(variable, "variable")?;
        let ok = unsafe { Self::call_retract(self.handle, c_var.as_ptr()) };
        if !ok {
            return Err(last_error_or("failed to retract evidence"));
        }
        Ok(())
    }

    /// Clear all evidence.
    pub fn clear(&mut self) -> Result<(), NxuskitError> {
        let ok = unsafe { Self::call_clear(self.handle) };
        if !ok {
            return Err(last_error_or("failed to clear evidence"));
        }
        Ok(())
    }
}

impl Drop for BnEvidence {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { Self::call_destroy(self.handle) };
        }
    }
}

// ── BnResult ─────────────────────────────────────────────────────

/// Inference result containing posterior marginal distributions. RAII wrapper.
pub struct BnResult {
    handle: *mut ffi::NxuskitBnResult,
}

unsafe impl Send for BnResult {}

impl BnResult {
    #[cfg(feature = "static-link")]
    unsafe fn call_destroy(h: *mut ffi::NxuskitBnResult) {
        unsafe { ffi::nxuskit_bn_result_destroy(h) }
    }
    #[cfg(feature = "dynamic-link")]
    unsafe fn call_destroy(h: *mut ffi::NxuskitBnResult) {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_bn_result_destroy)(h) }
    }

    #[cfg(feature = "static-link")]
    unsafe fn call_json(h: *const ffi::NxuskitBnResult) -> *mut c_char {
        unsafe { ffi::nxuskit_bn_result_json(h) }
    }
    #[cfg(feature = "dynamic-link")]
    unsafe fn call_json(h: *const ffi::NxuskitBnResult) -> *mut c_char {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_bn_result_json)(h) }
    }

    #[cfg(feature = "static-link")]
    unsafe fn call_query(h: *const ffi::NxuskitBnResult, var: *const c_char) -> *mut c_char {
        unsafe { ffi::nxuskit_bn_result_query(h, var) }
    }
    #[cfg(feature = "dynamic-link")]
    unsafe fn call_query(h: *const ffi::NxuskitBnResult, var: *const c_char) -> *mut c_char {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_bn_result_query)(h, var) }
    }

    #[cfg(feature = "static-link")]
    unsafe fn call_num_variables(h: *const ffi::NxuskitBnResult) -> i32 {
        unsafe { ffi::nxuskit_bn_result_num_variables(h) }
    }
    #[cfg(feature = "dynamic-link")]
    unsafe fn call_num_variables(h: *const ffi::NxuskitBnResult) -> i32 {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_bn_result_num_variables)(h) }
    }

    #[cfg(feature = "static-link")]
    unsafe fn call_next(h: *mut ffi::NxuskitBnResult) -> *mut c_char {
        unsafe { ffi::nxuskit_bn_result_next(h) }
    }
    #[cfg(feature = "dynamic-link")]
    unsafe fn call_next(h: *mut ffi::NxuskitBnResult) -> *mut c_char {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_bn_result_next)(h) }
    }

    #[cfg(feature = "static-link")]
    unsafe fn call_reset(h: *mut ffi::NxuskitBnResult) {
        unsafe { ffi::nxuskit_bn_result_reset(h) }
    }
    #[cfg(feature = "dynamic-link")]
    unsafe fn call_reset(h: *mut ffi::NxuskitBnResult) {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_bn_result_reset)(h) }
    }

    // -- Part 2 dispatch helpers --

    #[cfg(feature = "static-link")]
    unsafe fn call_mean(h: *const ffi::NxuskitBnResult, var: *const c_char) -> f64 {
        unsafe { ffi::nxuskit_bn_result_mean(h, var) }
    }
    #[cfg(feature = "dynamic-link")]
    unsafe fn call_mean(h: *const ffi::NxuskitBnResult, var: *const c_char) -> f64 {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_bn_result_mean)(h, var) }
    }

    #[cfg(feature = "static-link")]
    unsafe fn call_variance(h: *const ffi::NxuskitBnResult, var: *const c_char) -> f64 {
        unsafe { ffi::nxuskit_bn_result_variance(h, var) }
    }
    #[cfg(feature = "dynamic-link")]
    unsafe fn call_variance(h: *const ffi::NxuskitBnResult, var: *const c_char) -> f64 {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_bn_result_variance)(h, var) }
    }

    #[cfg(feature = "static-link")]
    unsafe fn call_continuous_marginal(
        h: *const ffi::NxuskitBnResult,
        var: *const c_char,
    ) -> *mut c_char {
        unsafe { ffi::nxuskit_bn_result_continuous_marginal(h, var) }
    }
    #[cfg(feature = "dynamic-link")]
    unsafe fn call_continuous_marginal(
        h: *const ffi::NxuskitBnResult,
        var: *const c_char,
    ) -> *mut c_char {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_bn_result_continuous_marginal)(h, var) }
    }

    /// Get the full result as a JSON string.
    pub fn to_json(&self) -> Result<String, NxuskitError> {
        let ptr = unsafe { Self::call_json(self.handle) };
        unsafe { read_and_free_string(ptr) }
    }

    /// Query a single variable's posterior distribution.
    pub fn query(&self, variable: &str) -> Result<HashMap<String, f64>, NxuskitError> {
        let c_var = to_cstring(variable, "variable")?;
        let ptr = unsafe { Self::call_query(self.handle, c_var.as_ptr()) };
        let json = unsafe { read_and_free_string(ptr)? };
        serde_json::from_str(&json).map_err(|e| NxuskitError::Internal {
            message: format!("Failed to parse distribution JSON: {e}"),
        })
    }

    /// Number of variables in the result.
    pub fn num_variables(&self) -> i32 {
        unsafe { Self::call_num_variables(self.handle) }
    }

    /// Iterate over variable names. Returns None when exhausted.
    pub fn next_variable(&mut self) -> Option<String> {
        let ptr = unsafe { Self::call_next(self.handle) };
        if ptr.is_null() {
            return None;
        }
        unsafe { read_and_free_string(ptr).ok() }
    }

    /// Reset the iteration cursor.
    pub fn reset_cursor(&mut self) {
        unsafe { Self::call_reset(self.handle) };
    }

    /// Collect all variable names into a Vec.
    pub fn variable_names(&mut self) -> Vec<String> {
        self.reset_cursor();
        let mut names = Vec::new();
        while let Some(name) = self.next_variable() {
            names.push(name);
        }
        names
    }

    /// Get the posterior mean of a continuous variable.
    /// Returns `f64::NAN` if the variable is not found or is discrete-only.
    pub fn mean(&self, variable: &str) -> Result<f64, NxuskitError> {
        let c_var = to_cstring(variable, "variable")?;
        let val = unsafe { Self::call_mean(self.handle, c_var.as_ptr()) };
        if val.is_nan() {
            return Err(last_error_or("failed to get mean for variable"));
        }
        Ok(val)
    }

    /// Get the posterior variance of a continuous variable.
    /// Returns `f64::NAN` if the variable is not found or is discrete-only.
    pub fn variance(&self, variable: &str) -> Result<f64, NxuskitError> {
        let c_var = to_cstring(variable, "variable")?;
        let val = unsafe { Self::call_variance(self.handle, c_var.as_ptr()) };
        if val.is_nan() {
            return Err(last_error_or("failed to get variance for variable"));
        }
        Ok(val)
    }

    /// Get the full continuous marginal for a variable as JSON.
    /// Returns `{"mean":..., "variance":..., "ci_lower":..., "ci_upper":...}`.
    pub fn continuous_marginal(&self, variable: &str) -> Result<ContinuousMarginal, NxuskitError> {
        let c_var = to_cstring(variable, "variable")?;
        let ptr = unsafe { Self::call_continuous_marginal(self.handle, c_var.as_ptr()) };
        let json = unsafe { read_and_free_string(ptr)? };
        serde_json::from_str(&json).map_err(|e| NxuskitError::Internal {
            message: format!("Failed to parse continuous marginal JSON: {e}"),
        })
    }
}

impl Drop for BnResult {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { Self::call_destroy(self.handle) };
        }
    }
}
