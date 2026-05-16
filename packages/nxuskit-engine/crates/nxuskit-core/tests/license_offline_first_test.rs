mod license_fixture_support;

use std::sync::atomic::{AtomicUsize, Ordering};

use license_fixture_support::{
    LicenseFixtureKind, make_license_fixture, sign_license_fixture_claims,
};
use nxuskit_core::license::{
    StubTokenVerifier, refresh_cached_license_with_client_and_verifier,
    validate_token_full_with_verifier_and_environment,
};

#[test]
fn cached_token_validation_is_local_until_explicit_refresh() {
    let fixture = make_license_fixture(LicenseFixtureKind::Deployment);
    let verifier = StubTokenVerifier::with_public_key(&fixture.public_key_pem);
    let api_calls = AtomicUsize::new(0);

    let validated =
        validate_token_full_with_verifier_and_environment(&fixture.token, &verifier, "test")
            .expect("cached deployment fixture must validate locally");

    assert_eq!(validated.claims.edition, "enterprise");
    assert_eq!(api_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn explicit_refresh_calls_licensing_api_once_and_returns_refreshed_entitlement() {
    let current = make_license_fixture(LicenseFixtureKind::Deployment);
    let mut refreshed_claims = current.claims.clone();
    refreshed_claims["edition"] = serde_json::json!("pro");
    refreshed_claims["tenant_id"] = serde_json::json!("org-refreshed-fixture");
    let (refreshed_token, _) = sign_license_fixture_claims(&refreshed_claims);
    let verifier = StubTokenVerifier::with_public_key(&current.public_key_pem);
    let api_calls = AtomicUsize::new(0);

    let resolution = refresh_cached_license_with_client_and_verifier(
        &current.token,
        &verifier,
        "test",
        |_url, body| {
            api_calls.fetch_add(1, Ordering::SeqCst);
            assert_eq!(
                body.get("token").and_then(|value| value.as_str()),
                Some(current.token.as_str())
            );
            Ok(serde_json::json!({
                "token": refreshed_token,
                "message": "refreshed"
            }))
        },
    )
    .expect("explicit refresh should validate returned token");

    assert_eq!(api_calls.load(Ordering::SeqCst), 1);
    assert!(resolution.valid);
    let claims = resolution.claims.expect("valid refresh must carry claims");
    assert_eq!(claims.edition, "pro");
    assert_eq!(claims.tenant_id.as_deref(), Some("org-refreshed-fixture"));
    assert_eq!(claims.token_type.to_string(), "deployment");
}
