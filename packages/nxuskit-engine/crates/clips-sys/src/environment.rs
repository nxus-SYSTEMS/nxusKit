//! Safe wrapper for CLIPS Environment
//!
//! The `ClipsEnvironment` struct provides a safe, thread-safe interface
//! to CLIPS environments.

use crate::error::{ClipsError, PtrExt, Result};
use crate::ffi;
use crate::value::{ClipsValue, RunCompletionReason, RunResult, SlotValues};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::path::Path;
use std::ptr;
use std::sync::Arc;

/// Watch items that can be enabled/disabled for debugging
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchItem {
    /// Watch fact assertions and retractions
    Facts,
    /// Watch rule firings
    Rules,
    /// Watch rule activations
    Activations,
    /// Watch construct compilations
    Compilations,
    /// Watch statistics
    Statistics,
    /// Watch global variable changes
    Globals,
    /// Watch deffunction calls
    Deffunctions,
    /// Watch instance creation and deletion
    Instances,
    /// Watch slot value changes
    Slots,
    /// Watch messages
    Messages,
    /// Watch message handler calls
    MessageHandlers,
    /// Watch generic function calls
    GenericFunctions,
    /// Watch method calls
    Methods,
    /// Watch focus changes
    Focus,
    /// Watch all items
    All,
}

impl WatchItem {
    fn to_ffi(self) -> i32 {
        match self {
            WatchItem::Facts => ffi::WATCH_FACTS,
            WatchItem::Rules => ffi::WATCH_RULES,
            WatchItem::Activations => ffi::WATCH_ACTIVATIONS,
            WatchItem::Compilations => ffi::WATCH_COMPILATIONS,
            WatchItem::Statistics => ffi::WATCH_STATISTICS,
            WatchItem::Globals => ffi::WATCH_GLOBALS,
            WatchItem::Deffunctions => ffi::WATCH_DEFFUNCTIONS,
            WatchItem::Instances => ffi::WATCH_INSTANCES,
            WatchItem::Slots => ffi::WATCH_SLOTS,
            WatchItem::Messages => ffi::WATCH_MESSAGES,
            WatchItem::MessageHandlers => ffi::WATCH_MESSAGE_HANDLERS,
            WatchItem::GenericFunctions => ffi::WATCH_GENERIC_FUNCTIONS,
            WatchItem::Methods => ffi::WATCH_METHODS,
            WatchItem::Focus => ffi::WATCH_FOCUS,
            WatchItem::All => ffi::WATCH_ALL,
        }
    }
}

/// Conflict resolution strategy for the agenda
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strategy {
    /// Depth-first (default)
    Depth,
    /// Breadth-first
    Breadth,
    /// LEX (Lexicographic)
    Lex,
    /// MEA (Means-Ends Analysis)
    Mea,
    /// Complexity (most specific patterns first)
    Complexity,
    /// Simplicity (least specific patterns first)
    Simplicity,
    /// Random
    Random,
}

impl Strategy {
    fn to_ffi(self) -> i32 {
        match self {
            Strategy::Depth => ffi::DEPTH_STRATEGY,
            Strategy::Breadth => ffi::BREADTH_STRATEGY,
            Strategy::Lex => ffi::LEX_STRATEGY,
            Strategy::Mea => ffi::MEA_STRATEGY,
            Strategy::Complexity => ffi::COMPLEXITY_STRATEGY,
            Strategy::Simplicity => ffi::SIMPLICITY_STRATEGY,
            Strategy::Random => ffi::RANDOM_STRATEGY,
        }
    }

    fn from_ffi(code: i32) -> Self {
        match code {
            ffi::DEPTH_STRATEGY => Strategy::Depth,
            ffi::BREADTH_STRATEGY => Strategy::Breadth,
            ffi::LEX_STRATEGY => Strategy::Lex,
            ffi::MEA_STRATEGY => Strategy::Mea,
            ffi::COMPLEXITY_STRATEGY => Strategy::Complexity,
            ffi::SIMPLICITY_STRATEGY => Strategy::Simplicity,
            ffi::RANDOM_STRATEGY => Strategy::Random,
            _ => Strategy::Depth,
        }
    }
}

/// Inner environment data protected by mutex
struct EnvironmentInner {
    env: *mut ffi::Environment,
}

// Safety: Access to the environment is protected by a Mutex
unsafe impl Send for EnvironmentInner {}

/// A thread-safe CLIPS environment wrapper
///
/// # Example
///
/// ```no_run
/// use clips_sys::ClipsEnvironment;
///
/// let env = ClipsEnvironment::new()?;
/// env.load("rules.clp")?;
/// env.assert_string("(patient (name \"John\") (age 65))")?;
/// let result = env.run(None)?;
/// println!("Rules fired: {}", result.rules_fired);
/// # Ok::<(), clips_sys::ClipsError>(())
/// ```
#[derive(Clone)]
pub struct ClipsEnvironment {
    inner: Arc<Mutex<EnvironmentInner>>,
}

impl std::fmt::Debug for ClipsEnvironment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClipsEnvironment").finish_non_exhaustive()
    }
}

/// Global lock that serializes `CreateEnvironment()` FFI calls.
///
/// The CLIPS C library's `CreateEnvironment` → `InitializeEnvironment` chain
/// touches global memory-tracking and symbol-table state that is not
/// thread-safe. Concurrent calls can corrupt the heap, causing SIGABRT.
/// Each environment is independent *after* creation, so this lock is only
/// held for the duration of the `CreateEnvironment()` call itself.
static ENV_CREATION_LOCK: Mutex<()> = Mutex::new(());

impl ClipsEnvironment {
    /// Create a new CLIPS environment
    pub fn new() -> Result<Self> {
        // Serialize environment creation to avoid CLIPS C-level race condition
        let _guard = ENV_CREATION_LOCK.lock();
        let env = unsafe { ffi::CreateEnvironment() };
        if env.is_null() {
            return Err(ClipsError::EnvironmentCreationFailed);
        }
        Ok(Self {
            inner: Arc::new(Mutex::new(EnvironmentInner { env })),
        })
    }

