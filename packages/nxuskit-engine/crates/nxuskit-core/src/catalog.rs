//! Product catalog — build-time-embedded edition, feature, and limit definitions.
//!
//! This module provides catalog-driven entitlement resolution sourced from
//! `DevOps/sharedData/product-catalog-v1.yaml`. The YAML is parsed at
//! build time by `build.rs` and compiled into Rust constants — zero runtime
//! parsing overhead.
//!
//! # Architecture
//!
//! This is Layer 2 of the three-layer licensing architecture:
//! - Layer 1: Token verification (`TokenVerifier` trait in `license_types.rs`)
//! - **Layer 2: Entitlement resolution (this module)**
//! - Layer 3: Product-specific interpretation (`entitlement.rs`)

use std::collections::{HashMap, HashSet};

use crate::license_types::{TokenType, ValidatedClaims};

/// Numerical limits for a product edition.
///
/// `None` values indicate unlimited (no enforcement).
/// Values are sourced from the product catalog at build time
/// and can be overridden per-token via `limits_override` claims.
///
/// # Examples
///
/// ```
/// use nxuskit_core::catalog::EditionLimits;
///
/// // Default limits are all None (unlimited)
/// let limits = EditionLimits::default();
/// assert_eq!(limits.max_sessions, None);
///
/// // Set specific limits for an edition
/// let community = EditionLimits {
///     max_sessions: Some(16),
///     max_cached_rulebases: Some(8),
///     ..Default::default()
/// };
/// assert_eq!(community.max_sessions, Some(16));
/// assert_eq!(community.seats, None); // unlimited
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EditionLimits {
    pub max_sessions: Option<u64>,
    pub max_cached_rulebases: Option<u64>,
    pub max_rules_per_session: Option<u64>,
    pub max_facts_per_session: Option<u64>,
    pub max_bayesian_nodes: Option<u64>,
    pub max_solver_constraints: Option<u64>,
    pub seats: Option<u64>,
}

/// Edition tier within a product.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Edition {
    Community = 0,
    Pro = 1,
    Enterprise = 2,
}

impl Edition {
    pub fn parse(s: &str) -> Self {
        match s {
            "community" | "oss" => Edition::Community,
            "pro" => Edition::Pro,
            "enterprise" => Edition::Enterprise,
            _ => Edition::Pro, // conservative default
        }
    }
}

impl std::fmt::Display for Edition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Edition::Community => write!(f, "community"),
            Edition::Pro => write!(f, "pro"),
            Edition::Enterprise => write!(f, "enterprise"),
        }
    }
}

/// Token source for audit/logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenSourceKind {
    EnvironmentVariable,
    TokenFile,
    ApiParameter,
    None,
}

/// The resolved result of validating a token against the catalog.
///
/// Contains the effective edition, the full feature set (catalog defaults
/// merged with token overrides), and the effective limits.
#[derive(Debug, Clone)]
pub struct EntitlementGrant {
    pub edition: Edition,
    pub features: HashSet<String>,
    pub limits: EditionLimits,
    pub token_type: TokenType,
    pub source: TokenSourceKind,
}

// Build-time generated catalog data
include!(concat!(env!("OUT_DIR"), "/catalog_generated.rs"));

