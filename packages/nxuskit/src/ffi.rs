//! Raw FFI declarations for the nxusKit C ABI.
//!
//! This module is private — consumers interact only with safe wrapper types.
//! All `unsafe` code in the crate is confined to this module and its callers
//! in `provider.rs`, `stream.rs`, and `clips.rs`.

use std::ffi::{c_char, c_int, c_void};

// ---------------------------------------------------------------------------
// Cross-feature dispatch macro
// ---------------------------------------------------------------------------

/// Dispatch a C ABI call through either static linking or dynamic linking.
///
/// Under `static-link`, calls the function directly via the extern block.
/// Under `dynamic-link`, calls through the function pointer table loaded at
/// runtime.
///
/// # Safety
///
/// The caller must ensure all arguments satisfy the C ABI contract for the
/// target function.
macro_rules! ffi_call {
    ($fn_name:ident $(, $arg:expr)*) => {{
        #[cfg(feature = "static-link")]
        {
            unsafe { $crate::ffi::$fn_name($($arg),*) }
        }
        #[cfg(feature = "dynamic-link")]
        {
            let sdk = $crate::ffi::dynamic::sdk_unchecked();
            unsafe { (sdk.$fn_name)($($arg),*) }
        }
    }};
}

pub(crate) use ffi_call;

// ---------------------------------------------------------------------------
// Opaque C types
// ---------------------------------------------------------------------------

/// Opaque provider handle (owned by the SDK).
#[repr(C)]
pub(crate) struct NxuskitProvider {
    _opaque: [u8; 0],
}

/// Opaque response handle (owned by the SDK).
#[repr(C)]
pub(crate) struct NxuskitResponse {
    _opaque: [u8; 0],
}

/// Opaque stream handle (owned by the SDK).
#[repr(C)]
pub(crate) struct NxuskitStream {
    _opaque: [u8; 0],
}

// ---------------------------------------------------------------------------
// Solver SDK opaque C types
// ---------------------------------------------------------------------------

/// Opaque solver session handle (owned by the SDK).
#[repr(C)]
pub(crate) struct NxuskitSolverSession {
    _opaque: [u8; 0],
}

// ---------------------------------------------------------------------------
// Bayesian Network SDK opaque C types
// ---------------------------------------------------------------------------

/// Opaque Bayesian Network handle (owned by the SDK).
#[repr(C)]
pub(crate) struct NxuskitBnNet {
    _opaque: [u8; 0],
}

/// Opaque Bayesian Network evidence handle (owned by the SDK).
#[repr(C)]
pub(crate) struct NxuskitBnEvidence {
    _opaque: [u8; 0],
}

/// Opaque Bayesian Network result handle (owned by the SDK).
#[repr(C)]
pub(crate) struct NxuskitBnResult {
    _opaque: [u8; 0],
}

// ---------------------------------------------------------------------------
// Callback type aliases
// ---------------------------------------------------------------------------

/// Callback invoked for each streaming chunk.
/// Return 0 to continue, non-zero to cancel the stream.
pub(crate) type NxuskitStreamCallback =
    unsafe extern "C" fn(chunk_json: *const c_char, user_data: *mut c_void) -> c_int;

/// Callback invoked when the stream completes.
pub(crate) type NxuskitStreamDoneCallback =
    unsafe extern "C" fn(final_json: *const c_char, user_data: *mut c_void);

// ---------------------------------------------------------------------------
// Static linking: extern "C" declarations resolved at link time
// ---------------------------------------------------------------------------

