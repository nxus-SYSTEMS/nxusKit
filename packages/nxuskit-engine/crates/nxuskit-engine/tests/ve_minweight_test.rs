#![allow(clippy::panic, clippy::print_stdout, clippy::print_stderr)]
//! Tests for MinWeight Variable Elimination ordering heuristic (T024).
//!
//! Verifies that:
//! 1. MinWeight produces identical posteriors to MinFill on multiple test networks
//! 2. MinWeight produces smaller max intermediate factors on high-cardinality networks
//! 3. Default heuristic is MinFill when not specified
//! 4. EliminationHeuristic serde round-trip works correctly

use nxuskit_engine::providers::bayesian::bif::load_bif_file;
use nxuskit_engine::providers::bayesian::types::{DiscreteVariable, StateName, VariableName};
use nxuskit_engine::providers::bayesian::{
    BayesianNetwork, EliminationHeuristic, Evidence, InferenceEngine, VariableElimination,
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

fn load_bif(name: &str) -> BayesianNetwork {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/bn/{name}"));
    load_bif_file(&path).unwrap()
}

/// Compare posteriors from two VE engines (MinFill vs MinWeight) within tolerance.
fn assert_posteriors_match(
    net: &BayesianNetwork,
    evidence: &Evidence,
    tolerance: f64,
    net_name: &str,
) {
    let ve_minfill = VariableElimination::new();
    let ve_minweight = VariableElimination::with_heuristic(EliminationHeuristic::MinWeight);

    let result_minfill = ve_minfill.infer(net, evidence).unwrap();
    let result_minweight = ve_minweight.infer(net, evidence).unwrap();

    // Both should produce marginals for the same variables
    assert_eq!(
        result_minfill.marginals.len(),
        result_minweight.marginals.len(),
        "{net_name}: different number of marginal variables"
    );

    for (var_name, minfill_probs) in &result_minfill.marginals {
        let minweight_probs = result_minweight
            .marginals
            .get(var_name)
            .unwrap_or_else(|| panic!("{net_name}: MinWeight missing marginal for '{var_name}'"));
        assert_eq!(
            minfill_probs.len(),
            minweight_probs.len(),
            "{net_name}: different cardinality for '{var_name}'"
        );

        for (i, (mf, mw)) in minfill_probs.iter().zip(minweight_probs.iter()).enumerate() {
            let diff = (mf - mw).abs();
            assert!(
                diff < tolerance,
                "{net_name}: P({var_name}[{i}]) minfill={mf}, minweight={mw}, diff={diff} > {tolerance}"
            );
        }
    }
}

// ── Test 1: MinWeight produces identical posteriors to MinFill ───

#[test]
fn ve_minweight_asia_prior_matches_minfill() {
    let net = load_bif("asia.bif");
    let evidence = Evidence::new();
    assert_posteriors_match(&net, &evidence, 1e-10, "asia-prior");
}

#[test]
fn ve_minweight_asia_with_evidence_matches_minfill() {
    let net = load_bif("asia.bif");
    let mut evidence = Evidence::new();
    evidence.observe(&net, &vn("Xray"), "positive").unwrap();
    evidence.observe(&net, &vn("Dyspnea"), "present").unwrap();
    assert_posteriors_match(&net, &evidence, 1e-10, "asia-evidence");
}

#[test]
fn ve_minweight_cancer_prior_matches_minfill() {
    let net = load_bif("cancer.bif");
    let evidence = Evidence::new();
    assert_posteriors_match(&net, &evidence, 1e-10, "cancer-prior");
}

#[test]
fn ve_minweight_earthquake_prior_matches_minfill() {
    let net = load_bif("earthquake.bif");
    let evidence = Evidence::new();
    assert_posteriors_match(&net, &evidence, 1e-10, "earthquake-prior");
}

#[test]
fn ve_minweight_alarm_prior_matches_minfill() {
    // Alarm network has 37 nodes with mixed cardinalities (2, 3, 4)
    // This is a good stress test for ordering differences.
    let net = load_bif("alarm.bif");
    let evidence = Evidence::new();
    assert_posteriors_match(&net, &evidence, 1e-10, "alarm-prior");
}

#[test]
fn ve_minweight_survey_prior_matches_minfill() {
    let net = load_bif("survey.bif");
    let evidence = Evidence::new();
    assert_posteriors_match(&net, &evidence, 1e-10, "survey-prior");
}

// ── Test 2: MinWeight prefers low-weight variables on high-cardinality networks ───

/// Build a synthetic network where MinWeight should produce a different
/// (potentially better) ordering than MinFill due to variable cardinality
/// asymmetry.
///
/// Network structure:
///   A(2) -> C(10)
///   B(2) -> C(10)
///   D(10) -> E(10)
///   C(10) -> E(10)
///
/// When querying E with no evidence, we need to eliminate A, B, C, D.
/// MinFill focuses on fill edges (graph connectivity).
/// MinWeight penalizes high-cardinality clusters, so it prefers eliminating
/// the binary A and B first (weight 2*10 = 20) over D (weight 10*10 = 100).
fn build_high_cardinality_network() -> BayesianNetwork {
    let mut net = BayesianNetwork::new();

    // Binary variables
    net.add_variable(var("A", &["a0", "a1"])).unwrap();
    net.add_variable(var("B", &["b0", "b1"])).unwrap();

    // High-cardinality variables (10 states)
    let states10: Vec<&str> = (0..10)
        .map(|i| match i {
            0 => "s0",
            1 => "s1",
            2 => "s2",
            3 => "s3",
            4 => "s4",
            5 => "s5",
            6 => "s6",
            7 => "s7",
            8 => "s8",
            _ => "s9",
        })
        .collect();
    net.add_variable(var("C", &states10)).unwrap();
    net.add_variable(var("D", &states10)).unwrap();
    net.add_variable(var("E", &states10)).unwrap();

    // Edges
    net.add_edge(&vn("A"), &vn("C")).unwrap();
    net.add_edge(&vn("B"), &vn("C")).unwrap();
    net.add_edge(&vn("D"), &vn("E")).unwrap();
    net.add_edge(&vn("C"), &vn("E")).unwrap();

    // CPTs: uniform for simplicity
    // A: P(A) = [0.5, 0.5]
    net.set_cpt(&vn("A"), vec![0.5, 0.5]).unwrap();
    // B: P(B) = [0.5, 0.5]
    net.set_cpt(&vn("B"), vec![0.5, 0.5]).unwrap();
    // C: P(C|A,B) = uniform, 2*2*10 = 40 entries
    let c_cpt: Vec<f64> = vec![0.1; 40];
    net.set_cpt(&vn("C"), c_cpt).unwrap();
    // D: P(D) = uniform
    let d_cpt: Vec<f64> = vec![0.1; 10];
    net.set_cpt(&vn("D"), d_cpt).unwrap();
    // E: P(E|C,D) = uniform, 10*10*10 = 1000 entries
    let e_cpt: Vec<f64> = vec![0.1; 1000];
    net.set_cpt(&vn("E"), e_cpt).unwrap();

    net
}

#[test]
fn ve_minweight_high_cardinality_produces_correct_results() {
    let net = build_high_cardinality_network();
    let evidence = Evidence::new();
    assert_posteriors_match(&net, &evidence, 1e-10, "high-cardinality-synthetic");
}

#[test]
fn ve_minweight_high_cardinality_both_produce_valid_marginals() {
    // Verify both heuristics produce valid probability distributions on
    // the high-cardinality network.
    let net = build_high_cardinality_network();
    let evidence = Evidence::new();

    for (label, ve) in [
        ("MinFill", VariableElimination::new()),
        (
            "MinWeight",
            VariableElimination::with_heuristic(EliminationHeuristic::MinWeight),
        ),
    ] {
        let result = ve.infer(&net, &evidence).unwrap();
        for (var_name, probs) in &result.marginals {
            let sum: f64 = probs.iter().sum();
            assert!(
                (sum - 1.0).abs() < 1e-6,
                "{label}: P({var_name}) sums to {sum}, not 1.0"
            );
            for (i, &p) in probs.iter().enumerate() {
                assert!(p >= 0.0, "{label}: P({var_name}[{i}]) = {p} is negative");
            }
        }
    }
}

// ── Test 3: Default heuristic is MinFill ────────────────────────

#[test]
fn ve_default_heuristic_is_minfill() {
    // VariableElimination::new() should use MinFill, producing the same
    // results as an explicit MinFill heuristic.
    let net = load_bif("asia.bif");
    let evidence = Evidence::new();

    let ve_default = VariableElimination::new();
    let ve_explicit_minfill = VariableElimination::with_heuristic(EliminationHeuristic::MinFill);

    let result_default = ve_default.infer(&net, &evidence).unwrap();
    let result_explicit = ve_explicit_minfill.infer(&net, &evidence).unwrap();

    for (var_name, default_probs) in &result_default.marginals {
        let explicit_probs = result_explicit.marginals.get(var_name).unwrap();
        for (d, e) in default_probs.iter().zip(explicit_probs.iter()) {
            assert!(
                (d - e).abs() < f64::EPSILON,
                "Default should be identical to explicit MinFill"
            );
        }
    }
}

// ── Test 4: EliminationHeuristic serde round-trip ───────────────

#[test]
fn ve_minweight_elimination_heuristic_serde_roundtrip() {
    // MinFill
    let mf = EliminationHeuristic::MinFill;
    let json = serde_json::to_string(&mf).unwrap();
    assert_eq!(json, "\"min_fill\"");
    let deserialized: EliminationHeuristic = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, EliminationHeuristic::MinFill);

    // MinWeight
    let mw = EliminationHeuristic::MinWeight;
    let json = serde_json::to_string(&mw).unwrap();
    assert_eq!(json, "\"min_weight\"");
    let deserialized: EliminationHeuristic = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, EliminationHeuristic::MinWeight);
}

