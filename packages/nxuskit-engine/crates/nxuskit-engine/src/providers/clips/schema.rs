//! CLIPS Schema Conversion Module
//!
//! Provides bidirectional conversion between CLIPS deftemplates and JSON Schema (2020-12).
//!
//! # Overview
//!
//! This module enables:
//! - Converting CLIPS deftemplates to JSON Schema for documentation and validation
//! - Converting JSON Schema to CLIPS deftemplates for code generation
//! - Extracting schema information from loaded CLIPS environments
//!
//! # JSON Schema 2020-12 Mapping
//!
//! | CLIPS                     | JSON Schema                          |
//! |---------------------------|--------------------------------------|
//! | `(type STRING)`           | `{"type": "string"}`                 |
//! | `(type INTEGER)`          | `{"type": "integer"}`                |
//! | `(type FLOAT)`            | `{"type": "number"}`                 |
//! | `(type SYMBOL)`           | `{"type": "string"}`                 |
//! | `(allowed-symbols a b c)` | `{"enum": ["a", "b", "c"]}`          |
//! | `(default X)`             | `{"default": X}`                     |
//! | `(range 0 100)`           | `{"minimum": 0, "maximum": 100}`     |
//! | `(cardinality 1 5)`       | `{"minItems": 1, "maxItems": 5}`     |
//! | multislot                 | `{"type": "array", "items": {...}}`  |
//!
//! # Example
//!
//! ```no_run
//! use nxuskit_engine::providers::clips::schema::{TemplateSchema, SlotSchema, SlotType};
//!
//! let template = TemplateSchema {
//!     name: "patient".to_string(),
//!     slots: vec![
//!         SlotSchema {
//!             name: "name".to_string(),
//!             slot_type: SlotType::String,
//!             is_multi: false,
//!             default: None,
//!             allowed_values: None,
//!             cardinality: None,
//!             range: None,
//!         },
//!         SlotSchema {
//!             name: "age".to_string(),
//!             slot_type: SlotType::Integer,
//!             is_multi: false,
//!             default: Some(serde_json::json!(0)),
//!             allowed_values: None,
//!             cardinality: None,
//!             range: Some((0.0, 150.0)),
//!         },
//!     ],
//!     documentation: Some("Patient information template".to_string()),
//! };
//!
//! let json_schema = nxuskit_engine::providers::clips::schema::deftemplate_to_json_schema(&template);
//! println!("{}", serde_json::to_string_pretty(&json_schema).unwrap());
//! ```

use serde::{Deserialize, Serialize};
use serde_json::{Value as JsonValue, json};

// Re-use SlotType from types module
pub use super::types::SlotType;

use clips_sys::ClipsEnvironment;

// ============================================================================
// Schema Types
// ============================================================================

/// A collection of template schemas extracted from a CLIPS environment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipsSchema {
    /// All templates in the environment
    pub templates: Vec<TemplateSchema>,
}

/// Schema representation of a CLIPS deftemplate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateSchema {
    /// Template name
    pub name: String,
    /// Slot definitions
    pub slots: Vec<SlotSchema>,
    /// Optional documentation string
    pub documentation: Option<String>,
}

/// Schema representation of a CLIPS slot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlotSchema {
    /// Slot name
    pub name: String,
    /// Slot type
    pub slot_type: SlotType,
    /// Whether this is a multislot
    pub is_multi: bool,
    /// Default value (if any)
    pub default: Option<JsonValue>,
    /// Allowed values (for constrained slots)
    pub allowed_values: Option<Vec<JsonValue>>,
    /// Cardinality constraint (min, max) for multislots
    pub cardinality: Option<(u32, u32)>,
    /// Range constraint (min, max) for numeric slots
    pub range: Option<(f64, f64)>,
}

/// Extension trait for SlotType to add schema conversion methods
pub trait SlotTypeSchemaExt {
    /// Convert from CLIPS type name string
    fn from_clips_type_name(type_name: &str) -> SlotType;
    /// Convert to JSON Schema type string
    fn to_json_schema_type(&self) -> &'static str;
    /// Convert to CLIPS type string
    fn to_clips_type_str(&self) -> &'static str;
}

