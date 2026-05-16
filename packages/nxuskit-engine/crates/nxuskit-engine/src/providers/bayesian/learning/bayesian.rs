//! Bayesian Parameter Learning with Dirichlet priors.
//!
//! Computes posterior CPTs using conjugate Dirichlet-Multinomial updates:
//!   P(state | parents) = (count + α_state) / (total_count + Σα)
//!
//! Supports uniform priors (same α for all states) and per-variable custom priors.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::providers::bayesian::error::{BayesError, BayesResult};
use crate::providers::bayesian::learning::mle::MissingStrategy;
use crate::providers::bayesian::learning::{Dataset, ParameterLearner};
use crate::providers::bayesian::network::BayesianNetwork;
use crate::providers::bayesian::types::VariableName;

/// Dirichlet prior specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DirichletPrior {
    /// Uniform prior: same α for every state of every variable.
    /// α=1.0 is equivalent to Laplace smoothing (uniform Dirichlet).
    /// α=0.5 is the Jeffreys prior.
    /// α=0.0 gives pure MLE (no prior).
    Uniform(f64),

    /// Per-variable priors: variable name → `Vec<f64>` of α values (one per state).
    /// Variables not in the map fall back to the `default_alpha`.
    PerVariable {
        /// Variable-specific priors.
        priors: HashMap<String, Vec<f64>>,
        /// Default α for variables without a specific prior.
        default_alpha: f64,
    },
}

impl Default for DirichletPrior {
    fn default() -> Self {
        DirichletPrior::Uniform(1.0)
    }
}

/// Configuration for Bayesian parameter learning.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BayesianConfig {
    /// Dirichlet prior specification.
    #[serde(default)]
    pub prior: DirichletPrior,

    /// Strategy for handling missing values.
    #[serde(default)]
    pub missing_strategy: MissingStrategy,
}

/// Bayesian parameter learner using Dirichlet priors.
///
/// Estimates CPTs using conjugate Dirichlet-Multinomial updates.
/// The posterior for each parent configuration row is:
///   P(state_i | parents) = (count_i + α_i) / (Σ count_i + Σ α_i)
#[derive(Debug, Clone)]
pub struct BayesianLearner {
    config: BayesianConfig,
}

impl BayesianLearner {
    pub fn new(config: BayesianConfig) -> Self {
        Self { config }
    }

    /// Create with uniform Dirichlet prior (α=1.0 = Laplace smoothing).
    pub fn with_uniform_prior(alpha: f64) -> Self {
        Self::new(BayesianConfig {
            prior: DirichletPrior::Uniform(alpha),
            ..Default::default()
        })
    }

    /// Create with per-variable custom priors.
    pub fn with_custom_priors(priors: HashMap<String, Vec<f64>>, default_alpha: f64) -> Self {
        Self::new(BayesianConfig {
            prior: DirichletPrior::PerVariable {
                priors,
                default_alpha,
            },
            ..Default::default()
        })
    }

    /// Get the Dirichlet α values for a variable.
    fn get_alphas(&self, variable: &VariableName, num_states: usize) -> Vec<f64> {
        match &self.config.prior {
            DirichletPrior::Uniform(alpha) => vec![*alpha; num_states],
            DirichletPrior::PerVariable {
                priors,
                default_alpha,
            } => {
                if let Some(alphas) = priors.get(variable.as_str()) {
                    if alphas.len() == num_states {
                        alphas.clone()
                    } else {
                        // Wrong size → fall back to default
                        vec![*default_alpha; num_states]
                    }
                } else {
                    vec![*default_alpha; num_states]
                }
            }
        }
    }

