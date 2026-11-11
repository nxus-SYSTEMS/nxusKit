//! Public graph projection DTOs shared by Pipeline and Orchestration.

use serde::{Deserialize, Serialize};

use super::orchestration::{
    OrchestrationDefinition, OrchestrationEdgeKind, OrchestrationNodeKind, OrchestrationPosture,
};
use super::{CONTRACT_VERSION, PipelineDefinition, PipelineValidationOutcome, StageType};

/// Stable public graph projection for SDK definitions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphProjection {
    pub contract_version: String,
    pub source_kind: GraphSourceKind,
    pub source_id: String,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fan_out_groups: Vec<GraphFanOutGroup>,
    pub validation_status: GraphValidationStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_hints: Option<LayoutHints>,
}

impl GraphProjection {
    /// Build a graph projection from a declaration-only orchestration.
    pub fn from_orchestration(def: &OrchestrationDefinition) -> Self {
        let validation_status = if def.validate().is_ok() {
            GraphValidationStatus::Valid
        } else {
            GraphValidationStatus::Invalid
        };
        let nodes = def
            .nodes
            .iter()
            .map(|node| GraphNode {
                id: node.id.clone(),
                label: node_label(
                    &node.id,
                    node.provider_ref.as_deref(),
                    node.model_ref.as_deref(),
                    node.capability_ref.as_deref(),
                ),
                kind: match node.kind {
                    OrchestrationNodeKind::Provider => GraphNodeKind::ProviderAction,
                    OrchestrationNodeKind::ToolCapability => GraphNodeKind::ToolCapability,
                    OrchestrationNodeKind::RulesProvider => GraphNodeKind::RulesProvider,
                    OrchestrationNodeKind::ExternalAction => GraphNodeKind::ExternalAction,
                    OrchestrationNodeKind::Branch => GraphNodeKind::Branch,
                    OrchestrationNodeKind::Convergence => GraphNodeKind::Convergence,
                    OrchestrationNodeKind::ConditionalRoute => GraphNodeKind::ConditionalRoute,
                },
                posture: match node.posture {
                    OrchestrationPosture::DeclarationOnly => GraphNodePosture::DeclarationOnly,
                    OrchestrationPosture::ExternalOnly => GraphNodePosture::ExternalOnly,
                    OrchestrationPosture::Deferred => GraphNodePosture::Deferred,
                    OrchestrationPosture::MissingPlugin => GraphNodePosture::MissingPlugin,
                },
                provider_ref: node.provider_ref.clone(),
                model_ref: node.model_ref.clone(),
                capability_ref: node.capability_ref.clone(),
            })
            .collect();
        let edges = def
            .edges
            .iter()
            .map(|edge| GraphEdge {
                from: edge.from.clone(),
                to: edge.to.clone(),
                kind: match edge.kind {
                    OrchestrationEdgeKind::Data => GraphEdgeKind::Data,
                    OrchestrationEdgeKind::Control => GraphEdgeKind::Control,
                    OrchestrationEdgeKind::Conditional => GraphEdgeKind::Conditional,
                },
                label: edge.condition.clone(),
            })
            .collect();
        Self {
            contract_version: CONTRACT_VERSION.to_owned(),
            source_kind: GraphSourceKind::Orchestration,
            source_id: def.id.clone(),
            nodes,
            edges,
            fan_out_groups: Vec::new(),
            validation_status,
            layout_hints: None,
        }
    }

    /// Build a basic graph projection from a pipeline definition.
    pub fn from_pipeline(def: &PipelineDefinition) -> Self {
        let validation_status = match def.validate() {
            PipelineValidationOutcome::Ready => GraphValidationStatus::Valid,
            _ => GraphValidationStatus::Invalid,
        };
        let nodes = def
            .stages
            .iter()
            .map(|stage| GraphNode {
                id: stage.name.as_str().to_owned(),
                label: stage.name.as_str().to_owned(),
                kind: match stage.stage_type {
                    StageType::Tool => GraphNodeKind::ToolCapability,
                    StageType::ClipsRuleEval => GraphNodeKind::RulesProvider,
                    _ => GraphNodeKind::Stage,
                },
                posture: match stage.stage_type {
                    StageType::Llm | StageType::ClipsRuleEval => GraphNodePosture::Executable,
                    StageType::Tool => GraphNodePosture::ExternalOnly,
                    StageType::BatchSource
                    | StageType::BatchSink
                    | StageType::DataStoreSearch
                    | StageType::Solver
                    | StageType::Bayesian
                    | StageType::Zen => GraphNodePosture::DeclarationOnly,
                },
                provider_ref: stage.provider_ref.as_ref().map(|p| p.as_str().to_owned()),
                model_ref: stage.model_ref.clone(),
                capability_ref: Some(stage.capability_ref.as_str().to_owned()),
            })
            .collect();
        Self {
            contract_version: CONTRACT_VERSION.to_owned(),
            source_kind: GraphSourceKind::Pipeline,
            source_id: def.pipeline_id.as_str().to_owned(),
            nodes,
            edges: Vec::new(),
            fan_out_groups: Vec::new(),
            validation_status,
            layout_hints: None,
        }
    }