#[cfg(feature = "static-link")]
unsafe extern "C" {
    pub(crate) fn nxuskit_version() -> *const c_char;
    pub(crate) fn nxuskit_abi_version() -> *const c_char;
    pub(crate) fn nxuskit_edition() -> *const c_char;
    pub(crate) fn nxuskit_capabilities() -> *mut c_char;
    pub(crate) fn nxuskit_build_info() -> *mut c_char;
    pub(crate) fn nxuskit_entitlement_info(license_key: *const c_char) -> *mut c_char;

    // License functions
    pub(crate) fn nxuskit_license_resolve(explicit_key: *const c_char) -> *mut c_char;
    pub(crate) fn nxuskit_license_validate(token_jwt: *const c_char) -> *mut c_char;
    pub(crate) fn nxuskit_license_machine_id() -> *mut c_char;
    pub(crate) fn nxuskit_license_activate(purchase_id: *const c_char) -> *mut c_char;
    pub(crate) fn nxuskit_license_deactivate() -> *mut c_char;
    pub(crate) fn nxuskit_license_trial_issue() -> *mut c_char;
    pub(crate) fn nxuskit_license_trial_activate(activation_code: *const c_char) -> *mut c_char;

    pub(crate) fn nxuskit_auth_set_credential(
        provider_id: *const c_char,
        api_key: *const c_char,
    ) -> c_int;
    pub(crate) fn nxuskit_auth_remove_credential(provider_id: *const c_char) -> c_int;
    pub(crate) fn nxuskit_auth_resolve(
        provider_id: *const c_char,
        explicit_key: *const c_char,
    ) -> *mut c_char;
    pub(crate) fn nxuskit_auth_status(provider_id: *const c_char) -> *mut c_char;
    pub(crate) fn nxuskit_auth_status_all() -> *mut c_char;
    pub(crate) fn nxuskit_auth_providers() -> *mut c_char;

    pub(crate) fn nxuskit_create_provider(config_json: *const c_char) -> *mut NxuskitProvider;
    pub(crate) fn nxuskit_free_provider(provider: *mut NxuskitProvider);

    pub(crate) fn nxuskit_chat(
        provider: *mut NxuskitProvider,
        request_json: *const c_char,
    ) -> *mut NxuskitResponse;
    pub(crate) fn nxuskit_response_json(response: *const NxuskitResponse) -> *const c_char;
    pub(crate) fn nxuskit_free_response(response: *mut NxuskitResponse);

    pub(crate) fn nxuskit_chat_stream(
        provider: *mut NxuskitProvider,
        request_json: *const c_char,
        on_chunk: NxuskitStreamCallback,
        on_done: NxuskitStreamDoneCallback,
        user_data: *mut c_void,
    ) -> *mut NxuskitStream;
    pub(crate) fn nxuskit_cancel_stream(stream: *mut NxuskitStream);
    pub(crate) fn nxuskit_free_stream(stream: *mut NxuskitStream);

    pub(crate) fn nxuskit_list_models(provider: *mut NxuskitProvider) -> *mut c_char;

    pub(crate) fn nxuskit_last_error() -> *const c_char;
    pub(crate) fn nxuskit_free_string(ptr: *mut c_char);

    // Plugin SDK
    pub(crate) fn nxuskit_plugin_load_dir(dir_path: *const c_char) -> i32;
    pub(crate) fn nxuskit_plugin_list() -> *mut c_char;
    pub(crate) fn nxuskit_plugin_info(name: *const c_char) -> *mut c_char;
    pub(crate) fn nxuskit_plugin_count() -> i32;
    pub(crate) fn nxuskit_plugin_loaded(name: *const c_char) -> i32;
    pub(crate) fn nxuskit_plugin_unload_all();
    pub(crate) fn nxuskit_plugin_set_trust_mode(mode: i32) -> i32;
    pub(crate) fn nxuskit_plugin_get_trust_mode() -> i32;
    pub(crate) fn nxuskit_plugin_load_dir_trusted(dir_path: *const c_char) -> i32;

    // OAuth functions
    pub(crate) fn nxuskit_oauth_start(provider_id: *const c_char, timeout_secs: u32)
    -> *mut c_char;
    pub(crate) fn nxuskit_oauth_status(provider_id: *const c_char) -> *mut c_char;
    pub(crate) fn nxuskit_oauth_revoke(provider_id: *const c_char) -> i32;

    // Bayesian Network SDK
    pub(crate) fn nxuskit_bn_net_create() -> *mut NxuskitBnNet;
    pub(crate) fn nxuskit_bn_net_destroy(net: *mut NxuskitBnNet);
    pub(crate) fn nxuskit_bn_net_load_file(path: *const c_char) -> *mut NxuskitBnNet;
    pub(crate) fn nxuskit_bn_net_num_variables(net: *const NxuskitBnNet) -> i32;
    pub(crate) fn nxuskit_bn_net_variables(net: *const NxuskitBnNet) -> *mut c_char;
    pub(crate) fn nxuskit_bn_net_variable_states(
        net: *const NxuskitBnNet,
        variable: *const c_char,
    ) -> *mut c_char;
    pub(crate) fn nxuskit_bn_ev_create() -> *mut NxuskitBnEvidence;
    pub(crate) fn nxuskit_bn_ev_destroy(ev: *mut NxuskitBnEvidence);
    pub(crate) fn nxuskit_bn_ev_set_discrete(
        ev: *mut NxuskitBnEvidence,
        net: *const NxuskitBnNet,
        variable: *const c_char,
        state: *const c_char,
    ) -> bool;
    pub(crate) fn nxuskit_bn_ev_retract(
        ev: *mut NxuskitBnEvidence,
        variable: *const c_char,
    ) -> bool;
    pub(crate) fn nxuskit_bn_ev_clear(ev: *mut NxuskitBnEvidence) -> bool;
    pub(crate) fn nxuskit_bn_infer(
        net: *const NxuskitBnNet,
        ev: *const NxuskitBnEvidence,
        algorithm: *const c_char,
        num_samples: u32,
        burn_in: u32,
        seed: u64,
    ) -> *mut NxuskitBnResult;
    pub(crate) fn nxuskit_bn_result_destroy(result: *mut NxuskitBnResult);
    pub(crate) fn nxuskit_bn_result_json(result: *const NxuskitBnResult) -> *mut c_char;
    pub(crate) fn nxuskit_bn_result_query(
        result: *const NxuskitBnResult,
        variable: *const c_char,
    ) -> *mut c_char;
    pub(crate) fn nxuskit_bn_result_num_variables(result: *const NxuskitBnResult) -> i32;
    pub(crate) fn nxuskit_bn_result_next(result: *mut NxuskitBnResult) -> *mut c_char;
    pub(crate) fn nxuskit_bn_result_reset(result: *mut NxuskitBnResult);

    // Bayesian Network SDK — Part 2 extensions
    pub(crate) fn nxuskit_bn_net_save_file(net: *const NxuskitBnNet, path: *const c_char) -> bool;
    pub(crate) fn nxuskit_bn_ev_set_continuous(
        ev: *mut NxuskitBnEvidence,
        net: *const NxuskitBnNet,
        variable: *const c_char,
        value: f64,
    ) -> bool;
    pub(crate) fn nxuskit_bn_net_add_gaussian_variable(
        net: *mut NxuskitBnNet,
        name: *const c_char,
        mean_base: f64,
        variance: f64,
    ) -> bool;
    pub(crate) fn nxuskit_bn_net_set_gaussian_weight(
        net: *mut NxuskitBnNet,
        variable: *const c_char,
        parent: *const c_char,
        weight: f64,
    ) -> bool;
    pub(crate) fn nxuskit_bn_result_mean(
        result: *const NxuskitBnResult,
        variable: *const c_char,
    ) -> f64;
    pub(crate) fn nxuskit_bn_result_variance(
        result: *const NxuskitBnResult,
        variable: *const c_char,
    ) -> f64;
    pub(crate) fn nxuskit_bn_result_continuous_marginal(
        result: *const NxuskitBnResult,
        variable: *const c_char,
    ) -> *mut c_char;
    pub(crate) fn nxuskit_bn_infer_with_config(
        net: *const NxuskitBnNet,
        ev: *const NxuskitBnEvidence,
        algorithm: *const c_char,
        config_json: *const c_char,
    ) -> *mut NxuskitBnResult;

    // Bayesian Network SDK — Streaming inference
    pub(crate) fn nxuskit_bn_infer_stream(
        net: *const NxuskitBnNet,
        ev: *const NxuskitBnEvidence,
        num_samples: u32,
        burn_in: u32,
        seed: u64,
        chunk_size: u32,
        on_chunk: Option<
            unsafe extern "C" fn(
                chunk_json: *const c_char,
                iteration: u32,
                total: u32,
                is_final: bool,
                user_data: *mut std::ffi::c_void,
            ) -> bool,
        >,
        user_data: *mut std::ffi::c_void,
    ) -> bool;

    // Bayesian Network SDK — Structure & Parameter Learning
    pub(crate) fn nxuskit_bn_search_structure(
        net: *const NxuskitBnNet,
        csv_path: *const c_char,
        algorithm: *const c_char,
        scoring: *const c_char,
        max_parents: u32,
        max_steps: u32,
        ess: f64,
        ordering_json: *const c_char,
    ) -> *mut c_char;
    pub(crate) fn nxuskit_bn_learn_mle(
        net: *mut NxuskitBnNet,
        csv_path: *const c_char,
        pseudocount: f64,
    ) -> bool;
    pub(crate) fn nxuskit_bn_log_likelihood(
        net: *const NxuskitBnNet,
        csv_path: *const c_char,
    ) -> f64;

    // ZEN SDK: Stateless evaluation
    pub(crate) fn nxuskit_zen_evaluate(
        model_json: *const c_char,
        input_json: *const c_char,
    ) -> *mut c_char;
    pub(crate) fn nxuskit_zen_free_result(result: *mut c_char);

    // Solver SDK: Session lifecycle
    pub(crate) fn nxuskit_solver_session_create(
        config_json: *const c_char,
    ) -> *mut NxuskitSolverSession;
    pub(crate) fn nxuskit_solver_session_destroy(session: *mut NxuskitSolverSession);
    pub(crate) fn nxuskit_solver_reset(session: *mut NxuskitSolverSession) -> bool;

    // Solver SDK: Model building
    pub(crate) fn nxuskit_solver_add_variables(
        session: *mut NxuskitSolverSession,
        vars_json: *const c_char,
    ) -> bool;
    pub(crate) fn nxuskit_solver_add_constraints(
        session: *mut NxuskitSolverSession,
        constraints_json: *const c_char,
    ) -> bool;
    pub(crate) fn nxuskit_solver_set_objective(
        session: *mut NxuskitSolverSession,
        objective_json: *const c_char,
    ) -> bool;
    pub(crate) fn nxuskit_solver_retract(
        session: *mut NxuskitSolverSession,
        names_json: *const c_char,
    ) -> bool;

    // Solver SDK: Scoping
    pub(crate) fn nxuskit_solver_push(session: *mut NxuskitSolverSession) -> bool;
    pub(crate) fn nxuskit_solver_pop(session: *mut NxuskitSolverSession) -> bool;

    // Solver SDK: Execution
    pub(crate) fn nxuskit_solver_solve(
        session: *mut NxuskitSolverSession,
        config_json: *const c_char,
    ) -> *mut c_char;
    pub(crate) fn nxuskit_solver_solve_stream(
        session: *mut NxuskitSolverSession,
        config_json: *const c_char,
        on_chunk: NxuskitStreamCallback,
        on_done: NxuskitStreamDoneCallback,
        user_data: *mut c_void,
    ) -> *mut NxuskitStream;

    // Solver SDK: Introspection
    pub(crate) fn nxuskit_solver_variables(session: *mut NxuskitSolverSession) -> *mut c_char;
    pub(crate) fn nxuskit_solver_constraints(session: *mut NxuskitSolverSession) -> *mut c_char;
    pub(crate) fn nxuskit_solver_status(session: *mut NxuskitSolverSession) -> *mut c_char;
    pub(crate) fn nxuskit_solver_capabilities(session: *mut NxuskitSolverSession) -> *mut c_char;
    pub(crate) fn nxuskit_solver_num_variables(session: *mut NxuskitSolverSession) -> i64;
    pub(crate) fn nxuskit_solver_num_constraints(session: *mut NxuskitSolverSession) -> i64;

    // Solver SDK: Multi-objective, Explanation & Assumptions
    pub(crate) fn nxuskit_solver_add_objective(
        session: *mut NxuskitSolverSession,
        objective_json: *const c_char,
    ) -> bool;
    pub(crate) fn nxuskit_solver_retract_objective(
        session: *mut NxuskitSolverSession,
        name: *const c_char,
    ) -> bool;
    pub(crate) fn nxuskit_solver_objectives(session: *mut NxuskitSolverSession) -> *mut c_char;
    pub(crate) fn nxuskit_solver_explanation(session: *mut NxuskitSolverSession) -> *mut c_char;
    pub(crate) fn nxuskit_solver_add_assumptions(
        session: *mut NxuskitSolverSession,
        assumptions_json: *const c_char,
    ) -> bool;

    // -----------------------------------------------------------------------
    // CLIPS Session API (u64 session handles — replaces opaque-pointer API)
    // -----------------------------------------------------------------------

    // Session lifecycle
    pub(crate) fn nxuskit_clips_session_create() -> u64;
    pub(crate) fn nxuskit_clips_session_destroy(session: u64);
    pub(crate) fn nxuskit_clips_session_reset(session: u64) -> i32;
    pub(crate) fn nxuskit_clips_session_clear(session: u64) -> i32;
    pub(crate) fn nxuskit_clips_session_info(session: u64) -> *mut c_char;

    // Session construct loading
    pub(crate) fn nxuskit_clips_session_load_file(session: u64, path: *const c_char) -> i32;
    pub(crate) fn nxuskit_clips_session_load_string(session: u64, constructs: *const c_char)
    -> i32;
    pub(crate) fn nxuskit_clips_session_load_binary(session: u64, path: *const c_char) -> i32;
    pub(crate) fn nxuskit_clips_session_save_binary(session: u64, path: *const c_char) -> i32;
    pub(crate) fn nxuskit_clips_session_build(session: u64, construct: *const c_char) -> i32;
    pub(crate) fn nxuskit_clips_session_batch(session: u64, path: *const c_char) -> i32;

    // Session fact operations
    pub(crate) fn nxuskit_clips_fact_assert_string(session: u64, fact: *const c_char) -> i64;
    pub(crate) fn nxuskit_clips_fact_assert_structured(
        session: u64,
        template: *const c_char,
        slots_json: *const c_char,
    ) -> i64;
    pub(crate) fn nxuskit_clips_fact_retract(session: u64, index: i64) -> i32;
    pub(crate) fn nxuskit_clips_fact_retract_by_template(
        session: u64,
        template: *const c_char,
    ) -> i32;
    pub(crate) fn nxuskit_clips_fact_exists(session: u64, index: i64) -> bool;
    pub(crate) fn nxuskit_clips_fact_get_slot(
        session: u64,
        index: i64,
        slot: *const c_char,
    ) -> *mut c_char;
    pub(crate) fn nxuskit_clips_fact_slot_values(session: u64, index: i64) -> *mut c_char;
    pub(crate) fn nxuskit_clips_fact_pp_form(session: u64, index: i64) -> *mut c_char;
    pub(crate) fn nxuskit_clips_fact_index(session: u64, index: i64) -> i64;
    pub(crate) fn nxuskit_clips_facts_list(session: u64) -> *mut c_char;
    pub(crate) fn nxuskit_clips_facts_by_template(
        session: u64,
        template: *const c_char,
    ) -> *mut c_char;

    // Session template operations
    pub(crate) fn nxuskit_clips_template_exists(session: u64, name: *const c_char) -> bool;
    pub(crate) fn nxuskit_clips_template_list(session: u64) -> *mut c_char;
    pub(crate) fn nxuskit_clips_template_slot_names(
        session: u64,
        name: *const c_char,
    ) -> *mut c_char;
    pub(crate) fn nxuskit_clips_template_slot_info(
        session: u64,
        name: *const c_char,
    ) -> *mut c_char;
    pub(crate) fn nxuskit_clips_template_facts(session: u64, name: *const c_char) -> *mut c_char;
    pub(crate) fn nxuskit_clips_template_pp_form(session: u64, name: *const c_char) -> *mut c_char;

    // Session rule operations
    pub(crate) fn nxuskit_clips_rule_exists(session: u64, name: *const c_char) -> bool;
    pub(crate) fn nxuskit_clips_rule_list(session: u64) -> *mut c_char;
    pub(crate) fn nxuskit_clips_rule_times_fired(session: u64, name: *const c_char) -> i64;
    pub(crate) fn nxuskit_clips_rule_breakpoint_set(session: u64, name: *const c_char) -> i32;
    pub(crate) fn nxuskit_clips_rule_breakpoint_remove(session: u64, name: *const c_char) -> i32;
    pub(crate) fn nxuskit_clips_rule_has_breakpoint(session: u64, name: *const c_char) -> bool;
    pub(crate) fn nxuskit_clips_rule_refresh(session: u64, name: *const c_char) -> i32;
    pub(crate) fn nxuskit_clips_rule_pp_form(session: u64, name: *const c_char) -> *mut c_char;
    pub(crate) fn nxuskit_clips_rule_delete(session: u64, name: *const c_char) -> i32;

    // Session execution & agenda
    pub(crate) fn nxuskit_clips_session_run(session: u64, limit: i64) -> i64;
    pub(crate) fn nxuskit_clips_session_halt(session: u64) -> i32;
    pub(crate) fn nxuskit_clips_agenda_size(session: u64) -> i64;
    pub(crate) fn nxuskit_clips_agenda_clear(session: u64) -> i32;
    pub(crate) fn nxuskit_clips_agenda_reorder(session: u64) -> i32;
    pub(crate) fn nxuskit_clips_strategy_get(session: u64) -> *mut c_char;
    pub(crate) fn nxuskit_clips_strategy_set(session: u64, strategy: *const c_char) -> i32;
    pub(crate) fn nxuskit_clips_salience_mode_get(session: u64) -> *mut c_char;
    pub(crate) fn nxuskit_clips_salience_mode_set(session: u64, mode: *const c_char) -> i32;

    // Session eval
    pub(crate) fn nxuskit_clips_eval(session: u64, expression: *const c_char) -> *mut c_char;
    pub(crate) fn nxuskit_clips_function_call(
        session: u64,
        name: *const c_char,
        args_json: *const c_char,
    ) -> *mut c_char;

    // Session settings
    pub(crate) fn nxuskit_clips_fact_duplication_get(session: u64) -> bool;
    pub(crate) fn nxuskit_clips_fact_duplication_set(session: u64, allow: bool) -> i32;
    pub(crate) fn nxuskit_clips_reset_globals_get(session: u64) -> bool;
    pub(crate) fn nxuskit_clips_reset_globals_set(session: u64, reset: bool) -> i32;

    // Session JSON loading
    pub(crate) fn nxuskit_clips_session_load_json(session: u64, json: *const c_char) -> i32;

    // Session cache
    pub(crate) fn nxuskit_clips_session_preload(
        name: *const c_char,
        rules_json: *const c_char,
    ) -> i32;
    pub(crate) fn nxuskit_clips_session_get_cached(name: *const c_char) -> u64;
    pub(crate) fn nxuskit_clips_session_cache_remove(name: *const c_char) -> i32;

    // Session module & focus
    pub(crate) fn nxuskit_clips_module_exists(session: u64, name: *const c_char) -> bool;
    pub(crate) fn nxuskit_clips_module_list(session: u64) -> *mut c_char;
    pub(crate) fn nxuskit_clips_module_current_get(session: u64) -> *mut c_char;
    pub(crate) fn nxuskit_clips_module_current_set(session: u64, name: *const c_char) -> i32;
    pub(crate) fn nxuskit_clips_focus_push(session: u64, module_name: *const c_char) -> i32;
    pub(crate) fn nxuskit_clips_focus_get(session: u64) -> *mut c_char;
    pub(crate) fn nxuskit_clips_focus_pop(session: u64) -> i32;
    pub(crate) fn nxuskit_clips_focus_clear(session: u64) -> i32;

    // Global variables
    pub(crate) fn nxuskit_clips_global_exists(session: u64, name: *const c_char) -> bool;
    pub(crate) fn nxuskit_clips_global_list(session: u64) -> *mut c_char;
    pub(crate) fn nxuskit_clips_global_get_value(session: u64, name: *const c_char) -> *mut c_char;
    pub(crate) fn nxuskit_clips_global_set_value(
        session: u64,
        name: *const c_char,
        value_json: *const c_char,
    ) -> i32;

    // Watch & diagnostics
    pub(crate) fn nxuskit_clips_watch(session: u64, item: *const c_char) -> i32;
    pub(crate) fn nxuskit_clips_unwatch(session: u64, item: *const c_char) -> i32;
    pub(crate) fn nxuskit_clips_dribble_on(session: u64, file_path: *const c_char) -> i32;
    pub(crate) fn nxuskit_clips_dribble_off(session: u64) -> i32;

}

