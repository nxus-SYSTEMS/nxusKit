use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionGateError {
    MissingField(&'static str),
}

impl Display for ActionGateError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingField(field) => write!(f, "missing action-gate field: {field}"),
        }
    }
}

impl Error for ActionGateError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionGateDecision {
    Allow,
    Deny,
    ReviewRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionGateStatus {
    DecisionDerived,
    MissingEvidence,
    ReviewHandoffMissing,
    ForbiddenAuthority,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionGateForbiddenAuthority {
    PublicProjection,
    RuntimeProviderClaim,
    SecretOrCustomerData,
    UnboundedFileOrNetworkAccess,
    AllHooksOpen,
    W561PlusContinuation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionGateActionFacts {
    kind: String,
    action_ref: String,
    public_projection_requested: bool,
    runtime_provider_claim_requested: bool,
    unbounded_file_or_network_access_requested: bool,
}

impl ActionGateActionFacts {
    pub fn new(kind: impl Into<String>, action_ref: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            action_ref: action_ref.into(),
            public_projection_requested: false,
            runtime_provider_claim_requested: false,
            unbounded_file_or_network_access_requested: false,
        }
    }

    pub fn with_public_projection_requested(mut self, requested: bool) -> Self {
        self.public_projection_requested = requested;
        self
    }

    pub fn with_runtime_provider_claim_requested(mut self, requested: bool) -> Self {
        self.runtime_provider_claim_requested = requested;
        self
    }

    pub fn with_unbounded_file_or_network_access_requested(mut self, requested: bool) -> Self {
        self.unbounded_file_or_network_access_requested = requested;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionGateActorContextFacts {
    actor_ref: String,
    context_ref: String,
    local_synthetic_context: bool,
    contains_secret_material: bool,
    contains_customer_or_production_data: bool,
}

impl ActionGateActorContextFacts {
    pub fn new(
        actor_ref: impl Into<String>,
        context_ref: impl Into<String>,
        local_synthetic_context: bool,
        contains_secret_material: bool,
        contains_customer_or_production_data: bool,
    ) -> Self {
        Self {
            actor_ref: actor_ref.into(),
            context_ref: context_ref.into(),
            local_synthetic_context,
            contains_secret_material,
            contains_customer_or_production_data,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionGateConsequenceFacts {
    kind: String,
    consequence_ref: String,
    exports_sensitive_data: bool,
    material_effect: bool,
    requires_review: bool,
}

impl ActionGateConsequenceFacts {
    pub fn new(
        kind: impl Into<String>,
        consequence_ref: impl Into<String>,
        exports_sensitive_data: bool,
        material_effect: bool,
        requires_review: bool,
    ) -> Self {
        Self {
            kind: kind.into(),
            consequence_ref: consequence_ref.into(),
            exports_sensitive_data,
            material_effect,
            requires_review,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionGatePolicyFacts {
    policy_ref: String,
    low_consequence_action_allowed: bool,
    sensitive_export_allowed: bool,
    material_effect_requires_review: bool,
    public_projection_allowed: bool,
}

impl ActionGatePolicyFacts {
    pub fn new(
        policy_ref: impl Into<String>,
        low_consequence_action_allowed: bool,
        sensitive_export_allowed: bool,
        material_effect_requires_review: bool,
        public_projection_allowed: bool,
    ) -> Self {
        Self {
            policy_ref: policy_ref.into(),
            low_consequence_action_allowed,
            sensitive_export_allowed,
            material_effect_requires_review,
            public_projection_allowed,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionGateEvidenceSet {
    evidence_refs: Vec<String>,
    required_evidence_refs: Vec<String>,
    receipt_ready_refs: Vec<String>,
    gate_ready_refs: Vec<String>,
}

impl ActionGateEvidenceSet {
    pub fn new(
        evidence_refs: impl IntoIterator<Item = impl Into<String>>,
        required_evidence_refs: impl IntoIterator<Item = impl Into<String>>,
        receipt_ready_refs: impl IntoIterator<Item = impl Into<String>>,
        gate_ready_refs: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            evidence_refs: evidence_refs.into_iter().map(Into::into).collect(),
            required_evidence_refs: required_evidence_refs.into_iter().map(Into::into).collect(),
            receipt_ready_refs: receipt_ready_refs.into_iter().map(Into::into).collect(),
            gate_ready_refs: gate_ready_refs.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionGateFixtureControls {
    bounded_fixture: bool,
    local_static_synthetic_only: bool,
    file_access_bounded: bool,
    network_access_bounded: bool,
    public_projection_denied: bool,
    runtime_provider_claims_denied: bool,
    secret_customer_data_denied: bool,
    review_handoff_ref: Option<String>,
}

impl ActionGateFixtureControls {
    pub fn bounded_private(review_handoff_ref: Option<&str>) -> Self {
        Self {
            bounded_fixture: true,
            local_static_synthetic_only: true,
            file_access_bounded: true,
            network_access_bounded: true,
            public_projection_denied: true,
            runtime_provider_claims_denied: true,
            secret_customer_data_denied: true,
            review_handoff_ref: review_handoff_ref.map(str::to_string),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionGateRequest {
    request_ref: String,
    case_ref: String,
    action: ActionGateActionFacts,
    actor_context: ActionGateActorContextFacts,
    consequence: ActionGateConsequenceFacts,
    policy: ActionGatePolicyFacts,
    evidence: ActionGateEvidenceSet,
    fixture_controls: ActionGateFixtureControls,
    forbidden_authority: Option<ActionGateForbiddenAuthority>,
}

impl ActionGateRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        request_ref: impl Into<String>,
        case_ref: impl Into<String>,
        action: ActionGateActionFacts,
        actor_context: ActionGateActorContextFacts,
        consequence: ActionGateConsequenceFacts,
        policy: ActionGatePolicyFacts,
        evidence: ActionGateEvidenceSet,
        fixture_controls: ActionGateFixtureControls,
    ) -> Self {
        Self {
            request_ref: request_ref.into(),
            case_ref: case_ref.into(),
            action,
            actor_context,
            consequence,
            policy,
            evidence,
            fixture_controls,
            forbidden_authority: None,
        }
    }

    pub fn with_evidence(mut self, evidence: ActionGateEvidenceSet) -> Self {
        self.evidence = evidence;
        self
    }

    pub fn with_fixture_controls(mut self, fixture_controls: ActionGateFixtureControls) -> Self {
        self.fixture_controls = fixture_controls;
        self
    }

    pub fn with_forbidden_authority(mut self, authority: ActionGateForbiddenAuthority) -> Self {
        self.forbidden_authority = Some(authority);
        self
    }
}

pub trait ActionGatePolicy {
    fn family(&self) -> &'static str;
    fn evaluate(&self, request: &ActionGateRequest) -> ActionGatePolicyOutcome;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionGatePolicyOutcome {
    decision: ActionGateDecision,
    decision_labels: Vec<String>,
    denial_labels: Vec<String>,
    requires_review_handoff: bool,
}

impl ActionGatePolicyOutcome {
    pub fn derived(
        decision: ActionGateDecision,
        decision_labels: impl IntoIterator<Item = impl Into<String>>,
        denial_labels: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            decision,
            decision_labels: decision_labels.into_iter().map(Into::into).collect(),
            denial_labels: denial_labels.into_iter().map(Into::into).collect(),
            requires_review_handoff: decision == ActionGateDecision::ReviewRequired,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct D1SupportPlaybookPolicy;

impl D1SupportPlaybookPolicy {
    pub fn new() -> Self {
        Self
    }
}

impl Default for D1SupportPlaybookPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl ActionGatePolicy for D1SupportPlaybookPolicy {
    fn family(&self) -> &'static str {
        "d1_support_playbook"
    }

    fn evaluate(&self, request: &ActionGateRequest) -> ActionGatePolicyOutcome {
        let account_export_shape = request.action.kind == "account_export"
            || request.consequence.kind == "account_export"
            || request.consequence.exports_sensitive_data;
        if request.action.public_projection_requested
            || (account_export_shape
                && (!request.policy.sensitive_export_allowed
                    || !request.policy.public_projection_allowed))
        {
            return ActionGatePolicyOutcome::derived(
                ActionGateDecision::Deny,
                ["d1_account_export_requires_separate_authority"],
                ["account_export_public_or_private_provider_authority_missing"],
            );
        }

        let service_credit_shape = request.action.kind == "service_credit"
            || request.consequence.kind == "service_credit"
            || request.consequence.material_effect
            || request.consequence.requires_review
            || request.policy.material_effect_requires_review;
        if service_credit_shape {
            return ActionGatePolicyOutcome::derived(
                ActionGateDecision::ReviewRequired,
                ["d1_service_credit_requires_human_review"],
                Vec::<String>::new(),
            );
        }

        let support_reply_shape = request.action.kind == "support_reply"
            && request.consequence.kind == "low_support_reply"
            && request.policy.low_consequence_action_allowed
            && !request.consequence.exports_sensitive_data
            && !request.consequence.material_effect;
        if support_reply_shape {
            return ActionGatePolicyOutcome::derived(
                ActionGateDecision::Allow,
                ["d1_support_reply_low_consequence_allowed"],
                Vec::<String>::new(),
            );
        }

        ActionGatePolicyOutcome::derived(
            ActionGateDecision::Deny,
            ["action_gate_default_deny"],
            ["action_gate_policy_did_not_authorize_action"],
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionGateEvaluator<P> {
    policy: P,
}

impl<P: ActionGatePolicy> ActionGateEvaluator<P> {
    pub fn new(policy: P) -> Self {
        Self { policy }
    }

    pub fn evaluate(
        &self,
        request: &ActionGateRequest,
    ) -> Result<ActionGateResponse, ActionGateError> {
        validate_request(request)?;

        if let Some(authority) = request.forbidden_authority {
            return Ok(ActionGateResponse::blocked(
                self.policy.family(),
                request,
                ActionGateStatus::ForbiddenAuthority,
                forbidden_authority_label(authority),
                forbidden_authority_denial(authority),
            ));
        }

        if !evidence_complete(&request.evidence) {
            return Ok(ActionGateResponse::blocked(
                self.policy.family(),
                request,
                ActionGateStatus::MissingEvidence,
                "missing_required_evidence_refs",
                "required evidence, receipt-ready, and gate-ready refs must be present",
            ));
        }

        if !bounded_private_fixture(request)
            || request.action.runtime_provider_claim_requested
            || request.action.unbounded_file_or_network_access_requested
            || request.actor_context.contains_secret_material
            || request.actor_context.contains_customer_or_production_data
            || !request.actor_context.local_synthetic_context
        {
            return Ok(ActionGateResponse::blocked(
                self.policy.family(),
                request,
                ActionGateStatus::ForbiddenAuthority,
                "private_fixture_boundary_forbidden",
                "action-gate evaluation accepts only bounded local synthetic private facts",
            ));
        }

        let outcome = self.policy.evaluate(request);
        if outcome.requires_review_handoff && request.fixture_controls.review_handoff_ref.is_none()
        {
            return Ok(ActionGateResponse::blocked(
                self.policy.family(),
                request,
                ActionGateStatus::ReviewHandoffMissing,
                "review_required_without_handoff",
                "review_required decision requires a private review handoff ref",
            ));
        }

        Ok(ActionGateResponse::derived(
            self.policy.family(),
            request,
            outcome,
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionGateResponse {
    policy_family: String,
    request_ref: String,
    case_ref: String,
    action_ref: String,
    actor_ref: String,
    context_ref: String,
    consequence_ref: String,
    policy_ref: String,
    status: ActionGateStatus,
    decision: ActionGateDecision,
    blocker_labels: Vec<String>,
    decision_labels: Vec<String>,
    evidence_ready_refs: Vec<String>,
    receipt_ready_refs: Vec<String>,
    gate_ready_refs: Vec<String>,
    handoff_labels: Vec<String>,
    review_handoff_refs: Vec<String>,
    denial_labels: Vec<String>,
    closed_gate_labels: Vec<String>,
    derived_from_typed_facts: bool,
}

impl ActionGateResponse {
    fn derived(
        policy_family: &str,
        request: &ActionGateRequest,
        outcome: ActionGatePolicyOutcome,
    ) -> Self {
        let review_handoff_refs = match (
            outcome.decision,
            request.fixture_controls.review_handoff_ref.clone(),
        ) {
            (ActionGateDecision::ReviewRequired, Some(handoff_ref)) => vec![handoff_ref],
            _ => Vec::new(),
        };
        let handoff_labels = if review_handoff_refs.is_empty() {
            Vec::new()
        } else {
            vec!["private_human_review_handoff_recorded".to_string()]
        };

        Self::new(
            policy_family,
            request,
            ActionGateStatus::DecisionDerived,
            outcome.decision,
            Vec::new(),
            outcome.decision_labels,
            handoff_labels,
            review_handoff_refs,
            outcome.denial_labels,
            true,
        )
    }

    fn blocked(
        policy_family: &str,
        request: &ActionGateRequest,
        status: ActionGateStatus,
        blocker_label: impl Into<String>,
        denial_label: impl Into<String>,
    ) -> Self {
        Self::new(
            policy_family,
            request,
            status,
            ActionGateDecision::Deny,
            vec![blocker_label.into()],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![denial_label.into()],
            false,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn new(
        policy_family: &str,
        request: &ActionGateRequest,
        status: ActionGateStatus,
        decision: ActionGateDecision,
        blocker_labels: Vec<String>,
        decision_labels: Vec<String>,
        handoff_labels: Vec<String>,
        review_handoff_refs: Vec<String>,
        denial_labels: Vec<String>,
        derived_from_typed_facts: bool,
    ) -> Self {
        Self {
            policy_family: policy_family.to_string(),
            request_ref: request.request_ref.clone(),
            case_ref: request.case_ref.clone(),
            action_ref: request.action.action_ref.clone(),
            actor_ref: request.actor_context.actor_ref.clone(),
            context_ref: request.actor_context.context_ref.clone(),
            consequence_ref: request.consequence.consequence_ref.clone(),
            policy_ref: request.policy.policy_ref.clone(),
            status,
            decision,
            blocker_labels,
            decision_labels,
            evidence_ready_refs: evidence_ready_refs(&request.evidence),
            receipt_ready_refs: request.evidence.receipt_ready_refs.clone(),
            gate_ready_refs: request.evidence.gate_ready_refs.clone(),
            handoff_labels,
            review_handoff_refs,
            denial_labels,
            closed_gate_labels: closed_gate_labels(),
            derived_from_typed_facts,
        }
    }

    pub fn policy_family(&self) -> &str {
        &self.policy_family
    }

    pub fn request_ref(&self) -> &str {
        &self.request_ref
    }

    pub fn case_ref(&self) -> &str {
        &self.case_ref
    }

    pub fn action_ref(&self) -> &str {
        &self.action_ref
    }

    pub fn actor_ref(&self) -> &str {
        &self.actor_ref
    }

    pub fn context_ref(&self) -> &str {
        &self.context_ref
    }

    pub fn consequence_ref(&self) -> &str {
        &self.consequence_ref
    }

    pub fn policy_ref(&self) -> &str {
        &self.policy_ref
    }

    pub fn status(&self) -> ActionGateStatus {
        self.status
    }

    pub fn decision(&self) -> ActionGateDecision {
        self.decision
    }

    pub fn blocker_labels(&self) -> Vec<&str> {
        self.blocker_labels.iter().map(String::as_str).collect()
    }

    pub fn decision_labels(&self) -> Vec<&str> {
        self.decision_labels.iter().map(String::as_str).collect()
    }

    pub fn evidence_ready_refs(&self) -> Vec<&str> {
        self.evidence_ready_refs
            .iter()
            .map(String::as_str)
            .collect()
    }

    pub fn receipt_ready_refs(&self) -> Vec<&str> {
        self.receipt_ready_refs.iter().map(String::as_str).collect()
    }

    pub fn gate_ready_refs(&self) -> Vec<&str> {
        self.gate_ready_refs.iter().map(String::as_str).collect()
    }

    pub fn handoff_labels(&self) -> Vec<&str> {
        self.handoff_labels.iter().map(String::as_str).collect()
    }

    pub fn review_handoff_ref(&self) -> Option<&str> {
        self.review_handoff_refs.first().map(String::as_str)
    }

    pub fn denial_labels(&self) -> Vec<&str> {
        self.denial_labels.iter().map(String::as_str).collect()
    }

    pub fn closed_gate_labels(&self) -> Vec<&str> {
        self.closed_gate_labels.iter().map(String::as_str).collect()
    }

    pub fn derived_from_typed_facts(&self) -> bool {
        self.derived_from_typed_facts
    }

    pub fn public_surface_authorized(&self) -> bool {
        false
    }

    pub fn public_projection_authorized(&self) -> bool {
        false
    }

    pub fn durable_write_authorized(&self) -> bool {
        false
    }

    pub fn callback_authorized(&self) -> bool {
        false
    }

    pub fn provider_runtime_authorized(&self) -> bool {
        false
    }

    pub fn customer_or_production_data_authorized(&self) -> bool {
        false
    }

    pub fn live_licensing_authorized(&self) -> bool {
        false
    }

    pub fn release_or_package_authorized(&self) -> bool {
        false
    }

    pub fn material_claim_authorized(&self) -> bool {
        false
    }

    pub fn downstream_publication_authorized(&self) -> bool {
        false
    }

    pub fn w561_plus_authorized(&self) -> bool {
        false
    }
}

fn validate_request(request: &ActionGateRequest) -> Result<(), ActionGateError> {
    if request.request_ref.trim().is_empty()
        || request.case_ref.trim().is_empty()
        || request.action.action_ref.trim().is_empty()
        || request.actor_context.actor_ref.trim().is_empty()
        || request.actor_context.context_ref.trim().is_empty()
        || request.consequence.consequence_ref.trim().is_empty()
        || request.policy.policy_ref.trim().is_empty()
    {
        return Err(ActionGateError::MissingField(
            "request/case/action/actor/context/consequence/policy refs",
        ));
    }
    Ok(())
}

fn evidence_complete(evidence: &ActionGateEvidenceSet) -> bool {
    if evidence.required_evidence_refs.is_empty()
        || evidence.evidence_refs.is_empty()
        || evidence.receipt_ready_refs.is_empty()
        || evidence.gate_ready_refs.is_empty()
    {
        return false;
    }

    let present = evidence
        .evidence_refs
        .iter()
        .filter(|value| !value.trim().is_empty())
        .collect::<BTreeSet<_>>();
    evidence
        .required_evidence_refs
        .iter()
        .all(|required| !required.trim().is_empty() && present.contains(required))
}

fn bounded_private_fixture(request: &ActionGateRequest) -> bool {
    let controls = &request.fixture_controls;
    controls.bounded_fixture
        && controls.local_static_synthetic_only
        && controls.file_access_bounded
        && controls.network_access_bounded
        && controls.public_projection_denied
        && controls.runtime_provider_claims_denied
        && controls.secret_customer_data_denied
}

fn evidence_ready_refs(evidence: &ActionGateEvidenceSet) -> Vec<String> {
    evidence
        .evidence_refs
        .iter()
        .chain(evidence.required_evidence_refs.iter())
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn forbidden_authority_label(authority: ActionGateForbiddenAuthority) -> &'static str {
    match authority {
        ActionGateForbiddenAuthority::PublicProjection => "public_projection_forbidden",
        ActionGateForbiddenAuthority::RuntimeProviderClaim => "runtime_provider_claim_forbidden",
        ActionGateForbiddenAuthority::SecretOrCustomerData => "secret_or_customer_data_forbidden",
        ActionGateForbiddenAuthority::UnboundedFileOrNetworkAccess => {
            "unbounded_file_or_network_access_forbidden"
        }
        ActionGateForbiddenAuthority::AllHooksOpen => "all_hooks_open_forbidden",
        ActionGateForbiddenAuthority::W561PlusContinuation => "w561_plus_forbidden",
    }
}

fn forbidden_authority_denial(authority: ActionGateForbiddenAuthority) -> &'static str {
    match authority {
        ActionGateForbiddenAuthority::PublicProjection => {
            "public projection requires separate authority"
        }
        ActionGateForbiddenAuthority::RuntimeProviderClaim => {
            "runtime provider claims require separate authority"
        }
        ActionGateForbiddenAuthority::SecretOrCustomerData => {
            "secret/customer data requires separate authority"
        }
        ActionGateForbiddenAuthority::UnboundedFileOrNetworkAccess => {
            "unbounded file/network access requires separate authority"
        }
        ActionGateForbiddenAuthority::AllHooksOpen => "all-hooks-open authority is invalid",
        ActionGateForbiddenAuthority::W561PlusContinuation => {
            "W561+ continuation requires separate authority"
        }
    }
}

fn closed_gate_labels() -> Vec<String> {
    vec![
        "public_sdk_api_schema_cli_wrapper_prelude_closed".to_string(),
        "public_projection_closed".to_string(),
        "durable_persistence_and_store_write_closed".to_string(),
        "callbacks_webhooks_managed_queues_closed".to_string(),
        "provider_runtime_and_external_io_closed".to_string(),
        "customer_or_production_data_closed".to_string(),
        "live_licensing_and_product_catalog_closed".to_string(),
        "release_package_and_public_artifact_closed".to_string(),
        "material_support_compliance_security_claims_closed".to_string(),
        "downstream_publication_closed".to_string(),
        "w561_plus_continuation_closed".to_string(),
    ]
}
