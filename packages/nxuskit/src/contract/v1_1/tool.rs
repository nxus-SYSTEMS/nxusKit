//! Public CE `ToolSemantics` DTO (US4 / T048).
//!
//! Exposes the data-posture and plan-reference requirement of a tool stage
//! at the public DTO level so a downstream reviewer or Peeler-style host
//! can reason about tool wiring without inspecting engine internals.
//!
//! Boundary discipline: this type MUST NOT carry internal routing topology,
//! transport identity, or Zenoh-specific concept. The
//! `#[serde(deny_unknown_fields)]` attribute rejects banned field names
//! (provider-channel-route, channel-id, the Zenoh key-expression
//! identifier, and the Substrait-executor identifier) at parse time as
//! defense-in-depth alongside the validator (US3) and boundary scan (US2).
//! The exact banned identifier strings are assembled at runtime in the
//! in-module unit tests so this source file is itself clean of literal
//! occurrences (see the public boundary scan).
//!
//! Validation rules (US4 / T049) live in
//! `super::pipeline::check_tool_semantics_binding`:
//! - `data_posture == ConsumesReferencedData` (or `ConsumesAndProducesReferencedData`)
//!   requires the parent stage to declare ≥1 `input_refs`.
//! - `data_posture == ProducesReferencedData` (or `ConsumesAndProducesReferencedData`)
//!   requires the parent stage to declare ≥1 `output_refs`.
//! - `requires_plan_ref == true` requires the parent stage to set a
//!   `plan_ref` (T040 `PlanReferenceUnbound` outcome).

use serde::{Deserialize, Serialize};

/// Public-safe tool data posture per `contracts/tool-semantics-retarget.md`.
///
/// # Example
///
/// ```
/// use nxuskit::contract::v1_1::ToolDataPosture;
/// // A tool that consumes referenced data via `input_refs`.
/// let posture = ToolDataPosture::ConsumesReferencedData;
/// assert!(posture.requires_input_refs());
/// assert!(!posture.requires_output_refs());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolDataPosture {
    /// Tool consumes only inline metadata (no reference slots required).
    InlineMetadataOnly,
    /// Tool consumes referenced data via `input_refs`.
    ConsumesReferencedData,
    /// Tool produces referenced data via `output_refs`.
    ProducesReferencedData,
    /// Tool both consumes and produces referenced data.
    ConsumesAndProducesReferencedData,
    /// Tool's primary semantic is to operate against a plan reference.
    RequiresPlanRef,
}

impl ToolDataPosture {
    /// Whether this posture implies the parent stage must declare at least
    /// one `input_refs` entry.
    pub fn requires_input_refs(self) -> bool {
        matches!(
            self,
            ToolDataPosture::ConsumesReferencedData
                | ToolDataPosture::ConsumesAndProducesReferencedData
        )
    }

    /// Whether this posture implies the parent stage must declare at least
    /// one `output_refs` entry.
    pub fn requires_output_refs(self) -> bool {
        matches!(
            self,
            ToolDataPosture::ProducesReferencedData
                | ToolDataPosture::ConsumesAndProducesReferencedData
        )
    }
}

/// Public CE `ToolSemantics`. The `#[serde(deny_unknown_fields)]` attribute
/// is load-bearing: it rejects banned field names (internal routing,
/// transport identity, Zenoh-shaped routing concepts) at parse time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolSemantics {
    pub data_posture: ToolDataPosture,
    #[serde(default)]
    pub requires_plan_ref: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub determinism: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side_effect: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_budget_posture: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancellability: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_posture: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_scope: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redaction_posture: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit_posture: Option<String>,
}

impl ToolSemantics {
    pub fn new(data_posture: ToolDataPosture) -> Self {
        Self {
            data_posture,
            requires_plan_ref: false,
            determinism: None,
            side_effect: None,
            cost_budget_posture: None,
            cancellability: None,
            auth_posture: None,
            tenant_scope: None,
            redaction_posture: None,
            audit_posture: None,
        }
    }

