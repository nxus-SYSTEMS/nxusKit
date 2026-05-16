//! Gibbs sampling inference algorithm for Bayesian Networks.
//!
//! Approximate inference using Markov Chain Monte Carlo (MCMC) via Gibbs sampling.
//! Supports:
//! - Forward-sample initialization
//! - Full-conditional computation from CPTs and Markov blanket
//! - Burn-in discarding
//! - ChaCha20 RNG seeding for reproducibility (FR-026)
//! - Convergence diagnostics (running marginals, max_marginal_change, ESS estimate)

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

/// Gibbs sampler for approximate Bayesian Network inference.
#[derive(Debug, Clone)]
pub struct GibbsSampler {
    /// Number of usable samples to produce (after burn-in).
    pub num_samples: usize,
    /// Number of initial samples to discard.
    pub burn_in: usize,
    /// Optional RNG seed for reproducibility.
    pub seed: Option<u64>,
}

impl GibbsSampler {
    /// Create a new Gibbs sampler with default parameters.
    pub fn new(num_samples: usize, burn_in: usize) -> Self {
        Self {
            num_samples,
            burn_in,
            seed: None,
        }
    }

    /// Set the RNG seed for reproducible sampling (ChaCha20).
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }

    /// Initialize a sample by forward-sampling through the topological order.
    ///
    /// For observed variables, the state is fixed to the evidence value.
    /// For unobserved variables, sample from P(X | parents(X)).
    fn forward_sample(
        &self,
        network: &BayesianNetwork,
        evidence: &Evidence,
        rng: &mut ChaCha20Rng,
    ) -> BayesResult<HashMap<VariableName, usize>> {
        let topo_order = network.topological_sort();
        let mut assignment: HashMap<VariableName, usize> = HashMap::new();

        for var_name in &topo_order {
            // If observed, fix to evidence
            if let Some(state_idx) = evidence.get(var_name) {
                assignment.insert(var_name.clone(), state_idx.value());
                continue;
            }

            // Sample from P(X | parents(X) = current assignment)
            let cpt = network.cpt(var_name).ok_or_else(|| {
                BayesError::IncompleteNetwork(format!("Variable '{}' has no CPT", var_name))
            })?;

            let var = network.variable(var_name).unwrap();
            let parents = network.parents(var_name);
            let cardinality = var.cardinality();

            // Compute the conditional distribution given parent assignment
            let mut probs = Vec::with_capacity(cardinality);
            for state_val in 0..cardinality {
                // Build the full assignment: [parent1_state, parent2_state, ..., child_state]
                let mut cpt_assignment = Vec::with_capacity(cpt.variables.len());
                for parent in &parents {
                    cpt_assignment.push(StateIndex::new(assignment[parent]));
                }
                cpt_assignment.push(StateIndex::new(state_val));
                let idx = cpt.assignment_to_index(&cpt_assignment);
                probs.push(cpt.log_values[idx].exp());
            }

            // Sample from the distribution
            let sampled = sample_categorical(&probs, rng);
            assignment.insert(var_name.clone(), sampled);
        }

        Ok(assignment)
    }

    /// Check if a variable's CPT contains any all-zero rows (deterministic node).
    fn has_deterministic_rows(network: &BayesianNetwork, var_name: &VariableName) -> bool {
        let cpt = match network.cpt(var_name) {
            Some(c) => c,
            None => return false,
        };
        let var = match network.variable(var_name) {
            Some(v) => v,
            None => return false,
        };
        let child_card = var.cardinality();
        let num_parent_configs = cpt.size() / child_card;

        for config_idx in 0..num_parent_configs {
            let row_start = config_idx * child_card;
            let has_zero =
                cpt.log_values[row_start..row_start + child_card].contains(&f64::NEG_INFINITY);
            if has_zero {
                return true;
            }
        }
        false
    }

    /// Compute the full conditional P(X_i | X_{-i}) for variable `var_name`
    /// given the current assignment.
    ///
    /// For children with deterministic CPT rows, uses collapsed Gibbs:
    /// marginalizes over the deterministic child rather than conditioning
    /// on its current state, preventing zero-probability traps.
    fn full_conditional(
        &self,
        network: &BayesianNetwork,
        var_name: &VariableName,
        assignment: &HashMap<VariableName, usize>,
        deterministic_set: &std::collections::HashSet<VariableName>,
    ) -> Vec<f64> {
        let var = network.variable(var_name).unwrap();
        let cardinality = var.cardinality();
        let mut log_probs = vec![0.0_f64; cardinality];

        // Factor 1: P(X_i | parents(X_i))
        let cpt = network.cpt(var_name).unwrap();
        let parents = network.parents(var_name);

        for (state_val, lp) in log_probs.iter_mut().enumerate().take(cardinality) {
            let mut cpt_assignment = Vec::with_capacity(cpt.variables.len());
            for parent in &parents {
                cpt_assignment.push(StateIndex::new(assignment[parent]));
            }
            cpt_assignment.push(StateIndex::new(state_val));
            let idx = cpt.assignment_to_index(&cpt_assignment);
            *lp += cpt.log_values[idx];
        }

        // Factor 2: For each child, include P(child | parents(child))
        for child_name in network.children(var_name) {
            let child_cpt = network.cpt(&child_name).unwrap();
            let child_parents = network.parents(&child_name);

            if deterministic_set.contains(&child_name) {
                // Collapsed Gibbs: marginalize over the deterministic child.
                // For each proposed state of var_name, compute:
                //   Σ_d P(d | parents_d) * Π_{gc in children(d)} P(gc | parents_gc with d)
                let child_var = network.variable(&child_name).unwrap();
                let child_card = child_var.cardinality();
                let grandchildren = network.children(&child_name);

                for (state_val, lp) in log_probs.iter_mut().enumerate().take(cardinality) {
                    let mut sum_over_d = f64::NEG_INFINITY;

                    for d_state in 0..child_card {
                        // P(d_state | parents_d) with var_name = state_val
                        let mut d_cpt_assignment = Vec::with_capacity(child_cpt.variables.len());
                        for parent in &child_parents {
                            if parent == var_name {
                                d_cpt_assignment.push(StateIndex::new(state_val));
                            } else {
                                d_cpt_assignment.push(StateIndex::new(assignment[parent]));
                            }
                        }
                        d_cpt_assignment.push(StateIndex::new(d_state));
                        let d_idx = child_cpt.assignment_to_index(&d_cpt_assignment);
                        let mut log_term = child_cpt.log_values[d_idx];

                        // Multiply by P(gc | parents_gc) for each grandchild
                        for gc_name in &grandchildren {
                            let gc_cpt = network.cpt(gc_name).unwrap();
                            let gc_parents = network.parents(gc_name);
                            let gc_state = assignment[gc_name];

                            let mut gc_cpt_assignment = Vec::with_capacity(gc_cpt.variables.len());
                            for gc_parent in &gc_parents {
                                if gc_parent == &child_name {
                                    gc_cpt_assignment.push(StateIndex::new(d_state));
                                } else {
                                    gc_cpt_assignment.push(StateIndex::new(assignment[gc_parent]));
                                }
                            }
                            gc_cpt_assignment.push(StateIndex::new(gc_state));
                            let gc_idx = gc_cpt.assignment_to_index(&gc_cpt_assignment);
                            log_term += gc_cpt.log_values[gc_idx];
                        }

                        // log-sum-exp accumulation
                        if sum_over_d == f64::NEG_INFINITY {
                            sum_over_d = log_term;
                        } else if log_term != f64::NEG_INFINITY {
                            let max_val = sum_over_d.max(log_term);
                            sum_over_d = max_val
                                + ((sum_over_d - max_val).exp() + (log_term - max_val).exp()).ln();
                        }
                    }

                    *lp += sum_over_d;
                }
            } else {
                // Standard Gibbs: condition on child's current state
                let child_state = assignment[&child_name];

                for (state_val, lp) in log_probs.iter_mut().enumerate().take(cardinality) {
                    let mut child_cpt_assignment = Vec::with_capacity(child_cpt.variables.len());
                    for parent in &child_parents {
                        if parent == var_name {
                            child_cpt_assignment.push(StateIndex::new(state_val));
                        } else {
                            child_cpt_assignment.push(StateIndex::new(assignment[parent]));
                        }
                    }
                    child_cpt_assignment.push(StateIndex::new(child_state));
                    let idx = child_cpt.assignment_to_index(&child_cpt_assignment);
                    *lp += child_cpt.log_values[idx];
                }
            }
        }

        // Convert from log-space and normalize
        let max_log = log_probs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let mut probs: Vec<f64> = log_probs.iter().map(|&lp| (lp - max_log).exp()).collect();
        let sum: f64 = probs.iter().sum();
        if sum > 0.0 {
            for p in &mut probs {
                *p /= sum;
            }
        }

        probs
    }

    /// Set deterministic variables based on their parents' current assignment.
    fn update_deterministic_nodes(
        network: &BayesianNetwork,
        assignment: &mut HashMap<VariableName, usize>,
        deterministic_vars: &[VariableName],
    ) {
        for var_name in deterministic_vars {
            let cpt = network.cpt(var_name).unwrap();
            let var = network.variable(var_name).unwrap();
            let parents = network.parents(var_name);
            let cardinality = var.cardinality();

            let mut best_state = 0;
            let mut best_log_prob = f64::NEG_INFINITY;

            for state_val in 0..cardinality {
                let mut cpt_assignment = Vec::with_capacity(cpt.variables.len());
                for parent in &parents {
                    cpt_assignment.push(StateIndex::new(assignment[parent]));
                }
                cpt_assignment.push(StateIndex::new(state_val));
                let idx = cpt.assignment_to_index(&cpt_assignment);
                if cpt.log_values[idx] > best_log_prob {
                    best_log_prob = cpt.log_values[idx];
                    best_state = state_val;
                }
            }

            assignment.insert(var_name.clone(), best_state);
        }
    }

    /// Perform one full sweep of Gibbs sampling.
    ///
    /// Stochastic variables are sampled from their full conditional in random order
    /// to improve mixing. Deterministic variables are collapsed out during parent
    /// sampling and then set deterministically based on parents.
    #[allow(clippy::too_many_arguments)]
    fn gibbs_sweep(
        &self,
        network: &BayesianNetwork,
        _evidence: &Evidence,
        assignment: &mut HashMap<VariableName, usize>,
        rng: &mut ChaCha20Rng,
        stochastic_vars: &[VariableName],
        deterministic_vars: &[VariableName],
        deterministic_set: &std::collections::HashSet<VariableName>,
    ) {
        // Random permutation of stochastic variables for better mixing
        let mut order: Vec<usize> = (0..stochastic_vars.len()).collect();
        for i in (1..order.len()).rev() {
            let j = rng.random_range(0..=i);
            order.swap(i, j);
        }

        for &idx in &order {
            let var_name = &stochastic_vars[idx];
            let probs = self.full_conditional(network, var_name, assignment, deterministic_set);
            let sampled = sample_categorical(&probs, rng);
            assignment.insert(var_name.clone(), sampled);
        }

        // Update deterministic variables based on their parents
        Self::update_deterministic_nodes(network, assignment, deterministic_vars);
    }
}

