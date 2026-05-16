//! Advanced CLIPS constructs and operations
//!
//! This module provides safe wrappers for advanced CLIPS features including:
//! - COOL (CLIPS Object-Oriented Language)
//! - Generic functions and methods
//! - Message handlers
//! - Pattern matching utilities
//! - Conflict resolution customization

use crate::environment::ClipsEnvironment;
use crate::error::{ClipsError, Result};
use crate::ffi;
use crate::value::ClipsValue;

use std::ffi::{CStr, CString};
use std::ptr;

// ============================================================================
// COOL (CLIPS Object-Oriented Language)
// ============================================================================

/// Handle to a defclass
pub struct DefclassHandle {
    class: *mut ffi::Defclass,
    env: ClipsEnvironment,
}

impl DefclassHandle {
    /// Get the class name
    pub fn name(&self) -> Result<String> {
        let name_ptr = unsafe { ffi::DefclassName(self.class) };
        if name_ptr.is_null() {
            return Err(ClipsError::NullPointer {
                operation: "DefclassName".to_string(),
            });
        }
        let name = unsafe { CStr::from_ptr(name_ptr).to_str()? };
        Ok(name.to_string())
    }

    /// Get the module name
    pub fn module_name(&self) -> Result<String> {
        let name_ptr = unsafe { ffi::DefclassModule(self.class) };
        if name_ptr.is_null() {
            return Err(ClipsError::NullPointer {
                operation: "DefclassModule".to_string(),
            });
        }
        let name = unsafe { CStr::from_ptr(name_ptr).to_str()? };
        Ok(name.to_string())
    }

    /// Get the pretty-print form
    pub fn pp_form(&self) -> Option<String> {
        let pp_ptr = unsafe { ffi::DefclassPPForm(self.class) };
        if pp_ptr.is_null() {
            return None;
        }
        unsafe { CStr::from_ptr(pp_ptr).to_str().ok().map(|s| s.to_string()) }
    }

    /// Get instances of this class
    pub fn instances(&self) -> InstanceIterator {
        InstanceIterator {
            class: self.class,
            env: self.env.clone(),
            current: ptr::null_mut(),
            started: false,
        }
    }
}

/// Handle to a CLIPS instance
pub struct InstanceHandle {
    instance: *mut ffi::Instance,
    env: ClipsEnvironment,
}

impl InstanceHandle {
    /// Get the instance name
    pub fn name(&self) -> Result<String> {
        let name_ptr = unsafe { ffi::InstanceName(self.instance) };
        if name_ptr.is_null() {
            return Err(ClipsError::NullPointer {
                operation: "InstanceName".to_string(),
            });
        }
        let name = unsafe { CStr::from_ptr(name_ptr).to_str()? };
        Ok(name.to_string())
    }

    /// Get a slot value
    pub fn get_slot(&self, slot_name: &str) -> Result<ClipsValue> {
        let c_name = CString::new(slot_name)?;
        let mut value = ffi::CLIPSValue::new();

        let success = unsafe { ffi::DirectGetSlot(self.instance, c_name.as_ptr(), &mut value) };

        if success {
            let env_ptr = unsafe { self.env.raw() };
            unsafe { ClipsValue::from_ffi(&mut value, env_ptr) }
        } else {
            Err(ClipsError::SlotNotFound {
                template: self.name().unwrap_or_default(),
                slot: slot_name.to_string(),
            })
        }
    }

    /// Set a slot value
    pub fn put_slot(&self, slot_name: &str, value: &ClipsValue) -> Result<()> {
        let c_name = CString::new(slot_name)?;
        let mut clips_value = self.value_to_ffi(value)?;

        let success =
            unsafe { ffi::DirectPutSlot(self.instance, c_name.as_ptr(), &mut clips_value) };

        if success {
            Ok(())
        } else {
            Err(ClipsError::InvalidSlotType {
                slot: slot_name.to_string(),
                expected: "compatible type".to_string(),
                got: value.type_name().to_string(),
            })
        }
    }

    /// Send a message to the instance
    pub fn send(&self, message: &str, args: Option<&str>) -> Result<ClipsValue> {
        let c_message = CString::new(message)?;
        let c_args = args.map(CString::new).transpose()?;
        let args_ptr = c_args.as_ref().map(|s| s.as_ptr()).unwrap_or(ptr::null());

        let mut instance_value = ffi::CLIPSValue::new();
        let mut result = ffi::CLIPSValue::new();

        // Set instance value
        unsafe {
            ffi::CVSetInstance(&mut instance_value, self.instance);
        }

        let env_ptr = unsafe { self.env.raw() };
        let success = unsafe {
            ffi::Send(
                env_ptr,
                &mut instance_value,
                c_message.as_ptr(),
                args_ptr,
                &mut result,
            )
        };

        if success {
            unsafe { ClipsValue::from_ffi(&mut result, env_ptr) }
        } else {
            Err(ClipsError::MessageSendFailed {
                message: message.to_string(),
            })
        }
    }

