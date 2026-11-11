#![allow(clippy::panic)]

use assert_cmd::Command;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use p256::ecdsa::SigningKey;
use p256::pkcs8::{EncodePrivateKey, EncodePublicKey, LineEnding};
use serde_json::Value;

const DEPLOYMENT_CLAIMS_V2_FIXTURE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../nxuskit-core/tests/fixtures/licensing/deployment-claims-v2.json"
));
const TEST_KEY_SEED: [u8; 32] = [
    0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10,
    0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20,
];

fn signed_fixture(release_line: u64) -> (String, String) {
    let signing_key = SigningKey::from_bytes((&TEST_KEY_SEED).into())
        .expect("deterministic test seed must be valid");
    let private_pem = signing_key
        .to_pkcs8_pem(LineEnding::LF)
        .expect("test key must encode")
        .to_string();
    let public_pem = signing_key
        .verifying_key()
        .to_public_key_pem(LineEnding::LF)
        .expect("test public key must encode");
    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some("es256-v1".to_string());
    let mut claims: Value = serde_json::from_str(DEPLOYMENT_CLAIMS_V2_FIXTURE)
        .expect("frozen Licensing fixture must be valid JSON");
    claims["sdk_release_line"] = serde_json::json!(release_line);
    let token = encode(
        &header,
        &claims,
        &EncodingKey::from_ec_pem(private_pem.as_bytes()).expect("test private key must parse"),
    )
    .expect("fixture token must sign");
    (token, public_pem)
}

#[test]
fn compiled_cli_inspects_a_validated_v2_token_without_printing_the_token() {
    let (token, public_pem) = signed_fixture(2);
    let key_path = std::env::temp_dir().join(format!(
        "nxuskit-cli-w540-v2-public-key-{}",
        std::process::id()
    ));
    std::fs::write(&key_path, public_pem).expect("write deterministic public key");

    let output = Command::cargo_bin("nxuskit-cli")
        .expect("compiled nxuskit-cli must be available")
        .args(["license", "inspect", "--token", &token, "--json"])
        .env("NXUS_SIGNING_KEY_PATH", &key_path)
        .output()
        .expect("invoke v2 deployment-token inspection command");
    let _ = std::fs::remove_file(&key_path);

    assert!(
        output.status.success(),
        "CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("CLI output must be UTF-8");
    let result: Value = serde_json::from_str(&stdout).expect("CLI must emit JSON");
    assert_eq!(result["valid"], true);
    assert_eq!(result["sdk_release_line"], 2);
    assert_eq!(result["grant_revision"], 3);
    assert_eq!(result["authorization_scope"], "client_entitlement");
    assert_eq!(result["signing_key_id"], "es256-v1");
    assert!(!stdout.contains(&token));
}

#[test]
fn compiled_cli_rejects_a_cross_line_v2_shape_without_printing_the_token() {
    let (token, public_pem) = signed_fixture(3);
    let key_path = std::env::temp_dir().join(format!(
        "nxuskit-cli-w540-v3-public-key-{}",
        std::process::id()
    ));
    std::fs::write(&key_path, public_pem).expect("write deterministic public key");

    let output = Command::cargo_bin("nxuskit-cli")
        .expect("compiled nxuskit-cli must be available")
        .args(["license", "inspect", "--token", &token, "--json"])
        .env("NXUS_SIGNING_KEY_PATH", &key_path)
        .output()
        .expect("invoke v2 deployment-token inspection command");
    let _ = std::fs::remove_file(&key_path);

    assert!(!output.status.success(), "cross-line token must fail");
    let stdout = String::from_utf8(output.stdout).expect("CLI output must be UTF-8");
    let result: Value = serde_json::from_str(&stdout).expect("CLI must emit JSON");
    assert_eq!(result["valid"], false);
    assert_eq!(result["result"], "incompatible_release_line");
    assert!(!stdout.contains(&token));
    assert!(!String::from_utf8_lossy(&output.stderr).contains(&token));
}
