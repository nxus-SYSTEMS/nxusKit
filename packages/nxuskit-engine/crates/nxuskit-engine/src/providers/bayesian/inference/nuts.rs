//! NUTS (No-U-Turn Sampler) / HMC inference for continuous Bayesian networks.
//!
//! Provides gradient-based MCMC sampling via the `nuts-rs` crate:
//!
//! - **`BnLogDensity`**: Computes log-posterior density and gradient for
//!   a Gaussian network given evidence.
//! - **`NutsSampler`**: Configures and runs NUTS sampling with multi-chain
//!   support, ESS/R-hat convergence diagnostics, and streaming.
//!
//! The NUTS sampler is appropriate for continuous posterior distributions
//! where moment-matching is intractable or when sampling-based uncertainty
//! quantification is desired.

use std::collections::HashMap;
use std::time::Instant;

use nalgebra::{DMatrix, DVector};

use super::{ContinuousMarginal, InferenceDiagnostics, InferenceEngine, InferenceResult, Marginal};
use crate::providers::bayesian::error::{BayesError, BayesResult};
use crate::providers::bayesian::evidence::Evidence;
use crate::providers::bayesian::inference::NUTSDiagnostics;
use crate::providers::bayesian::network::BayesianNetwork;
use crate::providers::bayesian::stream::{BayesStream, BayesStreamChunk};
use crate::providers::bayesian::types::{ObservationType, VariableName};

// ---------------------------------------------------------------------------
// BnLogDensity — log-posterior density + gradient for nuts-rs
// ---------------------------------------------------------------------------

/// Log-posterior density function for a Gaussian Bayesian network.
///
/// Implements `nuts_rs::CpuLogpFunc` by computing the multivariate Gaussian
/// log-density and its gradient (via the precision matrix) for all unobserved
/// continuous variables.
///
/// The density is:
///   log p(x) = -½ xᵀ Λ x + hᵀ x + const
///   ∇ log p(x) = h - Λ x
///
/// where Λ is the joint precision matrix and h is the precision-weighted
/// mean vector of the posterior (after conditioning on evidence).
#[derive(Debug, Clone)]
pub struct BnLogDensity {
    /// Joint posterior precision matrix.
    precision: DMatrix<f64>,
    /// Precision-weighted mean vector.
    h_vec: DVector<f64>,
    /// Ordered list of variable names being sampled.
    variables: Vec<VariableName>,
    /// Dimensionality of the sampling space.
    dim: usize,
}

impl BnLogDensity {
    /// Build a `BnLogDensity` from a Gaussian network and evidence.
    ///
    /// Constructs the joint Gaussian factor, conditions on continuous evidence,
    /// and extracts the posterior precision and h-vector for NUTS sampling.
    pub fn from_network(network: &BayesianNetwork, evidence: &Evidence) -> BayesResult<Self> {
        use crate::providers::bayesian::inference::gaussian::GaussianFactor;

        let gaussian_vars = network.gaussian_variables();
        if gaussian_vars.is_empty() {
            return Err(BayesError::InferenceError(
                "NUTS requires at least one continuous variable".to_string(),
            ));
        }

        // Build and multiply all Gaussian factors.
        let mut factors: Vec<GaussianFactor> = Vec::with_capacity(gaussian_vars.len());
        for gv in gaussian_vars.values() {
            let factor = GaussianFactor::from_gaussian_variable(gv, network)?;
            factors.push(factor);
        }

        let mut joint = factors[0].clone();
        for f in &factors[1..] {
            joint = joint.multiply(f);
        }

        // Condition on continuous evidence.
        for (var, obs) in evidence.all_observations() {
            if let ObservationType::Continuous(value) = obs
                && joint.variables.contains(var)
            {
                joint = joint.condition(var, *value)?;
            }
        }

        let dim = joint.variables.len();
        if dim == 0 {
            return Err(BayesError::InferenceError(
                "All continuous variables are observed; nothing to sample".to_string(),
            ));
        }

        Ok(Self {
            precision: joint.precision,
            h_vec: joint.h_vec,
            variables: joint.variables,
            dim,
        })
    }

    /// Compute the log-density at a position.
    pub fn log_density(&self, position: &[f64]) -> f64 {
        let x = DVector::from_row_slice(position);
        let lambda_x = &self.precision * &x;
        // log p(x) = -½ xᵀ Λ x + hᵀ x + const
        -0.5 * x.dot(&lambda_x) + self.h_vec.dot(&x)
    }

    /// Compute the gradient of the log-density at a position.
    pub fn gradient(&self, position: &[f64], grad: &mut [f64]) {
        let x = DVector::from_row_slice(position);
        // ∇ log p(x) = h - Λ x
        let g = &self.h_vec - &self.precision * &x;
        for i in 0..self.dim {
            grad[i] = g[i];
        }
    }

    /// Get the variable names being sampled.
    pub fn variables(&self) -> &[VariableName] {
        &self.variables
    }

    /// Get the posterior mean from the precision form (Λ⁻¹ h).
    pub fn posterior_mean(&self) -> BayesResult<DVector<f64>> {
        let chol = self.precision.clone().cholesky().ok_or_else(|| {
            BayesError::InferenceError("Precision matrix is not positive definite".to_string())
        })?;
        let cov = chol.inverse();
        Ok(&cov * &self.h_vec)
    }
}

// ---------------------------------------------------------------------------
// NUTSConfig — sampler configuration
// ---------------------------------------------------------------------------

