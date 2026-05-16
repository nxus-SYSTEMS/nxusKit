//! Output formatting and I/O for the CLI shell contract.
//!
//! All commands use `OutputWriter` to produce structured output in the
//! requested format (JSON, YAML, JSONL, text) and to emit errors to stderr.

use std::io::{self, BufWriter, Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::cli_error::CliError;
use crate::envelope::TraceFields;

/// Supported output formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Json,
    Yaml,
    Jsonl,
    Text,
    Human,
}

impl OutputFormat {
    /// Parse a format string, returning `CliError::InvalidFormat` for unknown values.
    pub fn parse(s: &str) -> Result<Self, CliError> {
        match s.to_lowercase().as_str() {
            "json" => Ok(Self::Json),
            "yaml" | "yml" => Ok(Self::Yaml),
            "jsonl" => Ok(Self::Jsonl),
            "text" | "txt" => Ok(Self::Text),
            "human" => Ok(Self::Human),
            _ => Err(CliError::InvalidFormat {
                value: s.to_string(),
            }),
        }
    }
}

/// Universal response envelope for all commands.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseEnvelope<T: Serialize> {
    pub trace_id: String,
    pub timestamp: String,
    pub request_hash: String,
    pub request_metadata: crate::envelope::RequestMetadata,
    pub result: T,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<UsageInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elapsed_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<serde_json::Value>>,
}

/// Token usage information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageInfo {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,
}

/// JSONL event for streaming output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonlEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(flatten)]
    pub data: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
}

/// Handles output writing to stdout/stderr/file with format awareness.
#[allow(dead_code)]
pub struct OutputWriter {
    pub format: OutputFormat,
    pub quiet: bool,
    pub output_path: Option<String>,
}

impl OutputWriter {
    pub fn new(format: OutputFormat, quiet: bool, output_path: Option<String>) -> Self {
        Self {
            format,
            quiet,
            output_path,
        }
    }

    /// Read input from a file path or stdin (`-`).
    pub fn read_input(path: &str) -> Result<String, CliError> {
        if path == "-" {
            let mut buf = String::new();
            io::stdin()
                .read_to_string(&mut buf)
                .map_err(|e| CliError::ParseError {
                    message: format!("Failed to read stdin: {e}"),
                })?;
            if buf.trim().is_empty() {
                return Err(CliError::EmptyInput);
            }
            Ok(buf)
        } else {
            if !Path::new(path).exists() {
                return Err(CliError::FileNotFound {
                    path: path.to_string(),
                });
            }
            std::fs::read_to_string(path).map_err(|e| CliError::ParseError {
                message: format!("Failed to read '{}': {}", path, e),
            })
        }
    }

    /// Write a successful response envelope to output.
    pub fn write_response<T: Serialize>(
        &self,
        result: T,
        trace: TraceFields,
        usage: Option<UsageInfo>,
        finish_reason: Option<String>,
        elapsed_ms: Option<f64>,
    ) -> Result<(), CliError> {
        let envelope = ResponseEnvelope {
            trace_id: trace.trace_id,
            timestamp: trace.timestamp,
            request_hash: trace.request_hash,
            request_metadata: trace.request_metadata,
            result,
            usage,
            finish_reason,
            elapsed_ms,
            tool_calls: None,
        };

        let output = match self.format {
            OutputFormat::Json => {
                serde_json::to_string_pretty(&envelope).map_err(|e| CliError::ParseError {
                    message: format!("Serialization failed: {e}"),
                })?
            }
            OutputFormat::Yaml => {
                serde_yaml_ng::to_string(&envelope).map_err(|e| CliError::ParseError {
                    message: format!("YAML serialization failed: {e}"),
                })?
            }
            OutputFormat::Text | OutputFormat::Human => {
                // For text/human mode, just serialize the result portion
                serde_json::to_string_pretty(&envelope.result).map_err(|e| {
                    CliError::ParseError {
                        message: format!("Serialization failed: {e}"),
                    }
                })?
            }
            OutputFormat::Jsonl => {
                // Single-line JSON for JSONL mode
                serde_json::to_string(&envelope).map_err(|e| CliError::ParseError {
                    message: format!("Serialization failed: {e}"),
                })?
            }
        };

        self.write_to_output(&output)
    }

    /// Write a JSONL event to stdout (streaming mode).
    pub fn write_jsonl_event(&self, event: &JsonlEvent) -> Result<(), CliError> {
        let line = serde_json::to_string(event).map_err(|e| CliError::ParseError {
            message: format!("Event serialization failed: {e}"),
        })?;
        let stdout = io::stdout();
        let mut writer = BufWriter::new(stdout.lock());
        writeln!(writer, "{line}").map_err(|e| CliError::ParseError {
            message: format!("Write failed: {e}"),
        })
    }

    /// Write an error envelope to stderr and exit.
    pub fn write_error_and_exit(err: &CliError) -> ! {
        let envelope = err.to_error_envelope();
        let json = serde_json::to_string(&envelope).unwrap_or_else(|_| {
            format!(
                r#"{{"code":"{}","message":"{}"}}"#,
                err.code(),
                err.to_string().replace('"', "\\\"")
            )
        });
        eprintln!("{json}");
        std::process::exit(err.exit_code())
    }

    /// Write content to the configured output destination.
    fn write_to_output(&self, content: &str) -> Result<(), CliError> {
        match &self.output_path {
            Some(path) if path != "-" => {
                std::fs::write(path, content).map_err(|e| CliError::ParseError {
                    message: format!("Failed to write to '{}': {}", path, e),
                })
            }
            _ => {
                let stdout = io::stdout();
                let mut writer = BufWriter::new(stdout.lock());
                writeln!(writer, "{content}").map_err(|e| CliError::ParseError {
                    message: format!("Write failed: {e}"),
                })
            }
        }
    }

    /// Print a status message to stderr (suppressed in quiet mode).
    #[allow(dead_code)]
    pub fn status(&self, msg: &str) {
        if !self.quiet {
            eprintln!("{msg}");
        }
    }
}
