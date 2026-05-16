//! Additional sampling algorithms for Bayesian Network inference.
//!
//! - **Forward Sampling** (ancestral): Generate samples from the joint distribution
//! - **Rejection Sampling**: Accept/reject forward samples based on evidence
//! - **Likelihood-Weighted Sampling**: Weight forward samples by evidence likelihood

use std::collections::HashMap;
use std::time::Instant;

use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha20Rng;

use super::{
    InferenceDiagnostics, InferenceEngine, InferenceResult, Marginal, Sample, SamplingInference,
};
use crate::providers::bayesian::error::{BayesError, BayesResult};
use crate::providers::bayesian::evidence::Evidence;
use crate::providers::bayesian::network::BayesianNetwork;
use crate::providers::bayesian::types::{StateIndex, VariableName};

// ── Forward Sampling ────────────────────────────────────────────────

/// Forward (ancestral) sampler for Bayesian Networks.
///
/// Generates samples from the joint distribution P(X₁, X₂, ..., Xₙ) by
/// traversing the network in topological order and sampling each variable
/// from its conditional distribution given parent values.
///
/// **Note**: Forward sampling does NOT condition on evidence. For inference
/// with evidence, use Rejection or Likelihood-Weighted sampling.
#[derive(Debug, Clone)]
pub struct ForwardSampler {
    pub num_samples: usize,
    pub seed: Option<u64>,
}

impl ForwardSampler {
    /// Create a forward sampler that draws `num_samples` from the joint distribution.
    pub fn new(num_samples: usize) -> Self {
        Self {
            num_samples,
            seed: None,
        }
    }

    /// Set the RNG seed for reproducible sampling (ChaCha20).
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }
}

/// Generate a single forward sample from the joint distribution.
fn forward_sample_one(
    network: &BayesianNetwork,
    rng: &mut ChaCha20Rng,
) -> BayesResult<HashMap<VariableName, usize>> {
    let topo_order = network.topological_sort();
    let mut assignment: HashMap<VariableName, usize> = HashMap::new();

    for var_name in &topo_order {
        let cpt = network.cpt(var_name).ok_or_else(|| {
            BayesError::IncompleteNetwork(format!("Variable '{}' has no CPT", var_name))
        })?;

        let var = network.variable(var_name).unwrap();
        let parents = network.parents(var_name);
        let cardinality = var.cardinality();

        let mut probs = Vec::with_capacity(cardinality);
        for state_val in 0..cardinality {
            let mut cpt_assignment = Vec::with_capacity(cpt.variables.len());
            for parent in &parents {
                cpt_assignment.push(StateIndex::new(assignment[parent]));
            }
            cpt_assignment.push(StateIndex::new(state_val));
            let idx = cpt.assignment_to_index(&cpt_assignment);
            probs.push(cpt.log_values[idx].exp());
        }

        let sampled = sample_categorical(&probs, rng);
        assignment.insert(var_name.clone(), sampled);
    }

    Ok(assignment)
}

impl InferenceEngine for ForwardSampler {
    fn infer(
        &self,
        network: &BayesianNetwork,
        evidence: &Evidence,
    ) -> BayesResult<InferenceResult> {
        network.validate()?;
        let start = Instant::now();

        let mut rng = match self.seed {
            Some(s) => ChaCha20Rng::seed_from_u64(s),
            None => ChaCha20Rng::from_rng(&mut rand::rng()),
        };

        // Forward sampling ignores evidence (just samples from the prior)
        let mut samples = Vec::with_capacity(self.num_samples);
        for _ in 0..self.num_samples {
            let assignment = forward_sample_one(network, &mut rng)?;
            let sample = Sample {
                assignments: assignment,
            };
            samples.push(sample);
        }

        let marginals = super::samples_to_marginals(&samples, network);
        let elapsed = start.elapsed();

        // Remove observed variables from marginals
        let observed = evidence.observations().keys().cloned().collect::<Vec<_>>();
        let mut filtered_marginals = HashMap::new();
        for (vn, probs) in &marginals {
            if !observed.contains(vn) {
                filtered_marginals.insert(vn.clone(), probs.clone());
            }
        }

        Ok(InferenceResult {
            log_marginals: HashMap::new(),
            marginals: filtered_marginals,
            algorithm: "forward_sampling".to_string(),
            elapsed,
            diagnostics: Some(InferenceDiagnostics {
                iterations: self.num_samples,
                ..Default::default()
            }),
            continuous_marginals: HashMap::new(),
            nuts_diagnostics: None,
        })
    }

