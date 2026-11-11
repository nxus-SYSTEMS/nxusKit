//! Resource-advisory helper for the v1.1 contract-native runner.
//!
//! This module deliberately stays public-DTO-only. It does not expose local
//! process details, provider internals, plugin paths, or transport topology.

use nxuskit::contract::v1_1::{
    ModelSizeClass, ModelSizeEstimate, PipelineDefinition, ResourceAdvisory,
    ResourceAdvisoryReason, ResourceAdvisoryReasonCode, ResourceAdvisorySeverity,
    ResourceProbeStatus, StageType,
};

use super::fanout::{normalize_fan_out, parse_fan_out_metadata};

/// Build a public-safe resource advisory for a planned pipeline.
///
/// v1.1.0 RC has no stable cross-provider model-size API, so local LLM
/// sizing is partial: the runner can identify planned local LLM operations,
/// but model sizes stay `unknown` unless a future provider exposes a public
/// size signal. If no useful operation context can be derived, the stable
/// `resource_probe_unavailable` fallback is returned.
pub fn assess_pipeline_resources(def: &PipelineDefinition) -> ResourceAdvisory {
    let operations = planned_local_llm_operations(def);
    if operations.is_empty() {
        return ResourceAdvisory::probe_unavailable();
    }

    let mut reasons = Vec::new();
    let operation_ids: Vec<String> = operations
        .iter()
        .map(|op| sanitize_public_token(&op.operation_id))
        .collect();

    reasons.push(ResourceAdvisoryReason {
        code: ResourceAdvisoryReasonCode::UnknownModelSize,
        detail: sanitize_detail("local model sizes are not publicly available"),
        operations: operation_ids.clone(),
    });

    let recommended_max_concurrency = if operations.len() > 1 { Some(1) } else { None };

    ResourceAdvisory {
        contract_version: nxuskit::contract::v1_1::CONTRACT_VERSION.to_owned(),
        severity: ResourceAdvisorySeverity::Caution,
        reasons,
        recommended_max_concurrency,
        model_size_estimates: operations
            .iter()
            .map(|op| ModelSizeEstimate {
                provider_ref: sanitize_public_token(&op.provider_ref),
                model_ref: sanitize_public_token(&op.model_ref),
                size_class: ModelSizeClass::Unknown,
                estimated_bytes: None,
            })
            .collect(),
        memory_estimate_mb: None,
        probe_status: ResourceProbeStatus::Partial,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PlannedOperation {
    operation_id: String,
    provider_ref: String,
    model_ref: String,
}

fn planned_local_llm_operations(def: &PipelineDefinition) -> Vec<PlannedOperation> {
    let mut operations = Vec::new();
    for stage in &def.stages {
        if stage.stage_type != StageType::Llm {
            continue;
        }
        let stage_id = stage.name.as_str();
        if let Some(spec) = parse_fan_out_metadata(&stage.metadata)
            && spec.mode == "parallel"
        {
            for op in normalize_fan_out(stage_id, &spec) {
                if is_local_provider(&op.provider_ref) {
                    operations.push(PlannedOperation {
                        operation_id: op.operation_id,
                        provider_ref: op.provider_ref,
                        model_ref: op.model_ref,
                    });
                }
            }
            continue;
        }

        let provider_ref = stage
            .provider_ref
            .as_ref()
            .map(|p| p.as_str())
            .unwrap_or("");
        if is_local_provider(provider_ref) {
            operations.push(PlannedOperation {
                operation_id: stage_id.to_owned(),
                provider_ref: provider_ref.to_owned(),
                model_ref: stage
                    .model_ref
                    .clone()
                    .unwrap_or_else(|| "unknown".to_owned()),
            });
        }
    }
    operations
}

fn is_local_provider(provider_ref: &str) -> bool {
    matches!(provider_ref, "loopback" | "ollama" | "lmstudio")
}

fn sanitize_detail(input: &str) -> String {
    sanitize_public_token(input)
}

fn sanitize_public_token(input: &str) -> String {
    const BANNED: &[&str] = &[
        concat!("Provider", "Channel"),
        concat!("Tool", "Catalog"),
        concat!("Request", "Envelope"),
        concat!("Result", "Envelope"),
        concat!("nxusKit", "-internal"),
        "nxusKit-examples-internal",
        "peeler-internal",
        "peeler-internals",
        "peeler-personas",
    ];
    let mut value = input.to_owned();
    for banned in BANNED {
        value = value.replace(banned, "redacted");
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use nxuskit::contract::v1_1::{CapabilityId, ProviderId, StageDecl, StageName};
    use serde_json::json;

    #[test]
    fn fan_out_pipeline_gets_partial_public_advisory() {
        let mut stage = StageDecl::llm(StageName::from("compare"));
        stage.provider_ref = Some(ProviderId::from("loopback"));
        stage.model_ref = Some("echo".to_owned());
        stage.capability_ref = CapabilityId::from("llm.text_completion");
        stage.metadata.insert(
            "fan_out".to_owned(),
            json!({
                "mode": "parallel",
                "provider_model_pairs": [
                    {"provider_ref": "loopback", "model_ref": "echo"},
                    {"provider_ref": "loopback", "model_ref": "echo-json-native"}
                ]
            }),
        );
        let def = PipelineDefinition::new("p", "1.0.0").with_stage(stage);
        let advisory = assess_pipeline_resources(&def);
        assert_eq!(advisory.probe_status, ResourceProbeStatus::Partial);
        assert_eq!(advisory.recommended_max_concurrency, Some(1));
        assert!(
            advisory
                .reasons
                .iter()
                .any(|r| r.code == ResourceAdvisoryReasonCode::UnknownModelSize)
        );
    }
}
