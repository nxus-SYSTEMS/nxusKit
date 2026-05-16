//! JSON to CLIPS conversion utilities
//!
//! This module provides functions to convert JSON structures to CLIPS syntax
//! and vice versa.

use super::types::*;
use std::collections::HashMap;

/// Converter for JSON to CLIPS transformations
#[derive(Debug)]
pub struct JsonToClipsConverter;

impl JsonToClipsConverter {
    /// Generate a CLIPS deftemplate from a TemplateDefinition
    ///
    /// # Example
    ///
    /// ```
    /// use nxuskit_engine::providers::clips::{TemplateDefinition, SlotDefinition, SlotType, JsonValue};
    /// use nxuskit_engine::providers::clips::converter::JsonToClipsConverter;
    ///
    /// let template = TemplateDefinition {
    ///     name: "person".to_string(),
    ///     doc: Some("A person record".to_string()),
    ///     slots: vec![
    ///         SlotDefinition {
    ///             name: "name".to_string(),
    ///             slot_type: SlotType::String,
    ///             ..Default::default()
    ///         },
    ///         SlotDefinition {
    ///             name: "age".to_string(),
    ///             slot_type: SlotType::Integer,
    ///             default: Some(JsonValue::Integer(0)),
    ///             ..Default::default()
    ///         },
    ///     ],
    /// };
    ///
    /// let clips = JsonToClipsConverter::generate_deftemplate(&template);
    /// assert!(clips.contains("(deftemplate person"));
    /// assert!(clips.contains("(slot name"));
    /// ```
    pub fn generate_deftemplate(template: &TemplateDefinition) -> String {
        let mut lines = Vec::new();

        // Opening with optional documentation
        if let Some(ref doc) = template.doc {
            lines.push(format!(
                "(deftemplate {} \"{}\"",
                template.name,
                escape_string(doc)
            ));
        } else {
            lines.push(format!("(deftemplate {}", template.name));
        }

        // Generate slots
        for slot in &template.slots {
            let slot_str = Self::generate_slot(slot);
            lines.push(format!("  {}", slot_str));
        }

        lines.push(")".to_string());
        lines.join("\n")
    }

    /// Generate a single slot definition
    fn generate_slot(slot: &SlotDefinition) -> String {
        let keyword = if slot.multislot { "multislot" } else { "slot" };
        let mut parts = vec![format!("({} {}", keyword, slot.name)];

        // Type constraint
        if let Some(type_str) = slot.slot_type.to_clips_string() {
            parts.push(format!("(type {})", type_str));
        }

        // Default value
        if let Some(ref default) = slot.default {
            parts.push(format!("(default {})", Self::value_to_clips(default)));
        }

        // Allowed values
        if let Some(ref allowed) = slot.allowed_values {
            let values: Vec<String> = allowed.iter().map(Self::value_to_clips).collect();
            // Determine if we should use allowed-symbols or allowed-values
            let all_symbols = allowed.iter().all(|v| {
                matches!(v, JsonValue::Symbol(_) | JsonValue::Bool(_))
                    || matches!(v, JsonValue::String(s) if !s.contains(' '))
            });
            if all_symbols {
                parts.push(format!("(allowed-symbols {})", values.join(" ")));
            } else {
                parts.push(format!("(allowed-values {})", values.join(" ")));
            }
        }

        // Range constraint
        if let Some((min, max)) = slot.range {
            parts.push(format!("(range {} {})", min, max));
        }

        // Cardinality for multislots
        if slot.multislot
            && let Some((min, max)) = slot.cardinality
        {
            parts.push(format!("(cardinality {} {})", min, max));
        }

        parts.push(")".to_string());
        parts.join(" ")
    }

