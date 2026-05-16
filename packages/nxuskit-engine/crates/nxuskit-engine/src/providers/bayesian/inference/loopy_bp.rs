//! Loopy Belief Propagation (LBP) inference algorithm.
//!
//! LBP is a message-passing algorithm for approximate inference on networks
//! with cycles (or large networks where exact inference is intractable).
//! Messages are passed between variable nodes and factor nodes iteratively
//! until convergence or a maximum iteration limit is reached.

use std::collections::HashMap;

use super::{InferenceDiagnostics, InferenceEngine, InferenceResult, Marginal};
use crate::providers::bayesian::error::{BayesError, BayesResult};
use crate::providers::bayesian::evidence::Evidence;
use crate::providers::bayesian::factor::Factor;
use crate::providers::bayesian::network::BayesianNetwork;
use crate::providers::bayesian::stream::{BayesStream, BayesStreamChunk};
use crate::providers::bayesian::types::VariableName;

/// Configuration for Loopy Belief Propagation.
#[derive(Debug, Clone)]
pub struct LBPConfig {
    /// Maximum number of message-passing iterations.
    pub max_iterations: usize,
    /// Convergence threshold: stop when max message delta < this value.
    pub convergence_threshold: f64,
    /// Damping factor (0.0 = no damping, 1.0 = full damping / no update).
    pub damping_factor: f64,
}

impl Default for LBPConfig {
    fn default() -> Self {
        Self {
            max_iterations: 100,
            convergence_threshold: 1e-4,
            damping_factor: 0.5,
        }
    }
}

/// Loopy Belief Propagation inference engine.
///
/// Performs approximate inference via iterative message passing on the
/// factor graph representation of a Bayesian network.
#[derive(Debug, Clone)]
pub struct LoopyBeliefPropagation {
    config: LBPConfig,
}

impl LoopyBeliefPropagation {
    /// Create a new LBP engine with default configuration.
    pub fn new() -> Self {
        Self {
            config: LBPConfig::default(),
        }
    }

    /// Set the damping factor.
    pub fn damping(mut self, factor: f64) -> Self {
        self.config.damping_factor = factor;
        self
    }

    /// Set the maximum iterations.
    pub fn max_iterations(mut self, max: usize) -> Self {
        self.config.max_iterations = max;
        self
    }

    /// Set the convergence threshold.
    pub fn convergence_threshold(mut self, threshold: f64) -> Self {
        self.config.convergence_threshold = threshold;
        self
    }

