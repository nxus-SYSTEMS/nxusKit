//! Gaussian / Conditional Linear Gaussian (CLG) inference.
//!
//! Implements exact inference for networks containing continuous (Gaussian)
//! variables, using the canonical (information) form parameterization.
//!
//! - **Purely Gaussian networks**: build a joint precision matrix from all
//!   `GaussianFactor`s, condition on evidence, read off posterior marginals.
//! - **Mixed CLG networks** (discrete + Gaussian): enumerate every discrete
//!   configuration, compute the conditional Gaussian posterior for each, and
//!   weight by the configuration's discrete probability.

use std::collections::HashMap;
use std::time::Instant;

use nalgebra::{DMatrix, DVector};

use super::{ContinuousMarginal, InferenceDiagnostics, InferenceEngine, InferenceResult, Marginal};
use crate::providers::bayesian::error::{BayesError, BayesResult};
use crate::providers::bayesian::evidence::Evidence;
use crate::providers::bayesian::network::BayesianNetwork;
use crate::providers::bayesian::types::{
    GaussianVariable, ObservationType, StateIndex, VariableName,
};

// ---------------------------------------------------------------------------
// GaussianFactor — canonical (information) form
// ---------------------------------------------------------------------------

/// A Gaussian factor in canonical (information / precision) form.
///
/// A multivariate Gaussian N(μ, Σ) can equivalently be represented as:
///
/// - **Precision matrix** Λ = Σ⁻¹
/// - **Precision-weighted mean** h = Λ μ
/// - **Log normalization constant** g (tracks partition function)
///
/// This representation is closed under multiplication and conditioning,
/// making it ideal for message-passing inference.
#[derive(Debug, Clone)]
pub struct GaussianFactor {
    /// Precision (inverse covariance) matrix — Λ.
    pub precision: DMatrix<f64>,
    /// Precision-weighted mean vector — h = Λ μ.
    pub h_vec: DVector<f64>,
    /// Log normalization constant.
    pub log_norm: f64,
    /// Ordered list of variable names in scope.
    pub variables: Vec<VariableName>,
}

impl GaussianFactor {
    /// Construct a `GaussianFactor` from a single `GaussianVariable`.
    ///
    /// For a variable X with conditional distribution
    ///   X | parents ~ N(mean_base + Σ wᵢ parentᵢ , variance)
    ///
    /// we build a joint canonical factor over {X, continuous_parents}.
    pub fn from_gaussian_variable(
        gv: &GaussianVariable,
        network: &BayesianNetwork,
    ) -> BayesResult<Self> {
        if gv.variance <= 0.0 || !gv.variance.is_finite() {
            return Err(BayesError::InvalidGaussianParameters(format!(
                "Variable '{}': variance must be finite and > 0, got {}",
                gv.name, gv.variance
            )));
        }

        // Collect only continuous parents (discrete parents are handled
        // separately via enumeration in the CLG inference loop).
        let continuous_weights: Vec<(VariableName, f64)> = gv
            .weights
            .iter()
            .filter(|(pname, _)| network.is_gaussian(pname))
            .cloned()
            .collect();

        let n_parents = continuous_weights.len();
        let dim = 1 + n_parents; // X + continuous parents

        // Variable ordering: [X, parent_0, parent_1, ...]
        let mut variables = Vec::with_capacity(dim);
        variables.push(gv.name.clone());
        for (pname, _) in &continuous_weights {
            variables.push(pname.clone());
        }

        let tau = 1.0 / gv.variance; // precision of the noise term

        // Build weight vector w (continuous parent weights).
        let w = DVector::from_iterator(n_parents, continuous_weights.iter().map(|(_, wi)| *wi));

        // Precision matrix Λ for the conditional p(x | pa):
        //   Λ = τ * [ 1      | -wᵀ    ]
        //            [ -w     |  w wᵀ   ]
        let mut precision = DMatrix::zeros(dim, dim);
        precision[(0, 0)] = tau;
        for i in 0..n_parents {
            precision[(0, i + 1)] = -tau * w[i];
            precision[(i + 1, 0)] = -tau * w[i];
        }
        for i in 0..n_parents {
            for j in 0..n_parents {
                precision[(i + 1, j + 1)] = tau * w[i] * w[j];
            }
        }

        // h vector:
        //   h = τ * [ mean_base ]
        //            [ -mean_base * w ]
        let mut h_vec = DVector::zeros(dim);
        h_vec[0] = tau * gv.mean_base;
        for i in 0..n_parents {
            h_vec[i + 1] = -tau * gv.mean_base * w[i];
        }

        // Log normalization:
        //   g = -½ (ln(2π) - ln(τ) + τ * mean_base²)
        let log_norm =
            -0.5 * (std::f64::consts::TAU.ln() - tau.ln() + tau * gv.mean_base * gv.mean_base);

        Ok(Self {
            precision,
            h_vec,
            log_norm,
            variables,
        })
    }