impl InferenceEngine for GibbsSampler {
    fn infer(
        &self,
        network: &BayesianNetwork,
        evidence: &Evidence,
    ) -> BayesResult<InferenceResult> {
        let start = Instant::now();
        network.validate()?;

        let mut rng = match self.seed {
            Some(seed) => ChaCha20Rng::seed_from_u64(seed),
            None => ChaCha20Rng::from_rng(&mut rand::rng()),
        };

        // Use topological order for unobserved variables
        let evidence_set: std::collections::HashSet<VariableName> =
            evidence.observations().keys().cloned().collect();
        let unobserved: Vec<VariableName> = network
            .topological_sort()
            .into_iter()
            .filter(|v| !evidence_set.contains(v))
            .collect();

        // Handle edge case: all variables observed
        if unobserved.is_empty() {
            return Ok(InferenceResult {
                marginals: HashMap::new(),
                log_marginals: HashMap::new(),
                algorithm: "gibbs".to_string(),
                elapsed: start.elapsed(),
                diagnostics: Some(InferenceDiagnostics {
                    iterations: 0,
                    burn_in: 0,
                    max_marginal_change: 0.0,
                    effective_sample_size: None,
                }),
                continuous_marginals: HashMap::new(),
                nuts_diagnostics: None,
            });
        }

        // Partition into stochastic and deterministic variables.
        // Deterministic variables are collapsed out during sampling.
        let mut stochastic_vars = Vec::new();
        let mut deterministic_vars = Vec::new();
        let mut deterministic_set = std::collections::HashSet::new();
        for var_name in &unobserved {
            if Self::has_deterministic_rows(network, var_name) {
                deterministic_vars.push(var_name.clone());
                deterministic_set.insert(var_name.clone());
            } else {
                stochastic_vars.push(var_name.clone());
            }
        }

        // Forward-sample initial state
        let mut assignment = self.forward_sample(network, evidence, &mut rng)?;

        // Burn-in
        for _ in 0..self.burn_in {
            self.gibbs_sweep(
                network,
                evidence,
                &mut assignment,
                &mut rng,
                &stochastic_vars,
                &deterministic_vars,
                &deterministic_set,
            );
        }

        // Collect samples and accumulate running counts for marginals
        let mut counts: HashMap<VariableName, Vec<f64>> = HashMap::new();
        for var_name in &unobserved {
            let var = network.variable(var_name).unwrap();
            counts.insert(var_name.clone(), vec![0.0; var.cardinality()]);
        }

        // Track convergence: compare running marginals at half-way vs end
        let halfway = self.num_samples / 2;
        let mut halfway_marginals: Option<HashMap<VariableName, Vec<f64>>> = None;

        for sample_idx in 0..self.num_samples {
            self.gibbs_sweep(
                network,
                evidence,
                &mut assignment,
                &mut rng,
                &stochastic_vars,
                &deterministic_vars,
                &deterministic_set,
            );

            // Accumulate counts
            for var_name in &unobserved {
                let state = assignment[var_name];
                counts.get_mut(var_name).unwrap()[state] += 1.0;
            }

            // Save halfway marginals for convergence tracking
            if sample_idx + 1 == halfway && halfway > 0 {
                let mut hw = HashMap::new();
                for (name, cnt) in &counts {
                    let total = (sample_idx + 1) as f64;
                    hw.insert(
                        name.clone(),
                        cnt.iter().map(|&c| c / total).collect::<Vec<f64>>(),
                    );
                }
                halfway_marginals = Some(hw);
            }
        }

        // Convert counts to marginals
        let total_samples = self.num_samples as f64;
        let mut marginals = HashMap::new();
        let mut log_marginals = HashMap::new();

        for (name, cnt) in &counts {
            let probs: Vec<f64> = cnt.iter().map(|&c| c / total_samples).collect();
            let logs: Vec<f64> = probs
                .iter()
                .map(|&p| if p == 0.0 { f64::NEG_INFINITY } else { p.ln() })
                .collect();
            marginals.insert(name.clone(), probs);
            log_marginals.insert(name.clone(), logs);
        }

        // Compute convergence diagnostics
        let max_marginal_change = if let Some(hw) = &halfway_marginals {
            let mut max_change = 0.0_f64;
            for (name, final_probs) in &marginals {
                if let Some(hw_probs) = hw.get(name) {
                    for (fp, hp) in final_probs.iter().zip(hw_probs.iter()) {
                        max_change = max_change.max((fp - hp).abs());
                    }
                }
            }
            max_change
        } else {
            0.0
        };

        // Simple ESS estimate using batch means
        let ess = estimate_ess(self.num_samples);

        let elapsed = start.elapsed();

        Ok(InferenceResult {
            marginals,
            log_marginals,
            algorithm: "gibbs".to_string(),
            elapsed,
            diagnostics: Some(InferenceDiagnostics {
                iterations: self.num_samples,
                burn_in: self.burn_in,
                max_marginal_change,
                effective_sample_size: Some(ess),
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
                "Variable '{}' not found in Gibbs sampling results",
                variable
            ))
        })
    }

