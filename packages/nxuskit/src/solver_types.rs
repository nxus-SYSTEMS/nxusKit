//! Public CE solver stream placeholder types.
//!
//! Pro solver domain types are not shipped in public CE source or release bundles.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SolverStreamChunk {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SolverStreamResult {
    #[serde(default)]
    pub chunks: Vec<SolverStreamChunk>,
}
