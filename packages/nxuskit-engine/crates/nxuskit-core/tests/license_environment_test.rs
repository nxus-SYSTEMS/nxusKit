#![allow(clippy::panic)]

mod license_fixture_support;

use license_fixture_support::{LicenseFixtureKind, make_license_fixture};
use nxuskit_core::license::{
    LicenseError, StubTokenVerifier, validate_token_full_with_verifier_and_environment,
};
use nxuskit_core::license_types::TokenType;

#[test]
fn release_environment_rejects_dev_lane_token_claims() {
    let fixture = make_license_fixture(LicenseFixtureKind::Deployment);
    let verifier = StubTokenVerifier::with_public_key(&fixture.public_key_pem);

    let error =
        validate_token_full_with_verifier_and_environment(&fixture.token, &verifier, "production")
            .expect_err("test-environment token must not validate in production lane");

    match error {
        LicenseError::EnvironmentMismatch { expected, actual } => {
            assert_eq!(expected, "production");
            assert_eq!(actual, "test");
        }
        other => panic!("expected environment mismatch, got {other:?}"),
    }
}

#[test]
fn dev_test_lane_accepts_dev_signed_fixture() {
    let fixture = make_license_fixture(LicenseFixtureKind::Deployment);
    let verifier = StubTokenVerifier::with_public_key(&fixture.public_key_pem);

    let validated =
        validate_token_full_with_verifier_and_environment(&fixture.token, &verifier, "test")
            .expect("test lane must accept test-environment fixture");

    assert_eq!(validated.claims.token_type, TokenType::Deployment);
    assert_eq!(validated.claims.product_id, "nxuskit");
    assert_eq!(validated.claims.edition, "enterprise");
}
