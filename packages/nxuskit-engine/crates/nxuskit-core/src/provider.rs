use std::time::Duration;

use nxuskit_engine::LLMProvider;
use serde_json::Value;

use crate::error::set_last_error;
use crate::types::NxuskitProvider;

/// Parse a JSON config string and create the appropriate provider.
///
/// Returns `Some(NxuskitProvider)` on success, or `None` after setting
/// the thread-local error.
pub(crate) fn create_provider_from_json(config_json: &str) -> Option<NxuskitProvider> {
    let config: Value = match serde_json::from_str(config_json) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(
                "invalid_config",
                &format!("Failed to parse config JSON: {e}"),
                None,
            );
            return None;
        }
    };

    let provider_type = match config.get("provider_type").and_then(Value::as_str) {
        Some(pt) => pt,
        None => {
            set_last_error(
                "invalid_config",
                "Missing required field: provider_type",
                None,
            );
            return None;
        }
    };

    let api_key = config.get("api_key").and_then(Value::as_str);
    let model = config.get("model").and_then(Value::as_str);
    let base_url = config.get("base_url").and_then(Value::as_str);
    let timeout_ms = config.get("timeout_ms").and_then(Value::as_u64);

    let license_key = config.get("license_key").and_then(Value::as_str);
    if let Some(key) = license_key {
        log::debug!("license_key present (length={})", key.len());
    } else {
        log::debug!("license_key absent");
    }
    crate::entitlement::set_license_key(license_key);

    let result: nxuskit_engine::Result<Box<dyn LLMProvider>> = match provider_type {
        "claude" => build_claude(api_key, model, base_url, timeout_ms),
        "openai" => build_openai(api_key, model, base_url, timeout_ms),
        "ollama" => build_ollama(model, base_url, timeout_ms),
        "lmstudio" => build_lmstudio(model, base_url, timeout_ms),
        "xai" => build_xai(api_key, model, base_url, timeout_ms),
        "groq" => build_groq(api_key, model, timeout_ms),
        "fireworks" => build_fireworks(api_key, model, timeout_ms),
        "together" => build_together(api_key, model, timeout_ms),
        "openrouter" => build_openrouter(api_key, model, timeout_ms),
        "perplexity" => build_perplexity(api_key, model, timeout_ms),
        "mistral" => build_mistral(api_key, model, timeout_ms),
        "mock" => build_mock(model),
        "loopback" => build_loopback(model),
        "clips" => build_clips(model),
        "mcp" => {
            if !crate::entitlement::check_entitlement("mcp", license_key) {
                return None;
            }
            build_mcp(model, base_url)
        }
        "bn" | "bayesian" => build_bn(model, &config),
        #[cfg(any(feature = "provider-local-llama", feature = "provider-local-mistralrs"))]
        "local" => build_local(model, &config),
        unknown => {
            #[allow(unused_mut)]
            let mut valid = vec![
                "claude",
                "openai",
                "ollama",
                "lmstudio",
                "xai",
                "groq",
                "fireworks",
                "together",
                "openrouter",
                "perplexity",
                "mistral",
                "mock",
                "loopback",
                "clips",
                "mcp",
                "bn",
            ];
            #[cfg(any(feature = "provider-local-llama", feature = "provider-local-mistralrs"))]
            valid.push("local");
            set_last_error(
                "unknown_provider",
                &format!(
                    "Unknown provider_type: '{}'. Valid types: {}",
                    unknown,
                    valid.join(", ")
                ),
                None,
            );
            return None;
        }
    };

    match result {
        Ok(provider) => Some(NxuskitProvider::new(provider)),
        Err(e) => {
            crate::error::set_from_nxuskit_error(&e);
            None
        }
    }
}

// --- Builder helpers for each provider ---

macro_rules! apply_common {
    ($builder:expr, $model:expr, $timeout_ms:expr) => {{
        let mut b = $builder;
        if let Some(m) = $model {
            b = b.model(m);
        }
        if let Some(t) = $timeout_ms {
            b = b.timeout(Duration::from_millis(t));
        }
        b
    }};
}

macro_rules! apply_api_key {
    ($builder:expr, $api_key:expr, $provider_name:expr) => {{
        match $api_key {
            Some(key) => Ok($builder.api_key(key)),
            None => Err(nxuskit_engine::NxuskitError::Configuration(format!(
                "api_key is required for {} provider",
                $provider_name
            ))),
        }
    }};
}