    /// Multiply two Gaussian factors in canonical form.
    ///
    /// Multiplication is addition of precision matrices, h-vectors, and
    /// log-normalization constants, after aligning variable orderings.
    pub fn multiply(&self, other: &GaussianFactor) -> GaussianFactor {
        // Build union of variable scopes.
        let mut vars = self.variables.clone();
        let mut other_to_union: Vec<usize> = Vec::with_capacity(other.variables.len());
        for ov in &other.variables {
            if let Some(pos) = vars.iter().position(|v| v == ov) {
                other_to_union.push(pos);
            } else {
                other_to_union.push(vars.len());
                vars.push(ov.clone());
            }
        }

        let dim = vars.len();
        let self_to_union: Vec<usize> = (0..self.variables.len()).collect();

        let mut precision = DMatrix::zeros(dim, dim);
        let mut h_vec = DVector::zeros(dim);

        // Add self's contributions.
        for i in 0..self.variables.len() {
            h_vec[self_to_union[i]] += self.h_vec[i];
            for j in 0..self.variables.len() {
                precision[(self_to_union[i], self_to_union[j])] += self.precision[(i, j)];
            }
        }

        // Add other's contributions.
        for i in 0..other.variables.len() {
            h_vec[other_to_union[i]] += other.h_vec[i];
            for j in 0..other.variables.len() {
                precision[(other_to_union[i], other_to_union[j])] += other.precision[(i, j)];
            }
        }

        let log_norm = self.log_norm + other.log_norm;

        GaussianFactor {
            precision,
            h_vec,
            log_norm,
            variables: vars,
        }
    }

    /// Marginalize out a variable using the Schur complement.
    pub fn marginalize(&self, var: &VariableName) -> BayesResult<GaussianFactor> {
        let pos = self
            .variables
            .iter()
            .position(|v| v == var)
            .ok_or_else(|| {
                BayesError::InferenceError(format!(
                    "Variable '{}' not in Gaussian factor scope",
                    var
                ))
            })?;

        let dim = self.variables.len();
        if dim == 1 {
            return Ok(GaussianFactor {
                precision: DMatrix::zeros(0, 0),
                h_vec: DVector::zeros(0),
                log_norm: self.log_norm
                    + 0.5
                        * (std::f64::consts::TAU.ln() - self.precision[(0, 0)].ln()
                            + self.h_vec[0] * self.h_vec[0] / self.precision[(0, 0)]),
                variables: Vec::new(),
            });
        }

        let lambda_xx = self.precision[(pos, pos)];
        if lambda_xx.abs() < 1e-15 {
            return Err(BayesError::InferenceError(format!(
                "Cannot marginalize '{}': zero precision (singular)",
                var
            )));
        }
        let inv_lambda_xx = 1.0 / lambda_xx;

        let remaining: Vec<usize> = (0..dim).filter(|&i| i != pos).collect();
        let n = remaining.len();

        let mut lambda_yx = DVector::zeros(n);
        for (ri, &idx) in remaining.iter().enumerate() {
            lambda_yx[ri] = self.precision[(idx, pos)];
        }

        let h_x = self.h_vec[pos];

        // Schur complement for precision.
        let mut new_precision = DMatrix::zeros(n, n);
        for (ri, &i) in remaining.iter().enumerate() {
            for (rj, &j) in remaining.iter().enumerate() {
                new_precision[(ri, rj)] =
                    self.precision[(i, j)] - lambda_yx[ri] * inv_lambda_xx * lambda_yx[rj];
            }
        }

        // Schur complement for h-vector.
        let mut new_h = DVector::zeros(n);
        for (ri, &i) in remaining.iter().enumerate() {
            new_h[ri] = self.h_vec[i] - lambda_yx[ri] * inv_lambda_xx * h_x;
        }

        let new_log_norm = self.log_norm
            + 0.5 * (std::f64::consts::TAU.ln() - lambda_xx.ln() + h_x * h_x * inv_lambda_xx);

        let new_vars: Vec<VariableName> = remaining
            .iter()
            .map(|&i| self.variables[i].clone())
            .collect();

        Ok(GaussianFactor {
            precision: new_precision,
            h_vec: new_h,
            log_norm: new_log_norm,
            variables: new_vars,
        })
    }

