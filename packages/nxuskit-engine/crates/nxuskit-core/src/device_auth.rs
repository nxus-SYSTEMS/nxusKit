//! RFC 8628 Device Authorization Grant flow for platform authentication.
//!
//! Implements the OAuth 2.0 Device Authorization Grant (RFC 8628) against
//! the Odoo-brokered licensing API. The user authenticates via browser;
//! the CLI polls for the access token.

use crate::auth_token::{self, AuthSession};
use std::time::Duration;

/// Transient state during the device code flow. Not persisted.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct DeviceCodeSession {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    #[serde(default = "default_expires_in")]
    pub expires_in: u64,
    #[serde(default = "default_interval")]
    pub interval: u64,
}

fn default_expires_in() -> u64 {
    600
}
fn default_interval() -> u64 {
    5
}

/// Errors during the device authorization flow.
#[derive(Debug, thiserror::Error)]
pub enum DeviceAuthError {
    #[error("Cannot reach licensing service: {0}")]
    NetworkError(String),
    #[error("Invalid response from licensing service: {0}")]
    InvalidResponse(String),
    #[error("Device code expired. Please try again.")]
    DeviceCodeExpired,
    #[error("Authorization denied by user.")]
    AccessDenied,
    #[error("Server requested slower polling.")]
    SlowDown,
    #[error("Authorization pending — user has not yet completed login.")]
    AuthorizationPending,
    #[error("Failed to store auth token: {0}")]
    TokenStorageError(String),
}

/// Token polling response error codes per RFC 8628 Section 3.5.
#[derive(Debug, serde::Deserialize)]
struct DeviceTokenErrorResponse {
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}

/// Successful token response.
#[derive(Debug, serde::Deserialize)]
struct DeviceTokenResponse {
    access_token: String,
    #[serde(default)]
    token_type: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
    #[serde(default)]
    instance_url: Option<String>,
}

/// Initiate the device code flow.
///
/// Calls `POST /licensing-api/v1/device/code` and returns the session
/// containing the device code, user code, and verification URI.
pub fn device_auth_initiate(server_url: &str) -> Result<DeviceCodeSession, DeviceAuthError> {
    let url = format!("{}/device/code", server_url.trim_end_matches('/'));
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| DeviceAuthError::NetworkError(e.to_string()))?;

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({"client_id": "nxuskit-cli"}))
        .send()
        .map_err(|e| DeviceAuthError::NetworkError(e.to_string()))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return Err(DeviceAuthError::InvalidResponse(format!(
            "HTTP {status}: {body}"
        )));
    }

    response
        .json::<DeviceCodeSession>()
        .map_err(|e| DeviceAuthError::InvalidResponse(e.to_string()))
}

/// Poll for the access token after the user authorizes.
///
/// Polls `POST /licensing-api/v1/device/token` at the specified interval.
/// Handles RFC 8628 error codes: `authorization_pending`, `slow_down`,
/// `access_denied`, `expired_token`.
pub fn device_auth_poll(
    server_url: &str,
    session: &DeviceCodeSession,
) -> Result<AuthSession, DeviceAuthError> {
    let url = format!("{}/device/token", server_url.trim_end_matches('/'));
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| DeviceAuthError::NetworkError(e.to_string()))?;

    let mut interval = session.interval;
    let deadline = std::time::Instant::now() + Duration::from_secs(session.expires_in);

    loop {
        if std::time::Instant::now() >= deadline {
            return Err(DeviceAuthError::DeviceCodeExpired);
        }

        std::thread::sleep(Duration::from_secs(interval));

        let body = serde_json::json!({
            "device_code": session.device_code,
            "client_id": "nxuskit-cli",
            "grant_type": "urn:ietf:params:oauth:grant-type:device_code"
        });

        let response = client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .map_err(|e| DeviceAuthError::NetworkError(e.to_string()))?;

        let status = response.status();

        if status.is_success() {
            let token_resp: DeviceTokenResponse = response
                .json()
                .map_err(|e| DeviceAuthError::InvalidResponse(e.to_string()))?;

            let expires_at = token_resp
                .expires_in
                .map(|secs| {
                    let _now = chrono_now_iso();
                    // Simple calculation: current time + expires_in seconds
                    format_expiry_from_now(secs)
                })
                .unwrap_or_else(|| format_expiry_from_now(30 * 86400)); // default 30 days

            return Ok(AuthSession {
                access_token: token_resp.access_token,
                token_type: token_resp
                    .token_type
                    .unwrap_or_else(|| "Bearer".to_string()),
                expires_at,
                instance_url: token_resp
                    .instance_url
                    .unwrap_or_else(|| server_url.to_string()),
                user_email: String::new(), // populated by caller if available
            });
        }

        // Parse error response
        let error_body = response.text().unwrap_or_default();
        if let Ok(err_resp) = serde_json::from_str::<DeviceTokenErrorResponse>(&error_body) {
            match err_resp.error.as_str() {
                "authorization_pending" => continue,
                "slow_down" => {
                    interval += 5; // RFC 8628 Section 3.5
                    continue;
                }
                "access_denied" => return Err(DeviceAuthError::AccessDenied),
                "expired_token" => return Err(DeviceAuthError::DeviceCodeExpired),
                other => {
                    return Err(DeviceAuthError::InvalidResponse(format!(
                        "Unknown error: {other}: {}",
                        err_resp.error_description.unwrap_or_default()
                    )));
                }
            }
        }

        return Err(DeviceAuthError::InvalidResponse(format!(
            "HTTP {status}: {error_body}"
        )));
    }
}

