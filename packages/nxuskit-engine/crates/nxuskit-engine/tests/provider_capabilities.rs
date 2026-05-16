//! Provider capability registry tests (feature 099 — Phases 2 and 3).
//!
//! Phase 2 (T006–T010, T015): foundational typed-model invariants.
//! Phase 3 (T016–T019): per-provider snapshot, model-override, evidence
//! freshness, and recognized-not-collapsed assertions for User Story 1.
//! Tests for both phases live in this single file because the Phase 3
//! assertions consume the same `registry` and feature-set surfaces.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use nxuskit_engine::capabilities::{
    CapabilityEvidence, CapabilityFeatureSet, CapabilityStatus, FeatureRequest,
    ModelCapabilityOverride, ProviderCapabilityRecord, ReasoningConfig, StructuredOutputConfig,
    StructuredOutputError, StructuredOutputMode, TextVerbosity, ToolCallConfig, ToolCallError,
    ToolChoice, ToolDefinition, ValidationOutcome, ValidationSeverity, derive_legacy_capabilities,
    registry, validate_request_against_record, validate_typed_request_parts,
};
use nxuskit_engine::types::ProviderCapabilities;

// ---------------------------------------------------------------------------
// T006: CapabilityStatus serialization, deserialization, default == Unknown.
// ---------------------------------------------------------------------------

#[test]
fn capability_status_serializes_to_snake_case() {
    let cases = [
        (CapabilityStatus::Supported, "\"supported\""),
        (CapabilityStatus::Unsupported, "\"unsupported\""),
        (CapabilityStatus::Recognized, "\"recognized\""),
        (CapabilityStatus::ProviderSpecific, "\"provider_specific\""),
        (CapabilityStatus::Future, "\"future\""),
        (CapabilityStatus::Unknown, "\"unknown\""),
    ];
    for (status, json) in cases {
        let encoded = serde_json::to_string(&status).expect("status serializes");
        assert_eq!(encoded, json, "{status:?} -> {json}");
        let round: CapabilityStatus = serde_json::from_str(json).expect("status deserializes");
        assert_eq!(round, status, "round-trip {json}");
    }
}

#[test]
fn capability_status_default_is_unknown() {
    assert_eq!(CapabilityStatus::default(), CapabilityStatus::Unknown);
}

#[test]
fn capability_feature_set_default_is_all_unknown() {
    let fs = CapabilityFeatureSet::default();
    assert_eq!(fs.reasoning.effort_control, CapabilityStatus::Unknown);
    assert_eq!(
        fs.structured_output.json_schema_strict,
        CapabilityStatus::Unknown
    );
    assert_eq!(fs.tool_calling.function_calling, CapabilityStatus::Unknown);
    assert_eq!(fs.hosted_tools.web_search, CapabilityStatus::Unknown);
    assert_eq!(
        fs.search_citations.citation_metadata,
        CapabilityStatus::Unknown
    );
    assert_eq!(fs.modalities.vision_input, CapabilityStatus::Unknown);
    assert_eq!(fs.routing.provider_routing, CapabilityStatus::Unknown);
    assert_eq!(fs.state.previous_response_id, CapabilityStatus::Unknown);
    assert_eq!(fs.logprobs.logprobs, CapabilityStatus::Unknown);
}

// ---------------------------------------------------------------------------
// T007: Supported status requires CapabilityEvidence with source URL plus
// either an adapter test, fixture path, or live-test identifier.
// ---------------------------------------------------------------------------

#[test]
fn supported_requires_source_url_and_proof() {
    let no_url = CapabilityEvidence {
        source_url: None,
        source_reviewed_on: "2026-05-09".into(),
        adapter_test: Some("t".into()),
        fixture_path: None,
        live_test: None,
        notes: vec![],
    };
    assert!(no_url.is_evidence_for_supported().is_err());

    let no_proof = CapabilityEvidence {
        source_url: Some("https://example.com/docs".into()),
        source_reviewed_on: "2026-05-09".into(),
        adapter_test: None,
        fixture_path: None,
        live_test: None,
        notes: vec![],
    };
    assert!(no_proof.is_evidence_for_supported().is_err());

    let ok_adapter = CapabilityEvidence {
        source_url: Some("https://example.com/docs".into()),
        source_reviewed_on: "2026-05-09".into(),
        adapter_test: Some("openai_function_calling_test".into()),
        fixture_path: None,
        live_test: None,
        notes: vec![],
    };
    assert!(ok_adapter.is_evidence_for_supported().is_ok());

    let ok_fixture = CapabilityEvidence {
        source_url: Some("https://example.com/docs".into()),
        source_reviewed_on: "2026-05-09".into(),
        adapter_test: None,
        fixture_path: Some("internal/tests/parity/.../json-schema-minimal.json".into()),
        live_test: None,
        notes: vec![],
    };
    assert!(ok_fixture.is_evidence_for_supported().is_ok());

    let ok_live = CapabilityEvidence {
        source_url: Some("https://example.com/docs".into()),
        source_reviewed_on: "2026-05-09".into(),
        adapter_test: None,
        fixture_path: None,
        live_test: Some("live_openai_responses_smoke".into()),
        notes: vec![],
    };
    assert!(ok_live.is_evidence_for_supported().is_ok());
}

#[test]
fn supported_rejects_empty_proof_strings() {
    // Empty source_url string is rejected just like `None`.
    let empty_url = CapabilityEvidence {
        source_url: Some(String::new()),
        source_reviewed_on: "2026-05-09".into(),
        adapter_test: Some("ok".into()),
        fixture_path: None,
        live_test: None,
        notes: vec![],
    };
    assert!(empty_url.is_evidence_for_supported().is_err());

    // Empty proof strings across all three proof slots count as "no proof".
    let empty_proofs = CapabilityEvidence {
        source_url: Some("https://example.com/docs".into()),
        source_reviewed_on: "2026-05-09".into(),
        adapter_test: Some(String::new()),
        fixture_path: Some(String::new()),
        live_test: Some(String::new()),
        notes: vec![],
    };
    assert!(empty_proofs.is_evidence_for_supported().is_err());

    // A single non-empty proof slot is enough — the others may stay empty.
    let one_real_proof = CapabilityEvidence {
        source_url: Some("https://example.com/docs".into()),
        source_reviewed_on: "2026-05-09".into(),
        adapter_test: Some(String::new()),
        fixture_path: Some("internal/tests/parity/.../json-schema-minimal.json".into()),
        live_test: Some(String::new()),
        notes: vec![],
    };
    assert!(one_real_proof.is_evidence_for_supported().is_ok());
}

// ---------------------------------------------------------------------------
// T008: registry completeness across existing, mock, loopback, Fireworks,
// and candidate providers.
// ---------------------------------------------------------------------------

#[test]
fn all_known_providers_have_capability_records() {
    let required = [
        // Existing.
        "openai",
        "anthropic",
        "mistral",
        "groq",
        "xai",
        "perplexity",
        "together",
        "openrouter",
        "ollama",
        "lmstudio",
        // Synthetic.
        "mock",
        "loopback",
        // Existing extra.
        "fireworks",
        // Candidate (design-only entries; full records land in later phases).
        "gemini",
        "cohere",
        "deepseek",
    ];

    let recs = registry::all_records();
    let ids: Vec<&str> = recs.iter().map(|r| r.provider_id.as_str()).collect();
    for provider in required {
        assert!(
            ids.contains(&provider),
            "registry missing provider record: {provider}; have {ids:?}"
        );
    }

    // Every record must declare a non-empty display name and a reviewed-on date.
    for r in &recs {
        assert!(
            !r.display_name.is_empty(),
            "{} missing display_name",
            r.provider_id
        );
        assert!(
            !r.last_reviewed_on.is_empty(),
            "{} missing last_reviewed_on",
            r.provider_id
        );
    }
}

#[test]
fn registry_lookup_is_consistent() {
    let by_id = registry::find("openai").expect("openai entry");
    assert_eq!(by_id.provider_id, "openai");
}

// ---------------------------------------------------------------------------
// T009: backward-compatibility — existing boolean ProviderCapabilities
// fields and call sites still compile and the legacy bridge produces the
// historical defaults.
// ---------------------------------------------------------------------------

#[test]
fn legacy_provider_capabilities_default_unchanged() {
    let caps = ProviderCapabilities::default();
    assert!(caps.supports_system_messages);
    assert!(!caps.supports_streaming);
    assert!(!caps.supports_vision);
    assert_eq!(caps.max_stop_sequences, None);
    assert!(!caps.supports_presence_penalty);
    assert!(!caps.supports_frequency_penalty);
    assert!(!caps.supports_seed);
    assert!(!caps.supports_logprobs);
    assert!(!caps.supports_streaming_logprobs);
    assert!(!caps.supports_json_mode);
    assert!(!caps.supports_json_schema);
    assert_eq!(caps.penalty_range, None);
    assert_eq!(caps.max_logprobs, None);
}

#[test]
fn legacy_bridge_derives_booleans_from_feature_set() {
    let mut fs = CapabilityFeatureSet::default();
    fs.modalities.vision_input = CapabilityStatus::Supported;
    fs.structured_output.json_mode = CapabilityStatus::Supported;
    fs.structured_output.json_schema_strict = CapabilityStatus::Supported;
    fs.logprobs.logprobs = CapabilityStatus::Supported;
    fs.logprobs.streaming_logprobs = CapabilityStatus::Supported;

    let legacy = derive_legacy_capabilities(&fs);
    assert!(legacy.supports_vision);
    assert!(legacy.supports_json_mode);
    assert!(legacy.supports_json_schema);
    assert!(legacy.supports_logprobs);
    assert!(legacy.supports_streaming_logprobs);
}

