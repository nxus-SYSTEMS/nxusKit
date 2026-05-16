//! Maximum Likelihood Estimation (MLE) for Bayesian Network parameters.
//!
//! Counts parent-child state configurations in the dataset and normalizes
//! to obtain CPT entries. Supports Laplace smoothing (pseudocount) and
//! available-case missing value handling.

use serde::{Deserialize, Serialize};

use crate::providers::bayesian::error::{BayesError, BayesResult};
use crate::providers::bayesian::learning::{Dataset, ParameterLearner};
use crate::providers::bayesian::network::BayesianNetwork;
use crate::providers::bayesian::types::{StateIndex, VariableName};

/// Strategy for handling missing values in the dataset.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MissingStrategy {
    /// Available-case: use each row for any variable where data is present,
    /// even if other variables are missing. This maximizes data usage.
    #[default]
    AvailableCase,
    /// Complete-case: skip the entire row if any relevant variable is missing.
    CompleteCase,
}

/// Configuration for MLE parameter learning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MleConfig {
    /// Laplace smoothing pseudocount added to each cell.
    /// 0.0 = no smoothing (pure MLE), 1.0 = standard Laplace.
    #[serde(default = "default_pseudocount")]
    pub pseudocount: f64,

    /// Strategy for handling missing values.
    #[serde(default)]
    pub missing_strategy: MissingStrategy,
}

fn default_pseudocount() -> f64 {
    1.0
}

impl Default for MleConfig {
    fn default() -> Self {
        Self {
            pseudocount: default_pseudocount(),
            missing_strategy: MissingStrategy::default(),
        }
    }
}

/// MLE parameter learner for Bayesian Networks.
///
/// Estimates CPTs by counting parent-child state configurations in the data
/// and normalizing. Optionally applies Laplace smoothing.
#[derive(Debug, Clone)]
pub struct MleLearner {
    config: MleConfig,
}

impl MleLearner {
    pub fn new(config: MleConfig) -> Self {
        Self { config }
    }

    /// Create with default config (pseudocount=1, available-case).
    pub fn with_defaults() -> Self {
        Self::new(MleConfig::default())
    }

    /// Create with a specific pseudocount (available-case strategy).
    pub fn with_pseudocount(pseudocount: f64) -> Self {
        Self::new(MleConfig {
            pseudocount,
            ..Default::default()
        })
    }

    /// Compute log-likelihood of the dataset given the current network CPTs.
    ///
    /// Returns Σ_rows Σ_vars log P(var=state | parents=parent_states).
    /// Missing values are skipped (available-case). Rows where any relevant
    /// variable is missing contribute 0 for that term.
    pub fn log_likelihood(&self, network: &BayesianNetwork, data: &Dataset) -> BayesResult<f64> {
        if !network.is_complete() {
            return Err(BayesError::IncompleteNetwork(
                "Cannot compute log-likelihood: network has missing CPTs".into(),
            ));
        }

        let var_names = network.variable_names();
        let mut ll = 0.0;

        for row in &data.rows {
            for vn in &var_names {
                let col_idx = match data.column_index(vn) {
                    Some(idx) => idx,
                    None => continue,
                };
                let child_state = match row[col_idx] {
                    Some(si) => si,
                    None => continue, // missing → skip this term
                };

                // Get parent states
                let parents = network.parents(vn);
                let mut all_parents_present = true;
                let mut parent_states = Vec::with_capacity(parents.len());

                for pn in &parents {
                    let pcol = match data.column_index(pn) {
                        Some(idx) => idx,
                        None => {
                            all_parents_present = false;
                            break;
                        }
                    };
                    match row[pcol] {
                        Some(si) => parent_states.push(si),
                        None => {
                            all_parents_present = false;
                            break;
                        }
                    }
                }

                if !all_parents_present {
                    continue;
                }

                // Look up P(child | parents) from CPT
                let prob =
                    self.lookup_cpt_entry(network, vn, &parents, &parent_states, child_state)?;
                if prob <= 0.0 {
                    return Ok(f64::NEG_INFINITY);
                }
                ll += prob.ln();
            }
        }

        Ok(ll)
    }

