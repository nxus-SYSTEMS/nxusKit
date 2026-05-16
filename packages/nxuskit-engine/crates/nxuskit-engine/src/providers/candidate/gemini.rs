//! Google Gemini candidate capability record.

use std::collections::HashMap;

use serde_json::Value;

use crate::capabilities::{
    CapabilityFeatureSet, CapabilityStatus, ModelCapabilityOverride, ProviderCapabilityRecord,
};

use super::{REVIEWED_ON, evidence};

pub(crate) fn record() -> ProviderCapabilityRecord {
    let docs_function = "https://ai.google.dev/gemini-api/docs/function-calling";
    let docs_structured = "https://ai.google.dev/gemini-api/docs/structured-output";
    let docs_grounding = "https://ai.google.dev/gemini-api/docs/grounding";
    let docs_code = "https://ai.google.dev/gemini-api/docs/code-execution";
    let docs_embeddings = "https://ai.google.dev/gemini-api/docs/embeddings";

    let mut features = CapabilityFeatureSet::default();
    features.structured_output.json_schema_strict = CapabilityStatus::Recognized;
    features.structured_output.json_schema_best_effort = CapabilityStatus::Recognized;
    features.structured_output.named_schemas = CapabilityStatus::Recognized;
    features.tool_calling.function_calling = CapabilityStatus::Recognized;
    features.tool_calling.parallel_tool_calls = CapabilityStatus::Recognized;
    features.search_citations.search_controls = CapabilityStatus::Recognized;
    features.search_citations.grounding_metadata = CapabilityStatus::Recognized;
    features.hosted_tools.web_search = CapabilityStatus::ProviderSpecific;
    features.hosted_tools.code_interpreter = CapabilityStatus::ProviderSpecific;
    features.modalities.vision_input = CapabilityStatus::Recognized;
    features.modalities.embeddings = CapabilityStatus::Future;

    let mut evidence_map = HashMap::new();
    evidence_map.insert(
        "structured_output".into(),
        evidence(
            docs_structured,
            "Design-only: structured output requires Gemini-native request mapping and fixtures.",
        ),
    );
    evidence_map.insert(
        "tool_calling".into(),
        evidence(
            docs_function,
            "Design-only: function-calling semantics require a Gemini adapter contract.",
        ),
    );
    evidence_map.insert(
        "search_citations".into(),
        evidence(
            docs_grounding,
            "Design-only: Google Search grounding needs citation/grounding metadata mapping.",
        ),
    );
    evidence_map.insert(
        "hosted_tools".into(),
        evidence(
            docs_code,
            "Provider-specific code execution and search grounding deferred in v0.9.4.",
        ),
    );
    evidence_map.insert(
        "modalities".into(),
        evidence(
            docs_embeddings,
            "Embeddings inform the deferred shared embeddings surface; no request API ships here.",
        ),
    );

    ProviderCapabilityRecord {
        provider_id: "gemini".into(),
        display_name: "Google Gemini (candidate)".into(),
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
                        "Define Gemini-native request/response adapter contract.",
                        "Add CE-safe fixtures for structured output and function calling.",
                        "Decide grounding/citation envelope before first-class search controls."
                    ]),
                ),
            ]
            .into_iter()
            .collect(),
        ),
    }
}