    /// Delete this instance
    pub fn delete(self) -> Result<()> {
        let success = unsafe { ffi::DeleteInstance(self.instance) };
        if success {
            Ok(())
        } else {
            Err(ClipsError::InstanceCreationFailed {
                message: "Failed to delete instance".to_string(),
            })
        }
    }

    /// Get the pretty-print form
    pub fn pp_form(&self) -> String {
        let mut buffer = vec![0u8; 4096];
        unsafe {
            ffi::InstancePPForm(self.instance, buffer.as_mut_ptr() as *mut i8, buffer.len());
        }
        let len = buffer.iter().position(|&b| b == 0).unwrap_or(buffer.len());
        String::from_utf8_lossy(&buffer[..len]).to_string()
    }

    fn value_to_ffi(&self, _value: &ClipsValue) -> Result<ffi::CLIPSValue> {
        // This would need full implementation to convert Rust values to CLIPS
        Ok(ffi::CLIPSValue::new())
    }
}

/// Iterator over instances of a class
pub struct InstanceIterator {
    class: *mut ffi::Defclass,
    env: ClipsEnvironment,
    current: *mut ffi::Instance,
    started: bool,
}

impl Iterator for InstanceIterator {
    type Item = Result<InstanceHandle>;

    fn next(&mut self) -> Option<Self::Item> {
        let next = if self.started {
            unsafe { ffi::GetNextInstanceInClass(self.class, self.current) }
        } else {
            self.started = true;
            unsafe { ffi::GetNextInstanceInClass(self.class, ptr::null_mut()) }
        };

        if next.is_null() {
            None
        } else {
            self.current = next;
            Some(Ok(InstanceHandle {
                instance: next,
                env: self.env.clone(),
            }))
        }
    }
}

// ============================================================================
// Generic Functions
// ============================================================================

/// Handle to a defgeneric
pub struct DefgenericHandle {
    generic: *mut ffi::Defgeneric,
    #[allow(dead_code)]
    env: ClipsEnvironment,
}

impl DefgenericHandle {
    /// Get the generic function name
    pub fn name(&self) -> Result<String> {
        let name_ptr = unsafe { ffi::DefgenericName(self.generic) };
        if name_ptr.is_null() {
            return Err(ClipsError::NullPointer {
                operation: "DefgenericName".to_string(),
            });
        }
        let name = unsafe { CStr::from_ptr(name_ptr).to_str()? };
        Ok(name.to_string())
    }

    /// Get the module name
    pub fn module_name(&self) -> Result<String> {
        let name_ptr = unsafe { ffi::DefgenericModule(self.generic) };
        if name_ptr.is_null() {
            return Err(ClipsError::NullPointer {
                operation: "DefgenericModule".to_string(),
            });
        }
        let name = unsafe { CStr::from_ptr(name_ptr).to_str()? };
        Ok(name.to_string())
    }

    /// Get the pretty-print form
    pub fn pp_form(&self) -> Option<String> {
        let pp_ptr = unsafe { ffi::DefgenericPPForm(self.generic) };
        if pp_ptr.is_null() {
            return None;
        }
        unsafe { CStr::from_ptr(pp_ptr).to_str().ok().map(|s| s.to_string()) }
    }
}

// ============================================================================
// Deffunction
// ============================================================================

/// Handle to a deffunction construct
pub struct DeffunctionHandle {
    func: *mut ffi::Deffunction,
    #[allow(dead_code)]
    env: ClipsEnvironment,
}

impl DeffunctionHandle {
    /// Get the deffunction name
    pub fn name(&self) -> Result<String> {
        let name_ptr = unsafe { ffi::DeffunctionName(self.func) };
        if name_ptr.is_null() {
            return Err(ClipsError::NullPointer {
                operation: "DeffunctionName".to_string(),
            });
        }
        let name = unsafe { CStr::from_ptr(name_ptr).to_str()? };
        Ok(name.to_string())
    }

