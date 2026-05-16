//! Integration tests for the Bayesian Network safe Rust wrapper.
//!
//! Tests marked `#[ignore]` require `libnxuskit` at runtime.
//! Run them with: `NXUSKIT_LIB_DIR=/path/to/lib cargo test --test bn_test -- --ignored`
//!
//! The BN wrapper provides RAII types (BnNetwork, BnEvidence, BnResult)
//! that call through to the C ABI via dual-dispatch (static-link or dynamic-link).

use nxuskit::bn::{BnEvidence, BnNetwork};

// ── Network Lifecycle Tests (require libnxuskit) ──────────────────────

#[test]
#[ignore = "requires libnxuskit at runtime"]
fn bn_network_create_empty() {
    let net = BnNetwork::create().expect("create should succeed");
    assert_eq!(net.num_variables(), 0);
    // Drop calls nxuskit_bn_net_destroy automatically
}

#[test]
#[ignore = "requires libnxuskit at runtime"]
fn bn_network_load_file() {
    let path = fixture_path("asia.bif");
    let net = BnNetwork::load_file(&path).expect("load_file should succeed");
    assert_eq!(net.num_variables(), 8);
}

#[test]
#[ignore = "requires libnxuskit at runtime"]
fn bn_network_load_file_nonexistent() {
    let result = BnNetwork::load_file("nonexistent-network.bif");
    assert!(result.is_err(), "loading nonexistent file should fail");
}

#[test]
#[ignore = "requires libnxuskit at runtime"]
fn bn_network_variables() {
    let path = fixture_path("asia.bif");
    let net = BnNetwork::load_file(&path).unwrap();
    let vars = net.variables().expect("variables should succeed");
    assert_eq!(vars.len(), 8);
    assert!(vars.contains(&"Smoking".to_string()));
    assert!(vars.contains(&"Bronchitis".to_string()));
}

#[test]
#[ignore = "requires libnxuskit at runtime"]
fn bn_network_variable_states() {
    let path = fixture_path("asia.bif");
    let net = BnNetwork::load_file(&path).unwrap();
    let states = net
        .variable_states("Smoking")
        .expect("variable_states should succeed");
    assert_eq!(states.len(), 2);
    assert!(states.contains(&"yes".to_string()));
    assert!(states.contains(&"no".to_string()));
}

// ── Evidence Tests (require libnxuskit) ──────────────────────────────

#[test]
#[ignore = "requires libnxuskit at runtime"]
fn bn_evidence_create() {
    let _ev = BnEvidence::create().expect("create should succeed");
    // Drop calls nxuskit_bn_ev_destroy automatically
}

#[test]
#[ignore = "requires libnxuskit at runtime"]
fn bn_evidence_set_discrete() {
    let path = fixture_path("asia.bif");
    let net = BnNetwork::load_file(&path).unwrap();
    let mut ev = BnEvidence::create().unwrap();
    ev.set_discrete(&net, "Smoking", "yes")
        .expect("set_discrete should succeed");
}

#[test]
#[ignore = "requires libnxuskit at runtime"]
fn bn_evidence_retract() {
    let path = fixture_path("asia.bif");
    let net = BnNetwork::load_file(&path).unwrap();
    let mut ev = BnEvidence::create().unwrap();
    ev.set_discrete(&net, "Smoking", "yes").unwrap();
    ev.retract("Smoking").expect("retract should succeed");
}

#[test]
#[ignore = "requires libnxuskit at runtime"]
fn bn_evidence_clear() {
    let path = fixture_path("asia.bif");
    let net = BnNetwork::load_file(&path).unwrap();
    let mut ev = BnEvidence::create().unwrap();
    ev.set_discrete(&net, "Smoking", "yes").unwrap();
    ev.clear().expect("clear should succeed");
}

// ── Inference Tests (require libnxuskit) ──────────────────────────────

#[test]
#[ignore = "requires libnxuskit at runtime"]
fn bn_infer_ve_no_evidence() {
    let path = fixture_path("asia.bif");
    let net = BnNetwork::load_file(&path).unwrap();
    let ev = BnEvidence::create().unwrap();

    let result = net.infer(&ev, "ve").expect("VE infer should succeed");
    assert_eq!(result.num_variables(), 8);
}

