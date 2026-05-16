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
    pub models: Vec<ModelEntry>,
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
    let writer = OutputWriter::new(format, args.quiet, args.output);

    let provider_impl =
        crate::create_provider(&args.provider).map_err(|e| CliError::ProviderError {
            message: format!("{e}"),
        })?;

    let raw_models = provider_impl
        .list_models()
        .await
        .map_err(|e| CliError::ProviderError {
            message: format!("{e}"),
        })?;

    let is_local = matches!(args.provider.as_str(), "ollama" | "lmstudio");

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
    writer.write_response(ModelsResult { models }, trace, None, None, None)
}