    fn query(
        &self,
        network: &BayesianNetwork,
        evidence: &Evidence,
        variable: &VariableName,
    ) -> BayesResult<Marginal> {
        let result = self.infer(network, evidence)?;
        result.marginals.get(variable).cloned().ok_or_else(|| {
            BayesError::InferenceError(format!(
                "Variable '{}' not found in forward sampling results",
                variable
            ))
        })
    }

    fn algorithm_name(&self) -> &str {
        "forward_sampling"
    }
}

impl SamplingInference for ForwardSampler {
    fn sample(
        &self,
        network: &BayesianNetwork,
        _evidence: &Evidence,
        num_samples: usize,
        _burn_in: usize,
        seed: Option<u64>,
    ) -> BayesResult<Vec<Sample>> {
        network.validate()?;
        let mut rng = match seed {
            Some(s) => ChaCha20Rng::seed_from_u64(s),
            None => ChaCha20Rng::from_rng(&mut rand::rng()),
        };

        let mut samples = Vec::with_capacity(num_samples);
        for _ in 0..num_samples {
            let assignment = forward_sample_one(network, &mut rng)?;
            samples.push(Sample {
                assignments: assignment,
            });
        }
        Ok(samples)
    }
}

// ── Rejection Sampling ──────────────────────────────────────────────

/// Rejection sampler for Bayesian Network inference.
///
/// Forward-samples from the joint distribution and rejects any sample
/// that doesn't match the observed evidence. Returns marginals computed
/// from accepted samples only.
///
/// **Warning**: Exponentially slow when evidence is unlikely.
#[derive(Debug, Clone)]
pub struct RejectionSampler {
    pub num_samples: usize,
    /// Maximum total attempts before giving up.
    pub max_attempts: usize,
    pub seed: Option<u64>,
}

impl RejectionSampler {
    /// Create a rejection sampler targeting `num_samples` accepted samples.
    /// Default `max_attempts` is `num_samples * 1000`.
    pub fn new(num_samples: usize) -> Self {
        Self {
            num_samples,
            max_attempts: num_samples * 1000,
            seed: None,
        }
    }

    /// Set the maximum total forward-sample attempts before giving up.
    pub fn with_max_attempts(mut self, max: usize) -> Self {
        self.max_attempts = max;
        self
    }

    /// Set the RNG seed for reproducible sampling (ChaCha20).
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }
}

impl InferenceEngine for RejectionSampler {
    fn infer(
        &self,
        network: &BayesianNetwork,
        evidence: &Evidence,
    ) -> BayesResult<InferenceResult> {
        network.validate()?;
        let start = Instant::now();

        let mut rng = match self.seed {
            Some(s) => ChaCha20Rng::seed_from_u64(s),
            None => ChaCha20Rng::from_rng(&mut rand::rng()),
        };

        let observed = evidence.observations().keys().cloned().collect::<Vec<_>>();
        let mut accepted = Vec::with_capacity(self.num_samples);
        let mut total_attempts = 0;

        while accepted.len() < self.num_samples && total_attempts < self.max_attempts {
            total_attempts += 1;
            let assignment = forward_sample_one(network, &mut rng)?;

            // Check if sample matches evidence
            let mut matches = true;
            for var_name in &observed {
                if let Some(ev_state) = evidence.get(var_name)
                    && let Some(&sample_state) = assignment.get(var_name)
                    && sample_state != ev_state.value()
                {
                    matches = false;
                    break;
                }
            }

            if matches {
                // Remove evidence variables from the sample
                let mut filtered: HashMap<VariableName, usize> = HashMap::new();
                for (vn, state) in &assignment {
                    if !observed.contains(vn) {
                        filtered.insert(vn.clone(), *state);
                    }
                }
                accepted.push(Sample {
                    assignments: filtered,
                });
            }
        }

        if accepted.is_empty() {
            return Err(BayesError::ZeroProbabilityEvidence(format!(
                "Rejection sampling: 0 accepted out of {} attempts. Evidence may be impossible.",
                total_attempts
            )));
        }

        let marginals = super::samples_to_marginals(&accepted, network);
        let elapsed = start.elapsed();
        let rejection_rate = 1.0 - (accepted.len() as f64 / total_attempts as f64);

        Ok(InferenceResult {
            log_marginals: HashMap::new(),
            marginals,
            algorithm: "rejection_sampling".to_string(),
            elapsed,
            diagnostics: Some(InferenceDiagnostics {
                iterations: total_attempts,
                max_marginal_change: rejection_rate,
                ..Default::default()
            }),
            continuous_marginals: HashMap::new(),
            nuts_diagnostics: None,
        })
    }