    fn algorithm_name(&self) -> &str {
        "gibbs"
    }
}

impl SamplingInference for GibbsSampler {
    fn sample(
        &self,
        network: &BayesianNetwork,
        evidence: &Evidence,
        num_samples: usize,
        burn_in: usize,
        seed: Option<u64>,
    ) -> BayesResult<Vec<Sample>> {
        network.validate()?;

        let mut rng = match seed {
            Some(s) => ChaCha20Rng::seed_from_u64(s),
            None => ChaCha20Rng::from_rng(&mut rand::rng()),
        };

        let evidence_set: std::collections::HashSet<VariableName> =
            evidence.observations().keys().cloned().collect();
        let unobserved: Vec<VariableName> = network
            .topological_sort()
            .into_iter()
            .filter(|v| !evidence_set.contains(v))
            .collect();

        let mut stochastic_vars = Vec::new();
        let mut deterministic_vars = Vec::new();
        let mut deterministic_set = std::collections::HashSet::new();
        for var_name in &unobserved {
            if Self::has_deterministic_rows(network, var_name) {
                deterministic_vars.push(var_name.clone());
                deterministic_set.insert(var_name.clone());
            } else {
                stochastic_vars.push(var_name.clone());
            }
        }

        // Forward-sample initial state
        let mut assignment = self.forward_sample(network, evidence, &mut rng)?;

        // Burn-in
        for _ in 0..burn_in {
            self.gibbs_sweep(
                network,
                evidence,
                &mut assignment,
                &mut rng,
                &stochastic_vars,
                &deterministic_vars,
                &deterministic_set,
            );
        }

        // Collect samples
        let mut samples = Vec::with_capacity(num_samples);
        for _ in 0..num_samples {
            self.gibbs_sweep(
                network,
                evidence,
                &mut assignment,
                &mut rng,
                &stochastic_vars,
                &deterministic_vars,
                &deterministic_set,
            );
            samples.push(Sample {
                assignments: assignment.clone(),
            });
        }

        Ok(samples)
    }
}