fn build_claude(
    api_key: Option<&str>,
    model: Option<&str>,
    base_url: Option<&str>,
    timeout_ms: Option<u64>,
) -> nxuskit_engine::Result<Box<dyn LLMProvider>> {
    let mut b = apply_api_key!(nxuskit_engine::ClaudeProvider::builder(), api_key, "claude")?;
    if let Some(url) = base_url {
        b = b.base_url(url);
    }
    let b = apply_common!(b, model, timeout_ms);
    Ok(Box::new(b.build()?))
}

fn build_openai(
    api_key: Option<&str>,
    model: Option<&str>,
    base_url: Option<&str>,
    timeout_ms: Option<u64>,
) -> nxuskit_engine::Result<Box<dyn LLMProvider>> {
    let mut b = apply_api_key!(nxuskit_engine::OpenAIProvider::builder(), api_key, "openai")?;
    if let Some(url) = base_url {
        b = b.base_url(url);
    }
    let b = apply_common!(b, model, timeout_ms);
    Ok(Box::new(b.build()?))
}

fn build_ollama(
    model: Option<&str>,
    base_url: Option<&str>,
    timeout_ms: Option<u64>,
) -> nxuskit_engine::Result<Box<dyn LLMProvider>> {
    let mut b = nxuskit_engine::OllamaProvider::builder();
    if let Some(url) = base_url {
        b = b.base_url(url);
    }
    let b = apply_common!(b, model, timeout_ms);
    Ok(Box::new(b.build()?))
}

fn build_lmstudio(
    model: Option<&str>,
    base_url: Option<&str>,
    timeout_ms: Option<u64>,
) -> nxuskit_engine::Result<Box<dyn LLMProvider>> {
    let mut b = nxuskit_engine::LmStudioProvider::builder();
    if let Some(url) = base_url {
        b = b.base_url(url);
    }
    let b = apply_common!(b, model, timeout_ms);
    Ok(Box::new(b.build()?))
}

fn build_xai(
    api_key: Option<&str>,
    model: Option<&str>,
    base_url: Option<&str>,
    timeout_ms: Option<u64>,
) -> nxuskit_engine::Result<Box<dyn LLMProvider>> {
    let mut b = apply_api_key!(nxuskit_engine::XaiProvider::builder(), api_key, "xai")?;
    if let Some(url) = base_url {
        b = b.base_url(url);
    }
    let b = apply_common!(b, model, timeout_ms);
    Ok(Box::new(b.build()?))
}

fn build_groq(
    api_key: Option<&str>,
    model: Option<&str>,
    timeout_ms: Option<u64>,
) -> nxuskit_engine::Result<Box<dyn LLMProvider>> {
    let b = apply_api_key!(nxuskit_engine::GroqProvider::builder(), api_key, "groq")?;
    let b = apply_common!(b, model, timeout_ms);
    Ok(Box::new(b.build()?))
}

fn build_fireworks(
    api_key: Option<&str>,
    model: Option<&str>,
    timeout_ms: Option<u64>,
) -> nxuskit_engine::Result<Box<dyn LLMProvider>> {
    let b = apply_api_key!(
        nxuskit_engine::FireworksProvider::builder(),
        api_key,
        "fireworks"
    )?;
    let b = apply_common!(b, model, timeout_ms);
    Ok(Box::new(b.build()?))
}

fn build_together(
    api_key: Option<&str>,
    model: Option<&str>,
    timeout_ms: Option<u64>,
) -> nxuskit_engine::Result<Box<dyn LLMProvider>> {
    let b = apply_api_key!(
        nxuskit_engine::TogetherProvider::builder(),
        api_key,
        "together"
    )?;
    let b = apply_common!(b, model, timeout_ms);
    Ok(Box::new(b.build()?))
}

fn build_openrouter(
    api_key: Option<&str>,
    _model: Option<&str>,
    _timeout_ms: Option<u64>,
) -> nxuskit_engine::Result<Box<dyn LLMProvider>> {
    let key = api_key.ok_or_else(|| {
        nxuskit_engine::NxuskitError::Configuration(
            "api_key is required for openrouter provider".into(),
        )
    })?;
    Ok(Box::new(nxuskit_engine::OpenRouterProvider::new(key)))
}

fn build_perplexity(
    api_key: Option<&str>,
    model: Option<&str>,
    timeout_ms: Option<u64>,
) -> nxuskit_engine::Result<Box<dyn LLMProvider>> {
    let b = apply_api_key!(
        nxuskit_engine::PerplexityProvider::builder(),
        api_key,
        "perplexity"
    )?;
    let b = apply_common!(b, model, timeout_ms);
    Ok(Box::new(b.build()?))
}