impl SlotTypeSchemaExt for SlotType {
    fn from_clips_type_name(type_name: &str) -> SlotType {
        match type_name.to_uppercase().as_str() {
            "STRING" => SlotType::String,
            "SYMBOL" => SlotType::Symbol,
            "INTEGER" => SlotType::Integer,
            "NUMBER" => SlotType::Number,
            "FLOAT" => SlotType::Float,
            "FACT-ADDRESS" => SlotType::FactAddress,
            "INSTANCE-ADDRESS" => SlotType::InstanceAddress,
            "INSTANCE-NAME" => SlotType::InstanceName,
            "EXTERNAL-ADDRESS" => SlotType::ExternalAddress,
            _ => SlotType::Any,
        }
    }

    fn to_json_schema_type(&self) -> &'static str {
        match self {
            SlotType::String | SlotType::Symbol => "string",
            SlotType::Integer => "integer",
            SlotType::Float | SlotType::Number => "number",
            SlotType::FactAddress
            | SlotType::InstanceAddress
            | SlotType::InstanceName
            | SlotType::ExternalAddress => "object",
            SlotType::Any => "string", // Default to string for any
        }
    }

    fn to_clips_type_str(&self) -> &'static str {
        match self {
            SlotType::String => "STRING",
            SlotType::Symbol => "SYMBOL",
            SlotType::Integer => "INTEGER",
            SlotType::Float => "FLOAT",
            SlotType::Number => "NUMBER",
            SlotType::FactAddress => "FACT-ADDRESS",
            SlotType::InstanceAddress => "INSTANCE-ADDRESS",
            SlotType::InstanceName => "INSTANCE-NAME",
            SlotType::ExternalAddress => "EXTERNAL-ADDRESS",
            SlotType::Any => "?VARIABLE",
        }
    }
}

// ============================================================================
// Schema Conversion Error
// ============================================================================

/// Error during schema conversion
#[derive(Debug, Clone)]
pub struct SchemaError {
    /// Error message
    pub message: String,
    /// Context (e.g., template or slot name)
    pub context: Option<String>,
}

impl std::fmt::Display for SchemaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(ref ctx) = self.context {
            write!(f, "{}: {}", ctx, self.message)
        } else {
            write!(f, "{}", self.message)
        }
    }
}

impl std::error::Error for SchemaError {}

// ============================================================================
// Deftemplate to JSON Schema
// ============================================================================

/// Convert a template schema to JSON Schema 2020-12 format
///
/// # Example
///
/// ```
/// use nxuskit_engine::providers::clips::schema::{TemplateSchema, SlotSchema, SlotType, deftemplate_to_json_schema};
///
/// let template = TemplateSchema {
///     name: "person".to_string(),
///     slots: vec![
///         SlotSchema {
///             name: "name".to_string(),
///             slot_type: SlotType::String,
///             is_multi: false,
///             default: None,
///             allowed_values: None,
///             cardinality: None,
///             range: None,
///         },
///     ],
///     documentation: Some("A person record".to_string()),
/// };
///
/// let schema = deftemplate_to_json_schema(&template);
/// assert!(schema.get("$schema").is_some());
/// ```
pub fn deftemplate_to_json_schema(template: &TemplateSchema) -> JsonValue {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();

    for slot in &template.slots {
        let slot_schema = slot_to_json_schema(slot);
        properties.insert(slot.name.clone(), slot_schema);

        // Slots without defaults are required
        if slot.default.is_none() && slot.allowed_values.is_none() {
            required.push(json!(slot.name.clone()));
        }
    }

    let mut schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": format!("clips:deftemplate:{}", template.name),
        "title": template.name,
        "type": "object",
        "properties": properties,
        "additionalProperties": false
    });

    if !required.is_empty() {
        schema["required"] = json!(required);
    }

    if let Some(ref doc) = template.documentation {
        schema["description"] = json!(doc);
    }

    schema
}

