//! CE-safe `nxuskit-cli solver` command surface.
//!
//! The solver command shape is retained for stable help/error behavior, but the
//! implementation is compiled only with the internal `pro-engines` feature.
#![allow(dead_code)]

use clap::{Args, Subcommand};

use crate::cli_error::CliError;
use crate::output::{OutputFormat, OutputWriter};

#[derive(Debug, Subcommand)]
pub enum SolverAction {
    /// Solve a constraint satisfaction problem
    Solve(SolverArgs),
    /// Solve with an additional assumption and compare results
    WhatIf(SolverWhatIfArgs),
}

#[derive(Debug, Args)]
pub struct SolverArgs {
    /// Input file path or `-` for stdin.
    ///
    /// Constraint solving requires nxusKit Pro. Public CE validates command
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
pub struct SolverWhatIfArgs {
    /// Input file path or `-` for stdin.
    #[arg(short = 'p', long = "problem")]
    pub problem: String,

    /// Additional assumption. Evaluation requires nxusKit Pro.
    #[arg(short, long)]
    pub assume: String,

    /// Request a comparison result. Evaluation requires nxusKit Pro.
    #[arg(long, default_value_t = false)]
    pub compare: bool,

    /// Output format
    #[arg(short, long, default_value = "json")]
    pub format: String,

    /// Shorthand for --format json
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

pub async fn run_solver_command(action: SolverAction) -> Result<(), CliError> {
    match action {
        SolverAction::Solve(args) => run_solver_solve(args).await,
        SolverAction::WhatIf(args) => run_solver_whatif(args).await,
    }
}

async fn run_solver_solve(args: SolverArgs) -> Result<(), CliError> {
    let _format = OutputFormat::parse(&args.format)?;
    let _raw_input = OutputWriter::read_input(&args.input)?;
    solver_unavailable()
}

async fn run_solver_whatif(args: SolverWhatIfArgs) -> Result<(), CliError> {
    let _format = if args.json {
        OutputFormat::Json
    } else {
        OutputFormat::parse(&args.format)?
    };
    let _raw_input = OutputWriter::read_input(&args.problem)?;
    solver_unavailable()
}

pub(crate) fn run_solver_stage_solve(
    _config: &serde_json::Value,
) -> Result<serde_json::Value, CliError> {
    solver_unavailable().map(|_| serde_json::Value::Null)
}

fn solver_unavailable() -> Result<(), CliError> {
    Err(CliError::EntitlementRequired {
        required_edition: "pro".to_string(),
        current_edition: "community".to_string(),
    })
}
