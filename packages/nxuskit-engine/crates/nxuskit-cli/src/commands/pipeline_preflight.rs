//! v1.1 contract-native `pipeline preflight` driver (Feature 106 / US5).

use clap::Args;
use nxuskit::contract::v1_1::{PipelineDefinition, load_pipeline_definition_json};
use nxuskit_engine::runner::assess_pipeline_resources;

use crate::cli_error::CliError;
use crate::output::OutputWriter;

#[derive(Debug, Args)]
pub struct PipelinePreflightArgs {
    /// Input file path or `-` for stdin.
    #[arg(short, long)]
    pub input: String,

    /// Output format. v1.1.0 RC supports `json`.
    #[arg(short, long, default_value = "json")]
    pub format: String,
}

pub async fn run_preflight_command(args: PipelinePreflightArgs) -> Result<(), CliError> {
    if args.format != "json" {
        return Err(CliError::ValidationFailed {
            message: "pipeline preflight supports --format json in v1.1.0 RC".to_owned(),
        });
    }

    let raw_input = OutputWriter::read_input(&args.input)?;
    let def: PipelineDefinition =
        load_pipeline_definition_json(&raw_input).map_err(|e| CliError::ParseError {
            message: format!("v1.1 PipelineDefinition load error: {e}"),
        })?;
    let advisory = assess_pipeline_resources(&def);
    let body = serde_json::to_string(&advisory).map_err(|e| CliError::ProviderError {
        message: format!("serialize resource advisory: {e}"),
    })?;
    println!("{body}");
    Ok(())
}
