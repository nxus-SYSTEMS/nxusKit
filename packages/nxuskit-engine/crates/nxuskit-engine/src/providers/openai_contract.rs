use super::*;

// Spec 119 Sprint 1A: OpenAIContractMapper — concrete provider contract mapper
//
// Pure mapper between the canonical `ProviderRequest`/`ProviderResponse` and
// OpenAI's `/v1/chat/completions` wire format. See
// `specs/119-v2-python-provider-surface-parity/data-model.md` and the Sprint
// 1A oracle packet under
// `packages/nxuskit-engine/crates/nxuskit-engine/tests/fixtures/provider_parity/`
// for the exact behavior this mapper implements. Independent of
// `OpenAIProvider` above: this mapper owns no transport/HTTP state, only DTO
// translation and failure classification.
// ============================================================================

/// Pure `ProviderContractMapper` for OpenAI's `/v1/chat/completions` wire
/// format. Accepts only the `openai` canonical provider ID.
#[derive(Debug)]
pub struct OpenAIContractMapper {
    config: crate::types::ProviderConfig,
}

impl OpenAIContractMapper {
    /// Constructs a mapper from a `ProviderConfig`, failing closed on an
    /// invalid config or a `provider_id` other than `openai`.
    pub fn new(config: crate::types::ProviderConfig) -> crate::error::ProviderContractResult<Self> {
        config.validate()?;
        if config.provider_id != "openai" {
            return Err(crate::error::ProviderError::invalid_request(
                "provider_id",
                format!(
                    "OpenAIContractMapper does not accept provider_id '{}'",
                    config.provider_id
                ),
            ));
        }
        Ok(Self { config })
    }

    fn effective_model(&self, request: &crate::types::ProviderRequest) -> String {
        request
            .model
            .clone()
            .unwrap_or_else(|| self.config.model.clone())
    }
}

fn openai_contract_message_role_str(role: crate::types::ProviderMessageRole) -> &'static str {
    match role {
        crate::types::ProviderMessageRole::System => "system",
        crate::types::ProviderMessageRole::User => "user",
        crate::types::ProviderMessageRole::Assistant => "assistant",
    }
}

fn openai_contract_reasoning_effort_str(
    effort: crate::types::ProviderReasoningEffort,
) -> &'static str {
    match effort {
        crate::types::ProviderReasoningEffort::Low => "low",
        crate::types::ProviderReasoningEffort::Medium => "medium",
        crate::types::ProviderReasoningEffort::High => "high",
        crate::types::ProviderReasoningEffort::Max => "max",
    }
}