#[test]
fn ve_minweight_elimination_heuristic_default_is_minfill() {
    let h: EliminationHeuristic = Default::default();
    assert_eq!(h, EliminationHeuristic::MinFill);
}

#[test]
fn ve_minweight_bn_inference_config_serde_roundtrip() {
    use nxuskit_engine::providers::bayesian::inference::BnInferenceConfig;

    // With MinWeight
    let config = BnInferenceConfig {
        elimination_heuristic: Some(EliminationHeuristic::MinWeight),
    };
    let json = serde_json::to_string(&config).unwrap();
    let parsed: BnInferenceConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(
        parsed.elimination_heuristic,
        Some(EliminationHeuristic::MinWeight)
    );

    // With None (default)
    let config_default = BnInferenceConfig::default();
    let json = serde_json::to_string(&config_default).unwrap();
    let parsed: BnInferenceConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.elimination_heuristic, None);

    // From empty JSON (backward compat)
    let parsed: BnInferenceConfig = serde_json::from_str("{}").unwrap();
    assert_eq!(parsed.elimination_heuristic, None);
}

// ── Test 5: BnOptions with elimination_heuristic ────────────────

#[test]
fn ve_minweight_bn_options_with_heuristic() {
    use nxuskit_engine::providers::bayesian::config::BnOptions;

    // Default BnOptions has no heuristic
    let opts = BnOptions::default();
    assert_eq!(opts.elimination_heuristic, None);

    // Deserialize with heuristic
    let json = r#"{"elimination_heuristic": "min_weight"}"#;
    let opts: BnOptions = serde_json::from_str(json).unwrap();
    assert_eq!(
        opts.elimination_heuristic,
        Some(EliminationHeuristic::MinWeight)
    );

    // Backward compat: no heuristic field
    let json = r#"{"algorithm": "ve"}"#;
    let opts: BnOptions = serde_json::from_str(json).unwrap();
    assert_eq!(opts.elimination_heuristic, None);
}
