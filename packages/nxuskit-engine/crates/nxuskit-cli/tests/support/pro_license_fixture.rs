#![allow(dead_code)]

use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use assert_cmd::Command;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use p256::ecdsa::SigningKey;
use p256::pkcs8::{EncodePrivateKey, EncodePublicKey, LineEnding};
use serde_json::{Value, json};

pub const FIXTURE_MODE_ENV: &str = "NXUSKIT_TEST_PRO_LICENSE_FIXTURE";

const DEPLOYMENT_CLAIMS_V2_FIXTURE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../nxuskit-core/tests/fixtures/licensing/deployment-claims-v2.json"
));
const SIGNING_KEY_SEED: [u8; 32] = [
    0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10,
    0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20,
];
const WRONG_KEY_SEED: [u8; 32] = [
    0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f, 0x30,
    0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e, 0x3f, 0x40,
];
static KEY_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug)]
pub enum FixtureKind {
    FullPro,
    WrongKey,
    MissingKey,
    RestrictiveSolverLimit,
}

pub struct FixtureGuard {
    key_path: PathBuf,
    remove_on_drop: bool,
}

impl Drop for FixtureGuard {
    fn drop(&mut self) {
        if self.remove_on_drop {
            let _ = std::fs::remove_file(&self.key_path);
        }
    }
}

pub fn fixture_mode_enabled() -> bool {
    match std::env::var(FIXTURE_MODE_ENV) {
        Err(std::env::VarError::NotPresent) => false,
        Ok(value) if value == "1" => true,
        Ok(value) => panic!("{FIXTURE_MODE_ENV} must be exactly 1 when set, got {value:?}"),
        Err(error) => panic!("{FIXTURE_MODE_ENV} must contain valid UTF-8: {error}"),
    }
}

pub fn authorize_if_fixture_mode(command: &mut Command) -> Option<FixtureGuard> {
    fixture_mode_enabled().then(|| authorize(command, FixtureKind::FullPro))
}

pub fn authorize(command: &mut Command, kind: FixtureKind) -> FixtureGuard {
    let mut claims: Value = serde_json::from_str(DEPLOYMENT_CLAIMS_V2_FIXTURE)
        .expect("frozen DeploymentClaimsV2 fixture must be valid JSON");
    claims["edition"] = json!("pro");
    claims["sdk_release_line"] = json!(2);
    claims["capability_grants"] = json!(["cap.solver", "cap.zen"]);
    claims["capability_denials"] = json!([]);
    claims["limits_override"] = match kind {
        FixtureKind::RestrictiveSolverLimit => json!({"solver.max_nodes": 1}),
        _ => json!({}),
    };

    let (private_pem, public_pem) = keypair(&SIGNING_KEY_SEED);
    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some("es256-v1".to_string());
    let token = encode(
        &header,
        &claims,
        &EncodingKey::from_ec_pem(private_pem.as_bytes())
            .expect("fixture ES256 private key must parse"),
    )
    .expect("fixture DeploymentClaimsV2 token must sign");

    let key_path = unique_key_path();
    let remove_on_drop = !matches!(kind, FixtureKind::MissingKey);
    if remove_on_drop {
        let key_bytes = if matches!(kind, FixtureKind::WrongKey) {
            keypair(&WRONG_KEY_SEED).1
        } else {
            public_pem
        };
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&key_path)
            .expect("create unique Pro fixture public-key file");
        file.write_all(key_bytes.as_bytes())
            .expect("write Pro fixture public-key file");
    }

    command
        .env_remove("NXUSKIT_LICENSE_KEY")
        .env_remove("NXUSKIT_PRO_TOKEN")
        .env_remove(FIXTURE_MODE_ENV)
        .env("NXUSKIT_LICENSE_TOKEN", token)
        .env("NXUS_SIGNING_KEY_PATH", &key_path);

    FixtureGuard {
        key_path,
        remove_on_drop,
    }
}

fn keypair(seed: &[u8; 32]) -> (String, String) {
    let signing_key = SigningKey::from_bytes(seed.into()).expect("fixture key seed must be valid");
    let private_pem = signing_key
        .to_pkcs8_pem(LineEnding::LF)
        .expect("fixture private key must encode")
        .to_string();
    let public_pem = signing_key
        .verifying_key()
        .to_public_key_pem(LineEnding::LF)
        .expect("fixture public key must encode");
    (private_pem, public_pem)
}

fn unique_key_path() -> PathBuf {
    let sequence = KEY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "nxuskit-pro-ci-fixture-{}-{sequence}.pem",
        std::process::id()
    ))
}
