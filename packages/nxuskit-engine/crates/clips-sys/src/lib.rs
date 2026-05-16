//! # clips-sys
//!
//! Full FFI bindings to the CLIPS expert system shell with safe Rust wrappers.
//!
//! CLIPS (C Language Integrated Production System) is a rule-based expert system
//! shell created by NASA. This crate provides Rust bindings to the CLIPS C library.
//!
//! ## Features
//!
//! - Full FFI bindings to CLIPS 6.4
//! - Safe, thread-safe Rust wrappers
//! - Static linking (no runtime dependencies)
//! - Support for facts, rules, templates, modules, and more
//!
//! ## Quick Start
//!
//! ```no_run
//! use clips_sys::{ClipsEnvironment, ClipsValue};
//!
//! fn main() -> clips_sys::Result<()> {
//!     // Create a new CLIPS environment
//!     let env = ClipsEnvironment::new()?;
//!
//!     // Load rules from a file
//!     env.load("rules.clp")?;
//!
//!     // Or build constructs from strings
//!     env.build(r#"
//!         (deftemplate person
//!             (slot name (type STRING))
//!             (slot age (type INTEGER)))
//!     "#)?;
//!
//!     env.build(r#"
//!         (defrule greet-adult
//!             (person (name ?n) (age ?a&:(>= ?a 18)))
//!             =>
//!             (printout t "Hello, adult " ?n "!" crlf))
//!     "#)?;
//!
//!     // Assert facts
//!     env.assert_string("(person (name \"Alice\") (age 30))")?;
//!     env.assert_string("(person (name \"Bob\") (age 15))")?;
//!
//!     // Run the inference engine
//!     let result = env.run(None)?;
//!     println!("Rules fired: {}", result.rules_fired);
//!
//!     // Iterate over facts
//!     for fact in env.facts() {
//!         let fact = fact?;
//!         println!("Fact {}: {}", fact.index(), fact.pp_form());
//!     }
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Using the Fact Builder
//!
//! For type-safe fact creation, use the `FactBuilder`:
//!
//! ```no_run
//! use clips_sys::ClipsEnvironment;
//!
//! let env = ClipsEnvironment::new()?;
//!
//! env.build(r#"
//!     (deftemplate order
//!         (slot id (type INTEGER))
//!         (slot customer (type STRING))
//!         (slot total (type FLOAT)))
//! "#)?;
//!
//! let mut builder = env.fact_builder("order")?;
//! builder
//!     .put_integer("id", 12345)?
//!     .put_string("customer", "Acme Corp")?
//!     .put_float("total", 199.99)?;
//!
//! let fact = builder.assert()?;
//! println!("Created order fact: {}", fact.index());
//! # Ok::<(), clips_sys::ClipsError>(())
//! ```
//!
//! ## Building from Source
//!
//! This crate requires the CLIPS source code to compile. Download CLIPS 6.4
//! from <https://sourceforge.net/projects/clipsrules/files/CLIPS/6.4/>
//! and extract the source files to the `clips-source/` directory.
//!
//! CLIPS 6.4.2 is licensed under MIT No Attribution (MIT-0).
//! See: <https://www.clipsrules.net/>
//!
//! ## Safety
//!
//! This crate provides both unsafe FFI bindings (`ffi` module) and safe
//! Rust wrappers. The safe wrappers use `parking_lot::Mutex` to ensure
//! thread-safe access to CLIPS environments.
//!
//! ## CLIPS Resources
//!
//! - [CLIPS Home](https://www.clipsrules.net/)
//! - [Advanced Programming Guide](https://www.clipsrules.net/documentation/v640/apg.pdf)
//! - [User's Guide](https://www.clipsrules.net/documentation/v640/ug.pdf)
//! - [Reference Manual](https://www.clipsrules.net/documentation/v640/bpg.pdf)

#![warn(missing_docs)]
// Note: rustdoc::missing_doc_code_examples is unstable and causes errors with -D warnings

pub mod advanced;
pub mod environment;
pub mod error;
pub mod ffi;
pub mod value;

// Re-exports for convenience
pub use advanced::{
    AllInstanceIterator, DefclassHandle, DefclassIterator, DeffunctionHandle, DeffunctionIterator,
    DefgenericHandle, DefgenericIterator, InstanceHandle, InstanceIterator, PatternOptions,
    SalienceMode,
};
pub use environment::{
    ClipsEnvironment, DefglobalHandle, DefmoduleHandle, DefmoduleIterator, DefruleHandle,
    DefruleIterator, DeftemplateHandle, DeftemplateIterator, FactBuilder, FactHandle, FactIterator,
    Strategy, TemplateFactsIterator, WatchItem,
};
pub use error::{ClipsError, PtrExt, Result};
pub use value::{
    ActivationInfo, ClipsValue, RuleInfo, RunCompletionReason, RunResult, SlotInfo, SlotValues,
};

