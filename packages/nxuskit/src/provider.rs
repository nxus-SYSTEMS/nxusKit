//! Safe wrapper around the opaque `NxuskitProvider` C handle.

use crate::error::NxuskitError;
use crate::ffi;
use crate::stream::StreamReceiver;
use crate::types::{Capabilities, ChatRequest, ChatResponse, ModelInfo, ProviderConfig};
use crate::version::check_version;
use std::ffi::{CStr, CString};

/// Wrapper to send raw provider handle across thread boundaries.
///
/// SAFETY: The C SDK guarantees provider handles are thread-safe.
/// This is only used internally by `spawn_blocking` in async methods.
#[derive(Clone, Copy)]
struct SendableHandle(*mut ffi::NxuskitProvider);
unsafe impl Send for SendableHandle {}

/// A provider instance that can execute chat requests.
///
/// Created via [`NxuskitProvider::new`].  The underlying C handle is freed
/// automatically when the value is dropped.
pub struct NxuskitProvider {
    handle: *mut ffi::NxuskitProvider,
}

impl std::fmt::Debug for NxuskitProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NxuskitProvider")
            .field("handle", &self.handle)
            .finish()
    }
}

// SAFETY: The C SDK guarantees that each provider handle is independent and
// thread-safe.  No shared mutable state exists between handles.
unsafe impl Send for NxuskitProvider {}
unsafe impl Sync for NxuskitProvider {}

impl NxuskitProvider {
    /// Create a new provider from the given configuration.
    pub fn new(config: ProviderConfig) -> Result<Self, NxuskitError> {
        // Eagerly validate SDK is loadable (populates OnceLock for all future calls).
        #[cfg(feature = "dynamic-link")]
        ffi::dynamic::sdk()?;

        // Version check first.
        let sdk_version = Self::sdk_version_str()?;
        check_version(&sdk_version)?;

        let config_json =
            serde_json::to_string(&config).map_err(|e| NxuskitError::Configuration {
                message: format!("failed to serialize config: {e}"),
            })?;
        let c_json = CString::new(config_json).map_err(|e| NxuskitError::Configuration {
            message: format!("config contains null byte: {e}"),
        })?;

        let handle = unsafe { Self::call_create_provider(c_json.as_ptr()) };
        if handle.is_null() {
            let err_msg = Self::last_error_string();
            return Err(NxuskitError::from_json_str(
                &err_msg.unwrap_or_else(|| "unknown error creating provider".into()),
            ));
        }

        Ok(Self { handle })
    }

    /// Send a synchronous chat request and return the full response.
    pub fn chat(&self, request: ChatRequest) -> Result<ChatResponse, NxuskitError> {
        Self::chat_with_handle(self.handle, request)
    }

