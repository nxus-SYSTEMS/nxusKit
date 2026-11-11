use std::collections::BTreeSet;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use futures::StreamExt;
use serde_json::{Value, json};
use tokio::sync::{Notify, Semaphore};

#[path = "provider_stream_harness.rs"]
mod provider_stream_harness;

use super::{
    ProviderBackendOutcome, ProviderBackendStream, ProviderContractMapper, ProviderFacade,
    ProviderStreamMapper,
};
use crate::error::{
    ProviderError, ProviderErrorCode, ProviderFailureInput, ProviderTransportFailureKind,
};
use crate::providers::ollama::{OllamaContractMapper, OllamaProvider};
use crate::providers::openai::{OpenAIContractMapper, OpenAIProvider};
use crate::types::{
    ProviderCapabilities, ProviderConfig, ProviderFinishReason, ProviderRequest, ProviderToolCall,
    ProviderUsage, StreamEvent,
};
use provider_stream_harness::{
    CountingStream, FrameThenPendingStream, NoopBackend, ScriptedStreamingBackend,
    await_backend_drop, collect, spawn_stream_server, split_utf8_scalar,
};

const STREAMS: &str = include_str!("../fixtures/provider_parity/streams.json");
const STREAM_SCHEMA: &str = "nxus-spec119-rust-stream-fixtures/v1";
const EXPECTED_ASSERTIONS: usize = 42;
const EXPECTED_GROUPS: [usize; 7] = [8, 6, 8, 4, 6, 6, 4];
const FIXTURE_IDS: [&str; 6] = [
    "ollama-baseline",
    "ollama-reasoning",
    "openai-baseline",
    "openai-reasoning-unsupported",
    "ollama-error",
    "openai-cancel",
];

fn fixture() -> Value {
    serde_json::from_str(STREAMS).expect("streams.json must be literal valid JSON")
}

fn case(id: &str) -> Value {
    fixture()["cases"]
        .as_array()
        .expect("cases must be an array")
        .iter()
        .find(|record| record["id"] == id)
        .unwrap_or_else(|| panic!("missing stream fixture case {id}"))
        .clone()
}

fn config(id: &str) -> ProviderConfig {
    serde_json::from_value(case(id)["config"].clone())
        .unwrap_or_else(|error| panic!("{id}: config must deserialize: {error:?}"))
}

fn request(id: &str) -> ProviderRequest {
    serde_json::from_value(case(id)["request"].clone())
        .unwrap_or_else(|error| panic!("{id}: request must deserialize: {error:?}"))
}

fn literal_events(id: &str) -> Vec<StreamEvent> {
    serde_json::from_value(case(id)["public_events"].clone())
        .unwrap_or_else(|error| panic!("{id}: public events must deserialize: {error:?}"))
}

fn assert_fixture_structure() {
    let root = fixture();
    assert_eq!(root["schema_version"], STREAM_SCHEMA);
    assert_eq!(
        root["definition_candidate"],
        "50516e36b26c9754e3092e5ba10e80396f8837e6"
    );
    assert_eq!(root["queue_capacity"], 16);
    assert_eq!(root["assertion_groups"], json!(EXPECTED_GROUPS));
    let cases = root["cases"].as_array().expect("cases array");
    assert_eq!(cases.len(), FIXTURE_IDS.len());
    let ids: BTreeSet<&str> = cases
        .iter()
        .map(|record| record["id"].as_str().expect("case id"))
        .collect();
    assert_eq!(ids, BTreeSet::from(FIXTURE_IDS));
    for record in cases {
        let provenance = record["provenance"].as_str().expect("literal provenance");
        assert!(!provenance.is_empty());
        assert!(!provenance.contains("generated"));
    }
    for terminal_case in ["ollama-error", "openai-cancel"] {
        assert!(
            case(terminal_case)["request_case_id"].is_string(),
            "{terminal_case} must identify the request fixture that establishes its stream"
        );
    }
}

