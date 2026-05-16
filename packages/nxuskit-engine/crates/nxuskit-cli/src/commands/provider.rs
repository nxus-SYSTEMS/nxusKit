//! `nxuskit-cli provider status|logout|list|info` — Provider management (FR-010, FR-011, FR-014).

use clap::{Args, Subcommand};
use nxuskit_engine::capabilities::{CapabilityStatus, registry};
use serde::Serialize;
use std::collections::HashMap;

use crate::cli_error::CliError;
use crate::envelope::TraceFields;
use crate::output::{OutputFormat, OutputWriter};

fn bool_to_yes_no(b: bool) -> &'static str {
    if b { "yes" } else { "no" }
}

#[derive(Debug, Subcommand)]
pub enum ProviderCommandAction {
    /// Show authentication status for providers
    Status(ProviderStatusArgs),
    /// Revoke authentication for a provider
    Logout(ProviderLogoutArgs),
    /// List all available providers with metadata
    List(ProviderListArgs),
    /// Show detailed information for a specific provider
    Info(ProviderInfoArgs),
}

// ── Args ───────────────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct ProviderStatusArgs {
    /// Specific provider (omit for all)
    pub provider: Option<String>,

    #[arg(short, long, default_value = "json")]
    pub format: String,

    #[arg(short, long, default_value_t = false)]
    pub quiet: bool,

    #[arg(short, long)]
    pub output: Option<String>,
}

#[derive(Debug, Args)]
pub struct ProviderLogoutArgs {
    /// Provider to log out from
    #[arg(long)]
    pub provider: String,

    #[arg(short, long, default_value = "json")]
    pub format: String,

    #[arg(short, long, default_value_t = false)]
    pub quiet: bool,

    #[arg(short, long)]
    pub output: Option<String>,
}

#[derive(Debug, Args)]
pub struct ProviderListArgs {
    /// Output format
    #[arg(short, long, default_value = "json")]
    pub format: String,

