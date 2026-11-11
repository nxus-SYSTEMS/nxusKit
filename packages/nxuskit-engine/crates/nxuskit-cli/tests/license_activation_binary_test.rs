#[path = "../../nxuskit-core/tests/license_fixture_support.rs"]
mod license_fixture_support;

use assert_cmd::Command;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use license_fixture_support::{
    LicenseFixtureKind, make_license_fixture, sign_license_fixture_claims_with_kid,
};
use serde_json::{Value, json};
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[test]
fn license_status_json_exposes_release_diagnostics_against_temp_token_path() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time must be after Unix epoch")
        .as_nanos();
    let home = std::env::temp_dir().join(format!(
        "nxuskit-cli-license-status-{}-{unique}",
        std::process::id()
    ));
    let token_dir = home.join(".nxuskit");
    fs::create_dir_all(&token_dir).expect("create temp token dir");
    fs::write(token_dir.join("license.token"), "not-a-jwt").expect("write temp token");

    let output = Command::cargo_bin("nxuskit-cli")
        .expect("nxuskit-cli binary should be available")
        .args(["license", "status", "--json"])
        .env("HOME", &home)
        .env("NXUSKIT_LICENSE_ENVIRONMENT", "test")
        .output()
        .expect("run license status");

    assert!(
        output.status.success(),
        "status command should emit diagnostics even for invalid token"
    );

    let json: Value = serde_json::from_slice(&output.stdout).expect("status output is JSON");
    assert_eq!(json["licensing"]["environment"], "test");
    assert_eq!(
        json["licensing"]["endpoint"]["default"],
        "https://nxus.systems/licensing-api/v1"
    );
    assert_eq!(
        json["licensing"]["signing_keys"]["kids"],
        serde_json::json!([
            "projects/nxus-licensing-prod/locations/us-central1/keyRings/licensing-signing/cryptoKeys/license-token-signing/cryptoKeyVersions/1"
        ])
    );
    let signing_key_source = json["licensing"]["signing_keys"]["source"]
        .as_str()
        .expect("signing key source should be a string");
    assert!(
        matches!(
            signing_key_source,
            "embedded-production" | "dev-test-fallback" | "embedded-key"
        ),
        "unexpected signing key source label: {signing_key_source}"
    );
    assert!(
        !signing_key_source.contains('/') && !signing_key_source.contains('\\'),
        "signing key source label must not expose a local filesystem path"
    );

    assert!(json["license"]["edition"].is_string());
    assert!(json["license"]["features"].is_array());
    assert!(json["license"]["effective_limits"].is_object());

    fs::remove_dir_all(home).ok();
}

#[test]
fn license_status_human_output_redacts_signing_key_path() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time must be after Unix epoch")
        .as_nanos();
    let home = std::env::temp_dir().join(format!(
        "nxuskit-cli-license-status-human-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&home).expect("create temp home");

    let output = Command::cargo_bin("nxuskit-cli")
        .expect("nxuskit-cli binary should be available")
        .args(["license", "status"])
        .env("HOME", &home)
        .output()
        .expect("run license status");

    assert!(output.status.success(), "status command should succeed");

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    assert!(stdout.contains(
        "Signing keys: projects/nxus-licensing-prod/locations/us-central1/keyRings/licensing-signing/cryptoKeys/license-token-signing/cryptoKeyVersions/1 ("
    ));
    assert!(
        !stdout.contains("DevOps/sharedData/keys")
            && !stdout.contains("es256-production-pubkey.pem")
            && !stdout.contains("/Users/"),
        "human status output must not expose local signing key paths:\n{stdout}"
    );

    fs::remove_dir_all(home).ok();
}

fn ga_s01b_home(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time must be after Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "nxuskit-ga-s01b-{label}-{}-{unique}",
        std::process::id()
    ))
}

fn ga_s01b_developer_token(home: &Path) -> (String, PathBuf) {
    let mut claims = make_license_fixture(LicenseFixtureKind::Developer).claims;
    claims["machine_id"] = json!(
        nxuskit_core::machine_id::get_machine_fingerprint()
            .expect("local machine fingerprint must be available for fixture binding")
    );
    ga_s01b_sign_claims(home, &claims)
}

