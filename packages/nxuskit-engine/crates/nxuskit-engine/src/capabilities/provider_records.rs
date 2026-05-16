//! Evidence-backed provider capability records (feature 099 — Phase 3, US1).
//!
//! Each function returns a [`ProviderCapabilityRecord`] for one provider.
//! Source URLs come from the Phase 1 snapshot at
//! `internal/tests/parity/provider_capabilities/fixtures/provider-source-snapshot.json`;
//! `adapter_test` references point at existing in-tree tests in
//! `crates/nxuskit-engine/src/providers/<provider>.rs` so the FR-018 rule
//! ("`Supported` requires source URL plus adapter/fixture/live proof") is
//! satisfied without claiming evidence we don't have.

use std::collections::HashMap;

use serde_json::Value;

use super::{
    CapabilityEvidence, CapabilityFeatureSet, CapabilityStatus, ModelCapabilityOverride,
    ProviderCapabilityRecord,
};

const SPRINT_REVIEW_DATE: &str = "2026-05-09";

fn evidence(source_url: &str, adapter_test: &str) -> CapabilityEvidence {
    CapabilityEvidence {
        source_url: Some(source_url.to_string()),
        source_reviewed_on: SPRINT_REVIEW_DATE.into(),
        adapter_test: Some(adapter_test.to_string()),
        fixture_path: None,
        live_test: None,
        notes: vec![],
    }
}

/// OpenAI capability record. GPT-5.5 (snapshot `gpt-5.5-2026-04-23`) is
/// the documented frontier model; GPT-5.4 and GPT-5.4-mini are the
/// lower-cost alternatives. Phase 4 (T035) wired typed
/// `StructuredOutputConfig` and `ToolCallConfig` through `OpenAIRequest`
/// (`response_format`, `tools`, `tool_choice`, `parallel_tool_calls`),
/// promoting the corresponding capability statuses to `Supported`.
/// `streaming_tool_calls` stays `Recognized` until typed `ToolCallDelta`
/// stream-decoding lands. Phase 5 (T049) wires explicit Responses payloads
/// for hosted tools, state, reasoning summary, text verbosity, include paths,
/// and search controls.
pub fn openai() -> ProviderCapabilityRecord {
    let docs_models = "https://platform.openai.com/docs/models";
    let docs_responses = "https://platform.openai.com/docs/api-reference/responses";
    let docs_reasoning = "https://platform.openai.com/docs/guides/reasoning";
    let docs_struct_out = "https://platform.openai.com/docs/guides/structured-outputs";
    let docs_tools = "https://platform.openai.com/docs/guides/function-calling";
    let docs_logprobs =
        "https://platform.openai.com/docs/api-reference/chat/create#chat-create-logprobs";

    let openai_test_module =
        "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/openai.rs::tests";
    let logprobs_stream_test = "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/openai.rs::tests::test_build_stream_request_includes_logprobs";
    let gpt54_guard_test = "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/openai.rs::tests::test_build_stream_request_applies_gpt54_reasoning_guard";

    let mut features = CapabilityFeatureSet::default();

    // Reasoning: GPT-5.4+ effort drop logic ships in Chat Completions today.
    // Phase 5 also serializes reasoning effort/summary into explicit
    // Responses payloads.
    features.reasoning.effort_control = CapabilityStatus::Supported;
    features.reasoning.reasoning_summary = CapabilityStatus::Supported;

    // Structured output: typed StructuredOutputConfig is now spliced into
    // OpenAIRequest::response_format via openai_wire::response_format
    // (T035). The four request-shape tests
    // `test_build_request_serializes_typed_structured_output_*` prove the
    // wire bytes for json_object and json_schema (strict + named-schema
    // form, with user-provided `additionalProperties: false` carried
    // verbatim).
    features.structured_output.json_mode = CapabilityStatus::Supported;
    features.structured_output.json_schema_strict = CapabilityStatus::Supported;
    features.structured_output.json_schema_best_effort = CapabilityStatus::Supported;
    features.structured_output.named_schemas = CapabilityStatus::Supported;
    features.structured_output.additionalprops_false = CapabilityStatus::Supported;

    // Tool calling: typed ToolCallConfig is now spliced into
    // OpenAIRequest::{tools, tool_choice, parallel_tool_calls} via
    // openai_wire::{tools, tool_choice} (T035), proven by
    // `test_build_request_serializes_typed_tools_and_tool_choice`.
    // Streaming tool-call decoding is NOT yet wired through the typed
    // ToolCallDelta surface — held at Recognized until a later slice.
    features.tool_calling.function_calling = CapabilityStatus::Supported;
    features.tool_calling.parallel_tool_calls = CapabilityStatus::Supported;
    features.tool_calling.streaming_tool_calls = CapabilityStatus::Recognized;
    features.tool_calling.tool_choice_modes = vec![
        "auto".into(),
        "none".into(),
        "required".into(),
        "named".into(),
    ];

    // Hosted tools: supported only through the explicit Responses transport.
    // Chat Completions `tools[]` serialization still filters hosted-tool names
    // because they are not function definitions.
    features.hosted_tools.web_search = CapabilityStatus::Supported;
    features.hosted_tools.file_search = CapabilityStatus::Supported;
    features.hosted_tools.code_interpreter = CapabilityStatus::Supported;
    features.hosted_tools.image_generation = CapabilityStatus::Supported;
    features.hosted_tools.computer_use = CapabilityStatus::Supported;
    features.hosted_tools.mcp_connector = CapabilityStatus::Supported;

    // Search/citations: explicit Responses `tool_search` payload support.
    features.search_citations.search_controls = CapabilityStatus::Supported;

    // Modalities: vision input is documented for gpt-4o+ and exercised by the
    // existing adapter's message_conversion path. Embeddings are deferred to
    // FR-014 — surfaced as Future, not Supported.
    features.modalities.vision_input = CapabilityStatus::Supported;
    features.modalities.embeddings = CapabilityStatus::Future;
    features.modalities.moderation = CapabilityStatus::Recognized;

    // Routing: OpenAI native API has no provider routing concept.
    features.routing.provider_routing = CapabilityStatus::Unsupported;

    // State: previous_response_id and phase are serialized only by the
    // explicit Responses payload builder.
    features.state.previous_response_id = CapabilityStatus::Supported;
    features.state.response_phase = CapabilityStatus::Supported;
    features.state.prompt_caching = CapabilityStatus::Recognized;

    // Logprobs: existing adapter path + S1 streaming work.
    features.logprobs.logprobs = CapabilityStatus::Supported;
    features.logprobs.streaming_logprobs = CapabilityStatus::Supported;
    features.logprobs.max_top_logprobs = Some(20);

    // Evidence keyed by feature group; the registry test accepts either an
    // exact-feature-key match or a group-prefix match.
    let mut ev: HashMap<String, CapabilityEvidence> = HashMap::new();
    ev.insert(
        "reasoning".into(),
        evidence(docs_reasoning, gpt54_guard_test),
    );
    ev.insert(
        "structured_output".into(),
        evidence(docs_struct_out, openai_test_module),
    );
    ev.insert(
        "tool_calling".into(),
        evidence(docs_tools, openai_test_module),
    );
    ev.insert(
        "modalities".into(),
        evidence(docs_models, openai_test_module),
    );
    ev.insert("models".into(), evidence(docs_models, openai_test_module));
    ev.insert(
        "logprobs".into(),
        evidence(docs_logprobs, logprobs_stream_test),
    );
    // hosted_tools, state, routing entries below allow Recognized statuses to
    // carry an attribution URL even though they don't claim Supported.
    ev.insert(
        "hosted_tools".into(),
        evidence(docs_responses, openai_test_module),
    );
    ev.insert(
        "search_citations".into(),
        evidence(docs_responses, openai_test_module),
    );
    ev.insert("state".into(), evidence(docs_responses, openai_test_module));
    ev.insert("routing".into(), evidence(docs_models, openai_test_module));

    // Frontier and lower-cost model overrides. Each must declare a non-empty
    // reviewed_on (T017 guard).
    let mut model_overrides: HashMap<String, ModelCapabilityOverride> = HashMap::new();
    model_overrides.insert(
        "gpt-5.5".into(),
        ModelCapabilityOverride {
            reviewed_on: SPRINT_REVIEW_DATE.into(),
            feature_overrides: HashMap::new(),
            notes: vec![
                "Current documented frontier API model (FR-001).".into(),
                "Inherits all GPT-5.4 Responses-era controls.".into(),
            ],
        },
    );
    model_overrides.insert(
        "gpt-5.5-2026-04-23".into(),
        ModelCapabilityOverride {
            reviewed_on: SPRINT_REVIEW_DATE.into(),
            feature_overrides: HashMap::new(),
            notes: vec!["Dated snapshot of gpt-5.5 (FR-001).".into()],
        },
    );
    model_overrides.insert(
        "gpt-5.4".into(),
        ModelCapabilityOverride {
            reviewed_on: SPRINT_REVIEW_DATE.into(),
            feature_overrides: HashMap::new(),
            notes: vec!["Lower-cost alternative; reasoning-effort drop logic active.".into()],
        },
    );
    model_overrides.insert(
        "gpt-5.4-mini".into(),
        ModelCapabilityOverride {
            reviewed_on: SPRINT_REVIEW_DATE.into(),
            feature_overrides: HashMap::new(),
            notes: vec!["Smaller GPT-5.4 variant; same reasoning-effort guard.".into()],
        },
    );

    ProviderCapabilityRecord {
        provider_id: "openai".into(),
        display_name: "OpenAI".into(),
        last_reviewed_on: SPRINT_REVIEW_DATE.into(),
        default_model: Some("gpt-5.5".into()),
        features,
        evidence: ev,
        model_overrides,
        provider_specific: Value::Null,
    }
}

