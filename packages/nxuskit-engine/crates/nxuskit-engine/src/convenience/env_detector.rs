//! Environment variable detection for convenience API
//!
//! Automatically detects API keys and base URLs from environment variables
//! following standard provider conventions.

use super::parser::ProviderName;
use std::env;

/// Environment configuration for a provider
#[derive(Debug, Clone)]
pub struct EnvConfig {
    /// Provider this configuration is for
    pub provider: ProviderName,
    /// API key (if set)
    pub api_key: Option<String>,
    /// Base URL (if set)
    pub base_url: Option<String>,
}

impl EnvConfig {
    /// Detect configuration from environment for specific provider
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nxuskit_engine::convenience::env_detector::EnvConfig;
    /// # use nxuskit_engine::convenience::parser::ProviderName;
    /// # unsafe {
    /// std::env::set_var("OPENAI_API_KEY", "sk-test123");
    /// # }
    /// let config = EnvConfig::detect(ProviderName::OpenAI);
    /// assert!(config.is_valid());
    /// ```
    pub fn detect(provider: ProviderName) -> Self {
        match provider {
            ProviderName::Anthropic => Self::detect_anthropic(),
            ProviderName::OpenAI => Self::detect_openai(),
            ProviderName::Ollama => Self::detect_ollama(),
        }
    }

    /// Detect Anthropic configuration
    fn detect_anthropic() -> Self {
        Self {
            provider: ProviderName::Anthropic,
            api_key: get_non_empty_var("ANTHROPIC_API_KEY"),
            base_url: get_non_empty_var("ANTHROPIC_BASE_URL"),
        }
    }

    /// Detect OpenAI configuration
    fn detect_openai() -> Self {
        Self {
            provider: ProviderName::OpenAI,
            api_key: get_non_empty_var("OPENAI_API_KEY"),
            base_url: get_non_empty_var("OPENAI_BASE_URL"),
        }
    }

    /// Detect Ollama configuration
    fn detect_ollama() -> Self {
        Self {
            provider: ProviderName::Ollama,
            api_key: get_non_empty_var("OLLAMA_API_KEY"), // Optional for Ollama
            base_url: get_non_empty_var("OLLAMA_API_URL"),
        }
    }

    /// Check if configuration is valid (has required credentials)
    ///
    /// Ollama doesn't require an API key, but OpenAI and Anthropic do.
    pub fn is_valid(&self) -> bool {
        match self.provider {
            // Ollama doesn't require an API key
            ProviderName::Ollama => true,
            // Other providers require an API key
            ProviderName::OpenAI | ProviderName::Anthropic => self.api_key.is_some(),
        }
    }

    /// Get error message for missing credentials
    ///
    /// Returns Some with the error message if credentials are missing,
    /// None if configuration is valid.
    pub fn missing_credential_error(&self) -> Option<String> {
        if self.is_valid() {
            return None;
        }

        let var_name = match self.provider {
            ProviderName::Anthropic => "ANTHROPIC_API_KEY",
            ProviderName::OpenAI => "OPENAI_API_KEY",
            ProviderName::Ollama => return None, // Ollama doesn't require credentials
        };

        Some(format!(
            "Missing API credentials for {}. Set environment variable: {}",
            self.provider, var_name
        ))
    }

    /// Get default base URL for provider
    pub fn default_base_url(&self) -> &'static str {
        match self.provider {
            ProviderName::Anthropic => "https://api.anthropic.com",
            ProviderName::OpenAI => "https://api.openai.com/v1",
            ProviderName::Ollama => "http://localhost:11434",
        }
    }

    /// Get effective base URL (custom or default)
    pub fn effective_base_url(&self) -> String {
        self.base_url
            .clone()
            .unwrap_or_else(|| self.default_base_url().to_string())
    }
}