/// Merge token-level limit overrides with catalog defaults.
///
/// Token values take precedence for recognized keys.
/// The string `"unlimited"` maps to `None` (no enforcement).
/// Unknown keys are silently ignored.
///
/// # Examples
///
/// ```
/// use std::collections::HashMap;
/// use nxuskit_core::catalog::{EditionLimits, merge_limits_override};
///
/// let base = EditionLimits {
///     max_sessions: Some(64),
///     seats: Some(3),
///     ..Default::default()
/// };
///
/// let mut overrides = HashMap::new();
/// overrides.insert("max_sessions".to_string(), serde_json::json!(512));
/// let merged = merge_limits_override(&base, &Some(overrides));
///
/// assert_eq!(merged.max_sessions, Some(512)); // overridden
/// assert_eq!(merged.seats, Some(3));           // unchanged
/// ```
pub fn merge_limits_override(
    base: &EditionLimits,
    overrides: &Option<HashMap<String, serde_json::Value>>,
) -> EditionLimits {
    let mut result = base.clone();
    let Some(overrides) = overrides else {
        return result;
    };

    for (key, value) in overrides {
        let parsed = match value {
            serde_json::Value::Number(n) => n.as_u64().map(Some),
            serde_json::Value::String(s) if s == "unlimited" => Some(None),
            _ => continue,
        };
        let Some(limit_val) = parsed else {
            continue;
        };
        match key.as_str() {
            "max_sessions" => result.max_sessions = limit_val,
            "max_cached_rulebases" => result.max_cached_rulebases = limit_val,
            "max_rules_per_session" => result.max_rules_per_session = limit_val,
            "max_facts_per_session" => result.max_facts_per_session = limit_val,
            "max_bayesian_nodes" => result.max_bayesian_nodes = limit_val,
            "max_solver_constraints" => result.max_solver_constraints = limit_val,
            "seats" => result.seats = limit_val,
            _ => {} // unknown keys silently ignored
        }
    }
    result
}

