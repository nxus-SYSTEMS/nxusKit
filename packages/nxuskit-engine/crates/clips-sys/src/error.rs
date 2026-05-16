//! Error types for clips-sys
//!
//! This module provides comprehensive error handling for CLIPS operations.

use std::ffi::NulError;
use thiserror::Error;

/// Result type for clips-sys operations
pub type Result<T> = std::result::Result<T, ClipsError>;

/// Comprehensive error type for CLIPS operations
#[derive(Debug, Error)]
pub enum ClipsError {
    /// Failed to create a CLIPS environment
    #[error("Failed to create CLIPS environment")]
    EnvironmentCreationFailed,

    /// Failed to load a file
    #[error("Failed to load file '{file}': {message}")]
    LoadFailed {
        /// The file that failed to load
        file: String,
        /// Error message
        message: String,
    },

    /// Failed to parse a construct
    #[error("Failed to parse construct: {message}")]
    ParseError {
        /// The construct that failed to parse
        construct: String,
        /// Error message
        message: String,
    },

    /// Failed to build a construct
    #[error("Failed to build construct: {construct}")]
    BuildFailed {
        /// The construct that failed to build
        construct: String,
    },

    /// Failed to assert a fact
    #[error("Failed to assert fact '{fact}': {message}")]
    AssertFailed {
        /// The fact that failed to assert
        fact: String,
        /// Error message
        message: String,
    },

    /// Failed to retract a fact
    #[error("Failed to retract fact with index {fact_index}")]
    RetractFailed {
        /// The fact index that failed to retract
        fact_index: i64,
    },

    /// Failed to evaluate an expression
    #[error("Failed to evaluate expression: {expression}")]
    EvaluationFailed {
        /// The expression that failed
        expression: String,
    },

    /// Deftemplate not found
    #[error("Deftemplate '{name}' not found")]
    TemplateNotFound {
        /// The template name that was not found
        name: String,
    },

    /// Defrule not found
    #[error("Defrule '{name}' not found")]
    RuleNotFound {
        /// The rule name that was not found
        name: String,
    },

    /// Defmodule not found
    #[error("Defmodule '{name}' not found")]
    ModuleNotFound {
        /// The module name that was not found
        name: String,
    },

    /// Defglobal not found
    #[error("Defglobal '{name}' not found")]
    GlobalNotFound {
        /// The global name that was not found
        name: String,
    },

    /// Deffunction not found
    #[error("Deffunction '{name}' not found")]
    FunctionNotFound {
        /// The function name that was not found
        name: String,
    },

    /// Defclass not found
    #[error("Defclass '{name}' not found")]
    ClassNotFound {
        /// The class name that was not found
        name: String,
    },

    /// Instance not found
    #[error("Instance '{name}' not found")]
    InstanceNotFound {
        /// The instance name that was not found
        name: String,
    },

    /// Slot not found
    #[error("Slot '{slot}' not found in template '{template}'")]
    SlotNotFound {
        /// The template name
        template: String,
        /// The slot name
        slot: String,
    },

    /// Invalid slot value type
    #[error("Invalid type for slot '{slot}': expected {expected}, got {got}")]
    InvalidSlotType {
        /// The slot name
        slot: String,
        /// Expected type
        expected: String,
        /// Actual type
        got: String,
    },

    /// Failed to create fact builder
    #[error("Failed to create fact builder for template '{template}'")]
    FactBuilderCreationFailed {
        /// The template name
        template: String,
    },

    /// Fact builder error
    #[error("Fact builder error: {message} (code: {code})")]
    FactBuilderError {
        /// Error code from FBError
        code: i32,
        /// Error message
        message: String,
    },

    /// Failed to reset environment
    #[error("Failed to reset CLIPS environment")]
    ResetFailed,

    /// Failed to clear environment
    #[error("Failed to clear CLIPS environment")]
    ClearFailed,

    /// Watch operation failed
    #[error("Failed to set watch state for item")]
    WatchFailed,

