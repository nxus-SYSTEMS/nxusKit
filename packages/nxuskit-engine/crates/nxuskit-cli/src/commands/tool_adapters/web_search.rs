//! Web search tool adapter for tool-loop.

use serde::Serialize;

use crate::cli_error::CliError;

#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// Search via configured search provider (Brave Search or SerpAPI).
pub async fn search(
    query: &str,
    _config: &serde_json::Value,
) -> Result<Vec<SearchResult>, CliError> {
    // Try Brave Search first
    let api_key = std::env::var("BRAVE_API_KEY")
        .or_else(|_| std::env::var("SERP_API_KEY"))
        .map_err(|_| CliError::ProviderError {
            message: "No search API key found. Set BRAVE_API_KEY or SERP_API_KEY.".to_string(),
        })?;

    let client = reqwest::Client::new();
    let url = format!(
        "https://api.search.brave.com/res/v1/web/search?q={}&count=5",
        urlencoding(query)
    );
    let resp = client
        .get(&url)
        .header("X-Subscription-Token", &api_key)
        .send()
        .await
        .map_err(|e| CliError::ProviderError {
            message: format!("Search request failed: {e}"),
        })?;

    let body: serde_json::Value = resp.json().await.map_err(|e| CliError::ProviderError {
        message: format!("Failed to parse search response: {e}"),
    })?;

    let results = body
        .get("web")
        .and_then(|w: &serde_json::Value| w.get("results"))
        .and_then(|r: &serde_json::Value| r.as_array())
        .map(|arr| {
            arr.iter()
                .map(|item| SearchResult {
                    title: item
                        .get("title")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    url: item
                        .get("url")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    snippet: item
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(results)
}

/// Simple URL encoding for query strings.
fn urlencoding(s: &str) -> String {
    s.replace(' ', "+")
        .replace('&', "%26")
        .replace('=', "%3D")
        .replace('?', "%3F")
}
