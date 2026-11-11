// ============================================================================
// Spec 119 Sprint 1A: canonical Rust provider contract mapper seam
//
// `ProviderContractMapper` is the pure, object-safe boundary between a
// canonical `ProviderRequest`/`ProviderResponse` and one provider's wire
// format. It is independent of `LLMProvider` above: `LLMProvider`
// implementations own transport (HTTP calls, streaming); a
// `ProviderContractMapper` only translates DTOs to and from
// `serde_json::Value` wire representations and classifies failures, with no
// I/O of its own. Concrete implementations (`OllamaContractMapper`,
// `OpenAIContractMapper`, ...) are introduced by a later Sprint 1A task.
// ============================================================================
use async_trait::async_trait;
use futures::{Stream, StreamExt};

pub trait ProviderContractMapper: Send + Sync {
    /// Maps a validated `ProviderRequest` to this provider's wire request
    /// body.
    fn request_to_wire(
        &self,
        request: &crate::types::ProviderRequest,
    ) -> crate::error::ProviderContractResult<serde_json::Value>;

    /// Maps this provider's raw wire response body to a canonical
    /// `ProviderResponse`.
    fn response_from_wire(
        &self,
        wire: &serde_json::Value,
    ) -> crate::error::ProviderContractResult<crate::types::ProviderResponse>;

    /// Classifies a failed operation into a canonical `ProviderError`.
    ///
    /// This is infallible: every `ProviderFailureInput` maps to exactly one
    /// `ProviderError`.
    fn error_from_failure(
        &self,
        failure: &crate::error::ProviderFailureInput,
    ) -> crate::error::ProviderError;
}

// ============================================================================
// Spec 119 Sprint 1B: ProviderFacade — canonical async, non-streaming Rust
// provider facade
//
// `ProviderFacade` composes exactly one `ProviderContractMapper` with exactly
// one injected `ProviderExecutionBackend` to execute a single canonical
// `chat` call: one request mapping, one backend call, and one
// response/error mapping. The backend performs the actual transport; the
// facade itself performs no I/O. `close()` marks the facade closed without
// waiting for in-flight calls: an in-flight call may still finish, while new
// calls made after `close()` fail with `ProviderErrorCode::Unavailable`
// before the backend is invoked.
// ============================================================================

/// The outcome of a single `ProviderExecutionBackend::execute` call: either
/// a raw provider wire response to map via
/// `ProviderContractMapper::response_from_wire`, or a failure input to
/// classify via `ProviderContractMapper::error_from_failure`, or an already
/// classified protocol error discovered while framing a streaming response.
#[derive(Debug, Clone)]
pub(crate) enum ProviderBackendOutcome {
    /// A raw wire response received from the provider.
    Response(serde_json::Value),
    /// A failure input to classify into a canonical `ProviderError`.
    Failure(crate::error::ProviderFailureInput),
    /// A protocol error that must be exposed without failure reclassification.
    ProtocolError(crate::error::ProviderError),
}

/// Test-governed async transport seam for `ProviderFacade`. Concrete
/// implementations perform the actual provider call (HTTP, FFI, ...);
/// `ProviderFacade` itself never performs I/O.
#[async_trait]
pub(crate) trait ProviderExecutionBackend: Send + Sync {
    /// Executes a single mapped wire request and returns its outcome.
    async fn execute(
        &self,
        wire: &serde_json::Value,
    ) -> crate::error::ProviderContractResult<ProviderBackendOutcome>;
}

// ============================================================================
// Spec 119 Sprint 1C: streaming seam for `ProviderFacade::chat_stream`.
//
// `ProviderStreamMapper` is the streaming counterpart of
// `ProviderContractMapper`: pure DTO/wire translation with no I/O, mapping a
// `ProviderRequest` to a provider's streaming wire request body and a single
// raw wire frame to zero or more canonical `StreamEvent`s. A mapper that
// supports both `chat` and `chat_stream` implements both traits (see
// `OllamaContractMapper`, `OpenAIContractMapper`).
//
// `ProviderStreamingBackend` is the streaming counterpart of
// `ProviderExecutionBackend`: it establishes and returns exactly one already
// -connected stream of raw backend outcomes per `chat_stream` call.
// `ProviderFacade` drives that raw stream into a bounded (capacity 16)
// canonical `StreamEvent` queue on a background task, exposed to the caller
// as a `ProviderStream`.
// ============================================================================

