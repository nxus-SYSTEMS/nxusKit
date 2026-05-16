//! CLIPS value types and conversions
//!
//! This module provides safe Rust representations of CLIPS values.

use crate::error::{ClipsError, Result};
use crate::ffi;
use std::collections::HashMap;
use std::ffi::CStr;

/// A safe Rust representation of a CLIPS value
#[derive(Debug, Clone, PartialEq, Default)]
pub enum ClipsValue {
    /// Void/nil value
    #[default]
    Void,
    /// Integer value
    Integer(i64),
    /// Float value
    Float(f64),
    /// Symbol value (unquoted identifier)
    Symbol(String),
    /// String value (quoted string)
    String(String),
    /// Boolean value (TRUE/FALSE symbols)
    Boolean(bool),
    /// Multifield value (list)
    Multifield(Vec<ClipsValue>),
    /// Fact address
    FactAddress(i64),
    /// Instance address
    InstanceAddress(String),
    /// External address (opaque pointer)
    ExternalAddress(usize),
}

impl ClipsValue {
    /// Create a CLIPS value from an FFI CLIPSValue
    ///
    /// # Safety
    /// The caller must ensure the CLIPSValue pointer is valid and the environment
    /// pointer matches the one used to create the value.
    #[allow(clippy::only_used_in_recursion)]
    pub unsafe fn from_ffi(
        value: *mut ffi::CLIPSValue,
        env: *mut ffi::Environment,
    ) -> Result<Self> {
        if value.is_null() {
            return Ok(ClipsValue::Void);
        }

        let type_bits = unsafe { ffi::CVType(value) };

        // Check type and convert
        if unsafe { ffi::CVIsType(value, ffi::VOID_BIT) } {
            Ok(ClipsValue::Void)
        } else if unsafe { ffi::CVIsType(value, ffi::INTEGER_BIT) } {
            Ok(ClipsValue::Integer(unsafe { ffi::CVToInteger(value) }))
        } else if unsafe { ffi::CVIsType(value, ffi::FLOAT_BIT) } {
            Ok(ClipsValue::Float(unsafe { ffi::CVToFloat(value) }))
        } else if unsafe { ffi::CVIsType(value, ffi::STRING_BIT) } {
            let ptr = unsafe { ffi::CVToString(value) };
            if ptr.is_null() {
                return Ok(ClipsValue::String(String::new()));
            }
            let s = unsafe { CStr::from_ptr(ptr) }.to_str()?.to_string();
            Ok(ClipsValue::String(s))
        } else if unsafe { ffi::CVIsType(value, ffi::SYMBOL_BIT) } {
            let ptr = unsafe { ffi::CVToString(value) };
            if ptr.is_null() {
                return Ok(ClipsValue::Symbol(String::new()));
            }
            let s = unsafe { CStr::from_ptr(ptr) }.to_str()?.to_string();
            // Check for boolean symbols
            match s.as_str() {
                "TRUE" => Ok(ClipsValue::Boolean(true)),
                "FALSE" => Ok(ClipsValue::Boolean(false)),
                _ => Ok(ClipsValue::Symbol(s)),
            }
        } else if unsafe { ffi::CVIsType(value, ffi::MULTIFIELD_BIT) } {
            let mf = unsafe { ffi::CVToMultifield(value) };
            if mf.is_null() {
                return Ok(ClipsValue::Multifield(vec![]));
            }
            let len = unsafe { ffi::MultifieldLength(mf) };
            let mut items = Vec::with_capacity(len);
            let mut item_value = ffi::CLIPSValue::new();
            for i in 0..len {
                unsafe { ffi::MultifieldSlot(mf, i, &mut item_value) };
                items.push(unsafe { ClipsValue::from_ffi(&mut item_value, env) }?);
            }
            Ok(ClipsValue::Multifield(items))
        } else if unsafe { ffi::CVIsType(value, ffi::FACT_ADDRESS_BIT) } {
            let fact = unsafe { ffi::CVToFact(value) };
            if fact.is_null() {
                return Ok(ClipsValue::Void);
            }
            Ok(ClipsValue::FactAddress(
                unsafe { ffi::FactIndex(fact) } as i64
            ))
        } else if unsafe { ffi::CVIsType(value, ffi::INSTANCE_ADDRESS_BIT) } {
            let instance = unsafe { ffi::CVToInstance(value) };
            if instance.is_null() {
                return Ok(ClipsValue::Void);
            }
            let name_ptr = unsafe { ffi::InstanceName(instance) };
            if name_ptr.is_null() {
                return Ok(ClipsValue::InstanceAddress(String::new()));
            }
            let name = unsafe { CStr::from_ptr(name_ptr) }.to_str()?.to_string();
            Ok(ClipsValue::InstanceAddress(name))
        } else if unsafe { ffi::CVIsType(value, ffi::EXTERNAL_ADDRESS_BIT) } {
            let addr = unsafe { ffi::CVToExternalAddress(value) };
            Ok(ClipsValue::ExternalAddress(addr as usize))
        } else {
            Err(ClipsError::InvalidValueType {
                type_code: type_bits,
            })
        }
    }