// ---------------------------------------------------------------------------
// Dynamic linking: function table loaded at runtime via libloading
// ---------------------------------------------------------------------------

#[cfg(feature = "dynamic-link")]
pub(crate) mod dynamic {
    use super::*;
    use libloading::{Library, Symbol};
    use std::sync::OnceLock;

    /// Holds the dynamically loaded library and resolved function pointers.
    #[allow(dead_code)]
    pub(crate) struct SdkFunctions {
        // Keep the library alive for the process lifetime.
        _lib: Library,

        pub nxuskit_version: unsafe extern "C" fn() -> *const c_char,
        pub nxuskit_abi_version: unsafe extern "C" fn() -> *const c_char,
        pub nxuskit_edition: unsafe extern "C" fn() -> *const c_char,
        pub nxuskit_capabilities: unsafe extern "C" fn() -> *mut c_char,
        pub nxuskit_build_info: unsafe extern "C" fn() -> *mut c_char,
        pub nxuskit_entitlement_info:
            unsafe extern "C" fn(license_key: *const c_char) -> *mut c_char,

        // License functions
        pub nxuskit_license_resolve:
            unsafe extern "C" fn(explicit_key: *const c_char) -> *mut c_char,
        pub nxuskit_license_validate: unsafe extern "C" fn(token_jwt: *const c_char) -> *mut c_char,
        pub nxuskit_license_machine_id: unsafe extern "C" fn() -> *mut c_char,
        pub nxuskit_license_activate:
            unsafe extern "C" fn(purchase_id: *const c_char) -> *mut c_char,
        pub nxuskit_license_deactivate: unsafe extern "C" fn() -> *mut c_char,
        pub nxuskit_license_trial_issue: unsafe extern "C" fn() -> *mut c_char,
        pub nxuskit_license_trial_activate:
            unsafe extern "C" fn(activation_code: *const c_char) -> *mut c_char,

        pub nxuskit_auth_set_credential:
            unsafe extern "C" fn(provider_id: *const c_char, api_key: *const c_char) -> c_int,
        pub nxuskit_auth_remove_credential:
            unsafe extern "C" fn(provider_id: *const c_char) -> c_int,
        pub nxuskit_auth_resolve: unsafe extern "C" fn(
            provider_id: *const c_char,
            explicit_key: *const c_char,
        ) -> *mut c_char,
        pub nxuskit_auth_status: unsafe extern "C" fn(provider_id: *const c_char) -> *mut c_char,
        pub nxuskit_auth_status_all: unsafe extern "C" fn() -> *mut c_char,
        pub nxuskit_auth_providers: unsafe extern "C" fn() -> *mut c_char,

        pub nxuskit_create_provider:
            unsafe extern "C" fn(config_json: *const c_char) -> *mut NxuskitProvider,
        pub nxuskit_free_provider: unsafe extern "C" fn(provider: *mut NxuskitProvider),

        pub nxuskit_chat: unsafe extern "C" fn(
            provider: *mut NxuskitProvider,
            request_json: *const c_char,
        ) -> *mut NxuskitResponse,
        pub nxuskit_response_json:
            unsafe extern "C" fn(response: *const NxuskitResponse) -> *const c_char,
        pub nxuskit_free_response: unsafe extern "C" fn(response: *mut NxuskitResponse),

        pub nxuskit_chat_stream: unsafe extern "C" fn(
            provider: *mut NxuskitProvider,
            request_json: *const c_char,
            on_chunk: NxuskitStreamCallback,
            on_done: NxuskitStreamDoneCallback,
            user_data: *mut c_void,
        ) -> *mut NxuskitStream,
        pub nxuskit_cancel_stream: unsafe extern "C" fn(stream: *mut NxuskitStream),
        pub nxuskit_free_stream: unsafe extern "C" fn(stream: *mut NxuskitStream),

        pub nxuskit_list_models:
            unsafe extern "C" fn(provider: *mut NxuskitProvider) -> *mut c_char,

