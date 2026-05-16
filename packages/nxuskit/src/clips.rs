//! Safe Rust wrapper for the CLIPS Session API.
//!
//! Provides an RAII `ClipsSession` handle that wraps the u64-based session
//! C ABI. All unsafe FFI calls are encapsulated — consumers interact only
//! with safe types.
//!
//! # Usage
//!
//! ```rust,no_run
//! use nxuskit::clips::{ClipsSession, ClipsValue};
//! use std::collections::HashMap;
//!
//! let session = ClipsSession::create()?;
//! session.build("(deftemplate sensor (slot name) (slot value (type INTEGER)))")?;
//! session.reset()?;
//!
//! let mut slots = HashMap::new();
//! slots.insert("name".into(), ClipsValue::String("temp-1".into()));
//! slots.insert("value".into(), ClipsValue::Integer(72));
//! session.fact_assert_structured("sensor", &slots)?;
//!
//! let fired = session.run(None)?;
//! # Ok::<(), nxuskit::NxuskitError>(())
//! ```

use std::collections::HashMap;
use std::ffi::{CStr, CString};

use serde::{Deserialize, Serialize};

use crate::error::NxuskitError;
use crate::ffi::ffi_call;

// ── ClipsValue ──────────────────────────────────────────────────────────

/// A typed CLIPS slot value.
///
/// CLIPS distinguishes between strings and symbols — both carry text but
/// have different semantics in the inference engine. This enum preserves
/// that distinction for round-trip fidelity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum ClipsValue {
    /// 64-bit signed integer.
    #[serde(rename = "integer")]
    Integer(i64),
    /// 64-bit IEEE 754 floating-point number.
    #[serde(rename = "float")]
    Float(f64),
    /// CLIPS string value (text in double quotes).
    #[serde(rename = "string")]
    String(std::string::String),
    /// CLIPS symbol value (unquoted identifier).
    #[serde(rename = "symbol")]
    Symbol(std::string::String),
    /// Ordered collection of typed values.
    #[serde(rename = "multifield")]
    Multifield(Vec<ClipsValue>),
    /// No value / unset slot.
    #[serde(rename = "void")]
    Void,
    /// Fact address (internal index reference).
    #[serde(rename = "fact_address")]
    FactAddress(i64),
    /// Instance address (name reference).
    #[serde(rename = "instance_address")]
    InstanceAddress(std::string::String),
    /// External address (opaque pointer as integer).
    #[serde(rename = "external_address")]
    ExternalAddress(u64),
}

impl ClipsValue {
    /// Extract as integer, or error if wrong type.
    pub fn as_integer(&self) -> Result<i64, NxuskitError> {
        match self {
            ClipsValue::Integer(v) => Ok(*v),
            other => Err(NxuskitError::InvalidResponse {
                message: format!("expected Integer, got {:?}", variant_name(other)),
            }),
        }
    }

    /// Extract as float, or error if wrong type.
    pub fn as_float(&self) -> Result<f64, NxuskitError> {
        match self {
            ClipsValue::Float(v) => Ok(*v),
            other => Err(NxuskitError::InvalidResponse {
                message: format!("expected Float, got {:?}", variant_name(other)),
            }),
        }
    }

    /// Extract as string, or error if wrong type.
    pub fn as_string(&self) -> Result<&str, NxuskitError> {
        match self {
            ClipsValue::String(v) => Ok(v),
            other => Err(NxuskitError::InvalidResponse {
                message: format!("expected String, got {:?}", variant_name(other)),
            }),
        }
    }

    /// Extract as symbol, or error if wrong type.
    pub fn as_symbol(&self) -> Result<&str, NxuskitError> {
        match self {
            ClipsValue::Symbol(v) => Ok(v),
            other => Err(NxuskitError::InvalidResponse {
                message: format!("expected Symbol, got {:?}", variant_name(other)),
            }),
        }
    }

    /// Extract as multifield, or error if wrong type.
    pub fn as_multifield(&self) -> Result<&[ClipsValue], NxuskitError> {
        match self {
            ClipsValue::Multifield(v) => Ok(v),
            other => Err(NxuskitError::InvalidResponse {
                message: format!("expected Multifield, got {:?}", variant_name(other)),
            }),
        }
    }

    /// Check if this is a void value.
    pub fn is_void(&self) -> bool {
        matches!(self, ClipsValue::Void)
    }
}