/// Anthropic / Claude capability record. Vision input is adapter-mapped
/// today (multimodal `ClaudeContentPart::Image` plus
/// `test_message_conversion`). Tool use is documented and the response
/// path recognizes the `tool_use` finish reason, but `ClaudeRequest`
/// does not currently carry `tools` / `tool_choice` — `Recognized` until
/// Phase 4 (T035) wires the request side. Extended thinking blocks are
/// parsed on the response side via `ContentBlock::Thinking`, but the
/// request-side typed reasoning carrier is deferred to Phase 5 — also
/// `Recognized`. JSON mode is `Unsupported` (Claude has no native JSON
/// mode — prompt-based only). Hosted web search, code execution, and the
/// MCP connector are beta features held at `Recognized` per OD-2;
/// streaming logprobs are `Unsupported` (not documented).
pub fn anthropic() -> ProviderCapabilityRecord {
    let docs_tools = "https://docs.anthropic.com/en/docs/build-with-claude/tool-use";
    let docs_thinking = "https://docs.anthropic.com/en/docs/build-with-claude/extended-thinking";
    let docs_web_search = "https://docs.anthropic.com/en/docs/build-with-claude/web-search-tool";
    let docs_code_exec = "https://docs.anthropic.com/en/docs/build-with-claude/code-execution-tool";
    let docs_mcp = "https://docs.anthropic.com/en/docs/agents-and-tools/mcp-connector";

    let claude_test_module =
        "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/claude.rs::tests";
    let claude_message_test = "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/claude.rs::tests::test_message_conversion";
    let claude_sse_test = "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/claude.rs::tests::test_sse_event_parsing";

    let mut features = CapabilityFeatureSet::default();

    // Reasoning: extended thinking blocks are parsed on the response side
    // (ContentBlock::Thinking handler), but `ClaudeRequest` does not carry a
    // first-class thinking request control. Held at Recognized until Phase 5
    // wires a typed reasoning carrier.
    features.reasoning.thinking_blocks = CapabilityStatus::Recognized;

    // Structured output: Claude has no native JSON mode or JSON Schema —
    // prompt-based only. Mark explicitly Unsupported, not Unknown.
    features.structured_output.json_mode = CapabilityStatus::Unsupported;
    features.structured_output.json_schema_strict = CapabilityStatus::Unsupported;
    features.structured_output.json_schema_best_effort = CapabilityStatus::Unsupported;

    // Tool calling: documented but `ClaudeRequest` does not currently
    // serialize `tools` / `tool_choice` — only `tool_use` finish-reason
    // mapping is wired on the response side. Held at Recognized until
    // Phase 4 (T035) ships request-side wiring.
    features.tool_calling.function_calling = CapabilityStatus::Recognized;
    features.tool_calling.parallel_tool_calls = CapabilityStatus::Recognized;
    features.tool_calling.streaming_tool_calls = CapabilityStatus::Recognized;
    features.tool_calling.tool_choice_modes = vec!["auto".into(), "any".into(), "tool".into()];

    // Hosted tools: beta + safety-sensitive — Recognized per OD-2.
    features.hosted_tools.web_search = CapabilityStatus::Recognized;
    features.hosted_tools.code_interpreter = CapabilityStatus::Recognized;
    features.hosted_tools.mcp_connector = CapabilityStatus::Recognized;

    // Modalities: vision input documented and exercised via message conversion.
    features.modalities.vision_input = CapabilityStatus::Supported;
    // Embeddings are not a documented Anthropic surface.
    features.modalities.embeddings = CapabilityStatus::Unsupported;

    // State: Claude prompt caching is documented but not mapped through this
    // capability surface yet — stays Recognized.
    features.state.prompt_caching = CapabilityStatus::Recognized;

    // Logprobs: not documented for Claude.
    features.logprobs.logprobs = CapabilityStatus::Unsupported;
    features.logprobs.streaming_logprobs = CapabilityStatus::Unsupported;

    let mut ev: HashMap<String, CapabilityEvidence> = HashMap::new();
    ev.insert(
        "reasoning".into(),
        evidence(docs_thinking, claude_test_module),
    );
    ev.insert("tool_calling".into(), evidence(docs_tools, claude_sse_test));
    ev.insert(
        "modalities".into(),
        evidence(docs_tools, claude_message_test),
    );
    // Hosted-tool entries are Recognized; the registry still benefits from
    // attribution URLs so future promotions can find the source quickly.
    let mut hosted_tools_evidence = evidence(docs_web_search, claude_test_module);
    hosted_tools_evidence.notes = vec![
        format!("Web search beta: {docs_web_search}"),
        format!("Code execution beta: {docs_code_exec}"),
        format!("MCP connector beta: {docs_mcp}"),
        "Held at Recognized per OD-2 in v0.9.4.".into(),
    ];
    ev.insert("hosted_tools".into(), hosted_tools_evidence);

    ProviderCapabilityRecord {
        provider_id: "anthropic".into(),
        display_name: "Anthropic Claude".into(),
        last_reviewed_on: SPRINT_REVIEW_DATE.into(),
        default_model: Some("claude-sonnet-4-5".into()),
        features,
        evidence: ev,
        model_overrides: HashMap::new(),
        provider_specific: Value::Null,
    }
}

