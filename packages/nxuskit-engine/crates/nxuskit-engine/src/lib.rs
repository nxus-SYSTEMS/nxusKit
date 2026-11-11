//! # nxusKit - Unified Rust Interface for Multiple LLM Providers
//!
//! A type-safe, async Rust library providing a unified interface for interacting with
//! multiple Large Language Model (LLM) providers.
//!
//! ## Features
//!
//! - **Provider Agnostic**: Single interface for Claude, OpenAI, and Ollama
//! - **Type Safe**: Leverages Rust's type system to catch errors at compile time
//! - **Async First**: Built on Tokio for high-performance async operations
//! - **Streaming Support**: Real-time response streaming across all providers
//!
//! ## Quick Start
//!
//! ```no_run
//! use nxuskit_engine::prelude::*;
//! use std::env;
//!
//! #[tokio::main]
//! async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
//!     let api_key = env::var("ANTHROPIC_API_KEY")
//!         .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
//!     let provider = ClaudeProvider::builder()
//!         .api_key(api_key)
//!         .model("claude-sonnet-4-5")
//!         .build()
//!         .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
//!
//!     let request = ChatRequest::new("claude-sonnet-4-5")
//!         .with_message(Message::user("What is Rust?"));
//!
//!     let response = provider.chat(&request).await
//!         .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
//!     println!("{}", response.content);
//!
//!     Ok(())
//! }
//! ```

/// Internal action-gate evaluation primitives for private RC evidence.
///
/// This module is intentionally not re-exported from the public wrapper crate or
/// prelude. It provides reusable internal request, decision, evidence, receipt,
/// and review-handoff types for private release-candidate validation.
#[doc(hidden)]
pub mod action_gate;
pub mod capabilities;
pub mod capability;
pub mod capability_cache;
pub mod clips_session_manager;
pub mod convenience;
/// Spec 119 Sprint 1D: typed, endpoint/auth-only Rust discovery surface.
/// Independent of `ProviderFacade`/`LLMProvider`/`ProviderFactory` lifecycle.
pub mod discovery;
pub mod error;
pub mod parameter_adapter;
pub mod pipeline;
pub mod pro;
pub mod provider;
pub mod providers;
/// Feature 106: internal contract-native pipeline runner seam.
///
/// NOT re-exported from the public `nxuskit` wrapper crate. The trait
/// surface accepts only public v1.1 DTOs and excludes engine-internal
/// routing/control types and topology.
pub mod runner;
pub mod token_estimator;
pub mod types;
pub mod utils;

// Re-export commonly used types
pub use capability::{CapabilityDetector, ModelCapabilities, VisionMode};
pub use convenience::{completion, completion_stream};
pub use error::{NxuskitError, Result};
pub use parameter_adapter::{AdaptedRequest, ParameterAdapter};
pub use provider::{LLMProvider, ModelLister};
pub use providers::{
    ClaudeProvider, FireworksProvider, GroqProvider, LmStudioProvider, LoopbackProvider,
    MistralProvider, MockProvider, OllamaProvider, OpenAIProvider, OpenRouterProvider,
    PerplexityProvider, TogetherProvider, XaiProvider,
};
pub use token_estimator::{EstimationMethod, TokenEstimator};
pub use types::{
    ChatRequest, ChatResponse, ClipsOptions, FinishReason, InferenceMetadata, InferenceStep,
    Message, ModelInfo, ParameterWarning, ProviderCapabilities, ProviderOptions, ResponseFormat,
    Role, StreamChunk, ThinkingMode, TokenCount, TokenUsage, WarningSeverity,
};

// Spec 119 Sprint 1A: canonical Rust provider contract surface. Additive and
// independent of the ChatRequest/ChatResponse re-exports above.
pub use error::{
    ProviderContractResult, ProviderError, ProviderErrorCode, ProviderFailureInput,
    ProviderTransportFailureKind,
};
pub use provider::{ProviderContractMapper, ProviderFacade, ProviderStream};
pub use types::{
    ProviderConfig, ProviderDescriptor, ProviderFinishReason, ProviderMessage, ProviderMessageRole,
    ProviderMetadataValue, ProviderReasoningEffort, ProviderRequest, ProviderResponse,
    ProviderResponseFormat, ProviderThinkingControl, ProviderToolCall, ProviderToolDefinition,
    ProviderTransportMode, ProviderUsage, StreamEvent,
};

pub use providers::{BayesianProvider, ClipsProvider, McpProvider};

#[cfg(any(feature = "provider-local-llama", feature = "provider-local-mistralrs"))]
pub use providers::LocalRuntimeProvider;


#[cfg(feature = "blocking-api")]
pub mod blocking;

#[cfg(test)]
#[path = "../tests/support/provider_discovery_tests.rs"]
mod spec119_discovery_tests;

/// Prelude module containing commonly used types
pub mod prelude {
    pub use crate::capability::{CapabilityDetector, ModelCapabilities, VisionMode};
    pub use crate::error::{NxuskitError, Result};
    pub use crate::parameter_adapter::{AdaptedRequest, ParameterAdapter};
    pub use crate::provider::{LLMProvider, ModelLister, ProviderFacade};
    pub use crate::providers::{
        ClaudeProvider, FireworksProvider, GroqProvider, LmStudioProvider, LoopbackProvider,
        MistralProvider, MockProvider, OllamaProvider, OpenAIProvider, OpenRouterProvider,
        PerplexityProvider, TogetherProvider, XaiProvider,
    };
    pub use crate::token_estimator::{EstimationMethod, TokenEstimator};
    pub use crate::types::{
        ChatRequest, ChatResponse, ClipsOptions, FinishReason, InferenceMetadata, InferenceStep,
        Message, ModelInfo, ParameterWarning, ProviderCapabilities, ProviderOptions,
        ResponseFormat, Role, StreamChunk, ThinkingMode, TokenCount, TokenUsage, WarningSeverity,
    };

    pub use crate::providers::{
        ClipsProvider, McpContent, McpProvider, McpResourceInfo, McpToolInfo, McpToolResult,
    };
}
