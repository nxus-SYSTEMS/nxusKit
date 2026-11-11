//! Feature 106 / US6 — public OrchestrationDefinition DTO tests.

use nxuskit::contract::v1_1::{
    OrchestrationDefinition, OrchestrationEdge, OrchestrationEdgeKind, OrchestrationNode,
    OrchestrationNodeKind, OrchestrationPosture, load_orchestration_definition_json,
};

fn sample_orchestration() -> OrchestrationDefinition {
    OrchestrationDefinition {
        contract_version: "1.1".to_string(),
        id: "dto-orchestration".to_string(),
        nodes: vec![
            OrchestrationNode {
                id: "collect".to_string(),
                kind: OrchestrationNodeKind::Provider,
                provider_ref: Some("loopback".to_string()),
                model_ref: Some("echo".to_string()),
                capability_ref: None,
                external_action_ref: None,
                posture: OrchestrationPosture::DeclarationOnly,
                layout_hint: None,
            },
            OrchestrationNode {
                id: "review".to_string(),
                kind: OrchestrationNodeKind::Convergence,
                provider_ref: None,
                model_ref: None,
                capability_ref: None,
                external_action_ref: None,
                posture: OrchestrationPosture::DeclarationOnly,
                layout_hint: None,
            },
        ],
        edges: vec![OrchestrationEdge {
            from: "collect".to_string(),
            to: "review".to_string(),
            kind: OrchestrationEdgeKind::Data,
            condition: None,
        }],
        metadata: Default::default(),
    }
}

#[test]
fn orchestration_definition_round_trips_and_hashes() {
    let orchestration = sample_orchestration();
    orchestration.validate().expect("valid");
    let json = orchestration.export_canonical_json().expect("export");
    let parsed = load_orchestration_definition_json(&json).expect("load");
    assert_eq!(parsed, orchestration);
    assert_eq!(parsed.canonical_hash().expect("hash").as_str().len(), 64);
}

#[test]
fn orchestration_rules_provider_must_be_declaration_only() {
    let mut orchestration = sample_orchestration();
    orchestration.nodes.push(OrchestrationNode {
        id: "rules".to_string(),
        kind: OrchestrationNodeKind::RulesProvider,
        provider_ref: None,
        model_ref: None,
        capability_ref: Some("rule_eval.generic".to_string()),
        external_action_ref: None,
        posture: OrchestrationPosture::Deferred,
        layout_hint: None,
    });
    let err = orchestration.validate().expect_err("must reject");
    assert!(err.to_string().contains("rules_provider"));
}