/// Full device code login flow: initiate, open browser, poll, store token.
pub fn device_auth_login(server_url: &str) -> Result<AuthSession, DeviceAuthError> {
    let session = device_auth_initiate(server_url)?;

    // Open browser to verification URI
    log::info!("Opening {} in browser", session.verification_uri);
    if let Err(e) = open::that(&session.verification_uri) {
        log::warn!(
            "Could not open browser: {e}. Please visit {} manually.",
            session.verification_uri
        );
    }

    // Poll for token
    let auth_session = device_auth_poll(server_url, &session)?;

    // Store token
    auth_token::write_auth_token(&auth_session)
        .map_err(|e| DeviceAuthError::TokenStorageError(e.to_string()))?;

    Ok(auth_session)
}

/// Delete auth token (logout).
pub fn device_auth_logout() -> Result<(), DeviceAuthError> {
    auth_token::delete_auth_token().map_err(|e| DeviceAuthError::TokenStorageError(e.to_string()))
}

/// Check auth token and auto-login if missing/expired.
///
/// Returns the access token string for Bearer auth.
pub fn ensure_authenticated(server_url: &str) -> Result<String, DeviceAuthError> {
    if let Some(session) = auth_token::read_auth_token() {
        if !session.is_expired() {
            return Ok(session.access_token);
        }
        log::info!("Auth token expired. Re-authenticating...");
    }

    let session = device_auth_login(server_url)?;
    Ok(session.access_token)
}

// ── Helpers ──────────────────────────────────────────────────────────

fn chrono_now_iso() -> String {
    // Simple ISO 8601 timestamp without chrono dependency
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Good enough for expiry comparison
    format!("{secs}")
}

