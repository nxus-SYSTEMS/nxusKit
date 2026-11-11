use nxuskit::tools::{CeFacadeError, CeToolPosture, ExternalTool, ToolCallingProvider, ToolResult};
use nxuskit::{
    AsyncProvider, ChatRequest, ChatResponse, FinishReason, Message, ModelInfo, NxuskitError, Role,
    StreamChunk, StreamReceiver, TokenUsage, ToolChoice, ToolDefinition,
};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug)]
struct ProviderNativeRecorder {
    requests: Mutex<Vec<ChatRequest>>,
    tool_calls: Vec<Value>,
}

impl ProviderNativeRecorder {
    fn new(tool_calls: Vec<Value>) -> Self {
        Self {
            requests: Mutex::new(Vec::new()),
            tool_calls,
        }
    }

    fn requests(&self) -> Vec<ChatRequest> {
        self.requests.lock().expect("request lock poisoned").clone()
    }
}

#[async_trait::async_trait]
impl AsyncProvider for ProviderNativeRecorder {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, NxuskitError> {
        self.requests
            .lock()
            .expect("request lock poisoned")
            .push(request);

        Ok(ChatResponse {
            provider_metadata: None,
            content: String::new(),
            model: "native-model".to_string(),
            provider: "native-recorder".to_string(),
            usage: TokenUsage::default(),
            finish_reason: Some(FinishReason::ToolCalls),
            raw_finish_reason: None,
            metadata: HashMap::new(),
            warnings: vec![],
            logprobs: None,
            tool_calls: Some(self.tool_calls.clone()),
            inference_metadata: None,
        })
    }

    fn chat_stream(&self, _request: ChatRequest) -> Result<StreamReceiver, NxuskitError> {
        Err(NxuskitError::Internal {
            message: "streaming is not needed for provider-native passthrough test".to_string(),
        })
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, NxuskitError> {
        Ok(Vec::new())
    }
}

impl ToolCallingProvider for ProviderNativeRecorder {
    fn supports_tool_calling(&self) -> bool {
        true
    }

    fn provider_name(&self) -> &str {
        "native-recorder"
    }

    fn tool_calling_posture(&self) -> CeToolPosture {
        CeToolPosture::Native
    }

    fn register_external_tool(&mut self, _tool: ExternalTool) -> Result<(), CeFacadeError> {
        Ok(())
    }

    fn invoke_external_tool(
        &self,
        _name: &str,
        _arguments: Value,
    ) -> Result<ToolResult, CeFacadeError> {
        Err(CeFacadeError::Unavailable)
    }
}

#[tokio::test]
async fn provider_native_tool_call_request_and_response_pass_through_unchanged() {
    let tool = serde_json::to_value(
        ToolDefinition::function("lookup")
            .description("Lookup one value")
            .parameters(json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string" }
                },
                "required": ["key"]
            }))
            .build(),
    )
    .expect("tool definition serializes");
    let tool_choice = serde_json::to_value(ToolChoice::named("lookup")).expect("choice serializes");
    let provider_tool_call = json!({
        "id": "call-001",
        "type": "function",
        "function": {
            "name": "lookup",
            "arguments": "{\"key\":\"abc\"}"
        }
    });

    let provider = ProviderNativeRecorder::new(vec![provider_tool_call.clone()]);
    let request = ChatRequest::new("native-model")
        .with_message(Message {
            role: Role::User,
            content: "Call lookup".into(),
        })
        .with_tools(vec![tool.clone()])
        .with_tool_choice(tool_choice.clone());

    let response = provider
        .chat(request)
        .await
        .expect("provider chat succeeds");

    let recorded = provider.requests();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].tools, Some(vec![tool]));
    assert_eq!(recorded[0].tool_choice, Some(tool_choice));
    assert_eq!(response.tool_calls, Some(vec![provider_tool_call]));
}

#[test]
fn provider_native_streaming_tool_call_chunks_remain_provider_shaped() {
    let chunk = StreamChunk {
        provider_metadata: None,
        delta: String::new(),
        index: 0,
        thinking: None,
        finish_reason: Some(FinishReason::ToolCalls),
        raw_finish_reason: None,
        terminal: false,
        usage: None,
        tool_calls: Some(vec![json!({
            "index": 0,
            "id": "call-001",
            "type": "function",
            "function": {
                "name": "lookup",
                "arguments": "{\"key\":\"abc\"}"
            }
        })]),
        logprobs: None,
    };

    let json = serde_json::to_string(&chunk).expect("chunk serializes");
    let parsed: StreamChunk = serde_json::from_str(&json).expect("chunk deserializes");

    assert!(parsed.has_tool_calls());
    assert_eq!(parsed.tool_calls, chunk.tool_calls);
}