fn ga_s01b_sign_claims(home: &Path, claims: &Value) -> (String, PathBuf) {
    let (token, public_key_pem) = sign_license_fixture_claims_with_kid(&claims, "es256-v1");
    let key_path = home.join("ga-s01b-test-verifier.pem");
    fs::write(&key_path, public_key_pem).expect("write deterministic test verifier");
    (token, key_path)
}

fn ga_s01b_store_token(home: &Path, token: &str) {
    let token_dir = home.join(".nxuskit");
    fs::create_dir_all(&token_dir).expect("create deterministic token directory");
    fs::write(token_dir.join("license.token"), token).expect("write deterministic token");
}

fn ga_s01b_read_request(stream: &mut std::net::TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set loopback read timeout");
    let mut bytes = Vec::new();
    let mut buf = [0_u8; 4096];
    let mut expected_total = None;

    loop {
        let n = stream.read(&mut buf).expect("read loopback request");
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);

        if expected_total.is_none() {
            if let Some(header_end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let header_len = header_end + 4;
                let headers = String::from_utf8_lossy(&bytes[..header_end]);
                let content_len = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())
                            .flatten()
                    })
                    .unwrap_or(0);
                expected_total = Some(header_len + content_len);
            }
        }

        if expected_total.is_some_and(|total| bytes.len() >= total) {
            break;
        }
    }

    String::from_utf8(bytes).expect("loopback request is utf8")
}

fn ga_s01b_activation_server(token: String, status: u16) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback activation fixture");
    let address = listener.local_addr().expect("read loopback address");
    listener
        .set_nonblocking(true)
        .expect("set loopback activation listener nonblocking");
    let handle = thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    stream
                        .set_nonblocking(false)
                        .expect("set accepted activation stream blocking");
                    break stream;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if std::time::Instant::now() >= deadline {
                        panic!("activation fixture timed out waiting for loopback connection");
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("accept activation request: {error}"),
            }
        };
        let request = ga_s01b_read_request(&mut stream);
        assert!(
            request.starts_with("POST /activate HTTP/1.1"),
            "unexpected activation request line: {request}"
        );
        assert!(
            request
                .to_ascii_lowercase()
                .contains("\r\nidempotency-key:"),
            "activation request must carry Idempotency-Key"
        );
        assert!(
            request.to_ascii_lowercase().contains("\r\nx-machine-id:"),
            "activation request must carry X-Machine-Id"
        );
        assert!(
            request
                .to_ascii_lowercase()
                .contains("\r\nauthorization: bearer ga-s01b-device-auth-fixture\r\n"),
            "activation request must carry deterministic Bearer auth fixture"
        );
        assert!(
            request.contains("\"purchase_id\":\"purchase-ga-s01b\""),
            "activation request must contain fixture purchase id: {request}"
        );

        let body = if status == 200 {
            json!({
                "token": token,
                "seats_used": 1,
                "seats_total": 3,
                "message": "fixture activated"
            })
            .to_string()
        } else {
            json!({
                "error": "service_unavailable",
                "message": "fixture unavailable"
            })
            .to_string()
        };
        let reason = if status == 200 {
            "OK"
        } else {
            "Service Unavailable"
        };
        let response = format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream
            .write_all(response.as_bytes())
            .expect("write activation fixture response");
    });
    (format!("http://{address}"), handle)
}

fn ga_s01b_common_env(command: &mut Command, home: &Path, key_path: &Path, server: Option<&str>) {
    let auth_path = home.join(".config/nxuskit/auth.json");
    fs::create_dir_all(auth_path.parent().expect("auth path parent"))
        .expect("create deterministic auth fixture directory");
    fs::write(
        &auth_path,
        serde_json::to_vec_pretty(&json!({
            "access_token": "ga-s01b-device-auth-fixture",
            "token_type": "Bearer",
            "expires_at": "",
            "instance_url": "http://127.0.0.1/ga-s01b-fixture",
            "user_email": "ga-s01b@example.invalid"
        }))
        .expect("serialize deterministic auth fixture"),
    )
    .expect("write deterministic auth fixture");

    command
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("NXUSKIT_AUTH_TOKEN_PATH", &auth_path)
        .env("NXUS_SIGNING_KEY_PATH", key_path)
        .env("NXUSKIT_LICENSE_ENVIRONMENT", "test")
        .env_remove("NXUSKIT_LICENSE_TOKEN")
        .env_remove("NXUSKIT_LICENSE_KEY");
    if let Some(server) = server {
        command.env("NXUSKIT_LICENSE_SERVER", server);
    } else {
        command.env_remove("NXUSKIT_LICENSE_SERVER");
    }
}