    /// Get the pretty-print form
    pub fn pp_form(&self) -> Option<String> {
        let pp_ptr = unsafe { ffi::DeffunctionPPForm(self.func) };
        if pp_ptr.is_null() {
            return None;
        }
        unsafe { CStr::from_ptr(pp_ptr).to_str().ok().map(|s| s.to_string()) }
    }
}

// ============================================================================
// Extended Environment Methods
// ============================================================================

impl ClipsEnvironment {
    /// Find a defclass by name
    pub fn find_class(&self, name: &str) -> Result<Option<DefclassHandle>> {
        let c_name = CString::new(name)?;

        let inner = unsafe { self.raw() };
        let class = unsafe { ffi::FindDefclass(inner, c_name.as_ptr()) };

        if class.is_null() {
            Ok(None)
        } else {
            Ok(Some(DefclassHandle {
                class,
                env: self.clone(),
            }))
        }
    }

    /// Find an instance by name
    pub fn find_instance(&self, name: &str) -> Result<Option<InstanceHandle>> {
        let c_name = CString::new(name)?;

        let inner = unsafe { self.raw() };
        let instance = unsafe { ffi::FindInstance(inner, ptr::null_mut(), c_name.as_ptr(), true) };

        if instance.is_null() {
            Ok(None)
        } else {
            Ok(Some(InstanceHandle {
                instance,
                env: self.clone(),
            }))
        }
    }

    /// Create an instance from a string
    pub fn make_instance(&self, instance_str: &str) -> Result<InstanceHandle> {
        let c_str = CString::new(instance_str)?;

        let inner = unsafe { self.raw() };
        let instance = unsafe { ffi::MakeInstance(inner, c_str.as_ptr()) };

        if instance.is_null() {
            Err(ClipsError::InstanceCreationFailed {
                message: format!("Failed to create instance from: {}", instance_str),
            })
        } else {
            Ok(InstanceHandle {
                instance,
                env: self.clone(),
            })
        }
    }

    /// Find a defgeneric by name
    pub fn find_generic(&self, name: &str) -> Result<Option<DefgenericHandle>> {
        let c_name = CString::new(name)?;

        let inner = unsafe { self.raw() };
        let generic = unsafe { ffi::FindDefgeneric(inner, c_name.as_ptr()) };

        if generic.is_null() {
            Ok(None)
        } else {
            Ok(Some(DefgenericHandle {
                generic,
                env: self.clone(),
            }))
        }
    }

    /// Find a defgeneric by name (alias for `find_generic`)
    pub fn find_defgeneric(&self, name: &str) -> Result<Option<DefgenericHandle>> {
        self.find_generic(name)
    }

    /// Find a defclass by name (alias for `find_class`)
    pub fn find_defclass(&self, name: &str) -> Result<Option<DefclassHandle>> {
        self.find_class(name)
    }

    /// Find a deffunction by name
    pub fn find_deffunction(&self, name: &str) -> Result<Option<DeffunctionHandle>> {
        let c_name = CString::new(name)?;

        let inner = unsafe { self.raw() };
        let func = unsafe { ffi::FindDeffunction(inner, c_name.as_ptr()) };

        if func.is_null() {
            Ok(None)
        } else {
            Ok(Some(DeffunctionHandle {
                func,
                env: self.clone(),
            }))
        }
    }

    /// Get an iterator over all defclasses
    pub fn classes(&self) -> DefclassIterator {
        DefclassIterator {
            env: self.clone(),
            current: ptr::null_mut(),
            started: false,
        }
    }

    /// Get an iterator over all defclasses (alias for `classes`)
    pub fn defclasses(&self) -> DefclassIterator {
        self.classes()
    }

    /// Get an iterator over all instances
    pub fn all_instances(&self) -> AllInstanceIterator {
        AllInstanceIterator {
            env: self.clone(),
            current: ptr::null_mut(),
            started: false,
        }
    }

    /// Get an iterator over all defgenerics
    pub fn generics(&self) -> DefgenericIterator {
        DefgenericIterator {
            env: self.clone(),
            current: ptr::null_mut(),
            started: false,
        }
    }

    /// Get an iterator over all defgenerics (alias for `generics`)
    pub fn defgenerics(&self) -> DefgenericIterator {
        self.generics()
    }

    /// Get an iterator over all deffunctions
    pub fn deffunctions(&self) -> DeffunctionIterator {
        DeffunctionIterator {
            env: self.clone(),
            current: ptr::null_mut(),
            started: false,
        }
    }

