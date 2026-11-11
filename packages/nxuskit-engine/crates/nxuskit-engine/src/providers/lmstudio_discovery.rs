//! LM Studio native v1 (server compatibility baseline 0.4.0).
//! <https://lmstudio.ai/docs/developer/rest/list>
//! Endpoint must explicitly end in /api/v1; never fall back to /v1 or load a model.
//! The catalog does not report server version. Missing load metadata is unknown.

use crate::discovery::{
    DiscoveryApi, DiscoveryBackend, DiscoveryCredentialResolver, DiscoveryLoadState,
    DiscoverySnapshot, LoadedInstance, ModelLoadInfo, build_discovery_http_client,
    execute_discovery_get, join_discovery_route, resolve_bearer_header,
};
use crate::error::{ProviderContractResult, ProviderError};
use crate::types::ModelInfo;
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Deserialize)]
struct Catalog {
    models: Vec<Model>,
}
#[derive(Deserialize)]
struct Model {
    key: String,
    #[serde(rename = "type")]
    kind: String,
    publisher: String,
    display_name: String,
    size_bytes: u64,
    max_context_length: u32,
    loaded_instances: Option<Vec<Instance>>,
}
#[derive(Deserialize)]
struct Instance {
    id: String,
    config: InstanceConfig,
}
#[derive(Deserialize)]
struct InstanceConfig {
    context_length: u64,
}

fn parse(bytes: &[u8]) -> ProviderContractResult<DiscoverySnapshot> {
    let malformed = || ProviderError::malformed_response("Discovery response malformed");
    let catalog: Catalog = serde_json::from_slice(bytes).map_err(|_| malformed())?;
    let mut models = Vec::new();
    let mut states = Vec::new();
    let mut model_keys = HashSet::new();
    let mut instance_ids = HashSet::new();
    let (mut loaded, mut unknown) = (false, false);
    for model in catalog.models {
        if model.key.is_empty()
            || !model_keys.insert(model.key.clone())
            || !matches!(model.kind.as_str(), "llm" | "embedding")
        {
            return Err(malformed());
        }
        let instances = match model.loaded_instances {
            None => {
                unknown = true;
                None
            }
            Some(instances) => {
                let mut result = Vec::new();
                for instance in instances {
                    if instance.id.is_empty()
                        || !instance_ids.insert(instance.id.clone())
                        || instance.config.context_length == 0
                    {
                        return Err(malformed());
                    }
                    result.push(LoadedInstance {
                        id: instance.id,
                        context_length: instance.config.context_length,
                    });
                }
                loaded |= !result.is_empty();
                Some(result)
            }
        };
        states.push(ModelLoadInfo {
            model: model.key.clone(),
            kind: model.kind.clone(),
            loaded_instances: instances,
        });
        models.push(ModelInfo {
            name: model.key,
            size_bytes: Some(model.size_bytes),
            description: Some(model.display_name),
            context_window: Some(model.max_context_length),
            metadata: HashMap::from([
                ("type".into(), model.kind),
                ("publisher".into(), model.publisher),
            ]),
        });
    }
    Ok(DiscoverySnapshot {
        models,
        model_states: states,
        load_state: if loaded {
            DiscoveryLoadState::Loaded
        } else if unknown {
            DiscoveryLoadState::Unknown
        } else {
            DiscoveryLoadState::NotLoaded
        },
        api: Some(DiscoveryApi {
            protocol: "lmstudio_native_v1".into(),
            minimum_server_version: "0.4.0".into(),
            reported_server_version: None,
        }),
    })
}

pub(crate) struct LmStudioDiscoveryBackend {
    client: reqwest::Client,
    url: reqwest::Url,
    selector: Option<String>,
    resolver: Arc<dyn DiscoveryCredentialResolver>,
}

impl LmStudioDiscoveryBackend {
    pub(crate) fn new(
        endpoint: &str,
        connect: u32,
        read: u32,
        selector: Option<String>,
        resolver: Arc<dyn DiscoveryCredentialResolver>,
    ) -> ProviderContractResult<Self> {
        let url = join_discovery_route(endpoint, "models")?;
        if !url.path().ends_with("/api/v1/models") {
            return Err(ProviderError::invalid_request(
                "endpoint",
                "LM Studio discovery requires a native /api/v1 endpoint",
            ));
        }
        Ok(Self {
            client: build_discovery_http_client(connect, read)?,
            url,
            selector,
            resolver,
        })
    }
    async fn fetch(&self) -> ProviderContractResult<DiscoverySnapshot> {
        let header = self
            .selector
            .as_ref()
            .map(|s| resolve_bearer_header(&self.resolver, s))
            .transpose()?;
        parse(&execute_discovery_get(&self.client, self.url.clone(), header).await?)
    }
}

#[async_trait]
impl DiscoveryBackend for LmStudioDiscoveryBackend {
    async fn health(&self) -> ProviderContractResult<()> {
        self.fetch().await.map(|_| ())
    }
    async fn list_models(&self) -> ProviderContractResult<Vec<ModelInfo>> {
        self.fetch().await.map(|s| s.models)
    }
    async fn snapshot(&self) -> ProviderContractResult<DiscoverySnapshot> {
        self.fetch().await
    }
}