    /// Check if the value is void/nil
    pub fn is_void(&self) -> bool {
        matches!(self, ClipsValue::Void)
    }

    /// Try to get as integer
    pub fn as_integer(&self) -> Option<i64> {
        match self {
            ClipsValue::Integer(i) => Some(*i),
            ClipsValue::Float(f) => Some(*f as i64),
            _ => None,
        }
    }

    /// Try to get as float
    pub fn as_float(&self) -> Option<f64> {
        match self {
            ClipsValue::Float(f) => Some(*f),
            ClipsValue::Integer(i) => Some(*i as f64),
            _ => None,
        }
    }

    /// Try to get as string
    pub fn as_string(&self) -> Option<&str> {
        match self {
            ClipsValue::String(s) | ClipsValue::Symbol(s) => Some(s),
            _ => None,
        }
    }

    /// Try to get as symbol
    pub fn as_symbol(&self) -> Option<&str> {
        match self {
            ClipsValue::Symbol(s) => Some(s),
            _ => None,
        }
    }

    /// Try to get as boolean
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            ClipsValue::Boolean(b) => Some(*b),
            ClipsValue::Symbol(s) => match s.as_str() {
                "TRUE" => Some(true),
                "FALSE" => Some(false),
                _ => None,
            },
            _ => None,
        }
    }

    /// Try to get as multifield
    pub fn as_multifield(&self) -> Option<&[ClipsValue]> {
        match self {
            ClipsValue::Multifield(items) => Some(items),
            _ => None,
        }
    }

    /// Try to get as fact address
    pub fn as_fact_address(&self) -> Option<i64> {
        match self {
            ClipsValue::FactAddress(idx) => Some(*idx),
            _ => None,
        }
    }

    /// Convert to CLIPS string representation
    pub fn to_clips_string(&self) -> String {
        match self {
            ClipsValue::Void => "nil".to_string(),
            ClipsValue::Integer(i) => i.to_string(),
            ClipsValue::Float(f) => format!("{:.6}", f),
            ClipsValue::Symbol(s) => s.clone(),
            ClipsValue::String(s) => {
                format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
            }
            ClipsValue::Boolean(b) => if *b { "TRUE" } else { "FALSE" }.to_string(),
            ClipsValue::Multifield(items) => {
                let strs: Vec<String> = items.iter().map(|v| v.to_clips_string()).collect();
                format!("({})", strs.join(" "))
            }
            ClipsValue::FactAddress(idx) => format!("<Fact-{}>", idx),
            ClipsValue::InstanceAddress(name) => format!("[{}]", name),
            ClipsValue::ExternalAddress(addr) => format!("<ExternalAddress-{:x}>", addr),
        }
    }

    /// Get the CLIPS type name
    pub fn type_name(&self) -> &'static str {
        match self {
            ClipsValue::Void => "VOID",
            ClipsValue::Integer(_) => "INTEGER",
            ClipsValue::Float(_) => "FLOAT",
            ClipsValue::Symbol(_) => "SYMBOL",
            ClipsValue::String(_) => "STRING",
            ClipsValue::Boolean(_) => "SYMBOL",
            ClipsValue::Multifield(_) => "MULTIFIELD",
            ClipsValue::FactAddress(_) => "FACT-ADDRESS",
            ClipsValue::InstanceAddress(_) => "INSTANCE-ADDRESS",
            ClipsValue::ExternalAddress(_) => "EXTERNAL-ADDRESS",
        }
    }
}