fn variant_name(v: &ClipsValue) -> &'static str {
    match v {
        ClipsValue::Integer(_) => "Integer",
        ClipsValue::Float(_) => "Float",
        ClipsValue::String(_) => "String",
        ClipsValue::Symbol(_) => "Symbol",
        ClipsValue::Multifield(_) => "Multifield",
        ClipsValue::Void => "Void",
        ClipsValue::FactAddress(_) => "FactAddress",
        ClipsValue::InstanceAddress(_) => "InstanceAddress",
        ClipsValue::ExternalAddress(_) => "ExternalAddress",
    }
}

/// Parse a JSON string (as returned by the C ABI) into a `ClipsValue`.
pub fn json_to_clips_value(json: &str) -> Result<ClipsValue, NxuskitError> {
    serde_json::from_str::<ClipsValue>(json).map_err(|e| NxuskitError::InvalidResponse {
        message: format!("failed to parse ClipsValue JSON: {e}"),
    })
}

// ── SessionInfo ─────────────────────────────────────────────────────────

/// Aggregate information about a CLIPS session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub fact_count: u64,
    pub rule_count: u64,
    pub template_count: u64,
    pub module_names: Vec<std::string::String>,
    pub global_count: u64,
    pub class_count: u64,
    pub agenda_size: u64,
    pub current_module: std::string::String,
}

// ── TemplateSlotInfo ────────────────────────────────────────────────────

/// Metadata about a single slot in a CLIPS deftemplate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateSlotInfo {
    pub name: std::string::String,
    pub slot_type: std::string::String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_constraint: Option<std::string::String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cardinality_min: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cardinality_max: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range_min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range_max: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<ClipsValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_values: Option<Vec<ClipsValue>>,
}

// ── Helpers ─────────────────────────────────────────────────────────────

/// Convert a Rust `&str` to a `CString`, returning `NxuskitError` on failure.
fn to_cstring(s: &str) -> Result<CString, NxuskitError> {
    CString::new(s).map_err(|_| NxuskitError::InvalidRequest {
        message: format!("string contains interior NUL byte: {s:?}"),
    })
}

/// Read a C string returned from the SDK. Returns the owned Rust string and
/// frees the C allocation.
///
/// Returns `None` if the pointer is null.
unsafe fn take_cstring(ptr: *mut std::ffi::c_char) -> Option<std::string::String> {
    if ptr.is_null() {
        return None;
    }
    let s = unsafe { CStr::from_ptr(ptr).to_string_lossy().into_owned() };
    ffi_call!(nxuskit_free_string, ptr);
    Some(s)
}

/// Check an i32 return code. 0 = success, negative = error.
fn check_rc(rc: i32, op: &str) -> Result<(), NxuskitError> {
    if rc == 0 {
        Ok(())
    } else {
        Err(NxuskitError::ClipsError {
            message: format!("{op} failed (rc={rc})"),
        })
    }
}

/// Parse a JSON array of strings returned from the C ABI.
fn parse_string_array(json: &str) -> Result<Vec<std::string::String>, NxuskitError> {
    serde_json::from_str(json).map_err(|e| NxuskitError::InvalidResponse {
        message: format!("failed to parse JSON string array: {e}"),
    })
}

/// Parse a JSON array of i64 values.
fn parse_i64_array(json: &str) -> Result<Vec<i64>, NxuskitError> {
    serde_json::from_str(json).map_err(|e| NxuskitError::InvalidResponse {
        message: format!("failed to parse JSON i64 array: {e}"),
    })
}

// ── ClipsSession ────────────────────────────────────────────────────────

/// A CLIPS inference session backed by the nxusKit Session API.
///
/// Each session is an isolated CLIPS environment identified by a u64 handle.
/// The session is automatically destroyed when this struct is dropped.
pub struct ClipsSession {
    handle: u64,
}

impl ClipsSession {
    // ── Lifecycle ────────────────────────────────────────────────────

    /// Create a new CLIPS session.
    pub fn create() -> Result<Self, NxuskitError> {
        let h = ffi_call!(nxuskit_clips_session_create);
        if h == 0 {
            return Err(NxuskitError::ClipsError {
                message: "failed to create CLIPS session".into(),
            });
        }
        Ok(Self { handle: h })
    }

    /// Reset the session (clears facts, resets globals, re-asserts initial-fact).
    pub fn reset(&self) -> Result<(), NxuskitError> {
        check_rc(ffi_call!(nxuskit_clips_session_reset, self.handle), "reset")
    }

    /// Clear the session (removes all constructs and facts).
    pub fn clear(&self) -> Result<(), NxuskitError> {
        check_rc(ffi_call!(nxuskit_clips_session_clear, self.handle), "clear")
    }