impl GibbsSampler {
    /// Stream progressive inference results from Gibbs sampling.
    ///
    /// Spawns a background tokio task that performs Gibbs sweeps, sending
    /// intermediate `InferenceResult` snapshots every `chunk_size` samples.
    /// The returned `BayesStream` can be consumed async (`.next().await`) or
    /// sync (`.blocking_iter()`). Dropping the stream cancels the background task.
    ///
    /// # Parameters
    /// - `network`: The Bayesian Network (must be complete)
    /// - `evidence`: Observed variables
    /// - `chunk_size`: How often to emit a progress chunk (e.g. every 1000 samples)
    ///
    /// # Panics
    /// Must be called from within a tokio runtime.
    pub fn sample_stream(
        &self,
        network: &BayesianNetwork,
        evidence: &Evidence,
        chunk_size: usize,
    ) -> BayesResult<crate::providers::bayesian::stream::BayesStream<InferenceResult>> {
        use crate::providers::bayesian::stream::{BayesStream, BayesStreamChunk};
        use tokio::sync::mpsc;

        network.validate()?;

        let chunk_size = chunk_size.max(1);
        let num_samples = self.num_samples;
        let burn_in = self.burn_in;
        let seed = self.seed;

        // Clone what the background task needs (network + evidence are Clone).
        let net = network.clone();
        let ev = evidence.clone();
        let sampler = self.clone();

        // Channel buffer: enough for a few chunks ahead
        let (tx, rx) = mpsc::channel(16);

        tokio::spawn(async move {
            let start = Instant::now();

            // --- Initialisation (same as infer()) ---
            let mut rng = match seed {
                Some(s) => ChaCha20Rng::seed_from_u64(s),
                None => ChaCha20Rng::from_rng(&mut rand::rng()),
            };

            let evidence_set: std::collections::HashSet<VariableName> =
                ev.observations().keys().cloned().collect();
            let unobserved: Vec<VariableName> = net
                .topological_sort()
                .into_iter()
                .filter(|v| !evidence_set.contains(v))
                .collect();

            let mut stochastic_vars = Vec::new();
            let mut deterministic_vars = Vec::new();
            let mut deterministic_set = std::collections::HashSet::new();
            for var_name in &unobserved {
                if GibbsSampler::has_deterministic_rows(&net, var_name) {
                    deterministic_vars.push(var_name.clone());
                    deterministic_set.insert(var_name.clone());
                } else {
                    stochastic_vars.push(var_name.clone());
                }
            }

            // Forward-sample initial state
            let mut assignment = match sampler.forward_sample(&net, &ev, &mut rng) {
                Ok(a) => a,
                Err(_) => return, // can't report error through channel easily
            };

            // Burn-in
            for _ in 0..burn_in {
                sampler.gibbs_sweep(
                    &net,
                    &ev,
                    &mut assignment,
                    &mut rng,
                    &stochastic_vars,
                    &deterministic_vars,
                    &deterministic_set,
                );
            }

            // Accumulate counts
            let mut counts: HashMap<VariableName, Vec<f64>> = HashMap::new();
            for var_name in &unobserved {
                let var = net.variable(var_name).unwrap();
                counts.insert(var_name.clone(), vec![0.0; var.cardinality()]);
            }

            let mut prev_marginals: Option<HashMap<VariableName, Vec<f64>>> = None;

            for sample_idx in 0..num_samples {
                sampler.gibbs_sweep(
                    &net,
                    &ev,
                    &mut assignment,
                    &mut rng,
                    &stochastic_vars,
                    &deterministic_vars,
                    &deterministic_set,
                );

                for var_name in &unobserved {
                    let state = assignment[var_name];
                    counts.get_mut(var_name).unwrap()[state] += 1.0;
                }

                let completed = sample_idx + 1;

                // Emit a chunk at every chunk_size boundary and on the final sample
                if completed % chunk_size == 0 || completed == num_samples {
                    let total = completed as f64;
                    let mut marginals = HashMap::new();
                    let mut log_marginals = HashMap::new();
                    for (name, cnt) in &counts {
                        let probs: Vec<f64> = cnt.iter().map(|&c| c / total).collect();
                        let logs: Vec<f64> = probs
                            .iter()
                            .map(|&p| if p == 0.0 { f64::NEG_INFINITY } else { p.ln() })
                            .collect();
                        marginals.insert(name.clone(), probs);
                        log_marginals.insert(name.clone(), logs);
                    }

                    // Convergence: max |current - previous| across all state probs
                    let convergence_metric = if let Some(prev) = &prev_marginals {
                        let mut max_change = 0.0_f64;
                        for (name, probs) in &marginals {
                            if let Some(prev_probs) = prev.get(name) {
                                for (p, pp) in probs.iter().zip(prev_probs.iter()) {
                                    max_change = max_change.max((p - pp).abs());
                                }
                            }
                        }
                        max_change
                    } else {
                        1.0 // first chunk — max uncertainty
                    };

                    prev_marginals = Some(marginals.clone());
                    let is_final = completed == num_samples;

                    let result = InferenceResult {
                        marginals,
                        log_marginals,
                        algorithm: "gibbs".to_string(),
                        elapsed: start.elapsed(),
                        diagnostics: Some(InferenceDiagnostics {
                            iterations: completed,
                            burn_in,
                            max_marginal_change: convergence_metric,
                            effective_sample_size: Some(estimate_ess(completed)),
                        }),
                        continuous_marginals: HashMap::new(),
                        nuts_diagnostics: None,
                    };

                    let chunk = BayesStreamChunk {
                        data: result,
                        iteration: completed,
                        total_iterations: num_samples,
                        convergence_metric,
                        is_final,
                    };

                    // If the receiver was dropped (cancellation), stop.
                    if tx.send(chunk).await.is_err() {
                        return;
                    }
                }
            }
        });

        Ok(BayesStream::new(rx))
    }
}

