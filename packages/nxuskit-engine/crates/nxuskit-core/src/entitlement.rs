//! Entitlement gate for premium feature domains.
//!
//! v0.9.1 model: Edition flag = **capability** (binary contains Pro code),
//! Token = **authorization** (sole source). A valid JWT token is required for
//! ALL Pro features, regardless of compile-time edition.
//!
//! Community/OSS features work without any token. Pro features require a valid
//! license token from the resolution chain (env → file → API param).
//!
//! License key storage: The most-recently-created provider's license key is cached
//! in a thread-local so that C ABI entry points (which have no config parameter)
//! can pass it to `check_entitlement` without changing the ABI surface.

use std::cell::RefCell;

use crate::license;
use crate::license_types::TokenType;

thread_local! {
    /// Thread-local cache of the license key set by `create_provider_from_json`.
    /// C ABI domain entry points read this via `current_license_key()`.
    static LICENSE_KEY: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Store a license key for the current thread.
///
/// Called by `create_provider_from_json` after extracting `license_key` from config JSON.
/// Passing `None` clears any previously stored key.
pub(crate) fn set_license_key(key: Option<&str>) {
    LICENSE_KEY.with(|cell| {
        *cell.borrow_mut() = key.map(String::from);
    });
}

/// Retrieve the license key stored for the current thread.
///
/// Returns `None` if no provider has been created on this thread or if the
/// provider config did not include a `license_key` field.
pub(crate) fn current_license_key() -> Option<String> {
    LICENSE_KEY.with(|cell| cell.borrow().clone())
}

// ── Edition Constants ───────────────────────────────────────────────

/// Compile-time edition from `NXUSKIT_EDITION` build env.
const EDITION_STR: &str = env!("NXUSKIT_EDITION");

/// Edition tiers (ordered by capability).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Edition {
    Oss = 0,
    Pro = 1,
    Enterprise = 2,
}

impl Edition {
    fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "pro" => Edition::Pro,
            "enterprise" => Edition::Enterprise,
            _ => Edition::Oss,
        }
    }
}

/// Get the compile-time edition.
pub(crate) fn current_edition() -> Edition {
    Edition::from_str(EDITION_STR)
}

// ── Feature-to-Edition Mapping (Catalog-Driven) ──────────────────────

/// Minimum edition required for a feature domain, resolved from the
/// product catalog embedded at build time.
///
/// Checks each edition (Community → Pro → Enterprise) and returns the
/// first (lowest) edition whose feature list includes the domain.
/// Unknown domains default to Pro (conservative, FR-019).
fn required_edition(domain: &str) -> Edition {
    use crate::catalog::catalog_features;

    let community_features = catalog_features("community");
    if community_features.contains(&domain) {
        return Edition::Oss;
    }

    let pro_features = catalog_features("pro");
    if pro_features.contains(&domain) {
        return Edition::Pro;
    }

    let enterprise_features = catalog_features("enterprise");
    if enterprise_features.contains(&domain) {
        return Edition::Enterprise;
    }

    // Unknown domain → default to Pro (conservative, backward compatible)
    Edition::Pro
}

/// Check whether a feature domain is available for a given edition,
/// considering token-level `features_override`.
///
/// Returns `true` if the domain is in the edition's catalog features
/// OR in the features_override list.
#[allow(dead_code)]
fn is_feature_available(
    domain: &str,
    edition_str: &str,
    features_override: Option<&[String]>,
) -> bool {
    use crate::catalog::catalog_features;

    let features = catalog_features(edition_str);
    if features.contains(&domain) {
        return true;
    }

    // Check features_override from token
    if let Some(overrides) = features_override
        && overrides.iter().any(|f| f == domain)
    {
        return true;
    }

    false
}

// ── Entitlement Result ──────────────────────────────────────────────

