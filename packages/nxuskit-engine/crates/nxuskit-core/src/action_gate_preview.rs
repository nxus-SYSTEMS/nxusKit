use std::collections::BTreeSet;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::panic::{AssertUnwindSafe, catch_unwind};

use nxuskit_engine::action_gate::{
    ActionGateActionFacts, ActionGateActorContextFacts, ActionGateConsequenceFacts,
    ActionGateDecision, ActionGateEvaluator, ActionGateEvidenceSet, ActionGateFixtureControls,
    ActionGateForbiddenAuthority, ActionGatePolicyFacts, ActionGateRequest, ActionGateStatus,
    D1SupportPlaybookPolicy,
};
use serde::{Deserialize, Serialize};

const REQUEST_SCHEMA_VERSION: &str = "nxuskit-action-gate-preview/request/v1";
const RESULT_SCHEMA_VERSION: &str = "nxuskit-action-gate-preview/result/v1";
const POLICY_FAMILY: &str = "d1_support_playbook";
const MAX_REQUEST_BYTES: usize = 65_536;
const MAX_REF_LEN: usize = 128;
const MAX_REFS: usize = 16;
const INVALID_CASE_REF: &str = "synthetic.case.invalid-request";

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PreviewActionKind {
    SupportReply,
    AccountExport,
    ServiceCredit,
}

impl PreviewActionKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::SupportReply => "support_reply",
            Self::AccountExport => "account_export",
            Self::ServiceCredit => "service_credit",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PreviewConsequenceKind {
    LowSupportReply,
    AccountExport,
    ServiceCredit,
}