/// Convert a single slot to JSON Schema property
fn slot_to_json_schema(slot: &SlotSchema) -> JsonValue {
    let mut prop = serde_json::Map::new();

    if slot.is_multi {
        // Multislot: array of items
        prop.insert("type".to_string(), json!("array"));

        let mut items = serde_json::Map::new();
        items.insert(
            "type".to_string(),
            json!(slot.slot_type.to_json_schema_type()),
        );

        // Add allowed values to items if present
        if let Some(ref allowed) = slot.allowed_values {
            items.insert("enum".to_string(), json!(allowed));
        }

        prop.insert("items".to_string(), JsonValue::Object(items));

        // Add cardinality constraints
        if let Some((min, max)) = slot.cardinality {
            prop.insert("minItems".to_string(), json!(min));
            if max < u32::MAX {
                prop.insert("maxItems".to_string(), json!(max));
            }
        }
    } else {
        // Single slot
        prop.insert(
            "type".to_string(),
            json!(SlotTypeSchemaExt::to_json_schema_type(&slot.slot_type)),
        );

        // Add allowed values as enum
        if let Some(ref allowed) = slot.allowed_values {
            prop.insert("enum".to_string(), json!(allowed));
        }

        // Add range constraints for numbers
        if let Some((min, max)) = slot.range {
            if min.is_finite() {
                prop.insert("minimum".to_string(), json!(min));
            }
            if max.is_finite() {
                prop.insert("maximum".to_string(), json!(max));
            }
        }
    }

    // Add default value
    if let Some(ref default) = slot.default {
        prop.insert("default".to_string(), default.clone());
    }

    JsonValue::Object(prop)
}

/// Convert multiple templates to a combined JSON Schema
pub fn templates_to_json_schema(templates: &[TemplateSchema]) -> JsonValue {
    let mut defs = serde_json::Map::new();

    for template in templates {
        let schema = deftemplate_to_json_schema(template);
        defs.insert(template.name.clone(), schema);
    }

    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "clips:schema",
        "title": "CLIPS Templates",
        "type": "object",
        "$defs": defs
    })
}

// ============================================================================
// JSON Schema to Deftemplate
// ============================================================================

/// Convert JSON Schema to CLIPS deftemplate string
///
/// # Example
///
/// ```
/// use nxuskit_engine::providers::clips::schema::json_schema_to_deftemplate;
/// use serde_json::json;
///
/// let schema = json!({
///     "title": "person",
///     "type": "object",
///     "properties": {
///         "name": {"type": "string"},
///         "age": {"type": "integer", "minimum": 0, "maximum": 150}
///     }
/// });
///
/// let deftemplate = json_schema_to_deftemplate(&schema).unwrap();
/// assert!(deftemplate.contains("deftemplate person"));
/// ```
pub fn json_schema_to_deftemplate(schema: &JsonValue) -> Result<String, SchemaError> {
    let title = schema
        .get("title")
        .and_then(|v| v.as_str())
        .ok_or_else(|| SchemaError {
            message: "Schema must have a 'title' field for template name".to_string(),
            context: None,
        })?;

    let properties = schema
        .get("properties")
        .and_then(|v| v.as_object())
        .ok_or_else(|| SchemaError {
            message: "Schema must have 'properties' object".to_string(),
            context: Some(title.to_string()),
        })?;

    let mut slots = Vec::new();
    let required: Vec<&str> = schema
        .get("required")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    for (name, prop_schema) in properties {
        let slot_str = property_to_slot(name, prop_schema, required.contains(&name.as_str()))?;
        slots.push(slot_str);
    }

    // Build deftemplate string
    let mut result = format!("(deftemplate {}\n", title);

    // Add documentation if present
    if let Some(desc) = schema.get("description").and_then(|v| v.as_str()) {
        result.push_str(&format!("    \"{}\"\n", desc.replace('"', "\\\"")));
    }

    for slot in slots {
        result.push_str(&format!("    {}\n", slot));
    }

    result.push(')');

    Ok(result)
}

