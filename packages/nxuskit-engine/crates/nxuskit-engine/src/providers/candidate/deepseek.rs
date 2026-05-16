//! DeepSeek candidate capability record.

use std::collections::HashMap;

use serde_json::Value;

use crate::capabilities::{
    CapabilityFeatureSet, CapabilityStatus, ModelCapabilityOverride, ProviderCapabilityRecord,
};

use super::{REVIEWED_ON, evidence};

pub(crate) fn record() -> ProviderCapabilityRecord {
    let docs_chat = "https://api-docs.deepseek.com/api/create-chat-completion";
    let docs_json = "https://api-docs.deepseek.com/guides/json_mode";
    let docs_function = "https://api-docs.deepseek.com/guides/function_calling";
    let docs_reasoning = "https://api-docs.deepseek.com/guides/reasoning_model";
    let docs_prefix = "https://api-docs.deepseek.com/guides/chat_prefix_completion";

    let mut features = CapabilityFeatureSet::default();
    features.reasoning.reasoning_content_field = CapabilityStatus::Recognized;
    features.structured_output.json_mode = CapabilityStatus::Recognized;
    features.structured_output.json_schema_strict = CapabilityStatus::ProviderSpecific;
    features.tool_calling.function_calling = CapabilityStatus::ProviderSpecific;
    features.logprobs.logprobs = CapabilityStatus::Recognized;

    let mut evidence_map = HashMap::new();
    evidence_map.insert(
        "reasoning".into(),
        evidence(
            docs_reasoning,
            "Design-only: reasoning content requires response decoder and fixtures.",
        ),
    );
    evidence_map.insert(
        "structured_output".into(),
        evidence(
            docs_json,
            "Design-only: JSON mode and strict schema beta are not mapped in v0.9.4.",
        ),
    );
    evidence_map.insert(
        "tool_calling".into(),
        evidence(
            docs_function,
            "Design-only: strict function-calling beta stays provider-specific.",
        ),
    );
    evidence_map.insert(
        "logprobs".into(),
        evidence(
            docs_chat,
            "Design-only: logprobs require adapter mapping before promotion.",
        ),
    );

    ProviderCapabilityRecord {
        provider_id: "deepseek".into(),
        display_name: "DeepSeek (candidate)".into(),
        last_reviewed_on: REVIEWED_ON.into(),
        default_model: None,
        features,
        evidence: evidence_map,
        model_overrides: HashMap::<String, ModelCapabilityOverride>::new(),
        provider_specific: Value::Object(
            [
                ("posture".into(), Value::String("design_only".into())),
                ("adapter_registered".into(), Value::Bool(false)),
                (
                    "promotion_criteria".into(),
                    serde_json::json!([
                        "Define OpenAI-compatible request mapping boundaries.",
                        "Add fixtures for JSON mode, reasoning content, logprobs, and strict function calling.",
                        format!("Review prefix/FIM beta separately before exposing request controls: {docs_prefix}")
                    ]),
                ),
            ]
            .into_iter()
            .collect(),
        ),
    }
}