        pub nxuskit_last_error: unsafe extern "C" fn() -> *const c_char,
        pub nxuskit_free_string: unsafe extern "C" fn(ptr: *mut c_char),

        // Plugin SDK
        pub nxuskit_plugin_load_dir: unsafe extern "C" fn(dir_path: *const c_char) -> i32,
        pub nxuskit_plugin_list: unsafe extern "C" fn() -> *mut c_char,
        pub nxuskit_plugin_info: unsafe extern "C" fn(name: *const c_char) -> *mut c_char,
        pub nxuskit_plugin_count: unsafe extern "C" fn() -> i32,
        pub nxuskit_plugin_loaded: unsafe extern "C" fn(name: *const c_char) -> i32,
        pub nxuskit_plugin_unload_all: unsafe extern "C" fn(),
        pub nxuskit_plugin_set_trust_mode: unsafe extern "C" fn(mode: i32) -> i32,
        pub nxuskit_plugin_get_trust_mode: unsafe extern "C" fn() -> i32,
        pub nxuskit_plugin_load_dir_trusted: unsafe extern "C" fn(dir_path: *const c_char) -> i32,

        // OAuth functions
        pub nxuskit_oauth_start:
            unsafe extern "C" fn(provider_id: *const c_char, timeout_secs: u32) -> *mut c_char,
        pub nxuskit_oauth_status: unsafe extern "C" fn(provider_id: *const c_char) -> *mut c_char,
        pub nxuskit_oauth_revoke: unsafe extern "C" fn(provider_id: *const c_char) -> i32,

        // Bayesian Network SDK
        pub nxuskit_bn_net_create: unsafe extern "C" fn() -> *mut NxuskitBnNet,
        pub nxuskit_bn_net_destroy: unsafe extern "C" fn(net: *mut NxuskitBnNet),
        pub nxuskit_bn_net_load_file:
            unsafe extern "C" fn(path: *const c_char) -> *mut NxuskitBnNet,
        pub nxuskit_bn_net_num_variables: unsafe extern "C" fn(net: *const NxuskitBnNet) -> i32,
        pub nxuskit_bn_net_variables: unsafe extern "C" fn(net: *const NxuskitBnNet) -> *mut c_char,
        pub nxuskit_bn_net_variable_states:
            unsafe extern "C" fn(net: *const NxuskitBnNet, variable: *const c_char) -> *mut c_char,
        pub nxuskit_bn_ev_create: unsafe extern "C" fn() -> *mut NxuskitBnEvidence,
        pub nxuskit_bn_ev_destroy: unsafe extern "C" fn(ev: *mut NxuskitBnEvidence),
        pub nxuskit_bn_ev_set_discrete: unsafe extern "C" fn(
            ev: *mut NxuskitBnEvidence,
            net: *const NxuskitBnNet,
            variable: *const c_char,
            state: *const c_char,
        ) -> bool,
        pub nxuskit_bn_ev_retract:
            unsafe extern "C" fn(ev: *mut NxuskitBnEvidence, variable: *const c_char) -> bool,
        pub nxuskit_bn_ev_clear: unsafe extern "C" fn(ev: *mut NxuskitBnEvidence) -> bool,
        pub nxuskit_bn_infer: unsafe extern "C" fn(
            net: *const NxuskitBnNet,
            ev: *const NxuskitBnEvidence,
            algorithm: *const c_char,
            num_samples: u32,
            burn_in: u32,
            seed: u64,
        ) -> *mut NxuskitBnResult,
        pub nxuskit_bn_result_destroy: unsafe extern "C" fn(result: *mut NxuskitBnResult),
        pub nxuskit_bn_result_json:
            unsafe extern "C" fn(result: *const NxuskitBnResult) -> *mut c_char,
        pub nxuskit_bn_result_query: unsafe extern "C" fn(
            result: *const NxuskitBnResult,
            variable: *const c_char,
        ) -> *mut c_char,
        pub nxuskit_bn_result_num_variables:
            unsafe extern "C" fn(result: *const NxuskitBnResult) -> i32,
        pub nxuskit_bn_result_next:
            unsafe extern "C" fn(result: *mut NxuskitBnResult) -> *mut c_char,
        pub nxuskit_bn_result_reset: unsafe extern "C" fn(result: *mut NxuskitBnResult),

        // Bayesian Network SDK — Part 2 extensions
        pub nxuskit_bn_net_save_file:
            unsafe extern "C" fn(net: *const NxuskitBnNet, path: *const c_char) -> bool,
        pub nxuskit_bn_ev_set_continuous: unsafe extern "C" fn(
            ev: *mut NxuskitBnEvidence,
            net: *const NxuskitBnNet,
            variable: *const c_char,
            value: f64,
        ) -> bool,
        pub nxuskit_bn_net_add_gaussian_variable: unsafe extern "C" fn(
            net: *mut NxuskitBnNet,
            name: *const c_char,
            mean_base: f64,
            variance: f64,
        ) -> bool,
        pub nxuskit_bn_net_set_gaussian_weight: unsafe extern "C" fn(
            net: *mut NxuskitBnNet,
            variable: *const c_char,
            parent: *const c_char,
            weight: f64,
        ) -> bool,
        pub nxuskit_bn_result_mean:
            unsafe extern "C" fn(result: *const NxuskitBnResult, variable: *const c_char) -> f64,
        pub nxuskit_bn_result_variance:
            unsafe extern "C" fn(result: *const NxuskitBnResult, variable: *const c_char) -> f64,
        pub nxuskit_bn_result_continuous_marginal: unsafe extern "C" fn(
            result: *const NxuskitBnResult,
            variable: *const c_char,
        ) -> *mut c_char,
        pub nxuskit_bn_infer_with_config: unsafe extern "C" fn(
            net: *const NxuskitBnNet,
            ev: *const NxuskitBnEvidence,
            algorithm: *const c_char,
            config_json: *const c_char,
        ) -> *mut NxuskitBnResult,

        // Bayesian Network SDK — Streaming inference
        pub nxuskit_bn_infer_stream: unsafe extern "C" fn(
            net: *const NxuskitBnNet,
            ev: *const NxuskitBnEvidence,
            num_samples: u32,
            burn_in: u32,
            seed: u64,
            chunk_size: u32,
            on_chunk: Option<
                unsafe extern "C" fn(
                    chunk_json: *const c_char,
                    iteration: u32,
                    total: u32,
                    is_final: bool,
                    user_data: *mut std::ffi::c_void,
                ) -> bool,
            >,
            user_data: *mut std::ffi::c_void,
        ) -> bool,

        // Bayesian Network SDK — Structure & Parameter Learning
        pub nxuskit_bn_search_structure: unsafe extern "C" fn(
            net: *const NxuskitBnNet,
            csv_path: *const c_char,
            algorithm: *const c_char,
            scoring: *const c_char,
            max_parents: u32,
            max_steps: u32,
            ess: f64,
            ordering_json: *const c_char,
        ) -> *mut c_char,
        pub nxuskit_bn_learn_mle: unsafe extern "C" fn(
            net: *mut NxuskitBnNet,
            csv_path: *const c_char,
            pseudocount: f64,
        ) -> bool,
        pub nxuskit_bn_log_likelihood:
            unsafe extern "C" fn(net: *const NxuskitBnNet, csv_path: *const c_char) -> f64,

        // ZEN SDK: Stateless evaluation
        pub nxuskit_zen_evaluate: unsafe extern "C" fn(
            model_json: *const c_char,
            input_json: *const c_char,
        ) -> *mut c_char,
        pub nxuskit_zen_free_result: unsafe extern "C" fn(result: *mut c_char),

        // Solver SDK: Session lifecycle
        pub nxuskit_solver_session_create:
            unsafe extern "C" fn(config_json: *const c_char) -> *mut NxuskitSolverSession,
        pub nxuskit_solver_session_destroy:
            unsafe extern "C" fn(session: *mut NxuskitSolverSession),
        pub nxuskit_solver_reset: unsafe extern "C" fn(session: *mut NxuskitSolverSession) -> bool,

        // Solver SDK: Model building
        pub nxuskit_solver_add_variables: unsafe extern "C" fn(
            session: *mut NxuskitSolverSession,
            vars_json: *const c_char,
        ) -> bool,
        pub nxuskit_solver_add_constraints: unsafe extern "C" fn(
            session: *mut NxuskitSolverSession,
            constraints_json: *const c_char,
        ) -> bool,
        pub nxuskit_solver_set_objective: unsafe extern "C" fn(
            session: *mut NxuskitSolverSession,
            objective_json: *const c_char,
        ) -> bool,
        pub nxuskit_solver_retract: unsafe extern "C" fn(
            session: *mut NxuskitSolverSession,
            names_json: *const c_char,
        ) -> bool,

        // Solver SDK: Scoping
        pub nxuskit_solver_push: unsafe extern "C" fn(session: *mut NxuskitSolverSession) -> bool,
        pub nxuskit_solver_pop: unsafe extern "C" fn(session: *mut NxuskitSolverSession) -> bool,

        // Solver SDK: Execution
        pub nxuskit_solver_solve: unsafe extern "C" fn(
            session: *mut NxuskitSolverSession,
            config_json: *const c_char,
        ) -> *mut c_char,
        pub nxuskit_solver_solve_stream: unsafe extern "C" fn(
            session: *mut NxuskitSolverSession,
            config_json: *const c_char,
            on_chunk: NxuskitStreamCallback,
            on_done: NxuskitStreamDoneCallback,
            user_data: *mut c_void,
        ) -> *mut NxuskitStream,

