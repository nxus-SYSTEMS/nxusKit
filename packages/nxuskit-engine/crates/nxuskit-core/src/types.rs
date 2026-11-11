use std::ffi::CString;
use std::os::raw::c_char;

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// Opaque handle wrapping a boxed provider trait object.
/// Consumers create via `nxuskit_create_provider` and free via `nxuskit_free_provider`.
#[repr(C)]
pub struct NxuskitProvider {
    inner: Box<dyn nxuskit_engine::LLMProvider>,
    // Opaque-handle metadata: callers never observe the Rust layout. This
    // carries the caller-declared Ollama execution profile without changing
    // any exported C function signature or pointer ownership rule.
    declared_profile: Option<String>,
}

impl NxuskitProvider {
    #[cfg(test)]
    pub(crate) fn new(provider: Box<dyn nxuskit_engine::LLMProvider>) -> Self {
        Self::with_declared_profile(provider, None)
    }

    pub(crate) fn with_declared_profile(
        provider: Box<dyn nxuskit_engine::LLMProvider>,
        declared_profile: Option<String>,
    ) -> Self {
        Self {
            inner: provider,
            declared_profile,
        }
    }

    pub(crate) fn inner(&self) -> &dyn nxuskit_engine::LLMProvider {
        &*self.inner
    }

    pub(crate) fn declared_profile(&self) -> Option<&str> {
        self.declared_profile.as_deref()
    }

    pub(crate) fn provider_id(&self) -> &str {
        self.declared_profile()
            .unwrap_or_else(|| self.inner().provider_name())
    }

    fn validate_request_with_presence(
        &self,
        request: &nxuskit_engine::ChatRequest,
        thinking_mode_explicit: bool,
    ) -> nxuskit_engine::Result<()> {
        for (name, value) in [
            ("presence_penalty", request.presence_penalty),
            ("frequency_penalty", request.frequency_penalty),
        ] {
            if value.is_some_and(|value| !value.is_finite() || !(-2.0..=2.0).contains(&value)) {
                return Err(nxuskit_engine::NxuskitError::InvalidRequest(format!(
                    "{name} must be finite and between -2 and 2"
                )));
            }
        }
        if request
            .top_k
            .is_some_and(|value| !(1..=2_147_483_647).contains(&value))
        {
            return Err(nxuskit_engine::NxuskitError::InvalidRequest(
                "top_k must be between 1 and 2147483647".into(),
            ));
        }
        if request
            .min_p
            .is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
        {
            return Err(nxuskit_engine::NxuskitError::InvalidRequest(
                "min_p must be finite and between 0 and 1".into(),
            ));
        }
        if request.top_logprobs.is_some_and(|value| value > 20) {
            return Err(nxuskit_engine::NxuskitError::InvalidRequest(
                "top_logprobs must be between 0 and 20".into(),
            ));
        }
        if let Some(effort) = request.reasoning_effort.as_deref()
            && !matches!(effort, "low" | "medium" | "high" | "max")
        {
            return Err(nxuskit_engine::NxuskitError::InvalidRequest(
                "reasoning_effort must be low, medium, high, or max".into(),
            ));
        }
        if let Some(tools) = &request.tools {
            for tool in tools {
                let function = tool.get("function").unwrap_or(tool);
                let valid_name = function
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|name| !name.is_empty());
                let valid_parameters = function
                    .get("parameters")
                    .is_none_or(serde_json::Value::is_object);
                let valid_type = tool
                    .get("type")
                    .is_none_or(|value| value.as_str() == Some("function"));
                if !valid_name || !valid_parameters || !valid_type {
                    return Err(nxuskit_engine::NxuskitError::InvalidRequest(
                        "invalid tool definition".into(),
                    ));
                }
            }
        }

        let provider = self.inner().provider_name();
        if provider != "ollama" {
            for (control, requested) in [
                ("top_k", request.top_k.is_some()),
                ("min_p", request.min_p.is_some()),
            ] {
                if requested {
                    return Err(nxuskit_engine::NxuskitError::unsupported_capability(
                        provider, control,
                    ));
                }
            }
            if thinking_mode_explicit
                && !matches!(request.thinking_mode, nxuskit_engine::ThinkingMode::Omit)
            {
                return Err(nxuskit_engine::NxuskitError::unsupported_capability(
                    provider, "thinking",
                ));
            }
        }

