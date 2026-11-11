//! Surface classification for v2 contract alpha artifacts.

use serde::{Deserialize, Serialize};

/// Public/private contract classification for source and schema artifacts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceClassification {
    Stable,
    Preview,
    Internal,
    ProPrivate,
    EnterprisePrivate,
}

/// Tier posture exposed in contract metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceTier {
    Community,
    Pro,
    Enterprise,
}
