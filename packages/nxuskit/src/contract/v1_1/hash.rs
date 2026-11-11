//! Canonical-JSON canonicalization + SHA-256 hashing helpers for the v1.1
//! contract surface (research.md §4–5).
//!
//! Canonicalization is the sorted-keys + no-whitespace subset of RFC 8785:
//!   * Every JSON object's keys are emitted in lexicographic byte order.
//!   * Arrays retain their input order (sequences are significant).
//!   * No whitespace between tokens.
//!   * Number, string, boolean, and null encoding follow `serde_json`'s
//!     default compact emitter (sufficient for our DTO shapes; reproducible
//!     across `serde_json` patch versions).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Write;

/// SHA-256 hex hash of the canonical JSON of an artifact payload.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ArtifactCanonicalHash(String);

impl ArtifactCanonicalHash {
    /// Borrow the 64-char lower-hex digest.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for ArtifactCanonicalHash {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for ArtifactCanonicalHash {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

/// 12-character hex prefix of an [`ArtifactCanonicalHash`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ArtifactDisplayHash(String);

impl ArtifactDisplayHash {
    /// Borrow the 12-char lower-hex prefix.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for ArtifactDisplayHash {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for ArtifactDisplayHash {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

/// Errors surfaced by the canonical hashing helpers.
#[derive(Debug, thiserror::Error)]
pub enum HashError {
    /// Underlying `serde_json` failure while encoding a value into canonical
    /// bytes. Wrapped so the public DTO surface does not leak the
    /// `serde_json::Error` type — consumers branch on this kind only.
    #[error("canonical encoding failed: {0}")]
    Encoding(String),
}

impl From<serde_json::Error> for HashError {
    fn from(value: serde_json::Error) -> Self {
        HashError::Encoding(value.to_string())
    }
}

/// Encode an arbitrary `serde_json::Value` into canonical JSON bytes.
///
/// Keys are sorted at every object nesting level; arrays are emitted in input
/// order. The output contains no inter-token whitespace.
pub fn canonical_json_bytes(value: &serde_json::Value) -> Result<Vec<u8>, HashError> {
    let mut buf = Vec::with_capacity(64);
    write_canonical(&mut buf, value)?;
    Ok(buf)
}

/// Compute the SHA-256 canonical hash of a `serde_json::Value`.
pub fn canonical_hash(value: &serde_json::Value) -> Result<ArtifactCanonicalHash, HashError> {
    let bytes = canonical_json_bytes(value)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for byte in digest.iter() {
        // lower-hex
        hex.push_str(&format!("{byte:02x}"));
    }
    Ok(ArtifactCanonicalHash(hex))
}

/// Deterministic 12-char display projection of a canonical hash.
pub fn display_short(hash: &ArtifactCanonicalHash) -> ArtifactDisplayHash {
    let s = &hash.0;
    debug_assert!(
        s.len() >= 12,
        "canonical hash must be at least 12 hex chars"
    );
    ArtifactDisplayHash(s[..12].to_owned())
}

// ---------------------------------------------------------------------------
// Internal canonical writer
//
// The writer walks the `serde_json::Value` tree once. For objects, keys are
// collected into a Vec, sorted by their UTF-8 bytes, and emitted in that
// order. Scalars are delegated to `serde_json::to_writer` over the scalar
// value, which produces a stable, compact encoding for null/bool/number/string.
// ---------------------------------------------------------------------------

fn write_canonical<W: Write>(w: &mut W, value: &serde_json::Value) -> Result<(), HashError> {
    match value {
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_) => {
            // serde_json's default writer is compact (no whitespace) for
            // scalars; this gives us stable encoding for these leaves.
            serde_json::to_writer(&mut *w, value)?;
            Ok(())
        }
        serde_json::Value::Array(items) => {
            w.write_all(b"[").map_err(io_to_hash)?;
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    w.write_all(b",").map_err(io_to_hash)?;
                }
                write_canonical(w, item)?;
            }
            w.write_all(b"]").map_err(io_to_hash)?;
            Ok(())
        }
        serde_json::Value::Object(map) => {
            // Sort keys lexicographically by UTF-8 bytes.
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_unstable();
            w.write_all(b"{").map_err(io_to_hash)?;
            for (i, key) in keys.iter().enumerate() {
                if i > 0 {
                    w.write_all(b",").map_err(io_to_hash)?;
                }
                // Re-encode the key as a JSON string via serde_json so escape
                // rules match the canonical encoder used for string scalars.
                serde_json::to_writer(&mut *w, &serde_json::Value::String((*key).clone()))?;
                w.write_all(b":").map_err(io_to_hash)?;
                write_canonical(w, &map[*key])?;
            }
            w.write_all(b"}").map_err(io_to_hash)?;
            Ok(())
        }
    }
}

fn io_to_hash(e: std::io::Error) -> HashError {
    HashError::Encoding(e.to_string())
}
