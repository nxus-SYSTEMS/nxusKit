//! BayesianProvider: LLMProvider trait implementation for Bayesian Network inference.
//!
//! Maps evidence (JSON in user messages) to posterior marginals (JSON in response).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::RwLock;

use crate::error::{NxuskitError, Result};
use crate::provider::{LLMProvider, ModelLister};
use crate::types::{
    ChatRequest, ChatResponse, ContentPart, FinishReason, MessageContent, ModelInfo,
    ProviderCapabilities, Role, StreamChunk, TokenCount, TokenUsage,
};

use super::bif::load_bif_file;
use super::config::{BnConfig, BnDiagnosticsOutput, BnInput, BnOutput};
use super::evidence::Evidence;
use super::inference::{
    GibbsSampler, InferenceEngine, InferenceResult, JunctionTree, LoopyBeliefPropagation,
    MomentMatchingInference, NutsSampler, VariableElimination,
};
use super::learning::bayesian::{BayesianConfig, BayesianLearner, DirichletPrior};
use super::learning::hill_climb::{HillClimbConfig, HillClimbLearner};
use super::learning::k2::{K2Config, K2Learner};
use super::learning::mle::{MleConfig, MleLearner};
use super::learning::scoring::ScoringFunction;
use super::learning::{Dataset, ParameterLearner, StructureLearner};
use super::network::BayesianNetwork;
use super::types::VariableName;

/// Bayesian Network inference provider.
///
/// Implements `LLMProvider` by interpreting:
/// - `model` field as the .bif network file name
/// - User message as JSON evidence (variable→state observations)
/// - Response as JSON posterior marginals
#[derive(Debug)]
pub struct BayesianProvider {
    config: BnConfig,
    /// Cache of loaded networks: model name → parsed BayesianNetwork.
    net_cache: Arc<RwLock<HashMap<String, BayesianNetwork>>>,
}

