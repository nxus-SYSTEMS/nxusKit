//! Core newtypes and data structures for Bayesian Network inference.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Validated variable name (non-empty, alphanumeric + underscore).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VariableName(String);

impl VariableName {
    /// Create a new `VariableName`, validating the input.
    ///
    /// # Errors
    /// Returns an error if the name is empty or contains invalid characters.
    pub fn new(name: impl Into<String>) -> Result<Self, super::BayesError> {
        let name = name.into();
        if name.is_empty() {
            return Err(super::BayesError::InvalidGraph(
                "Variable name cannot be empty".into(),
            ));
        }
        if !name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        {
            return Err(super::BayesError::InvalidGraph(format!(
                "Variable name '{}' contains invalid characters (allowed: alphanumeric, underscore, hyphen)",
                name
            )));
        }
        Ok(Self(name))
    }

    /// Return the inner string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for VariableName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for VariableName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// State name within a discrete variable.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StateName(String);

impl StateName {
    /// Create a new `StateName`.
    ///
    /// # Errors
    /// Returns an error if the name is empty.
    pub fn new(name: impl Into<String>) -> Result<Self, super::BayesError> {
        let name = name.into();
        if name.is_empty() {
            return Err(super::BayesError::InvalidGraph(
                "State name cannot be empty".into(),
            ));
        }
        Ok(Self(name))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for StateName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for StateName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Zero-based index into a variable's state list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StateIndex(usize);

impl StateIndex {
    pub fn new(index: usize) -> Self {
        Self(index)
    }

    pub fn value(self) -> usize {
        self.0
    }
}

impl fmt::Display for StateIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A discrete random variable in a Bayesian Network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscreteVariable {
    /// The variable's name.
    pub name: VariableName,
    /// Ordered list of possible states.
    pub states: Vec<StateName>,
}

impl DiscreteVariable {
    /// Create a new discrete variable.
    ///
    /// # Errors
    /// Returns an error if states is empty or contains duplicates.
    pub fn new(name: VariableName, states: Vec<StateName>) -> Result<Self, super::BayesError> {
        if states.is_empty() {
            return Err(super::BayesError::InvalidGraph(format!(
                "Variable '{}' must have at least one state",
                name
            )));
        }
        // Check for duplicate states
        let mut seen = std::collections::HashSet::new();
        for state in &states {
            if !seen.insert(state.as_str()) {
                return Err(super::BayesError::InvalidGraph(format!(
                    "Variable '{}' has duplicate state '{}'",
                    name, state
                )));
            }
        }
        Ok(Self { name, states })
    }

    /// Number of possible states (cardinality).
    pub fn cardinality(&self) -> usize {
        self.states.len()
    }

    /// Look up a state's index by name.
    pub fn state_index(&self, state: &str) -> Option<StateIndex> {
        self.states
            .iter()
            .position(|s| s.as_str() == state)
            .map(StateIndex::new)
    }
}

/// A continuous random variable with linear-Gaussian conditional distribution.
///
/// The conditional distribution is:
/// `X | Parents = mean_base + sum(weight_i * Parent_i) + noise(0, variance)`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GaussianVariable {
    /// The variable's name.
    pub name: VariableName,
    /// Unconditional mean (intercept).
    pub mean_base: f64,
    /// Conditional variance (must be > 0).
    pub variance: f64,
    /// Linear coefficients for each parent: (parent_name, weight).
    pub weights: Vec<(VariableName, f64)>,
}

impl GaussianVariable {
    /// Create a new Gaussian variable with the given mean and variance.
    ///
    /// # Errors
    /// Returns an error if variance <= 0 or parameters are not finite.
    pub fn new(
        name: impl Into<String>,
        mean_base: f64,
        variance: f64,
    ) -> Result<Self, super::BayesError> {
        let name = VariableName::new(name)?;
        if !mean_base.is_finite() {
            return Err(super::BayesError::InvalidGaussianParameters(format!(
                "Variable '{}': mean_base must be finite, got {}",
                name, mean_base
            )));
        }
        if !variance.is_finite() || variance <= 0.0 {
            return Err(super::BayesError::InvalidGaussianParameters(format!(
                "Variable '{}': variance must be finite and > 0, got {}",
                name, variance
            )));
        }
        Ok(Self {
            name,
            mean_base,
            variance,
            weights: Vec::new(),
        })
    }

    /// Add a linear weight for a parent variable.
    ///
    /// # Errors
    /// Returns an error if the weight is not finite.
    pub fn with_weight(
        mut self,
        parent: impl Into<String>,
        weight: f64,
    ) -> Result<Self, super::BayesError> {
        let parent_name = VariableName::new(parent)?;
        if !weight.is_finite() {
            return Err(super::BayesError::InvalidGaussianParameters(format!(
                "Variable '{}': weight for parent '{}' must be finite, got {}",
                self.name, parent_name, weight
            )));
        }
        self.weights.push((parent_name, weight));
        Ok(self)
    }
}

/// Observation type for evidence — either discrete (state index) or continuous (f64 value).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ObservationType {
    /// Observed discrete state.
    Discrete(StateIndex),
    /// Observed continuous value.
    Continuous(f64),
}

/// CPT type — either a discrete factor or a Gaussian conditional distribution.
#[derive(Debug, Clone)]
pub enum CPTType {
    /// Log-space CPT for discrete variables (existing).
    Discrete(super::factor::Factor),
    /// Linear-Gaussian conditional for continuous variables.
    Gaussian(GaussianVariable),
}

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn variable_name_valid() {
        assert!(VariableName::new("Smoking").is_ok());
        assert!(VariableName::new("X_ray").is_ok());
        assert!(VariableName::new("node-1").is_ok());
    }