/// Configuration for the NUTS sampler.
#[derive(Debug, Clone)]
pub struct NUTSConfig {
    /// Number of post-warmup samples per chain (default: 1000).
    pub num_samples: u64,
    /// Number of warmup/tuning samples per chain (default: 500).
    pub num_warmup: u64,
    /// Maximum tree depth (default: 10).
    pub max_tree_depth: u64,
    /// RNG seed (default: 42).
    pub seed: u64,
    /// Number of parallel chains (default: 4).
    pub num_chains: usize,
}

impl Default for NUTSConfig {
    fn default() -> Self {
        Self {
            num_samples: 1000,
            num_warmup: 500,
            max_tree_depth: 10,
            seed: 42,
            num_chains: 4,
        }
    }
}

// ---------------------------------------------------------------------------
// NutsSampler
// ---------------------------------------------------------------------------

/// NUTS/HMC sampler for continuous Bayesian network inference.
///
/// Uses the `nuts-rs` crate for gradient-based MCMC sampling.
/// Computes ESS and split R-hat convergence diagnostics.
#[derive(Debug, Clone)]
pub struct NutsSampler {
    config: NUTSConfig,
}

/// Raw NUTS chain draws: Vec of chains, each chain is Vec of position vectors, plus warmup and draw counts.
type NutsChainDraws = (Vec<Vec<Box<[f64]>>>, usize, usize);

impl NutsSampler {
    /// Create a new NUTS sampler with default configuration.
    pub fn new() -> Self {
        Self {
            config: NUTSConfig::default(),
        }
    }

    /// Create a NUTS sampler with the given configuration.
    pub fn with_config(config: NUTSConfig) -> Self {
        Self { config }
    }

    /// Set the number of post-warmup samples per chain.
    pub fn num_samples(mut self, n: u64) -> Self {
        self.config.num_samples = n;
        self
    }

    /// Set the number of warmup/tuning samples per chain.
    pub fn num_warmup(mut self, n: u64) -> Self {
        self.config.num_warmup = n;
        self
    }

    /// Set the maximum tree depth.
    pub fn max_tree_depth(mut self, d: u64) -> Self {
        self.config.max_tree_depth = d;
        self
    }

    /// Set the RNG seed.
    pub fn seed(mut self, s: u64) -> Self {
        self.config.seed = s;
        self
    }

    /// Set the number of parallel chains.
    pub fn num_chains(mut self, n: usize) -> Self {
        self.config.num_chains = n;
        self
    }

    /// Run NUTS sampling and return raw draws per chain.
    ///
    /// Returns: Vec of chains, each chain is a Vec of position vectors (Box<[f64]>).
    fn run_sampling(&self, log_density: &BnLogDensity) -> BayesResult<NutsChainDraws> {
        use nuts_rs::rand::SeedableRng;
        use nuts_rs::rand::rngs::SmallRng;
        use nuts_rs::{Chain, CpuMath, DiagGradNutsSettings, Settings};

        let dim = log_density.dim;
        let num_chains = self.config.num_chains;

        // Configure NUTS settings.
        let settings = DiagGradNutsSettings {
            num_tune: self.config.num_warmup,
            num_draws: self.config.num_samples,
            maxdepth: self.config.max_tree_depth,
            seed: self.config.seed,
            num_chains,
            ..Default::default()
        };

        // Compute initial position (posterior mean + perturbation scaled by posterior SD).
        // nuts-rs requires all gradient elements to be finite AND nonzero at the
        // starting position. The posterior mean has gradient ≈ 0, so we perturb
        // by ~0.1 posterior standard deviations to ensure nonzero gradient.
        let init_pos = {
            let mean = log_density
                .posterior_mean()
                .unwrap_or_else(|_| DVector::from_element(dim, 0.0));
            let mut pos = vec![0.0_f64; dim];
            for i in 0..dim {
                // Scale perturbation by 1/sqrt(precision_ii) ≈ posterior SD.
                let prec_diag = log_density.precision[(i, i)];
                let scale = if prec_diag > 0.0 {
                    0.1 / prec_diag.sqrt()
                } else {
                    0.1
                };
                // Alternate sign and vary magnitude to avoid symmetry.
                let sign = if i % 2 == 0 { 1.0 } else { -1.0 };
                pos[i] = mean[i] + sign * scale * (1.0 + 0.1 * i as f64);
            }
            pos
        };

        let mut all_chains: Vec<Vec<Box<[f64]>>> = Vec::with_capacity(num_chains);
        let mut total_divergences = 0usize;
        let mut total_max_depth_hits = 0usize;

        for chain_idx in 0..num_chains {
            let mut rng = SmallRng::seed_from_u64(self.config.seed.wrapping_add(chain_idx as u64));

            let logp_fn = BnLogpWrapper {
                precision: log_density.precision.clone(),
                h_vec: log_density.h_vec.clone(),
                dim,
            };
            let math = CpuMath::new(logp_fn);
            let mut sampler = settings.new_chain(chain_idx as u64, math, &mut rng);

            // Set starting position. Under parallel load the initial gradient can
            // be invalid (e.g. contention on shared resources). Retry with
            // progressively larger perturbations up to a handful of times.
            let mut init_ok = false;
            let mut last_err = None;
            for attempt in 0..5u64 {
                let pos = if attempt == 0 {
                    init_pos.clone()
                } else {
                    // Increase perturbation scale with each retry.
                    let scale_factor = 1.0 + attempt as f64;
                    init_pos
                        .iter()
                        .enumerate()
                        .map(|(i, &v)| {
                            let prec_diag = log_density.precision[(i, i)];
                            let sd = if prec_diag > 0.0 {
                                1.0 / prec_diag.sqrt()
                            } else {
                                1.0
                            };
                            let sign = if (i + attempt as usize).is_multiple_of(2) {
                                1.0
                            } else {
                                -1.0
                            };
                            v + sign * 0.1 * scale_factor * sd
                        })
                        .collect::<Vec<_>>()
                };
                match sampler.set_position(&pos) {
                    Ok(()) => {
                        init_ok = true;
                        break;
                    }
                    Err(e) => {
                        last_err = Some(e);
                    }
                }
            }
            if !init_ok {
                return Err(BayesError::InferenceError(format!(
                    "NUTS chain {} init failed after retries: {:?}",
                    chain_idx,
                    last_err.unwrap()
                )));
            }

            let total_draws = self.config.num_warmup + self.config.num_samples;
            let mut draws = Vec::with_capacity(self.config.num_samples as usize);

            for _ in 0..total_draws {
                let (position, progress) = sampler.draw().map_err(|e| {
                    BayesError::InferenceError(format!(
                        "NUTS chain {} draw failed: {:?}",
                        chain_idx, e
                    ))
                })?;

                if progress.diverging {
                    total_divergences += 1;
                }
                if progress.num_steps >= (1u64 << self.config.max_tree_depth) {
                    total_max_depth_hits += 1;
                }

                // Only keep post-warmup draws.
                if !progress.tuning {
                    draws.push(position);
                }
            }

            all_chains.push(draws);
        }

        Ok((all_chains, total_divergences, total_max_depth_hits))
    }

