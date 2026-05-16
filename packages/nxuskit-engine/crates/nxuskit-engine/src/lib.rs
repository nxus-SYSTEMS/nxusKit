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

pub mod capabilities;
pub mod capability;
pub mod capability_cache;
pub mod clips_session_manager;
pub mod convenience;
pub mod error;
pub mod parameter_adapter;
pub mod pipeline;
pub mod pro;
pub mod provider;
pub mod providers;
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

pub use providers::{BayesianProvider, ClipsProvider, McpProvider};

#[cfg(any(feature = "provider-local-llama", feature = "provider-local-mistralrs"))]
pub use providers::LocalRuntimeProvider;


#[cfg(feature = "blocking-api")]
pub mod blocking;

/// Prelude module containing commonly used types
pub mod prelude {
    pub use crate::capability::{CapabilityDetector, ModelCapabilities, VisionMode};
    pub use crate::error::{NxuskitError, Result};
    pub use crate::parameter_adapter::{AdaptedRequest, ParameterAdapter};
    pub use crate::provider::{LLMProvider, ModelLister};
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
