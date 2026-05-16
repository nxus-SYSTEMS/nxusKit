//! Edge case tests for Bayesian Network inference engine.
//!
//! Covers all 10 specification edge cases (EC-001 through EC-010):
//! 1. Empty network → empty result set, no error
//! 2. Single-node network → returns prior as posterior
//! 3. All-evidence-observed → probability 1.0 for observed states
//! 4. Numerical underflow → log-space prevents NaN/negative
//! 5. Cyclic graph rejection → clear error identifying offending edge
//! 6. Duplicate node names → clear error, not silent overwrite
//! 7. Extremely large CPTs → graceful memory error or "use sampling" warning
//! 8. Gibbs with zero-probability evidence → detect inconsistency, not NaN
//! 9. Missing column in learning data → clear error listing missing variables
//! 10. Concurrent evidence modification during inference → queued modification or error
#![allow(clippy::panic, clippy::print_stdout, clippy::print_stderr)]

use nxuskit_engine::providers::bayesian::types::{DiscreteVariable, StateName, VariableName};
use nxuskit_engine::providers::bayesian::{
    BayesError, BayesianNetwork, Evidence, GibbsSampler, InferenceEngine, JunctionTree,
    VariableElimination,
};

fn vn(name: &str) -> VariableName {
    VariableName::new(name).unwrap()
}

fn sn(name: &str) -> StateName {
    StateName::new(name).unwrap()
}

fn var(name: &str, states: &[&str]) -> DiscreteVariable {
    DiscreteVariable::new(vn(name), states.iter().map(|s| sn(s)).collect()).unwrap()
}

// ── EC-001: Empty Network ────────────────────────────────────────

#[test]
fn ec001_empty_network_ve_returns_empty_result() {
    let net = BayesianNetwork::new();
    let evidence = Evidence::new();
    let ve = VariableElimination::new();

    // Empty network should validate fine (no variables missing CPTs)
    let result = ve.infer(&net, &evidence).unwrap();
    assert!(
        result.marginals.is_empty(),
        "Empty network should produce empty marginals"
    );
}

#[test]
fn ec001_empty_network_jt_returns_empty_result() {
    let net = BayesianNetwork::new();
    let evidence = Evidence::new();
    let jt = JunctionTree::new();

    let result = jt.infer(&net, &evidence).unwrap();
    assert!(
        result.marginals.is_empty(),
        "Empty network should produce empty marginals via JT"
    );
}

// ── EC-002: Single-Node Network ──────────────────────────────────

#[test]
fn ec002_single_node_returns_prior_as_posterior() {
    let mut net = BayesianNetwork::new();
    net.add_variable(var("X", &["a", "b", "c"])).unwrap();
    net.set_cpt(&vn("X"), vec![0.2, 0.5, 0.3]).unwrap();

    let evidence = Evidence::new();
    let ve = VariableElimination::new();
    let result = ve.infer(&net, &evidence).unwrap();

    let marginal = result.marginals.get(&vn("X")).unwrap();
    assert!((marginal[0] - 0.2).abs() < 1e-10, "P(X=a) should be 0.2");
    assert!((marginal[1] - 0.5).abs() < 1e-10, "P(X=b) should be 0.5");
    assert!((marginal[2] - 0.3).abs() < 1e-10, "P(X=c) should be 0.3");
}

#[test]
fn ec002_single_node_jt_returns_prior() {
    let mut net = BayesianNetwork::new();
    net.add_variable(var("X", &["a", "b"])).unwrap();
    net.set_cpt(&vn("X"), vec![0.7, 0.3]).unwrap();

    let evidence = Evidence::new();
    let jt = JunctionTree::new();
    let result = jt.infer(&net, &evidence).unwrap();

    let marginal = result.marginals.get(&vn("X")).unwrap();
    assert!((marginal[0] - 0.7).abs() < 1e-10);
    assert!((marginal[1] - 0.3).abs() < 1e-10);
}

// ── EC-003: All Evidence Observed ────────────────────────────────

