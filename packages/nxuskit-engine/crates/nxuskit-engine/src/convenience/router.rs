//! Provider routing for convenience API
//!
//! Routes model specifications to appropriate provider instances based on
//! model name patterns and environment configuration.

use super::env_detector::EnvConfig;
use super::parser::{ModelSpecifier, ProviderName};
use crate::error::{NxuskitError, Result};
use crate::provider::LLMProvider;
use crate::providers::{ClaudeProvider, OllamaProvider, OpenAIProvider};

/// Routes convenience API calls to appropriate providers
#[derive(Debug)]
pub struct ProviderRouter;

impl ProviderRouter {
    /// Create a new provider router
    pub fn new() -> Self {
        Self
    }

    /// Route a model specifier to a provider instance
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nxuskit_engine::convenience::router::ProviderRouter;
    /// # use nxuskit_engine::convenience::parser::ModelSpecifier;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let router = ProviderRouter::new();
    /// let spec = ModelSpecifier::parse("gpt-4o")?;
    /// let provider = router.route(&spec).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn route(&self, spec: &ModelSpecifier) -> Result<Box<dyn LLMProvider>> {
        // Determine provider (explicit or inferred)
        let provider_name = if let Some(explicit) = spec.provider {
            explicit
        } else {
            self.infer_provider(&spec.model)?
        };

        // Detect environment configuration
        let config = EnvConfig::detect(provider_name);

        // Validate configuration
        if !config.is_valid()
            && let Some(err_msg) = config.missing_credential_error()
        {
            return Err(NxuskitError::MissingCredentials(
                provider_name.to_string(),
                err_msg,
            ));
        }

        // Build provider instance
        self.build_provider(provider_name, &spec.model, &config)
    }

    /// Infer provider from model name patterns
    fn infer_provider(&self, model: &str) -> Result<ProviderName> {
        // OpenAI model patterns
        if model.starts_with("gpt-")
            || model.starts_with("o1-")
            || model.starts_with("o3-")
            || model.starts_with("text-")
            || model.starts_with("davinci")
            || model.starts_with("curie")
            || model.starts_with("babbage")
            || model.starts_with("ada")
        {
            return Ok(ProviderName::OpenAI);
        }

        // Anthropic model patterns
        if model.starts_with("claude-") {
            return Ok(ProviderName::Anthropic);
        }

        // If no pattern matches, check which providers have credentials set
        // This allows using Ollama for any unrecognized model names
        let openai_config = EnvConfig::detect(ProviderName::OpenAI);
        let anthropic_config = EnvConfig::detect(ProviderName::Anthropic);

        // Count how many providers are configured
        let mut configured_providers = Vec::new();
        if openai_config.is_valid() {
            configured_providers.push(ProviderName::OpenAI);
        }
        if anthropic_config.is_valid() {
            configured_providers.push(ProviderName::Anthropic);
        }
        // Ollama is always "valid" as it doesn't require credentials
        configured_providers.push(ProviderName::Ollama);

        // If only Ollama is available, use it
        if configured_providers.len() == 1 && configured_providers[0] == ProviderName::Ollama {
            return Ok(ProviderName::Ollama);
        }

        // Otherwise, we can't infer - too ambiguous
        Err(NxuskitError::ProviderDetectionFailed(
            model.to_string(),
            ProviderName::all().join(", "),
        ))
    }

    /// Build provider instance from name and configuration
    fn build_provider(
        &self,
        provider: ProviderName,
        model: &str,
        config: &EnvConfig,
    ) -> Result<Box<dyn LLMProvider>> {
        match provider {
            ProviderName::OpenAI => {
                let api_key = config.api_key.as_ref().ok_or_else(|| {
                    NxuskitError::MissingCredentials(
                        "OpenAI".to_string(),
                        "OPENAI_API_KEY".to_string(),
                    )
                })?;

                let mut builder = OpenAIProvider::builder()
                    .api_key(api_key.clone())
                    .model(model);

                if let Some(base_url) = &config.base_url {
                    builder = builder.base_url(base_url.clone());
                }

                let provider = builder
                    .build()
                    .map_err(|e| NxuskitError::Configuration(e.to_string()))?;

                Ok(Box::new(provider))
            }

            ProviderName::Anthropic => {
                let api_key = config.api_key.as_ref().ok_or_else(|| {
                    NxuskitError::MissingCredentials(
                        "Anthropic".to_string(),
                        "ANTHROPIC_API_KEY".to_string(),
                    )
                })?;

                let mut builder = ClaudeProvider::builder()
                    .api_key(api_key.clone())
                    .model(model);

                if let Some(base_url) = &config.base_url {
                    builder = builder.base_url(base_url.clone());
                }

                let provider = builder
                    .build()
                    .map_err(|e| NxuskitError::Configuration(e.to_string()))?;

                Ok(Box::new(provider))
            }

            ProviderName::Ollama => {
                let base_url = config.effective_base_url();

                // Ollama doesn't require API keys
                let provider = OllamaProvider::builder()
                    .model(model)
                    .base_url(base_url)
                    .build()
                    .map_err(|e| NxuskitError::Configuration(e.to_string()))?;

                Ok(Box::new(provider))
            }
        }
    }
}

