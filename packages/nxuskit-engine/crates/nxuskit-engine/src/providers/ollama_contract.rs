use super::*;

// Spec 119 Sprint 1A: OllamaContractMapper — concrete provider contract mapper
//
// Pure mapper between the canonical `ProviderRequest`/`ProviderResponse` and
// Ollama's native `/api/chat` wire format. See
// `specs/119-v2-python-provider-surface-parity/data-model.md` and the Sprint
// 1A oracle packet under
// `packages/nxuskit-engine/crates/nxuskit-engine/tests/fixtures/provider_parity/`
// for the exact behavior this mapper implements. Independent of
// `OllamaProvider` above: this mapper owns no transport/HTTP state, only DTO
// translation and failure classification.
// ============================================================================

/// Pure `ProviderContractMapper` for Ollama's native `/api/chat` wire
/// format. Accepts only the `ollama_local` and `ollama_cloud_via_daemon`
/// canonical provider IDs.
#[derive(Debug)]
pub struct OllamaContractMapper {
    config: crate::types::ProviderConfig,
}

impl OllamaContractMapper {
    /// Constructs a mapper from a `ProviderConfig`, failing closed on an
    /// invalid config or a `provider_id` other than `ollama_local` /
    /// `ollama_cloud_via_daemon`.
    pub fn new(config: crate::types::ProviderConfig) -> crate::error::ProviderContractResult<Self> {
        config.validate()?;
        match config.provider_id.as_str() {
            "ollama_local" | "ollama_cloud_via_daemon" => Ok(Self { config }),
            other => Err(crate::error::ProviderError::invalid_request(
                "provider_id",
                format!("OllamaContractMapper does not accept provider_id '{other}'"),
            )),
        }
    }

    fn effective_model(&self, request: &crate::types::ProviderRequest) -> String {
        request
            .model
            .clone()
            .unwrap_or_else(|| self.config.model.clone())
    }

    fn locality(&self) -> &'static str {
        if self.config.provider_id == "ollama_cloud_via_daemon" {
            "cloud_via_local_daemon"
        } else {
            "local_daemon"
        }
    }
}

fn ollama_contract_message_role_str(role: crate::types::ProviderMessageRole) -> &'static str {
    match role {
        crate::types::ProviderMessageRole::System => "system",
        crate::types::ProviderMessageRole::User => "user",
        crate::types::ProviderMessageRole::Assistant => "assistant",
    }
}

fn ollama_contract_finish_reason_from_str(raw: &str) -> Option<crate::types::ProviderFinishReason> {
    match raw {
        "stop" => Some(crate::types::ProviderFinishReason::Stop),
        "length" => Some(crate::types::ProviderFinishReason::Length),
        "tool_calls" => Some(crate::types::ProviderFinishReason::ToolCalls),
        "content_filter" => Some(crate::types::ProviderFinishReason::ContentFilter),
        "cancelled" => Some(crate::types::ProviderFinishReason::Cancelled),
        "error" => Some(crate::types::ProviderFinishReason::Error),
        _ => None,
    }
}

// ============================================================================
// Spec 119 Sprint 1B: crate-private execution backend for
// `OllamaProvider::into_contract_facade`. Executes the already-mapped wire
// request exactly once against Ollama's non-streaming `/api/chat` endpoint
// using the provider's own `reqwest::Client` and configured timeout.
// ============================================================================
struct OllamaFacadeBackend {
    client: reqwest::Client,
    endpoint: String,
    timeout: std::time::Duration,
}

#[async_trait]
impl crate::provider::ProviderExecutionBackend for OllamaFacadeBackend {
    async fn execute(
        &self,
        wire: &serde_json::Value,
    ) -> crate::error::ProviderContractResult<crate::provider::ProviderBackendOutcome> {
        let send_result = self
            .client
            .post(&self.endpoint)
            .header("content-type", "application/json")
            .json(wire)
            .timeout(self.timeout)
            .send()
            .await;
        Ok(crate::provider::http::json_backend_outcome(send_result).await)
    }
}

