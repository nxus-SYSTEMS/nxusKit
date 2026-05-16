//! `nxuskit-cli packet validate` — JSON Schema validation (FR-009).

use clap::{Args, Subcommand};
use serde::Serialize;

use crate::cli_error::CliError;
use crate::envelope::TraceFields;
use crate::output::{OutputFormat, OutputWriter};

#[derive(Debug, Subcommand)]
pub enum PacketAction {
    /// Validate a packet against a JSON Schema
    Validate(PacketValidateArgs),
}

#[derive(Debug, Args)]
pub struct PacketValidateArgs {
    /// Packet data file (JSON) or `-` for stdin
    #[arg(short, long)]
    pub input: String,

    /// JSON Schema file for validation
    #[arg(long)]
    pub schema: String,

    #[arg(short, long, default_value = "json")]
    pub format: String,

    #[arg(short, long, default_value_t = false)]
    pub quiet: bool,

    #[arg(short, long)]
    pub output: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PacketValidateResult {
    pub valid: bool,
    pub errors: Vec<ValidationError>,
}

#[derive(Debug, Serialize)]
pub struct ValidationError {
    pub path: String,
    pub message: String,
    pub keyword: String,
}

pub async fn run_packet_command(action: PacketAction) -> Result<(), CliError> {
    match action {
        PacketAction::Validate(args) => run_packet_validate(args).await,
    }
}

async fn run_packet_validate(args: PacketValidateArgs) -> Result<(), CliError> {
    let format = OutputFormat::parse(&args.format)?;

    // Read packet
    let raw_input = OutputWriter::read_input(&args.input)?;
    let packet: serde_json::Value =
        serde_json::from_str(&raw_input).map_err(|e| CliError::ParseError {
            message: format!("Invalid packet JSON: {e}"),
        })?;

    // Read schema
    if !std::path::Path::new(&args.schema).exists() {
        return Err(CliError::SchemaNotFound {
            path: args.schema.clone(),
        });
    }
    let schema_str = std::fs::read_to_string(&args.schema).map_err(|e| CliError::ParseError {
        message: format!("Failed to read schema '{}': {e}", args.schema),
    })?;
    let schema: serde_json::Value =
        serde_json::from_str(&schema_str).map_err(|e| CliError::ParseError {
            message: format!("Invalid schema JSON: {e}"),
        })?;

    let trace = TraceFields::new("packet_validate", &raw_input, None, None);
    let writer = OutputWriter::new(format, args.quiet, args.output);

    // Validate using jsonschema
    let validator = jsonschema::validator_for(&schema).map_err(|e| CliError::ParseError {
        message: format!("Invalid JSON Schema: {e}"),
    })?;

    let errors: Vec<ValidationError> = validator
        .iter_errors(&packet)
        .map(|e| ValidationError {
            path: e.instance_path().to_string(),
            message: e.to_string(),
            keyword: String::new(),
        })
        .collect();

    let valid = errors.is_empty();
    let result = PacketValidateResult { valid, errors };

    if !valid {
        // Still write the result but exit with code 1
        writer.write_response(result, trace, None, None, None)?;
        return Err(CliError::ValidationFailed {
            message: "Packet validation failed".to_string(),
        });
    }

    writer.write_response(result, trace, None, None, None)
}