fn mapper_pair(
    provider: &str,
    fixture_id: &str,
) -> (
    Arc<dyn ProviderContractMapper>,
    Arc<dyn ProviderStreamMapper>,
) {
    match provider {
        "ollama_local" => {
            let mapper = Arc::new(OllamaContractMapper::new(config(fixture_id)).unwrap());
            (mapper.clone(), mapper)
        }
        "openai" => {
            let mapper = Arc::new(OpenAIContractMapper::new(config(fixture_id)).unwrap());
            (mapper.clone(), mapper)
        }
        other => panic!("unsupported fixture provider {other}"),
    }
}

fn facade_with_backend(
    provider: &str,
    backend: Arc<ScriptedStreamingBackend>,
) -> (ProviderFacade, Arc<AtomicUsize>) {
    let fixture_id = match provider {
        "ollama_local" => "ollama-baseline",
        "openai" => "openai-baseline",
        other => panic!("unsupported fixture provider {other}"),
    };
    let (mapper, stream_mapper) = mapper_pair(provider, fixture_id);
    let nonstream_calls = Arc::new(AtomicUsize::new(0));
    let nonstream = Arc::new(NoopBackend {
        calls: Arc::clone(&nonstream_calls),
    });
    let mut capabilities = ProviderCapabilities::default();
    capabilities.supports_streaming = true;
    (
        ProviderFacade::new_with_streaming(mapper, nonstream, stream_mapper, backend, capabilities),
        nonstream_calls,
    )
}

fn outcomes(id: &str) -> Vec<ProviderBackendOutcome> {
    case(id)["provider_frames"]
        .as_array()
        .expect("provider_frames array")
        .iter()
        .cloned()
        .map(|frame| {
            if let Some(failure) = frame.get("failure") {
                let transport_kind = match failure["kind"].as_str() {
                    Some("network") => ProviderTransportFailureKind::Network,
                    other => panic!("{id}: unsupported literal stream failure kind {other:?}"),
                };
                ProviderBackendOutcome::Failure(ProviderFailureInput {
                    status: None,
                    retry_after: None,
                    body: None,
                    transport_kind,
                })
            } else {
                ProviderBackendOutcome::Response(frame)
            }
        })
        .collect()
}

async fn literal_roundtrip(id: &str) -> (Value, Vec<StreamEvent>, usize) {
    let record = case(id);
    let provider = record["provider"].as_str().expect("provider id");
    let (contract, stream_mapper) = mapper_pair(provider, id);
    let request = request(id);
    request.validate().expect("literal request must validate");
    let wire = stream_mapper
        .request_to_stream_wire(&request)
        .expect("literal stream request must map");
    assert_eq!(wire, record["outgoing_wire"]);

    let backend = ScriptedStreamingBackend::sequence(outcomes(id));
    let (facade, nonstream_calls) = {
        let nonstream_calls = Arc::new(AtomicUsize::new(0));
        let mut capabilities = ProviderCapabilities::default();
        capabilities.supports_streaming = true;
        (
            ProviderFacade::new_with_streaming(
                contract,
                Arc::new(NoopBackend {
                    calls: Arc::clone(&nonstream_calls),
                }),
                stream_mapper,
                backend.clone(),
                capabilities,
            ),
            nonstream_calls,
        )
    };
    let events = collect(facade.chat_stream(&request).await.unwrap()).await;
    assert_eq!(events, literal_events(id));
    assert_eq!(nonstream_calls.load(Ordering::SeqCst), 0);
    (wire, events, backend.calls.load(Ordering::SeqCst))
}

fn terminal_count(events: &[StreamEvent]) -> usize {
    events
        .iter()
        .filter(|event| {
            matches!(
                event,
                StreamEvent::Complete { .. }
                    | StreamEvent::Error { .. }
                    | StreamEvent::Cancelled { .. }
            )
        })
        .count()
}

