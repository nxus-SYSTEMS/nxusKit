use std::collections::{BTreeSet, HashMap};
use std::ffi::OsString;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::{Notify, oneshot};

use crate::discovery::{
    DiscoveryBackend, DiscoveryClient, DiscoveryClientFactory, DiscoveryConfig,
    DiscoveryCredentialResolver,
};
use crate::error::{
    ProviderContractResult, ProviderError, ProviderErrorCode, ProviderFailureInput,
};
use crate::provider::{
    ProviderBackendOutcome, ProviderContractMapper, ProviderExecutionBackend, ProviderFacade,
};
use crate::types::{ModelInfo, ProviderCapabilities, ProviderRequest, ProviderResponse};

const FIXTURE: &str = include_str!("../fixtures/provider_parity/discovery.json");
const DISCOVERY_SOURCE: &str = include_str!("../../src/discovery.rs");
const EXPECTED_GROUPS: [&str; 6] = [
    "config_factory",
    "route_auth",
    "success_mapping",
    "http_errors",
    "malformed_redaction",
    "lifecycle_isolation",
];

fn fixture() -> Value {
    serde_json::from_str(FIXTURE).expect("discovery fixture must be valid JSON")
}

fn assert_fixture_contract() {
    let value = fixture();
    assert_eq!(value["schema"], "nxus-spec119-rust-discovery-fixtures/v1");
    assert_eq!(
        value["assertion_marker"],
        "SPEC119_RUST_DISCOVERY_ASSERTIONS=48"
    );
    let groups = value["assertion_groups"]
        .as_object()
        .expect("assertion_groups object");
    assert_eq!(groups.len(), 6);
    let mut all = BTreeSet::new();
    for group in EXPECTED_GROUPS {
        let ids = groups[group].as_array().expect("group array");
        assert_eq!(ids.len(), 8, "{group} must contain eight assertions");
        for id in ids {
            assert!(all.insert(id.as_str().expect("assertion id")));
        }
    }
    assert_eq!(all.len(), 48);
}

fn config(provider_id: &str, endpoint: String) -> DiscoveryConfig {
    DiscoveryConfig {
        provider_id: provider_id.to_owned(),
        endpoint,
        auth_selector: (provider_id == "openai").then(|| "SPEC119_DISCOVERY_TOKEN".to_owned()),
        connect_timeout_ms: 500,
        read_timeout_ms: 500,
    }
}

#[derive(Clone)]
struct StaticResolver {
    value: Option<OsString>,
    calls: Arc<AtomicUsize>,
}

impl StaticResolver {
    fn new(value: Option<OsString>) -> Self {
        Self {
            value,
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl DiscoveryCredentialResolver for StaticResolver {
    fn resolve(&self, _selector: &str) -> Option<OsString> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.value.clone()
    }
}

fn resolver(value: Option<&str>) -> Arc<StaticResolver> {
    Arc::new(StaticResolver::new(value.map(OsString::from)))
}

fn non_unicode_os_string() -> OsString {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        OsString::from_vec(vec![0xff])
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        OsString::from_wide(&[0xd800])
    }
}

struct CapturedRequest {
    request_line: String,
    authorization: Option<String>,
}

enum ServerReply {
    Complete(Vec<u8>),
    StallAfterHeaders(Vec<u8>),
    CloseWithoutResponse,
}

fn http_response(status: u16, extra_headers: &[(&str, &str)], body: &[u8]) -> Vec<u8> {
    let reason = match status {
        200 => "OK",
        401 => "Unauthorized",
        403 => "Forbidden",
        418 => "I'm a Teapot",
        429 => "Too Many Requests",
        503 => "Service Unavailable",
        _ => "Fixture",
    };
    let mut response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    )
    .into_bytes();
    for (name, value) in extra_headers {
        response.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
    }
    response.extend_from_slice(b"\r\n");
    response.extend_from_slice(body);
    response
}