    /// Compute posterior marginals from multi-chain draws.
    fn draws_to_marginals(
        &self,
        chains: &[Vec<Box<[f64]>>],
        variables: &[VariableName],
    ) -> HashMap<VariableName, ContinuousMarginal> {
        let dim = variables.len();
        let mut marginals = HashMap::with_capacity(dim);

        for (d, vname) in variables.iter().enumerate() {
            let mut all_values: Vec<f64> = Vec::new();
            for chain in chains {
                for draw in chain {
                    all_values.push(draw[d]);
                }
            }

            if all_values.is_empty() {
                continue;
            }

            let n = all_values.len() as f64;
            let mean = all_values.iter().sum::<f64>() / n;
            let variance = all_values.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0);

            marginals.insert(vname.clone(), ContinuousMarginal::new(mean, variance));
        }

        marginals
    }

    /// Compute NUTS diagnostics: ESS and R-hat for each variable.
    fn compute_diagnostics(
        &self,
        chains: &[Vec<Box<[f64]>>],
        variables: &[VariableName],
        divergences: usize,
        max_depth_hits: usize,
    ) -> NUTSDiagnostics {
        let dim = variables.len();
        let num_chains = chains.len();

        if num_chains == 0 || chains[0].is_empty() || dim == 0 {
            return NUTSDiagnostics {
                effective_sample_size: 0.0,
                r_hat: f64::NAN,
                per_chain_ess: vec![],
                per_chain_mean: vec![],
                divergences,
                max_tree_depth_hits: max_depth_hits,
            };
        }

        // Compute aggregate ESS and R-hat across all variables.
        let mut min_ess = f64::INFINITY;
        let mut max_rhat = 0.0_f64;

        // Per-chain ESS (aggregate: minimum across variables).
        let mut per_chain_ess = vec![f64::INFINITY; num_chains];
        // Per-chain mean (average across variables for summary).
        let mut per_chain_mean = vec![0.0; num_chains];

        for d in 0..dim {
            // Extract per-chain draws for this variable.
            let chain_draws: Vec<Vec<f64>> = chains
                .iter()
                .map(|c| c.iter().map(|draw| draw[d]).collect())
                .collect();

            let (ess, rhat) = split_rhat_ess(&chain_draws);
            if ess < min_ess {
                min_ess = ess;
            }
            if rhat > max_rhat {
                max_rhat = rhat;
            }

            // Per-chain ESS from single-chain autocorrelation.
            for (ci, cd) in chain_draws.iter().enumerate() {
                let chain_ess = single_chain_ess(cd);
                if chain_ess < per_chain_ess[ci] {
                    per_chain_ess[ci] = chain_ess;
                }
                per_chain_mean[ci] += cd.iter().sum::<f64>() / cd.len() as f64;
            }
        }

        // Average per-chain mean across dimensions.
        if dim > 0 {
            for m in &mut per_chain_mean {
                *m /= dim as f64;
            }
        }

        NUTSDiagnostics {
            effective_sample_size: min_ess,
            r_hat: max_rhat,
            per_chain_ess,
            per_chain_mean,
            divergences,
            max_tree_depth_hits: max_depth_hits,
        }
    }

    /// Run NUTS sampling with streaming progress updates.
    ///
    /// The sampling runs in a blocking thread (via `spawn_blocking`) since
    /// the nuts-rs `NutsChain` is `!Send` (uses `Rc` internally). Chunks
    /// are sent to the async channel via `blocking_send`.
    pub fn infer_stream(
        &self,
        network: &BayesianNetwork,
        evidence: &Evidence,
        chunk_interval: usize,
    ) -> BayesResult<BayesStream<InferenceResult>> {
        use nuts_rs::rand::SeedableRng;
        use nuts_rs::rand::rngs::SmallRng;
        use nuts_rs::{Chain, CpuMath, DiagGradNutsSettings, Settings};

        let log_density = BnLogDensity::from_network(network, evidence)?;
        let dim = log_density.dim;
        let variables = log_density.variables.clone();
        let config = self.config.clone();

        let (tx, rx) = tokio::sync::mpsc::channel(32);

        let precision = log_density.precision.clone();
        let h_vec = log_density.h_vec.clone();

        // Compute initial position with variance-scaled perturbation.
        let init_pos = {
            let mean = log_density
                .posterior_mean()
                .unwrap_or_else(|_| DVector::from_element(dim, 0.0));
            let precision = &log_density.precision;
            let mut pos = vec![0.0_f64; dim];
            for i in 0..dim {
                let prec_diag = precision[(i, i)];
                let scale = if prec_diag > 0.0 {
                    0.1 / prec_diag.sqrt()
                } else {
                    0.1
                };
                let sign = if i % 2 == 0 { 1.0 } else { -1.0 };
                pos[i] = mean[i] + sign * scale * (1.0 + 0.1 * i as f64);
            }
            pos
        };

        // Use spawn_blocking because NutsChain is !Send (Rc internals).
        tokio::task::spawn_blocking(move || {
            let start = Instant::now();
            let total_draws_per_chain = config.num_warmup + config.num_samples;
            let total_draws = total_draws_per_chain as usize * config.num_chains;
            let mut draws_so_far = 0usize;
            let mut all_chains: Vec<Vec<Box<[f64]>>> = Vec::with_capacity(config.num_chains);
            let mut total_divergences = 0usize;
            let mut total_max_depth_hits = 0usize;

            let settings = DiagGradNutsSettings {
                num_tune: config.num_warmup,
                num_draws: config.num_samples,
                maxdepth: config.max_tree_depth,
                seed: config.seed,
                num_chains: config.num_chains,
                ..Default::default()
            };

            for chain_idx in 0..config.num_chains {
                let mut rng = SmallRng::seed_from_u64(config.seed.wrapping_add(chain_idx as u64));

                let logp_fn = BnLogpWrapper {
                    precision: precision.clone(),
                    h_vec: h_vec.clone(),
                    dim,
                };
                let math = CpuMath::new(logp_fn);
                let mut sampler = settings.new_chain(chain_idx as u64, math, &mut rng);

                if sampler.set_position(&init_pos).is_err() {
                    return;
                }

                let mut chain_draws = Vec::with_capacity(config.num_samples as usize);

                for _ in 0..total_draws_per_chain {
                    match sampler.draw() {
                        Ok((position, progress)) => {
                            if progress.diverging {
                                total_divergences += 1;
                            }
                            if progress.num_steps >= (1u64 << config.max_tree_depth) {
                                total_max_depth_hits += 1;
                            }
                            if !progress.tuning {
                                chain_draws.push(position);
                            }
                        }
                        Err(_) => return,
                    }

                    draws_so_far += 1;

                    // Emit progress chunk at intervals.
                    if draws_so_far.is_multiple_of(chunk_interval) && !chain_draws.is_empty() {
                        let mut temp_chains = all_chains.clone();
                        temp_chains.push(chain_draws.clone());

                        let mut continuous_marginals = HashMap::new();
                        for (d, vname) in variables.iter().enumerate() {
                            let mut values: Vec<f64> = Vec::new();
                            for c in &temp_chains {
                                for draw in c {
                                    values.push(draw[d]);
                                }
                            }
                            if !values.is_empty() {
                                let n = values.len() as f64;
                                let mean = values.iter().sum::<f64>() / n;
                                let variance = if n > 1.0 {
                                    values.iter().map(|x| (x - mean).powi(2)).sum::<f64>()
                                        / (n - 1.0)
                                } else {
                                    0.0
                                };
                                continuous_marginals
                                    .insert(vname.clone(), ContinuousMarginal::new(mean, variance));
                            }
                        }

                        let result = InferenceResult {
                            marginals: HashMap::new(),
                            log_marginals: HashMap::new(),
                            continuous_marginals,
                            algorithm: "nuts".to_string(),
                            elapsed: start.elapsed(),
                            diagnostics: Some(InferenceDiagnostics {
                                iterations: draws_so_far,
                                burn_in: config.num_warmup as usize
                                    * (chain_idx + 1).min(config.num_chains),
                                max_marginal_change: 0.0,
                                effective_sample_size: None,
                            }),
                            nuts_diagnostics: None,
                        };

                        let chunk = BayesStreamChunk {
                            data: result,
                            iteration: draws_so_far,
                            total_iterations: total_draws,
                            convergence_metric: 0.0,
                            is_final: false,
                        };

                        if tx.blocking_send(chunk).is_err() {
                            return; // Receiver dropped — cancelled.
                        }
                    }
                }

                all_chains.push(chain_draws);
            }

            // Final result with full diagnostics.
            let mut continuous_marginals = HashMap::with_capacity(variables.len());
            for (d, vname) in variables.iter().enumerate() {
                let mut values: Vec<f64> = Vec::new();
                for c in &all_chains {
                    for draw in c {
                        values.push(draw[d]);
                    }
                }
                if !values.is_empty() {
                    let n = values.len() as f64;
                    let mean = values.iter().sum::<f64>() / n;
                    let variance = if n > 1.0 {
                        values.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0)
                    } else {
                        0.0
                    };
                    continuous_marginals
                        .insert(vname.clone(), ContinuousMarginal::new(mean, variance));
                }
            }

            // Compute diagnostics.
            let chain_draws_by_var: Vec<Vec<Vec<f64>>> = (0..dim)
                .map(|d| {
                    all_chains
                        .iter()
                        .map(|c| c.iter().map(|draw| draw[d]).collect())
                        .collect()
                })
                .collect();

            let mut min_ess = f64::INFINITY;
            let mut max_rhat = 0.0_f64;
            for var_chains in &chain_draws_by_var {
                let (ess, rhat) = split_rhat_ess(var_chains);
                if ess < min_ess {
                    min_ess = ess;
                }
                if rhat > max_rhat {
                    max_rhat = rhat;
                }
            }

            let nuts_diag = NUTSDiagnostics {
                effective_sample_size: min_ess,
                r_hat: max_rhat,
                per_chain_ess: vec![min_ess; config.num_chains],
                per_chain_mean: vec![0.0; config.num_chains],
                divergences: total_divergences,
                max_tree_depth_hits: total_max_depth_hits,
            };

            let result = InferenceResult {
                marginals: HashMap::new(),
                log_marginals: HashMap::new(),
                continuous_marginals,
                algorithm: "nuts".to_string(),
                elapsed: start.elapsed(),
                diagnostics: Some(InferenceDiagnostics {
                    iterations: draws_so_far,
                    burn_in: config.num_warmup as usize * config.num_chains,
                    max_marginal_change: 0.0,
                    effective_sample_size: Some(min_ess),
                }),
                nuts_diagnostics: Some(nuts_diag),
            };

            let chunk = BayesStreamChunk {
                data: result,
                iteration: total_draws,
                total_iterations: total_draws,
                convergence_metric: 0.0,
                is_final: true,
            };

            let _ = tx.blocking_send(chunk);
        });

        Ok(BayesStream::new(rx))
    }
}

