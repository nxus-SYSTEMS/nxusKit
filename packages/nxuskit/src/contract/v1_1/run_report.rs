//! Feature 106: public v1.1 RC `PipelineRunReport` DTO.
//!
//! Default `nxuskit-cli pipeline run --format json` output for inputs
//! whose `contract_version == "1.1"`. Backward-compat: legacy pipeline
//! inputs continue to route through the existing legacy runner with
//! the legacy output shape (FR-007).

use serde::{Deserialize, Serialize};

use super::ArtifactCanonicalHash;
use super::advisory::ResourceAdvisory;
use super::envelope::{ArtifactHistoryEnvelope, PipelineDefinitionRef};
use super::outcome::{OperationOutcome, StageOutcome};

/// Default `pipeline run --format json` output for v1.1 inputs.
///
/// Fields are emitted or omitted according to the active
/// `ReportPreset` and `--report-include` / `--report-omit`
/// selections (R5 decision; see the public pipeline run-report contract
/// documentation).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipelineRunReport {
    pub contract_version: String,
    /// Echo of the canonical pipeline reference.
    pub pipeline_ref: PipelineDefinitionRef,
    /// Canonical hash of the input `PipelineDefinition`. Matches
    /// `nxuskit-cli pipeline hash` output for the same input.
    pub pipeline_canonical_hash: ArtifactCanonicalHash,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readiness_summary: Option<ReadinessSummaryDigest>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage_results: Option<Vec<StageRunResult>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_outputs: Option<FinalOutputs>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_history_envelope: Option<ArtifactHistoryEnvelope>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_advisory: Option<ResourceAdvisory>,
}

/// Minimal public-safe readiness digest embedded in the run report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadinessSummaryDigest {
    pub captured_at: String,
    pub providers_ready: u32,
}

/// Per-stage run result emitted by the contract-native runner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageRunResult {
    pub stage_name: String,
    #[serde(flatten)]
    pub outcome: StageOutcome,
    pub elapsed_ms: u64,
    /// Optional small public-safe text summary of the stage's output.
    /// Bulk payloads are forbidden (FR-027).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Present when this stage expanded into a fan-out group.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fan_out: Option<FanOutGroupResult>,
}

/// Per-fan-out-group result: stable, pair-indexed operation outcomes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FanOutGroupResult {
    /// Stable group id: by default the parent stage name.
    pub group_id: String,
    pub operations: Vec<FanOutOperationResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FanOutOperationResult {
    /// Stable operation id `<stage_id>::op-<pair_index>` (FR-019).
    pub operation_id: String,
    pub pair_index: u32,
    pub provider_ref: String,
    pub model_ref: String,
    #[serde(flatten)]
    pub outcome: OperationOutcome,
    pub elapsed_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_summary: Option<String>,
}

/// Final outputs surfaced for human / downstream consumption.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FinalOutputs {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stage_outputs: Vec<StageFinalOutput>,
}

/// One final output entry — public-safe text only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageFinalOutput {
    pub stage_name: String,
    pub output_name: String,
    /// Small public-safe text (e.g., loopback LLM completion). Bulk
    /// payloads forbidden.
    pub text: String,
}

/// R5 decision: preset for `pipeline run --report-preset`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportPreset {
    /// Minimal: final outputs + readiness digest only.
    Summary,
    /// Default: readiness + stage details + final outputs + envelope.
    Standard,
    /// All sections, including embedded envelope.
    Provenance,
}

impl Default for ReportPreset {
    fn default() -> Self {
        Self::Standard
    }
}

impl ReportPreset {
    /// Parse from a CLI string. Unknown values fall back to `Standard`.
    pub fn parse(s: &str) -> Self {
        match s {
            "summary" => Self::Summary,
            "standard" | "" => Self::Standard,
            "provenance" => Self::Provenance,
            _ => Self::Standard,
        }
    }
}

/// R5 decision: section identifiers for `--report-include` /
/// `--report-omit`. The preset is applied first; include/omit refines
/// it; conflicts resolve in favor of omit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportSection {
    ReadinessSummary,
    StageDetails,
    FinalOutputs,
    ArtifactHistory,
    ResourceAdvisory,
}

