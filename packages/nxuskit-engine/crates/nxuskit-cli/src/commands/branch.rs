//! `nxuskit-cli branch fork|compare` — Multi-model forking (FR-012).

use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};

use std::sync::Arc;

use crate::cli_error::CliError;
use crate::envelope::TraceFields;
use crate::output::{OutputFormat, OutputWriter, UsageInfo};

#[derive(Debug, Subcommand)]
pub enum BranchAction {
    /// Fork a prompt across multiple models concurrently
    Fork(BranchForkArgs),
    /// Compare results from a branch fork
    Compare(BranchCompareArgs),
}

#[derive(Debug, Args)]
pub struct BranchForkArgs {
    /// Input file path or `-` for stdin.
    ///
    /// JSON: {"prompt": "...", "models": ["model-a", "model-b"],
    /// "provider": "...", "system": "..."}
    #[arg(short, long)]
    pub input: String,

    /// Comma-separated model list (alternative to JSON input)
    #[arg(long)]
    pub models: Option<String>,

    #[arg(short, long, default_value = "json")]
    pub format: String,

    #[arg(short, long, default_value_t = false)]
    pub quiet: bool,

    #[arg(short, long)]
    pub output: Option<String>,
}

#[derive(Debug, Args)]
pub struct BranchCompareArgs {
    /// Input file containing a branch fork result (JSON from `branch fork`)
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
pub struct BranchForkInput {
    pub prompt: String,
    pub models: Vec<String>,
    pub provider: Option<String>,
    pub system: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BranchForkResult {
    pub results: Vec<BranchModelResult>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BranchModelResult {
    pub model: String,
    pub content: String,
    pub usage: Option<UsageInfo>,
    pub elapsed_ms: f64,
}

#[derive(Debug, Serialize)]
pub struct BranchCompareResult {
    pub comparison: Vec<ComparisonEntry>,
    pub diffs: Vec<DiffEntry>,
}

#[derive(Debug, Serialize)]
pub struct ComparisonEntry {
    pub model: String,
    pub length: u32,
    pub quality_score: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct DiffEntry {
    pub field: String,
    pub values: serde_json::Value,
}

pub async fn run_branch_command(action: BranchAction) -> Result<(), CliError> {
    match action {
        BranchAction::Fork(args) => run_branch_fork(args).await,
        BranchAction::Compare(args) => run_branch_compare(args).await,
    }
}

async fn run_branch_fork(args: BranchForkArgs) -> Result<(), CliError> {
    let format = OutputFormat::parse(&args.format)?;
    let raw_input = OutputWriter::read_input(&args.input)?;

    let fork_input: BranchForkInput = if let Some(models_str) = &args.models {
        // Allow --models flag with minimal JSON input
        let mut input: serde_json::Value =
            serde_json::from_str(&raw_input).map_err(|e| CliError::ParseError {
                message: format!("Invalid input JSON: {e}"),
            })?;
        let models: Vec<String> = models_str
            .split(',')
            .map(|s| s.trim().to_string())
            .collect();
        input["models"] = serde_json::json!(models);
        serde_json::from_value(input).map_err(|e| CliError::ParseError {
            message: format!("Invalid fork input: {e}"),
        })?
    } else {
        serde_json::from_str(&raw_input).map_err(|e| CliError::ParseError {
            message: format!("Invalid fork input: {e}"),
        })?
    };

    let trace = TraceFields::new(
        "branch_fork",
        &raw_input,
        fork_input.provider.as_deref(),
        None,
    );
    let writer = OutputWriter::new(format, args.quiet, args.output);

    let provider_name = fork_input.provider.as_deref().unwrap_or("loopback");
    let provider_impl: Arc<dyn nxuskit_engine::prelude::LLMProvider> = Arc::from(
        crate::create_provider(provider_name).map_err(|e| CliError::ProviderError {
            message: format!("{e}"),
        })?,
    );

    // Fan out to all models concurrently
    let mut handles = Vec::new();
    for model in &fork_input.models {
        let provider = Arc::clone(&provider_impl);
        let model = model.clone();
        let prompt = fork_input.prompt.clone();
        let system = fork_input.system.clone();

        handles.push(tokio::spawn(async move {
            let start = std::time::Instant::now();
            let mut request = nxuskit_engine::prelude::ChatRequest::new(&model)
                .with_message(nxuskit_engine::prelude::Message::user(&prompt));
            if let Some(sys) = &system {
                request = request.with_message(nxuskit_engine::prelude::Message::system(sys));
            }
            let response = provider.chat(&request).await;
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            (model, response, elapsed)
        }));
    }

    let mut results = Vec::new();
    for handle in handles {
        let (model, response, elapsed) = handle.await.map_err(|e| CliError::ProviderError {
            message: format!("Task join error: {e}"),
        })?;
        match response {
            Ok(resp) => {
                let best = resp.usage.best_available();
                results.push(BranchModelResult {
                    model,
                    content: resp.content,
                    usage: Some(UsageInfo {
                        input_tokens: best.prompt_tokens,
                        output_tokens: best.completion_tokens,
                        total_tokens: best.total(),
                    }),
                    elapsed_ms: elapsed,
                });
            }
            Err(e) => {
                results.push(BranchModelResult {
                    model,
                    content: format!("Error: {e}"),
                    usage: None,
                    elapsed_ms: elapsed,
                });
            }
        }
    }

    writer.write_response(BranchForkResult { results }, trace, None, None, None)
}

async fn run_branch_compare(args: BranchCompareArgs) -> Result<(), CliError> {
    let format = OutputFormat::parse(&args.format)?;
    let raw_input = OutputWriter::read_input(&args.input)?;

    let fork_result: BranchForkResult =
        serde_json::from_str(&raw_input).map_err(|e| CliError::ParseError {
            message: format!("Invalid fork result input: {e}"),
        })?;

    let trace = TraceFields::new("branch_compare", &raw_input, None, None);
    let writer = OutputWriter::new(format, args.quiet, args.output);

    let comparison: Vec<ComparisonEntry> = fork_result
        .results
        .iter()
        .map(|r| ComparisonEntry {
            model: r.model.clone(),
            length: r.content.len() as u32,
            quality_score: None,
        })
        .collect();

    // Compute field-level structural diffs
    let mut diffs = Vec::new();
    if fork_result.results.len() >= 2 {
        // Content length comparison
        let mut content_lengths = serde_json::Map::new();
        for r in &fork_result.results {
            content_lengths.insert(r.model.clone(), serde_json::json!(r.content.len()));
        }
        diffs.push(DiffEntry {
            field: "content_length".to_string(),
            values: serde_json::Value::Object(content_lengths),
        });

        // Word count comparison
        let mut word_counts = serde_json::Map::new();
        for r in &fork_result.results {
            word_counts.insert(
                r.model.clone(),
                serde_json::json!(r.content.split_whitespace().count()),
            );
        }
        diffs.push(DiffEntry {
            field: "word_count".to_string(),
            values: serde_json::Value::Object(word_counts),
        });

        // Sentence count comparison
        let mut sentence_counts = serde_json::Map::new();
        for r in &fork_result.results {
            let sentences = r
                .content
                .split(['.', '!', '?'])
                .filter(|s| !s.trim().is_empty())
                .count();
            sentence_counts.insert(r.model.clone(), serde_json::json!(sentences));
        }
        diffs.push(DiffEntry {
            field: "sentence_count".to_string(),
            values: serde_json::Value::Object(sentence_counts),
        });

        // Elapsed time comparison
        let mut elapsed_values = serde_json::Map::new();
        for r in &fork_result.results {
            elapsed_values.insert(r.model.clone(), serde_json::json!(r.elapsed_ms));
        }
        diffs.push(DiffEntry {
            field: "elapsed_ms".to_string(),
            values: serde_json::Value::Object(elapsed_values),
        });

        // Content similarity (Jaccard-like overlap using unique words)
        if fork_result.results.len() == 2 {
            let words_a: std::collections::HashSet<&str> =
                fork_result.results[0].content.split_whitespace().collect();
            let words_b: std::collections::HashSet<&str> =
                fork_result.results[1].content.split_whitespace().collect();
            let intersection = words_a.intersection(&words_b).count();
            let union = words_a.union(&words_b).count();
            let similarity = if union > 0 {
                intersection as f64 / union as f64
            } else {
                0.0
            };
            diffs.push(DiffEntry {
                field: "content_similarity".to_string(),
                values: serde_json::json!({
                    "jaccard": (similarity * 1000.0).round() / 1000.0,
                    "shared_words": intersection,
                    "total_unique_words": union,
                }),
            });
        }
    }

    writer.write_response(
        BranchCompareResult { comparison, diffs },
        trace,
        None,
        None,
        None,
    )
}