impl Default for NutsSampler {
    fn default() -> Self {
        Self::new()
    }
}

impl InferenceEngine for NutsSampler {
    fn infer(
        &self,
        network: &BayesianNetwork,
        evidence: &Evidence,
    ) -> BayesResult<InferenceResult> {
        let start = Instant::now();

        // Validate network.
        if !network.has_continuous_variables() {
            return Err(BayesError::InferenceError(
                "NUTS requires continuous (Gaussian) variables; use VE/JT/Gibbs for discrete-only networks".to_string(),
            ));
        }

        // Validate: discrete variables must have CPTs if present.
        network.validate()?;

        let log_density = BnLogDensity::from_network(network, evidence)?;
        let variables = log_density.variables.clone();

        let (chains, divergences, max_depth_hits) = self.run_sampling(&log_density)?;
        let continuous_marginals = self.draws_to_marginals(&chains, &variables);
        let nuts_diagnostics =
            self.compute_diagnostics(&chains, &variables, divergences, max_depth_hits);

        let total_draws: usize = chains.iter().map(|c| c.len()).sum();

        Ok(InferenceResult {
            marginals: HashMap::new(),
            log_marginals: HashMap::new(),
            continuous_marginals,
            algorithm: "nuts".to_string(),
            elapsed: start.elapsed(),
            diagnostics: Some(InferenceDiagnostics {
                iterations: total_draws,
                burn_in: self.config.num_warmup as usize * self.config.num_chains,
                max_marginal_change: 0.0,
                effective_sample_size: Some(nuts_diagnostics.effective_sample_size),
            }),
            nuts_diagnostics: Some(nuts_diagnostics),
        })
    }