    /// Convert a FactAssertion to a CLIPS assert string
    ///
    /// # Example
    ///
    /// ```
    /// use nxuskit_engine::providers::clips::{FactAssertion, JsonValue};
    /// use nxuskit_engine::providers::clips::converter::JsonToClipsConverter;
    /// use std::collections::HashMap;
    ///
    /// let fact = FactAssertion {
    ///     template: "person".to_string(),
    ///     values: [
    ///         ("name".to_string(), JsonValue::String("Alice".to_string())),
    ///         ("age".to_string(), JsonValue::Integer(30)),
    ///     ].into_iter().collect(),
    ///     id: None,
    /// };
    ///
    /// let clips = JsonToClipsConverter::fact_to_assert_string(&fact);
    /// assert!(clips.contains("(person"));
    /// ```
    pub fn fact_to_assert_string(fact: &FactAssertion) -> String {
        let mut slot_strs = Vec::new();

        // Sort slots for consistent output
        let mut slots: Vec<_> = fact.values.iter().collect();
        slots.sort_by_key(|(k, _)| *k);

        for (slot_name, value) in slots {
            let clips_value = Self::value_to_clips(value);
            slot_strs.push(format!("({} {})", slot_name, clips_value));
        }

        format!("({} {})", fact.template, slot_strs.join(" "))
    }

    /// Convert a JsonValue to CLIPS syntax
    pub fn value_to_clips(value: &JsonValue) -> String {
        match value {
            JsonValue::Null => "nil".to_string(),
            JsonValue::Bool(b) => if *b { "TRUE" } else { "FALSE" }.to_string(),
            JsonValue::Integer(i) => i.to_string(),
            JsonValue::Float(f) => {
                // Ensure we always have decimal point
                if f.fract() == 0.0 {
                    format!("{:.1}", f)
                } else {
                    format!("{}", f)
                }
            }
            JsonValue::String(s) => format!("\"{}\"", escape_string(s)),
            JsonValue::Symbol(s) => s.symbol.clone(),
            JsonValue::Array(arr) => {
                // Multifield value
                let items: Vec<String> = arr.iter().map(Self::value_to_clips).collect();
                items.join(" ")
            }
            JsonValue::Object(obj) => {
                // Objects are serialized as a string representation
                let json_str = serde_json::to_string(obj).unwrap_or_default();
                format!("\"{}\"", escape_string(&json_str))
            }
        }
    }

    /// Convert a CLIPS value back to JsonValue
    ///
    /// This is used when reading facts from CLIPS environment.
    pub fn clips_value_to_json(value: &clips_sys::ClipsValue) -> JsonValue {
        use clips_sys::ClipsValue;

        match value {
            ClipsValue::Void => JsonValue::Null,
            ClipsValue::Integer(i) => JsonValue::Integer(*i),
            ClipsValue::Float(f) => JsonValue::Float(*f),
            ClipsValue::String(s) => JsonValue::String(s.clone()),
            ClipsValue::Symbol(s) => {
                // Check for boolean symbols
                match s.as_str() {
                    "TRUE" => JsonValue::Bool(true),
                    "FALSE" => JsonValue::Bool(false),
                    "nil" => JsonValue::Null,
                    _ => JsonValue::Symbol(SymbolValue { symbol: s.clone() }),
                }
            }
            ClipsValue::Boolean(b) => JsonValue::Bool(*b),
            ClipsValue::Multifield(items) => {
                JsonValue::Array(items.iter().map(Self::clips_value_to_json).collect())
            }
            ClipsValue::FactAddress(idx) => {
                // Represent as a special object
                let mut obj = HashMap::new();
                obj.insert("_fact_address".to_string(), JsonValue::Integer(*idx));
                JsonValue::Object(obj)
            }
            ClipsValue::InstanceAddress(name) => {
                let mut obj = HashMap::new();
                obj.insert("_instance".to_string(), JsonValue::String(name.clone()));
                JsonValue::Object(obj)
            }
            ClipsValue::ExternalAddress(addr) => {
                let mut obj = HashMap::new();
                obj.insert(
                    "_external_address".to_string(),
                    JsonValue::String(format!("0x{:x}", addr)),
                );
                JsonValue::Object(obj)
            }
        }
    }

    /// Convert slot values HashMap from CLIPS to JSON
    pub fn slot_values_to_json(
        values: &HashMap<String, clips_sys::ClipsValue>,
    ) -> HashMap<String, JsonValue> {
        values
            .iter()
            .map(|(k, v)| (k.clone(), Self::clips_value_to_json(v)))
            .collect()
    }

