//! CLI for the nxusKit library

mod cli_error;
mod commands;
mod entitlement_check;
mod envelope;
mod examples;
mod output;

use clap::{Parser, Subcommand};
use futures::StreamExt;
use nxuskit_engine::pipeline;
use nxuskit_engine::prelude::*;
use std::env;
use std::io::Write;
use std::path::PathBuf;
use std::process;

use nxuskit_core::auth_token;
use nxuskit_core::device_auth;

use nxuskit_engine::providers::clips::{
    extract_schemas_from_environment, json_schema_to_deftemplate, templates_to_json_schema,
};

#[derive(Parser)]
#[command(name = "nxuskit-cli")]
#[command(version)]
#[command(about = "JSON-first control plane for shell automation, CI, and multi-engine reasoning workflows", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Send a chat message to an LLM provider
    Chat {
        /// Provider to use (claude, openai, ollama, lmstudio, loopback, fireworks, xai, groq, mistral, openrouter, perplexity, together, clips, mcp, mock)
        #[arg(short, long)]
        provider: String,

        /// Model to use
        #[arg(short, long)]
        model: String,

        /// Message to send
        message: String,

        /// Enable streaming
        #[arg(short, long)]
        stream: bool,

        /// Temperature (0.0 to 2.0)
        #[arg(short, long)]
        temperature: Option<f32>,

        /// Maximum tokens to generate
        #[arg(long)]
        max_tokens: Option<u32>,
    },

    /// List available models from a provider (with filters)
    Models(commands::models::ModelsArgs),

    /// Get model capabilities (vision, streaming, function calling)
    Capabilities {
        /// Provider to query
        #[arg(short, long)]
        provider: String,

        /// Model name to query capabilities for
        model: String,

        /// Output format (text or json)
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// Convert between CLIPS deftemplate and JSON Schema
    Schema {
        #[command(subcommand)]
        action: SchemaAction,
    },

    /// Pipeline management commands
    Pipeline {
        #[command(subcommand)]
        action: PipelineAction,
    },

    /// Activate a Pro license on this machine
    Activate {
        /// Purchase ID received via email after Pro purchase
        #[arg(long)]
        key: Option<String>,

        /// Activate a trial instead of a paid license
        #[arg(long)]
        trial: bool,

        /// Activation code for trial (skips interactive prompt)
        #[arg(long)]
        code: Option<String>,

        /// Accept the nxus.SYSTEMS EULA non-interactively (required for CI/non-TTY)
        #[arg(long)]
        accept_eula: bool,

        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },

    /// Deactivate the Pro license on this machine
    Deactivate {
        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },

    /// License management commands
    License {
        #[command(subcommand)]
        action: LicenseAction,
    },

    /// Plugin management commands
    Plugin {
        #[command(subcommand)]
        action: PluginAction,
    },

    /// Provider authentication management commands (OAuth, API keys)
    Provider {
        #[command(subcommand)]
        action: ProviderAction,
    },

    /// Browse and search SDK examples
    Examples {
        #[command(subcommand)]
        action: examples::ExamplesAction,
    },

    /// Generate shell completion scripts (bash, zsh, fish)
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },

    // ── Level 1 Commands ────────────────────────────────────────────
    /// Machine-facing LLM invocation (JSON-first)
    Call(commands::call::CallArgs),

    /// CLIPS rule evaluation
    Clips {
        #[command(subcommand)]
        action: commands::clips::ClipsAction,
    },

    /// ZEN decision table evaluation (Pro)
    Zen {
        #[command(subcommand)]
        action: commands::zen::ZenAction,
    },

    /// Z3 constraint solver (Pro)
    Solver {
        #[command(subcommand)]
        action: commands::solver::SolverAction,
    },

    /// Bayesian network inference
    Bn {
        #[command(subcommand)]
        action: commands::bn::BnAction,
    },

    /// Validate packets against JSON Schema
    Packet {
        #[command(subcommand)]
        action: commands::packet::PacketAction,
    },

    /// Merge and summarize artifacts
    Artifact {
        #[command(subcommand)]
        action: commands::artifact::ArtifactAction,
    },

    /// Tool-augmented LLM loop
    ToolLoop {
        #[command(subcommand)]
        action: commands::tool_loop::ToolLoopAction,
    },

    /// LLM-based candidate selection
    Judge {
        #[command(subcommand)]
        action: commands::judge::JudgeAction,
    },

    /// Multi-model forking and comparison
    Branch {
        #[command(subcommand)]
        action: commands::branch::BranchAction,
    },
}

/// Pipeline actions
#[derive(Subcommand)]
enum PipelineAction {
    /// Validate a pipeline definition
    Validate {
        /// Path to pipeline file (.json or .yaml)
        input: PathBuf,
    },

    /// Convert pipeline between JSON and YAML formats
    Convert {
        /// Path to input pipeline file
        input: PathBuf,

        /// Path to output file (format determined by extension)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Output format (json or yaml)
        #[arg(short, long)]
        format: Option<String>,
    },

    /// Execute a multi-stage pipeline definition
    Run(commands::pipeline::PipelineRunArgs),
}

