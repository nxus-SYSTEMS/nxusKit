//! Typed provider builders for ergonomic provider construction.
//!
//! Each builder constructs a [`ProviderConfig`] and delegates to
//! [`NxuskitProvider::new`].  Required fields (like `api_key` for cloud
//! providers) are validated at build time, returning
//! [`NxuskitError::Configuration`] on missing values.
//!
//! # Examples
//!
//! ```no_run
//! use nxuskit::builders::*;
//!
//! // Cloud provider
//! let provider = ClaudeProvider::builder()
//!     .api_key("sk-ant-...")
//!     .model("claude-sonnet-4-6")
//!     .build()?;
//!
//! // Local provider
//! let provider = OllamaProvider::builder()
//!     .base_url("http://localhost:11434")
//!     .model("llama3")
//!     .build()?;
//!
//! // Testing
//! let provider = LoopbackProvider::builder()
//!     .model("echo")
//!     .build()?;
//! # Ok::<(), nxuskit::NxuskitError>(())
//! ```

use crate::error::NxuskitError;
use crate::provider::NxuskitProvider;
use crate::types::ProviderConfig;

// ---------------------------------------------------------------------------
// Macro: cloud provider builder (requires api_key)
// ---------------------------------------------------------------------------

macro_rules! cloud_provider_builder {
    (
        $(#[$meta:meta])*
        $provider:ident, $builder:ident, $provider_type:expr
    ) => {
        $(#[$meta])*
        pub struct $provider;

        impl $provider {
            /// Create a new builder for this provider.
            pub fn builder() -> $builder {
                $builder::default()
            }
        }

        /// Builder for
        #[doc = concat!("`", stringify!($provider), "`.")]
        #[derive(Default)]
        pub struct $builder {
            api_key: Option<String>,
            model: Option<String>,
            base_url: Option<String>,
            timeout_ms: Option<u64>,
            license_key: Option<String>,
        }

        impl $builder {
            /// Set the API key (required).
            pub fn api_key(mut self, key: impl Into<String>) -> Self {
                self.api_key = Some(key.into());
                self
            }

            /// Set the model identifier.
            pub fn model(mut self, model: impl Into<String>) -> Self {
                self.model = Some(model.into());
                self
            }

            /// Set a custom base URL.
            pub fn base_url(mut self, url: impl Into<String>) -> Self {
                self.base_url = Some(url.into());
                self
            }

            /// Set the request timeout in milliseconds.
            pub fn timeout_ms(mut self, ms: u64) -> Self {
                self.timeout_ms = Some(ms);
                self
            }

            /// Set a license key for entitlement checking.
            pub fn license_key(mut self, key: impl Into<String>) -> Self {
                self.license_key = Some(key.into());
                self
            }

            /// Build the provider config without creating a provider.
            ///
            /// Useful for tests that need to inspect config without the SDK.
            pub fn to_config(self) -> ProviderConfig {
                ProviderConfig {
                    provider_type: $provider_type.to_string(),
                    api_key: self.api_key,
                    model: self.model,
                    base_url: self.base_url,
                    timeout_ms: self.timeout_ms,
                    license_key: self.license_key,
                }
            }

            /// Build and create a [`NxuskitProvider`].
            ///
            /// Returns [`NxuskitError::Configuration`] if the API key is missing.
            pub fn build(self) -> Result<NxuskitProvider, NxuskitError> {
                if self.api_key.is_none() {
                    return Err(NxuskitError::Configuration {
                        message: format!(
                            "api_key is required for {} provider",
                            $provider_type
                        ),
                    });
                }
                NxuskitProvider::new(self.to_config())
            }
        }
    };
}

// ---------------------------------------------------------------------------
// Macro: local provider builder (no api_key required)
// ---------------------------------------------------------------------------

macro_rules! local_provider_builder {
    (
        $(#[$meta:meta])*
        $provider:ident, $builder:ident, $provider_type:expr
    ) => {
        $(#[$meta])*
        pub struct $provider;

        impl $provider {
            /// Create a new builder for this provider.
            pub fn builder() -> $builder {
                $builder::default()
            }
        }

        /// Builder for
        #[doc = concat!("`", stringify!($provider), "`.")]
        #[derive(Default)]
        pub struct $builder {
            model: Option<String>,
            base_url: Option<String>,
            timeout_ms: Option<u64>,
            license_key: Option<String>,
        }

        impl $builder {
            /// Set the model identifier.
            pub fn model(mut self, model: impl Into<String>) -> Self {
                self.model = Some(model.into());
                self
            }

            /// Set a custom base URL.
            pub fn base_url(mut self, url: impl Into<String>) -> Self {
                self.base_url = Some(url.into());
                self
            }

            /// Set the request timeout in milliseconds.
            pub fn timeout_ms(mut self, ms: u64) -> Self {
                self.timeout_ms = Some(ms);
                self
            }

            /// Set a license key for entitlement checking.
            pub fn license_key(mut self, key: impl Into<String>) -> Self {
                self.license_key = Some(key.into());
                self
            }

            /// Build the provider config without creating a provider.
            ///
            /// Useful for tests that need to inspect config without the SDK.
            pub fn to_config(self) -> ProviderConfig {
                ProviderConfig {
                    provider_type: $provider_type.to_string(),
                    api_key: None,
                    model: self.model,
                    base_url: self.base_url,
                    timeout_ms: self.timeout_ms,
                    license_key: self.license_key,
                }
            }

            /// Build and create a [`NxuskitProvider`].
            pub fn build(self) -> Result<NxuskitProvider, NxuskitError> {
                NxuskitProvider::new(self.to_config())
            }
        }
    };
}

// ---------------------------------------------------------------------------
// Cloud providers (require api_key)
// ---------------------------------------------------------------------------

cloud_provider_builder!(
    /// Claude (Anthropic) provider.
    ClaudeProvider, ClaudeProviderBuilder, "claude"
);

cloud_provider_builder!(
    /// OpenAI provider.
    OpenAIProvider, OpenAIProviderBuilder, "openai"
);

cloud_provider_builder!(
    /// Fireworks AI provider.
    FireworksProvider, FireworksProviderBuilder, "fireworks"
);

cloud_provider_builder!(
    /// xAI Grok provider.
    XaiProvider, XaiProviderBuilder, "xai"
);

cloud_provider_builder!(
    /// Groq provider.
    GroqProvider, GroqProviderBuilder, "groq"
);

cloud_provider_builder!(
    /// Together AI provider.
    TogetherProvider, TogetherProviderBuilder, "together"
);

cloud_provider_builder!(
    /// Mistral AI provider.
    MistralProvider, MistralProviderBuilder, "mistral"
);

cloud_provider_builder!(
    /// Perplexity AI provider.
    PerplexityProvider, PerplexityProviderBuilder, "perplexity"
);

cloud_provider_builder!(
    /// OpenRouter provider.
    OpenRouterProvider, OpenRouterProviderBuilder, "openrouter"
);

// ---------------------------------------------------------------------------
// Local providers (no api_key required)
// ---------------------------------------------------------------------------

local_provider_builder!(
    /// Ollama local provider.
    OllamaProvider, OllamaProviderBuilder, "ollama"
);

local_provider_builder!(
    /// LM Studio local provider.
    LmStudioProvider, LmStudioProviderBuilder, "lmstudio"
);

local_provider_builder!(
    /// Loopback (echo) provider for testing.
    LoopbackProvider, LoopbackProviderBuilder, "loopback"
);
