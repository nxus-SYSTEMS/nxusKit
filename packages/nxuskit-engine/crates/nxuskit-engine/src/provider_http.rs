use crate::error::{NxuskitError, ProviderFailureInput, ProviderTransportFailureKind};
use crate::provider::ProviderBackendOutcome;
use futures::StreamExt;
use serde::de::DeserializeOwned;

pub(crate) const MAX_UNARY_RESPONSE_BODY_BYTES: usize = 16 * 1024 * 1024;
pub(crate) const MAX_STREAM_RECORD_EVENT_BYTES: usize = 4 * 1024 * 1024;
pub(crate) const STREAM_BUFFER_DELIMITER_SLACK_BYTES: usize = 4;
pub(crate) const MAX_STREAM_ACCUMULATOR_BYTES: usize =
    MAX_STREAM_RECORD_EVENT_BYTES + STREAM_BUFFER_DELIMITER_SLACK_BYTES;

#[derive(Debug, thiserror::Error)]
pub(crate) enum BoundedStreamBufferError {
    #[error("stream frame buffer reached bounded ceiling before parser progress")]
    ParserProgressRequired { limit: usize },
}

pub(crate) struct BoundedStreamChunkCursor<'a> {
    chunk: &'a [u8],
    offset: usize,
}

impl<'a> BoundedStreamChunkCursor<'a> {
    pub(crate) fn new(chunk: &'a [u8]) -> Self {
        Self { chunk, offset: 0 }
    }

