//! Error types for Bayesian Network operations.

/// Errors that can occur during Bayesian Network operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum BayesError {
    /// Invalid graph structure (cycles, missing nodes, etc.)
    #[error("Invalid graph: {0}")]
    InvalidGraph(String),
    /// Invalid CPT (wrong dimensions, doesn't sum to 1, etc.)
    #[error("Invalid CPT: {0}")]
    InvalidCpt(String),
    /// Invalid evidence (unknown variable, unknown state, etc.)
    #[error("Invalid evidence: {0}")]
    InvalidEvidence(String),
    /// Network is incomplete (missing CPTs, unconnected nodes, etc.)
    #[error("Incomplete network: {0}")]
    IncompleteNetwork(String),
    /// Evidence is impossible (zero probability under model)
    #[error("Zero probability evidence: {0}")]
    ZeroProbabilityEvidence(String),
    /// Error parsing a BIF file or other input format
    #[error("Parse error: {0}")]
    ParseError(String),
    /// Missing column in dataset
    #[error("Missing column: {0}")]
    MissingColumn(String),
    /// Empty dataset provided
    #[error("Empty dataset: {0}")]
    EmptyDataset(String),
    /// Error during inference computation
    #[error("Inference error: {0}")]
    InferenceError(String),
    /// Invalid Gaussian variable parameters (variance <= 0, NaN/Inf)
    #[error("Invalid Gaussian parameters: {0}")]
    InvalidGaussianParameters(String),
    /// CLG constraint violation (continuous variable as parent of discrete)
    #[error("CLG violation: {0}")]
    CLGViolation(String),
    /// Continuous evidence set on a discrete variable
    #[error("Continuous evidence on discrete variable: {0}")]
    ContinuousEvidenceOnDiscrete(String),
    /// Discrete evidence set on a continuous variable
    #[error("Discrete evidence on continuous variable: {0}")]
    DiscreteEvidenceOnContinuous(String),
}

/// Convenience type alias for BN operations.
pub type BayesResult<T> = Result<T, BayesError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_contains_context() {
        let err = BayesError::InvalidGraph("cycle detected".into());
        let msg = err.to_string();
        assert!(msg.contains("Invalid graph"), "got: {}", msg);
        assert!(msg.contains("cycle detected"), "got: {}", msg);
    }

    #[test]
    fn all_variants_display() {
        let cases: Vec<BayesError> = vec![
            BayesError::InvalidGraph("g".into()),
            BayesError::InvalidCpt("c".into()),
            BayesError::InvalidEvidence("e".into()),
            BayesError::IncompleteNetwork("n".into()),
            BayesError::ZeroProbabilityEvidence("z".into()),
            BayesError::ParseError("p".into()),
            BayesError::MissingColumn("m".into()),
            BayesError::EmptyDataset("d".into()),
            BayesError::InferenceError("i".into()),
            BayesError::InvalidGaussianParameters("gp".into()),
            BayesError::CLGViolation("clg".into()),
            BayesError::ContinuousEvidenceOnDiscrete("ced".into()),
            BayesError::DiscreteEvidenceOnContinuous("dec".into()),
        ];
        for err in cases {
            let s = err.to_string();
            assert!(!s.is_empty(), "Display should not be empty for {:?}", err);
        }
    }

    #[test]
    fn error_is_std_error() {
        let err: Box<dyn std::error::Error> = Box::new(BayesError::InvalidGraph("test".into()));
        assert!(err.to_string().contains("test"));
    }
}