    // ========================================================================
    // Programmatic Rule & Module Generation (Feature 033)
    // ========================================================================

    /// Generate a CLIPS defmodule from a ModuleDefinition
    ///
    /// Creates a module definition with optional documentation and imports.
    ///
    /// # Example
    ///
    /// ```
    /// use nxuskit_engine::providers::clips::ModuleDefinition;
    /// use nxuskit_engine::providers::clips::converter::JsonToClipsConverter;
    ///
    /// let module = ModuleDefinition {
    ///     name: "SCREEN-SIZE".to_string(),
    ///     doc: Some("Screen classification module".to_string()),
    ///     imports: Some(vec!["MAIN".to_string()]),
    /// };
    ///
    /// let clips = JsonToClipsConverter::generate_defmodule(&module);
    /// assert!(clips.contains("(defmodule SCREEN-SIZE"));
    /// assert!(clips.contains("\"Screen classification module\""));
    /// assert!(clips.contains("(import MAIN deftemplate ?ALL)"));
    /// ```
    pub fn generate_defmodule(module: &ModuleDefinition) -> String {
        let mut lines = Vec::new();

        // Opening with optional documentation
        if let Some(ref doc) = module.doc {
            lines.push(format!(
                "(defmodule {} \"{}\"",
                module.name,
                escape_string(doc)
            ));
        } else {
            lines.push(format!("(defmodule {}", module.name));
        }

        // Generate imports
        if let Some(ref imports) = module.imports {
            for import_module in imports {
                lines.push(format!("  (import {} deftemplate ?ALL)", import_module));
            }
        }

        lines.push(")".to_string());
        lines.join("\n")
    }

    /// Generate a CLIPS defrule from a RuleDefinition
    ///
    /// Supports two modes:
    /// 1. Raw source string (full CLIPS expressiveness)
    /// 2. Structured JSON (conditions + actions)
    ///
    /// # Example
    ///
    /// ```
    /// use nxuskit_engine::providers::clips::{RuleDefinition, RuleCondition, RuleAction, FactAssertion, JsonValue};
    /// use nxuskit_engine::providers::clips::converter::JsonToClipsConverter;
    /// use std::collections::HashMap;
    ///
    /// // Source mode
    /// let rule = RuleDefinition {
    ///     name: "classify-mobile".to_string(),
    ///     module: Some("SCREEN-SIZE".to_string()),
    ///     source: Some("(defrule classify-mobile (screen-config (width ?w&:(< ?w 768))) => (assert (device-class (type mobile))))".to_string()),
    ///     conditions: None,
    ///     actions: None,
    ///     doc: Some("Classify screens under 768px as mobile".to_string()),
    ///     salience: Some(10),
    /// };
    ///
    /// let clips = JsonToClipsConverter::generate_defrule(&rule);
    /// assert!(clips.contains("(defrule"));
    /// assert!(clips.contains("(declare (salience 10))"));
    /// ```
    pub fn generate_defrule(rule: &RuleDefinition) -> String {
        let mut lines = Vec::new();

        // Determine rule name with optional module prefix
        let rule_name = if let Some(ref module) = rule.module {
            format!("{}::{}", module, rule.name)
        } else {
            rule.name.clone()
        };

        // Opening
        if let Some(ref doc) = rule.doc {
            lines.push(format!("(defrule {} \"{}\"", rule_name, escape_string(doc)));
        } else {
            lines.push(format!("(defrule {}", rule_name));
        }

        // Salience declaration if present
        if let Some(salience) = rule.salience {
            lines.push(format!("  (declare (salience {}))", salience));
        }

        // Handle source vs. structured modes
        if let Some(ref source) = rule.source {
            // Raw source mode: use source string directly
            // Source string should be in format: pattern1 pattern2 ... => action1 action2 ...
            // Example: "(sensor (value ?v&:(> ?v 100))) => (assert (alert (level high)))"
            if source.contains("=>") {
                // Source string already has conditions and actions
                if let Some(idx) = source.find("=>") {
                    let conditions_part = source[..idx].trim();
                    let actions_part = source[idx + 2..].trim();

                    // Use conditions and actions as-is - they should already be properly formatted
                    // If they're wrapped in parens, that's CLIPS syntax, not a wrapper
                    lines.push(format!("  {}", conditions_part));
                    lines.push("  =>".to_string());
                    lines.push(format!("  {}", actions_part));
                } else {
                    // Fallback: use source as-is
                    lines.push(format!("  {}", source));
                }
            } else {
                // Source doesn't have =>, use as-is
                lines.push(format!("  {}", source));
            }
        } else if let Some(ref conditions) = rule.conditions {
            // Structured mode: generate from conditions
            for condition in conditions {
                let cond_str = Self::generate_rule_condition(condition);
                lines.push(format!("  {}", cond_str));
            }

            if let Some(ref actions) = rule.actions {
                lines.push("  =>".to_string());
                for action in actions {
                    let action_str = Self::generate_rule_action(action);
                    lines.push(format!("  {}", action_str));
                }
            }
        }

        lines.push(")".to_string());
        lines.join("\n")
    }

