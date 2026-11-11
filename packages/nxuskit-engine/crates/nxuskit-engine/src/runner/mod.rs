//! Feature 106 (v1.1.0 RC) — internal contract-native pipeline runner
//! seam.
//!
//! ## Boundary
//!
//! This module is `pub` only inside `nxuskit-engine`. It is intentionally
//! **NOT** re-exported from the public `nxuskit` wrapper crate (FR-047,
//! FR-048). CE / public-API consumers see only:
//!
//! - public v1.1 DTOs on `nxuskit::contract::v1_1::*`, and
//! - the `nxuskit-cli` input/output contract.
//!
//! The trait surface here only accepts those public DTOs plus this
//! crate's own `PipelineRunOptions` value object. No engine-internal
//! routing or control types, no routing internals, and no Zenoh route
//! identifiers.
//!
//! ## RC implementation
//!
//! The RC path is [`DirectProviderRunner`](crate::runner::direct::DirectProviderRunner): it dispatches
//! through this crate's existing provider creation (loopback today;
//! other providers as their integrations land), which is enough to
//! satisfy US2/US3 while keeping the seam in place so a future
//! engine-internal implementation can swap in without changing the public
//! CLI input/output contracts.
//!
//! ## Why the seam matters
//!
//! Per FR-047 / FR-048 the public CLI contract MUST be invariant under
//! runner-implementation swaps. Centralising execution behind this
//! trait keeps `nxuskit-cli` honest: the CLI cannot accidentally start
//! consuming Pro-internal types because it can only call methods
//! defined here.

use async_trait::async_trait;

use nxuskit::contract::v1_1::{PipelineDefinition, PipelineRunReport, ReportPreset, ReportSection};

use crate::error::NxuskitError;

pub mod advisory;
pub mod direct;
pub mod events;
pub mod fanout;

pub use advisory::assess_pipeline_resources;
pub use direct::DirectProviderRunner;
pub use events::{JsonlEvent, JsonlEventSink, NullEventSink, VecEventSink};

/// Options passed to [`PipelineCapabilityRunner::run`]. Public-DTO-only
/// value object so the trait surface stays free of internal types.
#[derive(Clone)]
pub struct PipelineRunOptions {
    /// `--report-preset` value. Default: `Standard`.
    pub report_preset: ReportPreset,
    /// `--report-include` items.
    pub report_include: Vec<ReportSection>,
    /// `--report-omit` items.
    pub report_omit: Vec<ReportSection>,
    /// Max in-flight fan-out operations per group. Defaults to 4 when
    /// `None` (R-1 / contracts/fanout-semantics.md FR-018).
    pub fan_out_concurrency: Option<u32>,
    /// When `Some`, the runner streams JSONL events into the sink for
    /// the duration of the run.
    pub event_sink: Option<std::sync::Arc<dyn JsonlEventSink>>,
    /// When `true`, the run report MAY include a preflight resource
    /// advisory. v1.1.0 RC US5 wiring; left in place even though the
    /// US5 implementation lands later.
    pub preflight: bool,
}

impl Default for PipelineRunOptions {
    fn default() -> Self {
        Self {
            report_preset: ReportPreset::Standard,
            report_include: Vec::new(),
            report_omit: Vec::new(),
            fan_out_concurrency: None,
            event_sink: None,
            preflight: false,
        }
    }
}

impl std::fmt::Debug for PipelineRunOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PipelineRunOptions")
            .field("report_preset", &self.report_preset)
            .field("report_include", &self.report_include)
            .field("report_omit", &self.report_omit)
            .field("fan_out_concurrency", &self.fan_out_concurrency)
            .field("event_sink", &self.event_sink.as_ref().map(|_| "<sink>"))
            .field("preflight", &self.preflight)
            .finish()
    }
}

/// Narrow internal seam between the public CLI surface and the actual
/// pipeline execution implementation. All inputs and outputs are public
/// v1.1 DTOs.
///
/// FR-047 / FR-048: the v1.1.0 RC implementation backs this with
/// [`DirectProviderRunner`]; a future Pro / internal implementation may
/// back it with engine-internal dispatch without changing the public CLI
/// input/output contracts.
#[async_trait]
pub trait PipelineCapabilityRunner: Send + Sync {
    /// Execute a public v1.1 `PipelineDefinition` and return a public
    /// v1.1 `PipelineRunReport`. Errors are returned as
    /// `NxuskitError`, NOT as engine-internal request/result types.
    async fn run(
        &self,
        def: &PipelineDefinition,
        opts: &PipelineRunOptions,
    ) -> Result<PipelineRunReport, NxuskitError>;
}
