//! Feature 106: public v1.1 RC stage / operation outcome DTOs.
//!
//! These DTOs are additive on top of the v1.1 public contract surface.
//! They are emitted by the contract-native pipeline runner introduced
//! by feature 106 (v1.1.0 RC Peeler CLI update).
//!
//! Naming note: the existing
//! [`crate::contract::v1_1::ids::CapabilityDomain`] enum names capability
//! *kinds* (Llm/Tool/RuleEval/...). The new
//! [`RuntimeDomain`] enum below names the runtime/plugin domains that a
//! typed *non-execution* outcome targets (`arrow_runtime`,
//! `substrait_adapter`, `duckdb_adapter`, `zenoh_transport`,
//! `tool_emit_artifact`, `clips_rule_eval`). They are distinct concepts;
//! the two enums intentionally do NOT share variants.
//!
//! All variant strings are stable and documented in the public stage-outcome
//! vocabulary.

use serde::{Deserialize, Serialize};

/// Per-stage outcome shape emitted by the v1.1 contract-native runner.
///
/// JSON tagging is internally tagged on the field `outcome`. Other fields
/// (`reason`, `domain`, `detail`, `action`) are flattened siblings depending
/// on the variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum StageOutcome {
    /// Stage executed and produced a result.
    Success,
    /// Stage executed but failed.
    Failure { reason: String },
    /// Stage deliberately not run (e.g., a dependency failure already
    /// broke the chain).
    Skipped { reason: String },
    /// Stage references a capability that is declaration-only in v1.1.0
    /// RC (e.g., Substrait plan references).
    DeclarationOnly {
        domain: RuntimeDomain,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
    /// Capability requires a plugin that is not installed.
    MissingPlugin {
        domain: RuntimeDomain,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
    /// Capability requires an entitlement the current install does not
    /// have.
    EntitlementRequired {
        domain: RuntimeDomain,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
    /// Capability execution deferred for v1.1.0 RC (e.g., Zenoh
    /// transport).
    Deferred {
        domain: RuntimeDomain,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
    /// Stage type is not recognized or not supported by the v1.1.0 RC
    /// runner (e.g., a CLIPS factory-orchestrator request).
    UnsupportedStageType {
        domain: RuntimeDomain,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
    /// Stage is external-only and emits an external action request
    /// rather than executing.
    ExternalActionRequired {
        domain: RuntimeDomain,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
}

impl StageOutcome {
    /// Returns the short status string ("success" / "failure" / "skipped"
    /// / etc.) for human-readable summary output.
    pub fn status_str(&self) -> &'static str {
        match self {
            StageOutcome::Success => "success",
            StageOutcome::Failure { .. } => "failure",
            StageOutcome::Skipped { .. } => "skipped",
            StageOutcome::DeclarationOnly { .. } => "declaration_only",
            StageOutcome::MissingPlugin { .. } => "missing_plugin",
            StageOutcome::EntitlementRequired { .. } => "entitlement_required",
            StageOutcome::Deferred { .. } => "deferred",
            StageOutcome::UnsupportedStageType { .. } => "unsupported_stage_type",
            StageOutcome::ExternalActionRequired { .. } => "external_action_required",
        }
    }

    /// Returns true if the stage executed (success or failure).
    pub fn is_executed(&self) -> bool {
        matches!(self, StageOutcome::Success | StageOutcome::Failure { .. })
    }
}

/// Per-fan-out-operation outcome shape. Mirrors [`StageOutcome`] variant
/// for variant; kept as a sibling enum so fan-out per-op fields and
/// stage-level fields stay clearly distinct in JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum OperationOutcome {
    Success,
    Failure {
        reason: String,
    },
    Skipped {
        reason: String,
    },
    DeclarationOnly {
        domain: RuntimeDomain,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
    MissingPlugin {
        domain: RuntimeDomain,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
    EntitlementRequired {
        domain: RuntimeDomain,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
    Deferred {
        domain: RuntimeDomain,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
    UnsupportedStageType {
        domain: RuntimeDomain,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
}

impl OperationOutcome {
    pub fn status_str(&self) -> &'static str {
        match self {
            OperationOutcome::Success => "success",
            OperationOutcome::Failure { .. } => "failure",
            OperationOutcome::Skipped { .. } => "skipped",
            OperationOutcome::DeclarationOnly { .. } => "declaration_only",
            OperationOutcome::MissingPlugin { .. } => "missing_plugin",
            OperationOutcome::EntitlementRequired { .. } => "entitlement_required",
            OperationOutcome::Deferred { .. } => "deferred",
            OperationOutcome::UnsupportedStageType { .. } => "unsupported_stage_type",
        }
    }
}

/// Runtime / plugin domain referenced by a typed non-execution outcome.
///
/// Distinct from [`crate::contract::v1_1::ids::CapabilityDomain`] which
/// describes capability *kinds*. `RuntimeDomain` names the runtime
/// integration whose absence (or deferred posture) the outcome reports.
///
/// Variant strings are documented in the public stage-outcome vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeDomain {
    /// Arrow runtime (declaration-only / missing-plugin in CE).
    ArrowRuntime,
    /// Substrait plan adapter (declaration-only in v1.1.0 RC).
    SubstraitAdapter,
    /// DuckDB / Data Store runtime adapter.
    DuckdbAdapter,
    /// Zenoh transport (deferred in v1.1.0 RC).
    ZenohTransport,
    /// `tool.emit_artifact` external-only tool capability.
    ToolEmitArtifact,
    /// CLIPS rules-provider boundary (`rule_eval.generic`). Used when
    /// a CLIPS orchestration / non-generic request is rejected.
    ClipsRuleEval,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn stage_outcome_success_round_trips_as_simple_variant() {
        let outcome = StageOutcome::Success;
        let json = serde_json::to_value(&outcome).expect("ser");
        assert_eq!(json, json!({"outcome": "success"}));
        let parsed: StageOutcome = serde_json::from_value(json).expect("de");
        assert_eq!(parsed, outcome);
    }

    #[test]
    fn stage_outcome_declaration_only_carries_runtime_domain() {
        let outcome = StageOutcome::DeclarationOnly {
            domain: RuntimeDomain::SubstraitAdapter,
            detail: Some("Substrait plan ref declaration-only in v1.1.0 RC".into()),
        };
        let json = serde_json::to_value(&outcome).expect("ser");
        assert_eq!(json["outcome"], "declaration_only");
        assert_eq!(json["domain"], "substrait_adapter");
        assert!(json["detail"].is_string());
        let parsed: StageOutcome = serde_json::from_value(json).expect("de");
        assert_eq!(parsed, outcome);
    }

    #[test]
    fn stage_outcome_missing_plugin_arrow_round_trips() {
        let outcome = StageOutcome::MissingPlugin {
            domain: RuntimeDomain::ArrowRuntime,
            detail: None,
        };
        let json = serde_json::to_value(&outcome).expect("ser");
        assert_eq!(json["outcome"], "missing_plugin");
        assert_eq!(json["domain"], "arrow_runtime");
        let parsed: StageOutcome = serde_json::from_value(json).expect("de");
        assert_eq!(parsed, outcome);
    }

    #[test]
    fn stage_outcome_deferred_zenoh_round_trips() {
        let outcome = StageOutcome::Deferred {
            domain: RuntimeDomain::ZenohTransport,
            detail: None,
        };
        let json = serde_json::to_value(&outcome).expect("ser");
        assert_eq!(json["outcome"], "deferred");
        assert_eq!(json["domain"], "zenoh_transport");
        let parsed: StageOutcome = serde_json::from_value(json).expect("de");
        assert_eq!(parsed, outcome);
    }

    #[test]
    fn stage_outcome_external_action_required_carries_tool_domain() {
        let outcome = StageOutcome::ExternalActionRequired {
            domain: RuntimeDomain::ToolEmitArtifact,
            detail: Some("tool.emit_artifact is external-only".into()),
        };
        let json = serde_json::to_value(&outcome).expect("ser");
        assert_eq!(json["outcome"], "external_action_required");
        assert_eq!(json["domain"], "tool_emit_artifact");
        let parsed: StageOutcome = serde_json::from_value(json).expect("de");
        assert_eq!(parsed, outcome);
    }

    #[test]
    fn stage_outcome_unsupported_stage_type_carries_clips_domain() {
        let outcome = StageOutcome::UnsupportedStageType {
            domain: RuntimeDomain::ClipsRuleEval,
            detail: Some("CLIPS orchestration is not supported in v1.1.0 RC".into()),
        };
        let json = serde_json::to_value(&outcome).expect("ser");
        assert_eq!(json["outcome"], "unsupported_stage_type");
        assert_eq!(json["domain"], "clips_rule_eval");
        let parsed: StageOutcome = serde_json::from_value(json).expect("de");
        assert_eq!(parsed, outcome);
    }

    #[test]
    fn runtime_domain_variants_serialize_to_documented_snake_case() {
        for (variant, expected) in [
            (RuntimeDomain::ArrowRuntime, "arrow_runtime"),
            (RuntimeDomain::SubstraitAdapter, "substrait_adapter"),
            (RuntimeDomain::DuckdbAdapter, "duckdb_adapter"),
            (RuntimeDomain::ZenohTransport, "zenoh_transport"),
            (RuntimeDomain::ToolEmitArtifact, "tool_emit_artifact"),
            (RuntimeDomain::ClipsRuleEval, "clips_rule_eval"),
        ] {
            let json = serde_json::to_value(variant).expect("ser");
            assert_eq!(json, expected, "variant {variant:?} expected `{expected}`");
        }
    }

    #[test]
    fn operation_outcome_mirrors_stage_outcome_variants() {
        let outcome = OperationOutcome::Success;
        let json = serde_json::to_value(&outcome).expect("ser");
        assert_eq!(json, json!({"outcome": "success"}));

        let outcome = OperationOutcome::Failure {
            reason: "rate-limited".into(),
        };
        let json = serde_json::to_value(&outcome).expect("ser");
        assert_eq!(json["outcome"], "failure");
        assert_eq!(json["reason"], "rate-limited");
    }
}
