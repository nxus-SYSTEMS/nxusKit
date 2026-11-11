//! Endpoint-only discovery ABI. No inference handle, callback or borrowed config.
use crate::{error, runtime};
use futures::FutureExt;
use nxuskit_engine::discovery::{DiscoveryClient, DiscoveryClientFactory, DiscoveryConfig};
use nxuskit_engine::error::ProviderError;
use std::ffi::{CStr, CString, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Opaque caller-owned handle. Serialize free against other calls on this handle.
/// Started operations retain their own client Arc and survive handle free.
#[derive(Debug)]
pub struct NxuskitDiscovery {
    client: Arc<DiscoveryClient>,
}
/// Caller-owned operation. Serialize free against cancel/wait/result.
#[derive(Debug)]
pub struct NxuskitDiscoveryOperation {
    cancel: CancellationToken,
    result: Arc<(Mutex<Option<CString>>, Condvar)>,
}

/// Config is exactly provider_id, endpoint, optional auth_selector,
/// connect_timeout_ms and read_timeout_ms. No model or inference factory.
/// Returns NULL on rejection, with thread-local error. Config is copied.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_discovery_create(config: *const c_char) -> *mut NxuskitDiscovery {
    catch_unwind(AssertUnwindSafe(|| {
        error::clear_last_error();
        let parsed = if config.is_null() {
            None
        } else {
            unsafe { CStr::from_ptr(config) }
                .to_str()
                .ok()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        };
        let config = parsed.and_then(|v| {
            let map = v.as_object()?;
            if map.keys().any(|k| {
                !matches!(
                    k.as_str(),
                    "provider_id"
                        | "endpoint"
                        | "auth_selector"
                        | "connect_timeout_ms"
                        | "read_timeout_ms"
                )
            }) {
                return None;
            }
            serde_json::from_value::<DiscoveryConfig>(v).ok()
        });
        let Some(config) = config else {
            error::set_last_error("invalid_request", "Invalid discovery configuration", None);
            return std::ptr::null_mut();
        };
        match DiscoveryClientFactory::create(config) {
            Ok(client) => Box::into_raw(Box::new(NxuskitDiscovery {
                client: Arc::new(client),
            })),
            Err(e) => {
                let code = serde_json::to_value(e.code).unwrap_or_default();
                error::set_last_error(code.as_str().unwrap_or("internal"), &e.message, None);
                std::ptr::null_mut()
            }
        }
    }))
    .unwrap_or_else(|_| {
        error::set_last_error("internal", "Discovery creation failed", None);
        std::ptr::null_mut()
    })
}

/// Mark discovery closed (idempotent); previously started work may complete.
/// This is not operation cancellation. NULL/panic returns false.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_discovery_close(client: *mut NxuskitDiscovery) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        let Some(client) = (unsafe { client.as_ref() }) else {
            return false;
        };
        runtime::get_runtime()
            .block_on(client.client.close())
            .is_ok()
    }))
    .unwrap_or(false)
}

/// Free caller handle; accepted operations retain independent ownership.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_discovery_free(client: *mut NxuskitDiscovery) {
    if !client.is_null() {
        unsafe {
            drop(Box::from_raw(client));
        }
    }
}

/// Start one operation: 0=health, 1=models, 2=snapshot. Other values reject.
/// Result JSON is {"ok":true,"value":...} or {"ok":false,"error":{code,message,...}}.
/// health value is null; snapshot preserves typed API/load metadata. No callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_discovery_operation_start(
    client: *mut NxuskitDiscovery,
    operation: u32,
) -> *mut NxuskitDiscoveryOperation {
    catch_unwind(AssertUnwindSafe(|| {
        error::clear_last_error();
        let Some(client) = (unsafe { client.as_ref() }) else { return std::ptr::null_mut(); };
        if operation > 2 {
            error::set_last_error("invalid_request", "Unknown discovery operation", None);
            return std::ptr::null_mut();
        }
        let client = client.client.clone();
        let cancel = CancellationToken::new();
        let result = Arc::new((Mutex::new(None), Condvar::new()));
        let handle = Box::new(NxuskitDiscoveryOperation { cancel: cancel.clone(), result: result.clone() });
        runtime::get_runtime().spawn(async move {
            let outcome = AssertUnwindSafe(async {
                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => Err(ProviderError::cancelled("Discovery operation cancelled")),
                    value = async {
                        let value = match operation {
                            0 => { client.health().await?; serde_json::Value::Null },
                            1 => serde_json::to_value(client.list_models().await?).map_err(|_| ProviderError::internal("Discovery serialization failed"))?,
                            _ => serde_json::to_value(client.snapshot().await?).map_err(|_| ProviderError::internal("Discovery serialization failed"))?,
                        };
                        Ok(value)
                    } => value,
                }
            }).catch_unwind().await.unwrap_or_else(|_| Err(ProviderError::internal("Discovery operation failed")));
            // Losing transport future has been dropped before publishing completion.
            let envelope = match outcome {
                Ok(value) => serde_json::json!({"ok":true,"value":value}),
                Err(error) => serde_json::json!({"ok":false,"error":error}),
            };
            let encoded = CString::new(envelope.to_string()).expect("JSON escapes NUL");
            *result.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(encoded);
            result.1.notify_all();
        });
        Box::into_raw(handle)
    })).unwrap_or_else(|_| {
        error::set_last_error("internal", "Discovery scheduling failed", None);
        std::ptr::null_mut()
    })
}

/// Request cancellation; does not wait or promise remote stop.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_discovery_operation_cancel(op: *mut NxuskitDiscoveryOperation) {
    if let Some(op) = unsafe { op.as_ref() } {
        op.cancel.cancel();
    }
}
/// True once SDK work is quiescent and a result is available. Never on an event-loop thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_discovery_operation_wait(
    op: *mut NxuskitDiscoveryOperation,
    timeout_ms: u32,
) -> bool {
    let Some(op) = (unsafe { op.as_ref() }) else {
        return false;
    };
    let state = op.result.0.lock().unwrap_or_else(|e| e.into_inner());
    op.result
        .1
        .wait_timeout_while(state, Duration::from_millis(u64::from(timeout_ms)), |v| {
            v.is_none()
        })
        .unwrap_or_else(|e| e.into_inner())
        .0
        .is_some()
}
/// Return an owned JSON copy (free with nxuskit_free_string); NULL while pending.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_discovery_operation_result(
    op: *mut NxuskitDiscoveryOperation,
) -> *mut c_char {
    let Some(op) = (unsafe { op.as_ref() }) else {
        return std::ptr::null_mut();
    };
    op.result
        .0
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map(|v| v.clone().into_raw())
        .unwrap_or(std::ptr::null_mut())
}
/// Free only a completed operation; false retains caller ownership.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_discovery_operation_free(
    op: *mut NxuskitDiscoveryOperation,
) -> bool {
    if !unsafe { nxuskit_discovery_operation_wait(op, 0) } {
        return false;
    }
    unsafe {
        drop(Box::from_raw(op));
    }
    true
}
