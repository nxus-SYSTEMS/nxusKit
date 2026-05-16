use p256::pkcs8::DecodePublicKey;

const DEV_FALLBACK_PUBLIC_KEY_PEM: &str = "\
-----BEGIN PUBLIC KEY-----\n\
MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEtVZb9c5IG8tk8XX9jXTZXN5gTVD6\n\
fxJff/reMNBUVQ93zPoKwVomqCRvUcGRoT55ROyhkiaZKzLf9odouShJ9g==\n\
-----END PUBLIC KEY-----\n";

fn is_devops_production_key_source(source: &str) -> bool {
    source
        .replace('\\', "/")
        .ends_with("sharedData/keys/es256-production-pubkey.pem")
}

#[test]
fn release_key_embedding_uses_devops_production_public_key() {
    let source = nxuskit_core::license::embedded_es256_public_key_source();

    assert!(
        is_devops_production_key_source(source),
        "embedded key source should be a DevOps production artifact, got {source}"
    );
    assert_ne!(
        nxuskit_core::license::embedded_es256_public_key_pem(),
        DEV_FALLBACK_PUBLIC_KEY_PEM
    );
}

#[test]
fn production_key_source_check_accepts_platform_separators() {
    assert!(is_devops_production_key_source(
        "/home/runner/work/nxusKit/.devops-catalog/sharedData/keys/es256-production-pubkey.pem"
    ));
    assert!(is_devops_production_key_source(
        r"D:\a\nxusKit\nxusKit\.devops-catalog\sharedData\keys\es256-production-pubkey.pem"
    ));
    assert!(!is_devops_production_key_source(
        "/home/runner/work/nxusKit/dev-keys/es256-dev-pubkey.pem"
    ));
}

#[test]
fn embedded_production_key_metadata_is_parseable_p256_with_expected_kid() {
    let pem = nxuskit_core::license::embedded_es256_public_key_pem();
    let _public_key =
        p256::PublicKey::from_public_key_pem(pem).expect("embedded key must parse as P-256 PEM");

    assert!(pem.contains("BEGIN PUBLIC KEY"));
    assert_eq!(
        nxuskit_core::license::embedded_es256_public_key_kid(),
        "es256-v1"
    );
}