    /// Get the raw environment pointer (for advanced use)
    ///
    /// # Safety
    /// The caller must ensure proper synchronization and that the
    /// environment is not destroyed while the pointer is in use.
    pub unsafe fn raw(&self) -> *mut ffi::Environment {
        let inner = self.inner.lock();
        inner.env
    }

    /// Load constructs from a file
    ///
    /// Returns Ok(()) on success. The CLIPS Load function returns a LoadError enum:
    /// - LE_NO_ERROR (0) = success
    /// - LE_OPEN_FILE_ERROR (1) = could not open file
    /// - LE_PARSING_ERROR (2) = syntax error in file
    pub fn load(&self, path: impl AsRef<Path>) -> Result<()> {
        let path_str = path.as_ref().to_string_lossy();
        let c_path = CString::new(path_str.as_ref())?;

        let inner = self.inner.lock();
        let result = unsafe { ffi::Load(inner.env, c_path.as_ptr()) };

        // LE_NO_ERROR = 0 means success
        if result != 0 {
            let message = match result {
                1 => "Could not open file",
                2 => "Parsing error in file",
                _ => "Unknown error",
            };
            Err(ClipsError::LoadFailed {
                file: path_str.into_owned(),
                message: message.to_string(),
            })
        } else {
            Ok(())
        }
    }

    /// Load constructs from a string
    ///
    /// Returns Ok(()) on success. LoadFromString returns bool (true = success).
    pub fn load_from_string(&self, constructs: &str) -> Result<()> {
        let c_constructs = CString::new(constructs)?;

        let inner = self.inner.lock();
        // Use usize::MAX to read entire string
        let result = unsafe { ffi::LoadFromString(inner.env, c_constructs.as_ptr(), usize::MAX) };

        if result {
            Ok(())
        } else {
            Err(ClipsError::ParseError {
                construct: constructs.chars().take(100).collect(),
                message: "Failed to parse constructs".to_string(),
            })
        }
    }

    /// Build a single construct from a string
    ///
    /// Build returns a BuildError enum:
    /// - BE_NO_ERROR (0) = success
    /// - BE_COULD_NOT_BUILD_ERROR (1) = could not build
    /// - BE_CONSTRUCT_NOT_FOUND_ERROR (2) = construct not found
    /// - BE_PARSING_ERROR (3) = parsing error
    pub fn build(&self, construct: &str) -> Result<()> {
        let c_construct = CString::new(construct)?;

        let inner = self.inner.lock();
        let result = unsafe { ffi::Build(inner.env, c_construct.as_ptr()) };

        // BE_NO_ERROR = 0 means success
        if result == 0 {
            Ok(())
        } else {
            Err(ClipsError::BuildFailed {
                construct: construct.to_string(),
            })
        }
    }

    /// Reset the environment (retract facts, reset globals, clear agenda)
    pub fn reset(&self) -> Result<()> {
        let inner = self.inner.lock();
        unsafe { ffi::Reset(inner.env) };
        Ok(())
    }

    /// Clear all constructs from the environment
    pub fn clear(&self) -> Result<()> {
        let inner = self.inner.lock();
        let result = unsafe { ffi::Clear(inner.env) };

        if result {
            Ok(())
        } else {
            Err(ClipsError::ClearFailed)
        }
    }

    /// Assert a fact from a string representation
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use clips_sys::ClipsEnvironment;
    /// # let env = ClipsEnvironment::new()?;
    /// let fact = env.assert_string("(person (name \"Alice\") (age 30))")?;
    /// println!("Asserted fact with index: {}", fact.index());
    /// # Ok::<(), clips_sys::ClipsError>(())
    /// ```
    pub fn assert_string(&self, fact_str: &str) -> Result<FactHandle> {
        let c_fact = CString::new(fact_str)?;

        let inner = self.inner.lock();
        let fact = unsafe { ffi::AssertString(inner.env, c_fact.as_ptr()) };

        if fact.is_null() {
            Err(ClipsError::AssertFailed {
                fact: fact_str.to_string(),
                message: "AssertString returned null".to_string(),
            })
        } else {
            Ok(FactHandle {
                fact,
                env: self.clone(),
            })
        }
    }

    /// Run the inference engine
    ///
    /// # Arguments
    ///
    /// * `limit` - Maximum number of rules to fire. `None` for unlimited.
    pub fn run(&self, limit: Option<i64>) -> Result<RunResult> {
        use std::os::raw::c_long;
        let inner = self.inner.lock();
        let rules_fired = unsafe { ffi::Run(inner.env, limit.unwrap_or(-1) as c_long) };

        Ok(RunResult {
            rules_fired: rules_fired as u64,
            completion_reason: RunCompletionReason::AgendaExhausted, // Default
        })
    }

    /// Halt the inference engine (can be called from another thread)
    pub fn halt(&self) {
        let inner = self.inner.lock();
        unsafe { ffi::Halt(inner.env) };
    }

    /// Evaluate a CLIPS expression
    pub fn eval(&self, expression: &str) -> Result<ClipsValue> {
        let c_expr = CString::new(expression)?;
        let mut result = ffi::CLIPSValue::new();

        let inner = self.inner.lock();
        let err_code = unsafe { ffi::Eval(inner.env, c_expr.as_ptr(), &mut result) };

        if err_code == 0 {
            unsafe { ClipsValue::from_ffi(&mut result, inner.env) }
        } else {
            Err(ClipsError::EvaluationFailed {
                expression: expression.to_string(),
            })
        }
    }

    /// Find a deftemplate by name
    pub fn find_template(&self, name: &str) -> Result<Option<DeftemplateHandle>> {
        let c_name = CString::new(name)?;

        let inner = self.inner.lock();
        let template = unsafe { ffi::FindDeftemplate(inner.env, c_name.as_ptr()) };

        if template.is_null() {
            Ok(None)
        } else {
            Ok(Some(DeftemplateHandle {
                template,
                env: self.clone(),
            }))
        }
    }

