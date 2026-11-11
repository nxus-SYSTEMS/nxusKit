//! Additive unary operation ownership; legacy callback entry points are unchanged.

use std::ffi::{CStr, c_char, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::ThreadId;
use std::time::Duration;

use futures::FutureExt;
use tokio_util::sync::CancellationToken;

use crate::{NxuskitChatAsyncCallback, error, provider, runtime, types::NxuskitProvider};

#[derive(Debug, Default)]
struct Completion {
    done: bool,
    callback_thread: Option<ThreadId>,
}

/// Caller-owned operation handle. Serialize free against every other handle call.
/// The provider and callback state must remain valid until wait returns true.
#[derive(Debug)]
pub struct NxuskitChatOperation {
    cancel: CancellationToken,
    completion: Arc<(Mutex<Completion>, Condvar)>,
}

/// Start a unary request with an SDK work deadline (1..=u32::MAX milliseconds).
/// The clock starts on entry, including runtime queue delay. Expiry cooperatively
/// drops SDK work when the runtime polls it; callback return remains unbounded by
/// this deadline and must be acknowledged separately before freeing borrowed state.
/// NULL means rejected, with no callback. Otherwise exactly one terminal callback
/// occurs, including cancellation/deadline. No remote-stop guarantee is implied.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_chat_operation_start(
    provider: *mut NxuskitProvider,
    request: *const c_char,
    deadline_ms: u64,
    callback: NxuskitChatAsyncCallback,
    data: *mut c_void,
) -> *mut NxuskitChatOperation {
    let accepted_at = tokio::time::Instant::now();
    catch_unwind(AssertUnwindSafe(|| {
        if provider.is_null() || request.is_null() || callback.is_none()
            || deadline_ms == 0 || deadline_ms > u64::from(u32::MAX)
        {
            error::set_last_error("invalid_argument", "invalid chat operation argument", None);
            return std::ptr::null_mut();
        }
        let request_json = match unsafe { CStr::from_ptr(request) }.to_str() {
            Ok(request_json) => request_json,
            Err(_) => {
                error::set_last_error("invalid_argument", "invalid chat operation JSON", None);
                return std::ptr::null_mut();
            }
        };
        let request_value: serde_json::Value = match serde_json::from_str(request_json) {
            Ok(value) => value,
            Err(_) => {
                error::set_last_error("invalid_argument", "invalid chat operation JSON", None);
                return std::ptr::null_mut();
            }
        };
        let thinking_mode_explicit = request_value.get("thinking_mode").is_some();
        let request: nxuskit_engine::ChatRequest = match serde_json::from_value(request_value) {
            Ok(request) => request,
            Err(_) => {
                error::set_last_error("invalid_argument", "invalid chat operation JSON", None);
                return std::ptr::null_mut();
            }
        };
        let deadline = accepted_at + Duration::from_millis(deadline_ms);
        let cancel = CancellationToken::new();
        let completion = Arc::new((Mutex::new(Completion::default()), Condvar::new()));
        let operation = Box::new(NxuskitChatOperation { cancel: cancel.clone(), completion: completion.clone() });
        let provider = provider as usize;
        let data = data as usize;
        let callback = callback.expect("validated callback");
        runtime::get_runtime().spawn(async move {
            // The losing chat future is dropped before invoking the callback, so
            // owned transport/retry work ends before terminal settlement.
            let result = AssertUnwindSafe(async {
                // A just-registered timer may need a driver turn even when past
                // due. Do not poll ready provider work ahead of an expired clock.
                if tokio::time::Instant::now() >= deadline {
                    return Err(error::AsyncCallbackError::deadline());
                }
                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => Err(error::AsyncCallbackError::cancelled()),
                    _ = tokio::time::sleep_until(deadline) => Err(error::AsyncCallbackError::deadline()),
                    result = provider::chat_for_callback(
                        unsafe { &*(provider as *mut NxuskitProvider) },
                        request,
                        thinking_mode_explicit,
                    ) => result,
                }
            }).catch_unwind().await.unwrap_or_else(|_| Err(error::AsyncCallbackError::internal("chat operation panicked")));
            completion.0.lock().unwrap_or_else(|e| e.into_inner()).callback_thread = Some(std::thread::current().id());
            match result {
                Ok(response) => unsafe { callback(0, Box::into_raw(Box::new(response)), std::ptr::null(), data as *mut c_void) },
                Err(error) => unsafe { callback(error.status(), std::ptr::null_mut(), error.as_ptr(), data as *mut c_void) },
            }
            let mut state = completion.0.lock().unwrap_or_else(|e| e.into_inner());
            state.callback_thread = None;
            state.done = true;
            completion.1.notify_all();
        });
        Box::into_raw(operation)
    })).unwrap_or_else(|_| {
        error::set_last_error("internal_error", "chat operation scheduling panicked", None);
        std::ptr::null_mut()
    })
}

/// Request cancellation; does not wait, free, or promise a remote service stop.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_chat_operation_cancel(operation: *mut NxuskitChatOperation) {
    if let Some(operation) = unsafe { operation.as_ref() } {
        operation.cancel.cancel();
    }
}

/// Wait for callback return. False means still borrowed (or NULL/callback-thread
/// invocation). Never call with a freed handle; never block an event-loop thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_chat_operation_wait(
    operation: *mut NxuskitChatOperation,
    timeout_ms: u32,
) -> bool {
    let Some(operation) = (unsafe { operation.as_ref() }) else {
        return false;
    };
    let state = operation
        .completion
        .0
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if state.callback_thread == Some(std::thread::current().id()) {
        return false;
    }
    operation
        .completion
        .1
        .wait_timeout_while(
            state,
            Duration::from_millis(u64::from(timeout_ms)),
            |state| !state.done,
        )
        .unwrap_or_else(|e| e.into_inner())
        .0
        .done
}

/// Free only after callback quiescence; false leaves ownership with the caller.
/// Serialize this call with cancel/wait/free on the same handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_chat_operation_free(operation: *mut NxuskitChatOperation) -> bool {
    if !unsafe { nxuskit_chat_operation_wait(operation, 0) } {
        return false;
    }
    unsafe {
        drop(Box::from_raw(operation));
    }
    true
}
