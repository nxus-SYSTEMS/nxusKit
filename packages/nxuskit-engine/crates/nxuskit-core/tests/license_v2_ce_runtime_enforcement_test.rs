mod license_fixture_support;

use license_fixture_support::sign_license_fixture_claims_with_kid;
use nxuskit_core::license::resolve_token;
use std::sync::Mutex;

static ENV_MUTEX: Mutex<()> = Mutex::new(());

#[test]
fn public_native_runtime_retains_signed_v2_policy_facts_without_echoing_the_token() {
    let _guard = ENV_MUTEX
        .lock()
        .expect("test environment lock must be available");
    let claims = serde_json::json!({
        "iss": "nxus-licensing",
        "aud": "nxuskit-deployment-runtime",
        "sub": "account_7c1d9a",
        "jti": "8b3a28ec-d856-4e9e-884d-f9d1268db7e2",
        "iat": 1_721_779_200_i64,
        "type": "deployment",
        "edition": "pro",
        "product_id": "nxuskit",
        "grant_id": "grant_2f8b30",
        "grant_revision": 3,
        "purpose": "offline_deployment",
        "target_id": "target_7e1a5b",
        "policy_hash": "sha256:8d9a1f23c4567890abcdef1234567890abcdef1234567890abcdef1234567890",
        "sdk_release_line": 2,
        "authorization_scope": "client_entitlement",
        "capability_grants": ["cap.solver"],
        "capability_denials": ["cap.experimental"],
        "limits_override": {"solver.max_nodes": 1000}
    });
    let (token, public_key_pem) = sign_license_fixture_claims_with_kid(&claims, "es256-v1");
    let key_dir =
        tempfile::tempdir().expect("temporary public test key directory must be available");
    let key_path = key_dir.path().join("public-ce-runtime-key.pem");
    std::fs::write(&key_path, public_key_pem)
        .expect("deterministic public test key must be writable");
    // SAFETY: this isolated test process sets the debug-only test key before
    // the process-global verifier is initialized.
    unsafe {
        std::env::set_var("NXUS_SIGNING_KEY_PATH", &key_path);
        // resolve_token is explicitly environment-first. Bind the signed test
        // token to that documented source so unrelated caller state cannot
        // replace the fixture before the runtime verifier sees it.
        std::env::set_var("NXUSKIT_LICENSE_TOKEN", &token);
    }

    let resolution = resolve_token(Some(&token));
    assert!(
        resolution.valid,
        "public native runtime must resolve the signed fixture: {:?}",
        resolution.error
    );
    let policy = resolution
        .deployment_v2
        .as_ref()
        .expect("validated deployment claims retain policy facts for runtime boundaries");

    assert_eq!(policy.capability_grants, ["cap.solver"]);
    assert_eq!(policy.capability_denials, ["cap.experimental"]);
    assert_eq!(
        policy.limits_override["solver.max_nodes"],
        serde_json::json!(1000)
    );
    assert!(!format!("{policy:?}").contains(&token));
}