    /// Copies only the next bounded segment of one transport chunk into the
    /// SDK frame accumulator. Callers must invoke their frame parser between
    /// successful segments so complete frames can be drained before more
    /// transport bytes are copied.
    ///
    /// Returns true when bytes remain in this transport chunk after the append.
    pub(crate) fn append_next(
        &mut self,
        buffer: &mut Vec<u8>,
    ) -> Result<bool, BoundedStreamBufferError> {
        if self.offset >= self.chunk.len() {
            return Ok(false);
        }

        let available = MAX_STREAM_ACCUMULATOR_BYTES.saturating_sub(buffer.len());
        if available == 0 {
            return Err(BoundedStreamBufferError::ParserProgressRequired {
                limit: MAX_STREAM_RECORD_EVENT_BYTES,
            });
        }

        let remaining = &self.chunk[self.offset..];
        let take = remaining.len().min(available);
        buffer.extend_from_slice(&remaining[..take]);
        self.offset += take;
        Ok(self.offset < self.chunk.len())
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum BoundedBodyReadError {
    #[error("transport error: {0}")]
    Transport(#[source] reqwest::Error),
    #[error("response body exceeds {limit} bytes")]
    LimitExceeded { status: u16, limit: usize },
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum BoundedJsonReadError {
    #[error(transparent)]
    Body(#[from] BoundedBodyReadError),
    #[error("response JSON decode failed: {0}")]
    Decode(#[source] serde_json::Error),
}

impl From<BoundedBodyReadError> for NxuskitError {
    fn from(error: BoundedBodyReadError) -> Self {
        match error {
            BoundedBodyReadError::Transport(error) => NxuskitError::Network(error),
            BoundedBodyReadError::LimitExceeded { status, limit } => {
                NxuskitError::provider(status, format!("Response body exceeds {limit} bytes"))
            }
        }
    }
}

impl From<BoundedJsonReadError> for NxuskitError {
    fn from(error: BoundedJsonReadError) -> Self {
        match error {
            BoundedJsonReadError::Body(error) => error.into(),
            BoundedJsonReadError::Decode(error) => NxuskitError::Serialization(error),
        }
    }
}

pub(crate) async fn read_response_body_bounded(
    response: reqwest::Response,
    max_bytes: usize,
) -> Result<Vec<u8>, BoundedBodyReadError> {
    let status = response.status().as_u16();
    let mut stream = response.bytes_stream();
    let mut body = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(BoundedBodyReadError::Transport)?;
        if chunk.len() > max_bytes.saturating_sub(body.len()) {
            return Err(BoundedBodyReadError::LimitExceeded {
                status,
                limit: max_bytes,
            });
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

pub(crate) async fn read_response_text_bounded(
    response: reqwest::Response,
) -> Result<String, BoundedBodyReadError> {
    let body = read_response_body_bounded(response, MAX_UNARY_RESPONSE_BODY_BYTES).await?;
    Ok(String::from_utf8_lossy(&body).into_owned())
}

pub(crate) async fn read_response_json_bounded<T: DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T, BoundedJsonReadError> {
    let body = read_response_body_bounded(response, MAX_UNARY_RESPONSE_BODY_BYTES).await?;
    serde_json::from_slice(&body).map_err(BoundedJsonReadError::Decode)
}

fn transport_failure_kind(error: &reqwest::Error) -> ProviderTransportFailureKind {
    if error.is_timeout() {
        ProviderTransportFailureKind::Timeout
    } else {
        ProviderTransportFailureKind::Network
    }
}

pub(crate) async fn json_backend_outcome(
    send_result: Result<reqwest::Response, reqwest::Error>,
) -> ProviderBackendOutcome {
    let response = match send_result {
        Ok(response) => response,
        Err(error) => {
            let kind = transport_failure_kind(&error);
            return ProviderBackendOutcome::Failure(ProviderFailureInput {
                status: None,
                retry_after: None,
                body: None,
                transport_kind: kind,
            });
        }
    };

    let status = response.status();
    let retry_after = response
        .headers()
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let body_bytes = match read_response_body_bounded(response, MAX_UNARY_RESPONSE_BODY_BYTES).await
    {
        Ok(body_bytes) => body_bytes,
        Err(BoundedBodyReadError::Transport(error)) => {
            let kind = transport_failure_kind(&error);
            return ProviderBackendOutcome::Failure(ProviderFailureInput {
                status: Some(status.as_u16()),
                retry_after,
                body: None,
                transport_kind: kind,
            });
        }
        Err(BoundedBodyReadError::LimitExceeded { .. }) => {
            return ProviderBackendOutcome::Failure(ProviderFailureInput {
                status: Some(status.as_u16()),
                retry_after,
                body: None,
                transport_kind: ProviderTransportFailureKind::None,
            });
        }
    };
    let body_text = String::from_utf8_lossy(&body_bytes).into_owned();

    if !status.is_success() {
        return ProviderBackendOutcome::Failure(ProviderFailureInput {
            status: Some(status.as_u16()),
            retry_after,
            body: Some(body_text),
            transport_kind: ProviderTransportFailureKind::None,
        });
    }

    match serde_json::from_str::<serde_json::Value>(&body_text) {
        Ok(value) => ProviderBackendOutcome::Response(value),
        Err(_) => ProviderBackendOutcome::Failure(ProviderFailureInput {
            status: Some(status.as_u16()),
            retry_after,
            body: Some(body_text),
            transport_kind: ProviderTransportFailureKind::None,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::net::SocketAddr;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};
    use tokio::sync::oneshot;

    /// Drains one raw HTTP request off `socket` up to (and including) the
    /// `\r\n\r\n` header terminator, discarding both the headers and any
    /// body bytes read alongside them. Used only to let a loopback server
    /// consume the client's request before it responds.
    async fn drain_request_headers(socket: &mut TcpStream) {
        let mut buf = Vec::new();
        let mut chunk = [0_u8; 4096];
        loop {
            let read = socket
                .read(&mut chunk)
                .await
                .expect("loopback read must not fail");
            assert!(read > 0, "connection closed before request headers arrived");
            buf.extend_from_slice(&chunk[..read]);
            if buf.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
    }

    #[tokio::test]
    async fn json_backend_outcome_covers_transport_and_response_branches() {
        // 1. A closed localhost port: the connection is refused before any
        //    response is received, producing a non-timeout send error.
        let closed_listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("closed-port listener must bind");
        let closed_addr = closed_listener.local_addr().expect("closed-port addr");
        drop(closed_listener);
        let closed_result = reqwest::Client::new()
            .get(format!("http://{closed_addr}/"))
            .send()
            .await;
        let closed_error = closed_result
            .as_ref()
            .expect_err("a closed port must fail to connect");
        assert!(
            !closed_error.is_timeout(),
            "a refused connection must not classify as a timeout"
        );
        assert!(
            closed_error.is_connect(),
            "a refused connection must classify as a connect error"
        );
        match json_backend_outcome(closed_result).await {
            ProviderBackendOutcome::Failure(failure) => {
                assert_eq!(failure.status, None);
                assert_eq!(failure.retry_after, None);
                assert_eq!(failure.body, None);
                assert_eq!(
                    failure.transport_kind,
                    ProviderTransportFailureKind::Network
                );
            }
            other => panic!("expected a transport failure, got {other:?}"),
        }

        // 2. A server accepts the connection then withholds response
        //    headers beyond a bounded client timeout.
        let headers_listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("withheld-headers listener must bind");
        let headers_addr: SocketAddr = headers_listener
            .local_addr()
            .expect("withheld-headers addr");
        let (headers_release_tx, headers_release_rx) = oneshot::channel::<()>();
        let headers_server = tokio::spawn(async move {
            let (mut socket, _) = headers_listener
                .accept()
                .await
                .expect("withheld-headers listener must accept exactly one connection");
            drain_request_headers(&mut socket).await;
            let _ = headers_release_rx.await;
        });
        let headers_client = reqwest::Client::builder()
            .timeout(Duration::from_millis(200))
            .build()
            .expect("withheld-headers client must build");
        let headers_result = headers_client
            .get(format!("http://{headers_addr}/"))
            .send()
            .await;
        let headers_error = headers_result
            .as_ref()
            .expect_err("withheld response headers must not resolve before the client timeout");
        assert!(
            headers_error.is_timeout(),
            "withheld headers past the client timeout must classify as a timeout, got {headers_error:?}"
        );
        match json_backend_outcome(headers_result).await {
            ProviderBackendOutcome::Failure(failure) => {
                assert_eq!(failure.status, None);
                assert_eq!(failure.retry_after, None);
                assert_eq!(failure.body, None);
                assert_eq!(
                    failure.transport_kind,
                    ProviderTransportFailureKind::Timeout
                );
            }
            other => panic!("expected a transport failure, got {other:?}"),
        }
        let _ = headers_release_tx.send(());
        tokio::time::timeout(Duration::from_secs(5), headers_server)
            .await
            .expect("withheld-headers server task must not deadlock")
            .expect("withheld-headers server task must not panic");

        // 3. A server sends complete 200 headers and an incomplete body,
        //    then withholds the remainder beyond a bounded client timeout.
        let body_listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("withheld-body listener must bind");
        let body_addr: SocketAddr = body_listener.local_addr().expect("withheld-body addr");
        let (body_release_tx, body_release_rx) = oneshot::channel::<()>();
        let body_server = tokio::spawn(async move {
            let (mut socket, _) = body_listener
                .accept()
                .await
                .expect("withheld-body listener must accept exactly one connection");
            drain_request_headers(&mut socket).await;
            let partial_body: &[u8] = b"{\"trunc";
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                partial_body.len() + 100
            );
            socket
                .write_all(header.as_bytes())
                .await
                .expect("withheld-body server must write headers");
            socket
                .write_all(partial_body)
                .await
                .expect("withheld-body server must write its partial body");
            socket.flush().await.expect("withheld-body server flush");
            let _ = body_release_rx.await;
        });
        let body_client = reqwest::Client::builder()
            .timeout(Duration::from_millis(200))
            .build()
            .expect("withheld-body client must build");
        let body_result = body_client.get(format!("http://{body_addr}/")).send().await;
        match json_backend_outcome(body_result).await {
            ProviderBackendOutcome::Failure(failure) => {
                assert_eq!(failure.status, Some(200));
                assert_eq!(failure.retry_after, None);
                assert_eq!(failure.body, None);
                assert_eq!(
                    failure.transport_kind,
                    ProviderTransportFailureKind::Timeout
                );
            }
            other => panic!("expected a transport failure, got {other:?}"),
        }
        let _ = body_release_tx.send(());
        tokio::time::timeout(Duration::from_secs(5), body_server)
            .await
            .expect("withheld-body server task must not deadlock")
            .expect("withheld-body server task must not panic");

        // 4. A complete 429 response with an exact `Retry-After: 2` and
        //    exact body.
        let rate_limited_listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("rate-limited listener must bind");
        let rate_limited_addr: SocketAddr = rate_limited_listener
            .local_addr()
            .expect("rate-limited addr");
        let rate_limited_server = tokio::spawn(async move {
            let (mut socket, _) = rate_limited_listener
                .accept()
                .await
                .expect("rate-limited listener must accept exactly one connection");
            drain_request_headers(&mut socket).await;
            let body: &[u8] = b"{\"error\":\"rate_limited\"}";
            let response = format!(
                "HTTP/1.1 429 Too Many Requests\r\nContent-Type: application/json\r\nRetry-After: 2\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("rate-limited server must write headers");
            socket
                .write_all(body)
                .await
                .expect("rate-limited server must write its body");
            socket
                .shutdown()
                .await
                .expect("rate-limited server must shut down cleanly");
        });
        let rate_limited_result = reqwest::Client::new()
            .get(format!("http://{rate_limited_addr}/"))
            .send()
            .await;
        match json_backend_outcome(rate_limited_result).await {
            ProviderBackendOutcome::Failure(failure) => {
                assert_eq!(failure.status, Some(429));
                assert_eq!(failure.retry_after.as_deref(), Some("2"));
                assert_eq!(
                    failure.body.as_deref(),
                    Some("{\"error\":\"rate_limited\"}")
                );
                assert_eq!(failure.transport_kind, ProviderTransportFailureKind::None);
            }
            other => panic!("expected a status failure, got {other:?}"),
        }
        tokio::time::timeout(Duration::from_secs(5), rate_limited_server)
            .await
            .expect("rate-limited server task must not deadlock")
            .expect("rate-limited server task must not panic");

        // 5. A complete 200 response with an invalid JSON body.
        let invalid_json_listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("invalid-json listener must bind");
        let invalid_json_addr: SocketAddr = invalid_json_listener
            .local_addr()
            .expect("invalid-json addr");
        let invalid_json_server = tokio::spawn(async move {
            let (mut socket, _) = invalid_json_listener
                .accept()
                .await
                .expect("invalid-json listener must accept exactly one connection");
            drain_request_headers(&mut socket).await;
            let body: &[u8] = b"not-json";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("invalid-json server must write headers");
            socket
                .write_all(body)
                .await
                .expect("invalid-json server must write its body");
            socket
                .shutdown()
                .await
                .expect("invalid-json server must shut down cleanly");
        });
        let invalid_json_result = reqwest::Client::new()
            .get(format!("http://{invalid_json_addr}/"))
            .send()
            .await;
        match json_backend_outcome(invalid_json_result).await {
            ProviderBackendOutcome::Failure(failure) => {
                assert_eq!(failure.status, Some(200));
                assert_eq!(failure.retry_after, None);
                assert_eq!(failure.body.as_deref(), Some("not-json"));
                assert_eq!(failure.transport_kind, ProviderTransportFailureKind::None);
            }
            other => panic!("expected a malformed-body failure, got {other:?}"),
        }
        tokio::time::timeout(Duration::from_secs(5), invalid_json_server)
            .await
            .expect("invalid-json server task must not deadlock")
            .expect("invalid-json server task must not panic");

        // 6. A complete 200 response with a valid JSON body.
        let valid_json_listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("valid-json listener must bind");
        let valid_json_addr: SocketAddr =
            valid_json_listener.local_addr().expect("valid-json addr");
        let valid_json_server = tokio::spawn(async move {
            let (mut socket, _) = valid_json_listener
                .accept()
                .await
                .expect("valid-json listener must accept exactly one connection");
            drain_request_headers(&mut socket).await;
            let body = serde_json::to_vec(&json!({"ok": true})).expect("valid-json body encode");
            let mut response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .into_bytes();
            response.extend_from_slice(&body);
            socket
                .write_all(&response)
                .await
                .expect("valid-json server must write its response");
            socket
                .shutdown()
                .await
                .expect("valid-json server must shut down cleanly");
        });
        let valid_json_result = reqwest::Client::new()
            .get(format!("http://{valid_json_addr}/"))
            .send()
            .await;
        match json_backend_outcome(valid_json_result).await {
            ProviderBackendOutcome::Response(value) => {
                assert_eq!(value, json!({"ok": true}));
            }
            other => panic!("expected a successful JSON response, got {other:?}"),
        }
        tokio::time::timeout(Duration::from_secs(5), valid_json_server)
            .await
            .expect("valid-json server task must not deadlock")
            .expect("valid-json server task must not panic");
    }
}

#[cfg(test)]
mod resource_bound_tests {
    use super::*;
    use std::net::SocketAddr;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    fn json_body_exact(len: usize) -> Vec<u8> {
        let prefix = b"{\"x\":\"";
        let suffix = b"\"}";
        let mut body = Vec::with_capacity(len);
        body.extend_from_slice(prefix);
        body.extend(std::iter::repeat_n(b'a', len - prefix.len() - suffix.len()));
        body.extend_from_slice(suffix);
        assert_eq!(body.len(), len);
        body
    }

    async fn drain_headers(socket: &mut TcpStream) {
        let mut buf = Vec::new();
        let mut chunk = [0_u8; 4096];
        loop {
            let n = socket.read(&mut chunk).await.expect("read request");
            if n == 0 {
                return;
            }
            buf.extend_from_slice(&chunk[..n]);
            if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                return;
            }
        }
    }

    async fn response_with_body(
        body: Vec<u8>,
        chunked: bool,
    ) -> (reqwest::Response, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr: SocketAddr = listener.local_addr().expect("addr");
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            drain_headers(&mut socket).await;
            if chunked {
                let header = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n";
                socket.write_all(header.as_bytes()).await.expect("headers");
                socket
                    .write_all(format!("{:x}\r\n", body.len()).as_bytes())
                    .await
                    .expect("chunk size");
                socket.write_all(&body).await.expect("body");
                socket.write_all(b"\r\n0\r\n\r\n").await.expect("chunk end");
            } else {
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                socket.write_all(header.as_bytes()).await.expect("headers");
                socket.write_all(&body).await.expect("body");
            }
            let _ = socket.shutdown().await;
        });
        let response = reqwest::Client::new()
            .get(format!("http://{addr}/"))
            .send()
            .await
            .expect("response");
        (response, server)
    }

    #[tokio::test]
    async fn resource_bound_unary_exact_limit_and_one_over() {
        let (exact, server) = response_with_body(json_body_exact(16 * 1024 * 1024), false).await;
        match json_backend_outcome(Ok(exact)).await {
            ProviderBackendOutcome::Response(value) => assert!(value["x"].is_string()),
            _ => panic!("EXACT_UNARY_LIMIT_MUST_SUCCEED"),
        }
        server.await.expect("server");

        let (over, server) = response_with_body(json_body_exact(16 * 1024 * 1024 + 1), false).await;
        match json_backend_outcome(Ok(over)).await {
            ProviderBackendOutcome::Failure(failure)
                if failure.status == Some(200)
                    && failure.body.is_none()
                    && failure.transport_kind == ProviderTransportFailureKind::None => {}
            _ => panic!("SEMANTIC_RED_UNARY_OVER_ACCEPTED"),
        }
        server.await.expect("server");
    }

    #[tokio::test]
    async fn resource_bound_unary_chunked_one_over() {
        let (over, server) = response_with_body(json_body_exact(16 * 1024 * 1024 + 1), true).await;
        match json_backend_outcome(Ok(over)).await {
            ProviderBackendOutcome::Failure(failure)
                if failure.status == Some(200)
                    && failure.body.is_none()
                    && failure.transport_kind == ProviderTransportFailureKind::None => {}
            _ => panic!("SEMANTIC_RED_UNARY_CHUNKED_OVER_ACCEPTED"),
        }
        server.await.expect("server");
    }
}

#[cfg(test)]
mod unary_helper_contract_tests {
    use super::*;
    use serde_json::Value;
    use std::net::SocketAddr;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    async fn drain_headers(socket: &mut TcpStream) {
        let mut buf = Vec::new();
        let mut chunk = [0_u8; 4096];
        loop {
            let n = socket.read(&mut chunk).await.expect("read request");
            if n == 0 {
                return;
            }
            buf.extend_from_slice(&chunk[..n]);
            if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                return;
            }
        }
    }

    async fn response(body: Vec<u8>) -> reqwest::Response {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr: SocketAddr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            drain_headers(&mut socket).await;
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            socket.write_all(header.as_bytes()).await.expect("headers");
            socket.write_all(&body).await.expect("body");
            let _ = socket.shutdown().await;
        });
        reqwest::Client::new()
            .get(format!("http://{addr}/"))
            .send()
            .await
            .expect("response")
    }

    #[tokio::test]
    async fn unary_helper_json_decode_maps_serialization() {
        let response = response(b"not-json".to_vec()).await;
        let error = read_response_json_bounded::<Value>(response)
            .await
            .expect_err("malformed JSON must fail");
        let mapped: crate::error::NxuskitError = error.into();
        assert!(matches!(
            mapped,
            crate::error::NxuskitError::Serialization(_)
        ));
    }

    #[tokio::test]
    async fn unary_helper_limit_maps_existing_provider_variant() {
        let response = response(vec![b'a'; MAX_UNARY_RESPONSE_BODY_BYTES + 1]).await;
        let error = read_response_text_bounded(response)
            .await
            .expect_err("one-byte-over body must fail");
        match error {
            BoundedBodyReadError::LimitExceeded { status, limit } => {
                assert_eq!(status, 200);
                assert_eq!(limit, MAX_UNARY_RESPONSE_BODY_BYTES);
                let mapped: crate::error::NxuskitError =
                    BoundedBodyReadError::LimitExceeded { status, limit }.into();
                match mapped {
                    crate::error::NxuskitError::Provider { status, message } => {
                        assert_eq!(status, 200);
                        assert!(message.contains("16777216"));
                    }
                    _ => panic!("LIMIT_MUST_MAP_TO_EXISTING_PROVIDER_VARIANT"),
                }
            }
            _ => panic!("ONE_BYTE_OVER_MUST_BE_LIMIT_ERROR"),
        }
    }

    #[tokio::test]
    async fn unary_helper_text_preserves_small_body() {
        let response = response(b"{\"error\":\"hello\"}".to_vec()).await;
        let text = read_response_text_bounded(response)
            .await
            .expect("bounded text");
        assert_eq!(text, "{\"error\":\"hello\"}");
    }
}

#[cfg(test)]
mod stream_chunk_cursor_contract_tests {
    use super::*;

    #[test]
    fn stream_chunk_cursor_caps_sdk_accumulator() {
        let ceiling = MAX_STREAM_RECORD_EVENT_BYTES + STREAM_BUFFER_DELIMITER_SLACK_BYTES;
        let chunk = vec![b'x'; ceiling + 10];
        let mut cursor = BoundedStreamChunkCursor::new(&chunk);
        let mut buffer = Vec::new();

        assert!(
            cursor
                .append_next(&mut buffer)
                .expect("first bounded append")
        );
        assert_eq!(buffer.len(), ceiling);
        assert!(matches!(
            cursor.append_next(&mut buffer),
            Err(BoundedStreamBufferError::ParserProgressRequired { limit })
                if limit == MAX_STREAM_RECORD_EVENT_BYTES
        ));

        buffer.drain(..MAX_STREAM_RECORD_EVENT_BYTES);
        assert!(
            !cursor
                .append_next(&mut buffer)
                .expect("resume after parser progress")
        );
        assert_eq!(buffer.len(), STREAM_BUFFER_DELIMITER_SLACK_BYTES + 10);
        assert!(buffer.len() <= ceiling);
    }

    #[test]
    fn stream_chunk_cursor_allows_large_transport_chunk_after_parser_progress() {
        let ceiling = MAX_STREAM_RECORD_EVENT_BYTES + STREAM_BUFFER_DELIMITER_SLACK_BYTES;
        let chunk = vec![b'x'; ceiling * 2 + 17];
        let mut cursor = BoundedStreamChunkCursor::new(&chunk);
        let mut buffer = Vec::new();
        let mut observed_max = 0usize;

        assert!(cursor.append_next(&mut buffer).expect("segment one"));
        observed_max = observed_max.max(buffer.len());
        buffer.clear();

        assert!(cursor.append_next(&mut buffer).expect("segment two"));
        observed_max = observed_max.max(buffer.len());
        buffer.clear();

        assert!(!cursor.append_next(&mut buffer).expect("segment three"));
        observed_max = observed_max.max(buffer.len());

        assert_eq!(buffer.len(), 17);
        assert!(observed_max <= ceiling);
    }

    #[test]
    fn stream_chunk_cursor_empty_chunk_is_complete_without_growth() {
        let mut cursor = BoundedStreamChunkCursor::new(&[]);
        let mut buffer = vec![b'x'; 7];
        assert!(!cursor.append_next(&mut buffer).expect("empty chunk"));
        assert_eq!(buffer.len(), 7);
    }
}