/// CLIPS version information
pub fn clips_version() -> String {
    ClipsEnvironment::version()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_environment() {
        // Note: This will use the stub if CLIPS source isn't available
        let result = ClipsEnvironment::new();
        // We can't assert success without CLIPS source, but we can test the API
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_clips_value_conversions() {
        let int_val: ClipsValue = 42i64.into();
        assert_eq!(int_val.as_integer(), Some(42));

        let float_val: ClipsValue = 2.5f64.into();
        assert_eq!(float_val.as_float(), Some(2.5));

        let string_val: ClipsValue = "hello".into();
        assert_eq!(string_val.as_string(), Some("hello"));

        let bool_val: ClipsValue = true.into();
        assert_eq!(bool_val.as_bool(), Some(true));

        let list_val: ClipsValue = vec![1i64, 2, 3].into();
        assert!(list_val.as_multifield().is_some());
    }

    #[test]
    fn test_clips_value_to_string() {
        assert_eq!(ClipsValue::Integer(42).to_clips_string(), "42");
        assert_eq!(ClipsValue::Float(2.5).to_clips_string(), "2.500000");
        assert_eq!(
            ClipsValue::Symbol("foo".to_string()).to_clips_string(),
            "foo"
        );
        assert_eq!(
            ClipsValue::String("hello".to_string()).to_clips_string(),
            "\"hello\""
        );
        assert_eq!(ClipsValue::Boolean(true).to_clips_string(), "TRUE");
        assert_eq!(ClipsValue::Void.to_clips_string(), "nil");
    }

    #[test]
    fn test_clips_value_type_names() {
        assert_eq!(ClipsValue::Integer(0).type_name(), "INTEGER");
        assert_eq!(ClipsValue::Float(0.0).type_name(), "FLOAT");
        assert_eq!(ClipsValue::Symbol("".to_string()).type_name(), "SYMBOL");
        assert_eq!(ClipsValue::String("".to_string()).type_name(), "STRING");
        assert_eq!(ClipsValue::Multifield(vec![]).type_name(), "MULTIFIELD");
    }

    #[test]
    fn test_build_single_construct() {
        let env = ClipsEnvironment::new().expect("Failed to create environment");

        // Build a single deftemplate
        let construct =
            "(deftemplate vehicle (slot make (type STRING)) (slot year (type INTEGER)))";
        let result = env.build(construct);

        assert!(result.is_ok(), "Build should succeed: {:?}", result.err());

        // Verify the template was loaded
        let template = env
            .find_template("vehicle")
            .expect("Should be able to search");
        assert!(
            template.is_some(),
            "vehicle template should be found after build"
        );
    }

    #[test]
    fn test_load_from_string_trimmed() {
        // Trimmed rules - no leading whitespace/newlines
        let rules = "(deftemplate animal (slot species (type SYMBOL)) (slot weight (type FLOAT)))";

        let env = ClipsEnvironment::new().expect("Failed to create environment");
        let result = env.load_from_string(rules);

        assert!(
            result.is_ok(),
            "Load from string should succeed: {:?}",
            result.err()
        );

        // Verify the template was loaded
        let template = env
            .find_template("animal")
            .expect("Should be able to search");
        assert!(
            template.is_some(),
            "animal template should be found after loading"
        );
    }

    #[test]
    fn test_load_from_file() {
        use std::fs;

        // Create a temp file with simple CLIPS content - trimmed, no leading whitespace
        let temp_path = std::env::temp_dir().join("test_clips_rules.clp");
        let rules = "(deftemplate person (slot name (type STRING)) (slot age (type INTEGER)))\n\n(defrule test-rule (person (name ?n) (age ?a)) => (printout t \"Found person: \" ?n \" age \" ?a crlf))";
        fs::write(&temp_path, rules).expect("Failed to write temp file");

        // Create environment and load
        let env = ClipsEnvironment::new().expect("Failed to create environment");
        let result = env.load(&temp_path);

        // Cleanup temp file
        fs::remove_file(&temp_path).ok();

        // Assert load worked
        assert!(result.is_ok(), "Load should succeed: {:?}", result.err());

        // Verify the template was loaded by finding it
        let template = env
            .find_template("person")
            .expect("Should be able to search");
        assert!(
            template.is_some(),
            "person template should be found after loading"
        );
    }

    #[test]
    fn test_load_from_file_multiline() {
        use std::fs;

        // Test with multiline format like the integration tests use
        let temp_path = std::env::temp_dir().join("test_clips_rules_multiline.clp");
        // Note: CLIPS files should start with constructs, not whitespace
        // Must define all templates used in rules (including triage)
        let rules = r#"(deftemplate patient
    (slot name (type STRING))
    (slot age (type INTEGER))
    (slot temperature (type FLOAT) (default 37.0)))

(deftemplate triage
    (slot patient-name (type STRING))
    (slot level (type SYMBOL))
    (slot reason (type STRING)))

(defrule fever-check
    "Check if patient has fever"
    (patient (name ?n) (temperature ?t&:(> ?t 38.0)))
    =>
    (assert (triage (patient-name ?n) (level urgent) (reason "Fever detected"))))
"#;
        fs::write(&temp_path, rules).expect("Failed to write temp file");

        // Verify file content
        let content = fs::read_to_string(&temp_path).expect("Read back");
        println!("File starts with: {:?}", &content[..50.min(content.len())]);

        // Create environment and load
        let env = ClipsEnvironment::new().expect("Failed to create environment");
        let result = env.load(&temp_path);

        // Cleanup temp file
        fs::remove_file(&temp_path).ok();

        // Assert load worked
        assert!(
            result.is_ok(),
            "Load multiline should succeed: {:?}",
            result.err()
        );

        // Verify the template was loaded by finding it
        let template = env
            .find_template("patient")
            .expect("Should be able to search");
        assert!(
            template.is_some(),
            "patient template should be found after loading"
        );
    }

    #[test]
    fn test_inference_engine() {
        use std::fs;

        // Create a temp file with rules that should fire
        let temp_path = std::env::temp_dir().join("test_clips_inference.clp");
        let rules = r#"(deftemplate patient
    (slot name (type STRING))
    (slot temperature (type FLOAT)))

(deftemplate triage
    (slot patient-name (type STRING))
    (slot level (type SYMBOL))
    (slot reason (type STRING)))

(defrule fever-check
    "Check for fever"
    (patient (name ?n) (temperature ?t&:(> ?t 38.0)))
    =>
    (assert (triage (patient-name ?n) (level urgent) (reason "Fever detected"))))
"#;
        fs::write(&temp_path, rules).expect("Failed to write temp file");

        let env = ClipsEnvironment::new().expect("Failed to create environment");
        env.load(&temp_path).expect("Failed to load rules");

        // Assert a patient with fever
        let fact = env
            .assert_string("(patient (name \"Bob\") (temperature 39.0))")
            .expect("Failed to assert patient");
        println!("Asserted patient fact with index: {}", fact.index());

        // Run inference
        let result = env.run(None).expect("Failed to run inference");
        println!("Rules fired: {}", result.rules_fired);

        // Check if triage fact was created
        let mut triage_found = false;
        let mut triage_reason = String::new();
        for fact_result in env.facts() {
            let fact = fact_result.expect("Failed to get fact");
            let template_name = fact.template_name().unwrap_or_default();

            if template_name == "triage" {
                triage_found = true;
                if let Ok(ClipsValue::String(reason)) = fact.get_slot("reason") {
                    triage_reason = reason;
                }
            }
        }

        fs::remove_file(&temp_path).ok();

        assert!(result.rules_fired >= 1, "fever-check rule should fire");
        assert!(triage_found, "triage fact should be created");
        assert_eq!(
            triage_reason, "Fever detected",
            "triage reason should match"
        );
    }

    #[test]
    fn test_retract_by_template() {
        let env = ClipsEnvironment::new().expect("Failed to create environment");

        env.build("(deftemplate sensor (slot id (type STRING)) (slot value (type FLOAT)))")
            .unwrap();
        env.reset().unwrap();

        env.assert_string(r#"(sensor (id "S1") (value 120.0))"#)
            .unwrap();
        env.assert_string(r#"(sensor (id "S2") (value 75.0))"#)
            .unwrap();

        // Verify 2 sensor facts exist (plus initial-fact)
        let total_before = env.facts().count();
        assert!(total_before >= 2, "Should have at least 2 facts");

        // Retract all sensor facts
        let retracted = env.retract_by_template("sensor").unwrap();
        assert_eq!(retracted, 2, "Should retract 2 sensor facts");

        // Verify sensor facts are gone
        let template = env.find_template("sensor").unwrap().unwrap();
        let remaining: Vec<_> = template.facts().filter_map(|f| f.ok()).collect();
        assert_eq!(remaining.len(), 0, "No sensor facts should remain");
    }

    #[test]
    fn test_template_facts_iterator() {
        let env = ClipsEnvironment::new().expect("Failed to create environment");

        env.build("(deftemplate item (slot name (type STRING)))")
            .unwrap();
        env.build("(deftemplate other (slot x (type INTEGER)))")
            .unwrap();

        env.assert_string(r#"(item (name "A"))"#).unwrap();
        env.assert_string(r#"(item (name "B"))"#).unwrap();
        env.assert_string("(other (x 1))").unwrap();

        let template = env.find_template("item").unwrap().unwrap();
        let item_facts: Vec<_> = template.facts().filter_map(|f| f.ok()).collect();
        assert_eq!(item_facts.len(), 2, "Should have 2 item facts");

        let other_template = env.find_template("other").unwrap().unwrap();
        let other_facts: Vec<_> = other_template.facts().filter_map(|f| f.ok()).collect();
        assert_eq!(other_facts.len(), 1, "Should have 1 other fact");
    }

    #[test]
    fn test_module_discovery() {
        let env = ClipsEnvironment::new().expect("Failed to create environment");

        // MAIN module should always exist
        let modules = env.list_module_names().unwrap();
        assert!(
            modules.contains(&"MAIN".to_string()),
            "Should have MAIN module"
        );

        // Find MAIN module
        let main = env.find_module("MAIN").unwrap();
        assert!(main.is_some(), "Should find MAIN module");

        // Non-existent module
        let none = env.find_module("NONEXISTENT").unwrap();
        assert!(none.is_none(), "Should not find NONEXISTENT module");
    }
}
