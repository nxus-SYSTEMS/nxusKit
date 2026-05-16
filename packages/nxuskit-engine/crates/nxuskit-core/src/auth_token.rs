//! Auth token (device code flow) persistence at `~/.config/nxuskit/auth.json`.
//!
//! This module handles reading, writing, and deleting the platform auth token
//! obtained via the RFC 8628 device code flow. This token is SEPARATE from
//! license tokens (`~/.nxuskit/license.token`) and provider credentials.

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;

/// Persisted auth session from the device code flow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthSession {
    pub access_token: String,
    #[serde(default = "default_bearer")]
    pub token_type: String,
    #[serde(default)]
    pub expires_at: String,
    #[serde(default)]
    pub instance_url: String,
    #[serde(default)]
    pub user_email: String,
}

fn default_bearer() -> String {
    "Bearer".to_string()
}

impl AuthSession {
    /// Check whether this session has expired.
    pub fn is_expired(&self) -> bool {
        use std::time::{SystemTime, UNIX_EPOCH};
        // Empty or unparseable expires_at → treat as not expired
        // (device code tokens may not have an expiry)
        if self.expires_at.is_empty() {
            return false;
        }
        let expiry: u64 = match self.expires_at.parse() {
            Ok(v) => v,
            Err(_) => return false, // unparseable → not expired
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now >= expiry
    }
}

/// Return the path to the auth token file.
///
/// Default: `~/.config/nxuskit/auth.json`
/// Override: `NXUSKIT_AUTH_TOKEN_PATH` env var
pub fn auth_token_path() -> PathBuf {
    if let Ok(path) = std::env::var("NXUSKIT_AUTH_TOKEN_PATH") {
        return PathBuf::from(path);
    }

    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| "/tmp".to_string());

    PathBuf::from(home)
        .join(".config")
        .join("nxuskit")
        .join("auth.json")
}

/// Read the stored auth token, if it exists.
pub fn read_auth_token() -> Option<AuthSession> {
    let path = auth_token_path();
    let content = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Write the auth token to disk with owner-only permissions.
pub fn write_auth_token(session: &AuthSession) -> Result<(), String> {
    let path = auth_token_path();

    // Create parent directory
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Cannot create config dir: {e}"))?;
    }

    let json = serde_json::to_string_pretty(session)
        .map_err(|e| format!("Cannot serialize auth token: {e}"))?;

    let mut file =
        std::fs::File::create(&path).map_err(|e| format!("Cannot create auth token file: {e}"))?;

    file.write_all(json.as_bytes())
        .map_err(|e| format!("Cannot write auth token: {e}"))?;

    // Set file permissions to 0600 (owner-only) on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&path, perms)
            .map_err(|e| format!("Cannot set permissions: {e}"))?;
    }

    log::debug!("Auth token stored at {}", path.display());
    Ok(())
}

/// Delete the auth token file.
pub fn delete_auth_token() -> Result<(), String> {
    let path = auth_token_path();
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| format!("Cannot remove auth token: {e}"))?;
        log::debug!("Auth token removed from {}", path.display());
    }
    Ok(())
}

/// Read the Bearer token string for API calls, if available and not expired.
pub fn read_bearer_token() -> Option<String> {
    let path = auth_token_path();
    log::debug!("Reading auth token from: {}", path.display());

    let session = match read_auth_token() {
        Some(s) => s,
        None => {
            log::debug!("No auth token found at {}", path.display());
            return None;
        }
    };

    if session.is_expired() {
        log::debug!("Auth token expired (expires_at={})", session.expires_at);
        return None;
    }

    log::debug!("Auth token valid, {} chars", session.access_token.len());
    Some(session.access_token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_session_expired() {
        let session = AuthSession {
            access_token: "test".to_string(),
            token_type: "Bearer".to_string(),
            expires_at: "0".to_string(), // epoch = definitely expired
            instance_url: "https://test.example.com".to_string(),
            user_email: "test@example.com".to_string(),
        };
        assert!(session.is_expired());
    }

    #[test]
    fn test_auth_session_not_expired() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let future = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 86400; // 1 day from now
        let session = AuthSession {
            access_token: "test".to_string(),
            token_type: "Bearer".to_string(),
            expires_at: format!("{future}"),
            instance_url: "https://test.example.com".to_string(),
            user_email: "test@example.com".to_string(),
        };
        assert!(!session.is_expired());
    }
}