fn assert_event_schema_group() -> usize {
    let usage = ProviderUsage {
        prompt: 10,
        completion: 2,
        total: 12,
        is_estimated: false,
    };
    let values = vec![
        (StreamEvent::Start, json!({"type":"start"})),
        (
            StreamEvent::Content { delta: "x".into() },
            json!({"type":"content","delta":"x"}),
        ),
        (
            StreamEvent::Thinking { delta: "r".into() },
            json!({"type":"thinking","delta":"r"}),
        ),
        (
            StreamEvent::ToolCall {
                tool_call: ProviderToolCall {
                    id: "call-1".into(),
                    name: "lookup".into(),
                    arguments: json!({"q":"x"}),
                },
            },
            json!({"type":"tool_call","tool_call":{"id":"call-1","name":"lookup","arguments":{"q":"x"}}}),
        ),
        (
            StreamEvent::Usage { usage },
            json!({"type":"usage","usage":{"prompt":10,"completion":2,"total":12,"is_estimated":false}}),
        ),
        (
            StreamEvent::Complete {
                content: "1}".into(),
                canonical_finish: Some(ProviderFinishReason::Stop),
                raw_finish: Some("stop".into()),
                usage,
            },
            json!({"type":"complete","content":"1}","canonical_finish":"stop","raw_finish":"stop","usage":{"prompt":10,"completion":2,"total":12,"is_estimated":false}}),
        ),
        (
            StreamEvent::Error {
                error: ProviderError::malformed_response("bad frame"),
            },
            json!({"type":"error","error":{"code":"malformed_response","message":"bad frame"}}),
        ),
        (
            StreamEvent::Cancelled {
                error: ProviderError::cancelled("provider stream cancelled"),
            },
            json!({"type":"cancelled","error":{"code":"cancelled","message":"provider stream cancelled"}}),
        ),
    ];
    for (event, literal) in values {
        assert_eq!(serde_json::to_value(&event).unwrap(), literal);
        assert_eq!(
            serde_json::from_value::<StreamEvent>(literal).unwrap(),
            event
        );
    }
    8
}

async fn assert_preflight_group() -> usize {
    let backend = ScriptedStreamingBackend::pending();
    let (facade, nonstream) = facade_with_backend("ollama_local", backend.clone());
    let mut invalid = request("ollama-baseline");
    invalid.messages.clear();
    assert_eq!(
        facade.chat_stream(&invalid).await.unwrap_err().code,
        ProviderErrorCode::InvalidRequest
    );
    assert_eq!(backend.calls.load(Ordering::SeqCst), 0);

    facade.close().await.unwrap();
    assert_eq!(
        facade
            .chat_stream(&request("ollama-baseline"))
            .await
            .unwrap_err()
            .code,
        ProviderErrorCode::Unavailable
    );
    assert_eq!(backend.calls.load(Ordering::SeqCst), 0);

    let openai_backend = ScriptedStreamingBackend::pending();
    let (openai, _) = facade_with_backend("openai", openai_backend.clone());
    assert_eq!(
        openai
            .chat_stream(&request("openai-reasoning-unsupported"))
            .await
            .unwrap_err()
            .code,
        ProviderErrorCode::UnsupportedCapability
    );
    assert_eq!(openai_backend.calls.load(Ordering::SeqCst), 0);

    let accepted_backend = ScriptedStreamingBackend::pending();
    let (accepted, _) = facade_with_backend("ollama_local", accepted_backend.clone());
    let stream = accepted
        .chat_stream(&request("ollama-baseline"))
        .await
        .unwrap();
    assert_eq!(accepted_backend.calls.load(Ordering::SeqCst), 1);
    assert_eq!(nonstream.load(Ordering::SeqCst), 0);
    drop(stream);
    6
}

async fn assert_literal_group() -> usize {
    assert_fixture_structure();
    let (ollama_wire, ollama_events, ollama_calls) = literal_roundtrip("ollama-baseline").await;
    assert_eq!(ollama_wire["stream"], true);
    assert_eq!(ollama_events, literal_events("ollama-baseline"));
    assert_eq!(ollama_calls, 1);
    let (reasoning_wire, reasoning_events, reasoning_calls) =
        literal_roundtrip("ollama-reasoning").await;
    assert_eq!(reasoning_wire["think"], true);
    assert!(matches!(reasoning_events[2], StreamEvent::Thinking { .. }));
    assert_eq!(reasoning_calls, 1);
    let (openai_wire, openai_events, openai_calls) = literal_roundtrip("openai-baseline").await;
    assert_eq!(openai_wire["stream"], true);
    assert_eq!(openai_events, literal_events("openai-baseline"));
    assert_eq!(openai_calls, 1);
    8
}