#[test]
#[ignore = "requires libnxuskit at runtime"]
fn bn_infer_ve_with_evidence() {
    let path = fixture_path("asia.bif");
    let net = BnNetwork::load_file(&path).unwrap();
    let mut ev = BnEvidence::create().unwrap();
    ev.set_discrete(&net, "Smoking", "yes").unwrap();

    let result = net
        .infer(&ev, "ve")
        .expect("VE infer with evidence should succeed");
    let bronchitis = result.query("Bronchitis").expect("query should succeed");

    let p_present = bronchitis.get("present").unwrap();
    assert!(
        *p_present > 0.5,
        "P(Bronchitis=present|Smoking=yes) should be > 0.5"
    );
}

#[test]
#[ignore = "requires libnxuskit at runtime"]
fn bn_infer_jt() {
    let path = fixture_path("asia.bif");
    let net = BnNetwork::load_file(&path).unwrap();
    let ev = BnEvidence::create().unwrap();

    let result = net.infer(&ev, "jt").expect("JT infer should succeed");
    assert_eq!(result.num_variables(), 8);
}

#[test]
#[ignore = "requires libnxuskit at runtime"]
fn bn_infer_gibbs() {
    let path = fixture_path("asia.bif");
    let net = BnNetwork::load_file(&path).unwrap();
    let ev = BnEvidence::create().unwrap();

    let result = net
        .infer_with_options(&ev, "gibbs", 5000, 500, 42)
        .expect("Gibbs infer should succeed");
    assert_eq!(result.num_variables(), 8);
}

// ── Result Access Tests (require libnxuskit) ──────────────────────────

#[test]
#[ignore = "requires libnxuskit at runtime"]
fn bn_result_to_json() {
    let path = fixture_path("asia.bif");
    let net = BnNetwork::load_file(&path).unwrap();
    let ev = BnEvidence::create().unwrap();
    let result = net.infer(&ev, "ve").unwrap();

    let json = result.to_json().expect("to_json should succeed");
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed.get("marginals").is_some());
    assert_eq!(parsed["algorithm"], "ve");
}

#[test]
#[ignore = "requires libnxuskit at runtime"]
fn bn_result_query() {
    let path = fixture_path("asia.bif");
    let net = BnNetwork::load_file(&path).unwrap();
    let ev = BnEvidence::create().unwrap();
    let result = net.infer(&ev, "ve").unwrap();

    let dist = result.query("Smoking").expect("query should succeed");
    assert_eq!(dist.len(), 2);
    assert!((dist["yes"] - 0.5).abs() < 1e-6);
    assert!((dist["no"] - 0.5).abs() < 1e-6);
}

#[test]
#[ignore = "requires libnxuskit at runtime"]
fn bn_result_iteration() {
    let path = fixture_path("asia.bif");
    let net = BnNetwork::load_file(&path).unwrap();
    let ev = BnEvidence::create().unwrap();
    let mut result = net.infer(&ev, "ve").unwrap();

    let names = result.variable_names();
    assert_eq!(names.len(), 8);

    // Reset and iterate again
    result.reset_cursor();
    let names2 = result.variable_names();
    assert_eq!(names, names2, "Reset should produce same iteration order");
}

// ── VE vs JT Cross-Validation (require libnxuskit) ──────────────────

#[test]
#[ignore = "requires libnxuskit at runtime"]
fn bn_ve_jt_agreement() {
    let path = fixture_path("asia.bif");
    let net = BnNetwork::load_file(&path).unwrap();
    let mut ev = BnEvidence::create().unwrap();
    ev.set_discrete(&net, "Smoking", "yes").unwrap();

    let result_ve = net.infer(&ev, "ve").unwrap();
    let result_jt = net.infer(&ev, "jt").unwrap();

    let ve_dist = result_ve.query("Bronchitis").unwrap();
    let jt_dist = result_jt.query("Bronchitis").unwrap();

    for (state, p_ve) in &ve_dist {
        let p_jt = jt_dist.get(state).unwrap();
        assert!(
            (p_ve - p_jt).abs() < 1e-6,
            "VE vs JT mismatch for Bronchitis[{}]: {} vs {}",
            state,
            p_ve,
            p_jt
        );
    }
}

// ── RAII Drop Safety ──────────────────────────────────────────────────

#[test]
#[ignore = "requires libnxuskit at runtime"]
fn bn_raii_drop_multiple_resources() {
    // Create multiple resources and let them drop naturally
    let path = fixture_path("asia.bif");
    let net = BnNetwork::load_file(&path).unwrap();
    let mut ev = BnEvidence::create().unwrap();
    ev.set_discrete(&net, "Smoking", "yes").unwrap();
    let _result = net.infer(&ev, "ve").unwrap();

    // All Drop impls should fire without panicking
    // Drop order: _result, ev, net (reverse declaration order)
}