fn format_expiry_from_now(secs: u64) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let expiry = now + secs;
    // Store as unix timestamp string for simple comparison
    format!("{expiry}")
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // Wiremock is async; our functions are blocking (reqwest::blocking).
    // Use tokio::task::spawn_blocking to call our functions from async tests.

    /// T008: device_auth_initiate parses a valid /device/code response.
    #[tokio::test]
    async fn test_device_code_initiate() {
        let mock_server = wiremock::MockServer::start().await;

        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/licensing-api/v1/device/code"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "device_code": "abc123",
                    "user_code": "ABCD-1234",
                    "verification_uri": "https://nxus.systems/device",
                    "expires_in": 600,
                    "interval": 5
                })),
            )
            .mount(&mock_server)
            .await;

        let url = format!("{}/licensing-api/v1", mock_server.uri());
        let session = tokio::task::spawn_blocking(move || {
            device_auth_initiate(&url).expect("should parse device code response")
        })
        .await
        .unwrap();

        assert_eq!(session.device_code, "abc123");
        assert_eq!(session.user_code, "ABCD-1234");
        assert_eq!(session.verification_uri, "https://nxus.systems/device");
        assert_eq!(session.expires_in, 600);
        assert_eq!(session.interval, 5);
    }

    /// T009: device_auth_poll returns token after authorization_pending responses.
    #[tokio::test]
    async fn test_device_code_poll_success() {
        let mock_server = wiremock::MockServer::start().await;

        // First two calls return authorization_pending, third returns token
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/licensing-api/v1/device/token"))
            .respond_with(
                wiremock::ResponseTemplate::new(400).set_body_json(serde_json::json!({
                    "error": "authorization_pending"
                })),
            )
            .up_to_n_times(2)
            .mount(&mock_server)
            .await;

        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/licensing-api/v1/device/token"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "access_token": "eyJ_test_token",
                    "token_type": "Bearer",
                    "expires_in": 2592000
                })),
            )
            .mount(&mock_server)
            .await;

        let url = format!("{}/licensing-api/v1", mock_server.uri());
        let session = DeviceCodeSession {
            device_code: "test-device-code".to_string(),
            user_code: "TEST-1234".to_string(),
            verification_uri: "https://test.example.com/device".to_string(),
            expires_in: 60, // short timeout for test
            interval: 1,    // 1-second polling for fast test
        };

        let auth = tokio::task::spawn_blocking(move || {
            device_auth_poll(&url, &session).expect("should get token after pending")
        })
        .await
        .unwrap();
        assert_eq!(auth.access_token, "eyJ_test_token");
        assert_eq!(auth.token_type, "Bearer");
    }

    /// T010: device_auth_poll returns DeviceCodeExpired when timeout is reached.
    #[tokio::test]
    async fn test_device_code_poll_expired() {
        let mock_server = wiremock::MockServer::start().await;

        // Always return authorization_pending
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/licensing-api/v1/device/token"))
            .respond_with(
                wiremock::ResponseTemplate::new(400).set_body_json(serde_json::json!({
                    "error": "authorization_pending"
                })),
            )
            .mount(&mock_server)
            .await;

        let url = format!("{}/licensing-api/v1", mock_server.uri());
        let session = DeviceCodeSession {
            device_code: "test-device-code".to_string(),
            user_code: "TEST-1234".to_string(),
            verification_uri: "https://test.example.com/device".to_string(),
            expires_in: 2, // 2-second timeout
            interval: 1,
        };

        let result = tokio::task::spawn_blocking(move || device_auth_poll(&url, &session))
            .await
            .unwrap();
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), DeviceAuthError::DeviceCodeExpired),
            "should return DeviceCodeExpired"
        );
    }

    /// T011: device_auth_poll returns AccessDenied when user denies.
    #[tokio::test]
    async fn test_device_code_poll_denied() {
        let mock_server = wiremock::MockServer::start().await;

        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/licensing-api/v1/device/token"))
            .respond_with(
                wiremock::ResponseTemplate::new(400).set_body_json(serde_json::json!({
                    "error": "access_denied",
                    "error_description": "User denied the authorization request"
                })),
            )
            .mount(&mock_server)
            .await;

        let url = format!("{}/licensing-api/v1", mock_server.uri());
        let session = DeviceCodeSession {
            device_code: "test-device-code".to_string(),
            user_code: "TEST-1234".to_string(),
            verification_uri: "https://test.example.com/device".to_string(),
            expires_in: 60,
            interval: 1,
        };

        let result = tokio::task::spawn_blocking(move || device_auth_poll(&url, &session))
            .await
            .unwrap();
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), DeviceAuthError::AccessDenied),
            "should return AccessDenied"
        );
    }

    /// T012: device_auth_poll increases interval on slow_down response per RFC 8628.
    #[tokio::test]
    async fn test_device_code_poll_slow_down() {
        let mock_server = wiremock::MockServer::start().await;

        // First: slow_down, then: success
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/licensing-api/v1/device/token"))
            .respond_with(
                wiremock::ResponseTemplate::new(400).set_body_json(serde_json::json!({
                    "error": "slow_down"
                })),
            )
            .up_to_n_times(1)
            .mount(&mock_server)
            .await;

        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/licensing-api/v1/device/token"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "access_token": "eyJ_after_slowdown",
                    "token_type": "Bearer",
                    "expires_in": 2592000
                })),
            )
            .mount(&mock_server)
            .await;

        let url = format!("{}/licensing-api/v1", mock_server.uri());
        let session = DeviceCodeSession {
            device_code: "test-device-code".to_string(),
            user_code: "TEST-1234".to_string(),
            verification_uri: "https://test.example.com/device".to_string(),
            expires_in: 30, // enough time for slow_down + success
            interval: 1,    // starts at 1, should become 6 after slow_down
        };

        let start = std::time::Instant::now();
        let auth = tokio::task::spawn_blocking(move || {
            device_auth_poll(&url, &session).expect("should succeed after slow_down")
        })
        .await
        .unwrap();
        let elapsed = start.elapsed();

        assert_eq!(auth.access_token, "eyJ_after_slowdown");
        // After slow_down, interval should increase by 5 (RFC 8628 Section 3.5)
        // First poll: 1s sleep, gets slow_down → interval becomes 6
        // Second poll: 6s sleep, gets success
        // Total: at least 7 seconds
        assert!(
            elapsed.as_secs() >= 6,
            "should have waited longer due to slow_down (elapsed: {}s)",
            elapsed.as_secs()
        );
    }

    /// T016: auth token roundtrip — write and read back.
    #[test]
    fn test_auth_token_roundtrip() {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let token_path = temp_dir.path().join("auth.json");

        // Override token path for test
        unsafe {
            std::env::set_var("NXUSKIT_AUTH_TOKEN_PATH", token_path.to_str().unwrap());
        }

        let session = AuthSession {
            access_token: "test_access_token_123".to_string(),
            token_type: "Bearer".to_string(),
            expires_at: format_expiry_from_now(86400),
            instance_url: "https://test.nxus.systems".to_string(),
            user_email: "test@example.com".to_string(),
        };

        crate::auth_token::write_auth_token(&session).expect("write should succeed");

        let read_back = crate::auth_token::read_auth_token().expect("should read back");
        assert_eq!(read_back.access_token, "test_access_token_123");
        assert_eq!(read_back.user_email, "test@example.com");
        assert!(!read_back.is_expired());

        // Check permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::metadata(&token_path).unwrap().permissions();
            assert_eq!(perms.mode() & 0o777, 0o600, "should be owner-only");
        }

        // Cleanup
        crate::auth_token::delete_auth_token().expect("delete should succeed");
        assert!(
            crate::auth_token::read_auth_token().is_none(),
            "should be gone after delete"
        );

        unsafe {
            std::env::remove_var("NXUSKIT_AUTH_TOKEN_PATH");
        }
    }
}
