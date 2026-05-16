//! Evidence-gated provider capability registry (feature 099 — Phase 2).
//!
//! This module is the foundational typed capability model that Phase 3+
//! provider records will populate with real evidence. It is internal-first:
//! types live in the engine crate and remain hidden from public docs until
//! the Manifest v2 publication decision (FR-015) promotes them.
//!
//! Backward-compatibility commitments (per `data-model.md`):
//!
//! - The legacy boolean [`crate::types::ProviderCapabilities`] struct and all
//!   its public fields are preserved unchanged.
//! - [`derive_legacy_capabilities`] bridges the new [`CapabilityFeatureSet`]
//!   into the legacy boolean struct so existing call sites keep compiling and
//!   serializing.
//! - [`crate::types::ChatRequest`] / [`crate::types::ChatResponse`] /
//!   [`crate::types::StreamChunk`] are not modified in this phase; the new
//!   typed request carriers ([`StructuredOutputConfig`], [`ToolCallConfig`],
//!   [`OpenAIResponsesOptions`], etc.) live here and will be wired through
//!   adapters in Phase 4 (US2) and Phase 5 (US3).

#![allow(missing_docs)]
#![doc(hidden)]

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::types::ProviderCapabilities;

pub mod provider_records;

// ---------------------------------------------------------------------------
// CapabilityStatus
// ---------------------------------------------------------------------------

/// Evidence-gated capability status for a provider feature.
///
/// `Supported` requires at least one [`CapabilityEvidence`] with a non-empty
/// `source_url` and at least one of `adapter_test`, `fixture_path`, or
/// `live_test`. Validation is enforced by [`CapabilityEvidence::is_evidence_for_supported`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityStatus {
    /// Officially documented, adapter-mapped, and fixture/live-tested.
    Supported,
    /// Provider lacks this feature or rejects it.
    Unsupported,
    /// Provider documents the feature; nxusKit has not mapped or tested it yet.
    Recognized,
    /// Real feature that must stay in a provider-specific namespace.
    ProviderSpecific,
    /// Known feature deliberately deferred to a future sprint.
    Future,
    /// Evidence not yet reviewed or is stale.
    #[default]
    Unknown,
}

impl CapabilityStatus {
    /// Human-friendly display string used by CLI human-mode output.
    pub fn human(self) -> &'static str {
        match self {
            CapabilityStatus::Supported => "yes",
            CapabilityStatus::Unsupported => "no",
            CapabilityStatus::Recognized => "recognized",
            CapabilityStatus::ProviderSpecific => "provider-specific",
            CapabilityStatus::Future => "deferred",
            CapabilityStatus::Unknown => "unknown",
        }
    }
}

// ---------------------------------------------------------------------------
// CapabilityEvidence
// ---------------------------------------------------------------------------

/// Per-feature evidence record. Implementors MUST refresh `source_reviewed_on`
/// whenever the linked official documentation page changes (FR-002, FR-018).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityEvidence {
    /// Official provider documentation URL for this feature.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    /// ISO 8601 date when the evidence was last reviewed (`YYYY-MM-DD`).
    pub source_reviewed_on: String,
    /// Test function name proving adapter request/response behavior.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter_test: Option<String>,
    /// Path to a CE-safe recorded fixture proving wire shape.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fixture_path: Option<String>,
    /// `#[ignore]`-gated live test function name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub live_test: Option<String>,
    /// Additional constraints (model scoping, beta headers, tier requirements).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

/// Reason an evidence record is not yet sufficient for [`CapabilityStatus::Supported`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceGapReason {
    MissingSourceUrl,
    MissingProof,
}