// ── Alarm Network (37 nodes) ──────────────────────────────────────────

#[test]
#[ignore = "requires libnxuskit at runtime"]
fn bn_alarm_network() {
    let path = fixture_path("alarm.bif");
    let net = BnNetwork::load_file(&path).unwrap();
    assert_eq!(net.num_variables(), 37);

    let ev = BnEvidence::create().unwrap();
    let result = net.infer(&ev, "ve").unwrap();
    assert_eq!(result.num_variables(), 37);
}

// ── Part 2: BIF Export (require libnxuskit) ──────────────────────────

#[test]
#[ignore = "requires libnxuskit at runtime"]
fn bn_save_file() {
    let path = fixture_path("asia.bif");
    let net = BnNetwork::load_file(&path).unwrap();

    let tmp = std::env::temp_dir().join("nxuskit_bn_save_test.bif");
    let tmp_path = tmp.to_str().unwrap();
    net.save_file(tmp_path).expect("save_file should succeed");

    // Verify the output file exists and is non-empty
    let metadata = std::fs::metadata(&tmp).expect("saved file should exist");
    assert!(metadata.len() > 0, "saved BIF file should be non-empty");

    // Re-load and verify same variable count
    let reloaded = BnNetwork::load_file(tmp_path).expect("reloaded network should parse");
    assert_eq!(reloaded.num_variables(), 8);

    // Cleanup
    let _ = std::fs::remove_file(&tmp);
}

// ── Part 2: Gaussian Variables (require libnxuskit) ─────────────────

#[test]
#[ignore = "requires libnxuskit at runtime"]
fn bn_add_gaussian_variable() {
    let mut net = BnNetwork::create().unwrap();
    net.add_gaussian_variable("X", 0.0, 1.0)
        .expect("add_gaussian_variable should succeed");
}

#[test]
#[ignore = "requires libnxuskit at runtime"]
fn bn_set_gaussian_weight() {
    let mut net = BnNetwork::create().unwrap();
    net.add_gaussian_variable("X", 0.0, 1.0).unwrap();
    net.add_gaussian_variable("Y", 0.0, 1.0).unwrap();
    net.set_gaussian_weight("Y", "X", 0.5)
        .expect("set_gaussian_weight should succeed");
}

// ── Part 2: Continuous Evidence (require libnxuskit) ────────────────

#[test]
#[ignore = "requires libnxuskit at runtime"]
fn bn_evidence_set_continuous() {
    let mut net = BnNetwork::create().unwrap();
    net.add_gaussian_variable("X", 0.0, 1.0).unwrap();

    let mut ev = BnEvidence::create().unwrap();
    ev.set_continuous(&net, "X", 1.5)
        .expect("set_continuous should succeed");
}

// ── Part 2: LBP Inference (require libnxuskit) ──────────────────────

#[test]
#[ignore = "requires libnxuskit at runtime"]
fn bn_infer_lbp() {
    let path = fixture_path("asia.bif");
    let net = BnNetwork::load_file(&path).unwrap();
    let ev = BnEvidence::create().unwrap();

    let result = net.infer(&ev, "lbp").expect("LBP infer should succeed");
    assert_eq!(result.num_variables(), 8);
}

#[test]
#[ignore = "requires libnxuskit at runtime"]
fn bn_infer_lbp_with_config() {
    let path = fixture_path("asia.bif");
    let net = BnNetwork::load_file(&path).unwrap();
    let ev = BnEvidence::create().unwrap();

    let config = r#"{"max_iterations": 200, "damping": 0.3, "convergence_threshold": 1e-8}"#;
    let result = net
        .infer_with_config(&ev, "lbp", config)
        .expect("LBP infer_with_config should succeed");
    assert_eq!(result.num_variables(), 8);
}

// ── Part 2: NUTS Inference (require libnxuskit) ─────────────────────

