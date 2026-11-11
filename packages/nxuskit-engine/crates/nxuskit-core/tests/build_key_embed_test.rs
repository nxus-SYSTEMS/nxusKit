use p256::pkcs8::DecodePublicKey;

const DEV_FALLBACK_PUBLIC_KEY_PEM: &str = "\
-----BEGIN PUBLIC KEY-----\n\
MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEtVZb9c5IG8tk8XX9jXTZXN5gTVD6\n\
fxJff/reMNBUVQ93zPoKwVomqCRvUcGRoT55ROyhkiaZKzLf9odouShJ9g==\n\
-----END PUBLIC KEY-----\n";

fn is_bound_production_verifier_bundle_source(source: &str) -> bool {
    source
        .replace('\\', "/")
        .ends_with("resources/licensing/production-verifier-bundle-v1.json")
}

#[test]
fn release_key_embedding_uses_the_bound_production_verifier_bundle() {
    let source = nxuskit_core::license::embedded_es256_public_key_source();
    let bundle = nxuskit_core::license::embedded_es256_public_key_bundle();

    assert!(
        is_bound_production_verifier_bundle_source(source),
        "embedded key source should be the bound verifier-bundle manifest, got {source}"
    );
    assert!(
        bundle
            .iter()
            .all(|(_, pem)| *pem != DEV_FALLBACK_PUBLIC_KEY_PEM),
        "every native verifier key must come from the immutable production bundle"
    );
}

#[test]
fn production_bundle_source_check_accepts_platform_separators() {
    assert!(is_bound_production_verifier_bundle_source(
        "/home/runner/work/nxusKit/packages/nxuskit-engine/crates/nxuskit-core/resources/licensing/production-verifier-bundle-v1.json"
    ));
    assert!(is_bound_production_verifier_bundle_source(
        r"D:\a\nxusKit\nxusKit\packages\nxuskit-engine\crates\nxuskit-core\resources\licensing\production-verifier-bundle-v1.json"
    ));
    assert!(!is_bound_production_verifier_bundle_source(
        "/home/runner/work/nxusKit/dev-keys/es256-dev-pubkey.pem"
    ));
}

#[test]
fn embedded_production_bundle_metadata_is_parseable_p256_with_expected_kid() {
    let bundle = nxuskit_core::license::embedded_es256_public_key_bundle();
    assert_eq!(bundle.len(), 1, "the frozen inventory has one enabled key");
    for (_, pem) in bundle {
        let _public_key = p256::PublicKey::from_public_key_pem(pem)
            .expect("every embedded bundle key must parse as P-256 PEM");
        assert!(pem.contains("BEGIN PUBLIC KEY"));
    }
    assert_eq!(
        bundle[0].0,
        "projects/nxus-licensing-prod/locations/us-central1/keyRings/licensing-signing/cryptoKeys/license-token-signing/cryptoKeyVersions/1"
    );
}
