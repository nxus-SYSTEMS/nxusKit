//! Safe Rust wrapper for the nxusKit C ABI SDK.
//!
//! This crate provides ergonomic Rust types and a safe API surface over the
//! pre-built `libnxuskit` shared (or static) library.  All unsafe FFI calls
//! are confined to internal modules; consumers interact only with safe types.
//!
//! # Feature flags
//!
//! | Flag | Default | Description |
//! |------|---------|-------------|
//! | `dynamic-link` | **yes** | Load `libnxuskit` at runtime via `libloading` |
//! | `static-link` | no | Link `libnxuskit` at build time |
//!
//! Exactly one of `dynamic-link` or `static-link` should be enabled.

mod async_provider;
pub mod auth;
pub mod blocking;
pub mod bn;
pub mod builders;
pub mod clips;
mod error;
mod ffi;
pub mod license;
mod mock_provider;
pub mod mock_solver;
pub mod plugin;
mod provider;
pub mod solver;
pub mod solver_types;
mod stream;
pub mod tool_types;
mod types;
mod version;
pub mod zen;

// Public re-exports — flat API surface.
pub use async_provider::AsyncProvider;
pub use auth::{
    AuthResolution, AuthStatus, OAuthResult, OAuthStatus, ProviderAuthMetadata, auth_providers,
    auth_remove_credential, auth_resolve, auth_set_credential, auth_status, auth_status_all,
    oauth_revoke, oauth_start, oauth_start_async, oauth_status,
};
pub use blocking::BlockingProvider;
pub use bn::ContinuousMarginal;
pub use clips::{ClipsSession, ClipsValue, SessionInfo, TemplateSlotInfo};
pub use error::NxuskitError;
pub use license::{
    ActivationResult, LicenseResolution, TokenInfo, TrialResult, license_activate,
    license_deactivate, license_machine_id, license_resolve, license_trial_activate,
    license_trial_issue, license_validate,
};
pub use mock_provider::{MockProvider, MockProviderBuilder};
pub use plugin::{
    PluginInfo, TrustMode, get_plugin_trust_mode, is_plugin_loaded, list_plugins, load_plugins,
    plugin_count, plugin_info, set_plugin_trust_mode, unload_all_plugins,
};
pub use provider::NxuskitProvider;
pub use solver::SolverStreamReceiver;
pub use solver_types::{SolverStreamChunk, SolverStreamResult};
pub use stream::StreamReceiver;
pub use tool_types::{
    FunctionCall, FunctionCallDelta, FunctionDefinition, ToolCall, ToolCallDelta, ToolChoice,
    ToolChoiceFunction, ToolChoiceFunctionName, ToolDefinition, ToolDefinitionBuilder,
    ToolResultMessage,
};
pub use types::{
    Capabilities, CapabilityDomains, CapabilityStatus, ChatRequest, ChatResponse, ContentPart,
    FinishReason, ImageData, ImageSource, InferenceMetadata, InferenceStep, LogprobsData,
    ManifestPublicationPosture, Message, MessageContent, ModelInfo, ParameterWarning,
    ProviderConfig, PublicCapabilityManifest, PublicProviderCapability, ResponseFormat, Role,
    StreamChunk, StreamLogprobsDelta, ThinkingMode, TokenCount, TokenLogprob, TokenUsage,
    TopLogprob, WarningSeverity, PUBLIC_CAPABILITY_FIELDS,
};
pub use zen::zen_evaluate;

/// Get build information as a JSON value.
///
/// Returns: `abi_version`, `sdk_version`, `edition`, `build_target`, `build_profile`.
pub fn build_info() -> Result<serde_json::Value, NxuskitError> {
    use std::ffi::CStr;

    #[cfg(feature = "static-link")]
    let ptr = unsafe { ffi::nxuskit_build_info() };
    #[cfg(feature = "dynamic-link")]
    let ptr = {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_build_info)() }
    };

    if ptr.is_null() {
        return Err(NxuskitError::Internal {
            message: "nxuskit_build_info returned NULL".to_string(),
        });
    }
    let json_str = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|e| NxuskitError::Internal {
            message: format!("build info not valid UTF-8: {e}"),
        })?
        .to_string();

    #[cfg(feature = "static-link")]
    unsafe {
        ffi::nxuskit_free_string(ptr);
    }
    #[cfg(feature = "dynamic-link")]
    {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_free_string)(ptr) };
    }

    serde_json::from_str(&json_str).map_err(|e| NxuskitError::Internal {
        message: format!("Failed to parse build info JSON: {e}"),
    })
}

/// Get entitlement information as a JSON value.
///
/// If `license_key` is provided, the effective edition may be upgraded.
/// Returns: `edition`, `effective_edition`, `features`, `status`.
pub fn entitlement_info(license_key: Option<&str>) -> Result<serde_json::Value, NxuskitError> {
    use std::ffi::{CStr, CString};

    let lk_cstr = license_key.map(|k| CString::new(k).unwrap());
    let lk_ptr = lk_cstr
        .as_ref()
        .map(|c| c.as_ptr())
        .unwrap_or(std::ptr::null());

    #[cfg(feature = "static-link")]
    let ptr = unsafe { ffi::nxuskit_entitlement_info(lk_ptr) };
    #[cfg(feature = "dynamic-link")]
    let ptr = {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_entitlement_info)(lk_ptr) }
    };

    if ptr.is_null() {
        return Err(NxuskitError::Internal {
            message: "nxuskit_entitlement_info returned NULL".to_string(),
        });
    }
    let json_str = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|e| NxuskitError::Internal {
            message: format!("entitlement info not valid UTF-8: {e}"),
        })?
        .to_string();

    #[cfg(feature = "static-link")]
    unsafe {
        ffi::nxuskit_free_string(ptr);
    }
    #[cfg(feature = "dynamic-link")]
    {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_free_string)(ptr) };
    }

    serde_json::from_str(&json_str).map_err(|e| NxuskitError::Internal {
        message: format!("Failed to parse entitlement info JSON: {e}"),
    })
}

/// Common imports for quick start.
///
/// # Examples
///
/// ```no_run
/// use nxuskit::prelude::*;
///
/// let provider = LoopbackProvider::builder().build()?;
/// let answer = provider.completion("Hello!")?;
/// # Ok::<(), NxuskitError>(())
/// ```
pub mod prelude {
    pub use crate::builders::{
        ClaudeProvider, FireworksProvider, GroqProvider, LmStudioProvider, LoopbackProvider,
        MistralProvider, OllamaProvider, OpenAIProvider, OpenRouterProvider, PerplexityProvider,
        TogetherProvider, XaiProvider,
    };
    pub use crate::{
        AsyncProvider, BlockingProvider, ChatRequest, ChatResponse, Message, MockProvider,
        NxuskitError, NxuskitProvider, Role, StreamChunk, StreamReceiver,
    };
}
