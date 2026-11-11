use sha2::{Digest, Sha256};

const DEPLOYMENT_CLAIMS_V2_SCHEMA: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/licensing/deployment-claims-v2.schema.json"
));
const DEPLOYMENT_CLAIMS_V2_FIXTURE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/licensing/deployment-claims-v2.json"
));
const PRODUCTION_VERIFIER_BUNDLE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/resources/licensing/production-verifier-bundle-v1.json"
));
const PRODUCTION_VERIFIER_BUNDLE_SCHEMA: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/resources/licensing/production-verifier-bundle-v1.schema.json"
));
const PRODUCTION_VERIFIER_BUNDLE_PEM: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/resources/licensing/production-verifier-bundle-v1/version-1.pem"
));

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[test]
fn frozen_contract_bytes_and_provenance_match_manager_accepted_inputs() {
    assert_eq!(
        sha256_hex(DEPLOYMENT_CLAIMS_V2_SCHEMA),
        "1a2a2e011db578d262a63891c999839a8110a4fde0cfab4a7cefa9f1e68a321f"
    );
    assert_eq!(
        sha256_hex(DEPLOYMENT_CLAIMS_V2_FIXTURE),
        "ec07deb95c1658839fa2ced072e938fc0f97aec73df4d22d141a1178f7319ec4"
    );

    assert_eq!(
        sha256_hex(PRODUCTION_VERIFIER_BUNDLE),
        "ac569d0499cd994728f20c5f68e61af167dc2229648b770460a4e694e9410931"
    );
    assert_eq!(
        sha256_hex(PRODUCTION_VERIFIER_BUNDLE_SCHEMA),
        "fb95abeb2cc3f4af3433d4d39ff4453b1d6c70599ed18fc1b6d65a045006c50f"
    );
    assert_eq!(
        sha256_hex(PRODUCTION_VERIFIER_BUNDLE_PEM),
        "f5a48e001977f7b44a9f77f4ea38ae3d9707f5d49aca4968bd923eac38edfdaf"
    );
}
