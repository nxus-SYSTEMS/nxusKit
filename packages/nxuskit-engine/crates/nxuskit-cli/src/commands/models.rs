//! `nxuskit-cli models` — Model discovery with filters (FR-013).

use clap::Args;
use serde::Serialize;

use crate::cli_error::CliError;
use crate::envelope::TraceFields;
use crate::output::{OutputFormat, OutputWriter};

#[derive(Debug, Args)]
pub struct ModelsArgs {
    /// Provider to query
    #[arg(short, long)]
    pub provider: String,

    #[arg(short, long, default_value = "json")]
    pub format: String,

    /// Optional discovery endpoint. When supplied, model discovery uses the
    /// independent endpoint/auth-only DiscoveryClient surface.
    #[arg(long)]
    pub endpoint: Option<String>,

    /// Optional environment-variable credential selector for independent discovery.
    #[arg(long)]
    pub auth_selector: Option<String>,

    /// Independent discovery connect timeout in milliseconds.
    #[arg(long, default_value_t = 5000)]
    pub connect_timeout_ms: u32,

    /// Independent discovery read timeout in milliseconds.
    #[arg(long, default_value_t = 5000)]
    pub read_timeout_ms: u32,

    /// Perform the independent endpoint/auth-only discovery health operation.
    #[arg(long, default_value_t = false)]
    pub health: bool,

    /// Filter by capability: chat, streaming, vision, function_calling
    #[arg(long)]
    pub supports: Option<String>,

    /// Minimum context window size (tokens); excludes models without metadata
    #[arg(long)]
    pub min_context: Option<u32>,

    /// Show only locally-running models (ollama, lmstudio)
    #[arg(long, default_value_t = false)]
    pub local_only: bool,

    #[arg(short, long, default_value_t = false)]
    pub quiet: bool,

    #[arg(short, long)]
    pub output: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ModelsResult {
    pub provider_id: String,
    pub models: Vec<ModelEntry>,
}

#[derive(Debug, Serialize)]
pub struct DiscoveryHealthResult {
    pub provider_id: String,
    pub result: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ModelEntry {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub supports: Vec<String>,
    pub context_window: Option<u32>,
    pub local: bool,
}

pub async fn run_models_command(args: ModelsArgs) -> Result<(), CliError> {
    let format = OutputFormat::parse(&args.format)?;
    let writer = OutputWriter::new(format, args.quiet, args.output.clone());

    if args.health {
        use nxuskit_engine::discovery::{DiscoveryClientFactory, DiscoveryConfig};

        let endpoint = args
            .endpoint
            .as_deref()
            .ok_or_else(|| CliError::InvalidRequest {
                message: "--health requires --endpoint".to_string(),
            })?;
        let discovery_provider = match args.provider.as_str() {
            "ollama" => "ollama_local",
            other => other,
        };
        let client = DiscoveryClientFactory::create(DiscoveryConfig {
            provider_id: discovery_provider.to_string(),
            endpoint: endpoint.to_string(),
            auth_selector: args.auth_selector.clone(),
            connect_timeout_ms: args.connect_timeout_ms,
            read_timeout_ms: args.read_timeout_ms,
        })
        .map_err(CliError::from_contract_error)?;
        client
            .health()
            .await
            .map_err(CliError::from_contract_error)?;
        client
            .close()
            .await
            .map_err(CliError::from_contract_error)?;
        let trace = TraceFields::new("models_health", "", Some(&args.provider), None);
        return writer.write_response(
            DiscoveryHealthResult {
                provider_id: args.provider,
                result: "ok",
            },
            trace,
            None,
            None,
            None,
        );
    }

    let raw_models = if let Some(endpoint) = args.endpoint.as_deref() {
        use nxuskit_engine::discovery::{DiscoveryClientFactory, DiscoveryConfig};

        let discovery_provider = match args.provider.as_str() {
            "ollama" => "ollama_local",
            other => other,
        };
        let client = DiscoveryClientFactory::create(DiscoveryConfig {
            provider_id: discovery_provider.to_string(),
            endpoint: endpoint.to_string(),
            auth_selector: args.auth_selector.clone(),
            connect_timeout_ms: args.connect_timeout_ms,
            read_timeout_ms: args.read_timeout_ms,
        })
        .map_err(CliError::from_contract_error)?;
        let models = client
            .list_models()
            .await
            .map_err(CliError::from_contract_error)?;
        client
            .close()
            .await
            .map_err(CliError::from_contract_error)?;
        models
    } else {
        let provider_impl =
            crate::create_provider(&args.provider).map_err(|e| CliError::ProviderError {
                message: format!("{e}"),
            })?;
        provider_impl
            .list_models()
            .await
            .map_err(CliError::from_provider_error)?
    };

    let is_local = matches!(
        args.provider.as_str(),
        "ollama" | "ollama_local" | "ollama_cloud_via_daemon" | "lmstudio"
    );

    let mut models: Vec<ModelEntry> = raw_models
        .iter()
        .map(|m| {
            // Infer capabilities from model metadata and provider type
            let mut supports = Vec::new();

            // All models support chat
            supports.push("chat".to_string());

            // Infer streaming support: most providers support streaming
            if matches!(
                args.provider.as_str(),
                "claude"
                    | "openai"
                    | "ollama"
                    | "lmstudio"
                    | "loopback"
                    | "fireworks"
                    | "xai"
                    | "groq"
                    | "mistral"
                    | "openrouter"
                    | "perplexity"
                    | "together"
            ) {
                supports.push("streaming".to_string());
            }

            // Infer function_calling from provider type
            if matches!(
                args.provider.as_str(),
                "claude"
                    | "openai"
                    | "loopback"
                    | "fireworks"
                    | "xai"
                    | "groq"
                    | "mistral"
                    | "openrouter"
                    | "together"
            ) {
                supports.push("function_calling".to_string());
            }

            // Check if model name suggests vision capabilities
            let name_lower = m.name.to_lowercase();
            if name_lower.contains("vision")
                || name_lower.contains("gpt-4o")
                || name_lower.contains("claude-3")
                || name_lower.contains("claude-sonnet")
                || name_lower.contains("claude-opus")
            {
                supports.push("vision".to_string());
            }

            ModelEntry {
                id: m.name.clone(),
                name: m.name.clone(),
                provider: args.provider.clone(),
                supports,
                context_window: m.context_window,
                local: is_local,
            }
        })
        .collect();

    // Apply filters
    if let Some(supports_filter) = &args.supports {
        models.retain(|m| m.supports.iter().any(|s| s == supports_filter));
    }

    if let Some(min_ctx) = args.min_context {
        models.retain(|m| m.context_window.is_some_and(|ctx| ctx >= min_ctx));
    }

    if args.local_only {
        models.retain(|m| m.local);
    }

    let trace = TraceFields::new("models", "", Some(&args.provider), None);
    writer.write_response(
        ModelsResult {
            provider_id: args.provider,
            models,
        },
        trace,
        None,
        None,
        None,
    )
}
