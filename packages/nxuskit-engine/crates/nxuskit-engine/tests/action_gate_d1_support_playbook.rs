use nxuskit_engine::action_gate::{
    ActionGateActionFacts, ActionGateActorContextFacts, ActionGateConsequenceFacts,
    ActionGateDecision, ActionGateEvaluator, ActionGateEvidenceSet, ActionGateFixtureControls,
    ActionGateForbiddenAuthority, ActionGatePolicyFacts, ActionGateRequest, ActionGateStatus,
    D1SupportPlaybookPolicy,
};

#[test]
fn d1_support_playbook_derives_allow_deny_and_review_required_from_typed_facts() {
    let evaluator = ActionGateEvaluator::new(D1SupportPlaybookPolicy::new());

    let allow = evaluator
        .evaluate(&safe_support_reply_request())
        .expect("safe support reply evaluates");
    assert_eq!(allow.status(), ActionGateStatus::DecisionDerived);
    assert_eq!(allow.decision(), ActionGateDecision::Allow);
    assert_eq!(allow.case_ref(), "d1-support-reply-safe");
    assert_eq!(allow.policy_family(), "d1_support_playbook");
    assert!(allow.derived_from_typed_facts());
    assert!(
        allow
            .decision_labels()
            .contains(&"d1_support_reply_low_consequence_allowed")
    );
    assert!(
        allow
            .evidence_ready_refs()
            .contains(&"evidence.d1-support-reply-safe.policy")
    );
    assert!(
        allow
            .receipt_ready_refs()
            .contains(&"receipt.d1-support-reply-safe.agent_action_gate.ready")
    );
    assert!(
        allow
            .gate_ready_refs()
            .contains(&"gate.d1-support-reply-safe.agent_action_gate")
    );
    assert!(allow.review_handoff_ref().is_none());
    assert_private_gates_closed(&allow);

    let deny = evaluator
        .evaluate(&account_export_blocked_request())
        .expect("account export evaluates");
    assert_eq!(deny.status(), ActionGateStatus::DecisionDerived);
    assert_eq!(deny.decision(), ActionGateDecision::Deny);
    assert_eq!(deny.case_ref(), "d1-account-export-blocked");
    assert!(deny.derived_from_typed_facts());
    assert!(
        deny.decision_labels()
            .contains(&"d1_account_export_requires_separate_authority")
    );
    assert!(
        deny.denial_labels()
            .contains(&"account_export_public_or_private_provider_authority_missing")
    );
    assert!(deny.review_handoff_ref().is_none());
    assert_private_gates_closed(&deny);

    let review = evaluator
        .evaluate(&service_credit_review_request())
        .expect("service credit evaluates");
    assert_eq!(review.status(), ActionGateStatus::DecisionDerived);
    assert_eq!(review.decision(), ActionGateDecision::ReviewRequired);
    assert_eq!(review.case_ref(), "d1-service-credit-review");
    assert_eq!(
        review.review_handoff_ref(),
        Some("handoff.d1-service-credit-review.private_human_review")
    );
    assert!(
        review
            .decision_labels()
            .contains(&"d1_service_credit_requires_human_review")
    );
    assert!(
        review
            .handoff_labels()
            .contains(&"private_human_review_handoff_recorded")
    );
    assert_private_gates_closed(&review);
}

