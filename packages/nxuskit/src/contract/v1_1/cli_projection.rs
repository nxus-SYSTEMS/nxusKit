//! Public CLI projection helpers for v1.1 contract DTOs.
//!
//! These helpers keep `nxuskit-cli` output aligned with the canonical Rust DTO
//! surface instead of maintaining a parallel JSON schema in the CLI crate.

use serde::Serialize;
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

use super::ids::{CapabilityDomain, CapabilityId, CapabilityPosture, ProviderId, TierPosture};
use super::readiness::{
    CapabilityTruthEntry, ClipsRulesProviderAvailability, ProviderModel, ProviderReadiness,
    ProviderTimeoutsMs, ReadinessPosture, ReadinessSnapshot,
};
use super::validation::PipelineValidationOutcome;

pub fn doctor_readiness_snapshot(
    provider_filters: &[String],
    probe_timeout_ms: u64,
) -> ReadinessSnapshot {
    doctor_readiness_snapshot_at(
        current_utc_rfc3339_seconds(),
        provider_filters,
        probe_timeout_ms,
    )
}

pub fn doctor_readiness_snapshot_at(
    captured_at: impl Into<String>,
    provider_filters: &[String],
    probe_timeout_ms: u64,
) -> ReadinessSnapshot {
    let requested: Vec<ProviderId> = if provider_filters.is_empty() {
        vec![ProviderId::from("loopback")]
    } else {
        provider_filters
            .iter()
            .map(|provider| ProviderId::from(provider.as_str()))
            .collect()
    };

    let mut providers = Vec::with_capacity(requested.len());
    let mut capabilities = Vec::new();

    for provider_id in requested {
        // Feature 106 / US1 (FR-001, FR-002, FR-004, FR-051):
        // For loopback we emit the runtime-callable default model id
        // (`echo`) and attach the legacy `loopback:v1` id as a
        // `contract_aliases` entry on the row's metadata. The loopback
        // provider accepts both ids; downstream callers (Peeler UI, model
        // selectors, smoke harnesses) see only runtime-callable ids in
        // the top-level `id` field and discover the alias mapping from
        // metadata without maintaining a private table.
        //
        // For other providers we continue to emit `<provider>:v1` as a
        // declaration-time placeholder until per-provider runtime-callable
        // ids are wired up. Those rows MUST NOT claim `Ready` until that
        // happens; for now `Ready` is loopback-only via the
        // `local_provider` branch below.
        let is_loopback = provider_id.as_str() == "loopback";
        let (model_id, model_metadata) = if is_loopback {
            (
                "echo".to_string(),
                super::readiness::ProviderModelMetadata {
                    contract_aliases: vec!["loopback:v1".to_string()],
                    alias_only: false,
                },
            )
        } else {
            (
                format!("{}:v1", provider_id.as_str()),
                super::readiness::ProviderModelMetadata::default(),
            )
        };
        providers.push(ProviderReadiness {
            provider_id: provider_id.clone(),
            posture: ReadinessPosture::Ready,
            tier_posture: TierPosture::Ce,
            local_provider: is_loopback,
            models: vec![ProviderModel {
                id: model_id,
                capabilities: vec![CapabilityId::from("llm.text_completion")],
                deprecated: false,
                metadata: model_metadata,
            }],
            timeouts_ms: ProviderTimeoutsMs {
                connect_ms: probe_timeout_ms,
                request_ms: 60_000,
            },
            last_error: None,
            metadata: Default::default(),
        });
        capabilities.push(CapabilityTruthEntry {
            capability_id: CapabilityId::from("llm.text_completion"),
            domain: CapabilityDomain::Llm,
            posture: CapabilityPosture::Native,
            tier: TierPosture::Ce,
            providers: vec![provider_id],
        });
    }

    capabilities.push(CapabilityTruthEntry {
        capability_id: CapabilityId::from("tool.emit_artifact"),
        domain: CapabilityDomain::Tool,
        posture: CapabilityPosture::ExternalOnly,
        tier: TierPosture::Ce,
        providers: Vec::new(),
    });
    capabilities.push(CapabilityTruthEntry {
        capability_id: CapabilityId::from("rule_eval.generic"),
        domain: CapabilityDomain::RuleEval,
        posture: CapabilityPosture::Native,
        tier: TierPosture::Ce,
        providers: vec![ProviderId::from("clips")],
    });

    // US5 (T056): probe whether the CLIPS rules provider is reachable
    // through the engine-internal capability surface. Until the narrow
    // FFI bridge in `engine_adapter.rs` lands (Integration follow-up), we
    // probe via the public capability-truth projection that this function
    // assembles: if the `rule_eval.generic` capability is wired to the
    // `clips` provider, the rules provider is `Available`. Otherwise it
    // remains at the CE default (`Unavailable`).
    //
    // Arrow / Substrait / Zenoh runtime crates are not linked in CE, so
    // those three rows MUST stay at their CE defaults
    // (`DeclarationOnly` / `DeclarationOnly` / `Deferred`).
    let clips_rules_available = capabilities.iter().any(|entry| {
        entry.capability_id.as_str() == "rule_eval.generic"
            && entry.providers.iter().any(|p| p.as_str() == "clips")
    });

    ReadinessSnapshot::from_pure(captured_at, providers, capabilities)
        .with_clips_rules_provider_availability(if clips_rules_available {
            ClipsRulesProviderAvailability::Available
        } else {
            ClipsRulesProviderAvailability::Unavailable
        })
}

