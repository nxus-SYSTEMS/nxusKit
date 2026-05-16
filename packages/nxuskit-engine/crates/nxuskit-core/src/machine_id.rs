//! Machine fingerprinting for license token binding.
//!
//! Generates a deterministic, privacy-preserving machine identifier by hashing
//! the OS-level machine ID with an application-specific salt.

use sha2::{Digest, Sha256};

/// Application-specific salt to prevent cross-application fingerprint correlation.
const SALT: &str = "nxuskit-v1";

/// Get the machine fingerprint for this device.
///
/// Returns a `sha256:<64-hex-chars>` string derived from the OS machine ID
/// combined with an application-specific salt.
///
/// # Errors
///
/// Returns an error if the machine ID cannot be determined (e.g., in Docker
/// containers or minimal environments without `/etc/machine-id` or equivalent).
pub fn get_machine_fingerprint() -> Result<String, MachineIdError> {
    let raw_id = machine_uid::get().map_err(|e| MachineIdError::Unavailable(e.to_string()))?;

    if raw_id.trim().is_empty() {
        return Err(MachineIdError::Unavailable(
            "machine ID returned empty string".to_string(),
        ));
    }

    let fingerprint = compute_fingerprint(&raw_id);
    Ok(fingerprint)
}

/// Compute the salted fingerprint from a raw machine ID.
///
/// This is a pure function exposed for testing — the production code path
/// goes through `get_machine_fingerprint()`.
pub(crate) fn compute_fingerprint(raw_machine_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw_machine_id.as_bytes());
    hasher.update(b":");
    hasher.update(SALT.as_bytes());
    let hash = hasher.finalize();
    format!("sha256:{}", hex_encode(&hash))
}

/// Encode bytes as lowercase hexadecimal.
fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write;
        write!(s, "{b:02x}").unwrap();
    }
    s
}

/// Errors that can occur during machine fingerprinting.
#[derive(Debug)]
pub enum MachineIdError {
    /// Machine ID could not be determined (Docker, minimal container, etc.)
    Unavailable(String),
}

impl std::fmt::Display for MachineIdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MachineIdError::Unavailable(msg) => {
                write!(f, "machine ID unavailable: {msg}")
            }
        }
    }
}

impl std::error::Error for MachineIdError {}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_fingerprint_format() {
        let fp = compute_fingerprint("test-machine-id-12345");
        assert!(
            fp.starts_with("sha256:"),
            "Should start with sha256: prefix"
        );
        // sha256: (7 chars) + 64 hex chars = 71 chars total
        assert_eq!(fp.len(), 71, "Expected sha256: prefix + 64 hex chars");
        // Verify all chars after prefix are hex
        let hex_part = &fp[7..];
        assert!(
            hex_part.chars().all(|c| c.is_ascii_hexdigit()),
            "Hash part should be all hex digits"
        );
    }

    #[test]
    fn test_fingerprint_deterministic() {
        let fp1 = compute_fingerprint("same-machine-id");
        let fp2 = compute_fingerprint("same-machine-id");
        assert_eq!(fp1, fp2, "Same input should produce same fingerprint");
    }

    #[test]
    fn test_fingerprint_different_inputs() {
        let fp1 = compute_fingerprint("machine-a");
        let fp2 = compute_fingerprint("machine-b");
        assert_ne!(
            fp1, fp2,
            "Different inputs should produce different fingerprints"
        );
    }

    #[test]
    fn test_fingerprint_includes_salt() {
        // A fingerprint with our salt should differ from a plain SHA-256 of the input
        use sha2::Digest;
        let raw = "test-machine";
        let plain_hash = {
            let mut h = Sha256::new();
            h.update(raw.as_bytes());
            let hash = h.finalize();
            format!("sha256:{}", hex_encode(&hash))
        };
        let salted = compute_fingerprint(raw);
        assert_ne!(plain_hash, salted, "Salt should change the hash");
    }

    #[test]
    fn test_get_machine_fingerprint_succeeds() {
        // This test runs on the actual machine — it should succeed unless
        // running in a minimal container without a machine ID.
        match get_machine_fingerprint() {
            Ok(fp) => {
                assert!(fp.starts_with("sha256:"));
                assert_eq!(fp.len(), 71);
            }
            Err(MachineIdError::Unavailable(_)) => {
                // Acceptable in CI containers
            }
        }
    }
}