    /// Condition on an observed value for a variable.
    ///
    /// Given canonical form over (Y, X) and observing X = x:
    ///   Λ_new = Λ_YY
    ///   h_new = h_Y - Λ_YX · x
    ///   g_new = g + h_X · x - ½ · Λ_XX · x²
    pub fn condition(&self, var: &VariableName, value: f64) -> BayesResult<GaussianFactor> {
        let pos = self
            .variables
            .iter()
            .position(|v| v == var)
            .ok_or_else(|| {
                BayesError::InferenceError(format!(
                    "Variable '{}' not in Gaussian factor scope",
                    var
                ))
            })?;

        let dim = self.variables.len();
        if dim == 1 {
            let new_log_norm = self.log_norm + self.h_vec[0] * value
                - 0.5 * self.precision[(0, 0)] * value * value;
            return Ok(GaussianFactor {
                precision: DMatrix::zeros(0, 0),
                h_vec: DVector::zeros(0),
                log_norm: new_log_norm,
                variables: Vec::new(),
            });
        }

        let remaining: Vec<usize> = (0..dim).filter(|&i| i != pos).collect();
        let n = remaining.len();

        let mut new_precision = DMatrix::zeros(n, n);
        for (ri, &i) in remaining.iter().enumerate() {
            for (rj, &j) in remaining.iter().enumerate() {
                new_precision[(ri, rj)] = self.precision[(i, j)];
            }
        }

        let mut new_h = DVector::zeros(n);
        for (ri, &i) in remaining.iter().enumerate() {
            new_h[ri] = self.h_vec[i] - self.precision[(i, pos)] * value;
        }

        let new_log_norm = self.log_norm + self.h_vec[pos] * value
            - 0.5 * self.precision[(pos, pos)] * value * value;

        let new_vars: Vec<VariableName> = remaining
            .iter()
            .map(|&i| self.variables[i].clone())
            .collect();

        Ok(GaussianFactor {
            precision: new_precision,
            h_vec: new_h,
            log_norm: new_log_norm,
            variables: new_vars,
        })
    }

    /// Convert canonical form to moment form (mean, covariance).
    ///
    /// Σ = Λ⁻¹, μ = Σ h = Λ⁻¹ h
    pub fn to_moments(&self) -> BayesResult<(DVector<f64>, DMatrix<f64>)> {
        if self.variables.is_empty() {
            return Ok((DVector::zeros(0), DMatrix::zeros(0, 0)));
        }

        let chol = self.precision.clone().cholesky().ok_or_else(|| {
            BayesError::InferenceError(
                "Precision matrix is not positive definite; cannot convert to moments".to_string(),
            )
        })?;

        let covariance = chol.inverse();
        let mean = &covariance * &self.h_vec;

        Ok((mean, covariance))
    }
}

// ---------------------------------------------------------------------------
// MomentMatchingInference — exact inference for Gaussian / CLG networks
// ---------------------------------------------------------------------------

/// Exact inference engine for purely Gaussian and mixed CLG networks.
///
/// **Purely Gaussian**: builds the joint precision matrix from all
/// `GaussianFactor`s, conditions on continuous evidence, and reads off
/// posterior marginals via Cholesky inversion.
///
/// **Mixed CLG**: enumerates all discrete parent configurations, computes
/// the conditional Gaussian posterior for each configuration, and weights
/// by the discrete configuration probability.
#[derive(Debug, Clone, Default)]
pub struct MomentMatchingInference;

impl MomentMatchingInference {
    /// Create a new `MomentMatchingInference` engine.
    pub fn new() -> Self {
        Self
    }

    /// Infer posterior marginals for a purely Gaussian network.
    fn infer_purely_gaussian(
        &self,
        network: &BayesianNetwork,
        evidence: &Evidence,
    ) -> BayesResult<(
        HashMap<VariableName, ContinuousMarginal>,
        InferenceDiagnostics,
    )> {
        let gaussian_vars = network.gaussian_variables();

        let mut factors: Vec<GaussianFactor> = Vec::with_capacity(gaussian_vars.len());
        for gv in gaussian_vars.values() {
            let factor = GaussianFactor::from_gaussian_variable(gv, network)?;
            factors.push(factor);
        }

        if factors.is_empty() {
            return Ok((HashMap::new(), InferenceDiagnostics::default()));
        }

        // Multiply all factors into a single joint factor.
        let mut joint = factors[0].clone();
        for f in &factors[1..] {
            joint = joint.multiply(f);
        }

        // Condition on continuous evidence.
        for (var, obs) in evidence.all_observations() {
            if let ObservationType::Continuous(value) = obs
                && joint.variables.contains(var)
            {
                joint = joint.condition(var, *value)?;
            }
        }

        // Convert to moments and extract per-variable marginals.
        if joint.variables.is_empty() {
            return Ok((HashMap::new(), InferenceDiagnostics::default()));
        }

        let (mean, covariance) = joint.to_moments()?;

        let mut continuous_marginals = HashMap::new();
        for (i, vname) in joint.variables.iter().enumerate() {
            let m = mean[i];
            let v = covariance[(i, i)];
            continuous_marginals.insert(vname.clone(), ContinuousMarginal::new(m, v));
        }

        let diagnostics = InferenceDiagnostics {
            iterations: 0,
            burn_in: 0,
            max_marginal_change: 0.0,
            effective_sample_size: None,
        };

        Ok((continuous_marginals, diagnostics))
    }

