//! Public CE solver ABI stubs.
//!
//! The Pro solver implementation is not shipped in public CE source or release
//! bundles. These symbols remain available for ABI stability and return the
//! standard unavailable error.

use std::ffi::{c_char, c_void};

use crate::error;

/// Opaque solver session handle for ABI compatibility.
pub struct NxuskitSolverSession {
    _private: (),
}

fn set_unavailable() {
    error::set_last_error("feature_unavailable", "solver", None);
}

fn unavailable_bool() -> bool {
    set_unavailable();
    false
}

fn unavailable_ptr() -> *mut c_char {
    set_unavailable();
    std::ptr::null_mut()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_solver_session_create(
    _config_json: *const c_char,
) -> *mut NxuskitSolverSession {
    unavailable_ptr().cast()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_solver_session_destroy(_session: *mut NxuskitSolverSession) {}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_solver_add_variables(
    _session: *mut NxuskitSolverSession,
    _variables_json: *const c_char,
) -> bool {
    unavailable_bool()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_solver_add_constraints(
    _session: *mut NxuskitSolverSession,
    _constraints_json: *const c_char,
) -> bool {
    unavailable_bool()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_solver_set_objective(
    _session: *mut NxuskitSolverSession,
    _objective_json: *const c_char,
) -> bool {
    unavailable_bool()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_solver_retract(
    _session: *mut NxuskitSolverSession,
    _names_json: *const c_char,
) -> bool {
    unavailable_bool()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_solver_push(_session: *mut NxuskitSolverSession) -> bool {
    unavailable_bool()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_solver_pop(_session: *mut NxuskitSolverSession) -> bool {
    unavailable_bool()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_solver_solve(
    _session: *mut NxuskitSolverSession,
    _config_json: *const c_char,
) -> *mut c_char {
    unavailable_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_solver_solve_stream(
    _session: *mut NxuskitSolverSession,
    _config_json: *const c_char,
    _on_chunk: Option<unsafe extern "C" fn(*const c_char, *mut c_void) -> i32>,
    _on_done: Option<unsafe extern "C" fn(*const c_char, *mut c_void)>,
    _user_data: *mut c_void,
) -> bool {
    unavailable_bool()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_solver_reset(_session: *mut NxuskitSolverSession) -> bool {
    unavailable_bool()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_solver_variables(
    _session: *mut NxuskitSolverSession,
) -> *mut c_char {
    unavailable_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_solver_constraints(
    _session: *mut NxuskitSolverSession,
) -> *mut c_char {
    unavailable_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_solver_status(
    _session: *mut NxuskitSolverSession,
) -> *mut c_char {
    unavailable_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_solver_capabilities(
    _session: *mut NxuskitSolverSession,
) -> *mut c_char {
    unavailable_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_solver_num_variables(
    _session: *mut NxuskitSolverSession,
) -> i64 {
    -1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_solver_num_constraints(
    _session: *mut NxuskitSolverSession,
) -> i64 {
    -1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_solver_add_objective(
    _session: *mut NxuskitSolverSession,
    _objective_json: *const c_char,
) -> bool {
    unavailable_bool()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_solver_retract_objective(
    _session: *mut NxuskitSolverSession,
    _name: *const c_char,
) -> bool {
    unavailable_bool()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_solver_objectives(
    _session: *mut NxuskitSolverSession,
) -> *mut c_char {
    unavailable_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_solver_explanation(
    _session: *mut NxuskitSolverSession,
) -> *mut c_char {
    unavailable_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_solver_add_assumptions(
    _session: *mut NxuskitSolverSession,
    _assumptions_json: *const c_char,
) -> bool {
    unavailable_bool()
}