    /// Get aggregate session information.
    pub fn info(&self) -> Result<SessionInfo, NxuskitError> {
        let ptr = ffi_call!(nxuskit_clips_session_info, self.handle);
        let json = unsafe { take_cstring(ptr) }.ok_or_else(|| NxuskitError::ClipsError {
            message: "session info returned null".into(),
        })?;
        serde_json::from_str(&json).map_err(|e| NxuskitError::InvalidResponse {
            message: format!("failed to parse SessionInfo: {e}"),
        })
    }

    // ── Construct loading ────────────────────────────────────────────

    /// Load constructs from a `.clp` file.
    pub fn load_file(&self, path: &str) -> Result<(), NxuskitError> {
        let c_path = to_cstring(path)?;
        check_rc(
            ffi_call!(
                nxuskit_clips_session_load_file,
                self.handle,
                c_path.as_ptr()
            ),
            "load_file",
        )
    }

    /// Load constructs from a string.
    pub fn load_string(&self, constructs: &str) -> Result<(), NxuskitError> {
        let c = to_cstring(constructs)?;
        check_rc(
            ffi_call!(nxuskit_clips_session_load_string, self.handle, c.as_ptr()),
            "load_string",
        )
    }

    /// Load a binary image from a file.
    pub fn load_binary(&self, path: &str) -> Result<(), NxuskitError> {
        let c = to_cstring(path)?;
        check_rc(
            ffi_call!(nxuskit_clips_session_load_binary, self.handle, c.as_ptr()),
            "load_binary",
        )
    }

    /// Save a binary image to a file.
    pub fn save_binary(&self, path: &str) -> Result<(), NxuskitError> {
        let c = to_cstring(path)?;
        check_rc(
            ffi_call!(nxuskit_clips_session_save_binary, self.handle, c.as_ptr()),
            "save_binary",
        )
    }

    /// Build a single construct from a string (e.g. a deftemplate or defrule).
    pub fn build(&self, construct: &str) -> Result<(), NxuskitError> {
        let c = to_cstring(construct)?;
        check_rc(
            ffi_call!(nxuskit_clips_session_build, self.handle, c.as_ptr()),
            "build",
        )
    }

    /// Execute a batch file.
    pub fn batch(&self, path: &str) -> Result<(), NxuskitError> {
        let c = to_cstring(path)?;
        check_rc(
            ffi_call!(nxuskit_clips_session_batch, self.handle, c.as_ptr()),
            "batch",
        )
    }

    // ── Fact operations ─────────────────────────────────────────────

    /// Assert a fact using CLIPS string syntax, e.g. `"(sensor (name \"a\"))"`.
    /// Returns the fact index on success.
    pub fn fact_assert_string(&self, fact: &str) -> Result<i64, NxuskitError> {
        let c = to_cstring(fact)?;
        let idx = ffi_call!(nxuskit_clips_fact_assert_string, self.handle, c.as_ptr());
        if idx < 0 {
            return Err(NxuskitError::ClipsError {
                message: format!("fact_assert_string failed for: {fact}"),
            });
        }
        Ok(idx)
    }

    /// Assert a fact using structured slot data. Returns the fact index.
    pub fn fact_assert_structured(
        &self,
        template: &str,
        slots: &HashMap<std::string::String, ClipsValue>,
    ) -> Result<i64, NxuskitError> {
        let c_tmpl = to_cstring(template)?;
        let slots_json =
            serde_json::to_string(slots).map_err(|e| NxuskitError::InvalidRequest {
                message: format!("failed to serialize slots: {e}"),
            })?;
        let c_slots = to_cstring(&slots_json)?;
        let idx = ffi_call!(
            nxuskit_clips_fact_assert_structured,
            self.handle,
            c_tmpl.as_ptr(),
            c_slots.as_ptr()
        );
        if idx < 0 {
            return Err(NxuskitError::ClipsError {
                message: format!("fact_assert_structured failed for template: {template}"),
            });
        }
        Ok(idx)
    }

    /// Retract (remove) a fact by its index.
    pub fn fact_retract(&self, index: i64) -> Result<(), NxuskitError> {
        check_rc(
            ffi_call!(nxuskit_clips_fact_retract, self.handle, index),
            "fact_retract",
        )
    }

    /// Retract all facts of a given template. Returns the count retracted.
    pub fn fact_retract_by_template(&self, template: &str) -> Result<i32, NxuskitError> {
        let c = to_cstring(template)?;
        let count = ffi_call!(
            nxuskit_clips_fact_retract_by_template,
            self.handle,
            c.as_ptr()
        );
        if count < 0 {
            return Err(NxuskitError::ClipsError {
                message: format!("fact_retract_by_template failed for: {template}"),
            });
        }
        Ok(count)
    }

