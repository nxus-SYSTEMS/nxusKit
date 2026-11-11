//! v1.1 `pipeline normalize` command.

use clap::Args;
use nxuskit::contract::v1_1::{PipelineDefinition, load_pipeline_definition_json};

use crate::cli_error::CliError;
use crate::output::OutputWriter;

#[derive(Debug, Args)]
pub struct PipelineNormalizeArgs {
    /// Input file path or `-` for stdin.
    #[arg(short, long)]
    pub input: String,

    /// Output format. v1.1.0 RC supports `json`.
    #[arg(short, long, default_value = "json")]
    pub format: String,
}

pub fn run_normalize_command(args: PipelineNormalizeArgs) -> Result<(), CliError> {
    if args.format != "json" {
        return Err(CliError::ValidationFailed {
            message: "pipeline normalize supports --format json in v1.1.0 RC".to_owned(),
        });
    }
    let raw_input = OutputWriter::read_input(&args.input)?;
    let def: PipelineDefinition =
        load_pipeline_definition_json(&raw_input).map_err(|e| CliError::ParseError {
            message: format!("v1.1 PipelineDefinition load error: {e}"),
        })?;
    let body = def
        .export_canonical_json()
        .map_err(|e| CliError::ProviderError {
            message: format!("normalize pipeline: {e}"),
        })?;
    println!("{body}");
    Ok(())
}