/// License subcommands
#[derive(Subcommand)]
enum LicenseAction {
    /// Authenticate with the nxus.systems licensing platform
    Login {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Revoke auth session and delete stored credentials
    Logout {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Show authentication and license status
    Status {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Activate a license or trial
    Activate {
        /// Purchase ID for license activation
        #[arg(long)]
        key: Option<String>,
        /// Start a 30-day trial
        #[arg(long, default_value_t = false)]
        trial: bool,
        /// Accept the nxus.SYSTEMS EULA non-interactively (required for CI/non-TTY)
        #[arg(long, default_value_t = false)]
        accept_eula: bool,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Fetch an existing license token from the server
    ///
    /// Recovers from scenarios where the server created a trial or issued
    /// a token but the CLI timed out before storing it locally.
    Sync {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

/// Plugin subcommands
#[derive(Subcommand)]
enum PluginAction {
    /// Get or set the plugin trust mode
    Trust {
        /// Trust mode to set: "signed-only" or "allow-unsigned". Omit to show current mode.
        mode: Option<String>,

        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },
}

/// Provider auth subcommands
#[derive(Subcommand)]
enum ProviderAction {
    /// Initiate OAuth authentication for a provider
    Login {
        /// Provider to authenticate (e.g., "azure-openai")
        provider: String,

        /// Timeout in seconds (default 120)
        #[arg(long, default_value = "120")]
        timeout: u32,

        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },

    /// Show authentication status
    Status {
        /// Specific provider (omit for all providers)
        provider: Option<String>,

        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },

    /// Check provider reachability and latency (e.g., `nxuskit-cli provider ping --provider ollama --json`)
    Ping {
        /// Provider to ping
        #[arg(long)]
        provider: String,

        /// Output in JSON format
        #[arg(long)]
        json: bool,

        /// Output format: "json" or "text" (default: "json")
        #[arg(long, default_value = "json")]
        format: String,

        /// Timeout in milliseconds (default: 5000)
        #[arg(long, default_value = "5000")]
        timeout: u64,
    },

    /// List all available providers with capabilities (FR-010)
    List(commands::provider::ProviderListArgs),

    /// Show detailed information for a specific provider (FR-011)
    Info(commands::provider::ProviderInfoArgs),
}

/// Schema conversion actions
#[derive(Subcommand)]
enum SchemaAction {
    /// Convert CLIPS deftemplate to JSON Schema
    ToJson {
        /// Path to .clp file (extracts all deftemplates)
        input: PathBuf,

        /// Output file (stdout if omitted)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Pretty-print JSON output
        #[arg(long, default_value = "true")]
        pretty: bool,
    },

    /// Convert JSON Schema to CLIPS deftemplate
    ToClips {
        /// Path to JSON Schema file
        input: PathBuf,

        /// Output file (stdout if omitted)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Chat {
            provider,
            model,
            message,
            stream,
            temperature,
            max_tokens,
        } => {
            let provider_impl = create_provider(&provider).map_err(|e| format!("{}", e))?;

            let mut request = ChatRequest::new(&model).with_message(Message::user(&message));

            if let Some(temp) = temperature {
                request = request.with_temperature(temp);
            }

            if let Some(tokens) = max_tokens {
                request = request.with_max_tokens(tokens);
            }

            if stream {
                let mut stream = provider_impl.chat_stream(&request).await?;
                while let Some(chunk) = stream.next().await {
                    match chunk {
                        Ok(chunk) => {
                            if !chunk.delta.is_empty() {
                                print!("{}", chunk.delta);
                                std::io::stdout().flush().unwrap_or(());
                            }
                            if chunk.is_final() {
                                println!();
                            }
                        }
                        Err(e) => {
                            eprintln!("Stream error: {}", e);
                            break;
                        }
                    }
                }
            } else {
                let response = provider_impl.chat(&request).await?;
                println!("{}", response.content);
                println!("\nModel: {}", response.model);
                let usage = response.usage.best_available();
                println!(
                    "Tokens: {} prompt + {} completion = {} total",
                    usage.prompt_tokens,
                    usage.completion_tokens,
                    usage.total()
                );
            }
        }

        Commands::Models(args) => {
            if let Err(e) = commands::models::run_models_command(args).await {
                output::OutputWriter::write_error_and_exit(&e);
            }
        }

        Commands::Capabilities {
            provider,
            model,
            format,
        } => {
            let llm_provider = create_provider(&provider)?;
            let (capabilities, source) =
                if let Some(detector) = llm_provider.as_capability_detector() {
                    match detector.get_model_capabilities(&model).await {
                        Ok(caps) => (caps, "detected"),
                        Err(_) => (ModelCapabilities::default(), "default"),
                    }
                } else {
                    (ModelCapabilities::default(), "default")
                };

            match format.to_lowercase().as_str() {
                "json" => {
                    let output = serde_json::json!({
                        "source": source,
                        "capabilities": capabilities,
                    });
                    println!("{}", serde_json::to_string_pretty(&output)?);
                }
                _ => {
                    println!("Capabilities for model '{}':", model);
                    println!("  Source: {}", source);
                    println!("  - Vision mode: {}", capabilities.vision_mode);
                    if capabilities.supports_vision() {
                        println!(
                            "    └─ Supports multiple images: {}",
                            capabilities.supports_multiple_images()
                        );
                    }
                    println!("  - Streaming support: {}", capabilities.supports_streaming);
                    println!(
                        "  - Function calling: {}",
                        capabilities.supports_function_calling
                    );
                }
            }
        }

        Commands::Schema { action } => {
            handle_schema_command(action)?;
        }

        Commands::Pipeline { action } => match action {
            PipelineAction::Run(args) => {
                if let Err(e) = commands::pipeline::run_pipeline_command(
                    commands::pipeline::PipelineRunAction::Run(args),
                )
                .await
                {
                    output::OutputWriter::write_error_and_exit(&e);
                }
            }
            other => {
                handle_pipeline_command(other)?;
            }
        },

        Commands::Activate {
            key,
            trial,
            code,
            accept_eula,
            json,
        } => {
            // FR-001: EULA acceptance gate (legacy activate path)
            if nxuskit_core::eula::read_eula_acceptance().is_none() {
                if accept_eula {
                    if let Err(e) = nxuskit_core::eula::write_eula_acceptance(
                        nxuskit_core::eula::EulaMethod::Flag,
                    ) {
                        output::OutputWriter::write_error_and_exit(
                            &cli_error::CliError::ValidationFailed {
                                message: format!("Failed to record EULA acceptance: {e}"),
                            },
                        );
                    }
                } else if std::io::IsTerminal::is_terminal(&std::io::stdin()) {
                    eprintln!(
                        "By proceeding you accept the nxus.SYSTEMS EULA.\n\
                         Full text: {}\n",
                        nxuskit_core::eula::EULA_URL
                    );
                    eprint!("Accept? (y/N): ");
                    let mut response = String::new();
                    if std::io::stdin().read_line(&mut response).is_ok()
                        && response.trim().eq_ignore_ascii_case("y")
                    {
                        if let Err(e) = nxuskit_core::eula::write_eula_acceptance(
                            nxuskit_core::eula::EulaMethod::Interactive,
                        ) {
                            output::OutputWriter::write_error_and_exit(
                                &cli_error::CliError::ValidationFailed {
                                    message: format!("Failed to record EULA acceptance: {e}"),
                                },
                            );
                        }
                    } else {
                        output::OutputWriter::write_error_and_exit(
                            &cli_error::CliError::ValidationFailed {
                                message: "EULA acceptance required. Pass --accept-eula for non-interactive use.".to_string(),
                            },
                        );
                    }
                } else {
                    output::OutputWriter::write_error_and_exit(
                        &cli_error::CliError::ValidationFailed {
                            message: "EULA acceptance required. Pass --accept-eula for non-interactive environments.".to_string(),
                        },
                    );
                }
            }
            handle_activate(key, trial, code, json);
        }

        Commands::Deactivate { json } => {
            handle_deactivate(json);
        }

        Commands::License { action } => {
            handle_license_command(action);
        }

        Commands::Plugin { action } => {
            handle_plugin_command(action);
        }

        Commands::Provider { action } => match action {
            ProviderAction::Ping {
                provider,
                json: json_output,
                format: fmt,
                timeout: timeout_ms,
            } => {
                if let Err(e) = handle_provider_ping(&provider, json_output, &fmt, timeout_ms).await
                {
                    output::OutputWriter::write_error_and_exit(&e);
                }
            }
            ProviderAction::List(args) => {
                if let Err(e) = commands::provider::run_provider_list(args).await {
                    output::OutputWriter::write_error_and_exit(&e);
                }
            }
            ProviderAction::Info(args) => {
                if let Err(e) = commands::provider::run_provider_info(args).await {
                    output::OutputWriter::write_error_and_exit(&e);
                }
            }
            other => {
                handle_provider_command(other);
            }
        },

        Commands::Examples { action } => {
            let code = examples::handle_examples_command(action);
            if code != 0 {
                process::exit(code);
            }
        }

        Commands::Completions { shell } => {
            use clap::CommandFactory;
            clap_complete::generate(
                shell,
                &mut Cli::command(),
                "nxuskit-cli",
                &mut std::io::stdout(),
            );
        }

        // ── Level 1 Command Dispatch ────────────────────────────────
        Commands::Call(args) => {
            if let Err(e) = commands::call::run_call(args).await {
                output::OutputWriter::write_error_and_exit(&e);
            }
        }
        Commands::Clips { action } => {
            if let Err(e) = commands::clips::run_clips_command(action).await {
                output::OutputWriter::write_error_and_exit(&e);
            }
        }
        Commands::Zen { action } => {
            if let Err(e) = commands::zen::run_zen_command(action).await {
                output::OutputWriter::write_error_and_exit(&e);
            }
        }
        Commands::Solver { action } => {
            if let Err(e) = commands::solver::run_solver_command(action).await {
                output::OutputWriter::write_error_and_exit(&e);
            }
        }
        Commands::Bn { action } => {
            if let Err(e) = commands::bn::run_bn_command(action).await {
                output::OutputWriter::write_error_and_exit(&e);
            }
        }
        Commands::Packet { action } => {
            if let Err(e) = commands::packet::run_packet_command(action).await {
                output::OutputWriter::write_error_and_exit(&e);
            }
        }
        Commands::Artifact { action } => {
            if let Err(e) = commands::artifact::run_artifact_command(action).await {
                output::OutputWriter::write_error_and_exit(&e);
            }
        }
        Commands::ToolLoop { action } => {
            if let Err(e) = commands::tool_loop::run_tool_loop_command(action).await {
                output::OutputWriter::write_error_and_exit(&e);
            }
        }
        Commands::Judge { action } => {
            if let Err(e) = commands::judge::run_judge_command(action).await {
                output::OutputWriter::write_error_and_exit(&e);
            }
        }
        Commands::Branch { action } => {
            if let Err(e) = commands::branch::run_branch_command(action).await {
                output::OutputWriter::write_error_and_exit(&e);
            }
        }
    }

    Ok(())
}

/// Handle pipeline commands
fn handle_pipeline_command(
    action: PipelineAction,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    match action {
        PipelineAction::Validate { input } => {
            // Load pipeline
            let p = pipeline::load_pipeline(&input)
                .map_err(|e| format!("Failed to load pipeline: {}", e))?;

            // Validate
            p.validate()
                .map_err(|e| format!("Validation failed: {}", e))?;

            println!("✓ Pipeline '{}' is valid", p.name);
            println!("  ID: {}", p.id);
            println!("  Stages: {}", p.stages.len());

            // Show stage summary
            let mut stage_types = std::collections::HashMap::new();
            for stage in &p.stages {
                *stage_types.entry(stage.stage_type.to_string()).or_insert(0) += 1;
            }
            println!("  Stage types:");
            for (t, count) in &stage_types {
                println!("    - {}: {}", t, count);
            }
        }

        PipelineAction::Convert {
            input,
            output,
            format,
        } => {
            // Load pipeline
            let p = pipeline::load_pipeline(&input)
                .map_err(|e| format!("Failed to load pipeline: {}", e))?;

            // Determine output format
            let output_format = if let Some(ref f) = format {
                f.to_lowercase()
            } else if let Some(ref out) = output {
                match out.extension().and_then(|e| e.to_str()) {
                    Some("json") => "json".to_string(),
                    Some("yaml") | Some("yml") => "yaml".to_string(),
                    _ => return Err("Cannot determine output format from extension".into()),
                }
            } else {
                // Default to opposite of input
                let input_ext = input.extension().and_then(|e| e.to_str()).unwrap_or("");
                if input_ext == "json" {
                    "yaml".to_string()
                } else {
                    "json".to_string()
                }
            };

            // Convert
            let content = match output_format.as_str() {
                "json" => pipeline::pipeline_to_json(&p)
                    .map_err(|e| format!("Failed to convert to JSON: {}", e))?,
                "yaml" => pipeline::pipeline_to_yaml(&p)
                    .map_err(|e| format!("Failed to convert to YAML: {}", e))?,
                _ => return Err(format!("Unsupported output format: {}", output_format).into()),
            };

            // Output
            if let Some(out_path) = output {
                std::fs::write(&out_path, &content)?;
                println!("✓ Converted pipeline to {}", out_path.display());
            } else {
                print!("{}", content);
            }
        }
        PipelineAction::Run(_) => unreachable!("Run is dispatched in main match"),
    }

    Ok(())
}

/// Handle schema conversion commands
fn handle_schema_command(
    action: SchemaAction,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    use clips_sys::ClipsEnvironment;

    match action {
        SchemaAction::ToJson {
            input,
            output,
            pretty,
        } => {
            // Load the CLIPS file and extract schemas
            let env = ClipsEnvironment::new()
                .map_err(|e| format!("Failed to create CLIPS environment: {}", e))?;

            env.load(&input)
                .map_err(|e| format!("Failed to load '{}': {}", input.display(), e))?;

            let schema = extract_schemas_from_environment(&env);

            if schema.templates.is_empty() {
                eprintln!("No deftemplates found in '{}'", input.display());
                return Ok(());
            }

            let json_schema = templates_to_json_schema(&schema.templates);

            let json_str = if pretty {
                serde_json::to_string_pretty(&json_schema)?
            } else {
                serde_json::to_string(&json_schema)?
            };

            if let Some(out_path) = output {
                std::fs::write(&out_path, &json_str)?;
                println!("Wrote JSON Schema to '{}'", out_path.display());
            } else {
                println!("{}", json_str);
            }
        }

        SchemaAction::ToClips { input, output } => {
            // Read JSON Schema
            let json_str = std::fs::read_to_string(&input)
                .map_err(|e| format!("Failed to read '{}': {}", input.display(), e))?;

            let schema: serde_json::Value = serde_json::from_str(&json_str)
                .map_err(|e| format!("Invalid JSON in '{}': {}", input.display(), e))?;

            // Check if it's a multi-template schema with $defs
            let templates: Vec<serde_json::Value> = if let Some(defs) = schema.get("$defs") {
                defs.as_object()
                    .map(|obj| obj.values().cloned().collect())
                    .unwrap_or_default()
            } else {
                // Single template schema
                vec![schema]
            };

            let mut deftemplates = Vec::new();
            for template_schema in templates {
                match json_schema_to_deftemplate(&template_schema) {
                    Ok(deftemplate) => deftemplates.push(deftemplate),
                    Err(e) => {
                        eprintln!("Warning: Failed to convert template: {}", e);
                    }
                }
            }

            if deftemplates.is_empty() {
                return Err("No templates could be converted".into());
            }

            let output_str = deftemplates.join("\n\n");

            if let Some(out_path) = output {
                std::fs::write(&out_path, &output_str)?;
                println!(
                    "Wrote {} deftemplate(s) to '{}'",
                    deftemplates.len(),
                    out_path.display()
                );
            } else {
                println!("{}", output_str);
            }
        }
    }

    Ok(())
}

/// Handle `nxuskit-cli activate` command
fn handle_activate(key: Option<String>, trial: bool, code: Option<String>, json_output: bool) {
    if trial {
        // Trial activation flow
        if let Some(activation_code) = code {
            // Direct trial activation with code
            match nxuskit_core::license::trial_activate(&activation_code) {
                Ok(result) => {
                    if json_output {
                        let out = serde_json::json!({
                            "success": result.success,
                            "days_remaining": result.days_remaining,
                            "message": result.message,
                        });
                        println!("{}", serde_json::to_string_pretty(&out).unwrap());
                    } else if result.success {
                        println!(
                            "✓ Trial activated. {} days remaining.",
                            result.days_remaining
                        );
                        println!("Purchase Pro: nxus.systems/pricing");
                    } else {
                        eprintln!("✗ Trial activation failed: {}", result.message);
                        process::exit(1);
                    }
                }
                Err(e) => {
                    if json_output {
                        let out = serde_json::json!({ "success": false, "error": e.to_string() });
                        println!("{}", serde_json::to_string_pretty(&out).unwrap());
                    } else {
                        eprintln!("✗ Trial activation failed: {}", e);
                    }
                    process::exit(1);
                }
            }
        } else {
            // Issue a new trial
            match nxuskit_core::license::trial_issue() {
                Ok(result) => {
                    if json_output {
                        let out = serde_json::json!({
                            "success": result.success,
                            "days_remaining": result.days_remaining,
                            "message": result.message,
                        });
                        println!("{}", serde_json::to_string_pretty(&out).unwrap());
                    } else if result.success {
                        println!("✓ Trial issued. {} days remaining.", result.days_remaining);
                        println!("Activate your trial at: nxus.systems/trial/activate");
                        println!("Purchase Pro: nxus.systems/pricing");
                    } else {
                        eprintln!("✗ Trial issuance failed: {}", result.message);
                        process::exit(1);
                    }
                }
                Err(e) => {
                    if json_output {
                        let out = serde_json::json!({ "success": false, "error": e.to_string() });
                        println!("{}", serde_json::to_string_pretty(&out).unwrap());
                    } else {
                        eprintln!("✗ Trial issuance failed: {}", e);
                    }
                    process::exit(1);
                }
            }
        }
    } else {
        // Paid license activation
        let purchase_id = match key {
            Some(k) => k,
            None => {
                eprintln!(
                    "✗ Missing --key <PURCHASE_ID>. Use --key to provide your purchase ID, or --trial for a trial."
                );
                process::exit(1);
            }
        };

        match nxuskit_core::license::activate(&purchase_id) {
            Ok(result) => {
                if json_output {
                    let out = serde_json::json!({
                        "success": result.success,
                        "seats_used": result.seats_used,
                        "seats_total": result.seats_total,
                        "token_path": "~/.nxuskit/license.token",
                    });
                    println!("{}", serde_json::to_string_pretty(&out).unwrap());
                } else if result.success {
                    println!(
                        "✓ Activated. {}/{} machines used.",
                        result.seats_used, result.seats_total
                    );
                    println!("License stored at: ~/.nxuskit/license.token");
                    println!("Deployment token: set NXUSKIT_LICENSE_TOKEN in CI/production");
                } else {
                    eprintln!("✗ Activation failed: {}", result.message);
                    process::exit(1);
                }
            }
            Err(e) => {
                if json_output {
                    let out = serde_json::json!({ "success": false, "error": e.to_string() });
                    println!("{}", serde_json::to_string_pretty(&out).unwrap());
                } else {
                    eprintln!("✗ Activation failed: {}", e);
                }
                process::exit(1);
            }
        }
    }
}

/// Handle `nxuskit-cli deactivate` command
fn handle_deactivate(json_output: bool) {
    match nxuskit_core::license::deactivate() {
        Ok(result) => {
            if json_output {
                let out = serde_json::json!({
                    "success": result.success,
                    "seats_used": result.seats_used,
                    "seats_total": result.seats_total,
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else if result.success {
                println!(
                    "✓ Deactivated. {}/{} machines used.",
                    result.seats_used, result.seats_total
                );
                println!("License removed from: ~/.nxuskit/license.token");
            } else {
                eprintln!("✗ Deactivation failed: {}", result.message);
                process::exit(1);
            }
        }
        Err(e) => {
            if json_output {
                let out = serde_json::json!({ "success": false, "error": e.to_string() });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else {
                eprintln!("✗ Deactivation failed: {}", e);
            }
            process::exit(1);
        }
    }
}

/// Handle `nxuskit-cli plugin` subcommands
fn handle_plugin_command(action: PluginAction) {
    match action {
        PluginAction::Trust {
            mode,
            json: json_output,
        } => {
            if let Some(mode_str) = mode {
                // Set mode
                match nxuskit_core::plugin::TrustMode::from_str_loose(&mode_str) {
                    Some(m) => {
                        nxuskit_core::plugin::set_trust_mode(m);
                        if json_output {
                            let out = serde_json::json!({
                                "trust_mode": m.to_string(),
                            });
                            println!("{}", serde_json::to_string_pretty(&out).unwrap());
                        } else {
                            println!("Plugin trust mode set to: {}", m);
                            if m == nxuskit_core::plugin::TrustMode::AllowUnsigned {
                                println!(
                                    "⚠ Unsigned plugins will be loaded. All unsigned load attempts are audited."
                                );
                            }
                        }
                    }
                    None => {
                        eprintln!(
                            "✗ Invalid trust mode: '{}'. Use 'signed-only' or 'allow-unsigned'.",
                            mode_str
                        );
                        process::exit(1);
                    }
                }
            } else {
                // Show current mode
                let current = nxuskit_core::plugin::get_trust_mode();
                if json_output {
                    let out = serde_json::json!({
                        "trust_mode": current.to_string(),
                    });
                    println!("{}", serde_json::to_string_pretty(&out).unwrap());
                } else {
                    println!(
                        "Plugin trust mode: {} {}",
                        current,
                        if current == nxuskit_core::plugin::TrustMode::SignedOnly {
                            "(default)"
                        } else {
                            ""
                        }
                    );
                    match current {
                        nxuskit_core::plugin::TrustMode::SignedOnly => {
                            println!("Only cryptographically signed plugins will be loaded.");
                        }
                        nxuskit_core::plugin::TrustMode::AllowUnsigned => {
                            println!(
                                "⚠ Unsigned plugins will be loaded. All unsigned load attempts are audited."
                            );
                        }
                    }
                }
            }
        }
    }
}

/// Handle `nxuskit-cli license` subcommands
fn public_signing_key_source_label() -> &'static str {
    let source = nxuskit_core::license::embedded_es256_public_key_source();
    if source.contains("es256-production-pubkey.pem") {
        "embedded-production"
    } else if source.contains('/') || source.contains('\\') {
        "embedded-key"
    } else {
        source
    }
}

fn handle_license_command(action: LicenseAction) {
    match action {
        LicenseAction::Login { json: json_output } => {
            let server_url = nxuskit_core::license::license_server_url();

            // Initiate device code flow — we need the session to print user_code
            let session = match device_auth::device_auth_initiate(&server_url) {
                Ok(s) => s,
                Err(e) => {
                    if json_output {
                        let out = serde_json::json!({ "success": false, "error": e.to_string() });
                        println!("{}", serde_json::to_string_pretty(&out).unwrap());
                    } else {
                        eprintln!("Login failed: {}", e);
                    }
                    process::exit(1);
                }
            };

            // Print the user code and verification URI
            if !json_output {
                println!("Open this URL in your browser:");
                println!("  {}", session.verification_uri);
                println!();
                println!("Enter code: {}", session.user_code);
                println!();
                println!("Waiting for authorization...");
            }

            // Open browser
            if let Err(e) = open::that(&session.verification_uri)
                && !json_output
            {
                eprintln!(
                    "Could not open browser: {}. Please visit the URL above manually.",
                    e
                );
            }

            // Poll for token
            match device_auth::device_auth_poll(&server_url, &session) {
                Ok(auth_session) => {
                    // Store token
                    if let Err(e) = auth_token::write_auth_token(&auth_session) {
                        if json_output {
                            let out =
                                serde_json::json!({ "success": false, "error": e.to_string() });
                            println!("{}", serde_json::to_string_pretty(&out).unwrap());
                        } else {
                            eprintln!("Failed to store auth token: {}", e);
                        }
                        process::exit(1);
                    }

                    if json_output {
                        let out = serde_json::json!({
                            "success": true,
                            "email": auth_session.user_email,
                            "token_path": auth_token::auth_token_path().display().to_string(),
                        });
                        println!("{}", serde_json::to_string_pretty(&out).unwrap());
                    } else {
                        let email_display = if auth_session.user_email.is_empty() {
                            "authenticated".to_string()
                        } else {
                            auth_session.user_email
                        };
                        println!("Logged in as: {}", email_display);
                        println!(
                            "Token stored at: {}",
                            auth_token::auth_token_path().display()
                        );
                    }
                }
                Err(e) => {
                    if json_output {
                        let out = serde_json::json!({ "success": false, "error": e.to_string() });
                        println!("{}", serde_json::to_string_pretty(&out).unwrap());
                    } else {
                        eprintln!("Login failed: {}", e);
                    }
                    process::exit(1);
                }
            }
        }

        LicenseAction::Logout { json: json_output } => match device_auth::device_auth_logout() {
            Ok(()) => {
                if json_output {
                    let out = serde_json::json!({ "success": true });
                    println!("{}", serde_json::to_string_pretty(&out).unwrap());
                } else {
                    println!("Logged out.");
                }
            }
            Err(e) => {
                if json_output {
                    let out = serde_json::json!({ "success": false, "error": e.to_string() });
                    println!("{}", serde_json::to_string_pretty(&out).unwrap());
                } else {
                    eprintln!("Logout failed: {}", e);
                }
                process::exit(1);
            }
        },

        LicenseAction::Status { json: json_output } => {
            // Auth status
            let auth_status = auth_token::read_auth_token();

            // License/entitlement status
            let info = nxuskit_core::entitlement::entitlement_info(None);
            let endpoint_override = std::env::var("NXUSKIT_LICENSE_SERVER").ok();
            let environment_override = std::env::var("NXUSKIT_LICENSE_ENVIRONMENT").ok();
            let pro_engines_compiled = cfg!(feature = "pro-engines");
            let effective_edition = info
                .get("effective_edition")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let pro_engine_warning =
                matches!(effective_edition, "pro" | "enterprise") && !pro_engines_compiled;
            let cli_diagnostics = serde_json::json!({
                "pro_engines_compiled": pro_engines_compiled,
                "warnings": if pro_engine_warning {
                    vec!["valid Pro entitlement is present, but this CLI was built without Pro solver/ZEN engine modules"]
                } else {
                    Vec::<&str>::new()
                },
            });
            let licensing_diagnostics = serde_json::json!({
                "environment": nxuskit_core::license::license_environment(),
                "default_environment": nxuskit_core::license::default_license_environment(),
                "environment_override": environment_override,
                "endpoint": {
                    "url": nxuskit_core::license::license_server_url(),
                    "default": nxuskit_core::license::default_license_server_url(),
                    "override": endpoint_override,
                },
                "signing_key": {
                    "kid": nxuskit_core::license::embedded_es256_public_key_kid(),
                    "source": public_signing_key_source_label(),
                }
            });

            if json_output {
                let auth_json = match &auth_status {
                    Some(session) => serde_json::json!({
                        "authenticated": !session.is_expired(),
                        "email": session.user_email,
                        "expired": session.is_expired(),
                        "token_path": auth_token::auth_token_path().display().to_string(),
                    }),
                    None => serde_json::json!({
                        "authenticated": false,
                    }),
                };
                let out = serde_json::json!({
                    "auth": auth_json,
                    "license": info,
                    "cli": cli_diagnostics,
                    "licensing": licensing_diagnostics,
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else {
                // Auth section
                match &auth_status {
                    Some(session) if !session.is_expired() => {
                        let email_display = if session.user_email.is_empty() {
                            "authenticated".to_string()
                        } else {
                            session.user_email.clone()
                        };
                        println!("Auth: {} (logged in)", email_display);
                    }
                    Some(_) => {
                        println!("Auth: session expired (run `nxuskit-cli license login`)");
                    }
                    None => {
                        println!("Auth: not logged in");
                    }
                }

                // License section (existing logic)
                let edition = info
                    .get("effective_edition")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let status = info
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");

                // Token details
                if let Some(token) = info.get("token") {
                    let token_type = token.get("type").and_then(|v| v.as_str()).unwrap_or("none");
                    let source = token
                        .get("source")
                        .and_then(|v| v.as_str())
                        .unwrap_or("none");

                    let type_label = match token_type {
                        "developer" => "Pro Developer",
                        "trial" => "Trial",
                        "deployment" => "Deployment",
                        _ => "None",
                    };

                    if let Some(days) = token.get("days_remaining").and_then(|v| v.as_i64()) {
                        println!("License: {} ({} days remaining)", type_label, days);
                    } else {
                        println!("License: {}", type_label);
                    }
                    println!("Source: {}", source);
                } else {
                    println!("License: None");
                }

                // Machine ID
                let local_machine = match nxuskit_core::machine_id::get_machine_fingerprint() {
                    Ok(mid) => {
                        println!("Machine: {}", mid);
                        Some(mid)
                    }
                    Err(_) => {
                        println!("Machine: unavailable");
                        None
                    }
                };

                println!("Edition: {} ({})", edition, status);
                if pro_engine_warning {
                    println!(
                        "CLI warning: valid Pro entitlement is present, but this CLI was built without Pro solver/ZEN engine modules"
                    );
                }
                println!(
                    "Licensing endpoint: {}",
                    nxuskit_core::license::license_server_url()
                );
                println!(
                    "Licensing environment: {}",
                    nxuskit_core::license::license_environment()
                );
                println!(
                    "Signing key: {} ({})",
                    nxuskit_core::license::embedded_es256_public_key_kid(),
                    public_signing_key_source_label()
                );

                // Token diagnostics (when invalid)
                if status == "invalid"
                    && let Some(token) = info.get("token")
                {
                    if let Some(err) = token.get("error").and_then(|v| v.as_str()) {
                        println!();
                        println!("Token issue: {}", err);
                    }
                    if let Some(token_mid) = token.get("token_machine_id").and_then(|v| v.as_str())
                        && let Some(ref local) = local_machine
                        && token_mid != local
                    {
                        println!("  Token bound to: {}", token_mid);
                        println!("  This machine:   {}", local);
                        println!(
                            "  -> Machine ID mismatch. Token was issued for a different machine."
                        );
                        println!("  -> Fix: delete ~/.nxuskit/license.token, then:");
                        println!("     nxuskit-cli license login");
                        println!("     nxuskit-cli license activate --trial");
                    }
                    if let Some(activated) = token.get("token_activated").and_then(|v| v.as_bool())
                        && !activated
                    {
                        println!("  Token not yet activated (within 7-day grace period).");
                    }
                }

                // Feature availability
                if let Some(features) = info.get("features").and_then(|v| v.as_object()) {
                    println!();
                    println!("Features:");
                    for (name, details) in features {
                        let available = details
                            .get("available")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        let requires = details
                            .get("requires")
                            .and_then(|v| v.as_str())
                            .unwrap_or("oss");
                        let icon = if available { "✓" } else { "✗" };
                        let tier = match requires {
                            "pro" => "(Pro)",
                            _ => "(Community)",
                        };
                        println!("  {} {:<20} {}", icon, name, tier);
                    }
                }
            }
        }

        LicenseAction::Activate {
            key,
            trial,
            accept_eula,
            json: json_output,
        } => {
            // FR-001: EULA acceptance gate — check before any activation
            if nxuskit_core::eula::read_eula_acceptance().is_none() {
                if accept_eula {
                    // Non-interactive acceptance via --accept-eula flag
                    if let Err(e) = nxuskit_core::eula::write_eula_acceptance(
                        nxuskit_core::eula::EulaMethod::Flag,
                    ) {
                        output::OutputWriter::write_error_and_exit(
                            &cli_error::CliError::ValidationFailed {
                                message: format!("Failed to record EULA acceptance: {e}"),
                            },
                        );
                    }
                } else if std::io::IsTerminal::is_terminal(&std::io::stdin()) {
                    // Interactive TTY prompt
                    eprintln!(
                        "By proceeding you accept the nxus.SYSTEMS EULA.\n\
                         Full text: {}\n",
                        nxuskit_core::eula::EULA_URL
                    );
                    eprint!("Accept? (y/N): ");
                    let mut response = String::new();
                    if std::io::stdin().read_line(&mut response).is_ok()
                        && response.trim().eq_ignore_ascii_case("y")
                    {
                        if let Err(e) = nxuskit_core::eula::write_eula_acceptance(
                            nxuskit_core::eula::EulaMethod::Interactive,
                        ) {
                            output::OutputWriter::write_error_and_exit(
                                &cli_error::CliError::ValidationFailed {
                                    message: format!("Failed to record EULA acceptance: {e}"),
                                },
                            );
                        }
                    } else {
                        output::OutputWriter::write_error_and_exit(
                            &cli_error::CliError::ValidationFailed {
                                message: "EULA acceptance required. Pass --accept-eula for non-interactive use.".to_string(),
                            },
                        );
                    }
                } else {
                    // Non-TTY without --accept-eula flag
                    output::OutputWriter::write_error_and_exit(
                        &cli_error::CliError::ValidationFailed {
                            message: "EULA acceptance required. Pass --accept-eula for non-interactive environments.".to_string(),
                        },
                    );
                }
            }

            let server_url = nxuskit_core::license::license_server_url();

            // Ensure authenticated before activation
            match device_auth::ensure_authenticated(&server_url) {
                Ok(_bearer_token) => {}
                Err(e) => {
                    if json_output {
                        let out = serde_json::json!({ "success": false, "error": format!("Authentication required: {}", e) });
                        println!("{}", serde_json::to_string_pretty(&out).unwrap());
                    } else {
                        eprintln!(
                            "Authentication required. Run `nxuskit-cli license login` first."
                        );
                        eprintln!("Error: {}", e);
                    }
                    process::exit(1);
                }
            }

            if trial {
                // Trial activation
                match nxuskit_core::license::trial_issue() {
                    Ok(result) => {
                        if json_output {
                            let out = serde_json::json!({
                                "success": result.success,
                                "days_remaining": result.days_remaining,
                                "message": result.message,
                            });
                            println!("{}", serde_json::to_string_pretty(&out).unwrap());
                        } else if result.success {
                            println!("Trial activated. {} days remaining.", result.days_remaining);
                        } else {
                            eprintln!("Trial activation failed: {}", result.message);
                            process::exit(1);
                        }
                    }
                    Err(e) => {
                        if json_output {
                            let out =
                                serde_json::json!({ "success": false, "error": e.to_string() });
                            println!("{}", serde_json::to_string_pretty(&out).unwrap());
                        } else {
                            eprintln!("Trial activation failed: {}", e);
                        }
                        process::exit(1);
                    }
                }
            } else if let Some(purchase_id) = key {
                // Paid license activation
                match nxuskit_core::license::activate(&purchase_id) {
                    Ok(result) => {
                        if json_output {
                            let out = serde_json::json!({
                                "success": result.success,
                                "seats_used": result.seats_used,
                                "seats_total": result.seats_total,
                                "token_path": "~/.nxuskit/license.token",
                            });
                            println!("{}", serde_json::to_string_pretty(&out).unwrap());
                        } else if result.success {
                            println!(
                                "Activated. {}/{} machines used.",
                                result.seats_used, result.seats_total
                            );
                            println!("License stored at: ~/.nxuskit/license.token");
                        } else {
                            eprintln!("Activation failed: {}", result.message);
                            process::exit(1);
                        }
                    }
                    Err(e) => {
                        if json_output {
                            let out =
                                serde_json::json!({ "success": false, "error": e.to_string() });
                            println!("{}", serde_json::to_string_pretty(&out).unwrap());
                        } else {
                            eprintln!("Activation failed: {}", e);
                        }
                        process::exit(1);
                    }
                }
            } else {
                eprintln!(
                    "Provide --key <PURCHASE_ID> for paid activation or --trial for a trial."
                );
                process::exit(1);
            }
        }

        LicenseAction::Sync { json: json_output } => {
            let machine_id = match nxuskit_core::machine_id::get_machine_fingerprint() {
                Ok(id) => id,
                Err(e) => {
                    if json_output {
                        let out = serde_json::json!({ "success": false, "error": e.to_string() });
                        println!("{}", serde_json::to_string_pretty(&out).unwrap());
                    } else {
                        eprintln!("Failed to get machine ID: {}", e);
                    }
                    process::exit(1);
                }
            };

            let sync_result = match nxuskit_core::license::refresh_cached_license() {
                Ok(resolution) => Ok({
                    let days_remaining = resolution
                        .claims
                        .as_ref()
                        .and_then(|claims| claims.days_remaining())
                        .unwrap_or(0)
                        .max(0) as u32;
                    nxuskit_core::license_types::TrialIssuanceResult {
                        success: resolution.valid,
                        token: resolution.raw_token,
                        days_remaining,
                        message: "Cached license token refreshed.".to_string(),
                        error: None,
                    }
                }),
                Err(nxuskit_core::license::LicenseError::ValidationFailed(message)) => {
                    if message == "No cached license token found" {
                        nxuskit_core::license::trial_fetch(&machine_id)
                    } else {
                        Err(nxuskit_core::license::LicenseError::ValidationFailed(
                            message,
                        ))
                    }
                }
                Err(error) => Err(error),
            };

            match sync_result {
                Ok(result) => {
                    if json_output {
                        let out = serde_json::json!({
                            "success": result.success,
                            "days_remaining": result.days_remaining,
                            "message": result.message,
                        });
                        println!("{}", serde_json::to_string_pretty(&out).unwrap());
                    } else if result.success {
                        println!(
                            "License token synced. {} days remaining.",
                            result.days_remaining
                        );
                    } else {
                        eprintln!("Sync failed: {}", result.message);
                        process::exit(1);
                    }
                }
                Err(e) => {
                    if json_output {
                        let out = serde_json::json!({ "success": false, "error": e.to_string() });
                        println!("{}", serde_json::to_string_pretty(&out).unwrap());
                    } else {
                        eprintln!("Sync failed: {}", e);
                    }
                    process::exit(1);
                }
            }
        }
    }
}

pub(crate) fn create_provider(
    provider: &str,
) -> std::result::Result<Box<dyn LLMProvider>, Box<dyn std::error::Error>> {
    match provider.to_lowercase().as_str() {
        // Major providers
        "claude" => {
            let api_key = env::var("ANTHROPIC_API_KEY")
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            Ok(Box::new(
                ClaudeProvider::builder().api_key(api_key).build()?,
            ))
        }
        "openai" => {
            let api_key = env::var("OPENAI_API_KEY")
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            Ok(Box::new(
                OpenAIProvider::builder().api_key(api_key).build()?,
            ))
        }
        "ollama" => Ok(Box::new(OllamaProvider::builder().build()?)),

        // Local providers
        "lmstudio" | "lm-studio" => Ok(Box::new(LmStudioProvider::builder().build()?)),
        "loopback" => Ok(Box::new(LoopbackProvider::new())),

        // OpenAI-compatible cloud providers
        "fireworks" => {
            let api_key = env::var("FIREWORKS_API_KEY")
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            Ok(Box::new(
                FireworksProvider::builder().api_key(api_key).build()?,
            ))
        }
        "xai" => {
            let api_key =
                env::var("XAI_API_KEY").map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            Ok(Box::new(XaiProvider::builder().api_key(api_key).build()?))
        }
        "groq" => {
            let api_key =
                env::var("GROQ_API_KEY").map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            Ok(Box::new(GroqProvider::builder().api_key(api_key).build()?))
        }
        "mistral" => {
            let api_key = env::var("MISTRAL_API_KEY")
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            Ok(Box::new(MistralProvider::new(api_key)))
        }
        "openrouter" => {
            let api_key = env::var("OPENROUTER_API_KEY")
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            Ok(Box::new(OpenRouterProvider::new(api_key)))
        }
        "perplexity" => {
            let api_key = env::var("PERPLEXITY_API_KEY")
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            Ok(Box::new(
                PerplexityProvider::builder().api_key(api_key).build()?,
            ))
        }
        "together" => {
            let api_key = env::var("TOGETHER_API_KEY")
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            Ok(Box::new(
                TogetherProvider::builder().api_key(api_key).build()?,
            ))
        }

        // Test/debug provider
        "mock" => Ok(Box::new(MockProvider::default())),

        // Expert system provider
        "clips" => {
            let rules_dir = env::var("CLIPS_RULES_DIR").unwrap_or_else(|_| ".".to_string());
            Ok(Box::new(
                ClipsProvider::builder()
                    .rules_directory(rules_dir)
                    .build()?,
            ))
        }

        // MCP provider
        "mcp" => {
            let server_uri = env::var("MCP_SERVER").map_err(|_| {
                "MCP_SERVER environment variable not set. Set it to your MCP server URI (e.g., stdio://mcp-server)"
            })?;
            let auth_token = env::var("MCP_TOKEN").ok();

            let mut builder = McpProvider::builder().server_uri(server_uri);
            if let Some(token) = auth_token {
                builder = builder.auth_token(token);
            }
            Ok(Box::new(builder.build()?))
        }
        _ => Err(format!(
            "Unknown provider: '{}'. Supported providers:\n  \
             - claude (ANTHROPIC_API_KEY)\n  \
             - openai (OPENAI_API_KEY)\n  \
             - ollama (local, no key required)\n  \
             - lmstudio (local, no key required)\n  \
             - loopback (echo, for testing)\n  \
             - fireworks (FIREWORKS_API_KEY)\n  \
             - xai (XAI_API_KEY)\n  \
             - groq (GROQ_API_KEY)\n  \
             - mistral (MISTRAL_API_KEY)\n  \
             - openrouter (OPENROUTER_API_KEY)\n  \
             - perplexity (PERPLEXITY_API_KEY)\n  \
             - together (TOGETHER_API_KEY)\n  \
             - mock (testing)\n  \
             - clips (expert system, CLIPS_RULES_DIR)\n  \
             - mcp (MCP_SERVER, MCP_TOKEN)",
            provider
        )
        .into()),
    }
}

/// Handle `provider ping` — check reachability and latency.
async fn handle_provider_ping(
    provider_name: &str,
    json_output: bool,
    format: &str,
    timeout_ms: u64,
) -> std::result::Result<(), cli_error::CliError> {
    let provider_impl =
        create_provider(provider_name).map_err(|e| cli_error::CliError::ProviderError {
            message: format!("{e}"),
        })?;

    let start = std::time::Instant::now();
    let timeout_dur = std::time::Duration::from_millis(timeout_ms);

    let ping_result = match tokio::time::timeout(timeout_dur, provider_impl.list_models()).await {
        Ok(Ok(models)) => {
            let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
            serde_json::json!({
                "provider": provider_name,
                "reachable": true,
                "latency_ms": (latency_ms * 100.0).round() / 100.0,
                "models_found": models.len(),
            })
        }
        Ok(Err(e)) => {
            let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
            serde_json::json!({
                "provider": provider_name,
                "reachable": false,
                "latency_ms": (latency_ms * 100.0).round() / 100.0,
                "error": format!("{e}"),
            })
        }
        Err(_) => {
            serde_json::json!({
                "provider": provider_name,
                "reachable": false,
                "error": format!("timeout after {timeout_ms}ms"),
            })
        }
    };

    let reachable = ping_result["reachable"].as_bool().unwrap_or(false);
    let use_json = json_output || format == "json";

    if use_json {
        println!("{}", serde_json::to_string_pretty(&ping_result).unwrap());
    } else {
        // Text format
        if reachable {
            println!(
                "{}: reachable ({}ms, {} models)",
                provider_name, ping_result["latency_ms"], ping_result["models_found"]
            );
        } else {
            let err = ping_result["error"].as_str().unwrap_or("unknown error");
            println!("{}: unreachable ({})", provider_name, err);
        }
    }

    if !reachable {
        process::exit(1);
    }

    Ok(())
}

fn handle_provider_command(action: ProviderAction) {
    match action {
        ProviderAction::Login {
            provider,
            timeout,
            json: json_output,
        } => {
            let result = nxuskit_core::oauth::oauth_start(&provider, timeout);
            match result {
                Ok(oauth_result) => {
                    if json_output {
                        println!("{}", serde_json::to_string_pretty(&oauth_result).unwrap());
                    } else {
                        println!("✓ {}", oauth_result.message);
                    }
                }
                Err(e) => {
                    if json_output {
                        let err_json = serde_json::json!({
                            "success": false,
                            "provider_id": provider,
                            "error": e.to_string(),
                        });
                        println!("{}", serde_json::to_string_pretty(&err_json).unwrap());
                    } else {
                        eprintln!("Error: {e}");
                    }
                    process::exit(1);
                }
            }
        }

        ProviderAction::Status {
            provider,
            json: json_output,
        } => {
            if let Some(provider_id) = provider {
                match nxuskit_core::oauth::oauth_status(&provider_id) {
                    Ok(status) => {
                        if json_output {
                            println!("{}", serde_json::to_string_pretty(&status).unwrap());
                        } else {
                            let auth_str = if status.authenticated {
                                "authenticated"
                            } else {
                                "not authenticated"
                            };
                            println!("{}: {}", status.provider_id, auth_str);
                        }
                    }
                    Err(e) => {
                        eprintln!("Error: {e}");
                        process::exit(1);
                    }
                }
            } else {
                // Show all provider auth status
                let statuses = nxuskit_core::auth::status_all();
                if json_output {
                    println!("{}", serde_json::to_string_pretty(&statuses).unwrap());
                } else {
                    for s in &statuses {
                        let status_str = match &s.status {
                            nxuskit_core::auth::AuthStatusKind::AuthenticatedEnv => {
                                format!("✓ env ({})", s.masked_preview.as_deref().unwrap_or(""))
                            }
                            nxuskit_core::auth::AuthStatusKind::AuthenticatedStore => {
                                format!("✓ store ({})", s.masked_preview.as_deref().unwrap_or(""))
                            }
                            nxuskit_core::auth::AuthStatusKind::AuthenticatedOAuth => {
                                "✓ oauth".to_string()
                            }
                            nxuskit_core::auth::AuthStatusKind::AuthenticatedExplicit => {
                                "✓ explicit".to_string()
                            }
                            nxuskit_core::auth::AuthStatusKind::NotAuthenticated => {
                                "✗ not configured".to_string()
                            }
                            nxuskit_core::auth::AuthStatusKind::NotRequired => {
                                "- not required".to_string()
                            }
                        };
                        let oauth_tag = if s.oauth_capable { " [oauth]" } else { "" };
                        println!("  {:<14} {}{}", s.provider_id, status_str, oauth_tag);
                    }
                }
            }
        }
        // These are handled in the async dispatch before this function is called
        ProviderAction::Ping { .. } => unreachable!("Ping is dispatched separately"),
        ProviderAction::List(_) => unreachable!("List is dispatched separately"),
        ProviderAction::Info(_) => unreachable!("Info is dispatched separately"),
    }
}
