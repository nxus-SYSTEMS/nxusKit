//! Variable Elimination inference algorithm.
//!
//! Exact inference using factor operations: reduce evidence, eliminate variables
//! in greedy min-fill order, normalize to get posteriors.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use super::{EliminationHeuristic, InferenceEngine, InferenceResult, Marginal};
use crate::providers::bayesian::error::{BayesError, BayesResult};
use crate::providers::bayesian::evidence::Evidence;
use crate::providers::bayesian::factor::Factor;
use crate::providers::bayesian::network::BayesianNetwork;
use crate::providers::bayesian::types::VariableName;

/// Variable Elimination with configurable elimination ordering heuristic.
///
/// Supports two heuristics:
/// - **MinFill** (default): Greedily eliminate the variable that introduces
///   the fewest new fill edges in the moral graph.
/// - **MinWeight**: Greedily eliminate the variable whose elimination produces
///   the smallest intermediate factor, measured as the product of domain sizes
///   of the variable and all its current neighbors in the elimination graph.
#[derive(Debug, Clone)]
pub struct VariableElimination {
    heuristic: EliminationHeuristic,
}

impl Default for VariableElimination {
    fn default() -> Self {
        Self {
            heuristic: EliminationHeuristic::MinFill,
        }
    }
}

impl VariableElimination {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a `VariableElimination` engine with the given elimination heuristic.
    pub fn with_heuristic(heuristic: EliminationHeuristic) -> Self {
        Self { heuristic }
    }

    /// Return the domain size (cardinality) of a variable.
    /// Falls back to 2 for unknown variables (should not happen in practice).
    fn domain_size(network: &BayesianNetwork, var: &VariableName) -> usize {
        network.variable(var).map(|v| v.cardinality()).unwrap_or(2)
    }

    /// Compute the elimination order for the given variables using the
    /// configured heuristic.
    fn elimination_order(
        &self,
        network: &BayesianNetwork,
        query_vars: &HashSet<VariableName>,
        evidence_vars: &HashSet<VariableName>,
    ) -> Vec<VariableName> {
        // Variables to eliminate = all network vars - query vars - evidence vars
        let mut to_eliminate: Vec<VariableName> = network
            .variable_names()
            .into_iter()
            .filter(|v| !query_vars.contains(v) && !evidence_vars.contains(v))
            .collect();

        // Build adjacency from factor scopes (moral graph)
        let mut adj: HashMap<VariableName, HashSet<VariableName>> = HashMap::new();
        for name in network.variable_names() {
            adj.entry(name).or_default();
        }
        for cpt in network.cpts().values() {
            // All variables in the same CPT scope are neighbors in the moral graph
            for i in 0..cpt.variables.len() {
                for j in (i + 1)..cpt.variables.len() {
                    adj.entry(cpt.variables[i].clone())
                        .or_default()
                        .insert(cpt.variables[j].clone());
                    adj.entry(cpt.variables[j].clone())
                        .or_default()
                        .insert(cpt.variables[i].clone());
                }
            }
        }

        let mut order = Vec::new();
        while !to_eliminate.is_empty() {
            let best_idx = to_eliminate
                .iter()
                .enumerate()
                .min_by_key(|(_, var)| {
                    let neighbors: Vec<_> = adj
                        .get(*var)
                        .map(|s| s.iter().filter(|n| to_eliminate.contains(n)).collect())
                        .unwrap_or_default();

                    match self.heuristic {
                        EliminationHeuristic::MinFill => {
                            // Count fill edges needed
                            let mut fill: usize = 0;
                            for i in 0..neighbors.len() {
                                for j in (i + 1)..neighbors.len() {
                                    if !adj
                                        .get(neighbors[i])
                                        .map(|s| s.contains(neighbors[j]))
                                        .unwrap_or(false)
                                    {
                                        fill += 1;
                                    }
                                }
                            }
                            fill
                        }
                        EliminationHeuristic::MinWeight => {
                            // Product of domain sizes: variable * all its active neighbors.
                            // This estimates the size of the intermediate factor created
                            // when eliminating this variable.
                            let mut weight = Self::domain_size(network, var);
                            for n in &neighbors {
                                weight = weight.saturating_mul(Self::domain_size(network, n));
                            }
                            weight
                        }
                    }
                })
                .map(|(i, _)| i)
                .unwrap();

            let var = to_eliminate.remove(best_idx);

            // Add fill edges (required for both heuristics — they operate on
            // the evolving elimination graph)
            let neighbors: Vec<_> = adj
                .get(&var)
                .map(|s| {
                    s.iter()
                        .filter(|n| to_eliminate.contains(n))
                        .cloned()
                        .collect()
                })
                .unwrap_or_default();
            for i in 0..neighbors.len() {
                for j in (i + 1)..neighbors.len() {
                    adj.entry(neighbors[i].clone())
                        .or_default()
                        .insert(neighbors[j].clone());
                    adj.entry(neighbors[j].clone())
                        .or_default()
                        .insert(neighbors[i].clone());
                }
            }

            order.push(var);
        }

        order
    }
}

