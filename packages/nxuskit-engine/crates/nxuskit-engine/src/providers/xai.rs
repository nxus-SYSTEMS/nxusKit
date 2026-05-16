//! xAI Grok provider implementation.
//!
//! xAI exposes an OpenAI-compatible API at `https://api.x.ai/v1`. This adapter
//! reuses the existing OpenAI-compatible transport while preserving the public
//! provider id (`xai`) and Grok-vs-Groq naming boundary.

use async_trait::async_trait;
use futures::Stream;
use std::time::Duration;

use crate::{
    ChatRequest, ChatResponse, LLMProvider, ModelInfo, OpenAIProvider, StreamChunk,
    error::{NxuskitError, Result},
    parameter_adapter::ParameterAdapter,
    types::{FinishReason, InferenceMetadata, ProviderCapabilities},
};

const XAI_API_BASE: &str = "https://api.x.ai/v1";
const XAI_DEFAULT_MODEL: &str = "grok-4";

/// xAI Grok provider.
///
/// The canonical provider id is `xai`. Do not use or add `grok` as an alias;
/// `groq` remains Groq, Inc.
#[derive(Clone)]
pub struct XaiProvider {
    inner: OpenAIProvider,
    api_key: String,
    base_url: String,
    default_model: String,
    connection_timeout: Duration,
    stream_read_timeout: Duration,
    total_timeout: Duration,
}

impl XaiProvider {
    /// Create a new xAI Grok provider with the given API key.
    ///
    /// # Deprecated
    /// Use `XaiProvider::builder()` instead for more configuration options.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self::builder()
            .api_key(api_key)
            .build()
            .expect("failed to build xAI provider")
    }

    /// Set a custom base URL for the xAI API.
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self.rebuild_inner()
            .expect("failed to rebuild xAI provider after base URL update");
        self
    }

    /// Create a new builder for `XaiProvider`.
    pub fn builder() -> XaiProviderBuilder {
        XaiProviderBuilder::default()
    }

    /// Get the configured default model.
    pub fn default_model(&self) -> &str {
        &self.default_model
    }

    /// Get the configured base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Get the configured connection timeout.
    pub fn connection_timeout(&self) -> Duration {
        self.connection_timeout
    }

    /// Get the configured stream read timeout.
    pub fn stream_read_timeout(&self) -> Duration {
        self.stream_read_timeout
    }

    /// Get the configured total timeout.
    pub fn total_timeout(&self) -> Duration {
        self.total_timeout
    }

    /// Create a fresh session with no accumulated state.
    pub fn fresh_session(&self) -> Self {
        self.clone()
    }

    fn rebuild_inner(&mut self) -> Result<()> {
        self.inner = build_inner(
            self.api_key.clone(),
            self.base_url.clone(),
            self.default_model.clone(),
            self.connection_timeout,
            self.stream_read_timeout,
            self.total_timeout,
        )?;
        Ok(())
    }
}

impl std::fmt::Debug for XaiProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("XaiProvider")
            .field("base_url", &self.base_url)
            .field("default_model", &self.default_model)
            .field("connection_timeout", &self.connection_timeout)
            .field("stream_read_timeout", &self.stream_read_timeout)
            .field("total_timeout", &self.total_timeout)
            .field("api_key", &"[REDACTED]")
            .finish()
    }
}

/// Builder for `XaiProvider`.
#[derive(Debug, Default)]
pub struct XaiProviderBuilder {
    api_key: Option<String>,
    base_url: Option<String>,
    model: Option<String>,
    timeout: Option<Duration>,
    connection_timeout: Option<Duration>,
    stream_read_timeout: Option<Duration>,
    total_timeout: Option<Duration>,
}

impl XaiProviderBuilder {
    /// Set the API key for xAI.
    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// Set a custom base URL for the xAI API.
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    /// Set the default model to use.
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Set a general timeout for all operations.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Set the connection timeout.
    pub fn connection_timeout(mut self, timeout: Duration) -> Self {
        self.connection_timeout = Some(timeout);
        self
    }

    /// Set the timeout for reading each chunk in streaming responses.
    pub fn stream_read_timeout(mut self, timeout: Duration) -> Self {
        self.stream_read_timeout = Some(timeout);
        self
    }

    /// Set the total request timeout.
    pub fn total_timeout(mut self, timeout: Duration) -> Self {
        self.total_timeout = Some(timeout);
        self
    }

