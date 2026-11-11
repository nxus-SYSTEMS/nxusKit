//! Spec 119 Sprint 1D: typed, endpoint/auth-only Rust discovery surface.
//!
//! `DiscoveryClient` performs health and model-enumeration calls against an
//! OpenAI or Ollama endpoint without constructing an inference provider or
//! selecting a model. It is independent of `ProviderFacade`, `LLMProvider`,
//! and every other provider lifecycle: closing a `DiscoveryClient` never
//! closes a `ProviderFacade`, and vice versa. See
//! `specs/119-v2-python-provider-surface-parity/contracts/health-discovery-contract.md`
//! and the Sprint 1D design for the exact behavior this module implements.

use std::ffi::OsString;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::{
    ProviderContractResult, ProviderError, ProviderErrorCode, ProviderFailureInput,
    ProviderTransportFailureKind, classify_provider_failure, normalize_retry_after_ms,
};
use crate::types::ModelInfo;

/// Load evidence, independent from endpoint reachability and catalog membership.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryLoadState {
    Unknown,
    NotLoaded,
    Loaded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadedInstance {
    pub id: String,
    pub context_length: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelLoadInfo {
    pub model: String,
    pub kind: String,
    /// None means metadata was absent/null, not an empty instance list.
    pub loaded_instances: Option<Vec<LoadedInstance>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveryApi {
    pub protocol: String,
    /// Compatibility assumption, not a remotely verified server version.
    pub minimum_server_version: String,
    pub reported_server_version: Option<String>,
}

/// One response's catalog and load evidence. Existing model-only APIs are unchanged.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscoverySnapshot {
    pub models: Vec<ModelInfo>,
    pub model_states: Vec<ModelLoadInfo>,
    pub load_state: DiscoveryLoadState,
    pub api: Option<DiscoveryApi>,
}

/// Endpoint/auth-only configuration for a `DiscoveryClient`.
///
/// Deliberately has no `model`, transport selection, provider factory, or
/// inference lifecycle reference. `endpoint` must be an absolute HTTP or
/// HTTPS URL containing no userinfo, query, or fragment. Timeout values must
/// fall in the inclusive range 1 through 300000 milliseconds. `auth_selector`
/// is a nonsecret environment-variable name (never a credential value)
/// matching `[A-Z][A-Z0-9_]{0,127}` when present.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveryConfig {
    /// Discovery profile identifier: `openai`, `ollama_local`, or
    /// `ollama_cloud_via_daemon`, or `lmstudio` (native /api/v1).
    pub provider_id: String,
    /// Absolute HTTP or HTTPS endpoint, with no userinfo, query, or fragment.
    pub endpoint: String,
    /// Nonsecret environment-variable name resolved immediately before each
    /// request. `openai` requires this, `lmstudio` permits it; Ollama rejects it.
    pub auth_selector: Option<String>,
    /// Connect timeout in milliseconds, inclusive range 1 through 300000.
    pub connect_timeout_ms: u32,
    /// Read timeout in milliseconds, inclusive range 1 through 300000.
    pub read_timeout_ms: u32,
}

fn selector_matches_grammar(selector: &str) -> bool {
    let bytes = selector.as_bytes();
    if bytes.is_empty() || bytes.len() > 128 {
        return false;
    }
    if !bytes[0].is_ascii_uppercase() {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || *byte == b'_')
}

impl DiscoveryConfig {
    fn validate_fields(&self) -> ProviderContractResult<()> {
        if self.provider_id.is_empty() {
            return Err(ProviderError::invalid_request(
                "provider_id",
                "provider_id must not be empty",
            ));
        }
        let url = reqwest::Url::parse(&self.endpoint).map_err(|_| {
            ProviderError::invalid_request(
                "endpoint",
                "endpoint must be an absolute HTTP or HTTPS URL",
            )
        })?;
        if url.scheme() != "http" && url.scheme() != "https" {
            return Err(ProviderError::invalid_request(
                "endpoint",
                "endpoint must use the http or https scheme",
            ));
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(ProviderError::invalid_request(
                "endpoint",
                "endpoint must not contain userinfo",
            ));
        }
        if url.query().is_some() || url.fragment().is_some() {
            return Err(ProviderError::invalid_request(
                "endpoint",
                "endpoint must not contain a query or fragment",
            ));
        }
        if !(1..=300_000).contains(&self.connect_timeout_ms) {
            return Err(ProviderError::invalid_request(
                "connect_timeout_ms",
                "connect_timeout_ms must be between 1 and 300000",
            ));
        }
        if !(1..=300_000).contains(&self.read_timeout_ms) {
            return Err(ProviderError::invalid_request(
                "read_timeout_ms",
                "read_timeout_ms must be between 1 and 300000",
            ));
        }
        if let Some(selector) = &self.auth_selector
            && !selector_matches_grammar(selector)
        {
            return Err(ProviderError::invalid_request(
                "auth_selector",
                "auth_selector must match [A-Z][A-Z0-9_]{0,127}",
            ));
        }
        Ok(())
    }
}

