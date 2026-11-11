use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::task::{Context, Poll};

use async_trait::async_trait;
use futures::{Stream, StreamExt};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Notify, Semaphore, oneshot};

use crate::error::{ProviderContractResult, ProviderError};
use crate::provider::{
    ProviderBackendOutcome, ProviderBackendStream, ProviderExecutionBackend, ProviderStream,
    ProviderStreamingBackend,
};
use crate::types::StreamEvent;

#[derive(Clone)]
pub(super) struct NoopBackend {
    pub(super) calls: Arc<AtomicUsize>,
}

#[async_trait]
impl ProviderExecutionBackend for NoopBackend {
    async fn execute(&self, _wire: &Value) -> ProviderContractResult<ProviderBackendOutcome> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err(ProviderError::internal(
            "non-streaming backend must not be used by chat_stream",
        ))
    }
}

struct DropObservedStream {
    inner: Pin<Box<dyn Stream<Item = ProviderBackendOutcome> + Send>>,
    dropped: Arc<AtomicBool>,
    drop_notify: Arc<Notify>,
}

impl Stream for DropObservedStream {
    type Item = ProviderBackendOutcome;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}

impl Drop for DropObservedStream {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::SeqCst);
        self.drop_notify.notify_one();
    }
}

pub(super) struct ScriptedStreamingBackend {
    pub(super) calls: Arc<AtomicUsize>,
    stream: Mutex<Option<ProviderBackendStream>>,
    pub(super) dropped: Arc<AtomicBool>,
    pub(super) drop_notify: Arc<Notify>,
}

impl ScriptedStreamingBackend {
    pub(super) fn sequence(outcomes: Vec<ProviderBackendOutcome>) -> Arc<Self> {
        Self::from_stream(Box::pin(futures::stream::iter(outcomes)))
    }

    pub(super) fn pending() -> Arc<Self> {
        Self::from_stream(Box::pin(futures::stream::pending()))
    }

    pub(super) fn from_stream(stream: ProviderBackendStream) -> Arc<Self> {
        let dropped = Arc::new(AtomicBool::new(false));
        let drop_notify = Arc::new(Notify::new());
        let observed: ProviderBackendStream = Box::pin(DropObservedStream {
            inner: stream,
            dropped: Arc::clone(&dropped),
            drop_notify: Arc::clone(&drop_notify),
        });
        Arc::new(Self {
            calls: Arc::new(AtomicUsize::new(0)),
            stream: Mutex::new(Some(observed)),
            dropped,
            drop_notify,
        })
    }
}

#[async_trait]
impl ProviderStreamingBackend for ScriptedStreamingBackend {
    async fn execute_stream(&self, _wire: &Value) -> ProviderContractResult<ProviderBackendStream> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.stream
            .lock()
            .expect("backend stream lock")
            .take()
            .ok_or_else(|| ProviderError::internal("stream backend invoked more than once"))
    }
}

pub(super) struct CountingStream {
    pub(super) remaining: usize,
    pub(super) polls: Arc<AtomicUsize>,
    pub(super) poll_observed: Arc<Semaphore>,
    pub(super) dropped: Arc<AtomicBool>,
}

pub(super) struct FrameThenPendingStream {
    pub(super) outcome: Option<ProviderBackendOutcome>,
    pub(super) processed: Arc<Notify>,
}

impl Stream for FrameThenPendingStream {
    type Item = ProviderBackendOutcome;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(outcome) = self.outcome.take() {
            return Poll::Ready(Some(outcome));
        }
        self.processed.notify_one();
        Poll::Pending
    }
}

impl Stream for CountingStream {
    type Item = ProviderBackendOutcome;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.remaining == 0 {
            return Poll::Ready(None);
        }
        let ordinal = self.polls.fetch_add(1, Ordering::SeqCst);
        self.poll_observed.add_permits(1);
        self.remaining -= 1;
        Poll::Ready(Some(ProviderBackendOutcome::Response(json!({
            "message": {"role": "assistant", "content": format!("chunk-{ordinal}")},
            "done": false
        }))))
    }
}

impl Drop for CountingStream {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::SeqCst);
    }
}

pub(super) async fn await_backend_drop(dropped: &AtomicBool, drop_notify: &Notify) {
    if !dropped.load(Ordering::SeqCst) {
        tokio::time::timeout(std::time::Duration::from_secs(5), drop_notify.notified())
            .await
            .expect("dropping ProviderStream must drop its backend stream");
    }
    assert!(dropped.load(Ordering::SeqCst));
}

