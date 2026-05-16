//! `nxuskit-cli bn` - Bayesian network commands.
//!
//! - `bn infer` - inference over a fully-specified network (FR-007).
//! - `bn learn` - parameter learning (CPDs) from a CSV dataset given a network
//!   skeleton; output is the learned network, BIF-exportable (FR-006).
//! - `bn evidence` - validate/normalize an observations map against a network
//!   (FR-006).

use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::cli_error::CliError;
use crate::envelope::TraceFields;
use crate::output::{OutputFormat, OutputWriter};

#[derive(Debug, Subcommand)]
pub enum BnAction {
    /// Run Bayesian network inference
    Infer(BnInferArgs),
    /// Learn CPDs from a CSV dataset given a network skeleton (parameter learning)
    Learn(BnLearnArgs),
    /// Validate and normalize an observations map against a network
    Evidence(BnEvidenceArgs),
}

#[derive(Debug, Args)]
pub struct BnInferArgs {
    /// Input file path or `-` for stdin.
    ///
    /// JSON: {"network": {"nodes": [...], "edges": [...], "cpds": {...}},
    /// "evidence": {...}, "query_nodes": [...]}
    #[arg(short, long)]
    pub input: String,

    #[arg(short, long, default_value = "json")]
    pub format: String,

    #[arg(short, long, default_value_t = false)]
    pub quiet: bool,

    #[arg(short, long)]
    pub output: Option<String>,
}

#[derive(Debug, Args)]
pub struct BnLearnArgs {
    /// Input file path or `-` for stdin.
    ///
    /// JSON object with fields: `network` (a skeleton with `nodes` of
    /// `{name, states}` and `edges` of `{from, to}` -- no `cpds`); `data_file`
    /// (path to the CSV dataset; column headers map to variable names);
    /// `learner` ("mle" or "bayesian", default "mle"); `pseudocount` (number,
    /// default 1.0); `dirichlet_priors` (`{var: alpha}`, bayesian learner only).
    #[arg(short, long)]
    pub input: String,

    #[arg(short, long, default_value = "json")]
    pub format: String,

    #[arg(short, long, default_value_t = false)]
    pub quiet: bool,

    #[arg(short, long)]
    pub output: Option<String>,
}

#[derive(Debug, Args)]
pub struct BnEvidenceArgs {
    /// Input file path or `-` for stdin.
    ///
    /// JSON: {"network": {"nodes": [...], "edges": [...], "cpds": {...}},
    /// "observations": {var: state, ...}}. Each observation is validated against
    /// the network's variables and states.
    #[arg(short, long)]
    pub input: String,

    #[arg(short, long, default_value = "json")]
    pub format: String,

    #[arg(short, long, default_value_t = false)]
    pub quiet: bool,

    #[arg(short, long)]
    pub output: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BnInferInput {
    pub network: NetworkDef,
    #[serde(default)]
    pub evidence: HashMap<String, String>,
    pub query_nodes: Vec<String>,
    #[serde(default = "default_algorithm")]
    pub algorithm: String,
}

#[derive(Debug, Deserialize)]
pub struct NetworkDef {
    pub nodes: Vec<NodeDef>,
    pub edges: Vec<EdgeDef>,
    pub cpds: HashMap<String, CpdDef>,
}

#[derive(Debug, Deserialize)]
pub struct NodeDef {
    pub name: String,
    pub states: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct EdgeDef {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Deserialize)]
pub struct CpdDef {
    pub probabilities: Vec<f64>,
}

fn default_algorithm() -> String {
    "variable_elimination".to_string()
}

#[derive(Debug, Serialize)]
pub struct BnInferResult {
    pub posteriors: HashMap<String, HashMap<String, f64>>,
    pub algorithm: String,
    pub elapsed_ms: f64,
}

// ── `bn learn` ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct BnLearnInput {
    /// Network skeleton: nodes (name + states) and edges (from -> to). No CPDs.
    pub network: NetworkSkeleton,
    /// Path to the CSV dataset (columns map to variable names).
    pub data_file: String,
    /// Learner: `"mle"` (default) or `"bayesian"`.
    #[serde(default = "default_learner")]
    pub learner: String,
    /// Laplace pseudocount (default 1.0). For `bayesian`, used as the default Dirichlet alpha.
    #[serde(default = "default_pseudocount")]
    pub pseudocount: f64,
}

#[derive(Debug, Deserialize)]
pub struct NetworkSkeleton {
    pub nodes: Vec<NodeDef>,
    #[serde(default)]
    pub edges: Vec<EdgeDef>,
}

fn default_learner() -> String {
    "mle".to_string()
}

fn default_pseudocount() -> f64 {
    1.0
}