#[test]
fn action_gate_fails_closed_for_missing_refs_and_review_handoff() {
    let evaluator = ActionGateEvaluator::new(D1SupportPlaybookPolicy::new());

    let missing_evidence = evaluator
        .evaluate(
            &safe_support_reply_request().with_evidence(ActionGateEvidenceSet::new(
                Vec::<String>::new(),
                vec!["evidence.d1-support-reply-safe.policy"],
                vec!["receipt.d1-support-reply-safe.agent_action_gate.ready"],
                vec!["gate.d1-support-reply-safe.agent_action_gate"],
            )),
        )
        .expect("missing evidence refs fail closed");
    assert_eq!(missing_evidence.status(), ActionGateStatus::MissingEvidence);
    assert_eq!(missing_evidence.decision(), ActionGateDecision::Deny);
    assert!(!missing_evidence.derived_from_typed_facts());
    assert!(
        missing_evidence
            .blocker_labels()
            .contains(&"missing_required_evidence_refs")
    );
    assert_private_gates_closed(&missing_evidence);

    let missing_receipt = evaluator
        .evaluate(
            &safe_support_reply_request().with_evidence(ActionGateEvidenceSet::new(
                vec![
                    "evidence.d1-support-reply-safe.action",
                    "evidence.d1-support-reply-safe.actor_context",
                    "evidence.d1-support-reply-safe.policy",
                ],
                vec![
                    "evidence.d1-support-reply-safe.action",
                    "evidence.d1-support-reply-safe.actor_context",
                    "evidence.d1-support-reply-safe.policy",
                ],
                Vec::<String>::new(),
                vec!["gate.d1-support-reply-safe.agent_action_gate"],
            )),
        )
        .expect("missing receipt ref fails closed");
    assert_eq!(missing_receipt.status(), ActionGateStatus::MissingEvidence);
    assert_eq!(missing_receipt.decision(), ActionGateDecision::Deny);
    assert_private_gates_closed(&missing_receipt);

    let missing_gate = evaluator
        .evaluate(
            &safe_support_reply_request().with_evidence(ActionGateEvidenceSet::new(
                vec![
                    "evidence.d1-support-reply-safe.action",
                    "evidence.d1-support-reply-safe.actor_context",
                    "evidence.d1-support-reply-safe.policy",
                ],
                vec![
                    "evidence.d1-support-reply-safe.action",
                    "evidence.d1-support-reply-safe.actor_context",
                    "evidence.d1-support-reply-safe.policy",
                ],
                vec!["receipt.d1-support-reply-safe.agent_action_gate.ready"],
                Vec::<String>::new(),
            )),
        )
        .expect("missing gate ref fails closed");
    assert_eq!(missing_gate.status(), ActionGateStatus::MissingEvidence);
    assert_eq!(missing_gate.decision(), ActionGateDecision::Deny);
    assert_private_gates_closed(&missing_gate);

    let missing_handoff = evaluator
        .evaluate(
            &service_credit_review_request()
                .with_fixture_controls(ActionGateFixtureControls::bounded_private(None)),
        )
        .expect("missing review handoff fails closed");
    assert_eq!(
        missing_handoff.status(),
        ActionGateStatus::ReviewHandoffMissing
    );
    assert_eq!(missing_handoff.decision(), ActionGateDecision::Deny);
    assert!(!missing_handoff.derived_from_typed_facts());
    assert!(
        missing_handoff
            .blocker_labels()
            .contains(&"review_required_without_handoff")
    );
    assert_private_gates_closed(&missing_handoff);
}

#[test]
fn action_gate_rejects_forbidden_authority_and_unbounded_inputs() {
    let evaluator = ActionGateEvaluator::new(D1SupportPlaybookPolicy::new());

    let cases = [
        (
            ActionGateForbiddenAuthority::PublicProjection,
            ActionGateStatus::ForbiddenAuthority,
            "public_projection_forbidden",
        ),
        (
            ActionGateForbiddenAuthority::RuntimeProviderClaim,
            ActionGateStatus::ForbiddenAuthority,
            "runtime_provider_claim_forbidden",
        ),
        (
            ActionGateForbiddenAuthority::SecretOrCustomerData,
            ActionGateStatus::ForbiddenAuthority,
            "secret_or_customer_data_forbidden",
        ),
        (
            ActionGateForbiddenAuthority::UnboundedFileOrNetworkAccess,
            ActionGateStatus::ForbiddenAuthority,
            "unbounded_file_or_network_access_forbidden",
        ),
        (
            ActionGateForbiddenAuthority::AllHooksOpen,
            ActionGateStatus::ForbiddenAuthority,
            "all_hooks_open_forbidden",
        ),
        (
            ActionGateForbiddenAuthority::W561PlusContinuation,
            ActionGateStatus::ForbiddenAuthority,
            "w561_plus_forbidden",
        ),
    ];

    for (authority, expected_status, expected_label) in cases {
        let response = evaluator
            .evaluate(&safe_support_reply_request().with_forbidden_authority(authority))
            .expect("forbidden authority fails closed");
        assert_eq!(response.status(), expected_status);
        assert_eq!(response.decision(), ActionGateDecision::Deny);
        assert!(!response.derived_from_typed_facts());
        assert!(response.blocker_labels().contains(&expected_label));
        assert_private_gates_closed(&response);
    }
}