    /// Shorthand for --format json
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ProviderInfoArgs {
    /// Provider name to inspect
    pub provider: String,

    /// Output format
    #[arg(short, long, default_value = "json")]
    pub format: String,

    /// Shorthand for --format json
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

// ── Response types ─────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ProviderStatusResult {
    pub providers: Vec<ProviderEntry>,
}

#[derive(Debug, Serialize)]
pub struct ProviderEntry {
    pub name: String,
    pub authenticated: bool,
    pub models_available: Option<u32>,
    pub health: String,
}

#[derive(Debug, Serialize)]
pub struct ProviderLogoutResult {
    pub provider: String,
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct ProviderListResult {
    pub providers: Vec<ProviderDetail>,
}

#[derive(Debug, Serialize)]
pub struct ProviderDetail {
    pub name: String,
    pub display_name: String,
    pub auth_status: String,
    pub last_reviewed_on: String,
    pub provider_status: String,
    pub capabilities: ProviderCapabilities,
    pub capability_status: HashMap<String, CapabilityStatus>,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ProviderCapabilities {
    pub vision: bool,
    pub streaming: bool,
    pub tool_calling: bool,
    pub thinking: bool,
    pub streaming_logprobs: bool,
}

#[derive(Debug, Serialize)]
pub struct ProviderInfoResult {
    pub name: String,
    pub display_name: String,
    pub auth_status: String,
    pub last_reviewed_on: String,
    pub provider_status: String,
    pub capabilities: ProviderCapabilities,
    pub capability_status: HashMap<String, CapabilityStatus>,
    pub auth_methods: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<ModelSummary>,
}

#[derive(Debug, Serialize)]
pub struct ModelSummary {
    pub id: String,
    pub name: String,
}

// ── Provider metadata ──────────────────────────────────────────────────

struct ProviderMeta {
    name: &'static str,
    display_name: &'static str,
    auth_methods: &'static [&'static str],
    capabilities: ProviderCapabilities,
}

fn known_providers() -> Vec<ProviderMeta> {
    vec![
        ProviderMeta {
            name: "claude",
            display_name: "Anthropic Claude",
            auth_methods: &["api_key", "oauth"],
            capabilities: ProviderCapabilities {
                vision: true,
                streaming: true,
                tool_calling: true,
                thinking: true,
                streaming_logprobs: false,
            },
        },
        ProviderMeta {
            name: "openai",
            display_name: "OpenAI",
            auth_methods: &["api_key"],
            capabilities: ProviderCapabilities {
                vision: true,
                streaming: true,
                tool_calling: true,
                thinking: false,
                streaming_logprobs: true,
            },
        },
        ProviderMeta {
            name: "ollama",
            display_name: "Ollama (local)",
            auth_methods: &[],
            capabilities: ProviderCapabilities {
                vision: true,
                streaming: true,
                tool_calling: false,
                thinking: true,
                streaming_logprobs: false,
            },
        },
        ProviderMeta {
            name: "lmstudio",
            display_name: "LM Studio (local)",
            auth_methods: &[],
            capabilities: ProviderCapabilities {
                vision: false,
                streaming: true,
                tool_calling: false,
                thinking: false,
                streaming_logprobs: false,
            },
        },
        ProviderMeta {
            name: "loopback",
            display_name: "Loopback (echo test)",
            auth_methods: &[],
            capabilities: ProviderCapabilities {
                vision: false,
                streaming: true,
                tool_calling: false,
                thinking: false,
                streaming_logprobs: false,
            },
        },
        ProviderMeta {
            name: "fireworks",
            display_name: "Fireworks AI",
            auth_methods: &["api_key"],
            capabilities: ProviderCapabilities {
                vision: false,
                streaming: true,
                tool_calling: true,
                thinking: false,
                streaming_logprobs: false,
            },
        },
        ProviderMeta {
            name: "xai",
            display_name: "xAI Grok",
            auth_methods: &["api_key"],
            capabilities: ProviderCapabilities {
                vision: true,
                streaming: true,
                tool_calling: true,
                thinking: true,
                streaming_logprobs: false,
            },
        },
        ProviderMeta {
            name: "groq",
            display_name: "Groq",
            auth_methods: &["api_key"],
            capabilities: ProviderCapabilities {
                vision: true,
                streaming: true,
                tool_calling: true,
                thinking: false,
                streaming_logprobs: false,
            },
        },
        ProviderMeta {
            name: "mistral",
            display_name: "Mistral AI",
            auth_methods: &["api_key"],
            capabilities: ProviderCapabilities {
                vision: false,
                streaming: true,
                tool_calling: true,
                thinking: false,
                streaming_logprobs: false,
            },
        },
        ProviderMeta {
            name: "openrouter",
            display_name: "OpenRouter",
            auth_methods: &["api_key"],
            capabilities: ProviderCapabilities {
                vision: true,
                streaming: true,
                tool_calling: true,
                thinking: false,
                streaming_logprobs: false,
            },
        },
        ProviderMeta {
            name: "perplexity",
            display_name: "Perplexity AI",
            auth_methods: &["api_key"],
            capabilities: ProviderCapabilities {
                vision: false,
                streaming: true,
                tool_calling: false,
                thinking: false,
                streaming_logprobs: false,
            },
        },
        ProviderMeta {
            name: "together",
            display_name: "Together AI",
            auth_methods: &["api_key"],
            capabilities: ProviderCapabilities {
                vision: true,
                streaming: true,
                tool_calling: true,
                thinking: false,
                streaming_logprobs: false,
            },
        },
        ProviderMeta {
            name: "clips",
            display_name: "CLIPS Rule Engine",
            auth_methods: &[],
            capabilities: ProviderCapabilities {
                vision: false,
                streaming: false,
                tool_calling: false,
                thinking: false,
                streaming_logprobs: false,
            },
        },
        ProviderMeta {
            name: "mcp",
            display_name: "MCP Server",
            auth_methods: &["token"],
            capabilities: ProviderCapabilities {
                vision: false,
                streaming: true,
                tool_calling: true,
                thinking: false,
                streaming_logprobs: false,
            },
        },
        ProviderMeta {
            name: "mock",
            display_name: "Mock (testing)",
            auth_methods: &[],
            capabilities: ProviderCapabilities {
                vision: false,
                streaming: false,
                tool_calling: false,
                thinking: false,
                streaming_logprobs: false,
            },
        },
    ]
}

fn find_provider(name: &str) -> Option<&'static str> {
    // This is a workaround — we check against the static list
    let known = [
        "claude",
        "openai",
        "ollama",
        "lmstudio",
        "loopback",
        "fireworks",
        "xai",
        "groq",
        "mistral",
        "openrouter",
        "perplexity",
        "together",
        "clips",
        "mcp",
        "mock",
    ];
    known.iter().find(|&&k| k == name.to_lowercase()).copied()
}

fn suggest_similar(name: &str) -> Vec<String> {
    let known = [
        "claude",
        "openai",
        "ollama",
        "lmstudio",
        "loopback",
        "fireworks",
        "xai",
        "groq",
        "mistral",
        "openrouter",
        "perplexity",
        "together",
        "clips",
        "mcp",
        "mock",
    ];
    let mut candidates: Vec<(&str, f64)> = known
        .iter()
        .map(|&k| (k, strsim::normalized_levenshtein(&name.to_lowercase(), k)))
        .filter(|(_, score)| *score > 0.4)
        .collect();
    candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    candidates
        .into_iter()
        .take(3)
        .map(|(k, _)| k.to_string())
        .collect()
}

const PUBLIC_CAPABILITY_FIELDS: &[&str] = &[
    "vision_input",
    "tool_calling",
    "thinking_blocks",
    "streaming_logprobs",
    "json_mode",
    "json_schema_strict",
    "json_schema_best_effort",
    "embeddings",
    "rerank",
];

fn registry_id_for_cli(cli_name: &str) -> &str {
    match cli_name {
        "claude" => "anthropic",
        other => other,
    }
}

fn unknown_capability_status() -> HashMap<String, CapabilityStatus> {
    PUBLIC_CAPABILITY_FIELDS
        .iter()
        .map(|field| ((*field).to_string(), CapabilityStatus::Unknown))
        .collect()
}

fn public_manifest_by_provider() -> HashMap<String, registry::PublicProviderCapability> {
    registry::public_manifest()
        .providers
        .into_iter()
        .map(|provider| (provider.name.clone(), provider))
        .collect()
}

// ── Dispatch ───────────────────────────────────────────────────────────

pub async fn run_provider_command_l1(action: ProviderCommandAction) -> Result<(), CliError> {
    match action {
        ProviderCommandAction::Status(args) => run_provider_status(args).await,
        ProviderCommandAction::Logout(args) => run_provider_logout(args).await,
        ProviderCommandAction::List(args) => run_provider_list(args).await,
        ProviderCommandAction::Info(args) => run_provider_info(args).await,
    }
}

// ── List ────────────────────────────────────────────────────────────────

pub async fn run_provider_list(args: ProviderListArgs) -> Result<(), CliError> {
    let fmt = if args.json { "json" } else { &args.format };
    let format = OutputFormat::parse(fmt)?;
    let writer = OutputWriter::new(format, false, None);
    let manifest_by_provider = public_manifest_by_provider();

    let providers: Vec<ProviderDetail> = known_providers()
        .into_iter()
        .map(|meta| {
            let auth_status = check_auth_status(meta.name);
            let public = manifest_by_provider.get(registry_id_for_cli(meta.name));
            ProviderDetail {
                name: meta.name.to_string(),
                display_name: meta.display_name.to_string(),
                auth_status,
                last_reviewed_on: public
                    .map(|p| p.last_reviewed_on.clone())
                    .unwrap_or_default(),
                provider_status: public
                    .map(|p| p.provider_status.clone())
                    .unwrap_or_else(|| "unknown".to_string()),
                capabilities: meta.capabilities,
                capability_status: public
                    .map(|p| p.capabilities.clone())
                    .unwrap_or_else(unknown_capability_status),
            }
        })
        .collect();

    let trace = TraceFields::new("provider_list", "", None, None);
    writer.write_response(ProviderListResult { providers }, trace, None, None, None)
}

// ── Info ────────────────────────────────────────────────────────────────

pub async fn run_provider_info(args: ProviderInfoArgs) -> Result<(), CliError> {
    let fmt = if args.json { "json" } else { &args.format };
    let format = OutputFormat::parse(fmt)?;
    let writer = OutputWriter::new(format, false, None);

    let name_lower = args.provider.to_lowercase();

    // Find provider or suggest alternatives
    if find_provider(&name_lower).is_none() {
        let suggestions = suggest_similar(&args.provider);
        let msg = if suggestions.is_empty() {
            format!(
                "Unknown provider: '{}'. Run `provider list` to see available providers.",
                args.provider
            )
        } else {
            format!(
                "Unknown provider: '{}'. Did you mean: {}?",
                args.provider,
                suggestions.join(", ")
            )
        };
        return Err(CliError::ValidationFailed { message: msg });
    }

    // Find metadata
    let meta = known_providers()
        .into_iter()
        .find(|m| m.name == name_lower)
        .unwrap(); // safe: we checked above

    let auth_status = check_auth_status(meta.name);
    let public = public_manifest_by_provider().remove(registry_id_for_cli(meta.name));

    // Try to list models if provider is available
    let models = match crate::create_provider(meta.name) {
        Ok(p) => match p.list_models().await {
            Ok(model_list) => model_list
                .into_iter()
                .map(|m| ModelSummary {
                    id: m.name.clone(),
                    name: m.description.unwrap_or_default(),
                })
                .collect(),
            Err(_) => vec![],
        },
        Err(_) => vec![],
    };

    let result = ProviderInfoResult {
        name: meta.name.to_string(),
        display_name: meta.display_name.to_string(),
        auth_status,
        last_reviewed_on: public
            .as_ref()
            .map(|p| p.last_reviewed_on.clone())
            .unwrap_or_default(),
        provider_status: public
            .as_ref()
            .map(|p| p.provider_status.clone())
            .unwrap_or_else(|| "unknown".to_string()),
        capabilities: meta.capabilities,
        capability_status: public
            .map(|p| p.capabilities)
            .unwrap_or_else(unknown_capability_status),
        auth_methods: meta.auth_methods.iter().map(|s| s.to_string()).collect(),
        models,
    };

    if format == OutputFormat::Human {
        let caps = &result.capabilities;
        println!("provider           : {}", result.name);
        println!("display name       : {}", result.display_name);
        println!("auth status        : {}", result.auth_status);
        println!("last reviewed      : {}", result.last_reviewed_on);
        println!("provider status    : {}", result.provider_status);
        println!("vision             : {}", bool_to_yes_no(caps.vision));
        println!("streaming          : {}", bool_to_yes_no(caps.streaming));
        println!("tool calling       : {}", bool_to_yes_no(caps.tool_calling));
        println!("thinking           : {}", bool_to_yes_no(caps.thinking));
        println!(
            "streaming logprobs : {}",
            bool_to_yes_no(caps.streaming_logprobs)
        );
        println!("capability status  :");
        for field in PUBLIC_CAPABILITY_FIELDS {
            if let Some(status) = result.capability_status.get(*field) {
                println!("  {field:<23}: {}", status.human());
            }
        }
        return Ok(());
    }

    let trace = TraceFields::new("provider_info", "", Some(meta.name), None);
    writer.write_response(result, trace, None, None, None)
}

// ── Status ──────────────────────────────────────────────────────────────

async fn run_provider_status(args: ProviderStatusArgs) -> Result<(), CliError> {
    let format = OutputFormat::parse(&args.format)?;
    let writer = OutputWriter::new(format, args.quiet, args.output);

    let known = [
        "claude",
        "openai",
        "ollama",
        "lmstudio",
        "loopback",
        "fireworks",
        "xai",
        "groq",
        "mistral",
        "openrouter",
        "perplexity",
        "together",
    ];

    let providers: Vec<ProviderEntry> = if let Some(name) = &args.provider {
        vec![check_provider(name).await]
    } else {
        let mut entries = Vec::new();
        for name in &known {
            entries.push(check_provider(name).await);
        }
        entries
    };

    let trace = TraceFields::new("provider_status", "", args.provider.as_deref(), None);
    writer.write_response(ProviderStatusResult { providers }, trace, None, None, None)
}

async fn check_provider(name: &str) -> ProviderEntry {
    let (authenticated, models_available) = match crate::create_provider(name) {
        Ok(p) => {
            let models = p.list_models().await.ok().map(|m| m.len() as u32);
            (models.is_some(), models)
        }
        Err(_) => (false, None),
    };

    ProviderEntry {
        name: name.to_string(),
        authenticated,
        models_available,
        health: if authenticated { "ok" } else { "unknown" }.to_string(),
    }
}

// ── Logout ──────────────────────────────────────────────────────────────

async fn run_provider_logout(args: ProviderLogoutArgs) -> Result<(), CliError> {
    let format = OutputFormat::parse(&args.format)?;
    let writer = OutputWriter::new(format, args.quiet, args.output);

    let provider_name = &args.provider;
    let env_var = match provider_name.as_str() {
        "claude" => Some("ANTHROPIC_API_KEY"),
        "openai" => Some("OPENAI_API_KEY"),
        "fireworks" => Some("FIREWORKS_API_KEY"),
        "xai" => Some("XAI_API_KEY"),
        "groq" => Some("GROQ_API_KEY"),
        "mistral" => Some("MISTRAL_API_KEY"),
        "openrouter" => Some("OPENROUTER_API_KEY"),
        "perplexity" => Some("PERPLEXITY_API_KEY"),
        "together" => Some("TOGETHER_API_KEY"),
        _ => None,
    };

    let has_env_token = env_var.is_some_and(|v| std::env::var(v).is_ok());
    let deleted_global = nxuskit_core::auth_token::delete_auth_token().is_ok();

    let success = has_env_token || deleted_global;
    let message = if has_env_token {
        format!(
            "Provider '{}' uses environment variable {}. Remove it from your shell to log out.",
            provider_name,
            env_var.unwrap_or("(unknown)")
        )
    } else if deleted_global {
        format!("Successfully logged out from {}", provider_name)
    } else {
        format!("No credentials found for {}", provider_name)
    };

    let trace = TraceFields::new("provider_logout", "", Some(provider_name), None);
    writer.write_response(
        ProviderLogoutResult {
            provider: args.provider,
            success,
            message,
        },
        trace,
        None,
        None,
        None,
    )
}

// ── Helpers ─────────────────────────────────────────────────────────────

fn check_auth_status(name: &str) -> String {
    match name {
        // No auth needed
        "loopback" | "mock" | "ollama" | "lmstudio" => "not_required".to_string(),
        // CLIPS uses CLIPS_RULES_DIR, not API keys
        "clips" => {
            if std::env::var("CLIPS_RULES_DIR").is_ok() {
                "configured".to_string()
            } else {
                "not_configured".to_string()
            }
        }
        // MCP uses MCP_SERVER
        "mcp" => {
            if std::env::var("MCP_SERVER").is_ok() {
                "configured".to_string()
            } else {
                "not_configured".to_string()
            }
        }
        // Check for provider-specific env var
        _ => {
            let env_var = match name {
                "claude" => "ANTHROPIC_API_KEY",
                "openai" => "OPENAI_API_KEY",
                "fireworks" => "FIREWORKS_API_KEY",
                "xai" => "XAI_API_KEY",
                "groq" => "GROQ_API_KEY",
                "mistral" => "MISTRAL_API_KEY",
                "openrouter" => "OPENROUTER_API_KEY",
                "perplexity" => "PERPLEXITY_API_KEY",
                "together" => "TOGETHER_API_KEY",
                _ => return "unknown".to_string(),
            };
            if std::env::var(env_var).is_ok() {
                "authenticated".to_string()
            } else {
                "unauthenticated".to_string()
            }
        }
    }
}