impl OllamaProvider {
    /// Converts this provider into a `ProviderFacade` bound to a
    /// `ProviderConfig`/`OllamaContractMapper` derived from this provider's
    /// own base URL, model, and timeout. Performs no network call.
    pub fn into_contract_facade(
        self,
    ) -> crate::error::ProviderContractResult<crate::provider::ProviderFacade> {
        let capabilities = self.get_capabilities();
        let config = crate::types::ProviderConfig {
            provider_id: "ollama_local".to_string(),
            model: self.default_model.clone(),
            endpoint: Some(self.base_url.clone()),
            auth_selector: None,
            transport: crate::types::ProviderTransportMode::Native,
            connect_timeout_ms: super::super::duration_to_provider_timeout_ms(
                self.connection_timeout,
            ),
            read_timeout_ms: super::super::duration_to_provider_timeout_ms(self.total_timeout),
        };
        let mapper = std::sync::Arc::new(OllamaContractMapper::new(config)?);
        let backend = OllamaFacadeBackend {
            endpoint: format!("{}/api/chat", self.base_url),
            timeout: self.total_timeout,
            client: self.client.clone(),
        };
        let streaming_backend = OllamaStreamingBackend {
            endpoint: format!("{}/api/chat", self.base_url),
            timeout: self.total_timeout,
            client: self.client,
        };
        Ok(crate::provider::ProviderFacade::new_with_streaming(
            mapper.clone(),
            std::sync::Arc::new(backend),
            mapper,
            std::sync::Arc::new(streaming_backend),
            capabilities,
        ))
    }
}

impl crate::provider::ProviderContractMapper for OllamaContractMapper {
    fn request_to_wire(
        &self,
        request: &crate::types::ProviderRequest,
    ) -> crate::error::ProviderContractResult<serde_json::Value> {
        use crate::error::ProviderError;
        use crate::types::{ProviderResponseFormat, ProviderThinkingControl};

        let effective_model = self.effective_model(request);
        let mut wire = serde_json::Map::new();
        wire.insert("model".to_string(), serde_json::json!(effective_model));

        let messages: Vec<serde_json::Value> = request
            .messages
            .iter()
            .map(|m| {
                serde_json::json!({
                    "role": ollama_contract_message_role_str(m.role),
                    "content": m.content,
                })
            })
            .collect();
        wire.insert("messages".to_string(), serde_json::Value::Array(messages));
        wire.insert("stream".to_string(), serde_json::json!(false));

        match &request.response_format {
            None | Some(ProviderResponseFormat::Text) => {}
            Some(ProviderResponseFormat::Json) => {
                wire.insert("format".to_string(), serde_json::json!("json"));
            }
            Some(ProviderResponseFormat::JsonSchema { schema }) => {
                wire.insert(
                    "format".to_string(),
                    serde_json::Value::Object(schema.clone()),
                );
            }
        }

        match request.thinking {
            None => {}
            Some(ProviderThinkingControl::Auto) => {
                if OllamaProvider::is_thinking_model(&effective_model) {
                    wire.insert("think".to_string(), serde_json::json!(true));
                }
            }
            Some(ProviderThinkingControl::Enabled) => {
                wire.insert("think".to_string(), serde_json::json!(true));
            }
            Some(ProviderThinkingControl::Disabled) => {
                wire.insert("think".to_string(), serde_json::json!(false));
            }
        }

        if request.presence_penalty.is_some() {
            return Err(ProviderError::unsupported_capability(
                "presence_penalty",
                "Ollama does not support presence_penalty",
            ));
        }
        if request.frequency_penalty.is_some() {
            return Err(ProviderError::unsupported_capability(
                "frequency_penalty",
                "Ollama does not support frequency_penalty",
            ));
        }
        if request.reasoning_effort.is_some() {
            return Err(ProviderError::unsupported_capability(
                "reasoning_effort",
                "Ollama does not support reasoning_effort",
            ));
        }
        let mut options = serde_json::Map::new();
        if let Some(top_k) = request.top_k {
            options.insert("top_k".to_string(), serde_json::json!(top_k));
        }
        if let Some(min_p) = request.min_p {
            options.insert("min_p".to_string(), serde_json::json!(min_p));
        }
        if let Some(seed) = request.seed {
            options.insert("seed".to_string(), serde_json::json!(seed));
        }
        if let Some(temperature) = request.temperature {
            options.insert("temperature".to_string(), serde_json::json!(temperature));
        }
        if let Some(top_p) = request.top_p {
            options.insert("top_p".to_string(), serde_json::json!(top_p));
        }
        if let Some(max_tokens) = request.max_tokens {
            options.insert("num_predict".to_string(), serde_json::json!(max_tokens));
        }
        if !request.stop.is_empty() {
            options.insert("stop".to_string(), serde_json::json!(request.stop));
        }
        if !options.is_empty() {
            wire.insert("options".to_string(), serde_json::Value::Object(options));
        }
        if let Some(logprobs) = request.logprobs {
            wire.insert("logprobs".to_string(), serde_json::json!(logprobs));
        }
        if let Some(top_logprobs) = request.top_logprobs {
            wire.insert("top_logprobs".to_string(), serde_json::json!(top_logprobs));
        }

        if !request.tools.is_empty() {
            let tools: Vec<serde_json::Value> = request
                .tools
                .iter()
                .map(|tool| {
                    let mut function = serde_json::Map::new();
                    function.insert("name".to_string(), serde_json::json!(tool.name));
                    if let Some(description) = &tool.description {
                        function.insert("description".to_string(), serde_json::json!(description));
                    }
                    function.insert(
                        "parameters".to_string(),
                        serde_json::Value::Object(tool.parameters.clone()),
                    );
                    serde_json::json!({
                        "type": "function",
                        "function": function,
                    })
                })
                .collect();
            wire.insert("tools".to_string(), serde_json::Value::Array(tools));
        }

        Ok(serde_json::Value::Object(wire))
    }

