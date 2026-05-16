//! Evidence management for Bayesian Network inference.
//!
//! Provides the `Evidence` struct for setting, retracting, and querying
//! observed variables during inference. Supports both discrete and continuous
//! observations via `ObservationType`.

use std::collections::HashMap;

use super::error::{BayesError, BayesResult};
use super::network::BayesianNetwork;
use super::types::{ObservationType, StateIndex, VariableName};

/// Observed evidence on variables in a Bayesian Network.
#[derive(Debug, Clone)]
pub struct Evidence {
    /// Observed variable → observation (discrete state index or continuous value).
    observations: HashMap<VariableName, ObservationType>,
}

impl Evidence {
    /// Create a new, empty evidence set.
    pub fn new() -> Self {
        Self {
            observations: HashMap::new(),
        }
    }

    /// Set discrete evidence: observe that `variable` is in state `state_name`.
    ///
    /// # Errors
    /// Returns an error if the variable or state doesn't exist in the network,
    /// or if the variable is a continuous (Gaussian) variable.
    pub fn observe(
        &mut self,
        network: &BayesianNetwork,
        variable: &VariableName,
        state_name: &str,
    ) -> BayesResult<()> {
        // Check if this is a Gaussian variable — cannot set discrete evidence on it
        if network.gaussian_variable(variable).is_some() {
            return Err(BayesError::DiscreteEvidenceOnContinuous(format!(
                "Variable '{}' is continuous (Gaussian); use observe_continuous() instead",
                variable
            )));
        }

        let var = network.variable(variable).ok_or_else(|| {
            BayesError::InvalidEvidence(format!("Variable '{}' not in network", variable))
        })?;
        let state_idx = var.state_index(state_name).ok_or_else(|| {
            BayesError::InvalidEvidence(format!(
                "State '{}' not found for variable '{}'",
                state_name, variable
            ))
        })?;
        self.observations
            .insert(variable.clone(), ObservationType::Discrete(state_idx));
        Ok(())
    }

    /// Set continuous evidence: observe that `variable` has value `value`.
    ///
    /// # Errors
    /// Returns an error if the variable is not a Gaussian variable in the network,
    /// or if the value is NaN or Inf.
    pub fn observe_continuous(
        &mut self,
        network: &BayesianNetwork,
        variable: &VariableName,
        value: f64,
    ) -> BayesResult<()> {
        if !value.is_finite() {
            return Err(BayesError::InvalidEvidence(format!(
                "Continuous evidence value must be finite, got {}",
                value
            )));
        }

        // Must be a Gaussian variable
        if network.gaussian_variable(variable).is_none() {
            // Check if it's a discrete variable
            if network.variable(variable).is_some() {
                return Err(BayesError::ContinuousEvidenceOnDiscrete(format!(
                    "Variable '{}' is discrete; use observe() instead",
                    variable
                )));
            }
            return Err(BayesError::InvalidEvidence(format!(
                "Variable '{}' not in network",
                variable
            )));
        }

        self.observations
            .insert(variable.clone(), ObservationType::Continuous(value));
        Ok(())
    }

    /// Retract evidence for a variable.
    ///
    /// Returns true if evidence was retracted, false if there was none.
    pub fn retract(&mut self, variable: &VariableName) -> bool {
        self.observations.remove(variable).is_some()
    }

    /// Clear all evidence.
    pub fn clear(&mut self) {
        self.observations.clear();
    }

    /// Check if a variable has been observed.
    pub fn is_observed(&self, variable: &VariableName) -> bool {
        self.observations.contains_key(variable)
    }

    /// Get the observed state index for a discrete variable (backward-compatible).
    pub fn get(&self, variable: &VariableName) -> Option<StateIndex> {
        match self.observations.get(variable) {
            Some(ObservationType::Discrete(idx)) => Some(*idx),
            _ => None,
        }
    }

    /// Get the observation type for a variable.
    pub fn get_observation(&self, variable: &VariableName) -> Option<&ObservationType> {
        self.observations.get(variable)
    }

    /// Get all observations (backward-compatible: returns only discrete observations as StateIndex).
    pub fn observations(&self) -> HashMap<VariableName, StateIndex> {
        self.observations
            .iter()
            .filter_map(|(k, v)| match v {
                ObservationType::Discrete(idx) => Some((k.clone(), *idx)),
                ObservationType::Continuous(_) => None,
            })
            .collect()
    }

    /// Get all observations including continuous.
    pub fn all_observations(&self) -> &HashMap<VariableName, ObservationType> {
        &self.observations
    }

    /// Number of observed variables.
    pub fn len(&self) -> usize {
        self.observations.len()
    }

    /// Check if no evidence has been set.
    pub fn is_empty(&self) -> bool {
        self.observations.is_empty()
    }
}

impl Default for Evidence {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::bayesian::types::{DiscreteVariable, GaussianVariable, StateName};

    fn var_name(s: &str) -> VariableName {
        VariableName::new(s).unwrap()
    }

    fn state(s: &str) -> StateName {
        StateName::new(s).unwrap()
    }