async fn spawn_server(reply: ServerReply) -> (String, oneshot::Receiver<CapturedRequest>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("loopback listener");
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = oneshot::channel();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("one request");
        let mut bytes = Vec::new();
        let mut chunk = [0_u8; 1024];
        while !bytes.windows(4).any(|window| window == b"\r\n\r\n") {
            let count = socket.read(&mut chunk).await.expect("request read");
            assert!(count > 0, "request headers must complete");
            bytes.extend_from_slice(&chunk[..count]);
        }
        let head = String::from_utf8(bytes).expect("request headers UTF-8");
        let mut lines = head.lines();
        let request_line = lines.next().unwrap().to_owned();
        let authorization = lines.find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("authorization")
                .then(|| value.trim().to_owned())
        });
        let _ = tx.send(CapturedRequest {
            request_line,
            authorization,
        });
        match reply {
            ServerReply::CloseWithoutResponse => {
                socket
                    .shutdown()
                    .await
                    .expect("close without an HTTP response");
            }
            ServerReply::Complete(response) => {
                socket.write_all(&response).await.expect("response write");
            }
            ServerReply::StallAfterHeaders(headers) => {
                socket.write_all(&headers).await.expect("header write");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
        let _ = socket.shutdown().await;
    });
    (endpoint, rx)
}

async fn client_for(
    provider: &str,
    endpoint: String,
    resolver: Arc<dyn DiscoveryCredentialResolver>,
) -> DiscoveryClient {
    DiscoveryClientFactory::create_with_resolver(config(provider, endpoint), resolver)
        .expect("fixture client")
}

async fn request_once(
    provider: &str,
    endpoint_suffix: &str,
    body: &[u8],
) -> (Vec<ModelInfo>, CapturedRequest) {
    let (endpoint, captured) =
        spawn_server(ServerReply::Complete(http_response(200, &[], body))).await;
    let client = client_for(
        provider,
        format!("{endpoint}{endpoint_suffix}"),
        resolver(Some("fixture-secret")),
    )
    .await;
    let models = client.list_models().await.expect("fixture list success");
    (models, captured.await.expect("captured request"))
}

fn assert_error(error: ProviderError, code: ProviderErrorCode, message: &str) {
    assert_eq!(error.code, code);
    assert_eq!(error.message, message);
    assert!(error.retry_after_ms.is_none());
}

fn assert_discovery_transport_policy_is_single_attempt() {
    let function = DISCOVERY_SOURCE
        .split_once("pub(crate) fn build_discovery_http_client(")
        .expect("discovery client builder must exist")
        .1
        .split_once("\nenum DiscoveryProfile")
        .expect("discovery client builder must end before profile dispatch")
        .0;
    assert_eq!(
        function
            .matches(".redirect(reqwest::redirect::Policy::none())")
            .count(),
        1,
        "discovery client must disable redirects exactly once"
    );
    assert_eq!(
        function.matches(".retry(reqwest::retry::never())").count(),
        1,
        "discovery client must disable automatic protocol retries exactly once"
    );
}

struct IsolationMapper;

impl ProviderContractMapper for IsolationMapper {
    fn request_to_wire(&self, _request: &ProviderRequest) -> ProviderContractResult<Value> {
        unreachable!("isolation test never performs inference")
    }

    fn response_from_wire(&self, _wire: &Value) -> ProviderContractResult<ProviderResponse> {
        unreachable!("isolation test never performs inference")
    }

    fn error_from_failure(&self, _failure: &ProviderFailureInput) -> ProviderError {
        unreachable!("isolation test never performs inference")
    }
}

struct IsolationBackend;

#[async_trait]
impl ProviderExecutionBackend for IsolationBackend {
    async fn execute(&self, _wire: &Value) -> ProviderContractResult<ProviderBackendOutcome> {
        unreachable!("isolation test never performs inference")
    }
}

fn isolation_facade() -> ProviderFacade {
    ProviderFacade::new(
        Arc::new(IsolationMapper),
        Arc::new(IsolationBackend),
        ProviderCapabilities::default(),
    )
}

#[test]
fn spec119_discovery_config_factory_rejects_identity_and_endpoint_drift() {
    assert_fixture_contract();
    for (provider, endpoint, field) in [
        ("", "http://localhost", "provider_id"),
        ("openai", "ftp://localhost", "endpoint"),
        ("openai", "http://user@localhost", "endpoint"),
        ("openai", "http://localhost/path?q=1", "endpoint"),
        ("openai", "http://localhost/path#fragment", "endpoint"),
    ] {
        let error =
            DiscoveryClientFactory::create(config(provider, endpoint.to_owned())).unwrap_err();
        assert_eq!(error.code, ProviderErrorCode::InvalidRequest);
        assert_eq!(error.field.as_deref(), Some(field));
    }
}