#[derive(Debug, Serialize)]
pub struct BnLearnResult {
    /// Learned conditional probability tables, keyed by variable name.
    pub learned_cpts: HashMap<String, Vec<f64>>,
    /// The learned network serialized to BIF text (BIF-exportable).
    pub bif: String,
    /// Number of training rows consumed.
    pub num_rows: u32,
    /// Number of variables in the network.
    pub num_variables: u32,
    /// Which learner was used (`"mle"` or `"bayesian"`).
    pub learner: String,
    pub elapsed_ms: f64,
}

// ── `bn evidence` ───────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct BnEvidenceInput {
    pub network: NetworkDef,
    #[serde(default)]
    pub observations: HashMap<String, String>,
}

#[derive(Debug, Serialize)]
pub struct BnEvidenceResult {
    pub valid: bool,
    /// The normalized/validated observations (echoed back against the network).
    pub evidence: HashMap<String, String>,
    pub observation_count: u32,
    pub elapsed_ms: f64,
}

pub async fn run_bn_command(action: BnAction) -> Result<(), CliError> {
    match action {
        BnAction::Infer(args) => run_bn_infer(args).await,
        BnAction::Learn(args) => run_bn_learn(args).await,
        BnAction::Evidence(args) => run_bn_evidence(args).await,
    }
}

async fn run_bn_learn(args: BnLearnArgs) -> Result<(), CliError> {
    use nxuskit_engine::providers::bayesian::{
        BayesianConfig, BayesianLearner, BayesianNetwork, Dataset, DirichletPrior,
        DiscreteVariable, MleConfig, MleLearner, ParameterLearner, StateName, VariableName, bif,
    };

    let format = OutputFormat::parse(&args.format)?;
    let raw_input = OutputWriter::read_input(&args.input)?;
    let input: BnLearnInput =
        serde_json::from_str(&raw_input).map_err(|e| CliError::CommandValidation {
            code: "parse_error",
            message: format!("Invalid BN learn input: {e}"),
            details: None,
        })?;

    let trace = TraceFields::new("bn_learn", &raw_input, None, None);
    let writer = OutputWriter::new(format, args.quiet, args.output);
    let start = std::time::Instant::now();

    // Build the network skeleton (no CPDs yet).
    let mut network = BayesianNetwork::new();
    for node in &input.network.nodes {
        let var_name = VariableName::new(&node.name).map_err(|e| CliError::ValidationFailed {
            message: format!("Invalid variable name '{}': {e}", node.name),
        })?;
        let states: Vec<StateName> = node
            .states
            .iter()
            .map(StateName::new)
            .collect::<Result<_, _>>()
            .map_err(|e| CliError::ValidationFailed {
                message: format!("Invalid state name in '{}': {e}", node.name),
            })?;
        let var =
            DiscreteVariable::new(var_name, states).map_err(|e| CliError::ValidationFailed {
                message: format!("Failed to create variable '{}': {e}", node.name),
            })?;
        network
            .add_variable(var)
            .map_err(|e| CliError::ValidationFailed {
                message: format!("Failed to add variable '{}': {e}", node.name),
            })?;
    }
    for edge in &input.network.edges {
        let from = VariableName::new(&edge.from).map_err(|e| CliError::ValidationFailed {
            message: format!("Invalid edge source '{}': {e}", edge.from),
        })?;
        let to = VariableName::new(&edge.to).map_err(|e| CliError::ValidationFailed {
            message: format!("Invalid edge target '{}': {e}", edge.to),
        })?;
        network
            .add_edge(&from, &to)
            .map_err(|e| CliError::ValidationFailed {
                message: format!("Failed to add edge {} -> {}: {e}", edge.from, edge.to),
            })?;
    }

    // Load the CSV dataset.
    let data_path = std::path::Path::new(&input.data_file);
    if !data_path.exists() {
        return Err(CliError::ValidationFailed {
            message: format!("Training CSV not found: {}", input.data_file),
        });
    }
    let data = Dataset::from_csv(data_path, &network).map_err(|e| CliError::ValidationFailed {
        message: format!("Failed to load dataset '{}': {e}", input.data_file),
    })?;

    // Fit the requested learner.
    let learner_name = match input.learner.as_str() {
        "mle" => {
            let learner = MleLearner::new(MleConfig {
                pseudocount: input.pseudocount,
                ..Default::default()
            });
            learner
                .fit(&mut network, &data)
                .map_err(|e| CliError::ProviderError {
                    message: format!("MLE learning failed: {e}"),
                })?;
            "mle"
        }
        "bayesian" | "dirichlet" => {
            let learner = BayesianLearner::new(BayesianConfig {
                prior: DirichletPrior::Uniform(input.pseudocount),
                ..Default::default()
            });
            learner
                .fit(&mut network, &data)
                .map_err(|e| CliError::ProviderError {
                    message: format!("Bayesian learning failed: {e}"),
                })?;
            "bayesian"
        }
        other => {
            return Err(CliError::ValidationFailed {
                message: format!("Unknown learner '{other}'. Valid: mle, bayesian"),
            });
        }
    };

    // Extract the learned CPTs (probability form) and BIF text.
    let mut learned_cpts: HashMap<String, Vec<f64>> = HashMap::new();
    for vn in network.variable_names() {
        if let Some(factor) = network.cpt(&vn) {
            let probs: Vec<f64> = factor.log_values.iter().map(|lv| lv.exp()).collect();
            learned_cpts.insert(vn.to_string(), probs);
        }
    }
    let bif_text = bif::serialize_bif(&network).map_err(|e| CliError::ProviderError {
        message: format!("Failed to serialize learned network to BIF: {e}"),
    })?;

    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
    let result = BnLearnResult {
        learned_cpts,
        bif: bif_text,
        num_rows: data.num_rows() as u32,
        num_variables: network.num_variables() as u32,
        learner: learner_name.to_string(),
        elapsed_ms: elapsed,
    };
    writer.write_response(result, trace, None, None, Some(elapsed))
}

