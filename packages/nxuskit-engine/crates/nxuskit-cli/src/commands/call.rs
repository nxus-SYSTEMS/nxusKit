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
    #[serde(default, alias = "tools")]
    pub tool_definitions: Option<Vec<serde_json::Value>>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub stream: Option<bool>,
    #[serde(default, alias = "base_url")]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub top_k: Option<i64>,
    #[serde(default)]
    pub min_p: Option<f64>,
    #[serde(default)]
    pub seed: Option<i64>,
    #[serde(default)]
    pub presence_penalty: Option<f32>,
    #[serde(default)]
    pub frequency_penalty: Option<f32>,
    #[serde(default)]
    pub logprobs: Option<bool>,
    #[serde(default)]
    pub top_logprobs: Option<u8>,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    #[serde(default)]
    pub tool_choice: Option<serde_json::Value>,
    #[serde(default)]
    pub response_format: Option<serde_json::Value>,
    #[serde(default, alias = "thinking")]
    pub thinking_mode: Option<String>,
}

/// Result for the `call` command.
#[derive(Debug, Serialize)]
pub struct CallResult {
    pub content: String,
    pub model: String,
    pub provider: String,
    pub provider_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_finish: Option<String>,
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

/// Frozen Spec119 complete-output shell for the `call` command.
///
/// Kept local to the CLI while Action 7 is closing; A7-Q3 owns later
/// consolidation of output/validation mechanics after semantics are frozen.
fn write_call_complete_output(
    format: crate::output::OutputFormat,
    output_path: Option<&str>,
    value: &serde_json::Value,
) -> Result<(), CliError> {
    use std::io::Write;

    let serialized = match format {
        crate::output::OutputFormat::Json => {
            serde_json::to_string_pretty(value).map_err(|e| CliError::ParseError {
                message: format!("Serialization failed: {e}"),
            })?
        }
        crate::output::OutputFormat::Jsonl => {
            serde_json::to_string(value).map_err(|e| CliError::ParseError {
                message: format!("Serialization failed: {e}"),
            })?
        }
        crate::output::OutputFormat::Yaml => {
            serde_yaml_ng::to_string(value).map_err(|e| CliError::ParseError {
                message: format!("YAML serialization failed: {e}"),
            })?
        }
        crate::output::OutputFormat::Text | crate::output::OutputFormat::Human => {
            serde_json::to_string_pretty(&value["response"]).map_err(|e| CliError::ParseError {
                message: format!("Serialization failed: {e}"),
            })?
        }
    };

    match output_path {
        Some(path) if path != "-" => {
            std::fs::write(path, serialized).map_err(|e| CliError::ParseError {
                message: format!("Failed to write '{path}': {e}"),
            })
        }
        _ => {
            let stdout = std::io::stdout();
            let mut writer = stdout.lock();
            writeln!(writer, "{serialized}").map_err(|e| CliError::ParseError {
                message: format!("Write failed: {e}"),
            })
        }
    }
}

fn call_descriptor(
    provider_id: &str,
    model: &str,
    endpoint: Option<&str>,
) -> Result<serde_json::Value, CliError> {
    let endpoint_origin = if let Some(endpoint) = endpoint {
        let config = nxuskit_engine::types::ProviderConfig {
            provider_id: provider_id.to_string(),
            model: model.to_string(),
            endpoint: Some(endpoint.to_string()),
            auth_selector: None,
            transport: nxuskit_engine::types::ProviderTransportMode::Native,
            connect_timeout_ms: 5_000,
            read_timeout_ms: 5_000,
        };
        config
            .endpoint_origin()
            .map_err(CliError::from_contract_error)?
    } else {
        None
    };

    let locality = if matches!(provider_id, "ollama_local" | "lmstudio" | "lm-studio") {
        "local"
    } else {
        "remote"
    };

    Ok(serde_json::json!({
        "provider_id": provider_id,
        "model": model,
        "endpoint_origin": endpoint_origin,
        "locality": locality,
        "transport": "native",
    }))
}

/// Parse a JSON value into a `ResponseFormat` enum variant.
///
/// # Supported formats
/// - `{"type":"text"}` → `ResponseFormat::Text`
/// - `{"type":"json"}` or `{"type":"json_object"}` → `ResponseFormat::Json`
/// - `{"type":"json_schema","schema":{...}}` → `ResponseFormat::JsonSchema { schema }`
///
/// # Errors
/// Returns `CliError::InvalidRequest` for unknown type values or missing fields.
pub fn parse_response_format(
    value: serde_json::Value,
) -> Result<nxuskit_engine::prelude::ResponseFormat, CliError> {
    let type_str =
        value
            .get("type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| CliError::InvalidRequest {
                message: "response_format must have a \"type\" field".to_string(),
            })?;
    match type_str {
        "text" => Ok(nxuskit_engine::prelude::ResponseFormat::Text),
        "json" | "json_object" => Ok(nxuskit_engine::prelude::ResponseFormat::Json),
        "json_schema" => {
            let schema = value
                .get("schema")
                .cloned()
                .ok_or_else(|| CliError::InvalidRequest {
                    message: "response_format type \"json_schema\" requires a \"schema\" field"
                        .to_string(),
                })?;
            Ok(nxuskit_engine::prelude::ResponseFormat::JsonSchema { schema })
        }
        other => Err(CliError::InvalidRequest {
            message: format!(
                "unknown response_format type: \"{other}\". Expected \"text\", \"json\", \"json_object\", or \"json_schema\""
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
/// Returns `CliError::InvalidRequest` for unknown strings.
pub fn parse_thinking_mode(s: &str) -> Result<nxuskit_engine::prelude::ThinkingMode, CliError> {
    match s {
        "auto" => Ok(nxuskit_engine::prelude::ThinkingMode::Auto),
        "enabled" => Ok(nxuskit_engine::prelude::ThinkingMode::Enabled),
        "disabled" => Ok(nxuskit_engine::prelude::ThinkingMode::Disabled),
        "omit" => Ok(nxuskit_engine::prelude::ThinkingMode::Omit),
        other => Err(CliError::InvalidRequest {
            message: format!(
                "unknown thinking_mode: \"{other}\". Expected \"auto\", \"enabled\", \"disabled\", or \"omit\""
            ),
        }),
    }
}

fn prepare_cli_request(
    provider_id: &str,
    mut request: nxuskit_engine::prelude::ChatRequest,
    thinking_mode_explicit: bool,
) -> Result<nxuskit_engine::prelude::ChatRequest, CliError> {
    use nxuskit_engine::prelude::ThinkingMode;

    for (name, value) in [
        ("presence_penalty", request.presence_penalty),
        ("frequency_penalty", request.frequency_penalty),
    ] {
        if value.is_some_and(|value| !value.is_finite() || !(-2.0..=2.0).contains(&value)) {
            return Err(CliError::InvalidRequest {
                message: format!("{name} must be finite and between -2 and 2"),
            });
        }
    }
    if request
        .top_k
        .is_some_and(|value| !(1..=2_147_483_647).contains(&value))
    {
        return Err(CliError::InvalidRequest {
            message: "top_k must be between 1 and 2147483647".to_string(),
        });
    }
    if request
        .min_p
        .is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
    {
        return Err(CliError::InvalidRequest {
            message: "min_p must be finite and between 0 and 1".to_string(),
        });
    }
    if request.top_logprobs.is_some_and(|value| value > 20) {
        return Err(CliError::InvalidRequest {
            message: "top_logprobs must be between 0 and 20".to_string(),
        });
    }
    if let Some(effort) = request.reasoning_effort.as_deref()
        && !matches!(effort, "low" | "medium" | "high" | "max")
    {
        return Err(CliError::InvalidRequest {
            message: "reasoning_effort must be one of low, medium, high, or max".to_string(),
        });
    }
    if let Some(tools) = request.tools.as_ref() {
        for tool in tools {
            let object = tool.as_object().ok_or_else(|| CliError::InvalidRequest {
                message: "tool definitions must be JSON objects".to_string(),
            })?;
            let function = object
                .get("function")
                .and_then(serde_json::Value::as_object)
                .unwrap_or(object);
            let name = function
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            if name.trim().is_empty() {
                return Err(CliError::InvalidRequest {
                    message: "tool names must be nonempty".to_string(),
                });
            }
            if function
                .get("parameters")
                .is_some_and(|value| !value.is_object())
            {
                return Err(CliError::InvalidRequest {
                    message: "tool parameters must be a JSON object".to_string(),
                });
            }
            if object
                .get("type")
                .is_some_and(|value| value.as_str() != Some("function"))
            {
                return Err(CliError::InvalidRequest {
                    message: "tool type must be function".to_string(),
                });
            }
        }
    }

    // Frozen empty-tools semantics: omission and an explicit empty list are
    // the same no-tools request state.
    if request.tools.as_ref().is_some_and(|tools| tools.is_empty()) {
        request.tools = None;
    }

    // Frozen Spec119 input tools are canonical flat definitions; provider
    // wires use the function wrapper for every profile, including Ollama.
    if let Some(tools) = request.tools.as_mut() {
        for tool in tools {
            if tool.get("function").is_some() {
                continue;
            }
            let Some(object) = tool.as_object() else {
                continue;
            };
            let mut function = serde_json::Map::new();
            for key in ["name", "description", "parameters", "strict"] {
                if let Some(value) = object.get(key) {
                    function.insert(key.to_string(), value.clone());
                }
            }
            *tool = serde_json::json!({
                "type": "function",
                "function": serde_json::Value::Object(function),
            });
        }
    }

    let is_ollama = matches!(
        provider_id,
        "ollama" | "ollama_local" | "ollama_cloud_via_daemon"
    );
    if is_ollama {
        if request.presence_penalty.is_some() || request.frequency_penalty.is_some() {
            return Err(CliError::UnsupportedCapability {
                message: format!("{provider_id} does not support penalties"),
            });
        }
        if request.reasoning_effort.is_some() {
            return Err(CliError::UnsupportedCapability {
                message: format!("{provider_id} does not support reasoning_effort"),
            });
        }
    } else {
        if request.top_k.is_some() {
            return Err(CliError::UnsupportedCapability {
                message: format!("{provider_id} does not support top_k"),
            });
        }
        if request.min_p.is_some() {
            return Err(CliError::UnsupportedCapability {
                message: format!("{provider_id} does not support min_p"),
            });
        }
        if thinking_mode_explicit && request.thinking_mode != ThinkingMode::Omit {
            return Err(CliError::UnsupportedCapability {
                message: format!("{provider_id} does not support thinking"),
            });
        }
    }

    Ok(request)
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
        serde_json::from_str(&raw_input).map_err(|e| CliError::InvalidRequest {
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

    request.top_k = call_input.top_k;
    request.min_p = call_input.min_p;
    request.seed = call_input.seed;
    request.presence_penalty = call_input.presence_penalty;
    request.frequency_penalty = call_input.frequency_penalty;
    request.logprobs = call_input.logprobs;
    request.top_logprobs = call_input.top_logprobs;
    request.reasoning_effort = call_input.reasoning_effort.clone();

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
    let thinking_mode_explicit = call_input.thinking_mode.is_some();
    if let Some(ref tm) = call_input.thinking_mode {
        request = request.with_thinking_mode(parse_thinking_mode(tm)?);
    }

    request = prepare_cli_request(&provider_name, request, thinking_mode_explicit)?;

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

    // Construct the provider only after request parsing/validation so malformed
    // requests fail independently of credentials/provider construction.
    let provider_impl = crate::create_provider_with_base_url(
        &provider_name,
        call_input.endpoint.as_deref(),
        Some(model_name),
    )
    .map_err(|e| CliError::ProviderError {
        message: format!("{e}"),
    })?;

    let trace = TraceFields::new("call", &raw_input, Some(&provider_name), Some(model_name));

    let should_stream = args.stream || call_input.stream.unwrap_or(false);
    let writer = OutputWriter::new(format, args.quiet, args.output.clone());

    if should_stream {
        use crate::output::JsonlEvent;

        let mut stream = provider_impl
            .chat_stream(&request)
            .await
            .map_err(CliError::from_provider_error)?;

        let timeout_duration = std::time::Duration::from_secs(30);
        let mut full_content = String::new();
        let mut pending_content: Option<String> = None;
        let mut terminal_content = String::new();
        let mut terminal_finish: Option<String> = None;
        let mut terminal_raw_finish: Option<String> = None;
        let mut terminal_usage: Option<nxuskit_engine::types::TokenUsage> = None;
        let mut terminal_seen = false;

        writer.write_jsonl_event(&JsonlEvent {
            event_type: "start".to_string(),
            data: serde_json::json!({}),
            trace_id: None,
        })?;

        loop {
            match tokio::time::timeout(timeout_duration, stream.next()).await {
                Ok(Some(Ok(chunk))) => {
                    if chunk.is_final() {
                        if !chunk.delta.is_empty() {
                            if let Some(previous) = pending_content.take() {
                                full_content.push_str(&previous);
                                writer.write_jsonl_event(&JsonlEvent {
                                    event_type: "content".to_string(),
                                    data: serde_json::json!({ "delta": previous }),
                                    trace_id: None,
                                })?;
                            }
                            terminal_content = chunk.delta.clone();
                            full_content.push_str(&chunk.delta);
                        } else {
                            terminal_content = pending_content.take().unwrap_or_default();
                            full_content.push_str(&terminal_content);
                        }
                        if let Some(thinking) = chunk.thinking.as_ref()
                            && !thinking.is_empty()
                        {
                            if let Some(previous) = pending_content.take() {
                                full_content.push_str(&previous);
                                writer.write_jsonl_event(&JsonlEvent {
                                    event_type: "content".to_string(),
                                    data: serde_json::json!({ "delta": previous }),
                                    trace_id: None,
                                })?;
                            }
                            writer.write_jsonl_event(&JsonlEvent {
                                event_type: "thinking".to_string(),
                                data: serde_json::json!({ "delta": thinking }),
                                trace_id: None,
                            })?;
                        }
                        terminal_finish = chunk
                            .finish_reason
                            .as_ref()
                            .map(|reason| reason.as_str().to_string());
                        terminal_raw_finish = chunk.raw_finish_reason.clone();
                        terminal_usage = chunk.usage.clone().or(terminal_usage);
                        terminal_seen = true;
                        break;
                    }

                    if !chunk.delta.is_empty()
                        && let Some(previous) = pending_content.replace(chunk.delta.clone())
                    {
                        full_content.push_str(&previous);
                        writer.write_jsonl_event(&JsonlEvent {
                            event_type: "content".to_string(),
                            data: serde_json::json!({ "delta": previous }),
                            trace_id: None,
                        })?;
                    }
                    if let Some(thinking) = chunk.thinking.as_ref()
                        && !thinking.is_empty()
                    {
                        if let Some(previous) = pending_content.take() {
                            full_content.push_str(&previous);
                            writer.write_jsonl_event(&JsonlEvent {
                                event_type: "content".to_string(),
                                data: serde_json::json!({ "delta": previous }),
                                trace_id: None,
                            })?;
                        }
                        writer.write_jsonl_event(&JsonlEvent {
                            event_type: "thinking".to_string(),
                            data: serde_json::json!({ "delta": thinking }),
                            trace_id: None,
                        })?;
                    }
                    if chunk.usage.is_some() {
                        terminal_usage = chunk.usage.clone();
                    }
                }
                Ok(Some(Err(e))) => return Err(CliError::from_provider_error(e)),
                Ok(None) => break,
                Err(_) => return Err(CliError::IdleTimeout),
            }
        }

        if !terminal_seen {
            return Err(CliError::ProviderError {
                message: "provider stream ended without a terminal event".to_string(),
            });
        }

        let usage = terminal_usage.as_ref().map(|usage| {
            let best = usage.best_available();
            serde_json::json!({
                "prompt": best.prompt_tokens,
                "completion": best.completion_tokens,
                "total": best.total(),
            })
        });
        writer.write_jsonl_event(&JsonlEvent {
            event_type: "complete".to_string(),
            data: serde_json::json!({
                "content": terminal_content,
                "canonical_finish": terminal_finish,
                "raw_finish": terminal_raw_finish,
                "usage": usage,
            }),
            trace_id: None,
        })?;
    } else {
        let start = std::time::Instant::now();
        let response = provider_impl
            .chat(&request)
            .await
            .map_err(CliError::from_provider_error)?;
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

        let canonical_finish = response
            .finish_reason
            .as_ref()
            .map(|reason| reason.as_str().to_string());
        let result = CallResult {
            content: response.content.clone(),
            model: response.model.clone(),
            provider: provider_name.to_string(),
            provider_id: provider_name.to_string(),
            raw_finish: response.raw_finish_reason.clone(),
            tool_calls: response.tool_calls.clone(),
            tool_definitions_count: tool_defs_count,
            inference_metadata: CallInferenceMetadata {
                model: response.model.clone(),
                provider: provider_name.to_string(),
                finish_reason: canonical_finish.clone(),
            },
            warnings,
        };

        let complete_output = serde_json::json!({
            "trace_id": trace.trace_id,
            "timestamp": trace.timestamp,
            "request_hash": trace.request_hash,
            "request_metadata": trace.request_metadata,
            "descriptor": call_descriptor(
                &provider_name,
                &response.model,
                call_input.endpoint.as_deref(),
            )?,
            "response": result,
            "usage": {
                "prompt": usage.input_tokens,
                "completion": usage.output_tokens,
                "total": usage.total_tokens,
            },
            "canonical_finish": canonical_finish,
            "raw_finish": response.raw_finish_reason,
            "elapsed_ms": elapsed,
        });
        write_call_complete_output(format, args.output.as_deref(), &complete_output)?;
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

    #[test]
    fn test_parse_response_format_json_canonical_literal() {
        let val = serde_json::json!({"type": "json"});
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
            CliError::InvalidRequest { message } => {
                assert!(
                    message.contains("unknown"),
                    "message should mention unknown type: {message}"
                );
            }
            other => panic!("expected InvalidRequest, got {:?}", other),
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
            CliError::InvalidRequest { message } => {
                assert!(
                    message.contains("turbo"),
                    "message should mention the invalid value: {message}"
                );
            }
            other => panic!("expected InvalidRequest, got {:?}", other),
        }
    }

    // ── T024: parse_messages simple text ─────────────────────────────
    #[test]
    fn test_empty_tools_three_state_cli_r48b() {
        let omitted = nxuskit_engine::prelude::ChatRequest::new("m");
        let omitted = prepare_cli_request("ollama_local", omitted, false).unwrap();
        assert!(omitted.tools.is_none());

        let mut empty = nxuskit_engine::prelude::ChatRequest::new("m");
        empty.tools = Some(vec![]);
        let empty = prepare_cli_request("ollama_local", empty, false).unwrap();
        assert!(empty.tools.is_none());

        let mut nonempty = nxuskit_engine::prelude::ChatRequest::new("m");
        nonempty.tools = Some(vec![serde_json::json!({
            "name": "lookup",
            "parameters": {"type": "object"}
        })]);
        let nonempty = prepare_cli_request("ollama_local", nonempty, false).unwrap();
        assert_eq!(
            nonempty.tools,
            Some(vec![serde_json::json!({
                "type": "function",
                "function": {
                    "name": "lookup",
                    "parameters": {"type": "object"}
                }
            })])
        );
    }

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