    /// Run LBP and return marginals plus diagnostics.
    fn run_lbp(
        &self,
        network: &BayesianNetwork,
        evidence: &Evidence,
    ) -> BayesResult<(HashMap<VariableName, Marginal>, InferenceDiagnostics)> {
        network.validate()?;

        let variables = network.variable_names();
        let discrete_vars: Vec<_> = variables
            .iter()
            .filter(|v| network.is_discrete(v))
            .collect();

        if discrete_vars.is_empty() {
            return Err(BayesError::InferenceError(
                "LBP requires at least one discrete variable".into(),
            ));
        }

        // Build factors from CPTs, reducing by evidence
        let mut factors: Vec<Factor> = Vec::new();
        for var in &discrete_vars {
            if let Some(cpt) = network.cpt(var) {
                let mut factor = cpt.clone();
                for (ev_var, ev_state) in evidence.observations() {
                    if factor.variables.contains(&ev_var) {
                        factor = factor.reduce(&ev_var, ev_state)?;
                    }
                }
                factors.push(factor);
            }
        }

        // Collect all variable names that appear in any factor (unobserved)
        let mut all_vars: Vec<VariableName> = Vec::new();
        let mut var_cardinalities: HashMap<VariableName, usize> = HashMap::new();
        for factor in &factors {
            for (i, var) in factor.variables.iter().enumerate() {
                if !var_cardinalities.contains_key(var) {
                    all_vars.push(var.clone());
                    var_cardinalities.insert(var.clone(), factor.cardinalities[i]);
                }
            }
        }

        // Initialize messages: factor→variable and variable→factor
        // Messages are stored as probability vectors (not log-space) for simplicity.
        // factor_to_var[(factor_idx, var)] = message from factor to variable
        // var_to_factor[(var, factor_idx)] = message from variable to factor
        let mut factor_to_var: HashMap<(usize, VariableName), Vec<f64>> = HashMap::new();
        let mut var_to_factor: HashMap<(VariableName, usize), Vec<f64>> = HashMap::new();

        // Initialize all messages to uniform
        for (f_idx, factor) in factors.iter().enumerate() {
            for (v_idx, var) in factor.variables.iter().enumerate() {
                let card = factor.cardinalities[v_idx];
                let uniform = vec![1.0 / card as f64; card];
                factor_to_var.insert((f_idx, var.clone()), uniform.clone());
                var_to_factor.insert((var.clone(), f_idx), uniform);
            }
        }

        let mut max_delta = f64::MAX;
        let mut converged = false;
        let mut iterations = 0;

        for iter in 0..self.config.max_iterations {
            iterations = iter + 1;
            max_delta = 0.0;

            // --- Variable-to-factor messages ---
            for (f_idx, factor) in factors.iter().enumerate() {
                for var in &factor.variables {
                    let card = *var_cardinalities.get(var).unwrap();
                    let mut msg = vec![1.0; card];

                    // Product of all incoming factor→var messages EXCEPT from this factor
                    for (other_f_idx, other_factor) in factors.iter().enumerate() {
                        if other_f_idx == f_idx {
                            continue;
                        }
                        if other_factor.variables.contains(var)
                            && let Some(incoming) = factor_to_var.get(&(other_f_idx, var.clone()))
                        {
                            for (i, m) in msg.iter_mut().enumerate() {
                                if i < incoming.len() {
                                    *m *= incoming[i];
                                }
                            }
                        }
                    }

                    // Normalize
                    let sum: f64 = msg.iter().sum();
                    if sum > 0.0 {
                        for m in &mut msg {
                            *m /= sum;
                        }
                    }

                    var_to_factor.insert((var.clone(), f_idx), msg);
                }
            }

            // --- Factor-to-variable messages ---
            for (f_idx, factor) in factors.iter().enumerate() {
                for (target_v_idx, target_var) in factor.variables.iter().enumerate() {
                    let target_card = factor.cardinalities[target_v_idx];
                    let mut new_msg = vec![0.0; target_card];

                    // Sum-product: for each state of target variable, sum over
                    // all assignments to other variables in this factor's scope
                    let factor_size = factor.size();
                    for entry_idx in 0..factor_size {
                        let assignment = factor.index_to_assignment(entry_idx);
                        let target_state = assignment[target_v_idx].value();

                        // Factor potential (convert from log-space)
                        let potential = factor.log_values[entry_idx].exp();

                        // Product of incoming variable→factor messages for other variables
                        let mut product = potential;
                        for (v_idx, var) in factor.variables.iter().enumerate() {
                            if v_idx == target_v_idx {
                                continue;
                            }
                            if let Some(incoming) = var_to_factor.get(&(var.clone(), f_idx)) {
                                let state = assignment[v_idx].value();
                                if state < incoming.len() {
                                    product *= incoming[state];
                                }
                            }
                        }

                        new_msg[target_state] += product;
                    }

                    // Normalize
                    let sum: f64 = new_msg.iter().sum();
                    if sum > 0.0 {
                        for m in &mut new_msg {
                            *m /= sum;
                        }
                    }

                    // Apply damping
                    let key = (f_idx, target_var.clone());
                    if let Some(old_msg) = factor_to_var.get(&key) {
                        let d = self.config.damping_factor;
                        for (i, m) in new_msg.iter_mut().enumerate() {
                            if i < old_msg.len() {
                                let old = old_msg[i];
                                let delta = (*m - old).abs();
                                if delta > max_delta {
                                    max_delta = delta;
                                }
                                *m = d * old + (1.0 - d) * *m;
                            }
                        }
                    }

                    factor_to_var.insert(key, new_msg);
                }
            }

            if max_delta < self.config.convergence_threshold {
                converged = true;
                break;
            }
        }

        // Compute beliefs (marginals) by combining all incoming messages
        let mut marginals: HashMap<VariableName, Marginal> = HashMap::new();
        for var in &all_vars {
            if evidence.is_observed(var) {
                continue;
            }
            let card = *var_cardinalities.get(var).unwrap();
            let mut belief = vec![1.0; card];

            for (f_idx, factor) in factors.iter().enumerate() {
                if factor.variables.contains(var)
                    && let Some(msg) = factor_to_var.get(&(f_idx, var.clone()))
                {
                    for (i, b) in belief.iter_mut().enumerate() {
                        if i < msg.len() {
                            *b *= msg[i];
                        }
                    }
                }
            }

            // Normalize
            let sum: f64 = belief.iter().sum();
            if sum > 0.0 {
                for b in &mut belief {
                    *b /= sum;
                }
            }

            marginals.insert(var.clone(), belief);
        }

        let diagnostics = InferenceDiagnostics {
            iterations,
            burn_in: 0,
            max_marginal_change: max_delta,
            effective_sample_size: None,
        };

        if !converged {
            // Non-convergence: still return best-effort marginals with warning diagnostic
            // The max_marginal_change field indicates how far from convergence we are
        }

        Ok((marginals, diagnostics))
    }