pub(super) async fn collect(mut stream: ProviderStream) -> Vec<StreamEvent> {
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event);
    }
    events
}

fn find_double_crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|window| window == b"\r\n\r\n")
}

async fn read_full_http_request_body(socket: &mut TcpStream) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut chunk = [0_u8; 4096];
    let header_end = loop {
        let read = socket.read(&mut chunk).await.expect("loopback read");
        assert!(read > 0, "request headers must complete");
        buf.extend_from_slice(&chunk[..read]);
        if let Some(pos) = find_double_crlf(&buf) {
            break pos;
        }
    };
    let headers = std::str::from_utf8(&buf[..header_end]).expect("UTF-8 headers");
    let length = headers
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
    while buf.len() < body_start + length {
        let read = socket.read(&mut chunk).await.expect("loopback body read");
        assert!(read > 0, "request body must satisfy Content-Length");
        buf.extend_from_slice(&chunk[..read]);
    }
    buf[body_start..body_start + length].to_vec()
}

pub(super) async fn spawn_stream_server(
    content_type: &str,
    body_chunks: Vec<Vec<u8>>,
) -> (SocketAddr, oneshot::Receiver<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = oneshot::channel();
    let content_type = content_type.to_owned();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let request = read_full_http_request_body(&mut socket).await;
        let header = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n"
        );
        let write_result: std::io::Result<()> = async {
            socket.write_all(header.as_bytes()).await?;
            for body in body_chunks {
                socket
                    .write_all(format!("{:X}\r\n", body.len()).as_bytes())
                    .await?;
                socket.write_all(&body).await?;
                socket.write_all(b"\r\n").await?;
                socket.flush().await?;
            }
            socket.write_all(b"0\r\n\r\n").await?;
            Ok(())
        }
        .await;
        finish_stream_response(write_result, request, tx);
        let _ = socket.shutdown().await;
    });
    (addr, rx)
}

fn finish_stream_response(
    result: std::io::Result<()>,
    request: Vec<u8>,
    tx: oneshot::Sender<Vec<u8>>,
) {
    if let Err(error) = result {
        use std::io::ErrorKind::{BrokenPipe, ConnectionAborted, ConnectionReset};
        assert!(
            matches!(
                error.kind(),
                BrokenPipe | ConnectionAborted | ConnectionReset
            ),
            "unexpected loopback response write error: {error}"
        );
    }
    // Preserve requests after intentional peer cancellation, but do not report
    // success before checking unexpected write failures: those must close the
    // request channel so the owning test fails instead of losing a task panic.
    let _ = tx.send(request);
}

#[test]
fn stream_writer_accepts_only_expected_peer_disconnects() {
    use std::io::{Error, ErrorKind};
    for result in [
        Ok(()),
        Err(Error::from(ErrorKind::BrokenPipe)),
        Err(Error::from(ErrorKind::ConnectionAborted)),
        Err(Error::from(ErrorKind::ConnectionReset)),
    ] {
        let (tx, mut rx) = oneshot::channel();
        finish_stream_response(result, b"captured request".to_vec(), tx);
        assert_eq!(rx.try_recv().unwrap(), b"captured request");
    }
}

#[test]
fn stream_writer_fails_on_unexpected_io_errors() {
    use std::io::{Error, ErrorKind};
    for kind in [
        ErrorKind::PermissionDenied,
        ErrorKind::TimedOut,
        ErrorKind::InvalidData,
    ] {
        let (tx, mut rx) = oneshot::channel();
        assert!(
            std::thread::spawn(move || {
                finish_stream_response(Err(Error::from(kind)), b"captured request".to_vec(), tx);
            })
            .join()
            .is_err(),
            "unexpected {kind:?} must not be swallowed"
        );
        assert_eq!(rx.try_recv(), Err(oneshot::error::TryRecvError::Closed));
    }
}

pub(super) fn split_utf8_scalar(mut body: Vec<u8>) -> Vec<Vec<u8>> {
    let scalar = "€".as_bytes();
    let start = body
        .windows(scalar.len())
        .position(|window| window == scalar)
        .expect("stream canary body must contain a multibyte scalar");
    let split = start + 1;
    vec![body.drain(..split).collect(), body]
}
