//! `nxuskit-cli judge select` — LLM-based candidate selection (FR-012).

use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};

use crate::cli_error::CliError;
use crate::envelope::TraceFields;
use crate::output::{OutputFormat, OutputWriter};

#[derive(Debug, Subcommand)]
pub enum JudgeAction {
    /// Select the best candidate from a set
    Select(JudgeSelectArgs),
}

#[derive(Debug, Args)]
pub struct JudgeSelectArgs {
    /// Input file path or `-` for stdin.
    ///
    /// JSON: {"candidates": [{"id": "a", "content": "..."}],
    /// "criteria": "accuracy", "provider": "...", "model": "..."}
    #[arg(short, long)]
    pub input: String,

    #[arg(short, long, default_value = "json")]
    pub format: String,

    #[arg(short, long, default_value_t = false)]
    pub quiet: bool,

    #[arg(short, long)]
    pub output: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct JudgeSelectInput {
    pub candidates: Vec<Candidate>,
    pub criteria: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Candidate {
    pub id: String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JudgeSelectResult {
    pub selected_id: String,
    pub reasoning: String,
    pub scores: Vec<CandidateScore>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CandidateScore {
    pub id: String,
    pub score: f64,
    pub rationale: String,
}

pub async fn run_judge_command(action: JudgeAction) -> Result<(), CliError> {
    match action {
        JudgeAction::Select(args) => run_judge_select(args).await,
    }
}

async fn run_judge_select(args: JudgeSelectArgs) -> Result<(), CliError> {
    let format = OutputFormat::parse(&args.format)?;
    let raw_input = OutputWriter::read_input(&args.input)?;
    let select_input: JudgeSelectInput =
        serde_json::from_str(&raw_input).map_err(|e| CliError::ParseError {
            message: format!("Invalid judge select input: {e}"),
        })?;

    let trace = TraceFields::new(
        "judge_select",
        &raw_input,
        select_input.provider.as_deref(),
        select_input.model.as_deref(),
    );
    let writer = OutputWriter::new(format, args.quiet, args.output);

    let provider_name = select_input.provider.as_deref().unwrap_or("loopback");
    let model_name = select_input.model.as_deref().unwrap_or("default");

    let provider_impl =
        crate::create_provider(provider_name).map_err(|e| CliError::ProviderError {
            message: format!("{e}"),
        })?;

    // Build judge prompt
    let mut prompt = String::from("Given the following candidates, select the best one.\n\n");
    for (i, candidate) in select_input.candidates.iter().enumerate() {
        prompt.push_str(&format!(
            "Candidate {} (id: {}):\n{}\n\n",
            i + 1,
            candidate.id,
            candidate.content
        ));
    }
    if let Some(criteria) = &select_input.criteria {
        prompt.push_str(&format!("Evaluation criteria: {}\n\n", criteria));
    }
    prompt.push_str(
        "Respond in JSON format: {\"selected_id\": \"...\", \"reasoning\": \"...\", \"scores\": [{\"id\": \"...\", \"score\": 0.0-1.0, \"rationale\": \"...\"}]}"
    );

    let request = nxuskit_engine::prelude::ChatRequest::new(model_name)
        .with_message(nxuskit_engine::prelude::Message::user(&prompt));

    let response = provider_impl
        .chat(&request)
        .await
        .map_err(|e| CliError::ProviderError {
            message: format!("{e}"),
        })?;

    // Try to parse the LLM response as structured JSON
    // If the response contains JSON embedded in text, try to extract it
    let result: JudgeSelectResult = serde_json::from_str(&response.content)
        .or_else(|_| {
            // Try to find JSON object within the response text
            let content = &response.content;
            if let (Some(start), Some(end)) = (content.find('{'), content.rfind('}')) {
                return serde_json::from_str(&content[start..=end]);
            }
            Err(serde_json::Error::io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "no JSON found",
            )))
        })
        .map_err(|e| CliError::ParseError {
            message: format!(
                "Failed to parse judge response as structured JSON: {}. Raw response: '{}'",
                e,
                response.content.chars().take(200).collect::<String>()
            ),
        })?;

    writer.write_response(result, trace, None, None, None)
}