/// Crate-private, test-governed credential resolution seam. The default
/// resolver reads a process environment variable named by `selector`;
/// deterministic tests inject a static resolver instead of mutating process
/// environment.
pub(crate) trait DiscoveryCredentialResolver: Send + Sync {
    fn resolve(&self, selector: &str) -> Option<OsString>;
}

struct EnvCredentialResolver;

impl DiscoveryCredentialResolver for EnvCredentialResolver {
    fn resolve(&self, selector: &str) -> Option<OsString> {
        std::env::var_os(selector)
    }
}

/// Resolves `selector` through `resolver` and builds a `Bearer` header value.
/// Fails closed with `authentication`/`Discovery credential is unavailable`
/// on a missing value, a non-Unicode value, an empty value, or a value that
/// cannot form a valid HTTP header, without ever retaining the resolved
/// secret.
pub(crate) fn resolve_bearer_header(
    resolver: &Arc<dyn DiscoveryCredentialResolver>,
    selector: &str,
) -> ProviderContractResult<reqwest::header::HeaderValue> {
    let unavailable = || ProviderError::authentication("Discovery credential is unavailable");
    let raw = resolver.resolve(selector).ok_or_else(unavailable)?;
    let value = raw.into_string().map_err(|_| unavailable())?;
    if value.is_empty() {
        return Err(unavailable());
    }
    reqwest::header::HeaderValue::from_str(&format!("Bearer {value}")).map_err(|_| unavailable())
}

/// Crate-private, test-governed async transport seam for one discovery
/// protocol backend (OpenAI or Ollama). Concrete implementations perform the
/// actual HTTP call; `DiscoveryClient` itself performs no I/O.
#[async_trait]
pub(crate) trait DiscoveryBackend: Send + Sync {
    async fn health(&self) -> ProviderContractResult<()>;
    async fn list_models(&self) -> ProviderContractResult<Vec<ModelInfo>>;
    async fn snapshot(&self) -> ProviderContractResult<DiscoverySnapshot> {
        Ok(DiscoverySnapshot {
            models: self.list_models().await?,
            model_states: vec![],
            load_state: DiscoveryLoadState::Unknown,
            api: None,
        })
    }
}

fn classify_and_build_error(input: &ProviderFailureInput) -> ProviderError {
    match classify_provider_failure(input) {
        ProviderErrorCode::Timeout => ProviderError::timeout("Discovery request timed out"),
        ProviderErrorCode::Network => ProviderError::network("Discovery request failed"),
        ProviderErrorCode::Authentication => {
            ProviderError::authentication("Discovery authentication failed")
        }
        ProviderErrorCode::RateLimited => ProviderError::rate_limited(
            "Discovery request rate limited",
            normalize_retry_after_ms(input.retry_after.as_deref()),
        ),
        ProviderErrorCode::Unavailable => {
            ProviderError::unavailable("Discovery endpoint unavailable")
        }
        _ => ProviderError::internal("Discovery request failed internally"),
    }
}

fn transport_error(error: &reqwest::Error) -> ProviderError {
    let kind = if error.is_timeout() {
        ProviderTransportFailureKind::Timeout
    } else {
        ProviderTransportFailureKind::Network
    };
    classify_and_build_error(&ProviderFailureInput {
        status: None,
        retry_after: None,
        body: None,
        transport_kind: kind,
    })
}