        if matches!(
            self.declared_profile(),
            Some("ollama_local" | "ollama_cloud_via_daemon")
        ) {
            for (control, requested) in [
                ("presence_penalty", request.presence_penalty.is_some()),
                ("frequency_penalty", request.frequency_penalty.is_some()),
                ("reasoning_effort", request.reasoning_effort.is_some()),
            ] {
                if requested {
                    return Err(nxuskit_engine::NxuskitError::unsupported_capability(
                        "ollama", control,
                    ));
                }
            }
        }

        Ok(())
    }

    pub(crate) fn prepare_request(
        &self,
        mut request: nxuskit_engine::ChatRequest,
        thinking_mode_explicit: bool,
    ) -> nxuskit_engine::Result<nxuskit_engine::ChatRequest> {
        self.validate_request_with_presence(&request, thinking_mode_explicit)?;

        // Frozen empty-tools semantics: omission and an explicit empty list are
        // the same no-tools request state.
        if request.tools.as_ref().is_some_and(|tools| tools.is_empty()) {
            request.tools = None;
        }

        // The frozen canonical tool request is flat on input for every profile,
        // while provider wires use the function wrapper. Preserve already-wrapped
        // callers and normalize the flat form consistently, including Ollama.
        if let Some(tools) = request.tools.as_mut() {
            for tool in tools {
                if tool.get("function").is_some() {
                    continue;
                }
                let Some(object) = tool.as_object() else {
                    continue;
                };
                let mut function = serde_json::Map::new();
                for key in ["name", "description", "parameters", "strict"] {
                    if let Some(value) = object.get(key) {
                        function.insert(key.to_string(), value.clone());
                    }
                }
                *tool = serde_json::json!({
                    "type": "function",
                    "function": serde_json::Value::Object(function),
                });
            }
        }

        Ok(request)
    }

    pub(crate) fn decorate_response_value(&self, value: &mut serde_json::Value) {
        if let Some(object) = value.as_object_mut() {
            object.insert(
                "provider_id".to_string(),
                serde_json::Value::String(self.provider_id().to_string()),
            );
            if let Some(profile) = self.declared_profile() {
                object.insert(
                    "declared_profile".to_string(),
                    serde_json::Value::String(profile.to_string()),
                );
            }
        }
    }
}

/// Opaque handle wrapping a JSON response string.
/// Consumers read via `nxuskit_response_json` and free via `nxuskit_free_response`.
pub struct NxuskitResponse {
    json: CString,
}

impl NxuskitResponse {
    pub(crate) fn from_json(json: String) -> Option<Self> {
        CString::new(json).ok().map(|json| Self { json })
    }

    pub(crate) fn as_ptr(&self) -> *const c_char {
        self.json.as_ptr()
    }
}

/// Opaque handle for an in-progress streaming session.
/// Supports cancellation and must be freed after the stream completes.
pub struct NxuskitStream {
    cancel: CancellationToken,
    handle: Option<JoinHandle<()>>,
}

impl NxuskitStream {
    pub(crate) fn new(cancel: CancellationToken, handle: JoinHandle<()>) -> Self {
        Self {
            cancel,
            handle: Some(handle),
        }
    }

    pub(crate) fn cancel_token(&self) -> &CancellationToken {
        &self.cancel
    }

    pub(crate) fn take_handle(&mut self) -> Option<JoinHandle<()>> {
        self.handle.take()
    }
}

impl std::fmt::Debug for NxuskitProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NxuskitProvider").finish_non_exhaustive()
    }
}

impl std::fmt::Debug for NxuskitResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NxuskitResponse").finish_non_exhaustive()
    }
}

impl std::fmt::Debug for NxuskitStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NxuskitStream").finish_non_exhaustive()
    }
}
