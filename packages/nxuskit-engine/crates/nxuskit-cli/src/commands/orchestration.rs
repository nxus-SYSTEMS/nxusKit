//! v1.1 declaration-only orchestration commands.

use clap::{Args, Subcommand};
use nxuskit::contract::v1_1::{
    GraphProjection, OrchestrationDefinition, load_orchestration_definition_json,
};
use serde::Serialize;

use crate::cli_error::CliError;
use crate::output::OutputWriter;

#[derive(Debug, Subcommand)]
pub enum OrchestrationAction {
    /// Validate a declaration-only orchestration definition.
    Validate(OrchestrationInputArgs),
    /// Emit canonical JSON for a declaration-only orchestration definition.
    Normalize(OrchestrationInputArgs),
    /// Emit the canonical hash for a declaration-only orchestration definition.
    Hash(OrchestrationInputArgs),
    /// Emit a public graph projection or Mermaid rendering.
    Graph(OrchestrationGraphArgs),
}

#[derive(Debug, Args)]
pub struct OrchestrationInputArgs {
    /// Input file path or `-` for stdin.
    #[arg(short, long)]
    pub input: String,

    /// Output format. v1.1.0 RC supports `json`.
    #[arg(short, long, default_value = "json")]
    pub format: String,
}

#[derive(Debug, Args)]
pub struct OrchestrationGraphArgs {
    /// Input file path or `-` for stdin.
    #[arg(short, long)]
    pub input: String,

    /// Output format. v1.1.0 RC supports `json`.
    #[arg(short, long, default_value = "json")]
    pub format: String,

    /// Render Mermaid from the same public graph projection.
    #[arg(long, default_value_t = false)]
    pub mermaid: bool,
}

#[derive(Debug, Serialize)]
struct ValidationReport<'a> {
    contract_version: &'a str,
    outcome: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}

#[derive(Debug, Serialize)]
struct HashReport<'a> {
    contract_version: &'a str,
    kind: &'a str,
    hash: &'a str,
}

pub fn run_orchestration_command(action: OrchestrationAction) -> Result<(), CliError> {
    match action {
        OrchestrationAction::Validate(args) => run_validate(args),
        OrchestrationAction::Normalize(args) => run_normalize(args),
        OrchestrationAction::Hash(args) => run_hash(args),
        OrchestrationAction::Graph(args) => run_graph(args),
    }
}

fn load(input: &str) -> Result<OrchestrationDefinition, CliError> {
    let raw_input = OutputWriter::read_input(input)?;
    load_orchestration_definition_json(&raw_input).map_err(|e| CliError::ParseError {
        message: format!("v1.1 OrchestrationDefinition load error: {e}"),
    })
}

fn ensure_json(format: &str, command: &str) -> Result<(), CliError> {
    if format == "json" {
        Ok(())
    } else {
        Err(CliError::ValidationFailed {
            message: format!("orchestration {command} supports --format json in v1.1.0 RC"),
        })
    }
}

fn run_validate(args: OrchestrationInputArgs) -> Result<(), CliError> {
    ensure_json(&args.format, "validate")?;
    let raw_input = OutputWriter::read_input(&args.input)?;
    let report = match load_orchestration_definition_json(&raw_input) {
        Ok(def) => match def.validate() {
            Ok(()) => ValidationReport {
                contract_version: nxuskit::contract::v1_1::CONTRACT_VERSION,
                outcome: "valid",
                detail: None,
            },
            Err(err) => ValidationReport {
                contract_version: nxuskit::contract::v1_1::CONTRACT_VERSION,
                outcome: "invalid",
                detail: Some(err.to_string()),
            },
        },
        Err(err) => ValidationReport {
            contract_version: nxuskit::contract::v1_1::CONTRACT_VERSION,
            outcome: "invalid",
            detail: Some(err.to_string()),
        },
    };
    let success = report.outcome == "valid";
    let body = serde_json::to_string(&report).map_err(|e| CliError::ProviderError {
        message: format!("serialize validation report: {e}"),
    })?;
    println!("{body}");
    if success {
        Ok(())
    } else {
        Err(CliError::ValidationFailed {
            message: report
                .detail
                .unwrap_or_else(|| "invalid orchestration".to_owned()),
        })
    }
}

fn run_normalize(args: OrchestrationInputArgs) -> Result<(), CliError> {
    ensure_json(&args.format, "normalize")?;
    let def = load(&args.input)?;
    let body = def
        .export_canonical_json()
        .map_err(|e| CliError::ProviderError {
            message: format!("normalize orchestration: {e}"),
        })?;
    println!("{body}");
    Ok(())
}

fn run_hash(args: OrchestrationInputArgs) -> Result<(), CliError> {
    ensure_json(&args.format, "hash")?;
    let def = load(&args.input)?;
    let hash = def.canonical_hash().map_err(|e| CliError::ProviderError {
        message: format!("hash orchestration: {e}"),
    })?;
    let report = HashReport {
        contract_version: nxuskit::contract::v1_1::CONTRACT_VERSION,
        kind: "orchestration_definition",
        hash: hash.as_str(),
    };
    let body = serde_json::to_string(&report).map_err(|e| CliError::ProviderError {
        message: format!("serialize hash report: {e}"),
    })?;
    println!("{body}");
    Ok(())
}

fn run_graph(args: OrchestrationGraphArgs) -> Result<(), CliError> {
    if !args.mermaid {
        ensure_json(&args.format, "graph")?;
    }
    let def = load(&args.input)?;
    let graph = GraphProjection::from_orchestration(&def);
    if args.mermaid {
        print!("{}", graph.render_mermaid());
    } else {
        let body = serde_json::to_string(&graph).map_err(|e| CliError::ProviderError {
            message: format!("serialize graph projection: {e}"),
        })?;
        println!("{body}");
    }
    Ok(())
}
