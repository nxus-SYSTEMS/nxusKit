//! CE-safe `nxuskit-cli zen` command surface.
//!
//! The parser and validation-error ordering are retained, but the decision
//! engine is compiled only with the internal `pro-engines` feature.
#![allow(dead_code)]

use clap::{Args, Subcommand};

use crate::cli_error::CliError;
use crate::output::{OutputFormat, OutputWriter};

#[derive(Debug, Subcommand)]
pub enum ZenAction {
    /// Evaluate a ZEN decision table (Pro)
    Eval(ZenEvalArgs),
    /// Validate a ZEN decision model (Pro)
    Validate(ZenValidateArgs),
    /// Run ZEN fixture tests (Pro)
    Test(ZenTestArgs),
}

#[derive(Debug, Args)]
pub struct ZenEvalArgs {
    /// Input file path or `-` for stdin.
    ///
    /// ZEN evaluation requires nxusKit Pro. Public CE validates command
    /// wiring and returns a Pro entitlement error.
    #[arg(short, long)]
    pub input: String,

    #[arg(short, long, default_value = "json")]
    pub format: String,

    #[arg(short, long, default_value_t = false)]
    pub quiet: bool,

    #[arg(short, long)]
    pub output: Option<String>,
}

#[derive(Debug, Args)]
pub struct ZenValidateArgs {
    /// Input file path or `-` for stdin.
    ///
    /// ZEN validation requires nxusKit Pro. Public CE validates command
    /// wiring and returns a Pro entitlement error.
    #[arg(short, long)]
    pub input: String,

    #[arg(short, long, default_value = "json")]
    pub format: String,

    #[arg(short, long, default_value_t = false)]
    pub quiet: bool,

    #[arg(short, long)]
    pub output: Option<String>,
}

#[derive(Debug, Args)]
pub struct ZenTestArgs {
    /// Input file path or `-` for stdin.
    ///
    /// ZEN fixture testing requires nxusKit Pro. Public CE validates command
    /// wiring and returns a Pro entitlement error.
    #[arg(short, long)]
    pub input: String,

    #[arg(short, long, default_value = "json")]
    pub format: String,

    #[arg(short, long, default_value_t = false)]
    pub quiet: bool,

    #[arg(short, long)]
    pub output: Option<String>,
}

pub async fn run_zen_command(action: ZenAction) -> Result<(), CliError> {
    match action {
        ZenAction::Eval(args) => run_zen_eval(args).await,
        ZenAction::Validate(args) => run_zen_validate(args).await,
        ZenAction::Test(args) => run_zen_test(args).await,
    }
}

async fn run_zen_eval(args: ZenEvalArgs) -> Result<(), CliError> {
    let _format = OutputFormat::parse(&args.format)?;
    let raw_input = OutputWriter::read_input(&args.input)?;
    parse_json_for_pro_stub(&raw_input, "Invalid ZEN eval input")?;
    zen_unavailable()
}

async fn run_zen_validate(args: ZenValidateArgs) -> Result<(), CliError> {
    let _format = OutputFormat::parse(&args.format)?;
    let raw_input = OutputWriter::read_input(&args.input)?;
    parse_json_for_pro_stub(&raw_input, "Invalid ZEN validation input")?;
    zen_unavailable()
}

async fn run_zen_test(args: ZenTestArgs) -> Result<(), CliError> {
    let _format = OutputFormat::parse(&args.format)?;
    let raw_input = OutputWriter::read_input(&args.input)?;
    parse_json_for_pro_stub(&raw_input, "Invalid ZEN test input")?;
    zen_unavailable()
}

pub(crate) async fn run_zen_stage_eval(
    _config: &serde_json::Value,
) -> Result<serde_json::Value, CliError> {
    zen_unavailable().map(|_| serde_json::Value::Null)
}

fn parse_json_for_pro_stub(raw_input: &str, label: &str) -> Result<(), CliError> {
    let _value: serde_json::Value =
        serde_json::from_str(raw_input).map_err(|e| CliError::CommandValidation {
            code: "parse_error",
            message: format!("{label}: {e}"),
            details: None,
        })?;
    Ok(())
}

fn zen_unavailable() -> Result<(), CliError> {
    Err(CliError::EntitlementRequired {
        required_edition: "pro".to_string(),
        current_edition: "community".to_string(),
    })
}