    /// Find a defrule by name
    pub fn find_rule(&self, name: &str) -> Result<Option<DefruleHandle>> {
        let c_name = CString::new(name)?;

        let inner = self.inner.lock();
        let rule = unsafe { ffi::FindDefrule(inner.env, c_name.as_ptr()) };

        if rule.is_null() {
            Ok(None)
        } else {
            Ok(Some(DefruleHandle {
                rule,
                env: self.clone(),
            }))
        }
    }

    /// Find a defglobal by name
    pub fn find_global(&self, name: &str) -> Result<Option<DefglobalHandle>> {
        let c_name = CString::new(name)?;

        let inner = self.inner.lock();
        let global = unsafe { ffi::FindDefglobal(inner.env, c_name.as_ptr()) };

        if global.is_null() {
            Ok(None)
        } else {
            Ok(Some(DefglobalHandle {
                global,
                env: self.clone(),
            }))
        }
    }

    /// Get an iterator over all facts
    pub fn facts(&self) -> FactIterator {
        FactIterator {
            env: self.clone(),
            current: ptr::null_mut(),
            started: false,
        }
    }

    /// Get an iterator over all deftemplates
    pub fn templates(&self) -> DeftemplateIterator {
        DeftemplateIterator {
            env: self.clone(),
            current: ptr::null_mut(),
            started: false,
        }
    }

    /// Get an iterator over all defrules
    pub fn rules(&self) -> DefruleIterator {
        DefruleIterator {
            env: self.clone(),
            current: ptr::null_mut(),
            started: false,
        }
    }

    /// Get an iterator over all defmodules
    pub fn modules(&self) -> DefmoduleIterator {
        DefmoduleIterator {
            env: self.clone(),
            current: ptr::null_mut(),
            started: false,
        }
    }

    /// Find a defmodule by name
    ///
    /// Returns `None` if no module with that name exists.
    pub fn find_module(&self, name: &str) -> Result<Option<DefmoduleHandle>> {
        let c_name = CString::new(name)?;

        let inner = self.inner.lock();
        let module = unsafe { ffi::FindDefmodule(inner.env, c_name.as_ptr()) };

        if module.is_null() {
            Ok(None)
        } else {
            Ok(Some(DefmoduleHandle {
                module,
                env: self.clone(),
            }))
        }
    }

    /// Get all module names as a Vec
    pub fn list_module_names(&self) -> Result<Vec<String>> {
        let mut names = Vec::new();
        for module_result in self.modules() {
            let module = module_result?;
            names.push(module.name()?);
        }
        Ok(names)
    }

    /// Push a module onto the focus stack
    ///
    /// That module's rules become eligible to fire during `run()`.
    pub fn focus(&self, module: &DefmoduleHandle) {
        unsafe { ffi::Focus(module.module) };
    }

    /// Get the module currently at the top of the focus stack
    ///
    /// Returns `None` if the focus stack is empty.
    pub fn get_focus(&self) -> Option<DefmoduleHandle> {
        let inner = self.inner.lock();
        let module = unsafe { ffi::GetFocus(inner.env) };
        if module.is_null() {
            None
        } else {
            Some(DefmoduleHandle {
                module,
                env: self.clone(),
            })
        }
    }

    /// Pop the top module from the focus stack
    ///
    /// Returns the popped module, or `None` if the stack was empty.
    pub fn pop_focus(&self) -> Option<DefmoduleHandle> {
        let inner = self.inner.lock();
        let module = unsafe { ffi::PopFocus(inner.env) };
        if module.is_null() {
            None
        } else {
            Some(DefmoduleHandle {
                module,
                env: self.clone(),
            })
        }
    }

    /// Clear the entire focus stack
    pub fn clear_focus_stack(&self) {
        let inner = self.inner.lock();
        unsafe { ffi::ClearFocusStack(inner.env) };
    }

    /// Get the current module as a handle
    pub fn get_current_module(&self) -> Option<DefmoduleHandle> {
        let inner = self.inner.lock();
        let module = unsafe { ffi::GetCurrentModule(inner.env) };
        if module.is_null() {
            None
        } else {
            Some(DefmoduleHandle {
                module,
                env: self.clone(),
            })
        }
    }

    /// Set the current module. Returns the previous current module.
    pub fn set_current_module(&self, module: &DefmoduleHandle) -> Option<DefmoduleHandle> {
        let inner = self.inner.lock();
        let prev = unsafe { ffi::SetCurrentModule(inner.env, module.module) };
        if prev.is_null() {
            None
        } else {
            Some(DefmoduleHandle {
                module: prev,
                env: self.clone(),
            })
        }
    }

    /// Retract all facts belonging to a named deftemplate
    ///
    /// Returns the number of facts retracted.
    /// Returns `Err` if the template name is not found.
    ///
    /// Facts are collected first and then retracted to avoid
    /// iterator invalidation during retraction.
    pub fn retract_by_template(&self, template_name: &str) -> Result<usize> {
        let template = self.find_template(template_name)?;
        let template = template.ok_or_else(|| ClipsError::TemplateNotFound {
            name: template_name.to_string(),
        })?;

        // Collect all fact handles first to avoid iterator invalidation
        let facts: Vec<FactHandle> = template.facts().filter_map(|f| f.ok()).collect();
        let count = facts.len();

        for fact in facts {
            fact.retract()?;
        }

        Ok(count)
    }

    /// Create a fact builder for a deftemplate
    pub fn fact_builder(&self, template_name: &str) -> Result<FactBuilder> {
        let c_name = CString::new(template_name)?;

        let inner = self.inner.lock();
        let builder = unsafe { ffi::CreateFactBuilder(inner.env, c_name.as_ptr()) };

        if builder.is_null() {
            Err(ClipsError::FactBuilderCreationFailed {
                template: template_name.to_string(),
            })
        } else {
            Ok(FactBuilder {
                builder,
                env: self.clone(),
            })
        }
    }