// ---------------------------------------------------------------------------
// T010: pre-network validation — unsupported, recognized-only, future,
// and unknown features warn or fail before serialization.
// ---------------------------------------------------------------------------

fn baseline_record(status_for_struct_output: CapabilityStatus) -> ProviderCapabilityRecord {
    let mut fs = CapabilityFeatureSet::default();
    fs.structured_output.json_schema_strict = status_for_struct_output;
    ProviderCapabilityRecord {
        provider_id: "test".into(),
        display_name: "Test".into(),
        last_reviewed_on: "2026-05-09".into(),
        default_model: None,
        features: fs,
        evidence: HashMap::new(),
        model_overrides: HashMap::new(),
        provider_specific: serde_json::Value::Null,
    }
}

fn struct_output_request() -> FeatureRequest {
    // Best-effort (strict=None) mode: tests the original Phase 2 status
    // mapping where Recognized/Unknown warn rather than Block. The strict
    // path has tighter Block-on-not-Supported semantics covered by the
    // T029 strict_mode_requires_supported_capability test.
    FeatureRequest::structured_output(StructuredOutputConfig {
        mode: StructuredOutputMode::JsonSchema,
        schema: Some(serde_json::json!({"type": "object"})),
        schema_name: Some("country_capital".into()),
        strict: None,
        schema_subset: None,
    })
}

#[test]
fn validation_unsupported_fails_pre_network() {
    let rec = baseline_record(CapabilityStatus::Unsupported);
    let req = struct_output_request();
    let outcome = validate_request_against_record(&rec, &req);
    assert!(matches!(outcome, ValidationOutcome::Block { .. }));
    assert_eq!(outcome.severity(), ValidationSeverity::Error);
}

#[test]
fn validation_recognized_only_warns_pre_network() {
    let rec = baseline_record(CapabilityStatus::Recognized);
    let req = struct_output_request();
    let outcome = validate_request_against_record(&rec, &req);
    assert!(matches!(outcome, ValidationOutcome::Warn { .. }));
    assert_eq!(outcome.severity(), ValidationSeverity::Warning);
}

#[test]
fn validation_future_fails_pre_network() {
    let rec = baseline_record(CapabilityStatus::Future);
    let req = struct_output_request();
    let outcome = validate_request_against_record(&rec, &req);
    assert!(matches!(outcome, ValidationOutcome::Block { .. }));
}

#[test]
fn validation_unknown_warns_pre_network() {
    let rec = baseline_record(CapabilityStatus::Unknown);
    let req = struct_output_request();
    let outcome = validate_request_against_record(&rec, &req);
    assert!(matches!(outcome, ValidationOutcome::Warn { .. }));
}

#[test]
fn validation_supported_passes() {
    let rec = baseline_record(CapabilityStatus::Supported);
    let req = struct_output_request();
    let outcome = validate_request_against_record(&rec, &req);
    assert!(matches!(outcome, ValidationOutcome::Allow));
}

// ---------------------------------------------------------------------------
// T015: serde round-trip snapshots for every new public capability type.
// ---------------------------------------------------------------------------

fn round_trip<T>(value: &T)
where
    T: serde::Serialize + serde::de::DeserializeOwned + std::fmt::Debug,
{
    let encoded = serde_json::to_string(value).expect("serialize");
    let _decoded: T = serde_json::from_str(&encoded).expect("deserialize");
}

#[test]
fn round_trip_evidence_and_records() {
    let evidence = CapabilityEvidence {
        source_url: Some("https://example.com".into()),
        source_reviewed_on: "2026-05-09".into(),
        adapter_test: Some("t".into()),
        fixture_path: Some("p".into()),
        live_test: None,
        notes: vec!["note".into()],
    };
    round_trip(&evidence);

    let mut record = ProviderCapabilityRecord {
        provider_id: "openai".into(),
        display_name: "OpenAI".into(),
        last_reviewed_on: "2026-05-09".into(),
        default_model: Some("gpt-5.5".into()),
        features: CapabilityFeatureSet::default(),
        evidence: HashMap::new(),
        model_overrides: HashMap::new(),
        provider_specific: serde_json::json!({"transport": "responses"}),
    };
    record.evidence.insert("structured_output".into(), evidence);
    record.model_overrides.insert(
        "gpt-5.5".into(),
        ModelCapabilityOverride {
            reviewed_on: "2026-05-09".into(),
            feature_overrides: HashMap::new(),
            notes: vec![],
        },
    );
    round_trip(&record);
}

#[test]
fn round_trip_request_types() {
    let so = StructuredOutputConfig {
        mode: StructuredOutputMode::JsonSchema,
        schema: Some(serde_json::json!({"type": "object"})),
        schema_name: Some("name".into()),
        strict: Some(true),
        schema_subset: None,
    };
    round_trip(&so);

    let tc = ToolCallConfig {
        tools: vec![ToolDefinition {
            name: "search".into(),
            description: Some("desc".into()),
            parameters: serde_json::json!({"type": "object"}),
            strict: Some(true),
        }],
        tool_choice: ToolChoice::Auto,
        parallel_tool_calls: Some(true),
        streaming_tool_calls: Some(false),
    };
    round_trip(&tc);
    round_trip(&ToolChoice::Named("search".into()));
    round_trip(&TextVerbosity::Medium);
    round_trip(&ReasoningConfig {
        effort: Some("medium".into()),
        summary: None,
        include_encrypted_content: false,
        preserve_blocks: false,
    });
}

// ---------------------------------------------------------------------------
// T016 [US1]: registry snapshot — every non-candidate provider record must
// be evidence-backed. Each non-candidate ID below must have a non-empty
// `last_reviewed_on`, at least one evidence entry, and every feature flagged
// `Supported` must have matching evidence whose `is_evidence_for_supported()`
// is `Ok`.
// ---------------------------------------------------------------------------

#[test]
fn all_non_candidate_provider_records_are_evidence_backed() {
    use nxuskit_engine::capabilities::CapabilityFeatureSet;

    fn supported_feature_keys(fs: &CapabilityFeatureSet) -> Vec<&'static str> {
        let mut keys = Vec::new();
        // Reasoning.
        if fs.reasoning.effort_control == CapabilityStatus::Supported {
            keys.push("reasoning.effort_control");
        }
        if fs.reasoning.reasoning_summary == CapabilityStatus::Supported {
            keys.push("reasoning.reasoning_summary");
        }
        if fs.reasoning.thinking_blocks == CapabilityStatus::Supported {
            keys.push("reasoning.thinking_blocks");
        }
        if fs.reasoning.reasoning_content_field == CapabilityStatus::Supported {
            keys.push("reasoning.reasoning_content_field");
        }
        // Structured output.
        if fs.structured_output.json_mode == CapabilityStatus::Supported {
            keys.push("structured_output.json_mode");
        }
        if fs.structured_output.json_schema_strict == CapabilityStatus::Supported {
            keys.push("structured_output.json_schema_strict");
        }
        if fs.structured_output.json_schema_best_effort == CapabilityStatus::Supported {
            keys.push("structured_output.json_schema_best_effort");
        }
        if fs.structured_output.named_schemas == CapabilityStatus::Supported {
            keys.push("structured_output.named_schemas");
        }
        if fs.structured_output.additionalprops_false == CapabilityStatus::Supported {
            keys.push("structured_output.additionalprops_false");
        }
        // Tool calling.
        if fs.tool_calling.function_calling == CapabilityStatus::Supported {
            keys.push("tool_calling.function_calling");
        }
        if fs.tool_calling.parallel_tool_calls == CapabilityStatus::Supported {
            keys.push("tool_calling.parallel_tool_calls");
        }
        if fs.tool_calling.streaming_tool_calls == CapabilityStatus::Supported {
            keys.push("tool_calling.streaming_tool_calls");
        }
        // Hosted tools.
        if fs.hosted_tools.web_search == CapabilityStatus::Supported {
            keys.push("hosted_tools.web_search");
        }
        if fs.hosted_tools.file_search == CapabilityStatus::Supported {
            keys.push("hosted_tools.file_search");
        }
        if fs.hosted_tools.code_interpreter == CapabilityStatus::Supported {
            keys.push("hosted_tools.code_interpreter");
        }
        if fs.hosted_tools.image_generation == CapabilityStatus::Supported {
            keys.push("hosted_tools.image_generation");
        }
        if fs.hosted_tools.computer_use == CapabilityStatus::Supported {
            keys.push("hosted_tools.computer_use");
        }
        if fs.hosted_tools.mcp_connector == CapabilityStatus::Supported {
            keys.push("hosted_tools.mcp_connector");
        }
        // Search / citations.
        if fs.search_citations.search_controls == CapabilityStatus::Supported {
            keys.push("search_citations.search_controls");
        }
        if fs.search_citations.citation_metadata == CapabilityStatus::Supported {
            keys.push("search_citations.citation_metadata");
        }
        if fs.search_citations.grounding_metadata == CapabilityStatus::Supported {
            keys.push("search_citations.grounding_metadata");
        }
        // Modalities.
        if fs.modalities.vision_input == CapabilityStatus::Supported {
            keys.push("modalities.vision_input");
        }
        if fs.modalities.audio_input == CapabilityStatus::Supported {
            keys.push("modalities.audio_input");
        }
        if fs.modalities.audio_output == CapabilityStatus::Supported {
            keys.push("modalities.audio_output");
        }
        if fs.modalities.embeddings == CapabilityStatus::Supported {
            keys.push("modalities.embeddings");
        }
        if fs.modalities.rerank == CapabilityStatus::Supported {
            keys.push("modalities.rerank");
        }
        if fs.modalities.moderation == CapabilityStatus::Supported {
            keys.push("modalities.moderation");
        }
        // Routing.
        if fs.routing.provider_routing == CapabilityStatus::Supported {
            keys.push("routing.provider_routing");
        }
        if fs.routing.require_parameters == CapabilityStatus::Supported {
            keys.push("routing.require_parameters");
        }
        if fs.routing.fallback_policy == CapabilityStatus::Supported {
            keys.push("routing.fallback_policy");
        }
        // State.
        if fs.state.previous_response_id == CapabilityStatus::Supported {
            keys.push("state.previous_response_id");
        }
        if fs.state.response_phase == CapabilityStatus::Supported {
            keys.push("state.response_phase");
        }
        if fs.state.prompt_caching == CapabilityStatus::Supported {
            keys.push("state.prompt_caching");
        }
        if fs.state.context_caching == CapabilityStatus::Supported {
            keys.push("state.context_caching");
        }
        // Logprobs.
        if fs.logprobs.logprobs == CapabilityStatus::Supported {
            keys.push("logprobs.logprobs");
        }
        if fs.logprobs.streaming_logprobs == CapabilityStatus::Supported {
            keys.push("logprobs.streaming_logprobs");
        }
        keys
    }

    let required_ids = [
        "openai",
        "anthropic",
        "mistral",
        "groq",
        "xai",
        "perplexity",
        "together",
        "openrouter",
        "ollama",
        "lmstudio",
        "fireworks",
        "mock",
        "loopback",
    ];

    let recs = registry::all_records();
    for id in required_ids {
        let rec = recs
            .iter()
            .find(|r| r.provider_id == id)
            .expect("registry missing required non-candidate provider");

        assert!(
            !rec.last_reviewed_on.is_empty(),
            "{id}: last_reviewed_on must be non-empty"
        );
        assert!(
            !rec.evidence.is_empty(),
            "{id}: must carry at least one evidence entry"
        );

        for feature_key in supported_feature_keys(&rec.features) {
            // Either an exact-match evidence entry or a top-level (group-level)
            // evidence entry keyed by the feature group prefix is acceptable.
            let group = feature_key.split('.').next().unwrap_or(feature_key);
            let evidence = rec
                .evidence
                .get(feature_key)
                .or_else(|| rec.evidence.get(group))
                .expect("supported feature must have matching evidence entry");
            evidence
                .is_evidence_for_supported()
                .expect("evidence for Supported feature must be complete");
        }
    }
}