async fn run_bn_evidence(args: BnEvidenceArgs) -> Result<(), CliError> {
    use nxuskit_engine::providers::bayesian::{
        BayesianNetwork, DiscreteVariable, Evidence, StateName, VariableName,
    };

    let format = OutputFormat::parse(&args.format)?;
    let raw_input = OutputWriter::read_input(&args.input)?;
    let input: BnEvidenceInput =
        serde_json::from_str(&raw_input).map_err(|e| CliError::CommandValidation {
            code: "parse_error",
            message: format!("Invalid BN evidence input: {e}"),
            details: None,
        })?;

    let trace = TraceFields::new("bn_evidence", &raw_input, None, None);
    let writer = OutputWriter::new(format, args.quiet, args.output);
    let start = std::time::Instant::now();

    // Build the network (nodes/edges/CPDs are present for `bn evidence`, mirroring `bn infer`).
    let mut network = BayesianNetwork::new();
    for node in &input.network.nodes {
        let var_name = VariableName::new(&node.name).map_err(|e| CliError::ValidationFailed {
            message: format!("Invalid variable name '{}': {e}", node.name),
        })?;
        let states: Vec<StateName> = node
            .states
            .iter()
            .map(StateName::new)
            .collect::<Result<_, _>>()
            .map_err(|e| CliError::ValidationFailed {
                message: format!("Invalid state name in '{}': {e}", node.name),
            })?;
        let var =
            DiscreteVariable::new(var_name, states).map_err(|e| CliError::ValidationFailed {
                message: format!("Failed to create variable '{}': {e}", node.name),
            })?;
        network
            .add_variable(var)
            .map_err(|e| CliError::ValidationFailed {
                message: format!("Failed to add variable '{}': {e}", node.name),
            })?;
    }
    for edge in &input.network.edges {
        let from = VariableName::new(&edge.from).map_err(|e| CliError::ValidationFailed {
            message: format!("Invalid edge source '{}': {e}", edge.from),
        })?;
        let to = VariableName::new(&edge.to).map_err(|e| CliError::ValidationFailed {
            message: format!("Invalid edge target '{}': {e}", edge.to),
        })?;
        network
            .add_edge(&from, &to)
            .map_err(|e| CliError::ValidationFailed {
                message: format!("Failed to add edge {} -> {}: {e}", edge.from, edge.to),
            })?;
    }
    for (node_name, cpd) in &input.network.cpds {
        let var_name = VariableName::new(node_name).map_err(|e| CliError::ValidationFailed {
            message: format!("Invalid CPD variable name '{}': {e}", node_name),
        })?;
        network
            .set_cpt(&var_name, cpd.probabilities.clone())
            .map_err(|e| CliError::ValidationFailed {
                message: format!("Failed to set CPD for '{}': {e}", node_name),
            })?;
    }

    // Validate each observation against the network.
    let mut evidence = Evidence::new();
    let mut validated: HashMap<String, String> = HashMap::new();
    for (var, state) in &input.observations {
        let var_name = VariableName::new(var).map_err(|e| CliError::ValidationFailed {
            message: format!("Invalid observation variable '{var}': {e}"),
        })?;
        evidence
            .observe(&network, &var_name, state)
            .map_err(|e| CliError::ValidationFailed {
                message: format!("Invalid observation {var}={state}: {e}"),
            })?;
        validated.insert(var.clone(), state.clone());
    }

    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
    let result = BnEvidenceResult {
        valid: true,
        observation_count: validated.len() as u32,
        evidence: validated,
        elapsed_ms: elapsed,
    };
    writer.write_response(result, trace, None, None, Some(elapsed))
}

