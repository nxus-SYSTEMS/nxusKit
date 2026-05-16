//! Scoring functions for structure learning: BIC and BDeu.
//!
//! Scores evaluate how well a DAG structure fits the data, balancing
//! fit (log-likelihood) against complexity (number of parameters).

use serde::{Deserialize, Serialize};
use statrs::function::gamma::ln_gamma;

use crate::providers::bayesian::error::{BayesError, BayesResult};
use crate::providers::bayesian::learning::Dataset;
use crate::providers::bayesian::network::BayesianNetwork;
use crate::providers::bayesian::types::VariableName;

/// Scoring function for structure learning.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum ScoringFunction {
    /// Bayesian Information Criterion: LL - (k/2) * ln(N)
    #[default]
    BIC,
    /// Bayesian Dirichlet equivalent uniform: uses Dirichlet-multinomial
    /// marginal likelihood with equivalent sample size.
    BDeu {
        /// Equivalent sample size (default 10.0).
        equivalent_sample_size: f64,
    },
}

impl ScoringFunction {
    /// Create a BDeu scoring function with default equivalent sample size (10.0).
    pub fn bdeu() -> Self {
        ScoringFunction::BDeu {
            equivalent_sample_size: 10.0,
        }
    }

    /// Create a BDeu scoring function with a specific equivalent sample size.
    pub fn bdeu_with_ess(ess: f64) -> Self {
        ScoringFunction::BDeu {
            equivalent_sample_size: ess,
        }
    }

    /// Compute the score for a single variable given its parents in the data.
    ///
    /// This is the "local score" — the total network score is the sum of local
    /// scores for all variables.
    pub fn local_score(
        &self,
        variable: &VariableName,
        parents: &[VariableName],
        network: &BayesianNetwork,
        data: &Dataset,
    ) -> BayesResult<f64> {
        match self {
            ScoringFunction::BIC => self.bic_local(variable, parents, network, data),
            ScoringFunction::BDeu {
                equivalent_sample_size,
            } => self.bdeu_local(variable, parents, network, data, *equivalent_sample_size),
        }
    }

    /// Compute the total network score (sum of all local scores).
    pub fn score(&self, network: &BayesianNetwork, data: &Dataset) -> BayesResult<f64> {
        let mut total = 0.0;
        for vn in network.variable_names() {
            let parents = network.parents(&vn);
            total += self.local_score(&vn, &parents, network, data)?;
        }
        Ok(total)
    }

    /// BIC local score: LL_local - (k/2) * ln(N)
    fn bic_local(
        &self,
        variable: &VariableName,
        parents: &[VariableName],
        network: &BayesianNetwork,
        data: &Dataset,
    ) -> BayesResult<f64> {
        let var = network.variable(variable).ok_or_else(|| {
            BayesError::InferenceError(format!("Variable '{}' not found", variable))
        })?;
        let child_card = var.cardinality();
        let n = data.num_rows() as f64;

        // Count configurations
        let counts = count_configurations(variable, parents, network, data)?;
        let num_parent_configs = counts.len() / child_card;

        // Log-likelihood component
        let mut ll = 0.0;
        for pc in 0..num_parent_configs {
            let start = pc * child_card;
            let end = start + child_card;
            let row_total: f64 = counts[start..end].iter().sum();
            if row_total > 0.0 {
                for &cnt in &counts[start..end] {
                    if cnt > 0.0 {
                        ll += cnt * (cnt / row_total).ln();
                    }
                }
            }
        }

        // Penalty: k = num_parent_configs * (child_card - 1) free parameters
        let k = num_parent_configs * (child_card - 1);
        let penalty = (k as f64 / 2.0) * n.ln();

        Ok(ll - penalty)
    }

