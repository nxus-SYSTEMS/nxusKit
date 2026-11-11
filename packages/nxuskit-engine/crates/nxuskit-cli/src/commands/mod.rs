//! Command modules for Level 1 CLI commands.

pub mod artifact;
pub mod bn;
pub mod branch;
pub mod call;
pub mod clips;
pub mod contract_v2;
pub mod doctor;
pub mod judge;
#[allow(dead_code)]
pub mod models;
pub mod orchestration;
pub mod packet;
pub mod pipeline;
pub mod pipeline_hash;
pub mod pipeline_normalize;
pub mod pipeline_preflight;
/// Feature 106 (v1.1.0 RC) — contract-native pipeline run driver.
pub mod pipeline_run_v11;
#[allow(dead_code)]
pub mod provider;
#[path = "solver_stub.rs"]
pub mod solver;
pub mod tool_adapters;
pub mod tool_loop;
#[path = "zen_stub.rs"]
pub mod zen;