    fn response_from_wire(
        &self,
        wire: &serde_json::Value,
    ) -> crate::error::ProviderContractResult<crate::types::ProviderResponse> {
        use crate::error::ProviderError;

        let content = wire
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .ok_or_else(|| {
                ProviderError::malformed_response("Ollama response is missing message.content")
            })?
            .to_string();

        let thinking = wire
            .get("message")
            .and_then(|m| m.get("thinking"))
            .and_then(|t| t.as_str())
            .map(|s| s.to_string());

        let raw_finish = wire
            .get("done_reason")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let canonical_finish = raw_finish
            .as_deref()
            .and_then(ollama_contract_finish_reason_from_str);

        let prompt = wire
            .get("prompt_eval_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let completion = wire.get("eval_count").and_then(|v| v.as_u64()).unwrap_or(0);

        let model = wire
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| self.config.model.clone());

        let endpoint_origin = self.config.endpoint_origin()?;

        Ok(crate::types::ProviderResponse {
            provider_metadata: None,
            content,
            descriptor: crate::types::ProviderDescriptor {
                provider_id: self.config.provider_id.clone(),
                model,
                endpoint_origin,
                locality: self.locality().to_string(),
                transport: "ollama_native".to_string(),
            },
            usage: crate::types::ProviderUsage {
                prompt,
                completion,
                total: prompt + completion,
                is_estimated: false,
            },
            error: None,
            thinking,
            tool_calls: Vec::new(),
            canonical_finish,
            raw_finish,
        })
    }

    fn error_from_failure(
        &self,
        failure: &crate::error::ProviderFailureInput,
    ) -> crate::error::ProviderError {
        let code = crate::error::classify_provider_failure(failure);
        let retry_after_ms = crate::error::normalize_retry_after_ms(failure.retry_after.as_deref());
        crate::error::ProviderError {
            code,
            message: format!("Ollama request failed: {code:?}"),
            field: None,
            retry_after_ms,
        }
    }
}

// ============================================================================
// Spec 119 Sprint 1C: OllamaContractMapper streaming — `ProviderStreamMapper`
// for Ollama's native NDJSON `/api/chat` streaming wire format. Each NDJSON
// line is one wire frame; a frame with `done: false` yields `content`
// and/or `thinking` events, and a frame with `done: true` yields the single
// terminal `complete` event.
// ============================================================================
impl crate::provider::ProviderStreamMapper for OllamaContractMapper {
    fn request_to_stream_wire(
        &self,
        request: &crate::types::ProviderRequest,
    ) -> crate::error::ProviderContractResult<serde_json::Value> {
        let mut wire = self.request_to_wire(request)?;
        if let serde_json::Value::Object(map) = &mut wire {
            map.insert("stream".to_string(), serde_json::json!(true));
        }
        Ok(wire)
    }