/// Mistral capability record. Phase 4 (T036) wired typed
/// `StructuredOutputConfig` and `ToolCallConfig` through
/// `MistralRequest` (`response_format`, `tools`, `tool_choice`,
/// `parallel_tool_calls`), promoting the corresponding capability
/// statuses to `Supported`. The legacy boolean
/// `supports_json_schema: false` on the historical capability surface
/// is preserved for backward compatibility but the typed path covers
/// strict + named-schema modes faithfully. Streaming tool-call decoding
/// stays `Recognized` until typed `ToolCallDelta` lands. Mistral Agents
/// built-in tools (web search / code interpreter) are documented but
/// provider-specific in v0.9.4 — held at `Recognized`.
pub fn mistral() -> ProviderCapabilityRecord {
    let docs_function = "https://docs.mistral.ai/capabilities/function_calling/";
    let docs_json_mode = "https://docs.mistral.ai/capabilities/structured-output/json_mode/";
    let docs_json_schema =
        "https://docs.mistral.ai/capabilities/structured-output/custom_structured_output/";
    let docs_agents = "https://docs.mistral.ai/agents/connectors/";

    let mistral_test_module =
        "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/mistral.rs::tests";
    let mistral_caps_test = "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/mistral.rs::tests::test_mistral_capabilities";
    let mistral_message_test = "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/mistral.rs::tests::test_message_conversion";
    let mistral_typed_structured_test = "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/mistral.rs::tests::pro_tests::test_build_request_serializes_typed_structured_output_json_schema";
    let mistral_typed_tools_test = "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/mistral.rs::tests::pro_tests::test_build_request_serializes_typed_tools_and_tool_choice";

    let mut features = CapabilityFeatureSet::default();

    // T036 wired typed StructuredOutputConfig through MistralRequest's
    // response_format field via `openai_wire::response_format`; proven by
    // the four `test_build_request_serializes_typed_*` adapter tests.
    features.structured_output.json_mode = CapabilityStatus::Supported;
    features.structured_output.json_schema_strict = CapabilityStatus::Supported;
    features.structured_output.json_schema_best_effort = CapabilityStatus::Supported;
    features.structured_output.named_schemas = CapabilityStatus::Supported;
    features.structured_output.additionalprops_false = CapabilityStatus::Supported;

    // T036 wired typed ToolCallConfig through MistralRequest's tools /
    // tool_choice / parallel_tool_calls fields via `openai_wire::tools`
    // and `openai_wire::tool_choice`. Streaming tool-call decoding is
    // not yet wired through ToolCallDelta — held at Recognized.
    features.tool_calling.function_calling = CapabilityStatus::Supported;
    features.tool_calling.parallel_tool_calls = CapabilityStatus::Supported;
    features.tool_calling.streaming_tool_calls = CapabilityStatus::Recognized;
    features.tool_calling.tool_choice_modes = vec!["auto".into(), "any".into(), "none".into()];

    // Agents built-in tools: documented but provider-specific in v0.9.4.
    features.hosted_tools.web_search = CapabilityStatus::Recognized;
    features.hosted_tools.code_interpreter = CapabilityStatus::Recognized;

    // Modalities: vision is supported on Pixtral models but not adapter-
    // exercised in the existing test surface — left Recognized.
    features.modalities.vision_input = CapabilityStatus::Recognized;
    features.modalities.embeddings = CapabilityStatus::Recognized;

    // Logprobs: not part of the documented Mistral chat surface today.
    features.logprobs.logprobs = CapabilityStatus::Unknown;
    features.logprobs.streaming_logprobs = CapabilityStatus::Unknown;

    let mut ev: HashMap<String, CapabilityEvidence> = HashMap::new();
    ev.insert(
        "tool_calling".into(),
        evidence(docs_function, mistral_typed_tools_test),
    );
    ev.insert(
        "structured_output.json_mode".into(),
        evidence(docs_json_mode, mistral_caps_test),
    );
    ev.insert(
        "structured_output.json_schema_strict".into(),
        evidence(docs_json_schema, mistral_typed_structured_test),
    );
    ev.insert(
        "structured_output.json_schema_best_effort".into(),
        evidence(docs_json_schema, mistral_typed_structured_test),
    );
    ev.insert(
        "structured_output.named_schemas".into(),
        evidence(docs_json_schema, mistral_typed_structured_test),
    );
    ev.insert(
        "structured_output.additionalprops_false".into(),
        evidence(docs_json_schema, mistral_typed_structured_test),
    );
    let mut hosted = evidence(docs_agents, mistral_test_module);
    hosted.notes = vec![
        format!("Agents built-in tools: {docs_agents}"),
        "Held at Recognized in v0.9.4 (provider-specific).".into(),
    ];
    ev.insert("hosted_tools".into(), hosted);
    ev.insert(
        "modalities".into(),
        evidence(docs_function, mistral_message_test),
    );

    ProviderCapabilityRecord {
        provider_id: "mistral".into(),
        display_name: "Mistral AI".into(),
        last_reviewed_on: SPRINT_REVIEW_DATE.into(),
        default_model: None,
        features,
        evidence: ev,
        model_overrides: HashMap::new(),
        provider_specific: Value::Null,
    }
}