/// Get environment variable value, treating empty strings as None
///
/// This follows the convention that empty environment variables should be
/// treated the same as unset variables.
fn get_non_empty_var(name: &str) -> Option<String> {
    env::var(name).ok().and_then(|v| {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::sync::Mutex;

    // Mutex to serialize tests that modify environment variables
    // Environment variables are process-global, so tests modifying them must not run in parallel
    // This is pub so other test modules can use it for synchronization
    pub static ENV_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn test_get_non_empty_var() {
        let _guard = ENV_MUTEX.lock().unwrap();

        // Set test variable
        unsafe {
            env::set_var("TEST_VAR", "value");
        }
        assert_eq!(get_non_empty_var("TEST_VAR"), Some("value".to_string()));

        // Empty string treated as None
        unsafe {
            env::set_var("TEST_EMPTY", "");
        }
        assert_eq!(get_non_empty_var("TEST_EMPTY"), None);

        // Whitespace-only treated as None
        unsafe {
            env::set_var("TEST_WHITESPACE", "   ");
        }
        assert_eq!(get_non_empty_var("TEST_WHITESPACE"), None);

        // Unset variable
        unsafe {
            env::remove_var("TEST_UNSET");
        }
        assert_eq!(get_non_empty_var("TEST_UNSET"), None);
    }

    #[test]
    fn test_openai_config() {
        let _guard = ENV_MUTEX.lock().unwrap();

        unsafe {
            env::set_var("OPENAI_API_KEY", "sk-test123");
            env::remove_var("OPENAI_BASE_URL");
        }

        let config = EnvConfig::detect(ProviderName::OpenAI);
        assert_eq!(config.api_key, Some("sk-test123".to_string()));
        assert_eq!(config.base_url, None);
        assert!(config.is_valid());
        assert_eq!(config.missing_credential_error(), None);
        assert_eq!(config.effective_base_url(), "https://api.openai.com/v1");

        unsafe {
            env::remove_var("OPENAI_API_KEY");
        }
    }

    #[test]
    fn test_anthropic_config() {
        let _guard = ENV_MUTEX.lock().unwrap();

        unsafe {
            env::set_var("ANTHROPIC_API_KEY", "sk-ant-test");
            env::remove_var("ANTHROPIC_BASE_URL");
        }

        let config = EnvConfig::detect(ProviderName::Anthropic);
        assert_eq!(config.api_key, Some("sk-ant-test".to_string()));
        assert!(config.is_valid());

        unsafe {
            env::remove_var("ANTHROPIC_API_KEY");
        }
    }

    #[test]
    fn test_ollama_config() {
        let _guard = ENV_MUTEX.lock().unwrap();

        unsafe {
            env::remove_var("OLLAMA_API_KEY");
            env::remove_var("OLLAMA_API_URL");
        }

        let config = EnvConfig::detect(ProviderName::Ollama);
        // Ollama is always valid (doesn't require API key)
        assert!(config.is_valid());
        assert_eq!(config.missing_credential_error(), None);
        assert_eq!(config.effective_base_url(), "http://localhost:11434");
    }

    #[test]
    fn test_custom_base_url() {
        let _guard = ENV_MUTEX.lock().unwrap();

        unsafe {
            env::set_var("OPENAI_API_KEY", "sk-test");
            env::set_var("OPENAI_BASE_URL", "https://custom.api.com");
        }

        let config = EnvConfig::detect(ProviderName::OpenAI);
        assert_eq!(config.base_url, Some("https://custom.api.com".to_string()));
        assert_eq!(config.effective_base_url(), "https://custom.api.com");

        unsafe {
            env::remove_var("OPENAI_API_KEY");
            env::remove_var("OPENAI_BASE_URL");
        }
    }

    #[test]
    fn test_missing_credentials() {
        let _guard = ENV_MUTEX.lock().unwrap();

        unsafe {
            env::remove_var("OPENAI_API_KEY");
        }

        let config = EnvConfig::detect(ProviderName::OpenAI);
        assert!(!config.is_valid());
        assert!(config.missing_credential_error().is_some());
        assert!(
            config
                .missing_credential_error()
                .unwrap()
                .contains("OPENAI_API_KEY")
        );
    }

    #[test]
    fn test_empty_api_key_treated_as_missing() {
        let _guard = ENV_MUTEX.lock().unwrap();

        unsafe {
            env::set_var("OPENAI_API_KEY", "");
        }

        let config = EnvConfig::detect(ProviderName::OpenAI);
        assert_eq!(config.api_key, None);
        assert!(!config.is_valid());

        unsafe {
            env::remove_var("OPENAI_API_KEY");
        }
    }
}
