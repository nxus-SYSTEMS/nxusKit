//! `nxuskit-cli call` — Machine-facing LLM invocation (FR-001, FR-002).

use clap::Args;
use serde::{Deserialize, Serialize};

use crate::cli_error::CliError;
use crate::envelope::TraceFields;
use crate::output::{OutputWriter, UsageInfo};

#[derive(Debug, Args)]
pub struct CallArgs {
    /// Input file or `-` for stdin.
    ///
    /// JSON: {"prompt": "...", "provider": "...", "model": "...",
    /// "tool_definitions": [...], "max_tokens": 1024}
    #[arg(short, long)]
    pub input: String,

    /// Output format (json, yaml, jsonl, text)
    #[arg(short, long, default_value = "json")]
    pub format: String,

    /// Enable streaming mode (JSONL output)
    #[arg(short, long, default_value_t = false)]
    pub stream: bool,

    /// Provider to use
    #[arg(long)]
    pub provider: Option<String>,

    /// Model to use
    #[arg(long)]
    pub model: Option<String>,

    /// Suppress non-essential output
    #[arg(short, long, default_value_t = false)]
    pub quiet: bool,

    /// Output file path or `-` for stdout
    #[arg(short, long)]
    pub output: Option<String>,

    /// Image URL to include in the request as a vision content part
    #[arg(long, conflicts_with = "image_file")]
    pub image_url: Option<String>,

    /// Image file path to base64-encode and include as a vision content part.
    /// Supported formats: png, jpg, jpeg, gif, webp. Max 20MB.
    #[arg(long, conflicts_with = "image_url")]
    pub image_file: Option<String>,
}

/// Input schema for the `call` command.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct CallInput {
    pub prompt: Option<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub system: Option<String>,
    pub messages: Option<Vec<serde_json::Value>>,
    pub tool_definitions: Option<Vec<serde_json::Value>>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub stream: Option<bool>,
    #[serde(default)]
    pub tool_choice: Option<serde_json::Value>,
    #[serde(default)]
    pub response_format: Option<serde_json::Value>,
    #[serde(default)]
    pub thinking_mode: Option<String>,
}

/// Result for the `call` command.
#[derive(Debug, Serialize)]
pub struct CallResult {
    pub content: String,
    pub model: String,
    pub provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_definitions_count: Option<u32>,
    pub inference_metadata: CallInferenceMetadata,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warnings: Option<Vec<serde_json::Value>>,
}

/// Inference metadata included in call results.
#[derive(Debug, Serialize)]
pub struct CallInferenceMetadata {
    pub model: String,
    pub provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

/// Parse a JSON value into a `ResponseFormat` enum variant.
///
/// # Supported formats
/// - `{"type":"text"}` → `ResponseFormat::Text`
/// - `{"type":"json_object"}` → `ResponseFormat::Json`
/// - `{"type":"json_schema","schema":{...}}` → `ResponseFormat::JsonSchema { schema }`
///
/// # Errors
/// Returns `CliError::ParseError` for unknown type values or missing fields.
pub fn parse_response_format(
    value: serde_json::Value,
) -> Result<nxuskit_engine::prelude::ResponseFormat, CliError> {
    let type_str =
        value
            .get("type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| CliError::ParseError {
                message: "response_format must have a \"type\" field".to_string(),
            })?;
    match type_str {
        "text" => Ok(nxuskit_engine::prelude::ResponseFormat::Text),
        "json_object" => Ok(nxuskit_engine::prelude::ResponseFormat::Json),
        "json_schema" => {
            let schema = value
                .get("schema")
                .cloned()
                .ok_or_else(|| CliError::ParseError {
                    message: "response_format type \"json_schema\" requires a \"schema\" field"
                        .to_string(),
                })?;
            Ok(nxuskit_engine::prelude::ResponseFormat::JsonSchema { schema })
        }
        other => Err(CliError::ParseError {
            message: format!(
                "unknown response_format type: \"{other}\". Expected \"text\", \"json_object\", or \"json_schema\""
            ),
        }),
    }
}

