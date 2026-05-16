//! Provider auth metadata registry.
//!
//! Static registry of provider-specific authentication metadata used by the
//! auth helper system. Each entry describes how a provider authenticates
//! (env var name, dashboard URL, OAuth capability, etc.).

use serde::Serialize;

/// Authentication metadata for a single provider.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderAuthMetadata {
    pub provider_id: &'static str,
    pub display_name: &'static str,
    pub env_var_name: &'static str,
    pub auth_required: bool,
    pub dashboard_url: Option<&'static str>,
    pub oauth_capable: bool,
    pub auth_methods: &'static [&'static str],
    pub credential_service_name: &'static str,
}

/// Static registry of known providers.
static PROVIDERS: &[ProviderAuthMetadata] = &[
    ProviderAuthMetadata {
        provider_id: "openai",
        display_name: "OpenAI / GPT",
        env_var_name: "OPENAI_API_KEY",
        auth_required: true,
        dashboard_url: Some("https://platform.openai.com/api-keys"),
        oauth_capable: false,
        auth_methods: &["api_key"],
        credential_service_name: "nxuskit-openai",
    },
    ProviderAuthMetadata {
        provider_id: "claude",
        display_name: "Anthropic / Claude",
        env_var_name: "ANTHROPIC_API_KEY",
        auth_required: true,
        dashboard_url: Some("https://console.anthropic.com/settings/keys"),
        oauth_capable: false,
        auth_methods: &["api_key"],
        credential_service_name: "nxuskit-claude",
    },
    ProviderAuthMetadata {
        provider_id: "groq",
        display_name: "Groq",
        env_var_name: "GROQ_API_KEY",
        auth_required: true,
        dashboard_url: Some("https://console.groq.com/keys"),
        oauth_capable: false,
        auth_methods: &["api_key"],
        credential_service_name: "nxuskit-groq",
    },
    ProviderAuthMetadata {
        provider_id: "xai",
        display_name: "xAI Grok",
        env_var_name: "XAI_API_KEY",
        auth_required: true,
        dashboard_url: Some("https://console.x.ai/team/default/api-keys"),
        oauth_capable: false,
        auth_methods: &["api_key"],
        credential_service_name: "nxuskit-xai",
    },
    ProviderAuthMetadata {
        provider_id: "ollama",
        display_name: "Ollama",
        env_var_name: "OLLAMA_HOST",
        auth_required: false,
        dashboard_url: None,
        oauth_capable: false,
        auth_methods: &[],
        credential_service_name: "nxuskit-ollama",
    },
    ProviderAuthMetadata {
        provider_id: "lm-studio",
        display_name: "LM Studio",
        env_var_name: "LM_STUDIO_HOST",
        auth_required: false,
        dashboard_url: None,
        oauth_capable: false,
        auth_methods: &[],
        credential_service_name: "nxuskit-lm-studio",
    },
    ProviderAuthMetadata {
        provider_id: "mistral",
        display_name: "Mistral AI",
        env_var_name: "MISTRAL_API_KEY",
        auth_required: true,
        dashboard_url: Some("https://console.mistral.ai/api-keys"),
        oauth_capable: false,
        auth_methods: &["api_key"],
        credential_service_name: "nxuskit-mistral",
    },
    ProviderAuthMetadata {
        provider_id: "fireworks",
        display_name: "Fireworks AI",
        env_var_name: "FIREWORKS_API_KEY",
        auth_required: true,
        dashboard_url: Some("https://fireworks.ai/account/api-keys"),
        oauth_capable: false,
        auth_methods: &["api_key"],
        credential_service_name: "nxuskit-fireworks",
    },
    ProviderAuthMetadata {
        provider_id: "together",
        display_name: "Together AI",
        env_var_name: "TOGETHER_API_KEY",
        auth_required: true,
        dashboard_url: Some("https://api.together.ai/settings/api-keys"),
        oauth_capable: false,
        auth_methods: &["api_key"],
        credential_service_name: "nxuskit-together",
    },
    ProviderAuthMetadata {
        provider_id: "openrouter",
        display_name: "OpenRouter",
        env_var_name: "OPENROUTER_API_KEY",
        auth_required: true,
        dashboard_url: Some("https://openrouter.ai/settings/keys"),
        oauth_capable: false,
        auth_methods: &["api_key"],
        credential_service_name: "nxuskit-openrouter",
    },
    ProviderAuthMetadata {
        provider_id: "perplexity",
        display_name: "Perplexity",
        env_var_name: "PERPLEXITY_API_KEY",
        auth_required: true,
        dashboard_url: Some("https://www.perplexity.ai/settings/api"),
        oauth_capable: false,
        auth_methods: &["api_key"],
        credential_service_name: "nxuskit-perplexity",
    },
];

/// Look up provider metadata by ID.
pub fn lookup(provider_id: &str) -> Option<&'static ProviderAuthMetadata> {
    PROVIDERS.iter().find(|p| p.provider_id == provider_id)
}

/// Get all known provider metadata entries.
pub fn all_providers() -> &'static [ProviderAuthMetadata] {
    PROVIDERS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lookup_known_provider() {
        let meta = lookup("openai").unwrap();
        assert_eq!(meta.env_var_name, "OPENAI_API_KEY");
        assert!(meta.auth_required);
    }

    #[test]
    fn test_lookup_local_provider() {
        let meta = lookup("ollama").unwrap();
        assert!(!meta.auth_required);
        assert!(meta.auth_methods.is_empty());
    }

    #[test]
    fn test_lookup_unknown_returns_none() {
        assert!(lookup("nonexistent").is_none());
    }

    #[test]
    fn test_all_providers_count() {
        assert!(all_providers().len() >= 5, "At least 5 providers expected");
    }

    #[test]
    fn test_unique_provider_ids() {
        let ids: Vec<&str> = all_providers().iter().map(|p| p.provider_id).collect();
        let mut deduped = ids.clone();
        deduped.sort();
        deduped.dedup();
        assert_eq!(ids.len(), deduped.len(), "Provider IDs must be unique");
    }

    #[test]
    fn test_unique_service_names() {
        let names: Vec<&str> = all_providers()
            .iter()
            .map(|p| p.credential_service_name)
            .collect();
        let mut deduped = names.clone();
        deduped.sort();
        deduped.dedup();
        assert_eq!(names.len(), deduped.len(), "Service names must be unique");
    }
}
