//! Model specifier parsing for convenience API
//!
//! Handles parsing of model identifiers in both simple ("gpt-4o") and
//! provider/model ("openai/gpt-4o") formats.

use crate::error::{NxuskitError, Result};
use std::fmt;
use std::str::FromStr;

/// Supported LLM provider names for convenience API
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProviderName {
    /// Anthropic (Claude)
    Anthropic,
    /// OpenAI (GPT models)
    OpenAI,
    /// Ollama (local models)
    Ollama,
}

impl ProviderName {
    /// Parse provider name from string (case-insensitive)
    pub fn from_str_ignore_case(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "anthropic" => Ok(ProviderName::Anthropic),
            "openai" => Ok(ProviderName::OpenAI),
            "ollama" => Ok(ProviderName::Ollama),
            _ => Err(NxuskitError::ProviderDetectionFailed(
                s.to_string(),
                Self::supported_providers(),
            )),
        }
    }

    /// Get string representation of provider name
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderName::Anthropic => "anthropic",
            ProviderName::OpenAI => "openai",
            ProviderName::Ollama => "ollama",
        }
    }

    /// Get list of all supported provider names
    pub fn all() -> &'static [&'static str] {
        &["anthropic", "openai", "ollama"]
    }

    /// Get comma-separated list of supported providers
    fn supported_providers() -> String {
        Self::all().join(", ")
    }
}

impl fmt::Display for ProviderName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for ProviderName {
    type Err = NxuskitError;

    fn from_str(s: &str) -> Result<Self> {
        Self::from_str_ignore_case(s)
    }
}

/// Parsed model identifier
///
/// Represents a model name that may optionally include an explicit provider.
/// Supports two formats:
/// - Simple: `"gpt-4o"` (provider inferred)
/// - Explicit: `"openai/gpt-4o"` (provider specified)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelSpecifier {
    /// Explicit provider name (if specified as "provider/model")
    pub provider: Option<ProviderName>,
    /// Model name (e.g., "gpt-4o", "claude-sonnet-4-5")
    pub model: String,
}

impl ModelSpecifier {
    /// Parse model specifier from string
    ///
    /// # Examples
    ///
    /// ```
    /// # use nxuskit_engine::convenience::parser::ModelSpecifier;
    /// // Simple format (provider inferred)
    /// let spec = ModelSpecifier::parse("gpt-4o").unwrap();
    /// assert_eq!(spec.model, "gpt-4o");
    /// assert!(spec.provider.is_none());
    ///
    /// // Explicit format (provider specified)
    /// let spec = ModelSpecifier::parse("openai/gpt-4o").unwrap();
    /// assert_eq!(spec.model, "gpt-4o");
    /// assert!(spec.provider.is_some());
    /// ```
    pub fn parse(spec: &str) -> Result<Self> {
        // Check for empty string
        if spec.trim().is_empty() {
            return Err(NxuskitError::InvalidModelSpecifier(spec.to_string()));
        }

        // Check if contains separator
        if spec.contains('/') {
            Self::parse_with_provider(spec)
        } else {
            Ok(Self {
                provider: None,
                model: spec.to_string(),
            })
        }
    }

    /// Parse format with explicit provider: "provider/model"
    fn parse_with_provider(spec: &str) -> Result<Self> {
        let parts: Vec<&str> = spec.split('/').collect();

        // Must have exactly 2 parts
        if parts.len() != 2 {
            return Err(NxuskitError::InvalidModelSpecifier(spec.to_string()));
        }

        let provider_str = parts[0].trim();
        let model_str = parts[1].trim();

        // Both parts must be non-empty
        if provider_str.is_empty() || model_str.is_empty() {
            return Err(NxuskitError::InvalidModelSpecifier(spec.to_string()));
        }

        // Parse provider name
        let provider = ProviderName::from_str_ignore_case(provider_str)?;

        Ok(Self {
            provider: Some(provider),
            model: model_str.to_string(),
        })
    }

    /// Check if provider was explicitly specified
    pub fn has_explicit_provider(&self) -> bool {
        self.provider.is_some()
    }
}

impl FromStr for ModelSpecifier {
    type Err = NxuskitError;

    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_name_parsing() {
        assert_eq!(
            ProviderName::from_str_ignore_case("anthropic").unwrap(),
            ProviderName::Anthropic
        );
        assert_eq!(
            ProviderName::from_str_ignore_case("ANTHROPIC").unwrap(),
            ProviderName::Anthropic
        );
        assert_eq!(
            ProviderName::from_str_ignore_case("openai").unwrap(),
            ProviderName::OpenAI
        );
        assert_eq!(
            ProviderName::from_str_ignore_case("ollama").unwrap(),
            ProviderName::Ollama
        );
        assert!(ProviderName::from_str_ignore_case("invalid").is_err());
    }

    #[test]
    fn test_provider_name_as_str() {
        assert_eq!(ProviderName::Anthropic.as_str(), "anthropic");
        assert_eq!(ProviderName::OpenAI.as_str(), "openai");
        assert_eq!(ProviderName::Ollama.as_str(), "ollama");
    }

    #[test]
    fn test_simple_model_specifier() {
        let spec = ModelSpecifier::parse("gpt-4o").unwrap();
        assert_eq!(spec.model, "gpt-4o");
        assert_eq!(spec.provider, None);
        assert!(!spec.has_explicit_provider());
    }

    #[test]
    fn test_explicit_model_specifier() {
        let spec = ModelSpecifier::parse("openai/gpt-4o").unwrap();
        assert_eq!(spec.model, "gpt-4o");
        assert_eq!(spec.provider, Some(ProviderName::OpenAI));
        assert!(spec.has_explicit_provider());

        let spec = ModelSpecifier::parse("anthropic/claude-sonnet-4-5").unwrap();
        assert_eq!(spec.model, "claude-sonnet-4-5");
        assert_eq!(spec.provider, Some(ProviderName::Anthropic));
    }

    #[test]
    fn test_invalid_specifiers() {
        // Empty string
        assert!(ModelSpecifier::parse("").is_err());
        assert!(ModelSpecifier::parse("   ").is_err());

        // Missing parts
        assert!(ModelSpecifier::parse("/model").is_err());
        assert!(ModelSpecifier::parse("provider/").is_err());
        assert!(ModelSpecifier::parse("/").is_err());

        // Multiple separators
        assert!(ModelSpecifier::parse("openai/gpt/4o").is_err());

        // Unknown provider
        assert!(ModelSpecifier::parse("invalid/model").is_err());
    }

    #[test]
    fn test_case_insensitive_provider() {
        let spec = ModelSpecifier::parse("OpenAI/gpt-4o").unwrap();
        assert_eq!(spec.provider, Some(ProviderName::OpenAI));

        let spec = ModelSpecifier::parse("ANTHROPIC/claude").unwrap();
        assert_eq!(spec.provider, Some(ProviderName::Anthropic));
    }

    #[test]
    fn test_from_str_trait() {
        let spec: ModelSpecifier = "gpt-4o".parse().unwrap();
        assert_eq!(spec.model, "gpt-4o");

        let spec: ModelSpecifier = "openai/gpt-4o".parse().unwrap();
        assert_eq!(spec.provider, Some(ProviderName::OpenAI));
    }
}