fn build_mistral(
    api_key: Option<&str>,
    _model: Option<&str>,
    _timeout_ms: Option<u64>,
) -> nxuskit_engine::Result<Box<dyn LLMProvider>> {
    let key = api_key.ok_or_else(|| {
        nxuskit_engine::NxuskitError::Configuration(
            "api_key is required for mistral provider".into(),
        )
    })?;
    Ok(Box::new(nxuskit_engine::MistralProvider::new(key)))
}

fn build_mock(model: Option<&str>) -> nxuskit_engine::Result<Box<dyn LLMProvider>> {
    let mut b = nxuskit_engine::MockProvider::builder();
    if let Some(m) = model {
        b = b.with_model(m);
    }
    Ok(Box::new(b.build()?))
}

fn build_loopback(_model: Option<&str>) -> nxuskit_engine::Result<Box<dyn LLMProvider>> {
    let b = nxuskit_engine::LoopbackProvider::builder();
    Ok(Box::new(b.build()?))
}

fn build_clips(model: Option<&str>) -> nxuskit_engine::Result<Box<dyn LLMProvider>> {
    let mut b = nxuskit_engine::ClipsProvider::builder();
    if let Some(m) = model {
        b = b.rules_directory(m);
    }
    Ok(Box::new(b.build()?))
}

fn build_mcp(
    model: Option<&str>,
    base_url: Option<&str>,
) -> nxuskit_engine::Result<Box<dyn LLMProvider>> {
    let mut b = nxuskit_engine::McpProvider::builder();
    if let Some(url) = base_url {
        b = b.server_uri(url);
    }
    if let Some(m) = model {
        b = b.model_name(m);
    }
    Ok(Box::new(b.build()?))
}

#[cfg(any(feature = "provider-local-llama", feature = "provider-local-mistralrs"))]
fn build_local(
    model: Option<&str>,
    config: &Value,
) -> nxuskit_engine::Result<Box<dyn LLMProvider>> {
    // Determine backend for entitlement check
    let backend = config
        .get("backend")
        .and_then(Value::as_str)
        .unwrap_or("llama");
    let domain = if backend == "mistralrs" {
        "local_mistralrs"
    } else {
        "local_llama"
    };
    let license_key = config.get("license_key").and_then(Value::as_str);
    if !crate::entitlement::check_entitlement(domain, license_key) {
        return Err(nxuskit_engine::NxuskitError::Configuration(format!(
            "Feature '{domain}' is not entitled"
        )));
    }

    let mut b = nxuskit_engine::providers::local::LocalRuntimeProvider::builder();

    // Model path: from "model" field or dedicated "model_path" field
    if let Some(m) = model {
        b = b.model_path(m);
    }
    if let Some(path) = config.get("model_path").and_then(Value::as_str) {
        b = b.model_path(path);
    }

    // Backend selection
    if let Some(backend) = config.get("backend").and_then(Value::as_str) {
        b = b.backend(backend);
    }

    // GPU layers
    if let Some(layers) = config.get("n_gpu_layers").and_then(Value::as_i64) {
        b = b.n_gpu_layers(layers as i32);
    }

    // Context size
    if let Some(size) = config.get("context_size").and_then(Value::as_u64) {
        b = b.context_size(size as u32);
    }

    // Batch size
    if let Some(size) = config.get("batch_size").and_then(Value::as_u64) {
        b = b.batch_size(size as u32);
    }

    // CPU threads
    if let Some(threads) = config.get("threads").and_then(Value::as_u64) {
        b = b.threads(threads as u32);
    }

    // Search paths for model discovery
    if let Some(paths) = config.get("search_paths").and_then(Value::as_array) {
        for p in paths {
            if let Some(s) = p.as_str() {
                b = b.search_path(s);
            }
        }
    }

    Ok(Box::new(b.build()?))
}

fn build_bn(model: Option<&str>, config: &Value) -> nxuskit_engine::Result<Box<dyn LLMProvider>> {
    let mut b = nxuskit_engine::BayesianProvider::builder();
    if let Some(m) = model {
        b = b.networks_directory(m);
    }
    if let Some(dir) = config.get("networks_directory").and_then(Value::as_str) {
        b = b.networks_directory(dir);
    }
    if let Some(algo) = config.get("default_algorithm").and_then(Value::as_str) {
        b = b.default_algorithm(algo);
    }
    if let Some(n) = config.get("default_num_samples").and_then(Value::as_u64) {
        b = b.default_num_samples(n as usize);
    }
    if let Some(n) = config.get("default_burn_in").and_then(Value::as_u64) {
        b = b.default_burn_in(n as usize);
    }
    Ok(Box::new(b.build()?))
}

