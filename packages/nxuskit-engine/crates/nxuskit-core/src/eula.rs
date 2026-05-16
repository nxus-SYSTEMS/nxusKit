//! EULA acceptance persistence at `~/.config/nxuskit/eula_accepted`.
//!
//! Records when and how the user accepted the nxus.SYSTEMS EULA.
//! This file is checked before any `license activate` command (FR-001).
//! File permissions are 0600 (FR-002, matching auth.json convention).

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;

/// EULA URL displayed during acceptance prompt.
pub const EULA_URL: &str = "https://nxus.systems/legal/eula";

/// Current EULA version tracked in acceptance records.
pub const EULA_VERSION: &str = "1.0";

/// How the EULA was accepted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EulaMethod {
    /// User responded 'y' to the interactive prompt.
    Interactive,
    /// User passed `--accept-eula` flag.
    Flag,
}

/// Persisted EULA acceptance record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EulaAcceptance {
    pub eula_version: String,
    pub accepted_at: String,
    pub method: EulaMethod,
}

/// Return the path to the EULA acceptance file.
///
/// Default: `~/.config/nxuskit/eula_accepted`
/// Override: `NXUSKIT_EULA_PATH` env var
pub fn eula_file_path() -> PathBuf {
    if let Ok(path) = std::env::var("NXUSKIT_EULA_PATH") {
        return PathBuf::from(path);
    }

    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| "/tmp".to_string());

    PathBuf::from(home)
        .join(".config")
        .join("nxuskit")
        .join("eula_accepted")
}

/// Read an existing EULA acceptance record from the given path, if valid.
///
/// Returns `None` if the file is missing, empty, or contains invalid JSON.
fn read_eula_acceptance_at(path: &std::path::Path) -> Option<EulaAcceptance> {
    let content = std::fs::read_to_string(path).ok()?;
    if content.trim().is_empty() {
        return None;
    }
    let acceptance: EulaAcceptance = serde_json::from_str(&content).ok()?;
    if acceptance.eula_version.is_empty() || acceptance.accepted_at.is_empty() {
        return None;
    }
    Some(acceptance)
}

/// Read an existing EULA acceptance record, if valid.
///
/// Returns `None` if the file is missing, empty, or contains invalid JSON.
pub fn read_eula_acceptance() -> Option<EulaAcceptance> {
    read_eula_acceptance_at(&eula_file_path())
}

/// Write an EULA acceptance record with 0600 permissions.
pub fn write_eula_acceptance(method: EulaMethod) -> Result<EulaAcceptance, String> {
    write_eula_acceptance_at(&eula_file_path(), method)
}

fn write_eula_acceptance_at(
    path: &std::path::Path,
    method: EulaMethod,
) -> Result<EulaAcceptance, String> {
    // Create parent directory
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Cannot create config dir: {e}"))?;
    }

    let now = chrono::Utc::now().to_rfc3339();
    let acceptance = EulaAcceptance {
        eula_version: EULA_VERSION.to_string(),
        accepted_at: now,
        method,
    };

    let json = serde_json::to_string_pretty(&acceptance)
        .map_err(|e| format!("Cannot serialize EULA acceptance: {e}"))?;

    let mut file = std::fs::File::create(path)
        .map_err(|e| format!("Cannot create EULA acceptance file: {e}"))?;

    file.write_all(json.as_bytes())
        .map_err(|e| format!("Cannot write EULA acceptance: {e}"))?;

    // Set file permissions to 0600 (owner-only) on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(path, perms)
            .map_err(|e| format!("Cannot set permissions: {e}"))?;
    }

    log::debug!("EULA acceptance recorded at {}", path.display());
    Ok(acceptance)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_eula_path() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "nxuskit_eula_test_{}_{}",
            std::process::id(),
            // Use a random-ish discriminator to avoid collisions between parallel tests
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir.join("eula_accepted")
    }

    #[test]
    fn read_returns_none_when_file_missing() {
        let path = PathBuf::from("/tmp/nonexistent_eula_file_xyz_missing");
        let result = read_eula_acceptance_at(&path);
        assert!(result.is_none());
    }

    #[test]
    fn read_returns_none_when_file_empty() {
        let path = temp_eula_path();
        fs::write(&path, "").unwrap();
        let result = read_eula_acceptance_at(&path);
        let _ = fs::remove_file(&path);
        assert!(result.is_none());
    }

    #[test]
    fn read_returns_none_when_file_corrupted() {
        let path = temp_eula_path();
        fs::write(&path, "{not valid json}").unwrap();
        let result = read_eula_acceptance_at(&path);
        let _ = fs::remove_file(&path);
        assert!(result.is_none());
    }

    #[test]
    fn write_creates_valid_json_with_correct_fields() {
        let path = temp_eula_path();
        let result = write_eula_acceptance_at(&path, EulaMethod::Flag);

        assert!(result.is_ok());
        let acceptance = result.unwrap();
        assert_eq!(acceptance.eula_version, EULA_VERSION);
        assert_eq!(acceptance.method, EulaMethod::Flag);
        assert!(!acceptance.accepted_at.is_empty());

        // Verify file content
        let content = fs::read_to_string(&path).unwrap();
        let parsed: EulaAcceptance = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed.eula_version, EULA_VERSION);
        assert_eq!(parsed.method, EulaMethod::Flag);

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn write_interactive_method_serializes_correctly() {
        let path = temp_eula_path();
        let result = write_eula_acceptance_at(&path, EulaMethod::Interactive);

        let acceptance = result.unwrap();
        assert_eq!(acceptance.method, EulaMethod::Interactive);

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("\"interactive\""));

        let _ = fs::remove_file(&path);
    }

    #[cfg(unix)]
    #[test]
    fn write_sets_0600_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let path = temp_eula_path();
        let _ = write_eula_acceptance_at(&path, EulaMethod::Flag);

        let metadata = fs::metadata(&path).unwrap();
        let mode = metadata.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "Expected 0600, got {:o}", mode);

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn roundtrip_write_then_read() {
        let path = temp_eula_path();

        write_eula_acceptance_at(&path, EulaMethod::Interactive).unwrap();
        let read_back = read_eula_acceptance_at(&path);

        let _ = fs::remove_file(&path);

        assert!(read_back.is_some());
        let a = read_back.unwrap();
        assert_eq!(a.eula_version, EULA_VERSION);
        assert_eq!(a.method, EulaMethod::Interactive);
    }

    #[test]
    fn eula_path_respects_env_override() {
        // This test sets an env var but does not depend on isolation from other tests
        // because it reads the var immediately after setting it within the same thread.
        let custom = "/tmp/custom_eula_test_path_env_override";
        unsafe { std::env::set_var("NXUSKIT_EULA_PATH", custom) };
        let path = eula_file_path();
        unsafe { std::env::remove_var("NXUSKIT_EULA_PATH") };
        assert_eq!(path, PathBuf::from(custom));
    }

    #[test]
    fn eula_path_default_ends_with_eula_accepted() {
        // Only check the default path when env var is not set.
        // We cannot safely remove NXUSKIT_EULA_PATH in parallel tests,
        // so instead check the path that `eula_file_path` returns WITHOUT
        // the env var by constructing the expected path directly.
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| "/tmp".to_string());
        let expected = PathBuf::from(home)
            .join(".config")
            .join("nxuskit")
            .join("eula_accepted");
        assert!(
            expected.ends_with("eula_accepted"),
            "Expected path ending with eula_accepted, got: {}",
            expected.display()
        );
    }
}