/// Performs exactly one HTTP GET against `url`, applying `auth_header` when
/// present. Returns the complete response bytes for a successful (2xx)
/// response; every other outcome (transport failure or non-success status)
/// is classified into a canonical, redacted `ProviderError` with no raw body,
/// header, or URL retained.
pub(crate) async fn execute_discovery_get(
    client: &reqwest::Client,
    url: reqwest::Url,
    auth_header: Option<reqwest::header::HeaderValue>,
) -> ProviderContractResult<Vec<u8>> {
    let mut request = client.get(url);
    if let Some(header) = auth_header {
        request = request.header(reqwest::header::AUTHORIZATION, header);
    }
    let response = match request.send().await {
        Ok(response) => response,
        Err(error) => return Err(transport_error(&error)),
    };

    let status = response.status();
    let retry_after = response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);

    let bytes = match crate::provider::http::read_response_body_bounded(
        response,
        crate::provider::http::MAX_UNARY_RESPONSE_BODY_BYTES,
    )
    .await
    {
        Ok(bytes) => bytes,
        Err(crate::provider::http::BoundedBodyReadError::Transport(error)) => {
            return Err(transport_error(&error));
        }
        Err(crate::provider::http::BoundedBodyReadError::LimitExceeded { limit, .. }) => {
            if status.is_success() {
                return Err(ProviderError::malformed_response(format!(
                    "Discovery response exceeds {limit} bytes"
                )));
            }
            return Err(classify_and_build_error(&ProviderFailureInput {
                status: Some(status.as_u16()),
                retry_after,
                body: None,
                transport_kind: ProviderTransportFailureKind::None,
            }));
        }
    };

    if !status.is_success() {
        return Err(classify_and_build_error(&ProviderFailureInput {
            status: Some(status.as_u16()),
            retry_after,
            body: None,
            transport_kind: ProviderTransportFailureKind::None,
        }));
    }

    Ok(bytes)
}

/// Strictly decodes `bytes` as UTF-8, failing with `malformed_response` on
/// any invalid byte sequence rather than lossily substituting.
pub(crate) fn decode_discovery_utf8(bytes: &[u8]) -> ProviderContractResult<&str> {
    std::str::from_utf8(bytes)
        .map_err(|_| ProviderError::malformed_response("Discovery response malformed"))
}

/// Strictly parses `text` as JSON, failing with `malformed_response` on any
/// invalid document.
pub(crate) fn parse_discovery_json(text: &str) -> ProviderContractResult<serde_json::Value> {
    serde_json::from_str(text)
        .map_err(|_| ProviderError::malformed_response("Discovery response malformed"))
}

/// Resolves `relative` against `endpoint`, appending exactly one trailing
/// slash to `endpoint`'s existing path when absent, then joining the exact
/// relative path. The existing path is never discarded and no code
/// concatenates strings, adds userinfo, or accepts a query/fragment.
pub(crate) fn join_discovery_route(
    endpoint: &str,
    relative: &str,
) -> ProviderContractResult<reqwest::Url> {
    let invalid_endpoint = || {
        ProviderError::invalid_request("endpoint", "endpoint must be an absolute HTTP or HTTPS URL")
    };
    let mut url = reqwest::Url::parse(endpoint).map_err(|_| invalid_endpoint())?;
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    url.join(relative).map_err(|_| invalid_endpoint())
}

/// Builds a `reqwest::Client` for one discovery backend using the accepted
/// connect/read timeout bounds. Discovery has no distinct total-timeout
/// field, so read timeout also bounds the overall request.
pub(crate) fn build_discovery_http_client(
    connect_timeout_ms: u32,
    read_timeout_ms: u32,
) -> ProviderContractResult<reqwest::Client> {
    let connect = Duration::from_millis(connect_timeout_ms as u64);
    let read = Duration::from_millis(read_timeout_ms as u64);
    reqwest::Client::builder()
        .connect_timeout(connect)
        .read_timeout(read)
        .timeout(read)
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .build()
        .map_err(|error| ProviderError::internal(error.to_string()))
}

enum DiscoveryProfile {
    OpenAi,
    LmStudio,
    OllamaLocal,
    OllamaCloudViaDaemon,
}

impl DiscoveryProfile {
    fn from_id(id: &str) -> Option<Self> {
        match id {
            "openai" => Some(Self::OpenAi),
            "lmstudio" => Some(Self::LmStudio),
            "ollama_local" => Some(Self::OllamaLocal),
            "ollama_cloud_via_daemon" => Some(Self::OllamaCloudViaDaemon),
            _ => None,
        }
    }
}