/// Boxed stream of raw backend outcomes returned by
/// `ProviderStreamingBackend::execute_stream`.
pub(crate) type ProviderBackendStream =
    std::pin::Pin<Box<dyn Stream<Item = ProviderBackendOutcome> + Send>>;

/// Test-governed async transport seam for `ProviderFacade::chat_stream`.
/// Concrete implementations perform the actual provider streaming call
/// (HTTP, FFI, ...); `ProviderFacade` itself never performs I/O.
#[async_trait]
pub(crate) trait ProviderStreamingBackend: Send + Sync {
    /// Establishes and returns a single already-connected stream of raw
    /// backend outcomes for the given mapped streaming wire request.
    async fn execute_stream(
        &self,
        wire: &serde_json::Value,
    ) -> crate::error::ProviderContractResult<ProviderBackendStream>;
}

/// Pure, object-safe mapping boundary between a canonical
/// `ProviderRequest`/`StreamEvent` sequence and one provider's streaming
/// wire format. No I/O of its own.
pub(crate) trait ProviderStreamMapper: Send + Sync {
    /// Maps a validated `ProviderRequest` to this provider's streaming wire
    /// request body.
    fn request_to_stream_wire(
        &self,
        request: &crate::types::ProviderRequest,
    ) -> crate::error::ProviderContractResult<serde_json::Value>;

    /// Maps a single raw streaming wire frame to zero or more canonical
    /// `StreamEvent`s, in emission order.
    fn stream_events_from_wire(
        &self,
        wire: &serde_json::Value,
    ) -> crate::error::ProviderContractResult<Vec<crate::types::StreamEvent>>;
}

fn is_terminal_stream_event(event: &crate::types::StreamEvent) -> bool {
    matches!(
        event,
        crate::types::StreamEvent::Complete { .. }
            | crate::types::StreamEvent::Error { .. }
            | crate::types::StreamEvent::Cancelled { .. }
    )
}