    /// Call a CLIPS function
    pub fn function_call(&self, name: &str, args: Option<&str>) -> Result<ClipsValue> {
        let c_name = CString::new(name)?;
        let c_args = args.map(CString::new).transpose()?;
        let args_ptr = c_args.as_ref().map(|s| s.as_ptr()).unwrap_or(ptr::null());

        let mut result = ffi::CLIPSValue::new();

        let env = unsafe { self.raw() };
        let success = unsafe { ffi::FunctionCall(env, c_name.as_ptr(), args_ptr, &mut result) };

        if success {
            unsafe { ClipsValue::from_ffi(&mut result, env) }
        } else {
            Err(ClipsError::EvaluationFailed {
                expression: format!(
                    "({}{})",
                    name,
                    args.map(|a| format!(" {}", a)).unwrap_or_default()
                ),
            })
        }
    }

    /// Enable dribble output to a file
    pub fn dribble_on(&self, filename: &str) -> Result<()> {
        let c_filename = CString::new(filename)?;

        let env = unsafe { self.raw() };
        let success = unsafe { ffi::DribbleOn(env, c_filename.as_ptr()) };

        if success {
            Ok(())
        } else {
            Err(ClipsError::RouterFailed {
                message: format!("Failed to enable dribble to {}", filename),
            })
        }
    }

    /// Disable dribble output
    pub fn dribble_off(&self) -> Result<()> {
        let env = unsafe { self.raw() };
        let success = unsafe { ffi::DribbleOff(env) };

        if success {
            Ok(())
        } else {
            Err(ClipsError::RouterFailed {
                message: "Failed to disable dribble".to_string(),
            })
        }
    }

    /// Load binary constructs from a file
    pub fn bload(&self, filename: &str) -> Result<()> {
        let c_filename = CString::new(filename)?;

        let env = unsafe { self.raw() };
        let success = unsafe { ffi::Bload(env, c_filename.as_ptr()) };

        if success {
            Ok(())
        } else {
            Err(ClipsError::BinaryError {
                operation: "load".to_string(),
                file: filename.to_string(),
            })
        }
    }

    /// Save binary constructs to a file
    pub fn bsave(&self, filename: &str) -> Result<()> {
        let c_filename = CString::new(filename)?;

        let env = unsafe { self.raw() };
        let success = unsafe { ffi::Bsave(env, c_filename.as_ptr()) };

        if success {
            Ok(())
        } else {
            Err(ClipsError::BinaryError {
                operation: "save".to_string(),
                file: filename.to_string(),
            })
        }
    }

    /// Execute a batch file
    pub fn batch(&self, filename: &str) -> Result<()> {
        let c_filename = CString::new(filename)?;

        let env = unsafe { self.raw() };
        let success = unsafe { ffi::BatchStar(env, c_filename.as_ptr()) };

        if success {
            Ok(())
        } else {
            Err(ClipsError::LoadFailed {
                file: filename.to_string(),
                message: "Batch file execution failed".to_string(),
            })
        }
    }
}

// ============================================================================
// Additional Iterators
// ============================================================================

/// Iterator over defclasses
pub struct DefclassIterator {
    env: ClipsEnvironment,
    current: *mut ffi::Defclass,
    started: bool,
}

impl Iterator for DefclassIterator {
    type Item = Result<DefclassHandle>;

    fn next(&mut self) -> Option<Self::Item> {
        let inner = unsafe { self.env.raw() };
        let next = if self.started {
            unsafe { ffi::GetNextDefclass(inner, self.current) }
        } else {
            self.started = true;
            unsafe { ffi::GetNextDefclass(inner, ptr::null_mut()) }
        };

        if next.is_null() {
            None
        } else {
            self.current = next;
            Some(Ok(DefclassHandle {
                class: next,
                env: self.env.clone(),
            }))
        }
    }
}

/// Iterator over all instances
pub struct AllInstanceIterator {
    env: ClipsEnvironment,
    current: *mut ffi::Instance,
    started: bool,
}

impl Iterator for AllInstanceIterator {
    type Item = Result<InstanceHandle>;

    fn next(&mut self) -> Option<Self::Item> {
        let inner = unsafe { self.env.raw() };
        let next = if self.started {
            unsafe { ffi::GetNextInstance(inner, self.current) }
        } else {
            self.started = true;
            unsafe { ffi::GetNextInstance(inner, ptr::null_mut()) }
        };

        if next.is_null() {
            None
        } else {
            self.current = next;
            Some(Ok(InstanceHandle {
                instance: next,
                env: self.env.clone(),
            }))
        }
    }
}

/// Iterator over defgenerics
pub struct DefgenericIterator {
    env: ClipsEnvironment,
    current: *mut ffi::Defgeneric,
    started: bool,
}

