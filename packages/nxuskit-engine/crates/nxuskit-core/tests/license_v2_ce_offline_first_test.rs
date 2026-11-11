mod license_fixture_support;

use license_fixture_support::{LicenseFixtureKind, make_license_fixture};
use nxuskit_core::license::{StubTokenVerifier, validate_token_full_with_verifier_and_environment};

#[test]
fn public_native_deployment_validation_is_local_and_offline_first() {
    let fixture = make_license_fixture(LicenseFixtureKind::Deployment);
    let verifier = StubTokenVerifier::with_public_key(&fixture.public_key_pem);

    let validated =
        validate_token_full_with_verifier_and_environment(&fixture.token, &verifier, "test")
            .expect("a deterministic public deployment fixture validates without a service call");

    assert_eq!(validated.claims.edition, "enterprise");
    assert_eq!(validated.claims.token_type.to_string(), "deployment");
    assert!(validated.deployment_v2.is_some());
}