    fn query(
        &self,
        network: &BayesianNetwork,
        evidence: &Evidence,
        variable: &VariableName,
    ) -> BayesResult<Marginal> {
        let result = self.infer(network, evidence)?;
        result.marginals.get(variable).cloned().ok_or_else(|| {
            BayesError::InferenceError(format!(
                "Variable '{}' not found in rejection sampling results",
                variable
            ))
        })
    }

    fn algorithm_name(&self) -> &str {
        "rejection_sampling"
    }
}

// ── Likelihood-Weighted Sampling ────────────────────────────────────

/// Likelihood-weighted sampler for Bayesian Network inference.
///
/// Forward-samples non-evidence variables and weights each sample by the
/// likelihood of the evidence given the sample. More efficient than
/// rejection sampling for rare evidence.
#[derive(Debug, Clone)]
pub struct LikelihoodWeightedSampler {
    pub num_samples: usize,
    pub seed: Option<u64>,
}

impl LikelihoodWeightedSampler {
    /// Create a likelihood-weighted sampler drawing `num_samples` weighted samples.
    pub fn new(num_samples: usize) -> Self {
        Self {
            num_samples,
            seed: None,
        }
    }

    /// Set the RNG seed for reproducible sampling (ChaCha20).
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }
}

/// Generate a single likelihood-weighted sample.
/// Returns (sample_assignment, weight).
fn lw_sample_one(
    network: &BayesianNetwork,
    evidence: &Evidence,
    rng: &mut ChaCha20Rng,
) -> BayesResult<(HashMap<VariableName, usize>, f64)> {
    let topo_order = network.topological_sort();
    let mut assignment: HashMap<VariableName, usize> = HashMap::new();
    let mut weight = 1.0;

    for var_name in &topo_order {
        let cpt = network.cpt(var_name).ok_or_else(|| {
            BayesError::IncompleteNetwork(format!("Variable '{}' has no CPT", var_name))
        })?;

        let var = network.variable(var_name).unwrap();
        let parents = network.parents(var_name);
        let cardinality = var.cardinality();

        // Compute P(X | parents)
        let mut probs = Vec::with_capacity(cardinality);
        for state_val in 0..cardinality {
            let mut cpt_assignment = Vec::with_capacity(cpt.variables.len());
            for parent in &parents {
                cpt_assignment.push(StateIndex::new(assignment[parent]));
            }
            cpt_assignment.push(StateIndex::new(state_val));
            let idx = cpt.assignment_to_index(&cpt_assignment);
            probs.push(cpt.log_values[idx].exp());
        }

        if let Some(ev_state) = evidence.get(var_name) {
            // Evidence variable: fix to observed value, multiply weight by P(evidence | parents)
            let state_val = ev_state.value();
            weight *= probs[state_val];
            assignment.insert(var_name.clone(), state_val);
        } else {
            // Non-evidence variable: sample from conditional
            let sampled = sample_categorical(&probs, rng);
            assignment.insert(var_name.clone(), sampled);
        }
    }

    Ok((assignment, weight))
}

