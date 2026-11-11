//! v1.1 `pipeline hash` command.

use clap::Args;
use nxuskit::contract::v1_1::{PipelineDefinition, load_pipeline_definition_json};
use serde::Serialize;

use crate::cli_error::CliError;
use crate::output::OutputWriter;

#[derive(Debug, Args)]
pub struct PipelineHashArgs {
    /// Input file path or `-` for stdin.
    #[arg(short, long)]
    pub input: String,

    /// Output format. v1.1.0 RC supports `json`.
    #[arg(short, long, default_value = "json")]
    pub format: String,
}

#[derive(Debug, Serialize)]
struct HashReport<'a> {
    contract_version: &'a str,
    kind: &'a str,
    hash: &'a str,
}

pub fn run_hash_command(args: PipelineHashArgs) -> Result<(), CliError> {
    if args.format != "json" {
        return Err(CliError::ValidationFailed {
            message: "pipeline hash supports --format json in v1.1.0 RC".to_owned(),
        });
    }
    let raw_input = OutputWriter::read_input(&args.input)?;
    let def: PipelineDefinition =
        load_pipeline_definition_json(&raw_input).map_err(|e| CliError::ParseError {
            message: format!("v1.1 PipelineDefinition load error: {e}"),
        })?;
    let hash = def.canonical_hash().map_err(|e| CliError::ProviderError {
        message: format!("hash pipeline: {e}"),
    })?;
    let report = HashReport {
        contract_version: nxuskit::contract::v1_1::CONTRACT_VERSION,
        kind: "pipeline_definition",
        hash: hash.as_str(),
    };
    let body = serde_json::to_string(&report).map_err(|e| CliError::ProviderError {
        message: format!("serialize hash report: {e}"),
    })?;
    println!("{body}");
    Ok(())
}
