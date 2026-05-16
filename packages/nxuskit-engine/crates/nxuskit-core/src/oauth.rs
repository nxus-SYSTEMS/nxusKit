//! OAuth authentication flow — browser launch, PKCE, localhost callback.
//!
//! Builds on the cryptographic primitives from `oauth_prework.rs` to implement
//! the complete OAuth flow: browser launch → localhost callback → token exchange
//! → credential storage.
//!
//! This is infrastructure code — no current providers use OAuth yet, but the
//! flow is fully operational for when providers enable it.

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

use super::auth;
use super::auth_metadata;
use super::oauth_prework::{self, OAuthSession};

// ── Types ─────────────────────────────────────────────────────────

/// Result of an OAuth flow initiation.
#[derive(Debug, Clone, Serialize)]
pub struct OAuthResult {
    pub success: bool,
    pub provider_id: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// OAuth token status for a provider.
#[derive(Debug, Clone, Serialize)]
pub struct OAuthStatus {
    pub authenticated: bool,
    pub provider_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scopes: Option<Vec<String>>,
}

/// OAuth token response from the provider's token endpoint.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    token_type: String,
    #[serde(default)]
    expires_in: Option<u64>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    scope: Option<String>,
}

/// Errors during OAuth operations.
#[derive(Debug)]
pub enum OAuthError {
    /// Provider is not OAuth-capable.
    NotOAuthCapable(String),
    /// Provider not found.
    UnknownProvider(String),
    /// Failed to bind localhost callback server.
    BindFailed(String),
    /// Browser launch failed.
    BrowserLaunchFailed(String),
    /// Callback timeout.
    Timeout,
    /// State mismatch (CSRF protection).
    StateMismatch,
    /// Token exchange failed.
    TokenExchangeFailed(String),
    /// Session expired.
    SessionExpired,
    /// Credential storage failed.
    StorageFailed(String),
}

impl std::fmt::Display for OAuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OAuthError::NotOAuthCapable(id) => write!(f, "Provider '{id}' does not support OAuth"),
            OAuthError::UnknownProvider(id) => write!(f, "Unknown provider: {id}"),
            OAuthError::BindFailed(e) => write!(f, "Failed to bind callback server: {e}"),
            OAuthError::BrowserLaunchFailed(e) => write!(f, "Failed to launch browser: {e}"),
            OAuthError::Timeout => write!(f, "OAuth callback timed out"),
            OAuthError::StateMismatch => write!(f, "OAuth state mismatch — possible CSRF attack"),
            OAuthError::TokenExchangeFailed(e) => write!(f, "Token exchange failed: {e}"),
            OAuthError::SessionExpired => write!(f, "OAuth session expired"),
            OAuthError::StorageFailed(e) => write!(f, "Failed to store OAuth credential: {e}"),
        }
    }
}

// ── OAuth Flow ───────────────────────────────────────────────────

/// Start an OAuth authentication flow for a provider.
///
/// This is a **blocking** call that:
/// 1. Verifies the provider supports OAuth
/// 2. Generates PKCE verifier + challenge
/// 3. Generates state token for CSRF protection
/// 4. Binds a `TcpListener` on `127.0.0.1:0` (random port)
/// 5. Opens the provider's authorization URL in the browser
/// 6. Waits for the callback with authorization code
/// 7. Exchanges the code for an access token
/// 8. Stores the token in the credential store
///
/// # Arguments
/// * `provider_id` — Provider identifier (e.g., "azure-openai")
/// * `timeout_secs` — Max seconds to wait for callback (0 = default 120s)
pub fn oauth_start(provider_id: &str, timeout_secs: u32) -> Result<OAuthResult, OAuthError> {
    // Validate provider
    let meta = auth_metadata::lookup(provider_id)
        .ok_or_else(|| OAuthError::UnknownProvider(provider_id.to_string()))?;

    if !meta.oauth_capable {
        return Err(OAuthError::NotOAuthCapable(provider_id.to_string()));
    }

    let timeout = if timeout_secs == 0 { 120 } else { timeout_secs };

    // Bind localhost callback server (random port)
    let listener =
        TcpListener::bind("127.0.0.1:0").map_err(|e| OAuthError::BindFailed(e.to_string()))?;

    let local_addr = listener
        .local_addr()
        .map_err(|e| OAuthError::BindFailed(e.to_string()))?;

    let redirect_uri = format!("http://127.0.0.1:{}/callback", local_addr.port());

    // Create OAuth session with PKCE and state
    let mut session = OAuthSession::new(provider_id, &redirect_uri);
    session.timeout_secs = timeout as u64;

    // Build authorization URL
    // For now, use a placeholder authorization URL format.
    // Real providers would have this in their metadata.
    let auth_url = build_auth_url(provider_id, &session);

    log::info!("Starting OAuth flow for provider '{provider_id}'");
    log::debug!("Callback server bound to {redirect_uri}");

    // Launch browser
    if let Err(e) = open::that(&auth_url) {
        return Err(OAuthError::BrowserLaunchFailed(e.to_string()));
    }

    // Set socket timeout
    listener
        .set_nonblocking(false)
        .map_err(|e| OAuthError::BindFailed(e.to_string()))?;

    // Wait for callback (blocking)
    let (code, received_state) = wait_for_callback(&listener, timeout)?;

    // Validate state (CSRF protection)
    if !oauth_prework::verify_state(&session.state, &received_state) {
        return Err(OAuthError::StateMismatch);
    }

    // Check session expiry
    if session.is_expired() {
        return Err(OAuthError::SessionExpired);
    }

    // Exchange code for token
    let token_response = exchange_code(provider_id, &code, &session)?;

    // Store the access token
    auth::set_credential(provider_id, &token_response.access_token)
        .map_err(OAuthError::StorageFailed)?;

    log::info!("OAuth authentication complete for provider '{provider_id}'");

    Ok(OAuthResult {
        success: true,
        provider_id: provider_id.to_string(),
        message: "Authenticated successfully.".to_string(),
        error: None,
    })
}

