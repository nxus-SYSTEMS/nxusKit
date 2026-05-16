//! Tool/function calling types for the nxusKit C ABI.
//!
//! These types define the canonical tool calling contract used across all
//! wrappers (Rust, Go, Python). They serialize to/from JSON at the C ABI
//! boundary, matching the OpenAI-compatible tool calling schema.

use serde::{Deserialize, Serialize};

// ── Tool Definition (in ChatRequest) ─────────────────────────────

/// A tool available for the model to call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool type — always `"function"`.
    #[serde(rename = "type")]
    pub tool_type: String,
    /// The function definition.
    pub function: FunctionDefinition,
}

/// A function that can be called by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    /// Function name (alphanumeric + underscores).
    pub name: String,
    /// Human-readable description of what the function does.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON Schema describing the function's parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
}

// ── Tool Choice (in ChatRequest) ─────────────────────────────────

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

/// Named function reference inside `ToolChoiceFunction`.
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

// ── Tool Call (in ChatResponse) ──────────────────────────────────

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

// ── Tool Call Delta (in streaming response) ─────────────────────

/// Incremental tool call data in a streaming chunk.
///
/// During streaming, the model may emit tool calls across multiple chunks.
/// Each delta carries a partial function name and/or partial arguments string
/// that must be concatenated by the client to form a complete `ToolCall`.
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

/// Incremental function call data within a tool call delta.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCallDelta {
    /// Function name fragment (typically complete in first delta).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Arguments fragment (concatenate across deltas to form complete JSON).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
}

// ── Tool Result (in continuation request) ────────────────────────

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

// ── Validation ───────────────────────────────────────────────────

/// Validate a tool definition for basic correctness.
pub fn validate_tool_definition(tool: &ToolDefinition) -> Result<(), String> {
    if tool.tool_type != "function" {
        return Err(format!(
            "Invalid tool type '{}': expected 'function'",
            tool.tool_type
        ));
    }
    if tool.function.name.is_empty() {
        return Err("Function name must not be empty".to_string());
    }
    // Validate name contains only allowed characters
    if !tool
        .function
        .name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(format!(
            "Function name '{}' contains invalid characters (allowed: alphanumeric, _, -)",
            tool.function.name
        ));
    }
    // If parameters are provided, verify it looks like an object schema
    if let Some(params) = &tool.function.parameters
        && !params.is_object()
    {
        return Err("Function parameters must be a JSON object (JSON Schema)".to_string());
    }
    Ok(())
}