fn ga_s01b_assert_no_token_leak(output: &std::process::Output, token: &str) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stdout.contains(token), "stdout leaked raw token");
    assert!(!stderr.contains(token), "stderr leaked raw token");
}

fn ga_s01b_assert_invalid_token_denies_solver(
    label: &str,
    token: String,
    key_path: &Path,
    expected_status_error: &str,
) {
    let home = ga_s01b_home(label);
    fs::create_dir_all(&home).expect("create denial fixture home");
    ga_s01b_store_token(&home, &token);
    let copied_key_path = home.join("ga-s01b-test-verifier.pem");
    fs::copy(key_path, &copied_key_path).expect("copy deterministic denial verifier");

    let mut status = Command::cargo_bin("nxuskit-cli").expect("compiled CLI is available");
    status.args(["license", "status", "--json"]);
    ga_s01b_common_env(&mut status, &home, &copied_key_path, None);
    let status_output = status.output().expect("run denial status");
    assert!(
        status_output.status.success(),
        "{label} status must emit JSON diagnostics"
    );
    ga_s01b_assert_no_token_leak(&status_output, &token);
    let status_json: Value =
        serde_json::from_slice(&status_output.stdout).expect("denial status output is JSON");
    assert_eq!(status_json["license"]["status"], "invalid", "{label}");
    let status_error = status_json["license"]["error"]
        .as_str()
        .expect("invalid license status must include a diagnostic");
    assert!(
        status_error.contains(expected_status_error),
        "{label} status diagnostic must identify its denial reason: {status_error}"
    );

    let solver_fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/solver_sat.json");
    let mut solver = Command::cargo_bin("nxuskit-cli").expect("compiled CLI is available");
    solver.args([
        "solver",
        "solve",
        "--input",
        solver_fixture.to_str().expect("fixture path is utf8"),
        "--format",
        "json",
    ]);
    ga_s01b_common_env(&mut solver, &home, &copied_key_path, None);
    let solver_output = solver.output().expect("run denied solver");
    assert_eq!(solver_output.status.code(), Some(4), "{label}");
    ga_s01b_assert_no_token_leak(&solver_output, &token);
    let solver_error: Value =
        serde_json::from_slice(&solver_output.stderr).expect("solver denial output is JSON");
    assert_eq!(solver_error["code"], "entitlement_denied", "{label}");

    fs::remove_dir_all(home).ok();
}