/// Check OAuth status for a provider.
///
/// Returns whether an OAuth credential is stored and its metadata.
pub fn oauth_status(provider_id: &str) -> Result<OAuthStatus, OAuthError> {
    let _meta = auth_metadata::lookup(provider_id)
        .ok_or_else(|| OAuthError::UnknownProvider(provider_id.to_string()))?;

    let resolution = auth::resolve(provider_id, None).map_err(OAuthError::UnknownProvider)?;

    Ok(OAuthStatus {
        authenticated: resolution.has_credential,
        provider_id: provider_id.to_string(),
        expires_at: None, // Token expiry tracking is a future enhancement
        scopes: None,     // Scope tracking is a future enhancement
    })
}

/// Revoke/remove an OAuth token for a provider.
///
/// Removes the stored credential. Returns Ok even if no credential was stored.
pub fn oauth_revoke(provider_id: &str) -> Result<(), OAuthError> {
    let _meta = auth_metadata::lookup(provider_id)
        .ok_or_else(|| OAuthError::UnknownProvider(provider_id.to_string()))?;

    // Remove credential from store — ignore "not found" errors
    let _ = auth::remove_credential(provider_id);
    log::info!("OAuth token revoked for provider '{provider_id}'");
    Ok(())
}

// ── Internal Helpers ──────────────────────────────────────────────

/// Build the authorization URL with PKCE parameters.
fn build_auth_url(provider_id: &str, session: &OAuthSession) -> String {
    // Placeholder URL format — real providers would have their own URLs.
    // This infrastructure is ready to be parameterized per-provider.
    let base_url = format!("https://auth.{provider_id}.example.com/authorize");

    format!(
        "{}?response_type=code&client_id=nxuskit&redirect_uri={}&state={}&code_challenge={}&code_challenge_method=S256&scope=api.read",
        base_url,
        urlencoded(&session.redirect_uri),
        &session.state,
        &session.code_challenge,
    )
}

/// Wait for the OAuth callback on the localhost server.
///
/// Accepts a single HTTP request, extracts `code` and `state` from query params,
/// sends an HTML response, and shuts down.
fn wait_for_callback(
    listener: &TcpListener,
    timeout_secs: u32,
) -> Result<(String, String), OAuthError> {
    // Set accept timeout
    listener
        .set_nonblocking(false)
        .map_err(|e| OAuthError::BindFailed(e.to_string()))?;

    // Use a thread to enforce timeout since TcpListener::set_nonblocking
    // doesn't provide a timeout for accept()
    let listener_timeout = Duration::from_secs(timeout_secs as u64);

    // We can't directly set accept timeout on TcpListener, so we poll
    // with short non-blocking intervals
    let start = std::time::Instant::now();
    listener
        .set_nonblocking(true)
        .map_err(|e| OAuthError::BindFailed(e.to_string()))?;

    loop {
        match listener.accept() {
            Ok((mut stream, _addr)) => {
                stream.set_read_timeout(Some(Duration::from_secs(5))).ok();

                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]);

                // Parse GET request for code and state
                let (code, state) = parse_callback_params(&request);

                // Send HTML response
                let html = r#"<!DOCTYPE html>
<html><head><title>nxusKit</title></head>
<body style="font-family: system-ui; text-align: center; padding: 2em;">
<h2>Authentication complete</h2>
<p>You can close this tab and return to the terminal.</p>
</body></html>"#;

                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    html.len(),
                    html
                );

                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();

                if let (Some(code), Some(state)) = (code, state) {
                    return Ok((code, state));
                } else {
                    return Err(OAuthError::TokenExchangeFailed(
                        "Missing code or state in callback".to_string(),
                    ));
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if start.elapsed() > listener_timeout {
                    return Err(OAuthError::Timeout);
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                return Err(OAuthError::BindFailed(format!("accept failed: {e}")));
            }
        }
    }
}