        // Solver SDK: Introspection
        pub nxuskit_solver_variables:
            unsafe extern "C" fn(session: *mut NxuskitSolverSession) -> *mut c_char,
        pub nxuskit_solver_constraints:
            unsafe extern "C" fn(session: *mut NxuskitSolverSession) -> *mut c_char,
        pub nxuskit_solver_status:
            unsafe extern "C" fn(session: *mut NxuskitSolverSession) -> *mut c_char,
        pub nxuskit_solver_capabilities:
            unsafe extern "C" fn(session: *mut NxuskitSolverSession) -> *mut c_char,
        pub nxuskit_solver_num_variables:
            unsafe extern "C" fn(session: *mut NxuskitSolverSession) -> i64,
        pub nxuskit_solver_num_constraints:
            unsafe extern "C" fn(session: *mut NxuskitSolverSession) -> i64,

        // Solver SDK: Multi-objective, Explanation & Assumptions
        pub nxuskit_solver_add_objective: unsafe extern "C" fn(
            session: *mut NxuskitSolverSession,
            objective_json: *const c_char,
        ) -> bool,
        pub nxuskit_solver_retract_objective:
            unsafe extern "C" fn(session: *mut NxuskitSolverSession, name: *const c_char) -> bool,
        pub nxuskit_solver_objectives:
            unsafe extern "C" fn(session: *mut NxuskitSolverSession) -> *mut c_char,
        pub nxuskit_solver_explanation:
            unsafe extern "C" fn(session: *mut NxuskitSolverSession) -> *mut c_char,
        pub nxuskit_solver_add_assumptions: unsafe extern "C" fn(
            session: *mut NxuskitSolverSession,
            assumptions_json: *const c_char,
        ) -> bool,

        // -------------------------------------------------------------------
        // CLIPS Session API (u64 session handles)
        // -------------------------------------------------------------------

        // Session lifecycle
        pub nxuskit_clips_session_create: unsafe extern "C" fn() -> u64,
        pub nxuskit_clips_session_destroy: unsafe extern "C" fn(session: u64),
        pub nxuskit_clips_session_reset: unsafe extern "C" fn(session: u64) -> i32,
        pub nxuskit_clips_session_clear: unsafe extern "C" fn(session: u64) -> i32,
        pub nxuskit_clips_session_info: unsafe extern "C" fn(session: u64) -> *mut c_char,

        // Session construct loading
        pub nxuskit_clips_session_load_file:
            unsafe extern "C" fn(session: u64, path: *const c_char) -> i32,
        pub nxuskit_clips_session_load_string:
            unsafe extern "C" fn(session: u64, constructs: *const c_char) -> i32,
        pub nxuskit_clips_session_load_binary:
            unsafe extern "C" fn(session: u64, path: *const c_char) -> i32,
        pub nxuskit_clips_session_save_binary:
            unsafe extern "C" fn(session: u64, path: *const c_char) -> i32,
        pub nxuskit_clips_session_build:
            unsafe extern "C" fn(session: u64, construct: *const c_char) -> i32,
        pub nxuskit_clips_session_batch:
            unsafe extern "C" fn(session: u64, path: *const c_char) -> i32,

        // Session fact operations
        pub nxuskit_clips_fact_assert_string:
            unsafe extern "C" fn(session: u64, fact: *const c_char) -> i64,
        pub nxuskit_clips_fact_assert_structured: unsafe extern "C" fn(
            session: u64,
            template: *const c_char,
            slots_json: *const c_char,
        ) -> i64,
        pub nxuskit_clips_fact_retract: unsafe extern "C" fn(session: u64, index: i64) -> i32,
        pub nxuskit_clips_fact_retract_by_template:
            unsafe extern "C" fn(session: u64, template: *const c_char) -> i32,
        pub nxuskit_clips_fact_exists: unsafe extern "C" fn(session: u64, index: i64) -> bool,
        pub nxuskit_clips_fact_get_slot:
            unsafe extern "C" fn(session: u64, index: i64, slot: *const c_char) -> *mut c_char,
        pub nxuskit_clips_fact_slot_values:
            unsafe extern "C" fn(session: u64, index: i64) -> *mut c_char,
        pub nxuskit_clips_fact_pp_form:
            unsafe extern "C" fn(session: u64, index: i64) -> *mut c_char,
        pub nxuskit_clips_fact_index: unsafe extern "C" fn(session: u64, index: i64) -> i64,
        pub nxuskit_clips_facts_list: unsafe extern "C" fn(session: u64) -> *mut c_char,
        pub nxuskit_clips_facts_by_template:
            unsafe extern "C" fn(session: u64, template: *const c_char) -> *mut c_char,

        // Session template operations
        pub nxuskit_clips_template_exists:
            unsafe extern "C" fn(session: u64, name: *const c_char) -> bool,
        pub nxuskit_clips_template_list: unsafe extern "C" fn(session: u64) -> *mut c_char,
        pub nxuskit_clips_template_slot_names:
            unsafe extern "C" fn(session: u64, name: *const c_char) -> *mut c_char,
        pub nxuskit_clips_template_slot_info:
            unsafe extern "C" fn(session: u64, name: *const c_char) -> *mut c_char,
        pub nxuskit_clips_template_facts:
            unsafe extern "C" fn(session: u64, name: *const c_char) -> *mut c_char,
        pub nxuskit_clips_template_pp_form:
            unsafe extern "C" fn(session: u64, name: *const c_char) -> *mut c_char,

        // Session rule operations
        pub nxuskit_clips_rule_exists:
            unsafe extern "C" fn(session: u64, name: *const c_char) -> bool,
        pub nxuskit_clips_rule_list: unsafe extern "C" fn(session: u64) -> *mut c_char,
        pub nxuskit_clips_rule_times_fired:
            unsafe extern "C" fn(session: u64, name: *const c_char) -> i64,
        pub nxuskit_clips_rule_breakpoint_set:
            unsafe extern "C" fn(session: u64, name: *const c_char) -> i32,
        pub nxuskit_clips_rule_breakpoint_remove:
            unsafe extern "C" fn(session: u64, name: *const c_char) -> i32,
        pub nxuskit_clips_rule_has_breakpoint:
            unsafe extern "C" fn(session: u64, name: *const c_char) -> bool,
        pub nxuskit_clips_rule_refresh:
            unsafe extern "C" fn(session: u64, name: *const c_char) -> i32,
        pub nxuskit_clips_rule_pp_form:
            unsafe extern "C" fn(session: u64, name: *const c_char) -> *mut c_char,
        pub nxuskit_clips_rule_delete:
            unsafe extern "C" fn(session: u64, name: *const c_char) -> i32,

        // Session execution & agenda
        pub nxuskit_clips_session_run: unsafe extern "C" fn(session: u64, limit: i64) -> i64,
        pub nxuskit_clips_session_halt: unsafe extern "C" fn(session: u64) -> i32,
        pub nxuskit_clips_agenda_size: unsafe extern "C" fn(session: u64) -> i64,
        pub nxuskit_clips_agenda_clear: unsafe extern "C" fn(session: u64) -> i32,
        pub nxuskit_clips_agenda_reorder: unsafe extern "C" fn(session: u64) -> i32,
        pub nxuskit_clips_strategy_get: unsafe extern "C" fn(session: u64) -> *mut c_char,
        pub nxuskit_clips_strategy_set:
            unsafe extern "C" fn(session: u64, strategy: *const c_char) -> i32,
        pub nxuskit_clips_salience_mode_get: unsafe extern "C" fn(session: u64) -> *mut c_char,
        pub nxuskit_clips_salience_mode_set:
            unsafe extern "C" fn(session: u64, mode: *const c_char) -> i32,

        // Session eval
        pub nxuskit_clips_eval:
            unsafe extern "C" fn(session: u64, expression: *const c_char) -> *mut c_char,
        pub nxuskit_clips_function_call: unsafe extern "C" fn(
            session: u64,
            name: *const c_char,
            args_json: *const c_char,
        ) -> *mut c_char,

        // Session settings
        pub nxuskit_clips_fact_duplication_get: unsafe extern "C" fn(session: u64) -> bool,
        pub nxuskit_clips_fact_duplication_set:
            unsafe extern "C" fn(session: u64, allow: bool) -> i32,
        pub nxuskit_clips_reset_globals_get: unsafe extern "C" fn(session: u64) -> bool,
        pub nxuskit_clips_reset_globals_set: unsafe extern "C" fn(session: u64, reset: bool) -> i32,

        // JSON loading
        pub nxuskit_clips_session_load_json:
            unsafe extern "C" fn(session: u64, json: *const c_char) -> i32,

        // Cache
        pub nxuskit_clips_session_preload:
            unsafe extern "C" fn(name: *const c_char, rules_json: *const c_char) -> i32,
        pub nxuskit_clips_session_get_cached: unsafe extern "C" fn(name: *const c_char) -> u64,
        pub nxuskit_clips_session_cache_remove: unsafe extern "C" fn(name: *const c_char) -> i32,

        // Module & focus
        pub nxuskit_clips_module_exists:
            unsafe extern "C" fn(session: u64, name: *const c_char) -> bool,
        pub nxuskit_clips_module_list: unsafe extern "C" fn(session: u64) -> *mut c_char,
        pub nxuskit_clips_module_current_get: unsafe extern "C" fn(session: u64) -> *mut c_char,
        pub nxuskit_clips_module_current_set:
            unsafe extern "C" fn(session: u64, name: *const c_char) -> i32,
        pub nxuskit_clips_focus_push:
            unsafe extern "C" fn(session: u64, module_name: *const c_char) -> i32,
        pub nxuskit_clips_focus_get: unsafe extern "C" fn(session: u64) -> *mut c_char,
        pub nxuskit_clips_focus_pop: unsafe extern "C" fn(session: u64) -> i32,
        pub nxuskit_clips_focus_clear: unsafe extern "C" fn(session: u64) -> i32,