/// Groq capability record. Phase 4 (T037) wired typed
/// `StructuredOutputConfig` and `ToolCallConfig` through `GroqRequest`
/// (`response_format`, `tools`, `tool_choice`, `parallel_tool_calls`),
/// promoting the corresponding OpenAI-compatible statuses to `Supported`.
/// The legacy boolean `supports_json_schema: false` remains preserved for
/// backward compatibility. Compound systems (web search) and the
/// reasoning-output field remain recognized/provider-specific rather than
/// first-class v0.9.4 request fields. Logprobs are explicitly `Unsupported`.
pub fn groq() -> ProviderCapabilityRecord {
    let docs_struct_out = "https://console.groq.com/docs/structured-outputs";
    let docs_chat_api = "https://console.groq.com/docs/api-reference#chat-create";
    let docs_compound = "https://console.groq.com/docs/agentic-tooling";

    let groq_test_module =
        "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/groq.rs::tests";
    let groq_message_test = "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/groq.rs::tests::test_message_conversion_text_only";
    let groq_typed_structured_test = "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/groq.rs::tests::pro_tests::test_build_request_serializes_typed_structured_output_json_schema";
    let groq_typed_tools_test = "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/groq.rs::tests::pro_tests::test_build_request_serializes_typed_tools_and_tool_choice";

    let mut features = CapabilityFeatureSet::default();

    // T037 wired typed StructuredOutputConfig through GroqRequest's
    // response_format field via `openai_wire::response_format`.
    features.structured_output.json_mode = CapabilityStatus::Supported;
    features.structured_output.json_schema_strict = CapabilityStatus::Supported;
    features.structured_output.json_schema_best_effort = CapabilityStatus::Supported;
    features.structured_output.named_schemas = CapabilityStatus::Supported;
    features.structured_output.additionalprops_false = CapabilityStatus::Supported;

    // T037 wired typed ToolCallConfig through GroqRequest's tools /
    // tool_choice / parallel_tool_calls fields. Streaming tool-call decoding
    // is not yet wired through ToolCallDelta.
    features.tool_calling.function_calling = CapabilityStatus::Supported;
    features.tool_calling.parallel_tool_calls = CapabilityStatus::Supported;
    features.tool_calling.streaming_tool_calls = CapabilityStatus::Recognized;
    features.tool_calling.tool_choice_modes = vec![
        "auto".into(),
        "none".into(),
        "required".into(),
        "named".into(),
    ];

    // Compound web search and reasoning-output field: provider-specific in v0.9.4.
    features.hosted_tools.web_search = CapabilityStatus::Recognized;
    features.reasoning.reasoning_content_field = CapabilityStatus::ProviderSpecific;

    // Modalities: vision-capable models exist; not adapter-fixture-tested.
    features.modalities.vision_input = CapabilityStatus::Recognized;

    // Logprobs: the existing Groq adapter explicitly disables logprobs and
    // streaming logprobs (`supports_logprobs: false`). Treat as Unsupported.
    features.logprobs.logprobs = CapabilityStatus::Unsupported;
    features.logprobs.streaming_logprobs = CapabilityStatus::Unsupported;

    let mut ev: HashMap<String, CapabilityEvidence> = HashMap::new();
    ev.insert(
        "structured_output".into(),
        evidence(docs_struct_out, groq_typed_structured_test),
    );
    ev.insert(
        "tool_calling".into(),
        evidence(docs_chat_api, groq_typed_tools_test),
    );
    let mut hosted = evidence(docs_compound, groq_test_module);
    hosted.notes = vec![
        "Groq Compound tools (web search) are provider-specific in v0.9.4.".into(),
        format!("Source: {docs_compound}"),
    ];
    ev.insert("hosted_tools".into(), hosted);
    let mut reasoning = evidence(docs_compound, groq_test_module);
    reasoning.notes =
        vec!["reasoning_content field is provider-specific (Compound mode only).".into()];
    ev.insert("reasoning".into(), reasoning);
    ev.insert(
        "modalities".into(),
        evidence(docs_chat_api, groq_message_test),
    );

    ProviderCapabilityRecord {
        provider_id: "groq".into(),
        display_name: "Groq".into(),
        last_reviewed_on: SPRINT_REVIEW_DATE.into(),
        default_model: None,
        features,
        evidence: ev,
        model_overrides: HashMap::new(),
        provider_specific: Value::Null,
    }
}

/// xAI Grok capability record. The runtime adapter is registered under
/// provider id `xai` and uses xAI's OpenAI-compatible API. Grok is deliberately
/// distinct from Groq (`groq`), and no `grok` provider alias is registered.
pub fn xai() -> ProviderCapabilityRecord {
    let docs_chat = "https://docs.x.ai/docs/guides/chat";
    let docs_function = "https://docs.x.ai/docs/guides/function-calling";
    let docs_structured = "https://docs.x.ai/docs/guides/structured-outputs";
    let docs_models = "https://docs.x.ai/docs/models/";

    let xai_adapter_test =
        "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/xai.rs::tests";

    let mut features = CapabilityFeatureSet::default();

    // xAI documents OpenAI-compatible chat completions, function calling,
    // structured outputs, and vision-capable Grok models. The first v0.9.4
    // adapter slice registers runtime chat/stream/model-list support while
    // keeping advanced/model-scoped semantics evidence-gated.
    features.structured_output.json_mode = CapabilityStatus::Recognized;
    features.structured_output.json_schema_strict = CapabilityStatus::Recognized;
    features.structured_output.json_schema_best_effort = CapabilityStatus::Recognized;
    features.tool_calling.function_calling = CapabilityStatus::Recognized;
    features.tool_calling.parallel_tool_calls = CapabilityStatus::Recognized;
    features.reasoning.effort_control = CapabilityStatus::Recognized;
    features.modalities.vision_input = CapabilityStatus::Recognized;
    features.logprobs.logprobs = CapabilityStatus::Unsupported;
    features.logprobs.streaming_logprobs = CapabilityStatus::Unsupported;

    let mut ev: HashMap<String, CapabilityEvidence> = HashMap::new();
    ev.insert("chat".into(), evidence(docs_chat, xai_adapter_test));
    ev.insert(
        "structured_output".into(),
        evidence(docs_structured, xai_adapter_test),
    );
    ev.insert(
        "tool_calling".into(),
        evidence(docs_function, xai_adapter_test),
    );
    ev.insert("modalities".into(), evidence(docs_models, xai_adapter_test));

    ProviderCapabilityRecord {
        provider_id: "xai".into(),
        display_name: "xAI Grok".into(),
        last_reviewed_on: SPRINT_REVIEW_DATE.into(),
        default_model: Some("grok-4".into()),
        features,
        evidence: ev,
        model_overrides: HashMap::new(),
        provider_specific: serde_json::json!({
            "transport": "openai_compatible",
            "adapter_registered": true,
            "base_url": "https://api.x.ai/v1"
        }),
    }
}

