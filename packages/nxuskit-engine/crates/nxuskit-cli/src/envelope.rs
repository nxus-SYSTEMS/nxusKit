//! Trace envelope utilities for CLI response envelopes.
//!
//! Provides trace ID generation, timestamps, and request hashing
//! that every command uses to produce structured output (FR-010).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Generate a new UUID v7 trace ID (time-ordered).
pub fn new_trace_id() -> Uuid {
    Uuid::now_v7()
}

/// Return the current UTC time as an RFC 3339 string.
pub fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// Return the first 16 hex characters of the SHA-256 hash of `input`.
pub fn request_hash(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();
    hex_encode_prefix(&result, 8) // 8 bytes = 16 hex chars
}

fn hex_encode_prefix(bytes: &[u8], n: usize) -> String {
    bytes
        .iter()
        .take(n)
        .fold(String::with_capacity(n * 2), |mut s, b| {
            use std::fmt::Write;
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// Trace fields attached to every `ResponseEnvelope`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceFields {
    pub trace_id: String,
    pub timestamp: String,
    pub request_hash: String,
    pub request_metadata: RequestMetadata,
}

/// Metadata about the originating request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestMetadata {
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl TraceFields {
    /// Create new trace fields for a command invocation.
    pub fn new(command: &str, input: &str, provider: Option<&str>, model: Option<&str>) -> Self {
        Self {
            trace_id: new_trace_id().to_string(),
            timestamp: now_rfc3339(),
            request_hash: request_hash(input),
            request_metadata: RequestMetadata {
                command: command.to_string(),
                provider: provider.map(String::from),
                model: model.map(String::from),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_id_is_valid_uuid() {
        let id = new_trace_id();
        assert!(!id.is_nil());
    }

    #[test]
    fn timestamp_is_rfc3339() {
        let ts = now_rfc3339();
        assert!(ts.contains('T'));
        assert!(ts.ends_with('Z'));
    }

    #[test]
    fn request_hash_is_16_hex_chars() {
        let hash = request_hash("hello world");
        assert_eq!(hash.len(), 16);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn request_hash_is_deterministic() {
        assert_eq!(request_hash("test"), request_hash("test"));
    }
}
