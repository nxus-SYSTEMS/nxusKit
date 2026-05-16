//! CE-safe ZEN provider surface.
//!
//! The implementation lives behind an internal Pro feature. This module
//! preserves public types used by CLI/API wrappers without linking or shipping
//! the decision-table engine in community builds.

use serde::{Deserialize, Serialize};

use crate::error::{NxuskitError, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ZenValidationProblemKind {
    Schema,
    MissingNodes,
    InvalidNode,
    InvalidEdges,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ZenValidationProblem {
    pub kind: ZenValidationProblemKind,
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ZenValidationReport {
    pub valid: bool,
    pub node_count: u32,
    pub decision_table_count: u32,
    pub rule_count: u32,
    pub problems: Vec<ZenValidationProblem>,
}

pub async fn evaluate(_model_json: &str, _input_json: &str) -> Result<serde_json::Value> {
    Err(NxuskitError::FeatureUnavailable {
        feature: "zen".to_string(),
    })
}

pub fn validate(_model_json: &str) -> Result<ZenValidationReport> {
    Err(NxuskitError::FeatureUnavailable {
        feature: "zen".to_string(),
    })
}