/// Perplexity capability record. Sonar search controls and citation
/// metadata are documented and exercised by the existing adapter, but
/// they require a typed Perplexity options carrier — held at
/// `ProviderSpecific` (not `Supported`) for v0.9.4 per research.md. Agent
/// API streaming and structured output stay `Recognized` until a fixture
/// or live test promotes them; function/tool calling is not part of the
/// current Perplexity adapter surface.
pub fn perplexity() -> ProviderCapabilityRecord {
    let docs_chat = "https://docs.perplexity.ai/api-reference/chat-completions";
    let docs_struct_out = "https://docs.perplexity.ai/guides/structured-outputs";

    let perplexity_test_module =
        "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/perplexity.rs::tests";
    let perplexity_caps_test = "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/perplexity.rs::tests::test_perplexity_capabilities";
    let perplexity_request_test = "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/perplexity.rs::tests::pro_tests::test_build_request_basic";

    let mut features = CapabilityFeatureSet::default();

    // Search and citations: documented core of the Perplexity API. Adapter
    // already passes search results through `provider_metadata`, but a
    // first-class typed carrier is deferred — provider-specific in v0.9.4.
    features.search_citations.search_controls = CapabilityStatus::ProviderSpecific;
    features.search_citations.citation_metadata = CapabilityStatus::ProviderSpecific;

    // Structured output (Agent API): documented but not adapter-fixture-tested.
    features.structured_output.json_schema_strict = CapabilityStatus::Recognized;
    features.structured_output.json_schema_best_effort = CapabilityStatus::Recognized;

    // Tool calling: not part of the current Perplexity adapter surface.
    features.tool_calling.function_calling = CapabilityStatus::Unknown;

    // State: Agent API streaming exists but not adapter-mapped.
    features.state.response_phase = CapabilityStatus::Recognized;

    // Logprobs: not documented for Perplexity.
    features.logprobs.logprobs = CapabilityStatus::Unknown;

    let mut ev: HashMap<String, CapabilityEvidence> = HashMap::new();
    let mut search_ev = evidence(docs_chat, perplexity_request_test);
    search_ev.notes = vec![
        "Sonar search controls are provider-specific in v0.9.4; typed carrier deferred.".into(),
    ];
    ev.insert("search_citations".into(), search_ev);

    ev.insert(
        "structured_output".into(),
        evidence(docs_struct_out, perplexity_test_module),
    );

    ev.insert("state".into(), evidence(docs_chat, perplexity_caps_test));

    ProviderCapabilityRecord {
        provider_id: "perplexity".into(),
        display_name: "Perplexity".into(),
        last_reviewed_on: SPRINT_REVIEW_DATE.into(),
        default_model: Some("llama-3.1-sonar-small-128k-online".into()),
        features,
        evidence: ev,
        model_overrides: HashMap::new(),
        provider_specific: Value::Null,
    }
}

/// Together AI capability record. Phase 4 (T038) wired typed
/// `StructuredOutputConfig` and `ToolCallConfig` through `TogetherRequest`
/// (`response_format`, `tools`, `tool_choice`, `parallel_tool_calls`),
/// promoting the OpenAI-compatible JSON Schema and function-calling
/// surfaces used by the OD-1 fixture to `Supported`. Streaming tool calls
/// remain recognized-only until typed `ToolCallDelta` decoding lands.
pub fn together() -> ProviderCapabilityRecord {
    let docs_chat = "https://docs.together.ai/docs/chat-overview";
    let docs_json_mode = "https://docs.together.ai/docs/json-mode";
    let docs_function = "https://docs.together.ai/docs/function-calling";

    let together_json_mode_test = "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/together.rs::tests::pro_tests::test_build_request_with_json_mode";
    let together_vision_test = "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/together.rs::tests::pro_tests::test_message_conversion_with_vision";
    let together_typed_structured_test = "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/together.rs::tests::pro_tests::test_build_request_serializes_typed_structured_output_json_schema";
    let together_typed_tools_test = "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/together.rs::tests::pro_tests::test_build_request_serializes_typed_tools_and_tool_choice";

    let mut features = CapabilityFeatureSet::default();

    // T038 wired typed StructuredOutputConfig through TogetherRequest's
    // response_format field via `openai_wire::response_format`.
    features.structured_output.json_mode = CapabilityStatus::Supported;
    features.structured_output.json_schema_strict = CapabilityStatus::Supported;
    features.structured_output.json_schema_best_effort = CapabilityStatus::Supported;
    features.structured_output.named_schemas = CapabilityStatus::Supported;
    features.structured_output.additionalprops_false = CapabilityStatus::Supported;

    // T038 wired typed ToolCallConfig through TogetherRequest's tools /
    // tool_choice / parallel_tool_calls fields. Streaming tool-call decoding
    // remains recognized-only.
    features.tool_calling.function_calling = CapabilityStatus::Supported;
    features.tool_calling.parallel_tool_calls = CapabilityStatus::Supported;
    features.tool_calling.streaming_tool_calls = CapabilityStatus::Recognized;
    features.tool_calling.tool_choice_modes = vec!["auto".into(), "none".into(), "required".into()];

    // Modalities: vision-capable models exist via Together; the existing
    // adapter has a `test_message_conversion_with_vision` test that covers
    // multimodal message conversion.
    features.modalities.vision_input = CapabilityStatus::Supported;

    // Logprobs: not part of the adapter surface today.
    features.logprobs.logprobs = CapabilityStatus::Unknown;

    let mut ev: HashMap<String, CapabilityEvidence> = HashMap::new();
    ev.insert(
        "structured_output.json_mode".into(),
        evidence(docs_json_mode, together_json_mode_test),
    );
    ev.insert(
        "structured_output.json_schema_strict".into(),
        evidence(docs_json_mode, together_typed_structured_test),
    );
    ev.insert(
        "structured_output.json_schema_best_effort".into(),
        evidence(docs_json_mode, together_typed_structured_test),
    );
    ev.insert(
        "structured_output.named_schemas".into(),
        evidence(docs_json_mode, together_typed_structured_test),
    );
    ev.insert(
        "structured_output.additionalprops_false".into(),
        evidence(docs_json_mode, together_typed_structured_test),
    );
    ev.insert(
        "tool_calling".into(),
        evidence(docs_function, together_typed_tools_test),
    );
    ev.insert(
        "modalities".into(),
        evidence(docs_chat, together_vision_test),
    );

    ProviderCapabilityRecord {
        provider_id: "together".into(),
        display_name: "Together AI".into(),
        last_reviewed_on: SPRINT_REVIEW_DATE.into(),
        default_model: None,
        features,
        evidence: ev,
        model_overrides: HashMap::new(),
        provider_specific: Value::Null,
    }
}