impl PreviewConsequenceKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::LowSupportReply => "low_support_reply",
            Self::AccountExport => "account_export",
            Self::ServiceCredit => "service_credit",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviewRequestV1 {
    schema_version: String,
    request_ref: String,
    case_ref: String,
    action: PreviewActionFactsV1,
    actor_context: PreviewActorContextV1,
    consequence: PreviewConsequenceFactsV1,
    policy: PreviewPolicyFactsV1,
    evidence: PreviewEvidenceV1,
    review_handoff_ref: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviewActionFactsV1 {
    kind: PreviewActionKind,
    action_ref: String,
    public_projection_requested: bool,
    runtime_provider_claim_requested: bool,
    unbounded_file_or_network_access_requested: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviewActorContextV1 {
    actor_ref: String,
    context_ref: String,
    local_synthetic_context: bool,
    contains_secret_material: bool,
    contains_customer_or_production_data: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviewConsequenceFactsV1 {
    kind: PreviewConsequenceKind,
    consequence_ref: String,
    exports_sensitive_data: bool,
    material_effect: bool,
    requires_review: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviewPolicyFactsV1 {
    policy_ref: String,
    low_consequence_action_allowed: bool,
    sensitive_export_allowed: bool,
    material_effect_requires_review: bool,
    public_projection_allowed: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviewEvidenceV1 {
    evidence_refs: Vec<String>,
    required_evidence_refs: Vec<String>,
    receipt_ready_refs: Vec<String>,
    gate_ready_refs: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewDecision {
    Allow,
    Deny,
    ReviewRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewStatus {
    DecisionDerived,
    MissingEvidence,
    ReviewHandoffMissing,
    ForbiddenAuthority,
    InvalidRequest,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct PreviewResultV1 {
    pub schema_version: &'static str,
    pub preview: bool,
    pub case_ref: String,
    pub policy_family: &'static str,
    pub decision: PreviewDecision,
    pub status: PreviewStatus,
    pub derived_from_typed_facts: bool,
    pub decision_labels: Vec<String>,
    pub denial_labels: Vec<String>,
    pub blocker_labels: Vec<String>,
    pub handoff_labels: Vec<String>,
    pub evidence_ready_refs: Vec<String>,
    pub receipt_ready_refs: Vec<String>,
    pub gate_ready_refs: Vec<String>,
    pub review_handoff_ref: Option<String>,
    pub closed_gate_labels: Vec<String>,
    pub action_execution_authorized: bool,
    pub public_projection_authorized: bool,
    pub provider_runtime_authorized: bool,
    pub customer_or_production_data_authorized: bool,
    pub release_or_package_authorized: bool,
    pub downstream_publication_authorized: bool,
}

/// Evaluate a strict, local-only Action Gate preview request.
///
/// The returned projection contains only safe decision and evidence metadata.
/// It does not authorize or execute an action.
pub fn evaluate_preview_json(request_json: &str) -> PreviewResultV1 {
    if request_json.len() > MAX_REQUEST_BYTES {
        return invalid_request_result();
    }

    let request = match serde_json::from_str::<PreviewRequestV1>(request_json) {
        Ok(request) if valid_request(&request) => request,
        _ => return invalid_request_result(),
    };

    let forbidden_authority = requested_forbidden_authority(&request);
    let action =
        ActionGateActionFacts::new(request.action.kind.as_str(), &request.action.action_ref)
            .with_public_projection_requested(request.action.public_projection_requested)
            .with_runtime_provider_claim_requested(request.action.runtime_provider_claim_requested)
            .with_unbounded_file_or_network_access_requested(
                request.action.unbounded_file_or_network_access_requested,
            );
    let actor_context = ActionGateActorContextFacts::new(
        &request.actor_context.actor_ref,
        &request.actor_context.context_ref,
        request.actor_context.local_synthetic_context,
        request.actor_context.contains_secret_material,
        request.actor_context.contains_customer_or_production_data,
    );
    let consequence = ActionGateConsequenceFacts::new(
        request.consequence.kind.as_str(),
        &request.consequence.consequence_ref,
        request.consequence.exports_sensitive_data,
        request.consequence.material_effect,
        request.consequence.requires_review,
    );
    let policy = ActionGatePolicyFacts::new(
        &request.policy.policy_ref,
        request.policy.low_consequence_action_allowed,
        request.policy.sensitive_export_allowed,
        request.policy.material_effect_requires_review,
        request.policy.public_projection_allowed,
    );
    let evidence = ActionGateEvidenceSet::new(
        request.evidence.evidence_refs.iter().cloned(),
        request.evidence.required_evidence_refs.iter().cloned(),
        request.evidence.receipt_ready_refs.iter().cloned(),
        request.evidence.gate_ready_refs.iter().cloned(),
    );
    let action_request = ActionGateRequest::new(
        &request.request_ref,
        &request.case_ref,
        action,
        actor_context,
        consequence,
        policy,
        evidence,
        ActionGateFixtureControls::bounded_private(request.review_handoff_ref.as_deref()),
    );
    let action_request = match forbidden_authority {
        Some(authority) => action_request.with_forbidden_authority(authority),
        None => action_request,
    };

    let evaluator = ActionGateEvaluator::new(D1SupportPlaybookPolicy::new());
    match evaluator.evaluate(&action_request) {
        Ok(response) => PreviewResultV1 {
            schema_version: RESULT_SCHEMA_VERSION,
            preview: true,
            case_ref: response.case_ref().to_string(),
            policy_family: POLICY_FAMILY,
            decision: preview_decision(response.decision()),
            status: preview_status(response.status()),
            derived_from_typed_facts: response.derived_from_typed_facts(),
            decision_labels: sorted_strings(response.decision_labels()),
            denial_labels: sorted_strings(response.denial_labels()),
            blocker_labels: sorted_strings(response.blocker_labels()),
            handoff_labels: sorted_strings(response.handoff_labels()),
            evidence_ready_refs: safe_evidence_ready_refs(&request.evidence),
            receipt_ready_refs: sorted_strings(response.receipt_ready_refs()),
            gate_ready_refs: sorted_strings(response.gate_ready_refs()),
            review_handoff_ref: response.review_handoff_ref().map(str::to_string),
            closed_gate_labels: sorted_strings(response.closed_gate_labels()),
            action_execution_authorized: false,
            public_projection_authorized: false,
            provider_runtime_authorized: false,
            customer_or_production_data_authorized: false,
            release_or_package_authorized: false,
            downstream_publication_authorized: false,
        },
        Err(_) => invalid_request_result(),
    }
}

/// Evaluate one strict Action Gate preview request through the additive JSON C
/// ABI. The caller owns a non-null returned string and frees it with
/// `nxuskit_free_string`.
///
/// # Safety
///
/// `request_json` must be null or point to a NUL-terminated byte string valid
/// for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_action_gate_preview_evaluate_json(
    request_json: *const c_char,
) -> *mut c_char {
    super::error::clear_last_error();

    match catch_unwind(AssertUnwindSafe(|| {
        if request_json.is_null() {
            super::error::set_last_error(
                "invalid_argument",
                "action gate preview request must not be null",
                None,
            );
            return std::ptr::null_mut();
        }
        let request_json = match unsafe { CStr::from_ptr(request_json) }.to_str() {
            Ok(request_json) => request_json,
            Err(_) => {
                super::error::set_last_error(
                    "invalid_argument",
                    "action gate preview request must be valid UTF-8",
                    None,
                );
                return std::ptr::null_mut();
            }
        };
        let result = evaluate_preview_json(request_json);
        let serialized = match serde_json::to_string(&result) {
            Ok(serialized) => serialized,
            Err(_) => {
                super::error::set_last_error(
                    "internal_error",
                    "action gate preview result serialization failed",
                    None,
                );
                return std::ptr::null_mut();
            }
        };
        match CString::new(serialized) {
            Ok(result) => result.into_raw(),
            Err(_) => {
                super::error::set_last_error(
                    "internal_error",
                    "action gate preview result contains an interior NUL",
                    None,
                );
                std::ptr::null_mut()
            }
        }
    })) {
        Ok(result) => result,
        Err(_) => {
            super::error::set_last_error(
                "internal_error",
                "action gate preview evaluation failed",
                None,
            );
            std::ptr::null_mut()
        }
    }
}

fn valid_request(request: &PreviewRequestV1) -> bool {
    request.schema_version == REQUEST_SCHEMA_VERSION
        && valid_ref(&request.request_ref)
        && valid_ref(&request.case_ref)
        && valid_ref(&request.action.action_ref)
        && valid_ref(&request.consequence.consequence_ref)
        && valid_ref(&request.policy.policy_ref)
        && valid_ref(&request.actor_context.actor_ref)
        && valid_ref(&request.actor_context.context_ref)
        && request
            .actor_context
            .actor_ref
            .starts_with("synthetic.actor.")
        && request
            .actor_context
            .context_ref
            .starts_with("synthetic.context.")
        && request.actor_context.local_synthetic_context
        && !request.actor_context.contains_secret_material
        && !request.actor_context.contains_customer_or_production_data
        && valid_ref_list(&request.evidence.evidence_refs)
        && valid_ref_list(&request.evidence.required_evidence_refs)
        && valid_ref_list(&request.evidence.receipt_ready_refs)
        && valid_ref_list(&request.evidence.gate_ready_refs)
        && request.review_handoff_ref.as_deref().is_none_or(valid_ref)
}

fn valid_ref(reference: &str) -> bool {
    let bytes = reference.as_bytes();
    (1..=MAX_REF_LEN).contains(&bytes.len())
        && reference.is_ascii()
        && matches!(bytes.first(), Some(b'a'..=b'z' | b'0'..=b'9'))
        && bytes
            .iter()
            .all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b':' | b'-'))
}

fn valid_ref_list(references: &[String]) -> bool {
    references.len() <= MAX_REFS
        && references.iter().all(|reference| valid_ref(reference))
        && references.iter().collect::<BTreeSet<_>>().len() == references.len()
}

fn requested_forbidden_authority(
    request: &PreviewRequestV1,
) -> Option<ActionGateForbiddenAuthority> {
    if request.action.public_projection_requested {
        Some(ActionGateForbiddenAuthority::PublicProjection)
    } else if request.action.runtime_provider_claim_requested {
        Some(ActionGateForbiddenAuthority::RuntimeProviderClaim)
    } else if request.action.unbounded_file_or_network_access_requested {
        Some(ActionGateForbiddenAuthority::UnboundedFileOrNetworkAccess)
    } else {
        None
    }
}

fn preview_decision(decision: ActionGateDecision) -> PreviewDecision {
    match decision {
        ActionGateDecision::Allow => PreviewDecision::Allow,
        ActionGateDecision::Deny => PreviewDecision::Deny,
        ActionGateDecision::ReviewRequired => PreviewDecision::ReviewRequired,
    }
}

fn preview_status(status: ActionGateStatus) -> PreviewStatus {
    match status {
        ActionGateStatus::DecisionDerived => PreviewStatus::DecisionDerived,
        ActionGateStatus::MissingEvidence => PreviewStatus::MissingEvidence,
        ActionGateStatus::ReviewHandoffMissing => PreviewStatus::ReviewHandoffMissing,
        ActionGateStatus::ForbiddenAuthority => PreviewStatus::ForbiddenAuthority,
    }
}

fn safe_evidence_ready_refs(evidence: &PreviewEvidenceV1) -> Vec<String> {
    // Required-only references are not ready evidence. This keeps an
    // incomplete request from projecting a reference it never supplied.
    sorted_strings(evidence.evidence_refs.iter().map(String::as_str))
}

fn sorted_strings<'a>(values: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    values
        .into_iter()
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn invalid_request_result() -> PreviewResultV1 {
    PreviewResultV1 {
        schema_version: RESULT_SCHEMA_VERSION,
        preview: true,
        case_ref: INVALID_CASE_REF.to_string(),
        policy_family: POLICY_FAMILY,
        decision: PreviewDecision::Deny,
        status: PreviewStatus::InvalidRequest,
        derived_from_typed_facts: false,
        decision_labels: Vec::new(),
        denial_labels: vec!["preview request is invalid".to_string()],
        blocker_labels: vec!["invalid_preview_request".to_string()],
        handoff_labels: Vec::new(),
        evidence_ready_refs: Vec::new(),
        receipt_ready_refs: Vec::new(),
        gate_ready_refs: Vec::new(),
        review_handoff_ref: None,
        closed_gate_labels: vec![
            "callbacks_webhooks_managed_queues_closed".to_string(),
            "customer_or_production_data_closed".to_string(),
            "downstream_publication_closed".to_string(),
            "durable_persistence_and_store_write_closed".to_string(),
            "live_licensing_and_product_catalog_closed".to_string(),
            "material_support_compliance_security_claims_closed".to_string(),
            "provider_runtime_and_external_io_closed".to_string(),
            "public_projection_closed".to_string(),
            "public_sdk_api_schema_cli_wrapper_prelude_closed".to_string(),
            "release_package_and_public_artifact_closed".to_string(),
            "w561_plus_continuation_closed".to_string(),
        ],
        action_execution_authorized: false,
        public_projection_authorized: false,
        provider_runtime_authorized: false,
        customer_or_production_data_authorized: false,
        release_or_package_authorized: false,
        downstream_publication_authorized: false,
    }
}