impl From<i64> for ClipsValue {
    fn from(value: i64) -> Self {
        ClipsValue::Integer(value)
    }
}

impl From<i32> for ClipsValue {
    fn from(value: i32) -> Self {
        ClipsValue::Integer(value as i64)
    }
}

impl From<f64> for ClipsValue {
    fn from(value: f64) -> Self {
        ClipsValue::Float(value)
    }
}

impl From<f32> for ClipsValue {
    fn from(value: f32) -> Self {
        ClipsValue::Float(value as f64)
    }
}

impl From<bool> for ClipsValue {
    fn from(value: bool) -> Self {
        ClipsValue::Boolean(value)
    }
}

impl From<String> for ClipsValue {
    fn from(value: String) -> Self {
        ClipsValue::String(value)
    }
}

impl From<&str> for ClipsValue {
    fn from(value: &str) -> Self {
        ClipsValue::String(value.to_string())
    }
}

impl<T: Into<ClipsValue>> From<Vec<T>> for ClipsValue {
    fn from(value: Vec<T>) -> Self {
        ClipsValue::Multifield(value.into_iter().map(|v| v.into()).collect())
    }
}

/// A slot value map for a fact
pub type SlotValues = HashMap<String, ClipsValue>;

/// Information about a deftemplate slot
#[derive(Debug, Clone)]
pub struct SlotInfo {
    /// Slot name
    pub name: String,
    /// Whether this is a multislot
    pub is_multislot: bool,
    /// Default value (if any)
    pub default_value: Option<ClipsValue>,
    /// Allowed types
    pub allowed_types: Vec<String>,
    /// Allowed values (if constrained)
    pub allowed_values: Option<Vec<ClipsValue>>,
    /// Value range (min, max) for numeric slots
    pub range: Option<(f64, f64)>,
    /// Cardinality (min, max) for multislots
    pub cardinality: Option<(usize, usize)>,
}

/// Information about a rule
#[derive(Debug, Clone)]
pub struct RuleInfo {
    /// Rule name
    pub name: String,
    /// Module name
    pub module: String,
    /// Number of times fired
    pub times_fired: u64,
    /// Whether rule has a breakpoint
    pub has_breakpoint: bool,
    /// Salience value
    pub salience: i32,
    /// Pretty-print form
    pub pp_form: Option<String>,
}

/// Information about an activation on the agenda
#[derive(Debug, Clone)]
pub struct ActivationInfo {
    /// Rule name
    pub rule_name: String,
    /// Salience value
    pub salience: i32,
    /// Pretty-print form
    pub pp_form: String,
}

/// Result of running the inference engine
#[derive(Debug, Clone)]
pub struct RunResult {
    /// Number of rules that fired
    pub rules_fired: u64,
    /// Completion reason
    pub completion_reason: RunCompletionReason,
}

/// Reason for run completion
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunCompletionReason {
    /// Agenda was exhausted (normal completion)
    AgendaExhausted,
    /// Hit a rule breakpoint
    RuleBreakpoint,
    /// Execution was halted
    HaltExecution,
    /// Periodic callback requested stop
    PeriodicCallback,
    /// Step limit reached
    StepLimit,
    /// Focus stack exhausted
    FocusStackExhausted,
    /// Unknown reason
    Unknown(i32),
}

impl From<i32> for RunCompletionReason {
    fn from(code: i32) -> Self {
        match code {
            ffi::RUN_COMPLETION_AGENDA_EXHAUSTED => RunCompletionReason::AgendaExhausted,
            ffi::RUN_COMPLETION_RULE_BREAKPOINT => RunCompletionReason::RuleBreakpoint,
            ffi::RUN_COMPLETION_HALT_EXECUTION => RunCompletionReason::HaltExecution,
            ffi::RUN_COMPLETION_PERIODIC_CALLBACK => RunCompletionReason::PeriodicCallback,
            ffi::RUN_COMPLETION_STEP_LIMIT => RunCompletionReason::StepLimit,
            ffi::RUN_COMPLETION_FOCUS_STACK_EXHAUSTED => RunCompletionReason::FocusStackExhausted,
            other => RunCompletionReason::Unknown(other),
        }
    }
}
