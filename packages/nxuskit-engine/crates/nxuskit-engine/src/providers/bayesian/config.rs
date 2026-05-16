//! Configuration and serde types for the Bayesian Network provider.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::inference::EliminationHeuristic;

/// Provider-level configuration for BayesianProvider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BnConfig {
    /// Directory to scan for .bif network files.
    #[serde(default)]
    pub networks_directory: Option<PathBuf>,

    /// Default inference algorithm: "ve", "jt", or "gibbs".
    #[serde(default = "default_algorithm")]
    pub default_algorithm: String,

    /// Default number of Gibbs samples.
    #[serde(default = "default_num_samples")]
    pub default_num_samples: usize,

    /// Default burn-in for Gibbs.
    #[serde(default = "default_burn_in")]
    pub default_burn_in: usize,
}

fn default_algorithm() -> String {
    "ve".to_string()
}

fn default_num_samples() -> usize {
    10_000
}

fn default_burn_in() -> usize {
    1_000
}

impl Default for BnConfig {
    fn default() -> Self {
        Self {
            networks_directory: None,
            default_algorithm: default_algorithm(),
            default_num_samples: default_num_samples(),
            default_burn_in: default_burn_in(),
        }
    }
}

/// Per-request options that can override provider defaults.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BnOptions {
    /// Override inference algorithm for this request.
    #[serde(default)]
    pub algorithm: Option<String>,

    /// Override number of Gibbs samples.
    #[serde(default)]
    pub num_samples: Option<usize>,

    /// Override burn-in period.
    #[serde(default)]
    pub burn_in: Option<usize>,

    /// RNG seed for reproducible Gibbs sampling.
    #[serde(default)]
    pub seed: Option<u64>,

    /// Chunk size for streaming Gibbs inference.
    /// When set and algorithm is "gibbs", `chat_stream()` emits progressive
    /// chunks every `chunk_size` samples instead of a single final result.
    #[serde(default)]
    pub chunk_size: Option<usize>,

    /// LBP: maximum iterations (default 100).
    #[serde(default)]
    pub max_iterations: Option<usize>,

    /// LBP: convergence threshold (default 1e-4).
    #[serde(default)]
    pub convergence_threshold: Option<f64>,

    /// LBP: damping factor (default 0.5).
    #[serde(default)]
    pub damping_factor: Option<f64>,

    /// NUTS: number of post-warmup samples per chain (default 1000).
    #[serde(default)]
    pub nuts_num_samples: Option<u64>,

    /// NUTS: number of warmup/tuning samples per chain (default 500).
    #[serde(default)]
    pub nuts_num_warmup: Option<u64>,

    /// NUTS: maximum tree depth (default 10).
    #[serde(default)]
    pub nuts_max_tree_depth: Option<u64>,

    /// NUTS: number of parallel chains (default 4).
    #[serde(default)]
    pub nuts_num_chains: Option<usize>,

    /// Elimination ordering heuristic for Variable Elimination.
    /// Defaults to `MinFill` when not specified.
    #[serde(default)]
    pub elimination_heuristic: Option<EliminationHeuristic>,
}

/// Input parsed from the user message JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BnInput {
    /// Action: "infer" (default), "query", or "learn".
    #[serde(default = "default_action")]
    pub action: String,

    /// Evidence observations: variable_name → state_name.
    #[serde(default)]
    pub evidence: HashMap<String, String>,

    /// For "query" action: specific variable to query.
    #[serde(default)]
    pub query_variable: Option<String>,

    /// For "learn" action: path to CSV data file.
    #[serde(default)]
    pub data_file: Option<String>,

    /// For "learn" action: Laplace smoothing pseudocount (default 1.0).
    #[serde(default)]
    pub pseudocount: Option<f64>,

    /// For "learn" action: learner algorithm ("mle" or "bayesian", default "mle").
    #[serde(default)]
    pub learner: Option<String>,

    /// For "learn" action with "bayesian" learner: per-variable Dirichlet α values.
    /// Keys are variable names, values are arrays of α per state.
    #[serde(default)]
    pub dirichlet_priors: Option<HashMap<String, Vec<f64>>>,

    /// For "search" action: structure learning algorithm ("hill_climb" or "k2", default "hill_climb").
    #[serde(default)]
    pub structure_learner: Option<String>,

    /// For "search" action with "k2" learner: variable ordering (topological).
    #[serde(default)]
    pub ordering: Option<Vec<String>>,

    /// For "search" action: maximum parents per node.
    #[serde(default)]
    pub max_parents: Option<usize>,

    /// For "search" action with "hill_climb": maximum search steps.
    #[serde(default)]
    pub max_steps: Option<usize>,

    /// For "search" action: scoring function ("bic" or "bdeu", default "bic").
    #[serde(default)]
    pub scoring: Option<String>,

    /// For "search" action with "bdeu" scoring: equivalent sample size.
    #[serde(default)]
    pub equivalent_sample_size: Option<f64>,

    /// Per-request options.
    #[serde(default)]
    pub options: BnOptions,
}

fn default_action() -> String {
    "infer".to_string()
}

/// Observation entry in the evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BnObservation {
    pub variable: String,
    pub state: String,
}

/// Output payload returned in the ChatResponse content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BnOutput {
    /// Posterior marginal distributions: variable → { state → probability }.
    pub marginals: HashMap<String, HashMap<String, f64>>,

    /// Algorithm used for inference.
    pub algorithm: String,

    /// Elapsed time in milliseconds.
    pub elapsed_ms: f64,

    /// Diagnostics (if available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<BnDiagnosticsOutput>,

    /// Number of evidence variables provided.
    pub evidence_count: usize,

    /// Number of variables in the network.
    pub network_size: usize,
}

/// Diagnostics output from inference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BnDiagnosticsOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iterations: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub burn_in: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_marginal_change: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_sample_size: Option<f64>,
}
