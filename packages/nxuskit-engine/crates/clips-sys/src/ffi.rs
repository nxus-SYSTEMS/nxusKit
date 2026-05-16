//! Raw FFI bindings to the CLIPS C library
//!
//! This module provides unsafe, low-level bindings to CLIPS functions.
//! For safe Rust wrappers, use the higher-level modules.
//!
//! CLIPS documentation: <https://www.clipsrules.net/documentation/v640/apg.pdf>

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]

use std::os::raw::{
    c_char, c_double, c_int, c_long, c_longlong, c_uint, c_ulonglong, c_ushort, c_void,
};

// ============================================================================
// Opaque Types
// ============================================================================

/// CLIPS environment - the main context for all CLIPS operations
#[repr(C)]
pub struct Environment {
    _private: [u8; 0],
}

/// A fact in the CLIPS fact-list
#[repr(C)]
pub struct Fact {
    _private: [u8; 0],
}

/// A CLIPS instance (COOL object)
#[repr(C)]
pub struct Instance {
    _private: [u8; 0],
}

/// A deftemplate construct
#[repr(C)]
pub struct Deftemplate {
    _private: [u8; 0],
}

/// A defrule construct
#[repr(C)]
pub struct Defrule {
    _private: [u8; 0],
}

/// A defmodule construct
#[repr(C)]
pub struct Defmodule {
    _private: [u8; 0],
}

/// A deffunction construct
#[repr(C)]
pub struct Deffunction {
    _private: [u8; 0],
}

/// A defglobal construct
#[repr(C)]
pub struct Defglobal {
    _private: [u8; 0],
}

/// A defclass construct (COOL)
#[repr(C)]
pub struct Defclass {
    _private: [u8; 0],
}

/// A defgeneric construct
#[repr(C)]
pub struct Defgeneric {
    _private: [u8; 0],
}

/// A defmethod construct
#[repr(C)]
pub struct Defmethod {
    _private: [u8; 0],
}

/// An activation on the agenda
#[repr(C)]
pub struct Activation {
    _private: [u8; 0],
}

/// Builder for creating facts programmatically
#[repr(C)]
pub struct FactBuilder {
    _private: [u8; 0],
}

/// Builder for creating multifield values
#[repr(C)]
pub struct MultifieldBuilder {
    _private: [u8; 0],
}

/// Builder for creating strings
#[repr(C)]
pub struct StringBuilder {
    _private: [u8; 0],
}

/// Context for user-defined functions
#[repr(C)]
pub struct UDFContext {
    _private: [u8; 0],
}

/// Value for user-defined functions
#[repr(C)]
pub struct UDFValue {
    _private: [u8; 0],
}

/// A multifield value
#[repr(C)]
pub struct Multifield {
    _private: [u8; 0],
}

/// A lexeme (symbol or string)
#[repr(C)]
pub struct CLIPSLexeme {
    _private: [u8; 0],
}

/// An integer value
#[repr(C)]
pub struct CLIPSInteger {
    _private: [u8; 0],
}

/// A float value
#[repr(C)]
pub struct CLIPSFloat {
    _private: [u8; 0],
}

/// External address value
#[repr(C)]
pub struct CLIPSExternalAddress {
    _private: [u8; 0],
}

// ============================================================================
// Type Constants
// ============================================================================

/// CLIPS value type bit: floating-point number
pub const FLOAT_BIT: c_uint = 1 << 0;
/// CLIPS value type bit: integer number
pub const INTEGER_BIT: c_uint = 1 << 1;
/// CLIPS value type bit: symbol (unquoted identifier)
pub const SYMBOL_BIT: c_uint = 1 << 2;
/// CLIPS value type bit: string (quoted text)
pub const STRING_BIT: c_uint = 1 << 3;
/// CLIPS value type bit: multifield (list of values)
pub const MULTIFIELD_BIT: c_uint = 1 << 4;
/// CLIPS value type bit: external address
pub const EXTERNAL_ADDRESS_BIT: c_uint = 1 << 5;
/// CLIPS value type bit: fact address
pub const FACT_ADDRESS_BIT: c_uint = 1 << 6;
/// CLIPS value type bit: instance address
pub const INSTANCE_ADDRESS_BIT: c_uint = 1 << 7;
/// CLIPS value type bit: instance name
pub const INSTANCE_NAME_BIT: c_uint = 1 << 8;
/// CLIPS value type bit: void
pub const VOID_BIT: c_uint = 1 << 9;
/// CLIPS value type bit: boolean
pub const BOOLEAN_BIT: c_uint = 1 << 10;

/// Combined type mask: any numeric type (float or integer)
pub const NUMBER_BITS: c_uint = FLOAT_BIT | INTEGER_BIT;
/// Combined type mask: any lexeme (symbol or string)
pub const LEXEME_BITS: c_uint = SYMBOL_BIT | STRING_BIT;
/// Combined type mask: any address type
pub const ADDRESS_BITS: c_uint = EXTERNAL_ADDRESS_BIT | FACT_ADDRESS_BIT | INSTANCE_ADDRESS_BIT;
/// Combined type mask: any instance reference
pub const INSTANCE_BITS: c_uint = INSTANCE_ADDRESS_BIT | INSTANCE_NAME_BIT;
/// Combined type mask: any single-field value
pub const SINGLEFIELD_BITS: c_uint = NUMBER_BITS | LEXEME_BITS | ADDRESS_BITS | INSTANCE_NAME_BIT;
/// Combined type mask: any value type
pub const ANY_TYPE_BITS: c_uint = VOID_BIT | SINGLEFIELD_BITS | MULTIFIELD_BIT;

// ============================================================================
// Run Completion Reasons
// ============================================================================