    /// Check whether a fact with the given index exists.
    pub fn fact_exists(&self, index: i64) -> bool {
        ffi_call!(nxuskit_clips_fact_exists, self.handle, index)
    }

    /// Get the value of a single slot in a fact.
    pub fn fact_get_slot(&self, index: i64, slot: &str) -> Result<ClipsValue, NxuskitError> {
        let c_slot = to_cstring(slot)?;
        let ptr = ffi_call!(
            nxuskit_clips_fact_get_slot,
            self.handle,
            index,
            c_slot.as_ptr()
        );
        let json = unsafe { take_cstring(ptr) }.ok_or_else(|| NxuskitError::ClipsError {
            message: format!("fact_get_slot returned null for index={index}, slot={slot}"),
        })?;
        json_to_clips_value(&json)
    }

    /// Get all slot values for a fact as a map.
    pub fn fact_slot_values(
        &self,
        index: i64,
    ) -> Result<HashMap<std::string::String, ClipsValue>, NxuskitError> {
        let ptr = ffi_call!(nxuskit_clips_fact_slot_values, self.handle, index);
        let json = unsafe { take_cstring(ptr) }.ok_or_else(|| NxuskitError::ClipsError {
            message: format!("fact_slot_values returned null for index={index}"),
        })?;
        serde_json::from_str(&json).map_err(|e| NxuskitError::InvalidResponse {
            message: format!("failed to parse slot values: {e}"),
        })
    }

    /// Get the pretty-print form of a fact.
    pub fn fact_pp_form(&self, index: i64) -> Result<std::string::String, NxuskitError> {
        let ptr = ffi_call!(nxuskit_clips_fact_pp_form, self.handle, index);
        unsafe { take_cstring(ptr) }.ok_or_else(|| NxuskitError::ClipsError {
            message: format!("fact_pp_form returned null for index={index}"),
        })
    }

    /// List all fact indices in the session.
    pub fn facts(&self) -> Result<Vec<i64>, NxuskitError> {
        let ptr = ffi_call!(nxuskit_clips_facts_list, self.handle);
        let json = unsafe { take_cstring(ptr) }.ok_or_else(|| NxuskitError::ClipsError {
            message: "facts_list returned null".into(),
        })?;
        parse_i64_array(&json)
    }

    /// List fact indices for a specific template.
    pub fn facts_by_template(&self, template: &str) -> Result<Vec<i64>, NxuskitError> {
        let c = to_cstring(template)?;
        let ptr = ffi_call!(nxuskit_clips_facts_by_template, self.handle, c.as_ptr());
        let json = unsafe { take_cstring(ptr) }.ok_or_else(|| NxuskitError::ClipsError {
            message: format!("facts_by_template returned null for: {template}"),
        })?;
        parse_i64_array(&json)
    }

    // ── Template operations ─────────────────────────────────────────

    /// Check whether a deftemplate exists.
    pub fn template_exists(&self, name: &str) -> bool {
        let Ok(c) = to_cstring(name) else {
            return false;
        };
        ffi_call!(nxuskit_clips_template_exists, self.handle, c.as_ptr())
    }

    /// List all deftemplate names.
    pub fn template_list(&self) -> Result<Vec<std::string::String>, NxuskitError> {
        let ptr = ffi_call!(nxuskit_clips_template_list, self.handle);
        let json = unsafe { take_cstring(ptr) }.ok_or_else(|| NxuskitError::ClipsError {
            message: "template_list returned null".into(),
        })?;
        parse_string_array(&json)
    }

    /// List slot names for a deftemplate.
    pub fn template_slot_names(
        &self,
        name: &str,
    ) -> Result<Vec<std::string::String>, NxuskitError> {
        let c = to_cstring(name)?;
        let ptr = ffi_call!(nxuskit_clips_template_slot_names, self.handle, c.as_ptr());
        let json = unsafe { take_cstring(ptr) }.ok_or_else(|| NxuskitError::ClipsError {
            message: format!("template_slot_names returned null for: {name}"),
        })?;
        parse_string_array(&json)
    }

    /// Get detailed slot metadata for a deftemplate.
    pub fn template_slot_info(&self, name: &str) -> Result<Vec<TemplateSlotInfo>, NxuskitError> {
        let c = to_cstring(name)?;
        let ptr = ffi_call!(nxuskit_clips_template_slot_info, self.handle, c.as_ptr());
        let json = unsafe { take_cstring(ptr) }.ok_or_else(|| NxuskitError::ClipsError {
            message: format!("template_slot_info returned null for: {name}"),
        })?;
        serde_json::from_str(&json).map_err(|e| NxuskitError::InvalidResponse {
            message: format!("failed to parse TemplateSlotInfo: {e}"),
        })
    }

