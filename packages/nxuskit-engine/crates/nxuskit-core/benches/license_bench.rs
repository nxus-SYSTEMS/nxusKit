//! Performance benchmarks for licensing subsystem (T053-T055).
//!
//! Targets (Article XVI):
//! - Token validation (ES256 verify): <=5ms
//! - Catalog feature lookup: <=0.1ms
//! - Catalog limit lookup + merge: <=0.1ms

use std::collections::HashMap;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

use criterion::{Criterion, criterion_group, criterion_main};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use p256::ecdsa::SigningKey;
use p256::pkcs8::EncodePrivateKey;

use nxuskit_core::catalog::{catalog_features, catalog_limits, merge_limits_override};
use nxuskit_core::license::StubTokenVerifier;
use nxuskit_core::license_types::TokenVerifier;

/// Deterministic seed for reproducible test keys (matches test_fixtures.rs).
const TEST_KEY_SEED: [u8; 32] = [
    0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10,
    0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20,
];

/// Generate a deterministic ES256 key pair and a signed developer token.
/// Returns (jwt_token, public_key_pem).
fn make_developer_token() -> (String, String) {
    let signing_key = SigningKey::from_bytes((&TEST_KEY_SEED).into()).expect("valid 32-byte seed");
    let verifying_key = signing_key.verifying_key();

    let private_pem = signing_key
        .to_pkcs8_pem(p256::pkcs8::LineEnding::LF)
        .expect("PKCS8 encoding")
        .to_string();

    // Build public key PEM from the SEC1 encoded point
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
    let b64_lines: String = encoded
        .as_bytes()
        .chunks(64)
        .map(|c| std::str::from_utf8(c).unwrap())
        .collect::<Vec<&str>>()
        .join("\n");
    let public_pem = format!(
        "-----BEGIN PUBLIC KEY-----\n{}\n-----END PUBLIC KEY-----\n",
        b64_lines
    );

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let claims = serde_json::json!({
        "iss": "nxus-licensing",
        "iat": now,
        "nbf": now,
        "exp": now + 365 * 86400,
        "type": "developer",
        "edition": "pro",
        "product_id": "nxuskit",
        "tenant_id": "org-test-001",
        "machine_id": "sha256:test-machine-001",
        "seat_index": 1,
    });

    let encoding_key = EncodingKey::from_ec_pem(private_pem.as_bytes()).expect("valid EC PEM key");
    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some("test-key-001".to_string());
    let token = encode(&header, &claims, &encoding_key).expect("JWT encoding");
    (token, public_pem)
}

/// Write the public key PEM to a temp file and configure NXUS_SIGNING_KEY_PATH
/// so that `StubTokenVerifier::new()` picks it up.
/// Returns the verifier and a handle to the temp file (keep alive to prevent deletion).
fn create_verifier_with_key(pub_pem: &str) -> (StubTokenVerifier, tempfile::NamedTempFile) {
    let mut key_file = tempfile::NamedTempFile::new().expect("create temp file for public key");
    key_file
        .write_all(pub_pem.as_bytes())
        .expect("write public key PEM");
    key_file.flush().expect("flush temp file");

    // SAFETY: Benchmarks are single-threaded during setup; no concurrent env access.
    unsafe {
        // Set env var so StubTokenVerifier::new() reads from this file
        std::env::set_var("NXUS_SIGNING_KEY_PATH", key_file.path());
    }

    let verifier = StubTokenVerifier::new();

    // SAFETY: Same reasoning as above; clearing before benchmark loop starts.
    unsafe {
        // Clear env var to avoid interfering with other benchmarks
        std::env::remove_var("NXUS_SIGNING_KEY_PATH");
    }

    (verifier, key_file)
}

/// T053: Benchmark ES256 token validation via StubTokenVerifier.
/// Target: <=5ms per validation.
fn bench_token_validation(c: &mut Criterion) {
    let (token, pub_pem) = make_developer_token();
    let (verifier, _key_file) = create_verifier_with_key(&pub_pem);

    c.bench_function("token_validation_es256", |b| {
        b.iter(|| {
            let result = verifier.verify(criterion::black_box(&token));
            assert!(result.is_ok(), "token validation should succeed");
        });
    });
}

/// T054: Benchmark catalog feature lookup.
/// Target: <=0.1ms per lookup.
fn bench_catalog_feature_lookup(c: &mut Criterion) {
    c.bench_function("catalog_feature_lookup_pro", |b| {
        b.iter(|| {
            let features = catalog_features(criterion::black_box("pro"));
            assert!(!features.is_empty());
        });
    });
}

/// T055: Benchmark catalog limit lookup with override merge.
/// Target: <=0.1ms per lookup+merge.
fn bench_catalog_limit_lookup(c: &mut Criterion) {
    let overrides = {
        let mut m = HashMap::new();
        m.insert("max_sessions".to_string(), serde_json::json!(512));
        m.insert("seats".to_string(), serde_json::json!("unlimited"));
        Some(m)
    };

    c.bench_function("catalog_limit_lookup_merge", |b| {
        b.iter(|| {
            let base = catalog_limits(criterion::black_box("pro"));
            let merged = merge_limits_override(&base, criterion::black_box(&overrides));
            assert_eq!(merged.max_sessions, Some(512));
        });
    });
}

criterion_group!(
    benches,
    bench_token_validation,
    bench_catalog_feature_lookup,
    bench_catalog_limit_lookup
);
criterion_main!(benches);