#[test]
fn ga_s01b_compiled_activation_status_and_solver() {
    let home = ga_s01b_home("positive");
    fs::create_dir_all(&home).expect("create GA-S01B temp home");
    let (token, key_path) = ga_s01b_developer_token(&home);
    let (server, handle) = ga_s01b_activation_server(token.clone(), 200);

    let mut activate = Command::cargo_bin("nxuskit-cli").expect("compiled CLI is available");
    activate.args([
        "license",
        "activate",
        "--key",
        "purchase-ga-s01b",
        "--accept-eula",
        "--json",
    ]);
    ga_s01b_common_env(&mut activate, &home, &key_path, Some(&server));
    let activation = activate.output().expect("run compiled activation");
    if let Err(error) = handle.join() {
        panic!(
            "activation fixture thread failed after child exit status={:?}: stdout={} stderr={} thread={:?}",
            activation.status.code(),
            String::from_utf8_lossy(&activation.stdout),
            String::from_utf8_lossy(&activation.stderr),
            error
        );
    }
    assert!(
        activation.status.success(),
        "activation failed: stdout={} stderr={}",
        String::from_utf8_lossy(&activation.stdout),
        String::from_utf8_lossy(&activation.stderr)
    );
    ga_s01b_assert_no_token_leak(&activation, &token);
    let activation_json: Value =
        serde_json::from_slice(&activation.stdout).expect("activation output is JSON");
    assert_eq!(activation_json["success"], true);
    assert_eq!(activation_json["seats_used"], 1);
    assert_eq!(activation_json["seats_total"], 3);

    let token_file = home.join(".nxuskit/license.token");
    assert_eq!(
        fs::read_to_string(&token_file)
            .expect("activation stored token")
            .trim(),
        token
    );

    let mut status = Command::cargo_bin("nxuskit-cli").expect("compiled CLI is available");
    status.args(["license", "status", "--json"]);
    ga_s01b_common_env(&mut status, &home, &key_path, None);
    let status_output = status.output().expect("run compiled license status");
    assert!(
        status_output.status.success(),
        "status failed: stdout={} stderr={}",
        String::from_utf8_lossy(&status_output.stdout),
        String::from_utf8_lossy(&status_output.stderr)
    );
    ga_s01b_assert_no_token_leak(&status_output, &token);
    let status_json: Value =
        serde_json::from_slice(&status_output.stdout).expect("status output is JSON");
    assert_eq!(status_json["license"]["status"], "valid");
    assert_eq!(status_json["license"]["effective_edition"], "pro");
    assert_eq!(status_json["license"]["token"]["type"], "developer");

    let solver_fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/solver_sat.json");
    let mut solver = Command::cargo_bin("nxuskit-cli").expect("compiled CLI is available");
    solver.args([
        "solver",
        "solve",
        "--input",
        solver_fixture.to_str().expect("fixture path is utf8"),
        "--format",
        "json",
    ]);
    ga_s01b_common_env(&mut solver, &home, &key_path, None);
    let solver_output = solver.output().expect("run real Pro solver operation");
    assert!(
        solver_output.status.success(),
        "solver failed: stdout={} stderr={}",
        String::from_utf8_lossy(&solver_output.stdout),
        String::from_utf8_lossy(&solver_output.stderr)
    );
    ga_s01b_assert_no_token_leak(&solver_output, &token);
    let solver_json: Value =
        serde_json::from_slice(&solver_output.stdout).expect("solver output is JSON");
    assert_eq!(solver_json["result"]["satisfiable"], true);
    assert!(
        solver_json["result"]["assignments"]
            .as_object()
            .is_some_and(|assignments| !assignments.is_empty()),
        "real Pro solver operation must return assignments"
    );

    fs::remove_dir_all(home).ok();
}

#[test]
fn ga_s01b_unavailable_authority_cannot_claim_activation_success() {
    let home = ga_s01b_home("unavailable");
    fs::create_dir_all(&home).expect("create GA-S01B temp home");
    let (token, key_path) = ga_s01b_developer_token(&home);
    let (server, handle) = ga_s01b_activation_server(token.clone(), 503);

    let mut activate = Command::cargo_bin("nxuskit-cli").expect("compiled CLI is available");
    activate.args([
        "license",
        "activate",
        "--key",
        "purchase-ga-s01b",
        "--accept-eula",
        "--json",
    ]);
    ga_s01b_common_env(&mut activate, &home, &key_path, Some(&server));
    let output = activate
        .output()
        .expect("run unavailable-authority activation");
    if let Err(error) = handle.join() {
        panic!(
            "unavailable-authority fixture thread failed after child exit status={:?}: stdout={} stderr={} thread={:?}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
            error
        );
    }
    assert!(
        !output.status.success(),
        "unavailable authority must not report activation success"
    );
    ga_s01b_assert_no_token_leak(&output, &token);
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("activation failure output is JSON");
    assert_eq!(json["success"], false);
    assert!(
        !home.join(".nxuskit/license.token").exists(),
        "failed activation must not create a license token"
    );

    fs::remove_dir_all(home).ok();
}

fn ga_s01b_corrupt_signing_input(token: &str) -> String {
    let mut parts = token.split('.');
    let header = parts.next().expect("fixture header segment");
    let payload = parts.next().expect("fixture payload segment");
    let signature = parts.next().expect("fixture signature segment");
    assert!(
        parts.next().is_none(),
        "fixture must have three JWT segments"
    );
    let mut payload = URL_SAFE_NO_PAD
        .decode(payload)
        .expect("fixture payload must be canonical Base64URL");
    let claims: Value = serde_json::from_slice(&payload).expect("fixture claims JSON");
    payload.push(b' ');
    assert_eq!(
        serde_json::from_slice::<Value>(&payload).expect("mutated fixture claims JSON"),
        claims,
        "signing-input mutation must preserve decoded claims"
    );
    format!("{header}.{}.{signature}", URL_SAFE_NO_PAD.encode(payload))
}

