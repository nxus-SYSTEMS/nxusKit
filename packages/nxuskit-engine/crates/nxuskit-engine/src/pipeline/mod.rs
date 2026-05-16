//! Pipeline definition and validation module.
//!
//! This module provides infrastructure for defining multi-stage LLM workflows
//! as portable JSON/YAML configuration files compatible with Peeler.
//!
//! # Features
//!
//! - **Pipeline Definitions**: Define multi-stage workflows with LLM and CLIPS stages
//! - **DAG Validation**: Ensure pipeline stages form a valid directed acyclic graph
//! - **Format Support**: Load pipelines from JSON or YAML files
//! - **Peeler Compatibility**: Convert between nxusKit and Peeler formats
//!
//! # Example
//!
//! ```no_run
//! use nxuskit_engine::pipeline::{load_pipeline, PipelineDefinition};
//!
//! // Load a pipeline from a JSON file
//! let pipeline = load_pipeline("my-pipeline.json").unwrap();
//!
//! // Validate the pipeline DAG
//! pipeline.validate().unwrap();
//!
//! println!("Loaded {} stages", pipeline.stages.len());
//! ```

mod converter;
mod definition;
mod loader;
mod validation;

pub use converter::*;
pub use definition::*;
pub use loader::*;
pub use validation::*;