    /// Send a non-blocking chat request and return the full response.
    ///
    /// This is the async counterpart of [`chat`](Self::chat). The blocking FFI
    /// call is executed on a dedicated thread via [`tokio::task::spawn_blocking`],
    /// so it does not block the async runtime's worker threads.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nxuskit::{ChatRequest, Message, NxuskitProvider, ProviderConfig, Role};
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let provider = NxuskitProvider::new(ProviderConfig {
    ///     provider_type: "openai".into(),
    ///     model: Some("gpt-4o".into()),
    ///     ..Default::default()
    /// })?;
    ///
    /// let request = ChatRequest {
    ///     model: "gpt-4o".into(),
    ///     messages: vec![Message { role: Role::User, content: "Hello!".into() }],
    ///     ..Default::default()
    /// };
    ///
    /// let response = provider.chat_async(request).await?;
    /// println!("{}", response.content);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn chat_async(&self, request: ChatRequest) -> Result<ChatResponse, NxuskitError> {
        let handle = SendableHandle(self.handle);
        tokio::task::spawn_blocking(move || Self::chat_with_sendable(handle, request))
            .await
            .map_err(|e| NxuskitError::Internal {
                message: format!("task join error: {e}"),
            })?
    }

    // ------------------------------------------------------------------
    // Convenience methods
    // ------------------------------------------------------------------

    /// One-liner chat completion: sends a single user message and returns
    /// the response text.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nxuskit::{NxuskitProvider, ProviderConfig};
    /// # let provider = NxuskitProvider::new(ProviderConfig {
    /// #     provider_type: "loopback".into(), ..Default::default()
    /// # })?;
    /// let answer = provider.completion("What is 2+2?")?;
    /// println!("{answer}");
    /// # Ok::<(), nxuskit::NxuskitError>(())
    /// ```
    pub fn completion(&self, prompt: &str) -> Result<String, NxuskitError> {
        let request = ChatRequest::new("").with_message(crate::types::Message::user(prompt));
        let response = self.chat(request)?;
        Ok(response.content)
    }

    /// One-liner streaming chat completion: sends a single user message and
    /// returns a [`StreamReceiver`] for incremental chunks.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nxuskit::{NxuskitProvider, ProviderConfig};
    /// # let provider = NxuskitProvider::new(ProviderConfig {
    /// #     provider_type: "loopback".into(), ..Default::default()
    /// # })?;
    /// let mut stream = provider.completion_stream("Tell me a story")?;
    /// while let Some(chunk) = stream.next_chunk() {
    ///     print!("{}", chunk?.delta);
    /// }
    /// # Ok::<(), nxuskit::NxuskitError>(())
    /// ```
    pub fn completion_stream(&self, prompt: &str) -> Result<StreamReceiver, NxuskitError> {
        let request = ChatRequest::new("").with_message(crate::types::Message::user(prompt));
        self.chat_stream(request)
    }

    /// Async one-liner chat completion.
    ///
    /// See [`completion`](Self::completion) for the synchronous variant.
    pub async fn completion_async(&self, prompt: &str) -> Result<String, NxuskitError> {
        let request = ChatRequest::new("").with_message(crate::types::Message::user(prompt));
        let response = self.chat_async(request).await?;
        Ok(response.content)
    }

    /// Async one-liner streaming chat completion.
    ///
    /// See [`completion_stream`](Self::completion_stream) for the synchronous variant.
    pub async fn completion_stream_async(
        &self,
        prompt: &str,
    ) -> Result<StreamReceiver, NxuskitError> {
        // chat_stream itself is non-blocking (just sets up the callback)
        let request = ChatRequest::new("").with_message(crate::types::Message::user(prompt));
        self.chat_stream(request)
    }

    /// Start a streaming chat request, returning a receiver for incremental chunks.
    pub fn chat_stream(&self, mut request: ChatRequest) -> Result<StreamReceiver, NxuskitError> {
        request.stream = true;

        let req_json = serde_json::to_string(&request).map_err(|e| NxuskitError::Internal {
            message: format!("failed to serialize request: {e}"),
        })?;
        let c_json = CString::new(req_json).map_err(|e| NxuskitError::Internal {
            message: format!("request contains null byte: {e}"),
        })?;

        StreamReceiver::start(self.handle, c_json)
    }

    /// List models available from this provider (non-blocking).
    ///
    /// This is the async counterpart of [`list_models`](Self::list_models). The
    /// blocking FFI call is executed on a dedicated thread via
    /// [`tokio::task::spawn_blocking`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nxuskit::{NxuskitProvider, ProviderConfig};
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let provider = NxuskitProvider::new(ProviderConfig {
    ///     provider_type: "ollama".into(),
    ///     ..Default::default()
    /// })?;
    ///
    /// let models = provider.list_models_async().await?;
    /// for model in &models {
    ///     println!("{}: {}", model.id, model.name);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list_models_async(&self) -> Result<Vec<ModelInfo>, NxuskitError> {
        let handle = SendableHandle(self.handle);
        tokio::task::spawn_blocking(move || Self::list_models_with_sendable(handle))
            .await
            .map_err(|e| NxuskitError::Internal {
                message: format!("task join error: {e}"),
            })?
    }

    /// List models available from this provider.
    pub fn list_models(&self) -> Result<Vec<ModelInfo>, NxuskitError> {
        Self::list_models_with_handle(self.handle)
    }

    // ------------------------------------------------------------------
    // Introspection APIs
    // ------------------------------------------------------------------

    /// Return the ABI version string (e.g. `"0.8"`).
    ///
    /// This is a static string owned by the SDK — do not free.
    pub fn abi_version() -> Result<String, NxuskitError> {
        #[cfg(feature = "dynamic-link")]
        ffi::dynamic::sdk()?;

        let ptr = unsafe { Self::call_abi_version() };
        if ptr.is_null() {
            return Err(NxuskitError::Internal {
                message: "nxuskit_abi_version() returned null".into(),
            });
        }
        let s = unsafe { CStr::from_ptr(ptr) }
            .to_str()
            .map_err(|e| NxuskitError::Internal {
                message: format!("abi_version string is not valid UTF-8: {e}"),
            })?
            .to_owned();
        Ok(s)
    }

    /// Return the edition string (e.g. `"oss"`, `"pro"`, `"enterprise"`).
    ///
    /// This is a static string owned by the SDK — do not free.
    pub fn edition() -> Result<String, NxuskitError> {
        #[cfg(feature = "dynamic-link")]
        ffi::dynamic::sdk()?;

        let ptr = unsafe { Self::call_edition() };
        if ptr.is_null() {
            return Err(NxuskitError::Internal {
                message: "nxuskit_edition() returned null".into(),
            });
        }
        let s = unsafe { CStr::from_ptr(ptr) }
            .to_str()
            .map_err(|e| NxuskitError::Internal {
                message: format!("edition string is not valid UTF-8: {e}"),
            })?
            .to_owned();
        Ok(s)
    }

    /// Return the full runtime capabilities manifest.
    pub fn capabilities() -> Result<Capabilities, NxuskitError> {
        #[cfg(feature = "dynamic-link")]
        ffi::dynamic::sdk()?;

        let ptr = unsafe { Self::call_capabilities() };
        if ptr.is_null() {
            let err_msg = Self::last_error_string();
            return Err(NxuskitError::from_json_str(
                &err_msg.unwrap_or_else(|| "unknown error querying capabilities".into()),
            ));
        }

        let json_str = unsafe { CStr::from_ptr(ptr) }
            .to_str()
            .map_err(|e| {
                unsafe { Self::call_free_string(ptr) };
                NxuskitError::InvalidResponse {
                    message: format!("capabilities JSON is not valid UTF-8: {e}"),
                }
            })?
            .to_owned();

        unsafe { Self::call_free_string(ptr) };

        serde_json::from_str::<Capabilities>(&json_str).map_err(|e| NxuskitError::InvalidResponse {
            message: format!("failed to parse capabilities: {e}"),
        })
    }

    // ------------------------------------------------------------------
    // Internal helpers (handle-based, shared by sync + async paths)
    // ------------------------------------------------------------------

    fn chat_with_handle(
        handle: *mut ffi::NxuskitProvider,
        request: ChatRequest,
    ) -> Result<ChatResponse, NxuskitError> {
        let req_json = serde_json::to_string(&request).map_err(|e| NxuskitError::Internal {
            message: format!("failed to serialize request: {e}"),
        })?;
        let c_json = CString::new(req_json).map_err(|e| NxuskitError::Internal {
            message: format!("request contains null byte: {e}"),
        })?;

        let response = unsafe { Self::call_chat(handle, c_json.as_ptr()) };
        if response.is_null() {
            let err_msg = Self::last_error_string();
            return Err(NxuskitError::from_json_str(
                &err_msg.unwrap_or_else(|| "unknown error during chat".into()),
            ));
        }

        let json_ptr = unsafe { Self::call_response_json(response) };
        let json_str = if json_ptr.is_null() {
            unsafe { Self::call_free_response(response) };
            return Err(NxuskitError::InvalidResponse {
                message: "response JSON pointer was null".into(),
            });
        } else {
            unsafe { CStr::from_ptr(json_ptr) }
                .to_str()
                .map_err(|e| NxuskitError::InvalidResponse {
                    message: format!("response JSON is not valid UTF-8: {e}"),
                })?
                .to_owned()
        };

        unsafe { Self::call_free_response(response) };

        // Check if the C ABI returned an error wrapped as a response.
        // Error format: {"content":"","model":"","provider":"","error":{"error_type":"...","message":"..."}}
        if let Ok(raw) = serde_json::from_str::<serde_json::Value>(&json_str) {
            if let Some(err_val) = raw.get("error") {
                if err_val.is_object() {
                    return Err(NxuskitError::from_json(err_val));
                }
            }
        }

        serde_json::from_str::<ChatResponse>(&json_str).map_err(|e| NxuskitError::InvalidResponse {
            message: format!("failed to parse response: {e}"),
        })
    }

    fn list_models_with_handle(
        handle: *mut ffi::NxuskitProvider,
    ) -> Result<Vec<ModelInfo>, NxuskitError> {
        let ptr = unsafe { Self::call_list_models(handle) };
        if ptr.is_null() {
            let err_msg = Self::last_error_string();
            return Err(NxuskitError::from_json_str(
                &err_msg.unwrap_or_else(|| "unknown error listing models".into()),
            ));
        }

        let json_str = unsafe { CStr::from_ptr(ptr) }
            .to_str()
            .map_err(|e| {
                unsafe { Self::call_free_string(ptr) };
                NxuskitError::InvalidResponse {
                    message: format!("models JSON is not valid UTF-8: {e}"),
                }
            })?
            .to_owned();

        unsafe { Self::call_free_string(ptr) };

        serde_json::from_str::<Vec<ModelInfo>>(&json_str).map_err(|e| {
            NxuskitError::InvalidResponse {
                message: format!("failed to parse models list: {e}"),
            }
        })
    }

    // ------------------------------------------------------------------
    // Internal helpers (sendable wrappers for spawn_blocking)
    // ------------------------------------------------------------------

    fn chat_with_sendable(
        handle: SendableHandle,
        request: ChatRequest,
    ) -> Result<ChatResponse, NxuskitError> {
        Self::chat_with_handle(handle.0, request)
    }

    fn list_models_with_sendable(handle: SendableHandle) -> Result<Vec<ModelInfo>, NxuskitError> {
        Self::list_models_with_handle(handle.0)
    }

    // ------------------------------------------------------------------
    // Internal helpers (other)
    // ------------------------------------------------------------------

    fn sdk_version_str() -> Result<String, NxuskitError> {
        let ptr = unsafe { Self::call_version() };
        if ptr.is_null() {
            return Err(NxuskitError::Internal {
                message: "nxuskit_version() returned null".into(),
            });
        }
        // The version string is static in the SDK; we must not free it.
        let s = unsafe { CStr::from_ptr(ptr) }
            .to_str()
            .map_err(|e| NxuskitError::Internal {
                message: format!("version string is not valid UTF-8: {e}"),
            })?
            .to_owned();
        Ok(s)
    }

    fn last_error_string() -> Option<String> {
        let ptr = unsafe { Self::call_last_error() };
        if ptr.is_null() {
            return None;
        }
        let s = unsafe { CStr::from_ptr(ptr) }.to_str().ok()?.to_owned();
        Some(s)
    }

    // ------------------------------------------------------------------
    // FFI dispatch (dynamic vs static)
    // ------------------------------------------------------------------

    #[cfg(feature = "static-link")]
    unsafe fn call_version() -> *const std::ffi::c_char {
        unsafe { ffi::nxuskit_version() }
    }

    #[cfg(feature = "static-link")]
    unsafe fn call_abi_version() -> *const std::ffi::c_char {
        unsafe { ffi::nxuskit_abi_version() }
    }

    #[cfg(feature = "static-link")]
    unsafe fn call_edition() -> *const std::ffi::c_char {
        unsafe { ffi::nxuskit_edition() }
    }

    #[cfg(feature = "static-link")]
    unsafe fn call_capabilities() -> *mut std::ffi::c_char {
        unsafe { ffi::nxuskit_capabilities() }
    }

    #[cfg(feature = "static-link")]
    unsafe fn call_create_provider(
        config_json: *const std::ffi::c_char,
    ) -> *mut ffi::NxuskitProvider {
        unsafe { ffi::nxuskit_create_provider(config_json) }
    }

    #[cfg(feature = "static-link")]
    unsafe fn call_free_provider(provider: *mut ffi::NxuskitProvider) {
        unsafe { ffi::nxuskit_free_provider(provider) }
    }

    #[cfg(feature = "static-link")]
    unsafe fn call_chat(
        provider: *mut ffi::NxuskitProvider,
        request_json: *const std::ffi::c_char,
    ) -> *mut ffi::NxuskitResponse {
        unsafe { ffi::nxuskit_chat(provider, request_json) }
    }

    #[cfg(feature = "static-link")]
    unsafe fn call_response_json(response: *mut ffi::NxuskitResponse) -> *const std::ffi::c_char {
        unsafe { ffi::nxuskit_response_json(response) }
    }

    #[cfg(feature = "static-link")]
    unsafe fn call_free_response(response: *mut ffi::NxuskitResponse) {
        unsafe { ffi::nxuskit_free_response(response) }
    }

    #[cfg(feature = "static-link")]
    unsafe fn call_list_models(provider: *mut ffi::NxuskitProvider) -> *mut std::ffi::c_char {
        unsafe { ffi::nxuskit_list_models(provider) }
    }

    #[cfg(feature = "static-link")]
    unsafe fn call_last_error() -> *const std::ffi::c_char {
        unsafe { ffi::nxuskit_last_error() }
    }

    #[cfg(feature = "static-link")]
    unsafe fn call_free_string(ptr: *mut std::ffi::c_char) {
        unsafe { ffi::nxuskit_free_string(ptr) }
    }

    // --- Dynamic-link dispatch ---
    // All dynamic-link dispatch functions use `sdk_unchecked()` because the SDK
    // is eagerly validated in `NxuskitProvider::new()`. After construction, the
    // OnceLock is guaranteed to be populated.

    #[cfg(feature = "dynamic-link")]
    unsafe fn call_version() -> *const std::ffi::c_char {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_version)() }
    }