    fn stream_events_from_wire(
        &self,
        wire: &serde_json::Value,
    ) -> crate::error::ProviderContractResult<Vec<crate::types::StreamEvent>> {
        use crate::error::ProviderError;
        use crate::types::StreamEvent;

        let mut events = Vec::new();

        let content = wire
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or("");
        let thinking = wire
            .get("message")
            .and_then(|m| m.get("thinking"))
            .and_then(|t| t.as_str())
            .unwrap_or("");
        let done = wire.get("done").and_then(|v| v.as_bool()).unwrap_or(false);

        if !done {
            if !content.is_empty() {
                events.push(StreamEvent::Content {
                    delta: content.to_string(),
                });
            }
            if !thinking.is_empty() {
                events.push(StreamEvent::Thinking {
                    delta: thinking.to_string(),
                });
            }
        }

        if done {
            let raw_finish = wire
                .get("done_reason")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let canonical_finish = raw_finish
                .as_deref()
                .and_then(ollama_contract_finish_reason_from_str);
            let prompt = wire
                .get("prompt_eval_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let completion = wire.get("eval_count").and_then(|v| v.as_u64()).unwrap_or(0);
            events.push(StreamEvent::Complete {
                content: content.to_string(),
                canonical_finish,
                raw_finish,
                usage: crate::types::ProviderUsage {
                    prompt,
                    completion,
                    total: prompt + completion,
                    is_estimated: false,
                },
            });
        } else if content.is_empty() && thinking.is_empty() {
            return Err(ProviderError::malformed_response(
                "Ollama stream frame has no content, thinking, or done marker",
            ));
        }

        Ok(events)
    }
}

// ============================================================================
// Spec 119 Sprint 1C: crate-private streaming execution backend for
// `OllamaProvider::into_contract_facade`. Establishes exactly one streaming
// `/api/chat` connection and decodes it into a stream of raw NDJSON wire
// frames as `ProviderBackendOutcome`s.
// ============================================================================
pub(super) fn next_ollama_ndjson_frame(
    buffer: &mut Vec<u8>,
) -> Option<crate::error::ProviderContractResult<serde_json::Value>> {
    let max_bytes = crate::provider::http::MAX_STREAM_RECORD_EVENT_BYTES;
    loop {
        let newline = match buffer.iter().position(|byte| *byte == b'\n') {
            Some(newline) => newline,
            None => {
                let pending_len = if buffer.last() == Some(&b'\r') {
                    buffer.len().saturating_sub(1)
                } else {
                    buffer.len()
                };
                if pending_len > max_bytes {
                    return Some(Err(crate::error::ProviderError::malformed_response(
                        format!("Ollama NDJSON frame exceeds {max_bytes} bytes"),
                    )));
                }
                return None;
            }
        };
        let payload_len = if newline > 0 && buffer[newline - 1] == b'\r' {
            newline - 1
        } else {
            newline
        };
        if payload_len > max_bytes {
            buffer.drain(..=newline);
            return Some(Err(crate::error::ProviderError::malformed_response(
                format!("Ollama NDJSON frame exceeds {max_bytes} bytes"),
            )));
        }
        let mut line: Vec<u8> = buffer.drain(..=newline).collect();
        line.pop();
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let line = match std::str::from_utf8(&line) {
            Ok(line) => line,
            Err(_) => {
                return Some(Err(crate::error::ProviderError::malformed_response(
                    "Ollama stream frame is not valid UTF-8",
                )));
            }
        };
        return Some(serde_json::from_str(line).map_err(|_| {
            crate::error::ProviderError::malformed_response("Ollama stream frame is invalid JSON")
        }));
    }
}

struct OllamaStreamingBackend {
    client: reqwest::Client,
    endpoint: String,
    timeout: std::time::Duration,
}

#[async_trait]
impl crate::provider::ProviderStreamingBackend for OllamaStreamingBackend {
    async fn execute_stream(
        &self,
        wire: &serde_json::Value,
    ) -> crate::error::ProviderContractResult<crate::provider::ProviderBackendStream> {
        use crate::error::{ProviderFailureInput, ProviderTransportFailureKind};
        use crate::provider::ProviderBackendOutcome;

        let response = match self
            .client
            .post(&self.endpoint)
            .header("content-type", "application/json")
            .json(wire)
            .timeout(self.timeout)
            .send()
            .await
        {
            Ok(response) => response,
            Err(error) => {
                let kind = if error.is_timeout() {
                    ProviderTransportFailureKind::Timeout
                } else {
                    ProviderTransportFailureKind::Network
                };
                let outcome = ProviderBackendOutcome::Failure(ProviderFailureInput {
                    status: None,
                    retry_after: None,
                    body: None,
                    transport_kind: kind,
                });
                return Ok(Box::pin(futures::stream::once(async move { outcome })));
            }
        };

        let status = response.status();
        let retry_after = response
            .headers()
            .get("retry-after")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        if !status.is_success() {
            let body_text = crate::provider::http::read_response_text_bounded(response)
                .await
                .ok();
            let outcome = ProviderBackendOutcome::Failure(ProviderFailureInput {
                status: Some(status.as_u16()),
                retry_after,
                body: body_text,
                transport_kind: ProviderTransportFailureKind::None,
            });
            return Ok(Box::pin(futures::stream::once(async move { outcome })));
        }

        let bytes_stream = response.bytes_stream();
        let events = async_stream::stream! {
            let mut bytes_stream = bytes_stream;
            let mut buffer = Vec::new();
            loop {
                match bytes_stream.next().await {
                    Some(Ok(bytes)) => {
                        let mut chunk_cursor =
                            crate::provider::http::BoundedStreamChunkCursor::new(&bytes);
                        loop {
                            let has_more = match chunk_cursor.append_next(&mut buffer) {
                                Ok(has_more) => has_more,
                                Err(error) => { yield ProviderBackendOutcome::ProtocolError(crate::error::ProviderError::malformed_response(error.to_string())); return; }
                            };
                        while let Some(frame) = next_ollama_ndjson_frame(&mut buffer) {
                            match frame {
                                Ok(value) => yield ProviderBackendOutcome::Response(value),
                                Err(error) => {
                                    yield ProviderBackendOutcome::ProtocolError(error);
                                    return;
                                }
                            }
                        }
                            if !has_more {
                                break;
                            }
                        }
                    }
                    Some(Err(error)) => {
                        let kind = if error.is_timeout() {
                            ProviderTransportFailureKind::Timeout
                        } else {
                            ProviderTransportFailureKind::Network
                        };
                        yield ProviderBackendOutcome::Failure(ProviderFailureInput {
                            status: None,
                            retry_after: None,
                            body: None,
                            transport_kind: kind,
                        });
                        return;
                    }
                    None => return,
                }
            }
        };

        Ok(Box::pin(events))
    }
}

#[cfg(test)]
mod resource_bound_tests {
    use super::*;
    const LIMIT: usize = 4 * 1024 * 1024;

