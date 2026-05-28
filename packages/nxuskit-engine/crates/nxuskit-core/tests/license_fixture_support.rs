#![allow(dead_code)]

use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use p256::ecdsa::SigningKey;
use p256::pkcs8::EncodePrivateKey;
use serde_json::{Value, json};

const TEST_KEY_SEED: [u8; 32] = [
    0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10,
    0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LicenseFixtureKind {
    Trial,
    Developer,
    Deployment,
    RealPurchase,
    Leased,
}

#[derive(Debug, Clone)]
pub struct LicenseFixture {
    pub name: &'static str,
    pub kind: LicenseFixtureKind,
    pub token: String,
    pub public_key_pem: String,
    pub claims: Value,
}

impl LicenseFixture {
    pub fn token_type(&self) -> &str {
        self.claims["type"]
            .as_str()
            .expect("fixture token type claim must be a string")
    }

    pub fn edition(&self) -> &str {
        self.claims["edition"]
            .as_str()
            .expect("fixture edition claim must be a string")
    }

    pub fn machine_id(&self) -> Option<&str> {
        self.claims.get("machine_id").and_then(Value::as_str)
    }

    pub fn environment(&self) -> &str {
        self.claims["environment"]
            .as_str()
            .expect("fixture environment claim must be a string")
    }
}

pub fn all_license_fixtures() -> Vec<LicenseFixture> {
    vec![
        make_license_fixture(LicenseFixtureKind::Trial),
        make_license_fixture(LicenseFixtureKind::Developer),
        make_license_fixture(LicenseFixtureKind::Deployment),
        make_license_fixture(LicenseFixtureKind::RealPurchase),
        make_license_fixture(LicenseFixtureKind::Leased),
    ]
}

pub fn make_license_fixture(kind: LicenseFixtureKind) -> LicenseFixture {
    let (private_pem, public_key_pem) = test_es256_keypair();
    let now = now_unix();
    let one_day = 86_400;

    let (name, claims) = match kind {
        LicenseFixtureKind::Trial => (
            "trial",
            json!({
                "iss": "nxus-licensing",
                "iat": now,
                "nbf": now,
                "exp": now + 30 * one_day,
                "type": "trial",
                "edition": "pro",
                "product_id": "nxuskit",
                "machine_id": "sha256:test-machine-trial",
                "activated": true,
                "environment": "test",
                "features_override": ["solver", "zen"],
                "limits_override": {"solver.steps": 1000}
            }),
        ),
        LicenseFixtureKind::Developer => (
            "developer",
            json!({
                "iss": "nxus-licensing",
                "iat": now,
                "nbf": now,
                "exp": now + 365 * one_day,
                "type": "developer",
                "edition": "pro",
                "product_id": "nxuskit",
                "tenant_id": "org-dev-fixture",
                "machine_id": "sha256:test-machine-developer",
                "seat_index": 1,
                "environment": "test",
                "features_override": ["solver", "zen", "clips"],
                "limits_override": {"developer.seats": 3}
            }),
        ),
        LicenseFixtureKind::Deployment => (
            "deployment",
            json!({
                "iss": "nxus-licensing",
                "iat": now,
                "type": "deployment",
                "edition": "enterprise",
                "product_id": "nxuskit",
                "tenant_id": "org-deploy-fixture",
                "customer_email": "fixture@example.invalid",
                "sdk_version_ceiling": "1.0",
                "environment": "test",
                "features_override": ["solver", "zen", "clips", "bayesian"],
                "limits_override": {"deployment.nodes": 10}
            }),
        ),
        LicenseFixtureKind::RealPurchase => (
            "real_purchase",
            json!({
                "iss": "nxus-licensing",
                "iat": now,
                "nbf": now,
                "exp": now + 365 * one_day,
                "type": "real_purchase",
                "edition": "pro",
                "product_id": "nxuskit",
                "tenant_id": "org-purchase-fixture",
                "machine_id": "sha256:test-machine-real-purchase",
                "purchase_id": "pi_fixture_093",
                "billing_source": "stripe_live_shape_test",
                "environment": "production-shape-test",
                "features_override": ["solver", "zen", "clips"],
                "limits_override": {"purchased.seats": 1, "solver.steps": 5000}
            }),
        ),
        LicenseFixtureKind::Leased => (
            "leased",
            json!({
                "iss": "nxus-licensing",
                "iat": now,
                "nbf": now,
                "exp": now + 72 * 60 * 60,
                "type": "leased",
                "edition": "pro",
                "product_id": "nxuskit",
                "tenant_id": "org-ci-fixture",
                "machine_id": "sha256:test-machine-ci-lease",
                "seat_index": 1,
                "environment": "test",
                "features_override": ["solver", "zen", "clips"],
                "limits_override": {"lease.duration_hours": 72}
            }),
        ),
    };

    let token = sign_json_claims(&claims, &private_pem);
    LicenseFixture {
        name,
        kind,
        token,
        public_key_pem,
        claims,
    }
}

pub fn sign_license_fixture_claims(claims: &Value) -> (String, String) {
    sign_license_fixture_claims_with_kid(claims, "es256-v1-test")
}

pub fn sign_license_fixture_claims_with_kid(claims: &Value, kid: &str) -> (String, String) {
    let (private_pem, public_key_pem) = test_es256_keypair();
    (
        sign_json_claims_with_kid(claims, &private_pem, kid),
        public_key_pem,
    )
}

fn sign_json_claims(claims: &Value, private_pem: &str) -> String {
    sign_json_claims_with_kid(claims, private_pem, "es256-v1-test")
}

fn sign_json_claims_with_kid(claims: &Value, private_pem: &str, kid: &str) -> String {
    let encoding_key = EncodingKey::from_ec_pem(private_pem.as_bytes())
        .expect("test ES256 private key must be valid");
    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some(kid.to_string());
    encode(&header, claims, &encoding_key).expect("test JWT encoding must succeed")
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after Unix epoch")
        .as_secs() as i64
}

fn test_es256_keypair() -> (String, String) {
    let signing_key = SigningKey::from_bytes((&TEST_KEY_SEED).into())
        .expect("deterministic test seed must be valid");
    let verifying_key = signing_key.verifying_key();

    let private_pem = signing_key
        .to_pkcs8_pem(p256::pkcs8::LineEnding::LF)
        .expect("PKCS8 test private key encoding must succeed")
        .to_string();

    let point = verifying_key.to_encoded_point(false);
    let der = encode_ec_public_key_der(point.as_bytes());
    let encoded = base64::engine::general_purpose::STANDARD.encode(der);
    let body = encoded
        .as_bytes()
        .chunks(64)
        .map(|chunk| std::str::from_utf8(chunk).expect("base64 is utf8"))
        .collect::<Vec<_>>()
        .join("\n");
    let public_key_pem = format!("-----BEGIN PUBLIC KEY-----\n{body}\n-----END PUBLIC KEY-----\n");

    (private_pem, public_key_pem)
}

fn encode_ec_public_key_der(point_bytes: &[u8]) -> Vec<u8> {
    let algo_oid: &[u8] = &[
        0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08, 0x2a, 0x86,
        0x48, 0xce, 0x3d, 0x03, 0x01, 0x07,
    ];

    let mut bit_string = vec![0x03];
    let bit_string_len = 1 + point_bytes.len();
    if bit_string_len < 128 {
        bit_string.push(bit_string_len as u8);
    } else {
        bit_string.push(0x81);
        bit_string.push(bit_string_len as u8);
    }
    bit_string.push(0x00);
    bit_string.extend_from_slice(point_bytes);

    let inner_len = algo_oid.len() + bit_string.len();
    let mut der = vec![0x30];
    if inner_len < 128 {
        der.push(inner_len as u8);
    } else {
        der.push(0x81);
        der.push(inner_len as u8);
    }
    der.extend_from_slice(algo_oid);
    der.extend_from_slice(&bit_string);
    der
}
