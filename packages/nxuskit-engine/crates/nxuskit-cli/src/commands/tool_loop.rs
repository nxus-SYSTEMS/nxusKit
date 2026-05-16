//! `nxuskit-cli tool-loop run` — Iterative tool-augmented LLM invocation (FR-011).

use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};

use crate::cli_error::CliError;
use crate::envelope::TraceFields;
use crate::output::{OutputFormat, OutputWriter};

#[derive(Debug, Subcommand)]
pub enum ToolLoopAction {
    /// Run a tool-augmented LLM loop
    Run(ToolLoopArgs),
}

#[derive(Debug, Args)]
pub struct ToolLoopArgs {
    /// Input file path or `-` for stdin.
    ///
    /// JSON: {"prompt": "...", "tools": ["file_reader", "calculator"],
    /// "tool_definitions": [...], "max_iterations": 10}
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
#[allow(dead_code)]
pub struct ToolLoopInput {
    pub prompt: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u32,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub tool_configs: serde_json::Value,
    /// Tool definitions to pass to the LLM for function calling
    #[serde(default)]
    pub tool_definitions: Option<Vec<serde_json::Value>>,
}

fn default_max_iterations() -> u32 {
    10
}

#[derive(Debug, Serialize)]
pub struct ToolLoopResult {
    pub final_answer: String,
    pub iterations: Vec<IterationResult>,
    pub total_iterations: u32,
    pub converged: bool,
}

#[derive(Debug, Serialize)]
pub struct IterationResult {
    pub iteration: u32,
    pub tool_calls: Vec<serde_json::Value>,
    pub model_response: String,
}

pub async fn run_tool_loop_command(action: ToolLoopAction) -> Result<(), CliError> {
    match action {
        ToolLoopAction::Run(args) => run_tool_loop(args).await,
    }
}

async fn run_tool_loop(args: ToolLoopArgs) -> Result<(), CliError> {
    let format = OutputFormat::parse(&args.format)?;
    let raw_input = OutputWriter::read_input(&args.input)?;
    let loop_input: ToolLoopInput =
        serde_json::from_str(&raw_input).map_err(|e| CliError::ParseError {
            message: format!("Invalid tool-loop input: {e}"),
        })?;

    // Check MCP entitlement if any MCP adapter is requested
    if loop_input.tools.iter().any(|t| t == "mcp") {
        crate::entitlement_check::require_entitlement("mcp")?;
    }

    let trace = TraceFields::new(
        "tool_loop_run",
        &raw_input,
        loop_input.provider.as_deref(),
        loop_input.model.as_deref(),
    );
    let writer = OutputWriter::new(format, args.quiet, args.output);

    let provider_name = loop_input
        .provider
        .as_deref()
        .or_else(|| {
            std::env::var("NXUSKIT_PROVIDER")
                .ok()
                .as_deref()
                .map(|_| unreachable!())
        })
        .unwrap_or("loopback");
    let provider_name = if loop_input.provider.is_some() {
        provider_name.to_string()
    } else {
        std::env::var("NXUSKIT_PROVIDER").unwrap_or_else(|_| provider_name.to_string())
    };

    let provider_impl =
        crate::create_provider(&provider_name).map_err(|e| CliError::ProviderError {
            message: format!("{e}"),
        })?;

    let model_name = loop_input.model.as_deref().unwrap_or("default");
    let mut messages = vec![nxuskit_engine::prelude::Message::user(&loop_input.prompt)];
    let mut iterations = Vec::new();
    let mut converged = false;

    for i in 0..loop_input.max_iterations {
        let mut request = nxuskit_engine::prelude::ChatRequest::new(model_name);
        for msg in &messages {
            request = request.with_message(msg.clone());
        }

        // T035: Include tool definitions in request
        if let Some(tool_defs) = &loop_input.tool_definitions {
            request.tools = Some(tool_defs.clone());
        }

        let response = provider_impl
            .chat(&request)
            .await
            .map_err(|e| CliError::ProviderError {
                message: format!("{e}"),
            })?;

        // Check if the response requests tool calls via finish_reason
        let has_tool_calls = response
            .finish_reason
            .as_ref()
            .is_some_and(|r| matches!(r, nxuskit_engine::prelude::FinishReason::ToolCalls));

        // T036: Check response.tool_calls first (structured), then fall back to content parsing
        let has_structured_tool_calls = response
            .tool_calls
            .as_ref()
            .is_some_and(|tc| !tc.is_empty());

        if !has_tool_calls && !has_structured_tool_calls {
            // Model converged — no more tool calls
            converged = true;
            iterations.push(IterationResult {
                iteration: i + 1,
                tool_calls: vec![],
                model_response: response.content.clone(),
            });
            break;
        }

        // T036: Prefer structured tool_calls from response, fall back to content parsing
        let parsed_calls: Vec<serde_json::Value> = if let Some(structured) = &response.tool_calls {
            structured.clone()
        } else {
            serde_json::from_str(&response.content).unwrap_or_default()
        };

        let mut tool_call_results = Vec::new();
        for tc in &parsed_calls {
            let tool_name = tc.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let tool_args = tc
                .get("arguments")
                .cloned()
                .unwrap_or(serde_json::Value::Null);

            let result = dispatch_tool(tool_name, &tool_args, &loop_input).await?;
            tool_call_results.push(serde_json::json!({
                "name": tool_name,
                "arguments": tool_args,
                "result": result,
            }));

            messages.push(nxuskit_engine::prelude::Message::assistant(format!(
                "Tool '{}' returned: {}",
                tool_name,
                serde_json::to_string(&result).unwrap_or_default()
            )));
        }

        iterations.push(IterationResult {
            iteration: i + 1,
            tool_calls: tool_call_results,
            model_response: response.content.clone(),
        });

        messages.push(nxuskit_engine::prelude::Message::assistant(
            &response.content,
        ));
    }

    let final_answer = iterations
        .last()
        .map(|it| it.model_response.clone())
        .unwrap_or_default();

    let result = ToolLoopResult {
        final_answer,
        total_iterations: iterations.len() as u32,
        iterations,
        converged,
    };

    writer.write_response(result, trace, None, None, None)
}

async fn dispatch_tool(
    name: &str,
    args: &serde_json::Value,
    _config: &ToolLoopInput,
) -> Result<serde_json::Value, CliError> {
    match name {
        "file_reader" => {
            let path =
                args.get("path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| CliError::ParseError {
                        message: "file_reader requires 'path' argument".to_string(),
                    })?;
            let content = super::tool_adapters::file_reader::read_file(path)?;
            Ok(serde_json::json!({ "content": content }))
        }
        "calculator" => {
            let expr = args
                .get("expression")
                .and_then(|v| v.as_str())
                .ok_or_else(|| CliError::ParseError {
                    message: "calculator requires 'expression' argument".to_string(),
                })?;
            let result = super::tool_adapters::calculator::evaluate(expr)?;
            Ok(serde_json::json!({ "result": result }))
        }
        "web_search" => {
            let query =
                args.get("query")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| CliError::ParseError {
                        message: "web_search requires 'query' argument".to_string(),
                    })?;
            let results =
                super::tool_adapters::web_search::search(query, &serde_json::Value::Null).await?;
            Ok(serde_json::to_value(&results).unwrap_or_default())
        }
        other => Err(CliError::ParseError {
            message: format!("Unknown tool adapter: {other}"),
        }),
    }
}
