//! Factor representation for Bayesian Network inference.
//!
//! Factors store conditional probability tables (CPTs) in log-space to prevent
//! numerical underflow. Operations include multiplication (log-addition),
//! marginalization, normalization (log-sum-exp), and mixed-radix parent-config indexing.

use super::error::BayesResult;
use super::types::{StateIndex, VariableName};

/// A factor over a set of discrete variables, stored in log-space.
///
/// The values represent log-probabilities. Factor operations are performed
/// in log-space (multiplication becomes addition, normalization uses log-sum-exp).
#[derive(Debug, Clone)]
pub struct Factor {
    /// Ordered variable names that define the factor's scope.
    pub variables: Vec<VariableName>,
    /// Cardinalities for each variable (same order as `variables`).
    pub cardinalities: Vec<usize>,
    /// Log-probability values in row-major (mixed-radix) order.
    pub log_values: Vec<f64>,
}

impl Factor {
    /// Create a new factor from probability values (converted to log-space).
    ///
    /// # Errors
    /// Returns an error if the number of values doesn't match the product of cardinalities,
    /// or if any probability is negative.
    pub fn from_probabilities(
        variables: Vec<VariableName>,
        cardinalities: Vec<usize>,
        probabilities: Vec<f64>,
    ) -> BayesResult<Self> {
        let expected_size: usize = cardinalities.iter().product();
        if probabilities.len() != expected_size {
            return Err(super::BayesError::InvalidCpt(format!(
                "Expected {} values, got {}",
                expected_size,
                probabilities.len()
            )));
        }
        if probabilities.iter().any(|&p| p < 0.0) {
            return Err(super::BayesError::InvalidCpt(
                "Probabilities cannot be negative".into(),
            ));
        }
        let log_values = probabilities
            .iter()
            .map(|&p| if p == 0.0 { f64::NEG_INFINITY } else { p.ln() })
            .collect();

        Ok(Self {
            variables,
            cardinalities,
            log_values,
        })
    }

    /// Create a factor directly from log-probability values.
    pub fn from_log_values(
        variables: Vec<VariableName>,
        cardinalities: Vec<usize>,
        log_values: Vec<f64>,
    ) -> BayesResult<Self> {
        let expected_size: usize = cardinalities.iter().product();
        if log_values.len() != expected_size {
            return Err(super::BayesError::InvalidCpt(format!(
                "Expected {} log-values, got {}",
                expected_size,
                log_values.len()
            )));
        }
        Ok(Self {
            variables,
            cardinalities,
            log_values,
        })
    }

    /// Total number of entries in this factor.
    pub fn size(&self) -> usize {
        self.log_values.len()
    }

    /// Convert a flat index to a mixed-radix assignment tuple.
    pub fn index_to_assignment(&self, mut index: usize) -> Vec<StateIndex> {
        let mut assignment = vec![StateIndex::new(0); self.cardinalities.len()];
        for i in (0..self.cardinalities.len()).rev() {
            assignment[i] = StateIndex::new(index % self.cardinalities[i]);
            index /= self.cardinalities[i];
        }
        assignment
    }

    /// Convert a mixed-radix assignment tuple to a flat index.
    pub fn assignment_to_index(&self, assignment: &[StateIndex]) -> usize {
        let mut index = 0;
        let mut stride = 1;
        for i in (0..self.cardinalities.len()).rev() {
            index += assignment[i].value() * stride;
            stride *= self.cardinalities[i];
        }
        index
    }

    /// Multiply two factors (add in log-space).
    ///
    /// The result factor's scope is the union of both factors' scopes.
    pub fn multiply(&self, other: &Factor) -> BayesResult<Factor> {
        // Build the union of variable scopes
        let mut result_vars = self.variables.clone();
        let mut result_cards = self.cardinalities.clone();
        let mut other_var_indices = Vec::new();

        for (i, var) in other.variables.iter().enumerate() {
            if let Some(pos) = result_vars.iter().position(|v| v == var) {
                other_var_indices.push(pos);
            } else {
                other_var_indices.push(result_vars.len());
                result_vars.push(var.clone());
                result_cards.push(other.cardinalities[i]);
            }
        }

        let result_size: usize = result_cards.iter().product();
        let mut result_log_values = vec![0.0_f64; result_size];

        let result_factor_template = Factor::from_log_values(
            result_vars.clone(),
            result_cards.clone(),
            vec![0.0; result_size],
        )?;

        for (idx, val) in result_log_values.iter_mut().enumerate().take(result_size) {
            let assignment = result_factor_template.index_to_assignment(idx);

            // Project assignment onto self's variables
            let self_assignment: Vec<StateIndex> = (0..self.variables.len())
                .map(|i| {
                    let pos = result_vars
                        .iter()
                        .position(|v| v == &self.variables[i])
                        .unwrap();
                    assignment[pos]
                })
                .collect();
            let self_idx = self.assignment_to_index(&self_assignment);

            // Project assignment onto other's variables
            let other_assignment: Vec<StateIndex> = other_var_indices
                .iter()
                .map(|&pos| assignment[pos])
                .collect();
            let other_idx = other.assignment_to_index(&other_assignment);

            *val = self.log_values[self_idx] + other.log_values[other_idx];
        }

        Factor::from_log_values(result_vars, result_cards, result_log_values)
    }