    fn ndjson_frame(payload_len: usize) -> Vec<u8> {
        let prefix = b"{\"x\":\"";
        let suffix = b"\"}";
        let mut frame = Vec::with_capacity(payload_len + 1);
        frame.extend_from_slice(prefix);
        frame.extend(std::iter::repeat_n(
            b'a',
            payload_len - prefix.len() - suffix.len(),
        ));
        frame.extend_from_slice(suffix);
        frame.push(b'\n');
        frame
    }

    #[test]
    fn resource_bound_ollama_exact_limit_and_one_over() {
        let mut exact = ndjson_frame(LIMIT);
        match next_ollama_ndjson_frame(&mut exact).expect("exact") {
            Ok(v) if v["x"].is_string() => {}
            _ => panic!("EXACT_NDJSON_LIMIT_MUST_SUCCEED"),
        }
        let mut over = ndjson_frame(LIMIT + 1);
        match next_ollama_ndjson_frame(&mut over).expect("over result") {
            Err(e) if e.to_string().contains("exceeds 4194304 bytes") => {}
            _ => panic!("SEMANTIC_RED_OLLAMA_NDJSON_OVER_ACCEPTED"),
        }
    }

    #[test]
    fn resource_bound_ollama_cursor_composes_large_multi_frame_chunk() {
        let payload_len = crate::provider::http::MAX_STREAM_RECORD_EVENT_BYTES / 2 + 1024;
        let mut chunk = ndjson_frame(payload_len);
        chunk.extend_from_slice(&ndjson_frame(payload_len));
        assert!(chunk.len() > crate::provider::http::MAX_STREAM_ACCUMULATOR_BYTES);

        let mut cursor = crate::provider::http::BoundedStreamChunkCursor::new(&chunk);
        let mut buffer = Vec::new();
        let mut seen = 0usize;
        loop {
            let has_more = cursor.append_next(&mut buffer).expect("bounded append");
            while let Some(frame) = next_ollama_ndjson_frame(&mut buffer) {
                let value = frame.expect("valid NDJSON frame");
                assert!(value["x"].is_string());
                seen += 1;
            }
            if !has_more {
                break;
            }
        }
        assert_eq!(seen, 2);
        assert!(buffer.is_empty());
    }

    #[test]
    fn resource_bound_ollama_fragmented_utf8_remains_valid() {
        let bytes = "{\"x\":\"€\"}\n".as_bytes();
        let euro = bytes
            .windows(3)
            .position(|w| w == "€".as_bytes())
            .expect("euro");
        let mut buffer = bytes[..euro + 1].to_vec();
        assert!(next_ollama_ndjson_frame(&mut buffer).is_none());
        buffer.extend_from_slice(&bytes[euro + 1..]);
        match next_ollama_ndjson_frame(&mut buffer).expect("complete") {
            Ok(v) if v["x"] == "€" => {}
            _ => panic!("FRAGMENTED_UTF8_NDJSON_MUST_SUCCEED"),
        }
    }
}
