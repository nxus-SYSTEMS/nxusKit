mod license_fixture_support;

use license_fixture_support::{
    LicenseFixtureKind, make_license_fixture, sign_license_fixture_claims,
};
use nxuskit_core::license::{StubTokenVerifier, validate_token_full_with_verifier_and_environment};
use nxuskit_core::license_types::TokenType;

#[test]
fn public_native_verifier_preserves_supported_legacy_leased_behavior() {
    let mut fixture = make_license_fixture(LicenseFixtureKind::Leased);
    let local_machine = nxuskit_core::machine_id::get_machine_fingerprint()
        .expect("local public-native test machine fingerprint must be available");
    fixture.claims["machine_id"] = serde_json::json!(local_machine);
    let (token, public_key_pem) = sign_license_fixture_claims(&fixture.claims);
    let verifier = StubTokenVerifier::with_public_key(&public_key_pem);

    let validated = validate_token_full_with_verifier_and_environment(&token, &verifier, "test")
        .expect("legacy leased behavior remains supported by the public native verifier");

    assert_eq!(validated.claims.token_type, TokenType::Leased);
    assert_eq!(validated.claims.edition, "pro");
    assert!(validated.claims.exp.is_some());
}
