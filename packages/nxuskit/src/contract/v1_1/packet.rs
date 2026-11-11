//! Generic regulated-workflow packet DTOs (US1 floor).
//!
//! US1 scope: enough shape for the Peeler-style direct-consumption smoke to
//! build a packet, attach redaction flags, export to canonical JSON, and
//! re-import with redaction flags byte-equal. Public CE language only — no
//! medical / legal / coding / billing / payer-policy / payer-contract /
//! persona-specific copy or field names.
//!
//! TODO(US3+): broaden review-status transitions, evidence-validation
//! outcomes, and per-stage workflow-pattern helpers as those features are
//! exercised by later user stories. This module pins the shape; broader
//! semantics are additive.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::validation::ContractError;
use super::{CONTRACT_VERSION, hash::canonical_json_bytes};

/// Top-level regulated-workflow packet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegulatedWorkflowPacket {
    pub contract_version: String,
    pub packet_id: String,
    pub source_packet: SourcePacket,
    pub evidence_map: EvidenceMap,
    pub draft_output: DraftOutput,
    pub review_status: ReviewStatus,
    #[serde(default)]
    pub redaction_flags: Vec<RedactionFlag>,
    pub human_review_disposition: HumanReviewDisposition,
    pub non_automated_decision_marker: bool,
    #[serde(default)]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

impl RegulatedWorkflowPacket {
    pub fn builder(packet_id: impl Into<String>) -> RegulatedWorkflowPacketBuilder {
        RegulatedWorkflowPacketBuilder::new(packet_id.into())
    }

    pub fn export_canonical_json(&self) -> Result<String, ContractError> {
        let value = serde_json::to_value(self)
            .map_err(|e| ContractError::Malformed(format!("serialize: {e}")))?;
        let bytes =
            canonical_json_bytes(&value).map_err(|e| ContractError::Malformed(e.to_string()))?;
        String::from_utf8(bytes).map_err(|e| ContractError::Malformed(e.to_string()))
    }

    pub fn import_canonical_json(s: &str) -> Result<Self, ContractError> {
        serde_json::from_str(s).map_err(|e| ContractError::Malformed(format!("deserialize: {e}")))
    }
}

/// Builder for the US1 packet floor.
#[derive(Debug, Clone)]
pub struct RegulatedWorkflowPacketBuilder {
    packet_id: String,
    source_packet: SourcePacket,
    evidence_map: EvidenceMap,
    draft_output: Option<DraftOutput>,
    review_status: ReviewStatus,
    redaction_flags: Vec<RedactionFlag>,
    disposition: HumanReviewDisposition,
    metadata: BTreeMap<String, serde_json::Value>,
}

impl RegulatedWorkflowPacketBuilder {
    fn new(packet_id: String) -> Self {
        Self {
            packet_id,
            source_packet: SourcePacket::default(),
            evidence_map: EvidenceMap::default(),
            draft_output: None,
            review_status: ReviewStatus::AwaitingReview,
            redaction_flags: Vec::new(),
            disposition: HumanReviewDisposition::pending(),
            metadata: BTreeMap::new(),
        }
    }

    pub fn source_document(mut self, source_id: impl Into<String>) -> Self {
        self.source_packet = SourcePacket {
            source_id: source_id.into(),
            kind: SourceKind::Document,
            source_references: Vec::new(),
            metadata: BTreeMap::new(),
        };
        self
    }

    pub fn add_source_reference(mut self, uri: impl Into<String>) -> Self {
        self.source_packet.source_references.push(SourceRef {
            uri: uri.into(),
            metadata: BTreeMap::new(),
        });
        self
    }

    pub fn push_evidence(
        mut self,
        candidate_id: impl Into<String>,
        candidate_text: impl Into<String>,
        source_uri: impl Into<String>,
    ) -> Self {
        self.evidence_map.entries.push(EvidenceEntry {
            candidate_id: candidate_id.into(),
            candidate_text: candidate_text.into(),
            source_ref: SourceRef {
                uri: source_uri.into(),
                metadata: BTreeMap::new(),
            },
            validation_outcomes: Vec::new(),
            missing_information_flags: Vec::new(),
        });
        self
    }

    pub fn draft(mut self, draft_id: impl Into<String>, body: impl Into<String>) -> Self {
        self.draft_output = Some(DraftOutput {
            draft_id: draft_id.into(),
            body: body.into(),
            attached_evidence: Vec::new(),
            attached_validations: Vec::new(),
            metadata: BTreeMap::new(),
        });
        self
    }

    pub fn review_status(mut self, s: ReviewStatus) -> Self {
        self.review_status = s;
        self
    }

    pub fn add_redaction(mut self, r: RedactionFlag) -> Self {
        self.redaction_flags.push(r);
        self
    }

    pub fn disposition(mut self, d: HumanReviewDisposition) -> Self {
        self.disposition = d;
        self
    }

    pub fn build(self) -> RegulatedWorkflowPacket {
        let draft = self.draft_output.unwrap_or_else(|| DraftOutput {
            draft_id: "draft_default".to_owned(),
            body: String::new(),
            attached_evidence: Vec::new(),
            attached_validations: Vec::new(),
            metadata: BTreeMap::new(),
        });
        RegulatedWorkflowPacket {
            contract_version: CONTRACT_VERSION.to_owned(),
            packet_id: self.packet_id,
            source_packet: self.source_packet,
            evidence_map: self.evidence_map,
            draft_output: draft,
            review_status: self.review_status,
            redaction_flags: self.redaction_flags,
            human_review_disposition: self.disposition,
            non_automated_decision_marker: true,
            metadata: self.metadata,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SourcePacket {
    pub source_id: String,
    pub kind: SourceKind,
    #[serde(default)]
    pub source_references: Vec<SourceRef>,
    #[serde(default)]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    #[default]
    Document,
    StructuredRecord,
    Composite,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRef {
    pub uri: String,
    #[serde(default)]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EvidenceMap {
    #[serde(default)]
    pub entries: Vec<EvidenceEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceEntry {
    pub candidate_id: String,
    pub candidate_text: String,
    pub source_ref: SourceRef,
    #[serde(default)]
    pub validation_outcomes: Vec<EvidenceValidationOutcome>,
    #[serde(default)]
    pub missing_information_flags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EvidenceValidationOutcome {
    RulePass,
    RuleFail,
    RuleUnknown { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftOutput {
    pub draft_id: String,
    pub body: String,
    #[serde(default)]
    pub attached_evidence: Vec<String>,
    #[serde(default)]
    pub attached_validations: Vec<EvidenceValidationOutcome>,
    #[serde(default)]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    AwaitingReview,
    InReview,
    AcceptedByReviewer,
    RejectedByReviewer,
    NeedsRevision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanReviewDisposition {
    pub disposition: DispositionKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewer_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewer_notes: Option<String>,
}

impl HumanReviewDisposition {
    pub fn pending() -> Self {
        Self {
            disposition: DispositionKind::Pending,
            reviewer_id: None,
            decided_at: None,
            reviewer_notes: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DispositionKind {
    Pending,
    Accept,
    Reject,
    NeedsRevision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactionFlag {
    pub field_path: String,
    pub redaction_kind: RedactionKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl RedactionFlag {
    pub fn new(field_path: impl Into<String>, kind: RedactionKind) -> Self {
        Self {
            field_path: field_path.into(),
            redaction_kind: kind,
            reason: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RedactionKind {
    Pii,
    Phi,
    Confidential,
    ClientPrivilege,
    Other,
}