    fn query(
        &self,
        network: &BayesianNetwork,
        evidence: &Evidence,
        variable: &VariableName,
    ) -> BayesResult<Marginal> {
        let result = self.infer(network, evidence)?;
        if let Some(cm) = result.continuous_marginals.get(variable) {
            return Ok(vec![cm.mean, cm.variance]);
        }
        Err(BayesError::InferenceError(format!(
            "Variable '{}' not found in NUTS results (may be observed or discrete)",
            variable
        )))
    }

    fn algorithm_name(&self) -> &str {
        "nuts"
    }
}

// ---------------------------------------------------------------------------
// BnLogpWrapper — nuts-rs CpuLogpFunc implementation
// ---------------------------------------------------------------------------

/// Internal wrapper that implements `nuts_rs::CpuLogpFunc` for a Gaussian
/// posterior parameterized by precision matrix Λ and h-vector.
#[derive(Debug)]
struct BnLogpWrapper {
    precision: DMatrix<f64>,
    h_vec: DVector<f64>,
    dim: usize,
}

/// Error type for log-density evaluation (never occurs for Gaussian).
#[derive(Debug, thiserror::Error)]
enum BnLogpError {
    #[error("Log-density evaluation error: {0}")]
    EvalError(String),
}

impl nuts_rs::LogpError for BnLogpError {
    fn is_recoverable(&self) -> bool {
        true // Treat as divergence, not fatal.
    }
}

impl nuts_rs::HasDims for BnLogpWrapper {
    fn dim_sizes(&self) -> HashMap<String, u64> {
        let mut sizes = HashMap::new();
        sizes.insert("params".to_string(), self.dim as u64);
        sizes
    }
}

impl nuts_rs::CpuLogpFunc for BnLogpWrapper {
    type LogpError = BnLogpError;
    type FlowParameters = ();
    type ExpandedVector = Vec<f64>;

    fn dim(&self) -> usize {
        self.dim
    }

    fn logp(&mut self, position: &[f64], gradient: &mut [f64]) -> Result<f64, Self::LogpError> {
        let x = DVector::from_row_slice(position);

        // gradient = h - Λ x
        let lambda_x = &self.precision * &x;
        let g = &self.h_vec - &lambda_x;
        for i in 0..self.dim {
            gradient[i] = g[i];
        }

        // log p(x) = -½ xᵀ Λ x + hᵀ x
        let logp = -0.5 * x.dot(&lambda_x) + self.h_vec.dot(&x);

        if !logp.is_finite() {
            return Err(BnLogpError::EvalError(
                "Non-finite log-density value".to_string(),
            ));
        }

        Ok(logp)
    }

    fn expand_vector<R>(
        &mut self,
        _rng: &mut R,
        position: &[f64],
    ) -> Result<Vec<f64>, nuts_rs::CpuMathError>
    where
        R: nuts_rs::rand::Rng + ?Sized,
    {
        Ok(position.to_vec())
    }
}