/// Entitlement check result codes (matches C ABI contract).
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EntitlementResult {
    /// Feature access granted.
    Granted = 0,
    /// Domain not available in current edition (binary lacks Pro code).
    FeatureUnavailable = 1,
    /// License token has expired.
    LicenseExpired = 2,
    /// License token format or signature is invalid.
    LicenseInvalid = 3,
    /// Domain requires a higher edition than the token grants.
    EditionInsufficient = 4,
    /// Trial token suspended (not activated within 7-day grace period).
    TrialSuspended = 5,
    /// Trial auto-issuance failed (kept for C ABI backward compat, no longer constructed).
    #[allow(dead_code)]
    TrialIssuanceFailed = 6,
    /// Deployment token version ceiling exceeded.
    VersionCeilingExceeded = 7,
}

/// Check whether the given domain is entitled under the current edition/license.
///
/// Returns `true` if the feature is allowed, `false` if denied.
/// When returning `false`, sets the last error via `set_last_error()`.
///
/// # Arguments
/// * `domain` — Feature domain being accessed.
/// * `license_key` — Optional license key from consumer config (lowest priority
///   in the resolution chain).
pub fn check_entitlement(domain: &str, license_key: Option<&str>) -> bool {
    let result = check_entitlement_detailed(domain, license_key);
    if result != EntitlementResult::Granted {
        let message = match result {
            EntitlementResult::FeatureUnavailable => format!(
                "Feature '{domain}' requires a Pro license. \
                 Run: nxuskit-cli license login && nxuskit-cli license activate --trial"
            ),
            EntitlementResult::LicenseExpired => format!(
                "License expired. Feature '{domain}' requires an active Pro license. \
                 Renew at: https://nxus.systems/pricing"
            ),
            EntitlementResult::LicenseInvalid => format!(
                "Invalid license token for feature '{domain}'. \
                 Run: nxuskit-cli license status (to diagnose)"
            ),
            EntitlementResult::EditionInsufficient => format!(
                "Feature '{domain}' requires a higher edition. \
                 Current token does not grant '{domain}' access."
            ),
            EntitlementResult::TrialSuspended => format!(
                "Trial suspended (not activated within 7-day grace period). \
                 Feature '{domain}' unavailable. Contact support."
            ),
            EntitlementResult::TrialIssuanceFailed => format!(
                "Feature '{domain}' requires a Pro license. \
                 Run: nxuskit-cli license login && nxuskit-cli license activate --trial"
            ),
            EntitlementResult::VersionCeilingExceeded => format!(
                "SDK version exceeds token ceiling for feature '{domain}'. \
                 Update your deployment token for this SDK version."
            ),
            EntitlementResult::Granted => unreachable!(),
        };
        let error_type = match result {
            EntitlementResult::FeatureUnavailable => "feature_unavailable",
            EntitlementResult::LicenseExpired => "license_expired",
            EntitlementResult::LicenseInvalid => "license_invalid",
            EntitlementResult::EditionInsufficient => "edition_insufficient",
            EntitlementResult::TrialSuspended => "trial_suspended",
            EntitlementResult::TrialIssuanceFailed => "trial_issuance_failed",
            EntitlementResult::VersionCeilingExceeded => "version_ceiling_exceeded",
            EntitlementResult::Granted => unreachable!(),
        };
        crate::error::set_last_error(error_type, &message, None);
        false
    } else {
        true
    }
}