impl BayesianProvider {
    /// Create a new BayesianProvider with the given config.
    pub fn new(config: BnConfig) -> Self {
        Self {
            config,
            net_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a builder for BayesianProvider.
    pub fn builder() -> BayesianProviderBuilder {
        BayesianProviderBuilder::new()
    }

    /// Resolve a model name to a .bif file path.
    fn resolve_model(&self, model_name: &str) -> Result<PathBuf> {
        // Try direct path first
        let direct = Path::new(model_name);
        if direct.exists() {
            return Ok(direct.to_path_buf());
        }

        // Try with .bif extension
        let with_ext = direct.with_extension("bif");
        if with_ext.exists() {
            return Ok(with_ext);
        }

        // Search in networks_directory
        if let Some(ref dir) = self.config.networks_directory {
            let in_dir = dir.join(model_name);
            if in_dir.exists() {
                return Ok(in_dir);
            }
            let in_dir_ext = dir.join(model_name).with_extension("bif");
            if in_dir_ext.exists() {
                return Ok(in_dir_ext);
            }
        }

        // Check BN_MODEL_PATH env
        if let Ok(paths) = std::env::var("BN_MODEL_PATH") {
            for dir in paths.split(':') {
                let p = Path::new(dir).join(model_name);
                if p.exists() {
                    return Ok(p);
                }
                let p_ext = Path::new(dir).join(model_name).with_extension("bif");
                if p_ext.exists() {
                    return Ok(p_ext);
                }
            }
        }

        Err(NxuskitError::InvalidRequest(format!(
            "BIF network file not found: '{}'",
            model_name
        )))
    }

    /// Load and cache a network from a .bif file.
    fn load_network(&self, model_name: &str) -> Result<BayesianNetwork> {
        // Check cache
        {
            let cache = self.net_cache.read();
            if let Some(net) = cache.get(model_name) {
                return Ok(net.clone());
            }
        }

        // Resolve and load
        let path = self.resolve_model(model_name)?;
        let net = load_bif_file(&path).map_err(|e| NxuskitError::Provider {
            status: 500,
            message: format!("Failed to load BIF file '{}': {}", path.display(), e),
        })?;

        // Cache the loaded network
        let mut cache = self.net_cache.write();
        cache.insert(model_name.to_string(), net.clone());
        Ok(net)
    }

    /// Extract text content from MessageContent.
    fn extract_text_content(content: &MessageContent) -> String {
        match content {
            MessageContent::Text(text) => text.clone(),
            MessageContent::Parts(parts) => parts
                .iter()
                .filter_map(|part| match part {
                    ContentPart::Text { text } => Some(text.as_str()),
                    ContentPart::Image { .. } => None,
                })
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }

    /// Convert an InferenceResult + network context into a BnOutput.
    fn result_to_output(
        result: &InferenceResult,
        network: &BayesianNetwork,
        evidence_count: usize,
    ) -> BnOutput {
        let mut marginals = HashMap::new();
        for (var_name, probs) in &result.marginals {
            let var = network.variable(var_name).unwrap();
            let mut state_map = HashMap::new();
            for (i, state) in var.states.iter().enumerate() {
                state_map.insert(state.to_string(), probs[i]);
            }
            marginals.insert(var_name.to_string(), state_map);
        }

        let diagnostics = result.diagnostics.as_ref().map(|d| BnDiagnosticsOutput {
            iterations: if d.iterations > 0 {
                Some(d.iterations)
            } else {
                None
            },
            burn_in: if d.burn_in > 0 { Some(d.burn_in) } else { None },
            max_marginal_change: if d.max_marginal_change > 0.0 {
                Some(d.max_marginal_change)
            } else {
                None
            },
            effective_sample_size: d.effective_sample_size,
        });

        BnOutput {
            marginals,
            algorithm: result.algorithm.clone(),
            elapsed_ms: result.elapsed.as_secs_f64() * 1000.0,
            diagnostics,
            evidence_count,
            network_size: network.num_variables(),
        }
    }

    /// Parse request into (network, evidence, input) tuple.
    fn parse_request(&self, request: &ChatRequest) -> Result<(BayesianNetwork, Evidence, BnInput)> {
        let model_name = &request.model;
        let network = self.load_network(model_name)?;

        let last_user_msg = request
            .messages
            .iter()
            .rev()
            .find(|m| matches!(m.role, Role::User))
            .ok_or_else(|| NxuskitError::InvalidRequest("No user message found".into()))?;

        let text_content = Self::extract_text_content(&last_user_msg.content);
        let input: BnInput = serde_json::from_str(&text_content).map_err(|e| {
            NxuskitError::InvalidRequest(format!("Failed to parse BnInput JSON: {}", e))
        })?;

        let mut evidence = Evidence::new();
        for (var_str, state_str) in &input.evidence {
            let var_name = VariableName::new(var_str).map_err(|e| {
                NxuskitError::InvalidRequest(format!("Invalid variable name '{}': {}", var_str, e))
            })?;
            evidence
                .observe(&network, &var_name, state_str)
                .map_err(|e| NxuskitError::InvalidRequest(format!("Evidence error: {}", e)))?;
        }

        Ok((network, evidence, input))
    }

    /// Run inference and produce BnOutput.
    fn run_inference(
        &self,
        network: &BayesianNetwork,
        evidence: &Evidence,
        input: &BnInput,
    ) -> Result<BnOutput> {
        let algorithm = input
            .options
            .algorithm
            .as_deref()
            .unwrap_or(&self.config.default_algorithm);

        let result = match algorithm {
            "ve" | "variable_elimination" => {
                let ve = match input.options.elimination_heuristic {
                    Some(h) => VariableElimination::with_heuristic(h),
                    None => VariableElimination::new(),
                };
                if let Some(ref qvar) = input.query_variable {
                    let vn = VariableName::new(qvar).map_err(|e| {
                        NxuskitError::InvalidRequest(format!("Invalid variable name: {}", e))
                    })?;
                    let marginal =
                        ve.query(network, evidence, &vn)
                            .map_err(|e| NxuskitError::Provider {
                                status: 500,
                                message: format!("VE query failed: {}", e),
                            })?;
                    let var = network
                        .variable(&vn)
                        .ok_or_else(|| NxuskitError::Provider {
                            status: 500,
                            message: format!("Variable '{}' not found", qvar),
                        })?;
                    let mut marginals = HashMap::new();
                    let mut state_map = HashMap::new();
                    for (i, state) in var.states.iter().enumerate() {
                        state_map.insert(state.to_string(), marginal[i]);
                    }
                    marginals.insert(qvar.clone(), state_map);

                    return Ok(BnOutput {
                        marginals,
                        algorithm: "ve".to_string(),
                        elapsed_ms: 0.0,
                        diagnostics: None,
                        evidence_count: evidence.len(),
                        network_size: network.num_variables(),
                    });
                }
                ve.infer(network, evidence)
                    .map_err(|e| NxuskitError::Provider {
                        status: 500,
                        message: format!("VE inference failed: {}", e),
                    })
            }
            "jt" | "junction_tree" => {
                let jt = JunctionTree::new();
                jt.infer(network, evidence)
                    .map_err(|e| NxuskitError::Provider {
                        status: 500,
                        message: format!("JT inference failed: {}", e),
                    })
            }
            "gibbs" => {
                let num_samples = input
                    .options
                    .num_samples
                    .unwrap_or(self.config.default_num_samples);
                let burn_in = input.options.burn_in.unwrap_or(self.config.default_burn_in);

                let mut gibbs = GibbsSampler::new(num_samples, burn_in);
                if let Some(seed) = input.options.seed {
                    gibbs = gibbs.with_seed(seed);
                }
                gibbs
                    .infer(network, evidence)
                    .map_err(|e| NxuskitError::Provider {
                        status: 500,
                        message: format!("Gibbs inference failed: {}", e),
                    })
            }
            "lbp" | "loopy_bp" | "belief_propagation" => {
                let mut lbp = LoopyBeliefPropagation::new();
                if let Some(max_iter) = input.options.max_iterations {
                    lbp = lbp.max_iterations(max_iter);
                }
                if let Some(threshold) = input.options.convergence_threshold {
                    lbp = lbp.convergence_threshold(threshold);
                }
                if let Some(damping) = input.options.damping_factor {
                    lbp = lbp.damping(damping);
                }
                lbp.infer(network, evidence)
                    .map_err(|e| NxuskitError::Provider {
                        status: 500,
                        message: format!("LBP inference failed: {}", e),
                    })
            }
            "gaussian" | "moment_matching" => {
                let mm = MomentMatchingInference::new();
                mm.infer(network, evidence)
                    .map_err(|e| NxuskitError::Provider {
                        status: 500,
                        message: format!("Gaussian inference failed: {}", e),
                    })
            }
            "nuts" | "hmc" => {
                let mut sampler = NutsSampler::new();
                if let Some(n) = input.options.nuts_num_samples {
                    sampler = sampler.num_samples(n);
                }
                if let Some(n) = input.options.nuts_num_warmup {
                    sampler = sampler.num_warmup(n);
                }
                if let Some(d) = input.options.nuts_max_tree_depth {
                    sampler = sampler.max_tree_depth(d);
                }
                if let Some(s) = input.options.seed {
                    sampler = sampler.seed(s);
                }
                if let Some(c) = input.options.nuts_num_chains {
                    sampler = sampler.num_chains(c);
                }
                sampler
                    .infer(network, evidence)
                    .map_err(|e| NxuskitError::Provider {
                        status: 500,
                        message: format!("NUTS inference failed: {}", e),
                    })
            }
            other => {
                return Err(NxuskitError::InvalidRequest(format!(
                    "Unknown inference algorithm: '{}'. Valid: ve, jt, gibbs, lbp, gaussian, nuts",
                    other
                )));
            }
        }?;

        Ok(Self::result_to_output(&result, network, evidence.len()))
    }

    /// Handle the "learn" action: fit CPTs from CSV data using MLE or Bayesian learning.
    fn handle_learn(
        &self,
        model_name: &str,
        mut network: BayesianNetwork,
        input: &super::config::BnInput,
    ) -> Result<ChatResponse> {
        let data_file = input.data_file.as_deref().ok_or_else(|| {
            NxuskitError::InvalidRequest("'learn' action requires 'data_file' field".into())
        })?;

        let data_path = Path::new(data_file);
        let data = Dataset::from_csv(data_path, &network).map_err(|e| {
            NxuskitError::InvalidRequest(format!("Failed to load dataset '{}': {}", data_file, e))
        })?;

        let pseudocount = input.pseudocount.unwrap_or(1.0);
        let learner_type = input.learner.as_deref().unwrap_or("mle");

        let algorithm_name = match learner_type {
            "mle" => {
                let learner = MleLearner::new(MleConfig {
                    pseudocount,
                    ..Default::default()
                });
                learner
                    .fit(&mut network, &data)
                    .map_err(|e| NxuskitError::Provider {
                        status: 500,
                        message: format!("MLE learning failed: {}", e),
                    })?;
                "mle"
            }
            "bayesian" | "dirichlet" => {
                let prior = if let Some(ref priors_map) = input.dirichlet_priors {
                    DirichletPrior::PerVariable {
                        priors: priors_map.clone(),
                        default_alpha: pseudocount,
                    }
                } else {
                    DirichletPrior::Uniform(pseudocount)
                };
                let learner = BayesianLearner::new(BayesianConfig {
                    prior,
                    ..Default::default()
                });
                learner
                    .fit(&mut network, &data)
                    .map_err(|e| NxuskitError::Provider {
                        status: 500,
                        message: format!("Bayesian learning failed: {}", e),
                    })?;
                "bayesian"
            }
            other => {
                return Err(NxuskitError::InvalidRequest(format!(
                    "Unknown learner: '{}'. Valid: mle, bayesian",
                    other
                )));
            }
        };

        // Compute log-likelihood with learned CPTs
        let ll_learner = MleLearner::with_defaults();
        let ll =
            ll_learner
                .log_likelihood(&network, &data)
                .map_err(|e| NxuskitError::Provider {
                    status: 500,
                    message: format!("Log-likelihood computation failed: {}", e),
                })?;

        // Build output with learned CPTs
        let mut learned_cpts = HashMap::new();
        for vn in network.variable_names() {
            if let Some(factor) = network.cpt(&vn) {
                let probs: Vec<f64> = factor.log_values.iter().map(|lv| lv.exp()).collect();
                learned_cpts.insert(vn.to_string(), probs);
            }
        }

        let output = serde_json::json!({
            "action": "learn",
            "algorithm": algorithm_name,
            "pseudocount": pseudocount,
            "log_likelihood": ll,
            "num_rows": data.num_rows(),
            "num_variables": network.num_variables(),
            "learned_cpts": learned_cpts,
        });

        let content = serde_json::to_string(&output).map_err(|e| NxuskitError::Provider {
            status: 500,
            message: format!("Failed to serialize learn output: {}", e),
        })?;

        let estimated = TokenCount::new(data.num_rows() as u32, network.num_variables() as u32);
        let usage = TokenUsage::estimated_only(estimated);

        // Update the cache with the learned network
        let mut cache = self.net_cache.write();
        cache.insert(model_name.to_string(), network);

        let mut response = ChatResponse::new(content, model_name.to_string(), usage)
            .with_finish_reason(FinishReason::Stop);
        response.provider = self.provider_name().to_string();
        Ok(response)
    }

    /// Handle the "search" action: discover network structure from data.
    fn handle_search(
        &self,
        model_name: &str,
        network: BayesianNetwork,
        input: &super::config::BnInput,
    ) -> Result<ChatResponse> {
        let data_file = input.data_file.as_deref().ok_or_else(|| {
            NxuskitError::InvalidRequest("'search' action requires 'data_file' field".into())
        })?;

        let data_path = Path::new(data_file);
        let data = Dataset::from_csv(data_path, &network).map_err(|e| {
            NxuskitError::InvalidRequest(format!("Failed to load dataset '{}': {}", data_file, e))
        })?;

        // Parse scoring function
        let scoring = match input.scoring.as_deref().unwrap_or("bic") {
            "bic" => ScoringFunction::BIC,
            "bdeu" => {
                let ess = input.equivalent_sample_size.unwrap_or(10.0);
                ScoringFunction::bdeu_with_ess(ess)
            }
            other => {
                return Err(NxuskitError::InvalidRequest(format!(
                    "Unknown scoring function: '{}'. Valid: bic, bdeu",
                    other
                )));
            }
        };

        let learner_type = input.structure_learner.as_deref().unwrap_or("hill_climb");

        let search_result = match learner_type {
            "hill_climb" => {
                let config = HillClimbConfig {
                    scoring: scoring.clone(),
                    max_steps: input.max_steps.unwrap_or(1000),
                    max_parents: input.max_parents.unwrap_or(5),
                    ..Default::default()
                };
                let learner = HillClimbLearner::new(config);
                learner
                    .search(&network, &data)
                    .map_err(|e| NxuskitError::Provider {
                        status: 500,
                        message: format!("Hill-Climb search failed: {}", e),
                    })?
            }
            "k2" => {
                let ordering = input.ordering.clone().ok_or_else(|| {
                    NxuskitError::InvalidRequest(
                        "K2 structure learning requires 'ordering' field".into(),
                    )
                })?;
                let config = K2Config {
                    ordering,
                    max_parents: input.max_parents.unwrap_or(3),
                    scoring: scoring.clone(),
                };
                let learner = K2Learner::new(config);
                learner
                    .search(&network, &data)
                    .map_err(|e| NxuskitError::Provider {
                        status: 500,
                        message: format!("K2 search failed: {}", e),
                    })?
            }
            other => {
                return Err(NxuskitError::InvalidRequest(format!(
                    "Unknown structure learner: '{}'. Valid: hill_climb, k2",
                    other
                )));
            }
        };

        // Build output with discovered edges
        let mut edges: Vec<(String, String)> = Vec::new();
        for vn in search_result.network.variable_names() {
            for parent in search_result.network.parents(&vn) {
                edges.push((parent.to_string(), vn.to_string()));
            }
        }

        let output = serde_json::json!({
            "action": "search",
            "algorithm": learner_type,
            "scoring": input.scoring.as_deref().unwrap_or("bic"),
            "score": search_result.score,
            "iterations": search_result.iterations,
            "num_edges": edges.len(),
            "edges": edges,
            "num_variables": search_result.network.num_variables(),
        });

        let content = serde_json::to_string(&output).map_err(|e| NxuskitError::Provider {
            status: 500,
            message: format!("Failed to serialize search output: {}", e),
        })?;

        let estimated = TokenCount::new(
            data.num_rows() as u32,
            search_result.network.num_variables() as u32,
        );
        let usage = TokenUsage::estimated_only(estimated);

        // Update cache with the discovered network
        let mut cache = self.net_cache.write();
        cache.insert(model_name.to_string(), search_result.network);

        let mut response = ChatResponse::new(content, model_name.to_string(), usage)
            .with_finish_reason(FinishReason::Stop);
        response.provider = self.provider_name().to_string();
        Ok(response)
    }
}

#[async_trait]
impl LLMProvider for BayesianProvider {
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse> {
        let model_name = &request.model;
        let (network, evidence, input) = self.parse_request(request)?;

        if input.action == "learn" {
            return self.handle_learn(model_name, network, &input);
        }

        if input.action == "search" {
            return self.handle_search(model_name, network, &input);
        }

        // Run inference
        let output = self.run_inference(&network, &evidence, &input)?;
        let content = serde_json::to_string(&output).map_err(|e| NxuskitError::Provider {
            status: 500,
            message: format!("Failed to serialize BnOutput: {}", e),
        })?;

        let estimated = TokenCount::new(input.evidence.len() as u32, output.marginals.len() as u32);
        let usage = TokenUsage::estimated_only(estimated);

        let mut response = ChatResponse::new(content, model_name.clone(), usage)
            .with_finish_reason(FinishReason::Stop);
        response.provider = self.provider_name().to_string();
        Ok(response)
    }

    async fn chat_stream(
        &self,
        request: &ChatRequest,
    ) -> Result<Box<dyn futures::Stream<Item = Result<StreamChunk>> + Send + Unpin>> {
        let (network, evidence, input) = self.parse_request(request)?;

        let algorithm = input
            .options
            .algorithm
            .as_deref()
            .unwrap_or(&self.config.default_algorithm);

        // For Gibbs with chunk_size, stream progressive results
        if algorithm == "gibbs"
            && let Some(chunk_size) = input.options.chunk_size
        {
            let num_samples = input
                .options
                .num_samples
                .unwrap_or(self.config.default_num_samples);
            let burn_in = input.options.burn_in.unwrap_or(self.config.default_burn_in);

            let mut gibbs = GibbsSampler::new(num_samples, burn_in);
            if let Some(seed) = input.options.seed {
                gibbs = gibbs.with_seed(seed);
            }

            let bayes_stream = gibbs
                .sample_stream(&network, &evidence, chunk_size)
                .map_err(|e| NxuskitError::Provider {
                    status: 500,
                    message: format!("Gibbs streaming failed: {}", e),
                })?;

            let evidence_count = evidence.len();
            let net = network.clone();

            // Map BayesStreamChunk<InferenceResult> → Result<StreamChunk>
            let mapped = futures::StreamExt::map(bayes_stream, move |bsc| {
                let output = Self::result_to_output(&bsc.data, &net, evidence_count);
                let content =
                    serde_json::to_string(&output).map_err(|e| NxuskitError::Provider {
                        status: 500,
                        message: format!("Failed to serialize BnOutput: {}", e),
                    })?;

                let chunk = if bsc.is_final {
                    let estimated =
                        TokenCount::new(evidence_count as u32, output.marginals.len() as u32);
                    StreamChunk {
                        delta: content,
                        thinking: None,
                        finish_reason: Some(FinishReason::Stop),
                        usage: Some(TokenUsage::estimated_only(estimated)),
                        tool_calls: None,
                        logprobs: None,
                    }
                } else {
                    StreamChunk::new(content)
                };
                Ok(chunk)
            });

            return Ok(Box::new(mapped));
        }

        // For LBP with chunk_size, stream progressive results
        if matches!(algorithm, "lbp" | "loopy_bp" | "belief_propagation")
            && let Some(chunk_size) = input.options.chunk_size
        {
            let mut lbp = LoopyBeliefPropagation::new();
            if let Some(max_iter) = input.options.max_iterations {
                lbp = lbp.max_iterations(max_iter);
            }
            if let Some(threshold) = input.options.convergence_threshold {
                lbp = lbp.convergence_threshold(threshold);
            }
            if let Some(damping) = input.options.damping_factor {
                lbp = lbp.damping(damping);
            }

            let bayes_stream = lbp
                .infer_stream(&network, &evidence, chunk_size)
                .map_err(|e| NxuskitError::Provider {
                    status: 500,
                    message: format!("LBP streaming failed: {}", e),
                })?;

            let evidence_count = evidence.len();
            let net = network.clone();

            let mapped = futures::StreamExt::map(bayes_stream, move |bsc| {
                let output = Self::result_to_output(&bsc.data, &net, evidence_count);
                let content =
                    serde_json::to_string(&output).map_err(|e| NxuskitError::Provider {
                        status: 500,
                        message: format!("Failed to serialize BnOutput: {}", e),
                    })?;

                let chunk = if bsc.is_final {
                    let estimated =
                        TokenCount::new(evidence_count as u32, output.marginals.len() as u32);
                    StreamChunk {
                        delta: content,
                        thinking: None,
                        finish_reason: Some(FinishReason::Stop),
                        usage: Some(TokenUsage::estimated_only(estimated)),
                        tool_calls: None,
                        logprobs: None,
                    }
                } else {
                    StreamChunk::new(content)
                };
                Ok(chunk)
            });

            return Ok(Box::new(mapped));
        }

        // For NUTS with chunk_size, stream progressive results
        if matches!(algorithm, "nuts" | "hmc")
            && let Some(chunk_size) = input.options.chunk_size
        {
            let mut sampler = NutsSampler::new();
            if let Some(n) = input.options.nuts_num_samples {
                sampler = sampler.num_samples(n);
            }
            if let Some(n) = input.options.nuts_num_warmup {
                sampler = sampler.num_warmup(n);
            }
            if let Some(d) = input.options.nuts_max_tree_depth {
                sampler = sampler.max_tree_depth(d);
            }
            if let Some(s) = input.options.seed {
                sampler = sampler.seed(s);
            }
            if let Some(c) = input.options.nuts_num_chains {
                sampler = sampler.num_chains(c);
            }

            let bayes_stream = sampler
                .infer_stream(&network, &evidence, chunk_size)
                .map_err(|e| NxuskitError::Provider {
                    status: 500,
                    message: format!("NUTS streaming failed: {}", e),
                })?;

            let evidence_count = evidence.len();
            let net = network.clone();

            let mapped = futures::StreamExt::map(bayes_stream, move |bsc| {
                let output = Self::result_to_output(&bsc.data, &net, evidence_count);
                let content =
                    serde_json::to_string(&output).map_err(|e| NxuskitError::Provider {
                        status: 500,
                        message: format!("Failed to serialize BnOutput: {}", e),
                    })?;

                let chunk = if bsc.is_final {
                    let estimated =
                        TokenCount::new(evidence_count as u32, output.marginals.len() as u32);
                    StreamChunk {
                        delta: content,
                        thinking: None,
                        finish_reason: Some(FinishReason::Stop),
                        usage: Some(TokenUsage::estimated_only(estimated)),
                        tool_calls: None,
                        logprobs: None,
                    }
                } else {
                    StreamChunk::new(content)
                };
                Ok(chunk)
            });

            return Ok(Box::new(mapped));
        }

        // For non-streaming algorithms (VE, JT) or Gibbs without chunk_size,
        // return full result as a single chunk.
        let output = self.run_inference(&network, &evidence, &input)?;
        let content = serde_json::to_string(&output).map_err(|e| NxuskitError::Provider {
            status: 500,
            message: format!("Failed to serialize BnOutput: {}", e),
        })?;

        let estimated = TokenCount::new(input.evidence.len() as u32, output.marginals.len() as u32);
        let chunk = StreamChunk {
            delta: content,
            thinking: None,
            finish_reason: Some(FinishReason::Stop),
            usage: Some(TokenUsage::estimated_only(estimated)),
            tool_calls: None,
            logprobs: None,
        };
        Ok(Box::new(futures::stream::iter(vec![Ok(chunk)])))
    }

    async fn stream_with_usage(
        &self,
        request: &ChatRequest,
    ) -> Result<(
        Box<dyn futures::Stream<Item = Result<StreamChunk>> + Send + Unpin>,
        tokio::sync::oneshot::Receiver<TokenUsage>,
    )> {
        let response = self.chat(request).await?;
        let usage = response.usage.clone();

        let chunk = StreamChunk {
            delta: response.content,
            thinking: None,
            finish_reason: Some(FinishReason::Stop),
            usage: None,
            tool_calls: None,
            logprobs: None,
        };

        let (tx, rx) = tokio::sync::oneshot::channel();
        let _ = tx.send(usage);

        Ok((Box::new(futures::stream::iter(vec![Ok(chunk)])), rx))
    }

    fn provider_name(&self) -> &str {
        "bn"
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let mut models = Vec::new();

        if let Some(ref dir) = self.config.networks_directory
            && dir.is_dir()
        {
            Self::scan_directory(dir, &mut models);
        }

        // Also check BN_MODEL_PATH
        if let Ok(paths) = std::env::var("BN_MODEL_PATH") {
            for dir in paths.split(':') {
                let p = Path::new(dir);
                if p.is_dir() {
                    Self::scan_directory(p, &mut models);
                }
            }
        }

        Ok(models)
    }

    fn get_capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_system_messages: false,
            supports_streaming: true,
            supports_vision: false,
            supports_json_mode: true,
            ..Default::default()
        }
    }
}

impl BayesianProvider {
    fn scan_directory(dir: &Path, models: &mut Vec<ModelInfo>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("bif")
                    && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                {
                    let mut info = ModelInfo::new(stem);
                    if let Ok(meta) = path.metadata() {
                        info.size_bytes = Some(meta.len());
                    }
                    info.description = Some(format!("BIF network: {}", path.display()));
                    models.push(info);
                }
            }
        }
    }
}