#[test]
fn spec119_discovery_config_factory_rejects_bounds_selector_and_provider_drift() {
    for value in [0, 300_001] {
        let mut candidate = config("openai", "http://localhost".to_owned());
        candidate.connect_timeout_ms = value;
        assert_eq!(
            DiscoveryClientFactory::create(candidate)
                .unwrap_err()
                .field
                .as_deref(),
            Some("connect_timeout_ms")
        );
    }
    for value in [0, 300_001] {
        let mut candidate = config("openai", "http://localhost".to_owned());
        candidate.read_timeout_ms = value;
        assert_eq!(
            DiscoveryClientFactory::create(candidate)
                .unwrap_err()
                .field
                .as_deref(),
            Some("read_timeout_ms")
        );
    }
    for selector in ["", "lowercase", &"A".repeat(129)] {
        let mut candidate = config("openai", "http://localhost".to_owned());
        candidate.auth_selector = Some(selector.to_owned());
        assert_eq!(
            DiscoveryClientFactory::create(candidate)
                .unwrap_err()
                .field
                .as_deref(),
            Some("auth_selector")
        );
    }
    let resolver = resolver(Some("unused"));
    let error = DiscoveryClientFactory::create_with_resolver(
        config("unknown", "http://localhost".to_owned()),
        resolver.clone(),
    )
    .unwrap_err();
    assert_eq!(error.code, ProviderErrorCode::UnsupportedCapability);
    assert_eq!(resolver.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn spec119_discovery_routes_are_literal_and_prefix_preserving() {
    for (provider, suffix, expected) in [
        ("openai", "/v1", "/v1/models"),
        ("openai", "/proxy/v1/", "/proxy/v1/models"),
        ("ollama_local", "", "/api/tags"),
        (
            "ollama_cloud_via_daemon",
            "/proxy/ollama",
            "/proxy/ollama/api/tags",
        ),
    ] {
        let body = if provider == "openai" {
            br#"{"data":[]}"#.as_slice()
        } else {
            br#"{"models":[]}"#.as_slice()
        };
        let (_, captured) = request_once(provider, suffix, body).await;
        assert_eq!(captured.request_line, format!("GET {expected} HTTP/1.1"));
    }
}

#[tokio::test]
async fn spec119_discovery_auth_is_late_bound_and_fail_closed() {
    let (endpoint, captured) = spawn_server(ServerReply::Complete(http_response(
        200,
        &[],
        br#"{"data":[]}"#,
    )))
    .await;
    let client = client_for(
        "openai",
        format!("{endpoint}/v1"),
        resolver(Some("fixture-secret")),
    )
    .await;
    client.health().await.unwrap();
    assert_eq!(
        captured.await.unwrap().authorization.as_deref(),
        Some("Bearer fixture-secret")
    );

    let missing = resolver(None);
    let client = client_for(
        "openai",
        "http://127.0.0.1:9/v1".to_owned(),
        missing.clone(),
    )
    .await;
    assert_error(
        client.health().await.unwrap_err(),
        ProviderErrorCode::Authentication,
        "Discovery credential is unavailable",
    );
    assert_eq!(missing.calls.load(Ordering::SeqCst), 1);

    for value in [
        OsString::from(""),
        non_unicode_os_string(),
        OsString::from("bad\nheader"),
    ] {
        let invalid = Arc::new(StaticResolver::new(Some(value)));
        let client = client_for(
            "openai",
            "http://127.0.0.1:9/v1".to_owned(),
            invalid.clone(),
        )
        .await;
        assert_error(
            client.health().await.unwrap_err(),
            ProviderErrorCode::Authentication,
            "Discovery credential is unavailable",
        );
        assert_eq!(invalid.calls.load(Ordering::SeqCst), 1);
    }

    for profile in ["ollama_local", "ollama_cloud_via_daemon"] {
        let mut ollama = config(profile, "http://127.0.0.1:9".to_owned());
        ollama.auth_selector = Some("SPEC119_DISCOVERY_TOKEN".to_owned());
        let resolver = resolver(Some("must-not-be-read"));
        let error =
            DiscoveryClientFactory::create_with_resolver(ollama, resolver.clone()).unwrap_err();
        assert_eq!(error.code, ProviderErrorCode::InvalidRequest);
        assert_eq!(resolver.calls.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn spec119_discovery_openai_health_order_fields_and_empty_are_literal() {
    let value = fixture();
    let empty = serde_json::to_vec(&value["payloads"]["openai_empty"]).unwrap();
    let (endpoint, captured) =
        spawn_server(ServerReply::Complete(http_response(200, &[], &empty))).await;
    client_for(
        "openai",
        format!("{endpoint}/v1"),
        resolver(Some("fixture-secret")),
    )
    .await
    .health()
    .await
    .unwrap();
    captured.await.unwrap();
    let populated = serde_json::to_vec(&value["payloads"]["openai_models"]).unwrap();
    let (models, _) = request_once("openai", "/v1", &populated).await;
    assert_eq!(
        models.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(),
        ["gpt-b", "gpt-a"]
    );
    assert_eq!(
        models[0].metadata,
        HashMap::from([
            ("created".to_owned(), "2".to_owned()),
            ("owned_by".to_owned(), "openai".to_owned())
        ])
    );
    assert_eq!(
        (
            models[0].size_bytes,
            models[0].description.as_ref(),
            models[0].context_window
        ),
        (None, None, None)
    );
    let (models, _) = request_once("openai", "/v1", &empty).await;
    assert!(models.is_empty());
}

#[tokio::test]
async fn spec119_discovery_ollama_health_order_fields_and_empty_are_literal() {
    let value = fixture();
    let empty = serde_json::to_vec(&value["payloads"]["ollama_empty"]).unwrap();
    let (endpoint, captured) =
        spawn_server(ServerReply::Complete(http_response(200, &[], &empty))).await;
    client_for("ollama_local", endpoint, resolver(None))
        .await
        .health()
        .await
        .unwrap();
    captured.await.unwrap();
    let populated = serde_json::to_vec(&value["payloads"]["ollama_models"]).unwrap();
    let (models, _) = request_once("ollama_local", "", &populated).await;
    assert_eq!(
        models.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(),
        ["qwen3:8b", "llama3.2"]
    );
    assert_eq!(models[0].size_bytes, Some(42));
    assert_eq!(
        models[0].metadata,
        HashMap::from([
            ("digest".to_owned(), "sha256:abc".to_owned()),
            ("modified_at".to_owned(), "2026-08-26T00:00:00Z".to_owned())
        ])
    );
    let (models, _) = request_once("ollama_local", "", &empty).await;
    assert!(models.is_empty());
}

async fn error_for(status: u16, headers: &[(&str, &str)]) -> ProviderError {
    let (endpoint, _) = spawn_server(ServerReply::Complete(http_response(
        status,
        headers,
        b"fixture-body",
    )))
    .await;
    client_for("ollama_local", endpoint, resolver(None))
        .await
        .list_models()
        .await
        .unwrap_err()
}

#[tokio::test]
async fn spec119_discovery_http_transport_and_auth_errors_are_exact() {
    // Own the endpoint until a real request arrives; an unused port can be
    // reused or take longer than the client timeout to refuse on Windows.
    let (endpoint, captured) = spawn_server(ServerReply::CloseWithoutResponse).await;
    assert_error(
        client_for("ollama_local", endpoint, resolver(None))
            .await
            .health()
            .await
            .unwrap_err(),
        ProviderErrorCode::Network,
        "Discovery request failed",
    );
    assert_eq!(
        captured.await.unwrap().request_line,
        "GET /api/tags HTTP/1.1"
    );

    let headers = b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n".to_vec();
    let (endpoint, _) = spawn_server(ServerReply::StallAfterHeaders(headers)).await;
    let mut cfg = config("ollama_local", endpoint);
    cfg.read_timeout_ms = 25;
    let client = DiscoveryClientFactory::create_with_resolver(cfg, resolver(None)).unwrap();
    assert_error(
        client.health().await.unwrap_err(),
        ProviderErrorCode::Timeout,
        "Discovery request timed out",
    );
    assert_error(
        error_for(401, &[]).await,
        ProviderErrorCode::Authentication,
        "Discovery authentication failed",
    );
    assert_error(
        error_for(403, &[]).await,
        ProviderErrorCode::Authentication,
        "Discovery authentication failed",
    );
}

#[tokio::test]
async fn spec119_discovery_http_status_errors_and_retry_metadata_are_exact() {
    assert_discovery_transport_policy_is_single_attempt();

    let error = error_for(429, &[("Retry-After", "2")]).await;
    assert_eq!(
        (error.code, error.message.as_str(), error.retry_after_ms),
        (
            ProviderErrorCode::RateLimited,
            "Discovery request rate limited",
            Some(2000)
        )
    );
    assert_error(
        error_for(429, &[]).await,
        ProviderErrorCode::RateLimited,
        "Discovery request rate limited",
    );
    assert_error(
        error_for(503, &[]).await,
        ProviderErrorCode::Unavailable,
        "Discovery endpoint unavailable",
    );
    assert_error(
        error_for(418, &[]).await,
        ProviderErrorCode::Internal,
        "Discovery request failed internally",
    );

    let (redirect_target, redirected_request) = spawn_server(ServerReply::Complete(http_response(
        200,
        &[],
        br#"{"models":[]}"#,
    )))
    .await;
    let (origin, origin_request) = spawn_server(ServerReply::Complete(http_response(
        302,
        &[("Location", redirect_target.as_str())],
        b"redirect must not be followed",
    )))
    .await;
    let error = client_for("ollama_local", origin, resolver(None))
        .await
        .list_models()
        .await
        .unwrap_err();
    assert_error(
        error,
        ProviderErrorCode::Internal,
        "Discovery request failed internally",
    );
    origin_request.await.expect("one origin request");
    assert!(
        tokio::time::timeout(Duration::from_millis(50), redirected_request)
            .await
            .is_err(),
        "discovery must not follow redirects or issue a second request"
    );
}

async fn malformed_for(provider: &str, body: &[u8]) -> ProviderError {
    let (endpoint, _) = spawn_server(ServerReply::Complete(http_response(200, &[], body))).await;
    client_for(provider, endpoint, resolver(Some("fixture-secret")))
        .await
        .list_models()
        .await
        .unwrap_err()
}

#[tokio::test]
async fn spec119_discovery_malformed_bytes_json_and_roots_are_exact() {
    assert_error(
        malformed_for("ollama_local", &[0xff]).await,
        ProviderErrorCode::MalformedResponse,
        "Discovery response malformed",
    );
    assert_error(
        malformed_for("openai", br#"{"data":["#).await,
        ProviderErrorCode::MalformedResponse,
        "Discovery response malformed",
    );
    assert_error(
        malformed_for("openai", br#"{"models":[]}"#).await,
        ProviderErrorCode::MalformedResponse,
        "Discovery response malformed",
    );
    assert_error(
        malformed_for("ollama_local", br#"{"data":[]}"#).await,
        ProviderErrorCode::MalformedResponse,
        "Discovery response malformed",
    );
}

#[tokio::test]
async fn spec119_discovery_shape_truncation_redaction_and_one_call_are_exact() {
    assert_error(
        malformed_for(
            "openai",
            br#"{"data":[{"id":1,"created":2,"owned_by":"x"}]}"#,
        )
        .await,
        ProviderErrorCode::MalformedResponse,
        "Discovery response malformed",
    );
    assert_error(
        malformed_for("openai", br#"{"data":[{"id":"gpt-x","owned_by":"x"}]}"#).await,
        ProviderErrorCode::MalformedResponse,
        "Discovery response malformed",
    );
    assert_error(
        malformed_for(
            "openai",
            br#"{"data":[{"id":"gpt-x","created":2,"owned_by":3}]}"#,
        )
        .await,
        ProviderErrorCode::MalformedResponse,
        "Discovery response malformed",
    );
    assert_error(
        malformed_for("ollama_local", br#"{"models":[{}]}"#).await,
        ProviderErrorCode::MalformedResponse,
        "Discovery response malformed",
    );
    assert_error(
        malformed_for(
            "ollama_local",
            br#"{"models":[{"name":"qwen3","size":-1}]}"#,
        )
        .await,
        ProviderErrorCode::MalformedResponse,
        "Discovery response malformed",
    );
    assert_error(
        malformed_for(
            "ollama_local",
            br#"{"models":[{"name":"qwen3","digest":42}]}"#,
        )
        .await,
        ProviderErrorCode::MalformedResponse,
        "Discovery response malformed",
    );
    let truncated =
        b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n{".to_vec();
    let (endpoint, _) = spawn_server(ServerReply::Complete(truncated)).await;
    let error = client_for("openai", endpoint, resolver(Some("fixture-secret")))
        .await
        .list_models()
        .await
        .unwrap_err();
    assert_eq!(error.code, ProviderErrorCode::Network);
    let rendered = format!("{error:?}");
    assert!(!rendered.contains("fixture-secret"));
    assert!(!rendered.contains("Content-Length"));
    let truncated_status = b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 100\r\nConnection: close\r\n\r\nsecret-body{".to_vec();
    let (endpoint, _) = spawn_server(ServerReply::Complete(truncated_status)).await;
    let error = client_for(
        "openai",
        format!("{endpoint}/private-prefix"),
        resolver(Some("fixture-secret")),
    )
    .await
    .list_models()
    .await
    .unwrap_err();
    assert_error(
        error.clone(),
        ProviderErrorCode::Network,
        "Discovery request failed",
    );
    let rendered = format!("{error:?}");
    for forbidden in [
        "fixture-secret",
        "secret-body",
        "private-prefix",
        "Content-Length",
    ] {
        assert!(!rendered.contains(forbidden), "failure leaked {forbidden}");
    }
    let (endpoint, captured) = spawn_server(ServerReply::Complete(http_response(
        200,
        &[],
        br#"{"models":[]}"#,
    )))
    .await;
    let client = client_for("ollama_local", endpoint, resolver(None)).await;
    client.health().await.unwrap();
    captured.await.unwrap();
}

#[derive(Clone)]
struct ControlledBackend {
    calls: Arc<AtomicUsize>,
    entered: Arc<Notify>,
    release: Arc<Notify>,
}

#[async_trait]
impl DiscoveryBackend for ControlledBackend {
    async fn health(&self) -> ProviderContractResult<()> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.entered.notify_one();
        self.release.notified().await;
        Ok(())
    }

    async fn list_models(&self) -> ProviderContractResult<Vec<ModelInfo>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.entered.notify_one();
        self.release.notified().await;
        Ok(vec![])
    }
}

#[tokio::test]
async fn spec119_discovery_close_is_idempotent_fail_closed_and_inflight_safe() {
    let backend = Arc::new(ControlledBackend {
        calls: Arc::new(AtomicUsize::new(0)),
        entered: Arc::new(Notify::new()),
        release: Arc::new(Notify::new()),
    });
    let client = Arc::new(DiscoveryClient::from_backend_for_test(backend.clone()));
    let in_flight = {
        let client = client.clone();
        tokio::spawn(async move { client.health().await })
    };
    backend.entered.notified().await;
    client.close().await.unwrap();
    client.close().await.unwrap();
    assert_error(
        client.health().await.unwrap_err(),
        ProviderErrorCode::Unavailable,
        "Discovery client is closed",
    );
    assert_error(
        client.list_models().await.unwrap_err(),
        ProviderErrorCode::Unavailable,
        "Discovery client is closed",
    );
    assert_eq!(backend.calls.load(Ordering::SeqCst), 1);
    let facade = isolation_facade();
    assert!(
        !facade.is_closed(),
        "closing discovery must not close inference facade"
    );
    backend.release.notify_waiters();
    in_flight.await.unwrap().unwrap();
}

#[tokio::test]
async fn spec119_discovery_list_inflight_isolated_and_config_serialization_is_exact() {
    let backend = Arc::new(ControlledBackend {
        calls: Arc::new(AtomicUsize::new(0)),
        entered: Arc::new(Notify::new()),
        release: Arc::new(Notify::new()),
    });
    let client = Arc::new(DiscoveryClient::from_backend_for_test(backend.clone()));
    let independently_open_discovery = DiscoveryClient::from_backend_for_test(backend.clone());
    let in_flight = {
        let client = client.clone();
        tokio::spawn(async move { client.list_models().await })
    };
    backend.entered.notified().await;
    client.close().await.unwrap();
    assert!(client.is_closed());
    let facade = isolation_facade();
    facade.close().await.unwrap();
    assert!(facade.is_closed());
    assert!(
        !independently_open_discovery.is_closed(),
        "closing inference facade must not close discovery"
    );
    backend.release.notify_waiters();
    assert!(in_flight.await.unwrap().unwrap().is_empty());
    let serialized =
        serde_json::to_value(config("ollama_local", "http://localhost:11434".to_owned())).unwrap();
    let keys = serialized
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    assert_eq!(
        keys,
        BTreeSet::from([
            "auth_selector".to_owned(),
            "connect_timeout_ms".to_owned(),
            "endpoint".to_owned(),
            "provider_id".to_owned(),
            "read_timeout_ms".to_owned()
        ])
    );
    assert!(!serialized.to_string().contains("model"));
    println!("SPEC119_RUST_DISCOVERY_ASSERTIONS=48");
}
