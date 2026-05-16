//! v0.9.3 cached license entitlement performance smoke benchmark (T106).
//!
//! Run with:
//!
//!   cargo bench -p nxuskit-core --bench license_offline_check

#![allow(clippy::print_stdout)]

use std::hint::black_box;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use criterion::Criterion;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use p256::ecdsa::SigningKey;
use p256::pkcs8::EncodePrivateKey;

use nxuskit_core::license::{StubTokenVerifier, validate_token_full_with_verifier_and_environment};

const ITERATIONS: u32 = 2_000;
const MAX_AVG_NS: u128 = 400_000_000; // 400ms
const TEST_KEY_SEED: [u8; 32] = [
    0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10,
    0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20,
];

fn make_deployment_token() -> (String, String) {
    let signing_key = SigningKey::from_bytes((&TEST_KEY_SEED).into()).expect("valid test seed");
    let verifying_key = signing_key.verifying_key();

    let private_pem = signing_key
        .to_pkcs8_pem(p256::pkcs8::LineEnding::LF)
        .expect("PKCS8 encoding")
        .to_string();

    let point = verifying_key.to_encoded_point(false);
    let point_bytes = point.as_bytes();
    let algo_oid: &[u8] = &[
        0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08, 0x2a, 0x86,
        0x48, 0xce, 0x3d, 0x03, 0x01, 0x07,
    ];

    let bit_string_content_len = 1 + point_bytes.len();
    let mut bit_string = vec![0x03];
    if bit_string_content_len < 128 {
        bit_string.push(bit_string_content_len as u8);
    } else {
        bit_string.push(0x81);
        bit_string.push(bit_string_content_len as u8);
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

    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&der);
    let public_pem = format!(
        "-----BEGIN PUBLIC KEY-----\n{}\n-----END PUBLIC KEY-----\n",
        encoded
            .as_bytes()
            .chunks(64)
            .map(|chunk| std::str::from_utf8(chunk).unwrap())
            .collect::<Vec<_>>()
            .join("\n")
    );

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let claims = serde_json::json!({
        "iss": "nxus-licensing",
        "iat": now,
        "nbf": now,
        "type": "deployment",
        "edition": "enterprise",
        "product_id": "nxuskit",
        "tenant_id": "org-perf-001",
        "features_override": ["solver", "zen", "clips"],
        "limits_override": {
            "max_sessions": 256,
            "max_cached_rulebases": 64
        }
    });

    let encoding_key = EncodingKey::from_ec_pem(private_pem.as_bytes()).expect("valid EC key");
    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some("es256-v1".to_string());
    let token = encode(&header, &claims, &encoding_key).expect("JWT encoding");
    (token, public_pem)
}

fn avg_nanos(total: Duration) -> u128 {
    total.as_nanos() / u128::from(ITERATIONS)
}

fn main() {
    let (token, public_key_pem) = make_deployment_token();
    let verifier = StubTokenVerifier::with_public_key(&public_key_pem);

    let validated = validate_token_full_with_verifier_and_environment(&token, &verifier, "test")
        .expect("fixture token should validate locally");
    assert_eq!(validated.claims.edition, "enterprise");

    let start = std::time::Instant::now();
    for _ in 0..ITERATIONS {
        let validated = validate_token_full_with_verifier_and_environment(
            black_box(&token),
            black_box(&verifier),
            black_box("test"),
        )
        .expect("cached token validation should stay local");
        black_box(validated);
    }
    let avg = avg_nanos(start.elapsed());
    println!("license_offline_check_avg_ns={avg} iterations={ITERATIONS}");
    assert!(
        avg <= MAX_AVG_NS,
        "offline entitlement validation must stay <=400ms; observed {avg}ns"
    );

    // Keep the crate's existing Criterion dev dependency exercised in bench
    // builds without adding another benchmark group to this executable target.
    let _ = std::mem::size_of::<Criterion>();
}