    /// Look up a single CPT entry: P(variable=child_state | parents=parent_states).
    fn lookup_cpt_entry(
        &self,
        network: &BayesianNetwork,
        variable: &VariableName,
        parents: &[VariableName],
        parent_states: &[StateIndex],
        child_state: StateIndex,
    ) -> BayesResult<f64> {
        let var = network.variable(variable).ok_or_else(|| {
            BayesError::InferenceError(format!("Variable '{}' not found", variable))
        })?;
        let child_card = var.cardinality();

        // Compute flat index into CPT (parents in canonical order, child last)
        let mut flat_idx = 0;
        let mut stride = child_card;
        for (pn, ps) in parents.iter().zip(parent_states.iter()).rev() {
            let parent_var = network
                .variable(pn)
                .ok_or_else(|| BayesError::InferenceError(format!("Parent '{}' not found", pn)))?;
            flat_idx += ps.value() * stride;
            stride *= parent_var.cardinality();
        }
        flat_idx += child_state.value();

        // Get CPT factor from network (stored in log-space)
        let factor = network.cpt(variable).ok_or_else(|| {
            BayesError::IncompleteNetwork(format!("Variable '{}' has no CPT", variable))
        })?;

        // Convert from log-space back to probability
        Ok(factor.log_values[flat_idx].exp())
    }

    /// Count parent-child configurations for a single variable.
    ///
    /// Returns a flat array of counts indexed the same way as the CPT:
    /// `[parent_config_0_child_0, parent_config_0_child_1, ..., parent_config_N_child_K]`
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

        // Initialize with pseudocount
        let mut counts = vec![self.config.pseudocount; cpt_size];

        let child_col = match data.column_index(variable) {
            Some(idx) => idx,
            None => return Ok(counts), // variable not in dataset → use only pseudocounts
        };

        // Precompute parent column indices
        let parent_cols: Vec<Option<usize>> =
            parents.iter().map(|pn| data.column_index(pn)).collect();