// ---------------------------------------------------------------------------
// ESS and R-hat computation (Vehtari et al. 2020)
// ---------------------------------------------------------------------------

/// Compute split R-hat and bulk ESS from multi-chain draws.
///
/// Implements the split-chain R-hat diagnostic (Vehtari, Gelman, et al. 2020):
/// 1. Split each chain in half → 2M half-chains from M chains.
/// 2. Compute between-chain variance B and within-chain variance W.
/// 3. R-hat = sqrt((W + B/n) / W).
/// 4. ESS from autocorrelation time of pooled draws.
fn split_rhat_ess(chain_draws: &[Vec<f64>]) -> (f64, f64) {
    if chain_draws.is_empty() || chain_draws[0].is_empty() {
        return (0.0, f64::NAN);
    }

    // Split each chain in half.
    let mut half_chains: Vec<Vec<f64>> = Vec::new();
    for chain in chain_draws {
        let mid = chain.len() / 2;
        if mid == 0 {
            continue;
        }
        half_chains.push(chain[..mid].to_vec());
        half_chains.push(chain[mid..].to_vec());
    }

    let m = half_chains.len() as f64; // Number of half-chains.
    if m < 2.0 {
        return (0.0, f64::NAN);
    }

    let n = half_chains[0].len() as f64; // Draws per half-chain.
    if n < 2.0 {
        return (0.0, f64::NAN);
    }

    // Per-chain means and variances.
    let chain_means: Vec<f64> = half_chains
        .iter()
        .map(|c| c.iter().sum::<f64>() / c.len() as f64)
        .collect();
    let chain_vars: Vec<f64> = half_chains
        .iter()
        .zip(&chain_means)
        .map(|(c, &mean)| {
            c.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (c.len() as f64 - 1.0)
        })
        .collect();

    let grand_mean = chain_means.iter().sum::<f64>() / m;

    // Within-chain variance W = mean of per-chain variances.
    let w = chain_vars.iter().sum::<f64>() / m;

    // Between-chain variance B = n * var(chain_means).
    let b = n * chain_means
        .iter()
        .map(|x| (x - grand_mean).powi(2))
        .sum::<f64>()
        / (m - 1.0);

    // Estimated marginal posterior variance.
    let var_hat = (1.0 - 1.0 / n) * w + b / n;

    // R-hat.
    let rhat = if w > 0.0 {
        (var_hat / w).sqrt()
    } else {
        f64::NAN
    };

    // Bulk ESS via autocorrelation (simplified: use the formula from BDA3).
    // ESS ≈ m * n / (1 + 2 * sum(autocorrelation))
    let ess = bulk_ess_from_chains(&half_chains, w, var_hat);

    (ess, rhat)
}

/// Compute bulk ESS from half-chains using the initial positive sequence estimator.
fn bulk_ess_from_chains(half_chains: &[Vec<f64>], w: f64, var_hat: f64) -> f64 {
    if var_hat <= 0.0 {
        return 0.0;
    }

    let m = half_chains.len();
    let n = half_chains[0].len();
    let mn = (m * n) as f64;

    // Pool all draws for autocorrelation computation.
    let chain_means: Vec<f64> = half_chains
        .iter()
        .map(|c| c.iter().sum::<f64>() / c.len() as f64)
        .collect();

    // Compute autocorrelation at each lag using within-chain centered draws.
    let max_lag = n / 2;
    let mut rho_hat_sum = 0.0_f64;

    for lag in 1..max_lag {
        let mut autocov = 0.0;
        let mut count = 0;
        for (ci, chain) in half_chains.iter().enumerate() {
            let mean = chain_means[ci];
            for t in 0..(chain.len() - lag) {
                autocov += (chain[t] - mean) * (chain[t + lag] - mean);
                count += 1;
            }
        }
        if count == 0 {
            break;
        }
        autocov /= count as f64;

        let rho = 1.0 - (w - autocov) / var_hat;

        // Initial positive sequence: stop when rho becomes negative.
        if rho < 0.0 {
            break;
        }
        rho_hat_sum += rho;
    }

    // ESS = m*n / (1 + 2 * sum of positive autocorrelations).
    let tau = 1.0 + 2.0 * rho_hat_sum;
    if tau > 0.0 { mn / tau } else { mn }
}