/// Drives one already-established raw backend stream into `tx`, prefixed by
/// a synthetic `Start` event, applying cooperative cancellation and the
/// exactly-one-terminal-event rule. Runs on its own spawned task, detached
/// from the `ProviderStream` handle returned to the caller.
async fn drive_provider_stream(
    tx: tokio::sync::mpsc::Sender<crate::types::StreamEvent>,
    mut backend_stream: ProviderBackendStream,
    mapper: std::sync::Arc<dyn ProviderContractMapper>,
    stream_mapper: std::sync::Arc<dyn ProviderStreamMapper>,
    cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    cancel_notify: std::sync::Arc<tokio::sync::Notify>,
) {
    use crate::types::StreamEvent;

    if tx.send(StreamEvent::Start).await.is_err() {
        return;
    }

    loop {
        if cancelled.load(std::sync::atomic::Ordering::SeqCst) {
            let _ = tx
                .send(StreamEvent::Cancelled {
                    error: crate::error::ProviderError::cancelled("provider stream cancelled"),
                })
                .await;
            return;
        }

        tokio::select! {
            _ = cancel_notify.notified() => {
                let _ = tx
                    .send(StreamEvent::Cancelled {
                        error: crate::error::ProviderError::cancelled("provider stream cancelled"),
                    })
                    .await;
                return;
            }
            outcome = backend_stream.next() => {
                match outcome {
                    None => {
                        let _ = tx
                            .send(StreamEvent::Error {
                                error: crate::error::ProviderError::malformed_response(
                                    "provider stream ended before a terminal event",
                                ),
                            })
                            .await;
                        return;
                    }
                    Some(ProviderBackendOutcome::Failure(failure)) => {
                        let error = mapper.error_from_failure(&failure);
                        let _ = tx.send(StreamEvent::Error { error }).await;
                        return;
                    }
                    Some(ProviderBackendOutcome::ProtocolError(error)) => {
                        let _ = tx.send(StreamEvent::Error { error }).await;
                        return;
                    }
                    Some(ProviderBackendOutcome::Response(raw)) => {
                        match stream_mapper.stream_events_from_wire(&raw) {
                            Ok(events) => {
                                for event in events {
                                    let terminal = is_terminal_stream_event(&event);
                                    if tx.send(event).await.is_err() {
                                        return;
                                    }
                                    if terminal {
                                        return;
                                    }
                                }
                            }
                            Err(error) => {
                                let _ = tx.send(StreamEvent::Error { error }).await;
                                return;
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A single canonical event stream returned by `ProviderFacade::chat_stream`.
///
/// Backed by a bounded (capacity 16) queue: the backend cannot outrun a slow
/// consumer past 16 buffered events. Exactly one terminal event (`Complete`,
/// `Error`, or `Cancelled`) is ever yielded, and no event follows it.
/// `cancel()` requests cancellation, suppresses any already-queued
/// nonterminal event, and causes the next yielded event to be the terminal
/// `Cancelled` event. Dropping the stream without calling `cancel()` first
/// has the same cancelling effect on the underlying backend.
pub struct ProviderStream {
    receiver: tokio::sync::mpsc::Receiver<crate::types::StreamEvent>,
    cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    cancel_notify: std::sync::Arc<tokio::sync::Notify>,
    terminated: bool,
}

impl ProviderStream {
    /// Requests cancellation. Idempotent.
    pub fn cancel(&self) {
        self.cancelled
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.cancel_notify.notify_one();
    }

    /// Returns `true` once `cancel()` has been called.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl std::fmt::Debug for ProviderStream {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProviderStream")
            .field("is_cancelled", &self.is_cancelled())
            .finish_non_exhaustive()
    }
}

impl Stream for ProviderStream {
    type Item = crate::types::StreamEvent;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.terminated {
            return std::task::Poll::Ready(None);
        }
        if this.cancelled.load(std::sync::atomic::Ordering::SeqCst) {
            this.terminated = true;
            return std::task::Poll::Ready(Some(crate::types::StreamEvent::Cancelled {
                error: crate::error::ProviderError::cancelled("provider stream cancelled"),
            }));
        }
        match this.receiver.poll_recv(cx) {
            std::task::Poll::Ready(Some(event)) => {
                if this.cancelled.load(std::sync::atomic::Ordering::SeqCst) {
                    this.terminated = true;
                    return std::task::Poll::Ready(Some(crate::types::StreamEvent::Cancelled {
                        error: crate::error::ProviderError::cancelled("provider stream cancelled"),
                    }));
                }
                if is_terminal_stream_event(&event) {
                    this.terminated = true;
                }
                std::task::Poll::Ready(Some(event))
            }
            std::task::Poll::Ready(None) => {
                this.terminated = true;
                std::task::Poll::Ready(None)
            }
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}

impl Drop for ProviderStream {
    fn drop(&mut self) {
        self.cancelled
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.cancel_notify.notify_one();
    }
}

/// Public async, non-streaming facade over one accepted provider mapper and
/// one injected execution backend.
///
/// Each accepted `chat` call performs exactly one request mapping, one
/// backend call, and one response/error mapping. `close()` is idempotent
/// and does not wait for in-flight calls; once closed, new calls fail with
/// `ProviderErrorCode::Unavailable` before the backend is invoked.
pub struct ProviderFacade {
    mapper: std::sync::Arc<dyn ProviderContractMapper>,
    backend: std::sync::Arc<dyn ProviderExecutionBackend>,
    stream_mapper: Option<std::sync::Arc<dyn ProviderStreamMapper>>,
    streaming_backend: Option<std::sync::Arc<dyn ProviderStreamingBackend>>,
    capabilities: crate::types::ProviderCapabilities,
    closed: std::sync::atomic::AtomicBool,
}

impl std::fmt::Debug for ProviderFacade {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProviderFacade")
            .field("closed", &self.is_closed())
            .finish_non_exhaustive()
    }
}

impl ProviderFacade {
    /// Constructs a test-only non-streaming facade over the given mapper,
    /// backend, and capability snapshot.
    #[cfg(test)]
    pub(crate) fn new(
        mapper: std::sync::Arc<dyn ProviderContractMapper>,
        backend: std::sync::Arc<dyn ProviderExecutionBackend>,
        capabilities: crate::types::ProviderCapabilities,
    ) -> Self {
        Self {
            mapper,
            backend,
            stream_mapper: None,
            streaming_backend: None,
            capabilities,
            closed: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Constructs a facade over the given mapper, non-streaming backend,
    /// stream mapper, streaming backend, and capability snapshot.
    pub(crate) fn new_with_streaming(
        mapper: std::sync::Arc<dyn ProviderContractMapper>,
        backend: std::sync::Arc<dyn ProviderExecutionBackend>,
        stream_mapper: std::sync::Arc<dyn ProviderStreamMapper>,
        streaming_backend: std::sync::Arc<dyn ProviderStreamingBackend>,
        capabilities: crate::types::ProviderCapabilities,
    ) -> Self {
        Self {
            mapper,
            backend,
            stream_mapper: Some(stream_mapper),
            streaming_backend: Some(streaming_backend),
            capabilities,
            closed: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Returns `true` once `close()` has been called.
    pub fn is_closed(&self) -> bool {
        self.closed.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Returns the capability snapshot supplied at construction. Stable
    /// across `close()`.
    pub fn capabilities(&self) -> crate::types::ProviderCapabilities {
        self.capabilities.clone()
    }

    /// Marks the facade closed. Idempotent; does not wait for in-flight
    /// `chat` calls, which may still complete.
    pub async fn close(&self) -> crate::error::ProviderContractResult<()> {
        self.closed.store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    /// Executes exactly one accepted `chat` call: validates the request,
    /// maps it to wire, invokes the backend once, and maps the outcome
    /// back to a canonical response or error.
    pub async fn chat(
        &self,
        request: &crate::types::ProviderRequest,
    ) -> crate::error::ProviderContractResult<crate::types::ProviderResponse> {
        if self.is_closed() {
            return Err(crate::error::ProviderError::unavailable(
                "provider facade is closed",
            ));
        }
        request.validate()?;
        let wire = self.mapper.request_to_wire(request)?;
        match self.backend.execute(&wire).await? {
            ProviderBackendOutcome::Response(raw) => self.mapper.response_from_wire(&raw),
            ProviderBackendOutcome::Failure(failure) => {
                Err(self.mapper.error_from_failure(&failure))
            }
            ProviderBackendOutcome::ProtocolError(error) => Err(error),
        }
    }

    /// Executes exactly one accepted `chat_stream` call: validates the
    /// request, maps it to a streaming wire request, invokes the streaming
    /// backend once, and returns a `ProviderStream` of canonical
    /// `StreamEvent`s. Preflight failures (invalid request, closed facade,
    /// unsupported capability) return before the backend is invoked.
    pub async fn chat_stream(
        &self,
        request: &crate::types::ProviderRequest,
    ) -> crate::error::ProviderContractResult<ProviderStream> {
        if self.is_closed() {
            return Err(crate::error::ProviderError::unavailable(
                "provider facade is closed",
            ));
        }
        let (stream_mapper, streaming_backend) =
            match (&self.stream_mapper, &self.streaming_backend) {
                (Some(stream_mapper), Some(streaming_backend)) => {
                    (stream_mapper.clone(), streaming_backend.clone())
                }
                _ => {
                    return Err(crate::error::ProviderError::unsupported_capability(
                        "chat_stream",
                        "this provider facade does not support streaming",
                    ));
                }
            };
        request.validate()?;
        let wire = stream_mapper.request_to_stream_wire(request)?;
        let backend_stream = streaming_backend.execute_stream(&wire).await?;

        let (tx, rx) = tokio::sync::mpsc::channel(16);
        let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cancel_notify = std::sync::Arc::new(tokio::sync::Notify::new());
        tokio::spawn(drive_provider_stream(
            tx,
            backend_stream,
            self.mapper.clone(),
            stream_mapper,
            cancelled.clone(),
            cancel_notify.clone(),
        ));

        Ok(ProviderStream {
            receiver: rx,
            cancelled,
            cancel_notify,
            terminated: false,
        })
    }
}

#[cfg(test)]
#[path = "../tests/support/provider_facade_tests.rs"]
mod spec119_facade_tests;

#[cfg(test)]
#[path = "../tests/support/provider_stream_tests.rs"]
mod spec119_stream_tests;