    /// Run LBP with streaming support, emitting intermediate results.
    pub fn infer_stream(
        &self,
        network: &BayesianNetwork,
        evidence: &Evidence,
        chunk_interval: usize,
    ) -> BayesResult<BayesStream<InferenceResult>> {
        let network = network.clone();
        let evidence = evidence.clone();
        let config = self.config.clone();
        let chunk_interval = if chunk_interval == 0 {
            10
        } else {
            chunk_interval
        };

        let (tx, rx) = tokio::sync::mpsc::channel(32);

        tokio::spawn(async move {
            let _ = run_lbp_streaming(&network, &evidence, &config, chunk_interval, tx).await;
        });

        Ok(BayesStream::new(rx))
    }
}

impl Default for LoopyBeliefPropagation {
    fn default() -> Self {
        Self::new()
    }
}

impl InferenceEngine for LoopyBeliefPropagation {
    fn infer(
        &self,
        network: &BayesianNetwork,
        evidence: &Evidence,
    ) -> BayesResult<InferenceResult> {
        let start = std::time::Instant::now();
        let (marginals, diagnostics) = self.run_lbp(network, evidence)?;

        let log_marginals = marginals
            .iter()
            .map(|(k, v)| {
                let logs: Vec<f64> = v
                    .iter()
                    .map(|&p| if p <= 0.0 { f64::NEG_INFINITY } else { p.ln() })
                    .collect();
                (k.clone(), logs)
            })
            .collect();

        Ok(InferenceResult {
            marginals,
            log_marginals,
            continuous_marginals: HashMap::new(),
            algorithm: "lbp".to_string(),
            elapsed: start.elapsed(),
            diagnostics: Some(diagnostics),
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
            BayesError::InferenceError(format!("Variable '{}' not found in LBP results", variable))
        })
    }

    fn algorithm_name(&self) -> &str {
        "lbp"
    }
}