/// Parse a string into a `ThinkingMode` enum variant.
///
/// # Supported values
/// - `"auto"` → `ThinkingMode::Auto`
/// - `"enabled"` → `ThinkingMode::Enabled`
/// - `"disabled"` → `ThinkingMode::Disabled`
/// - `"omit"` → `ThinkingMode::Omit`
///
/// # Errors
/// Returns `CliError::ParseError` for unknown strings.
pub fn parse_thinking_mode(s: &str) -> Result<nxuskit_engine::prelude::ThinkingMode, CliError> {
    match s {
        "auto" => Ok(nxuskit_engine::prelude::ThinkingMode::Auto),
        "enabled" => Ok(nxuskit_engine::prelude::ThinkingMode::Enabled),
        "disabled" => Ok(nxuskit_engine::prelude::ThinkingMode::Disabled),
        "omit" => Ok(nxuskit_engine::prelude::ThinkingMode::Omit),
        other => Err(CliError::ParseError {
            message: format!(
                "unknown thinking_mode: \"{other}\". Expected \"auto\", \"enabled\", \"disabled\", or \"omit\""
            ),
        }),
    }
}

/// Parse a JSON array of message objects into `Vec<Message>`.
///
/// Each message must have a `role` field (`"user"`, `"assistant"`, `"system"`)
/// and a `content` field that is either a string (simple text) or an array of
/// content parts (multimodal).
///
/// # Content part types
/// - `{"type":"text","text":"..."}` → `ContentPart::Text`
/// - `{"type":"image","url":"..."}` → `ContentPart::Image` with `ImageData::Url`
/// - `{"type":"image","base64":"...","media_type":"..."}` → `ContentPart::Image` with `ImageData::Base64`
///
/// # Errors
/// Returns `CliError::ParseError` for missing fields, unknown roles, or unrecognized content part types.
pub fn parse_messages(
    messages: &[serde_json::Value],
) -> Result<Vec<nxuskit_engine::prelude::Message>, CliError> {
    use nxuskit_engine::types::{ContentPart, ImageData, ImageSource, MessageContent};

    let mut result = Vec::with_capacity(messages.len());
    for msg in messages {
        let role_str =
            msg.get("role")
                .and_then(|v| v.as_str())
                .ok_or_else(|| CliError::ParseError {
                    message: "each message must have a \"role\" field".to_string(),
                })?;
        let role = match role_str {
            "user" => nxuskit_engine::prelude::Role::User,
            "assistant" => nxuskit_engine::prelude::Role::Assistant,
            "system" => nxuskit_engine::prelude::Role::System,
            other => {
                return Err(CliError::ParseError {
                    message: format!(
                        "unknown message role: \"{other}\". Expected \"user\", \"assistant\", or \"system\""
                    ),
                });
            }
        };

        let content_val = msg.get("content").ok_or_else(|| CliError::ParseError {
            message: "each message must have a \"content\" field".to_string(),
        })?;

        let message = if let Some(text) = content_val.as_str() {
            nxuskit_engine::prelude::Message {
                role,
                content: MessageContent::Text(text.to_string()),
            }
        } else if let Some(parts_arr) = content_val.as_array() {
            let mut parts = Vec::with_capacity(parts_arr.len());
            for part in parts_arr {
                let part_type = part.get("type").and_then(|v| v.as_str()).ok_or_else(|| {
                    CliError::ParseError {
                        message: "each content part must have a \"type\" field".to_string(),
                    }
                })?;
                match part_type {
                    "text" => {
                        let text = part.get("text").and_then(|v| v.as_str()).ok_or_else(|| {
                            CliError::ParseError {
                                message: "text content part must have a \"text\" field".to_string(),
                            }
                        })?;
                        parts.push(ContentPart::Text {
                            text: text.to_string(),
                        });
                    }
                    "image" => {
                        let source = if let Some(url) = part.get("url").and_then(|v| v.as_str()) {
                            ImageSource {
                                data: ImageData::Url {
                                    url: url.to_string(),
                                },
                                detail: None,
                            }
                        } else if let Some(b64) = part.get("base64").and_then(|v| v.as_str()) {
                            let media_type = part
                                .get("media_type")
                                .and_then(|v| v.as_str())
                                .ok_or_else(|| CliError::ParseError {
                                    message: "base64 image part must have a \"media_type\" field"
                                        .to_string(),
                                })?;
                            ImageSource {
                                data: ImageData::Base64 {
                                    media_type: media_type.to_string(),
                                    data: b64.to_string(),
                                },
                                detail: None,
                            }
                        } else {
                            return Err(CliError::ParseError {
                                message: "image content part must have either \"url\" or \"base64\" field".to_string(),
                            });
                        };
                        parts.push(ContentPart::Image { source });
                    }
                    other => {
                        return Err(CliError::ParseError {
                            message: format!(
                                "unrecognized content part type: \"{other}\". Expected \"text\" or \"image\""
                            ),
                        });
                    }
                }
            }
            nxuskit_engine::prelude::Message {
                role,
                content: MessageContent::Parts(parts),
            }
        } else {
            return Err(CliError::ParseError {
                message: "message content must be a string or an array of content parts"
                    .to_string(),
            });
        };

        result.push(message);
    }
    Ok(result)
}