    /// List fact indices for a specific template (alias for `facts_by_template`).
    pub fn template_facts(&self, name: &str) -> Result<Vec<i64>, NxuskitError> {
        let c = to_cstring(name)?;
        let ptr = ffi_call!(nxuskit_clips_template_facts, self.handle, c.as_ptr());
        let json = unsafe { take_cstring(ptr) }.ok_or_else(|| NxuskitError::ClipsError {
            message: format!("template_facts returned null for: {name}"),
        })?;
        parse_i64_array(&json)
    }

    /// Get the pretty-print form of a deftemplate.
    pub fn template_pp_form(&self, name: &str) -> Result<std::string::String, NxuskitError> {
        let c = to_cstring(name)?;
        let ptr = ffi_call!(nxuskit_clips_template_pp_form, self.handle, c.as_ptr());
        unsafe { take_cstring(ptr) }.ok_or_else(|| NxuskitError::ClipsError {
            message: format!("template_pp_form returned null for: {name}"),
        })
    }

    // ── Rule operations ─────────────────────────────────────────────

    /// Check whether a defrule exists.
    pub fn rule_exists(&self, name: &str) -> bool {
        let Ok(c) = to_cstring(name) else {
            return false;
        };
        ffi_call!(nxuskit_clips_rule_exists, self.handle, c.as_ptr())
    }

    /// List all defrule names.
    pub fn rule_list(&self) -> Result<Vec<std::string::String>, NxuskitError> {
        let ptr = ffi_call!(nxuskit_clips_rule_list, self.handle);
        let json = unsafe { take_cstring(ptr) }.ok_or_else(|| NxuskitError::ClipsError {
            message: "rule_list returned null".into(),
        })?;
        parse_string_array(&json)
    }

    /// Get the number of times a rule has fired.
    pub fn rule_times_fired(&self, name: &str) -> Result<i64, NxuskitError> {
        let c = to_cstring(name)?;
        Ok(ffi_call!(
            nxuskit_clips_rule_times_fired,
            self.handle,
            c.as_ptr()
        ))
    }

    /// Set a breakpoint on a rule.
    pub fn rule_breakpoint_set(&self, name: &str) -> Result<(), NxuskitError> {
        let c = to_cstring(name)?;
        check_rc(
            ffi_call!(nxuskit_clips_rule_breakpoint_set, self.handle, c.as_ptr()),
            "rule_breakpoint_set",
        )
    }

    /// Remove a breakpoint from a rule.
    pub fn rule_breakpoint_remove(&self, name: &str) -> Result<(), NxuskitError> {
        let c = to_cstring(name)?;
        check_rc(
            ffi_call!(
                nxuskit_clips_rule_breakpoint_remove,
                self.handle,
                c.as_ptr()
            ),
            "rule_breakpoint_remove",
        )
    }

    /// Check whether a rule has a breakpoint set.
    pub fn rule_has_breakpoint(&self, name: &str) -> bool {
        let Ok(c) = to_cstring(name) else {
            return false;
        };
        ffi_call!(nxuskit_clips_rule_has_breakpoint, self.handle, c.as_ptr())
    }

    /// Refresh a rule (re-evaluate its activations).
    pub fn rule_refresh(&self, name: &str) -> Result<(), NxuskitError> {
        let c = to_cstring(name)?;
        check_rc(
            ffi_call!(nxuskit_clips_rule_refresh, self.handle, c.as_ptr()),
            "rule_refresh",
        )
    }

    /// Get the pretty-print form of a rule.
    pub fn rule_pp_form(&self, name: &str) -> Result<std::string::String, NxuskitError> {
        let c = to_cstring(name)?;
        let ptr = ffi_call!(nxuskit_clips_rule_pp_form, self.handle, c.as_ptr());
        unsafe { take_cstring(ptr) }.ok_or_else(|| NxuskitError::ClipsError {
            message: format!("rule_pp_form returned null for: {name}"),
        })
    }

    /// Delete a rule.
    pub fn rule_delete(&self, name: &str) -> Result<(), NxuskitError> {
        let c = to_cstring(name)?;
        check_rc(
            ffi_call!(nxuskit_clips_rule_delete, self.handle, c.as_ptr()),
            "rule_delete",
        )
    }

    // ── Execution & agenda ──────────────────────────────────────────