    /// Enable watching for a specific item
    pub fn watch(&self, item: WatchItem) -> Result<()> {
        let inner = self.inner.lock();
        let result = unsafe { ffi::Watch(inner.env, item.to_ffi()) };

        if result {
            Ok(())
        } else {
            Err(ClipsError::WatchFailed)
        }
    }

    /// Disable watching for a specific item
    pub fn unwatch(&self, item: WatchItem) -> Result<()> {
        let inner = self.inner.lock();
        let result = unsafe { ffi::Unwatch(inner.env, item.to_ffi()) };

        if result {
            Ok(())
        } else {
            Err(ClipsError::UnwatchFailed)
        }
    }

    /// Get the current conflict resolution strategy
    pub fn get_strategy(&self) -> Strategy {
        let inner = self.inner.lock();
        let code = unsafe { ffi::GetStrategy(inner.env) };
        Strategy::from_ffi(code)
    }

    /// Set the conflict resolution strategy
    pub fn set_strategy(&self, strategy: Strategy) {
        let inner = self.inner.lock();
        unsafe { ffi::SetStrategy(inner.env, strategy.to_ffi()) };
    }

    /// Get fact duplication setting
    ///
    /// Returns true if duplicate facts are allowed.
    pub fn get_fact_duplication(&self) -> bool {
        let inner = self.inner.lock();
        unsafe { ffi::GetFactDuplication(inner.env) }
    }

    /// Set fact duplication setting
    ///
    /// When set to true, CLIPS allows identical facts to be asserted multiple times.
    /// The default is false (duplicate facts are rejected).
    ///
    /// Returns the previous value.
    pub fn set_fact_duplication(&self, allow: bool) -> bool {
        let inner = self.inner.lock();
        unsafe { ffi::SetFactDuplication(inner.env, allow) }
    }

    /// Get the number of activations on the agenda
    pub fn agenda_size(&self) -> usize {
        let inner = self.inner.lock();
        let size = unsafe { ffi::GetAgendaSize(inner.env, ptr::null_mut()) };
        size as usize
    }

    /// Clear the agenda
    pub fn clear_agenda(&self) {
        let inner = self.inner.lock();
        unsafe { ffi::ClearAgenda(inner.env, ptr::null_mut()) };
    }

    /// Get the CLIPS version string
    pub fn version() -> String {
        unsafe {
            let ptr = ffi::Version();
            if ptr.is_null() {
                return "Unknown".to_string();
            }
            CStr::from_ptr(ptr)
                .to_str()
                .unwrap_or("Unknown")
                .to_string()
        }
    }
}