/// Validate a tool choice value.
pub fn validate_tool_choice(choice: &ToolChoice) -> Result<(), String> {
    match choice {
        ToolChoice::Mode(mode) => {
            if !["auto", "none", "required"].contains(&mode.as_str()) {
                return Err(format!(
                    "Invalid tool_choice mode '{}': expected 'auto', 'none', or 'required'",
                    mode
                ));
            }
        }
        ToolChoice::Named(named) => {
            if named.tool_type != "function" {
                return Err(format!(
                    "Invalid tool_choice type '{}': expected 'function'",
                    named.tool_type
                ));
            }
            if named.function.name.is_empty() {
                return Err("Named tool_choice function name must not be empty".to_string());
            }
        }
    }
    Ok(())
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_definition_roundtrip() {
        let tool = ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "get_weather".to_string(),
                description: Some("Get current weather for a location".to_string()),
                parameters: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "location": { "type": "string", "description": "City name" },
                        "unit": { "type": "string", "enum": ["celsius", "fahrenheit"] }
                    },
                    "required": ["location"]
                })),
            },
        };

        let json = serde_json::to_string(&tool).unwrap();
        let parsed: ToolDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.tool_type, "function");
        assert_eq!(parsed.function.name, "get_weather");
        assert!(parsed.function.description.is_some());
    }

    #[test]
    fn test_tool_choice_auto_serde() {
        let choice = ToolChoice::auto();
        let json = serde_json::to_string(&choice).unwrap();
        assert_eq!(json, r#""auto""#);
    }

    #[test]
    fn test_tool_choice_none_serde() {
        let choice = ToolChoice::none();
        let json = serde_json::to_string(&choice).unwrap();
        assert_eq!(json, r#""none""#);
    }

    #[test]
    fn test_tool_choice_required_serde() {
        let choice = ToolChoice::required();
        let json = serde_json::to_string(&choice).unwrap();
        assert_eq!(json, r#""required""#);
    }

    #[test]
    fn test_tool_choice_named_serde() {
        let choice = ToolChoice::named("get_weather");
        let json = serde_json::to_string(&choice).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "function");
        assert_eq!(parsed["function"]["name"], "get_weather");
    }

    #[test]
    fn test_tool_choice_deserialize_auto() {
        let choice: ToolChoice = serde_json::from_str(r#""auto""#).unwrap();
        match choice {
            ToolChoice::Mode(m) => assert_eq!(m, "auto"),
            _ => panic!("expected Mode"),
        }
    }

    #[test]
    fn test_tool_choice_deserialize_named() {
        let json = r#"{"type":"function","function":{"name":"get_weather"}}"#;
        let choice: ToolChoice = serde_json::from_str(json).unwrap();
        match choice {
            ToolChoice::Named(n) => assert_eq!(n.function.name, "get_weather"),
            _ => panic!("expected Named"),
        }
    }

    #[test]
    fn test_tool_call_serde() {
        let call = ToolCall {
            id: "call_abc123".to_string(),
            tool_type: "function".to_string(),
            function: FunctionCall {
                name: "get_weather".to_string(),
                arguments: r#"{"location":"Tokyo","unit":"celsius"}"#.to_string(),
            },
        };

        let json = serde_json::to_string(&call).unwrap();
        let parsed: ToolCall = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "call_abc123");
        assert_eq!(parsed.function.name, "get_weather");
    }

    #[test]
    fn test_tool_result_message_serde() {
        let result = ToolResultMessage {
            role: "tool".to_string(),
            tool_call_id: "call_abc123".to_string(),
            content: r#"{"temperature": 22, "condition": "sunny"}"#.to_string(),
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: ToolResultMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.role, "tool");
        assert_eq!(parsed.tool_call_id, "call_abc123");
    }

    #[test]
    fn test_validate_valid_tool() {
        let tool = ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "get_weather".to_string(),
                description: None,
                parameters: None,
            },
        };
        assert!(validate_tool_definition(&tool).is_ok());
    }

    #[test]
    fn test_validate_invalid_tool_type() {
        let tool = ToolDefinition {
            tool_type: "invalid".to_string(),
            function: FunctionDefinition {
                name: "get_weather".to_string(),
                description: None,
                parameters: None,
            },
        };
        let err = validate_tool_definition(&tool).unwrap_err();
        assert!(err.contains("Invalid tool type"));
    }

    #[test]
    fn test_validate_empty_name() {
        let tool = ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "".to_string(),
                description: None,
                parameters: None,
            },
        };
        let err = validate_tool_definition(&tool).unwrap_err();
        assert!(err.contains("must not be empty"));
    }

    #[test]
    fn test_validate_invalid_name_chars() {
        let tool = ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "get weather!".to_string(),
                description: None,
                parameters: None,
            },
        };
        let err = validate_tool_definition(&tool).unwrap_err();
        assert!(err.contains("invalid characters"));
    }

    #[test]
    fn test_validate_non_object_parameters() {
        let tool = ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "test_fn".to_string(),
                description: None,
                parameters: Some(serde_json::json!("not an object")),
            },
        };
        let err = validate_tool_definition(&tool).unwrap_err();
        assert!(err.contains("JSON object"));
    }

    #[test]
    fn test_validate_tool_choice_valid_modes() {
        assert!(validate_tool_choice(&ToolChoice::auto()).is_ok());
        assert!(validate_tool_choice(&ToolChoice::none()).is_ok());
        assert!(validate_tool_choice(&ToolChoice::required()).is_ok());
    }

    #[test]
    fn test_validate_tool_choice_invalid_mode() {
        let choice = ToolChoice::Mode("invalid".to_string());
        let err = validate_tool_choice(&choice).unwrap_err();
        assert!(err.contains("Invalid tool_choice mode"));
    }

    #[test]
    fn test_validate_tool_choice_named() {
        assert!(validate_tool_choice(&ToolChoice::named("fn_name")).is_ok());
    }

    #[test]
    fn test_validate_tool_choice_named_empty() {
        let choice = ToolChoice::Named(ToolChoiceFunction {
            tool_type: "function".to_string(),
            function: ToolChoiceFunctionName {
                name: "".to_string(),
            },
        });
        let err = validate_tool_choice(&choice).unwrap_err();
        assert!(err.contains("must not be empty"));
    }

    #[test]
    fn test_full_contract_schema() {
        // Verify the full JSON schema matches the contract doc
        let request_tools = serde_json::json!({
            "tools": [
                {
                    "type": "function",
                    "function": {
                        "name": "get_weather",
                        "description": "Get current weather for a location",
                        "parameters": {
                            "type": "object",
                            "properties": {
                                "location": { "type": "string", "description": "City name" },
                                "unit": { "type": "string", "enum": ["celsius", "fahrenheit"] }
                            },
                            "required": ["location"]
                        }
                    }
                }
            ],
            "tool_choice": "auto"
        });

        let tools: Vec<ToolDefinition> =
            serde_json::from_value(request_tools["tools"].clone()).unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].function.name, "get_weather");

        let choice: ToolChoice =
            serde_json::from_value(request_tools["tool_choice"].clone()).unwrap();
        match choice {
            ToolChoice::Mode(m) => assert_eq!(m, "auto"),
            _ => panic!("expected Mode"),
        }
    }

    #[test]
    fn test_response_tool_calls_schema() {
        // Verify the response tool_calls JSON matches the contract doc
        let response_json = serde_json::json!({
            "tool_calls": [
                {
                    "id": "call_abc123",
                    "type": "function",
                    "function": {
                        "name": "get_weather",
                        "arguments": "{\"location\":\"Tokyo\",\"unit\":\"celsius\"}"
                    }
                }
            ],
            "finish_reason": "tool_calls"
        });

        let calls: Vec<ToolCall> =
            serde_json::from_value(response_json["tool_calls"].clone()).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_abc123");
        assert_eq!(calls[0].function.name, "get_weather");
    }

    #[test]
    fn test_tool_call_delta_first_chunk() {
        // First delta: includes id, type, and function name
        let json =
            r#"{"index":0,"id":"call_abc123","type":"function","function":{"name":"get_weather"}}"#;
        let delta: ToolCallDelta = serde_json::from_str(json).unwrap();
        assert_eq!(delta.index, 0);
        assert_eq!(delta.id.as_deref(), Some("call_abc123"));
        assert_eq!(delta.tool_type.as_deref(), Some("function"));
        assert_eq!(
            delta.function.as_ref().unwrap().name.as_deref(),
            Some("get_weather")
        );
    }

    #[test]
    fn test_tool_call_delta_argument_chunk() {
        // Subsequent delta: only index and partial arguments
        let json = r#"{"index":0,"function":{"arguments":"{\"location\":\"Tok"}}"#;
        let delta: ToolCallDelta = serde_json::from_str(json).unwrap();
        assert_eq!(delta.index, 0);
        assert!(delta.id.is_none());
        assert!(delta.tool_type.is_none());
        assert_eq!(
            delta.function.as_ref().unwrap().arguments.as_deref(),
            Some("{\"location\":\"Tok")
        );
    }

    #[test]
    fn test_tool_call_delta_roundtrip() {
        let delta = ToolCallDelta {
            index: 1,
            id: Some("call_xyz".to_string()),
            tool_type: Some("function".to_string()),
            function: Some(FunctionCallDelta {
                name: Some("get_time".to_string()),
                arguments: None,
            }),
        };
        let json = serde_json::to_string(&delta).unwrap();
        let parsed: ToolCallDelta = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.index, 1);
        assert_eq!(parsed.id.as_deref(), Some("call_xyz"));
        assert_eq!(
            parsed.function.as_ref().unwrap().name.as_deref(),
            Some("get_time")
        );
    }

    #[test]
    fn test_tool_call_delta_minimal() {
        // Minimal delta: just index
        let json = r#"{"index":0}"#;
        let delta: ToolCallDelta = serde_json::from_str(json).unwrap();
        assert_eq!(delta.index, 0);
        assert!(delta.id.is_none());
        assert!(delta.function.is_none());
    }

    #[test]
    fn test_streaming_tool_call_assembly() {
        // Simulate assembling a complete tool call from streaming deltas.
        // This mimics the OpenAI-compatible streaming format.
        let deltas_json = [
            // Chunk 1: first delta with id, type, and function name
            r#"{"index":0,"id":"call_abc123","type":"function","function":{"name":"get_weather"}}"#,
            // Chunk 2: first argument fragment
            r#"{"index":0,"function":{"arguments":"{\"location\":\"Tok"}}"#,
            // Chunk 3: second argument fragment
            r#"{"index":0,"function":{"arguments":"yo\",\"unit\":\"celsius\"}"}}"#,
        ];

        let deltas: Vec<ToolCallDelta> = deltas_json
            .iter()
            .map(|j| serde_json::from_str(j).unwrap())
            .collect();

        // Assembly logic: concatenate fields across deltas
        let mut id = String::new();
        let mut name = String::new();
        let mut arguments = String::new();

        for d in &deltas {
            if let Some(ref i) = d.id {
                id = i.clone();
            }
            if let Some(ref f) = d.function {
                if let Some(ref n) = f.name {
                    name.push_str(n);
                }
                if let Some(ref a) = f.arguments {
                    arguments.push_str(a);
                }
            }
        }

        assert_eq!(id, "call_abc123");
        assert_eq!(name, "get_weather");
        assert_eq!(arguments, r#"{"location":"Tokyo","unit":"celsius"}"#);

        // Verify assembled arguments parse as valid JSON
        let parsed_args: serde_json::Value = serde_json::from_str(&arguments).unwrap();
        assert_eq!(parsed_args["location"], "Tokyo");
        assert_eq!(parsed_args["unit"], "celsius");
    }
}