    /// Generate a rule condition pattern from a RuleCondition
    fn generate_rule_condition(condition: &RuleCondition) -> String {
        let mut pattern = format!("({}", condition.template);

        // Add slot constraints
        if let Some(ref bindings) = condition.bindings {
            for (slot_name, var) in bindings {
                pattern.push(' ');
                pattern.push_str(&format!("({} {})", slot_name, var));

                // Add constraints for this variable if present
                if let Some(ref constraints) = condition.constraints {
                    for constraint in constraints {
                        if constraint.contains(var) {
                            pattern.push_str(&format!("&:{}", constraint));
                            break; // Only apply first matching constraint
                        }
                    }
                }
            }
        }

        pattern.push(')');
        pattern
    }

    /// Generate a rule action from a RuleAction
    fn generate_rule_action(action: &RuleAction) -> String {
        if let Some(ref fact) = action.assert {
            let fact_str = Self::fact_to_assert_string(fact);
            format!("(assert {})", fact_str)
        } else if let Some(ref pattern) = action.retract {
            format!("(retract {})", pattern)
        } else if let Some(ref modifications) = action.modify {
            // Generate modify statement
            let mut modify_parts = vec!["(modify".to_string()];
            for (slot, value) in modifications {
                modify_parts.push(format!("({}  {})", slot, value));
            }
            modify_parts.push(")".to_string());
            modify_parts.join(" ")
        } else {
            "".to_string()
        }
    }
}

/// Converter for CLIPS to JSON transformations
#[derive(Debug)]
pub struct ClipsToJsonConverter;

impl ClipsToJsonConverter {
    /// Parse a CLIPS fact string back to a FactOutput
    ///
    /// This is a basic parser for simple facts. Complex patterns may need
    /// the full CLIPS environment to parse correctly.
    pub fn parse_fact_string(fact_str: &str) -> Option<(String, HashMap<String, JsonValue>)> {
        let fact_str = fact_str.trim();

        // Basic validation
        if !fact_str.starts_with('(') || !fact_str.ends_with(')') {
            return None;
        }

        // Remove outer parentheses
        let inner = &fact_str[1..fact_str.len() - 1];

        // Split into template name and slots
        let mut parts = Vec::new();
        let mut current = String::new();
        let mut depth = 0;
        let mut in_string = false;
        let mut escape_next = false;

        for ch in inner.chars() {
            if escape_next {
                current.push(ch);
                escape_next = false;
                continue;
            }

            match ch {
                '\\' if in_string => {
                    escape_next = true;
                    current.push(ch);
                }
                '"' => {
                    in_string = !in_string;
                    current.push(ch);
                }
                '(' if !in_string => {
                    if depth == 0 && !current.trim().is_empty() {
                        parts.push(current.trim().to_string());
                        current = String::new();
                    }
                    depth += 1;
                    current.push(ch);
                }
                ')' if !in_string => {
                    depth -= 1;
                    current.push(ch);
                    if depth == 0 {
                        parts.push(current.trim().to_string());
                        current = String::new();
                    }
                }
                ' ' | '\t' | '\n' if !in_string && depth == 0 => {
                    if !current.trim().is_empty() {
                        parts.push(current.trim().to_string());
                        current = String::new();
                    }
                }
                _ => current.push(ch),
            }
        }

        if !current.trim().is_empty() {
            parts.push(current.trim().to_string());
        }

        if parts.is_empty() {
            return None;
        }

        let template_name = parts[0].clone();
        let mut values = HashMap::new();

        // Parse slot values
        for slot_str in &parts[1..] {
            if let Some((name, value)) = Self::parse_slot(slot_str) {
                values.insert(name, value);
            }
        }

        Some((template_name, values))
    }