impl Drop for ClipsEnvironment {
    fn drop(&mut self) {
        // Intentionally skip DestroyEnvironment() to avoid CLIPS 6.4.2 SIGABRT.
        //
        // Root cause: CLIPS 6.4.2 internal symbol tables and memory tracking
        // become corrupted during sequential environment destruction, triggering
        // SIGABRT in DestroyEnvironment(). This is a known limitation of CLIPS
        // 6.4.2 documented in the v0.8.0 and v0.8.1 release notes.
        //
        // Impact: The leaked environment pointer (~10-50 KB per environment) is
        // reclaimed by the OS at process exit. For long-running services creating
        // many environments, this may increase resident memory. A proper fix via
        // CLIPS 6.5.x upgrade is tracked for v0.9.0.
        //
        // Previous implementation called:
        //   let _guard = ENV_CREATION_LOCK.lock();
        //   unsafe { ffi::DestroyEnvironment(inner.env) };
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Convert a `ClipsValue` to an FFI `CLIPSValue`.
///
/// This is a best-effort conversion used for setting global values and instance
/// slots. Values that cannot be represented directly (e.g. `FactAddress`,
/// `InstanceAddress`) are converted to void.
pub(crate) fn value_to_ffi(
    value: &ClipsValue,
    env: *mut ffi::Environment,
) -> Result<ffi::CLIPSValue> {
    let mut cv = ffi::CLIPSValue::new();
    match value {
        ClipsValue::Void => unsafe { ffi::CVSetVoid(&mut cv) },
        ClipsValue::Integer(i) => unsafe { ffi::CVSetInteger(env, &mut cv, *i) },
        ClipsValue::Float(f) => unsafe { ffi::CVSetFloat(env, &mut cv, *f) },
        ClipsValue::Boolean(b) => {
            let sym: &[u8] = if *b { b"TRUE\0" } else { b"FALSE\0" };
            unsafe { ffi::CVSetSymbol(env, &mut cv, sym.as_ptr() as *const i8) }
        }
        ClipsValue::Symbol(s) => {
            let c_s = CString::new(s.as_str())?;
            unsafe { ffi::CVSetSymbol(env, &mut cv, c_s.as_ptr()) }
        }
        ClipsValue::String(s) => {
            let c_s = CString::new(s.as_str())?;
            unsafe { ffi::CVSetString(env, &mut cv, c_s.as_ptr()) }
        }
        // FactAddress, InstanceAddress, ExternalAddress, and Multifield
        // cannot be trivially round-tripped here; set void as a safe fallback.
        _ => unsafe { ffi::CVSetVoid(&mut cv) },
    }
    Ok(cv)
}

// ============================================================================
// Handle Types
// ============================================================================

/// Handle to a fact in the fact-list
pub struct FactHandle {
    fact: *mut ffi::Fact,
    env: ClipsEnvironment,
}

impl FactHandle {
    /// Get the fact's index
    pub fn index(&self) -> i64 {
        unsafe { ffi::FactIndex(self.fact) as i64 }
    }

    /// Check if the fact still exists
    pub fn exists(&self) -> bool {
        unsafe { ffi::FactExistp(self.fact) }
    }

    /// Retract this fact
    pub fn retract(self) -> Result<()> {
        let result = unsafe { ffi::Retract(self.fact) };
        if result == 0 {
            Ok(())
        } else {
            Err(ClipsError::RetractFailed {
                fact_index: self.index(),
            })
        }
    }

    /// Get a slot value
    ///
    /// GetFactSlot returns a GetSlotError enum:
    /// - GSE_NO_ERROR (0) = success
    /// - GSE_NULL_POINTER_ERROR (1) = null pointer
    /// - GSE_INVALID_TARGET_ERROR (2) = invalid target
    /// - GSE_SLOT_NOT_FOUND_ERROR (3) = slot not found
    pub fn get_slot(&self, slot_name: &str) -> Result<ClipsValue> {
        let c_name = CString::new(slot_name)?;
        let mut value = ffi::CLIPSValue::new();

        let result = unsafe { ffi::GetFactSlot(self.fact, c_name.as_ptr(), &mut value) };

        // GSE_NO_ERROR = 0 means success
        if result == 0 {
            let env_ptr = unsafe { self.env.raw() };
            unsafe { ClipsValue::from_ffi(&mut value, env_ptr) }
        } else {
            Err(ClipsError::SlotNotFound {
                template: self.template_name().unwrap_or_default(),
                slot: slot_name.to_string(),
            })
        }
    }

    /// Get the template name
    pub fn template_name(&self) -> Result<String> {
        let template = unsafe { ffi::FactDeftemplate(self.fact) };
        if template.is_null() {
            return Ok("implied".to_string());
        }
        let name_ptr = unsafe { ffi::DeftemplateName(template) };
        if name_ptr.is_null() {
            return Ok("unknown".to_string());
        }
        let name = unsafe { CStr::from_ptr(name_ptr).to_str()? };
        Ok(name.to_string())
    }

    /// Get all slot values as a map
    pub fn slot_values(&self) -> Result<SlotValues> {
        let template = unsafe { ffi::FactDeftemplate(self.fact) };
        if template.is_null() {
            return Ok(HashMap::new());
        }

        // Get slot names (DeftemplateSlotNames returns void, populates the CLIPSValue)
        let mut slot_names_value = ffi::CLIPSValue::new();
        unsafe { ffi::DeftemplateSlotNames(template, &mut slot_names_value) };

        let env_ptr = unsafe { self.env.raw() };
        let slot_names = unsafe { ClipsValue::from_ffi(&mut slot_names_value, env_ptr)? };

        let mut values = HashMap::new();
        if let ClipsValue::Multifield(names) = slot_names {
            for name in names {
                if let ClipsValue::Symbol(slot_name) = name
                    && let Ok(value) = self.get_slot(&slot_name)
                {
                    values.insert(slot_name, value);
                }
            }
        }

        Ok(values)
    }

    /// Get the pretty-print form
    pub fn pp_form(&self) -> String {
        let mut buffer = vec![0u8; 4096];
        unsafe {
            ffi::FactPPForm(self.fact, buffer.as_mut_ptr() as *mut i8, buffer.len());
        }
        let len = buffer.iter().position(|&b| b == 0).unwrap_or(buffer.len());
        String::from_utf8_lossy(&buffer[..len]).to_string()
    }
}

/// Handle to a deftemplate
pub struct DeftemplateHandle {
    template: *mut ffi::Deftemplate,
    env: ClipsEnvironment,
}

impl DeftemplateHandle {
    /// Get the template name
    pub fn name(&self) -> Result<String> {
        let name_ptr = unsafe { ffi::DeftemplateName(self.template) };
        name_ptr.null_check("DeftemplateName")?;
        let name = unsafe { CStr::from_ptr(name_ptr).to_str()? };
        Ok(name.to_string())
    }

    /// Get the module name
    pub fn module_name(&self) -> Result<String> {
        let name_ptr = unsafe { ffi::DeftemplateModule(self.template) };
        name_ptr.null_check("DeftemplateModule")?;
        let name = unsafe { CStr::from_ptr(name_ptr).to_str()? };
        Ok(name.to_string())
    }

    /// Get the pretty-print form
    pub fn pp_form(&self) -> Option<String> {
        let pp_ptr = unsafe { ffi::DeftemplatePPForm(self.template) };
        if pp_ptr.is_null() {
            return None;
        }
        unsafe { CStr::from_ptr(pp_ptr).to_str().ok().map(|s| s.to_string()) }
    }

    /// Check if a slot exists
    pub fn slot_exists(&self, slot_name: &str) -> Result<bool> {
        let c_name = CString::new(slot_name)?;
        Ok(unsafe { ffi::DeftemplateSlotExistP(self.template, c_name.as_ptr()) })
    }

    /// Check if a slot is a multislot
    pub fn slot_is_multi(&self, slot_name: &str) -> Result<bool> {
        let c_name = CString::new(slot_name)?;
        Ok(unsafe { ffi::DeftemplateSlotMultiP(self.template, c_name.as_ptr()) })
    }

    /// Get slot names
    pub fn slot_names(&self) -> Result<Vec<String>> {
        let mut value = ffi::CLIPSValue::new();
        // DeftemplateSlotNames returns void and populates the CLIPSValue
        unsafe { ffi::DeftemplateSlotNames(self.template, &mut value) };

        let env_ptr = unsafe { self.env.raw() };
        let clips_value = unsafe { ClipsValue::from_ffi(&mut value, env_ptr)? };

        let mut names = Vec::new();
        if let ClipsValue::Multifield(items) = clips_value {
            for item in items {
                if let ClipsValue::Symbol(name) = item {
                    names.push(name);
                }
            }
        }

        Ok(names)
    }

    /// Get the default value of a slot
    ///
    /// Returns `None` if the slot doesn't have a default value or if it uses
    /// `(default ?DERIVE)` or `(default ?NONE)`.
    pub fn slot_default_value(&self, slot_name: &str) -> Result<Option<ClipsValue>> {
        let c_name = CString::new(slot_name)?;
        let mut value = ffi::CLIPSValue::new();

        let success =
            unsafe { ffi::DeftemplateSlotDefaultValue(self.template, c_name.as_ptr(), &mut value) };

        if !success {
            return Err(ClipsError::SlotNotFound {
                template: self.name().unwrap_or_default(),
                slot: slot_name.to_string(),
            });
        }

        let env_ptr = unsafe { self.env.raw() };
        let clips_value = unsafe { ClipsValue::from_ffi(&mut value, env_ptr)? };

        // Check for special CLIPS symbols that indicate no concrete default
        match &clips_value {
            ClipsValue::Symbol(s) if s == "?DERIVE" || s == "?NONE" => Ok(None),
            ClipsValue::Void => Ok(None),
            _ => Ok(Some(clips_value)),
        }
    }

    /// Get the allowed values of a slot
    ///
    /// Returns a list of allowed values if the slot has an `allowed-symbols`,
    /// `allowed-strings`, `allowed-integers`, `allowed-floats`, or `allowed-values`
    /// constraint. Returns `None` if no constraint is defined.
    pub fn slot_allowed_values(&self, slot_name: &str) -> Result<Option<Vec<ClipsValue>>> {
        let c_name = CString::new(slot_name)?;
        let mut value = ffi::CLIPSValue::new();

        let success = unsafe {
            ffi::DeftemplateSlotAllowedValues(self.template, c_name.as_ptr(), &mut value)
        };

        if !success {
            return Err(ClipsError::SlotNotFound {
                template: self.name().unwrap_or_default(),
                slot: slot_name.to_string(),
            });
        }

        let env_ptr = unsafe { self.env.raw() };
        let clips_value = unsafe { ClipsValue::from_ffi(&mut value, env_ptr)? };

        match clips_value {
            ClipsValue::Multifield(items) => {
                if items.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(items))
                }
            }
            ClipsValue::Symbol(s) if s == "FALSE" => Ok(None),
            _ => Ok(None),
        }
    }