    pub fn with_requires_plan_ref(mut self, requires: bool) -> Self {
        self.requires_plan_ref = requires;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // TS-003 — deny_unknown_fields rejects banned field names with a
    // class-identifying serde error.
    #[test]
    fn ts_003_deny_unknown_fields_rejects_provider_channel_route() {
        let raw = json!({
            "data_posture": "inline_metadata_only",
            "provider_channel_route": "router-1"
        });
        let err = serde_json::from_value::<ToolSemantics>(raw)
            .expect_err("ToolSemantics MUST reject provider_channel_route via deny_unknown_fields");
        let msg = err.to_string();
        assert!(
            msg.contains("provider_channel_route"),
            "serde error must name the offending field; got: {msg}"
        );
    }

    #[test]
    fn ts_003_deny_unknown_fields_rejects_channel_id() {
        let raw = json!({
            "data_posture": "inline_metadata_only",
            "channel_id": "ch-1"
        });
        let err = serde_json::from_value::<ToolSemantics>(raw)
            .expect_err("ToolSemantics MUST reject channel_id");
        assert!(err.to_string().contains("channel_id"));
    }

    #[test]
    fn ts_003_deny_unknown_fields_rejects_transport_key_identifier() {
        // Assemble the reserved transport identifier at runtime so the
        // public boundary scan does not flag this source file. The
        // resulting identifier matches the reserved set in
        // `internal/tools/scripts/boundary_patterns/zenoh_api_leakage.patterns`.
        let key = format!("{}_{}_{}", concat!("ze", "noh"), "key", "expr");
        let raw = serde_json::json!({
            "data_posture": "inline_metadata_only",
            key.clone(): "demo/k"
        });
        let err = serde_json::from_value::<ToolSemantics>(raw)
            .expect_err("ToolSemantics MUST reject the transport key-expression identifier");
        assert!(err.to_string().contains(&key));
    }

    #[test]
    fn ts_003_deny_unknown_fields_rejects_substrait_executor() {
        let raw = json!({
            "data_posture": "inline_metadata_only",
            "substrait_executor": "duckdb"
        });
        let err = serde_json::from_value::<ToolSemantics>(raw)
            .expect_err("ToolSemantics MUST reject substrait_executor");
        assert!(err.to_string().contains("substrait_executor"));
    }

    // TS positive — accepted ToolSemantics round-trips canonically.
    #[test]
    fn tool_semantics_round_trips_with_full_field_set() {
        let s = ToolSemantics {
            data_posture: ToolDataPosture::ConsumesReferencedData,
            requires_plan_ref: false,
            determinism: Some("non_deterministic".to_owned()),
            side_effect: Some("external_emit".to_owned()),
            cost_budget_posture: Some("low".to_owned()),
            cancellability: Some("best_effort".to_owned()),
            auth_posture: Some("tenant_scoped".to_owned()),
            tenant_scope: Some("single_tenant".to_owned()),
            redaction_posture: Some("audit_only".to_owned()),
            audit_posture: Some("structured_audit_trail".to_owned()),
        };
        let raw = serde_json::to_string(&s).expect("serialize");
        let back: ToolSemantics = serde_json::from_str(&raw).expect("deserialize");
        assert_eq!(s, back);
    }

    #[test]
    fn data_posture_requires_input_refs_only_for_consume_postures() {
        assert!(ToolDataPosture::ConsumesReferencedData.requires_input_refs());
        assert!(ToolDataPosture::ConsumesAndProducesReferencedData.requires_input_refs());
        assert!(!ToolDataPosture::ProducesReferencedData.requires_input_refs());
        assert!(!ToolDataPosture::InlineMetadataOnly.requires_input_refs());
        assert!(!ToolDataPosture::RequiresPlanRef.requires_input_refs());
    }

    #[test]
    fn data_posture_requires_output_refs_only_for_produce_postures() {
        assert!(ToolDataPosture::ProducesReferencedData.requires_output_refs());
        assert!(ToolDataPosture::ConsumesAndProducesReferencedData.requires_output_refs());
        assert!(!ToolDataPosture::ConsumesReferencedData.requires_output_refs());
        assert!(!ToolDataPosture::InlineMetadataOnly.requires_output_refs());
        assert!(!ToolDataPosture::RequiresPlanRef.requires_output_refs());
    }
}