#[test]
fn ga_s01b_invalid_developer_claims_never_authorize_solver_or_leak_tokens() {
    // Public synthetic ES256-length bytes cover all canonical final symbols.
    // The oracle changes only JSON whitespace in the signed payload.
    for (last_byte, ending) in [(0, 'A'), (1, 'Q'), (2, 'g'), (3, 'w')] {
        let mut original = [0_u8; 64];
        original[63] = last_byte;
        let signature = URL_SAFE_NO_PAD.encode(original);
        assert_eq!(signature.chars().last(), Some(ending));
        let token = format!("e30.e30.{signature}");
        let corrupted = ga_s01b_corrupt_signing_input(&token);
        let (prefix, changed_signature) = corrupted.rsplit_once('.').expect("signature segment");
        assert_eq!(
            prefix, "e30.e30g",
            "only one payload space must be appended"
        );
        assert_ne!(prefix, "e30.e30", "JWT signing input must change");
        assert_eq!(
            changed_signature, signature,
            "signature text must be unchanged"
        );
        let (_, changed_payload) = prefix.split_once('.').expect("header and payload");
        let changed_claims = URL_SAFE_NO_PAD.decode(changed_payload).unwrap();
        assert_eq!(changed_claims, b"{} ");
        assert_eq!(
            serde_json::from_slice::<Value>(&changed_claims).unwrap(),
            json!({})
        );
        let decoded = URL_SAFE_NO_PAD
            .decode(changed_signature)
            .expect("unchanged signature must remain canonical Base64URL");
        assert_eq!(
            decoded, original,
            "decoded signature bytes must be unchanged"
        );
        assert_eq!(
            decoded.len(),
            64,
            "ES256 signature length must be unchanged"
        );
        assert_eq!(URL_SAFE_NO_PAD.encode(decoded), signature);
    }
    let signing_home = ga_s01b_home("denial-signing");
    fs::create_dir_all(&signing_home).expect("create deterministic signing home");

    let current_machine = nxuskit_core::machine_id::get_machine_fingerprint()
        .expect("local machine fingerprint must be available for fixture binding");

    let mut wrong_subject = make_license_fixture(LicenseFixtureKind::Developer).claims;
    wrong_subject["machine_id"] =
        json!("sha256:0000000000000000000000000000000000000000000000000000000000000000");
    let (wrong_subject_token, key_path) = ga_s01b_sign_claims(&signing_home, &wrong_subject);
    ga_s01b_assert_invalid_token_denies_solver(
        "wrong-subject",
        wrong_subject_token,
        &key_path,
        "Token bound to machine",
    );

    let mut expired = make_license_fixture(LicenseFixtureKind::Developer).claims;
    expired["machine_id"] = json!(current_machine.clone());
    expired["exp"] = json!(0);
    let (expired_token, key_path) = ga_s01b_sign_claims(&signing_home, &expired);
    ga_s01b_assert_invalid_token_denies_solver(
        "expired",
        expired_token,
        &key_path,
        "developer token has expired",
    );

    let mut suspended = make_license_fixture(LicenseFixtureKind::Trial).claims;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time must be after Unix epoch")
        .as_secs() as i64;
    suspended["machine_id"] = json!(current_machine);
    suspended["iat"] = json!(now - 8 * 86_400);
    suspended["nbf"] = json!(now - 8 * 86_400);
    suspended["exp"] = json!(now + 30 * 86_400);
    suspended["activated"] = json!(false);
    let (suspended_token, key_path) = ga_s01b_sign_claims(&signing_home, &suspended);
    ga_s01b_assert_invalid_token_denies_solver(
        "suspended-trial",
        suspended_token,
        &key_path,
        "Trial suspended",
    );

    let mut valid = make_license_fixture(LicenseFixtureKind::Developer).claims;
    valid["machine_id"] = json!(
        nxuskit_core::machine_id::get_machine_fingerprint()
            .expect("local machine fingerprint must remain available for fixture binding")
    );
    let (valid_signature_token, key_path) = ga_s01b_sign_claims(&signing_home, &valid);
    let invalid_signature_token = ga_s01b_corrupt_signing_input(&valid_signature_token);
    ga_s01b_assert_invalid_token_denies_solver(
        "invalid-signature",
        invalid_signature_token,
        &key_path,
        "InvalidSignature",
    );

    fs::remove_dir_all(signing_home).ok();
}
