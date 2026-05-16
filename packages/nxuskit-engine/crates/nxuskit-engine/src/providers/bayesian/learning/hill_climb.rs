//! Hill-Climb structure learning for Bayesian Networks.
//!
//! Greedy search over DAG space using edge add/remove/reverse operators.
//! Scores structures using BIC or BDeu scoring functions.

use serde::{Deserialize, Serialize};

use crate::providers::bayesian::error::{BayesError, BayesResult};
use crate::providers::bayesian::learning::scoring::ScoringFunction;
use crate::providers::bayesian::learning::{Dataset, StructureLearner, StructureSearchResult};
use crate::providers::bayesian::network::BayesianNetwork;
use crate::providers::bayesian::types::VariableName;

/// Configuration for Hill-Climb structure learning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HillClimbConfig {
    /// Scoring function to use.
    #[serde(default)]
    pub scoring: ScoringFunction,

    /// Maximum number of iterations (0 = unlimited).
    #[serde(default = "default_max_steps")]
    pub max_steps: usize,

    /// Maximum number of parents per node (0 = unlimited).
    #[serde(default)]
    pub max_parents: usize,

    /// Convergence threshold: stop if best improvement < this value.
    #[serde(default = "default_threshold")]
    pub threshold: f64,
}

fn default_max_steps() -> usize {
    1000
}

fn default_threshold() -> f64 {
    1e-8
}

impl Default for HillClimbConfig {
    fn default() -> Self {
        Self {
            scoring: ScoringFunction::BIC,
            max_steps: default_max_steps(),
            max_parents: 0,
            threshold: default_threshold(),
        }
    }
}

/// Hill-Climb structure learner.
#[derive(Debug, Clone)]
pub struct HillClimbLearner {
    config: HillClimbConfig,
}

/// An operation on the DAG.
#[derive(Debug, Clone)]
#[allow(clippy::enum_variant_names)]
enum Operation {
    AddEdge(VariableName, VariableName),
    RemoveEdge(VariableName, VariableName),
    ReverseEdge(VariableName, VariableName),
}

impl HillClimbLearner {
    pub fn new(config: HillClimbConfig) -> Self {
        Self { config }
    }

    /// Create with BIC scoring and default settings.
    pub fn with_bic() -> Self {
        Self::new(HillClimbConfig::default())
    }

    /// Create with BDeu scoring.
    pub fn with_bdeu(ess: f64) -> Self {
        Self::new(HillClimbConfig {
            scoring: ScoringFunction::bdeu_with_ess(ess),
            ..Default::default()
        })
    }

    /// Check if adding edge from→to would create a cycle.
    fn would_create_cycle(
        network: &BayesianNetwork,
        from: &VariableName,
        to: &VariableName,
    ) -> bool {
        // Adding from→to creates a cycle if there's already a path from `to` to `from`.
        // BFS/DFS from `to` following existing edges to see if `from` is reachable.
        let mut visited = std::collections::HashSet::new();
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(to.clone());

        while let Some(current) = queue.pop_front() {
            if &current == from {
                return true;
            }
            if !visited.insert(current.clone()) {
                continue;
            }
            for child in network.children(&current) {
                if !visited.contains(&child) {
                    queue.push_back(child);
                }
            }
        }
        false
    }

