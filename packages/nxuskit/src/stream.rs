//! Streaming response receiver bridging C callbacks to Rust channels.

use crate::error::NxuskitError;
use crate::ffi;
use crate::types::StreamChunk;
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::pin::Pin;
use std::task::{Context, Poll};

/// Receives incremental stream chunks from a streaming chat request.
///
/// Implements both [`Iterator`] (blocking) and [`futures_core::Stream`] (async).
/// Drop the receiver or call [`cancel`](StreamReceiver::cancel) to stop the stream.
pub struct StreamReceiver {
    rx: tokio::sync::mpsc::Receiver<Result<StreamChunk, NxuskitError>>,
    stream_handle: *mut ffi::NxuskitStream,
}

// SAFETY: The stream handle is only used for cancellation (which is thread-safe
// per the C SDK contract) and for cleanup on drop.
unsafe impl Send for StreamReceiver {}

impl StreamReceiver {
    /// Internal: start a streaming request and return the receiver.
    pub(crate) fn start(
        provider: *mut ffi::NxuskitProvider,
        request_json: CString,
    ) -> Result<Self, NxuskitError> {
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<StreamChunk, NxuskitError>>(32);

        // Box the sender so we can pass it as a raw pointer through C.
        let user_data = Box::into_raw(Box::new(tx)) as *mut c_void;

        let stream_handle = unsafe {
            Self::call_chat_stream(
                provider,
                request_json.as_ptr(),
                on_chunk_callback,
                on_done_callback,
                user_data,
            )
        };

        if stream_handle.is_null() {
            // Reclaim the sender so it doesn't leak.
            let _ = unsafe {
                Box::from_raw(
                    user_data as *mut tokio::sync::mpsc::Sender<Result<StreamChunk, NxuskitError>>,
                )
            };
            let err_msg = Self::last_error_string();
            return Err(NxuskitError::from_json_str(
                &err_msg.unwrap_or_else(|| "unknown error starting stream".into()),
            ));
        }

        Ok(Self { rx, stream_handle })
    }

    /// Receive the next chunk (blocking).
    ///
    /// Returns `None` when the stream is complete.
    pub fn next_chunk(&mut self) -> Option<Result<StreamChunk, NxuskitError>> {
        self.rx.blocking_recv()
    }

    /// Cancel the stream.
    pub fn cancel(&mut self) {
        if !self.stream_handle.is_null() {
            unsafe { Self::call_cancel_stream(self.stream_handle) };
        }
    }

    // ------------------------------------------------------------------
    // FFI dispatch helpers
    // ------------------------------------------------------------------

    fn last_error_string() -> Option<String> {
        let ptr = unsafe { Self::call_last_error() };
        if ptr.is_null() {
            return None;
        }
        let s = unsafe { CStr::from_ptr(ptr) }.to_str().ok()?.to_owned();
        Some(s)
    }

    #[cfg(feature = "static-link")]
    unsafe fn call_chat_stream(
        provider: *mut ffi::NxuskitProvider,
        request_json: *const c_char,
        on_chunk: ffi::NxuskitStreamCallback,
        on_done: ffi::NxuskitStreamDoneCallback,
        user_data: *mut c_void,
    ) -> *mut ffi::NxuskitStream {
        unsafe { ffi::nxuskit_chat_stream(provider, request_json, on_chunk, on_done, user_data) }
    }

    #[cfg(feature = "static-link")]
    unsafe fn call_cancel_stream(stream: *mut ffi::NxuskitStream) {
        unsafe { ffi::nxuskit_cancel_stream(stream) }
    }

    #[cfg(feature = "static-link")]
    unsafe fn call_free_stream(stream: *mut ffi::NxuskitStream) {
        unsafe { ffi::nxuskit_free_stream(stream) }
    }

    #[cfg(feature = "static-link")]
    unsafe fn call_last_error() -> *const c_char {
        unsafe { ffi::nxuskit_last_error() }
    }

    #[cfg(feature = "dynamic-link")]
    unsafe fn call_chat_stream(
        provider: *mut ffi::NxuskitProvider,
        request_json: *const c_char,
        on_chunk: ffi::NxuskitStreamCallback,
        on_done: ffi::NxuskitStreamDoneCallback,
        user_data: *mut c_void,
    ) -> *mut ffi::NxuskitStream {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_chat_stream)(provider, request_json, on_chunk, on_done, user_data) }
    }

    #[cfg(feature = "dynamic-link")]
    unsafe fn call_cancel_stream(stream: *mut ffi::NxuskitStream) {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_cancel_stream)(stream) }
    }

    #[cfg(feature = "dynamic-link")]
    unsafe fn call_free_stream(stream: *mut ffi::NxuskitStream) {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_free_stream)(stream) }
    }

    #[cfg(feature = "dynamic-link")]
    unsafe fn call_last_error() -> *const c_char {
        let sdk = ffi::dynamic::sdk_unchecked();
        unsafe { (sdk.nxuskit_last_error)() }
    }
}

impl Iterator for StreamReceiver {
    type Item = Result<StreamChunk, NxuskitError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_chunk()
    }
}

impl futures_core::Stream for StreamReceiver {
    type Item = Result<StreamChunk, NxuskitError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}

impl Drop for StreamReceiver {
    fn drop(&mut self) {
        if !self.stream_handle.is_null() {
            unsafe { Self::call_free_stream(self.stream_handle) };
        }
    }
}

// ---------------------------------------------------------------------------
// C callback trampolines
// ---------------------------------------------------------------------------

/// Called by the C SDK for each streaming chunk.
/// Returns 0 to continue, non-zero to cancel.
unsafe extern "C" fn on_chunk_callback(chunk_json: *const c_char, user_data: *mut c_void) -> c_int {
    let tx = unsafe {
        &*(user_data as *const tokio::sync::mpsc::Sender<Result<StreamChunk, NxuskitError>>)
    };

    if chunk_json.is_null() {
        let _ = tx.try_send(Err(NxuskitError::Stream {
            message: "chunk callback received null pointer".into(),
        }));
        return 1; // cancel
    }

    let json_str = match unsafe { CStr::from_ptr(chunk_json) }.to_str() {
        Ok(s) => s,
        Err(e) => {
            let _ = tx.try_send(Err(NxuskitError::Stream {
                message: format!("chunk is not valid UTF-8: {e}"),
            }));
            return 1;
        }
    };

    let chunk = match serde_json::from_str::<StreamChunk>(json_str) {
        Ok(c) => c,
        Err(e) => {
            let _ = tx.try_send(Err(NxuskitError::Stream {
                message: format!("failed to parse chunk: {e}"),
            }));
            return 1;
        }
    };

    match tx.try_send(Ok(chunk)) {
        Ok(()) => 0, // continue
        Err(_) => 1, // receiver dropped; cancel stream
    }
}

/// Called by the C SDK when the stream finishes.
/// We drop the Sender to signal stream completion.
unsafe extern "C" fn on_done_callback(_final_json: *const c_char, user_data: *mut c_void) {
    // Reclaim and drop the boxed sender, closing the channel.
    let _ = unsafe {
        Box::from_raw(
            user_data as *mut tokio::sync::mpsc::Sender<Result<StreamChunk, NxuskitError>>,
        )
    };
}