/// Compute weighted marginals from likelihood-weighted samples.
fn weighted_marginals(
    samples: &[(HashMap<VariableName, usize>, f64)],
    network: &BayesianNetwork,
    evidence: &Evidence,
) -> HashMap<VariableName, Vec<f64>> {
    let mut marginals = HashMap::new();
    let observed = evidence.observations().keys().cloned().collect::<Vec<_>>();

    if samples.is_empty() {
        return marginals;
    }

    let total_weight: f64 = samples.iter().map(|(_, w)| w).sum();
    if total_weight <= 0.0 {
        return marginals;
    }

    // Collect non-evidence variable names
    let var_names: Vec<VariableName> = samples[0]
        .0
        .keys()
        .filter(|vn| !observed.contains(vn))
        .cloned()
        .collect();

    for var_name in &var_names {
        let var = match network.variable(var_name) {
            Some(v) => v,
            None => continue,
        };
        let cardinality = var.cardinality();
        let mut weighted_counts = vec![0.0_f64; cardinality];

        for (assignment, weight) in samples {
            if let Some(&state) = assignment.get(var_name)
                && state < cardinality
            {
                weighted_counts[state] += weight;
            }
        }

        let probs: Vec<f64> = weighted_counts.iter().map(|&c| c / total_weight).collect();
        marginals.insert(var_name.clone(), probs);
    }

    marginals
}

impl InferenceEngine for LikelihoodWeightedSampler {
    fn infer(
        &self,
        network: &BayesianNetwork,
        evidence: &Evidence,
    ) -> BayesResult<InferenceResult> {
        network.validate()?;
        let start = Instant::now();

        let mut rng = match self.seed {
            Some(s) => ChaCha20Rng::seed_from_u64(s),
            None => ChaCha20Rng::from_rng(&mut rand::rng()),
        };

        let mut samples = Vec::with_capacity(self.num_samples);
        let mut total_weight = 0.0;

        for _ in 0..self.num_samples {
            let (assignment, weight) = lw_sample_one(network, evidence, &mut rng)?;
            total_weight += weight;
            samples.push((assignment, weight));
        }

        if total_weight <= 0.0 {
            return Err(BayesError::ZeroProbabilityEvidence(
                "Likelihood-weighted sampling: all weights are zero. Evidence may be impossible."
                    .into(),
            ));
        }

        let marginals = weighted_marginals(&samples, network, evidence);
        let elapsed = start.elapsed();
        let avg_weight = total_weight / self.num_samples as f64;

        Ok(InferenceResult {
            log_marginals: HashMap::new(),
            marginals,
            algorithm: "likelihood_weighted".to_string(),
            elapsed,
            diagnostics: Some(InferenceDiagnostics {
                iterations: self.num_samples,
                effective_sample_size: Some(effective_sample_size_lw(&samples)),
                max_marginal_change: avg_weight,
                ..Default::default()
            }),
            continuous_marginals: HashMap::new(),
            nuts_diagnostics: None,
        })
    }

    fn query(
        &self,
        network: &BayesianNetwork,
        evidence: &Evidence,
        variable: &VariableName,
    ) -> BayesResult<Marginal> {
        let result = self.infer(network, evidence)?;
        result.marginals.get(variable).cloned().ok_or_else(|| {
            BayesError::InferenceError(format!(
                "Variable '{}' not found in LW sampling results",
                variable
            ))
        })
    }

    fn algorithm_name(&self) -> &str {
        "likelihood_weighted"
    }
}

/// Compute effective sample size for likelihood-weighted samples.
/// ESS = (sum w_i)^2 / sum(w_i^2)
fn effective_sample_size_lw(samples: &[(HashMap<VariableName, usize>, f64)]) -> f64 {
    let sum_w: f64 = samples.iter().map(|(_, w)| w).sum();
    let sum_w2: f64 = samples.iter().map(|(_, w)| w * w).sum();
    if sum_w2 <= 0.0 {
        return 0.0;
    }
    (sum_w * sum_w) / sum_w2
}

// ── Shared helpers ──────────────────────────────────────────────────