        // Global variables
        pub nxuskit_clips_global_exists:
            unsafe extern "C" fn(session: u64, name: *const c_char) -> bool,
        pub nxuskit_clips_global_list: unsafe extern "C" fn(session: u64) -> *mut c_char,
        pub nxuskit_clips_global_get_value:
            unsafe extern "C" fn(session: u64, name: *const c_char) -> *mut c_char,
        pub nxuskit_clips_global_set_value: unsafe extern "C" fn(
            session: u64,
            name: *const c_char,
            value_json: *const c_char,
        ) -> i32,

        // Watch & diagnostics
        pub nxuskit_clips_watch: unsafe extern "C" fn(session: u64, item: *const c_char) -> i32,
        pub nxuskit_clips_unwatch: unsafe extern "C" fn(session: u64, item: *const c_char) -> i32,
        pub nxuskit_clips_dribble_on:
            unsafe extern "C" fn(session: u64, file_path: *const c_char) -> i32,
        pub nxuskit_clips_dribble_off: unsafe extern "C" fn(session: u64) -> i32,
    }

    // SAFETY: The SDK functions are thread-safe (each provider handle is
    // independent, and thread-local error storage is used).
    unsafe impl Send for SdkFunctions {}
    unsafe impl Sync for SdkFunctions {}

    /// Global singleton holding the loaded SDK.
    static SDK: OnceLock<Result<SdkFunctions, String>> = OnceLock::new();