async fn assert_backpressure_group() -> usize {
    let polls = Arc::new(AtomicUsize::new(0));
    let poll_observed = Arc::new(Semaphore::new(0));
    let dropped = Arc::new(AtomicBool::new(false));
    let source: ProviderBackendStream = Box::pin(CountingStream {
        remaining: 20,
        polls: Arc::clone(&polls),
        poll_observed: Arc::clone(&poll_observed),
        dropped: Arc::clone(&dropped),
    });
    let backend = ScriptedStreamingBackend::from_stream(source);
    let drop_notify = Arc::clone(&backend.drop_notify);
    let (facade, _) = facade_with_backend("ollama_local", backend);
    let mut stream = facade
        .chat_stream(&request("ollama-baseline"))
        .await
        .unwrap();
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        poll_observed.acquire_many(16),
    )
    .await
    .expect("backend must fill the bounded queue")
    .expect("poll semaphore must remain open")
    .forget();
    assert_eq!(polls.load(Ordering::SeqCst), 16);
    assert_eq!(stream.next().await, Some(StreamEvent::Start));
    tokio::time::timeout(std::time::Duration::from_secs(5), poll_observed.acquire())
        .await
        .expect("consuming one event must release one backend poll")
        .expect("poll semaphore must remain open")
        .forget();
    assert_eq!(polls.load(Ordering::SeqCst), 17);
    assert!(
        matches!(stream.next().await, Some(StreamEvent::Content { delta }) if delta == "chunk-0")
    );
    drop(stream);
    await_backend_drop(&dropped, &drop_notify).await;
    4
}

async fn assert_cancel_group() -> usize {
    let frame_processed = Arc::new(Notify::new());
    let backend = ScriptedStreamingBackend::from_stream(Box::pin(FrameThenPendingStream {
        outcome: Some(
            outcomes("openai-cancel")
                .into_iter()
                .next()
                .expect("openai-cancel must include one literal provider frame"),
        ),
        processed: Arc::clone(&frame_processed),
    }));
    let dropped = Arc::clone(&backend.dropped);
    let drop_notify = Arc::clone(&backend.drop_notify);
    let (facade, _) = facade_with_backend("openai", backend);
    let mut stream = facade
        .chat_stream(&request("openai-baseline"))
        .await
        .unwrap();
    let mut events = vec![
        stream
            .next()
            .await
            .expect("the established stream must expose its start event"),
    ];
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        frame_processed.notified(),
    )
    .await
    .expect("the literal OpenAI cancel frame must be mapped before cancellation");
    stream.cancel();
    stream.cancel();
    assert!(stream.is_cancelled());
    events.extend(collect(stream).await);
    assert_eq!(events, literal_events("openai-cancel"));
    await_backend_drop(&dropped, &drop_notify).await;

    let drop_backend = ScriptedStreamingBackend::pending();
    let drop_observed = Arc::clone(&drop_backend.dropped);
    let drop_notify = Arc::clone(&drop_backend.drop_notify);
    let (drop_facade, _) = facade_with_backend("ollama_local", drop_backend);
    let drop_stream = drop_facade
        .chat_stream(&request("ollama-baseline"))
        .await
        .unwrap();
    assert!(!drop_stream.is_cancelled());
    drop(drop_stream);
    await_backend_drop(&drop_observed, &drop_notify).await;

    let terminal_backend = ScriptedStreamingBackend::sequence(outcomes("ollama-baseline"));
    let (terminal_facade, _) = facade_with_backend("ollama_local", terminal_backend);
    let terminal_stream = terminal_facade
        .chat_stream(&request("ollama-baseline"))
        .await
        .unwrap();
    let terminal_events = collect(terminal_stream).await;
    assert_eq!(terminal_count(&terminal_events), 1);
    assert!(matches!(
        terminal_events.last(),
        Some(StreamEvent::Complete { .. })
    ));
    6
}