    /// Marginalize out a variable (log-sum-exp over its states).
    pub fn marginalize(&self, var: &VariableName) -> BayesResult<Factor> {
        let var_pos = self
            .variables
            .iter()
            .position(|v| v == var)
            .ok_or_else(|| {
                super::BayesError::InferenceError(format!("Variable '{}' not in factor scope", var))
            })?;

        let mut result_vars = self.variables.clone();
        let mut result_cards = self.cardinalities.clone();
        result_vars.remove(var_pos);
        let _removed_card = result_cards.remove(var_pos);

        if result_vars.is_empty() {
            // Marginalizing the last variable: return a scalar factor
            let log_sum = log_sum_exp(&self.log_values);
            return Factor::from_log_values(vec![], vec![], vec![log_sum]);
        }

        let result_size: usize = result_cards.iter().product();
        let mut result_log_values = vec![f64::NEG_INFINITY; result_size];

        let result_factor_template = Factor::from_log_values(
            result_vars.clone(),
            result_cards.clone(),
            vec![0.0; result_size],
        )?;

        for idx in 0..self.size() {
            let assignment = self.index_to_assignment(idx);
            // Build the result assignment by removing the marginalized variable
            let result_assignment: Vec<StateIndex> = assignment
                .iter()
                .enumerate()
                .filter(|&(i, _)| i != var_pos)
                .map(|(_, &s)| s)
                .collect();
            let result_idx = result_factor_template.assignment_to_index(&result_assignment);

            // log-sum-exp accumulation
            let a = result_log_values[result_idx];
            let b = self.log_values[idx];
            result_log_values[result_idx] = log_add_exp(a, b);
        }

        Factor::from_log_values(result_vars, result_cards, result_log_values)
    }

    /// Normalize this factor so probabilities sum to 1 (in log-space: subtract log-partition).
    pub fn normalize(&self) -> BayesResult<Factor> {
        let log_z = log_sum_exp(&self.log_values);
        if log_z == f64::NEG_INFINITY {
            return Err(super::BayesError::ZeroProbabilityEvidence(
                "Cannot normalize factor with all-zero probabilities".into(),
            ));
        }
        let normalized = self.log_values.iter().map(|&v| v - log_z).collect();
        Factor::from_log_values(
            self.variables.clone(),
            self.cardinalities.clone(),
            normalized,
        )
    }

    /// Get probability values (exponentiated from log-space).
    pub fn to_probabilities(&self) -> Vec<f64> {
        self.log_values.iter().map(|&v| v.exp()).collect()
    }

    /// Reduce this factor by fixing a variable to a specific state.
    ///
    /// Returns a new factor with the specified variable removed from scope,
    /// keeping only entries consistent with the evidence.
    pub fn reduce(&self, var: &VariableName, state: StateIndex) -> BayesResult<Factor> {
        let var_pos = self
            .variables
            .iter()
            .position(|v| v == var)
            .ok_or_else(|| {
                super::BayesError::InferenceError(format!("Variable '{}' not in factor scope", var))
            })?;

        let mut result_vars = self.variables.clone();
        let mut result_cards = self.cardinalities.clone();
        result_vars.remove(var_pos);
        result_cards.remove(var_pos);

        if result_vars.is_empty() {
            // Single-variable factor reduced to scalar
            return Factor::from_log_values(vec![], vec![], vec![self.log_values[state.value()]]);
        }

        let result_size: usize = result_cards.iter().product();
        let mut result_log_values = Vec::with_capacity(result_size);

        for idx in 0..self.size() {
            let assignment = self.index_to_assignment(idx);
            if assignment[var_pos] == state {
                result_log_values.push(self.log_values[idx]);
            }
        }

        Factor::from_log_values(result_vars, result_cards, result_log_values)
    }
}

/// Compute log(sum(exp(values))) numerically stably.
fn log_sum_exp(values: &[f64]) -> f64 {
    if values.is_empty() {
        return f64::NEG_INFINITY;
    }
    let max_val = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    if max_val == f64::NEG_INFINITY {
        return f64::NEG_INFINITY;
    }
    let sum_exp: f64 = values.iter().map(|&v| (v - max_val).exp()).sum();
    max_val + sum_exp.ln()
}

