use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use nxuskit::tools::{
    CeFacadeError, CeToolPosture, ExternalTool, ToolCallingProvider, ToolResult,
    register_external_tool, supports_tool_calling, tool_calling_support,
};
use serde_json::{Value, json};

#[derive(Debug)]
struct RecordingToolProvider {
    provider_name: String,
    posture: CeToolPosture,
    tools: HashMap<String, ExternalTool>,
    invocations: Arc<Mutex<Vec<Value>>>,
}

impl RecordingToolProvider {
    fn new(provider_name: impl Into<String>, posture: CeToolPosture) -> Self {
        Self {
            provider_name: provider_name.into(),
            posture,
            tools: HashMap::new(),
            invocations: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn invoked_args(&self) -> Vec<Value> {
        self.invocations
            .lock()
            .expect("invocation lock poisoned")
            .clone()
    }
}

impl ToolCallingProvider for RecordingToolProvider {
    fn supports_tool_calling(&self) -> bool {
        !matches!(self.posture, CeToolPosture::Unavailable)
    }

    fn provider_name(&self) -> &str {
        &self.provider_name
    }

    fn tool_calling_posture(&self) -> CeToolPosture {
        self.posture
    }

    fn register_external_tool(&mut self, tool: ExternalTool) -> Result<(), CeFacadeError> {
        self.tools.insert(tool.name().to_string(), tool);
        Ok(())
    }

    fn invoke_external_tool(
        &self,
        name: &str,
        arguments: Value,
    ) -> Result<ToolResult, CeFacadeError> {
        self.invocations
            .lock()
            .expect("invocation lock poisoned")
            .push(arguments.clone());

        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| CeFacadeError::Contract(format!("unknown tool: {name}")))?;
        Ok(tool.invoke(arguments))
    }
}

#[test]
fn external_tool_facade_invokes_user_handler_with_provider_arguments() {
    let mut provider = RecordingToolProvider::new("loopback", CeToolPosture::ExternalOnly);

    register_external_tool(
        &mut provider,
        ExternalTool::new(
            "echo",
            "Echo provider-supplied arguments",
            json!({
                "type": "object",
                "properties": {
                    "value": { "type": "string" }
                }
            }),
            ToolResult::success,
        ),
    )
    .expect("external tool registration succeeds");

    let result = provider
        .invoke_external_tool("echo", json!({ "value": "from-provider" }))
        .expect("external tool invocation succeeds");

    assert_eq!(
        provider.invoked_args(),
        vec![json!({ "value": "from-provider" })]
    );
    assert_eq!(result.output(), &json!({ "value": "from-provider" }));
    assert!(result.error().is_none());
}

#[test]
fn tool_calling_support_reports_ce_safe_postures() {
    let native = RecordingToolProvider::new("native-provider", CeToolPosture::Native);
    let external = RecordingToolProvider::new("external-provider", CeToolPosture::ExternalOnly);
    let unavailable = RecordingToolProvider::new("plain-provider", CeToolPosture::Unavailable);

    let native_support = tool_calling_support(&native);
    assert_eq!(native_support.provider_name(), "native-provider");
    assert_eq!(native_support.posture(), CeToolPosture::Native);
    assert!(native_support.available());
    assert!(supports_tool_calling(&native));

    let external_support = tool_calling_support(&external);
    assert_eq!(external_support.posture(), CeToolPosture::ExternalOnly);
    assert!(external_support.available());
    assert!(supports_tool_calling(&external));

    let unavailable_support = tool_calling_support(&unavailable);
    assert_eq!(unavailable_support.posture(), CeToolPosture::Unavailable);
    assert!(!unavailable_support.available());
    assert!(!supports_tool_calling(&unavailable));

    let support_debug = format!("{native_support:?} {external_support:?} {unavailable_support:?}");
    for forbidden in [
        "internal",
        "channel",
        "catalog",
        "pro_routing",
        "nxuskit_engine",
    ] {
        assert!(
            !support_debug.to_lowercase().contains(forbidden),
            "CE support debug leaked forbidden detail: {forbidden}"
        );
    }
}

#[test]
fn unavailable_provider_rejects_external_tool_without_internal_detail() {
    let mut provider = RecordingToolProvider::new("plain-provider", CeToolPosture::Unavailable);

    let err = register_external_tool(
        &mut provider,
        ExternalTool::new(
            "echo",
            "Echo",
            json!({ "type": "object" }),
            ToolResult::success,
        ),
    )
    .expect_err("unavailable provider rejects external tools");

    assert_eq!(err, CeFacadeError::UnsupportedByProvider);

    let rendered = format!("{err:?} {err}");
    for forbidden in [
        "channel",
        "tool_catalog",
        "internal",
        "pro_",
        "nxuskit_engine",
    ] {
        assert!(
            !rendered.to_lowercase().contains(forbidden),
            "CE facade error leaked forbidden detail: {forbidden}"
        );
    }
}

#[test]
fn external_tool_registration_validates_parameter_schema_shape() {
    let mut provider = RecordingToolProvider::new("loopback", CeToolPosture::ExternalOnly);

    register_external_tool(
        &mut provider,
        ExternalTool::new(
            "extract",
            "Extract typed values",
            json!({
                "type": "object",
                "properties": {
                    "code": {
                        "type": "string",
                        "enum": ["A", "B"]
                    },
                    "confidence": {
                        "type": ["number", "null"]
                    }
                },
                "required": ["code"],
                "additionalProperties": false
            }),
            ToolResult::success,
        ),
    )
    .expect("well-formed object parameter schema registers");
}

#[test]
fn external_tool_registration_rejects_non_object_parameter_schema() {
    let mut provider = RecordingToolProvider::new("loopback", CeToolPosture::ExternalOnly);

    let err = register_external_tool(
        &mut provider,
        ExternalTool::new(
            "bad",
            "Bad parameter schema",
            json!("not-a-schema-object"),
            ToolResult::success,
        ),
    )
    .expect_err("non-object parameter schema is rejected");

    assert!(matches!(err, CeFacadeError::Contract(_)));
    assert!(
        err.to_string().contains("JSON object schema"),
        "error should identify the parameter-schema shape, got {err}"
    );
}

#[test]
fn external_tool_registration_rejects_malformed_schema_keywords() {
    let mut provider = RecordingToolProvider::new("loopback", CeToolPosture::ExternalOnly);

    let malformed = [
        json!({
            "type": "string",
            "properties": {}
        }),
        json!({
            "type": "object",
            "properties": {
                "code": "string"
            }
        }),
        json!({
            "type": "object",
            "properties": {},
            "required": ["code", 42]
        }),
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": "no"
        }),
    ];

    for schema in malformed {
        let err = register_external_tool(
            &mut provider,
            ExternalTool::new("bad", "Bad parameter schema", schema, ToolResult::success),
        )
        .expect_err("malformed schema keyword is rejected");

        assert!(
            matches!(err, CeFacadeError::Contract(_)),
            "malformed schema must return a CE contract error, got {err:?}"
        );
    }
}