    /// Infer posterior marginals for a mixed CLG network.
    ///
    /// Enumerates all discrete configurations, for each one computes
    /// the conditional Gaussian posterior, then mixes the results.
    fn infer_mixed_clg(
        &self,
        network: &BayesianNetwork,
        evidence: &Evidence,
    ) -> BayesResult<InferenceResult> {
        let start = Instant::now();

        let discrete_vars: Vec<VariableName> = network.variables().keys().cloned().collect();
        let gaussian_vars: Vec<VariableName> =
            network.gaussian_variables().keys().cloned().collect();

        // Observed discrete variables.
        let discrete_evidence: HashMap<VariableName, usize> = evidence
            .observations()
            .into_iter()
            .map(|(k, v)| (k, v.value()))
            .collect();

        // Unobserved discrete variables.
        let unobserved_discrete: Vec<VariableName> = discrete_vars
            .iter()
            .filter(|v| !discrete_evidence.contains_key(*v))
            .cloned()
            .collect();

        let cardinalities: Vec<usize> = unobserved_discrete
            .iter()
            .map(|v| network.variable(v).unwrap().cardinality())
            .collect();

        let num_configs: usize = if cardinalities.is_empty() {
            1
        } else {
            cardinalities.iter().product()
        };

        // Accumulators for discrete marginals (weighted counts).
        let mut discrete_marginal_accum: HashMap<VariableName, Vec<f64>> = HashMap::new();
        for v in &unobserved_discrete {
            let card = network.variable(v).unwrap().cardinality();
            discrete_marginal_accum.insert(v.clone(), vec![0.0; card]);
        }

        // Accumulators for continuous marginals.
        let mut cont_mean_accum: HashMap<VariableName, f64> = HashMap::new();
        let mut cont_var_accum: HashMap<VariableName, f64> = HashMap::new();
        let mut cont_mean_sq_accum: HashMap<VariableName, f64> = HashMap::new();
        for v in &gaussian_vars {
            if !evidence.is_observed(v) {
                cont_mean_accum.insert(v.clone(), 0.0);
                cont_var_accum.insert(v.clone(), 0.0);
                cont_mean_sq_accum.insert(v.clone(), 0.0);
            }
        }

        let mut total_weight = 0.0;

        for config_idx in 0..num_configs {
            // Decode configuration index into assignments.
            let mut assignment: HashMap<VariableName, usize> = discrete_evidence.clone();
            let mut remaining = config_idx;
            for (i, v) in unobserved_discrete.iter().enumerate().rev() {
                let card = cardinalities[i];
                assignment.insert(v.clone(), remaining % card);
                remaining /= card;
            }

            // Compute discrete configuration probability from CPTs.
            let log_prob = self.discrete_config_log_prob(network, &assignment)?;
            let weight = log_prob.exp();

            if weight < 1e-300 {
                continue;
            }

            // Build Gaussian posterior for this discrete configuration.
            let cont_marginals =
                self.gaussian_posterior_for_config(network, evidence, &assignment)?;

            total_weight += weight;

            // Accumulate discrete marginals.
            for v in &unobserved_discrete {
                let state = assignment[v];
                discrete_marginal_accum.get_mut(v).unwrap()[state] += weight;
            }

            // Accumulate continuous marginals using law of total expectation/variance.
            for (vname, cmarg) in &cont_marginals {
                if let Some(acc) = cont_mean_accum.get_mut(vname) {
                    *acc += weight * cmarg.mean;
                }
                if let Some(acc) = cont_var_accum.get_mut(vname) {
                    *acc += weight * cmarg.variance;
                }
                if let Some(acc) = cont_mean_sq_accum.get_mut(vname) {
                    *acc += weight * cmarg.mean * cmarg.mean;
                }
            }
        }

        if total_weight < 1e-300 {
            return Err(BayesError::ZeroProbabilityEvidence(
                "All discrete configurations have zero probability".to_string(),
            ));
        }

        // Normalize discrete marginals.
        let mut marginals = HashMap::new();
        let mut log_marginals = HashMap::new();
        for (v, counts) in &discrete_marginal_accum {
            let probs: Vec<f64> = counts.iter().map(|c| c / total_weight).collect();
            let logs: Vec<f64> = probs
                .iter()
                .map(|&p| if p > 0.0 { p.ln() } else { f64::NEG_INFINITY })
                .collect();
            marginals.insert(v.clone(), probs);
            log_marginals.insert(v.clone(), logs);
        }

        // Normalize continuous marginals using law of total variance.
        let mut continuous_marginals = HashMap::new();
        for v in &gaussian_vars {
            if let Some(&mean_acc) = cont_mean_accum.get(v) {
                let mean = mean_acc / total_weight;
                let var_within = cont_var_accum.get(v).unwrap_or(&0.0) / total_weight;
                let mean_sq = cont_mean_sq_accum.get(v).unwrap_or(&0.0) / total_weight;
                // Law of total variance: Var[X] = E[Var[X|D]] + Var[E[X|D]]
                let variance = var_within + (mean_sq - mean * mean).max(0.0);
                continuous_marginals.insert(v.clone(), ContinuousMarginal::new(mean, variance));
            }
        }

        let elapsed = start.elapsed();
        let diagnostics = InferenceDiagnostics {
            iterations: num_configs,
            burn_in: 0,
            max_marginal_change: 0.0,
            effective_sample_size: None,
        };

        Ok(InferenceResult {
            marginals,
            log_marginals,
            continuous_marginals,
            algorithm: "gaussian".to_string(),
            elapsed,
            diagnostics: Some(diagnostics),
            nuts_diagnostics: None,
        })
    }