    #[cfg(feature = "dynamic-link")]
    unsafe fn call_abi_version() -> *const std::ffi::c_char {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_abi_version)() }
    }

    #[cfg(feature = "dynamic-link")]
    unsafe fn call_edition() -> *const std::ffi::c_char {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_edition)() }
    }

    #[cfg(feature = "dynamic-link")]
    unsafe fn call_capabilities() -> *mut std::ffi::c_char {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_capabilities)() }
    }

    #[cfg(feature = "dynamic-link")]
    unsafe fn call_create_provider(
        config_json: *const std::ffi::c_char,
    ) -> *mut ffi::NxuskitProvider {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_create_provider)(config_json) }
    }

    #[cfg(feature = "dynamic-link")]
    unsafe fn call_free_provider(provider: *mut ffi::NxuskitProvider) {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_free_provider)(provider) }
    }

    #[cfg(feature = "dynamic-link")]
    unsafe fn call_chat(
        provider: *mut ffi::NxuskitProvider,
        request_json: *const std::ffi::c_char,
    ) -> *mut ffi::NxuskitResponse {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_chat)(provider, request_json) }
    }

    #[cfg(feature = "dynamic-link")]
    unsafe fn call_response_json(response: *mut ffi::NxuskitResponse) -> *const std::ffi::c_char {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_response_json)(response) }
    }

    #[cfg(feature = "dynamic-link")]
    unsafe fn call_free_response(response: *mut ffi::NxuskitResponse) {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_free_response)(response) }
    }

    #[cfg(feature = "dynamic-link")]
    unsafe fn call_list_models(provider: *mut ffi::NxuskitProvider) -> *mut std::ffi::c_char {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_list_models)(provider) }
    }

    #[cfg(feature = "dynamic-link")]
    unsafe fn call_last_error() -> *const std::ffi::c_char {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_last_error)() }
    }

    #[cfg(feature = "dynamic-link")]
    unsafe fn call_free_string(ptr: *mut std::ffi::c_char) {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_free_string)(ptr) }
    }
}

impl Drop for NxuskitProvider {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { Self::call_free_provider(self.handle) };
        }
    }
}
