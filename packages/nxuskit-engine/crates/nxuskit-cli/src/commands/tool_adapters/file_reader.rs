//! File reader tool adapter for tool-loop.

use crate::cli_error::CliError;

/// Read a local file, rejecting path traversal attempts.
pub fn read_file(path: &str) -> Result<String, CliError> {
    // Security: reject paths with `..` components
    if path.contains("..") {
        return Err(CliError::ParseError {
            message: format!("Path traversal not allowed: {path}"),
        });
    }

    if !std::path::Path::new(path).exists() {
        return Err(CliError::FileNotFound {
            path: path.to_string(),
        });
    }

    std::fs::read_to_string(path).map_err(|e| CliError::ParseError {
        message: format!("Failed to read '{path}': {e}"),
    })
}