/// Convert a JSON Schema property to CLIPS slot definition
fn property_to_slot(
    name: &str,
    prop: &JsonValue,
    _is_required: bool,
) -> Result<String, SchemaError> {
    let prop_type = prop
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("string");

    let is_array = prop_type == "array";
    let slot_keyword = if is_array { "multislot" } else { "slot" };

    let mut parts = vec![format!("({} {}", slot_keyword, name)];

    // Determine CLIPS type
    let clips_type = if is_array {
        // For arrays, get item type
        prop.get("items")
            .and_then(|items| items.get("type"))
            .and_then(|t| t.as_str())
            .map(json_type_to_clips)
            .unwrap_or("STRING")
    } else {
        json_type_to_clips(prop_type)
    };

    parts.push(format!("(type {})", clips_type));

    // Handle enum (allowed values)
    let enum_source = if is_array {
        prop.get("items").and_then(|i| i.get("enum"))
    } else {
        prop.get("enum")
    };

    if let Some(enum_values) = enum_source.and_then(|v| v.as_array()) {
        let allowed: Vec<String> = enum_values
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        if !allowed.is_empty() {
            parts.push(format!("(allowed-symbols {})", allowed.join(" ")));
        }
    }

    // Handle default value
    if let Some(default) = prop.get("default") {
        let default_str = match default {
            JsonValue::String(s) => format!("\"{}\"", s),
            JsonValue::Number(n) => n.to_string(),
            JsonValue::Bool(b) => if *b { "TRUE" } else { "FALSE" }.to_string(),
            _ => default.to_string(),
        };
        parts.push(format!("(default {})", default_str));
    }

    // Handle range constraints
    if let (Some(min), Some(max)) = (prop.get("minimum"), prop.get("maximum"))
        && let (Some(min_n), Some(max_n)) = (min.as_f64(), max.as_f64())
    {
        parts.push(format!("(range {} {})", min_n, max_n));
    }

    // Handle cardinality for arrays
    if is_array {
        let min_items = prop.get("minItems").and_then(|v| v.as_u64()).unwrap_or(0);
        let max_items = prop
            .get("maxItems")
            .and_then(|v| v.as_u64())
            .map(|v| v.to_string())
            .unwrap_or_else(|| "?VARIABLE".to_string());
        parts.push(format!("(cardinality {} {})", min_items, max_items));
    }

    parts.push(")".to_string());

    Ok(parts.join(" "))
}

/// Convert JSON Schema type to CLIPS type
fn json_type_to_clips(json_type: &str) -> &'static str {
    match json_type {
        "string" => "STRING",
        "integer" => "INTEGER",
        "number" => "FLOAT",
        "boolean" => "SYMBOL",
        "object" => "EXTERNAL-ADDRESS",
        "array" => "STRING", // Fallback for nested arrays
        _ => "STRING",
    }
}

// ============================================================================
// Environment Extraction
// ============================================================================

/// Extract all template schemas from a CLIPS environment
pub fn extract_schemas_from_environment(env: &ClipsEnvironment) -> ClipsSchema {
    let mut templates = Vec::new();

    for template in env.templates().flatten() {
        let name = template.name().unwrap_or_else(|_| "unknown".to_string());

        // Skip system templates
        if name.starts_with("initial-") {
            continue;
        }

        let mut slots = Vec::new();

        if let Ok(slot_names) = template.slot_names() {
            for slot_name in slot_names {
                let is_multi = template.slot_is_multi(&slot_name).unwrap_or(false);

                // Determine slot type from types constraint
                let slot_type = template
                    .slot_types(&slot_name)
                    .ok()
                    .flatten()
                    .and_then(|types| types.first().cloned())
                    .map(|t| SlotType::from_clips_type_name(&t))
                    .unwrap_or(SlotType::Any);

                // Get default value
                let default = template
                    .slot_default_value(&slot_name)
                    .ok()
                    .flatten()
                    .map(|v| clips_value_to_json(&v));

                // Get allowed values
                let allowed_values = template
                    .slot_allowed_values(&slot_name)
                    .ok()
                    .flatten()
                    .map(|values| values.iter().map(clips_value_to_json).collect());

                // Get cardinality
                let cardinality =
                    template
                        .slot_cardinality(&slot_name)
                        .ok()
                        .flatten()
                        .map(|(min, max)| {
                            (
                                min.max(0) as u32,
                                if max == i64::MAX {
                                    u32::MAX
                                } else {
                                    max as u32
                                },
                            )
                        });

                // Get range
                let range = template.slot_range(&slot_name).ok().flatten();

                slots.push(SlotSchema {
                    name: slot_name,
                    slot_type,
                    is_multi,
                    default,
                    allowed_values,
                    cardinality,
                    range,
                });
            }
        }

        // Extract documentation from pp_form
        let documentation = template.pp_form().and_then(|pp| {
            // Try to extract string literal after template name
            let lines: Vec<&str> = pp.lines().collect();
            if lines.len() > 1 {
                let second_line = lines[1].trim();
                if second_line.starts_with('"') && second_line.ends_with('"') {
                    return Some(
                        second_line
                            .trim_matches('"')
                            .replace("\\\"", "\"")
                            .to_string(),
                    );
                }
            }
            None
        });

        templates.push(TemplateSchema {
            name,
            slots,
            documentation,
        });
    }

    ClipsSchema { templates }
}

