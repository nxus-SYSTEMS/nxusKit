//! User-supplied external tool support for CE providers.
//!
//! tier: ce-impl

use std::fmt;
use std::sync::Arc;

use serde_json::{Map, Value};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CeToolPosture {
    Native,
    Emulated,
    ExternalOnly,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCallingSupport {
    provider_name: String,
    posture: CeToolPosture,
    available: bool,
}

impl ToolCallingSupport {
    pub fn new(provider_name: impl Into<String>, posture: CeToolPosture) -> Self {
        let available = !matches!(posture, CeToolPosture::Unavailable);
        Self {
            provider_name: provider_name.into(),
            posture,
            available,
        }
    }

    pub fn provider_name(&self) -> &str {
        &self.provider_name
    }

    pub fn posture(&self) -> CeToolPosture {
        self.posture
    }

    pub fn available(&self) -> bool {
        self.available
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CeFacadeError {
    #[error("tool calling is unsupported by this provider")]
    UnsupportedByProvider,

    #[error("tool calling is unavailable")]
    Unavailable,

    #[error("tool calling is not entitled")]
    Entitlement,

    #[error("tool contract error: {0}")]
    Contract(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolResult {
    output: Value,
    error: Option<String>,
}

impl ToolResult {
    pub fn success(output: Value) -> Self {
        Self {
            output,
            error: None,
        }
    }

    pub fn failure(message: impl Into<String>) -> Self {
        Self {
            output: Value::Null,
            error: Some(message.into()),
        }
    }

    pub fn output(&self) -> &Value {
        &self.output
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

#[derive(Clone)]
pub struct ExternalTool {
    name: String,
    description: String,
    parameters: Value,
    handler: Arc<dyn Fn(Value) -> ToolResult + Send + Sync>,
}

impl fmt::Debug for ExternalTool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExternalTool")
            .field("name", &self.name)
            .field("description", &self.description)
            .field("parameters", &self.parameters)
            .finish_non_exhaustive()
    }
}

impl ExternalTool {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: Value,
        handler: impl Fn(Value) -> ToolResult + Send + Sync + 'static,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters,
            handler: Arc::new(handler),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn parameters(&self) -> &Value {
        &self.parameters
    }

    pub fn invoke(&self, arguments: Value) -> ToolResult {
        (self.handler)(arguments)
    }

    pub fn validate(&self) -> Result<(), CeFacadeError> {
        if self.name.trim().is_empty() {
            return Err(CeFacadeError::Contract(
                "external tool name is required".to_string(),
            ));
        }

        validate_parameter_schema(&self.parameters)?;

        Ok(())
    }
}

fn validate_parameter_schema(parameters: &Value) -> Result<(), CeFacadeError> {
    let Some(schema) = parameters.as_object() else {
        let message = if parameters.is_null() {
            "external tool parameters are required"
        } else {
            "external tool parameters must be a JSON object schema"
        };
        return Err(CeFacadeError::Contract(message.to_string()));
    };

    if let Some(root_type) = schema.get("type") {
        match root_type {
            Value::String(value) if value == "object" => {}
            _ => {
                return Err(CeFacadeError::Contract(
                    "external tool parameters root type must be \"object\"".to_string(),
                ));
            }
        }
    }

    validate_schema_object(schema, "parameters")
}

fn validate_schema_object(schema: &Map<String, Value>, path: &str) -> Result<(), CeFacadeError> {
    if let Some(type_value) = schema.get("type") {
        validate_type_value(type_value, path)?;
    }

    if let Some(properties) = schema.get("properties") {
        let properties = properties.as_object().ok_or_else(|| {
            CeFacadeError::Contract(format!("{path}.properties must be an object"))
        })?;

        for (property_name, property_schema) in properties {
            validate_nested_schema(
                property_schema,
                &format!("{path}.properties.{property_name}"),
            )?;
        }
    }

    if let Some(required) = schema.get("required") {
        let required = required.as_array().ok_or_else(|| {
            CeFacadeError::Contract(format!("{path}.required must be an array of strings"))
        })?;

        for item in required {
            let Some(field) = item.as_str() else {
                return Err(CeFacadeError::Contract(format!(
                    "{path}.required entries must be strings"
                )));
            };

            if field.trim().is_empty() {
                return Err(CeFacadeError::Contract(format!(
                    "{path}.required entries must not be empty"
                )));
            }
        }
    }

    if let Some(additional_properties) = schema.get("additionalProperties") {
        match additional_properties {
            Value::Bool(_) => {}
            Value::Object(nested) => {
                validate_schema_object(nested, &format!("{path}.additionalProperties"))?;
            }
            _ => {
                return Err(CeFacadeError::Contract(format!(
                    "{path}.additionalProperties must be a boolean or schema object"
                )));
            }
        }
    }

    if let Some(items) = schema.get("items") {
        validate_nested_schema(items, &format!("{path}.items"))?;
    }

    for keyword in ["allOf", "anyOf", "oneOf"] {
        if let Some(branches) = schema.get(keyword) {
            let branches = branches.as_array().ok_or_else(|| {
                CeFacadeError::Contract(format!("{path}.{keyword} must be an array of schemas"))
            })?;

            for (index, branch) in branches.iter().enumerate() {
                validate_nested_schema(branch, &format!("{path}.{keyword}[{index}]"))?;
            }
        }
    }

    Ok(())
}

fn validate_nested_schema(schema: &Value, path: &str) -> Result<(), CeFacadeError> {
    match schema {
        Value::Bool(_) => Ok(()),
        Value::Object(schema) => validate_schema_object(schema, path),
        _ => Err(CeFacadeError::Contract(format!(
            "{path} must be a schema object or boolean schema"
        ))),
    }
}

fn validate_type_value(type_value: &Value, path: &str) -> Result<(), CeFacadeError> {
    match type_value {
        Value::String(value) if !value.trim().is_empty() => Ok(()),
        Value::Array(values) => {
            if values.is_empty() {
                return Err(CeFacadeError::Contract(format!(
                    "{path}.type array must not be empty"
                )));
            }

            for value in values {
                let Some(type_name) = value.as_str() else {
                    return Err(CeFacadeError::Contract(format!(
                        "{path}.type entries must be strings"
                    )));
                };

                if type_name.trim().is_empty() {
                    return Err(CeFacadeError::Contract(format!(
                        "{path}.type entries must not be empty"
                    )));
                }
            }

            Ok(())
        }
        _ => Err(CeFacadeError::Contract(format!(
            "{path}.type must be a string or array of strings"
        ))),
    }
}

pub trait ToolCallingProvider {
    fn supports_tool_calling(&self) -> bool;
    fn provider_name(&self) -> &str;

    fn tool_calling_posture(&self) -> CeToolPosture {
        super::provider_native::provider_native_posture(self.supports_tool_calling())
    }

    fn register_external_tool(&mut self, tool: ExternalTool) -> Result<(), CeFacadeError>;
    fn invoke_external_tool(
        &self,
        name: &str,
        arguments: Value,
    ) -> Result<ToolResult, CeFacadeError>;
}

pub fn register_external_tool(
    provider: &mut dyn ToolCallingProvider,
    tool: ExternalTool,
) -> Result<(), CeFacadeError> {
    if !provider.supports_tool_calling() {
        return Err(CeFacadeError::UnsupportedByProvider);
    }

    tool.validate()?;
    provider.register_external_tool(tool)
}

pub fn tool_calling_support(provider: &dyn ToolCallingProvider) -> ToolCallingSupport {
    let posture = if provider.supports_tool_calling() {
        provider.tool_calling_posture()
    } else {
        CeToolPosture::Unavailable
    };
    ToolCallingSupport::new(provider.provider_name(), posture)
}

pub fn supports_tool_calling(provider: &dyn ToolCallingProvider) -> bool {
    tool_calling_support(provider).available()
}