    /// Count parent-child configurations for a single variable.
    fn count_configurations(
        &self,
        variable: &VariableName,
        parents: &[VariableName],
        network: &BayesianNetwork,
        data: &Dataset,
    ) -> BayesResult<Vec<f64>> {
        let var = network.variable(variable).ok_or_else(|| {
            BayesError::InferenceError(format!("Variable '{}' not found", variable))
        })?;
        let child_card = var.cardinality();

        // Compute total CPT size
        let mut cpt_size = child_card;
        for pn in parents {
            let pvar = network
                .variable(pn)
                .ok_or_else(|| BayesError::InferenceError(format!("Parent '{}' not found", pn)))?;
            cpt_size *= pvar.cardinality();
        }

        // Get alpha values for this variable
        let alphas = self.get_alphas(variable, child_card);

        // Initialize counts with Dirichlet α per parent configuration
        let num_parent_configs = cpt_size / child_card;
        let mut counts = Vec::with_capacity(cpt_size);
        for _pc in 0..num_parent_configs {
            counts.extend_from_slice(&alphas);
        }

        let child_col = match data.column_index(variable) {
            Some(idx) => idx,
            None => return Ok(counts),
        };

        // Precompute parent column indices
        let parent_cols: Vec<Option<usize>> =
            parents.iter().map(|pn| data.column_index(pn)).collect();

        for row in &data.rows {
            let child_state = match row[child_col] {
                Some(si) => si,
                None => continue,
            };

            let mut all_present = true;
            let mut parent_states = Vec::with_capacity(parents.len());

            for (i, _pn) in parents.iter().enumerate() {
                let pcol = match parent_cols[i] {
                    Some(idx) => idx,
                    None => {
                        all_present = false;
                        break;
                    }
                };
                match row[pcol] {
                    Some(si) => parent_states.push(si),
                    None => {
                        all_present = false;
                        break;
                    }
                }
            }

            if !all_present {
                continue;
            }

            // Compute flat index
            let mut flat_idx = 0;
            let mut stride = child_card;
            for (pn, ps) in parents.iter().zip(parent_states.iter()).rev() {
                let parent_var = network.variable(pn).unwrap();
                flat_idx += ps.value() * stride;
                stride *= parent_var.cardinality();
            }
            flat_idx += child_state.value();

            counts[flat_idx] += 1.0;
        }

        Ok(counts)
    }
}