/// Convert CLIPS value to JSON value
fn clips_value_to_json(value: &clips_sys::ClipsValue) -> JsonValue {
    use clips_sys::ClipsValue;

    match value {
        ClipsValue::Void => JsonValue::Null,
        ClipsValue::Integer(i) => json!(i),
        ClipsValue::Float(f) => json!(f),
        ClipsValue::String(s) => json!(s),
        ClipsValue::Symbol(s) => match s.as_str() {
            "TRUE" => json!(true),
            "FALSE" => json!(false),
            "nil" => JsonValue::Null,
            _ => json!(s),
        },
        ClipsValue::Boolean(b) => json!(b),
        ClipsValue::Multifield(items) => {
            let arr: Vec<JsonValue> = items.iter().map(clips_value_to_json).collect();
            json!(arr)
        }
        ClipsValue::FactAddress(idx) => json!({"_fact_address": idx}),
        ClipsValue::InstanceAddress(name) => json!({"_instance": name}),
        ClipsValue::ExternalAddress(addr) => json!({"_external_address": format!("0x{:x}", addr)}),
    }
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Generate a human-readable description of a template
pub fn describe_template(template: &TemplateSchema) -> String {
    let mut lines = Vec::new();

    lines.push(format!("Template: {}", template.name));
    lines.push("-".repeat(40));

    if let Some(ref doc) = template.documentation {
        lines.push(format!("Description: {}", doc));
        lines.push(String::new());
    }

    lines.push("Slots:".to_string());
    for slot in &template.slots {
        let mut slot_desc = format!(
            "  {} ({}){}",
            slot.name,
            slot.slot_type.to_clips_type_str(),
            if slot.is_multi { " [multi]" } else { "" }
        );

        if let Some(ref default) = slot.default {
            slot_desc.push_str(&format!(" = {}", default));
        }

        if let Some(ref allowed) = slot.allowed_values {
            let values: Vec<String> = allowed.iter().map(|v| v.to_string()).collect();
            slot_desc.push_str(&format!(" allowed: [{}]", values.join(", ")));
        }

        if let Some((min, max)) = slot.range {
            slot_desc.push_str(&format!(" range: [{}, {}]", min, max));
        }

        if let Some((min, max)) = slot.cardinality {
            let max_str = if max == u32::MAX {
                "*".to_string()
            } else {
                max.to_string()
            };
            slot_desc.push_str(&format!(" cardinality: [{}, {}]", min, max_str));
        }

        lines.push(slot_desc);
    }

    lines.join("\n")
}

/// Generate human-readable descriptions for all templates
pub fn describe_all_templates(schema: &ClipsSchema) -> String {
    schema
        .templates
        .iter()
        .map(describe_template)
        .collect::<Vec<_>>()
        .join("\n\n")
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slot_type_conversion() {
        assert_eq!(SlotType::from_clips_type_name("STRING"), SlotType::String);
        assert_eq!(SlotType::from_clips_type_name("INTEGER"), SlotType::Integer);
        assert_eq!(SlotType::from_clips_type_name("FLOAT"), SlotType::Float);
        assert_eq!(SlotType::from_clips_type_name("SYMBOL"), SlotType::Symbol);

        assert_eq!(SlotType::String.to_json_schema_type(), "string");
        assert_eq!(SlotType::Integer.to_json_schema_type(), "integer");
        assert_eq!(SlotType::Float.to_json_schema_type(), "number");

        assert_eq!(SlotType::String.to_clips_type_str(), "STRING");
        assert_eq!(SlotType::Integer.to_clips_type_str(), "INTEGER");
        assert_eq!(SlotType::Float.to_clips_type_str(), "FLOAT");
    }

    #[test]
    fn test_deftemplate_to_json_schema() {
        let template = TemplateSchema {
            name: "person".to_string(),
            slots: vec![
                SlotSchema {
                    name: "name".to_string(),
                    slot_type: SlotType::String,
                    is_multi: false,
                    default: None,
                    allowed_values: None,
                    cardinality: None,
                    range: None,
                },
                SlotSchema {
                    name: "age".to_string(),
                    slot_type: SlotType::Integer,
                    is_multi: false,
                    default: Some(json!(0)),
                    allowed_values: None,
                    cardinality: None,
                    range: Some((0.0, 150.0)),
                },
                SlotSchema {
                    name: "status".to_string(),
                    slot_type: SlotType::Symbol,
                    is_multi: false,
                    default: Some(json!("active")),
                    allowed_values: Some(vec![json!("active"), json!("inactive")]),
                    cardinality: None,
                    range: None,
                },
            ],
            documentation: Some("A person record".to_string()),
        };

        let schema = deftemplate_to_json_schema(&template);

        assert_eq!(
            schema["$schema"],
            "https://json-schema.org/draft/2020-12/schema"
        );
        assert_eq!(schema["title"], "person");
        assert_eq!(schema["description"], "A person record");

        let props = schema["properties"].as_object().unwrap();
        assert!(props.contains_key("name"));
        assert!(props.contains_key("age"));
        assert!(props.contains_key("status"));

        assert_eq!(props["age"]["minimum"], 0.0);
        assert_eq!(props["age"]["maximum"], 150.0);
        assert_eq!(props["status"]["enum"], json!(["active", "inactive"]));
    }

    #[test]
    fn test_json_schema_to_deftemplate() {
        let schema = json!({
            "title": "order",
            "description": "An order record",
            "type": "object",
            "properties": {
                "id": {"type": "string"},
                "amount": {"type": "number", "minimum": 0, "maximum": 10000},
                "items": {"type": "array", "items": {"type": "string"}, "minItems": 1}
            },
            "required": ["id", "amount"]
        });

        let deftemplate = json_schema_to_deftemplate(&schema).unwrap();

        assert!(deftemplate.contains("deftemplate order"));
        assert!(deftemplate.contains("slot id"));
        assert!(deftemplate.contains("slot amount"));
        assert!(deftemplate.contains("multislot items"));
        assert!(deftemplate.contains("(range 0 10000)"));
        assert!(deftemplate.contains("(cardinality 1"));
    }

    #[test]
    fn test_multislot_schema() {
        let template = TemplateSchema {
            name: "shopping-cart".to_string(),
            slots: vec![SlotSchema {
                name: "items".to_string(),
                slot_type: SlotType::String,
                is_multi: true,
                default: None,
                allowed_values: None,
                cardinality: Some((0, 100)),
                range: None,
            }],
            documentation: None,
        };

        let schema = deftemplate_to_json_schema(&template);

        assert_eq!(schema["properties"]["items"]["type"], "array");
        assert_eq!(schema["properties"]["items"]["items"]["type"], "string");
        assert_eq!(schema["properties"]["items"]["minItems"], 0);
        assert_eq!(schema["properties"]["items"]["maxItems"], 100);
    }

    #[test]
    fn test_describe_template() {
        let template = TemplateSchema {
            name: "test".to_string(),
            slots: vec![SlotSchema {
                name: "value".to_string(),
                slot_type: SlotType::Integer,
                is_multi: false,
                default: Some(json!(42)),
                allowed_values: None,
                cardinality: None,
                range: Some((0.0, 100.0)),
            }],
            documentation: Some("A test template".to_string()),
        };

        let description = describe_template(&template);

        assert!(description.contains("Template: test"));
        assert!(description.contains("A test template"));
        assert!(description.contains("value"));
        assert!(description.contains("INTEGER"));
        assert!(description.contains("= 42"));
        assert!(description.contains("range: [0, 100]"));
    }
}
