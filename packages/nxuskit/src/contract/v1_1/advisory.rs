//! Public v1.1 resource-advisory DTOs.
//!
//! These types are intentionally data-only. Advisory computation lives behind
//! the CLI/engine runner boundary; consumers see only this public-safe
//! contract shape.

use serde::{Deserialize, Serialize};

use super::CONTRACT_VERSION;

/// Public-safe advisory for planned local resource pressure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceAdvisory {
    pub contract_version: String,
    pub severity: ResourceAdvisorySeverity,
    #[serde(default)]
    pub reasons: Vec<ResourceAdvisoryReason>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommended_max_concurrency: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub model_size_estimates: Vec<ModelSizeEstimate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_estimate_mb: Option<u64>,
    pub probe_status: ResourceProbeStatus,
}

impl ResourceAdvisory {
    /// Stable fallback when the SDK cannot probe local resources.
    pub fn probe_unavailable() -> Self {
        Self {
            contract_version: CONTRACT_VERSION.to_owned(),
            severity: ResourceAdvisorySeverity::Caution,
            reasons: vec![ResourceAdvisoryReason {
                code: ResourceAdvisoryReasonCode::ResourceProbeUnavailable,
                detail: "resource probe unavailable".to_owned(),
                operations: Vec::new(),
            }],
            recommended_max_concurrency: None,
            model_size_estimates: Vec::new(),
            memory_estimate_mb: None,
            probe_status: ResourceProbeStatus::Unavailable,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceAdvisorySeverity {
    Ok,
    Caution,
    HighRisk,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceProbeStatus {
    Available,
    Unavailable,
    Partial,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceAdvisoryReason {
    pub code: ResourceAdvisoryReasonCode,
    pub detail: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub operations: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceAdvisoryReasonCode {
    InsufficientAvailableMemory,
    UnknownModelSize,
    ProviderAlreadyBusy,
    MultipleLargeModels,
    ResourceProbeUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelSizeEstimate {
    pub provider_ref: String,
    pub model_ref: String,
    pub size_class: ModelSizeClass,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_bytes: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelSizeClass {
    Small,
    Medium,
    Large,
    Unknown,
}