    /// Compute the log-probability of a full discrete assignment using CPTs.
    fn discrete_config_log_prob(
        &self,
        network: &BayesianNetwork,
        assignment: &HashMap<VariableName, usize>,
    ) -> BayesResult<f64> {
        let mut log_prob = 0.0;

        for vname in network.variables().keys() {
            let cpt = network.cpt(vname).ok_or_else(|| {
                BayesError::IncompleteNetwork(format!("Variable '{}' has no CPT", vname))
            })?;

            let mut factor_assignment = Vec::with_capacity(cpt.variables.len());
            for fv in &cpt.variables {
                let state = assignment.get(fv).ok_or_else(|| {
                    BayesError::InferenceError(format!(
                        "Variable '{}' not in discrete assignment",
                        fv
                    ))
                })?;
                factor_assignment.push(StateIndex::new(*state));
            }

            let idx = cpt.assignment_to_index(&factor_assignment);
            log_prob += cpt.log_values[idx];
        }

        Ok(log_prob)
    }

    /// Compute Gaussian posterior marginals for a specific discrete configuration.
    ///
    /// For each Gaussian variable, its effective mean becomes:
    ///   mean_eff = mean_base + Σ w_disc * disc_state
    /// where `disc_state` is treated as the state index (integer encoding).
    fn gaussian_posterior_for_config(
        &self,
        network: &BayesianNetwork,
        evidence: &Evidence,
        discrete_assignment: &HashMap<VariableName, usize>,
    ) -> BayesResult<HashMap<VariableName, ContinuousMarginal>> {
        let gaussian_vars = network.gaussian_variables();
        if gaussian_vars.is_empty() {
            return Ok(HashMap::new());
        }

        let mut factors: Vec<GaussianFactor> = Vec::with_capacity(gaussian_vars.len());

        for gv in gaussian_vars.values() {
            // Compute the effective mean shift from discrete parents.
            let mut mean_shift = 0.0;
            for (pname, w) in &gv.weights {
                if network.is_discrete(pname)
                    && let Some(&state) = discrete_assignment.get(pname)
                {
                    mean_shift += w * (state as f64);
                }
            }

            // Build a temporary GaussianVariable with adjusted mean_base.
            let adjusted_gv = GaussianVariable {
                name: gv.name.clone(),
                mean_base: gv.mean_base + mean_shift,
                variance: gv.variance,
                weights: gv.weights.clone(),
            };

            let factor = GaussianFactor::from_gaussian_variable(&adjusted_gv, network)?;
            factors.push(factor);
        }

        if factors.is_empty() {
            return Ok(HashMap::new());
        }

        // Multiply all Gaussian factors into a joint.
        let mut joint = factors[0].clone();
        for f in &factors[1..] {
            joint = joint.multiply(f);
        }

        // Condition on continuous evidence.
        for (var, obs) in evidence.all_observations() {
            if let ObservationType::Continuous(value) = obs
                && joint.variables.contains(var)
            {
                joint = joint.condition(var, *value)?;
            }
        }

        if joint.variables.is_empty() {
            return Ok(HashMap::new());
        }

        let (mean, covariance) = joint.to_moments()?;

        let mut result = HashMap::new();
        for (i, vname) in joint.variables.iter().enumerate() {
            let m = mean[i];
            let v = covariance[(i, i)];
            result.insert(vname.clone(), ContinuousMarginal::new(m, v));
        }

        Ok(result)
    }
}

impl InferenceEngine for MomentMatchingInference {
    fn infer(
        &self,
        network: &BayesianNetwork,
        evidence: &Evidence,
    ) -> BayesResult<InferenceResult> {
        let start = Instant::now();

        // Validate discrete variables have CPTs.
        network.validate()?;

        let has_discrete = !network.variables().is_empty();

        if !has_discrete {
            // Purely Gaussian.
            let (continuous_marginals, diagnostics) =
                self.infer_purely_gaussian(network, evidence)?;

            return Ok(InferenceResult {
                marginals: HashMap::new(),
                log_marginals: HashMap::new(),
                continuous_marginals,
                algorithm: "gaussian".to_string(),
                elapsed: start.elapsed(),
                diagnostics: Some(diagnostics),
                nuts_diagnostics: None,
            });
        }

        // Mixed CLG.
        self.infer_mixed_clg(network, evidence)
    }

