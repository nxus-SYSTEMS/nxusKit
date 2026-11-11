#![allow(clippy::panic)]

mod license_fixture_support;

use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use license_fixture_support::{
    sign_license_fixture_claims_with_blank_kid, sign_license_fixture_claims_with_kid,
    sign_license_fixture_claims_without_kid,
};
use nxuskit_core::license::{
    LicenseError, NamedPemKey, StubTokenVerifier, validate_token_full,
    validate_token_full_with_verifier_and_environment,
};
use nxuskit_core::license_types::{TokenVerifier, VerifyError};
use serde_json::{Value, json};

const CURRENT_KID: &str = "projects/nxus-licensing-prod/locations/us-central1/keyRings/licensing-signing/cryptoKeys/license-token-signing/cryptoKeyVersions/1";
const RETAINED_KID: &str = "projects/nxus-licensing-prod/locations/us-central1/keyRings/licensing-signing/cryptoKeys/license-token-signing/cryptoKeyVersions/0";

fn frozen_claims() -> Value {
    json!({
        "iss": "nxus-licensing",
        "aud": "nxuskit-deployment-runtime",
        "sub": "account_7c1d9a",
        "jti": "8b3a28ec-d856-4e9e-884d-f9d1268db7e2",
        "iat": 1_721_779_200_i64,
        "type": "deployment",
        "product_id": "nxuskit",
        "edition": "pro",
        "grant_id": "grant_2f8b30",
        "grant_revision": 3,
        "purpose": "offline_deployment",
        "target_id": "target_7e1a5b",
        "policy_hash": "sha256:8d9a1f23c4567890abcdef1234567890abcdef1234567890abcdef1234567890",
        "sdk_release_line": 2,
        "authorization_scope": "client_entitlement",
        "capability_grants": ["cap.solver"],
        "capability_denials": ["cap.experimental"],
        "limits_override": {"solver.max_nodes": 1000}
    })
}

#[test]
fn native_bundle_is_additive_exact_kid_and_rejects_invalid_bundle_entries() {
    let claims = frozen_claims();
    let (_, public_key_pem) = sign_license_fixture_claims_with_kid(&claims, CURRENT_KID);
    let bundle = vec![
        NamedPemKey {
            kid: CURRENT_KID.to_string(),
            pem: public_key_pem.as_bytes().to_vec(),
        },
        NamedPemKey {
            kid: RETAINED_KID.to_string(),
            pem: public_key_pem.as_bytes().to_vec(),
        },
    ];
    let verifier = StubTokenVerifier::from_named_pem_bundle(bundle)
        .expect("the current and retained exact-kid public keys must form an additive bundle");

    for kid in [CURRENT_KID, RETAINED_KID] {
        let (token, _) = sign_license_fixture_claims_with_kid(&claims, kid);
        let result = verifier.verify(&token);
        assert!(
            result.is_ok(),
            "the exact retained/current kid {kid} must select its bundle key: {result:?}"
        );
    }

    let (unknown_token, _) = sign_license_fixture_claims_with_kid(
        &claims,
        "projects/nxus-licensing-prod/locations/us-central1/keyRings/licensing-signing/cryptoKeys/license-token-signing/cryptoKeyVersions/unknown",
    );
    assert!(matches!(
        verifier.verify(&unknown_token),
        Err(VerifyError::KeyError(_))
    ));

    let (missing_kid_token, _) = sign_license_fixture_claims_without_kid(&claims);
    assert!(matches!(
        validate_token_full_with_verifier_and_environment(
            &missing_kid_token,
            &verifier,
            "development"
        ),
        Err(LicenseError::MissingDeploymentKeyId)
    ));

    let (blank_kid_token, _) = sign_license_fixture_claims_with_blank_kid(&claims);
    assert!(matches!(
        validate_token_full_with_verifier_and_environment(
            &blank_kid_token,
            &verifier,
            "development"
        ),
        Err(LicenseError::MissingDeploymentKeyId)
    ));

    let mut algorithm_mismatch_header = Header::new(Algorithm::HS256);
    algorithm_mismatch_header.kid = Some(CURRENT_KID.to_string());
    let algorithm_mismatch_token = encode(
        &algorithm_mismatch_header,
        &claims,
        &EncodingKey::from_secret(b"not-an-es256-key"),
    )
    .expect("HS256 mismatch fixture must encode");
    assert!(matches!(
        verifier.verify(&algorithm_mismatch_token),
        Err(VerifyError::InvalidSignature { .. })
    ));

    let duplicate = StubTokenVerifier::from_named_pem_bundle(vec![
        NamedPemKey {
            kid: CURRENT_KID.to_string(),
            pem: public_key_pem.as_bytes().to_vec(),
        },
        NamedPemKey {
            kid: CURRENT_KID.to_string(),
            pem: public_key_pem.as_bytes().to_vec(),
        },
    ]);
    assert!(matches!(duplicate, Err(VerifyError::KeyError(_))));

    let blank = StubTokenVerifier::from_named_pem_bundle(vec![NamedPemKey {
        kid: " ".to_string(),
        pem: public_key_pem.as_bytes().to_vec(),
    }]);
    assert!(matches!(blank, Err(VerifyError::KeyError(_))));

    let malformed = StubTokenVerifier::from_named_pem_bundle(vec![NamedPemKey {
        kid: CURRENT_KID.to_string(),
        pem: b"not a PEM".to_vec(),
    }]);
    assert!(matches!(malformed, Err(VerifyError::KeyError(_))));
}

#[test]
fn no_default_features_runtime_rejects_unknown_deployment_kid() {
    let claims = frozen_claims();
    let (current_token, public_key_pem) =
        sign_license_fixture_claims_with_kid(&claims, CURRENT_KID);
    let key_path = std::env::temp_dir().join("nxuskit-native-bundle-test-key.pem");
    std::fs::write(&key_path, public_key_pem)
        .expect("the deterministic native test public key must be writable");
    // SAFETY: this dedicated no-default-features test process sets the only
    // debug/test override before the global verifier initializes.
    unsafe { std::env::set_var("NXUS_SIGNING_KEY_PATH", &key_path) };
    validate_token_full(&current_token)
        .expect("the native no-default-features runtime must accept the exact current kid");

    let (unknown_token, _) = sign_license_fixture_claims_with_kid(
        &claims,
        "projects/nxus-licensing-prod/locations/us-central1/keyRings/licensing-signing/cryptoKeys/license-token-signing/cryptoKeyVersions/unknown",
    );
    assert!(matches!(
        validate_token_full(&unknown_token),
        Err(LicenseError::KeyError(_))
    ));
}