// ---------------------------------------------------------------------------
// T017 [US1]: OpenAI frontier model overrides — gpt-5.5, the dated snapshot
// gpt-5.5-2026-04-23, gpt-5.4, and gpt-5.4-mini must all appear in the
// OpenAI record's `model_overrides` with a non-empty `reviewed_on`.
// ---------------------------------------------------------------------------

// Local helper: list the dotted feature keys that are currently `Supported`
// on a feature set. Kept inline to keep this test file self-contained.
fn supported_feature_keys_for(
    fs: &nxuskit_engine::capabilities::CapabilityFeatureSet,
) -> Vec<&'static str> {
    let mut keys = Vec::new();
    if fs.reasoning.effort_control == CapabilityStatus::Supported {
        keys.push("reasoning.effort_control");
    }
    if fs.reasoning.reasoning_summary == CapabilityStatus::Supported {
        keys.push("reasoning.reasoning_summary");
    }
    if fs.reasoning.thinking_blocks == CapabilityStatus::Supported {
        keys.push("reasoning.thinking_blocks");
    }
    if fs.reasoning.reasoning_content_field == CapabilityStatus::Supported {
        keys.push("reasoning.reasoning_content_field");
    }
    if fs.structured_output.json_mode == CapabilityStatus::Supported {
        keys.push("structured_output.json_mode");
    }
    if fs.structured_output.json_schema_strict == CapabilityStatus::Supported {
        keys.push("structured_output.json_schema_strict");
    }
    if fs.structured_output.json_schema_best_effort == CapabilityStatus::Supported {
        keys.push("structured_output.json_schema_best_effort");
    }
    if fs.structured_output.named_schemas == CapabilityStatus::Supported {
        keys.push("structured_output.named_schemas");
    }
    if fs.structured_output.additionalprops_false == CapabilityStatus::Supported {
        keys.push("structured_output.additionalprops_false");
    }
    if fs.tool_calling.function_calling == CapabilityStatus::Supported {
        keys.push("tool_calling.function_calling");
    }
    if fs.tool_calling.parallel_tool_calls == CapabilityStatus::Supported {
        keys.push("tool_calling.parallel_tool_calls");
    }
    if fs.tool_calling.streaming_tool_calls == CapabilityStatus::Supported {
        keys.push("tool_calling.streaming_tool_calls");
    }
    if fs.hosted_tools.web_search == CapabilityStatus::Supported {
        keys.push("hosted_tools.web_search");
    }
    if fs.hosted_tools.file_search == CapabilityStatus::Supported {
        keys.push("hosted_tools.file_search");
    }
    if fs.hosted_tools.code_interpreter == CapabilityStatus::Supported {
        keys.push("hosted_tools.code_interpreter");
    }
    if fs.hosted_tools.image_generation == CapabilityStatus::Supported {
        keys.push("hosted_tools.image_generation");
    }
    if fs.hosted_tools.computer_use == CapabilityStatus::Supported {
        keys.push("hosted_tools.computer_use");
    }
    if fs.hosted_tools.mcp_connector == CapabilityStatus::Supported {
        keys.push("hosted_tools.mcp_connector");
    }
    if fs.search_citations.search_controls == CapabilityStatus::Supported {
        keys.push("search_citations.search_controls");
    }
    if fs.search_citations.citation_metadata == CapabilityStatus::Supported {
        keys.push("search_citations.citation_metadata");
    }
    if fs.search_citations.grounding_metadata == CapabilityStatus::Supported {
        keys.push("search_citations.grounding_metadata");
    }
    if fs.modalities.vision_input == CapabilityStatus::Supported {
        keys.push("modalities.vision_input");
    }
    if fs.modalities.audio_input == CapabilityStatus::Supported {
        keys.push("modalities.audio_input");
    }
    if fs.modalities.audio_output == CapabilityStatus::Supported {
        keys.push("modalities.audio_output");
    }
    if fs.modalities.embeddings == CapabilityStatus::Supported {
        keys.push("modalities.embeddings");
    }
    if fs.modalities.rerank == CapabilityStatus::Supported {
        keys.push("modalities.rerank");
    }
    if fs.modalities.moderation == CapabilityStatus::Supported {
        keys.push("modalities.moderation");
    }
    if fs.routing.provider_routing == CapabilityStatus::Supported {
        keys.push("routing.provider_routing");
    }
    if fs.routing.require_parameters == CapabilityStatus::Supported {
        keys.push("routing.require_parameters");
    }
    if fs.routing.fallback_policy == CapabilityStatus::Supported {
        keys.push("routing.fallback_policy");
    }
    if fs.state.previous_response_id == CapabilityStatus::Supported {
        keys.push("state.previous_response_id");
    }
    if fs.state.response_phase == CapabilityStatus::Supported {
        keys.push("state.response_phase");
    }
    if fs.state.prompt_caching == CapabilityStatus::Supported {
        keys.push("state.prompt_caching");
    }
    if fs.state.context_caching == CapabilityStatus::Supported {
        keys.push("state.context_caching");
    }
    if fs.logprobs.logprobs == CapabilityStatus::Supported {
        keys.push("logprobs.logprobs");
    }
    if fs.logprobs.streaming_logprobs == CapabilityStatus::Supported {
        keys.push("logprobs.streaming_logprobs");
    }
    keys
}

const NON_CANDIDATE_PROVIDER_IDS: &[&str] = &[
    "openai",
    "anthropic",
    "mistral",
    "groq",
    "xai",
    "perplexity",
    "together",
    "openrouter",
    "ollama",
    "lmstudio",
    "fireworks",
    "mock",
    "loopback",
];

#[test]
fn supported_features_require_fresh_official_evidence() {
    // Walk every non-candidate registry record. Each must declare a non-empty
    // `last_reviewed_on`, and every feature flagged `Supported` must have an
    // evidence entry (keyed exactly or by group prefix) with a non-empty
    // `source_url` plus at least one proof slot.
    let recs = registry::all_records();
    let mut at_least_one_supported_seen = false;
    for id in NON_CANDIDATE_PROVIDER_IDS {
        let rec = recs
            .iter()
            .find(|r| r.provider_id == *id)
            .expect("registry missing required non-candidate provider");

        assert!(
            !rec.last_reviewed_on.is_empty(),
            "{id}: last_reviewed_on must be non-empty"
        );

        for feature_key in supported_feature_keys_for(&rec.features) {
            at_least_one_supported_seen = true;
            let group = feature_key.split('.').next().unwrap_or(feature_key);
            let evidence = rec
                .evidence
                .get(feature_key)
                .or_else(|| rec.evidence.get(group))
                .expect("supported feature must have matching evidence entry");
            assert!(
                evidence
                    .source_url
                    .as_deref()
                    .is_some_and(|s| !s.is_empty()),
                "{id}/{feature_key}: evidence must have a non-empty source_url"
            );
            evidence
                .is_evidence_for_supported()
                .expect("evidence for Supported feature must be complete");
        }
    }

    // Sanity guard: at least one non-candidate provider must declare a
    // Supported feature once the registry is evidence-backed; otherwise the
    // walk above is vacuously green and offers no real protection. This
    // assertion fails red until T020+ fills in actual provider records.
    assert!(
        at_least_one_supported_seen,
        "no non-candidate provider records expose any Supported feature yet — \
         T018 cannot be vacuously green"
    );
}