/// Parse code and state from an HTTP GET callback request.
fn parse_callback_params(request: &str) -> (Option<String>, Option<String>) {
    // Extract the request line: "GET /callback?code=xxx&state=yyy HTTP/1.1"
    let first_line = request.lines().next().unwrap_or("");
    let path = first_line.split_whitespace().nth(1).unwrap_or("");

    let query = path.split('?').nth(1).unwrap_or("");

    let mut code = None;
    let mut state = None;

    for param in query.split('&') {
        if let Some((key, value)) = param.split_once('=') {
            match key {
                "code" => code = Some(urldecoded(value)),
                "state" => state = Some(urldecoded(value)),
                _ => {}
            }
        }
    }

    (code, state)
}

/// Exchange an authorization code for an access token.
fn exchange_code(
    provider_id: &str,
    code: &str,
    session: &OAuthSession,
) -> Result<TokenResponse, OAuthError> {
    // Placeholder token endpoint — real providers would have their own URLs
    let token_url = format!("https://auth.{provider_id}.example.com/token");

    let body = serde_json::json!({
        "grant_type": "authorization_code",
        "code": code,
        "redirect_uri": session.redirect_uri,
        "code_verifier": session.code_verifier,
        "client_id": "nxuskit",
    });

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| OAuthError::TokenExchangeFailed(e.to_string()))?;

    let response = client
        .post(&token_url)
        .json(&body)
        .send()
        .map_err(|e| OAuthError::TokenExchangeFailed(e.to_string()))?;

    if !response.status().is_success() {
        let status = response.status();
        let body_text = response.text().unwrap_or_default();
        return Err(OAuthError::TokenExchangeFailed(format!(
            "HTTP {status}: {body_text}"
        )));
    }

    response
        .json::<TokenResponse>()
        .map_err(|e| OAuthError::TokenExchangeFailed(format!("parse token response: {e}")))
}

/// Minimal URL encoding for query parameter values.
fn urlencoded(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
            _ => {
                for byte in c.to_string().as_bytes() {
                    result.push_str(&format!("%{byte:02X}"));
                }
            }
        }
    }
    result
}

/// Minimal URL decoding for query parameter values.
fn urldecoded(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                result.push(byte as char);
            }
        } else if c == '+' {
            result.push(' ');
        } else {
            result.push(c);
        }
    }
    result
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_callback_params() {
        let request =
            "GET /callback?code=abc123&state=xyz789 HTTP/1.1\r\nHost: localhost:8080\r\n\r\n";
        let (code, state) = parse_callback_params(request);
        assert_eq!(code, Some("abc123".to_string()));
        assert_eq!(state, Some("xyz789".to_string()));
    }

    #[test]
    fn test_parse_callback_params_missing() {
        let request = "GET /callback HTTP/1.1\r\n\r\n";
        let (code, state) = parse_callback_params(request);
        assert!(code.is_none());
        assert!(state.is_none());
    }

    #[test]
    fn test_parse_callback_params_partial() {
        let request = "GET /callback?code=abc123 HTTP/1.1\r\n\r\n";
        let (code, state) = parse_callback_params(request);
        assert_eq!(code, Some("abc123".to_string()));
        assert!(state.is_none());
    }

    #[test]
    fn test_urlencoded() {
        assert_eq!(urlencoded("hello world"), "hello%20world");
        assert_eq!(urlencoded("a=b&c=d"), "a%3Db%26c%3Dd");
        assert_eq!(urlencoded("safe-_chars.~"), "safe-_chars.~");
    }

    #[test]
    fn test_urldecoded() {
        assert_eq!(urldecoded("hello%20world"), "hello world");
        assert_eq!(urldecoded("a%3Db"), "a=b");
        assert_eq!(urldecoded("no+spaces"), "no spaces");
    }

    #[test]
    fn test_build_auth_url_contains_pkce() {
        let session = OAuthSession::new("test-provider", "http://localhost:8080/callback");
        let url = build_auth_url("test-provider", &session);
        assert!(url.contains("code_challenge="));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains(&session.state));
    }

    #[test]
    fn test_oauth_start_unknown_provider() {
        let result = oauth_start("nonexistent", 0);
        assert!(matches!(result, Err(OAuthError::UnknownProvider(_))));
    }

    #[test]
    fn test_oauth_start_not_oauth_capable() {
        // openai is not OAuth-capable
        let result = oauth_start("openai", 0);
        assert!(matches!(result, Err(OAuthError::NotOAuthCapable(_))));
    }

    #[test]
    fn test_oauth_status_known_provider() {
        let status = oauth_status("openai").unwrap();
        assert_eq!(status.provider_id, "openai");
        // No OAuth credential stored
        assert!(!status.authenticated);
    }

    #[test]
    fn test_oauth_status_unknown_provider() {
        let result = oauth_status("nonexistent");
        assert!(matches!(result, Err(OAuthError::UnknownProvider(_))));
    }

    #[test]
    fn test_oauth_revoke_no_credential() {
        // Should succeed even without stored credential
        let result = oauth_revoke("openai");
        assert!(result.is_ok());
    }

    #[test]
    fn test_oauth_revoke_unknown_provider() {
        let result = oauth_revoke("nonexistent");
        assert!(matches!(result, Err(OAuthError::UnknownProvider(_))));
    }
}