    /// Get the cardinality constraint of a multislot
    ///
    /// Returns `(min, max)` where max is `i64::MAX` for unbounded cardinality.
    /// Returns `None` if no cardinality constraint is defined.
    pub fn slot_cardinality(&self, slot_name: &str) -> Result<Option<(i64, i64)>> {
        let c_name = CString::new(slot_name)?;
        let mut value = ffi::CLIPSValue::new();

        let success =
            unsafe { ffi::DeftemplateSlotCardinality(self.template, c_name.as_ptr(), &mut value) };

        if !success {
            return Err(ClipsError::SlotNotFound {
                template: self.name().unwrap_or_default(),
                slot: slot_name.to_string(),
            });
        }

        let env_ptr = unsafe { self.env.raw() };
        let clips_value = unsafe { ClipsValue::from_ffi(&mut value, env_ptr)? };

        match clips_value {
            ClipsValue::Multifield(items) if items.len() >= 2 => {
                let min = match &items[0] {
                    ClipsValue::Integer(n) => *n,
                    _ => 0,
                };
                let max = match &items[1] {
                    ClipsValue::Integer(n) => *n,
                    ClipsValue::Symbol(s) if s == "+" || s == "*" => i64::MAX,
                    _ => i64::MAX,
                };
                Ok(Some((min, max)))
            }
            ClipsValue::Symbol(s) if s == "FALSE" => Ok(None),
            _ => Ok(None),
        }
    }

    /// Get the range constraint of a numeric slot
    ///
    /// Returns `(min, max)` for the allowed range. Uses `f64::MIN`/`f64::MAX` for unbounded.
    /// Returns `None` if no range constraint is defined.
    pub fn slot_range(&self, slot_name: &str) -> Result<Option<(f64, f64)>> {
        let c_name = CString::new(slot_name)?;
        let mut value = ffi::CLIPSValue::new();

        let success =
            unsafe { ffi::DeftemplateSlotRange(self.template, c_name.as_ptr(), &mut value) };

        if !success {
            return Err(ClipsError::SlotNotFound {
                template: self.name().unwrap_or_default(),
                slot: slot_name.to_string(),
            });
        }

        let env_ptr = unsafe { self.env.raw() };
        let clips_value = unsafe { ClipsValue::from_ffi(&mut value, env_ptr)? };

        match clips_value {
            ClipsValue::Multifield(items) if items.len() >= 2 => {
                let min = match &items[0] {
                    ClipsValue::Float(n) => *n,
                    ClipsValue::Integer(n) => *n as f64,
                    ClipsValue::Symbol(s) if s == "-oo" => f64::NEG_INFINITY,
                    _ => f64::NEG_INFINITY,
                };
                let max = match &items[1] {
                    ClipsValue::Float(n) => *n,
                    ClipsValue::Integer(n) => *n as f64,
                    ClipsValue::Symbol(s) if s == "+oo" => f64::INFINITY,
                    _ => f64::INFINITY,
                };
                Ok(Some((min, max)))
            }
            ClipsValue::Symbol(s) if s == "FALSE" => Ok(None),
            _ => Ok(None),
        }
    }

    /// Iterate over all facts currently asserted for this template
    pub fn facts(&self) -> TemplateFactsIterator {
        TemplateFactsIterator {
            template: self.template,
            env: self.env.clone(),
            current: ptr::null_mut(),
            started: false,
        }
    }