// ---------------------------------------------------------------------------
// T019 [US1]: recognized / provider_specific must NOT collapse into legacy
// boolean `true` via derive_legacy_capabilities. Only `Supported` should
// flip a legacy bool on.
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Phase 3 no-overclaim guard (Codex remediation): locks the Phase 3 status
// posture for features that are documented but not yet wired through their
// adapter request structs. These should remain Recognized (or Unsupported)
// until Phase 4/5 ships the typed carriers — bumping any of them to
// Supported requires updating both the adapter and this guard test.
// ---------------------------------------------------------------------------
#[test]
fn phase3_does_not_overclaim_unwired_request_features() {
    fn rec(id: &str) -> ProviderCapabilityRecord {
        registry::find(id).expect("registry record must exist")
    }

    fn assert_not_supported(provider: &str, feature: &str, status: CapabilityStatus) {
        assert_ne!(
            status,
            CapabilityStatus::Supported,
            "{provider}: {feature} must not be Supported until adapter request \
             serialization lands"
        );
    }

    // OpenAI: T035 (Phase 4) wired typed StructuredOutputConfig and
    // ToolCallConfig through OpenAIRequest, proven by the four
    // `test_build_request_serializes_typed_*` adapter tests in
    // src/providers/openai.rs. Those surfaces are now Supported;
    // streaming_tool_calls stays not-Supported because typed
    // ToolCallDelta stream-decoding is not yet wired.
    let openai = rec("openai");
    let so = &openai.features.structured_output;
    assert_eq!(
        so.json_mode,
        CapabilityStatus::Supported,
        "openai: json_mode must be Supported (T035 wired response_format json_object)"
    );
    assert_eq!(
        so.json_schema_strict,
        CapabilityStatus::Supported,
        "openai: json_schema_strict must be Supported (T035 wired strict response_format)"
    );
    assert_eq!(
        so.json_schema_best_effort,
        CapabilityStatus::Supported,
        "openai: json_schema_best_effort must be Supported"
    );
    assert_eq!(
        so.named_schemas,
        CapabilityStatus::Supported,
        "openai: named_schemas must be Supported (T035 wired schema_name)"
    );
    assert_eq!(
        so.additionalprops_false,
        CapabilityStatus::Supported,
        "openai: additionalprops_false must be Supported (carried verbatim through schema)"
    );
    let tc = &openai.features.tool_calling;
    assert_eq!(
        tc.function_calling,
        CapabilityStatus::Supported,
        "openai: function_calling must be Supported (T035 wired tools + tool_choice)"
    );
    assert_eq!(
        tc.parallel_tool_calls,
        CapabilityStatus::Supported,
        "openai: parallel_tool_calls must be Supported (T035 wired the field)"
    );
    // streaming_tool_calls stays not-Supported until typed ToolCallDelta
    // stream-decoding lands.
    assert_not_supported(
        "openai",
        "tool_calling.streaming_tool_calls",
        tc.streaming_tool_calls,
    );
    // Surfaces that ARE wired today MUST stay Supported.
    assert_eq!(
        openai.features.reasoning.effort_control,
        CapabilityStatus::Supported,
        "openai: reasoning.effort_control regression"
    );
    assert_eq!(
        openai.features.modalities.vision_input,
        CapabilityStatus::Supported,
        "openai: modalities.vision_input regression"
    );
    assert_eq!(
        openai.features.logprobs.logprobs,
        CapabilityStatus::Supported,
        "openai: logprobs regression"
    );
    assert_eq!(
        openai.features.logprobs.streaming_logprobs,
        CapabilityStatus::Supported,
        "openai: streaming logprobs regression"
    );

    // Anthropic: tool_calling and reasoning.thinking_blocks lack request-side
    // serialization in ClaudeRequest. Vision input remains Supported.
    let anthropic = rec("anthropic");
    let atc = &anthropic.features.tool_calling;
    assert_not_supported(
        "anthropic",
        "tool_calling.function_calling",
        atc.function_calling,
    );
    assert_not_supported(
        "anthropic",
        "tool_calling.parallel_tool_calls",
        atc.parallel_tool_calls,
    );
    assert_not_supported(
        "anthropic",
        "tool_calling.streaming_tool_calls",
        atc.streaming_tool_calls,
    );
    assert_not_supported(
        "anthropic",
        "reasoning.thinking_blocks",
        anthropic.features.reasoning.thinking_blocks,
    );
    assert_eq!(
        anthropic.features.modalities.vision_input,
        CapabilityStatus::Supported,
        "anthropic: vision_input regression"
    );

    // Mistral: T036 (Phase 4) wired typed StructuredOutputConfig and
    // ToolCallConfig through MistralRequest, proven by the four
    // `test_build_request_serializes_typed_*` adapter tests in
    // src/providers/mistral.rs. Streaming tool-call decoding stays
    // not-Supported because typed ToolCallDelta is not yet wired.
    let mistral = rec("mistral");
    let m_so = &mistral.features.structured_output;
    assert_eq!(
        m_so.json_mode,
        CapabilityStatus::Supported,
        "mistral: json_mode"
    );
    assert_eq!(
        m_so.json_schema_strict,
        CapabilityStatus::Supported,
        "mistral: json_schema_strict (T036 wired strict response_format)"
    );
    assert_eq!(
        m_so.json_schema_best_effort,
        CapabilityStatus::Supported,
        "mistral: json_schema_best_effort"
    );
    assert_eq!(
        m_so.named_schemas,
        CapabilityStatus::Supported,
        "mistral: named_schemas (T036 wired schema_name)"
    );
    assert_eq!(
        m_so.additionalprops_false,
        CapabilityStatus::Supported,
        "mistral: additionalprops_false"
    );
    let m_tc = &mistral.features.tool_calling;
    assert_eq!(
        m_tc.function_calling,
        CapabilityStatus::Supported,
        "mistral: function_calling (T036 wired tools + tool_choice)"
    );
    assert_eq!(
        m_tc.parallel_tool_calls,
        CapabilityStatus::Supported,
        "mistral: parallel_tool_calls (T036 wired the field)"
    );
    assert_not_supported(
        "mistral",
        "tool_calling.streaming_tool_calls",
        m_tc.streaming_tool_calls,
    );

    // Groq: T037 wired typed StructuredOutputConfig and ToolCallConfig
    // through GroqRequest. Streaming tool-call decoding stays not-Supported.
    let groq = rec("groq");
    let g_so = &groq.features.structured_output;
    assert_eq!(
        g_so.json_mode,
        CapabilityStatus::Supported,
        "groq: json_mode"
    );
    assert_eq!(
        g_so.json_schema_strict,
        CapabilityStatus::Supported,
        "groq: json_schema_strict (T037 wired strict response_format)"
    );
    assert_eq!(
        g_so.json_schema_best_effort,
        CapabilityStatus::Supported,
        "groq: json_schema_best_effort"
    );
    assert_eq!(
        g_so.named_schemas,
        CapabilityStatus::Supported,
        "groq: named_schemas"
    );
    assert_eq!(
        g_so.additionalprops_false,
        CapabilityStatus::Supported,
        "groq: additionalprops_false"
    );
    let g_tc = &groq.features.tool_calling;
    assert_eq!(
        g_tc.function_calling,
        CapabilityStatus::Supported,
        "groq: function_calling"
    );
    assert_eq!(
        g_tc.parallel_tool_calls,
        CapabilityStatus::Supported,
        "groq: parallel_tool_calls"
    );
    assert_not_supported(
        "groq",
        "tool_calling.streaming_tool_calls",
        g_tc.streaming_tool_calls,
    );

    // Together: T038 wired typed StructuredOutputConfig and ToolCallConfig
    // through TogetherRequest. Streaming tool-call decoding stays not-Supported.
    let together = rec("together");
    let t_so = &together.features.structured_output;
    assert_eq!(
        t_so.json_mode,
        CapabilityStatus::Supported,
        "together: json_mode"
    );
    assert_eq!(
        t_so.json_schema_strict,
        CapabilityStatus::Supported,
        "together: json_schema_strict"
    );
    assert_eq!(
        t_so.json_schema_best_effort,
        CapabilityStatus::Supported,
        "together: json_schema_best_effort"
    );
    assert_eq!(
        t_so.named_schemas,
        CapabilityStatus::Supported,
        "together: named_schemas"
    );
    assert_eq!(
        t_so.additionalprops_false,
        CapabilityStatus::Supported,
        "together: additionalprops_false"
    );
    let t_tc = &together.features.tool_calling;
    assert_eq!(
        t_tc.function_calling,
        CapabilityStatus::Supported,
        "together: function_calling"
    );
    assert_eq!(
        t_tc.parallel_tool_calls,
        CapabilityStatus::Supported,
        "together: parallel_tool_calls"
    );
    assert_not_supported(
        "together",
        "tool_calling.streaming_tool_calls",
        t_tc.streaming_tool_calls,
    );
    // Together's modality evidence cites the with-vision adapter test.
    let modality_ev = together
        .evidence
        .get("modalities")
        .expect("together: modalities evidence");
    let adapter_test = modality_ev
        .adapter_test
        .as_deref()
        .expect("together: modality adapter_test must be set");
    assert!(
        adapter_test.ends_with("test_message_conversion_with_vision"),
        "together: modality adapter_test must reference \
         test_message_conversion_with_vision (got: {adapter_test})"
    );
}