fn safe_support_reply_request() -> ActionGateRequest {
    d1_request(
        "d1-support-reply-safe",
        ActionGateActionFacts::new(
            "support_reply",
            "action.d1-support-reply-safe.support_reply",
        ),
        ActionGateConsequenceFacts::new(
            "low_support_reply",
            "consequence.d1-support-reply-safe.low",
            false,
            false,
            false,
        ),
        ActionGatePolicyFacts::new("policy.d1-support-reply-safe", true, false, false, false),
        None,
    )
}

fn account_export_blocked_request() -> ActionGateRequest {
    d1_request(
        "d1-account-export-blocked",
        ActionGateActionFacts::new(
            "account_export",
            "action.d1-account-export-blocked.account_export",
        )
        .with_public_projection_requested(true),
        ActionGateConsequenceFacts::new(
            "account_export",
            "consequence.d1-account-export-blocked.export",
            true,
            false,
            false,
        ),
        ActionGatePolicyFacts::new(
            "policy.d1-account-export-blocked",
            false,
            false,
            false,
            false,
        ),
        None,
    )
}

fn service_credit_review_request() -> ActionGateRequest {
    d1_request(
        "d1-service-credit-review",
        ActionGateActionFacts::new(
            "service_credit",
            "action.d1-service-credit-review.service_credit",
        ),
        ActionGateConsequenceFacts::new(
            "service_credit",
            "consequence.d1-service-credit-review.credit",
            false,
            true,
            true,
        ),
        ActionGatePolicyFacts::new("policy.d1-service-credit-review", false, false, true, false),
        Some("handoff.d1-service-credit-review.private_human_review"),
    )
}

fn d1_request(
    case_ref: &str,
    action: ActionGateActionFacts,
    consequence: ActionGateConsequenceFacts,
    policy: ActionGatePolicyFacts,
    handoff_ref: Option<&str>,
) -> ActionGateRequest {
    ActionGateRequest::new(
        "request.d1_support_playbook.private",
        case_ref,
        action,
        ActionGateActorContextFacts::new(
            format!("actor.{case_ref}.support_agent"),
            format!("context.{case_ref}.private_synthetic"),
            true,
            false,
            false,
        ),
        consequence,
        policy,
        ActionGateEvidenceSet::new(
            vec![
                format!("evidence.{case_ref}.action"),
                format!("evidence.{case_ref}.actor_context"),
                format!("evidence.{case_ref}.policy"),
            ],
            vec![
                format!("evidence.{case_ref}.action"),
                format!("evidence.{case_ref}.actor_context"),
                format!("evidence.{case_ref}.policy"),
            ],
            vec![format!("receipt.{case_ref}.agent_action_gate.ready")],
            vec![format!("gate.{case_ref}.agent_action_gate")],
        ),
        ActionGateFixtureControls::bounded_private(handoff_ref),
    )
}

fn assert_private_gates_closed(response: &nxuskit_engine::action_gate::ActionGateResponse) {
    assert!(!response.public_surface_authorized());
    assert!(!response.public_projection_authorized());
    assert!(!response.durable_write_authorized());
    assert!(!response.callback_authorized());
    assert!(!response.provider_runtime_authorized());
    assert!(!response.customer_or_production_data_authorized());
    assert!(!response.live_licensing_authorized());
    assert!(!response.release_or_package_authorized());
    assert!(!response.material_claim_authorized());
    assert!(!response.downstream_publication_authorized());
    assert!(!response.w561_plus_authorized());
    assert!(
        response
            .closed_gate_labels()
            .contains(&"w561_plus_continuation_closed")
    );
}