    /// Get the allowed types of a slot
    ///
    /// Returns a list of type names (e.g., `["STRING", "SYMBOL", "INTEGER"]`).
    /// Returns `None` if all types are allowed.
    pub fn slot_types(&self, slot_name: &str) -> Result<Option<Vec<String>>> {
        let c_name = CString::new(slot_name)?;
        let mut value = ffi::CLIPSValue::new();

        let success =
            unsafe { ffi::DeftemplateSlotTypes(self.template, c_name.as_ptr(), &mut value) };

        if !success {
            return Err(ClipsError::SlotNotFound {
                template: self.name().unwrap_or_default(),
                slot: slot_name.to_string(),
            });
        }

        let env_ptr = unsafe { self.env.raw() };
        let clips_value = unsafe { ClipsValue::from_ffi(&mut value, env_ptr)? };

        match clips_value {
            ClipsValue::Multifield(items) => {
                let types: Vec<String> = items
                    .into_iter()
                    .filter_map(|v| match v {
                        ClipsValue::Symbol(s) => Some(s),
                        _ => None,
                    })
                    .collect();
                if types.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(types))
                }
            }
            ClipsValue::Symbol(s) if s == "FALSE" => Ok(None),
            _ => Ok(None),
        }
    }
}

/// Handle to a defrule
pub struct DefruleHandle {
    rule: *mut ffi::Defrule,
    #[allow(dead_code)]
    env: ClipsEnvironment,
}

impl DefruleHandle {
    /// Get the rule name
    pub fn name(&self) -> Result<String> {
        let name_ptr = unsafe { ffi::DefruleName(self.rule) };
        name_ptr.null_check("DefruleName")?;
        let name = unsafe { CStr::from_ptr(name_ptr).to_str()? };
        Ok(name.to_string())
    }

    /// Get the module name
    pub fn module_name(&self) -> Result<String> {
        let name_ptr = unsafe { ffi::DefruleModule(self.rule) };
        name_ptr.null_check("DefruleModule")?;
        let name = unsafe { CStr::from_ptr(name_ptr).to_str()? };
        Ok(name.to_string())
    }

    /// Get the number of times this rule has fired
    pub fn times_fired(&self) -> u64 {
        unsafe { ffi::GetDefruleFirings(self.rule) as u64 }
    }

    /// Check if this rule has a breakpoint
    pub fn has_breakpoint(&self) -> bool {
        unsafe { ffi::DefruleHasBreakpoint(self.rule) }
    }

    /// Set a breakpoint on this rule
    pub fn set_breakpoint(&self) {
        unsafe { ffi::SetBreak(self.rule) };
    }

    /// Remove the breakpoint from this rule
    pub fn remove_breakpoint(&self) -> bool {
        unsafe { ffi::RemoveBreak(self.rule) }
    }

    /// Get the pretty-print form
    pub fn pp_form(&self) -> Option<String> {
        let pp_ptr = unsafe { ffi::DefrulePPForm(self.rule) };
        if pp_ptr.is_null() {
            return None;
        }
        unsafe { CStr::from_ptr(pp_ptr).to_str().ok().map(|s| s.to_string()) }
    }
}

/// Handle to a defmodule
pub struct DefmoduleHandle {
    module: *mut ffi::Defmodule,
    #[allow(dead_code)]
    env: ClipsEnvironment,
}

impl DefmoduleHandle {
    /// Get the module name
    pub fn name(&self) -> Result<String> {
        let name_ptr = unsafe { ffi::DefmoduleName(self.module) };
        name_ptr.null_check("DefmoduleName")?;
        let name = unsafe { CStr::from_ptr(name_ptr).to_str()? };
        Ok(name.to_string())
    }

    /// Get the pretty-print form of the module definition
    pub fn pp_form(&self) -> Option<String> {
        let pp_ptr = unsafe { ffi::DefmodulePPForm(self.module) };
        if pp_ptr.is_null() {
            return None;
        }
        unsafe { CStr::from_ptr(pp_ptr).to_str().ok().map(|s| s.to_string()) }
    }
}

/// Handle to a defglobal
pub struct DefglobalHandle {
    global: *mut ffi::Defglobal,
    env: ClipsEnvironment,
}

impl DefglobalHandle {
    /// Get the global name
    pub fn name(&self) -> Result<String> {
        let name_ptr = unsafe { ffi::DefglobalName(self.global) };
        name_ptr.null_check("DefglobalName")?;
        let name = unsafe { CStr::from_ptr(name_ptr).to_str()? };
        Ok(name.to_string())
    }

    /// Get the global value
    pub fn get_value(&self) -> Result<ClipsValue> {
        let mut value = ffi::CLIPSValue::new();
        // DefglobalGetValue is void in CLIPS 6.4 — always succeeds if global handle is valid
        unsafe { ffi::DefglobalGetValue(self.global, &mut value) };
        let env_ptr = unsafe { self.env.raw() };
        unsafe { ClipsValue::from_ffi(&mut value, env_ptr) }
    }

    /// Set the global value
    ///
    /// Sets the current value of this defglobal. The value must be a compatible
    /// CLIPS value type. Returns `Ok(())` on success.
    pub fn set_value(&self, value: &ClipsValue) -> Result<()> {
        let env_ptr = unsafe { self.env.raw() };
        let mut clips_value = value_to_ffi(value, env_ptr)?;
        // DefglobalSetValue is void in CLIPS 6.4 — always succeeds if global handle is valid
        unsafe { ffi::DefglobalSetValue(self.global, &mut clips_value) };
        Ok(())
    }
}

// ============================================================================
// Fact Builder
// ============================================================================

/// Builder for creating facts programmatically
pub struct FactBuilder {
    builder: *mut ffi::FactBuilder,
    env: ClipsEnvironment,
}

impl FactBuilder {
    /// Put an integer value in a slot
    pub fn put_integer(&mut self, slot_name: &str, value: i64) -> Result<&mut Self> {
        use std::os::raw::c_long;
        let c_name = CString::new(slot_name)?;
        let result =
            unsafe { ffi::FBPutSlotInteger(self.builder, c_name.as_ptr(), value as c_long) };
        if result == 0 {
            Ok(self)
        } else {
            Err(self.get_error())
        }
    }

