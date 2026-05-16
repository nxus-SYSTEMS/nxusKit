//! K2 structure learning algorithm for Bayesian Networks.
//!
//! The K2 algorithm (Cooper & Herskovits, 1992) searches for the best parent set
//! for each variable in a given topological ordering. It greedily adds parents
//! that maximize the scoring function, up to a max_parents limit.

use serde::{Deserialize, Serialize};

use crate::providers::bayesian::error::{BayesError, BayesResult};
use crate::providers::bayesian::learning::scoring::ScoringFunction;
use crate::providers::bayesian::learning::{Dataset, StructureLearner, StructureSearchResult};
use crate::providers::bayesian::network::BayesianNetwork;
use crate::providers::bayesian::types::VariableName;

/// Configuration for K2 structure learning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct K2Config {
    /// Variable ordering (names in topological order).
    /// Variables earlier in the list can only be parents of later variables.
    pub ordering: Vec<String>,

    /// Maximum number of parents per node.
    #[serde(default = "default_max_parents")]
    pub max_parents: usize,

    /// Scoring function to use.
    #[serde(default)]
    pub scoring: ScoringFunction,
}

fn default_max_parents() -> usize {
    3
}

impl Default for K2Config {
    fn default() -> Self {
        Self {
            ordering: Vec::new(),
            max_parents: default_max_parents(),
            scoring: ScoringFunction::BIC,
        }
    }
}

/// K2 structure learner.
#[derive(Debug, Clone)]
pub struct K2Learner {
    config: K2Config,
}

impl K2Learner {
    pub fn new(config: K2Config) -> Self {
        Self { config }
    }

    /// Create with a given ordering and default settings.
    pub fn with_ordering(ordering: Vec<String>) -> Self {
        Self::new(K2Config {
            ordering,
            ..Default::default()
        })
    }
}

