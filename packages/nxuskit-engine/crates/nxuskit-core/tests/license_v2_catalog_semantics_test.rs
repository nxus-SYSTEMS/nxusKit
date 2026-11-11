use std::collections::HashMap;

use nxuskit_core::entitlement::{DeploymentCatalogV2, resolve_v2_deployment_entitlement};
use nxuskit_core::license_types::DeploymentV2Metadata;

const CATALOG_V2_2: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/licensing/catalog-v2-2.json"
));
const CATALOG_V2_9: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/licensing/catalog-v2-9.json"
));

fn deployment_metadata() -> DeploymentV2Metadata {
    DeploymentV2Metadata {
        sdk_release_line: 2,
        grant_revision: 3,
        authorization_scope: "client_entitlement".to_string(),
        capability_grants: vec!["cap.solver".to_string()],
        capability_denials: vec!["cap.experimental".to_string()],
        limits_override: HashMap::from([("solver.max_nodes".to_string(), serde_json::json!(1000))]),
    }
}

#[test]
fn same_line_catalog_growth_applies_without_reissuing_the_deployment_token() {
    let claims = deployment_metadata();
    let catalog_v2_2: DeploymentCatalogV2 =
        serde_json::from_str(CATALOG_V2_2).expect("v2.2 catalog fixture must parse");
    let catalog_v2_9: DeploymentCatalogV2 =
        serde_json::from_str(CATALOG_V2_9).expect("v2.9 catalog fixture must parse");

    let before = resolve_v2_deployment_entitlement(&catalog_v2_2, "pro", &claims)
        .expect("v2.2 catalog must resolve the signed deployment facts");
    let after = resolve_v2_deployment_entitlement(&catalog_v2_9, "pro", &claims)
        .expect("v2.9 catalog must resolve the same signed deployment facts");

    assert!(!before.capabilities.contains("cap.guardrail.policy_engine"));
    assert!(after.capabilities.contains("cap.guardrail.policy_engine"));
    assert!(after.capabilities.contains("cap.solver"));
    assert!(!after.capabilities.contains("cap.experimental"));
    assert!(!after.capabilities.contains("addon.managed_evidence"));
    assert_eq!(before.limits["solver.max_nodes"], serde_json::json!(1000));
    assert_eq!(after.limits["solver.max_nodes"], serde_json::json!(1000));
}

#[test]
fn signed_grants_are_additive_but_signed_denials_are_last_and_limits_stay_fixed() {
    let catalog: DeploymentCatalogV2 =
        serde_json::from_str(CATALOG_V2_9).expect("v2.9 catalog fixture must parse");
    let mut claims = deployment_metadata();
    claims
        .capability_grants
        .push("addon.managed_evidence".to_string());
    claims
        .capability_denials
        .push("addon.managed_evidence".to_string());

    let resolved = resolve_v2_deployment_entitlement(&catalog, "pro", &claims)
        .expect("signed deployment facts must resolve");
    assert!(
        !resolved.capabilities.contains("addon.managed_evidence"),
        "a signed denial must win even over a signed grant"
    );
    assert_eq!(resolved.limits["solver.max_nodes"], serde_json::json!(1000));
}

#[test]
fn unknown_catalog_facts_and_cross_line_catalogs_fail_closed() {
    let catalog: DeploymentCatalogV2 =
        serde_json::from_str(CATALOG_V2_9).expect("v2.9 catalog fixture must parse");
    let mut unknown_capability = deployment_metadata();
    unknown_capability
        .capability_grants
        .push("cap.not_in_catalog".to_string());
    assert!(
        resolve_v2_deployment_entitlement(&catalog, "pro", &unknown_capability).is_err(),
        "an unrecognized signed capability must not become entitled"
    );

    let mut cross_line = deployment_metadata();
    cross_line.sdk_release_line = 3;
    assert!(
        resolve_v2_deployment_entitlement(&catalog, "pro", &cross_line).is_err(),
        "a v2 catalog must never resolve a v3 deployment token"
    );
}

#[test]
fn unknown_catalog_shape_fields_are_rejected_before_entitlement_resolution() {
    let mut malformed: serde_json::Value =
        serde_json::from_str(CATALOG_V2_9).expect("v2.9 catalog fixture must parse as JSON");
    malformed["unrecognized_catalog_field"] = serde_json::json!(true);
    assert!(
        serde_json::from_value::<DeploymentCatalogV2>(malformed).is_err(),
        "the SDK must not silently accept an unknown catalog shape field"
    );
}