    /// Returns the library file name for the current platform.
    fn lib_name() -> &'static str {
        if cfg!(target_os = "macos") {
            "libnxuskit.dylib"
        } else if cfg!(target_os = "windows") {
            "nxuskit.dll"
        } else {
            "libnxuskit.so"
        }
    }

    /// Canonicalize a path from an environment variable.
    ///
    /// Relative paths are resolved against the current working directory and
    /// converted to absolute form. This prevents failures when `cargo` or
    /// the runtime changes the working directory.
    fn resolve_dir(raw: &str) -> String {
        let p = std::path::Path::new(raw);
        match p.canonicalize() {
            Ok(abs) => abs.to_string_lossy().into_owned(),
            Err(_) => raw.to_string(), // Fall back to the raw value.
        }
    }

    /// Discover library search paths in priority order.
    fn search_paths() -> Vec<String> {
        let mut paths = Vec::new();

        // Priority 1: Explicit lib directory.
        if let Ok(dir) = std::env::var("NXUSKIT_LIB_DIR") {
            let dir = resolve_dir(&dir);
            paths.push(format!("{dir}/{}", lib_name()));
        }

        // Priority 2: SDK root with /lib subdirectory.
        if let Ok(sdk) = std::env::var("NXUSKIT_SDK_DIR") {
            let sdk = resolve_dir(&sdk);
            paths.push(format!("{sdk}/lib/{}", lib_name()));
        }

        // Priority 3: Standard install path (~/.nxuskit/sdk/current/lib/).
        if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            let sdk_lib = std::path::PathBuf::from(home)
                .join(".nxuskit")
                .join("sdk")
                .join("current")
                .join("lib")
                .join(lib_name());
            if sdk_lib.is_file() {
                paths.push(sdk_lib.to_string_lossy().into_owned());
            }
        }

        // Priority 4: Bare library name (system search path / LD_LIBRARY_PATH / DYLD_LIBRARY_PATH).
        paths.push(lib_name().to_string());

        paths
    }

    /// Load a single symbol from the library.
    ///
    /// # Safety
    /// The caller must ensure the symbol has the correct type.
    unsafe fn load_sym<T: Copy>(lib: &Library, name: &[u8]) -> Result<T, String> {
        unsafe {
            let sym: Symbol<T> = lib.get(name).map_err(|e| {
                format!(
                    "Failed to load symbol {}: {e}",
                    String::from_utf8_lossy(name)
                )
            })?;
            Ok(*sym)
        }
    }

    /// Load the SDK library and resolve all function pointers.
    fn load_sdk() -> Result<SdkFunctions, String> {
        let paths = search_paths();

        let mut last_err = String::new();
        for path in &paths {
            match unsafe { Library::new(path) } {
                Ok(lib) => {
                    // Resolve all functions (LLM + CLIPS SDK).
                    let funcs = unsafe {
                        SdkFunctions {
                            // Core introspection
                            nxuskit_version: load_sym(&lib, b"nxuskit_version\0")?,
                            nxuskit_abi_version: load_sym(&lib, b"nxuskit_abi_version\0")?,
                            nxuskit_edition: load_sym(&lib, b"nxuskit_edition\0")?,
                            nxuskit_capabilities: load_sym(&lib, b"nxuskit_capabilities\0")?,
                            nxuskit_build_info: load_sym(&lib, b"nxuskit_build_info\0")?,
                            nxuskit_entitlement_info: load_sym(
                                &lib,
                                b"nxuskit_entitlement_info\0",
                            )?,
                            // License functions
                            nxuskit_license_resolve: load_sym(&lib, b"nxuskit_license_resolve\0")?,
                            nxuskit_license_validate: load_sym(
                                &lib,
                                b"nxuskit_license_validate\0",
                            )?,
                            nxuskit_license_machine_id: load_sym(
                                &lib,
                                b"nxuskit_license_machine_id\0",
                            )?,
                            nxuskit_license_activate: load_sym(
                                &lib,
                                b"nxuskit_license_activate\0",
                            )?,
                            nxuskit_license_deactivate: load_sym(
                                &lib,
                                b"nxuskit_license_deactivate\0",
                            )?,
                            nxuskit_license_trial_issue: load_sym(
                                &lib,
                                b"nxuskit_license_trial_issue\0",
                            )?,
                            nxuskit_license_trial_activate: load_sym(
                                &lib,
                                b"nxuskit_license_trial_activate\0",
                            )?,
                            // Auth helper functions
                            nxuskit_auth_set_credential: load_sym(
                                &lib,
                                b"nxuskit_auth_set_credential\0",
                            )?,
                            nxuskit_auth_remove_credential: load_sym(
                                &lib,
                                b"nxuskit_auth_remove_credential\0",
                            )?,
                            nxuskit_auth_resolve: load_sym(&lib, b"nxuskit_auth_resolve\0")?,
                            nxuskit_auth_status: load_sym(&lib, b"nxuskit_auth_status\0")?,
                            nxuskit_auth_status_all: load_sym(&lib, b"nxuskit_auth_status_all\0")?,
                            nxuskit_auth_providers: load_sym(&lib, b"nxuskit_auth_providers\0")?,
                            // LLM API functions
                            nxuskit_create_provider: load_sym(&lib, b"nxuskit_create_provider\0")?,
                            nxuskit_free_provider: load_sym(&lib, b"nxuskit_free_provider\0")?,
                            nxuskit_chat: load_sym(&lib, b"nxuskit_chat\0")?,
                            nxuskit_response_json: load_sym(&lib, b"nxuskit_response_json\0")?,
                            nxuskit_free_response: load_sym(&lib, b"nxuskit_free_response\0")?,
                            nxuskit_chat_stream: load_sym(&lib, b"nxuskit_chat_stream\0")?,
                            nxuskit_cancel_stream: load_sym(&lib, b"nxuskit_cancel_stream\0")?,
                            nxuskit_free_stream: load_sym(&lib, b"nxuskit_free_stream\0")?,
                            nxuskit_list_models: load_sym(&lib, b"nxuskit_list_models\0")?,
                            nxuskit_last_error: load_sym(&lib, b"nxuskit_last_error\0")?,
                            nxuskit_free_string: load_sym(&lib, b"nxuskit_free_string\0")?,
                            // Plugin SDK
                            nxuskit_plugin_load_dir: load_sym(&lib, b"nxuskit_plugin_load_dir\0")?,
                            nxuskit_plugin_list: load_sym(&lib, b"nxuskit_plugin_list\0")?,
                            nxuskit_plugin_info: load_sym(&lib, b"nxuskit_plugin_info\0")?,
                            nxuskit_plugin_count: load_sym(&lib, b"nxuskit_plugin_count\0")?,
                            nxuskit_plugin_loaded: load_sym(&lib, b"nxuskit_plugin_loaded\0")?,
                            nxuskit_plugin_unload_all: load_sym(
                                &lib,
                                b"nxuskit_plugin_unload_all\0",
                            )?,
                            nxuskit_plugin_set_trust_mode: load_sym(
                                &lib,
                                b"nxuskit_plugin_set_trust_mode\0",
                            )?,
                            nxuskit_plugin_get_trust_mode: load_sym(
                                &lib,
                                b"nxuskit_plugin_get_trust_mode\0",
                            )?,
                            nxuskit_oauth_start: load_sym(&lib, b"nxuskit_oauth_start\0")?,
                            nxuskit_oauth_status: load_sym(&lib, b"nxuskit_oauth_status\0")?,
                            nxuskit_oauth_revoke: load_sym(&lib, b"nxuskit_oauth_revoke\0")?,
                            nxuskit_plugin_load_dir_trusted: load_sym(
                                &lib,
                                b"nxuskit_plugin_load_dir_trusted\0",
                            )?,
                            // Bayesian Network SDK
                            nxuskit_bn_net_create: load_sym(&lib, b"nxuskit_bn_net_create\0")?,
                            nxuskit_bn_net_destroy: load_sym(&lib, b"nxuskit_bn_net_destroy\0")?,
                            nxuskit_bn_net_load_file: load_sym(
                                &lib,
                                b"nxuskit_bn_net_load_file\0",
                            )?,
                            nxuskit_bn_net_num_variables: load_sym(
                                &lib,
                                b"nxuskit_bn_net_num_variables\0",
                            )?,
                            nxuskit_bn_net_variables: load_sym(
                                &lib,
                                b"nxuskit_bn_net_variables\0",
                            )?,
                            nxuskit_bn_net_variable_states: load_sym(
                                &lib,
                                b"nxuskit_bn_net_variable_states\0",
                            )?,
                            nxuskit_bn_ev_create: load_sym(&lib, b"nxuskit_bn_ev_create\0")?,
                            nxuskit_bn_ev_destroy: load_sym(&lib, b"nxuskit_bn_ev_destroy\0")?,
                            nxuskit_bn_ev_set_discrete: load_sym(
                                &lib,
                                b"nxuskit_bn_ev_set_discrete\0",
                            )?,
                            nxuskit_bn_ev_retract: load_sym(&lib, b"nxuskit_bn_ev_retract\0")?,
                            nxuskit_bn_ev_clear: load_sym(&lib, b"nxuskit_bn_ev_clear\0")?,
                            nxuskit_bn_infer: load_sym(&lib, b"nxuskit_bn_infer\0")?,
                            nxuskit_bn_result_destroy: load_sym(
                                &lib,
                                b"nxuskit_bn_result_destroy\0",
                            )?,
                            nxuskit_bn_result_json: load_sym(&lib, b"nxuskit_bn_result_json\0")?,
                            nxuskit_bn_result_query: load_sym(&lib, b"nxuskit_bn_result_query\0")?,
                            nxuskit_bn_result_num_variables: load_sym(
                                &lib,
                                b"nxuskit_bn_result_num_variables\0",
                            )?,
                            nxuskit_bn_result_next: load_sym(&lib, b"nxuskit_bn_result_next\0")?,
                            nxuskit_bn_result_reset: load_sym(&lib, b"nxuskit_bn_result_reset\0")?,
                            // BN Part 2 extensions
                            nxuskit_bn_net_save_file: load_sym(
                                &lib,
                                b"nxuskit_bn_net_save_file\0",
                            )?,
                            nxuskit_bn_ev_set_continuous: load_sym(
                                &lib,
                                b"nxuskit_bn_ev_set_continuous\0",
                            )?,
                            nxuskit_bn_net_add_gaussian_variable: load_sym(
                                &lib,
                                b"nxuskit_bn_net_add_gaussian_variable\0",
                            )?,
                            nxuskit_bn_net_set_gaussian_weight: load_sym(
                                &lib,
                                b"nxuskit_bn_net_set_gaussian_weight\0",
                            )?,
                            nxuskit_bn_result_mean: load_sym(&lib, b"nxuskit_bn_result_mean\0")?,
                            nxuskit_bn_result_variance: load_sym(
                                &lib,
                                b"nxuskit_bn_result_variance\0",
                            )?,
                            nxuskit_bn_result_continuous_marginal: load_sym(
                                &lib,
                                b"nxuskit_bn_result_continuous_marginal\0",
                            )?,
                            nxuskit_bn_infer_with_config: load_sym(
                                &lib,
                                b"nxuskit_bn_infer_with_config\0",
                            )?,
                            // BN Streaming inference
                            nxuskit_bn_infer_stream: load_sym(&lib, b"nxuskit_bn_infer_stream\0")?,
                            // BN Structure & Parameter Learning
                            nxuskit_bn_search_structure: load_sym(
                                &lib,
                                b"nxuskit_bn_search_structure\0",
                            )?,
                            nxuskit_bn_learn_mle: load_sym(&lib, b"nxuskit_bn_learn_mle\0")?,
                            nxuskit_bn_log_likelihood: load_sym(
                                &lib,
                                b"nxuskit_bn_log_likelihood\0",
                            )?,
                            // ZEN SDK: Stateless evaluation
                            nxuskit_zen_evaluate: load_sym(&lib, b"nxuskit_zen_evaluate\0")?,
                            nxuskit_zen_free_result: load_sym(&lib, b"nxuskit_zen_free_result\0")?,
                            // Solver SDK: Session lifecycle
                            nxuskit_solver_session_create: load_sym(
                                &lib,
                                b"nxuskit_solver_session_create\0",
                            )?,
                            nxuskit_solver_session_destroy: load_sym(
                                &lib,
                                b"nxuskit_solver_session_destroy\0",
                            )?,
                            nxuskit_solver_reset: load_sym(&lib, b"nxuskit_solver_reset\0")?,
                            // Solver SDK: Model building
                            nxuskit_solver_add_variables: load_sym(
                                &lib,
                                b"nxuskit_solver_add_variables\0",
                            )?,
                            nxuskit_solver_add_constraints: load_sym(
                                &lib,
                                b"nxuskit_solver_add_constraints\0",
                            )?,
                            nxuskit_solver_set_objective: load_sym(
                                &lib,
                                b"nxuskit_solver_set_objective\0",
                            )?,
                            nxuskit_solver_retract: load_sym(&lib, b"nxuskit_solver_retract\0")?,
                            // Solver SDK: Scoping
                            nxuskit_solver_push: load_sym(&lib, b"nxuskit_solver_push\0")?,
                            nxuskit_solver_pop: load_sym(&lib, b"nxuskit_solver_pop\0")?,
                            // Solver SDK: Execution
                            nxuskit_solver_solve: load_sym(&lib, b"nxuskit_solver_solve\0")?,
                            nxuskit_solver_solve_stream: load_sym(
                                &lib,
                                b"nxuskit_solver_solve_stream\0",
                            )?,
                            // Solver SDK: Introspection
                            nxuskit_solver_variables: load_sym(
                                &lib,
                                b"nxuskit_solver_variables\0",
                            )?,
                            nxuskit_solver_constraints: load_sym(
                                &lib,
                                b"nxuskit_solver_constraints\0",
                            )?,
                            nxuskit_solver_status: load_sym(&lib, b"nxuskit_solver_status\0")?,
                            nxuskit_solver_capabilities: load_sym(
                                &lib,
                                b"nxuskit_solver_capabilities\0",
                            )?,
                            nxuskit_solver_num_variables: load_sym(
                                &lib,
                                b"nxuskit_solver_num_variables\0",
                            )?,
                            nxuskit_solver_num_constraints: load_sym(
                                &lib,
                                b"nxuskit_solver_num_constraints\0",
                            )?,
                            // Solver SDK: Multi-objective, Explanation & Assumptions
                            nxuskit_solver_add_objective: load_sym(
                                &lib,
                                b"nxuskit_solver_add_objective\0",
                            )?,
                            nxuskit_solver_retract_objective: load_sym(
                                &lib,
                                b"nxuskit_solver_retract_objective\0",
                            )?,
                            nxuskit_solver_objectives: load_sym(
                                &lib,
                                b"nxuskit_solver_objectives\0",
                            )?,
                            nxuskit_solver_explanation: load_sym(
                                &lib,
                                b"nxuskit_solver_explanation\0",
                            )?,
                            nxuskit_solver_add_assumptions: load_sym(
                                &lib,
                                b"nxuskit_solver_add_assumptions\0",
                            )?,
                            // CLIPS Session API (u64 session handles)
                            // Session lifecycle
                            nxuskit_clips_session_create: load_sym(
                                &lib,
                                b"nxuskit_clips_session_create\0",
                            )?,
                            nxuskit_clips_session_destroy: load_sym(
                                &lib,
                                b"nxuskit_clips_session_destroy\0",
                            )?,
                            nxuskit_clips_session_reset: load_sym(
                                &lib,
                                b"nxuskit_clips_session_reset\0",
                            )?,
                            nxuskit_clips_session_clear: load_sym(
                                &lib,
                                b"nxuskit_clips_session_clear\0",
                            )?,
                            nxuskit_clips_session_info: load_sym(
                                &lib,
                                b"nxuskit_clips_session_info\0",
                            )?,
                            // Session construct loading
                            nxuskit_clips_session_load_file: load_sym(
                                &lib,
                                b"nxuskit_clips_session_load_file\0",
                            )?,
                            nxuskit_clips_session_load_string: load_sym(
                                &lib,
                                b"nxuskit_clips_session_load_string\0",
                            )?,
                            nxuskit_clips_session_load_binary: load_sym(
                                &lib,
                                b"nxuskit_clips_session_load_binary\0",
                            )?,
                            nxuskit_clips_session_save_binary: load_sym(
                                &lib,
                                b"nxuskit_clips_session_save_binary\0",
                            )?,
                            nxuskit_clips_session_build: load_sym(
                                &lib,
                                b"nxuskit_clips_session_build\0",
                            )?,
                            nxuskit_clips_session_batch: load_sym(
                                &lib,
                                b"nxuskit_clips_session_batch\0",
                            )?,
                            // Session fact operations
                            nxuskit_clips_fact_assert_string: load_sym(
                                &lib,
                                b"nxuskit_clips_fact_assert_string\0",
                            )?,
                            nxuskit_clips_fact_assert_structured: load_sym(
                                &lib,
                                b"nxuskit_clips_fact_assert_structured\0",
                            )?,
                            nxuskit_clips_fact_retract: load_sym(
                                &lib,
                                b"nxuskit_clips_fact_retract\0",
                            )?,
                            nxuskit_clips_fact_retract_by_template: load_sym(
                                &lib,
                                b"nxuskit_clips_fact_retract_by_template\0",
                            )?,
                            nxuskit_clips_fact_exists: load_sym(
                                &lib,
                                b"nxuskit_clips_fact_exists\0",
                            )?,
                            nxuskit_clips_fact_get_slot: load_sym(
                                &lib,
                                b"nxuskit_clips_fact_get_slot\0",
                            )?,
                            nxuskit_clips_fact_slot_values: load_sym(
                                &lib,
                                b"nxuskit_clips_fact_slot_values\0",
                            )?,
                            nxuskit_clips_fact_pp_form: load_sym(
                                &lib,
                                b"nxuskit_clips_fact_pp_form\0",
                            )?,
                            nxuskit_clips_fact_index: load_sym(
                                &lib,
                                b"nxuskit_clips_fact_index\0",
                            )?,
                            nxuskit_clips_facts_list: load_sym(
                                &lib,
                                b"nxuskit_clips_facts_list\0",
                            )?,
                            nxuskit_clips_facts_by_template: load_sym(
                                &lib,
                                b"nxuskit_clips_facts_by_template\0",
                            )?,
                            // Session template operations
                            nxuskit_clips_template_exists: load_sym(
                                &lib,
                                b"nxuskit_clips_template_exists\0",
                            )?,
                            nxuskit_clips_template_list: load_sym(
                                &lib,
                                b"nxuskit_clips_template_list\0",
                            )?,
                            nxuskit_clips_template_slot_names: load_sym(
                                &lib,
                                b"nxuskit_clips_template_slot_names\0",
                            )?,
                            nxuskit_clips_template_slot_info: load_sym(
                                &lib,
                                b"nxuskit_clips_template_slot_info\0",
                            )?,
                            nxuskit_clips_template_facts: load_sym(
                                &lib,
                                b"nxuskit_clips_template_facts\0",
                            )?,
                            nxuskit_clips_template_pp_form: load_sym(
                                &lib,
                                b"nxuskit_clips_template_pp_form\0",
                            )?,
                            // Session rule operations
                            nxuskit_clips_rule_exists: load_sym(
                                &lib,
                                b"nxuskit_clips_rule_exists\0",
                            )?,
                            nxuskit_clips_rule_list: load_sym(&lib, b"nxuskit_clips_rule_list\0")?,
                            nxuskit_clips_rule_times_fired: load_sym(
                                &lib,
                                b"nxuskit_clips_rule_times_fired\0",
                            )?,
                            nxuskit_clips_rule_breakpoint_set: load_sym(
                                &lib,
                                b"nxuskit_clips_rule_breakpoint_set\0",
                            )?,
                            nxuskit_clips_rule_breakpoint_remove: load_sym(
                                &lib,
                                b"nxuskit_clips_rule_breakpoint_remove\0",
                            )?,
                            nxuskit_clips_rule_has_breakpoint: load_sym(
                                &lib,
                                b"nxuskit_clips_rule_has_breakpoint\0",
                            )?,
                            nxuskit_clips_rule_refresh: load_sym(
                                &lib,
                                b"nxuskit_clips_rule_refresh\0",
                            )?,
                            nxuskit_clips_rule_pp_form: load_sym(
                                &lib,
                                b"nxuskit_clips_rule_pp_form\0",
                            )?,
                            nxuskit_clips_rule_delete: load_sym(
                                &lib,
                                b"nxuskit_clips_rule_delete\0",
                            )?,
                            // Session execution & agenda
                            nxuskit_clips_session_run: load_sym(
                                &lib,
                                b"nxuskit_clips_session_run\0",
                            )?,
                            nxuskit_clips_session_halt: load_sym(
                                &lib,
                                b"nxuskit_clips_session_halt\0",
                            )?,
                            nxuskit_clips_agenda_size: load_sym(
                                &lib,
                                b"nxuskit_clips_agenda_size\0",
                            )?,
                            nxuskit_clips_agenda_clear: load_sym(
                                &lib,
                                b"nxuskit_clips_agenda_clear\0",
                            )?,
                            nxuskit_clips_agenda_reorder: load_sym(
                                &lib,
                                b"nxuskit_clips_agenda_reorder\0",
                            )?,
                            nxuskit_clips_strategy_get: load_sym(
                                &lib,
                                b"nxuskit_clips_strategy_get\0",
                            )?,
                            nxuskit_clips_strategy_set: load_sym(
                                &lib,
                                b"nxuskit_clips_strategy_set\0",
                            )?,
                            nxuskit_clips_salience_mode_get: load_sym(
                                &lib,
                                b"nxuskit_clips_salience_mode_get\0",
                            )?,
                            nxuskit_clips_salience_mode_set: load_sym(
                                &lib,
                                b"nxuskit_clips_salience_mode_set\0",
                            )?,
                            // Session eval
                            nxuskit_clips_eval: load_sym(&lib, b"nxuskit_clips_eval\0")?,
                            nxuskit_clips_function_call: load_sym(
                                &lib,
                                b"nxuskit_clips_function_call\0",
                            )?,
                            // Session settings
                            nxuskit_clips_fact_duplication_get: load_sym(
                                &lib,
                                b"nxuskit_clips_fact_duplication_get\0",
                            )?,
                            nxuskit_clips_fact_duplication_set: load_sym(
                                &lib,
                                b"nxuskit_clips_fact_duplication_set\0",
                            )?,
                            nxuskit_clips_reset_globals_get: load_sym(
                                &lib,
                                b"nxuskit_clips_reset_globals_get\0",
                            )?,
                            nxuskit_clips_reset_globals_set: load_sym(
                                &lib,
                                b"nxuskit_clips_reset_globals_set\0",
                            )?,
                            // JSON loading
                            nxuskit_clips_session_load_json: load_sym(
                                &lib,
                                b"nxuskit_clips_session_load_json\0",
                            )?,
                            // Cache
                            nxuskit_clips_session_preload: load_sym(
                                &lib,
                                b"nxuskit_clips_session_preload\0",
                            )?,
                            nxuskit_clips_session_get_cached: load_sym(
                                &lib,
                                b"nxuskit_clips_session_get_cached\0",
                            )?,
                            nxuskit_clips_session_cache_remove: load_sym(
                                &lib,
                                b"nxuskit_clips_session_cache_remove\0",
                            )?,
                            // Module & focus
                            nxuskit_clips_module_exists: load_sym(
                                &lib,
                                b"nxuskit_clips_module_exists\0",
                            )?,
                            nxuskit_clips_module_list: load_sym(
                                &lib,
                                b"nxuskit_clips_module_list\0",
                            )?,
                            nxuskit_clips_module_current_get: load_sym(
                                &lib,
                                b"nxuskit_clips_module_current_get\0",
                            )?,
                            nxuskit_clips_module_current_set: load_sym(
                                &lib,
                                b"nxuskit_clips_module_current_set\0",
                            )?,
                            nxuskit_clips_focus_push: load_sym(
                                &lib,
                                b"nxuskit_clips_focus_push\0",
                            )?,
                            nxuskit_clips_focus_get: load_sym(&lib, b"nxuskit_clips_focus_get\0")?,
                            nxuskit_clips_focus_pop: load_sym(&lib, b"nxuskit_clips_focus_pop\0")?,
                            nxuskit_clips_focus_clear: load_sym(
                                &lib,
                                b"nxuskit_clips_focus_clear\0",
                            )?,
                            // Global variables
                            nxuskit_clips_global_exists: load_sym(
                                &lib,
                                b"nxuskit_clips_global_exists\0",
                            )?,
                            nxuskit_clips_global_list: load_sym(
                                &lib,
                                b"nxuskit_clips_global_list\0",
                            )?,
                            nxuskit_clips_global_get_value: load_sym(
                                &lib,
                                b"nxuskit_clips_global_get_value\0",
                            )?,
                            nxuskit_clips_global_set_value: load_sym(
                                &lib,
                                b"nxuskit_clips_global_set_value\0",
                            )?,
                            // Watch & diagnostics
                            nxuskit_clips_watch: load_sym(&lib, b"nxuskit_clips_watch\0")?,
                            nxuskit_clips_unwatch: load_sym(&lib, b"nxuskit_clips_unwatch\0")?,
                            nxuskit_clips_dribble_on: load_sym(
                                &lib,
                                b"nxuskit_clips_dribble_on\0",
                            )?,
                            nxuskit_clips_dribble_off: load_sym(
                                &lib,
                                b"nxuskit_clips_dribble_off\0",
                            )?,
                            _lib: lib,
                        }
                    };
                    return Ok(funcs);
                }
                Err(e) => {
                    last_err = format!("{path}: {e}");
                }
            }
        }

        Err(format!(
            "Could not load nxusKit SDK library. Searched: [{}]. Last error: {last_err}. \
             Set NXUSKIT_LIB_DIR or NXUSKIT_SDK_DIR to the SDK location.",
            paths.join(", ")
        ))
    }

    /// Get the loaded SDK functions, initializing on first call.
    pub(crate) fn sdk() -> Result<&'static SdkFunctions, crate::NxuskitError> {
        let result = SDK.get_or_init(load_sdk);
        match result {
            Ok(funcs) => Ok(funcs),
            Err(msg) => Err(crate::NxuskitError::LibraryNotFound {
                message: msg.clone(),
            }),
        }
    }

    /// Get the loaded SDK functions for cleanup/destructor paths where errors
    /// cannot be propagated.
    ///
    /// # Panics
    ///
    /// Panics if the SDK was never successfully loaded. In practice this cannot
    /// happen because `NxuskitProvider::new()` validates the SDK on construction.
    pub(crate) fn sdk_unchecked() -> &'static SdkFunctions {
        sdk().expect("SDK not loaded (cleanup path — provider was created without a loaded SDK)")
    }
}
