//! Spec 119 Sprint 1D: OpenAI discovery protocol backend.
//!
//! Owns the OpenAI `models` relative path and bearer-selector authorization.
//! The literal default anchor `https://api.openai.com/v1` becomes
//! `https://api.openai.com/v1/models`. Independent of `OpenAIProvider`: this
//! backend owns no inference request construction, streaming, or provider
//! lifecycle state.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

use crate::discovery::{
    DiscoveryBackend, DiscoveryCredentialResolver, build_discovery_http_client,
    decode_discovery_utf8, execute_discovery_get, join_discovery_route, parse_discovery_json,
    resolve_bearer_header,
};
use crate::error::{ProviderContractResult, ProviderError};
use crate::types::ModelInfo;

fn visit_openai_models(
    bytes: &[u8],
    mut visit: impl FnMut(&str, i64, &str),
) -> ProviderContractResult<()> {
    let malformed = || ProviderError::malformed_response("Discovery response malformed");
    let text = decode_discovery_utf8(bytes)?;
    let value = parse_discovery_json(text)?;
    let data = value
        .as_object()
        .and_then(|obj| obj.get("data"))
        .and_then(|data| data.as_array())
        .ok_or_else(malformed)?;

    for item in data {
        let obj = item.as_object().ok_or_else(malformed)?;
        let id = obj
            .get("id")
            .and_then(|value| value.as_str())
            .ok_or_else(malformed)?;
        let created = obj
            .get("created")
            .and_then(|value| value.as_i64())
            .ok_or_else(malformed)?;
        let owned_by = obj
            .get("owned_by")
            .and_then(|value| value.as_str())
            .ok_or_else(malformed)?;

        visit(id, created, owned_by);
    }
    Ok(())
}

fn parse_openai_models(bytes: &[u8]) -> ProviderContractResult<Vec<ModelInfo>> {
    let mut models = Vec::new();
    visit_openai_models(bytes, |id, created, owned_by| {
        let mut metadata = HashMap::new();
        metadata.insert("created".to_string(), created.to_string());
        metadata.insert("owned_by".to_string(), owned_by.to_string());
        models.push(ModelInfo {
            name: id.to_string(),
            size_bytes: None,
            description: None,
            context_window: None,
            metadata,
        });
    })?;
    Ok(models)
}

/// Crate-private `DiscoveryBackend` for the OpenAI anchor protocol. Applies
/// the resolved selector value as a `Bearer` token, freshly resolved before
/// every request.
pub(crate) struct OpenAiDiscoveryBackend {
    client: reqwest::Client,
    endpoint: String,
    selector: String,
    resolver: Arc<dyn DiscoveryCredentialResolver>,
}

impl OpenAiDiscoveryBackend {
    pub(crate) fn new(
        endpoint: &str,
        connect_timeout_ms: u32,
        read_timeout_ms: u32,
        selector: String,
        resolver: Arc<dyn DiscoveryCredentialResolver>,
    ) -> ProviderContractResult<Self> {
        let client = build_discovery_http_client(connect_timeout_ms, read_timeout_ms)?;
        Ok(Self {
            client,
            endpoint: endpoint.to_string(),
            selector,
            resolver,
        })
    }

    async fn fetch_models(&self) -> ProviderContractResult<Vec<ModelInfo>> {
        let header = resolve_bearer_header(&self.resolver, &self.selector)?;
        let url = join_discovery_route(&self.endpoint, "models")?;
        let bytes = execute_discovery_get(&self.client, url, Some(header)).await?;
        parse_openai_models(&bytes)
    }
}

#[async_trait]
impl DiscoveryBackend for OpenAiDiscoveryBackend {
    async fn health(&self) -> ProviderContractResult<()> {
        let header = resolve_bearer_header(&self.resolver, &self.selector)?;
        let url = join_discovery_route(&self.endpoint, "models")?;
        let bytes = execute_discovery_get(&self.client, url, Some(header)).await?;
        visit_openai_models(&bytes, |_, _, _| {})
    }

    async fn list_models(&self) -> ProviderContractResult<Vec<ModelInfo>> {
        self.fetch_models().await
    }
}