#[test]
#[ignore = "requires libnxuskit at runtime"]
fn bn_infer_nuts_gaussian() {
    let mut net = BnNetwork::create().unwrap();
    net.add_gaussian_variable("X", 0.0, 1.0).unwrap();
    net.add_gaussian_variable("Y", 0.0, 1.0).unwrap();
    net.set_gaussian_weight("Y", "X", 0.8).unwrap();

    let ev = BnEvidence::create().unwrap();
    let config = r#"{"num_samples": 500, "num_tune": 200, "seed": 42}"#;
    let result = net
        .infer_with_config(&ev, "nuts", config)
        .expect("NUTS infer should succeed");

    // NUTS returns continuous marginals
    let json = result.to_json().expect("to_json should succeed");
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(
        parsed.get("continuous_marginals").is_some(),
        "NUTS result should contain continuous_marginals"
    );
}

// ── Part 2: Continuous Marginal Access (require libnxuskit) ─────────

#[test]
#[ignore = "requires libnxuskit at runtime"]
fn bn_result_mean_variance() {
    let mut net = BnNetwork::create().unwrap();
    net.add_gaussian_variable("X", 5.0, 2.0).unwrap();

    let ev = BnEvidence::create().unwrap();
    let config = r#"{"num_samples": 1000, "num_tune": 500, "seed": 42}"#;
    let result = net
        .infer_with_config(&ev, "nuts", config)
        .expect("NUTS infer should succeed");

    let mean = result.mean("X").expect("mean should succeed");
    assert!(
        (mean - 5.0).abs() < 2.0,
        "Posterior mean for X should be near prior mean 5.0, got {}",
        mean
    );

    let var = result.variance("X").expect("variance should succeed");
    assert!(var > 0.0, "Variance should be positive, got {}", var);
}

#[test]
#[ignore = "requires libnxuskit at runtime"]
fn bn_result_continuous_marginal() {
    let mut net = BnNetwork::create().unwrap();
    net.add_gaussian_variable("X", 0.0, 1.0).unwrap();

    let ev = BnEvidence::create().unwrap();
    let config = r#"{"num_samples": 500, "num_tune": 200, "seed": 42}"#;
    let result = net
        .infer_with_config(&ev, "nuts", config)
        .expect("NUTS infer should succeed");

    let marginal = result
        .continuous_marginal("X")
        .expect("continuous_marginal should succeed");

    assert!(marginal.variance > 0.0, "Variance should be positive");
    assert!(
        marginal.ci_lower < marginal.mean,
        "CI lower should be below mean"
    );
    assert!(
        marginal.ci_upper > marginal.mean,
        "CI upper should be above mean"
    );
}

// ── Part 2: Gibbs with Config (require libnxuskit) ──────────────────

#[test]
#[ignore = "requires libnxuskit at runtime"]
fn bn_infer_gibbs_with_config() {
    let path = fixture_path("asia.bif");
    let net = BnNetwork::load_file(&path).unwrap();
    let ev = BnEvidence::create().unwrap();

    let config = r#"{"num_samples": 5000, "burn_in": 500, "seed": 42}"#;
    let result = net
        .infer_with_config(&ev, "gibbs", config)
        .expect("Gibbs infer_with_config should succeed");
    assert_eq!(result.num_variables(), 8);
}

// ── Part 2: Algorithm Cross-Validation (require libnxuskit) ─────────

#[test]
#[ignore = "requires libnxuskit at runtime"]
fn bn_lbp_ve_agreement() {
    let path = fixture_path("asia.bif");
    let net = BnNetwork::load_file(&path).unwrap();
    let mut ev = BnEvidence::create().unwrap();
    ev.set_discrete(&net, "Smoking", "yes").unwrap();

    let result_ve = net.infer(&ev, "ve").unwrap();
    let result_lbp = net.infer(&ev, "lbp").unwrap();

    let ve_dist = result_ve.query("Bronchitis").unwrap();
    let lbp_dist = result_lbp.query("Bronchitis").unwrap();

    for (state, p_ve) in &ve_dist {
        let p_lbp = lbp_dist.get(state).unwrap();
        // LBP is approximate, so allow wider tolerance
        assert!(
            (p_ve - p_lbp).abs() < 0.05,
            "VE vs LBP mismatch for Bronchitis[{}]: {} vs {}",
            state,
            p_ve,
            p_lbp
        );
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Resolve a BIF fixture file relative to the nxuskit_engine test fixtures directory.
fn fixture_path(name: &str) -> String {
    // The BIF fixtures live in the nxuskit_engine crate's test directory.
    // From nxuskit/Cargo.toml, navigate to the sibling crate.
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("nxuskit_engine")
        .join("crates")
        .join("nxuskit_engine")
        .join("tests")
        .join("fixtures")
        .join("bn")
        .join(name);
    base.to_str().unwrap().to_string()
}