    #[test]
    fn variable_name_empty_rejected() {
        assert!(VariableName::new("").is_err());
    }

    #[test]
    fn variable_name_invalid_chars_rejected() {
        assert!(VariableName::new("has space").is_err());
        assert!(VariableName::new("has.dot").is_err());
    }

    #[test]
    fn variable_name_display() {
        let name = VariableName::new("Smoking").unwrap();
        assert_eq!(name.to_string(), "Smoking");
    }

    #[test]
    fn variable_name_equality_and_hashing() {
        let a = VariableName::new("X").unwrap();
        let b = VariableName::new("X").unwrap();
        let c = VariableName::new("Y").unwrap();
        assert_eq!(a, b);
        assert_ne!(a, c);

        let mut set = std::collections::HashSet::new();
        set.insert(a);
        assert!(set.contains(&b));
    }

    #[test]
    fn state_name_valid() {
        assert!(StateName::new("yes").is_ok());
        assert!(StateName::new("positive").is_ok());
    }

    #[test]
    fn state_name_empty_rejected() {
        assert!(StateName::new("").is_err());
    }

    #[test]
    fn state_index_display() {
        let idx = StateIndex::new(3);
        assert_eq!(idx.to_string(), "3");
        assert_eq!(idx.value(), 3);
    }

    #[test]
    fn discrete_variable_creation() {
        let var = DiscreteVariable::new(
            VariableName::new("Smoking").unwrap(),
            vec![
                StateName::new("yes").unwrap(),
                StateName::new("no").unwrap(),
            ],
        )
        .unwrap();
        assert_eq!(var.cardinality(), 2);
        assert_eq!(var.state_index("yes"), Some(StateIndex::new(0)));
        assert_eq!(var.state_index("no"), Some(StateIndex::new(1)));
        assert_eq!(var.state_index("maybe"), None);
    }

    #[test]
    fn discrete_variable_empty_states_rejected() {
        assert!(DiscreteVariable::new(VariableName::new("X").unwrap(), vec![]).is_err());
    }

    #[test]
    fn discrete_variable_duplicate_states_rejected() {
        assert!(
            DiscreteVariable::new(
                VariableName::new("X").unwrap(),
                vec![StateName::new("a").unwrap(), StateName::new("a").unwrap()],
            )
            .is_err()
        );
    }

    // === GaussianVariable tests (Part 2) ===

    #[test]
    fn gaussian_variable_valid() {
        let gv = GaussianVariable::new("Temperature", 20.0, 5.0).unwrap();
        assert_eq!(gv.name.as_str(), "Temperature");
        assert_eq!(gv.mean_base, 20.0);
        assert_eq!(gv.variance, 5.0);
        assert!(gv.weights.is_empty());
    }

    #[test]
    fn gaussian_variable_with_weights() {
        let gv = GaussianVariable::new("Sensor", 0.0, 1.0)
            .unwrap()
            .with_weight("Temperature", 1.0)
            .unwrap()
            .with_weight("Humidity", 0.5)
            .unwrap();
        assert_eq!(gv.weights.len(), 2);
        assert_eq!(gv.weights[0].0.as_str(), "Temperature");
        assert_eq!(gv.weights[0].1, 1.0);
        assert_eq!(gv.weights[1].0.as_str(), "Humidity");
        assert_eq!(gv.weights[1].1, 0.5);
    }

    #[test]
    fn gaussian_variable_zero_variance_rejected() {
        assert!(GaussianVariable::new("X", 0.0, 0.0).is_err());
    }

    #[test]
    fn gaussian_variable_negative_variance_rejected() {
        assert!(GaussianVariable::new("X", 0.0, -1.0).is_err());
    }

    #[test]
    fn gaussian_variable_nan_mean_rejected() {
        assert!(GaussianVariable::new("X", f64::NAN, 1.0).is_err());
    }

    #[test]
    fn gaussian_variable_inf_variance_rejected() {
        assert!(GaussianVariable::new("X", 0.0, f64::INFINITY).is_err());
    }

    #[test]
    fn gaussian_variable_nan_weight_rejected() {
        let result = GaussianVariable::new("X", 0.0, 1.0)
            .unwrap()
            .with_weight("Parent", f64::NAN);
        assert!(result.is_err());
    }

    #[test]
    fn gaussian_variable_inf_weight_rejected() {
        let result = GaussianVariable::new("X", 0.0, 1.0)
            .unwrap()
            .with_weight("Parent", f64::INFINITY);
        assert!(result.is_err());
    }

    // === ObservationType tests ===

    #[test]
    fn observation_type_discrete() {
        let obs = ObservationType::Discrete(StateIndex::new(1));
        if let ObservationType::Discrete(idx) = obs {
            assert_eq!(idx.value(), 1);
        } else {
            panic!("Expected Discrete");
        }
    }

    #[test]
    fn observation_type_continuous() {
        let obs = ObservationType::Continuous(22.5);
        if let ObservationType::Continuous(val) = obs {
            assert!((val - 22.5).abs() < 1e-10);
        } else {
            panic!("Expected Continuous");
        }
    }

    #[test]
    fn observation_type_equality() {
        let a = ObservationType::Discrete(StateIndex::new(0));
        let b = ObservationType::Discrete(StateIndex::new(0));
        let c = ObservationType::Continuous(1.0);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