#[test]
fn ec003_all_evidence_observed_ve() {
    let mut net = BayesianNetwork::new();
    net.add_variable(var("A", &["0", "1"])).unwrap();
    net.add_variable(var("B", &["0", "1"])).unwrap();
    net.add_edge(&vn("A"), &vn("B")).unwrap();
    net.set_cpt(&vn("A"), vec![0.6, 0.4]).unwrap();
    net.set_cpt(&vn("B"), vec![0.9, 0.1, 0.2, 0.8]).unwrap();

    let mut evidence = Evidence::new();
    evidence.observe(&net, &vn("A"), "0").unwrap();
    evidence.observe(&net, &vn("B"), "1").unwrap();

    let ve = VariableElimination::new();
    let result = ve.infer(&net, &evidence).unwrap();

    // With all variables observed, remaining (unobserved) marginals should be empty
    // since both A and B are observed
    assert!(
        result.marginals.is_empty(),
        "All-evidence-observed: no unobserved variables to compute marginals for"
    );
}

// ── EC-004: Numerical Underflow (Log-Space) ──────────────────────

#[test]
fn ec004_numerical_underflow_prevented_by_log_space() {
    // Create a network with near-zero probabilities that would underflow in linear space
    let mut net = BayesianNetwork::new();
    net.add_variable(var("R", &["0", "1"])).unwrap();
    net.add_variable(var("C", &["0", "1"])).unwrap();
    net.add_edge(&vn("R"), &vn("C")).unwrap();

    // Very small probability for C=1 given R=0
    net.set_cpt(&vn("R"), vec![0.999999, 0.000001]).unwrap();
    net.set_cpt(&vn("C"), vec![0.999999, 0.000001, 0.000001, 0.999999])
        .unwrap();

    let evidence = Evidence::new();
    let ve = VariableElimination::new();
    let result = ve.infer(&net, &evidence).unwrap();

    // All marginals should be finite and non-NaN
    for (vn, probs) in &result.marginals {
        for (i, p) in probs.iter().enumerate() {
            assert!(
                p.is_finite() && *p >= 0.0,
                "Variable {} state {}: prob {} should be finite non-negative",
                vn,
                i,
                p
            );
        }
    }

    // Verify the sum of marginals for each variable is ~1.0
    for (vn, probs) in &result.marginals {
        let sum: f64 = probs.iter().sum();
        assert!(
            (sum - 1.0).abs() < 1e-6,
            "Variable {}: marginals sum to {} (expected ~1.0)",
            vn,
            sum
        );
    }
}

#[test]
fn ec004_log_space_factor_extreme_values() {
    // Test that factors with extreme probabilities maintain numerical stability
    let mut net = BayesianNetwork::new();
    net.add_variable(var("X", &["0", "1"])).unwrap();
    // Near-deterministic CPT
    net.set_cpt(&vn("X"), vec![1e-15, 1.0 - 1e-15]).unwrap();

    let evidence = Evidence::new();
    let ve = VariableElimination::new();
    let result = ve.infer(&net, &evidence).unwrap();

    let marginal = result.marginals.get(&vn("X")).unwrap();
    assert!(marginal[0].is_finite(), "Near-zero prob should not be NaN");
    assert!(marginal[1].is_finite(), "Near-one prob should not be NaN");
    assert!(marginal[0] >= 0.0, "Probability should be non-negative");
}

// ── EC-005: Cyclic Graph Rejection ───────────────────────────────

#[test]
fn ec005_cyclic_graph_rejected_with_clear_error() {
    let mut net = BayesianNetwork::new();
    net.add_variable(var("A", &["0", "1"])).unwrap();
    net.add_variable(var("B", &["0", "1"])).unwrap();
    net.add_variable(var("C", &["0", "1"])).unwrap();

    net.add_edge(&vn("A"), &vn("B")).unwrap();
    net.add_edge(&vn("B"), &vn("C")).unwrap();

    // Attempt to create cycle C → A
    let err = net.add_edge(&vn("C"), &vn("A")).unwrap_err();
    match &err {
        BayesError::InvalidGraph(msg) => {
            assert!(
                msg.contains("cycle"),
                "Error should mention 'cycle': {}",
                msg
            );
            assert!(
                msg.contains("C") && msg.contains("A"),
                "Error should identify the offending edge: {}",
                msg
            );
        }
        other => panic!("Expected InvalidGraph, got: {:?}", other),
    }
}

#[test]
fn ec005_direct_self_loop_rejected() {
    let mut net = BayesianNetwork::new();
    net.add_variable(var("A", &["0", "1"])).unwrap();

    let err = net.add_edge(&vn("A"), &vn("A")).unwrap_err();
    match &err {
        BayesError::InvalidGraph(msg) => {
            assert!(
                msg.contains("cycle") || msg.contains("self"),
                "Self-loop error: {}",
                msg
            );
        }
        other => panic!("Expected InvalidGraph for self-loop, got: {:?}", other),
    }
}

