//! Public v1.1 declaration-only orchestration DTOs.
//!
//! v1.1.0 RC supports validate, normalize, hash, and graph operations for
//! orchestration definitions. It deliberately does not define runtime
//! execution semantics.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use super::{ArtifactCanonicalHash, CONTRACT_VERSION, ContractError, canonical_hash};

/// Declaration-only orchestration definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrchestrationDefinition {
    pub contract_version: String,
    pub id: String,
    pub nodes: Vec<OrchestrationNode>,
    #[serde(default)]
    pub edges: Vec<OrchestrationEdge>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

impl OrchestrationDefinition {
    /// Pure schema validation for the declaration-only RC contract.
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.contract_version != CONTRACT_VERSION {
            return Err(ContractError::SchemaInvalid {
                path: "contract_version".to_owned(),
                message: format!("expected contract_version=\"{CONTRACT_VERSION}\""),
            });
        }
        if self.id.is_empty() {
            return Err(ContractError::SchemaInvalid {
                path: "id".to_owned(),
                message: "id MUST be non-empty".to_owned(),
            });
        }

        let mut node_ids: BTreeSet<&str> = BTreeSet::new();
        for (idx, node) in self.nodes.iter().enumerate() {
            let path = format!("nodes[{idx}]");
            if node.id.is_empty() {
                return Err(ContractError::SchemaInvalid {
                    path: format!("{path}.id"),
                    message: "node id MUST be non-empty".to_owned(),
                });
            }
            if !node_ids.insert(node.id.as_str()) {
                return Err(ContractError::SchemaInvalid {
                    path: format!("{path}.id"),
                    message: format!("duplicate node id {:?}", node.id),
                });
            }
            match node.kind {
                OrchestrationNodeKind::Provider => {
                    if node.provider_ref.as_deref().unwrap_or("").is_empty() {
                        return Err(ContractError::SchemaInvalid {
                            path: format!("{path}.provider_ref"),
                            message: "provider nodes MUST set provider_ref".to_owned(),
                        });
                    }
                }
                OrchestrationNodeKind::ToolCapability => {
                    if node.capability_ref.as_deref().unwrap_or("").is_empty() {
                        return Err(ContractError::SchemaInvalid {
                            path: format!("{path}.capability_ref"),
                            message: "tool_capability nodes MUST set capability_ref".to_owned(),
                        });
                    }
                }
                OrchestrationNodeKind::RulesProvider => {
                    if node.posture != OrchestrationPosture::DeclarationOnly {
                        return Err(ContractError::SchemaInvalid {
                            path: format!("{path}.posture"),
                            message: "rules_provider nodes MUST be declaration_only in v1.1.0 RC"
                                .to_owned(),
                        });
                    }
                }
                OrchestrationNodeKind::ExternalAction
                | OrchestrationNodeKind::Branch
                | OrchestrationNodeKind::Convergence
                | OrchestrationNodeKind::ConditionalRoute => {}
            }
        }

        for (idx, edge) in self.edges.iter().enumerate() {
            let path = format!("edges[{idx}]");
            if !node_ids.contains(edge.from.as_str()) {
                return Err(ContractError::SchemaInvalid {
                    path: format!("{path}.from"),
                    message: format!("edge.from references unknown node {:?}", edge.from),
                });
            }
            if !node_ids.contains(edge.to.as_str()) {
                return Err(ContractError::SchemaInvalid {
                    path: format!("{path}.to"),
                    message: format!("edge.to references unknown node {:?}", edge.to),
                });
            }
        }

        Ok(())
    }

    /// Serialize as canonical JSON.
    pub fn export_canonical_json(&self) -> Result<String, ContractError> {
        self.validate()?;
        let value = serde_json::to_value(self)
            .map_err(|e| ContractError::Malformed(format!("serialize: {e}")))?;
        let bytes = super::hash::canonical_json_bytes(&value)
            .map_err(|e| ContractError::Malformed(e.to_string()))?;
        String::from_utf8(bytes).map_err(|e| ContractError::Malformed(e.to_string()))
    }

    /// Deserialize from any well-formed JSON, then validate.
    pub fn import_canonical_json(s: &str) -> Result<Self, ContractError> {
        let parsed: Self = serde_json::from_str(s)
            .map_err(|e| ContractError::Malformed(format!("deserialize: {e}")))?;
        parsed.validate()?;
        Ok(parsed)
    }

    /// Canonical hash of the orchestration definition.
    pub fn canonical_hash(&self) -> Result<ArtifactCanonicalHash, ContractError> {
        self.validate()?;
        let value = serde_json::to_value(self)
            .map_err(|e| ContractError::Malformed(format!("serialize: {e}")))?;
        canonical_hash(&value).map_err(|e| ContractError::Malformed(e.to_string()))
    }
}

/// One logical orchestration node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrchestrationNode {
    pub id: String,
    pub kind: OrchestrationNodeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_action_ref: Option<String>,
    pub posture: OrchestrationPosture,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_hint: Option<LayoutHint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrchestrationNodeKind {
    Provider,
    ToolCapability,
    RulesProvider,
    ExternalAction,
    Branch,
    Convergence,
    ConditionalRoute,
}

/// One directed orchestration edge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrchestrationEdge {
    pub from: String,
    pub to: String,
    pub kind: OrchestrationEdgeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrchestrationEdgeKind {
    Data,
    Control,
    Conditional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrchestrationPosture {
    DeclarationOnly,
    ExternalOnly,
    Deferred,
    MissingPlugin,
}

/// Optional layout hint for human graph renderers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutHint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rank: Option<u32>,
}