impl Iterator for DefgenericIterator {
    type Item = Result<DefgenericHandle>;

    fn next(&mut self) -> Option<Self::Item> {
        let inner = unsafe { self.env.raw() };
        let next = if self.started {
            unsafe { ffi::GetNextDefgeneric(inner, self.current) }
        } else {
            self.started = true;
            unsafe { ffi::GetNextDefgeneric(inner, ptr::null_mut()) }
        };

        if next.is_null() {
            None
        } else {
            self.current = next;
            Some(Ok(DefgenericHandle {
                generic: next,
                env: self.env.clone(),
            }))
        }
    }
}

/// Iterator over deffunctions
pub struct DeffunctionIterator {
    env: ClipsEnvironment,
    current: *mut ffi::Deffunction,
    started: bool,
}

impl Iterator for DeffunctionIterator {
    type Item = Result<DeffunctionHandle>;

    fn next(&mut self) -> Option<Self::Item> {
        let inner = unsafe { self.env.raw() };
        let next = if self.started {
            unsafe { ffi::GetNextDeffunction(inner, self.current) }
        } else {
            self.started = true;
            unsafe { ffi::GetNextDeffunction(inner, ptr::null_mut()) }
        };

        if next.is_null() {
            None
        } else {
            self.current = next;
            Some(Ok(DeffunctionHandle {
                func: next,
                env: self.env.clone(),
            }))
        }
    }
}

// ============================================================================
// Pattern Matching Utilities
// ============================================================================

/// Pattern matching options
#[derive(Debug, Clone, Default)]
pub struct PatternOptions {
    /// Use salience for conflict resolution
    pub use_salience: bool,

    /// Salience evaluation mode
    pub salience_mode: SalienceMode,
}

/// Salience evaluation modes
#[derive(Debug, Clone, Copy, Default)]
pub enum SalienceMode {
    /// Evaluate when rule is defined
    #[default]
    WhenDefined,
    /// Evaluate when rule is activated
    WhenActivated,
    /// Evaluate every cycle
    EveryCycle,
}

impl SalienceMode {
    fn to_ffi(self) -> i32 {
        match self {
            SalienceMode::WhenDefined => ffi::WHEN_DEFINED,
            SalienceMode::WhenActivated => ffi::WHEN_ACTIVATED,
            SalienceMode::EveryCycle => ffi::EVERY_CYCLE,
        }
    }
}

impl ClipsEnvironment {
    /// Get the salience evaluation mode
    pub fn get_salience_mode(&self) -> SalienceMode {
        let env = unsafe { self.raw() };
        let mode = unsafe { ffi::GetSalienceEvaluation(env) };
        match mode {
            ffi::WHEN_DEFINED => SalienceMode::WhenDefined,
            ffi::WHEN_ACTIVATED => SalienceMode::WhenActivated,
            ffi::EVERY_CYCLE => SalienceMode::EveryCycle,
            _ => SalienceMode::WhenDefined,
        }
    }

    /// Set the salience evaluation mode
    pub fn set_salience_mode(&self, mode: SalienceMode) {
        let env = unsafe { self.raw() };
        unsafe { ffi::SetSalienceEvaluation(env, mode.to_ffi()) };
    }

    /// Refresh a rule's activations
    pub fn refresh_rule(&self, rule_name: &str) -> Result<()> {
        let c_name = CString::new(rule_name)?;

        let env = unsafe { self.raw() };
        let rule = unsafe { ffi::FindDefrule(env, c_name.as_ptr()) };

        if rule.is_null() {
            return Err(ClipsError::RuleNotFound {
                name: rule_name.to_string(),
            });
        }

        unsafe { ffi::Refresh(env, rule) };
        Ok(())
    }

    /// Reorder the agenda for all modules
    pub fn reorder_agenda(&self) {
        let env = unsafe { self.raw() };
        unsafe { ffi::ReorderAgenda(env, ptr::null_mut()) };
    }

    /// Refresh the agenda for all modules
    pub fn refresh_agenda(&self) {
        let env = unsafe { self.raw() };
        unsafe { ffi::RefreshAgenda(env, ptr::null_mut()) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_salience_mode_conversion() {
        assert_eq!(SalienceMode::WhenDefined.to_ffi(), ffi::WHEN_DEFINED);
        assert_eq!(SalienceMode::WhenActivated.to_ffi(), ffi::WHEN_ACTIVATED);
        assert_eq!(SalienceMode::EveryCycle.to_ffi(), ffi::EVERY_CYCLE);
    }
}