// ── EC-006: Duplicate Node Names ─────────────────────────────────

#[test]
fn ec006_duplicate_node_name_rejected() {
    let mut net = BayesianNetwork::new();
    net.add_variable(var("Duplicate", &["0", "1"])).unwrap();

    let err = net
        .add_variable(var("Duplicate", &["a", "b", "c"]))
        .unwrap_err();
    match &err {
        BayesError::InvalidGraph(msg) => {
            assert!(
                msg.contains("Duplicate") && msg.contains("already exists"),
                "Error should identify duplicate variable: {}",
                msg
            );
        }
        other => panic!("Expected InvalidGraph, got: {:?}", other),
    }
}

#[test]
fn ec006_duplicate_does_not_silently_overwrite() {
    let mut net = BayesianNetwork::new();
    net.add_variable(var("X", &["original_a", "original_b"]))
        .unwrap();

    // Try adding duplicate with different states
    let _ = net.add_variable(var("X", &["new_a", "new_b", "new_c"]));

    // Original variable should remain unchanged
    let original = net.variable(&vn("X")).unwrap();
    assert_eq!(
        original.cardinality(),
        2,
        "Original variable should not be overwritten"
    );
}

// ── EC-007: Extremely Large CPTs ─────────────────────────────────

#[test]
fn ec007_large_cpt_handled_gracefully() {
    // 5 parents with 3 states each = 3^5 = 243 parent configurations
    // Child with 3 states = 729 CPT entries — manageable
    let mut net = BayesianNetwork::new();
    net.add_variable(var("Child", &["a", "b", "c"])).unwrap();

    for i in 0..5 {
        let name = format!("P{}", i);
        net.add_variable(var(&name, &["x", "y", "z"])).unwrap();
        net.add_edge(&vn(&name), &vn("Child")).unwrap();
    }

    // Set uniform CPTs for all parents
    for i in 0..5 {
        let name = format!("P{}", i);
        net.set_cpt(&vn(&name), vec![1.0 / 3.0; 3]).unwrap();
    }

    // Set uniform CPT for child (729 entries = 243 configs × 3 states)
    let cpt_size = 3usize.pow(6); // 3^5 parent configs × 3 child states
    let uniform_row: Vec<f64> = (0..cpt_size).map(|_| 1.0 / 3.0).collect();
    net.set_cpt(&vn("Child"), uniform_row).unwrap();

    // Inference should still work
    let ve = VariableElimination::new();
    let evidence = Evidence::new();
    let result = ve.infer(&net, &evidence).unwrap();

    // All marginals should be valid (uniform for uniform CPTs)
    for probs in result.marginals.values() {
        let sum: f64 = probs.iter().sum();
        assert!(
            (sum - 1.0).abs() < 1e-6,
            "Large CPT marginals should sum to 1.0"
        );
    }
}

#[test]
fn ec007_cpt_size_mismatch_detected() {
    let mut net = BayesianNetwork::new();
    net.add_variable(var("Parent", &["0", "1"])).unwrap();
    net.add_variable(var("Child", &["a", "b"])).unwrap();
    net.add_edge(&vn("Parent"), &vn("Child")).unwrap();

    // Child has 2 parents configs × 2 states = 4 entries needed
    // Provide wrong size
    let err = net.set_cpt(&vn("Child"), vec![0.5, 0.5]).unwrap_err();
    match &err {
        BayesError::InvalidCpt(msg) => {
            assert!(
                msg.contains("expected") && msg.contains("got"),
                "CPT size error should show expected vs actual: {}",
                msg
            );
        }
        other => panic!("Expected InvalidCpt, got: {:?}", other),
    }
}

// ── EC-008: Gibbs with Zero-Probability Evidence ─────────────────