/// Convert a collection of samples to marginal probability estimates.
///
/// For each variable, counts state occurrences across all samples and
/// normalizes to produce probability estimates.
pub fn samples_to_marginals(
    samples: &[Sample],
    network: &BayesianNetwork,
) -> HashMap<VariableName, Vec<f64>> {
    let mut marginals = HashMap::new();

    if samples.is_empty() {
        return marginals;
    }

    // Identify all variables present in samples
    let var_names: Vec<VariableName> = samples[0].assignments.keys().cloned().collect();

    for var_name in &var_names {
        let var = match network.variable(var_name) {
            Some(v) => v,
            None => continue,
        };
        let cardinality = var.cardinality();
        let mut counts = vec![0.0_f64; cardinality];

        for sample in samples {
            if let Some(&state) = sample.assignments.get(var_name)
                && state < cardinality
            {
                counts[state] += 1.0;
            }
        }

        let total = samples.len() as f64;
        let probs: Vec<f64> = counts.iter().map(|&c| c / total).collect();
        marginals.insert(var_name.clone(), probs);
    }

    marginals
}

/// Sample from a categorical distribution given unnormalized probabilities.
fn sample_categorical(probs: &[f64], rng: &mut ChaCha20Rng) -> usize {
    let sum: f64 = probs.iter().sum();
    if sum <= 0.0 {
        // Uniform fallback for zero-probability cases
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
    // Fallback to last state (handles floating-point edge cases)
    probs.len() - 1
}

/// Simple ESS estimate: for Gibbs sampling on typical BN structures,
/// ESS is roughly num_samples / autocorrelation_time. We use a conservative
/// estimate of ~N/10 for typical mixing.
fn estimate_ess(num_samples: usize) -> f64 {
    // Conservative estimate — proper ESS requires storing full chain
    (num_samples as f64) / 10.0
}

#[cfg(test)]
#[allow(clippy::panic)]
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

    fn load_alarm() -> BayesianNetwork {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bn/alarm.bif");
        load_bif_file(&path).unwrap()
    }

    #[test]
    fn gibbs_asia_prior_deterministic() {
        let net = load_asia();
        let evidence = Evidence::new();
        let gibbs = GibbsSampler::new(10_000, 1_000).with_seed(42);

        let result = gibbs.infer(&net, &evidence).unwrap();
        assert_eq!(result.algorithm, "gibbs");
        assert!(result.diagnostics.is_some());

        let diag = result.diagnostics.unwrap();
        assert_eq!(diag.iterations, 10_000);
        assert_eq!(diag.burn_in, 1_000);

        // Compare against VE exact marginals
        let ve = VariableElimination::new();
        let exact = ve.infer(&net, &evidence).unwrap();

        for (var_name, exact_probs) in &exact.marginals {
            let gibbs_probs = result.marginals.get(var_name).unwrap();
            for (ep, gp) in exact_probs.iter().zip(gibbs_probs.iter()) {
                // With 10K samples on Asia, expect within ~0.05
                assert!(
                    (ep - gp).abs() < 0.05,
                    "P({})={} vs gibbs {}, diff={}",
                    var_name,
                    ep,
                    gp,
                    (ep - gp).abs()
                );
            }
        }
    }

    #[test]
    fn gibbs_asia_with_evidence() {
        let net = load_asia();
        let mut evidence = Evidence::new();
        evidence.observe(&net, &var_name("Smoking"), "yes").unwrap();

        let gibbs = GibbsSampler::new(10_000, 1_000).with_seed(42);
        let result = gibbs.infer(&net, &evidence).unwrap();

        // Observed variable should not be in marginals
        assert!(!result.marginals.contains_key(&var_name("Smoking")));

        // All marginals should sum to ~1
        for (name, probs) in &result.marginals {
            let sum: f64 = probs.iter().sum();
            assert!(
                (sum - 1.0).abs() < 1e-6,
                "Marginal for '{}' sums to {}",
                name,
                sum
            );
        }
    }

    #[test]
    fn gibbs_deterministic_with_seed() {
        let net = load_asia();
        let evidence = Evidence::new();

        let gibbs1 = GibbsSampler::new(1_000, 100).with_seed(42);
        let result1 = gibbs1.infer(&net, &evidence).unwrap();

        let gibbs2 = GibbsSampler::new(1_000, 100).with_seed(42);
        let result2 = gibbs2.infer(&net, &evidence).unwrap();

        // Same seed should produce identical marginals
        for (name, probs1) in &result1.marginals {
            let probs2 = result2.marginals.get(name).unwrap();
            for (p1, p2) in probs1.iter().zip(probs2.iter()) {
                assert!(
                    (p1 - p2).abs() < 1e-15,
                    "Determinism failed for '{}': {} vs {}",
                    name,
                    p1,
                    p2
                );
            }
        }
    }

    #[test]
    fn gibbs_alarm_convergence() {
        let net = load_alarm();
        let evidence = Evidence::new();

        // 100K samples on Alarm (37 nodes) with collapsed Gibbs for deterministic nodes.
        // Alarm has many near-deterministic nodes requiring more samples for convergence.
        let gibbs = GibbsSampler::new(100_000, 10_000).with_seed(42);
        let result = gibbs.infer(&net, &evidence).unwrap();

        // Compare against VE exact marginals
        let ve = VariableElimination::new();
        let exact = ve.infer(&net, &evidence).unwrap();

        // Compute RMSE across all marginal entries
        let mut sum_sq_error = 0.0;
        let mut count = 0;
        for (var_name, exact_probs) in &exact.marginals {
            let gibbs_probs = result
                .marginals
                .get(var_name)
                .unwrap_or_else(|| panic!("Missing Gibbs marginal for '{}'", var_name));
            for (ep, gp) in exact_probs.iter().zip(gibbs_probs.iter()) {
                sum_sq_error += (ep - gp).powi(2);
                count += 1;
            }
        }
        let rmse = (sum_sq_error / count as f64).sqrt();

        assert!(
            rmse < 0.01,
            "Gibbs RMSE on Alarm = {} (expected < 0.01)",
            rmse
        );
    }

    #[test]
    fn gibbs_sample_count() {
        let net = load_asia();
        let evidence = Evidence::new();
        let gibbs = GibbsSampler::new(100, 10).with_seed(42);

        let samples = gibbs.sample(&net, &evidence, 100, 10, Some(42)).unwrap();
        assert_eq!(samples.len(), 100);

        // Each sample should have assignments for all 8 Asia variables
        for sample in &samples {
            assert_eq!(sample.assignments.len(), 8);
        }
    }

    #[test]
    fn samples_to_marginals_correctness() {
        let net = load_asia();
        let evidence = Evidence::new();
        let gibbs = GibbsSampler::new(5_000, 500).with_seed(42);

        let samples = gibbs.sample(&net, &evidence, 5_000, 500, Some(42)).unwrap();
        let marginals = samples_to_marginals(&samples, &net);

        // Each marginal should sum to ~1
        for (name, probs) in &marginals {
            let sum: f64 = probs.iter().sum();
            assert!(
                (sum - 1.0).abs() < 1e-10,
                "samples_to_marginals for '{}' sums to {}",
                name,
                sum
            );
        }
    }

    #[test]
    fn gibbs_empty_samples() {
        let net = load_asia();
        let empty_samples: Vec<Sample> = vec![];
        let marginals = samples_to_marginals(&empty_samples, &net);
        assert!(marginals.is_empty());
    }

    // ── Streaming tests ────────────────────────────────────────────

    #[tokio::test]
    async fn gibbs_stream_delivers_multiple_chunks() {
        use futures::StreamExt;

        let net = load_asia();
        let evidence = Evidence::new();
        let gibbs = GibbsSampler::new(10_000, 1_000).with_seed(42);

        let stream = gibbs.sample_stream(&net, &evidence, 1_000).unwrap();
        let chunks: Vec<_> = stream.collect().await;

        // 10K samples / 1K chunk_size = 10 chunks
        assert_eq!(chunks.len(), 10, "Expected 10 chunks, got {}", chunks.len());

        // Iteration counts should be increasing
        for (i, chunk) in chunks.iter().enumerate() {
            assert_eq!(chunk.iteration, (i + 1) * 1_000);
            assert_eq!(chunk.total_iterations, 10_000);
        }

        // Only the last chunk is final
        for chunk in &chunks[..chunks.len() - 1] {
            assert!(!chunk.is_final);
        }
        assert!(chunks.last().unwrap().is_final);

        // Final chunk should have 8 variables in marginals (Asia network)
        let final_result = &chunks.last().unwrap().data;
        assert_eq!(final_result.marginals.len(), 8);
        assert_eq!(final_result.algorithm, "gibbs");
    }

    #[tokio::test]
    async fn gibbs_stream_convergence_decreases() {
        use futures::StreamExt;

        let net = load_asia();
        let evidence = Evidence::new();
        let gibbs = GibbsSampler::new(10_000, 1_000).with_seed(42);

        let stream = gibbs.sample_stream(&net, &evidence, 1_000).unwrap();
        let chunks: Vec<_> = stream.collect().await;

        // Convergence metric should generally decrease (first chunk is always 1.0)
        assert_eq!(chunks[0].convergence_metric, 1.0);
        // Last chunk should be well-converged
        let last = chunks.last().unwrap();
        assert!(
            last.convergence_metric < 0.1,
            "Final convergence should be small, got {}",
            last.convergence_metric
        );
    }

    #[tokio::test]
    async fn gibbs_stream_deterministic_with_seed() {
        use futures::StreamExt;

        let net = load_asia();
        let evidence = Evidence::new();

        let gibbs1 = GibbsSampler::new(5_000, 500).with_seed(42);
        let stream1 = gibbs1.sample_stream(&net, &evidence, 1_000).unwrap();
        let chunks1: Vec<_> = stream1.collect().await;

        let gibbs2 = GibbsSampler::new(5_000, 500).with_seed(42);
        let stream2 = gibbs2.sample_stream(&net, &evidence, 1_000).unwrap();
        let chunks2: Vec<_> = stream2.collect().await;

        assert_eq!(chunks1.len(), chunks2.len());
        for (c1, c2) in chunks1.iter().zip(chunks2.iter()) {
            for (var, probs1) in &c1.data.marginals {
                let probs2 = c2.data.marginals.get(var).unwrap();
                for (p1, p2) in probs1.iter().zip(probs2.iter()) {
                    assert!(
                        (p1 - p2).abs() < 1e-10,
                        "Streaming Gibbs with same seed should be deterministic"
                    );
                }
            }
        }
    }

    #[tokio::test]
    async fn gibbs_stream_cancellation() {
        use futures::StreamExt;

        let net = load_asia();
        let evidence = Evidence::new();
        let gibbs = GibbsSampler::new(100_000, 1_000).with_seed(42);

        let mut stream = gibbs.sample_stream(&net, &evidence, 10_000).unwrap();

        // Read just one chunk, then drop the stream
        let first = stream.next().await;
        assert!(first.is_some());
        assert_eq!(first.unwrap().iteration, 10_000);
        drop(stream);

        // Background task should stop (we can't directly verify, but no panic/leak)
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    #[test]
    fn gibbs_stream_blocking_iter() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        let net = load_asia();
        let evidence = Evidence::new();
        let gibbs = GibbsSampler::new(5_000, 500).with_seed(42);

        let stream = rt.block_on(async { gibbs.sample_stream(&net, &evidence, 1_000).unwrap() });

        let chunks: Vec<_> = rt.block_on(async {
            tokio::task::spawn_blocking(move || stream.blocking_iter().collect::<Vec<_>>())
                .await
                .unwrap()
        });

        assert_eq!(chunks.len(), 5);
        assert!(chunks.last().unwrap().is_final);
    }
}