    /// Unwatch operation failed
    #[error("Failed to unset watch state for item")]
    UnwatchFailed,

    /// Router operation failed
    #[error("Router operation failed: {message}")]
    RouterFailed {
        /// Error message
        message: String,
    },

    /// Instance creation failed
    #[error("Failed to create instance: {message}")]
    InstanceCreationFailed {
        /// Error message
        message: String,
    },

    /// Message send failed
    #[error("Failed to send message '{message}' to instance")]
    MessageSendFailed {
        /// The message name
        message: String,
    },

    /// Null pointer received from CLIPS
    #[error("Null pointer received from CLIPS operation: {operation}")]
    NullPointer {
        /// The operation that returned null
        operation: String,
    },

    /// String contains null byte
    #[error("String contains null byte")]
    NulError(#[from] NulError),

    /// UTF-8 conversion error
    #[error("UTF-8 conversion error: {0}")]
    Utf8Error(#[from] std::str::Utf8Error),

    /// Invalid value type
    #[error("Invalid CLIPS value type: {type_code}")]
    InvalidValueType {
        /// The invalid type code
        type_code: i32,
    },

    /// Multifield index out of bounds
    #[error("Multifield index {index} out of bounds (length: {length})")]
    MultifieldIndexOutOfBounds {
        /// The requested index
        index: usize,
        /// The multifield length
        length: usize,
    },

    /// Environment was destroyed
    #[error("CLIPS environment has been destroyed")]
    EnvironmentDestroyed,

    /// Operation timeout
    #[error("Operation timed out after {duration_ms}ms")]
    Timeout {
        /// Timeout duration in milliseconds
        duration_ms: u64,
    },

    /// Execution was halted
    #[error("CLIPS execution was halted")]
    ExecutionHalted,

    /// Binary load/save error
    #[error("Binary {operation} failed for file '{file}'")]
    BinaryError {
        /// The operation (load/save)
        operation: String,
        /// The file path
        file: String,
    },
}

impl ClipsError {
    /// Check if this error is recoverable
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            ClipsError::TemplateNotFound { .. }
                | ClipsError::RuleNotFound { .. }
                | ClipsError::GlobalNotFound { .. }
                | ClipsError::InstanceNotFound { .. }
                | ClipsError::SlotNotFound { .. }
                | ClipsError::Timeout { .. }
        )
    }

    /// Create a fact builder error from an error code
    pub fn fact_builder_error(code: i32) -> Self {
        let message = match code {
            0 => "No error",
            1 => "Null pointer error",
            2 => "Deftemplate not found",
            3 => "Implied deftemplate error",
            4 => "Could not assert error",
            5 => "Rule network error",
            _ => "Unknown error",
        };
        ClipsError::FactBuilderError {
            code,
            message: message.to_string(),
        }
    }
}

/// Extension trait for converting Option results
pub trait OptionExt<T> {
    /// Convert None to a ClipsError
    fn null_check(self, operation: &str) -> Result<T>;
}

impl<T> OptionExt<T> for Option<T> {
    fn null_check(self, operation: &str) -> Result<T> {
        self.ok_or_else(|| ClipsError::NullPointer {
            operation: operation.to_string(),
        })
    }
}

/// Extension trait for null-checking raw pointers
pub trait PtrExt {
    /// Check if pointer is null, returning error if so
    fn null_check(self, operation: &str) -> Result<Self>
    where
        Self: Sized;
}

impl<T> PtrExt for *const T {
    fn null_check(self, operation: &str) -> Result<Self> {
        if self.is_null() {
            Err(ClipsError::NullPointer {
                operation: operation.to_string(),
            })
        } else {
            Ok(self)
        }
    }
}

impl<T> PtrExt for *mut T {
    fn null_check(self, operation: &str) -> Result<Self> {
        if self.is_null() {
            Err(ClipsError::NullPointer {
                operation: operation.to_string(),
            })
        } else {
            Ok(self)
        }
    }
}