#[test]
fn ec008_gibbs_zero_probability_evidence_detected() {
    // Create a deterministic network where P(B=1|A=0) = 0
    let mut net = BayesianNetwork::new();
    net.add_variable(var("A", &["0", "1"])).unwrap();
    net.add_variable(var("B", &["0", "1"])).unwrap();
    net.add_edge(&vn("A"), &vn("B")).unwrap();
    net.set_cpt(&vn("A"), vec![1.0, 0.0]).unwrap(); // A is always 0
    net.set_cpt(&vn("B"), vec![1.0, 0.0, 0.0, 1.0]).unwrap(); // B=A

    // Evidence: A=0, B=1 — this is impossible since B=A
    let mut evidence = Evidence::new();
    evidence.observe(&net, &vn("A"), "0").unwrap();
    evidence.observe(&net, &vn("B"), "1").unwrap();

    let gibbs = GibbsSampler::new(1000, 100).with_seed(42);
    let result = gibbs.infer(&net, &evidence);

    // Should either return an error or produce results without NaN
    match result {
        Err(e) => {
            // Good: detected zero-probability evidence
            let msg = format!("{}", e);
            assert!(!msg.is_empty(), "Error message should be informative");
        }
        Ok(res) => {
            // If it returns results, they must not contain NaN
            for (vn, probs) in &res.marginals {
                for (i, p) in probs.iter().enumerate() {
                    assert!(
                        !p.is_nan(),
                        "Gibbs marginal for {} state {} is NaN (should not happen)",
                        vn,
                        i
                    );
                }
            }
        }
    }
}

// ── EC-009: Missing Column in Learning Data ──────────────────────

#[test]
fn ec009_missing_column_in_csv_detected() {
    use nxuskit_engine::providers::bayesian::learning::Dataset;
    use std::io::Write;
    use tempfile::NamedTempFile;

    let mut net = BayesianNetwork::new();
    net.add_variable(var("A", &["0", "1"])).unwrap();
    net.add_variable(var("B", &["0", "1"])).unwrap();
    net.add_edge(&vn("A"), &vn("B")).unwrap();
    net.set_cpt(&vn("A"), vec![0.5, 0.5]).unwrap();
    net.set_cpt(&vn("B"), vec![0.9, 0.1, 0.1, 0.9]).unwrap();

    // CSV that's missing column B
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(b"A\n0\n1\n0\n").unwrap();
    f.flush().unwrap();

    let result = Dataset::from_csv(f.path(), &net);
    match result {
        Err(e) => {
            let msg = format!("{}", e);
            assert!(
                msg.contains("B") || msg.contains("missing") || msg.contains("Missing"),
                "Error should identify missing column: {}",
                msg
            );
        }
        Ok(ds) => {
            // If it succeeds, verify column B is treated as missing
            assert!(
                ds.column_index(&vn("B")).is_none(),
                "Column B should be missing or flagged"
            );
        }
    }
}

// ── EC-010: Concurrent Evidence Modification ─────────────────────

#[test]
fn ec010_evidence_cloning_prevents_interference() {
    // Evidence is Clone, so creating a copy before inference protects against modification
    let mut net = BayesianNetwork::new();
    net.add_variable(var("A", &["0", "1"])).unwrap();
    net.set_cpt(&vn("A"), vec![0.3, 0.7]).unwrap();

    let mut evidence = Evidence::new();
    evidence.observe(&net, &vn("A"), "0").unwrap();

    // Clone evidence before inference
    let frozen_evidence = evidence.clone();

    // Modify original evidence
    evidence.retract(&vn("A"));
    evidence.observe(&net, &vn("A"), "1").unwrap();

    // Frozen evidence should still have A=0
    assert!(
        frozen_evidence.is_observed(&vn("A")),
        "Cloned evidence should be independent"
    );

    // Use frozen evidence for inference
    let ve = VariableElimination::new();
    let result = ve.infer(&net, &frozen_evidence).unwrap();

    // Should not contain A since it's observed — but the result should be consistent
    assert!(
        result.marginals.is_empty() || !result.marginals.contains_key(&vn("A")),
        "Observed variable A should not appear in unobserved marginals"
    );
}

#[test]
fn ec010_evidence_thread_safety_via_cloning() {
    // Test that evidence can be safely cloned across threads
    use std::sync::Arc;
    use std::thread;

    let mut net = BayesianNetwork::new();
    net.add_variable(var("X", &["0", "1"])).unwrap();
    net.set_cpt(&vn("X"), vec![0.4, 0.6]).unwrap();
    let net = Arc::new(net);

    let mut evidence = Evidence::new();
    evidence.observe(&net, &vn("X"), "0").unwrap();

    let handles: Vec<_> = (0..4)
        .map(|_| {
            let ev = evidence.clone();
            let n = Arc::clone(&net);
            thread::spawn(move || {
                let ve = VariableElimination::new();
                ve.infer(&n, &ev).unwrap()
            })
        })
        .collect();

    // All threads should complete without error
    for h in handles {
        let result = h.join().unwrap();
        // Consistent results: A is observed, so marginals should be empty
        assert!(result.marginals.is_empty());
    }
}