/// OpenRouter capability record. OpenRouter routes OpenAI-compatible
/// chat requests to a wide pool of upstream providers. Structured output,
/// tool calling, and streaming tool-call passthrough are documented but
/// model-dependent — held at `Recognized` until per-model fixture proof
/// lands. Provider routing and `require_parameters` are documented control
/// fields that require a typed routing carrier — `ProviderSpecific` in
/// v0.9.4.
pub fn openrouter() -> ProviderCapabilityRecord {
    let docs_struct_out = "https://openrouter.ai/docs/features/structured-outputs";
    let docs_tools = "https://openrouter.ai/docs/features/tool-calling";
    let docs_routing = "https://openrouter.ai/docs/features/provider-routing";

    let openrouter_test_module =
        "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/openrouter.rs::tests";
    let openrouter_caps_test = "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/openrouter.rs::tests::test_openrouter_capabilities";
    let openrouter_message_test = "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/openrouter.rs::tests::pro_tests::test_message_conversion";

    let mut features = CapabilityFeatureSet::default();

    // Structured output: routed pass-through; documented but model-dependent.
    features.structured_output.json_mode = CapabilityStatus::Recognized;
    features.structured_output.json_schema_strict = CapabilityStatus::Recognized;
    features.structured_output.json_schema_best_effort = CapabilityStatus::Recognized;
    features.structured_output.named_schemas = CapabilityStatus::Recognized;

    // Tool calling: routed pass-through, model-dependent; not adapter-fixture-tested.
    features.tool_calling.function_calling = CapabilityStatus::Recognized;
    features.tool_calling.parallel_tool_calls = CapabilityStatus::Recognized;
    features.tool_calling.streaming_tool_calls = CapabilityStatus::Recognized;
    features.tool_calling.tool_choice_modes = vec![
        "auto".into(),
        "none".into(),
        "required".into(),
        "named".into(),
    ];

    // Routing: typed carrier deferred — provider-specific in v0.9.4.
    features.routing.provider_routing = CapabilityStatus::ProviderSpecific;
    features.routing.require_parameters = CapabilityStatus::ProviderSpecific;
    features.routing.fallback_policy = CapabilityStatus::ProviderSpecific;

    // Modalities: vision pass-through depends on the upstream model.
    features.modalities.vision_input = CapabilityStatus::Recognized;

    // Logprobs: routed pass-through; not adapter-tested.
    features.logprobs.logprobs = CapabilityStatus::Recognized;
    features.logprobs.streaming_logprobs = CapabilityStatus::Recognized;

    let mut ev: HashMap<String, CapabilityEvidence> = HashMap::new();
    ev.insert(
        "structured_output".into(),
        evidence(docs_struct_out, openrouter_caps_test),
    );
    ev.insert(
        "tool_calling".into(),
        evidence(docs_tools, openrouter_caps_test),
    );
    let mut routing = evidence(docs_routing, openrouter_test_module);
    routing.notes = vec![
        "Provider routing + require_parameters require a typed carrier (deferred).".into(),
        format!("Source: {docs_routing}"),
    ];
    ev.insert("routing".into(), routing);
    ev.insert(
        "modalities".into(),
        evidence(docs_struct_out, openrouter_message_test),
    );
    ev.insert(
        "logprobs".into(),
        evidence(docs_struct_out, openrouter_test_module),
    );

    ProviderCapabilityRecord {
        provider_id: "openrouter".into(),
        display_name: "OpenRouter".into(),
        last_reviewed_on: SPRINT_REVIEW_DATE.into(),
        default_model: None,
        features,
        evidence: ev,
        model_overrides: HashMap::new(),
        provider_specific: Value::Null,
    }
}

