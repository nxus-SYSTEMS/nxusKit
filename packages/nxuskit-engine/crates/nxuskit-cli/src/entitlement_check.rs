//! CLI-level entitlement checking helper.
//!
//! Wraps the core entitlement gate and maps failures to CLI error envelopes.

use crate::cli_error::CliError;

/// Check whether a feature domain is entitled under the current edition/license.
///
/// Returns `Ok(())` if entitled, or `Err(CliError::EntitlementRequired)` if not.
pub fn require_entitlement(domain: &str) -> Result<(), CliError> {
    if nxuskit_core::entitlement::check_entitlement(domain, None) {
        return Ok(());
    }

    let info = nxuskit_core::entitlement::entitlement_info(None);
    let status = info
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let effective = info
        .get("effective_edition")
        .and_then(|v| v.as_str())
        .unwrap_or("community");

    let current_edition = if status == "valid" {
        effective.to_string()
    } else {
        "community".to_string()
    };

    Err(CliError::EntitlementRequired {
        required_edition: "pro".to_string(),
        current_edition,
    })
}