impl CapabilityEvidence {
    /// Returns `Ok(())` when this evidence record satisfies the FR-018 rule
    /// for promoting a feature status to `Supported`.
    ///
    /// All four fields (`source_url`, `adapter_test`, `fixture_path`,
    /// `live_test`) are checked for non-empty content, not just `Some(_)`:
    /// `Some(String::new())` is treated as missing, since an empty identifier
    /// cannot serve as evidence.
    pub fn is_evidence_for_supported(&self) -> Result<(), EvidenceGapReason> {
        fn non_empty(slot: &Option<String>) -> bool {
            slot.as_deref().is_some_and(|s| !s.is_empty())
        }
        if !non_empty(&self.source_url) {
            return Err(EvidenceGapReason::MissingSourceUrl);
        }
        if !non_empty(&self.adapter_test)
            && !non_empty(&self.fixture_path)
            && !non_empty(&self.live_test)
        {
            return Err(EvidenceGapReason::MissingProof);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Feature modules (T012)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReasoningCapabilities {
    pub effort_control: CapabilityStatus,
    pub reasoning_summary: CapabilityStatus,
    pub thinking_blocks: CapabilityStatus,
    pub reasoning_content_field: CapabilityStatus,
    #[serde(default)]
    pub reasoning_billed_separately: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StructuredOutputCapabilities {
    pub json_mode: CapabilityStatus,
    pub json_schema_strict: CapabilityStatus,
    pub json_schema_best_effort: CapabilityStatus,
    pub named_schemas: CapabilityStatus,
    pub additionalprops_false: CapabilityStatus,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolCallingCapabilities {
    pub function_calling: CapabilityStatus,
    pub parallel_tool_calls: CapabilityStatus,
    pub streaming_tool_calls: CapabilityStatus,
    /// Tool-choice modes the provider accepts (e.g. `["auto","none","required","named"]`).
    #[serde(default)]
    pub tool_choice_modes: Vec<String>,
}

/// OpenAI hosted-tool entries may be promoted to `Supported` only for explicit
/// Responses transport with full evidence. Non-OpenAI hosted/server-side tools
/// remain `Recognized` per OD-2 in v0.9.4.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HostedToolCapabilities {
    pub web_search: CapabilityStatus,
    pub file_search: CapabilityStatus,
    pub code_interpreter: CapabilityStatus,
    pub image_generation: CapabilityStatus,
    pub computer_use: CapabilityStatus,
    pub mcp_connector: CapabilityStatus,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchCitationCapabilities {
    pub search_controls: CapabilityStatus,
    pub citation_metadata: CapabilityStatus,
    pub grounding_metadata: CapabilityStatus,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModalityCapabilities {
    pub vision_input: CapabilityStatus,
    pub audio_input: CapabilityStatus,
    pub audio_output: CapabilityStatus,
    pub embeddings: CapabilityStatus,
    pub rerank: CapabilityStatus,
    pub moderation: CapabilityStatus,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RoutingCapabilities {
    pub provider_routing: CapabilityStatus,
    pub require_parameters: CapabilityStatus,
    pub fallback_policy: CapabilityStatus,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StateCapabilities {
    pub previous_response_id: CapabilityStatus,
    pub response_phase: CapabilityStatus,
    pub prompt_caching: CapabilityStatus,
    pub context_caching: CapabilityStatus,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LogprobCapabilities {
    pub logprobs: CapabilityStatus,
    pub streaming_logprobs: CapabilityStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_top_logprobs: Option<u8>,
}

/// Aggregate per-provider feature statuses. All statuses default to `Unknown`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapabilityFeatureSet {
    #[serde(default)]
    pub reasoning: ReasoningCapabilities,
    #[serde(default)]
    pub structured_output: StructuredOutputCapabilities,
    #[serde(default)]
    pub tool_calling: ToolCallingCapabilities,
    #[serde(default)]
    pub hosted_tools: HostedToolCapabilities,
    #[serde(default)]
    pub search_citations: SearchCitationCapabilities,
    #[serde(default)]
    pub modalities: ModalityCapabilities,
    #[serde(default)]
    pub routing: RoutingCapabilities,
    #[serde(default)]
    pub state: StateCapabilities,
    #[serde(default)]
    pub logprobs: LogprobCapabilities,
}

// ---------------------------------------------------------------------------
// ProviderCapabilityRecord and ModelCapabilityOverride
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCapabilityRecord {
    pub provider_id: String,
    pub display_name: String,
    /// ISO 8601 date of most recent evidence review (`YYYY-MM-DD`).
    pub last_reviewed_on: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    #[serde(default)]
    pub features: CapabilityFeatureSet,
    #[serde(default)]
    pub evidence: HashMap<String, CapabilityEvidence>,
    #[serde(default)]
    pub model_overrides: HashMap<String, ModelCapabilityOverride>,
    #[serde(default)]
    pub provider_specific: serde_json::Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelCapabilityOverride {
    pub reviewed_on: String,
    #[serde(default)]
    pub feature_overrides: HashMap<String, CapabilityStatus>,
    #[serde(default)]
    pub notes: Vec<String>,
}

// ---------------------------------------------------------------------------
// Request types (T012 — typed surfaces consumed by Phase 4/5)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredOutputConfig {
    pub mode: StructuredOutputMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_subset: Option<SchemaSubsetKind>,
}

/// Pure config-level errors for [`StructuredOutputConfig`]. Provider-aware
/// validation lives in [`validate_request_against_record`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StructuredOutputError {
    /// `JsonSchema` mode requires a non-null, non-empty `schema`.
    JsonSchemaModeMissingSchema,
}

impl StructuredOutputConfig {
    /// Pure config-level shape check. `JsonSchema` mode requires a
    /// non-null, non-empty `schema`; other modes have no shape
    /// constraints. Provider-aware checks (strict-mode capability,
    /// JSON-object provider warnings) live in
    /// [`validate_request_against_record`].
    pub fn validate_shape(&self) -> Result<(), StructuredOutputError> {
        match self.mode {
            StructuredOutputMode::JsonSchema => {
                let schema_is_present = match self.schema.as_ref() {
                    None => false,
                    Some(serde_json::Value::Null) => false,
                    Some(serde_json::Value::Object(map)) => !map.is_empty(),
                    Some(_) => true,
                };
                if schema_is_present {
                    Ok(())
                } else {
                    Err(StructuredOutputError::JsonSchemaModeMissingSchema)
                }
            }
            StructuredOutputMode::JsonObject | StructuredOutputMode::Text => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StructuredOutputMode {
    Text,
    JsonObject,
    JsonSchema,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchemaSubsetKind {
    Full,
    Restricted,
    BestEffort,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallConfig {
    pub tools: Vec<ToolDefinition>,
    pub tool_choice: ToolChoice,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub streaming_tool_calls: Option<bool>,
}

/// Pure config-level errors for [`ToolCallConfig`]. Provider-aware errors
/// (unsupported individual tools, missing namespacing for provider-specific
/// hosted tools) come back as [`ValidationOutcome::Block`] from
/// [`validate_request_against_record`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolCallError {
    /// A [`ToolDefinition`] has an empty `name`.
    EmptyToolName,
    /// At least one tool was provided but `tool_choice == None`. The caller
    /// almost certainly meant `Auto`, `Required`, or a `Named` choice.
    ToolsProvidedButChoiceIsNone,
}

impl ToolCallConfig {
    /// Pure config-level shape check. Errors when any tool has an empty
    /// `name`, or when the request supplies tools but pins
    /// `tool_choice = ToolChoice::None` (almost always a caller bug).
    /// Provider-aware per-tool gating (unsupported / namespaced
    /// hosted-tool checks) lives in [`validate_request_against_record`].
    pub fn validate_shape(&self) -> Result<(), ToolCallError> {
        if self.tools.iter().any(|t| t.name.is_empty()) {
            return Err(ToolCallError::EmptyToolName);
        }
        if !self.tools.is_empty() && matches!(self.tool_choice, ToolChoice::None) {
            return Err(ToolCallError::ToolsProvidedButChoiceIsNone);
        }
        Ok(())
    }
}

/// OpenAI-compatible Chat Completions wire-shape helpers. Each returns the
/// JSON value that an OpenAI / OpenAI-compatible adapter (OpenAI, Mistral,
/// Groq, Together, OpenRouter, Fireworks) splices into the outgoing
/// request body. Returning `Value::Null` is the omit-the-field signal —
/// adapters skip the field rather than serialize a JSON null.
pub mod openai_wire {
    use super::{
        ProviderCapabilityRecord, StructuredOutputConfig, StructuredOutputMode, ToolCallConfig,
        ToolChoice, ToolDefinition, hosted_tool_status_for,
    };

    /// Returns the value for the OpenAI `response_format` request field,
    /// or `Null` to omit it entirely (Text mode, the default).
    pub fn response_format(cfg: &StructuredOutputConfig) -> serde_json::Value {
        match cfg.mode {
            StructuredOutputMode::Text => serde_json::Value::Null,
            StructuredOutputMode::JsonObject => {
                serde_json::json!({"type": "json_object"})
            }
            StructuredOutputMode::JsonSchema => {
                let mut json_schema = serde_json::Map::new();
                if let Some(name) = cfg.schema_name.as_deref() {
                    json_schema.insert("name".into(), serde_json::Value::String(name.into()));
                }
                if let Some(strict) = cfg.strict {
                    json_schema.insert("strict".into(), serde_json::Value::Bool(strict));
                }
                if let Some(schema) = cfg.schema.clone() {
                    json_schema.insert("schema".into(), schema);
                }
                serde_json::json!({
                    "type": "json_schema",
                    "json_schema": serde_json::Value::Object(json_schema),
                })
            }
        }
    }

    /// Returns the value for the OpenAI `tools` request field (a JSON
    /// array of `{type: "function", function: {...}}` entries) without
    /// any provider-aware filtering. Use [`tools_for`] when you have a
    /// [`ProviderCapabilityRecord`] available.
    pub fn tools(cfg: &ToolCallConfig) -> serde_json::Value {
        let entries: Vec<serde_json::Value> =
            cfg.tools.iter().map(serialize_function_tool).collect();
        serde_json::Value::Array(entries)
    }

    /// Returns the value for the OpenAI `tools` request field, filtered
    /// according to `record`: any tool whose name matches a hosted-tool
    /// capability is omitted from Chat Completions function-tool
    /// serialization. Hosted tools are first-class Responses transport
    /// options, not function definitions.
    pub fn tools_for(record: &ProviderCapabilityRecord, cfg: &ToolCallConfig) -> serde_json::Value {
        let entries: Vec<serde_json::Value> = cfg
            .tools
            .iter()
            .filter(|t| hosted_tool_status_for(record, &t.name).is_none())
            .map(serialize_function_tool)
            .collect();
        serde_json::Value::Array(entries)
    }

    /// Returns the value for the OpenAI `tool_choice` request field.
    pub fn tool_choice(cfg: &ToolCallConfig) -> serde_json::Value {
        match &cfg.tool_choice {
            ToolChoice::Auto => serde_json::Value::String("auto".into()),
            ToolChoice::None => serde_json::Value::String("none".into()),
            ToolChoice::Required => serde_json::Value::String("required".into()),
            ToolChoice::Named(name) => serde_json::json!({
                "type": "function",
                "function": {"name": name},
            }),
        }
    }

    /// Provider-aware `tool_choice` variant. Named choices that point at a
    /// hosted-tool capability are omitted because [`tools_for`] also filters
    /// hosted tools out of Chat Completions function-tool payloads.
    pub fn tool_choice_for(
        record: &ProviderCapabilityRecord,
        cfg: &ToolCallConfig,
    ) -> serde_json::Value {
        if let ToolChoice::Named(name) = &cfg.tool_choice
            && hosted_tool_status_for(record, name).is_some()
        {
            return serde_json::Value::Null;
        }
        tool_choice(cfg)
    }

    fn serialize_function_tool(tool: &ToolDefinition) -> serde_json::Value {
        let mut function = serde_json::Map::new();
        function.insert("name".into(), serde_json::Value::String(tool.name.clone()));
        if let Some(desc) = tool.description.as_deref() {
            function.insert("description".into(), serde_json::Value::String(desc.into()));
        }
        function.insert("parameters".into(), tool.parameters.clone());
        if let Some(strict) = tool.strict {
            function.insert("strict".into(), serde_json::Value::Bool(strict));
        }
        serde_json::json!({
            "type": "function",
            "function": serde_json::Value::Object(function),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parameters: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolChoice {
    Auto,
    None,
    Required,
    Named(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallDelta {
    pub index: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments_delta: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments_complete: Option<String>,
}

/// Decoded tool-call response — the typed form of an entry in
/// `ChatResponse.tool_calls`. Adapter wiring for provider-specific decoding
/// lands with T035 (OpenAI) and T036–T038 (Mistral / Groq / Together).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecodedToolCall {
    /// Provider-assigned tool-call ID, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Function (or hosted-tool) name the model invoked.
    pub name: String,
    /// JSON-encoded argument string as the model emitted it.
    pub arguments_raw: String,
    /// Optionally pre-parsed arguments. Adapters may populate this when
    /// `arguments_raw` is valid JSON; consumers should fall back to
    /// reparsing `arguments_raw` if `arguments` is `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OpenAIResponsesOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ReasoningConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_verbosity: Option<TextVerbosity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub include: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hosted_tools: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_search: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextVerbosity {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReasoningConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default)]
    pub include_encrypted_content: bool,
    #[serde(default)]
    pub preserve_blocks: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchOptions {
    #[serde(default)]
    pub query_controls: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub provider_specific: serde_json::Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CitationMetadata {
    #[serde(default)]
    pub citations: Vec<Citation>,
    #[serde(default)]
    pub provider_metadata: serde_json::Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Citation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    #[serde(default)]
    pub provider_fields: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Validation (T014)
// ---------------------------------------------------------------------------

/// Feature being requested by a [`crate::types::ChatRequest`]-equivalent
/// payload at validation time. New variants will be added in Phase 4/5 as
/// adapters wire typed surfaces through.
#[derive(Debug, Clone)]
pub enum FeatureRequest {
    StructuredOutput(StructuredOutputConfig),
    ToolCall(ToolCallConfig),
    OpenAIResponses(OpenAIResponsesOptions),
    Search(SearchOptions),
}

impl FeatureRequest {
    pub fn structured_output(cfg: StructuredOutputConfig) -> Self {
        FeatureRequest::StructuredOutput(cfg)
    }
    pub fn tool_call(cfg: ToolCallConfig) -> Self {
        FeatureRequest::ToolCall(cfg)
    }
    pub fn openai_responses(opts: OpenAIResponsesOptions) -> Self {
        FeatureRequest::OpenAIResponses(opts)
    }
    pub fn search(opts: SearchOptions) -> Self {
        FeatureRequest::Search(opts)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub enum ValidationOutcome {
    /// Capability is `Supported`; request may be serialized.
    Allow,
    /// Capability is `Recognized`/`ProviderSpecific`/`Unknown`; warn and
    /// proceed (the adapter may still drop or ignore the field).
    Warn {
        feature: &'static str,
        reason: String,
    },
    /// Capability is `Unsupported`/`Future`; block before any network I/O.
    Block {
        feature: &'static str,
        reason: String,
    },
}

impl ValidationOutcome {
    pub fn severity(&self) -> ValidationSeverity {
        match self {
            ValidationOutcome::Allow => ValidationSeverity::Info,
            ValidationOutcome::Warn { .. } => ValidationSeverity::Warning,
            ValidationOutcome::Block { .. } => ValidationSeverity::Error,
        }
    }
}

/// Pre-network validation: maps a [`FeatureRequest`] to the relevant
/// capability status on the record and returns a [`ValidationOutcome`] that
/// adapters can consume *before* serializing to the wire.
pub fn validate_request_against_record(
    record: &ProviderCapabilityRecord,
    request: &FeatureRequest,
) -> ValidationOutcome {
    match request {
        FeatureRequest::StructuredOutput(cfg) => validate_structured_output(record, cfg),
        FeatureRequest::ToolCall(cfg) => validate_tool_call(record, cfg),
        FeatureRequest::OpenAIResponses(_) => classify_status(
            "openai_responses",
            record.features.state.previous_response_id,
        ),
        FeatureRequest::Search(_) => classify_status(
            "search_controls",
            record.features.search_citations.search_controls,
        ),
    }
}

/// Validate the typed request surfaces that are spliced into provider
/// request bodies during Phase 4. Shape errors are reported as blocking
/// outcomes before provider capability status is considered.
pub fn validate_typed_request_parts(
    record: &ProviderCapabilityRecord,
    structured_output: Option<&StructuredOutputConfig>,
    tool_call_config: Option<&ToolCallConfig>,
) -> Vec<ValidationOutcome> {
    let mut outcomes = Vec::new();

    if let Some(cfg) = structured_output {
        if let Err(err) = cfg.validate_shape() {
            outcomes.push(ValidationOutcome::Block {
                feature: "structured_output",
                reason: match err {
                    StructuredOutputError::JsonSchemaModeMissingSchema => {
                        "json_schema mode requires a non-null, non-empty schema".into()
                    }
                },
            });
        } else {
            outcomes.push(validate_request_against_record(
                record,
                &FeatureRequest::structured_output(cfg.clone()),
            ));
        }
    }

    if let Some(cfg) = tool_call_config {
        if let Err(err) = cfg.validate_shape() {
            outcomes.push(ValidationOutcome::Block {
                feature: "tool_calling",
                reason: match err {
                    ToolCallError::EmptyToolName => "tool names must be non-empty".into(),
                    ToolCallError::ToolsProvidedButChoiceIsNone => {
                        "tool_choice none cannot be used when tools are provided".into()
                    }
                },
            });
        } else {
            outcomes.push(validate_request_against_record(
                record,
                &FeatureRequest::tool_call(cfg.clone()),
            ));
        }
    }

    outcomes
}

fn validate_structured_output(
    record: &ProviderCapabilityRecord,
    cfg: &StructuredOutputConfig,
) -> ValidationOutcome {
    match cfg.mode {
        StructuredOutputMode::Text => ValidationOutcome::Allow,
        StructuredOutputMode::JsonObject => {
            let status = record.features.structured_output.json_mode;
            match status {
                CapabilityStatus::Supported => ValidationOutcome::Allow,
                CapabilityStatus::Unsupported => ValidationOutcome::Block {
                    feature: "structured_output",
                    reason: "provider does not support json_object mode".into(),
                },
                CapabilityStatus::Future => ValidationOutcome::Block {
                    feature: "structured_output",
                    reason: "json_object mode is deferred to a future sprint".into(),
                },
                _ => {
                    // Recognized / ProviderSpecific / Unknown — surface the
                    // provider's evidence note when present so callers see
                    // the doc-driven warning verbatim.
                    let provider_note = record
                        .evidence
                        .get("structured_output")
                        .or_else(|| record.evidence.get("structured_output.json_mode"))
                        .map(|ev| ev.notes.join(" "))
                        .filter(|s| !s.is_empty());
                    let base = match status {
                        CapabilityStatus::Recognized => {
                            "json_object is documented but not adapter-mapped yet"
                        }
                        CapabilityStatus::ProviderSpecific => {
                            "json_object requires a provider-specific namespace"
                        }
                        _ => "json_object status has not been reviewed",
                    };
                    let reason = match provider_note {
                        Some(note) => format!("{base}: {note}"),
                        None => base.into(),
                    };
                    ValidationOutcome::Warn {
                        feature: "structured_output",
                        reason,
                    }
                }
            }
        }
        StructuredOutputMode::JsonSchema => {
            if cfg.strict.unwrap_or(false) {
                // Strict-mode: must be Supported. Anything else Blocks
                // before network I/O.
                let status = record.features.structured_output.json_schema_strict;
                if status == CapabilityStatus::Supported {
                    ValidationOutcome::Allow
                } else {
                    ValidationOutcome::Block {
                        feature: "structured_output",
                        reason: format!(
                            "strict json_schema requires Supported \
                             json_schema_strict capability; provider status is {status:?}"
                        ),
                    }
                }
            } else {
                // Best-effort: fall back to best-effort capability when
                // declared, otherwise to strict capability evidence.
                let be = record.features.structured_output.json_schema_best_effort;
                let status = if be == CapabilityStatus::Unknown {
                    record.features.structured_output.json_schema_strict
                } else {
                    be
                };
                classify_status("structured_output", status)
            }
        }
    }
}

/// Names of hosted-tool capability slots, used for per-tool gating.
const HOSTED_TOOL_NAMES: &[&str] = &[
    "web_search",
    "file_search",
    "code_interpreter",
    "image_generation",
    "computer_use",
    "mcp_connector",
];

/// Returns the hosted-tool capability status for a given tool name on
/// `record`, or `None` if the name does not match any hosted-tool slot.
fn hosted_tool_status_for(
    record: &ProviderCapabilityRecord,
    tool_name: &str,
) -> Option<CapabilityStatus> {
    let bare = tool_name
        .strip_prefix(&format!("{}.", record.provider_id))
        .unwrap_or(tool_name);
    if !HOSTED_TOOL_NAMES.contains(&bare) {
        return None;
    }
    let h = &record.features.hosted_tools;
    Some(match bare {
        "web_search" => h.web_search,
        "file_search" => h.file_search,
        "code_interpreter" => h.code_interpreter,
        "image_generation" => h.image_generation,
        "computer_use" => h.computer_use,
        "mcp_connector" => h.mcp_connector,
        _ => return None,
    })
}

fn validate_tool_call(
    record: &ProviderCapabilityRecord,
    cfg: &ToolCallConfig,
) -> ValidationOutcome {
    // Per-tool capability gating runs before the aggregate function-calling
    // status, so a stray Unsupported hosted-tool fails the request even if
    // the provider supports plain function calling.
    let mut hosted_warning: Option<String> = None;

    for tool in &cfg.tools {
        let Some(status) = hosted_tool_status_for(record, &tool.name) else {
            continue; // Plain function tool — gated only by aggregate status below.
        };
        let namespaced = tool.name.starts_with(&format!("{}.", record.provider_id));
        match status {
            CapabilityStatus::Supported => {}
            CapabilityStatus::Unsupported => {
                return ValidationOutcome::Block {
                    feature: "tool_calling",
                    reason: format!(
                        "tool {:?} maps to an Unsupported hosted-tool capability",
                        tool.name
                    ),
                };
            }
            CapabilityStatus::Future => {
                return ValidationOutcome::Block {
                    feature: "tool_calling",
                    reason: format!(
                        "tool {:?} maps to a hosted-tool capability deferred to a future sprint",
                        tool.name
                    ),
                };
            }
            CapabilityStatus::ProviderSpecific => {
                if !namespaced {
                    return ValidationOutcome::Block {
                        feature: "tool_calling",
                        reason: format!(
                            "tool {:?} requires a provider-specific namespace; \
                             try {:?}",
                            tool.name,
                            format!("{}.{}", record.provider_id, tool.name)
                        ),
                    };
                } else if hosted_warning.is_none() {
                    hosted_warning = Some(format!(
                        "tool {:?} maps to a provider-specific hosted-tool capability; \
                         adapter may omit first-class hosted-tool serialization",
                        tool.name
                    ));
                }
            }
            CapabilityStatus::Recognized | CapabilityStatus::Unknown => {
                if hosted_warning.is_none() {
                    hosted_warning = Some(format!(
                        "tool {:?} maps to a hosted-tool capability that is {status:?}; \
                         adapter will not serialize it as a first-class hosted tool",
                        tool.name
                    ));
                }
            }
        }
    }

    let aggregate = classify_status(
        "tool_calling",
        record.features.tool_calling.function_calling,
    );
    match aggregate {
        ValidationOutcome::Allow => hosted_warning
            .map(|reason| ValidationOutcome::Warn {
                feature: "tool_calling",
                reason,
            })
            .unwrap_or(ValidationOutcome::Allow),
        other => other,
    }
}

fn classify_status(feature: &'static str, status: CapabilityStatus) -> ValidationOutcome {
    match status {
        CapabilityStatus::Supported => ValidationOutcome::Allow,
        CapabilityStatus::Unsupported => ValidationOutcome::Block {
            feature,
            reason: format!("provider does not support {feature}"),
        },
        CapabilityStatus::Future => ValidationOutcome::Block {
            feature,
            reason: format!("{feature} is deferred to a future sprint"),
        },
        CapabilityStatus::Recognized => ValidationOutcome::Warn {
            feature,
            reason: format!("{feature} is documented but not adapter-mapped yet"),
        },
        CapabilityStatus::ProviderSpecific => ValidationOutcome::Warn {
            feature,
            reason: format!("{feature} requires a provider-specific namespace"),
        },
        CapabilityStatus::Unknown => ValidationOutcome::Warn {
            feature,
            reason: format!("{feature} status has not been reviewed"),
        },
    }
}

// ---------------------------------------------------------------------------
// Legacy bridge (T013)
// ---------------------------------------------------------------------------

/// Project a [`CapabilityFeatureSet`] onto the legacy boolean
/// [`ProviderCapabilities`] surface. Phase 3 provider records keep the
/// historical defaults of [`ProviderCapabilities::default`] for unset fields;
/// only fields with `Supported` status flip on.
pub fn derive_legacy_capabilities(fs: &CapabilityFeatureSet) -> ProviderCapabilities {
    let supports_logprobs = matches!(fs.logprobs.logprobs, CapabilityStatus::Supported);
    ProviderCapabilities {
        // Streaming is a transport concern; existing wiring keeps owning it.
        supports_streaming: false,
        supports_vision: matches!(fs.modalities.vision_input, CapabilityStatus::Supported),
        supports_json_mode: matches!(fs.structured_output.json_mode, CapabilityStatus::Supported),
        supports_json_schema: matches!(
            fs.structured_output.json_schema_strict,
            CapabilityStatus::Supported
        ) || matches!(
            fs.structured_output.json_schema_best_effort,
            CapabilityStatus::Supported
        ),
        supports_logprobs,
        supports_streaming_logprobs: supports_logprobs
            && matches!(fs.logprobs.streaming_logprobs, CapabilityStatus::Supported),
        max_logprobs: if supports_logprobs {
            fs.logprobs.max_top_logprobs
        } else {
            None
        },
        ..ProviderCapabilities::default()
    }
}

// ---------------------------------------------------------------------------
// Registry (Phase 2 skeleton — Phase 3 fills in evidence and per-feature
// statuses; this scaffold guarantees registry coverage tests pass).
// ---------------------------------------------------------------------------

pub mod registry {
    use super::{CapabilityFeatureSet, CapabilityStatus, ProviderCapabilityRecord};
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;

    const SPRINT_REVIEW_DATE: &str = "2026-05-09";

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ProviderKind {
        Existing,
        Synthetic,
        Candidate,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum ManifestPublicationPosture {
        Split,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PublicCapabilityManifest {
        pub schema_version: String,
        pub posture: ManifestPublicationPosture,
        pub providers: Vec<PublicProviderCapability>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PublicProviderCapability {
        pub name: String,
        pub display_name: String,
        pub last_reviewed_on: String,
        pub provider_status: String,
        pub capabilities: HashMap<String, CapabilityStatus>,
    }

    /// (provider_id, display_name, kind, default_model)
    const KNOWN_PROVIDERS: &[(&str, &str, ProviderKind, Option<&str>)] = &[
        ("openai", "OpenAI", ProviderKind::Existing, Some("gpt-5.5")),
        (
            "anthropic",
            "Anthropic Claude",
            ProviderKind::Existing,
            Some("claude-sonnet-4-5"),
        ),
        ("mistral", "Mistral AI", ProviderKind::Existing, None),
        ("groq", "Groq", ProviderKind::Existing, None),
        ("perplexity", "Perplexity", ProviderKind::Existing, None),
        ("together", "Together AI", ProviderKind::Existing, None),
        ("openrouter", "OpenRouter", ProviderKind::Existing, None),
        ("ollama", "Ollama", ProviderKind::Existing, None),
        ("lmstudio", "LM Studio", ProviderKind::Existing, None),
        ("fireworks", "Fireworks AI", ProviderKind::Existing, None),
        ("mock", "Mock provider", ProviderKind::Synthetic, None),
        (
            "loopback",
            "Loopback provider",
            ProviderKind::Synthetic,
            None,
        ),
        (
            "gemini",
            "Google Gemini (candidate)",
            ProviderKind::Candidate,
            None,
        ),
        ("xai", "xAI Grok", ProviderKind::Existing, Some("grok-4")),
        (
            "cohere",
            "Cohere (candidate)",
            ProviderKind::Candidate,
            None,
        ),
        (
            "deepseek",
            "DeepSeek (candidate)",
            ProviderKind::Candidate,
            None,
        ),
    ];

    /// All registry records. Evidence-backed records are pulled from the
    /// `super::provider_records` module as they land in Phase 3 (US1);
    /// providers without an evidence-backed record yet fall back to a
    /// placeholder default so registry coverage stays complete.
    pub fn all_records() -> Vec<ProviderCapabilityRecord> {
        KNOWN_PROVIDERS
            .iter()
            .map(|(id, name, _kind, default_model)| match *id {
                "openai" => super::provider_records::openai(),
                "anthropic" => super::provider_records::anthropic(),
                "mistral" => super::provider_records::mistral(),
                "groq" => super::provider_records::groq(),
                "perplexity" => super::provider_records::perplexity(),
                "together" => super::provider_records::together(),
                "openrouter" => super::provider_records::openrouter(),
                "ollama" => super::provider_records::ollama(),
                "lmstudio" => super::provider_records::lmstudio(),
                "fireworks" => super::provider_records::fireworks(),
                "mock" => super::provider_records::mock(),
                "loopback" => super::provider_records::loopback(),
                "gemini" => crate::providers::candidate::gemini::record(),
                "xai" => super::provider_records::xai(),
                "cohere" => crate::providers::candidate::cohere::record(),
                "deepseek" => crate::providers::candidate::deepseek::record(),
                _ => ProviderCapabilityRecord {
                    provider_id: (*id).into(),
                    display_name: (*name).into(),
                    last_reviewed_on: SPRINT_REVIEW_DATE.into(),
                    default_model: default_model.map(|s| s.to_string()),
                    features: CapabilityFeatureSet::default(),
                    evidence: HashMap::new(),
                    model_overrides: HashMap::new(),
                    provider_specific: serde_json::Value::Null,
                },
            })
            .collect()
    }

    pub fn find(provider_id: &str) -> Option<ProviderCapabilityRecord> {
        all_records()
            .into_iter()
            .find(|r| r.provider_id == provider_id)
    }

    /// Public preview projection for Capability Manifest v2.
    ///
    /// This deliberately omits internal-only fields such as evidence,
    /// model_overrides, provider_specific, and nested feature structs. The
    /// full internal registry remains available through [`all_records`].
    pub fn public_manifest() -> PublicCapabilityManifest {
        PublicCapabilityManifest {
            schema_version: "capability-manifest-v2-public-preview/1".into(),
            posture: ManifestPublicationPosture::Split,
            providers: all_records()
                .into_iter()
                .map(|record| PublicProviderCapability {
                    name: record.provider_id,
                    display_name: record.display_name,
                    last_reviewed_on: record.last_reviewed_on,
                    provider_status: "unknown".into(),
                    capabilities: public_capability_map(&record.features),
                })
                .collect(),
        }
    }

    fn public_capability_map(fs: &CapabilityFeatureSet) -> HashMap<String, CapabilityStatus> {
        HashMap::from([
            ("vision_input".into(), fs.modalities.vision_input),
            ("tool_calling".into(), fs.tool_calling.function_calling),
            ("thinking_blocks".into(), fs.reasoning.thinking_blocks),
            ("streaming_logprobs".into(), fs.logprobs.streaming_logprobs),
            ("json_mode".into(), fs.structured_output.json_mode),
            (
                "json_schema_strict".into(),
                fs.structured_output.json_schema_strict,
            ),
            (
                "json_schema_best_effort".into(),
                fs.structured_output.json_schema_best_effort,
            ),
            ("embeddings".into(), fs.modalities.embeddings),
            ("rerank".into(), fs.modalities.rerank),
        ])
    }

    /// Provider IDs categorized as candidate-direct providers (FR-013).
    /// Used by Phase 6 guard tests to enforce that no candidate ships an
    /// adapter implementation in v0.9.4.
    pub fn candidate_provider_ids() -> Vec<&'static str> {
        KNOWN_PROVIDERS
            .iter()
            .filter(|(_, _, kind, _)| *kind == ProviderKind::Candidate)
            .map(|(id, _, _, _)| *id)
            .collect()
    }
}
