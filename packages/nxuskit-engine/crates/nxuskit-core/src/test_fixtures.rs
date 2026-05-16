//! ES256-signed JWT test fixture generation (cfg(test) only).
//!
//! Provides deterministic key pairs and token generators for license
//! validation tests. Uses the `p256` crate for EC P-256 key generation
//! and `jsonwebtoken` for ES256 JWT signing.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use p256::ecdsa::SigningKey;
use p256::pkcs8::EncodePrivateKey;

use crate::license_types::TokenClaims;
use crate::license_types::TokenType;

/// Deterministic seed for reproducible test keys.
const TEST_KEY_SEED: [u8; 32] = [
    0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10,
    0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20,
];

/// Generate a deterministic ES256 key pair for testing.
///
/// Returns (private_key_pem, public_key_pem) strings.
/// The same seed always produces the same key pair.
pub fn test_es256_keypair() -> (String, String) {
    let signing_key = SigningKey::from_bytes((&TEST_KEY_SEED).into()).expect("valid 32-byte seed");
    let verifying_key = signing_key.verifying_key();

    let private_pem = signing_key
        .to_pkcs8_pem(p256::pkcs8::LineEnding::LF)
        .expect("PKCS8 encoding")
        .to_string();

    // Build PEM manually from the SEC1 encoded point
    let point = verifying_key.to_encoded_point(false);
    let point_bytes = point.as_bytes();

    // DER encoding for EC public key: SEQUENCE { SEQUENCE { OID, OID }, BIT STRING }
    // OID for id-ecPublicKey: 1.2.840.10045.2.1
    // OID for secp256r1: 1.2.840.10045.3.1.7
    let algo_oid: &[u8] = &[
        0x30, 0x13, // SEQUENCE (19 bytes)
        0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, // OID: id-ecPublicKey
        0x06, 0x08, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07, // OID: secp256r1
    ];

    // BIT STRING wrapper: 0x03, length, 0x00 (no unused bits), then the point
    let bit_string_content_len = 1 + point_bytes.len(); // 0x00 prefix + point
    let mut bit_string = vec![0x03];
    if bit_string_content_len < 128 {
        bit_string.push(bit_string_content_len as u8);
    } else {
        bit_string.push(0x81);
        bit_string.push(bit_string_content_len as u8);
    }
    bit_string.push(0x00); // no unused bits
    bit_string.extend_from_slice(point_bytes);

    // Outer SEQUENCE
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

    let b64 = base64_encode_pem(&der);
    let public_pem = format!(
        "-----BEGIN PUBLIC KEY-----\n{}\n-----END PUBLIC KEY-----\n",
        b64
    );

    (private_pem, public_pem)
}

fn base64_encode_pem(data: &[u8]) -> String {
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(data);
    // Split into 64-char lines
    encoded
        .as_bytes()
        .chunks(64)
        .map(|c| std::str::from_utf8(c).unwrap())
        .collect::<Vec<&str>>()
        .join("\n")
}