    /// Put a float value in a slot
    pub fn put_float(&mut self, slot_name: &str, value: f64) -> Result<&mut Self> {
        let c_name = CString::new(slot_name)?;
        let result = unsafe { ffi::FBPutSlotFloat(self.builder, c_name.as_ptr(), value) };
        if result == 0 {
            Ok(self)
        } else {
            Err(self.get_error())
        }
    }

    /// Put a string value in a slot
    pub fn put_string(&mut self, slot_name: &str, value: &str) -> Result<&mut Self> {
        let c_name = CString::new(slot_name)?;
        let c_value = CString::new(value)?;
        let result =
            unsafe { ffi::FBPutSlotString(self.builder, c_name.as_ptr(), c_value.as_ptr()) };
        if result == 0 {
            Ok(self)
        } else {
            Err(self.get_error())
        }
    }

    /// Put a symbol value in a slot
    pub fn put_symbol(&mut self, slot_name: &str, value: &str) -> Result<&mut Self> {
        let c_name = CString::new(slot_name)?;
        let c_value = CString::new(value)?;
        let result =
            unsafe { ffi::FBPutSlotSymbol(self.builder, c_name.as_ptr(), c_value.as_ptr()) };
        if result == 0 {
            Ok(self)
        } else {
            Err(self.get_error())
        }
    }

    /// Assert the built fact
    pub fn assert(self) -> Result<FactHandle> {
        let fact = unsafe { ffi::FBAssert(self.builder) };
        if fact.is_null() {
            Err(self.get_error())
        } else {
            Ok(FactHandle {
                fact,
                env: self.env.clone(),
            })
        }
    }

    /// Abort the fact builder without asserting
    pub fn abort(self) {
        unsafe { ffi::FBAbort(self.builder) };
    }

    fn get_error(&self) -> ClipsError {
        let env_ptr = unsafe { self.env.raw() };
        let code = unsafe { ffi::FBError(env_ptr) };
        ClipsError::fact_builder_error(code)
    }
}

impl Drop for FactBuilder {
    fn drop(&mut self) {
        unsafe { ffi::FBDispose(self.builder) };
    }
}

// ============================================================================
// Iterators
// ============================================================================

/// Iterator over facts
pub struct FactIterator {
    env: ClipsEnvironment,
    current: *mut ffi::Fact,
    started: bool,
}

impl Iterator for FactIterator {
    type Item = Result<FactHandle>;

    fn next(&mut self) -> Option<Self::Item> {
        let inner = self.env.inner.lock();
        let next = if self.started {
            unsafe { ffi::GetNextFact(inner.env, self.current) }
        } else {
            self.started = true;
            unsafe { ffi::GetNextFact(inner.env, ptr::null_mut()) }
        };

        if next.is_null() {
            None
        } else {
            self.current = next;
            Some(Ok(FactHandle {
                fact: next,
                env: self.env.clone(),
            }))
        }
    }
}

/// Iterator over deftemplates
pub struct DeftemplateIterator {
    env: ClipsEnvironment,
    current: *mut ffi::Deftemplate,
    started: bool,
}

impl Iterator for DeftemplateIterator {
    type Item = Result<DeftemplateHandle>;

    fn next(&mut self) -> Option<Self::Item> {
        let inner = self.env.inner.lock();
        let next = if self.started {
            unsafe { ffi::GetNextDeftemplate(inner.env, self.current) }
        } else {
            self.started = true;
            unsafe { ffi::GetNextDeftemplate(inner.env, ptr::null_mut()) }
        };

        if next.is_null() {
            None
        } else {
            self.current = next;
            Some(Ok(DeftemplateHandle {
                template: next,
                env: self.env.clone(),
            }))
        }
    }
}

/// Iterator over defrules
pub struct DefruleIterator {
    env: ClipsEnvironment,
    current: *mut ffi::Defrule,
    started: bool,
}

impl Iterator for DefruleIterator {
    type Item = Result<DefruleHandle>;

    fn next(&mut self) -> Option<Self::Item> {
        let inner = self.env.inner.lock();
        let next = if self.started {
            unsafe { ffi::GetNextDefrule(inner.env, self.current) }
        } else {
            self.started = true;
            unsafe { ffi::GetNextDefrule(inner.env, ptr::null_mut()) }
        };

        if next.is_null() {
            None
        } else {
            self.current = next;
            Some(Ok(DefruleHandle {
                rule: next,
                env: self.env.clone(),
            }))
        }
    }
}

/// Iterator over defmodules
pub struct DefmoduleIterator {
    env: ClipsEnvironment,
    current: *mut ffi::Defmodule,
    started: bool,
}

impl Iterator for DefmoduleIterator {
    type Item = Result<DefmoduleHandle>;

    fn next(&mut self) -> Option<Self::Item> {
        let inner = self.env.inner.lock();
        let next = if self.started {
            unsafe { ffi::GetNextDefmodule(inner.env, self.current) }
        } else {
            self.started = true;
            unsafe { ffi::GetNextDefmodule(inner.env, ptr::null_mut()) }
        };

        if next.is_null() {
            None
        } else {
            self.current = next;
            Some(Ok(DefmoduleHandle {
                module: next,
                env: self.env.clone(),
            }))
        }
    }
}

/// Iterator over facts belonging to a specific deftemplate
pub struct TemplateFactsIterator {
    template: *mut ffi::Deftemplate,
    env: ClipsEnvironment,
    current: *mut ffi::Fact,
    started: bool,
}

impl Iterator for TemplateFactsIterator {
    type Item = Result<FactHandle>;

    fn next(&mut self) -> Option<Self::Item> {
        let next = if self.started {
            unsafe { ffi::GetNextFactInTemplate(self.template, self.current) }
        } else {
            self.started = true;
            unsafe { ffi::GetNextFactInTemplate(self.template, ptr::null_mut()) }
        };

        if next.is_null() {
            None
        } else {
            self.current = next;
            Some(Ok(FactHandle {
                fact: next,
                env: self.env.clone(),
            }))
        }
    }
}