    /// Parse a single slot string like "(name \"Alice\")"
    fn parse_slot(slot_str: &str) -> Option<(String, JsonValue)> {
        let slot_str = slot_str.trim();

        if !slot_str.starts_with('(') || !slot_str.ends_with(')') {
            return None;
        }

        let inner = &slot_str[1..slot_str.len() - 1].trim();

        // Find the slot name (first token)
        let first_space = inner.find(|c: char| c.is_whitespace())?;
        let name = inner[..first_space].to_string();
        let value_str = inner[first_space..].trim();

        let value = Self::parse_value(value_str);
        Some((name, value))
    }

    /// Parse a CLIPS value string to JsonValue
    pub fn parse_value(value_str: &str) -> JsonValue {
        let value_str = value_str.trim();

        // Check for string (quoted)
        if value_str.starts_with('"') && value_str.ends_with('"') {
            return JsonValue::String(unescape_string(&value_str[1..value_str.len() - 1]));
        }

        // Check for nil
        if value_str == "nil" {
            return JsonValue::Null;
        }

        // Check for boolean
        if value_str == "TRUE" {
            return JsonValue::Bool(true);
        }
        if value_str == "FALSE" {
            return JsonValue::Bool(false);
        }

        // Check for integer
        if let Ok(i) = value_str.parse::<i64>() {
            return JsonValue::Integer(i);
        }

        // Check for float
        if let Ok(f) = value_str.parse::<f64>() {
            return JsonValue::Float(f);
        }

        // Check for multifield (space-separated values)
        if value_str.contains(' ') && !value_str.starts_with('"') {
            let parts: Vec<JsonValue> = value_str
                .split_whitespace()
                .map(Self::parse_value)
                .collect();
            return JsonValue::Array(parts);
        }

        // Default to symbol
        JsonValue::Symbol(SymbolValue {
            symbol: value_str.to_string(),
        })
    }
}

/// Escape special characters in a string for CLIPS
fn escape_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Unescape a CLIPS string
fn unescape_string(s: &str) -> String {
    let mut result = String::new();
    let mut escape = false;

    for ch in s.chars() {
        if escape {
            result.push(ch);
            escape = false;
        } else if ch == '\\' {
            escape = true;
        } else {
            result.push(ch);
        }
    }

    result
}

/// Builder for creating CLIPS constructs programmatically
#[derive(Debug)]
pub struct ClipsCodeBuilder {
    constructs: Vec<String>,
}

