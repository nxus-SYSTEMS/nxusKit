//! Cohere candidate capability record.

use std::collections::HashMap;

use serde_json::Value;

use crate::capabilities::{
    CapabilityFeatureSet, CapabilityStatus, ModelCapabilityOverride, ProviderCapabilityRecord,
};

use super::{REVIEWED_ON, evidence};

pub(crate) fn record() -> ProviderCapabilityRecord {
    let docs_chat = "https://docs.cohere.com/reference/chat";
    let docs_structured = "https://docs.cohere.com/docs/structured-outputs";
    let docs_tool_use = "https://docs.cohere.com/docs/tool-use";
    let docs_embed = "https://docs.cohere.com/reference/embed";
    let docs_rerank = "https://docs.cohere.com/reference/rerank";

    let mut features = CapabilityFeatureSet::default();
    features.structured_output.json_schema_best_effort = CapabilityStatus::Recognized;
    features.structured_output.named_schemas = CapabilityStatus::Recognized;
    features.tool_calling.function_calling = CapabilityStatus::Recognized;
    features.search_citations.citation_metadata = CapabilityStatus::Recognized;
    features.modalities.embeddings = CapabilityStatus::Future;
    features.modalities.rerank = CapabilityStatus::Future;

    let mut evidence_map = HashMap::new();
    evidence_map.insert(
        "structured_output".into(),
        evidence(
            docs_structured,
            "Design-only: structured-output behavior needs a Cohere adapter fixture.",
        ),
    );
    evidence_map.insert(
        "tool_calling".into(),
        evidence(
            docs_tool_use,
            "Design-only: tool-use and citation metadata need provider-neutral mapping.",
        ),
    );
    evidence_map.insert(
        "search_citations".into(),
        evidence(
            docs_chat,
            "Design-only: citation metadata is not normalized into the SDK envelope yet.",
        ),
    );
    evidence_map.insert(
        "modalities".into(),
        evidence(
            docs_embed,
            format!(
                "Embed ({docs_embed}) and rerank ({docs_rerank}) are deferred shared SDK surfaces."
            ),
        ),
    );

    ProviderCapabilityRecord {
        provider_id: "cohere".into(),
        display_name: "Cohere (candidate)".into(),
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
                        "Define chat/tool-use request mapping and citation envelope behavior.",
                        "Add structured-output and tool-use fixtures.",
                        "Keep embed/rerank as separate shared SDK surfaces until FR-014 is designed."
                    ]),
                ),
            ]
            .into_iter()
            .collect(),
        ),
    }
}