/// Compute log(exp(a) + exp(b)) numerically stably.
fn log_add_exp(a: f64, b: f64) -> f64 {
    if a == f64::NEG_INFINITY {
        return b;
    }
    if b == f64::NEG_INFINITY {
        return a;
    }
    let max_val = a.max(b);
    max_val + ((a - max_val).exp() + (b - max_val).exp()).ln()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn var(name: &str) -> VariableName {
        VariableName::new(name).unwrap()
    }

    #[test]
    fn factor_from_probabilities() {
        let f = Factor::from_probabilities(vec![var("A")], vec![2], vec![0.3, 0.7]).unwrap();
        assert_eq!(f.size(), 2);
        let probs = f.to_probabilities();
        assert!((probs[0] - 0.3).abs() < 1e-10);
        assert!((probs[1] - 0.7).abs() < 1e-10);
    }

    #[test]
    fn factor_negative_probability_rejected() {
        let result = Factor::from_probabilities(vec![var("A")], vec![2], vec![0.5, -0.1]);
        assert!(result.is_err());
    }

    #[test]
    fn factor_wrong_size_rejected() {
        let result = Factor::from_probabilities(vec![var("A")], vec![2], vec![0.5]);
        assert!(result.is_err());
    }

    #[test]
    fn factor_zero_probability_maps_to_neg_infinity() {
        let f = Factor::from_probabilities(vec![var("A")], vec![2], vec![0.0, 1.0]).unwrap();
        assert_eq!(f.log_values[0], f64::NEG_INFINITY);
        assert!((f.log_values[1] - 0.0).abs() < 1e-10);
    }

    #[test]
    fn mixed_radix_indexing_roundtrip() {
        let f = Factor::from_probabilities(
            vec![var("A"), var("B")],
            vec![2, 3],
            vec![0.1, 0.2, 0.3, 0.15, 0.1, 0.15],
        )
        .unwrap();

        for idx in 0..f.size() {
            let assignment = f.index_to_assignment(idx);
            let back = f.assignment_to_index(&assignment);
            assert_eq!(idx, back, "Round-trip failed for index {}", idx);
        }
    }

    #[test]
    fn factor_multiplication() {
        // P(A) * P(B|A) should give a joint factor over (A, B)
        let fa = Factor::from_probabilities(vec![var("A")], vec![2], vec![0.6, 0.4]).unwrap();
        let fb_given_a = Factor::from_probabilities(
            vec![var("A"), var("B")],
            vec![2, 2],
            vec![0.2, 0.8, 0.75, 0.25],
        )
        .unwrap();

        let joint = fa.multiply(&fb_given_a).unwrap();
        let probs = joint.to_probabilities();

        // P(A=0,B=0) = 0.6 * 0.2 = 0.12
        // P(A=0,B=1) = 0.6 * 0.8 = 0.48
        // P(A=1,B=0) = 0.4 * 0.75 = 0.30
        // P(A=1,B=1) = 0.4 * 0.25 = 0.10
        assert!((probs[0] - 0.12).abs() < 1e-10);
        assert!((probs[1] - 0.48).abs() < 1e-10);
        assert!((probs[2] - 0.30).abs() < 1e-10);
        assert!((probs[3] - 0.10).abs() < 1e-10);
    }

    #[test]
    fn factor_marginalization() {
        let joint = Factor::from_probabilities(
            vec![var("A"), var("B")],
            vec![2, 2],
            vec![0.12, 0.48, 0.30, 0.10],
        )
        .unwrap();

        let marginal_a = joint.marginalize(&var("B")).unwrap();
        let probs = marginal_a.to_probabilities();

        // P(A=0) = 0.12 + 0.48 = 0.60
        // P(A=1) = 0.30 + 0.10 = 0.40
        assert!((probs[0] - 0.60).abs() < 1e-10);
        assert!((probs[1] - 0.40).abs() < 1e-10);
    }

    #[test]
    fn factor_normalization() {
        let f = Factor::from_probabilities(vec![var("A")], vec![2], vec![2.0, 3.0]).unwrap();
        let normed = f.normalize().unwrap();
        let probs = normed.to_probabilities();
        assert!((probs[0] - 0.4).abs() < 1e-10);
        assert!((probs[1] - 0.6).abs() < 1e-10);
    }

    #[test]
    fn factor_normalize_all_zero_fails() {
        let f = Factor::from_probabilities(vec![var("A")], vec![2], vec![0.0, 0.0]).unwrap();
        assert!(f.normalize().is_err());
    }

    #[test]
    fn log_sum_exp_correctness() {
        let result = log_sum_exp(&[0.0_f64.ln(), 0.0_f64.ln()]);
        // ln(0) + ln(0) = -inf, log_sum_exp should be -inf
        assert_eq!(result, f64::NEG_INFINITY);

        let result = log_sum_exp(&[0.5_f64.ln(), 0.5_f64.ln()]);
        assert!((result.exp() - 1.0).abs() < 1e-10);
    }

    #[test]
    fn factor_reduce() {
        let f = Factor::from_probabilities(
            vec![var("A"), var("B")],
            vec![2, 2],
            vec![0.1, 0.2, 0.3, 0.4],
        )
        .unwrap();

        let reduced = f.reduce(&var("A"), StateIndex::new(0)).unwrap();
        let probs = reduced.to_probabilities();
        assert_eq!(probs.len(), 2);
        assert!((probs[0] - 0.1).abs() < 1e-10);
        assert!((probs[1] - 0.2).abs() < 1e-10);
    }
}
