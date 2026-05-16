//! OAuth pre-work utilities — state, nonce, and PKCE helpers.
//!
//! These are reusable building blocks for future OAuth flows (v0.9.1+).
//! No user-facing OAuth flow is implemented in v0.9.0 — just the
//! cryptographic primitives and session types.
//!
//! All public items are intentionally `#[allow(dead_code)]` since they
//! are pre-work for v0.9.1 OAuth support.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngExt;
use serde::Serialize;
use sha2::{Digest, Sha256};

// ── State / Nonce ─────────────────────────────────────────────────

/// Generate a cryptographically random state parameter (32 bytes, hex-encoded).
pub fn generate_state() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    hex::encode(bytes)
}

/// Generate a cryptographically random nonce (32 bytes, hex-encoded).
pub fn generate_nonce() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    hex::encode(bytes)
}

/// Verify that a received state matches the expected state (constant-time comparison).
pub fn verify_state(expected: &str, received: &str) -> bool {
    if expected.len() != received.len() {
        return false;
    }
    // Constant-time comparison to prevent timing attacks
    let mut diff = 0u8;
    for (a, b) in expected.bytes().zip(received.bytes()) {
        diff |= a ^ b;
    }
    diff == 0
}

// ── PKCE ──────────────────────────────────────────────────────────

/// Generate a PKCE code verifier (43-128 characters, URL-safe base64).
///
/// Per RFC 7636, the verifier is a random string of 43-128 characters
/// from the unreserved set [A-Z, a-z, 0-9, "-", ".", "_", "~"].
pub fn generate_code_verifier() -> String {
    let mut bytes = [0u8; 32]; // 32 bytes → 43 chars in base64url
    rand::rng().fill(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Generate a PKCE code challenge from a verifier (S256 method).
///
/// challenge = BASE64URL(SHA256(verifier))
pub fn generate_code_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

// ── Session Types ─────────────────────────────────────────────────

/// OAuth session model (pre-work types, not active in v0.9.0).
#[derive(Debug, Clone, Serialize)]
pub struct OAuthSession {
    pub state: String,
    pub nonce: String,
    pub code_verifier: String,
    pub code_challenge: String,
    pub provider_id: String,
    pub created_at: u64,
    pub timeout_secs: u64,
    pub redirect_uri: String,
}

impl OAuthSession {
    /// Create a new OAuth session with generated cryptographic parameters.
    pub fn new(provider_id: &str, redirect_uri: &str) -> Self {
        let verifier = generate_code_verifier();
        let challenge = generate_code_challenge(&verifier);
        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            state: generate_state(),
            nonce: generate_nonce(),
            code_verifier: verifier,
            code_challenge: challenge,
            provider_id: provider_id.to_string(),
            created_at,
            timeout_secs: 300,
            redirect_uri: redirect_uri.to_string(),
        }
    }

    /// Check if the session has expired.
    pub fn is_expired(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now > self.created_at + self.timeout_secs
    }
}

// ── Hex encoding helper (minimal, no extra dependency) ────────────

mod hex {
    const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";

    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        let bytes = bytes.as_ref();
        let mut s = String::with_capacity(bytes.len() * 2);
        for &b in bytes {
            s.push(HEX_CHARS[(b >> 4) as usize] as char);
            s.push(HEX_CHARS[(b & 0x0f) as usize] as char);
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_state_length() {
        let state = generate_state();
        assert_eq!(state.len(), 64, "32 bytes hex → 64 chars");
    }

    #[test]
    fn test_generate_state_uniqueness() {
        let s1 = generate_state();
        let s2 = generate_state();
        assert_ne!(s1, s2, "Two states should differ");
    }

    #[test]
    fn test_generate_nonce_length() {
        let nonce = generate_nonce();
        assert_eq!(nonce.len(), 64, "32 bytes hex → 64 chars");
    }

    #[test]
    fn test_verify_state_match() {
        let state = generate_state();
        assert!(verify_state(&state, &state));
    }

    #[test]
    fn test_verify_state_mismatch() {
        let s1 = generate_state();
        let s2 = generate_state();
        assert!(!verify_state(&s1, &s2));
    }

    #[test]
    fn test_verify_state_different_lengths() {
        assert!(!verify_state("short", "longer_string"));
    }

    #[test]
    fn test_code_verifier_length() {
        let verifier = generate_code_verifier();
        // 32 bytes → 43 chars in base64url (no padding)
        assert_eq!(verifier.len(), 43, "32 bytes → 43 base64url chars");
        assert!(verifier.len() >= 43 && verifier.len() <= 128);
    }

    #[test]
    fn test_code_challenge_deterministic() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = generate_code_challenge(verifier);
        // SHA-256 of the verifier, base64url encoded
        assert!(!challenge.is_empty());
        // Known test vector from RFC 7636 Appendix B
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn test_code_verifier_url_safe() {
        let verifier = generate_code_verifier();
        // Should only contain URL-safe base64 characters
        for ch in verifier.chars() {
            assert!(
                ch.is_ascii_alphanumeric() || ch == '-' || ch == '_',
                "Unexpected char '{ch}' in verifier"
            );
        }
    }

    #[test]
    fn test_oauth_session_new() {
        let session = OAuthSession::new("openai", "http://localhost:8080/callback");
        assert_eq!(session.provider_id, "openai");
        assert_eq!(session.timeout_secs, 300);
        assert!(!session.state.is_empty());
        assert!(!session.nonce.is_empty());
        assert!(!session.code_verifier.is_empty());
        assert!(!session.code_challenge.is_empty());
        assert!(!session.is_expired());
    }

    #[test]
    fn test_oauth_session_expired() {
        let mut session = OAuthSession::new("openai", "http://localhost:8080/callback");
        session.created_at = 0; // Epoch → expired
        assert!(session.is_expired());
    }
}