    /// Find the best single-step operation and its score improvement.
    fn find_best_operation(
        &self,
        network: &BayesianNetwork,
        data: &Dataset,
        current_local_scores: &std::collections::HashMap<String, f64>,
    ) -> BayesResult<Option<(Operation, f64)>> {
        let var_names = network.variable_names();
        let mut best_op: Option<Operation> = None;
        let mut best_improvement = 0.0;

        for from in &var_names {
            for to in &var_names {
                if from == to {
                    continue;
                }

                let has_edge = network.parents(to).contains(from);

                // Try adding edge from→to
                if !has_edge {
                    // Check max_parents constraint
                    if self.config.max_parents > 0
                        && network.parents(to).len() >= self.config.max_parents
                    {
                        continue;
                    }

                    // Check acyclicity
                    if Self::would_create_cycle(network, from, to) {
                        continue;
                    }

                    // Compute score change: only `to` changes its local score
                    let mut new_parents = network.parents(to);
                    new_parents.push(from.clone());
                    let new_local =
                        self.config
                            .scoring
                            .local_score(to, &new_parents, network, data)?;
                    let old_local = current_local_scores[to.as_str()];
                    let improvement = new_local - old_local;

                    if improvement > best_improvement {
                        best_improvement = improvement;
                        best_op = Some(Operation::AddEdge(from.clone(), to.clone()));
                    }
                }

                // Try removing edge from→to
                if has_edge {
                    let new_parents: Vec<_> = network
                        .parents(to)
                        .into_iter()
                        .filter(|p| p != from)
                        .collect();
                    let new_local =
                        self.config
                            .scoring
                            .local_score(to, &new_parents, network, data)?;
                    let old_local = current_local_scores[to.as_str()];
                    let improvement = new_local - old_local;

                    if improvement > best_improvement {
                        best_improvement = improvement;
                        best_op = Some(Operation::RemoveEdge(from.clone(), to.clone()));
                    }
                }

                // Try reversing edge from→to (becomes to→from)
                if has_edge {
                    // Check if reverse edge already exists
                    if network.parents(from).contains(to) {
                        continue;
                    }
                    // Check max_parents for `from` gaining a parent
                    if self.config.max_parents > 0
                        && network.parents(from).len() >= self.config.max_parents
                    {
                        continue;
                    }

                    // Temporarily compute: remove from→to, add to→from
                    // This changes local scores for both `to` and `from`
                    let to_new_parents: Vec<_> = network
                        .parents(to)
                        .into_iter()
                        .filter(|p| p != from)
                        .collect();

                    let mut from_new_parents = network.parents(from);
                    from_new_parents.push(to.clone());

                    // Check acyclicity of the reversed edge
                    // We need a temp network to check — build the modified parent set
                    // Simple check: after reversing, would to→from create a cycle?
                    // This is equivalent to checking if `from` can reach `to` without the from→to edge.
                    // We approximate by building temp check.
                    let mut temp_net = network.clone();
                    temp_net.remove_edge(from, to).ok();
                    if Self::would_create_cycle(&temp_net, to, from) {
                        continue;
                    }

                    let to_new_local =
                        self.config
                            .scoring
                            .local_score(to, &to_new_parents, network, data)?;
                    let from_new_local =
                        self.config
                            .scoring
                            .local_score(from, &from_new_parents, network, data)?;

                    let old_to = current_local_scores[to.as_str()];
                    let old_from = current_local_scores[from.as_str()];
                    let improvement = (to_new_local + from_new_local) - (old_to + old_from);

                    if improvement > best_improvement {
                        best_improvement = improvement;
                        best_op = Some(Operation::ReverseEdge(from.clone(), to.clone()));
                    }
                }
            }
        }

        if let Some(op) = best_op
            && best_improvement > self.config.threshold
        {
            return Ok(Some((op, best_improvement)));
        }

        Ok(None)
    }
}

