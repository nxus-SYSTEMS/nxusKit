//! Public CE ZEN ABI stubs.
//!
//! The Pro ZEN implementation is not shipped in public CE source or release
//! bundles. These symbols remain available for ABI stability and return the
//! standard unavailable error.

use std::ffi::c_char;

use crate::error;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_zen_evaluate(
    _model_json: *const c_char,
    _input_json: *const c_char,
) -> *mut c_char {
    error::set_last_error("feature_unavailable", "zen", None);
    std::ptr::null_mut()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_zen_free_result(_result: *mut c_char) {}