impl StructureLearner for K2Learner {
    fn search(
        &self,
        template: &BayesianNetwork,
        data: &Dataset,
    ) -> BayesResult<StructureSearchResult> {
        if self.config.ordering.is_empty() {
            return Err(BayesError::InferenceError(
                "K2 requires a variable ordering".into(),
            ));
        }

        // Validate ordering
        let var_names = template.variable_names();
        let ordering: Vec<VariableName> = self
            .config
            .ordering
            .iter()
            .map(|s| {
                VariableName::new(s).map_err(|e| {
                    BayesError::InferenceError(format!("Invalid variable name '{}': {}", s, e))
                })
            })
            .collect::<BayesResult<Vec<_>>>()?;

        for vn in &ordering {
            if template.variable(vn).is_none() {
                return Err(BayesError::InferenceError(format!(
                    "Variable '{}' in ordering not found in network",
                    vn
                )));
            }
        }

        // Build the result network
        let mut network = BayesianNetwork::new();
        for vn in &var_names {
            let var = template.variable(vn).unwrap();
            network.add_variable(var.clone())?;
        }

        // Set initial CPTs (uniform)
        for vn in &var_names {
            let card = network.variable(vn).unwrap().cardinality();
            network.set_cpt(vn, vec![1.0 / card as f64; card])?;
        }

        let mut total_iterations = 0;

        // For each variable in order, greedily find best parents
        for (idx, vn) in ordering.iter().enumerate() {
            // Candidate parents: only variables earlier in the ordering
            let candidates: Vec<VariableName> = ordering[..idx].to_vec();

            let mut current_parents: Vec<VariableName> = Vec::new();
            let mut current_score =
                self.config
                    .scoring
                    .local_score(vn, &current_parents, &network, data)?;

            loop {
                if current_parents.len() >= self.config.max_parents {
                    break;
                }

                let mut best_candidate: Option<VariableName> = None;
                let mut best_score = current_score;

                for candidate in &candidates {
                    if current_parents.contains(candidate) {
                        continue;
                    }

                    total_iterations += 1;

                    let mut trial_parents = current_parents.clone();
                    trial_parents.push(candidate.clone());

                    let trial_score =
                        self.config
                            .scoring
                            .local_score(vn, &trial_parents, &network, data)?;

                    if trial_score > best_score {
                        best_score = trial_score;
                        best_candidate = Some(candidate.clone());
                    }
                }

                match best_candidate {
                    Some(parent) => {
                        current_parents.push(parent.clone());

                        // Add edge to the network
                        network.add_edge(&parent, vn)?;

                        // Update CPT
                        let parents = network.parents(vn);
                        let mut cpt_size = network.variable(vn).unwrap().cardinality();
                        for p in &parents {
                            cpt_size *= network.variable(p).unwrap().cardinality();
                        }
                        let uniform_val = 1.0 / network.variable(vn).unwrap().cardinality() as f64;
                        network.set_cpt(vn, vec![uniform_val; cpt_size])?;

                        current_score = best_score;
                    }
                    None => break, // No improving parent found
                }
            }
        }

        let total_score = self.config.scoring.score(&network, data)?;

        Ok(StructureSearchResult {
            network,
            score: total_score,
            iterations: total_iterations,
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
    fn k2_ordering_constraint_respected() {
        // K2 with ordering [A, B, C]: A can only be root, B can have A as parent,
        // C can have A and/or B as parents.
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
        for _ in 0..100 {
            rows.push(vec![
                Some(StateIndex::new(1)),
                Some(StateIndex::new(1)),
                Some(StateIndex::new(1)),
            ]);
        }
        let data = Dataset::from_rows(cols, rows).unwrap();

        let learner =
            K2Learner::with_ordering(vec!["A".to_string(), "B".to_string(), "C".to_string()]);
        let result = learner.search(&template, &data).unwrap();

        // A should have no parents (first in ordering)
        assert!(
            result.network.parents(&vn("A")).is_empty(),
            "A should have no parents (first in ordering)"
        );

        // B can only have A as parent (or none)
        let b_parents = result.network.parents(&vn("B"));
        for p in &b_parents {
            assert_eq!(p.as_str(), "A", "B's parent should only be A");
        }

        // C can only have A and/or B as parents (or none)
        let c_parents = result.network.parents(&vn("C"));
        for p in &c_parents {
            assert!(
                p.as_str() == "A" || p.as_str() == "B",
                "C's parents should only be A or B, got '{}'",
                p
            );
        }

        assert!(result.network.topological_sort().len() == result.network.num_variables());
    }

    #[test]
    fn k2_empty_ordering_fails() {
        let mut template = BayesianNetwork::new();
        template.add_variable(var("A", &["0", "1"])).unwrap();
        template.set_cpt(&vn("A"), vec![0.5, 0.5]).unwrap();

        let cols = vec![vn("A")];
        let rows = vec![vec![Some(StateIndex::new(0))]];
        let data = Dataset::from_rows(cols, rows).unwrap();

        let learner = K2Learner::new(K2Config::default());
        let result = learner.search(&template, &data);
        assert!(result.is_err());
    }

    #[test]
    fn k2_max_parents_respected() {
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

        let learner = K2Learner::new(K2Config {
            ordering: vec!["A".into(), "B".into(), "C".into()],
            max_parents: 1,
            ..Default::default()
        });
        let result = learner.search(&template, &data).unwrap();

        for vn_ref in result.network.variable_names() {
            assert!(
                result.network.parents(&vn_ref).len() <= 1,
                "{} has {} parents, max=1",
                vn_ref,
                result.network.parents(&vn_ref).len()
            );
        }
    }

    #[test]
    fn k2_output_is_consistent_with_ordering() {
        let mut template = BayesianNetwork::new();
        template.add_variable(var("X", &["0", "1"])).unwrap();
        template.add_variable(var("Y", &["0", "1"])).unwrap();
        template.set_cpt(&vn("X"), vec![0.5, 0.5]).unwrap();
        template.set_cpt(&vn("Y"), vec![0.5, 0.5]).unwrap();

        let cols = vec![vn("X"), vn("Y")];
        let mut rows = Vec::new();
        for _ in 0..80 {
            rows.push(vec![Some(StateIndex::new(0)), Some(StateIndex::new(0))]);
        }
        for _ in 0..20 {
            rows.push(vec![Some(StateIndex::new(1)), Some(StateIndex::new(1))]);
        }
        let data = Dataset::from_rows(cols, rows).unwrap();

        let learner = K2Learner::with_ordering(vec!["X".into(), "Y".into()]);
        let result = learner.search(&template, &data).unwrap();

        assert!(result.score.is_finite());
        assert!(result.network.topological_sort().len() == result.network.num_variables());
    }
}