    /// Run the inference engine. Pass `None` for unlimited, or `Some(n)` to
    /// fire at most `n` rules. Returns the number of rules fired.
    pub fn run(&self, limit: Option<i64>) -> Result<i64, NxuskitError> {
        let lim = limit.unwrap_or(-1);
        Ok(ffi_call!(nxuskit_clips_session_run, self.handle, lim))
    }

    /// Halt the inference engine.
    pub fn halt(&self) -> Result<(), NxuskitError> {
        check_rc(ffi_call!(nxuskit_clips_session_halt, self.handle), "halt")
    }

    /// Get the number of activations on the agenda.
    pub fn agenda_size(&self) -> i64 {
        ffi_call!(nxuskit_clips_agenda_size, self.handle)
    }

    /// Clear all activations from the agenda.
    pub fn agenda_clear(&self) -> Result<(), NxuskitError> {
        check_rc(
            ffi_call!(nxuskit_clips_agenda_clear, self.handle),
            "agenda_clear",
        )
    }

    /// Reorder the agenda using the current conflict-resolution strategy.
    pub fn agenda_reorder(&self) -> Result<(), NxuskitError> {
        check_rc(
            ffi_call!(nxuskit_clips_agenda_reorder, self.handle),
            "agenda_reorder",
        )
    }

    /// Get the current conflict-resolution strategy name.
    pub fn strategy_get(&self) -> Result<std::string::String, NxuskitError> {
        let ptr = ffi_call!(nxuskit_clips_strategy_get, self.handle);
        unsafe { take_cstring(ptr) }.ok_or_else(|| NxuskitError::ClipsError {
            message: "strategy_get returned null".into(),
        })
    }

    /// Set the conflict-resolution strategy (e.g. "depth", "breadth", "random").
    pub fn strategy_set(&self, strategy: &str) -> Result<(), NxuskitError> {
        let c = to_cstring(strategy)?;
        check_rc(
            ffi_call!(nxuskit_clips_strategy_set, self.handle, c.as_ptr()),
            "strategy_set",
        )
    }

    /// Get the current salience evaluation mode.
    pub fn salience_mode_get(&self) -> Result<std::string::String, NxuskitError> {
        let ptr = ffi_call!(nxuskit_clips_salience_mode_get, self.handle);
        unsafe { take_cstring(ptr) }.ok_or_else(|| NxuskitError::ClipsError {
            message: "salience_mode_get returned null".into(),
        })
    }

    /// Set the salience evaluation mode.
    pub fn salience_mode_set(&self, mode: &str) -> Result<(), NxuskitError> {
        let c = to_cstring(mode)?;
        check_rc(
            ffi_call!(nxuskit_clips_salience_mode_set, self.handle, c.as_ptr()),
            "salience_mode_set",
        )
    }

    // ── Expression evaluation ───────────────────────────────────────

    /// Evaluate a CLIPS expression and return the result.
    pub fn eval(&self, expression: &str) -> Result<ClipsValue, NxuskitError> {
        let c = to_cstring(expression)?;
        let ptr = ffi_call!(nxuskit_clips_eval, self.handle, c.as_ptr());
        let json = unsafe { take_cstring(ptr) }.ok_or_else(|| NxuskitError::ClipsError {
            message: format!("eval returned null for: {expression}"),
        })?;
        json_to_clips_value(&json)
    }

    /// Call a CLIPS function by name with optional arguments JSON.
    pub fn function_call(
        &self,
        name: &str,
        args: &[ClipsValue],
    ) -> Result<ClipsValue, NxuskitError> {
        let c_name = to_cstring(name)?;
        let args_json = serde_json::to_string(args).map_err(|e| NxuskitError::InvalidRequest {
            message: format!("failed to serialize function args: {e}"),
        })?;
        let c_args = to_cstring(&args_json)?;
        let ptr = ffi_call!(
            nxuskit_clips_function_call,
            self.handle,
            c_name.as_ptr(),
            c_args.as_ptr()
        );
        let json = unsafe { take_cstring(ptr) }.ok_or_else(|| NxuskitError::ClipsError {
            message: format!("function_call returned null for: {name}"),
        })?;
        json_to_clips_value(&json)
    }

    // ── Settings ────────────────────────────────────────────────────

    /// Get whether fact duplication is allowed.
    pub fn fact_duplication_get(&self) -> bool {
        ffi_call!(nxuskit_clips_fact_duplication_get, self.handle)
    }