impl ClipsCodeBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            constructs: Vec::new(),
        }
    }

    /// Add a comment
    pub fn comment(&mut self, text: &str) -> &mut Self {
        for line in text.lines() {
            self.constructs.push(format!("; {}", line));
        }
        self
    }

    /// Add a section header comment
    pub fn section(&mut self, title: &str) -> &mut Self {
        self.constructs.push(String::new());
        self.constructs.push(format!(";;; {}", title));
        self.constructs
            .push(format!(";;; {}", "=".repeat(title.len())));
        self.constructs.push(String::new());
        self
    }

    /// Add a deftemplate
    pub fn deftemplate(&mut self, template: &TemplateDefinition) -> &mut Self {
        self.constructs
            .push(JsonToClipsConverter::generate_deftemplate(template));
        self.constructs.push(String::new());
        self
    }

    /// Add a raw construct string
    pub fn raw(&mut self, code: &str) -> &mut Self {
        self.constructs.push(code.to_string());
        self
    }

    /// Add a defrule with documentation
    pub fn defrule(
        &mut self,
        name: &str,
        doc: Option<&str>,
        salience: Option<i32>,
        patterns: &[&str],
        actions: &[&str],
    ) -> &mut Self {
        let mut parts = vec![format!("(defrule {}", name)];

        if let Some(d) = doc {
            parts.push(format!("  \"{}\"", escape_string(d)));
        }

        if let Some(s) = salience {
            parts.push(format!("  (declare (salience {}))", s));
        }

        parts.push(String::new());

        for pattern in patterns {
            parts.push(format!("  {}", pattern));
        }

        parts.push(String::new());
        parts.push("  =>".to_string());
        parts.push(String::new());

        for action in actions {
            parts.push(format!("  {}", action));
        }

        parts.push(")".to_string());

        self.constructs.push(parts.join("\n"));
        self.constructs.push(String::new());
        self
    }

    /// Add a deffacts block
    pub fn deffacts(&mut self, name: &str, facts: &[&str]) -> &mut Self {
        let mut parts = vec![format!("(deffacts {}", name)];

        for fact in facts {
            parts.push(format!("  {}", fact));
        }

        parts.push(")".to_string());

        self.constructs.push(parts.join("\n"));
        self.constructs.push(String::new());
        self
    }

    /// Add a defglobal
    pub fn defglobal(&mut self, name: &str, value: &JsonValue) -> &mut Self {
        let clips_value = JsonToClipsConverter::value_to_clips(value);
        self.constructs
            .push(format!("(defglobal ?*{}* = {})", name, clips_value));
        self
    }

    /// Build the final CLIPS code
    pub fn build(&self) -> String {
        self.constructs.join("\n")
    }
}

impl Default for ClipsCodeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for SlotDefinition {
    fn default() -> Self {
        Self {
            name: String::new(),
            slot_type: SlotType::Any,
            multislot: false,
            default: None,
            allowed_values: None,
            range: None,
            cardinality: None,
        }
    }
}