/// Internal: run LBP with streaming output.
async fn run_lbp_streaming(
    network: &BayesianNetwork,
    evidence: &Evidence,
    config: &LBPConfig,
    chunk_interval: usize,
    tx: tokio::sync::mpsc::Sender<BayesStreamChunk<InferenceResult>>,
) -> BayesResult<()> {
    network.validate()?;

    let discrete_vars: Vec<_> = network
        .variable_names()
        .into_iter()
        .filter(|v| network.is_discrete(v))
        .collect();

    // Build factors from CPTs
    let mut factors: Vec<Factor> = Vec::new();
    for var in &discrete_vars {
        if let Some(cpt) = network.cpt(var) {
            let mut factor = cpt.clone();
            for (ev_var, ev_state) in evidence.observations() {
                if factor.variables.contains(&ev_var) {
                    factor = factor.reduce(&ev_var, ev_state)?;
                }
            }
            factors.push(factor);
        }
    }

    let mut all_vars: Vec<VariableName> = Vec::new();
    let mut var_cardinalities: HashMap<VariableName, usize> = HashMap::new();
    for factor in &factors {
        for (i, var) in factor.variables.iter().enumerate() {
            if !var_cardinalities.contains_key(var) {
                all_vars.push(var.clone());
                var_cardinalities.insert(var.clone(), factor.cardinalities[i]);
            }
        }
    }

    let mut factor_to_var: HashMap<(usize, VariableName), Vec<f64>> = HashMap::new();
    let mut var_to_factor: HashMap<(VariableName, usize), Vec<f64>> = HashMap::new();

    for (f_idx, factor) in factors.iter().enumerate() {
        for (v_idx, var) in factor.variables.iter().enumerate() {
            let card = factor.cardinalities[v_idx];
            let uniform = vec![1.0 / card as f64; card];
            factor_to_var.insert((f_idx, var.clone()), uniform.clone());
            var_to_factor.insert((var.clone(), f_idx), uniform);
        }
    }

    let start = std::time::Instant::now();

    for iter in 0..config.max_iterations {
        let mut max_delta = 0.0;

        // Variable-to-factor messages
        for (f_idx, factor) in factors.iter().enumerate() {
            for var in &factor.variables {
                let card = *var_cardinalities.get(var).unwrap();
                let mut msg = vec![1.0; card];
                for (other_f_idx, other_factor) in factors.iter().enumerate() {
                    if other_f_idx == f_idx {
                        continue;
                    }
                    if other_factor.variables.contains(var)
                        && let Some(incoming) = factor_to_var.get(&(other_f_idx, var.clone()))
                    {
                        for (i, m) in msg.iter_mut().enumerate() {
                            if i < incoming.len() {
                                *m *= incoming[i];
                            }
                        }
                    }
                }
                let sum: f64 = msg.iter().sum();
                if sum > 0.0 {
                    for m in &mut msg {
                        *m /= sum;
                    }
                }
                var_to_factor.insert((var.clone(), f_idx), msg);
            }
        }

        // Factor-to-variable messages
        for (f_idx, factor) in factors.iter().enumerate() {
            for (target_v_idx, target_var) in factor.variables.iter().enumerate() {
                let target_card = factor.cardinalities[target_v_idx];
                let mut new_msg = vec![0.0; target_card];

                for entry_idx in 0..factor.size() {
                    let assignment = factor.index_to_assignment(entry_idx);
                    let target_state = assignment[target_v_idx].value();
                    let potential = factor.log_values[entry_idx].exp();
                    let mut product = potential;
                    for (v_idx, var) in factor.variables.iter().enumerate() {
                        if v_idx == target_v_idx {
                            continue;
                        }
                        if let Some(incoming) = var_to_factor.get(&(var.clone(), f_idx)) {
                            let state = assignment[v_idx].value();
                            if state < incoming.len() {
                                product *= incoming[state];
                            }
                        }
                    }
                    new_msg[target_state] += product;
                }

                let sum: f64 = new_msg.iter().sum();
                if sum > 0.0 {
                    for m in &mut new_msg {
                        *m /= sum;
                    }
                }

                let key = (f_idx, target_var.clone());
                if let Some(old_msg) = factor_to_var.get(&key) {
                    let d = config.damping_factor;
                    for (i, m) in new_msg.iter_mut().enumerate() {
                        if i < old_msg.len() {
                            let delta = (*m - old_msg[i]).abs();
                            if delta > max_delta {
                                max_delta = delta;
                            }
                            *m = d * old_msg[i] + (1.0 - d) * *m;
                        }
                    }
                }

                factor_to_var.insert(key, new_msg);
            }
        }

        // Emit streaming chunk if it's time
        if (iter + 1) % chunk_interval == 0
            || iter + 1 == config.max_iterations
            || max_delta < config.convergence_threshold
        {
            // Compute current beliefs
            let mut marginals: HashMap<VariableName, Marginal> = HashMap::new();
            for var in &all_vars {
                if evidence.is_observed(var) {
                    continue;
                }
                let card = *var_cardinalities.get(var).unwrap();
                let mut belief = vec![1.0; card];
                for (f_idx, factor) in factors.iter().enumerate() {
                    if factor.variables.contains(var)
                        && let Some(msg) = factor_to_var.get(&(f_idx, var.clone()))
                    {
                        for (i, b) in belief.iter_mut().enumerate() {
                            if i < msg.len() {
                                *b *= msg[i];
                            }
                        }
                    }
                }
                let sum: f64 = belief.iter().sum();
                if sum > 0.0 {
                    for b in &mut belief {
                        *b /= sum;
                    }
                }
                marginals.insert(var.clone(), belief);
            }

            let is_final =
                max_delta < config.convergence_threshold || iter + 1 == config.max_iterations;

            let result = InferenceResult {
                marginals,
                log_marginals: HashMap::new(),
                continuous_marginals: HashMap::new(),
                algorithm: "lbp".to_string(),
                elapsed: start.elapsed(),
                diagnostics: Some(InferenceDiagnostics {
                    iterations: iter + 1,
                    burn_in: 0,
                    max_marginal_change: max_delta,
                    effective_sample_size: None,
                }),
                nuts_diagnostics: None,
            };

            let chunk = BayesStreamChunk {
                data: result,
                is_final,
                iteration: iter + 1,
                total_iterations: config.max_iterations,
                convergence_metric: max_delta,
            };

            if tx.send(chunk).await.is_err() {
                break; // Receiver dropped
            }

            if is_final {
                break;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::bayesian::bif::load_bif_file;
    use crate::providers::bayesian::inference::VariableElimination;
    use std::path::PathBuf;

    fn fixture_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("bn")
            .join(name)
    }

    #[test]
    fn lbp_config_defaults() {
        let config = LBPConfig::default();
        assert_eq!(config.max_iterations, 100);
        assert!((config.convergence_threshold - 1e-4).abs() < 1e-10);
        assert!((config.damping_factor - 0.5).abs() < 1e-10);
    }

    #[test]
    fn lbp_builder_pattern() {
        let lbp = LoopyBeliefPropagation::new()
            .damping(0.3)
            .max_iterations(50)
            .convergence_threshold(1e-6);
        assert_eq!(lbp.config.max_iterations, 50);
        assert!((lbp.config.damping_factor - 0.3).abs() < 1e-10);
        assert!((lbp.config.convergence_threshold - 1e-6).abs() < 1e-10);
    }

    #[test]
    fn lbp_cancer_network() {
        let network = load_bif_file(&fixture_path("cancer.bif")).unwrap();
        let evidence = Evidence::new();

        let lbp = LoopyBeliefPropagation::new()
            .damping(0.5)
            .max_iterations(100)
            .convergence_threshold(1e-6);

        let result = lbp.infer(&network, &evidence).unwrap();
        assert!(!result.marginals.is_empty());
        assert_eq!(result.algorithm, "lbp");

        // Compare against exact inference (VE)
        let ve = VariableElimination::new();
        let exact = ve.infer(&network, &evidence).unwrap();

        // RMSE should be very small for a tree-structured network (LBP is exact on trees)
        let mut squared_errors = Vec::new();
        for (var, lbp_marginal) in &result.marginals {
            if let Some(exact_marginal) = exact.marginals.get(var) {
                for (i, &lbp_p) in lbp_marginal.iter().enumerate() {
                    let exact_p = exact_marginal[i];
                    squared_errors.push((lbp_p - exact_p).powi(2));
                }
            }
        }
        let rmse = (squared_errors.iter().sum::<f64>() / squared_errors.len() as f64).sqrt();
        assert!(
            rmse < 0.02,
            "LBP RMSE vs VE on Cancer: {:.6} (expected < 0.02)",
            rmse
        );
    }

    #[test]
    fn lbp_alarm_network_rmse() {
        let network = load_bif_file(&fixture_path("alarm.bif")).unwrap();
        let evidence = Evidence::new();

        let lbp = LoopyBeliefPropagation::new()
            .damping(0.5)
            .max_iterations(200)
            .convergence_threshold(1e-6);

        let result = lbp.infer(&network, &evidence).unwrap();

        // Compare against VE for RMSE
        let ve = VariableElimination::new();
        let exact = ve.infer(&network, &evidence).unwrap();

        let mut squared_errors = Vec::new();
        for (var, lbp_marginal) in &result.marginals {
            if let Some(exact_marginal) = exact.marginals.get(var) {
                for (i, &lbp_p) in lbp_marginal.iter().enumerate() {
                    if i < exact_marginal.len() {
                        let exact_p = exact_marginal[i];
                        squared_errors.push((lbp_p - exact_p).powi(2));
                    }
                }
            }
        }
        let rmse = (squared_errors.iter().sum::<f64>() / squared_errors.len() as f64).sqrt();
        assert!(
            rmse < 0.02,
            "LBP RMSE vs VE on Alarm: {:.6} (expected < 0.02)",
            rmse
        );
    }

    #[test]
    fn lbp_with_evidence() {
        let network = load_bif_file(&fixture_path("cancer.bif")).unwrap();
        let mut evidence = Evidence::new();
        let pollution = VariableName::new("Pollution").unwrap();
        evidence.observe(&network, &pollution, "low").unwrap();

        let lbp = LoopyBeliefPropagation::new()
            .damping(0.5)
            .max_iterations(100);

        let result = lbp.infer(&network, &evidence).unwrap();
        // Observed variable should not be in marginals
        assert!(!result.marginals.contains_key(&pollution));
    }

    #[test]
    fn lbp_convergence_diagnostics() {
        let network = load_bif_file(&fixture_path("cancer.bif")).unwrap();
        let evidence = Evidence::new();

        let lbp = LoopyBeliefPropagation::new()
            .damping(0.5)
            .max_iterations(100)
            .convergence_threshold(1e-6);

        let result = lbp.infer(&network, &evidence).unwrap();
        let diag = result.diagnostics.unwrap();
        assert!(diag.iterations > 0);
        assert!(diag.iterations <= 100);
        // Should converge on tree-structured Cancer network
        assert!(
            diag.max_marginal_change < 1e-4,
            "Expected convergence, max_delta={:.6}",
            diag.max_marginal_change
        );
    }

    #[test]
    fn lbp_non_convergence_returns_best_effort() {
        let network = load_bif_file(&fixture_path("alarm.bif")).unwrap();
        let evidence = Evidence::new();

        // Very few iterations + very tight threshold = likely won't converge
        let lbp = LoopyBeliefPropagation::new()
            .damping(0.1)
            .max_iterations(2)
            .convergence_threshold(1e-20);

        let result = lbp.infer(&network, &evidence).unwrap();
        let diag = result.diagnostics.unwrap();
        assert_eq!(diag.iterations, 2); // Hit max iterations
        // Should still return marginals even without convergence
        assert!(!result.marginals.is_empty());
    }

    #[test]
    fn lbp_algorithm_name() {
        let lbp = LoopyBeliefPropagation::new();
        assert_eq!(lbp.algorithm_name(), "lbp");
    }

    #[test]
    fn lbp_query_single_variable() {
        let network = load_bif_file(&fixture_path("cancer.bif")).unwrap();
        let evidence = Evidence::new();

        let lbp = LoopyBeliefPropagation::new();
        let cancer = VariableName::new("Cancer").unwrap();
        let marginal = lbp.query(&network, &evidence, &cancer).unwrap();
        assert_eq!(marginal.len(), 2); // binary variable
        let sum: f64 = marginal.iter().sum();
        assert!((sum - 1.0).abs() < 1e-6, "Marginal should sum to 1.0");
    }
}