#[test]
fn recognized_and_provider_specific_do_not_project_to_legacy_true() {
    let mut fs = CapabilityFeatureSet::default();
    fs.structured_output.json_schema_strict = CapabilityStatus::Recognized;
    fs.structured_output.json_mode = CapabilityStatus::ProviderSpecific;
    fs.modalities.vision_input = CapabilityStatus::Recognized;
    fs.logprobs.logprobs = CapabilityStatus::ProviderSpecific;
    fs.logprobs.streaming_logprobs = CapabilityStatus::ProviderSpecific;

    let legacy = derive_legacy_capabilities(&fs);
    assert!(
        !legacy.supports_json_schema,
        "Recognized must not become true"
    );
    assert!(
        !legacy.supports_json_mode,
        "ProviderSpecific must not become true"
    );
    assert!(!legacy.supports_vision, "Recognized must not become true");
    assert!(
        !legacy.supports_logprobs,
        "ProviderSpecific must not become true"
    );
    assert!(
        !legacy.supports_streaming_logprobs,
        "ProviderSpecific must not become true"
    );
}

#[test]
fn openai_frontier_model_overrides_are_declared() {
    let rec = registry::find("openai").expect("openai record");
    for model in ["gpt-5.5", "gpt-5.5-2026-04-23", "gpt-5.4", "gpt-5.4-mini"] {
        let ovr = rec
            .model_overrides
            .get(model)
            .expect("openai record must include expected model override");
        assert!(
            !ovr.reviewed_on.is_empty(),
            "openai/{model}: reviewed_on must be non-empty"
        );
    }
}

#[test]
fn openai_gpt55_frontier_metadata_has_source_evidence() {
    let rec = registry::find("openai").expect("openai record");
    assert_eq!(
        rec.default_model.as_deref(),
        Some("gpt-5.5"),
        "OpenAI default model must track the current documented frontier"
    );

    let frontier = rec
        .model_overrides
        .get("gpt-5.5")
        .expect("gpt-5.5 model override");
    assert!(
        frontier.notes.iter().any(|note| note.contains("frontier")),
        "gpt-5.5 override must document its frontier role"
    );

    let model_evidence = rec.evidence.get("models").expect("OpenAI model evidence");
    assert!(
        model_evidence
            .source_url
            .as_deref()
            .is_some_and(|url| url == "https://platform.openai.com/docs/models"),
        "OpenAI GPT-5.5 metadata must cite the official models documentation"
    );
    model_evidence
        .is_evidence_for_supported()
        .expect("OpenAI model metadata evidence must include a proof reference");
}

// ---------------------------------------------------------------------------
// T029 [US2]: StructuredOutputConfig invariants —
//   1. JsonSchema mode requires a non-null, non-empty `schema`.
//   2. `strict = Some(true)` against a non-Supported strict capability must
//      Block before any network I/O (currently returns Warn — failure proves
//      the gap before T034/T039 implementation).
//   3. JsonObject mode against a Recognized provider must surface the
//      provider's evidence note in the warning, not just a generic message.
// ---------------------------------------------------------------------------

fn make_record_with_strict(strict_status: CapabilityStatus) -> ProviderCapabilityRecord {
    let mut fs = CapabilityFeatureSet::default();
    fs.structured_output.json_schema_strict = strict_status;
    ProviderCapabilityRecord {
        provider_id: "test".into(),
        display_name: "Test".into(),
        last_reviewed_on: "2026-05-09".into(),
        default_model: None,
        features: fs,
        evidence: HashMap::new(),
        model_overrides: HashMap::new(),
        provider_specific: serde_json::Value::Null,
    }
}

fn make_record_with_json_mode(
    json_mode_status: CapabilityStatus,
    evidence_note: Option<&str>,
) -> ProviderCapabilityRecord {
    let mut fs = CapabilityFeatureSet::default();
    fs.structured_output.json_mode = json_mode_status;
    let mut ev = HashMap::new();
    if let Some(note) = evidence_note {
        ev.insert(
            "structured_output".into(),
            CapabilityEvidence {
                source_url: Some("https://example.com/docs".into()),
                source_reviewed_on: "2026-05-09".into(),
                adapter_test: None,
                fixture_path: None,
                live_test: None,
                notes: vec![note.into()],
            },
        );
    }
    ProviderCapabilityRecord {
        provider_id: "test".into(),
        display_name: "Test".into(),
        last_reviewed_on: "2026-05-09".into(),
        default_model: None,
        features: fs,
        evidence: ev,
        model_overrides: HashMap::new(),
        provider_specific: serde_json::Value::Null,
    }
}

#[test]
fn json_schema_mode_requires_schema() {
    let no_schema = StructuredOutputConfig {
        mode: StructuredOutputMode::JsonSchema,
        schema: None,
        schema_name: Some("country_capital".into()),
        strict: Some(true),
        schema_subset: None,
    };
    assert_eq!(
        no_schema.validate_shape(),
        Err(StructuredOutputError::JsonSchemaModeMissingSchema),
        "JsonSchema mode without schema must fail validate_shape"
    );

    let empty_schema = StructuredOutputConfig {
        mode: StructuredOutputMode::JsonSchema,
        schema: Some(serde_json::Value::Object(serde_json::Map::new())),
        schema_name: Some("country_capital".into()),
        strict: Some(true),
        schema_subset: None,
    };
    assert_eq!(
        empty_schema.validate_shape(),
        Err(StructuredOutputError::JsonSchemaModeMissingSchema),
        "JsonSchema mode with empty schema must fail validate_shape"
    );

    let with_schema = StructuredOutputConfig {
        mode: StructuredOutputMode::JsonSchema,
        schema: Some(serde_json::json!({"type": "object"})),
        schema_name: Some("country_capital".into()),
        strict: Some(true),
        schema_subset: None,
    };
    assert!(
        with_schema.validate_shape().is_ok(),
        "JsonSchema mode with non-empty schema must pass validate_shape"
    );

    // JsonObject and Text modes do not require schema.
    let json_object = StructuredOutputConfig {
        mode: StructuredOutputMode::JsonObject,
        schema: None,
        schema_name: None,
        strict: None,
        schema_subset: None,
    };
    assert!(json_object.validate_shape().is_ok());

    let text = StructuredOutputConfig {
        mode: StructuredOutputMode::Text,
        schema: None,
        schema_name: None,
        strict: None,
        schema_subset: None,
    };
    assert!(text.validate_shape().is_ok());
}

#[test]
fn strict_mode_requires_supported_capability() {
    let cfg = StructuredOutputConfig {
        mode: StructuredOutputMode::JsonSchema,
        schema: Some(serde_json::json!({"type": "object"})),
        schema_name: Some("country_capital".into()),
        strict: Some(true),
        schema_subset: None,
    };

    // Strict against Recognized → Block (not Warn).
    let recognized_rec = make_record_with_strict(CapabilityStatus::Recognized);
    let outcome = validate_request_against_record(
        &recognized_rec,
        &FeatureRequest::structured_output(cfg.clone()),
    );
    assert!(
        matches!(outcome, ValidationOutcome::Block { .. }),
        "strict=true against Recognized json_schema_strict must Block, got {outcome:?}"
    );
    assert_eq!(outcome.severity(), ValidationSeverity::Error);

    // Strict against Unknown → Block.
    let unknown_rec = make_record_with_strict(CapabilityStatus::Unknown);
    let outcome = validate_request_against_record(
        &unknown_rec,
        &FeatureRequest::structured_output(cfg.clone()),
    );
    assert!(
        matches!(outcome, ValidationOutcome::Block { .. }),
        "strict=true against Unknown json_schema_strict must Block, got {outcome:?}"
    );

    // Strict against Supported → Allow (already true; locked here).
    let supported_rec = make_record_with_strict(CapabilityStatus::Supported);
    let outcome =
        validate_request_against_record(&supported_rec, &FeatureRequest::structured_output(cfg));
    assert!(
        matches!(outcome, ValidationOutcome::Allow),
        "strict=true against Supported json_schema_strict must Allow, got {outcome:?}"
    );
}

#[test]
fn json_object_mode_preserves_provider_warning() {
    let cfg = StructuredOutputConfig {
        mode: StructuredOutputMode::JsonObject,
        schema: None,
        schema_name: None,
        strict: None,
        schema_subset: None,
    };

    let provider_note = "Mistral requires a JSON instruction in the prompt for json_object mode.";
    let rec = make_record_with_json_mode(CapabilityStatus::Recognized, Some(provider_note));

    let outcome = validate_request_against_record(&rec, &FeatureRequest::structured_output(cfg));
    assert!(
        matches!(outcome, ValidationOutcome::Warn { .. }),
        "json_object against Recognized must Warn (carrying provider note); \
         got {outcome:?}"
    );
    if let ValidationOutcome::Warn { reason, .. } = outcome {
        assert!(
            reason.contains(provider_note),
            "json_object Warn against Recognized must surface the \
             provider's evidence note. Got: {reason}"
        );
    }
}

// ---------------------------------------------------------------------------
// T030 [US2]: ToolCallConfig invariants —
//   1. Pure shape: empty tool names and tools-with-None-choice fail
//      validate_shape pre-network.
//   2. Per-tool capability gating: a tool whose name matches a hosted-tool
//      capability whose status is Unsupported/Future must Block, even when
//      function_calling itself is Supported.
//   3. Provider-specific hosted tools require namespacing: a bare
//      "web_search" against an Anthropic-shaped record (web_search status
//      = ProviderSpecific) must Block; "anthropic.web_search" must Allow
//      (or at worst Warn, but never silently accept the bare name).
// ---------------------------------------------------------------------------

fn tool(name: &str) -> ToolDefinition {
    ToolDefinition {
        name: name.into(),
        description: None,
        parameters: serde_json::json!({"type": "object"}),
        strict: None,
    }
}

fn record_with_function_calling_and_hosted(
    function_calling: CapabilityStatus,
    web_search: CapabilityStatus,
) -> ProviderCapabilityRecord {
    let mut fs = CapabilityFeatureSet::default();
    fs.tool_calling.function_calling = function_calling;
    fs.hosted_tools.web_search = web_search;
    ProviderCapabilityRecord {
        provider_id: "anthropic".into(),
        display_name: "Anthropic Claude".into(),
        last_reviewed_on: "2026-05-09".into(),
        default_model: None,
        features: fs,
        evidence: HashMap::new(),
        model_overrides: HashMap::new(),
        provider_specific: serde_json::Value::Null,
    }
}

