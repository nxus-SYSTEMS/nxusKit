//! Validation helpers for the v1.1 contract surface.
//!
//! US1 ships the schema-level pipeline validator on [`PipelineDefinition`]
//! itself (see `pipeline::PipelineDefinition::validate`). Cross-checking
//! against the engine adapter (capability registry, readiness snapshot) is
//! exposed here as a thin helper that consumers can compose. The helper
//! does NOT depend on engine-internal types — it accepts pre-built public
//! DTOs only.

use std::collections::BTreeSet;

use super::pipeline::PipelineDefinition;
use super::readiness::ReadinessSnapshot;
use super::validation::PipelineValidationOutcome;

/// Cross-check a pipeline against a public readiness snapshot.
///
/// Runs the structural validator first (so structural / boundary / ambiguity
/// / reference-binding failures surface with their typed outcomes), then —
/// only on a `Ready` structural result — walks each declared stage and
/// emits `CapabilityUnavailable { stage_name, capability_id }` for the
/// first stage whose `capability_ref` is not in `snapshot.capabilities`.
/// Rule order (locked, per `research.md` §R-006):
/// structural → boundary → ambiguity → reference binding → capability.
pub fn validate_with_readiness(
    pipeline: &PipelineDefinition,
    snapshot: &ReadinessSnapshot,
) -> PipelineValidationOutcome {
    let structural = pipeline.validate();
    if !matches!(structural, PipelineValidationOutcome::Ready) {
        return structural;
    }
    if snapshot.capabilities.is_empty() {
        return PipelineValidationOutcome::ReadinessUnknown {
            reason: "readiness snapshot has no capability entries".to_owned(),
        };
    }
    let available: BTreeSet<&str> = snapshot
        .capabilities
        .iter()
        .map(|entry| entry.capability_id.as_str())
        .collect();
    for stage in &pipeline.stages {
        // Empty `capability_ref` is allowed (some declaration-only stages
        // do not pin a capability id at the contract floor); skip those.
        let cap = stage.capability_ref.as_str();
        if cap.is_empty() {
            continue;
        }
        if !available.contains(cap) {
            return PipelineValidationOutcome::CapabilityUnavailable {
                stage_name: stage.name.clone(),
                capability_id: stage.capability_ref.clone(),
            };
        }
    }
    PipelineValidationOutcome::Ready
}
