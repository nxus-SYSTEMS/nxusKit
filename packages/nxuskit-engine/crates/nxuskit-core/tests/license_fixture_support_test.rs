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
        assert!(fixture.claims["limits_override"].is_object());
        if fixture.kind == LicenseFixtureKind::Deployment {
            assert_eq!(fixture.claims["aud"], "nxuskit-deployment-runtime");
            assert_eq!(fixture.claims["sdk_release_line"], 2);
            assert!(fixture.claims["capability_grants"].is_array());
            assert!(fixture.claims["capability_denials"].is_array());
            assert!(fixture.claims.get("customer_email").is_none());
            assert!(fixture.claims.get("sdk_version_ceiling").is_none());
            assert!(fixture.claims.get("environment").is_none());
        } else {
            assert!(fixture.claims["features_override"].is_array());
            assert!(!fixture.environment().is_empty());
        }
    }
}

#[test]
fn fixture_public_keys_preserve_uncompressed_spki_and_es256_verification() {
    use base64::Engine;
    use p256::ecdsa::{Signature, VerifyingKey, signature::Verifier};
    use p256::pkcs8::DecodePublicKey;

    // RFC 5480: id-ecPublicKey + prime256v1, followed by an uncompressed
    // 65-byte SEC1 point in a BIT STRING with zero unused bits.
    const SPKI_PREFIX: &[u8] = &[
        0x30, 0x59, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08,
        0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07, 0x03, 0x42, 0x00, 0x04,
    ];
    let fixtures = all_license_fixtures();
    assert_eq!(fixtures.len(), 5);
    for fixture in fixtures {
        let body = fixture
            .public_key_pem
            .lines()
            .filter(|line| !line.starts_with("-----"))
            .collect::<String>();
        let der = base64::engine::general_purpose::STANDARD
            .decode(body)
            .expect("fixture SPKI must use standard base64");
        assert_eq!(der.len(), 91);
        assert!(der.starts_with(SPKI_PREFIX));
        let key = VerifyingKey::from_public_key_pem(&fixture.public_key_pem)
            .expect("fixture public key must decode as P-256 SPKI");
        let parts = fixture.token.split('.').collect::<Vec<_>>();
        assert_eq!(parts.len(), 3);
        let header: serde_json::Value = serde_json::from_slice(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(parts[0])
                .expect("JWT header base64url"),
        )
        .expect("JWT header JSON");
        assert_eq!(header["alg"], "ES256");
        let signature_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(parts[2])
            .expect("JWT signature base64url");
        assert_eq!(signature_bytes.len(), 64, "JWT uses raw P-256 r || s");
        let signature = Signature::from_slice(&signature_bytes).expect("raw ES256 signature");
        let message = format!("{}.{}", parts[0], parts[1]);
        key.verify(message.as_bytes(), &signature)
            .expect("AWS-LC fixture JWT must verify through the independent Rust P-256 API");
        assert!(
            key.verify(format!("{message}!corrupt").as_bytes(), &signature)
                .is_err(),
            "modified signing input must not verify",
        );
        let mut corrupted = signature_bytes;
        corrupted[63] ^= 1;
        assert!(match Signature::from_slice(&corrupted) {
            Ok(corrupted) => key.verify(message.as_bytes(), &corrupted).is_err(),
            Err(_) => true,
        });
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