#[test]
fn tool_call_config_validate_shape_rejects_empty_name() {
    let cfg = ToolCallConfig {
        tools: vec![tool(""), tool("ok")],
        tool_choice: ToolChoice::Auto,
        parallel_tool_calls: None,
        streaming_tool_calls: None,
    };
    assert_eq!(
        cfg.validate_shape(),
        Err(ToolCallError::EmptyToolName),
        "validate_shape must reject empty tool names"
    );

    let ok = ToolCallConfig {
        tools: vec![tool("search_inventory")],
        tool_choice: ToolChoice::Auto,
        parallel_tool_calls: None,
        streaming_tool_calls: None,
    };
    assert!(ok.validate_shape().is_ok());
}

#[test]
fn tool_call_config_validate_shape_rejects_tools_with_none_choice() {
    let cfg = ToolCallConfig {
        tools: vec![tool("search_inventory")],
        tool_choice: ToolChoice::None,
        parallel_tool_calls: None,
        streaming_tool_calls: None,
    };
    assert_eq!(
        cfg.validate_shape(),
        Err(ToolCallError::ToolsProvidedButChoiceIsNone),
        "tools provided with tool_choice=None must fail validate_shape"
    );

    // tool_choice=None is fine when there are no tools.
    let ok_empty = ToolCallConfig {
        tools: vec![],
        tool_choice: ToolChoice::None,
        parallel_tool_calls: None,
        streaming_tool_calls: None,
    };
    assert!(ok_empty.validate_shape().is_ok());
}

#[test]
fn unsupported_hosted_tool_blocks_pre_network() {
    // function_calling=Supported, but web_search hosted tool is Unsupported.
    // A request that includes a bare "web_search" tool name must Block.
    let rec = record_with_function_calling_and_hosted(
        CapabilityStatus::Supported,
        CapabilityStatus::Unsupported,
    );
    let cfg = ToolCallConfig {
        tools: vec![tool("web_search")],
        tool_choice: ToolChoice::Auto,
        parallel_tool_calls: None,
        streaming_tool_calls: None,
    };
    let outcome = validate_request_against_record(&rec, &FeatureRequest::tool_call(cfg));
    assert!(
        matches!(outcome, ValidationOutcome::Block { .. }),
        "tool referencing an Unsupported hosted-tool capability must Block, \
         got {outcome:?}"
    );
    assert_eq!(outcome.severity(), ValidationSeverity::Error);
}

#[test]
fn provider_specific_hosted_tool_requires_namespacing() {
    // web_search is ProviderSpecific. Bare name → Block; namespaced name → Allow.
    let rec = record_with_function_calling_and_hosted(
        CapabilityStatus::Supported,
        CapabilityStatus::ProviderSpecific,
    );

    let bare = ToolCallConfig {
        tools: vec![tool("web_search")],
        tool_choice: ToolChoice::Auto,
        parallel_tool_calls: None,
        streaming_tool_calls: None,
    };
    let outcome = validate_request_against_record(&rec, &FeatureRequest::tool_call(bare));
    assert!(
        matches!(outcome, ValidationOutcome::Block { .. }),
        "bare provider-specific hosted-tool name must Block (require namespace), \
         got {outcome:?}"
    );

    let namespaced = ToolCallConfig {
        tools: vec![tool("anthropic.web_search")],
        tool_choice: ToolChoice::Auto,
        parallel_tool_calls: None,
        streaming_tool_calls: None,
    };
    let outcome = validate_request_against_record(&rec, &FeatureRequest::tool_call(namespaced));
    assert!(
        !matches!(outcome, ValidationOutcome::Block { .. }),
        "namespaced provider-specific hosted-tool name must not Block, \
         got {outcome:?}"
    );
}

// ---------------------------------------------------------------------------
// T031 [US2]: OpenAI-compatible request JSON wire-shape tests for
// structured output and function/tool definitions.
//
// The helpers in `nxuskit_engine::capabilities::openai_wire` produce the
// JSON values that an OpenAI / OpenAI-compatible adapter (OpenAI, Mistral,
// Groq, Together, OpenRouter, Fireworks) splices into the outgoing
// request body. T035 implements the mappings; these tests pin the
// expected shapes per the OpenAI Chat Completions reference.
// ---------------------------------------------------------------------------

use nxuskit_engine::capabilities::openai_wire;

#[test]
fn openai_wire_response_format_json_schema() {
    let cfg = StructuredOutputConfig {
        mode: StructuredOutputMode::JsonSchema,
        schema: Some(serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["country"],
            "properties": {"country": {"type": "string"}}
        })),
        schema_name: Some("country_capital".into()),
        strict: Some(true),
        schema_subset: None,
    };
    let got = openai_wire::response_format(&cfg);
    let want = serde_json::json!({
        "type": "json_schema",
        "json_schema": {
            "name": "country_capital",
            "strict": true,
            "schema": {
                "type": "object",
                "additionalProperties": false,
                "required": ["country"],
                "properties": {"country": {"type": "string"}}
            }
        }
    });
    assert_eq!(got, want, "OpenAI json_schema response_format wire shape");
}

#[test]
fn openai_wire_response_format_json_object() {
    let cfg = StructuredOutputConfig {
        mode: StructuredOutputMode::JsonObject,
        schema: None,
        schema_name: None,
        strict: None,
        schema_subset: None,
    };
    let got = openai_wire::response_format(&cfg);
    let want = serde_json::json!({"type": "json_object"});
    assert_eq!(got, want, "OpenAI json_object response_format wire shape");
}

#[test]
fn openai_wire_response_format_text_is_null() {
    let cfg = StructuredOutputConfig {
        mode: StructuredOutputMode::Text,
        schema: None,
        schema_name: None,
        strict: None,
        schema_subset: None,
    };
    let got = openai_wire::response_format(&cfg);
    assert_eq!(
        got,
        serde_json::Value::Null,
        "Text mode must omit response_format entirely (Null sentinel)"
    );
}

#[test]
fn openai_wire_tools_function_definitions() {
    let cfg = ToolCallConfig {
        tools: vec![
            ToolDefinition {
                name: "search_inventory".into(),
                description: Some("Search the warehouse inventory.".into()),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {"sku": {"type": "string"}},
                    "required": ["sku"],
                }),
                strict: Some(true),
            },
            ToolDefinition {
                name: "lookup_user".into(),
                description: None,
                parameters: serde_json::json!({"type": "object"}),
                strict: None,
            },
        ],
        tool_choice: ToolChoice::Auto,
        parallel_tool_calls: None,
        streaming_tool_calls: None,
    };
    let got = openai_wire::tools(&cfg);
    let want = serde_json::json!([
        {
            "type": "function",
            "function": {
                "name": "search_inventory",
                "description": "Search the warehouse inventory.",
                "parameters": {
                    "type": "object",
                    "properties": {"sku": {"type": "string"}},
                    "required": ["sku"],
                },
                "strict": true
            }
        },
        {
            "type": "function",
            "function": {
                "name": "lookup_user",
                "parameters": {"type": "object"}
            }
        }
    ]);
    assert_eq!(got, want, "OpenAI tools wire shape");
}

#[test]
fn openai_wire_tool_choice_modes() {
    fn cfg(choice: ToolChoice) -> ToolCallConfig {
        ToolCallConfig {
            tools: vec![tool("search_inventory")],
            tool_choice: choice,
            parallel_tool_calls: None,
            streaming_tool_calls: None,
        }
    }
    assert_eq!(
        openai_wire::tool_choice(&cfg(ToolChoice::Auto)),
        serde_json::json!("auto")
    );
    assert_eq!(
        openai_wire::tool_choice(&cfg(ToolChoice::None)),
        serde_json::json!("none")
    );
    assert_eq!(
        openai_wire::tool_choice(&cfg(ToolChoice::Required)),
        serde_json::json!("required")
    );
    assert_eq!(
        openai_wire::tool_choice(&cfg(ToolChoice::Named("search_inventory".into()))),
        serde_json::json!({
            "type": "function",
            "function": {"name": "search_inventory"}
        })
    );
}

// ---------------------------------------------------------------------------
// T032 [US2]: shared-fixture semantic tests — JSON Schema structured
// output across Groq, Together, and Mistral (OD-1 proof point).
//
// Loads the single CE-safe fixture
// internal/tests/parity/provider_capabilities/fixtures/json-schema-minimal.json,
// builds one StructuredOutputConfig, and asserts:
//   1. The OpenAI-compatible `response_format` wire JSON is identical
//      across the three providers (because all three share the OpenAI-
//      compatible request shape).
//   2. Validating that config against the groq, together, and mistral
//      registry records produces the SAME validation outcome class
//      (semantic parity — they should agree on Allow/Warn/Block, not
//      diverge per-provider).
// ---------------------------------------------------------------------------

const OD1_FIXTURE_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../internal/tests/parity/provider_capabilities/fixtures/json-schema-minimal.json"
);

fn load_od1_structured_output() -> StructuredOutputConfig {
    let raw = std::fs::read_to_string(OD1_FIXTURE_PATH).expect("read OD-1 fixture");
    let parsed: serde_json::Value = serde_json::from_str(&raw).expect("OD-1 fixture parses");
    StructuredOutputConfig {
        mode: StructuredOutputMode::JsonSchema,
        schema: parsed.get("schema").cloned(),
        schema_name: parsed
            .get("name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        strict: parsed.get("strict").and_then(|v| v.as_bool()),
        schema_subset: None,
    }
}

fn outcome_class(o: &ValidationOutcome) -> &'static str {
    match o {
        ValidationOutcome::Allow => "allow",
        ValidationOutcome::Warn { .. } => "warn",
        ValidationOutcome::Block { .. } => "block",
    }
}

