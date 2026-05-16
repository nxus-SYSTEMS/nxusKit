//! SDK version compatibility checking.

use crate::NxuskitError;

/// The version of the nxuskit wrapper crate.
const WRAPPER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Check that the loaded SDK version is compatible with this wrapper.
///
/// Compatibility rule: `sdk.major == wrapper.major && sdk.minor >= wrapper.minor`.
/// A major version mismatch is a hard error. A minor version where SDK < wrapper
/// produces an error (wrapper expects features the SDK doesn't have).
pub(crate) fn check_version(sdk_version_str: &str) -> Result<(), NxuskitError> {
    let sdk_ver =
        semver::Version::parse(sdk_version_str).map_err(|e| NxuskitError::VersionMismatch {
            expected: WRAPPER_VERSION.to_string(),
            found: format!("{sdk_version_str} (parse error: {e})"),
        })?;

    let wrapper_ver =
        semver::Version::parse(WRAPPER_VERSION).map_err(|e| NxuskitError::Internal {
            message: format!("Failed to parse wrapper version {WRAPPER_VERSION}: {e}"),
        })?;

    // Major version must match exactly.
    if sdk_ver.major != wrapper_ver.major {
        return Err(NxuskitError::VersionMismatch {
            expected: WRAPPER_VERSION.to_string(),
            found: sdk_version_str.to_string(),
        });
    }

    // SDK minor must be >= wrapper minor (wrapper may use features from its minor).
    if sdk_ver.minor < wrapper_ver.minor {
        return Err(NxuskitError::VersionMismatch {
            expected: WRAPPER_VERSION.to_string(),
            found: sdk_version_str.to_string(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatible_versions() {
        // Same version is always compatible.
        assert!(check_version(WRAPPER_VERSION).is_ok());
    }

    #[test]
    fn sdk_newer_minor_is_ok() {
        // SDK 0.99.0 is compatible with wrapper 0.1.0 (same major, higher minor).
        let wrapper = semver::Version::parse(WRAPPER_VERSION).unwrap();
        let newer = format!("{}.99.0", wrapper.major);
        assert!(check_version(&newer).is_ok());
    }

    #[test]
    fn major_mismatch_fails() {
        let wrapper = semver::Version::parse(WRAPPER_VERSION).unwrap();
        let different_major = format!("{}.0.0", wrapper.major + 1);
        assert!(check_version(&different_major).is_err());
    }

    #[test]
    fn invalid_version_string_fails() {
        assert!(check_version("not-a-version").is_err());
    }
}