impl Default for ProviderRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convenience::env_detector::tests::ENV_MUTEX;

    #[test]
    fn test_infer_openai_models() {
        let router = ProviderRouter::new();

        assert_eq!(
            router.infer_provider("gpt-4o").unwrap(),
            ProviderName::OpenAI
        );
        assert_eq!(
            router.infer_provider("gpt-3.5-turbo").unwrap(),
            ProviderName::OpenAI
        );
        assert_eq!(
            router.infer_provider("gpt-4-turbo").unwrap(),
            ProviderName::OpenAI
        );
        assert_eq!(
            router.infer_provider("o1-preview").unwrap(),
            ProviderName::OpenAI
        );
        assert_eq!(
            router.infer_provider("text-davinci-003").unwrap(),
            ProviderName::OpenAI
        );
    }

    #[test]
    fn test_infer_anthropic_models() {
        let router = ProviderRouter::new();

        assert_eq!(
            router.infer_provider("claude-3-opus").unwrap(),
            ProviderName::Anthropic
        );
        assert_eq!(
            router.infer_provider("claude-3-sonnet").unwrap(),
            ProviderName::Anthropic
        );
        assert_eq!(
            router.infer_provider("claude-sonnet-4-5").unwrap(),
            ProviderName::Anthropic
        );
        assert_eq!(
            router.infer_provider("claude-2.1").unwrap(),
            ProviderName::Anthropic
        );
    }

    #[test]
    fn test_infer_unknown_model_with_only_ollama() {
        // Use shared mutex to prevent race conditions with other env var tests
        let _guard = ENV_MUTEX.lock().unwrap();

        // Save original values to restore later
        let orig_openai = std::env::var("OPENAI_API_KEY").ok();
        let orig_anthropic = std::env::var("ANTHROPIC_API_KEY").ok();

        // Clear API keys so only Ollama is available
        unsafe {
            std::env::remove_var("OPENAI_API_KEY");
            std::env::remove_var("ANTHROPIC_API_KEY");
        }

        let router = ProviderRouter::new();

        // Unknown model should default to Ollama when it's the only option
        let result = router.infer_provider("llama2");
        assert_eq!(result.unwrap(), ProviderName::Ollama);

        let result = router.infer_provider("mistral");
        assert_eq!(result.unwrap(), ProviderName::Ollama);

        // Restore original values
        unsafe {
            if let Some(key) = orig_openai {
                std::env::set_var("OPENAI_API_KEY", key);
            }
            if let Some(key) = orig_anthropic {
                std::env::set_var("ANTHROPIC_API_KEY", key);
            }
        }
    }

    #[test]
    fn test_infer_ambiguous_fails() {
        // Use shared mutex to prevent race conditions with other env var tests
        let _guard = ENV_MUTEX.lock().unwrap();

        // Save original values
        let orig_openai = std::env::var("OPENAI_API_KEY").ok();
        let orig_anthropic = std::env::var("ANTHROPIC_API_KEY").ok();

        // Set multiple provider credentials to create ambiguity
        unsafe {
            std::env::set_var("OPENAI_API_KEY", "test-key");
            std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        }

        let router = ProviderRouter::new();

        // Unknown model with multiple providers configured should fail
        let result = router.infer_provider("unknown-model");
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(NxuskitError::ProviderDetectionFailed(_, _))
        ));

        // Restore original values (or remove if they weren't set)
        unsafe {
            if let Some(key) = orig_openai {
                std::env::set_var("OPENAI_API_KEY", key);
            } else {
                std::env::remove_var("OPENAI_API_KEY");
            }
            if let Some(key) = orig_anthropic {
                std::env::set_var("ANTHROPIC_API_KEY", key);
            } else {
                std::env::remove_var("ANTHROPIC_API_KEY");
            }
        }
    }

    #[test]
    fn test_explicit_provider_overrides_inference() {
        let spec = ModelSpecifier::parse("anthropic/gpt-4o").unwrap();
        // Even though "gpt-4o" would normally be inferred as OpenAI,
        // explicit provider takes precedence
        assert_eq!(spec.provider, Some(ProviderName::Anthropic));
    }
}