#[test]
fn od1_fixture_response_format_is_identical_across_groq_together_mistral() {
    let cfg = load_od1_structured_output();
    let groq_wire = openai_wire::response_format(&cfg);
    let together_wire = openai_wire::response_format(&cfg);
    let mistral_wire = openai_wire::response_format(&cfg);

    let want = serde_json::json!({
        "type": "json_schema",
        "json_schema": {
            "name": "country_capital",
            "strict": true,
            "schema": {
                "type": "object",
                "additionalProperties": false,
                "required": ["country", "capital", "population_estimate"],
                "properties": {
                    "country": {
                        "type": "string",
                        "description": "ISO 3166-1 alpha-2 country code (e.g. 'FR')."
                    },
                    "capital": {
                        "type": "string",
                        "description": "Capital city name in English."
                    },
                    "population_estimate": {
                        "type": "integer",
                        "minimum": 0,
                        "description": "Approximate population of the capital city."
                    }
                }
            }
        }
    });

    assert_eq!(groq_wire, want, "groq response_format wire shape mismatch");
    assert_eq!(
        together_wire, want,
        "together response_format wire shape mismatch"
    );
    assert_eq!(
        mistral_wire, want,
        "mistral response_format wire shape mismatch"
    );
    assert_eq!(groq_wire, together_wire, "groq vs together divergence");
    assert_eq!(groq_wire, mistral_wire, "groq vs mistral divergence");
}

#[test]
fn od1_fixture_validates_consistently_across_groq_together_mistral() {
    let cfg = load_od1_structured_output();
    let req = FeatureRequest::structured_output(cfg);

    let groq = registry::find("groq").expect("groq record");
    let together = registry::find("together").expect("together record");
    let mistral = registry::find("mistral").expect("mistral record");

    let groq_outcome = validate_request_against_record(&groq, &req);
    let together_outcome = validate_request_against_record(&together, &req);
    let mistral_outcome = validate_request_against_record(&mistral, &req);

    let g = outcome_class(&groq_outcome);
    let t = outcome_class(&together_outcome);
    let m = outcome_class(&mistral_outcome);

    assert_eq!(
        g, t,
        "groq ({g:?}) vs together ({t:?}) outcome class divergence on OD-1 fixture"
    );
    assert_eq!(
        g, m,
        "groq ({g:?}) vs mistral ({m:?}) outcome class divergence on OD-1 fixture"
    );

    // T036-T038 wire the OD-1 JSON Schema request path for all three
    // providers, so the shared strict fixture now validates as supported.
    assert_eq!(
        g, "allow",
        "OD-1 strict=true must Allow after T036-T038 adapter wiring; \
         got class={g} (groq), {t} (together), {m} (mistral)"
    );
}

// ---------------------------------------------------------------------------
// T033 [US2]: negative tests — recognized-only non-OpenAI hosted /
// server-side tools must NOT be serialized as first-class OpenAI
// `tools[]` entries (OD-2).
//
// Phase 3 leaves Anthropic web_search / code_interpreter / mcp_connector
// at Recognized; Mistral web_search / code_interpreter at Recognized;
// Groq web_search at Recognized. A request that mixes these hosted-tool
// names with a regular function tool must produce a `tools` array that
// includes ONLY the regular function tool — the hosted-tool entries are
// metadata, not first-class request fields, until a later sprint
// promotes them with explicit entitlement.
// ---------------------------------------------------------------------------

#[test]
fn anthropic_recognized_hosted_tools_are_not_serialized_first_class() {
    let rec = registry::find("anthropic").expect("anthropic record");
    // Sanity-check the Phase 3 posture this test depends on.
    assert_eq!(
        rec.features.hosted_tools.web_search,
        CapabilityStatus::Recognized
    );
    assert_eq!(
        rec.features.hosted_tools.code_interpreter,
        CapabilityStatus::Recognized
    );
    assert_eq!(
        rec.features.hosted_tools.mcp_connector,
        CapabilityStatus::Recognized
    );

    let cfg = ToolCallConfig {
        tools: vec![
            tool("web_search"),
            tool("code_interpreter"),
            tool("mcp_connector"),
            tool("search_inventory"),
        ],
        tool_choice: ToolChoice::Auto,
        parallel_tool_calls: None,
        streaming_tool_calls: None,
    };

    let got = openai_wire::tools_for(&rec, &cfg);
    let want = serde_json::json!([
        {
            "type": "function",
            "function": {
                "name": "search_inventory",
                "parameters": {"type": "object"}
            }
        }
    ]);
    assert_eq!(
        got, want,
        "OD-2: Recognized non-OpenAI hosted tools must be filtered out of \
         the OpenAI tools[] array; only plain function tools survive"
    );
}

#[test]
fn mistral_recognized_hosted_tools_are_not_serialized_first_class() {
    let rec = registry::find("mistral").expect("mistral record");
    assert_eq!(
        rec.features.hosted_tools.web_search,
        CapabilityStatus::Recognized
    );
    assert_eq!(
        rec.features.hosted_tools.code_interpreter,
        CapabilityStatus::Recognized
    );

    let cfg = ToolCallConfig {
        tools: vec![tool("web_search"), tool("lookup_user")],
        tool_choice: ToolChoice::Auto,
        parallel_tool_calls: None,
        streaming_tool_calls: None,
    };
    let got = openai_wire::tools_for(&rec, &cfg);
    let want = serde_json::json!([
        {
            "type": "function",
            "function": {
                "name": "lookup_user",
                "parameters": {"type": "object"}
            }
        }
    ]);
    assert_eq!(
        got, want,
        "OD-2: Mistral Recognized hosted tools must be filtered out"
    );
}

#[test]
fn groq_recognized_compound_web_search_is_not_serialized_first_class() {
    let rec = registry::find("groq").expect("groq record");
    assert_eq!(
        rec.features.hosted_tools.web_search,
        CapabilityStatus::Recognized
    );

    let cfg = ToolCallConfig {
        tools: vec![tool("web_search"), tool("search_inventory")],
        tool_choice: ToolChoice::Auto,
        parallel_tool_calls: None,
        streaming_tool_calls: None,
    };
    let got = openai_wire::tools_for(&rec, &cfg);
    let want = serde_json::json!([
        {
            "type": "function",
            "function": {
                "name": "search_inventory",
                "parameters": {"type": "object"}
            }
        }
    ]);
    assert_eq!(
        got, want,
        "OD-2: Groq Compound web_search (Recognized) must be filtered out"
    );
}

#[test]
fn empty_tools_array_when_only_recognized_hosted_tools_supplied() {
    // If every tool in the request maps to a Recognized hosted-tool
    // capability, the OpenAI tools[] field must be an empty array (or
    // omitted via Null), never a mixed array that would silently leak
    // unverified hosted-tool calls onto the wire.
    let rec = registry::find("anthropic").expect("anthropic record");
    let cfg = ToolCallConfig {
        tools: vec![tool("web_search"), tool("code_interpreter")],
        tool_choice: ToolChoice::Auto,
        parallel_tool_calls: None,
        streaming_tool_calls: None,
    };
    let got = openai_wire::tools_for(&rec, &cfg);
    assert!(
        got == serde_json::json!([]) || got == serde_json::Value::Null,
        "all-Recognized-hosted-tool request must serialize to [] or Null \
         (got: {got})"
    );
}

#[test]
fn openai_supported_hosted_tools_are_not_serialized_as_chat_functions() {
    let rec = registry::find("openai").expect("openai record");
    assert_eq!(
        rec.features.hosted_tools.web_search,
        CapabilityStatus::Supported,
        "OpenAI web search is Supported only by the explicit Responses transport"
    );

    let cfg = ToolCallConfig {
        tools: vec![tool("web_search"), tool("search_inventory")],
        tool_choice: ToolChoice::Named("web_search".into()),
        parallel_tool_calls: None,
        streaming_tool_calls: None,
    };

    let tools = openai_wire::tools_for(&rec, &cfg);
    assert_eq!(
        tools,
        serde_json::json!([
            {
                "type": "function",
                "function": {
                    "name": "search_inventory",
                    "parameters": {"type": "object"}
                }
            }
        ]),
        "OpenAI hosted tools must not be downgraded into Chat Completions function tools"
    );
    assert_eq!(
        openai_wire::tool_choice_for(&rec, &cfg),
        serde_json::Value::Null,
        "Chat Completions tool_choice must be omitted when the named tool was filtered"
    );
}

// ---------------------------------------------------------------------------
// T039 [US2]: pre-serialization validation for typed request surfaces.
// Unsupported or malformed typed surfaces must Block before any provider
// request body is built; recognized hosted-tool metadata must Warn and be
// filtered out of the OpenAI-compatible `tools[]` payload.
// ---------------------------------------------------------------------------

#[test]
fn recognized_hosted_tool_warns_before_serialization() {
    let rec = registry::find("groq").expect("groq record");
    assert_eq!(
        rec.features.tool_calling.function_calling,
        CapabilityStatus::Supported
    );
    assert_eq!(
        rec.features.hosted_tools.web_search,
        CapabilityStatus::Recognized
    );

    let cfg = ToolCallConfig {
        tools: vec![tool("web_search"), tool("search_inventory")],
        tool_choice: ToolChoice::Auto,
        parallel_tool_calls: None,
        streaming_tool_calls: None,
    };

    let outcome = validate_request_against_record(&rec, &FeatureRequest::tool_call(cfg));
    assert!(
        matches!(outcome, ValidationOutcome::Warn { .. }),
        "recognized hosted tool must Warn before serialization, got {outcome:?}"
    );
    if let ValidationOutcome::Warn { feature, reason } = outcome {
        assert_eq!(feature, "tool_calling");
        assert!(
            reason.contains("web_search") && reason.contains("Recognized"),
            "recognized hosted-tool warning must name the tool and status, got: {reason}"
        );
    }
}