async fn run_bn_infer(args: BnInferArgs) -> Result<(), CliError> {
    let format = OutputFormat::parse(&args.format)?;
    let raw_input = OutputWriter::read_input(&args.input)?;
    let infer_input: BnInferInput =
        serde_json::from_str(&raw_input).map_err(|e| CliError::ParseError {
            message: format!("Invalid BN inference input: {e}"),
        })?;

    let trace = TraceFields::new("bn_infer", &raw_input, None, None);
    let writer = OutputWriter::new(format, args.quiet, args.output);
    let start = std::time::Instant::now();

    let posteriors = run_inference(&infer_input)?;
    let elapsed = start.elapsed().as_secs_f64() * 1000.0;

    let result = BnInferResult {
        posteriors,
        algorithm: infer_input.algorithm,
        elapsed_ms: elapsed,
    };

    writer.write_response(result, trace, None, None, Some(elapsed))
}

pub(crate) fn run_inference(
    input: &BnInferInput,
) -> Result<HashMap<String, HashMap<String, f64>>, CliError> {
    use nxuskit_engine::providers::bayesian::{
        BayesianNetwork, DiscreteVariable, Evidence, InferenceEngine, StateName,
        VariableElimination, VariableName,
    };

    let mut network = BayesianNetwork::new();

    // Add nodes
    for node in &input.network.nodes {
        let var_name = VariableName::new(&node.name).map_err(|e| CliError::ProviderError {
            message: format!("Invalid variable name '{}': {e}", node.name),
        })?;
        let states: Vec<StateName> = node
            .states
            .iter()
            .map(StateName::new)
            .collect::<Result<_, _>>()
            .map_err(|e| CliError::ProviderError {
                message: format!("Invalid state name: {e}"),
            })?;
        let var = DiscreteVariable::new(var_name, states).map_err(|e| CliError::ProviderError {
            message: format!("Failed to create variable '{}': {e}", node.name),
        })?;
        network
            .add_variable(var)
            .map_err(|e| CliError::ProviderError {
                message: format!("Failed to add variable '{}': {e}", node.name),
            })?;
    }

    // Add edges
    for edge in &input.network.edges {
        let from = VariableName::new(&edge.from).map_err(|e| CliError::ProviderError {
            message: format!("Invalid edge source '{}': {e}", edge.from),
        })?;
        let to = VariableName::new(&edge.to).map_err(|e| CliError::ProviderError {
            message: format!("Invalid edge target '{}': {e}", edge.to),
        })?;
        network
            .add_edge(&from, &to)
            .map_err(|e| CliError::ProviderError {
                message: format!("Failed to add edge {} -> {}: {e}", edge.from, edge.to),
            })?;
    }

    // Set CPDs
    for (node_name, cpd) in &input.network.cpds {
        let var_name = VariableName::new(node_name).map_err(|e| CliError::ProviderError {
            message: format!("Invalid CPD variable name '{}': {e}", node_name),
        })?;
        network
            .set_cpt(&var_name, cpd.probabilities.clone())
            .map_err(|e| CliError::ProviderError {
                message: format!("Failed to set CPD for '{}': {e}", node_name),
            })?;
    }

    // Build evidence
    let mut evidence = Evidence::new();
    for (var, state) in &input.evidence {
        let var_name = VariableName::new(var).map_err(|e| CliError::ProviderError {
            message: format!("Invalid evidence variable '{}': {e}", var),
        })?;
        evidence
            .observe(&network, &var_name, state)
            .map_err(|e| CliError::ProviderError {
                message: format!("Failed to set evidence for '{}': {e}", var),
            })?;
    }

    // Run inference
    let engine = VariableElimination::new();
    let result = engine
        .infer(&network, &evidence)
        .map_err(|e| CliError::ProviderError {
            message: format!("Inference failed: {e}"),
        })?;

    // Extract posteriors for query nodes — marginals is HashMap<VariableName, Vec<f64>>
    let mut posteriors = HashMap::new();
    for node_name in &input.query_nodes {
        let var_name = VariableName::new(node_name).map_err(|e| CliError::ProviderError {
            message: format!("Invalid query node '{}': {e}", node_name),
        })?;
        if let Some(marginal) = result.marginals.get(&var_name) {
            // Map indices back to state names
            let node_def = input.network.nodes.iter().find(|n| n.name == *node_name);
            let mut state_probs = HashMap::new();
            if let Some(node) = node_def {
                for (i, prob) in marginal.iter().enumerate() {
                    let state_name = node
                        .states
                        .get(i)
                        .cloned()
                        .unwrap_or_else(|| format!("state_{i}"));
                    state_probs.insert(state_name, *prob);
                }
            }
            posteriors.insert(node_name.clone(), state_probs);
        }
    }

    Ok(posteriors)
}