#[cfg(test)]
#[allow(clippy::approx_constant)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_deftemplate() {
        let template = TemplateDefinition {
            name: "person".to_string(),
            doc: Some("A person record".to_string()),
            slots: vec![
                SlotDefinition {
                    name: "name".to_string(),
                    slot_type: SlotType::String,
                    ..Default::default()
                },
                SlotDefinition {
                    name: "age".to_string(),
                    slot_type: SlotType::Integer,
                    default: Some(JsonValue::Integer(0)),
                    ..Default::default()
                },
            ],
        };

        let clips = JsonToClipsConverter::generate_deftemplate(&template);
        assert!(clips.contains("(deftemplate person"));
        assert!(clips.contains("\"A person record\""));
        assert!(clips.contains("(slot name"));
        assert!(clips.contains("(type STRING)"));
        assert!(clips.contains("(default 0)"));
    }

    #[test]
    fn test_fact_to_assert_string() {
        let fact = FactAssertion {
            template: "person".to_string(),
            values: [
                ("name".to_string(), JsonValue::String("Alice".to_string())),
                ("age".to_string(), JsonValue::Integer(30)),
            ]
            .into_iter()
            .collect(),
            id: None,
        };

        let clips = JsonToClipsConverter::fact_to_assert_string(&fact);
        assert!(clips.starts_with("(person"));
        assert!(clips.contains("(name \"Alice\")"));
        assert!(clips.contains("(age 30)"));
    }

    #[test]
    fn test_value_conversions() {
        assert_eq!(
            JsonToClipsConverter::value_to_clips(&JsonValue::Integer(42)),
            "42"
        );
        assert_eq!(
            JsonToClipsConverter::value_to_clips(&JsonValue::Float(3.14)),
            "3.14"
        );
        assert_eq!(
            JsonToClipsConverter::value_to_clips(&JsonValue::String("hello".to_string())),
            "\"hello\""
        );
        assert_eq!(
            JsonToClipsConverter::value_to_clips(&JsonValue::Bool(true)),
            "TRUE"
        );
        assert_eq!(
            JsonToClipsConverter::value_to_clips(&JsonValue::Null),
            "nil"
        );
    }

    #[test]
    fn test_parse_value() {
        assert_eq!(
            ClipsToJsonConverter::parse_value("42"),
            JsonValue::Integer(42)
        );
        assert_eq!(
            ClipsToJsonConverter::parse_value("3.14"),
            JsonValue::Float(3.14)
        );
        assert_eq!(
            ClipsToJsonConverter::parse_value("\"hello\""),
            JsonValue::String("hello".to_string())
        );
        assert_eq!(
            ClipsToJsonConverter::parse_value("TRUE"),
            JsonValue::Bool(true)
        );
        assert_eq!(ClipsToJsonConverter::parse_value("nil"), JsonValue::Null);
    }

    #[test]
    fn test_code_builder() {
        let mut builder = ClipsCodeBuilder::new();
        builder
            .section("Templates")
            .deftemplate(&TemplateDefinition {
                name: "test".to_string(),
                doc: None,
                slots: vec![SlotDefinition {
                    name: "value".to_string(),
                    slot_type: SlotType::Integer,
                    ..Default::default()
                }],
            })
            .defrule(
                "test-rule",
                Some("A test rule"),
                Some(10),
                &["(test (value ?v&:(> ?v 0)))"],
                &["(printout t \"Value is positive\" crlf)"],
            );

        let code = builder.build();
        assert!(code.contains(";;; Templates"));
        assert!(code.contains("(deftemplate test"));
        assert!(code.contains("(defrule test-rule"));
        assert!(code.contains("(declare (salience 10))"));
    }

    // ========================================================================
    // Converter Extension Tests (Feature 033)
    // ========================================================================

    #[test]
    fn test_generate_defmodule_with_doc_and_imports() {
        let module = ModuleDefinition {
            name: "SCREEN-SIZE".to_string(),
            doc: Some("Screen classification module".to_string()),
            imports: Some(vec!["MAIN".to_string()]),
        };

        let clips = JsonToClipsConverter::generate_defmodule(&module);
        assert!(clips.contains("(defmodule SCREEN-SIZE"));
        assert!(clips.contains("\"Screen classification module\""));
        assert!(clips.contains("(import MAIN deftemplate ?ALL)"));
    }

    #[test]
    fn test_generate_defmodule_minimal() {
        let module = ModuleDefinition {
            name: "TEST".to_string(),
            doc: None,
            imports: None,
        };

        let clips = JsonToClipsConverter::generate_defmodule(&module);
        assert!(clips.contains("(defmodule TEST"));
        assert!(clips.contains(")"));
    }

    #[test]
    fn test_generate_defmodule_multiple_imports() {
        let module = ModuleDefinition {
            name: "RULES".to_string(),
            doc: None,
            imports: Some(vec!["MAIN".to_string(), "DATA".to_string()]),
        };

        let clips = JsonToClipsConverter::generate_defmodule(&module);
        assert!(clips.contains("(import MAIN deftemplate ?ALL)"));
        assert!(clips.contains("(import DATA deftemplate ?ALL)"));
    }

    #[test]
    fn test_generate_defrule_source_mode() {
        let rule = RuleDefinition {
            name: "classify-mobile".to_string(),
            module: Some("SCREEN-SIZE".to_string()),
            source: Some(
                "(screen-config (width ?w&:(< ?w 768))) => (assert (device-class (type mobile)))"
                    .to_string(),
            ),
            conditions: None,
            actions: None,
            doc: Some("Classify screens under 768px as mobile".to_string()),
            salience: Some(10),
        };

        let clips = JsonToClipsConverter::generate_defrule(&rule);
        assert!(clips.contains("(defrule SCREEN-SIZE::classify-mobile"));
        assert!(clips.contains("\"Classify screens under 768px as mobile\""));
        assert!(clips.contains("(declare (salience 10))"));
        assert!(clips.contains("=>"));
    }

    #[test]
    fn test_generate_defrule_source_no_module() {
        let rule = RuleDefinition {
            name: "simple-rule".to_string(),
            module: None,
            source: Some("(fact ?x) => (assert (result ?x))".to_string()),
            conditions: None,
            actions: None,
            doc: None,
            salience: None,
        };

        let clips = JsonToClipsConverter::generate_defrule(&rule);
        assert!(clips.contains("(defrule simple-rule"));
        assert!(!clips.contains("::"));
    }

    #[test]
    fn test_generate_defrule_structured_mode() {
        let rule = RuleDefinition {
            name: "test-rule".to_string(),
            module: None,
            source: None,
            conditions: Some(vec![RuleCondition {
                template: "screen-config".to_string(),
                bindings: Some(
                    [("width".to_string(), "?w".to_string())]
                        .into_iter()
                        .collect(),
                ),
                constraints: Some(vec!["(< ?w 768)".to_string()]),
            }]),
            actions: Some(vec![RuleAction {
                assert: Some(FactAssertion {
                    template: "device-class".to_string(),
                    values: [("type".to_string(), JsonValue::String("mobile".to_string()))]
                        .into_iter()
                        .collect(),
                    id: None,
                }),
                retract: None,
                modify: None,
            }]),
            doc: None,
            salience: Some(5),
        };

        let clips = JsonToClipsConverter::generate_defrule(&rule);
        assert!(clips.contains("(defrule test-rule"));
        assert!(clips.contains("(declare (salience 5))"));
        assert!(clips.contains("(screen-config"));
        assert!(clips.contains("(width ?w)"));
        assert!(clips.contains("=>"));
        assert!(clips.contains("(assert"));
        assert!(clips.contains("(device-class"));
    }

    #[test]
    fn test_generate_defrule_with_retract() {
        let rule = RuleDefinition {
            name: "cleanup-rule".to_string(),
            module: None,
            source: None,
            conditions: Some(vec![RuleCondition {
                template: "temp-fact".to_string(),
                bindings: None,
                constraints: None,
            }]),
            actions: Some(vec![RuleAction {
                assert: None,
                retract: Some("(temp-fact)".to_string()),
                modify: None,
            }]),
            doc: None,
            salience: None,
        };

        let clips = JsonToClipsConverter::generate_defrule(&rule);
        assert!(clips.contains("(defrule cleanup-rule"));
        assert!(clips.contains("(retract"));
    }

    #[test]
    fn test_generate_rule_condition_with_constraints() {
        let condition = RuleCondition {
            template: "sensor".to_string(),
            bindings: Some(
                [
                    ("temperature".to_string(), "?t".to_string()),
                    ("location".to_string(), "?l".to_string()),
                ]
                .into_iter()
                .collect(),
            ),
            constraints: Some(vec!["(> ?t 30)".to_string(), "(eq ?l office)".to_string()]),
        };

        let pattern = JsonToClipsConverter::generate_rule_condition(&condition);
        assert!(pattern.contains("(sensor"));
        assert!(pattern.contains("(temperature ?t)"));
        assert!(pattern.contains("(location ?l)"));
    }

    #[test]
    fn test_generate_rule_action_assert() {
        let action = RuleAction {
            assert: Some(FactAssertion {
                template: "result".to_string(),
                values: [("value".to_string(), JsonValue::Integer(42))]
                    .into_iter()
                    .collect(),
                id: None,
            }),
            retract: None,
            modify: None,
        };

        let action_str = JsonToClipsConverter::generate_rule_action(&action);
        assert!(action_str.contains("(assert"));
        assert!(action_str.contains("(result"));
        assert!(action_str.contains("42"));
    }

    #[test]
    fn test_generate_rule_action_retract() {
        let action = RuleAction {
            assert: None,
            retract: Some("(old-fact ?x)".to_string()),
            modify: None,
        };

        let action_str = JsonToClipsConverter::generate_rule_action(&action);
        assert!(action_str.contains("(retract"));
        assert!(action_str.contains("(old-fact ?x)"));
    }
}