impl ParameterLearner for BayesianLearner {
    fn fit(&self, network: &mut BayesianNetwork, data: &Dataset) -> BayesResult<()> {
        let var_names = network.variable_names();

        for vn in &var_names {
            let parents = network.parents(vn);
            let counts = self.count_configurations(vn, &parents, network, data)?;

            let child_card = network.variable(vn).unwrap().cardinality();

            // Normalize each parent configuration row
            let num_parent_configs = counts.len() / child_card;
            let mut cpt = vec![0.0; counts.len()];

            for pc in 0..num_parent_configs {
                let start = pc * child_card;
                let end = start + child_card;
                let row_sum: f64 = counts[start..end].iter().sum();

                if row_sum > 0.0 {
                    for (c, cnt) in cpt[start..end].iter_mut().zip(counts[start..end].iter()) {
                        *c = cnt / row_sum;
                    }
                } else {
                    let uniform = 1.0 / child_card as f64;
                    for c in &mut cpt[start..end] {
                        *c = uniform;
                    }
                }
            }

            network.set_cpt(vn, cpt)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::bayesian::learning::mle::MleLearner;
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

    fn read_cpt(net: &BayesianNetwork, name: &str) -> Vec<f64> {
        net.cpt(&vn(name))
            .unwrap()
            .log_values
            .iter()
            .map(|lv| lv.exp())
            .collect()
    }

    #[test]
    fn uniform_prior_matches_closed_form() {
        // Uniform Dirichlet (α=1) should match (count+1)/(total+K)
        let mut net = BayesianNetwork::new();
        net.add_variable(var("X", &["a", "b", "c"])).unwrap();
        net.set_cpt(&vn("X"), vec![1.0 / 3.0; 3]).unwrap();

        let cols = vec![vn("X")];
        let rows = vec![
            vec![Some(StateIndex::new(0))], // a
            vec![Some(StateIndex::new(0))], // a
            vec![Some(StateIndex::new(0))], // a
            vec![Some(StateIndex::new(1))], // b
        ];
        let data = Dataset::from_rows(cols, rows).unwrap();

        let learner = BayesianLearner::with_uniform_prior(1.0);
        learner.fit(&mut net, &data).unwrap();

        let cpt = read_cpt(&net, "X");
        // Closed form: (count+α)/(total+Σα) = (3+1)/(4+3)=4/7, (1+1)/(4+3)=2/7, (0+1)/(4+3)=1/7
        assert!((cpt[0] - 4.0 / 7.0).abs() < 1e-6, "P(a)={}", cpt[0]);
        assert!((cpt[1] - 2.0 / 7.0).abs() < 1e-6, "P(b)={}", cpt[1]);
        assert!((cpt[2] - 1.0 / 7.0).abs() < 1e-6, "P(c)={}", cpt[2]);
    }

    #[test]
    fn uniform_prior_alpha1_matches_mle_laplace() {
        // BayesianLearner(α=1) should produce identical results to MleLearner(pseudocount=1)
        let mut net_bayes = BayesianNetwork::new();
        net_bayes.add_variable(var("X", &["a", "b"])).unwrap();
        net_bayes.set_cpt(&vn("X"), vec![0.5, 0.5]).unwrap();

        let mut net_mle = BayesianNetwork::new();
        net_mle.add_variable(var("X", &["a", "b"])).unwrap();
        net_mle.set_cpt(&vn("X"), vec![0.5, 0.5]).unwrap();

        let cols = vec![vn("X")];
        let rows = vec![
            vec![Some(StateIndex::new(0))],
            vec![Some(StateIndex::new(0))],
            vec![Some(StateIndex::new(0))],
            vec![Some(StateIndex::new(1))],
        ];
        let data = Dataset::from_rows(cols, rows).unwrap();

        BayesianLearner::with_uniform_prior(1.0)
            .fit(&mut net_bayes, &data)
            .unwrap();
        MleLearner::with_defaults()
            .fit(&mut net_mle, &data)
            .unwrap();

        let cpt_bayes = read_cpt(&net_bayes, "X");
        let cpt_mle = read_cpt(&net_mle, "X");

        for (b, m) in cpt_bayes.iter().zip(cpt_mle.iter()) {
            assert!(
                (b - m).abs() < 1e-10,
                "Bayesian(α=1) should match MLE(pseudocount=1): {} vs {}",
                b,
                m
            );
        }
    }

    #[test]
    fn per_variable_custom_priors() {
        let mut net = BayesianNetwork::new();
        net.add_variable(var("X", &["a", "b"])).unwrap();
        net.set_cpt(&vn("X"), vec![0.5, 0.5]).unwrap();

        // Custom prior: α = [10.0, 1.0] — strongly biased toward "a"
        let mut priors = HashMap::new();
        priors.insert("X".to_string(), vec![10.0, 1.0]);

        let cols = vec![vn("X")];
        let rows = vec![
            vec![Some(StateIndex::new(1))], // b
            vec![Some(StateIndex::new(1))], // b
        ];
        let data = Dataset::from_rows(cols, rows).unwrap();

        let learner = BayesianLearner::with_custom_priors(priors, 1.0);
        learner.fit(&mut net, &data).unwrap();

        let cpt = read_cpt(&net, "X");
        // (0+10)/(2+11)=10/13 for "a", (2+1)/(2+11)=3/13 for "b"
        assert!((cpt[0] - 10.0 / 13.0).abs() < 1e-6, "P(a)={}", cpt[0]);
        assert!((cpt[1] - 3.0 / 13.0).abs() < 1e-6, "P(b)={}", cpt[1]);
    }

    #[test]
    fn default_uniform_prior_when_none_specified() {
        // Variables not in the per-variable map should use default_alpha
        let mut net = BayesianNetwork::new();
        net.add_variable(var("Y", &["0", "1"])).unwrap();
        net.set_cpt(&vn("Y"), vec![0.5, 0.5]).unwrap();

        let priors = HashMap::new(); // empty — no variable-specific priors

        let cols = vec![vn("Y")];
        let rows = vec![
            vec![Some(StateIndex::new(0))],
            vec![Some(StateIndex::new(0))],
            vec![Some(StateIndex::new(0))],
        ];
        let data = Dataset::from_rows(cols, rows).unwrap();

        let learner = BayesianLearner::with_custom_priors(priors, 2.0);
        learner.fit(&mut net, &data).unwrap();

        let cpt = read_cpt(&net, "Y");
        // (3+2)/(3+4) = 5/7, (0+2)/(3+4) = 2/7
        assert!((cpt[0] - 5.0 / 7.0).abs() < 1e-6, "P(0)={}", cpt[0]);
        assert!((cpt[1] - 2.0 / 7.0).abs() < 1e-6, "P(1)={}", cpt[1]);
    }

    #[test]
    fn small_dataset_prior_dominates() {
        // With 1 data point and strong prior, the prior should dominate
        let mut net = BayesianNetwork::new();
        net.add_variable(var("X", &["a", "b"])).unwrap();
        net.set_cpt(&vn("X"), vec![0.5, 0.5]).unwrap();

        let cols = vec![vn("X")];
        let rows = vec![vec![Some(StateIndex::new(1))]]; // single observation of "b"
        let data = Dataset::from_rows(cols, rows).unwrap();

        // Strong prior: α=100 uniform → prior weight dominates
        let learner = BayesianLearner::with_uniform_prior(100.0);
        learner.fit(&mut net, &data).unwrap();

        let cpt = read_cpt(&net, "X");
        // (0+100)/(1+200) = 100/201 ≈ 0.4975
        // (1+100)/(1+200) = 101/201 ≈ 0.5025
        // Prior dominates: both close to 0.5
        assert!(
            (cpt[0] - 0.5).abs() < 0.01,
            "prior should dominate: P(a)={}",
            cpt[0]
        );
        assert!(
            (cpt[1] - 0.5).abs() < 0.01,
            "prior should dominate: P(b)={}",
            cpt[1]
        );
    }

    #[test]
    fn large_dataset_data_dominates() {
        // With lots of data and weak prior, data should dominate
        let mut net = BayesianNetwork::new();
        net.add_variable(var("X", &["a", "b"])).unwrap();
        net.set_cpt(&vn("X"), vec![0.5, 0.5]).unwrap();

        let cols = vec![vn("X")];
        let mut rows = Vec::new();
        for _ in 0..900 {
            rows.push(vec![Some(StateIndex::new(0))]); // a
        }
        for _ in 0..100 {
            rows.push(vec![Some(StateIndex::new(1))]); // b
        }
        let data = Dataset::from_rows(cols, rows).unwrap();

        // Weak prior: α=0.001
        let learner = BayesianLearner::with_uniform_prior(0.001);
        learner.fit(&mut net, &data).unwrap();

        let cpt = read_cpt(&net, "X");
        // Should be very close to MLE: 900/1000=0.9, 100/1000=0.1
        assert!(
            (cpt[0] - 0.9).abs() < 0.001,
            "data should dominate: P(a)={}",
            cpt[0]
        );
        assert!(
            (cpt[1] - 0.1).abs() < 0.001,
            "data should dominate: P(b)={}",
            cpt[1]
        );
    }

    #[test]
    fn jeffreys_prior() {
        // Jeffreys prior α=0.5
        let mut net = BayesianNetwork::new();
        net.add_variable(var("X", &["a", "b"])).unwrap();
        net.set_cpt(&vn("X"), vec![0.5, 0.5]).unwrap();

        let cols = vec![vn("X")];
        let rows = vec![
            vec![Some(StateIndex::new(0))],
            vec![Some(StateIndex::new(0))],
            vec![Some(StateIndex::new(0))],
        ];
        let data = Dataset::from_rows(cols, rows).unwrap();

        let learner = BayesianLearner::with_uniform_prior(0.5);
        learner.fit(&mut net, &data).unwrap();

        let cpt = read_cpt(&net, "X");
        // (3+0.5)/(3+1.0) = 3.5/4.0 = 0.875
        // (0+0.5)/(3+1.0) = 0.5/4.0 = 0.125
        assert!((cpt[0] - 0.875).abs() < 1e-6, "P(a)={}", cpt[0]);
        assert!((cpt[1] - 0.125).abs() < 1e-6, "P(b)={}", cpt[1]);
    }

    #[test]
    fn with_parents_and_prior() {
        let mut net = BayesianNetwork::new();
        net.add_variable(var("A", &["0", "1"])).unwrap();
        net.add_variable(var("B", &["0", "1"])).unwrap();
        net.add_edge(&vn("A"), &vn("B")).unwrap();
        net.set_cpt(&vn("A"), vec![0.5, 0.5]).unwrap();
        net.set_cpt(&vn("B"), vec![0.5, 0.5, 0.5, 0.5]).unwrap();

        let cols = vec![vn("A"), vn("B")];
        let rows = vec![
            vec![Some(StateIndex::new(0)), Some(StateIndex::new(0))], // A=0, B=0
            vec![Some(StateIndex::new(0)), Some(StateIndex::new(0))], // A=0, B=0
            vec![Some(StateIndex::new(1)), Some(StateIndex::new(1))], // A=1, B=1
        ];
        let data = Dataset::from_rows(cols, rows).unwrap();

        let learner = BayesianLearner::with_uniform_prior(1.0);
        learner.fit(&mut net, &data).unwrap();

        let cpt_b = read_cpt(&net, "B");
        // For A=0: B=0 count=2, B=1 count=0 → (2+1)/(2+2)=3/4, (0+1)/(2+2)=1/4
        // For A=1: B=0 count=0, B=1 count=1 → (0+1)/(1+2)=1/3, (1+1)/(1+2)=2/3
        assert!((cpt_b[0] - 0.75).abs() < 1e-6, "P(B=0|A=0)={}", cpt_b[0]);
        assert!((cpt_b[1] - 0.25).abs() < 1e-6, "P(B=1|A=0)={}", cpt_b[1]);
        assert!(
            (cpt_b[2] - 1.0 / 3.0).abs() < 1e-6,
            "P(B=0|A=1)={}",
            cpt_b[2]
        );
        assert!(
            (cpt_b[3] - 2.0 / 3.0).abs() < 1e-6,
            "P(B=1|A=1)={}",
            cpt_b[3]
        );
    }
}
