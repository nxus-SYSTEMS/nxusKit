use std::net::SocketAddr;
use std::sync::{
    Arc, Barrier, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;

use crate::error::{
    ProviderContractResult, ProviderError, ProviderFailureInput, ProviderTransportFailureKind,
};
use crate::provider::{ProviderBackendOutcome, ProviderContractMapper, ProviderExecutionBackend};
use crate::types::{ProviderRequest, ProviderResponse};

#[derive(Clone)]
pub(super) struct RecordingBackend {
    pub(super) calls: Arc<AtomicUsize>,
    outcome: Arc<Mutex<Option<ProviderBackendOutcome>>>,
    pub(super) entered: Arc<Barrier>,
    pub(super) release: Arc<Barrier>,
    /// Shared with the request/response mapper instrumentation (see
    /// `OrderTrackingMapper`) so a single ledger proves the exact
    /// mapper/backend call order for one `chat` invocation.
    pub(super) events: Arc<Mutex<Vec<&'static str>>>,
    /// The exact wire `ProviderFacade::chat` delivered to `execute`, captured
    /// verbatim so tests can assert against it directly rather than trusting
    /// a value recomputed by calling a mapper a second time.
    captured_wire: Arc<Mutex<Option<Value>>>,
}

impl RecordingBackend {
    pub(super) fn events_snapshot(&self) -> Vec<&'static str> {
        self.events
            .lock()
            .expect("event lock must not be poisoned")
            .clone()
    }

    /// Fail-closed accessor for the captured wire: panics rather than
    /// returning `None` if `execute` was never invoked, so a caller cannot
    /// mistake "backend never called" for "matched an empty wire".
    pub(super) fn captured_wire(&self) -> Value {
        self.captured_wire
            .lock()
            .expect("captured wire lock must not be poisoned")
            .clone()
            .expect("backend must have captured a wire before captured_wire() is read")
    }

    /// `true` only if `execute` has never been invoked; used by expected
    /// request/mapping-failure assertions to prove no wire was captured.
    pub(super) fn captured_wire_is_absent(&self) -> bool {
        self.captured_wire
            .lock()
            .expect("captured wire lock must not be poisoned")
            .is_none()
    }
}

#[async_trait]
impl ProviderExecutionBackend for RecordingBackend {
    async fn execute(&self, wire: &Value) -> ProviderContractResult<ProviderBackendOutcome> {
        *self
            .captured_wire
            .lock()
            .expect("captured wire lock must not be poisoned") = Some(wire.clone());
        self.events
            .lock()
            .expect("event lock must not be poisoned")
            .push("backend_execute");
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.entered.wait();
        self.release.wait();
        self.outcome
            .lock()
            .expect("backend outcome lock must not be poisoned")
            .take()
            .ok_or_else(|| ProviderError::internal("recording backend outcome already consumed"))
    }
}

/// Wraps a real `ProviderContractMapper` and records the order in which its
/// three methods are invoked into a ledger shared with a `RecordingBackend`.
/// Proves the facade performs exactly one request mapping before the
/// backend call and exactly one response/failure mapping after it, rather
/// than merely asserting the backend call count.
pub(super) struct OrderTrackingMapper {
    pub(super) inner: Arc<dyn ProviderContractMapper>,
    pub(super) events: Arc<Mutex<Vec<&'static str>>>,
}

impl ProviderContractMapper for OrderTrackingMapper {
    fn request_to_wire(&self, request: &ProviderRequest) -> ProviderContractResult<Value> {
        let result = self.inner.request_to_wire(request);
        self.events
            .lock()
            .expect("event lock must not be poisoned")
            .push("request_to_wire");
        result
    }

    fn response_from_wire(&self, wire: &Value) -> ProviderContractResult<ProviderResponse> {
        let result = self.inner.response_from_wire(wire);
        self.events
            .lock()
            .expect("event lock must not be poisoned")
            .push("response_from_wire");
        result
    }

    fn error_from_failure(&self, failure: &ProviderFailureInput) -> ProviderError {
        let result = self.inner.error_from_failure(failure);
        self.events
            .lock()
            .expect("event lock must not be poisoned")
            .push("error_from_failure");
        result
    }
}

pub(super) fn immediate_backend(outcome: ProviderBackendOutcome) -> RecordingBackend {
    RecordingBackend {
        calls: Arc::new(AtomicUsize::new(0)),
        outcome: Arc::new(Mutex::new(Some(outcome))),
        entered: Arc::new(Barrier::new(1)),
        release: Arc::new(Barrier::new(1)),
        events: Arc::new(Mutex::new(Vec::new())),
        captured_wire: Arc::new(Mutex::new(None)),
    }
}