pub async fn run_call(args: CallArgs) -> Result<(), CliError> {
    use crate::output::OutputFormat;
    use futures::StreamExt;

    let format = OutputFormat::parse(&args.format)?;
    let raw_input = OutputWriter::read_input(&args.input)?;

    // T038: Replay detection — if input is a saved ResponseEnvelope, extract original request
    let (raw_input, _replayed_from) = detect_replay(&raw_input);

    let call_input: CallInput =
        serde_json::from_str(&raw_input).map_err(|e| CliError::ParseError {
            message: format!("Invalid call input JSON: {e}"),
        })?;

    // Determine provider
    let provider_name = args
        .provider
        .as_deref()
        .or(call_input.provider.as_deref())
        .or_else(|| {
            std::env::var("NXUSKIT_PROVIDER")
                .ok()
                .as_deref()
                .map(|_| unreachable!())
        })
        .unwrap_or("loopback");

    // Resolve provider name from env if args/input didn't specify
    let provider_name = if args.provider.is_some() || call_input.provider.is_some() {
        provider_name.to_string()
    } else {
        std::env::var("NXUSKIT_PROVIDER").unwrap_or_else(|_| provider_name.to_string())
    };

    let provider_impl =
        crate::create_provider(&provider_name).map_err(|e| CliError::ProviderError {
            message: format!("{e}"),
        })?;

    let model_name = args
        .model
        .as_deref()
        .or(call_input.model.as_deref())
        .unwrap_or("default");

    let mut request = nxuskit_engine::prelude::ChatRequest::new(model_name);

    if let Some(prompt) = &call_input.prompt {
        request = request.with_message(nxuskit_engine::prelude::Message::user(prompt));
    }

    // T035: Use parse_messages for multimodal support
    if let Some(messages) = &call_input.messages {
        let parsed = parse_messages(messages)?;
        request = request.with_messages(parsed);
    }

    if let Some(system) = &call_input.system {
        request = request.with_message(nxuskit_engine::prelude::Message::system(system));
    }

    if let Some(temp) = call_input.temperature {
        request = request.with_temperature(temp);
    }

    if let Some(tokens) = call_input.max_tokens {
        request = request.with_max_tokens(tokens);
    }

    // T026: Propagate tool definitions
    if let Some(tool_defs) = &call_input.tool_definitions {
        request.tools = Some(tool_defs.clone());
    }

    // T019: Forward tool_choice
    if let Some(tc) = call_input.tool_choice {
        request.tool_choice = Some(tc);
    }

    // T020: Forward response_format
    if let Some(rf) = call_input.response_format {
        request.response_format = Some(parse_response_format(rf)?);
    }

    // T021: Forward thinking_mode
    if let Some(ref tm) = call_input.thinking_mode {
        request = request.with_thinking_mode(parse_thinking_mode(tm)?);
    }

    // T036: Handle --image-url flag
    if let Some(ref url) = args.image_url {
        use nxuskit_engine::types::{ContentPart, ImageData, ImageSource};
        let image_part = ContentPart::Image {
            source: ImageSource {
                data: ImageData::Url { url: url.clone() },
                detail: None,
            },
        };
        append_image_part(&mut request, image_part);
    }

    // T037: Handle --image-file flag
    if let Some(ref path) = args.image_file {
        use base64::Engine;
        use nxuskit_engine::types::{ContentPart, ImageData, ImageSource};

        if path == "-" {
            return Err(CliError::ParseError {
                message: "stdin not supported for --image-file; use a file path".to_string(),
            });
        }

        let ext = std::path::Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase());
        let media_type = match ext.as_deref() {
            Some("png") => "image/png",
            Some("jpg") | Some("jpeg") => "image/jpeg",
            Some("gif") => "image/gif",
            Some("webp") => "image/webp",
            _ => {
                return Err(CliError::ParseError {
                    message:
                        "unrecognized image extension. Supported formats: png, jpg, jpeg, gif, webp"
                            .to_string(),
                });
            }
        };

        let metadata = std::fs::metadata(path).map_err(|e| CliError::ParseError {
            message: format!("cannot read image file: {e}"),
        })?;
        const MAX_SIZE: u64 = 20 * 1024 * 1024;
        if metadata.len() > MAX_SIZE {
            return Err(CliError::ParseError {
                message: format!(
                    "image file size ({:.1}MB) exceeds 20MB limit",
                    metadata.len() as f64 / (1024.0 * 1024.0)
                ),
            });
        }

        let bytes = std::fs::read(path).map_err(|e| CliError::ParseError {
            message: format!("cannot read image file: {e}"),
        })?;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        let image_part = ContentPart::Image {
            source: ImageSource {
                data: ImageData::Base64 {
                    media_type: media_type.to_string(),
                    data: b64,
                },
                detail: None,
            },
        };
        append_image_part(&mut request, image_part);
    }

    let trace = TraceFields::new("call", &raw_input, Some(&provider_name), Some(model_name));

    let should_stream = args.stream || call_input.stream.unwrap_or(false);
    let writer = OutputWriter::new(format, args.quiet, args.output.clone());

    if should_stream {
        use crate::output::JsonlEvent;

        let mut stream =
            provider_impl
                .chat_stream(&request)
                .await
                .map_err(|e| CliError::ProviderError {
                    message: format!("{e}"),
                })?;

        let timeout_duration = std::time::Duration::from_secs(30);
        let mut full_content = String::new();

        loop {
            match tokio::time::timeout(timeout_duration, stream.next()).await {
                Ok(Some(Ok(chunk))) => {
                    if !chunk.delta.is_empty() {
                        full_content.push_str(&chunk.delta);
                        let event = JsonlEvent {
                            event_type: "chunk".to_string(),
                            data: serde_json::json!({ "content": chunk.delta }),
                            trace_id: Some(trace.trace_id.clone()),
                        };
                        writer.write_jsonl_event(&event)?;
                    }
                    if chunk.is_final() {
                        break;
                    }
                }
                Ok(Some(Err(e))) => {
                    return Err(CliError::ProviderError {
                        message: format!("{e}"),
                    });
                }
                Ok(None) => break,
                Err(_) => return Err(CliError::IdleTimeout),
            }
        }

        // Emit summary event
        let summary = JsonlEvent {
            event_type: "summary".to_string(),
            data: serde_json::json!({
                "content": full_content,
                "model": model_name,
                "provider": provider_name,
                "finish_reason": "stop",
            }),
            trace_id: Some(trace.trace_id.clone()),
        };
        writer.write_jsonl_event(&summary)?;
    } else {
        let start = std::time::Instant::now();
        let response = provider_impl
            .chat(&request)
            .await
            .map_err(|e| CliError::ProviderError {
                message: format!("{e}"),
            })?;
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;

        let best_usage = response.usage.best_available();
        let usage = UsageInfo {
            input_tokens: best_usage.prompt_tokens,
            output_tokens: best_usage.completion_tokens,
            total_tokens: best_usage.total(),
        };

        let tool_defs_count = call_input.tool_definitions.as_ref().map(|t| t.len() as u32);

        // T022: Serialize warnings from provider response
        let warnings = if response.warnings.is_empty() {
            None
        } else {
            Some(
                response
                    .warnings
                    .iter()
                    .map(|w| serde_json::to_value(w).unwrap_or_default())
                    .collect(),
            )
        };

        let result = CallResult {
            content: response.content.clone(),
            model: response.model.clone(),
            provider: provider_name.to_string(),
            tool_calls: response.tool_calls.clone(),
            tool_definitions_count: tool_defs_count,
            inference_metadata: CallInferenceMetadata {
                model: response.model.clone(),
                provider: provider_name.to_string(),
                finish_reason: response.finish_reason.as_ref().map(|r| format!("{:?}", r)),
            },
            warnings,
        };

        writer.write_response(
            result,
            trace,
            Some(usage),
            Some(
                response
                    .finish_reason
                    .map(|r| format!("{:?}", r))
                    .unwrap_or_else(|| "stop".to_string()),
            ),
            Some(elapsed),
        )?;
    }

    Ok(())
}