impl InferenceEngine for VariableElimination {
    fn infer(
        &self,
        network: &BayesianNetwork,
        evidence: &Evidence,
    ) -> BayesResult<InferenceResult> {
        let start = Instant::now();
        network.validate()?;

        let evidence_vars: HashSet<VariableName> =
            evidence.observations().keys().cloned().collect();

        // Collect all factors (CPTs), reducing by evidence
        let mut factors: Vec<Factor> = Vec::new();
        for cpt in network.cpts().values() {
            let mut factor = cpt.clone();
            // Reduce factor by any observed variables in its scope
            for ev_var in &evidence_vars {
                if factor.variables.contains(ev_var)
                    && let Some(state_idx) = evidence.get(ev_var)
                {
                    factor = factor.reduce(ev_var, state_idx)?;
                }
            }
            factors.push(factor);
        }

        // Compute marginals for all unobserved variables
        let query_vars: HashSet<VariableName> = network
            .variable_names()
            .into_iter()
            .filter(|v| !evidence_vars.contains(v))
            .collect();

        let mut marginals = HashMap::new();
        let mut log_marginals = HashMap::new();

        for query_var in &query_vars {
            // For each query variable, eliminate all other non-evidence, non-query variables
            let single_query: HashSet<VariableName> = std::iter::once(query_var.clone()).collect();
            let elim_order = self.elimination_order(network, &single_query, &evidence_vars);

            let mut working_factors = factors.clone();

            for elim_var in &elim_order {
                // Collect all factors that mention this variable
                let (relevant, remaining): (Vec<_>, Vec<_>) = working_factors
                    .into_iter()
                    .partition(|f| f.variables.contains(elim_var));

                if relevant.is_empty() {
                    working_factors = remaining;
                    continue;
                }

                // Multiply all relevant factors together
                let mut product = relevant[0].clone();
                for f in &relevant[1..] {
                    product = product.multiply(f)?;
                }

                // Marginalize out the elimination variable
                let marginalized = product.marginalize(elim_var)?;
                working_factors = remaining;
                working_factors.push(marginalized);
            }

            // Multiply remaining factors and normalize
            if working_factors.is_empty() {
                continue;
            }
            let mut result_factor = working_factors[0].clone();
            for f in &working_factors[1..] {
                result_factor = result_factor.multiply(f)?;
            }

            // Marginalize out any remaining variables that aren't the query variable
            let vars_to_remove: Vec<VariableName> = result_factor
                .variables
                .iter()
                .filter(|v| *v != query_var)
                .cloned()
                .collect();
            for v in &vars_to_remove {
                result_factor = result_factor.marginalize(v)?;
            }

            let normalized = result_factor.normalize()?;
            let probs = normalized.to_probabilities();
            let logs = normalized.log_values.clone();

            marginals.insert(query_var.clone(), probs);
            log_marginals.insert(query_var.clone(), logs);
        }

        let elapsed = start.elapsed();

        Ok(InferenceResult {
            marginals,
            log_marginals,
            algorithm: "ve".to_string(),
            elapsed,
            diagnostics: None,
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
        // For a single variable query, we can be more efficient:
        // only eliminate variables not in the query set
        let result = self.infer(network, evidence)?;
        result.marginals.get(variable).cloned().ok_or_else(|| {
            BayesError::InferenceError(format!(
                "Variable '{}' not found in inference results",
                variable
            ))
        })
    }

    fn algorithm_name(&self) -> &str {
        "variable_elimination"
    }
}

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;
    use crate::providers::bayesian::bif::load_bif_file;
    use crate::providers::bayesian::types::{DiscreteVariable, StateName};

    fn var_name(s: &str) -> VariableName {
        VariableName::new(s).unwrap()
    }

    fn state(s: &str) -> StateName {
        StateName::new(s).unwrap()
    }

    fn load_asia() -> BayesianNetwork {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bn/asia.bif");
        load_bif_file(&path).unwrap()
    }

    fn load_reference(name: &str) -> HashMap<String, HashMap<String, f64>> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("tests/fixtures/bn/reference/{}", name));
        let content = std::fs::read_to_string(&path).unwrap();
        serde_json::from_str(&content).unwrap()
    }

    #[test]
    fn ve_asia_prior_marginals() {
        let net = load_asia();
        let evidence = Evidence::new();
        let ve = VariableElimination::new();

        let result = ve.infer(&net, &evidence).unwrap();
        let reference = load_reference("asia_prior_marginals.json");

        for (var_name_str, ref_dist) in &reference {
            let vn = var_name(var_name_str);
            let computed = result
                .marginals
                .get(&vn)
                .unwrap_or_else(|| panic!("Missing marginal for '{}'", var_name_str));
            let var = net.variable(&vn).unwrap();
            for (state_name, &ref_prob) in ref_dist {
                let idx = var.state_index(state_name).unwrap().value();
                let diff = (computed[idx] - ref_prob).abs();
                assert!(
                    diff < 1e-6,
                    "P({}={}) = {}, expected {}, diff = {}",
                    var_name_str,
                    state_name,
                    computed[idx],
                    ref_prob,
                    diff
                );
            }
        }
    }

    #[test]
    fn ve_asia_with_evidence() {
        let net = load_asia();
        let mut evidence = Evidence::new();
        evidence
            .observe(&net, &var_name("Xray"), "positive")
            .unwrap();
        evidence
            .observe(&net, &var_name("Dyspnea"), "present")
            .unwrap();

        let ve = VariableElimination::new();
        let result = ve.infer(&net, &evidence).unwrap();

        // Should have marginals for all unobserved variables
        assert!(result.marginals.contains_key(&var_name("Bronchitis")));
        assert!(result.marginals.contains_key(&var_name("LungCancer")));
        assert!(!result.marginals.contains_key(&var_name("Xray")));
        assert!(!result.marginals.contains_key(&var_name("Dyspnea")));

        // Each marginal should sum to ~1
        for (name, probs) in &result.marginals {
            let sum: f64 = probs.iter().sum();
            assert!(
                (sum - 1.0).abs() < 1e-6,
                "Marginal for '{}' sums to {}, not 1.0",
                name,
                sum
            );
        }
    }

    #[test]
    fn ve_single_variable_query() {
        let net = load_asia();
        let evidence = Evidence::new();
        let ve = VariableElimination::new();

        let marginal = ve.query(&net, &evidence, &var_name("Smoking")).unwrap();
        assert_eq!(marginal.len(), 2);
        assert!((marginal[0] - 0.5).abs() < 1e-6);
        assert!((marginal[1] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn ve_cancer_prior_marginals() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bn/cancer.bif");
        let net = load_bif_file(&path).unwrap();
        let evidence = Evidence::new();
        let ve = VariableElimination::new();

        let result = ve.infer(&net, &evidence).unwrap();
        let reference = load_reference("cancer_prior_marginals.json");

        for (var_name_str, ref_dist) in &reference {
            let vn = var_name(var_name_str);
            let computed = result.marginals.get(&vn).unwrap();
            let var = net.variable(&vn).unwrap();
            for (state_name, &ref_prob) in ref_dist {
                let idx = var.state_index(state_name).unwrap().value();
                let diff = (computed[idx] - ref_prob).abs();
                assert!(
                    diff < 1e-6,
                    "Cancer net: P({}={}) = {}, expected {}, diff = {}",
                    var_name_str,
                    state_name,
                    computed[idx],
                    ref_prob,
                    diff
                );
            }
        }
    }

    #[test]
    fn ve_incomplete_network_fails() {
        let mut net = BayesianNetwork::new();
        net.add_variable(
            DiscreteVariable::new(var_name("A"), vec![state("yes"), state("no")]).unwrap(),
        )
        .unwrap();
        // No CPT set

        let evidence = Evidence::new();
        let ve = VariableElimination::new();
        assert!(ve.infer(&net, &evidence).is_err());
    }
}