    fn query(
        &self,
        network: &BayesianNetwork,
        evidence: &Evidence,
        variable: &VariableName,
    ) -> BayesResult<Marginal> {
        let result = self.infer(network, evidence)?;
        // For discrete variables, return the marginal probabilities.
        if let Some(marginal) = result.marginals.get(variable) {
            return Ok(marginal.clone());
        }
        // For continuous variables, return [mean, variance] as a 2-element vector.
        if let Some(cm) = result.continuous_marginals.get(variable) {
            return Ok(vec![cm.mean, cm.variance]);
        }
        Err(BayesError::InferenceError(format!(
            "Variable '{}' not found in inference results (may be observed)",
            variable
        )))
    }

    fn algorithm_name(&self) -> &str {
        "gaussian"
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;
    use crate::providers::bayesian::network::BayesianNetwork;
    use crate::providers::bayesian::types::{
        DiscreteVariable, GaussianVariable, StateName, VariableName,
    };

    fn var_name(s: &str) -> VariableName {
        VariableName::new(s).unwrap()
    }

    fn state(s: &str) -> StateName {
        StateName::new(s).unwrap()
    }

    /// Build a purely Gaussian 5-node chain: X1 → X2 → X3 → X4 → X5.
    /// X1: mean=5.0, var=2.0 (root)
    /// Xi: mean = 0 + 1.0*X(i-1), var=1.0 (i=2..5)
    fn build_gaussian_chain() -> BayesianNetwork {
        let mut net = BayesianNetwork::new();

        let x1 = GaussianVariable::new("X1", 5.0, 2.0).unwrap();
        let x2 = GaussianVariable::new("X2", 0.0, 1.0)
            .unwrap()
            .with_weight("X1", 1.0)
            .unwrap();
        let x3 = GaussianVariable::new("X3", 0.0, 1.0)
            .unwrap()
            .with_weight("X2", 1.0)
            .unwrap();
        let x4 = GaussianVariable::new("X4", 0.0, 1.0)
            .unwrap()
            .with_weight("X3", 1.0)
            .unwrap();
        let x5 = GaussianVariable::new("X5", 0.0, 1.0)
            .unwrap()
            .with_weight("X4", 1.0)
            .unwrap();

        net.add_gaussian_variable(x1).unwrap();
        net.add_gaussian_variable(x2).unwrap();
        net.add_gaussian_variable(x3).unwrap();
        net.add_gaussian_variable(x4).unwrap();
        net.add_gaussian_variable(x5).unwrap();

        net.add_edge(&var_name("X1"), &var_name("X2")).unwrap();
        net.add_edge(&var_name("X2"), &var_name("X3")).unwrap();
        net.add_edge(&var_name("X3"), &var_name("X4")).unwrap();
        net.add_edge(&var_name("X4"), &var_name("X5")).unwrap();

        net
    }

    #[test]
    fn gaussian_factor_from_variable() {
        let mut net = BayesianNetwork::new();
        let gv = GaussianVariable::new("X", 5.0, 2.0).unwrap();
        net.add_gaussian_variable(gv.clone()).unwrap();

        let factor = GaussianFactor::from_gaussian_variable(&gv, &net).unwrap();

        assert_eq!(factor.variables.len(), 1);
        assert_eq!(factor.variables[0], var_name("X"));

        // Precision = 1/variance = 0.5
        assert!(
            (factor.precision[(0, 0)] - 0.5).abs() < 1e-10,
            "precision = {}",
            factor.precision[(0, 0)]
        );

        // h = precision * mean = 0.5 * 5 = 2.5
        assert!(
            (factor.h_vec[0] - 2.5).abs() < 1e-10,
            "h = {}",
            factor.h_vec[0]
        );
    }

    #[test]
    fn gaussian_factor_multiply_independent() {
        let mut net = BayesianNetwork::new();
        let gv_x = GaussianVariable::new("X", 2.0, 1.0).unwrap();
        let gv_y = GaussianVariable::new("Y", 3.0, 0.5).unwrap();
        net.add_gaussian_variable(gv_x.clone()).unwrap();
        net.add_gaussian_variable(gv_y.clone()).unwrap();

        let fx = GaussianFactor::from_gaussian_variable(&gv_x, &net).unwrap();
        let fy = GaussianFactor::from_gaussian_variable(&gv_y, &net).unwrap();
        let joint = fx.multiply(&fy);

        assert_eq!(joint.variables.len(), 2);

        let x_idx = joint
            .variables
            .iter()
            .position(|v| v == &var_name("X"))
            .unwrap();
        let y_idx = joint
            .variables
            .iter()
            .position(|v| v == &var_name("Y"))
            .unwrap();

        // Diagonal: [1/1, 1/0.5] = [1, 2]
        assert!((joint.precision[(x_idx, x_idx)] - 1.0).abs() < 1e-10);
        assert!((joint.precision[(y_idx, y_idx)] - 2.0).abs() < 1e-10);
        assert!(joint.precision[(x_idx, y_idx)].abs() < 1e-10);

        // h = [1*2, 2*3] = [2, 6]
        assert!((joint.h_vec[x_idx] - 2.0).abs() < 1e-10);
        assert!((joint.h_vec[y_idx] - 6.0).abs() < 1e-10);
    }

    #[test]
    fn gaussian_factor_marginalize_independent() {
        let mut net = BayesianNetwork::new();
        let gv_x = GaussianVariable::new("X", 2.0, 1.0).unwrap();
        let gv_y = GaussianVariable::new("Y", 3.0, 0.5).unwrap();
        net.add_gaussian_variable(gv_x.clone()).unwrap();
        net.add_gaussian_variable(gv_y.clone()).unwrap();

        let fx = GaussianFactor::from_gaussian_variable(&gv_x, &net).unwrap();
        let fy = GaussianFactor::from_gaussian_variable(&gv_y, &net).unwrap();
        let joint = fx.multiply(&fy);

        let marginal_x = joint.marginalize(&var_name("Y")).unwrap();
        assert_eq!(marginal_x.variables.len(), 1);
        assert_eq!(marginal_x.variables[0], var_name("X"));

        // Independent marginalization preserves X's params.
        assert!((marginal_x.precision[(0, 0)] - 1.0).abs() < 1e-10);
        assert!((marginal_x.h_vec[0] - 2.0).abs() < 1e-10);

        let (mean, cov) = marginal_x.to_moments().unwrap();
        assert!((mean[0] - 2.0).abs() < 1e-10);
        assert!((cov[(0, 0)] - 1.0).abs() < 1e-10);
    }

    #[test]
    fn gaussian_factor_condition_linear() {
        // X → Y: Y ~ N(X * 1.0 + 0.0, 1.0), X ~ N(0.0, 1.0)
        let mut net = BayesianNetwork::new();
        let gv_x = GaussianVariable::new("X", 0.0, 1.0).unwrap();
        let gv_y = GaussianVariable::new("Y", 0.0, 1.0)
            .unwrap()
            .with_weight("X", 1.0)
            .unwrap();
        net.add_gaussian_variable(gv_x.clone()).unwrap();
        net.add_gaussian_variable(gv_y.clone()).unwrap();
        net.add_edge(&var_name("X"), &var_name("Y")).unwrap();

        let fx = GaussianFactor::from_gaussian_variable(&gv_x, &net).unwrap();
        let fy = GaussianFactor::from_gaussian_variable(&gv_y, &net).unwrap();
        let joint = fx.multiply(&fy);

        // Condition on X = 3.
        let posterior = joint.condition(&var_name("X"), 3.0).unwrap();
        assert_eq!(posterior.variables.len(), 1);
        assert_eq!(posterior.variables[0], var_name("Y"));

        // Y|X=3 ~ N(3, 1)
        let (mean, cov) = posterior.to_moments().unwrap();
        assert!(
            (mean[0] - 3.0).abs() < 1e-6,
            "E[Y|X=3] = {}, expected 3.0",
            mean[0]
        );
        assert!(
            (cov[(0, 0)] - 1.0).abs() < 1e-6,
            "Var[Y|X=3] = {}, expected 1.0",
            cov[(0, 0)]
        );
    }

    #[test]
    fn gaussian_factor_to_moments_roundtrip() {
        let mut net = BayesianNetwork::new();
        let gv = GaussianVariable::new("X", 5.0, 2.0).unwrap();
        net.add_gaussian_variable(gv.clone()).unwrap();

        let factor = GaussianFactor::from_gaussian_variable(&gv, &net).unwrap();
        let (mean, cov) = factor.to_moments().unwrap();

        assert!((mean[0] - 5.0).abs() < 1e-10);
        assert!((cov[(0, 0)] - 2.0).abs() < 1e-10);
    }

    #[test]
    fn moment_matching_purely_gaussian_chain() {
        let net = build_gaussian_chain();
        let evidence = Evidence::new();
        let engine = MomentMatchingInference::new();

        let result = engine.infer(&net, &evidence).unwrap();
        assert_eq!(result.algorithm, "gaussian");

        // All means should be 5.0 (linear pass-through).
        for i in 1..=5 {
            let vname = var_name(&format!("X{}", i));
            let cm = result
                .continuous_marginals
                .get(&vname)
                .unwrap_or_else(|| panic!("Missing marginal for X{}", i));
            assert!(
                (cm.mean - 5.0).abs() < 1e-6,
                "E[X{}] = {}, expected 5.0",
                i,
                cm.mean
            );
        }

        // Variances: 2, 3, 4, 5, 6 (accumulating noise).
        let expected_variances = [2.0, 3.0, 4.0, 5.0, 6.0];
        for (i, &expected_var) in expected_variances.iter().enumerate() {
            let vname = var_name(&format!("X{}", i + 1));
            let cm = result.continuous_marginals.get(&vname).unwrap();
            assert!(
                (cm.variance - expected_var).abs() < 1e-6,
                "Var[X{}] = {}, expected {}",
                i + 1,
                cm.variance,
                expected_var
            );
        }
    }

    #[test]
    fn moment_matching_with_evidence() {
        let net = build_gaussian_chain();
        let mut evidence = Evidence::new();
        evidence
            .observe_continuous(&net, &var_name("X1"), 3.0)
            .unwrap();

        let engine = MomentMatchingInference::new();
        let result = engine.infer(&net, &evidence).unwrap();

        // Posterior means shift to 3.0.
        for i in 2..=5 {
            let vname = var_name(&format!("X{}", i));
            let cm = result
                .continuous_marginals
                .get(&vname)
                .unwrap_or_else(|| panic!("Missing marginal for X{}", i));
            assert!(
                (cm.mean - 3.0).abs() < 1e-6,
                "E[X{}|X1=3] = {}, expected 3.0",
                i,
                cm.mean
            );
        }

        // Posterior variances: Var[Xi|X1] = i-1 (1,2,3,4 for X2..X5).
        let expected_posterior_variances = [1.0, 2.0, 3.0, 4.0];
        for (i, &expected_var) in expected_posterior_variances.iter().enumerate() {
            let vname = var_name(&format!("X{}", i + 2));
            let cm = result.continuous_marginals.get(&vname).unwrap();
            assert!(
                (cm.variance - expected_var).abs() < 1e-6,
                "Var[X{}|X1=3] = {}, expected {}",
                i + 2,
                cm.variance,
                expected_var
            );
        }
    }

    #[test]
    fn moment_matching_mixed_clg() {
        // D (binary) → G1, G2
        // P(D=off)=0.3, P(D=on)=0.7
        // G1 ~ N(D*10, 1), G2 ~ N(D*5, 2)
        let mut net = BayesianNetwork::new();

        let d_var = DiscreteVariable::new(var_name("D"), vec![state("off"), state("on")]).unwrap();
        net.add_variable(d_var).unwrap();
        net.set_cpt(&var_name("D"), vec![0.3, 0.7]).unwrap();

        let g1 = GaussianVariable::new("G1", 0.0, 1.0)
            .unwrap()
            .with_weight("D", 10.0)
            .unwrap();
        let g2 = GaussianVariable::new("G2", 0.0, 2.0)
            .unwrap()
            .with_weight("D", 5.0)
            .unwrap();
        net.add_gaussian_variable(g1).unwrap();
        net.add_gaussian_variable(g2).unwrap();
        net.add_edge(&var_name("D"), &var_name("G1")).unwrap();
        net.add_edge(&var_name("D"), &var_name("G2")).unwrap();

        let evidence = Evidence::new();
        let engine = MomentMatchingInference::new();
        let result = engine.infer(&net, &evidence).unwrap();

        // Discrete: P(D) should match prior.
        let d_marginal = result.marginals.get(&var_name("D")).unwrap();
        assert!(
            (d_marginal[0] - 0.3).abs() < 1e-6,
            "P(D=off) = {}",
            d_marginal[0]
        );
        assert!(
            (d_marginal[1] - 0.7).abs() < 1e-6,
            "P(D=on) = {}",
            d_marginal[1]
        );

        // E[G1] = 0.3*0 + 0.7*10 = 7.0
        let g1m = result.continuous_marginals.get(&var_name("G1")).unwrap();
        assert!((g1m.mean - 7.0).abs() < 1e-6, "E[G1] = {}", g1m.mean);

        // E[G2] = 0.3*0 + 0.7*5 = 3.5
        let g2m = result.continuous_marginals.get(&var_name("G2")).unwrap();
        assert!((g2m.mean - 3.5).abs() < 1e-6, "E[G2] = {}", g2m.mean);

        // Var[G1] = E[Var[G1|D]] + Var[E[G1|D]] = 1 + 0.3*49+0.7*9 = 1+21 = 22
        assert!(
            (g1m.variance - 22.0).abs() < 1e-6,
            "Var[G1] = {}",
            g1m.variance
        );

        // Var[G2] = 2 + 0.3*12.25+0.7*2.25 = 2+5.25 = 7.25
        assert!(
            (g2m.variance - 7.25).abs() < 1e-6,
            "Var[G2] = {}",
            g2m.variance
        );

        // 95% CI is set.
        assert!(g1m.ci_lower < g1m.mean);
        assert!(g1m.ci_upper > g1m.mean);
    }

    #[test]
    fn moment_matching_zero_variance_error() {
        // Construction-time validation.
        let result = GaussianVariable::new("X", 0.0, 0.0);
        assert!(result.is_err());

        // Factory method validation.
        let mut gv = GaussianVariable::new("X", 0.0, 1.0).unwrap();
        gv.variance = 0.0;
        let net = BayesianNetwork::new();
        let result = GaussianFactor::from_gaussian_variable(&gv, &net);
        assert!(result.is_err());
    }
}
