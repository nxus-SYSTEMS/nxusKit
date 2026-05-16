//! Bayesian Network Inference Provider
//!
//! This module provides a pure-Rust Bayesian Network inference engine integrated
//! into nxusKit, following the CLIPS integration pattern. It supports:
//!
//! - Discrete Bayesian Network construction and validation
//! - Exact inference via Variable Elimination and Junction Tree (Shafer-Shenoy)
//! - Approximate inference via Gibbs sampling
//! - BIF file format parsing
//! - Evidence management with observe/retract semantics
//!
//! # Integration Layers
//!
//! The BN engine is exposed through three layers:
//! 1. **C ABI** (`nxuskit_bn_*`) — opaque handles for cross-language use
//! 2. **`BayesianProvider`** — `LLMProvider` trait implementation (evidence→messages, posteriors→response)
//! 3. **nxuskit** — safe Rust RAII wrappers (`BnNetwork`, `BnEvidence`, `BnResult`)

pub mod bif;
pub mod config;
pub mod error;
pub mod evidence;
pub mod factor;
pub mod inference;
pub mod learning;
pub mod network;
pub mod provider;
pub mod stream;
pub mod types;

pub use config::{BnConfig, BnDiagnosticsOutput, BnInput, BnObservation, BnOptions, BnOutput};
pub use error::{BayesError, BayesResult};
pub use evidence::Evidence;
pub use factor::Factor;
pub use inference::{
    ContinuousMarginal, EliminationHeuristic, ForwardSampler, GaussianFactor, GibbsSampler,
    InferenceDiagnostics, InferenceEngine, InferenceResult, JunctionTree, LBPConfig,
    LikelihoodWeightedSampler, LoopyBeliefPropagation, Marginal, MomentMatchingInference,
    NUTSConfig, NUTSDiagnostics, NutsSampler, RejectionSampler, Sample, SamplingInference,
    VariableElimination, samples_to_marginals,
};
pub use learning::bayesian::{BayesianConfig, BayesianLearner, DirichletPrior};
pub use learning::hill_climb::{HillClimbConfig, HillClimbLearner};
pub use learning::k2::{K2Config, K2Learner};
pub use learning::mle::{MissingStrategy, MleConfig, MleLearner};
pub use learning::scoring::ScoringFunction;
pub use learning::{Dataset, ParameterLearner, StructureLearner, StructureSearchResult};
pub use network::BayesianNetwork;
pub use provider::{BayesianProvider, BayesianProviderBuilder};
pub use stream::{BayesBlockingIter, BayesStream, BayesStreamChunk};
pub use types::{
    CPTType, DiscreteVariable, GaussianVariable, ObservationType, StateIndex, StateName,
    VariableName,
};