/// Resolve entitlement from validated claims against the product catalog.
///
/// 1. Look up edition in catalog → base features + limits
/// 2. Union `features_override` from token (if present) with catalog features
/// 3. Merge `limits_override` from token (if present), token values take precedence
/// 4. Return `EntitlementGrant`
pub fn resolve_entitlement(claims: &ValidatedClaims) -> EntitlementGrant {
    let edition = Edition::parse(&claims.edition);
    let edition_str = edition.to_string();

    // Get catalog features for this edition (build-time generated)
    let catalog_features = catalog_features(&edition_str);
    let mut features: HashSet<String> = catalog_features.iter().map(|s| s.to_string()).collect();

    // Union with token-level feature overrides
    if let Some(ref overrides) = claims.features_override {
        for f in overrides {
            features.insert(f.clone());
        }
    }

    // Get catalog limits and merge with token-level overrides
    let base_limits = catalog_limits(&edition_str);
    let limits = merge_limits_override(&base_limits, &claims.limits_override);

    EntitlementGrant {
        edition,
        features,
        limits,
        token_type: claims.token_type,
        source: TokenSourceKind::None, // caller sets actual source
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edition_from_str() {
        assert_eq!(Edition::parse("community"), Edition::Community);
        assert_eq!(Edition::parse("oss"), Edition::Community);
        assert_eq!(Edition::parse("pro"), Edition::Pro);
        assert_eq!(Edition::parse("enterprise"), Edition::Enterprise);
        assert_eq!(Edition::parse("unknown"), Edition::Pro);
    }

    // ── T031-T035: Catalog Limits Tests ──────────────────────────

    #[test]
    fn test_catalog_community_limits() {
        let limits = catalog_limits("community");
        assert_eq!(limits.max_sessions, Some(16));
        assert_eq!(limits.max_cached_rulebases, Some(8));
        assert_eq!(limits.max_rules_per_session, Some(500));
        assert_eq!(limits.max_facts_per_session, Some(10000));
        assert_eq!(limits.max_bayesian_nodes, Some(50));
    }

    #[test]
    fn test_catalog_pro_limits() {
        let limits = catalog_limits("pro");
        assert_eq!(limits.max_sessions, Some(64));
        assert_eq!(limits.max_cached_rulebases, Some(32));
        assert_eq!(limits.max_solver_constraints, Some(10000));
        assert_eq!(limits.seats, Some(3));
    }

    #[test]
    fn test_catalog_enterprise_unlimited() {
        let limits = catalog_limits("enterprise");
        assert_eq!(limits.seats, None); // unlimited
        assert_eq!(limits.max_sessions, Some(256));
    }

    #[test]
    fn test_limits_override_precedence() {
        let base = catalog_limits("pro");
        let overrides = {
            let mut m = HashMap::new();
            m.insert("max_sessions".to_string(), serde_json::json!(512));
            Some(m)
        };
        let merged = merge_limits_override(&base, &overrides);
        assert_eq!(merged.max_sessions, Some(512)); // overridden
        assert_eq!(merged.max_cached_rulebases, Some(32)); // catalog default
        assert_eq!(merged.seats, Some(3)); // catalog default
    }

    #[test]
    fn test_unknown_limit_key_ignored() {
        let base = catalog_limits("pro");
        let overrides = {
            let mut m = HashMap::new();
            m.insert("max_quantum_qubits".to_string(), serde_json::json!(100));
            Some(m)
        };
        let merged = merge_limits_override(&base, &overrides);
        // All limits unchanged — unknown key ignored
        assert_eq!(merged.max_sessions, base.max_sessions);
        assert_eq!(merged.max_cached_rulebases, base.max_cached_rulebases);
    }

    #[test]
    fn test_unlimited_override_string() {
        let base = catalog_limits("pro");
        let overrides = {
            let mut m = HashMap::new();
            m.insert("seats".to_string(), serde_json::json!("unlimited"));
            Some(m)
        };
        let merged = merge_limits_override(&base, &overrides);
        assert_eq!(merged.seats, None); // "unlimited" → None
    }

    #[test]
    fn test_resolve_entitlement_basic() {
        let claims = ValidatedClaims {
            product_id: "nxuskit".to_string(),
            token_type: TokenType::Developer,
            edition: "pro".to_string(),
            iss: "nxus-licensing".to_string(),
            iat: 0,
            nbf: None,
            exp: None,
            tenant_id: None,
            machine_id: None,
            seat_index: None,
            activated: None,
            sdk_version_ceiling: None,
            customer_email: None,
            limits_override: None,
            features_override: None,
            environment: None,
        };
        let grant = resolve_entitlement(&claims);
        assert_eq!(grant.edition, Edition::Pro);
        assert!(grant.features.contains("zen"));
        assert!(grant.features.contains("clips")); // inherited from community
        assert_eq!(grant.limits.max_sessions, Some(64));
    }

    #[test]
    fn test_resolve_entitlement_with_overrides() {
        let claims = ValidatedClaims {
            product_id: "nxuskit".to_string(),
            token_type: TokenType::Developer,
            edition: "community".to_string(),
            iss: "nxus-licensing".to_string(),
            iat: 0,
            nbf: None,
            exp: None,
            tenant_id: None,
            machine_id: None,
            seat_index: None,
            activated: None,
            sdk_version_ceiling: None,
            customer_email: None,
            limits_override: Some({
                let mut m = HashMap::new();
                m.insert("max_sessions".to_string(), serde_json::json!(128));
                m
            }),
            features_override: Some(vec!["solver".to_string()]),
            environment: None,
        };
        let grant = resolve_entitlement(&claims);
        assert_eq!(grant.edition, Edition::Community);
        assert!(grant.features.contains("solver")); // from override
        assert!(grant.features.contains("clips")); // from catalog
        assert!(!grant.features.contains("zen")); // not in override or community
        assert_eq!(grant.limits.max_sessions, Some(128)); // overridden
        assert_eq!(grant.limits.max_cached_rulebases, Some(8)); // catalog default
    }

    // ── Catalog v1.1 Compatibility (T004) ─────────────────────────

    #[test]
    fn test_catalog_ignores_unknown_fields() {
        // Verify that catalog functions return correct values even though
        // the v1.1 YAML has extra fields (trial_allowed, trial_edition,
        // grace_period_days, status, saas_integrations product, etc.)
        // that our build.rs structs don't model.
        let community_features = catalog_features("community");
        assert!(
            !community_features.is_empty(),
            "Community features should be parsed from v1.1 YAML"
        );
        assert!(community_features.contains(&"llm_cloud"));

        let pro_limits = catalog_limits("pro");
        assert_eq!(pro_limits.max_sessions, Some(64));
        assert_eq!(pro_limits.seats, Some(3));

        // Unknown edition returns empty/default (not a parse error)
        let unknown = catalog_features("saas_integrations");
        assert!(unknown.is_empty(), "Non-nxuskit products should not appear");
    }
}