#[async_trait]
impl ModelLister for BayesianProvider {
    async fn list_available_models(&self) -> Result<Vec<ModelInfo>> {
        self.list_models().await
    }
}

/// Builder for BayesianProvider.
#[derive(Debug, Default)]
pub struct BayesianProviderBuilder {
    config: BnConfig,
}

impl BayesianProviderBuilder {
    /// Create a new builder with default configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the directory to scan for `.bif` network files.
    pub fn networks_directory(mut self, path: impl Into<PathBuf>) -> Self {
        self.config.networks_directory = Some(path.into());
        self
    }

    /// Set the default inference algorithm (`"ve"`, `"jt"`, `"gibbs"`, or `"lbp"`).
    pub fn default_algorithm(mut self, algo: &str) -> Self {
        self.config.default_algorithm = algo.to_string();
        self
    }

    /// Set the default number of Gibbs samples (default: 10,000).
    pub fn default_num_samples(mut self, n: usize) -> Self {
        self.config.default_num_samples = n;
        self
    }

    /// Set the default Gibbs burn-in period (default: 1,000).
    pub fn default_burn_in(mut self, n: usize) -> Self {
        self.config.default_burn_in = n;
        self
    }

    /// Build the `BayesianProvider` with the configured settings.
    pub fn build(self) -> Result<BayesianProvider> {
        Ok(BayesianProvider::new(self.config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Message;

    fn fixture_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bn")
    }

    #[tokio::test]
    async fn provider_chat_asia_ve() {
        let provider = BayesianProvider::builder()
            .networks_directory(fixture_dir())
            .build()
            .unwrap();

        let input_json = r#"{"action":"infer","evidence":{}}"#;
        let request = ChatRequest::new("asia.bif").with_message(Message::user(input_json));

        let response = provider.chat(&request).await.unwrap();

        let output: BnOutput = serde_json::from_str(&response.content).unwrap();
        assert_eq!(output.algorithm, "ve");
        assert_eq!(output.network_size, 8);
        assert_eq!(output.evidence_count, 0);
        assert_eq!(output.marginals.len(), 8);

        // Check Smoking prior is 0.5/0.5
        let smoking = output.marginals.get("Smoking").unwrap();
        assert!((smoking.get("yes").unwrap() - 0.5).abs() < 1e-6);
        assert!((smoking.get("no").unwrap() - 0.5).abs() < 1e-6);
    }

    #[tokio::test]
    async fn provider_chat_with_evidence() {
        let provider = BayesianProvider::builder()
            .networks_directory(fixture_dir())
            .build()
            .unwrap();

        let input_json = r#"{"action":"infer","evidence":{"Smoking":"yes"}}"#;
        let request = ChatRequest::new("asia.bif").with_message(Message::user(input_json));

        let response = provider.chat(&request).await.unwrap();
        let output: BnOutput = serde_json::from_str(&response.content).unwrap();
        assert_eq!(output.evidence_count, 1);
        // Smoking should not be in marginals (it's observed)
        assert!(!output.marginals.contains_key("Smoking"));
    }

    #[tokio::test]
    async fn provider_chat_gibbs() {
        let provider = BayesianProvider::builder()
            .networks_directory(fixture_dir())
            .build()
            .unwrap();

        let input_json = r#"{"action":"infer","evidence":{},"options":{"algorithm":"gibbs","num_samples":1000,"burn_in":100,"seed":42}}"#;
        let request = ChatRequest::new("asia.bif").with_message(Message::user(input_json));

        let response = provider.chat(&request).await.unwrap();
        let output: BnOutput = serde_json::from_str(&response.content).unwrap();
        assert_eq!(output.algorithm, "gibbs");
        assert!(output.diagnostics.is_some());
    }

    #[tokio::test]
    async fn provider_list_models() {
        let provider = BayesianProvider::builder()
            .networks_directory(fixture_dir())
            .build()
            .unwrap();

        let models = provider.list_models().await.unwrap();
        assert!(models.len() >= 4); // asia, cancer, earthquake, survey, alarm
    }

    #[tokio::test]
    async fn provider_invalid_model() {
        let provider = BayesianProvider::builder().build().unwrap();

        let input_json = r#"{"action":"infer","evidence":{}}"#;
        let request = ChatRequest::new("nonexistent.bif").with_message(Message::user(input_json));

        let result = provider.chat(&request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn provider_name_is_bn() {
        let provider = BayesianProvider::builder().build().unwrap();
        assert_eq!(provider.provider_name(), "bn");
    }

    #[tokio::test]
    async fn provider_search_hill_climb() {
        let provider = BayesianProvider::builder()
            .networks_directory(fixture_dir())
            .build()
            .unwrap();

        let csv_path = fixture_dir().join("cancer_data.csv");
        let input_json = format!(
            r#"{{"action":"search","data_file":"{}","scoring":"bic"}}"#,
            csv_path.display()
        );
        let request = ChatRequest::new("cancer.bif").with_message(Message::user(&input_json));

        let response = provider.chat(&request).await.unwrap();
        let output: serde_json::Value = serde_json::from_str(&response.content).unwrap();
        assert_eq!(output["action"], "search");
        assert_eq!(output["algorithm"], "hill_climb");
        assert!(output["score"].as_f64().unwrap().is_finite());
        assert!(output["num_variables"].as_u64().unwrap() == 5);
    }

    #[tokio::test]
    async fn provider_search_k2() {
        let provider = BayesianProvider::builder()
            .networks_directory(fixture_dir())
            .build()
            .unwrap();

        let csv_path = fixture_dir().join("cancer_data.csv");
        let input_json = format!(
            r#"{{"action":"search","data_file":"{}","structure_learner":"k2","ordering":["Pollution","Smoker","Cancer","Xray","Dyspnea"]}}"#,
            csv_path.display()
        );
        let request = ChatRequest::new("cancer.bif").with_message(Message::user(&input_json));

        let response = provider.chat(&request).await.unwrap();
        let output: serde_json::Value = serde_json::from_str(&response.content).unwrap();
        assert_eq!(output["action"], "search");
        assert_eq!(output["algorithm"], "k2");
        assert!(output["score"].as_f64().unwrap().is_finite());
    }

    #[tokio::test]
    async fn provider_chat_lbp() {
        let provider = BayesianProvider::builder()
            .networks_directory(fixture_dir())
            .build()
            .unwrap();

        let input_json = r#"{"action":"infer","evidence":{},"options":{"algorithm":"lbp","max_iterations":100,"convergence_threshold":1e-6,"damping_factor":0.5}}"#;
        let request = ChatRequest::new("asia.bif").with_message(Message::user(input_json));

        let response = provider.chat(&request).await.unwrap();
        let output: BnOutput = serde_json::from_str(&response.content).unwrap();
        assert_eq!(output.algorithm, "lbp");
        assert_eq!(output.network_size, 8);
        assert_eq!(output.evidence_count, 0);
        assert_eq!(output.marginals.len(), 8);
        assert!(output.diagnostics.is_some());
    }

    #[tokio::test]
    async fn provider_chat_lbp_with_evidence() {
        let provider = BayesianProvider::builder()
            .networks_directory(fixture_dir())
            .build()
            .unwrap();

        let input_json =
            r#"{"action":"infer","evidence":{"Smoking":"yes"},"options":{"algorithm":"lbp"}}"#;
        let request = ChatRequest::new("asia.bif").with_message(Message::user(input_json));

        let response = provider.chat(&request).await.unwrap();
        let output: BnOutput = serde_json::from_str(&response.content).unwrap();
        assert_eq!(output.algorithm, "lbp");
        assert_eq!(output.evidence_count, 1);
        assert!(!output.marginals.contains_key("Smoking"));
    }
}