/// Append an image content part to the last user message in a `ChatRequest`,
/// or create a new user message if no user message exists.
fn append_image_part(
    request: &mut nxuskit_engine::prelude::ChatRequest,
    image_part: nxuskit_engine::types::ContentPart,
) {
    use nxuskit_engine::types::{ContentPart, MessageContent};

    // Find the last user message and convert to multimodal
    if let Some(last_user) = request
        .messages
        .iter_mut()
        .rev()
        .find(|m| matches!(m.role, nxuskit_engine::prelude::Role::User))
    {
        match &mut last_user.content {
            MessageContent::Text(text) => {
                let text_part = ContentPart::Text {
                    text: std::mem::take(text),
                };
                last_user.content = MessageContent::Parts(vec![text_part, image_part]);
            }
            MessageContent::Parts(parts) => {
                parts.push(image_part);
            }
        }
    } else {
        // No user message — create one with just the image
        request.messages.push(nxuskit_engine::prelude::Message {
            role: nxuskit_engine::prelude::Role::User,
            content: MessageContent::Parts(vec![image_part]),
        });
    }
}

/// Detect if input is a saved ResponseEnvelope and extract the original request.
/// Returns (input_to_use, optional_replayed_from_trace_id).
fn detect_replay(raw_input: &str) -> (String, Option<String>) {
    if let Ok(envelope) = serde_json::from_str::<serde_json::Value>(raw_input)
        && envelope.get("trace_id").is_some()
        && envelope.get("request_metadata").is_some()
        && envelope["request_metadata"]["command"].as_str() == Some("call")
    {
        // This is a saved ResponseEnvelope — extract a replay request
        let trace_id = envelope["trace_id"].as_str().unwrap_or("").to_string();
        let result = &envelope["result"];
        // Build a minimal CallInput from the metadata
        let provider = envelope["request_metadata"]["provider"]
            .as_str()
            .unwrap_or("loopback");
        let model = envelope["request_metadata"]["model"]
            .as_str()
            .unwrap_or("default");
        let content = result["content"].as_str().unwrap_or("");

        let replay_input = serde_json::json!({
            "prompt": format!("(replayed) {}", content),
            "provider": provider,
            "model": model,
        });
        return (replay_input.to_string(), Some(trace_id));
    }
    (raw_input.to_string(), None)
}

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;

    // ── T009: parse_response_format text ──────────────────────────────
    #[test]
    fn test_parse_response_format_text() {
        let val = serde_json::json!({"type": "text"});
        let result = parse_response_format(val).unwrap();
        assert!(matches!(
            result,
            nxuskit_engine::prelude::ResponseFormat::Text
        ));
    }

    // ── T010: parse_response_format json_object ──────────────────────
    #[test]
    fn test_parse_response_format_json_object() {
        let val = serde_json::json!({"type": "json_object"});
        let result = parse_response_format(val).unwrap();
        assert!(matches!(
            result,
            nxuskit_engine::prelude::ResponseFormat::Json
        ));
    }

    // ── T011: parse_response_format json_schema ──────────────────────
    #[test]
    fn test_parse_response_format_json_schema() {
        let val = serde_json::json!({"type": "json_schema", "schema": {"type": "object"}});
        let result = parse_response_format(val).unwrap();
        match result {
            nxuskit_engine::prelude::ResponseFormat::JsonSchema { schema } => {
                assert_eq!(schema, serde_json::json!({"type": "object"}));
            }
            other => panic!("expected JsonSchema, got {:?}", other),
        }
    }

    // ── T012: parse_response_format invalid ───────────────────────���──
    #[test]
    fn test_parse_response_format_invalid() {
        let val = serde_json::json!({"type": "unknown"});
        let err = parse_response_format(val).unwrap_err();
        match err {
            CliError::ParseError { message } => {
                assert!(
                    message.contains("unknown"),
                    "message should mention unknown type: {message}"
                );
            }
            other => panic!("expected ParseError, got {:?}", other),
        }
    }

    // ── T013: parse_thinking_mode variants ───────────────────────────
    #[test]
    fn test_parse_thinking_mode_auto() {
        let result = parse_thinking_mode("auto").unwrap();
        assert!(matches!(
            result,
            nxuskit_engine::prelude::ThinkingMode::Auto
        ));
    }

    #[test]
    fn test_parse_thinking_mode_enabled() {
        let result = parse_thinking_mode("enabled").unwrap();
        assert!(matches!(
            result,
            nxuskit_engine::prelude::ThinkingMode::Enabled
        ));
    }

    #[test]
    fn test_parse_thinking_mode_disabled() {
        let result = parse_thinking_mode("disabled").unwrap();
        assert!(matches!(
            result,
            nxuskit_engine::prelude::ThinkingMode::Disabled
        ));
    }

    #[test]
    fn test_parse_thinking_mode_omit() {
        let result = parse_thinking_mode("omit").unwrap();
        assert!(matches!(
            result,
            nxuskit_engine::prelude::ThinkingMode::Omit
        ));
    }

    #[test]
    fn test_parse_thinking_mode_invalid() {
        let err = parse_thinking_mode("turbo").unwrap_err();
        match err {
            CliError::ParseError { message } => {
                assert!(
                    message.contains("turbo"),
                    "message should mention the invalid value: {message}"
                );
            }
            other => panic!("expected ParseError, got {:?}", other),
        }
    }

    // ── T024: parse_messages simple text ─────────────────────────────
    #[test]
    fn test_parse_messages_simple_text() {
        use nxuskit_engine::types::MessageContent;
        let msgs = vec![serde_json::json!({"role": "user", "content": "hello"})];
        let result = parse_messages(&msgs).unwrap();
        assert_eq!(result.len(), 1);
        assert!(matches!(
            result[0].role,
            nxuskit_engine::prelude::Role::User
        ));
        match &result[0].content {
            MessageContent::Text(t) => assert_eq!(t, "hello"),
            other => panic!("expected Text, got {:?}", other),
        }
    }

    // ── T025: parse_messages multimodal URL ─────────────────────────
    #[test]
    fn test_parse_messages_multimodal_url() {
        use nxuskit_engine::types::{ContentPart, ImageData, MessageContent};
        let msgs = vec![serde_json::json!({
            "role": "user",
            "content": [
                {"type": "text", "text": "describe this"},
                {"type": "image", "url": "https://example.com/photo.jpg"}
            ]
        })];
        let result = parse_messages(&msgs).unwrap();
        assert_eq!(result.len(), 1);
        match &result[0].content {
            MessageContent::Parts(parts) => {
                assert_eq!(parts.len(), 2);
                match &parts[0] {
                    ContentPart::Text { text } => assert_eq!(text, "describe this"),
                    other => panic!("expected Text part, got {:?}", other),
                }
                match &parts[1] {
                    ContentPart::Image { source } => match &source.data {
                        ImageData::Url { url } => assert_eq!(url, "https://example.com/photo.jpg"),
                        other => panic!("expected Url, got {:?}", other),
                    },
                    other => panic!("expected Image part, got {:?}", other),
                }
            }
            other => panic!("expected Parts, got {:?}", other),
        }
    }

    // ── T026: parse_messages multimodal base64 ──────────────────────
    #[test]
    fn test_parse_messages_multimodal_base64() {
        use nxuskit_engine::types::{ContentPart, ImageData, MessageContent};
        let msgs = vec![serde_json::json!({
            "role": "user",
            "content": [
                {"type": "text", "text": "what is this?"},
                {"type": "image", "base64": "iVBORw0KGgo=", "media_type": "image/png"}
            ]
        })];
        let result = parse_messages(&msgs).unwrap();
        match &result[0].content {
            MessageContent::Parts(parts) => {
                assert_eq!(parts.len(), 2);
                match &parts[1] {
                    ContentPart::Image { source } => match &source.data {
                        ImageData::Base64 { media_type, data } => {
                            assert_eq!(media_type, "image/png");
                            assert_eq!(data, "iVBORw0KGgo=");
                        }
                        other => panic!("expected Base64, got {:?}", other),
                    },
                    other => panic!("expected Image part, got {:?}", other),
                }
            }
            other => panic!("expected Parts, got {:?}", other),
        }
    }

    // ── T027: parse_messages invalid content part type ───────────────
    #[test]
    fn test_parse_messages_invalid_content_part() {
        let msgs = vec![serde_json::json!({
            "role": "user",
            "content": [{"type": "video", "url": "https://example.com/vid.mp4"}]
        })];
        let err = parse_messages(&msgs).unwrap_err();
        match err {
            CliError::ParseError { message } => {
                assert!(
                    message.contains("video"),
                    "error should mention unrecognized type: {message}"
                );
            }
            other => panic!("expected ParseError, got {:?}", other),
        }
    }
}