    /// Build the xAI provider.
    pub fn build(self) -> Result<XaiProvider> {
        let api_key = self
            .api_key
            .ok_or_else(|| NxuskitError::Configuration("API key is required".to_string()))?;

        let default_timeout = Duration::from_secs(60);
        let default_stream_timeout = Duration::from_secs(120);
        let connection_timeout = self
            .connection_timeout
            .or(self.timeout)
            .unwrap_or(default_timeout);
        let stream_read_timeout = self
            .stream_read_timeout
            .or(self.timeout)
            .unwrap_or(default_stream_timeout);
        let total_timeout = self
            .total_timeout
            .or(self.timeout)
            .unwrap_or(default_timeout);
        let base_url = self.base_url.unwrap_or_else(|| XAI_API_BASE.to_string());
        let default_model = self.model.unwrap_or_else(|| XAI_DEFAULT_MODEL.to_string());
        let inner = build_inner(
            api_key.clone(),
            base_url.clone(),
            default_model.clone(),
            connection_timeout,
            stream_read_timeout,
            total_timeout,
        )?;

        Ok(XaiProvider {
            inner,
            api_key,
            base_url,
            default_model,
            connection_timeout,
            stream_read_timeout,
            total_timeout,
        })
    }
}

fn build_inner(
    api_key: String,
    base_url: String,
    default_model: String,
    connection_timeout: Duration,
    stream_read_timeout: Duration,
    total_timeout: Duration,
) -> Result<OpenAIProvider> {
    OpenAIProvider::builder()
        .api_key(api_key)
        .base_url(base_url)
        .model(default_model)
        .connection_timeout(connection_timeout)
        .stream_read_timeout(stream_read_timeout)
        .total_timeout(total_timeout)
        .build()
}

fn normalize_response(mut response: ChatResponse) -> ChatResponse {
    response.provider = "xai".to_string();
    let finish_reason = response.finish_reason.unwrap_or(FinishReason::Stop);
    let metadata = serde_json::json!({
        "provider": "xai",
        "transport": "openai_compatible"
    });
    response.inference_metadata = InferenceMetadata::completed(finish_reason)
        .with_token_usage(response.usage.clone())
        .with_provider_metadata(metadata);
    response
}

fn enrich_xai_model(info: &mut ModelInfo) {
    if info.name.starts_with("grok-") {
        info.metadata.insert("family".into(), "grok".into());
        if info.description.is_none() {
            info.description = Some("xAI Grok model".into());
        }
    }
}

#[async_trait]
impl LLMProvider for XaiProvider {
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse> {
        if request.openai_responses.is_some() {
            return Err(NxuskitError::InvalidRequest(
                "xAI provider currently registers the OpenAI-compatible chat completions transport; Responses transport is not exposed for provider id xai".into(),
            ));
        }

        let capabilities = self.get_capabilities();
        let adapted = ParameterAdapter::adapt(request, &capabilities);
        let mut response = self.inner.chat(&adapted.request).await?;
        response.warnings.splice(0..0, adapted.warnings);
        Ok(normalize_response(response))
    }

    async fn chat_stream(
        &self,
        request: &ChatRequest,
    ) -> Result<Box<dyn Stream<Item = Result<StreamChunk>> + Send + Unpin>> {
        if request.openai_responses.is_some() {
            return Err(NxuskitError::InvalidRequest(
                "xAI provider currently registers the OpenAI-compatible chat completions transport; Responses transport is not exposed for provider id xai".into(),
            ));
        }

        let capabilities = self.get_capabilities();
        let adapted = ParameterAdapter::adapt(request, &capabilities);
        self.inner.chat_stream(&adapted.request).await
    }

    fn provider_name(&self) -> &str {
        "xai"
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let mut models = self.inner.list_models().await?;
        for model in &mut models {
            enrich_xai_model(model);
        }
        Ok(models)
    }

    fn get_capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_system_messages: true,
            supports_streaming: true,
            supports_vision: true,
            max_stop_sequences: None,
            supports_presence_penalty: false,
            supports_frequency_penalty: false,
            supports_seed: false,
            supports_logprobs: false,
            supports_streaming_logprobs: false,
            supports_json_mode: true,
            supports_json_schema: true,
            penalty_range: None,
            max_logprobs: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xai_default_base_url_matches_release_contract() {
        let provider = XaiProvider::new("test-key");
        assert_eq!(provider.base_url(), "https://api.x.ai/v1");
        assert_eq!(provider.default_model(), "grok-4");
    }

    #[test]
    fn xai_custom_base_url_is_preserved() {
        let provider = XaiProvider::new("test-key").with_base_url("https://eu-west-1.api.x.ai/v1");
        assert_eq!(provider.base_url(), "https://eu-west-1.api.x.ai/v1");
    }

    #[test]
    fn xai_provider_name_is_canonical_id() {
        let provider = XaiProvider::new("test-key");
        assert_eq!(provider.provider_name(), "xai");
    }

    #[test]
    fn xai_capabilities_are_conservative_for_logprobs() {
        let caps = XaiProvider::new("test-key").get_capabilities();
        assert!(caps.supports_streaming);
        assert!(caps.supports_vision);
        assert!(caps.supports_json_schema);
        assert!(!caps.supports_logprobs);
        assert!(!caps.supports_streaming_logprobs);
    }
}