/// Detailed entitlement check returning a specific result code.
///
/// v0.9.1 logic:
/// 1. OSS features → always granted (no token needed)
/// 2. Pro/Enterprise features → binary must have capability (compile-time edition)
///    AND a valid JWT token must authorize access.
/// 3. Token resolution chain: env var → file → explicit `license_key` param
pub(crate) fn check_entitlement_detailed(
    domain: &str,
    license_key: Option<&str>,
) -> EntitlementResult {
    let edition = current_edition();
    let required = required_edition(domain);

    // OSS features are always granted regardless of token/edition
    if required == Edition::Oss {
        return EntitlementResult::Granted;
    }

    // Pro/Enterprise features require:
    // 1. Binary capability: compile-time edition must include the feature code
    if required > edition {
        // Binary doesn't contain the code for this feature domain
        return EntitlementResult::FeatureUnavailable;
    }

    // 2. Token authorization: a valid JWT token must grant access
    //    The resolution chain uses `license_key` as the lowest-priority explicit param.
    let resolution = license::resolve_token(license_key);

    if resolution.valid
        && let Some(ref claims) = resolution.claims
    {
        // Map token type to edition grant
        let token_edition = match claims.token_type {
            TokenType::Trial
            | TokenType::Developer
            | TokenType::Deployment
            | TokenType::RealPurchase
            | TokenType::Leased => Edition::Pro,
        };

        if required > token_edition {
            return EntitlementResult::EditionInsufficient;
        }

        return EntitlementResult::Granted;
    }

    // Token resolution failed — check the error for specific result codes
    if let Some(ref error_str) = resolution.error {
        if error_str.contains("expired") {
            return EntitlementResult::LicenseExpired;
        }
        if error_str.contains("suspended") {
            return EntitlementResult::TrialSuspended;
        }
        if error_str.contains("ceiling") {
            return EntitlementResult::VersionCeilingExceeded;
        }
        // Token was found but invalid
        return EntitlementResult::LicenseInvalid;
    }

    // No token found → require explicit license installation.
    // Auto-trial issuance removed per 057 spec. Users must explicitly:
    //   nxuskit-cli license login → nxuskit-cli license activate --trial
    EntitlementResult::FeatureUnavailable
}

/// Get current UTC time as ISO 8601 string (public, for audit events).
pub fn chrono_now_iso_public() -> String {
    chrono_now_iso()
}

/// Get current UTC time as ISO 8601 string (without external crate).
fn chrono_now_iso() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    // Convert to rough ISO format: YYYY-MM-DDTHH:MM:SSZ
    let secs = now.as_secs();
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Calculate year/month/day from days since epoch (1970-01-01)
    let (year, month, day) = days_to_date(days);

    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