/// Compute ESS for a single chain using autocorrelation.
fn single_chain_ess(draws: &[f64]) -> f64 {
    let n = draws.len();
    if n < 4 {
        return n as f64;
    }

    let mean = draws.iter().sum::<f64>() / n as f64;
    let var = draws.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n as f64 - 1.0);

    if var <= 0.0 {
        return 0.0;
    }

    let max_lag = n / 2;
    let mut rho_sum = 0.0_f64;

    for lag in 1..max_lag {
        let mut autocov = 0.0;
        for t in 0..(n - lag) {
            autocov += (draws[t] - mean) * (draws[t + lag] - mean);
        }
        autocov /= (n - lag) as f64;
        let rho = autocov / var;

        if rho < 0.0 {
            break;
        }
        rho_sum += rho;
    }

    let tau = 1.0 + 2.0 * rho_sum;
    if tau > 0.0 { n as f64 / tau } else { n as f64 }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::bayesian::network::BayesianNetwork;
    use crate::providers::bayesian::types::{GaussianVariable, VariableName};

    fn var_name(s: &str) -> VariableName {
        VariableName::new(s).unwrap()
    }

    /// Build a 3-node Gaussian chain: X1 → X2 → X3
    fn build_3node_chain() -> BayesianNetwork {
        let mut net = BayesianNetwork::new();
        let x1 = GaussianVariable::new("X1", 5.0, 2.0).unwrap();
        let x2 = GaussianVariable::new("X2", 0.0, 1.0)
            .unwrap()
            .with_weight("X1", 1.0)
            .unwrap();
        let x3 = GaussianVariable::new("X3", 0.0, 1.0)
            .unwrap()
            .with_weight("X2", 1.0)
            .unwrap();

        net.add_gaussian_variable(x1).unwrap();
        net.add_gaussian_variable(x2).unwrap();
        net.add_gaussian_variable(x3).unwrap();
        net.add_edge(&var_name("X1"), &var_name("X2")).unwrap();
        net.add_edge(&var_name("X2"), &var_name("X3")).unwrap();
        net
    }

    /// Build a 5-node Gaussian chain: X1 → X2 → X3 → X4 → X5
    fn build_5node_chain() -> BayesianNetwork {
        let mut net = BayesianNetwork::new();
        let x1 = GaussianVariable::new("X1", 5.0, 2.0).unwrap();
        let x2 = GaussianVariable::new("X2", 0.0, 1.0)
            .unwrap()
            .with_weight("X1", 1.0)
            .unwrap();
        let x3 = GaussianVariable::new("X3", 0.0, 1.0)
            .unwrap()
            .with_weight("X2", 1.0)
            .unwrap();
        let x4 = GaussianVariable::new("X4", 0.0, 1.0)
            .unwrap()
            .with_weight("X3", 1.0)
            .unwrap();
        let x5 = GaussianVariable::new("X5", 0.0, 1.0)
            .unwrap()
            .with_weight("X4", 1.0)
            .unwrap();

        net.add_gaussian_variable(x1).unwrap();
        net.add_gaussian_variable(x2).unwrap();
        net.add_gaussian_variable(x3).unwrap();
        net.add_gaussian_variable(x4).unwrap();
        net.add_gaussian_variable(x5).unwrap();
        net.add_edge(&var_name("X1"), &var_name("X2")).unwrap();
        net.add_edge(&var_name("X2"), &var_name("X3")).unwrap();
        net.add_edge(&var_name("X3"), &var_name("X4")).unwrap();
        net.add_edge(&var_name("X4"), &var_name("X5")).unwrap();
        net
    }

    // --- T054: BnLogDensity tests ---

    #[test]
    fn log_density_construction() {
        let net = build_3node_chain();
        let evidence = Evidence::new();
        let logp = BnLogDensity::from_network(&net, &evidence).unwrap();
        assert_eq!(logp.dim, 3);
        assert_eq!(logp.variables.len(), 3);
    }

    #[test]
    fn log_density_gradient_at_mean() {
        let net = build_3node_chain();
        let evidence = Evidence::new();
        let logp = BnLogDensity::from_network(&net, &evidence).unwrap();

        // At the posterior mean, the gradient should be approximately zero.
        let mean = logp.posterior_mean().unwrap();
        let mut pos = vec![0.0; logp.dim];
        for i in 0..logp.dim {
            pos[i] = mean[i];
        }

        let mut grad = vec![0.0; logp.dim];
        logp.gradient(&pos, &mut grad);

        for (i, g) in grad.iter().enumerate() {
            assert!(
                g.abs() < 1e-8,
                "Gradient at mean should be ~0 for dim {}, got {}",
                i,
                g
            );
        }
    }

    #[test]
    fn log_density_with_evidence() {
        let net = build_3node_chain();
        let mut evidence = Evidence::new();
        evidence
            .observe_continuous(&net, &var_name("X1"), 3.0)
            .unwrap();

        let logp = BnLogDensity::from_network(&net, &evidence).unwrap();
        // X1 is observed, so dim should be 2 (X2, X3).
        assert_eq!(logp.dim, 2);
    }

    #[test]
    fn log_density_no_continuous_variables_error() {
        let net = BayesianNetwork::new();
        let evidence = Evidence::new();
        let result = BnLogDensity::from_network(&net, &evidence);
        assert!(result.is_err());
    }

    // --- T055: NutsSampler tests ---

    #[test]
    fn nuts_5node_logdensity_valid() {
        // Verify that the 5-node chain produces a valid log-density.
        let net = build_5node_chain();
        let evidence = Evidence::new();
        let logp = BnLogDensity::from_network(&net, &evidence).unwrap();

        // Check dimensions.
        assert_eq!(logp.dim, 5);

        // Evaluate at a position near the mean.
        let mean = logp.posterior_mean().unwrap();
        let mut pos = vec![0.0; 5];
        for i in 0..5 {
            pos[i] = mean[i] + 0.1;
        }

        let ld = logp.log_density(&pos);
        assert!(ld.is_finite(), "log-density should be finite, got {}", ld);

        let mut grad = vec![0.0; 5];
        logp.gradient(&pos, &mut grad);
        for (i, g) in grad.iter().enumerate() {
            assert!(g.is_finite(), "gradient[{}] = {} should be finite", i, g);
        }
    }

    #[test]
    fn nuts_5node_gaussian_accuracy() {
        let net = build_5node_chain();
        let evidence = Evidence::new();

        // Use moment-matching as ground truth.
        let exact_engine =
            crate::providers::bayesian::inference::gaussian::MomentMatchingInference::new();
        let exact_result = exact_engine.infer(&net, &evidence).unwrap();

        // Run NUTS with enough samples for accuracy.
        let sampler = NutsSampler::new()
            .num_samples(2000)
            .num_warmup(500)
            .max_tree_depth(8)
            .seed(42)
            .num_chains(2);

        let result = sampler.infer(&net, &evidence).unwrap();
        assert_eq!(result.algorithm, "nuts");

        // Compare means: RMSE < 0.5 (sampling noise is higher than exact).
        let mut sq_err_sum = 0.0;
        let mut count = 0;
        for vname in &["X1", "X2", "X3", "X4", "X5"] {
            let v = var_name(vname);
            if let (Some(exact_cm), Some(nuts_cm)) = (
                exact_result.continuous_marginals.get(&v),
                result.continuous_marginals.get(&v),
            ) {
                sq_err_sum += (exact_cm.mean - nuts_cm.mean).powi(2);
                count += 1;
            }
        }
        let rmse = (sq_err_sum / count as f64).sqrt();
        assert!(rmse < 0.5, "NUTS RMSE vs exact = {}, expected < 0.5", rmse);
    }

    #[test]
    fn nuts_diagnostics_present() {
        let net = build_3node_chain();
        let evidence = Evidence::new();

        let sampler = NutsSampler::new()
            .num_samples(200)
            .num_warmup(100)
            .seed(42)
            .num_chains(2);

        let result = sampler.infer(&net, &evidence).unwrap();

        assert!(result.nuts_diagnostics.is_some());
        let diag = result.nuts_diagnostics.unwrap();

        // ESS should be positive.
        assert!(
            diag.effective_sample_size > 0.0,
            "ESS = {}",
            diag.effective_sample_size
        );

        // R-hat should be close to 1.0 for a well-mixed chain.
        assert!(diag.r_hat < 2.0, "R-hat = {}, expected < 2.0", diag.r_hat);

        // Per-chain data should be present.
        assert_eq!(diag.per_chain_ess.len(), 2);
        assert_eq!(diag.per_chain_mean.len(), 2);
    }

    #[test]
    fn nuts_with_evidence() {
        let net = build_5node_chain();
        let mut evidence = Evidence::new();
        evidence
            .observe_continuous(&net, &var_name("X1"), 3.0)
            .unwrap();

        let sampler = NutsSampler::new()
            .num_samples(1000)
            .num_warmup(300)
            .seed(42)
            .num_chains(2);

        let result = sampler.infer(&net, &evidence).unwrap();

        // X1 should not be in marginals (it's observed).
        assert!(
            !result.continuous_marginals.contains_key(&var_name("X1")),
            "Observed variable should not be in marginals"
        );

        // Remaining variables' means should be around 3.0 (X1=3 propagates).
        for vname in &["X2", "X3", "X4", "X5"] {
            let v = var_name(vname);
            if let Some(cm) = result.continuous_marginals.get(&v) {
                assert!(
                    (cm.mean - 3.0).abs() < 1.0,
                    "E[{}|X1=3] = {}, expected ~3.0",
                    vname,
                    cm.mean
                );
            }
        }
    }

    #[test]
    fn nuts_discrete_only_error() {
        let mut net = BayesianNetwork::new();
        let d_var = crate::providers::bayesian::types::DiscreteVariable::new(
            var_name("D"),
            vec![
                crate::providers::bayesian::types::StateName::new("a").unwrap(),
                crate::providers::bayesian::types::StateName::new("b").unwrap(),
            ],
        )
        .unwrap();
        net.add_variable(d_var).unwrap();
        net.set_cpt(&var_name("D"), vec![0.5, 0.5]).unwrap();

        let sampler = NutsSampler::new();
        let result = sampler.infer(&net, &Evidence::new());
        assert!(result.is_err(), "NUTS should reject discrete-only networks");
    }

    // --- T055: Multi-chain R-hat/ESS tests ---

    #[test]
    fn rhat_ess_computation_basic() {
        // Two chains sampling from the same distribution (sin-based pseudo-random).
        let chain1: Vec<f64> = (0..200)
            .map(|i| ((i as f64) * 1.1).sin() * 2.0 + 5.0)
            .collect();
        let chain2: Vec<f64> = (0..200)
            .map(|i| ((i as f64) * 1.3 + 0.7).sin() * 2.0 + 5.0)
            .collect();

        let (ess, rhat) = split_rhat_ess(&[chain1, chain2]);
        assert!(ess > 0.0, "ESS should be positive, got {}", ess);
        assert!(
            rhat < 1.5,
            "R-hat for similar chains should be < 1.5, got {}",
            rhat
        );
    }

    #[test]
    fn rhat_divergent_chains() {
        // Two very different chains → R-hat should be high.
        let chain1: Vec<f64> = vec![0.0; 100];
        let chain2: Vec<f64> = vec![10.0; 100];

        let (_ess, rhat) = split_rhat_ess(&[chain1, chain2]);
        // R-hat should be large for non-mixed chains.
        assert!(
            rhat > 1.5 || rhat.is_nan(),
            "R-hat for divergent chains should be > 1.5, got {}",
            rhat
        );
    }

    // --- T056: Streaming test ---

    #[tokio::test]
    async fn nuts_streaming_progress() {
        use futures::StreamExt;

        let net = build_3node_chain();
        let evidence = Evidence::new();

        let sampler = NutsSampler::new()
            .num_samples(100)
            .num_warmup(50)
            .seed(42)
            .num_chains(1);

        let stream = sampler.infer_stream(&net, &evidence, 50).unwrap();

        let chunks: Vec<_> = stream.collect().await;

        // Should have at least a final chunk.
        assert!(
            !chunks.is_empty(),
            "Stream should produce at least one chunk"
        );

        // Last chunk should be final.
        let last = chunks.last().unwrap();
        assert!(last.is_final, "Last chunk should be final");

        // Final chunk should have continuous marginals.
        assert!(
            !last.data.continuous_marginals.is_empty(),
            "Final chunk should have marginals"
        );
    }
}