#[test]
fn typed_request_parts_block_malformed_json_schema_before_serialization() {
    let rec = registry::find("openai").expect("openai record");
    let malformed = StructuredOutputConfig {
        mode: StructuredOutputMode::JsonSchema,
        schema: None,
        schema_name: Some("missing_schema".into()),
        strict: Some(true),
        schema_subset: None,
    };

    let outcomes = validate_typed_request_parts(&rec, Some(&malformed), None);
    assert!(
        outcomes.iter().any(|outcome| matches!(
            outcome,
            ValidationOutcome::Block {
                feature: "structured_output",
                reason
            } if reason.contains("non-null")
        )),
        "malformed typed JsonSchema request must Block before serialization: {outcomes:?}"
    );
}

#[test]
fn typed_request_parts_warn_and_filter_recognized_hosted_tools() {
    let rec = registry::find("groq").expect("groq record");
    let cfg = ToolCallConfig {
        tools: vec![tool("web_search"), tool("search_inventory")],
        tool_choice: ToolChoice::Auto,
        parallel_tool_calls: None,
        streaming_tool_calls: None,
    };

    let outcomes = validate_typed_request_parts(&rec, None, Some(&cfg));
    assert!(
        outcomes.iter().any(|outcome| matches!(
            outcome,
            ValidationOutcome::Warn {
                feature: "tool_calling",
                reason
            } if reason.contains("web_search")
        )),
        "recognized hosted tool must produce a warning outcome: {outcomes:?}"
    );

    let wire = openai_wire::tools_for(&rec, &cfg);
    let tools = wire.as_array().expect("tools array");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["function"]["name"], "search_inventory");

    let named_hosted = ToolCallConfig {
        tool_choice: ToolChoice::Named("web_search".into()),
        ..cfg
    };
    assert_eq!(
        openai_wire::tool_choice_for(&rec, &named_hosted),
        serde_json::Value::Null,
        "named choice for a recognized hosted tool must be omitted with the filtered tool"
    );
}

// ---------------------------------------------------------------------------
// T057-T058 [US4]: Candidate direct providers are design-only in this sprint.
// They may have evidence-backed Recognized/Future metadata, but no runnable
// direct adapter module or Supported feature claim may ship without explicit
// promotion.
// ---------------------------------------------------------------------------

#[test]
fn candidate_direct_providers_are_design_only_not_registered_adapters() {
    let provider_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/providers");
    let candidates = registry::candidate_provider_ids();
    assert_eq!(candidates, vec!["gemini", "cohere", "deepseek"]);

    for id in candidates {
        let direct_adapter_file = provider_dir.join(format!("{id}.rs"));
        assert!(
            !direct_adapter_file.exists(),
            "{id}: direct adapter file must not ship in feature 099"
        );

        let rec = registry::find(id).expect("candidate record");
        assert_eq!(
            rec.provider_specific
                .get("posture")
                .and_then(|v| v.as_str()),
            Some("design_only"),
            "{id}: candidate posture must be design_only"
        );
        assert_eq!(
            rec.provider_specific
                .get("adapter_registered")
                .and_then(|v| v.as_bool()),
            Some(false),
            "{id}: candidate record must declare no registered adapter"
        );
        assert!(
            supported_feature_keys_for(&rec.features).is_empty(),
            "{id}: candidate records must not claim Supported features in v0.9.4"
        );
    }
}

#[test]
fn candidate_records_defer_shared_embeddings_and_rerank_surfaces() {
    let gemini = registry::find("gemini").expect("gemini candidate record");
    assert_eq!(
        gemini.features.modalities.embeddings,
        CapabilityStatus::Future,
        "Gemini embeddings inform the future shared embeddings surface"
    );

    let cohere = registry::find("cohere").expect("cohere candidate record");
    assert_eq!(
        cohere.features.modalities.embeddings,
        CapabilityStatus::Future,
        "Cohere embeddings must stay Future until FR-014 shared surface design"
    );
    assert_eq!(
        cohere.features.modalities.rerank,
        CapabilityStatus::Future,
        "Cohere rerank must stay Future until FR-014 shared surface design"
    );
    let evidence = cohere
        .evidence
        .get("modalities")
        .expect("cohere modality evidence");
    assert!(
        evidence.notes.iter().any(|note| note.contains("deferred")),
        "Cohere modality evidence must record the deferred shared-surface decision"
    );
}

// ---------------------------------------------------------------------------
// T059-T061 [US5]: Manifest v2 split publication decision and public preview
// projection. The public projection is intentionally flat and excludes
// internal-only fields from JSON serialization.
// ---------------------------------------------------------------------------

fn manifest_v2_decision_path() -> std::path::PathBuf {
    [
        env!("CARGO_MANIFEST_DIR"),
        "..",
        "..",
        "..",
        "..",
        "specs",
        "099-v094-provider-capability-xl",
        "contracts",
        "manifest-v2-decision.md",
    ]
    .iter()
    .collect()
}

#[test]
fn manifest_v2_decision_artifact_is_complete() {
    let decision_path = manifest_v2_decision_path();
    let Ok(doc) = std::fs::read_to_string(&decision_path) else {
        // Public source exports intentionally omit internal planning artifacts.
        return;
    };
    for required in [
        "Posture: Split",
        "Rationale",
        "Public fields",
        "Internal-only fields",
        "Stability policy",
        "Promotion criteria",
        "Release-note copy",
    ] {
        assert!(
            doc.contains(required),
            "manifest decision artifact must include section/content: {required}"
        );
    }
}

#[test]
fn public_manifest_projection_has_expected_preview_fields_for_every_provider() {
    let manifest = registry::public_manifest();
    assert_eq!(
        manifest.schema_version,
        "capability-manifest-v2-public-preview/1"
    );
    assert_eq!(
        manifest.posture,
        registry::ManifestPublicationPosture::Split
    );
    assert_eq!(
        manifest.providers.len(),
        registry::all_records().len(),
        "public projection must include every registry provider"
    );

    let expected_keys = [
        "vision_input",
        "tool_calling",
        "thinking_blocks",
        "streaming_logprobs",
        "json_mode",
        "json_schema_strict",
        "json_schema_best_effort",
        "embeddings",
        "rerank",
    ];
    for provider in &manifest.providers {
        assert!(!provider.name.is_empty());
        assert!(!provider.display_name.is_empty());
        assert!(!provider.last_reviewed_on.is_empty());
        assert_eq!(provider.provider_status, "unknown");
        for key in expected_keys {
            assert!(
                provider.capabilities.contains_key(key),
                "{} missing public capability field {key}",
                provider.name
            );
        }
        assert_eq!(
            provider.capabilities.len(),
            expected_keys.len(),
            "{} public capability map drifted from split preview contract",
            provider.name
        );
    }
}

#[test]
fn public_manifest_json_excludes_internal_only_fields() {
    let value = serde_json::to_value(registry::public_manifest()).unwrap();
    for forbidden in [
        "features",
        "evidence",
        "model_overrides",
        "provider_specific",
        "source_url",
        "adapter_test",
        "fixture_path",
        "live_test",
    ] {
        assert!(
            !json_contains_key(&value, forbidden),
            "public manifest JSON must not expose internal-only field key {forbidden}: {value}"
        );
    }
}

#[test]
fn public_manifest_lookup_p95_stays_under_one_ms() {
    registry::public_manifest();

    let mut samples = Vec::with_capacity(128);
    for _ in 0..128 {
        let started = Instant::now();
        let manifest = registry::public_manifest();
        assert!(!manifest.providers.is_empty());
        samples.push(started.elapsed());
    }

    let p95 = percentile_95(&mut samples);
    assert!(
        p95 <= Duration::from_millis(1),
        "public manifest lookup p95 {p95:?} exceeded 1 ms target"
    );
}

#[test]
fn request_validation_p95_stays_under_five_ms() {
    let record = registry::find("openai").expect("openai record");
    let request = FeatureRequest::StructuredOutput(StructuredOutputConfig {
        mode: StructuredOutputMode::JsonSchema,
        schema: Some(serde_json::json!({"type": "object"})),
        schema_name: Some("capability_smoke".into()),
        strict: Some(true),
        schema_subset: None,
    });

    let warmup = validate_request_against_record(&record, &request);
    assert!(
        matches!(warmup, ValidationOutcome::Allow),
        "warmup validation must allow supported strict JSON Schema: {warmup:?}"
    );

    let mut samples = Vec::with_capacity(128);
    for _ in 0..128 {
        let started = Instant::now();
        let outcome = validate_request_against_record(&record, &request);
        assert!(matches!(outcome, ValidationOutcome::Allow));
        samples.push(started.elapsed());
    }

    let p95 = percentile_95(&mut samples);
    assert!(
        p95 <= Duration::from_millis(5),
        "request validation p95 {p95:?} exceeded 5 ms target"
    );
}

fn percentile_95(samples: &mut [Duration]) -> Duration {
    samples.sort_unstable();
    let index = ((samples.len() * 95) / 100).min(samples.len() - 1);
    samples[index]
}

fn json_contains_key(value: &serde_json::Value, key: &str) -> bool {
    match value {
        serde_json::Value::Object(map) => {
            map.contains_key(key) || map.values().any(|v| json_contains_key(v, key))
        }
        serde_json::Value::Array(values) => values.iter().any(|v| json_contains_key(v, key)),
        _ => false,
    }
}