/// Convert days since Unix epoch to (year, month, day).
fn days_to_date(days: u64) -> (u64, u64, u64) {
    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

// ── Limit Enforcement ───────────────────────────────────────────────

/// Resolve the effective limits for the current edition and token.
///
/// Resolution order:
/// 1. Resolve token from the chain (env → file → explicit param)
/// 2. If valid token: use token's edition + token-level limit overrides
/// 3. If no valid token: use compile-time edition (Community for OSS build)
/// 4. Merge catalog defaults with any token overrides
pub fn effective_limits(license_key: Option<&str>) -> crate::catalog::EditionLimits {
    use crate::catalog;

    let resolution = license::resolve_token(license_key);

    if resolution.valid
        && let Some(ref claims) = resolution.claims
    {
        return catalog::catalog_limits(&claims.edition);
    }

    // No valid token — use compile-time edition catalog limits
    let edition_str = match current_edition() {
        Edition::Oss => "community",
        Edition::Pro => "pro",
        Edition::Enterprise => "enterprise",
    };
    catalog::catalog_limits(edition_str)
}

/// Check a numerical limit and set an error if exceeded.
///
/// Returns `true` if within limit, `false` if limit exceeded.
/// When `limit` is `None`, the limit is unlimited (always returns `true`).
pub fn check_limit(limit: Option<u64>, current: u64, limit_name: &str, tier: &str) -> bool {
    if let Some(max) = limit
        && current >= max
    {
        crate::error::set_last_error(
            "limit_exceeded",
            &format!(
                "{limit_name} limit reached ({current}/{max}). \
                 Current tier: {tier}. Upgrade to Pro for higher limits: \
                 https://nxus.systems/pricing"
            ),
            None,
        );
        return false;
    }
    true
}

// ── C ABI Functions ─────────────────────────────────────────────────

/// Get entitlement information as JSON.
///
/// Returns edition, effective edition (considering token), feature list,
/// token status, and license details.
pub fn entitlement_info(license_key: Option<&str>) -> serde_json::Value {
    let edition = current_edition();
    let edition_str = match edition {
        Edition::Oss => "oss",
        Edition::Pro => "pro",
        Edition::Enterprise => "enterprise",
    };

    // Resolve token from the full chain
    let resolution = license::resolve_token(license_key);

    // Determine effective edition from token
    let (effective_edition, token_status, token_info) = if resolution.valid {
        if let Some(ref claims) = resolution.claims {
            let token_edition = match claims.token_type {
                TokenType::Trial
                | TokenType::Developer
                | TokenType::Deployment
                | TokenType::RealPurchase
                | TokenType::Leased => Edition::Pro,
            };
            let effective = std::cmp::max(edition, token_edition);
            let info = serde_json::json!({
                "type": claims.token_type.to_string(),
                "source": resolution.source.to_string(),
                "days_remaining": claims.days_remaining(),
                "edition": claims.edition,
            });
            (effective, "valid", Some(info))
        } else {
            (edition, "no_token", None)
        }
    } else if let Some(ref error) = resolution.error {
        let info = serde_json::json!({
            "error": error,
            "source": resolution.source.to_string(),
        });
        // Include claims if available (token parsed but failed validation)
        let info = if let Some(ref claims) = resolution.claims {
            let mut i = info;
            i["token_machine_id"] = serde_json::json!(claims.machine_id);
            i["token_edition"] = serde_json::json!(claims.edition);
            i["token_type"] = serde_json::json!(claims.token_type.to_string());
            i["token_activated"] = serde_json::json!(claims.activated);
            i
        } else {
            info
        };
        (edition, "invalid", Some(info))
    } else {
        (edition, "no_token", None)
    };

    let effective_str = match effective_edition {
        Edition::Oss => "oss",
        Edition::Pro => "pro",
        Edition::Enterprise => "enterprise",
    };
    // Catalog uses "community" for the OSS edition
    let catalog_edition = match effective_edition {
        Edition::Oss => "community",
        Edition::Pro => "pro",
        Edition::Enterprise => "enterprise",
    };

    // Build feature list from catalog (catalog-driven, not hardcoded)
    let features: Vec<&str> = crate::catalog::catalog_features(catalog_edition).to_vec();

    // Get effective limits from catalog
    let effective_limits = crate::catalog::catalog_limits(catalog_edition);

    let mut result = serde_json::json!({
        "edition": edition_str,
        "effective_edition": effective_str,
        "features": features,
        "effective_limits": {
            "max_sessions": effective_limits.max_sessions,
            "max_cached_rulebases": effective_limits.max_cached_rulebases,
            "max_rules_per_session": effective_limits.max_rules_per_session,
            "max_facts_per_session": effective_limits.max_facts_per_session,
            "max_bayesian_nodes": effective_limits.max_bayesian_nodes,
            "max_solver_constraints": effective_limits.max_solver_constraints,
            "seats": effective_limits.seats,
        },
        "status": token_status,
    });

    if let Some(info) = token_info {
        result["token"] = info;
    }
    if let Some(ref error) = resolution.error {
        result["error"] = serde_json::Value::String(error.clone());
    }

    result
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Mutex to serialize tests that mutate process-global environment variables
    /// (HOME, NXUSKIT_LICENSE_TOKEN). Without this, concurrent test threads race
    /// on env state and produce flaky failures.
    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn test_edition_from_str() {
        assert_eq!(Edition::from_str("oss"), Edition::Oss);
        assert_eq!(Edition::from_str("pro"), Edition::Pro);
        assert_eq!(Edition::from_str("enterprise"), Edition::Enterprise);
        assert_eq!(Edition::from_str("unknown"), Edition::Oss);
        assert_eq!(Edition::from_str("PRO"), Edition::Pro);
    }

    #[test]
    fn test_required_edition_mapping() {
        // Community/OSS features (from catalog)
        assert_eq!(required_edition("llm_cloud"), Edition::Oss);
        assert_eq!(required_edition("llm_local"), Edition::Oss);
        assert_eq!(required_edition("clips"), Edition::Oss);
        assert_eq!(required_edition("bayesian"), Edition::Oss);
        assert_eq!(required_edition("auth"), Edition::Oss);
        assert_eq!(required_edition("tool_calling"), Edition::Oss);

        // Pro features (from catalog)
        assert_eq!(required_edition("zen"), Edition::Pro);
        assert_eq!(required_edition("solver"), Edition::Pro);
        assert_eq!(required_edition("mcp"), Edition::Pro);
        assert_eq!(required_edition("plugin_loading"), Edition::Pro);
        assert_eq!(required_edition("clips_advanced"), Edition::Pro);

        // Enterprise features (from catalog)
        assert_eq!(required_edition("plugin_config_paths"), Edition::Enterprise);
        assert_eq!(
            required_edition("delegated_trust_roots"),
            Edition::Enterprise
        );
        assert_eq!(required_edition("priority_support"), Edition::Enterprise);

        // Unknown domain defaults to Pro (FR-019)
        assert_eq!(required_edition("unknown_feature"), Edition::Pro);
        assert_eq!(required_edition("quantum"), Edition::Pro);
    }

    // ── Catalog Feature Gate Tests (T023-T027) ────────────────────

    #[test]
    fn test_catalog_community_features() {
        use crate::catalog::catalog_features;
        let features = catalog_features("community");
        assert!(features.contains(&"llm_cloud"));
        assert!(features.contains(&"llm_local"));
        assert!(features.contains(&"clips"));
        assert!(features.contains(&"bayesian"));
        assert!(features.contains(&"auth"));
        assert!(features.contains(&"tool_calling"));
        // Pro features should NOT be in community
        assert!(!features.contains(&"zen"));
        assert!(!features.contains(&"solver"));
    }

    #[test]
    fn test_catalog_pro_inherits_community() {
        use crate::catalog::catalog_features;
        let features = catalog_features("pro");
        // All community features inherited
        assert!(features.contains(&"llm_cloud"));
        assert!(features.contains(&"clips"));
        assert!(features.contains(&"bayesian"));
        // Pro-only features
        assert!(features.contains(&"zen"));
        assert!(features.contains(&"solver"));
        assert!(features.contains(&"mcp"));
        assert!(features.contains(&"plugin_loading"));
        assert!(features.contains(&"clips_advanced"));
    }

    #[test]
    fn test_catalog_enterprise_inherits_pro() {
        use crate::catalog::catalog_features;
        let features = catalog_features("enterprise");
        // All pro features inherited
        assert!(features.contains(&"zen"));
        assert!(features.contains(&"solver"));
        assert!(features.contains(&"clips"));
        // Enterprise-only features
        assert!(features.contains(&"plugin_config_paths"));
        assert!(features.contains(&"delegated_trust_roots"));
        assert!(features.contains(&"priority_support"));
    }

    #[test]
    fn test_features_override_union() {
        // A community token with features_override should grant overridden features
        assert!(is_feature_available(
            "solver",
            "community",
            Some(&["solver".to_string()])
        ));
        // Without override, community shouldn't have solver
        assert!(!is_feature_available("solver", "community", None));
    }

    #[test]
    fn test_unknown_domain_defaults_pro() {
        // Unknown domain should require Pro
        assert_eq!(required_edition("quantum"), Edition::Pro);
        assert_eq!(required_edition("some_future_feature"), Edition::Pro);
    }

    #[test]
    fn test_oss_allows_llm() {
        // OSS features should always be granted regardless of token
        let result = check_entitlement_detailed("llm_cloud", None);
        assert_eq!(result, EntitlementResult::Granted);
    }

    #[test]
    fn test_oss_features_no_token_needed() {
        // All community features should work without any token
        let _guard = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        // SAFETY: test-only env manipulation
        unsafe { std::env::remove_var("NXUSKIT_LICENSE_TOKEN") };
        for domain in &[
            "llm_cloud",
            "llm_local",
            "clips",
            "bayesian",
            "auth",
            "tool_calling",
        ] {
            let result = check_entitlement_detailed(domain, None);
            assert_eq!(
                result,
                EntitlementResult::Granted,
                "OSS domain {domain} should be granted"
            );
        }
    }

    #[test]
    fn test_pro_feature_denied_without_token() {
        // Pro features should be denied without any token
        let _guard = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        // SAFETY: test-only env manipulation
        unsafe { std::env::remove_var("NXUSKIT_LICENSE_TOKEN") };
        let result = check_entitlement_detailed("zen", None);
        // On OSS binary: FeatureUnavailable (no code); on Pro binary: FeatureUnavailable (no token)
        assert_ne!(
            result,
            EntitlementResult::Granted,
            "Pro domain 'zen' should NOT be granted without a token"
        );
    }

    #[test]
    fn test_days_to_date() {
        // 2024-01-01 is day 19723 from epoch
        let (y, m, d) = days_to_date(19723);
        assert_eq!((y, m, d), (2024, 1, 1));
    }

    #[test]
    fn test_chrono_now_iso_format() {
        let now = chrono_now_iso();
        assert!(now.len() >= 20, "ISO string too short: {now}");
        assert!(now.ends_with('Z'), "Should end with Z: {now}");
        assert!(now.contains('T'), "Should contain T: {now}");
    }

    #[test]
    fn test_entitlement_result_repr() {
        // Verify C ABI repr values are stable
        assert_eq!(EntitlementResult::Granted as i32, 0);
        assert_eq!(EntitlementResult::FeatureUnavailable as i32, 1);
        assert_eq!(EntitlementResult::LicenseExpired as i32, 2);
        assert_eq!(EntitlementResult::LicenseInvalid as i32, 3);
        assert_eq!(EntitlementResult::EditionInsufficient as i32, 4);
        assert_eq!(EntitlementResult::TrialSuspended as i32, 5);
        assert_eq!(EntitlementResult::TrialIssuanceFailed as i32, 6);
        assert_eq!(EntitlementResult::VersionCeilingExceeded as i32, 7);
    }

    // ── T111: No-token Community access tests (FR-005) ──────────────
    //
    // Per sdk-integration-test-ownership-20260318.md, nxusKit owns these.
    // Verify that ALL Community-tier features return Granted when no token
    // is present (no env var, no file, no API param).

    #[test]
    fn test_no_token_community_features_all_granted() {
        // Clear env var to ensure no token source
        let _guard = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        // SAFETY: test-only env manipulation
        unsafe { std::env::remove_var("NXUSKIT_LICENSE_TOKEN") };

        let community_domains = [
            "llm_cloud",
            "llm_local",
            "clips",
            "bayesian",
            "auth",
            "tool_calling",
        ];
        for domain in &community_domains {
            let result = check_entitlement_detailed(domain, None);
            assert_eq!(
                result,
                EntitlementResult::Granted,
                "Community domain '{domain}' must be Granted without any token"
            );
        }
    }

    #[test]
    fn test_no_token_community_entitlement_info_reports_oss() {
        // Verify entitlement_info JSON reports correct edition/status with no token.
        // Must sandbox HOME to avoid picking up ~/.nxuskit/license.token from
        // the developer's real home directory.
        let _guard = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let sandbox = tempfile::tempdir().expect("create sandbox HOME");
        let orig_home = std::env::var("HOME").ok();
        let orig_token = std::env::var("NXUSKIT_LICENSE_TOKEN").ok();
        #[cfg(windows)]
        let orig_userprofile = std::env::var("USERPROFILE").ok();

        // SAFETY: test-only env manipulation
        unsafe {
            std::env::set_var("HOME", sandbox.path().as_os_str());
            #[cfg(windows)]
            std::env::set_var("USERPROFILE", sandbox.path().as_os_str());
            std::env::remove_var("NXUSKIT_LICENSE_TOKEN");
        }

        let info = entitlement_info(None);

        // Restore env
        unsafe {
            match orig_home {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
            #[cfg(windows)]
            match orig_userprofile {
                Some(v) => std::env::set_var("USERPROFILE", v),
                None => std::env::remove_var("USERPROFILE"),
            }
            match orig_token {
                Some(v) => std::env::set_var("NXUSKIT_LICENSE_TOKEN", v),
                None => std::env::remove_var("NXUSKIT_LICENSE_TOKEN"),
            }
        }

        assert_eq!(info["status"], "no_token");
        // Edition reflects compile-time NXUSKIT_EDITION (oss or pro)
        let expected_edition = env!("NXUSKIT_EDITION");
        assert_eq!(info["edition"], expected_edition);
        // effective_edition matches compile-time edition (no token to change it)
        assert_eq!(info["effective_edition"], expected_edition);
        // Community features should be listed
        let features = info["features"]
            .as_array()
            .expect("features should be array");
        let feature_names: Vec<&str> = features.iter().filter_map(|v| v.as_str()).collect();
        for domain in &[
            "llm_cloud",
            "llm_local",
            "clips",
            "bayesian",
            "auth",
            "tool_calling",
        ] {
            assert!(
                feature_names.contains(domain),
                "Community feature '{domain}' should be in features list"
            );
        }
        // On OSS builds, Pro features should NOT be in the features list
        // without a token. On Pro builds, the catalog includes Pro features
        // at compile time regardless of token presence.
        if expected_edition == "oss" {
            for domain in &["zen", "solver", "mcp", "plugin_loading"] {
                assert!(
                    !feature_names.contains(domain),
                    "Pro feature '{domain}' should NOT be in features list on OSS build without token"
                );
            }
        } else {
            // Pro build: Pro features ARE expected even without a token
            for domain in &["zen", "solver"] {
                assert!(
                    feature_names.contains(domain),
                    "Pro feature '{domain}' should be in features list on Pro build"
                );
            }
        }
    }

    // ── T112: No-token Pro FeatureUnavailable tests (FR-005) ────────
    //
    // Per sdk-integration-test-ownership-20260318.md, nxusKit owns these.
    // When no token is present and trial issuance is unavailable, Pro-tier
    // features must return a denial (not silent success).
    //
    // On OSS build (default): FeatureUnavailable (binary lacks Pro code).
    // On Pro build (NXUSKIT_EDITION=pro): TrialIssuanceFailed when mock
    // server is unreachable, or FeatureUnavailable when trial already used.
    //
    // NOTE: The TrialIssuanceFailed path requires NXUSKIT_EDITION=pro at
    // compile time. Run with:
    //   NXUSKIT_EDITION=pro cargo test -p nxuskit-core -- no_token_pro

    #[test]
    fn test_no_token_pro_features_denied() {
        // Clear env var to ensure no token source
        let _guard = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        // SAFETY: test-only env manipulation
        unsafe { std::env::remove_var("NXUSKIT_LICENSE_TOKEN") };

        let pro_domains = ["zen", "solver", "mcp", "plugin_loading"];
        for domain in &pro_domains {
            let result = check_entitlement_detailed(domain, None);
            assert_ne!(
                result,
                EntitlementResult::Granted,
                "Pro domain '{domain}' must NOT be Granted without a token"
            );
            // On OSS build: FeatureUnavailable (binary check at line 164-167)
            // On Pro build: depends on trial issuance outcome
            if current_edition() < Edition::Pro {
                assert_eq!(
                    result,
                    EntitlementResult::FeatureUnavailable,
                    "On OSS build, Pro domain '{domain}' should be FeatureUnavailable"
                );
            }
        }
    }

    #[test]
    fn test_no_token_pro_check_entitlement_returns_false() {
        // Verify the bool wrapper also denies Pro features
        let _guard = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        // SAFETY: test-only env manipulation
        unsafe { std::env::remove_var("NXUSKIT_LICENSE_TOKEN") };

        assert!(
            !check_entitlement("zen", None),
            "check_entitlement('zen') must return false without token"
        );
        assert!(
            !check_entitlement("solver", None),
            "check_entitlement('solver') must return false without token"
        );
    }

    #[test]
    fn test_no_token_unknown_domain_denied() {
        // Unknown domains default to Pro edition requirement (conservative)
        let _guard = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        // SAFETY: test-only env manipulation
        unsafe { std::env::remove_var("NXUSKIT_LICENSE_TOKEN") };

        let result = check_entitlement_detailed("some_future_feature", None);
        assert_ne!(
            result,
            EntitlementResult::Granted,
            "Unknown domains should default to Pro requirement and be denied without token"
        );
    }

    // ── T055a: JSON shape contract test for entitlement_info ──────

    #[test]
    fn test_entitlement_info_json_shape() {
        // Verify the JSON response from entitlement_info() matches the
        // contract in contracts/entitlement-api.md.
        // Uses no-token path (community defaults) to avoid env sensitivity.
        let _guard = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let sandbox = tempfile::tempdir().expect("create sandbox HOME");
        let orig_home = std::env::var("HOME").ok();
        let orig_token = std::env::var("NXUSKIT_LICENSE_TOKEN").ok();
        #[cfg(windows)]
        let orig_userprofile = std::env::var("USERPROFILE").ok();

        // SAFETY: test-only env manipulation
        unsafe {
            std::env::set_var("HOME", sandbox.path().as_os_str());
            #[cfg(windows)]
            std::env::set_var("USERPROFILE", sandbox.path().as_os_str());
            std::env::remove_var("NXUSKIT_LICENSE_TOKEN");
        }

        let info = entitlement_info(None);

        // Restore env
        unsafe {
            match orig_home {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
            #[cfg(windows)]
            match orig_userprofile {
                Some(v) => std::env::set_var("USERPROFILE", v),
                None => std::env::remove_var("USERPROFILE"),
            }
            match orig_token {
                Some(v) => std::env::set_var("NXUSKIT_LICENSE_TOKEN", v),
                None => std::env::remove_var("NXUSKIT_LICENSE_TOKEN"),
            }
        }

        // edition: must be a string
        assert!(
            info["edition"].is_string(),
            "edition must be a string, got: {:?}",
            info["edition"]
        );

        // effective_edition: must be a string
        assert!(
            info["effective_edition"].is_string(),
            "effective_edition must be a string, got: {:?}",
            info["effective_edition"]
        );

        // features: must be an array
        assert!(
            info["features"].is_array(),
            "features must be an array, got: {:?}",
            info["features"]
        );

        // effective_limits: must be an object with all 7 required fields
        let limits = &info["effective_limits"];
        assert!(
            limits.is_object(),
            "effective_limits must be an object, got: {:?}",
            limits
        );
        for field in &[
            "max_sessions",
            "max_cached_rulebases",
            "max_rules_per_session",
            "max_facts_per_session",
            "max_bayesian_nodes",
            "max_solver_constraints",
            "seats",
        ] {
            assert!(
                limits.get(field).is_some(),
                "effective_limits must contain '{field}', keys present: {:?}",
                limits.as_object().map(|o| o.keys().collect::<Vec<_>>())
            );
        }

        // status: must be a string
        assert!(
            info["status"].is_string(),
            "status must be a string, got: {:?}",
            info["status"]
        );
    }

    // ── T022a: Legacy license key fallback preserved after ES256 migration ──
}
