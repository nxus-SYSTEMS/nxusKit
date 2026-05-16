mod license_fixture_support;

use std::collections::HashSet;

use license_fixture_support::{LicenseFixtureKind, all_license_fixtures};

#[test]
fn generated_fixtures_have_distinct_semantic_claims() {
    let fixtures = all_license_fixtures();
    assert_eq!(fixtures.len(), 5);

    let names = fixtures.iter().map(|f| f.name).collect::<HashSet<_>>();
    assert_eq!(
        names,
        HashSet::from([
            "trial",
            "developer",
            "deployment",
            "real_purchase",
            "leased"
        ])
    );

    let token_types = fixtures
        .iter()
        .map(|f| f.token_type())
        .collect::<HashSet<_>>();
    assert_eq!(
        token_types,
        HashSet::from([
            "trial",
            "developer",
            "deployment",
            "real_purchase",
            "leased"
        ])
    );

    let machines = fixtures
        .iter()
        .filter_map(|fixture| fixture.machine_id())
        .collect::<HashSet<_>>();
    assert_eq!(machines.len(), 4);
    assert!(machines.contains("sha256:test-machine-trial"));
    assert!(machines.contains("sha256:test-machine-developer"));
    assert!(machines.contains("sha256:test-machine-real-purchase"));
    assert!(machines.contains("sha256:test-machine-ci-lease"));

    let editions = fixtures.iter().map(|f| f.edition()).collect::<HashSet<_>>();
    assert!(editions.contains("pro"));
    assert!(editions.contains("enterprise"));

    for fixture in &fixtures {
        assert!(
            fixture.token.matches('.').count() == 2,
            "JWT has three segments"
        );
        assert!(fixture.public_key_pem.contains("BEGIN PUBLIC KEY"));
        assert!(!fixture.public_key_pem.contains("PRIVATE KEY"));
        assert_eq!(fixture.claims["product_id"], "nxuskit");
        assert!(fixture.claims["features_override"].is_array());
        assert!(fixture.claims["limits_override"].is_object());
        assert!(!fixture.environment().is_empty());
    }
}

#[test]
fn leased_fixture_matches_internal_ci_lease_contract() {
    let fixture = all_license_fixtures()
        .into_iter()
        .find(|fixture| fixture.kind == LicenseFixtureKind::Leased)
        .expect("leased fixture must exist");

    assert_eq!(fixture.token_type(), "leased");
    assert_eq!(fixture.edition(), "pro");
    assert_eq!(fixture.environment(), "test");
    assert_eq!(
        fixture.claims["limits_override"]["lease.duration_hours"],
        72
    );
    assert!(fixture.claims["exp"].as_i64().is_some());
    assert!(fixture.token.len() > 80);
    assert!(!fixture.token.contains("sk_live"));
    assert!(!fixture.token.contains("BEGIN PRIVATE KEY"));
}

#[test]
fn real_purchase_fixture_carries_storefront_shape_without_live_secrets() {
    let fixture = all_license_fixtures()
        .into_iter()
        .find(|fixture| fixture.kind == LicenseFixtureKind::RealPurchase)
        .expect("real-purchase fixture must exist");

    assert_eq!(fixture.token_type(), "real_purchase");
    assert_eq!(fixture.claims["purchase_id"], "pi_fixture_093");
    assert_eq!(fixture.claims["billing_source"], "stripe_live_shape_test");
    assert_eq!(fixture.environment(), "production-shape-test");
    assert!(fixture.token.len() > 80);
    assert!(!fixture.token.contains("sk_live"));
    assert!(!fixture.token.contains("BEGIN PRIVATE KEY"));
}
