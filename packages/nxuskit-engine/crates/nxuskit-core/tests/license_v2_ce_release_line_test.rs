mod license_fixture_support;

use license_fixture_support::sign_license_fixture_claims_with_kid;
use nxuskit_core::license::{
    StubTokenVerifier, allows_sdk_version, validate_token_full_with_verifier_and_environment,
};

#[test]
fn public_native_v2_release_line_accepts_v2_and_rejects_v3() {
    let claims = serde_json::json!({
        "iss": "nxus-licensing",
        "aud": "nxuskit-deployment-runtime",
        "sub": "public-ce-fixture",
        "jti": "11111111-1111-4111-8111-111111111111",
        "iat": 1_700_000_000_i64,
        "type": "deployment",
        "edition": "enterprise",
        "product_id": "nxuskit",
        "grant_id": "public-ce-grant",
        "grant_revision": 3,
        "purpose": "offline_deployment",
        "target_id": "public-ce-target",
        "policy_hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
        "sdk_release_line": 2,
        "authorization_scope": "client_entitlement",
        "capability_grants": ["cap.solver"],
        "capability_denials": [],
        "limits_override": {}
    });
    let (token, public_key_pem) = sign_license_fixture_claims_with_kid(&claims, "public-ce-kid");
    let verifier = StubTokenVerifier::with_public_key(&public_key_pem);

    assert!(allows_sdk_version("2.9.0", 2));
    assert!(!allows_sdk_version("3.0.0", 2));
    assert!(validate_token_full_with_verifier_and_environment(&token, &verifier, "test").is_ok());

    let mut v3_claims = claims;
    v3_claims["sdk_release_line"] = serde_json::json!(3);
    let (v3_token, _) = sign_license_fixture_claims_with_kid(&v3_claims, "public-ce-kid");
    assert!(
        validate_token_full_with_verifier_and_environment(&v3_token, &verifier, "test").is_err()
    );
}