    /// Set whether fact duplication is allowed.
    pub fn fact_duplication_set(&self, allow: bool) -> Result<(), NxuskitError> {
        check_rc(
            ffi_call!(nxuskit_clips_fact_duplication_set, self.handle, allow),
            "fact_duplication_set",
        )
    }

    /// Get whether globals are reset on `(reset)`.
    pub fn reset_globals_get(&self) -> bool {
        ffi_call!(nxuskit_clips_reset_globals_get, self.handle)
    }

    /// Set whether globals are reset on `(reset)`.
    pub fn reset_globals_set(&self, reset: bool) -> Result<(), NxuskitError> {
        check_rc(
            ffi_call!(nxuskit_clips_reset_globals_set, self.handle, reset),
            "reset_globals_set",
        )
    }

    // ── JSON loading ───────────────────────────────────────────────

    /// Load constructs from a JSON definition.
    ///
    /// The JSON object supports optional keys: `modules`, `templates`, `rules`, `facts`.
    /// Processing order: modules → templates → rules → facts.
    pub fn load_json(&self, json: &str) -> Result<(), NxuskitError> {
        let c = to_cstring(json)?;
        check_rc(
            ffi_call!(nxuskit_clips_session_load_json, self.handle, c.as_ptr()),
            "load_json",
        )
    }

    // ── Cache (static methods) ──────────────────────────────────────

    /// Preload a named session with rules JSON. Stores in LRU cache with
    /// SHA-256 content-hash deduplication.
    pub fn preload(name: &str, rules_json: &str) -> Result<(), NxuskitError> {
        let c_name = to_cstring(name)?;
        let c_json = to_cstring(rules_json)?;
        check_rc(
            ffi_call!(
                nxuskit_clips_session_preload,
                c_name.as_ptr(),
                c_json.as_ptr()
            ),
            "preload",
        )
    }

    /// Retrieve an independent clone of a cached session (pre-loaded with rules).
    pub fn get_cached(name: &str) -> Result<Self, NxuskitError> {
        let c_name = to_cstring(name)?;
        let h = ffi_call!(nxuskit_clips_session_get_cached, c_name.as_ptr());
        if h == 0 {
            return Err(NxuskitError::ClipsError {
                message: format!("failed to get cached session '{name}'"),
            });
        }
        Ok(Self { handle: h })
    }

    /// Remove a cached session by name.
    pub fn cache_remove(name: &str) -> Result<(), NxuskitError> {
        let c_name = to_cstring(name)?;
        check_rc(
            ffi_call!(nxuskit_clips_session_cache_remove, c_name.as_ptr()),
            "cache_remove",
        )
    }

    // ── Module & focus stack ────────────────────────────────────────

    /// Check whether a defmodule exists.
    pub fn module_exists(&self, name: &str) -> bool {
        let Ok(c) = to_cstring(name) else {
            return false;
        };
        ffi_call!(nxuskit_clips_module_exists, self.handle, c.as_ptr())
    }

    /// List all defmodule names.
    pub fn module_list(&self) -> Result<Vec<std::string::String>, NxuskitError> {
        let ptr = ffi_call!(nxuskit_clips_module_list, self.handle);
        let json = unsafe { take_cstring(ptr) }.ok_or_else(|| NxuskitError::ClipsError {
            message: "module_list returned null".into(),
        })?;
        parse_string_array(&json)
    }

    /// Get the current module name.
    pub fn module_current_get(&self) -> Result<std::string::String, NxuskitError> {
        let ptr = ffi_call!(nxuskit_clips_module_current_get, self.handle);
        unsafe { take_cstring(ptr) }.ok_or_else(|| NxuskitError::ClipsError {
            message: "module_current_get returned null".into(),
        })
    }

    /// Set the current module.
    pub fn module_current_set(&self, name: &str) -> Result<(), NxuskitError> {
        let c = to_cstring(name)?;
        check_rc(
            ffi_call!(nxuskit_clips_module_current_set, self.handle, c.as_ptr()),
            "module_current_set",
        )
    }

    /// Push a module onto the focus stack.
    pub fn focus_push(&self, module_name: &str) -> Result<(), NxuskitError> {
        let c = to_cstring(module_name)?;
        check_rc(
            ffi_call!(nxuskit_clips_focus_push, self.handle, c.as_ptr()),
            "focus_push",
        )
    }

    /// Get the module at the top of the focus stack.
    pub fn focus_get(&self) -> Option<std::string::String> {
        let ptr = ffi_call!(nxuskit_clips_focus_get, self.handle);
        unsafe { take_cstring(ptr) }
    }