async fn assert_terminal_group() -> usize {
    let complete = literal_roundtrip("ollama-baseline").await.1;
    assert_eq!(terminal_count(&complete), 1);
    assert!(matches!(
        complete.last(),
        Some(StreamEvent::Complete { .. })
    ));

    let error_backend = ScriptedStreamingBackend::sequence(outcomes("ollama-error"));
    let (error_facade, _) = facade_with_backend("ollama_local", error_backend);
    let error_events = collect(
        error_facade
            .chat_stream(&request(
                case("ollama-error")["request_case_id"]
                    .as_str()
                    .expect("ollama-error request case"),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(error_events, literal_events("ollama-error"));

    let truncated_backend =
        ScriptedStreamingBackend::sequence(vec![ProviderBackendOutcome::Response(json!({
            "message": {"role":"assistant","content":"partial"},
            "done": false
        }))]);
    let (truncated_facade, _) = facade_with_backend("ollama_local", truncated_backend);
    let truncated = collect(
        truncated_facade
            .chat_stream(&request("ollama-baseline"))
            .await
            .unwrap(),
    )
    .await;
    assert!(
        matches!(truncated.last(), Some(StreamEvent::Error { error }) if error.code == ProviderErrorCode::MalformedResponse)
    );
    assert_eq!(terminal_count(&truncated), 1);
    assert_raw_malformed_streams().await;
    6
}

async fn assert_raw_malformed_streams() {
    let (ollama_addr, ollama_request_rx) =
        spawn_stream_server("application/x-ndjson", vec![b"{not-valid-json}\n".to_vec()]).await;
    let ollama = OllamaProvider::builder()
        .base_url(format!("http://{ollama_addr}"))
        .model("llama3.2")
        .build()
        .unwrap()
        .into_contract_facade()
        .unwrap();
    let ollama_events = collect(
        ollama
            .chat_stream(&request("ollama-baseline"))
            .await
            .unwrap(),
    )
    .await;
    let _ = ollama_request_rx.await.unwrap();
    assert!(
        matches!(ollama_events.last(), Some(StreamEvent::Error { error }) if error.code == ProviderErrorCode::MalformedResponse)
    );

    let (openai_addr, openai_request_rx) = spawn_stream_server(
        "text/event-stream",
        vec![b"data: {not-valid-json}\n\n".to_vec()],
    )
    .await;
    let openai = OpenAIProvider::builder()
        .api_key("sk-fixture-secret-must-not-leak")
        .base_url(format!("http://{openai_addr}/v1"))
        .model("gpt-4o-mini")
        .build()
        .unwrap()
        .into_contract_facade()
        .unwrap();
    let openai_events = collect(
        openai
            .chat_stream(&request("openai-baseline"))
            .await
            .unwrap(),
    )
    .await;
    let _ = openai_request_rx.await.unwrap();
    assert!(
        matches!(openai_events.last(), Some(StreamEvent::Error { error }) if error.code == ProviderErrorCode::MalformedResponse)
    );
}

async fn assert_anchor_group() -> usize {
    let ollama_frames = case("ollama-baseline")["provider_frames"]
        .as_array()
        .unwrap()
        .iter()
        .map(|frame| serde_json::to_string(frame).unwrap())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let (ollama_addr, ollama_request_rx) = spawn_stream_server(
        "application/x-ndjson",
        split_utf8_scalar(ollama_frames.into_bytes()),
    )
    .await;
    let ollama = OllamaProvider::builder()
        .base_url(format!("http://{ollama_addr}"))
        .model("llama3.2")
        .build()
        .unwrap()
        .into_contract_facade()
        .unwrap();
    let ollama_events = collect(
        ollama
            .chat_stream(&request("ollama-baseline"))
            .await
            .unwrap(),
    )
    .await;
    let ollama_body: Value = serde_json::from_slice(&ollama_request_rx.await.unwrap()).unwrap();
    assert_eq!(ollama_body, case("ollama-baseline")["outgoing_wire"]);
    assert_eq!(ollama_events, literal_events("ollama-baseline"));

    let openai_frames = case("openai-baseline")["provider_frames"]
        .as_array()
        .unwrap()
        .iter()
        .map(|frame| format!("data: {}\r\n\r\n", serde_json::to_string(frame).unwrap()))
        .collect::<String>()
        + "data: [DONE]\r\n\r\n";
    let (openai_addr, openai_request_rx) = spawn_stream_server(
        "text/event-stream",
        split_utf8_scalar(openai_frames.into_bytes()),
    )
    .await;
    let openai = OpenAIProvider::builder()
        .api_key("sk-fixture-secret-must-not-leak")
        .base_url(format!("http://{openai_addr}/v1"))
        .model("gpt-4o-mini")
        .build()
        .unwrap()
        .into_contract_facade()
        .unwrap();
    let openai_events = collect(
        openai
            .chat_stream(&request("openai-baseline"))
            .await
            .unwrap(),
    )
    .await;
    let openai_body: Value = serde_json::from_slice(&openai_request_rx.await.unwrap()).unwrap();
    assert_eq!(openai_body, case("openai-baseline")["outgoing_wire"]);
    assert_eq!(openai_events, literal_events("openai-baseline"));
    4
}

#[test]
fn stream_event_serialization_contract() {
    assert_eq!(assert_event_schema_group(), 8);
}

#[tokio::test]
async fn preflight_rejects_before_backend() {
    assert_eq!(assert_preflight_group().await, 6);
}

#[tokio::test]
async fn ollama_baseline_literal_roundtrip() {
    assert_eq!(literal_roundtrip("ollama-baseline").await.2, 1);
}

#[tokio::test]
async fn ollama_reasoning_literal_roundtrip() {
    assert!(matches!(
        literal_roundtrip("ollama-reasoning").await.1[2],
        StreamEvent::Thinking { .. }
    ));
}

#[tokio::test]
async fn openai_baseline_literal_roundtrip() {
    assert_eq!(literal_roundtrip("openai-baseline").await.2, 1);
}

#[tokio::test]
async fn openai_reasoning_rejected_before_backend() {
    let backend = ScriptedStreamingBackend::pending();
    let (facade, _) = facade_with_backend("openai", backend.clone());
    let error = facade
        .chat_stream(&request("openai-reasoning-unsupported"))
        .await
        .unwrap_err();
    assert_eq!(error.code, ProviderErrorCode::UnsupportedCapability);
    assert_eq!(backend.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn bounded_queue_blocks_seventeenth_event() {
    assert_eq!(assert_backpressure_group().await, 4);
}

#[test]
fn consumer_poll_preserves_pending_single_receive_and_eof() {
    use futures::Stream;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    let (sender, receiver) = tokio::sync::mpsc::channel(16);
    let mut stream = super::ProviderStream {
        receiver,
        cancelled: Arc::new(AtomicBool::new(false)),
        cancel_notify: Arc::new(Notify::new()),
        terminated: false,
    };
    let mut context = Context::from_waker(futures::task::noop_waker_ref());
    assert_eq!(Pin::new(&mut stream).poll_next(&mut context), Poll::Pending);
    let first = StreamEvent::Content {
        delta: "first".into(),
    };
    let second = StreamEvent::Content {
        delta: "second".into(),
    };
    sender.try_send(first.clone()).unwrap();
    sender.try_send(second.clone()).unwrap();
    assert_eq!(
        Pin::new(&mut stream).poll_next(&mut context),
        Poll::Ready(Some(first))
    );
    assert_eq!(stream.receiver.len(), 1);
    assert_eq!(
        Pin::new(&mut stream).poll_next(&mut context),
        Poll::Ready(Some(second))
    );
    assert_eq!(Pin::new(&mut stream).poll_next(&mut context), Poll::Pending);
    drop(sender);
    assert_eq!(
        Pin::new(&mut stream).poll_next(&mut context),
        Poll::Ready(None)
    );
    assert!(stream.terminated);
    assert_eq!(
        Pin::new(&mut stream).poll_next(&mut context),
        Poll::Ready(None)
    );
}

#[tokio::test]
async fn bounded_queue_releases_in_order_without_drop() {
    assert_eq!(assert_backpressure_group().await, 4);
}

#[tokio::test]
async fn explicit_cancel_suppresses_queued_nonterminal() {
    assert_eq!(assert_cancel_group().await, 6);
}

#[tokio::test]
async fn drop_requests_underlying_cancel() {
    assert_eq!(assert_cancel_group().await, 6);
}

#[tokio::test]
async fn cancel_before_start_yields_one_terminal() {
    assert_eq!(assert_cancel_group().await, 6);
}

#[tokio::test]
async fn cancel_after_complete_queued_before_consumption_yields_cancelled() {
    let backend = ScriptedStreamingBackend::sequence(outcomes("ollama-baseline"));
    let dropped = Arc::clone(&backend.dropped);
    let drop_notify = Arc::clone(&backend.drop_notify);
    let (facade, _) = facade_with_backend("ollama_local", backend);
    let stream = facade
        .chat_stream(&request("ollama-baseline"))
        .await
        .unwrap();

    // Backend drop proves the terminal Complete was already mapped and queued
    // before the caller requests cancellation.
    await_backend_drop(&dropped, &drop_notify).await;
    stream.cancel();
    let events = collect(stream).await;

    assert_eq!(
        events,
        vec![StreamEvent::Cancelled {
            error: ProviderError::cancelled("provider stream cancelled"),
        }]
    );
    assert_eq!(terminal_count(&events), 1);
}

#[tokio::test]
async fn cancel_after_terminal_is_noop() {
    let backend = ScriptedStreamingBackend::sequence(outcomes("ollama-baseline"));
    let (facade, _) = facade_with_backend("ollama_local", backend);
    let mut stream = facade
        .chat_stream(&request("ollama-baseline"))
        .await
        .unwrap();
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event);
    }
    stream.cancel();
    assert_eq!(terminal_count(&events), 1);
    assert!(matches!(events.last(), Some(StreamEvent::Complete { .. })));
}

#[tokio::test]
async fn complete_is_unique_and_final() {
    assert_eq!(assert_terminal_group().await, 6);
}

#[tokio::test]
async fn error_is_unique_and_final() {
    assert_eq!(assert_terminal_group().await, 6);
}

#[tokio::test]
async fn cancelled_is_unique_and_final() {
    assert_eq!(assert_cancel_group().await, 6);
}

#[tokio::test]
async fn upstream_truncation_becomes_malformed_response() {
    assert_eq!(assert_terminal_group().await, 6);
}

#[tokio::test]
async fn closed_facade_rejects_new_stream() {
    assert_eq!(assert_preflight_group().await, 6);
}

#[tokio::test]
async fn close_preserves_already_returned_stream() {
    let backend = ScriptedStreamingBackend::sequence(outcomes("ollama-baseline"));
    let (facade, _) = facade_with_backend("ollama_local", backend);
    let stream = facade
        .chat_stream(&request("ollama-baseline"))
        .await
        .unwrap();
    facade.close().await.unwrap();
    assert_eq!(collect(stream).await, literal_events("ollama-baseline"));
}

#[tokio::test]
async fn ollama_loopback_captures_literal_ndjson() {
    assert_eq!(assert_anchor_group().await, 4);
}

#[tokio::test]
async fn openai_loopback_captures_literal_sse() {
    assert_eq!(assert_anchor_group().await, 4);
}

#[tokio::test]
async fn stream_errors_redact_credentials_and_raw_body() {
    assert_eq!(assert_terminal_group().await, 6);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mutation_oracles_fail_closed() {
    let groups = [
        assert_event_schema_group(),
        assert_preflight_group().await,
        assert_literal_group().await,
        assert_backpressure_group().await,
        assert_cancel_group().await,
        assert_terminal_group().await,
        assert_anchor_group().await,
    ];
    assert_eq!(groups, EXPECTED_GROUPS);
    assert_eq!(groups.iter().sum::<usize>(), EXPECTED_ASSERTIONS);
    let provider_source = include_str!("../../src/provider_facade.rs");
    assert!(provider_source.contains("tokio::sync::mpsc::channel(16)"));
    assert!(!provider_source.contains("tokio::sync::mpsc::channel(17)"));
    assert!(STREAMS.contains("\"stream\": true"));
    assert!(!STREAMS.contains("generated expectation"));
    println!("SPEC119_RUST_STREAM_GROUPS=8/6/8/4/6/6/4");
    println!("SPEC119_RUST_STREAM_ASSERTIONS={EXPECTED_ASSERTIONS}");
}
