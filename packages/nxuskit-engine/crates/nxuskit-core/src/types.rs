use std::ffi::CString;
use std::os::raw::c_char;

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// Opaque handle wrapping a boxed provider trait object.
/// Consumers create via `nxuskit_create_provider` and free via `nxuskit_free_provider`.
#[repr(C)]
pub struct NxuskitProvider {
    inner: Box<dyn nxuskit_engine::LLMProvider>,
}

impl NxuskitProvider {
    pub(crate) fn new(provider: Box<dyn nxuskit_engine::LLMProvider>) -> Self {
        Self { inner: provider }
    }

    pub(crate) fn inner(&self) -> &dyn nxuskit_engine::LLMProvider {
        &*self.inner
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
