//! Spec 119 Sprint 1D: Ollama discovery protocol backend.
//!
//! Owns the Ollama `api/tags` relative path for both the `ollama_local` and
//! `ollama_cloud_via_daemon` profiles. The literal default anchor
//! `http://localhost:11434` becomes `http://localhost:11434/api/tags`.
//! Neither profile accepts a direct SDK bearer selector: local access is
//! unauthenticated and cloud authentication remains daemon-managed.
//! Independent of `OllamaProvider`: this backend owns no inference request
//! construction, streaming, or provider lifecycle state.

use std::collections::HashMap;

use async_trait::async_trait;

use crate::discovery::{
    DiscoveryBackend, build_discovery_http_client, decode_discovery_utf8, execute_discovery_get,
    join_discovery_route, parse_discovery_json,
};
use crate::error::{ProviderContractResult, ProviderError};
use crate::types::ModelInfo;

fn visit_ollama_models(
    bytes: &[u8],
    mut visit: impl FnMut(&str, Option<u64>, Option<&str>, Option<&str>),
) -> ProviderContractResult<()> {
    let malformed = || ProviderError::malformed_response("Discovery response malformed");
    let text = decode_discovery_utf8(bytes)?;
    let value = parse_discovery_json(text)?;
    let models_json = value
        .as_object()
        .and_then(|obj| obj.get("models"))
        .and_then(|models| models.as_array())
        .ok_or_else(malformed)?;

    for item in models_json {
        let obj = item.as_object().ok_or_else(malformed)?;
        let name = obj
            .get("name")
            .and_then(|value| value.as_str())
            .ok_or_else(malformed)?;
        let size_bytes = match obj.get("size") {
            None => None,
            Some(value) => Some(value.as_u64().ok_or_else(malformed)?),
        };

        let digest = obj
            .get("digest")
            .map(|value| value.as_str().ok_or_else(malformed))
            .transpose()?;
        let modified_at = obj
            .get("modified_at")
            .map(|value| value.as_str().ok_or_else(malformed))
            .transpose()?;
        visit(name, size_bytes, digest, modified_at);
    }
    Ok(())
}

fn parse_ollama_models(bytes: &[u8]) -> ProviderContractResult<Vec<ModelInfo>> {
    let mut models = Vec::new();
    visit_ollama_models(bytes, |name, size_bytes, digest, modified_at| {
        let mut metadata = HashMap::new();
        if let Some(digest) = digest {
            metadata.insert("digest".to_string(), digest.to_string());
        }
        if let Some(modified_at) = modified_at {
            metadata.insert("modified_at".to_string(), modified_at.to_string());
        }
        models.push(ModelInfo {
            name: name.to_string(),
            size_bytes,
            description: None,
            context_window: None,
            metadata,
        });
    })?;
    Ok(models)
}

/// Crate-private `DiscoveryBackend` for the Ollama anchor protocol. Sends no
/// `Authorization` header for either the local or daemon-proxied profile.
pub(crate) struct OllamaDiscoveryBackend {
    client: reqwest::Client,
    endpoint: String,
}

impl OllamaDiscoveryBackend {
    pub(crate) fn new(
        endpoint: &str,
        connect_timeout_ms: u32,
        read_timeout_ms: u32,
    ) -> ProviderContractResult<Self> {
        let client = build_discovery_http_client(connect_timeout_ms, read_timeout_ms)?;
        Ok(Self {
            client,
            endpoint: endpoint.to_string(),
        })
    }

    async fn fetch_models(&self) -> ProviderContractResult<Vec<ModelInfo>> {
        let url = join_discovery_route(&self.endpoint, "api/tags")?;
        let bytes = execute_discovery_get(&self.client, url, None).await?;
        parse_ollama_models(&bytes)
    }
}

#[async_trait]
impl DiscoveryBackend for OllamaDiscoveryBackend {
    async fn health(&self) -> ProviderContractResult<()> {
        let url = join_discovery_route(&self.endpoint, "api/tags")?;
        let bytes = execute_discovery_get(&self.client, url, None).await?;
        visit_ollama_models(&bytes, |_, _, _, _| {})
    }

    async fn list_models(&self) -> ProviderContractResult<Vec<ModelInfo>> {
        self.fetch_models().await
    }
}