    /// Pop the top module from the focus stack.
    pub fn focus_pop(&self) -> Result<(), NxuskitError> {
        check_rc(ffi_call!(nxuskit_clips_focus_pop, self.handle), "focus_pop")
    }

    /// Clear the entire focus stack.
    pub fn focus_clear(&self) -> Result<(), NxuskitError> {
        check_rc(
            ffi_call!(nxuskit_clips_focus_clear, self.handle),
            "focus_clear",
        )
    }

    // ── Global variables ──────────────────────────────────────────────

    /// Check whether a defglobal with the given name exists.
    pub fn global_exists(&self, name: &str) -> bool {
        let Ok(c) = to_cstring(name) else {
            return false;
        };
        ffi_call!(nxuskit_clips_global_exists, self.handle, c.as_ptr())
    }

    /// List all defglobal names as a JSON array string.
    pub fn global_list(&self) -> Result<Vec<std::string::String>, NxuskitError> {
        let ptr = ffi_call!(nxuskit_clips_global_list, self.handle);
        let json = unsafe { take_cstring(ptr) }.ok_or_else(|| NxuskitError::ClipsError {
            message: "global_list returned NULL".into(),
        })?;
        serde_json::from_str(&json).map_err(|e| NxuskitError::ClipsError {
            message: format!("global_list JSON parse error: {e}"),
        })
    }

    /// Get the current value of a defglobal as a `ClipsValue`.
    pub fn global_get_value(&self, name: &str) -> Result<ClipsValue, NxuskitError> {
        let c_name = to_cstring(name)?;
        let ptr = ffi_call!(nxuskit_clips_global_get_value, self.handle, c_name.as_ptr());
        let json = unsafe { take_cstring(ptr) }.ok_or_else(|| NxuskitError::ClipsError {
            message: format!("global_get_value({name}) returned NULL"),
        })?;
        serde_json::from_str(&json).map_err(|e| NxuskitError::ClipsError {
            message: format!("global_get_value JSON parse error: {e}"),
        })
    }

    /// Set the value of a defglobal. `value_json` is a JSON object with
    /// `"type"` and `"value"` keys (e.g. `{"type":"integer","value":42}`).
    pub fn global_set_value(&self, name: &str, value_json: &str) -> Result<(), NxuskitError> {
        let c_name = to_cstring(name)?;
        let c_val = to_cstring(value_json)?;
        check_rc(
            ffi_call!(
                nxuskit_clips_global_set_value,
                self.handle,
                c_name.as_ptr(),
                c_val.as_ptr()
            ),
            "global_set_value",
        )
    }

    // ── Watch & diagnostics ─────────────────────────────────────────

    /// Enable a CLIPS watch item (e.g. "facts", "rules", "activations", "all").
    pub fn watch(&self, item: &str) -> Result<(), NxuskitError> {
        let c_item = to_cstring(item)?;
        check_rc(
            ffi_call!(nxuskit_clips_watch, self.handle, c_item.as_ptr()),
            "watch",
        )
    }

    /// Disable a CLIPS watch item.
    pub fn unwatch(&self, item: &str) -> Result<(), NxuskitError> {
        let c_item = to_cstring(item)?;
        check_rc(
            ffi_call!(nxuskit_clips_unwatch, self.handle, c_item.as_ptr()),
            "unwatch",
        )
    }

    /// Start dribbling CLIPS output to a file.
    pub fn dribble_on(&self, file_path: &str) -> Result<(), NxuskitError> {
        let c_path = to_cstring(file_path)?;
        check_rc(
            ffi_call!(nxuskit_clips_dribble_on, self.handle, c_path.as_ptr()),
            "dribble_on",
        )
    }

    /// Stop dribbling CLIPS output.
    pub fn dribble_off(&self) -> Result<(), NxuskitError> {
        check_rc(
            ffi_call!(nxuskit_clips_dribble_off, self.handle),
            "dribble_off",
        )
    }

    /// Explicitly destroy the session, releasing all resources immediately.
    ///
    /// After calling this, the session handle is invalidated and subsequent
    /// method calls will return errors. This is optional — sessions are also
    /// destroyed automatically when `ClipsSession` is dropped.
    pub fn destroy(&mut self) {
        if self.handle != 0 {
            ffi_call!(nxuskit_clips_session_destroy, self.handle);
            self.handle = 0;
        }
    }

    /// Return the raw session handle. For advanced use only.
    pub fn handle(&self) -> u64 {
        self.handle
    }
}

impl Drop for ClipsSession {
    fn drop(&mut self) {
        if self.handle != 0 {
            ffi_call!(nxuskit_clips_session_destroy, self.handle);
        }
    }
}
