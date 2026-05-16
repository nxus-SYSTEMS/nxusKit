mod license_fixture_support;

use license_fixture_support::{
    LicenseFixtureKind, make_license_fixture, sign_license_fixture_claims,
    sign_license_fixture_claims_with_kid,
};
use nxuskit_core::license::{StubTokenVerifier, validate_token_full_with_verifier_and_environment};
use nxuskit_core::license_types::{TokenType, TokenVerifier};
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn leased_token_validates_as_standard_expiring_machine_bound_token() {
    let mut fixture = make_license_fixture(LicenseFixtureKind::Leased);
    let local_machine = nxuskit_core::machine_id::get_machine_fingerprint()
        .expect("local machine fingerprint should be available");
    fixture.claims["machine_id"] = serde_json::json!(local_machine);
    let (token, public_key_pem) = sign_license_fixture_claims(&fixture.claims);
    let verifier = StubTokenVerifier::with_public_key(&public_key_pem);

    let validated = validate_token_full_with_verifier_and_environment(&token, &verifier, "test")
        .expect("leased fixture should validate in test lane");

    assert_eq!(validated.claims.token_type, TokenType::Leased);
    assert_eq!(validated.claims.edition, "pro");
    assert!(validated.claims.exp.is_some());
    assert_eq!(
        validated.claims.machine_id.as_deref(),
        Some(local_machine.as_str())
    );
}

#[cfg(feature = "licensing-client")]
#[test]
fn client_crate_verifier_falls_back_for_extended_token_kinds_until_client_updates() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let previous_key_path = std::env::var_os("NXUS_SIGNING_KEY_PATH");

    for (kind, expected_type) in [
        (LicenseFixtureKind::Leased, TokenType::Leased),
        (LicenseFixtureKind::RealPurchase, TokenType::RealPurchase),
    ] {
        let fixture = make_license_fixture(kind);
        let (token, public_key_pem) =
            sign_license_fixture_claims_with_kid(&fixture.claims, "es256-v1");
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let key_path = temp_dir.path().join("extended-kind-test-pubkey.pem");
        std::fs::write(&key_path, public_key_pem).expect("write public key");

        unsafe {
            std::env::set_var("NXUS_SIGNING_KEY_PATH", &key_path);
        }

        let verifier = nxuskit_core::license::ClientCrateVerifier::new()
            .expect("client verifier should initialize with env key");
        let claims = verifier
            .verify(&token)
            .expect("extended token kind should fall back to SDK verifier");

        assert_eq!(claims.token_type, expected_type);
    }

    unsafe {
        if let Some(previous_key_path) = previous_key_path {
            std::env::set_var("NXUS_SIGNING_KEY_PATH", previous_key_path);
        } else {
            std::env::remove_var("NXUS_SIGNING_KEY_PATH");
        }
    }
}
