//! Tool/function calling types for the nxusKit Rust SDK.
//!
//! These types define the canonical tool calling contract. They serialize to
//! JSON for the C ABI boundary, matching the OpenAI-compatible schema.

use serde::{Deserialize, Serialize};

/// A tool available for the model to call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool type — always `"function"`.
    #[serde(rename = "type")]
    pub tool_type: String,
    /// The function definition.
    pub function: FunctionDefinition,
}

impl ToolDefinition {
    /// Create a new tool definition for a function.
    pub fn function(name: impl Into<String>) -> ToolDefinitionBuilder {
        ToolDefinitionBuilder {
            name: name.into(),
            description: None,
            parameters: None,
        }
    }
}

/// Builder for [`ToolDefinition`].
pub struct ToolDefinitionBuilder {
    name: String,
    description: Option<String>,
    parameters: Option<serde_json::Value>,
}

impl ToolDefinitionBuilder {
    /// Set the function description.
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set the function parameters as a JSON Schema object.
    pub fn parameters(mut self, params: serde_json::Value) -> Self {
        self.parameters = Some(params);
        self
    }

    /// Build the tool definition.
    pub fn build(self) -> ToolDefinition {
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: self.name,
                description: self.description,
                parameters: self.parameters,
            },
        }
    }
}

/// A function that can be called by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    /// Function name.
    pub name: String,
    /// Human-readable description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON Schema describing the function's parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
}

/// Controls how the model selects tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolChoice {
    /// String mode: `"auto"`, `"none"`, or `"required"`.
    Mode(String),
    /// Named function: force the model to call a specific function.
    Named(ToolChoiceFunction),
}

/// Force a specific function call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolChoiceFunction {
    /// Always `"function"`.
    #[serde(rename = "type")]
    pub tool_type: String,
    /// The function to call.
    pub function: ToolChoiceFunctionName,
}

/// Named function reference inside [`ToolChoiceFunction`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolChoiceFunctionName {
    /// The function name.
    pub name: String,
}

impl ToolChoice {
    /// Create an `"auto"` tool choice.
    pub fn auto() -> Self {
        Self::Mode("auto".to_string())
    }

    /// Create a `"none"` tool choice.
    pub fn none() -> Self {
        Self::Mode("none".to_string())
    }

    /// Create a `"required"` tool choice.
    pub fn required() -> Self {
        Self::Mode("required".to_string())
    }

    /// Create a named function tool choice.
    pub fn named(function_name: impl Into<String>) -> Self {
        Self::Named(ToolChoiceFunction {
            tool_type: "function".to_string(),
            function: ToolChoiceFunctionName {
                name: function_name.into(),
            },
        })
    }
}

/// A tool call requested by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Provider-generated unique ID for this call.
    pub id: String,
    /// Always `"function"`.
    #[serde(rename = "type")]
    pub tool_type: String,
    /// The function to call.
    pub function: FunctionCall,
}

/// Function invocation details from the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    /// Function name to invoke.
    pub name: String,
    /// JSON-encoded arguments string.
    pub arguments: String,
}

/// Result of executing a tool, sent back to the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultMessage {
    /// Always `"tool"`.
    pub role: String,
    /// The ID of the tool call this result corresponds to.
    pub tool_call_id: String,
    /// The result content (typically JSON string).
    pub content: String,
}

impl ToolResultMessage {
    /// Create a tool result message.
    pub fn new(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".to_string(),
            tool_call_id: tool_call_id.into(),
            content: content.into(),
        }
    }
}

/// Incremental tool call data in a streaming chunk.
///
/// During streaming, the model may emit tool calls across multiple chunks.
/// Each delta carries a partial function name and/or partial arguments string
/// that must be concatenated by the client to form a complete [`ToolCall`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallDelta {
    /// Index of this tool call within the chunk's `tool_calls` array.
    pub index: u32,
    /// Provider-generated unique ID (only in first delta for this index).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Always `"function"` (only in first delta for this index).
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub tool_type: Option<String>,
    /// Incremental function call data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function: Option<FunctionCallDelta>,
}

/// Incremental function call data within a [`ToolCallDelta`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCallDelta {
    /// Function name fragment (typically complete in first delta).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Arguments fragment (concatenate across deltas to form complete JSON).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
}
