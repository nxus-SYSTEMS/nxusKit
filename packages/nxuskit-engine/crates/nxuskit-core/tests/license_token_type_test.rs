#![allow(clippy::panic)]

mod license_fixture_support;

use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use license_fixture_support::all_license_fixtures;
use nxuskit_core::license_types::{TokenType, ValidatedClaims};

#[test]
fn successful_token_classification_fixtures_validate_to_expected_types() {
    let fixtures = all_license_fixtures();

    for fixture in fixtures {
        let decoding_key = DecodingKey::from_ec_pem(fixture.public_key_pem.as_bytes())
            .expect("fixture public key must parse");
        let mut validation = Validation::new(Algorithm::ES256);
        validation.set_issuer(&["nxus-licensing"]);
        validation.validate_exp = false;
        validation.validate_nbf = false;
        validation.required_spec_claims.clear();

        let token =
            jsonwebtoken::decode::<ValidatedClaims>(&fixture.token, &decoding_key, &validation)
                .expect("fixture token must validate with its non-production public key");

        let expected = match fixture.name {
            "trial" => TokenType::Trial,
            "developer" => TokenType::Developer,
            "deployment" => TokenType::Deployment,
            "real_purchase" => TokenType::RealPurchase,
            "leased" => TokenType::Leased,
            other => panic!("unexpected fixture {other}"),
        };

        assert_eq!(token.claims.token_type, expected);
        assert_eq!(token.claims.product_id, "nxuskit");
        assert!(!token.claims.edition.is_empty());
        assert!(
            token
                .claims
                .features_override
                .as_ref()
                .is_some_and(|v| !v.is_empty()),
            "fixture {} must include feature entitlements",
            fixture.name
        );
        assert!(
            token
                .claims
                .limits_override
                .as_ref()
                .is_some_and(|v| !v.is_empty()),
            "fixture {} must include limit overrides",
            fixture.name
        );
    }
}