    /// Render this projection as deterministic Mermaid.
    pub fn render_mermaid(&self) -> String {
        let mut out = String::from("flowchart TD\n");
        for node in &self.nodes {
            out.push_str(&format!(
                "  {}[\"{}: {}\"]\n",
                mermaid_id(&node.id),
                escape_mermaid(&node.id),
                escape_mermaid(&node.label)
            ));
        }
        for edge in &self.edges {
            let label = edge.label.as_deref().unwrap_or(edge.kind.as_str());
            let arrow = match edge.kind {
                GraphEdgeKind::Data => "-->",
                GraphEdgeKind::Control => "-.->",
                GraphEdgeKind::Conditional => "-.->",
                GraphEdgeKind::FanOut => "==>",
            };
            out.push_str(&format!(
                "  {} {}|{}| {}\n",
                mermaid_id(&edge.from),
                arrow,
                escape_mermaid(label),
                mermaid_id(&edge.to)
            ));
        }
        out.push_str("  %% posture classes: executable external_only declaration_only deferred missing_plugin entitlement_required unsupported_stage_type\n");
        out.push_str("  classDef external_only stroke-dasharray: 3 3;\n");
        out.push_str("  classDef declaration_only stroke-dasharray: 5 5;\n");
        out.push_str("  classDef deferred stroke-dasharray: 2 4;\n");
        out.push_str("  classDef missing_plugin stroke-width: 3px;\n");
        out.push_str("  classDef entitlement_required stroke-width: 3px;\n");
        out.push_str("  classDef unsupported_stage_type stroke-width: 3px;\n");
        for node in &self.nodes {
            if node.posture != GraphNodePosture::Executable {
                out.push_str(&format!(
                    "  class {} {};\n",
                    mermaid_id(&node.id),
                    node.posture.as_str()
                ));
            }
        }
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphSourceKind {
    Pipeline,
    Orchestration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub kind: GraphNodeKind,
    pub posture: GraphNodePosture,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_ref: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphNodeKind {
    Stage,
    ProviderAction,
    ToolCapability,
    RulesProvider,
    ExternalAction,
    Branch,
    Convergence,
    ConditionalRoute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphNodePosture {
    Executable,
    ExternalOnly,
    DeclarationOnly,
    Deferred,
    MissingPlugin,
    EntitlementRequired,
    UnsupportedStageType,
}

impl GraphNodePosture {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Executable => "executable",
            Self::ExternalOnly => "external_only",
            Self::DeclarationOnly => "declaration_only",
            Self::Deferred => "deferred",
            Self::MissingPlugin => "missing_plugin",
            Self::EntitlementRequired => "entitlement_required",
            Self::UnsupportedStageType => "unsupported_stage_type",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    pub kind: GraphEdgeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphEdgeKind {
    Data,
    Control,
    Conditional,
    FanOut,
}

impl GraphEdgeKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Data => "data",
            Self::Control => "control",
            Self::Conditional => "conditional",
            Self::FanOut => "fan_out",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphFanOutGroup {
    pub group_id: String,
    pub member_node_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphValidationStatus {
    Valid,
    Invalid,
    Partial,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutHints {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ranks: Vec<LayoutRank>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutRank {
    pub rank: u32,
    pub node_ids: Vec<String>,
}

fn node_label(
    id: &str,
    provider_ref: Option<&str>,
    model_ref: Option<&str>,
    capability_ref: Option<&str>,
) -> String {
    match (provider_ref, model_ref, capability_ref) {
        (Some(provider), Some(model), _) => format!("{provider} {model}"),
        (Some(provider), None, _) => provider.to_owned(),
        (_, _, Some(capability)) => capability.to_owned(),
        _ => id.to_owned(),
    }
}

fn mermaid_id(raw: &str) -> String {
    let mut out = String::from("n_");
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    out
}

fn escape_mermaid(raw: &str) -> String {
    raw.replace('\\', "\\\\").replace('"', "\\\"")
}