/// Sign a JWT with ES256 using the test private key.
pub fn sign_test_jwt(claims: &TokenClaims, private_pem: &str) -> String {
    let encoding_key = EncodingKey::from_ec_pem(private_pem.as_bytes()).expect("valid EC PEM key");
    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some("es256-v1".to_string());
    encode(&header, claims, &encoding_key).expect("JWT encoding")
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// Generate an ES256-signed trial token.
pub fn make_trial_token() -> (String, String) {
    let (priv_pem, pub_pem) = test_es256_keypair();
    let now = now_unix();
    let claims = TokenClaims {
        product_id: "nxuskit".to_string(),
        iss: "nxus-licensing".to_string(),
        iat: now,
        nbf: Some(now),
        exp: Some(now + 30 * 86400),
        token_type: TokenType::Trial,
        edition: "pro".to_string(),
        tenant_id: None,
        machine_id: Some("sha256:test-machine-001".to_string()),
        seat_index: None,
        activated: Some(false),
        sdk_version_ceiling: None,
        customer_email: None,
    };
    (sign_test_jwt(&claims, &priv_pem), pub_pem)
}

/// Generate an ES256-signed developer token.
pub fn make_developer_token() -> (String, String) {
    let (priv_pem, pub_pem) = test_es256_keypair();
    let now = now_unix();
    let claims = TokenClaims {
        product_id: "nxuskit".to_string(),
        iss: "nxus-licensing".to_string(),
        iat: now,
        nbf: Some(now),
        exp: Some(now + 365 * 86400),
        token_type: TokenType::Developer,
        edition: "pro".to_string(),
        tenant_id: Some("org-test-001".to_string()),
        machine_id: Some("sha256:test-machine-001".to_string()),
        seat_index: Some(1),
        activated: None,
        sdk_version_ceiling: None,
        customer_email: None,
    };
    (sign_test_jwt(&claims, &priv_pem), pub_pem)
}

/// Generate an ES256-signed deployment token (no expiry).
pub fn make_deployment_token() -> (String, String) {
    let (priv_pem, pub_pem) = test_es256_keypair();
    let now = now_unix();
    let claims = TokenClaims {
        product_id: "nxuskit".to_string(),
        iss: "nxus-licensing".to_string(),
        iat: now,
        nbf: None,
        exp: None,
        token_type: TokenType::Deployment,
        edition: "pro".to_string(),
        tenant_id: Some("org-test-001".to_string()),
        machine_id: None,
        seat_index: None,
        activated: None,
        sdk_version_ceiling: Some("0.9".to_string()),
        customer_email: Some("test@example.com".to_string()),
    };
    (sign_test_jwt(&claims, &priv_pem), pub_pem)
}

/// Generate an ES256-signed community-edition token.
pub fn make_community_token() -> (String, String) {
    let (priv_pem, pub_pem) = test_es256_keypair();
    let now = now_unix();
    let claims = TokenClaims {
        product_id: "nxuskit".to_string(),
        iss: "nxus-licensing".to_string(),
        iat: now,
        nbf: Some(now),
        exp: Some(now + 365 * 86400),
        token_type: TokenType::Developer,
        edition: "community".to_string(),
        tenant_id: Some("org-oss-001".to_string()),
        machine_id: Some("sha256:test-machine-001".to_string()),
        seat_index: Some(1),
        activated: None,
        sdk_version_ceiling: None,
        customer_email: None,
    };
    (sign_test_jwt(&claims, &priv_pem), pub_pem)
}

/// Generate an ES256-signed enterprise-edition token.
pub fn make_enterprise_token() -> (String, String) {
    let (priv_pem, pub_pem) = test_es256_keypair();
    let now = now_unix();
    let claims = TokenClaims {
        product_id: "nxuskit".to_string(),
        iss: "nxus-licensing".to_string(),
        iat: now,
        nbf: Some(now),
        exp: Some(now + 365 * 86400),
        token_type: TokenType::Developer,
        edition: "enterprise".to_string(),
        tenant_id: Some("org-ent-001".to_string()),
        machine_id: Some("sha256:test-machine-001".to_string()),
        seat_index: Some(1),
        activated: None,
        sdk_version_ceiling: None,
        customer_email: None,
    };
    (sign_test_jwt(&claims, &priv_pem), pub_pem)
}

/// Generate a token with custom limits_override.
#[allow(dead_code)]
pub fn make_token_with_limits_override(
    overrides: HashMap<String, serde_json::Value>,
) -> (String, String) {
    let (priv_pem, pub_pem) = test_es256_keypair();
    let now = now_unix();

    // We need to manually construct the JSON since TokenClaims doesn't have limits_override
    let claims_json = serde_json::json!({
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
        "limits_override": overrides,
    });

    let encoding_key = EncodingKey::from_ec_pem(priv_pem.as_bytes()).expect("valid EC PEM key");
    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some("es256-v1".to_string());
    let token = encode(&header, &claims_json, &encoding_key).expect("JWT encoding");
    (token, pub_pem)
}

/// Generate a token with custom features_override.
#[allow(dead_code)]
pub fn make_token_with_features_override(features: Vec<String>) -> (String, String) {
    let (priv_pem, pub_pem) = test_es256_keypair();
    let now = now_unix();

    let claims_json = serde_json::json!({
        "iss": "nxus-licensing",
        "iat": now,
        "nbf": now,
        "exp": now + 365 * 86400,
        "type": "developer",
        "edition": "community",
        "product_id": "nxuskit",
        "tenant_id": "org-test-001",
        "machine_id": "sha256:test-machine-001",
        "seat_index": 1,
        "features_override": features,
    });

    let encoding_key = EncodingKey::from_ec_pem(priv_pem.as_bytes()).expect("valid EC PEM key");
    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some("es256-v1".to_string());
    let token = encode(&header, &claims_json, &encoding_key).expect("JWT encoding");
    (token, pub_pem)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keypair_deterministic() {
        let (priv1, pub1) = test_es256_keypair();
        let (priv2, pub2) = test_es256_keypair();
        assert_eq!(priv1, priv2);
        assert_eq!(pub1, pub2);
    }

    #[test]
    fn test_sign_and_verify_roundtrip() {
        let (token, pub_pem) = make_trial_token();
        assert!(!token.is_empty());
        assert!(pub_pem.contains("BEGIN PUBLIC KEY"));

        // Verify the token can be decoded with the public key
        let decoding_key =
            jsonwebtoken::DecodingKey::from_ec_pem(pub_pem.as_bytes()).expect("valid pub key");
        let mut validation = jsonwebtoken::Validation::new(Algorithm::ES256);
        validation.set_issuer(&["nxus-licensing"]);
        validation.validate_exp = false;
        validation.set_required_spec_claims(&["iss", "iat"]);

        let token_data = jsonwebtoken::decode::<TokenClaims>(&token, &decoding_key, &validation)
            .expect("token should verify");
        assert_eq!(token_data.claims.iss, "nxus-licensing");
        assert_eq!(token_data.claims.token_type, TokenType::Trial);
        assert_eq!(token_data.claims.edition, "pro");
    }

    #[test]
    fn test_all_token_types_generate() {
        let (t, _) = make_trial_token();
        assert!(!t.is_empty());
        let (t, _) = make_developer_token();
        assert!(!t.is_empty());
        let (t, _) = make_deployment_token();
        assert!(!t.is_empty());
        let (t, _) = make_community_token();
        assert!(!t.is_empty());
        let (t, _) = make_enterprise_token();
        assert!(!t.is_empty());
    }
}