/// Run completed: agenda exhausted (no more rules to fire)
pub const RUN_COMPLETION_AGENDA_EXHAUSTED: c_int = 0;
/// Run completed: rule breakpoint was hit
pub const RUN_COMPLETION_RULE_BREAKPOINT: c_int = 1;
/// Run completed: halt was called
pub const RUN_COMPLETION_HALT_EXECUTION: c_int = 2;
/// Run completed: periodic callback requested stop
pub const RUN_COMPLETION_PERIODIC_CALLBACK: c_int = 3;
/// Run completed: step limit was reached
pub const RUN_COMPLETION_STEP_LIMIT: c_int = 4;
/// Run completed: focus stack exhausted
pub const RUN_COMPLETION_FOCUS_STACK_EXHAUSTED: c_int = 5;

// ============================================================================
// Watch Items
// ============================================================================

/// Watch item: facts
pub const WATCH_FACTS: c_int = 0;
/// Watch item: rules
pub const WATCH_RULES: c_int = 1;
/// Watch item: activations
pub const WATCH_ACTIVATIONS: c_int = 2;
/// Watch item: compilations
pub const WATCH_COMPILATIONS: c_int = 3;
/// Watch item: statistics
pub const WATCH_STATISTICS: c_int = 4;
/// Watch item: globals
pub const WATCH_GLOBALS: c_int = 5;
/// Watch item: deffunctions
pub const WATCH_DEFFUNCTIONS: c_int = 6;
/// Watch item: instances
pub const WATCH_INSTANCES: c_int = 7;
/// Watch item: slots
pub const WATCH_SLOTS: c_int = 8;
/// Watch item: messages
pub const WATCH_MESSAGES: c_int = 9;
/// Watch item: message handlers
pub const WATCH_MESSAGE_HANDLERS: c_int = 10;
/// Watch item: generic functions
pub const WATCH_GENERIC_FUNCTIONS: c_int = 11;
/// Watch item: methods
pub const WATCH_METHODS: c_int = 12;
/// Watch item: focus
pub const WATCH_FOCUS: c_int = 13;
/// Watch item: all
pub const WATCH_ALL: c_int = 14;

// ============================================================================
// Salience Evaluation Modes
// ============================================================================

/// Salience mode: evaluate when rule is defined
pub const WHEN_DEFINED: c_int = 0;
/// Salience mode: evaluate when rule is activated
pub const WHEN_ACTIVATED: c_int = 1;
/// Salience mode: evaluate every cycle
pub const EVERY_CYCLE: c_int = 2;

// ============================================================================
// Conflict Resolution Strategies
// ============================================================================

/// Conflict resolution strategy: depth (LIFO)
pub const DEPTH_STRATEGY: c_int = 0;
/// Conflict resolution strategy: breadth (FIFO)
pub const BREADTH_STRATEGY: c_int = 1;
/// Conflict resolution strategy: LEX (lexicographic)
pub const LEX_STRATEGY: c_int = 2;
/// Conflict resolution strategy: MEA (means-ends analysis)
pub const MEA_STRATEGY: c_int = 3;
/// Conflict resolution strategy: complexity (most complex first)
pub const COMPLEXITY_STRATEGY: c_int = 4;
/// Conflict resolution strategy: simplicity (least complex first)
pub const SIMPLICITY_STRATEGY: c_int = 5;
/// Conflict resolution strategy: random
pub const RANDOM_STRATEGY: c_int = 6;

// ============================================================================
// Fact Builder Error Codes
// ============================================================================

/// Fact builder error: no error
pub const FBE_NO_ERROR: c_int = 0;
/// Fact builder error: null pointer
pub const FBE_NULL_POINTER_ERROR: c_int = 1;
/// Fact builder error: deftemplate not found
pub const FBE_DEFTEMPLATE_NOT_FOUND_ERROR: c_int = 2;
/// Fact builder error: implied deftemplate
pub const FBE_IMPLIED_DEFTEMPLATE_ERROR: c_int = 3;
/// Fact builder error: could not assert
pub const FBE_COULD_NOT_ASSERT_ERROR: c_int = 4;
/// Fact builder error: rule network error
pub const FBE_RULE_NETWORK_ERROR: c_int = 5;

// ============================================================================
// CLIPSValue - The unified value type
// ============================================================================

/// CLIPS unified value structure
#[repr(C)]
pub struct CLIPSValue {
    /// Pointer to the header containing type information
    pub header: *mut c_void,
}

impl CLIPSValue {
    /// Create a new void value
    pub fn new() -> Self {
        CLIPSValue {
            header: std::ptr::null_mut(),
        }
    }
}