impl ReportSection {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "readiness_summary" => Some(Self::ReadinessSummary),
            "stage_details" => Some(Self::StageDetails),
            "final_outputs" => Some(Self::FinalOutputs),
            "artifact_history" => Some(Self::ArtifactHistory),
            "resource_advisory" => Some(Self::ResourceAdvisory),
            _ => None,
        }
    }
}

/// Compute the effective enabled-section set from a preset + include /
/// omit lists.
pub fn effective_sections(
    preset: ReportPreset,
    include: &[ReportSection],
    omit: &[ReportSection],
) -> Vec<ReportSection> {
    let mut active: Vec<ReportSection> = match preset {
        ReportPreset::Summary => vec![ReportSection::FinalOutputs, ReportSection::ReadinessSummary],
        ReportPreset::Standard => vec![
            ReportSection::ReadinessSummary,
            ReportSection::StageDetails,
            ReportSection::FinalOutputs,
            ReportSection::ArtifactHistory,
        ],
        ReportPreset::Provenance => vec![
            ReportSection::ReadinessSummary,
            ReportSection::StageDetails,
            ReportSection::FinalOutputs,
            ReportSection::ArtifactHistory,
            ReportSection::ResourceAdvisory,
        ],
    };
    for s in include {
        if !active.contains(s) {
            active.push(*s);
        }
    }
    active.retain(|s| !omit.contains(s));
    active
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_preset_default_is_standard() {
        assert_eq!(ReportPreset::default(), ReportPreset::Standard);
    }

    #[test]
    fn report_preset_parse_recognises_documented_values() {
        assert_eq!(ReportPreset::parse("summary"), ReportPreset::Summary);
        assert_eq!(ReportPreset::parse("standard"), ReportPreset::Standard);
        assert_eq!(ReportPreset::parse("provenance"), ReportPreset::Provenance);
        assert_eq!(ReportPreset::parse(""), ReportPreset::Standard);
        assert_eq!(ReportPreset::parse("garbage"), ReportPreset::Standard);
    }

    #[test]
    fn report_section_parse_round_trips() {
        for (text, expected) in [
            ("readiness_summary", ReportSection::ReadinessSummary),
            ("stage_details", ReportSection::StageDetails),
            ("final_outputs", ReportSection::FinalOutputs),
            ("artifact_history", ReportSection::ArtifactHistory),
            ("resource_advisory", ReportSection::ResourceAdvisory),
        ] {
            assert_eq!(ReportSection::parse(text), Some(expected));
        }
        assert_eq!(ReportSection::parse("nope"), None);
    }

    #[test]
    fn effective_sections_standard_excludes_resource_advisory_by_default() {
        let sections = effective_sections(ReportPreset::Standard, &[], &[]);
        assert!(sections.contains(&ReportSection::ReadinessSummary));
        assert!(sections.contains(&ReportSection::StageDetails));
        assert!(sections.contains(&ReportSection::FinalOutputs));
        assert!(sections.contains(&ReportSection::ArtifactHistory));
        assert!(!sections.contains(&ReportSection::ResourceAdvisory));
    }

    #[test]
    fn effective_sections_omit_overrides_include() {
        let sections = effective_sections(
            ReportPreset::Standard,
            &[ReportSection::ResourceAdvisory],
            &[ReportSection::ArtifactHistory],
        );
        assert!(sections.contains(&ReportSection::ResourceAdvisory));
        assert!(!sections.contains(&ReportSection::ArtifactHistory));
    }

    #[test]
    fn effective_sections_summary_is_minimal() {
        let sections = effective_sections(ReportPreset::Summary, &[], &[]);
        assert!(sections.contains(&ReportSection::FinalOutputs));
        assert!(sections.contains(&ReportSection::ReadinessSummary));
        assert!(!sections.contains(&ReportSection::StageDetails));
        assert!(!sections.contains(&ReportSection::ArtifactHistory));
    }
}