/// Public factory that validates a `DiscoveryConfig` and constructs a
/// `DiscoveryClient` bound to the correct protocol backend for its
/// `provider_id`. Every identifier other than `openai`, `ollama_local`,
/// `ollama_cloud_via_daemon`, and `lmstudio` fails with `unsupported_capability` before any
/// credential resolution or network work.
///
/// ```rust,no_run
/// use nxuskit_engine::discovery::{DiscoveryClient, DiscoveryClientFactory, DiscoveryConfig};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let config = DiscoveryConfig {
///     provider_id: "openai".to_string(),
///     endpoint: "https://api.openai.com/v1".to_string(),
///     auth_selector: Some("OPENAI_API_KEY".to_string()),
///     connect_timeout_ms: 5_000,
///     read_timeout_ms: 5_000,
/// };
/// let client: DiscoveryClient = DiscoveryClientFactory::create(config)?;
/// client.health().await?;
/// client.close().await?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct DiscoveryClientFactory;

impl DiscoveryClientFactory {
    /// Constructs a `DiscoveryClient` using the default environment-variable
    /// credential resolver.
    pub fn create(config: DiscoveryConfig) -> ProviderContractResult<DiscoveryClient> {
        Self::create_with_resolver(config, Arc::new(EnvCredentialResolver))
    }

    /// Constructs a `DiscoveryClient` using the given credential resolver.
    /// Crate-private: deterministic tests inject a static resolver instead
    /// of mutating process environment.
    pub(crate) fn create_with_resolver(
        config: DiscoveryConfig,
        resolver: Arc<dyn DiscoveryCredentialResolver>,
    ) -> ProviderContractResult<DiscoveryClient> {
        config.validate_fields()?;
        let profile = DiscoveryProfile::from_id(&config.provider_id).ok_or_else(|| {
            ProviderError::unsupported_capability(
                "provider_id",
                "Discovery provider is unsupported",
            )
        })?;

        let backend: Arc<dyn DiscoveryBackend> = match profile {
            DiscoveryProfile::LmStudio => Arc::new(
                crate::providers::lmstudio_discovery::LmStudioDiscoveryBackend::new(
                    &config.endpoint,
                    config.connect_timeout_ms,
                    config.read_timeout_ms,
                    config.auth_selector.clone(),
                    resolver,
                )?,
            ),
            DiscoveryProfile::OpenAi => {
                let selector = config.auth_selector.clone().ok_or_else(|| {
                    ProviderError::invalid_request(
                        "auth_selector",
                        "auth_selector is required for OpenAI discovery",
                    )
                })?;
                Arc::new(
                    crate::providers::openai_discovery::OpenAiDiscoveryBackend::new(
                        &config.endpoint,
                        config.connect_timeout_ms,
                        config.read_timeout_ms,
                        selector,
                        resolver,
                    )?,
                )
            }
            DiscoveryProfile::OllamaLocal | DiscoveryProfile::OllamaCloudViaDaemon => {
                if config.auth_selector.is_some() {
                    return Err(ProviderError::invalid_request(
                        "auth_selector",
                        "auth_selector is not supported for Ollama discovery",
                    ));
                }
                Arc::new(
                    crate::providers::ollama_discovery::OllamaDiscoveryBackend::new(
                        &config.endpoint,
                        config.connect_timeout_ms,
                        config.read_timeout_ms,
                    )?,
                )
            }
        };

        Ok(DiscoveryClient::new(backend))
    }
}

/// Public async discovery client bound to exactly one protocol backend.
///
/// Each `health()`/`list_models()` call performs exactly one HTTP request.
/// `close()` is idempotent and does not wait for in-flight calls: an
/// in-flight call may still complete, while a new call made after `close()`
/// fails with `unavailable`/`Discovery client is closed` before any
/// resolver or backend work. `DiscoveryClient` shares no state with
/// `ProviderFacade`: closing one never closes the other.
pub struct DiscoveryClient {
    backend: Arc<dyn DiscoveryBackend>,
    closed: AtomicBool,
}

impl std::fmt::Debug for DiscoveryClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DiscoveryClient")
            .field("is_closed", &self.is_closed())
            .finish_non_exhaustive()
    }
}

impl DiscoveryClient {
    fn new(backend: Arc<dyn DiscoveryBackend>) -> Self {
        Self {
            backend,
            closed: AtomicBool::new(false),
        }
    }