fn openai_contract_finish_reason_from_str(raw: &str) -> Option<crate::types::ProviderFinishReason> {
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
// `OpenAIProvider::into_contract_facade`. Executes the already-mapped wire
// request exactly once against OpenAI's `/chat/completions` endpoint using
// the provider's own `reqwest::Client`, configured timeout, and bearer
// credential. The credential is never persisted or surfaced beyond the
// single outgoing request.
// ============================================================================
struct OpenAIFacadeBackend {
    client: reqwest::Client,
    endpoint: String,
    api_key: String,
    timeout: std::time::Duration,
}

#[async_trait]
impl crate::provider::ProviderExecutionBackend for OpenAIFacadeBackend {
    async fn execute(
        &self,
        wire: &serde_json::Value,
    ) -> crate::error::ProviderContractResult<crate::provider::ProviderBackendOutcome> {
        let send_result = self
            .client
            .post(&self.endpoint)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("content-type", "application/json")
            .json(wire)
            .timeout(self.timeout)
            .send()
            .await;
        Ok(crate::provider::http::json_backend_outcome(send_result).await)
    }
}

impl OpenAIProvider {
    /// Converts this provider into a `ProviderFacade` bound to a
    /// `ProviderConfig`/`OpenAIContractMapper` derived from this provider's
    /// own base URL, model, timeout, and API key. Performs no network call.
    pub fn into_contract_facade(
        self,
    ) -> crate::error::ProviderContractResult<crate::provider::ProviderFacade> {
        let capabilities = self.get_capabilities();
        let config = crate::types::ProviderConfig {
            provider_id: "openai".to_string(),
            model: self.default_model.clone(),
            endpoint: Some(self.base_url.clone()),
            auth_selector: None,
            transport: crate::types::ProviderTransportMode::Native,
            connect_timeout_ms: super::super::duration_to_provider_timeout_ms(
                self.connection_timeout,
            ),
            read_timeout_ms: super::super::duration_to_provider_timeout_ms(self.total_timeout),
        };
        let mapper = std::sync::Arc::new(OpenAIContractMapper::new(config)?);
        let backend = OpenAIFacadeBackend {
            endpoint: format!("{}/chat/completions", self.base_url),
            api_key: self.api_key.clone(),
            timeout: self.total_timeout,
            client: self.client.clone(),
        };
        let streaming_backend = OpenAIStreamingBackend {
            endpoint: format!("{}/chat/completions", self.base_url),
            api_key: self.api_key,
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

impl crate::provider::ProviderContractMapper for OpenAIContractMapper {
    fn request_to_wire(
        &self,
        request: &crate::types::ProviderRequest,
    ) -> crate::error::ProviderContractResult<serde_json::Value> {
        use crate::error::ProviderError;
        use crate::types::ProviderResponseFormat;

        if request.top_k.is_some() {
            return Err(ProviderError::unsupported_capability(
                "top_k",
                "OpenAI does not support the Ollama-only top_k control",
            ));
        }
        if request.min_p.is_some() {
            return Err(ProviderError::unsupported_capability(
                "min_p",
                "OpenAI does not support the Ollama-only min_p control",
            ));
        }
        if request.thinking.is_some() {
            return Err(ProviderError::unsupported_capability(
                "thinking",
                "OpenAI does not support the Ollama-only thinking control",
            ));
        }

        let mut wire = serde_json::Map::new();
        wire.insert(
            "model".to_string(),
            serde_json::json!(self.effective_model(request)),
        );

        let messages: Vec<serde_json::Value> = request
            .messages
            .iter()
            .map(|m| {
                serde_json::json!({
                    "role": openai_contract_message_role_str(m.role),
                    "content": m.content,
                })
            })
            .collect();
        wire.insert("messages".to_string(), serde_json::Value::Array(messages));

        if let Some(temperature) = request.temperature {
            wire.insert("temperature".to_string(), serde_json::json!(temperature));
        }
        if let Some(top_p) = request.top_p {
            wire.insert("top_p".to_string(), serde_json::json!(top_p));
        }
        if let Some(max_tokens) = request.max_tokens {
            wire.insert("max_tokens".to_string(), serde_json::json!(max_tokens));
        }
        if let Some(seed) = request.seed {
            wire.insert("seed".to_string(), serde_json::json!(seed));
        }
        if !request.stop.is_empty() {
            wire.insert("stop".to_string(), serde_json::json!(request.stop));
        }
        if let Some(presence_penalty) = request.presence_penalty {
            wire.insert(
                "presence_penalty".to_string(),
                serde_json::json!(presence_penalty),
            );
        }
        if let Some(frequency_penalty) = request.frequency_penalty {
            wire.insert(
                "frequency_penalty".to_string(),
                serde_json::json!(frequency_penalty),
            );
        }
        if let Some(reasoning_effort) = request.reasoning_effort {
            wire.insert(
                "reasoning_effort".to_string(),
                serde_json::json!(openai_contract_reasoning_effort_str(reasoning_effort)),
            );
        }
        if let Some(logprobs) = request.logprobs {
            wire.insert("logprobs".to_string(), serde_json::json!(logprobs));
        }
        if let Some(top_logprobs) = request.top_logprobs {
            wire.insert("top_logprobs".to_string(), serde_json::json!(top_logprobs));
        }

        match &request.response_format {
            None | Some(ProviderResponseFormat::Text) => {}
            Some(ProviderResponseFormat::Json) => {
                wire.insert(
                    "response_format".to_string(),
                    serde_json::json!({"type": "json_object"}),
                );
            }
            Some(ProviderResponseFormat::JsonSchema { schema }) => {
                wire.insert(
                    "response_format".to_string(),
                    serde_json::json!({
                        "type": "json_schema",
                        "json_schema": {
                            "name": "response",
                            "schema": serde_json::Value::Object(schema.clone()),
                            "strict": true
                        }
                    }),
                );
            }
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

        let choice = wire
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .ok_or_else(|| {
                ProviderError::malformed_response("OpenAI response is missing choices[0]")
            })?;

        let content = choice
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .ok_or_else(|| {
                ProviderError::malformed_response(
                    "OpenAI response is missing choices[0].message.content",
                )
            })?
            .to_string();

        let raw_finish = choice
            .get("finish_reason")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let canonical_finish = raw_finish
            .as_deref()
            .and_then(openai_contract_finish_reason_from_str);

        let prompt = wire
            .get("usage")
            .and_then(|u| u.get("prompt_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let completion = wire
            .get("usage")
            .and_then(|u| u.get("completion_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let total = wire
            .get("usage")
            .and_then(|u| u.get("total_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(prompt + completion);

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
                locality: "remote".to_string(),
                transport: "remote_https".to_string(),
            },
            usage: crate::types::ProviderUsage {
                prompt,
                completion,
                total,
                is_estimated: false,
            },
            error: None,
            thinking: None,
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
            message: format!("OpenAI request failed: {code:?}"),
            field: None,
            retry_after_ms,
        }
    }
}

// ============================================================================
// Spec 119 Sprint 1C: OpenAIContractMapper streaming — `ProviderStreamMapper`
// for OpenAI's `/v1/chat/completions` SSE streaming wire format. Each SSE
// `data:` frame carries one `choices[0].delta`; a nonnull `finish_reason`
// marks the single terminal `complete` event. The literal `[DONE]` sentinel
// is a transport concern handled by the streaming backend, never surfaced
// to this mapper.
// ============================================================================
impl crate::provider::ProviderStreamMapper for OpenAIContractMapper {
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

        let choice = wire
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .ok_or_else(|| {
                ProviderError::malformed_response("OpenAI stream frame is missing choices[0]")
            })?;

        let mut events = Vec::new();
        let content = choice
            .get("delta")
            .and_then(|d| d.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or("");

        let raw_finish = choice
            .get("finish_reason")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        if raw_finish.is_none() && !content.is_empty() {
            events.push(StreamEvent::Content {
                delta: content.to_string(),
            });
        }

        if let Some(raw_finish) = raw_finish {
            let canonical_finish = openai_contract_finish_reason_from_str(&raw_finish);
            let prompt = wire
                .get("usage")
                .and_then(|u| u.get("prompt_tokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let completion = wire
                .get("usage")
                .and_then(|u| u.get("completion_tokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let total = wire
                .get("usage")
                .and_then(|u| u.get("total_tokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(prompt + completion);
            events.push(StreamEvent::Complete {
                content: content.to_string(),
                canonical_finish,
                raw_finish: Some(raw_finish),
                usage: crate::types::ProviderUsage {
                    prompt,
                    completion,
                    total,
                    is_estimated: false,
                },
            });
        } else if content.is_empty() {
            return Err(ProviderError::malformed_response(
                "OpenAI stream frame has no content delta or finish_reason",
            ));
        }

        Ok(events)
    }
}

// ============================================================================
// Spec 119 Sprint 1C: crate-private streaming execution backend for
// `OpenAIProvider::into_contract_facade`. Establishes exactly one streaming
// `/chat/completions` connection and decodes its SSE body into a stream of
// raw wire frames as `ProviderBackendOutcome`s. The `[DONE]` sentinel simply
// ends the raw stream; it is not itself a `ProviderBackendOutcome`.
// ============================================================================
pub(in crate::providers) enum OpenAiSseFrame {
    Data(serde_json::Value),
    Done,
    Ignored,
}

pub(in crate::providers) fn next_openai_sse_frame(
    buffer: &mut Vec<u8>,
) -> Option<crate::error::ProviderContractResult<OpenAiSseFrame>> {
    let max_bytes = crate::provider::http::MAX_STREAM_RECORD_EVENT_BYTES;
    let lf_boundary = buffer.windows(2).position(|window| window == b"\n\n");
    let crlf_boundary = buffer.windows(4).position(|window| window == b"\r\n\r\n");
    let (frame_end, delimiter_len) = match (lf_boundary, crlf_boundary) {
        (Some(lf), Some(crlf)) if lf < crlf => (lf, 2),
        (Some(_), Some(crlf)) => (crlf, 4),
        (Some(lf), None) => (lf, 2),
        (None, Some(crlf)) => (crlf, 4),
        (None, None) => {
            if buffer.len() > max_bytes.saturating_add(3) {
                return Some(Err(crate::error::ProviderError::malformed_response(
                    format!("OpenAI SSE frame exceeds {max_bytes} bytes"),
                )));
            }
            return None;
        }
    };
    if frame_end > max_bytes {
        buffer.drain(..frame_end + delimiter_len);
        return Some(Err(crate::error::ProviderError::malformed_response(
            format!("OpenAI SSE frame exceeds {max_bytes} bytes"),
        )));
    }
    let frame: Vec<u8> = buffer.drain(..frame_end + delimiter_len).collect();
    let frame = &frame[..frame_end];
    let frame = match std::str::from_utf8(frame) {
        Ok(frame) => frame,
        Err(_) => {
            return Some(Err(crate::error::ProviderError::malformed_response(
                "OpenAI SSE frame is not valid UTF-8",
            )));
        }
    };
    let Some(data) = frame.lines().find_map(|line| line.strip_prefix("data: ")) else {
        return Some(Ok(OpenAiSseFrame::Ignored));
    };
    if data == "[DONE]" {
        return Some(Ok(OpenAiSseFrame::Done));
    }
    Some(
        serde_json::from_str(data)
            .map(OpenAiSseFrame::Data)
            .map_err(|_| {
                crate::error::ProviderError::malformed_response("OpenAI SSE frame is invalid JSON")
            }),
    )
}

struct OpenAIStreamingBackend {
    client: reqwest::Client,
    endpoint: String,
    api_key: String,
    timeout: std::time::Duration,
}

#[async_trait]
impl crate::provider::ProviderStreamingBackend for OpenAIStreamingBackend {
    async fn execute_stream(
        &self,
        wire: &serde_json::Value,
    ) -> crate::error::ProviderContractResult<crate::provider::ProviderBackendStream> {
        use crate::error::{ProviderFailureInput, ProviderTransportFailureKind};
        use crate::provider::ProviderBackendOutcome;

        let response = match self
            .client
            .post(&self.endpoint)
            .header("Authorization", format!("Bearer {}", self.api_key))
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
                        while let Some(frame) = next_openai_sse_frame(&mut buffer) {
                            match frame {
                                Ok(OpenAiSseFrame::Data(value)) => {
                                    yield ProviderBackendOutcome::Response(value)
                                }
                                Ok(OpenAiSseFrame::Done) => return,
                                Ok(OpenAiSseFrame::Ignored) => continue,
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

    fn sse_frame(payload_len: usize) -> Vec<u8> {
        let prefix = b"data: {\"x\":\"";
        let suffix = b"\"}";
        let mut frame = Vec::with_capacity(payload_len + 2);
        frame.extend_from_slice(prefix);
        frame.extend(std::iter::repeat_n(
            b'a',
            payload_len - prefix.len() - suffix.len(),
        ));
        frame.extend_from_slice(suffix);
        frame.extend_from_slice(b"\n\n");
        frame
    }

    #[test]
    fn resource_bound_openai_exact_limit_and_one_over() {
        let mut exact = sse_frame(LIMIT);
        match next_openai_sse_frame(&mut exact).expect("exact") {
            Ok(OpenAiSseFrame::Data(v)) if v["x"].is_string() => {}
            _ => panic!("EXACT_SSE_LIMIT_MUST_SUCCEED"),
        }
        let mut over = sse_frame(LIMIT + 1);
        match next_openai_sse_frame(&mut over).expect("over result") {
            Err(e) if e.to_string().contains("exceeds 4194304 bytes") => {}
            _ => panic!("SEMANTIC_RED_OPENAI_SSE_OVER_ACCEPTED"),
        }
    }

    #[test]
    fn resource_bound_openai_cursor_composes_large_multi_frame_chunk() {
        let payload_len = crate::provider::http::MAX_STREAM_RECORD_EVENT_BYTES / 2 + 1024;
        let mut chunk = sse_frame(payload_len);
        chunk.extend_from_slice(&sse_frame(payload_len));
        assert!(chunk.len() > crate::provider::http::MAX_STREAM_ACCUMULATOR_BYTES);

        let mut cursor = crate::provider::http::BoundedStreamChunkCursor::new(&chunk);
        let mut buffer = Vec::new();
        let mut seen = 0usize;
        loop {
            let has_more = cursor.append_next(&mut buffer).expect("bounded append");
            while let Some(frame) = next_openai_sse_frame(&mut buffer) {
                match frame.expect("valid SSE frame") {
                    OpenAiSseFrame::Data(value) if value["x"].is_string() => seen += 1,
                    _ => panic!("OPENAI_CURSOR_COMPOSITION_FRAME_MISMATCH"),
                }
            }
            if !has_more {
                break;
            }
        }
        assert_eq!(seen, 2);
        assert!(buffer.is_empty());
    }

    #[test]
    fn resource_bound_openai_fragmented_utf8_remains_valid() {
        let bytes = "data: {\"x\":\"€\"}\n\n".as_bytes();
        let euro = bytes
            .windows(3)
            .position(|w| w == "€".as_bytes())
            .expect("euro");
        let mut buffer = bytes[..euro + 1].to_vec();
        assert!(next_openai_sse_frame(&mut buffer).is_none());
        buffer.extend_from_slice(&bytes[euro + 1..]);
        match next_openai_sse_frame(&mut buffer).expect("complete") {
            Ok(OpenAiSseFrame::Data(v)) if v["x"] == "€" => {}
            _ => panic!("FRAGMENTED_UTF8_SSE_MUST_SUCCEED"),
        }
    }
}