/// Sample from a categorical distribution given unnormalized probabilities.
fn sample_categorical(probs: &[f64], rng: &mut ChaCha20Rng) -> usize {
    let sum: f64 = probs.iter().sum();
    if sum <= 0.0 {
        return rng.random_range(0..probs.len());
    }

    let threshold: f64 = rng.random::<f64>() * sum;
    let mut cumulative = 0.0;
    for (i, &p) in probs.iter().enumerate() {
        cumulative += p;
        if cumulative >= threshold {
            return i;
        }
    }
    probs.len() - 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::bayesian::bif::load_bif_file;
    use crate::providers::bayesian::inference::VariableElimination;

    fn var_name(s: &str) -> VariableName {
        VariableName::new(s).unwrap()
    }

    fn load_asia() -> BayesianNetwork {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bn/asia.bif");
        load_bif_file(&path).unwrap()
    }

    // ── Forward Sampling ────────────────────────────────────────

    #[test]
    fn forward_sampling_prior_marginals() {
        let net = load_asia();
        let evidence = Evidence::new();
        let sampler = ForwardSampler::new(50_000).with_seed(42);

        let result = sampler.infer(&net, &evidence).unwrap();
        assert_eq!(result.algorithm, "forward_sampling");
        assert_eq!(result.marginals.len(), 8);

        // Compare against VE exact prior marginals
        let ve = VariableElimination::new();
        let exact = ve.infer(&net, &evidence).unwrap();

        for (var_name, exact_probs) in &exact.marginals {
            let sampled_probs = result.marginals.get(var_name).unwrap();
            for (ep, sp) in exact_probs.iter().zip(sampled_probs.iter()) {
                assert!(
                    (ep - sp).abs() < 0.02,
                    "Forward sampling P({})={} vs exact {}, diff={}",
                    var_name,
                    sp,
                    ep,
                    (ep - sp).abs()
                );
            }
        }
    }

    #[test]
    fn forward_sampling_deterministic_with_seed() {
        let net = load_asia();
        let evidence = Evidence::new();

        let result1 = ForwardSampler::new(1000)
            .with_seed(42)
            .infer(&net, &evidence)
            .unwrap();
        let result2 = ForwardSampler::new(1000)
            .with_seed(42)
            .infer(&net, &evidence)
            .unwrap();

        for (vn, probs1) in &result1.marginals {
            let probs2 = result2.marginals.get(vn).unwrap();
            for (p1, p2) in probs1.iter().zip(probs2.iter()) {
                assert!(
                    (p1 - p2).abs() < 1e-15,
                    "Forward sampling should be deterministic with same seed"
                );
            }
        }
    }

    #[test]
    fn forward_sampling_returns_samples() {
        let net = load_asia();
        let evidence = Evidence::new();
        let sampler = ForwardSampler::new(100).with_seed(42);

        let samples = sampler.sample(&net, &evidence, 100, 0, Some(42)).unwrap();
        assert_eq!(samples.len(), 100);
        for s in &samples {
            assert_eq!(s.assignments.len(), 8);
        }
    }

    // ── Rejection Sampling ──────────────────────────────────────

    #[test]
    fn rejection_sampling_with_evidence() {
        let net = load_asia();
        let mut evidence = Evidence::new();
        evidence.observe(&net, &var_name("Smoking"), "yes").unwrap();

        let sampler = RejectionSampler::new(10_000).with_seed(42);
        let result = sampler.infer(&net, &evidence).unwrap();
        assert_eq!(result.algorithm, "rejection_sampling");

        // Smoking should not be in marginals
        assert!(!result.marginals.contains_key(&var_name("Smoking")));

        // Compare against VE
        let ve = VariableElimination::new();
        let exact = ve.infer(&net, &evidence).unwrap();

        for (vn, exact_probs) in &exact.marginals {
            if let Some(sampled_probs) = result.marginals.get(vn) {
                for (ep, sp) in exact_probs.iter().zip(sampled_probs.iter()) {
                    assert!(
                        (ep - sp).abs() < 0.05,
                        "Rejection P({})={} vs exact {}, diff={}",
                        vn,
                        sp,
                        ep,
                        (ep - sp).abs()
                    );
                }
            }
        }
    }

    #[test]
    fn rejection_sampling_no_evidence() {
        let net = load_asia();
        let evidence = Evidence::new();
        let sampler = RejectionSampler::new(10_000).with_seed(42);

        let result = sampler.infer(&net, &evidence).unwrap();
        // With no evidence, all forward samples are accepted
        assert_eq!(result.marginals.len(), 8);
    }

    #[test]
    fn rejection_sampling_rejection_count() {
        let net = load_asia();
        let mut evidence = Evidence::new();
        evidence.observe(&net, &var_name("Smoking"), "yes").unwrap();

        let sampler = RejectionSampler::new(100).with_seed(42);
        let result = sampler.infer(&net, &evidence).unwrap();

        // With Smoking=yes (P=0.5), about half should be rejected
        let total_attempts = result.diagnostics.as_ref().unwrap().iterations;
        assert!(
            total_attempts >= 100,
            "Should have attempted at least 100, got {}",
            total_attempts
        );
    }

    // ── Likelihood-Weighted Sampling ────────────────────────────

    #[test]
    fn lw_sampling_with_evidence() {
        let net = load_asia();
        let mut evidence = Evidence::new();
        evidence.observe(&net, &var_name("Smoking"), "yes").unwrap();

        let sampler = LikelihoodWeightedSampler::new(10_000).with_seed(42);
        let result = sampler.infer(&net, &evidence).unwrap();
        assert_eq!(result.algorithm, "likelihood_weighted");

        // Smoking should not be in marginals
        assert!(!result.marginals.contains_key(&var_name("Smoking")));

        // Compare against VE
        let ve = VariableElimination::new();
        let exact = ve.infer(&net, &evidence).unwrap();

        for (vn, exact_probs) in &exact.marginals {
            if let Some(sampled_probs) = result.marginals.get(vn) {
                for (ep, sp) in exact_probs.iter().zip(sampled_probs.iter()) {
                    assert!(
                        (ep - sp).abs() < 0.05,
                        "LW P({})={} vs exact {}, diff={}",
                        vn,
                        sp,
                        ep,
                        (ep - sp).abs()
                    );
                }
            }
        }
    }

    #[test]
    fn lw_sampling_ess() {
        let net = load_asia();
        let evidence = Evidence::new();
        let sampler = LikelihoodWeightedSampler::new(1_000).with_seed(42);

        let result = sampler.infer(&net, &evidence).unwrap();
        let ess = result
            .diagnostics
            .as_ref()
            .unwrap()
            .effective_sample_size
            .unwrap();

        // With no evidence, all weights are equal, so ESS should equal num_samples
        assert!(
            (ess - 1000.0).abs() < 1.0,
            "ESS with no evidence should be ~N, got {}",
            ess
        );
    }

    #[test]
    fn lw_sampling_weights_correct() {
        let net = load_asia();
        let mut evidence = Evidence::new();
        evidence.observe(&net, &var_name("Smoking"), "yes").unwrap();

        let sampler = LikelihoodWeightedSampler::new(1_000).with_seed(42);
        let result = sampler.infer(&net, &evidence).unwrap();
        let ess = result
            .diagnostics
            .as_ref()
            .unwrap()
            .effective_sample_size
            .unwrap();

        // With Smoking=yes (P=0.5), ESS should be ~N (all weights are the same 0.5)
        // since evidence is a root node with uniform conditional
        assert!(
            ess > 500.0,
            "ESS with single root evidence should be reasonable, got {}",
            ess
        );
    }

    #[test]
    fn lw_sampling_no_evidence() {
        let net = load_asia();
        let evidence = Evidence::new();
        let sampler = LikelihoodWeightedSampler::new(50_000).with_seed(42);

        let result = sampler.infer(&net, &evidence).unwrap();
        assert_eq!(result.marginals.len(), 8);

        // Compare against VE exact prior marginals
        let ve = VariableElimination::new();
        let exact = ve.infer(&net, &evidence).unwrap();

        for (vn, exact_probs) in &exact.marginals {
            let sampled_probs = result.marginals.get(vn).unwrap();
            for (ep, sp) in exact_probs.iter().zip(sampled_probs.iter()) {
                assert!(
                    (ep - sp).abs() < 0.02,
                    "LW prior P({})={} vs exact {}",
                    vn,
                    sp,
                    ep
                );
            }
        }
    }
}