    /// Constructs a `DiscoveryClient` directly over a test-governed backend,
    /// bypassing config validation and protocol selection. Crate-private:
    /// used only by lifecycle/isolation tests.
    #[cfg(test)]
    pub(crate) fn from_backend_for_test(backend: Arc<dyn DiscoveryBackend>) -> Self {
        Self::new(backend)
    }

    /// Performs the provider's model-collection request and validates the
    /// complete success envelope without constructing or retaining a model.
    pub async fn health(&self) -> ProviderContractResult<()> {
        if self.is_closed() {
            return Err(ProviderError::unavailable("Discovery client is closed"));
        }
        self.backend.health().await
    }

    /// Performs one request and returns the ordered `ModelInfo` values from
    /// the response.
    pub async fn list_models(&self) -> ProviderContractResult<Vec<ModelInfo>> {
        if self.is_closed() {
            return Err(ProviderError::unavailable("Discovery client is closed"));
        }
        self.backend.list_models().await
    }

    /// Get a single-response snapshot; load state is unknown for older profiles.
    pub async fn snapshot(&self) -> ProviderContractResult<DiscoverySnapshot> {
        if self.is_closed() {
            return Err(ProviderError::unavailable("Discovery client is closed"));
        }
        self.backend.snapshot().await
    }

    /// Marks this client closed. Idempotent; does not wait for in-flight
    /// calls, which may still complete.
    pub async fn close(&self) -> ProviderContractResult<()> {
        self.closed.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Returns `true` once `close()` has been called.
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod discovery_resource_bound_tests {
    use super::*;
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

    async fn serve(
        status: &'static str,
        retry_after: Option<&'static str>,
        body: Vec<u8>,
    ) -> (reqwest::Url, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr: SocketAddr = listener.local_addr().expect("addr");
        let retry = retry_after
            .map(|v| format!("Retry-After: {v}\r\n"))
            .unwrap_or_default();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            drain_headers(&mut socket).await;
            let header = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/octet-stream\r\n{retry}Content-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = socket.write_all(header.as_bytes()).await;
            let _ = socket.write_all(&body).await;
            let _ = socket.shutdown().await;
        });
        let url = reqwest::Url::parse(&format!("http://{addr}/models")).expect("url");
        (url, server)
    }

    #[tokio::test]
    async fn discovery_resource_exact_limit_succeeds() {
        let body = vec![b'a'; crate::provider::http::MAX_UNARY_RESPONSE_BODY_BYTES];
        let (url, server) = serve("200 OK", None, body).await;
        let result = execute_discovery_get(&reqwest::Client::new(), url, None)
            .await
            .expect("exactly-at-limit discovery body");
        assert_eq!(
            result.len(),
            crate::provider::http::MAX_UNARY_RESPONSE_BODY_BYTES
        );
        server.await.expect("server");
    }

    #[tokio::test]
    async fn discovery_resource_one_over_success_is_malformed_response() {
        let body = vec![b'a'; crate::provider::http::MAX_UNARY_RESPONSE_BODY_BYTES + 1];
        let (url, server) = serve("200 OK", None, body).await;
        let error = match execute_discovery_get(&reqwest::Client::new(), url, None).await {
            Err(error) => error,
            Ok(_) => panic!("SEMANTIC_RED_DISCOVERY_OVER_SUCCESS_ACCEPTED"),
        };
        assert_eq!(error.code, ProviderErrorCode::MalformedResponse);
        assert_eq!(error.message, "Discovery response exceeds 16777216 bytes");
        assert_eq!(error.retry_after_ms, None);
        assert_eq!(error.field, None);
        server.await.expect("server");
    }

    #[tokio::test]
    async fn discovery_resource_one_over_rate_limit_preserves_status_classification() {
        let body = vec![b'a'; crate::provider::http::MAX_UNARY_RESPONSE_BODY_BYTES + 1];
        let (url, server) = serve("429 Too Many Requests", Some("2"), body).await;
        let error = match execute_discovery_get(&reqwest::Client::new(), url, None).await {
            Err(error) => error,
            Ok(_) => panic!("OVERSIZED_429_MUST_REMAIN_ERROR"),
        };
        assert_eq!(error.code, ProviderErrorCode::RateLimited);
        assert_eq!(error.message, "Discovery request rate limited");
        assert_eq!(error.retry_after_ms, Some(2000));
        assert_eq!(error.field, None);
        server.await.expect("server");
    }
}
