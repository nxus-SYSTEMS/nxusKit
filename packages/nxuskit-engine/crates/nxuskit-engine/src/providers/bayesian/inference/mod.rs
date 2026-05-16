//! Inference algorithms for Bayesian Networks.
//!
//! This module provides the `InferenceEngine` trait and concrete implementations:
//! - Variable Elimination (exact)
//! - Junction Tree / Shafer-Shenoy (exact)
//! - Gibbs sampling (approximate)

pub mod gaussian;
pub mod gibbs;
pub mod junction_tree;
pub mod loopy_bp;
pub mod nuts;
pub mod sampling;
pub mod variable_elimination;

use std::collections::HashMap;

use super::error::BayesResult;
use super::evidence::Evidence;
use super::network::BayesianNetwork;
use super::types::VariableName;
use serde::{Deserialize, Serialize};

pub use gaussian::{GaussianFactor, MomentMatchingInference};
pub use gibbs::{GibbsSampler, samples_to_marginals};
pub use junction_tree::JunctionTree;
pub use loopy_bp::{LBPConfig, LoopyBeliefPropagation};
pub use nuts::{NUTSConfig, NutsSampler};
pub use sampling::{ForwardSampler, LikelihoodWeightedSampler, RejectionSampler};
pub use variable_elimination::VariableElimination;

/// Elimination ordering heuristic for Variable Elimination inference.
///
/// # Variants
/// - `MinFill` (default): Minimize the number of fill edges added during elimination.
/// - `MinWeight`: Minimize the product of domain sizes of the eliminated variable
///   and its neighbors, producing tighter elimination orders for high-cardinality networks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EliminationHeuristic {
    /// Minimize fill edges (current default, backward compatible).
    #[default]
    MinFill,
    /// Minimize product of domain sizes of variable and neighbors.
    MinWeight,
}

/// Configuration for Bayesian Network inference operations.
///
/// All fields are optional with `#[serde(default)]` for backward compatibility.
/// When not specified, existing defaults are used.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BnInferenceConfig {
    /// Elimination ordering heuristic for Variable Elimination.
    /// Defaults to `MinFill` when `None`.
    #[serde(default)]
    pub elimination_heuristic: Option<EliminationHeuristic>,
}

/// Posterior marginal distribution for a single variable.
/// Indices correspond to the variable's state ordering.
/// Values sum to 1.0 (or all 0.0 if evidence has zero probability).
pub type Marginal = Vec<f64>;

/// Posterior marginal for a continuous (Gaussian) variable.
#[derive(Debug, Clone)]
pub struct ContinuousMarginal {
    /// Posterior mean.
    pub mean: f64,
    /// Posterior variance.
    pub variance: f64,
    /// Lower bound of 95% credible interval.
    pub ci_lower: f64,
    /// Upper bound of 95% credible interval.
    pub ci_upper: f64,
}

impl ContinuousMarginal {
    /// Create a new continuous marginal from mean and variance,
    /// computing 95% credible intervals automatically.
    pub fn new(mean: f64, variance: f64) -> Self {
        let std_dev = variance.sqrt();
        Self {
            mean,
            variance,
            ci_lower: mean - 1.96 * std_dev,
            ci_upper: mean + 1.96 * std_dev,
        }
    }
}

/// Diagnostics from NUTS/HMC sampling.
#[derive(Debug, Clone)]
pub struct NUTSDiagnostics {
    /// Bulk effective sample size across all chains.
    pub effective_sample_size: f64,
    /// Split R-hat convergence diagnostic.
    pub r_hat: f64,
    /// ESS per chain.
    pub per_chain_ess: Vec<f64>,
    /// Mean per chain.
    pub per_chain_mean: Vec<f64>,
    /// Number of divergent transitions.
    pub divergences: usize,
    /// Number of times max tree depth was reached.
    pub max_tree_depth_hits: usize,
}

/// Result of running inference on a Bayesian Network.
#[derive(Debug, Clone)]
pub struct InferenceResult {
    /// Posterior marginal distributions: variable name → state probabilities.
    pub marginals: HashMap<VariableName, Marginal>,
    /// Log-marginals (for numerical precision).
    pub log_marginals: HashMap<VariableName, Vec<f64>>,
    /// Continuous posterior marginals: variable name → mean/variance/CI.
    pub continuous_marginals: HashMap<VariableName, ContinuousMarginal>,
    /// Name of the algorithm used.
    pub algorithm: String,
    /// Wall-clock elapsed time.
    pub elapsed: std::time::Duration,
    /// Algorithm-specific diagnostics.
    pub diagnostics: Option<InferenceDiagnostics>,
    /// NUTS-specific diagnostics.
    pub nuts_diagnostics: Option<NUTSDiagnostics>,
}

/// Diagnostics from an inference run.
#[derive(Debug, Clone, Default)]
pub struct InferenceDiagnostics {
    /// Number of iterations (for iterative algorithms).
    pub iterations: usize,
    /// Burn-in period (for sampling algorithms).
    pub burn_in: usize,
    /// Maximum marginal change in last iteration (for convergence tracking).
    pub max_marginal_change: f64,
    /// Effective sample size estimate (for sampling algorithms).
    pub effective_sample_size: Option<f64>,
}

/// A single sample from a sampling-based inference algorithm.
#[derive(Debug, Clone)]
pub struct Sample {
    /// Variable assignments in this sample (variable → state index).
    pub assignments: HashMap<VariableName, usize>,
}

/// Common contract for all inference algorithms.
///
/// Implementations: VariableElimination, JunctionTree, GibbsSampler
pub trait InferenceEngine {
    /// Compute posterior marginals for all unobserved variables.
    ///
    /// # Errors
    /// - `BayesError::IncompleteNetwork` if any variable lacks a CPT
    /// - `BayesError::InvalidEvidence` if evidence references unknown variables/states
    /// - `BayesError::ZeroProbabilityEvidence` if evidence is impossible
    fn infer(&self, network: &BayesianNetwork, evidence: &Evidence)
    -> BayesResult<InferenceResult>;

    /// Compute posterior marginal for a single variable.
    ///
    /// More efficient than `infer()` for VE (avoids computing all marginals).
    /// For JT, equivalent to `infer()` followed by lookup.
    fn query(
        &self,
        network: &BayesianNetwork,
        evidence: &Evidence,
        variable: &VariableName,
    ) -> BayesResult<Marginal>;

    /// Return the algorithm name (for logging/diagnostics).
    fn algorithm_name(&self) -> &str;
}

/// Extended contract for sampling-based inference.
///
/// Implementations: GibbsSampler
pub trait SamplingInference: InferenceEngine {
    /// Draw N posterior samples (after burn-in).
    ///
    /// # Parameters
    /// - `num_samples`: Number of usable samples to produce
    /// - `burn_in`: Number of initial samples to discard
    /// - `seed`: Optional RNG seed for reproducibility (ChaCha20)
    fn sample(
        &self,
        network: &BayesianNetwork,
        evidence: &Evidence,
        num_samples: usize,
        burn_in: usize,
        seed: Option<u64>,
    ) -> BayesResult<Vec<Sample>>;
}