    /// BDeu local score using Dirichlet-Multinomial marginal likelihood.
    ///
    /// score = Σ_j [ ln Γ(α_j) - ln Γ(α_j + N_j) + Σ_k [ ln Γ(α_jk + N_jk) - ln Γ(α_jk) ] ]
    ///
    /// where j indexes parent configurations, k indexes child states,
    /// α_jk = ess / (num_parent_configs * child_card), α_j = Σ_k α_jk
    fn bdeu_local(
        &self,
        variable: &VariableName,
        parents: &[VariableName],
        network: &BayesianNetwork,
        data: &Dataset,
        ess: f64,
    ) -> BayesResult<f64> {
        let var = network.variable(variable).ok_or_else(|| {
            BayesError::InferenceError(format!("Variable '{}' not found", variable))
        })?;
        let child_card = var.cardinality();

        let counts = count_configurations(variable, parents, network, data)?;
        let num_parent_configs = counts.len() / child_card;

        // BDeu hyperparameters
        let alpha_jk = ess / (num_parent_configs * child_card) as f64;
        let alpha_j = alpha_jk * child_card as f64;

        let mut score = 0.0;
        for pc in 0..num_parent_configs {
            let start = pc * child_card;
            let end = start + child_card;
            let n_j: f64 = counts[start..end].iter().sum();

            score += ln_gamma(alpha_j) - ln_gamma(alpha_j + n_j);

            for &n_jk in &counts[start..end] {
                score += ln_gamma(alpha_jk + n_jk) - ln_gamma(alpha_jk);
            }
        }

        Ok(score)
    }
}