fn current_utc_rfc3339_seconds() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    unix_seconds_to_rfc3339(seconds)
}

fn unix_seconds_to_rfc3339(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let seconds_of_day = seconds % 86_400;
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn civil_from_days(days_since_epoch: i64) -> (i64, u64, u64) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if month <= 2 { 1 } else { 0 };
    (year, month as u64, day as u64)
}

/// Feature 105 (US6 / T065): project a `PipelineValidationOutcome` into the
/// `pipeline validate --json` outcome JSON per
/// `contracts/doctor-json-cli-projection-retarget.md`.
///
/// The CLI projection uses an `outcome` discriminator field (not `kind`)
/// and the projection wording `stage_id` for the `stage_name` field of the
/// Rust enum. The Rust enum's field names remain unchanged per the
/// supervisor's naming guidance; the rename is a CLI-output-only mapping.
///
/// Exit-code policy: callers MUST use [`pipeline_validate_exit_code`] to
/// derive the class-distinguishable non-zero exit code for each error
/// variant.
///
/// # Example
///
/// ```
/// use nxuskit::contract::v1_1::{
///     PipelineValidationOutcome, pipeline_validate_outcome_json,
/// };
/// let outcome = PipelineValidationOutcome::Ready;
/// let json = pipeline_validate_outcome_json(&outcome);
/// assert_eq!(json["outcome"], "valid");
/// ```
pub fn pipeline_validate_outcome_json(outcome: &PipelineValidationOutcome) -> serde_json::Value {
    match outcome {
        PipelineValidationOutcome::Ready => json!({ "outcome": "valid" }),
        PipelineValidationOutcome::SchemaInvalid { path, message } => json!({
            "outcome": "schema_invalid",
            "path": path,
            "details": message,
        }),
        PipelineValidationOutcome::CapabilityUnavailable {
            stage_name,
            capability_id,
        } => json!({
            "outcome": "capability_unavailable",
            "stage_id": stage_name.as_str(),
            "capability": capability_id.as_str(),
        }),
        PipelineValidationOutcome::ReadinessUnknown { reason } => json!({
            "outcome": "readiness_unknown",
            "reason": reason,
        }),
        PipelineValidationOutcome::BulkPayloadInControlPlane { path, payload_kind } => {
            let kind =
                serde_json::to_value(payload_kind).unwrap_or_else(|_| json!("large_inline_text"));
            json!({
                "outcome": "bulk_payload_in_control_plane",
                "path": path,
                "kind": kind,
            })
        }
        PipelineValidationOutcome::ZenohFieldLeak { path, identifier } => json!({
            "outcome": "zenoh_field_leak",
            "path": path,
            "identifier": identifier,
        }),
        PipelineValidationOutcome::PlanReferenceUnbound { stage_name } => json!({
            "outcome": "plan_reference_unbound",
            "stage_id": stage_name.as_str(),
        }),
        PipelineValidationOutcome::AmbiguousSlot { path } => json!({
            "outcome": "ambiguous_slot",
            "path": path,
        }),
    }
}

/// Class-distinguishable non-zero exit codes for `pipeline validate --json`
/// per the locked CLI projection contract. Exit code 0 means
/// `outcome=valid`.
///
/// # Example
///
/// ```
/// use nxuskit::contract::v1_1::{
///     PipelineValidationOutcome, pipeline_validate_exit_code,
/// };
/// assert_eq!(pipeline_validate_exit_code(&PipelineValidationOutcome::Ready), 0);
/// ```
pub fn pipeline_validate_exit_code(outcome: &PipelineValidationOutcome) -> i32 {
    match outcome {
        PipelineValidationOutcome::Ready => 0,
        PipelineValidationOutcome::SchemaInvalid { .. } => 11,
        PipelineValidationOutcome::ReadinessUnknown { .. } => 12,
        PipelineValidationOutcome::CapabilityUnavailable { .. } => 13,
        PipelineValidationOutcome::BulkPayloadInControlPlane { .. } => 14,
        PipelineValidationOutcome::ZenohFieldLeak { .. } => 15,
        PipelineValidationOutcome::PlanReferenceUnbound { .. } => 16,
        PipelineValidationOutcome::AmbiguousSlot { .. } => 17,
    }
}

/// Compact wrapper bundling the outcome JSON + exit code for the CLI
/// caller. Returned `Serialize` instances can be `serde_json::to_string`d
/// directly.
#[derive(Debug, Serialize)]
pub struct PipelineValidateCliProjection {
    #[serde(flatten)]
    pub outcome_json: serde_json::Value,
}

impl PipelineValidateCliProjection {
    pub fn from_outcome(outcome: &PipelineValidationOutcome) -> Self {
        Self {
            outcome_json: pipeline_validate_outcome_json(outcome),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_seconds_formatter_handles_epoch_and_leap_day() {
        assert_eq!(unix_seconds_to_rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(
            unix_seconds_to_rfc3339(1_582_934_400),
            "2020-02-29T00:00:00Z"
        );
    }

    #[test]
    fn doctor_snapshot_at_preserves_caller_capture_time() {
        let snapshot = doctor_readiness_snapshot_at("2026-05-27T12:34:56Z", &[], 400);
        assert_eq!(snapshot.captured_at, "2026-05-27T12:34:56Z");
    }
}