        for row in &data.rows {
            // Get child state
            let child_state = match row[child_col] {
                Some(si) => si,
                None => continue, // missing child → skip
            };

            // Get parent states
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
                    None => match self.config.missing_strategy {
                        MissingStrategy::AvailableCase => {
                            all_present = false;
                            break;
                        }
                        MissingStrategy::CompleteCase => {
                            all_present = false;
                            break;
                        }
                    },
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

impl ParameterLearner for MleLearner {
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
                    // Uniform distribution if no data (shouldn't happen with pseudocount > 0)
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
    use crate::providers::bayesian::bif::load_bif_file;
    use crate::providers::bayesian::types::{DiscreteVariable, StateName};
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn fixture_dir() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bn")
    }

    fn load_cancer() -> BayesianNetwork {
        load_bif_file(&fixture_dir().join("cancer.bif")).unwrap()
    }

    fn write_csv(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f.flush().unwrap();
        f
    }

    fn vn(name: &str) -> VariableName {
        VariableName::new(name).unwrap()
    }

    fn sn(name: &str) -> StateName {
        StateName::new(name).unwrap()
    }

    fn var(name: &str, states: &[&str]) -> DiscreteVariable {
        DiscreteVariable::new(vn(name), states.iter().map(|s| sn(s)).collect()).unwrap()
    }

    /// Read CPT probabilities from the network factor (stored in log-space).
    fn read_cpt(net: &BayesianNetwork, name: &str) -> Vec<f64> {
        net.cpt(&vn(name))
            .unwrap()
            .log_values
            .iter()
            .map(|lv| lv.exp())
            .collect()
    }

    #[test]
    fn mle_simple_no_parents() {
        let mut net = BayesianNetwork::new();
        net.add_variable(var("Coin", &["heads", "tails"])).unwrap();
        net.set_cpt(&vn("Coin"), vec![0.5, 0.5]).unwrap();

        let cols = vec![vn("Coin")];
        let mut rows = Vec::new();
        for _ in 0..7 {
            rows.push(vec![Some(StateIndex::new(0))]); // heads
        }
        for _ in 0..3 {
            rows.push(vec![Some(StateIndex::new(1))]); // tails
        }
        let data = Dataset::from_rows(cols, rows).unwrap();

        let learner = MleLearner::with_defaults(); // pseudocount=1
        learner.fit(&mut net, &data).unwrap();

        let cpt = read_cpt(&net, "Coin");
        // With Laplace: (7+1)/(10+2) = 0.6667, (3+1)/(10+2) = 0.3333
        assert!((cpt[0] - 8.0 / 12.0).abs() < 1e-6, "heads: {}", cpt[0]);
        assert!((cpt[1] - 4.0 / 12.0).abs() < 1e-6, "tails: {}", cpt[1]);
    }

    #[test]
    fn mle_no_smoothing() {
        let mut net = BayesianNetwork::new();
        net.add_variable(var("Coin", &["heads", "tails"])).unwrap();
        net.set_cpt(&vn("Coin"), vec![0.5, 0.5]).unwrap();

        let cols = vec![vn("Coin")];
        let mut rows = Vec::new();
        for _ in 0..7 {
            rows.push(vec![Some(StateIndex::new(0))]);
        }
        for _ in 0..3 {
            rows.push(vec![Some(StateIndex::new(1))]);
        }
        let data = Dataset::from_rows(cols, rows).unwrap();

        let learner = MleLearner::with_pseudocount(0.0);
        learner.fit(&mut net, &data).unwrap();

        let cpt = read_cpt(&net, "Coin");
        assert!((cpt[0] - 0.7).abs() < 1e-6, "heads: {}", cpt[0]);
        assert!((cpt[1] - 0.3).abs() < 1e-6, "tails: {}", cpt[1]);
    }

    #[test]
    fn mle_with_parents() {
        let mut net = BayesianNetwork::new();
        net.add_variable(var("Rain", &["yes", "no"])).unwrap();
        net.add_variable(var("Wet", &["yes", "no"])).unwrap();
        net.add_edge(&vn("Rain"), &vn("Wet")).unwrap();
        net.set_cpt(&vn("Rain"), vec![0.5, 0.5]).unwrap();
        net.set_cpt(&vn("Wet"), vec![0.9, 0.1, 0.2, 0.8]).unwrap();

        // Dataset: Rain=yes→Wet=yes 8, Rain=yes→Wet=no 2,
        //          Rain=no→Wet=yes 1, Rain=no→Wet=no 9
        let cols = vec![vn("Rain"), vn("Wet")];
        let mut rows = Vec::new();
        for _ in 0..8 {
            rows.push(vec![Some(StateIndex::new(0)), Some(StateIndex::new(0))]);
        }
        for _ in 0..2 {
            rows.push(vec![Some(StateIndex::new(0)), Some(StateIndex::new(1))]);
        }
        rows.push(vec![Some(StateIndex::new(1)), Some(StateIndex::new(0))]);
        for _ in 0..9 {
            rows.push(vec![Some(StateIndex::new(1)), Some(StateIndex::new(1))]);
        }
        let data = Dataset::from_rows(cols, rows).unwrap();

        let learner = MleLearner::with_pseudocount(0.0);
        learner.fit(&mut net, &data).unwrap();

        let cpt = read_cpt(&net, "Wet");
        assert!(
            (cpt[0] - 0.8).abs() < 1e-6,
            "P(wet=yes|rain=yes): {}",
            cpt[0]
        );
        assert!(
            (cpt[1] - 0.2).abs() < 1e-6,
            "P(wet=no|rain=yes): {}",
            cpt[1]
        );
        assert!(
            (cpt[2] - 0.1).abs() < 1e-6,
            "P(wet=yes|rain=no): {}",
            cpt[2]
        );
        assert!((cpt[3] - 0.9).abs() < 1e-6, "P(wet=no|rain=no): {}", cpt[3]);
    }

    #[test]
    fn mle_laplace_prevents_zero_probs() {
        let mut net = BayesianNetwork::new();
        net.add_variable(var("X", &["a", "b", "c"])).unwrap();
        net.set_cpt(&vn("X"), vec![1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0])
            .unwrap();

        let cols = vec![vn("X")];
        let rows = vec![
            vec![Some(StateIndex::new(0))],
            vec![Some(StateIndex::new(0))],
            vec![Some(StateIndex::new(0))],
        ];
        let data = Dataset::from_rows(cols, rows).unwrap();

        let learner = MleLearner::with_defaults();
        learner.fit(&mut net, &data).unwrap();

        let cpt = read_cpt(&net, "X");
        assert!(cpt[1] > 0.0, "Laplace should prevent zero: P(b)={}", cpt[1]);
        assert!(cpt[2] > 0.0, "Laplace should prevent zero: P(c)={}", cpt[2]);
        assert!((cpt[0] - 4.0 / 6.0).abs() < 1e-6);
        assert!((cpt[1] - 1.0 / 6.0).abs() < 1e-6);
    }

    #[test]
    fn mle_missing_values_available_case() {
        let mut net = BayesianNetwork::new();
        net.add_variable(var("A", &["0", "1"])).unwrap();
        net.add_variable(var("B", &["0", "1"])).unwrap();
        net.add_edge(&vn("A"), &vn("B")).unwrap();
        net.set_cpt(&vn("A"), vec![0.5, 0.5]).unwrap();
        net.set_cpt(&vn("B"), vec![0.5, 0.5, 0.5, 0.5]).unwrap();

        let cols = vec![vn("A"), vn("B")];
        let rows = vec![
            vec![Some(StateIndex::new(0)), Some(StateIndex::new(0))],
            vec![None, Some(StateIndex::new(1))], // A missing
            vec![Some(StateIndex::new(1)), Some(StateIndex::new(1))],
        ];
        let data = Dataset::from_rows(cols, rows).unwrap();

        let learner = MleLearner::with_pseudocount(0.0);
        learner.fit(&mut net, &data).unwrap();

        let a_cpt = read_cpt(&net, "A");
        // A: 1 count of 0, 1 count of 1 (missing row skipped)
        assert!((a_cpt[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn mle_from_csv_cancer() {
        let mut net = load_cancer();
        let csv = write_csv(
            "Pollution,Smoker,Cancer,Xray,Dyspnea\n\
             low,True,True,positive,True\n\
             low,True,True,positive,True\n\
             low,False,False,negative,False\n\
             high,True,True,positive,True\n\
             high,False,False,negative,False\n",
        );
        let data = Dataset::from_csv(csv.path(), &net).unwrap();

        let learner = MleLearner::with_defaults();
        learner.fit(&mut net, &data).unwrap();

        assert!(net.is_complete());
    }

    #[test]
    fn log_likelihood_basic() {
        let mut net = BayesianNetwork::new();
        net.add_variable(var("Coin", &["H", "T"])).unwrap();
        net.set_cpt(&vn("Coin"), vec![0.7, 0.3]).unwrap();

        let cols = vec![vn("Coin")];
        let rows = vec![
            vec![Some(StateIndex::new(0))], // H
            vec![Some(StateIndex::new(0))], // H
            vec![Some(StateIndex::new(1))], // T
        ];
        let data = Dataset::from_rows(cols, rows).unwrap();

        let learner = MleLearner::with_defaults();
        let ll = learner.log_likelihood(&net, &data).unwrap();
        let expected = 2.0 * 0.7_f64.ln() + 0.3_f64.ln();
        assert!(
            (ll - expected).abs() < 1e-10,
            "LL={}, expected={}",
            ll,
            expected
        );
    }

    #[test]
    fn log_likelihood_with_parents() {
        let mut net = BayesianNetwork::new();
        net.add_variable(var("A", &["0", "1"])).unwrap();
        net.add_variable(var("B", &["0", "1"])).unwrap();
        net.add_edge(&vn("A"), &vn("B")).unwrap();
        net.set_cpt(&vn("A"), vec![0.6, 0.4]).unwrap();
        net.set_cpt(&vn("B"), vec![0.9, 0.1, 0.2, 0.8]).unwrap();

        let cols = vec![vn("A"), vn("B")];
        let rows = vec![vec![Some(StateIndex::new(0)), Some(StateIndex::new(0))]];
        let data = Dataset::from_rows(cols, rows).unwrap();

        let learner = MleLearner::with_defaults();
        let ll = learner.log_likelihood(&net, &data).unwrap();
        let expected = 0.6_f64.ln() + 0.9_f64.ln();
        assert!(
            (ll - expected).abs() < 1e-10,
            "LL={}, expected={}",
            ll,
            expected
        );
    }
}