impl StructureLearner for HillClimbLearner {
    fn search(
        &self,
        template: &BayesianNetwork,
        data: &Dataset,
    ) -> BayesResult<StructureSearchResult> {
        // Start with an empty structure (no edges) but same variables
        let var_names = template.variable_names();
        let mut network = BayesianNetwork::new();
        for vn in &var_names {
            let var = template.variable(vn).ok_or_else(|| {
                BayesError::InferenceError(format!("Variable '{}' not found in template", vn))
            })?;
            network.add_variable(var.clone())?;
        }

        // Set uniform CPTs for all variables (needed for scoring)
        for vn in &var_names {
            let card = network.variable(vn).unwrap().cardinality();
            let uniform = vec![1.0 / card as f64; card];
            network.set_cpt(vn, uniform)?;
        }

        // Compute initial local scores
        let mut local_scores: std::collections::HashMap<String, f64> =
            std::collections::HashMap::new();
        for vn in &var_names {
            let parents = network.parents(vn);
            let score = self
                .config
                .scoring
                .local_score(vn, &parents, &network, data)?;
            local_scores.insert(vn.to_string(), score);
        }

        let mut total_score: f64 = local_scores.values().sum();
        let mut iterations = 0;
        let max_steps = if self.config.max_steps == 0 {
            usize::MAX
        } else {
            self.config.max_steps
        };

        while iterations < max_steps {
            iterations += 1;

            let best = self.find_best_operation(&network, data, &local_scores)?;

            match best {
                None => break, // No improving operation found
                Some((op, _improvement)) => {
                    match op {
                        Operation::AddEdge(ref from, ref to) => {
                            network.add_edge(from, to)?;
                            // Update CPT for `to` to accommodate new parent
                            let parents = network.parents(to);
                            let mut cpt_size = network.variable(to).unwrap().cardinality();
                            for p in &parents {
                                cpt_size *= network.variable(p).unwrap().cardinality();
                            }
                            let uniform_val =
                                1.0 / network.variable(to).unwrap().cardinality() as f64;
                            network.set_cpt(to, vec![uniform_val; cpt_size])?;

                            // Recompute local score for `to`
                            let new_score = self
                                .config
                                .scoring
                                .local_score(to, &parents, &network, data)?;
                            local_scores.insert(to.to_string(), new_score);
                        }
                        Operation::RemoveEdge(ref from, ref to) => {
                            network.remove_edge(from, to).map_err(|e| {
                                BayesError::InferenceError(format!("Failed to remove edge: {}", e))
                            })?;
                            // Update CPT for `to`
                            let parents = network.parents(to);
                            let mut cpt_size = network.variable(to).unwrap().cardinality();
                            for p in &parents {
                                cpt_size *= network.variable(p).unwrap().cardinality();
                            }
                            let uniform_val =
                                1.0 / network.variable(to).unwrap().cardinality() as f64;
                            network.set_cpt(to, vec![uniform_val; cpt_size])?;

                            let new_score = self
                                .config
                                .scoring
                                .local_score(to, &parents, &network, data)?;
                            local_scores.insert(to.to_string(), new_score);
                        }
                        Operation::ReverseEdge(ref from, ref to) => {
                            network.remove_edge(from, to).map_err(|e| {
                                BayesError::InferenceError(format!("Failed to remove edge: {}", e))
                            })?;
                            network.add_edge(to, from)?;

                            // Update CPTs for both `to` and `from`
                            for vn in &[to.clone(), from.clone()] {
                                let parents = network.parents(vn);
                                let mut cpt_size = network.variable(vn).unwrap().cardinality();
                                for p in &parents {
                                    cpt_size *= network.variable(p).unwrap().cardinality();
                                }
                                let uniform_val =
                                    1.0 / network.variable(vn).unwrap().cardinality() as f64;
                                network.set_cpt(vn, vec![uniform_val; cpt_size])?;

                                let new_score = self
                                    .config
                                    .scoring
                                    .local_score(vn, &parents, &network, data)?;
                                local_scores.insert(vn.to_string(), new_score);
                            }
                        }
                    }
                    total_score = local_scores.values().sum();
                }
            }
        }

        Ok(StructureSearchResult {
            network,
            score: total_score,
            iterations,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::bayesian::learning::Dataset;
    use crate::providers::bayesian::types::{DiscreteVariable, StateIndex, StateName};

    fn vn(name: &str) -> VariableName {
        VariableName::new(name).unwrap()
    }

    fn sn(name: &str) -> StateName {
        StateName::new(name).unwrap()
    }

    fn var(name: &str, states: &[&str]) -> DiscreteVariable {
        DiscreteVariable::new(vn(name), states.iter().map(|s| sn(s)).collect()).unwrap()
    }

    #[test]
    fn hill_climb_discovers_simple_dependency() {
        // Data: A strongly determines B (A=0 → B=0, A=1 → B=1)
        let mut template = BayesianNetwork::new();
        template.add_variable(var("A", &["0", "1"])).unwrap();
        template.add_variable(var("B", &["0", "1"])).unwrap();
        template.set_cpt(&vn("A"), vec![0.5, 0.5]).unwrap();
        template.set_cpt(&vn("B"), vec![0.5, 0.5]).unwrap();

        let cols = vec![vn("A"), vn("B")];
        let mut rows = Vec::new();
        for _ in 0..100 {
            rows.push(vec![Some(StateIndex::new(0)), Some(StateIndex::new(0))]);
        }
        for _ in 0..100 {
            rows.push(vec![Some(StateIndex::new(1)), Some(StateIndex::new(1))]);
        }
        let data = Dataset::from_rows(cols, rows).unwrap();

        let learner = HillClimbLearner::with_bic();
        let result = learner.search(&template, &data).unwrap();

        // Should discover an edge (either A→B or B→A)
        let a_parents = result.network.parents(&vn("A"));
        let b_parents = result.network.parents(&vn("B"));
        let has_edge = !a_parents.is_empty() || !b_parents.is_empty();
        assert!(
            has_edge,
            "Hill-climb should discover dependency between A and B"
        );
        assert!(result.iterations > 0);
    }

    #[test]
    fn hill_climb_result_is_dag() {
        let mut template = BayesianNetwork::new();
        template.add_variable(var("A", &["0", "1"])).unwrap();
        template.add_variable(var("B", &["0", "1"])).unwrap();
        template.add_variable(var("C", &["0", "1"])).unwrap();
        template.set_cpt(&vn("A"), vec![0.5, 0.5]).unwrap();
        template.set_cpt(&vn("B"), vec![0.5, 0.5]).unwrap();
        template.set_cpt(&vn("C"), vec![0.5, 0.5]).unwrap();

        let cols = vec![vn("A"), vn("B"), vn("C")];
        let mut rows = Vec::new();
        for _ in 0..50 {
            rows.push(vec![
                Some(StateIndex::new(0)),
                Some(StateIndex::new(0)),
                Some(StateIndex::new(0)),
            ]);
        }
        for _ in 0..50 {
            rows.push(vec![
                Some(StateIndex::new(1)),
                Some(StateIndex::new(1)),
                Some(StateIndex::new(1)),
            ]);
        }
        let data = Dataset::from_rows(cols, rows).unwrap();

        let learner = HillClimbLearner::with_bic();
        let result = learner.search(&template, &data).unwrap();

        // Verify it's a valid DAG (topological sort should succeed)
        assert!(
            result.network.topological_sort().len() == result.network.num_variables(),
            "Result should be a valid DAG"
        );
        assert!(result.score.is_finite());
    }

    #[test]
    fn hill_climb_max_parents_respected() {
        let mut template = BayesianNetwork::new();
        template.add_variable(var("A", &["0", "1"])).unwrap();
        template.add_variable(var("B", &["0", "1"])).unwrap();
        template.add_variable(var("C", &["0", "1"])).unwrap();
        template.set_cpt(&vn("A"), vec![0.5, 0.5]).unwrap();
        template.set_cpt(&vn("B"), vec![0.5, 0.5]).unwrap();
        template.set_cpt(&vn("C"), vec![0.5, 0.5]).unwrap();

        let cols = vec![vn("A"), vn("B"), vn("C")];
        let mut rows = Vec::new();
        for _ in 0..100 {
            rows.push(vec![
                Some(StateIndex::new(0)),
                Some(StateIndex::new(0)),
                Some(StateIndex::new(0)),
            ]);
        }
        let data = Dataset::from_rows(cols, rows).unwrap();

        let learner = HillClimbLearner::new(HillClimbConfig {
            max_parents: 1,
            ..Default::default()
        });
        let result = learner.search(&template, &data).unwrap();

        // No node should have more than 1 parent
        for vn_ref in result.network.variable_names() {
            let parents = result.network.parents(&vn_ref);
            assert!(
                parents.len() <= 1,
                "Variable '{}' has {} parents, max_parents=1",
                vn_ref,
                parents.len()
            );
        }
    }

    #[test]
    fn hill_climb_with_bdeu() {
        let mut template = BayesianNetwork::new();
        template.add_variable(var("A", &["0", "1"])).unwrap();
        template.add_variable(var("B", &["0", "1"])).unwrap();
        template.set_cpt(&vn("A"), vec![0.5, 0.5]).unwrap();
        template.set_cpt(&vn("B"), vec![0.5, 0.5]).unwrap();

        let cols = vec![vn("A"), vn("B")];
        let mut rows = Vec::new();
        for _ in 0..80 {
            rows.push(vec![Some(StateIndex::new(0)), Some(StateIndex::new(0))]);
        }
        for _ in 0..20 {
            rows.push(vec![Some(StateIndex::new(1)), Some(StateIndex::new(1))]);
        }
        let data = Dataset::from_rows(cols, rows).unwrap();

        let learner = HillClimbLearner::with_bdeu(10.0);
        let result = learner.search(&template, &data).unwrap();
        assert!(result.score.is_finite());
        assert!(result.network.topological_sort().len() == result.network.num_variables());
    }
}