pub(super) fn blocked_backend(outcome: ProviderBackendOutcome) -> RecordingBackend {
    RecordingBackend {
        calls: Arc::new(AtomicUsize::new(0)),
        outcome: Arc::new(Mutex::new(Some(outcome))),
        entered: Arc::new(Barrier::new(2)),
        release: Arc::new(Barrier::new(2)),
        events: Arc::new(Mutex::new(Vec::new())),
        captured_wire: Arc::new(Mutex::new(None)),
    }
}

pub(super) fn failure(
    status: Option<u16>,
    retry_after: Option<&str>,
    body: Option<&str>,
    kind: ProviderTransportFailureKind,
) -> ProviderBackendOutcome {
    ProviderBackendOutcome::Failure(ProviderFailureInput {
        status,
        retry_after: retry_after.map(str::to_owned),
        body: body.map(str::to_owned),
        transport_kind: kind,
    })
}

/// Starts a local-only loopback HTTP/1.1 listener that accepts exactly one
/// connection, drains the request, writes the given raw HTTP response
/// bytes, then closes the connection. Used to deterministically control
/// exactly what a real anchor's `reqwest::Client` receives — no external
/// network access, credentials, sleeps, or ambient ports.
pub(super) async fn spawn_loopback_server(response: Vec<u8>) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("loopback listener must bind");
    let addr = listener
        .local_addr()
        .expect("loopback listener must have a local address");
    tokio::spawn(async move {
        let (mut socket, _) = listener
            .accept()
            .await
            .expect("loopback listener must accept exactly one connection");
        let mut request_buf = [0_u8; 4096];
        let _ = socket.read(&mut request_buf).await;
        socket
            .write_all(&response)
            .await
            .expect("loopback server must write its response");
        let _ = socket.shutdown().await;
    });
    addr
}

pub(super) fn http_ok_json_response(wire: &Value) -> Vec<u8> {
    let body = serde_json::to_vec(wire).expect("wire fixture must serialize");
    let mut response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(&body);
    response
}

/// Finds the byte offset of the first `\r\n\r\n` header terminator.
fn find_double_crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|window| window == b"\r\n\r\n")
}

/// Reads one HTTP/1.1 request off `socket` and returns its complete body,
/// using the request's own `Content-Length` header for deterministic
/// framing (never a sleep, timeout race, or ambient read count). Retains
/// the full body — unlike `spawn_loopback_server`, which drains and
/// discards it — so real anchor request bytes can be asserted against the
/// independent `wires.json` fixture.
async fn read_full_http_request_body(socket: &mut TcpStream) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut chunk = [0_u8; 4096];
    let header_end = loop {
        let read = socket
            .read(&mut chunk)
            .await
            .expect("loopback read must not fail");
        assert!(
            read > 0,
            "loopback connection closed before request headers were fully received"
        );
        buf.extend_from_slice(&chunk[..read]);
        if let Some(pos) = find_double_crlf(&buf) {
            break pos;
        }
    };
    let header_text =
        std::str::from_utf8(&buf[..header_end]).expect("request headers must be valid UTF-8");
    let content_length: usize = header_text
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.trim()
                .eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    let body_start = header_end + 4;
    while buf.len() < body_start + content_length {
        let read = socket
            .read(&mut chunk)
            .await
            .expect("loopback read must not fail");
        assert!(
            read > 0,
            "loopback connection closed before the request's advertised Content-Length body was \
             fully received"
        );
        buf.extend_from_slice(&chunk[..read]);
    }
    buf[body_start..body_start + content_length].to_vec()
}

/// Like `spawn_loopback_server`, but retains the complete request body (per
/// its own `Content-Length` framing) and delivers it through the returned
/// channel instead of discarding it.
pub(super) async fn spawn_loopback_server_capturing_body(
    response: Vec<u8>,
) -> (SocketAddr, oneshot::Receiver<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("loopback listener must bind");
    let addr = listener
        .local_addr()
        .expect("loopback listener must have a local address");
    let (body_tx, body_rx) = oneshot::channel();
    tokio::spawn(async move {
        let (mut socket, _) = listener
            .accept()
            .await
            .expect("loopback listener must accept exactly one connection");
        let body = read_full_http_request_body(&mut socket).await;
        socket
            .write_all(&response)
            .await
            .expect("loopback server must write its response");
        let _ = socket.shutdown().await;
        let _ = body_tx.send(body);
    });
    (addr, body_rx)
}

/// A syntactically valid `200 OK` whose `Content-Length` promises far more
/// bytes than the connection actually delivers before closing —
/// deterministically reproducing a post-header body read failure that must
/// classify as a transport failure, never a malformed/status-derived
/// response.
pub(super) fn http_ok_truncated_response() -> Vec<u8> {
    let body: &[u8] = b"{\"truncated";
    let mut response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len() + 200
    )
    .into_bytes();
    response.extend_from_slice(body);
    response
}