/// Ollama capability record. The native `format` field with a JSON Schema
/// document is documented and adapter-mapped (see
/// `test_ollama_format_json_schema_serialization` and
/// `test_convert_response_format_json_schema`). Thinking/extended-reasoning
/// is documented and exercised by the existing thinking-aware stream
/// chunk tests. Embeddings and tool calling are documented but the
/// shared SDK surface is deferred to FR-014 / Phase 4 — `Recognized`.
pub fn ollama() -> ProviderCapabilityRecord {
    let docs_api = "https://github.com/ollama/ollama/blob/main/docs/api.md";
    let docs_struct_out = "https://ollama.com/blog/structured-outputs";
    let docs_thinking = "https://ollama.com/blog/thinking";

    let ollama_test_module =
        "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/ollama.rs::tests";
    let ollama_format_schema_test = "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/ollama.rs::tests::test_ollama_format_json_schema_serialization";
    let ollama_format_json_test = "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/ollama.rs::tests::test_ollama_format_json_serialization";
    let ollama_thinking_test = "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/ollama.rs::tests::test_ollama_message_with_thinking_deserialization";
    let ollama_message_test = "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/ollama.rs::tests::test_message_conversion";

    let mut features = CapabilityFeatureSet::default();

    // Reasoning: Ollama thinking blocks are documented and the existing
    // adapter exposes `ThinkingMode` plus thinking-aware stream parsing.
    features.reasoning.thinking_blocks = CapabilityStatus::Supported;

    // Structured output: native `format` field accepts both `"json"` and a
    // full JSON Schema document. Both paths are adapter-tested.
    features.structured_output.json_mode = CapabilityStatus::Supported;
    features.structured_output.json_schema_strict = CapabilityStatus::Supported;
    features.structured_output.json_schema_best_effort = CapabilityStatus::Supported;

    // Tool calling: documented but model-dependent and not adapter-fixture-tested.
    features.tool_calling.function_calling = CapabilityStatus::Recognized;
    features.tool_calling.streaming_tool_calls = CapabilityStatus::Recognized;

    // Modalities: vision via llava-family models is documented; not adapter-
    // fixture-tested in tree, so Recognized.
    features.modalities.vision_input = CapabilityStatus::Recognized;
    // Embeddings — shared SDK surface decision pending (FR-014).
    features.modalities.embeddings = CapabilityStatus::Recognized;

    // Logprobs: not part of the Ollama API.
    features.logprobs.logprobs = CapabilityStatus::Unsupported;
    features.logprobs.streaming_logprobs = CapabilityStatus::Unsupported;

    let mut ev: HashMap<String, CapabilityEvidence> = HashMap::new();
    ev.insert(
        "reasoning".into(),
        evidence(docs_thinking, ollama_thinking_test),
    );
    ev.insert(
        "structured_output.json_mode".into(),
        evidence(docs_struct_out, ollama_format_json_test),
    );
    ev.insert(
        "structured_output.json_schema_strict".into(),
        evidence(docs_struct_out, ollama_format_schema_test),
    );
    ev.insert(
        "structured_output.json_schema_best_effort".into(),
        evidence(docs_struct_out, ollama_format_schema_test),
    );
    ev.insert(
        "tool_calling".into(),
        evidence(docs_api, ollama_test_module),
    );
    ev.insert("modalities".into(), evidence(docs_api, ollama_message_test));

    ProviderCapabilityRecord {
        provider_id: "ollama".into(),
        display_name: "Ollama".into(),
        last_reviewed_on: SPRINT_REVIEW_DATE.into(),
        default_model: None,
        features,
        evidence: ev,
        model_overrides: HashMap::new(),
        provider_specific: Value::Null,
    }
}

/// LM Studio capability record. The current adapter only wires model
/// listing; structured output, tool calling, and vision are documented in
/// LM Studio's OpenAI-compatible API but not adapter-fixture-tested in
/// tree. Held conservatively at `Recognized` so the registry does not
/// over-claim. Local MCP is documented but `Recognized` per OD-2 posture.
pub fn lmstudio() -> ProviderCapabilityRecord {
    let docs_api = "https://lmstudio.ai/docs/api/openai-api";
    let docs_struct_out = "https://lmstudio.ai/docs/api/structured-output";
    let docs_tools = "https://lmstudio.ai/docs/app/tool-use";

    let lmstudio_test_module =
        "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/lmstudio.rs::tests";
    let lmstudio_model_info_test = "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/lmstudio.rs::tests::test_model_info_conversion";

    let mut features = CapabilityFeatureSet::default();

    // OpenAI-compatible — structured output and tool calling exist on paper
    // but the in-tree adapter does not exercise them. Stay Recognized.
    features.structured_output.json_mode = CapabilityStatus::Recognized;
    features.structured_output.json_schema_strict = CapabilityStatus::Recognized;
    features.structured_output.json_schema_best_effort = CapabilityStatus::Recognized;

    features.tool_calling.function_calling = CapabilityStatus::Recognized;

    // Local MCP: documented, OD-2 posture keeps it Recognized.
    features.hosted_tools.mcp_connector = CapabilityStatus::Recognized;

    // Vision: model-dependent, not adapter-tested.
    features.modalities.vision_input = CapabilityStatus::Recognized;

    // Embeddings: OpenAI-compatible /v1/embeddings exists; not in-tree mapped.
    features.modalities.embeddings = CapabilityStatus::Recognized;

    // Logprobs: not part of the LM Studio chat surface today.
    features.logprobs.logprobs = CapabilityStatus::Unknown;

    let mut ev: HashMap<String, CapabilityEvidence> = HashMap::new();
    ev.insert(
        "structured_output".into(),
        evidence(docs_struct_out, lmstudio_test_module),
    );
    ev.insert(
        "tool_calling".into(),
        evidence(docs_tools, lmstudio_test_module),
    );
    let mut hosted = evidence(docs_tools, lmstudio_test_module);
    hosted.notes = vec!["Local MCP support is documented; held at Recognized per OD-2.".into()];
    ev.insert("hosted_tools".into(), hosted);
    ev.insert(
        "modalities".into(),
        evidence(docs_api, lmstudio_model_info_test),
    );

    ProviderCapabilityRecord {
        provider_id: "lmstudio".into(),
        display_name: "LM Studio".into(),
        last_reviewed_on: SPRINT_REVIEW_DATE.into(),
        default_model: None,
        features,
        evidence: ev,
        model_overrides: HashMap::new(),
        provider_specific: Value::Null,
    }
}

/// Fireworks AI capability record. JSON object mode and grammar-based
/// structured output are documented and adapter-mapped (vision-aware
/// message conversion + JSON-mode request shaping). Tool calling is
/// documented but not adapter-fixture-tested in tree — `Recognized`.
pub fn fireworks() -> ProviderCapabilityRecord {
    let docs_chat = "https://docs.fireworks.ai/api-reference/post-chatcompletions";
    let docs_grammar =
        "https://docs.fireworks.ai/structured-responses/structured-output-grammar-based";
    let docs_struct_resp =
        "https://docs.fireworks.ai/structured-responses/structured-response-formatting";

    let fireworks_test_module =
        "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/fireworks.rs::tests";
    let fireworks_caps_test = "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/fireworks.rs::tests::test_fireworks_capabilities";
    let fireworks_json_mode_test = "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/fireworks.rs::tests::pro_tests::test_build_request_with_json_mode";
    let fireworks_vision_test = "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/fireworks.rs::tests::pro_tests::test_message_conversion_with_vision";

    let mut features = CapabilityFeatureSet::default();

    features.structured_output.json_mode = CapabilityStatus::Supported;
    features.structured_output.json_schema_strict = CapabilityStatus::Recognized;
    features.structured_output.json_schema_best_effort = CapabilityStatus::Recognized;

    features.tool_calling.function_calling = CapabilityStatus::Recognized;

    features.modalities.vision_input = CapabilityStatus::Supported;

    features.logprobs.logprobs = CapabilityStatus::Unknown;

    let mut ev: HashMap<String, CapabilityEvidence> = HashMap::new();
    ev.insert(
        "structured_output.json_mode".into(),
        evidence(docs_struct_resp, fireworks_json_mode_test),
    );
    ev.insert(
        "structured_output.json_schema_strict".into(),
        evidence(docs_grammar, fireworks_caps_test),
    );
    ev.insert(
        "structured_output.json_schema_best_effort".into(),
        evidence(docs_grammar, fireworks_caps_test),
    );
    ev.insert(
        "tool_calling".into(),
        evidence(docs_chat, fireworks_test_module),
    );
    ev.insert(
        "modalities".into(),
        evidence(docs_chat, fireworks_vision_test),
    );

    ProviderCapabilityRecord {
        provider_id: "fireworks".into(),
        display_name: "Fireworks AI".into(),
        last_reviewed_on: SPRINT_REVIEW_DATE.into(),
        default_model: None,
        features,
        evidence: ev,
        model_overrides: HashMap::new(),
        provider_specific: Value::Null,
    }
}