impl Default for CLIPSValue {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Type Definitions for Callbacks
// ============================================================================

/// Router query function type
pub type RouterQueryFunction =
    Option<unsafe extern "C" fn(*mut Environment, *const c_char, *mut c_void) -> bool>;

/// Router write function type
pub type RouterWriteFunction =
    Option<unsafe extern "C" fn(*mut Environment, *const c_char, *const c_char, *mut c_void)>;

/// Router read function type
pub type RouterReadFunction =
    Option<unsafe extern "C" fn(*mut Environment, *const c_char, *mut c_void) -> c_int>;

/// Router unread function type
pub type RouterUnreadFunction =
    Option<unsafe extern "C" fn(*mut Environment, *const c_char, c_int, *mut c_void) -> c_int>;

/// Router exit function type
pub type RouterExitFunction = Option<unsafe extern "C" fn(*mut Environment, c_int, *mut c_void)>;

/// User-defined function type
pub type UserDefinedFunction =
    Option<unsafe extern "C" fn(*mut Environment, *mut UDFContext, *mut UDFValue)>;

/// Periodic function type
pub type PeriodicFunction = Option<unsafe extern "C" fn(*mut Environment, *mut c_void)>;

// ============================================================================
// External Function Declarations
// ============================================================================

#[link(name = "clips", kind = "static")]
unsafe extern "C" {
    // ========================================================================
    // Environment Management
    // ========================================================================

    /// Create a new CLIPS environment
    pub fn CreateEnvironment() -> *mut Environment;

    /// Destroy a CLIPS environment and free all resources
    pub fn DestroyEnvironment(env: *mut Environment) -> bool;

    /// Clear all constructs from the environment
    pub fn Clear(env: *mut Environment) -> bool;

    /// Reset the environment (retract facts, reset globals, clear agenda)
    /// Note: This function returns void in CLIPS, not bool
    pub fn Reset(env: *mut Environment);

    /// Get the current environment data pointer
    pub fn GetEnvironmentData(env: *mut Environment, position: c_uint) -> *mut c_void;

    /// Set environment data pointer
    pub fn SetEnvironmentData(env: *mut Environment, position: c_uint, data: *mut c_void) -> bool;

    /// Allocate environment data slot
    pub fn AllocateEnvironmentData(
        env: *mut Environment,
        position: c_uint,
        size: usize,
        cleanup: Option<unsafe extern "C" fn(*mut Environment)>,
    ) -> bool;

    // ========================================================================
    // Loading and Parsing
    // ========================================================================

    /// Load constructs from a file
    pub fn Load(env: *mut Environment, filename: *const c_char) -> c_int;

    /// Load constructs from a string
    ///
    /// Parameters:
    /// - env: The CLIPS environment
    /// - input: The string containing CLIPS constructs
    /// - max_position: Maximum characters to read (use SIZE_MAX for entire string)
    ///
    /// Returns: true on success, false on failure
    pub fn LoadFromString(env: *mut Environment, input: *const c_char, max_position: usize)
    -> bool;

    /// Load binary constructs from a file
    pub fn Bload(env: *mut Environment, filename: *const c_char) -> bool;

    /// Save binary constructs to a file
    pub fn Bsave(env: *mut Environment, filename: *const c_char) -> bool;

    /// Execute a batch file
    pub fn BatchStar(env: *mut Environment, filename: *const c_char) -> bool;

    /// Build a construct from a string
    ///
    /// Returns a BuildError enum:
    /// - BE_NO_ERROR (0) = success
    /// - BE_COULD_NOT_BUILD_ERROR (1) = could not build
    /// - BE_CONSTRUCT_NOT_FOUND_ERROR (2) = construct not found
    /// - BE_PARSING_ERROR (3) = parsing error
    pub fn Build(env: *mut Environment, construct_string: *const c_char) -> c_int;

    // ========================================================================
    // Evaluation
    // ========================================================================

    /// Evaluate a CLIPS expression (returns EvalError: 0 = EE_NO_ERROR)
    pub fn Eval(env: *mut Environment, expression: *const c_char, result: *mut CLIPSValue)
    -> c_int;

    /// Custom C wrapper: call a CLIPS function by name with optional args string.
    /// Returns true on success (uses FunctionCallBuilder internally).
    pub fn FunctionCall(
        env: *mut Environment,
        name: *const c_char,
        args: *const c_char,
        result: *mut CLIPSValue,
    ) -> bool;

    // ========================================================================
    // Fact Operations
    // ========================================================================

    /// Assert a fact from a string representation
    pub fn AssertString(env: *mut Environment, fact_string: *const c_char) -> *mut Fact;

    /// Retract a fact
    ///
    /// Returns a RetractError enum:
    /// - RE_NO_ERROR (0) = success
    /// - RE_NULL_POINTER_ERROR (1) = null pointer
    /// - RE_COULD_NOT_RETRACT_ERROR (2) = could not retract
    /// - RE_RULE_NETWORK_ERROR (3) = rule network error
    pub fn Retract(fact: *mut Fact) -> c_int;

    /// Get the first fact in the fact-list
    pub fn GetNextFact(env: *mut Environment, fact: *mut Fact) -> *mut Fact;

    /// Get the next fact in a template's fact-list
    pub fn GetNextFactInTemplate(template: *mut Deftemplate, fact: *mut Fact) -> *mut Fact;

    /// Get a fact's index
    pub fn FactIndex(fact: *mut Fact) -> c_long;

    /// Check if a fact exists
    pub fn FactExistp(fact: *mut Fact) -> bool;

    /// Get a slot value from a fact
    ///
    /// Returns a GetSlotError enum:
    /// - GSE_NO_ERROR (0) = success
    /// - GSE_NULL_POINTER_ERROR (1) = null pointer
    /// - GSE_INVALID_TARGET_ERROR (2) = invalid target
    /// - GSE_SLOT_NOT_FOUND_ERROR (3) = slot not found
    pub fn GetFactSlot(fact: *mut Fact, slot_name: *const c_char, value: *mut CLIPSValue) -> c_int;

    /// Get the pretty-print form of a fact
    pub fn FactPPForm(fact: *mut Fact, buffer: *mut c_char, buffer_size: usize);

    /// Get the deftemplate of a fact
    pub fn FactDeftemplate(fact: *mut Fact) -> *mut Deftemplate;

    /// Get the environment of a fact
    pub fn FactEnv(fact: *mut Fact) -> *mut Environment;

    /// Retain a fact (prevent garbage collection)
    pub fn RetainFact(fact: *mut Fact);

    /// Release a retained fact
    pub fn ReleaseFact(fact: *mut Fact);

    // ========================================================================
    // Fact Builder
    // ========================================================================

    /// Create a fact builder for a deftemplate
    pub fn CreateFactBuilder(
        env: *mut Environment,
        template_name: *const c_char,
    ) -> *mut FactBuilder;

    /// Put an integer value in a slot.
    /// Returns PutSlotError enum (0 = PSE_NO_ERROR = success).
    pub fn FBPutSlotInteger(
        builder: *mut FactBuilder,
        slot_name: *const c_char,
        value: c_long,
    ) -> c_int;

    /// Put a float value in a slot.
    /// Returns PutSlotError enum (0 = PSE_NO_ERROR = success).
    pub fn FBPutSlotFloat(
        builder: *mut FactBuilder,
        slot_name: *const c_char,
        value: c_double,
    ) -> c_int;

    /// Put a string value in a slot.
    /// Returns PutSlotError enum (0 = PSE_NO_ERROR = success).
    pub fn FBPutSlotString(
        builder: *mut FactBuilder,
        slot_name: *const c_char,
        value: *const c_char,
    ) -> c_int;

    /// Put a symbol value in a slot.
    /// Returns PutSlotError enum (0 = PSE_NO_ERROR = success).
    pub fn FBPutSlotSymbol(
        builder: *mut FactBuilder,
        slot_name: *const c_char,
        value: *const c_char,
    ) -> c_int;

    /// Put a multifield value in a slot.
    /// Returns PutSlotError enum (0 = PSE_NO_ERROR = success).
    pub fn FBPutSlotMultifield(
        builder: *mut FactBuilder,
        slot_name: *const c_char,
        value: *mut Multifield,
    ) -> c_int;

    /// Put a fact address in a slot.
    /// Returns PutSlotError enum (0 = PSE_NO_ERROR = success).
    pub fn FBPutSlotFact(
        builder: *mut FactBuilder,
        slot_name: *const c_char,
        fact: *mut Fact,
    ) -> c_int;

    /// Put an instance address in a slot.
    /// Returns PutSlotError enum (0 = PSE_NO_ERROR = success).
    pub fn FBPutSlotInstance(
        builder: *mut FactBuilder,
        slot_name: *const c_char,
        instance: *mut Instance,
    ) -> c_int;

    /// Put a CLIPSValue in a slot.
    /// Returns PutSlotError enum (0 = PSE_NO_ERROR = success).
    pub fn FBPutSlotCLIPSValue(
        builder: *mut FactBuilder,
        slot_name: *const c_char,
        value: *mut CLIPSValue,
    ) -> c_int;

    /// Assert the fact built by the builder
    pub fn FBAssert(builder: *mut FactBuilder) -> *mut Fact;

    /// Dispose of a fact builder
    pub fn FBDispose(builder: *mut FactBuilder);

    /// Abort a fact builder
    pub fn FBAbort(builder: *mut FactBuilder);

    /// Set the deftemplate for a fact builder.
    /// Returns FactBuilderError enum (0 = FBE_NO_ERROR = success).
    pub fn FBSetDeftemplate(builder: *mut FactBuilder, template_name: *const c_char) -> c_int;

    /// Get the last error from a fact builder
    pub fn FBError(env: *mut Environment) -> c_int;

    // ========================================================================
    // Template Operations
    // ========================================================================

    /// Find a deftemplate by name
    pub fn FindDeftemplate(env: *mut Environment, name: *const c_char) -> *mut Deftemplate;

    /// Get the next deftemplate
    pub fn GetNextDeftemplate(
        env: *mut Environment,
        template: *mut Deftemplate,
    ) -> *mut Deftemplate;

    /// Get the name of a deftemplate
    pub fn DeftemplateName(template: *mut Deftemplate) -> *const c_char;

    /// Get the module name of a deftemplate
    pub fn DeftemplateModule(template: *mut Deftemplate) -> *const c_char;

    /// Get the pretty-print form of a deftemplate
    pub fn DeftemplatePPForm(template: *mut Deftemplate) -> *const c_char;

    /// Get the slot names of a deftemplate
    /// Note: This function returns void and populates the CLIPSValue with a multifield
    pub fn DeftemplateSlotNames(template: *mut Deftemplate, value: *mut CLIPSValue);

    /// Get the default value of a slot
    pub fn DeftemplateSlotDefaultValue(
        template: *mut Deftemplate,
        slot_name: *const c_char,
        value: *mut CLIPSValue,
    ) -> bool;

    /// Get the allowed values of a slot
    pub fn DeftemplateSlotAllowedValues(
        template: *mut Deftemplate,
        slot_name: *const c_char,
        value: *mut CLIPSValue,
    ) -> bool;

    /// Get the cardinality of a slot
    pub fn DeftemplateSlotCardinality(
        template: *mut Deftemplate,
        slot_name: *const c_char,
        value: *mut CLIPSValue,
    ) -> bool;

    /// Get the range of a slot
    pub fn DeftemplateSlotRange(
        template: *mut Deftemplate,
        slot_name: *const c_char,
        value: *mut CLIPSValue,
    ) -> bool;

    /// Get the types of a slot
    pub fn DeftemplateSlotTypes(
        template: *mut Deftemplate,
        slot_name: *const c_char,
        value: *mut CLIPSValue,
    ) -> bool;

    /// Check if a slot is a multislot
    pub fn DeftemplateSlotMultiP(template: *mut Deftemplate, slot_name: *const c_char) -> bool;

    /// Check if a slot is a single slot
    pub fn DeftemplateSlotSingleP(template: *mut Deftemplate, slot_name: *const c_char) -> bool;

    /// Check if a slot exists
    pub fn DeftemplateSlotExistP(template: *mut Deftemplate, slot_name: *const c_char) -> bool;

    /// Delete a deftemplate
    pub fn Undeftemplate(template: *mut Deftemplate, env: *mut Environment) -> bool;

    /// Check if a deftemplate is deletable
    pub fn DeftemplateIsDeletable(template: *mut Deftemplate) -> bool;

    // ========================================================================
    // Rule Operations
    // ========================================================================

    /// Find a defrule by name
    pub fn FindDefrule(env: *mut Environment, name: *const c_char) -> *mut Defrule;

    /// Get the next defrule
    pub fn GetNextDefrule(env: *mut Environment, rule: *mut Defrule) -> *mut Defrule;

    /// Get the name of a defrule
    pub fn DefruleName(rule: *mut Defrule) -> *const c_char;

    /// Get the module name of a defrule
    pub fn DefruleModule(rule: *mut Defrule) -> *const c_char;

    /// Get the pretty-print form of a defrule
    pub fn DefrulePPForm(rule: *mut Defrule) -> *const c_char;

    /// Get the number of times a rule has fired (wrapper function - returns 0)
    /// Note: CLIPS doesn't track per-rule firing counts directly
    #[link_name = "clips_get_defrule_firings"]
    pub fn GetDefruleFirings(rule: *mut Defrule) -> c_ulonglong;

    /// Set a breakpoint on a rule
    pub fn SetBreak(rule: *mut Defrule);

    /// Remove a breakpoint from a rule
    pub fn RemoveBreak(rule: *mut Defrule) -> bool;

    /// Check if a rule has a breakpoint
    pub fn DefruleHasBreakpoint(rule: *mut Defrule) -> bool;

    /// Delete a defrule
    pub fn Undefrule(rule: *mut Defrule, env: *mut Environment) -> bool;

    /// Check if a defrule is deletable
    pub fn DefruleIsDeletable(rule: *mut Defrule) -> bool;

    /// Refresh a rule's activations
    pub fn Refresh(env: *mut Environment, rule: *mut Defrule);

    /// Get watch activations state for a rule
    pub fn GetDefruleWatchActivations(rule: *mut Defrule) -> bool;

    /// Get watch firings state for a rule
    pub fn GetDefruleWatchFirings(rule: *mut Defrule) -> bool;

    /// Set watch activations state for a rule
    pub fn SetDefruleWatchActivations(rule: *mut Defrule, value: bool);

    /// Set watch firings state for a rule
    pub fn SetDefruleWatchFirings(rule: *mut Defrule, value: bool);

    // ========================================================================
    // Inference Engine
    // ========================================================================

    /// Run the inference engine
    pub fn Run(env: *mut Environment, limit: c_long) -> c_long;

    /// Halt the inference engine
    pub fn Halt(env: *mut Environment);

    /// Get the halt rules flag
    pub fn GetHaltRules(env: *mut Environment) -> bool;

    /// Set the halt rules flag
    pub fn SetHaltRules(env: *mut Environment, value: bool);

    /// Get the halt execution flag
    pub fn GetHaltExecution(env: *mut Environment) -> bool;

    /// Set the halt execution flag
    pub fn SetHaltExecution(env: *mut Environment, value: bool);

    /// Get the evaluation error flag
    pub fn GetEvaluationError(env: *mut Environment) -> bool;

    /// Set the evaluation error flag
    pub fn SetEvaluationError(env: *mut Environment, value: bool);

    /// Add a periodic function to be called during Run
    pub fn AddPeriodicFunction(
        env: *mut Environment,
        name: *const c_char,
        func: PeriodicFunction,
        priority: c_int,
        context: *mut c_void,
    ) -> bool;

    /// Remove a periodic function
    pub fn RemovePeriodicFunction(env: *mut Environment, name: *const c_char) -> bool;

    // ========================================================================
    // Agenda Operations
    // ========================================================================

    /// Get the first activation on the agenda
    pub fn GetNextActivation(env: *mut Environment, activation: *mut Activation)
    -> *mut Activation;

    /// Get the name of an activation
    pub fn ActivationRuleName(activation: *mut Activation) -> *const c_char;

    /// Get the salience of an activation
    pub fn ActivationGetSalience(activation: *mut Activation) -> c_int;

    /// Set the salience of an activation
    pub fn ActivationSetSalience(activation: *mut Activation, salience: c_int) -> c_int;

    /// Get the pretty-print form of an activation
    pub fn ActivationPPForm(activation: *mut Activation, buffer: *mut c_char, size: usize);

    /// Delete an activation
    pub fn DeleteActivation(activation: *mut Activation) -> bool;

    /// Clear the agenda
    pub fn ClearAgenda(env: *mut Environment, module: *mut Defmodule);

    /// Reorder the agenda
    pub fn ReorderAgenda(env: *mut Environment, module: *mut Defmodule);

    /// Refresh the agenda
    pub fn RefreshAgenda(env: *mut Environment, module: *mut Defmodule);

    /// Get the number of activations on the agenda
    pub fn GetAgendaSize(env: *mut Environment, module: *mut Defmodule) -> c_long;

    /// Get the salience evaluation mode
    pub fn GetSalienceEvaluation(env: *mut Environment) -> c_int;

    /// Set the salience evaluation mode
    pub fn SetSalienceEvaluation(env: *mut Environment, mode: c_int) -> c_int;

    /// Get the conflict resolution strategy
    pub fn GetStrategy(env: *mut Environment) -> c_int;

    /// Set the conflict resolution strategy
    pub fn SetStrategy(env: *mut Environment, strategy: c_int) -> c_int;

    /// Get fact duplication setting
    pub fn GetFactDuplication(env: *mut Environment) -> bool;

    /// Set fact duplication setting
    ///
    /// When set to true, CLIPS allows identical facts to be asserted multiple times.
    /// The default is false (duplicate facts are rejected).
    pub fn SetFactDuplication(env: *mut Environment, allow: bool) -> bool;

    // ========================================================================
    // Module Operations
    // ========================================================================

    /// Find a defmodule by name
    pub fn FindDefmodule(env: *mut Environment, name: *const c_char) -> *mut Defmodule;

    /// Get the next defmodule
    pub fn GetNextDefmodule(env: *mut Environment, module: *mut Defmodule) -> *mut Defmodule;

    /// Get the name of a defmodule
    pub fn DefmoduleName(module: *mut Defmodule) -> *const c_char;

    /// Get the pretty-print form of a defmodule
    pub fn DefmodulePPForm(module: *mut Defmodule) -> *const c_char;

    /// Get the current module
    pub fn GetCurrentModule(env: *mut Environment) -> *mut Defmodule;

    /// Set the current module
    pub fn SetCurrentModule(env: *mut Environment, module: *mut Defmodule) -> *mut Defmodule;

    /// Get the focus module
    pub fn GetFocus(env: *mut Environment) -> *mut Defmodule;

    /// Set the focus module
    pub fn Focus(module: *mut Defmodule);

    /// Pop the focus stack
    pub fn PopFocus(env: *mut Environment) -> *mut Defmodule;

    /// Clear the focus stack
    pub fn ClearFocusStack(env: *mut Environment);

    // ========================================================================
    // Global Operations
    // ========================================================================

    /// Find a defglobal by name
    pub fn FindDefglobal(env: *mut Environment, name: *const c_char) -> *mut Defglobal;

    /// Get the next defglobal
    pub fn GetNextDefglobal(env: *mut Environment, global: *mut Defglobal) -> *mut Defglobal;

    /// Get the name of a defglobal
    pub fn DefglobalName(global: *mut Defglobal) -> *const c_char;

    /// Get the module name of a defglobal
    pub fn DefglobalModule(global: *mut Defglobal) -> *const c_char;

    /// Get the pretty-print form of a defglobal
    pub fn DefglobalPPForm(global: *mut Defglobal) -> *const c_char;

    /// Get the value of a defglobal (void return in CLIPS 6.4 C API)
    pub fn DefglobalGetValue(global: *mut Defglobal, value: *mut CLIPSValue);

    /// Set the value of a defglobal (void return in CLIPS 6.4 C API)
    pub fn DefglobalSetValue(global: *mut Defglobal, value: *mut CLIPSValue);

    /// Delete a defglobal
    pub fn Undefglobal(global: *mut Defglobal, env: *mut Environment) -> bool;

    /// Get the reset globals flag
    pub fn GetResetGlobals(env: *mut Environment) -> bool;

    /// Set the reset globals flag
    pub fn SetResetGlobals(env: *mut Environment, value: bool) -> bool;

    // ========================================================================
    // Function Operations
    // ========================================================================

    /// Find a deffunction by name
    pub fn FindDeffunction(env: *mut Environment, name: *const c_char) -> *mut Deffunction;

    /// Get the next deffunction
    pub fn GetNextDeffunction(env: *mut Environment, func: *mut Deffunction) -> *mut Deffunction;

    /// Get the name of a deffunction
    pub fn DeffunctionName(func: *mut Deffunction) -> *const c_char;

    /// Get the module name of a deffunction
    pub fn DeffunctionModule(func: *mut Deffunction) -> *const c_char;

    /// Get the pretty-print form of a deffunction
    pub fn DeffunctionPPForm(func: *mut Deffunction) -> *const c_char;

    /// Delete a deffunction
    pub fn Undeffunction(func: *mut Deffunction, env: *mut Environment) -> bool;

    // ========================================================================
    // Generic Functions
    // ========================================================================

    /// Find a defgeneric by name
    pub fn FindDefgeneric(env: *mut Environment, name: *const c_char) -> *mut Defgeneric;

    /// Get the next defgeneric
    pub fn GetNextDefgeneric(env: *mut Environment, generic: *mut Defgeneric) -> *mut Defgeneric;

    /// Get the name of a defgeneric
    pub fn DefgenericName(generic: *mut Defgeneric) -> *const c_char;

    /// Get the module name of a defgeneric
    pub fn DefgenericModule(generic: *mut Defgeneric) -> *const c_char;

    /// Get the pretty-print form of a defgeneric
    pub fn DefgenericPPForm(generic: *mut Defgeneric) -> *const c_char;

    // ========================================================================
    // COOL (Class/Instance) Operations
    // ========================================================================

    /// Find a defclass by name
    pub fn FindDefclass(env: *mut Environment, name: *const c_char) -> *mut Defclass;

    /// Get the next defclass
    pub fn GetNextDefclass(env: *mut Environment, class: *mut Defclass) -> *mut Defclass;

    /// Get the name of a defclass
    pub fn DefclassName(class: *mut Defclass) -> *const c_char;

    /// Get the module name of a defclass
    pub fn DefclassModule(class: *mut Defclass) -> *const c_char;

    /// Get the pretty-print form of a defclass
    pub fn DefclassPPForm(class: *mut Defclass) -> *const c_char;

    /// Create a raw instance
    pub fn CreateRawInstance(class: *mut Defclass, name: *const c_char) -> *mut Instance;

    /// Make an instance from a string
    pub fn MakeInstance(env: *mut Environment, instance_string: *const c_char) -> *mut Instance;

    /// Delete an instance
    pub fn DeleteInstance(instance: *mut Instance) -> bool;

    /// Unmake an instance
    pub fn UnmakeInstance(instance: *mut Instance) -> bool;

    /// Get the next instance
    pub fn GetNextInstance(env: *mut Environment, instance: *mut Instance) -> *mut Instance;

    /// Get the next instance in a class
    pub fn GetNextInstanceInClass(class: *mut Defclass, instance: *mut Instance) -> *mut Instance;

    /// Find an instance by name
    pub fn FindInstance(
        env: *mut Environment,
        module: *mut Defmodule,
        name: *const c_char,
        search: bool,
    ) -> *mut Instance;

    /// Get the name of an instance
    pub fn InstanceName(instance: *mut Instance) -> *const c_char;

    /// Get the pretty-print form of an instance
    pub fn InstancePPForm(instance: *mut Instance, buffer: *mut c_char, size: usize);

    /// Get a slot value from an instance
    pub fn DirectGetSlot(
        instance: *mut Instance,
        slot_name: *const c_char,
        value: *mut CLIPSValue,
    ) -> bool;

    /// Put a slot value in an instance
    pub fn DirectPutSlot(
        instance: *mut Instance,
        slot_name: *const c_char,
        value: *mut CLIPSValue,
    ) -> bool;

    /// Send a message to an instance
    pub fn Send(
        env: *mut Environment,
        value: *mut CLIPSValue,
        message: *const c_char,
        args: *const c_char,
        result: *mut CLIPSValue,
    ) -> bool;

    /// Retain an instance
    pub fn RetainInstance(instance: *mut Instance);

    /// Release an instance
    pub fn ReleaseInstance(instance: *mut Instance);

    // ========================================================================
    // Watch/Debug Operations
    // ========================================================================

    /// Enable watching for an item
    pub fn Watch(env: *mut Environment, item: c_int) -> bool;

    /// Disable watching for an item
    pub fn Unwatch(env: *mut Environment, item: c_int) -> bool;

    /// Get the watch state for an item
    pub fn GetWatchState(env: *mut Environment, item: c_int) -> bool;

    /// Set the watch state for an item
    pub fn SetWatchState(env: *mut Environment, item: c_int, state: bool);

    /// Enable dribble output to a file
    pub fn DribbleOn(env: *mut Environment, filename: *const c_char) -> bool;

    /// Disable dribble output
    pub fn DribbleOff(env: *mut Environment) -> bool;

    /// Check if dribble is active
    pub fn DribbleActive(env: *mut Environment) -> bool;

    // ========================================================================
    // I/O Router Operations
    // ========================================================================

    /// Add an I/O router
    pub fn AddRouter(
        env: *mut Environment,
        name: *const c_char,
        priority: c_int,
        query: RouterQueryFunction,
        write: RouterWriteFunction,
        read: RouterReadFunction,
        unread: RouterUnreadFunction,
        exit: RouterExitFunction,
        context: *mut c_void,
    ) -> bool;

    /// Delete an I/O router
    pub fn DeleteRouter(env: *mut Environment, name: *const c_char) -> bool;

    /// Activate an I/O router
    pub fn ActivateRouter(env: *mut Environment, name: *const c_char) -> bool;

    /// Deactivate an I/O router
    pub fn DeactivateRouter(env: *mut Environment, name: *const c_char) -> bool;

    /// Write to a logical name
    pub fn WriteString(env: *mut Environment, logical_name: *const c_char, str: *const c_char);

    /// Write a line to a logical name
    pub fn Writeln(env: *mut Environment, logical_name: *const c_char, str: *const c_char);

    // ========================================================================
    // Multifield Operations
    // ========================================================================

    /// Create a multifield builder
    pub fn CreateMultifieldBuilder(
        env: *mut Environment,
        capacity: usize,
    ) -> *mut MultifieldBuilder;

    /// Append an integer to a multifield builder
    pub fn MBAppendInteger(builder: *mut MultifieldBuilder, value: c_long);

    /// Append a float to a multifield builder
    pub fn MBAppendFloat(builder: *mut MultifieldBuilder, value: c_double);

    /// Append a string to a multifield builder
    pub fn MBAppendString(builder: *mut MultifieldBuilder, value: *const c_char);

    /// Append a symbol to a multifield builder
    pub fn MBAppendSymbol(builder: *mut MultifieldBuilder, value: *const c_char);

    /// Append a fact to a multifield builder
    pub fn MBAppendFact(builder: *mut MultifieldBuilder, fact: *mut Fact);

    /// Append an instance to a multifield builder
    pub fn MBAppendInstance(builder: *mut MultifieldBuilder, instance: *mut Instance);

    /// Append a CLIPSValue to a multifield builder
    pub fn MBAppendCLIPSValue(builder: *mut MultifieldBuilder, value: *mut CLIPSValue);

    /// Create a multifield from the builder
    pub fn MBCreate(builder: *mut MultifieldBuilder) -> *mut Multifield;

    /// Reset a multifield builder
    pub fn MBReset(builder: *mut MultifieldBuilder);

    /// Dispose of a multifield builder
    pub fn MBDispose(builder: *mut MultifieldBuilder);

    /// Get the length of a multifield (wrapper function)
    #[link_name = "clips_multifield_length"]
    pub fn MultifieldLength(multifield: *const Multifield) -> usize;

    /// Get a value from a multifield (wrapper function)
    #[link_name = "clips_multifield_slot"]
    pub fn MultifieldSlot(multifield: *mut Multifield, index: usize, value: *mut CLIPSValue);

    // ========================================================================
    // String Builder
    // ========================================================================

    /// Create a string builder
    pub fn CreateStringBuilder(env: *mut Environment, capacity: usize) -> *mut StringBuilder;

    /// Append a string to the builder
    pub fn SBAppend(builder: *mut StringBuilder, str: *const c_char);

    /// Append an integer to the builder
    pub fn SBAppendInteger(builder: *mut StringBuilder, value: c_long);

    /// Append a float to the builder
    pub fn SBAppendFloat(builder: *mut StringBuilder, value: c_double);

    /// Get the contents of the string builder
    pub fn SBCopy(builder: *mut StringBuilder) -> *const c_char;

    /// Reset the string builder
    pub fn SBReset(builder: *mut StringBuilder);

    /// Dispose of a string builder
    pub fn SBDispose(builder: *mut StringBuilder);

    // ========================================================================
    // User Defined Functions
    // ========================================================================

    /// Add a user-defined function
    pub fn AddUDF(
        env: *mut Environment,
        name: *const c_char,
        return_types: *const c_char,
        min_args: c_int,
        max_args: c_int,
        arg_types: *const c_char,
        func: UserDefinedFunction,
        function_name: *const c_char,
        context: *mut c_void,
    ) -> bool;

    /// Get the first argument in a UDF
    pub fn UDFFirstArgument(context: *mut UDFContext, types: c_uint, value: *mut UDFValue) -> bool;

    /// Get the next argument in a UDF
    pub fn UDFNextArgument(context: *mut UDFContext, types: c_uint, value: *mut UDFValue) -> bool;

    /// Get the Nth argument in a UDF
    pub fn UDFNthArgument(
        context: *mut UDFContext,
        n: c_uint,
        types: c_uint,
        value: *mut UDFValue,
    ) -> bool;

    /// Get the argument count for a UDF
    pub fn UDFArgumentCount(context: *mut UDFContext) -> c_uint;

    /// Check if there are more arguments
    pub fn UDFHasNextArgument(context: *mut UDFContext) -> bool;

    /// Throw an error in a UDF
    pub fn UDFThrowError(context: *mut UDFContext);

    // ========================================================================
    // Value Access Functions (wrapper functions for CLIPS macros)
    // ========================================================================

    /// Get the type of a CLIPSValue (wrapper function)
    #[link_name = "clips_cv_type"]
    pub fn CVType(value: *mut CLIPSValue) -> c_int;

    /// Check if a value is of a specific type using type bits (wrapper function)
    #[link_name = "clips_cv_is_type"]
    pub fn CVIsType(value: *mut CLIPSValue, type_bits: c_uint) -> bool;

    /// Get integer value (wrapper function)
    #[link_name = "clips_cv_to_integer"]
    pub fn CVToInteger(value: *mut CLIPSValue) -> c_longlong;

    /// Get float value (wrapper function)
    #[link_name = "clips_cv_to_float"]
    pub fn CVToFloat(value: *mut CLIPSValue) -> c_double;

    /// Get lexeme (string/symbol) value (wrapper function)
    #[link_name = "clips_cv_to_string"]
    pub fn CVToString(value: *mut CLIPSValue) -> *const c_char;

    /// Get fact value (wrapper function)
    #[link_name = "clips_cv_to_fact"]
    pub fn CVToFact(value: *const CLIPSValue) -> *mut Fact;

    /// Get instance value (wrapper function)
    #[link_name = "clips_cv_to_instance"]
    pub fn CVToInstance(value: *const CLIPSValue) -> *mut Instance;

    /// Get multifield value (wrapper function)
    #[link_name = "clips_cv_to_multifield"]
    pub fn CVToMultifield(value: *const CLIPSValue) -> *mut Multifield;

    /// Get external address value (wrapper function)
    #[link_name = "clips_cv_to_external_address"]
    pub fn CVToExternalAddress(value: *mut CLIPSValue) -> *mut c_void;

    /// Set void value (wrapper function)
    #[link_name = "clips_cv_set_void"]
    pub fn CVSetVoid(value: *mut CLIPSValue);

    /// Set integer value (wrapper function)
    #[link_name = "clips_cv_set_integer"]
    pub fn CVSetInteger(env: *mut Environment, value: *mut CLIPSValue, integer: c_longlong);

    /// Set float value (wrapper function)
    #[link_name = "clips_cv_set_float"]
    pub fn CVSetFloat(env: *mut Environment, value: *mut CLIPSValue, float_val: c_double);

    /// Set symbol value (wrapper function)
    #[link_name = "clips_cv_set_symbol"]
    pub fn CVSetSymbol(env: *mut Environment, value: *mut CLIPSValue, symbol: *const c_char);

    /// Set string value (wrapper function)
    #[link_name = "clips_cv_set_string"]
    pub fn CVSetString(env: *mut Environment, value: *mut CLIPSValue, string: *const c_char);

    /// Set fact value (wrapper function)
    #[link_name = "clips_cv_set_fact"]
    pub fn CVSetFact(value: *mut CLIPSValue, fact: *mut Fact);

    /// Set instance value (wrapper function)
    #[link_name = "clips_cv_set_instance"]
    pub fn CVSetInstance(value: *mut CLIPSValue, instance: *mut Instance);

    /// Set multifield value (wrapper function)
    #[link_name = "clips_cv_set_multifield"]
    pub fn CVSetMultifield(value: *mut CLIPSValue, multifield: *mut Multifield);

    /// Set external address value (wrapper function)
    #[link_name = "clips_cv_set_external_address"]
    pub fn CVSetExternalAddress(
        env: *mut Environment,
        value: *mut CLIPSValue,
        address: *mut c_void,
        type_index: c_ushort,
    );

    // ========================================================================
    // Memory Management
    // ========================================================================

    /// Allocate memory from CLIPS memory pool
    pub fn genalloc(env: *mut Environment, size: usize) -> *mut c_void;

    /// Free memory to CLIPS memory pool
    pub fn genfree(env: *mut Environment, ptr: *mut c_void, size: usize);

    /// Get memory usage statistics
    pub fn MemUsed(env: *mut Environment) -> c_long;

    /// Get memory requests count
    pub fn MemRequests(env: *mut Environment) -> c_long;

    // ========================================================================
    // Utility Functions
    // ========================================================================

    /// Get the CLIPS version string
    pub fn Version() -> *const c_char;
}