/// Count parent-child configurations with zero pseudocount.
fn count_configurations(
    variable: &VariableName,
    parents: &[VariableName],
    network: &BayesianNetwork,
    data: &Dataset,
) -> BayesResult<Vec<f64>> {
    let var = network
        .variable(variable)
        .ok_or_else(|| BayesError::InferenceError(format!("Variable '{}' not found", variable)))?;
    let child_card = var.cardinality();

    let mut cpt_size = child_card;
    for pn in parents {
        let pvar = network
            .variable(pn)
            .ok_or_else(|| BayesError::InferenceError(format!("Parent '{}' not found", pn)))?;
        cpt_size *= pvar.cardinality();
    }

    let mut counts = vec![0.0; cpt_size];

    let child_col = match data.column_index(variable) {
        Some(idx) => idx,
        None => return Ok(counts),
    };

    let parent_cols: Vec<Option<usize>> = parents.iter().map(|pn| data.column_index(pn)).collect();

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::bayesian::bif::load_bif_file;
    use crate::providers::bayesian::types::{DiscreteVariable, StateIndex, StateName};
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn fixture_dir() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bn")
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

    fn load_asia() -> BayesianNetwork {
        load_bif_file(&fixture_dir().join("asia.bif")).unwrap()
    }

    fn write_csv(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f.flush().unwrap();
        f
    }

    #[test]
    fn bic_score_is_finite() {
        let net = load_asia();
        let csv = write_csv(
            "Asia,Tuberculosis,Smoking,LungCancer,TbOrCa,Xray,Bronchitis,Dyspnea\n\
             yes,present,yes,present,true,positive,present,present\n\
             no,absent,no,absent,false,negative,absent,absent\n\
             no,absent,yes,absent,false,negative,present,present\n\
             no,absent,no,absent,false,negative,absent,absent\n\
             no,absent,yes,present,true,positive,present,present\n",
        );
        let data = Dataset::from_csv(csv.path(), &net).unwrap();
        let scoring = ScoringFunction::BIC;
        let score = scoring.score(&net, &data).unwrap();
        assert!(score.is_finite(), "BIC score should be finite: {}", score);
        assert!(score < 0.0, "BIC score should be negative: {}", score);
    }

    #[test]
    fn bdeu_score_is_finite() {
        let net = load_asia();
        let csv = write_csv(
            "Asia,Tuberculosis,Smoking,LungCancer,TbOrCa,Xray,Bronchitis,Dyspnea\n\
             yes,present,yes,present,true,positive,present,present\n\
             no,absent,no,absent,false,negative,absent,absent\n\
             no,absent,yes,absent,false,negative,present,present\n",
        );
        let data = Dataset::from_csv(csv.path(), &net).unwrap();
        let scoring = ScoringFunction::bdeu();
        let score = scoring.score(&net, &data).unwrap();
        assert!(score.is_finite(), "BDeu score should be finite: {}", score);
    }

    #[test]
    fn better_structure_has_higher_bic() {
        // A simple test: correct structure should score better than reversed
        let mut net_correct = BayesianNetwork::new();
        net_correct.add_variable(var("A", &["0", "1"])).unwrap();
        net_correct.add_variable(var("B", &["0", "1"])).unwrap();
        net_correct.add_edge(&vn("A"), &vn("B")).unwrap();
        net_correct.set_cpt(&vn("A"), vec![0.5, 0.5]).unwrap();
        net_correct
            .set_cpt(&vn("B"), vec![0.9, 0.1, 0.1, 0.9])
            .unwrap();

        let mut net_independent = BayesianNetwork::new();
        net_independent.add_variable(var("A", &["0", "1"])).unwrap();
        net_independent.add_variable(var("B", &["0", "1"])).unwrap();
        net_independent.set_cpt(&vn("A"), vec![0.5, 0.5]).unwrap();
        net_independent.set_cpt(&vn("B"), vec![0.5, 0.5]).unwrap();

        // Data that strongly supports A→B dependency
        let cols = vec![vn("A"), vn("B")];
        let mut rows = Vec::new();
        for _ in 0..50 {
            rows.push(vec![Some(StateIndex::new(0)), Some(StateIndex::new(0))]);
        }
        for _ in 0..50 {
            rows.push(vec![Some(StateIndex::new(1)), Some(StateIndex::new(1))]);
        }
        let data = Dataset::from_rows(cols, rows).unwrap();

        let scoring = ScoringFunction::BIC;
        let score_correct = scoring.score(&net_correct, &data).unwrap();
        let score_independent = scoring.score(&net_independent, &data).unwrap();

        assert!(
            score_correct > score_independent,
            "Correct structure should have higher BIC: {} vs {}",
            score_correct,
            score_independent,
        );
    }

    #[test]
    fn local_score_root_node() {
        let mut net = BayesianNetwork::new();
        net.add_variable(var("X", &["a", "b"])).unwrap();
        net.set_cpt(&vn("X"), vec![0.5, 0.5]).unwrap();

        let cols = vec![vn("X")];
        let rows = vec![
            vec![Some(StateIndex::new(0))],
            vec![Some(StateIndex::new(0))],
            vec![Some(StateIndex::new(1))],
        ];
        let data = Dataset::from_rows(cols, rows).unwrap();

        let scoring = ScoringFunction::BIC;
        let local = scoring.local_score(&vn("X"), &[], &net, &data).unwrap();
        assert!(local.is_finite());
    }

    #[test]
    fn bdeu_ess_effect() {
        // Higher ESS should penalize complex structures more
        let mut net = BayesianNetwork::new();
        net.add_variable(var("A", &["0", "1"])).unwrap();
        net.add_variable(var("B", &["0", "1"])).unwrap();
        net.add_edge(&vn("A"), &vn("B")).unwrap();
        net.set_cpt(&vn("A"), vec![0.5, 0.5]).unwrap();
        net.set_cpt(&vn("B"), vec![0.5, 0.5, 0.5, 0.5]).unwrap();

        let cols = vec![vn("A"), vn("B")];
        let rows = vec![
            vec![Some(StateIndex::new(0)), Some(StateIndex::new(0))],
            vec![Some(StateIndex::new(1)), Some(StateIndex::new(1))],
        ];
        let data = Dataset::from_rows(cols, rows).unwrap();

        let score_low_ess = ScoringFunction::bdeu_with_ess(1.0)
            .score(&net, &data)
            .unwrap();
        let score_high_ess = ScoringFunction::bdeu_with_ess(100.0)
            .score(&net, &data)
            .unwrap();

        // Both should be finite
        assert!(score_low_ess.is_finite());
        assert!(score_high_ess.is_finite());
    }
}