/// Mock provider capability record. The mock provider exists for tests
/// only; capabilities here describe what the in-tree mock adapter
/// actually exposes (chat + stream + list-models). No external doc URL
/// applies, so evidence cites the mock provider source as the
/// authoritative spec.
pub fn mock() -> ProviderCapabilityRecord {
    let mock_source = "https://github.com/nxus-SYSTEMS/nxusKit/blob/main/packages/nxuskit-engine/crates/nxuskit-engine/src/providers/mock.rs";
    let mock_test_module =
        "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/mock.rs::tests";
    let mock_chat_test = "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/mock.rs::tests::test_mock_provider_chat";
    let mock_stream_test = "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/mock.rs::tests::test_mock_provider_stream";

    let mut features = CapabilityFeatureSet::default();

    // The mock provider deterministically returns canned text; nothing more.
    // Logprobs / structured output / tool calling are not part of its surface.
    features.structured_output.json_mode = CapabilityStatus::Unsupported;
    features.structured_output.json_schema_strict = CapabilityStatus::Unsupported;
    features.tool_calling.function_calling = CapabilityStatus::Unsupported;
    features.modalities.vision_input = CapabilityStatus::Unsupported;
    features.logprobs.logprobs = CapabilityStatus::Unsupported;
    features.logprobs.streaming_logprobs = CapabilityStatus::Unsupported;

    let mut ev: HashMap<String, CapabilityEvidence> = HashMap::new();
    let mut chat_ev = evidence(mock_source, mock_chat_test);
    chat_ev.notes = vec!["Test-only deterministic provider; no live API.".into()];
    ev.insert("structured_output".into(), chat_ev);

    let mut stream_ev = evidence(mock_source, mock_stream_test);
    stream_ev.notes = vec!["Streaming is exercised by test_mock_provider_stream.".into()];
    ev.insert("logprobs".into(), stream_ev);

    ev.insert(
        "tool_calling".into(),
        evidence(mock_source, mock_test_module),
    );
    ev.insert("modalities".into(), evidence(mock_source, mock_test_module));

    ProviderCapabilityRecord {
        provider_id: "mock".into(),
        display_name: "Mock provider".into(),
        last_reviewed_on: SPRINT_REVIEW_DATE.into(),
        default_model: None,
        features,
        evidence: ev,
        model_overrides: HashMap::new(),
        provider_specific: Value::Null,
    }
}

/// Loopback provider capability record. The loopback provider is a
/// deterministic in-process echo/U-turn provider used by adapter-validation
/// and parameter-warning tests. Evidence cites the in-tree provider source
/// plus the rich loopback test suite that already covers JSON-native
/// behavior, synthesized logprobs, and limited-Claude / limited-minimal
/// guardrails.
pub fn loopback() -> ProviderCapabilityRecord {
    let loopback_source = "https://github.com/nxus-SYSTEMS/nxusKit/blob/main/packages/nxuskit-engine/crates/nxuskit-engine/src/providers/loopback.rs";
    let loopback_test_module =
        "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/loopback.rs::tests";
    let loopback_echo_test = "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/loopback.rs::tests::test_echo_basic";
    let loopback_logprobs_test = "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/loopback.rs::tests::test_echo_synthesizes_logprobs_when_requested";
    let loopback_uturn_test = "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/loopback.rs::tests::test_uturn_json_valid";
    let loopback_json_native_test = "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/loopback.rs::tests::test_json_native_no_warning";

    let mut features = CapabilityFeatureSet::default();

    // Loopback explicitly supports a JSON-native U-turn mode plus an
    // adapter-warning JSON fallback path — both adapter-tested.
    features.structured_output.json_mode = CapabilityStatus::Supported;

    // Loopback synthesizes logprobs when requested (test_echo_synthesizes_logprobs_when_requested).
    features.logprobs.logprobs = CapabilityStatus::Supported;
    features.logprobs.streaming_logprobs = CapabilityStatus::Unsupported;

    // No real tool calling, vision, or hosted tools.
    features.tool_calling.function_calling = CapabilityStatus::Unsupported;
    features.modalities.vision_input = CapabilityStatus::Unsupported;
    features.structured_output.json_schema_strict = CapabilityStatus::Unsupported;

    let mut ev: HashMap<String, CapabilityEvidence> = HashMap::new();
    let mut json_ev = evidence(loopback_source, loopback_json_native_test);
    json_ev.notes = vec![
        "JSON-native U-turn mode covered by tests::test_uturn_json_valid + test_json_native_no_warning."
            .into(),
        format!("Additional evidence: {loopback_uturn_test}"),
    ];
    ev.insert("structured_output".into(), json_ev);

    let mut logprobs_ev = evidence(loopback_source, loopback_logprobs_test);
    logprobs_ev.notes =
        vec!["Loopback synthesizes logprobs deterministically when requested.".into()];
    ev.insert("logprobs".into(), logprobs_ev);

    ev.insert(
        "tool_calling".into(),
        evidence(loopback_source, loopback_test_module),
    );
    ev.insert(
        "modalities".into(),
        evidence(loopback_source, loopback_echo_test),
    );

    ProviderCapabilityRecord {
        provider_id: "loopback".into(),
        display_name: "Loopback provider".into(),
        last_reviewed_on: SPRINT_REVIEW_DATE.into(),
        default_model: None,
        features,
        evidence: ev,
        model_overrides: HashMap::new(),
        provider_specific: Value::Null,
    }
}