    fn make_network() -> BayesianNetwork {
        let mut net = BayesianNetwork::new();
        net.add_variable(
            DiscreteVariable::new(var_name("Smoking"), vec![state("yes"), state("no")]).unwrap(),
        )
        .unwrap();
        net.add_variable(
            DiscreteVariable::new(var_name("Cancer"), vec![state("present"), state("absent")])
                .unwrap(),
        )
        .unwrap();
        net
    }

    fn make_mixed_network() -> BayesianNetwork {
        let mut net = make_network();
        let gv = GaussianVariable::new("Temperature", 20.0, 5.0).unwrap();
        net.add_gaussian_variable(gv).unwrap();
        net
    }

    #[test]
    fn observe_and_query() {
        let net = make_network();
        let mut ev = Evidence::new();
        ev.observe(&net, &var_name("Smoking"), "yes").unwrap();
        assert!(ev.is_observed(&var_name("Smoking")));
        assert!(!ev.is_observed(&var_name("Cancer")));
        assert_eq!(ev.len(), 1);
    }

    #[test]
    fn observe_invalid_variable() {
        let net = make_network();
        let mut ev = Evidence::new();
        assert!(ev.observe(&net, &var_name("Unknown"), "yes").is_err());
    }

    #[test]
    fn observe_invalid_state() {
        let net = make_network();
        let mut ev = Evidence::new();
        assert!(ev.observe(&net, &var_name("Smoking"), "maybe").is_err());
    }

    #[test]
    fn retract_evidence() {
        let net = make_network();
        let mut ev = Evidence::new();
        ev.observe(&net, &var_name("Smoking"), "yes").unwrap();
        assert!(ev.retract(&var_name("Smoking")));
        assert!(!ev.is_observed(&var_name("Smoking")));
        assert!(ev.is_empty());
    }

    #[test]
    fn retract_nonexistent_returns_false() {
        let mut ev = Evidence::new();
        assert!(!ev.retract(&var_name("X")));
    }

    #[test]
    fn clear_evidence() {
        let net = make_network();
        let mut ev = Evidence::new();
        ev.observe(&net, &var_name("Smoking"), "yes").unwrap();
        ev.observe(&net, &var_name("Cancer"), "present").unwrap();
        assert_eq!(ev.len(), 2);
        ev.clear();
        assert!(ev.is_empty());
    }

    #[test]
    fn re_observe_overwrites() {
        let net = make_network();
        let mut ev = Evidence::new();
        ev.observe(&net, &var_name("Smoking"), "yes").unwrap();
        ev.observe(&net, &var_name("Smoking"), "no").unwrap();
        assert_eq!(ev.len(), 1);
        assert_eq!(
            ev.get(&var_name("Smoking")),
            Some(super::super::types::StateIndex::new(1))
        );
    }

    // === Continuous evidence tests (Part 2) ===

    #[test]
    fn observe_continuous_valid() {
        let net = make_mixed_network();
        let mut ev = Evidence::new();
        ev.observe_continuous(&net, &var_name("Temperature"), 22.5)
            .unwrap();
        assert!(ev.is_observed(&var_name("Temperature")));
        assert_eq!(
            ev.get_observation(&var_name("Temperature")),
            Some(&ObservationType::Continuous(22.5))
        );
    }

    #[test]
    fn observe_continuous_nan_rejected() {
        let net = make_mixed_network();
        let mut ev = Evidence::new();
        assert!(
            ev.observe_continuous(&net, &var_name("Temperature"), f64::NAN)
                .is_err()
        );
    }

    #[test]
    fn observe_continuous_inf_rejected() {
        let net = make_mixed_network();
        let mut ev = Evidence::new();
        assert!(
            ev.observe_continuous(&net, &var_name("Temperature"), f64::INFINITY)
                .is_err()
        );
    }

    #[test]
    fn observe_continuous_on_discrete_rejected() {
        let net = make_mixed_network();
        let mut ev = Evidence::new();
        let result = ev.observe_continuous(&net, &var_name("Smoking"), 1.0);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("discrete"));
    }

    #[test]
    fn observe_discrete_on_gaussian_rejected() {
        let net = make_mixed_network();
        let mut ev = Evidence::new();
        let result = ev.observe(&net, &var_name("Temperature"), "yes");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("continuous"));
    }

    #[test]
    fn mixed_evidence_discrete_and_continuous() {
        let net = make_mixed_network();
        let mut ev = Evidence::new();
        ev.observe(&net, &var_name("Smoking"), "yes").unwrap();
        ev.observe_continuous(&net, &var_name("Temperature"), 22.5)
            .unwrap();
        assert_eq!(ev.len(), 2);
        // Backward-compatible observations() only returns discrete
        assert_eq!(ev.observations().len(), 1);
        // all_observations() returns both
        assert_eq!(ev.all_observations().len(), 2);
    }

    #[test]
    fn get_returns_none_for_continuous() {
        let net = make_mixed_network();
        let mut ev = Evidence::new();
        ev.observe_continuous(&net, &var_name("Temperature"), 22.5)
            .unwrap();
        // get() returns None for continuous observations (backward compat)
        assert_eq!(ev.get(&var_name("Temperature")), None);
        // get_observation() works for both
        assert!(ev.get_observation(&var_name("Temperature")).is_some());
    }
}
