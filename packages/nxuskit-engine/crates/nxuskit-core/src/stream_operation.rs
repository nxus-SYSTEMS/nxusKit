//! Additive owned streaming. The legacy stream ABI is deliberately unchanged.

use std::ffi::{CStr, CString, c_char, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::ThreadId;
use std::time::Duration;

use futures::{FutureExt, StreamExt};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{error, runtime, types::NxuskitProvider};
use error::AsyncCallbackError;

/// Borrowed chunk JSON, valid only during this callback. Nonzero requests cancel.
pub type NxuskitStreamOperationChunkCallback =
    Option<unsafe extern "C" fn(*const c_char, *mut c_void) -> i32>;
/// Exactly one terminal: status 0/error NULL is success; otherwise error is a
/// borrowed typed JSON object with code/message, valid only during the callback.
pub type NxuskitStreamOperationDoneCallback =
    Option<unsafe extern "C" fn(i32, *const c_char, *mut c_void)>;

#[derive(Default)]
struct State {
    cancelled: bool,
    terminal_admitted: bool,
    done: bool,
    callback_thread: Option<ThreadId>,
    producer_result: Option<Result<(), AsyncCallbackError>>,
}

struct Shared {
    cancel: CancellationToken,
    state: Mutex<State>,
    changed: Condvar,
}

impl Shared {
    fn cancel(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if !state.terminal_admitted {
            state.cancelled = true;
            self.cancel.cancel();
        }
    }

    fn producer_finished(&self, result: Result<(), AsyncCallbackError>) {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .producer_result = Some(result);
        self.changed.notify_all();
    }
}

/// Caller-owned handle. Provider and callback/user-data borrows last until wait
/// returns true, including after cancel. Serialize free against all handle calls.
/// Dropping/freeing does not cancel implicitly or block a destructor.
pub struct NxuskitStreamOperation {
    shared: Arc<Shared>,
}

impl std::fmt::Debug for NxuskitStreamOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NxuskitStreamOperation")
            .finish_non_exhaustive()
    }
}

async fn forward(
    mut stream: Box<
        dyn futures::Stream<Item = nxuskit_engine::Result<nxuskit_engine::StreamChunk>>
            + Send
            + Unpin,
    >,
    sender: &mpsc::Sender<CString>,
) -> Result<(), AsyncCallbackError> {
    let mut index = 0_u64;
    loop {
        // Reserve before polling upstream: no seventeenth prefetched SDK event.
        let permit = sender
            .reserve()
            .await
            .map_err(|_| AsyncCallbackError::cancelled())?;
        let Some(chunk) = stream.next().await else {
            return Ok(());
        };
        let chunk = chunk.map_err(|error| AsyncCallbackError::from_provider(&error))?;
        let mut json = serde_json::to_value(chunk)
            .map_err(|_| AsyncCallbackError::internal("stream chunk serialization failed"))?;
        json["index"] = index.into();
        index += 1;
        let json = CString::new(json.to_string())
            .map_err(|_| AsyncCallbackError::internal("stream chunk encoding failed"))?;
        permit.send(json);
    }
}

fn consume(
    mut receiver: mpsc::Receiver<CString>,
    shared: Arc<Shared>,
    chunk: unsafe extern "C" fn(*const c_char, *mut c_void) -> i32,
    done: unsafe extern "C" fn(i32, *const c_char, *mut c_void),
    data: usize,
) {
    let thread = std::thread::current().id();
    while let Some(json) = receiver.blocking_recv() {
        {
            let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
            // Cancel and callback admission linearize under this short lock.
            // A callback already admitted may finish after cancel returns.
            if state.cancelled || state.producer_result.as_ref().is_some_and(Result::is_err) {
                break;
            }
            state.callback_thread = Some(thread);
        }
        let stop = unsafe { chunk(json.as_ptr(), data as *mut c_void) };
        shared
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .callback_thread = None;
        if stop != 0 {
            shared.cancel();
        }
    }
    // Wake a parked send before waiting for producer quiescence.
    drop(receiver);
    let result = {
        let state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
        let mut state = shared
            .changed
            .wait_while(state, |s| s.producer_result.is_none())
            .unwrap_or_else(|e| e.into_inner());
        let result = state.producer_result.take().expect("producer completed");
        let result = if state.cancelled {
            Err(AsyncCallbackError::cancelled())
        } else {
            result
        };
        // After this point cancel is a no-op. No mutable success/error race.
        state.terminal_admitted = true;
        state.callback_thread = Some(thread);
        result
    };
    match result {
        Ok(()) => unsafe { done(0, std::ptr::null(), data as *mut c_void) },
        Err(error) => unsafe { done(error.status(), error.as_ptr(), data as *mut c_void) },
    }
    let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
    state.callback_thread = None;
    state.done = true;
    shared.changed.notify_all();
}

/// Return an owned handle without waiting for network establishment. NULL means
/// rejected with no callbacks. Otherwise callbacks run serially on a dedicated
/// thread, never inline. At most 16 pending SDK chunk events are queued (plus
/// one admitted callback); this is not a byte/socket/parser-buffer bound.
///
/// The absolute SDK-work deadline starts on entry, includes scheduling and
/// backpressure, and cooperatively drops establishment/stream work. Neither
/// cancel nor deadline interrupts foreign callbacks or guarantees a remote stop.
/// Cancellation before terminal admission wins over buffered success; after
/// terminal admission it is a no-op. A previously admitted chunk may finish.
/// Provider and callback/data must remain valid until wait acknowledges callback
/// return. Do not block an event loop or a callback waiting on this operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_stream_operation_start(
    provider: *mut NxuskitProvider,
    request: *const c_char,
    deadline_ms: u64,
    on_chunk: NxuskitStreamOperationChunkCallback,
    on_done: NxuskitStreamOperationDoneCallback,
    data: *mut c_void,
) -> *mut NxuskitStreamOperation {
    let accepted_at = tokio::time::Instant::now();
    catch_unwind(AssertUnwindSafe(|| {
        error::clear_last_error();
        if provider.is_null() || request.is_null() || on_chunk.is_none() || on_done.is_none() || deadline_ms == 0 || deadline_ms > u64::from(u32::MAX) {
            error::set_last_error("invalid_argument", "invalid stream operation argument", None);
            return std::ptr::null_mut();
        }
        let request: nxuskit_engine::ChatRequest = match unsafe { CStr::from_ptr(request) }.to_str().ok().and_then(|s| serde_json::from_str(s).ok()) {
            Some(request) => request,
            None => {
                error::set_last_error("invalid_argument", "invalid stream operation JSON", None);
                return std::ptr::null_mut();
            }
        };
        let runtime = runtime::get_runtime();
        let deadline = accepted_at + Duration::from_millis(deadline_ms);
        let shared = Arc::new(Shared { cancel: CancellationToken::new(), state: Mutex::new(State::default()), changed: Condvar::new() });
        let operation = Box::new(NxuskitStreamOperation { shared: shared.clone() });
        let (sender, receiver) = mpsc::channel(16);
        let consumer = shared.clone();
        let data = data as usize;
        // Start the consumer first. Failure rejects without launching any work.
        if std::thread::Builder::new().name("nxuskit-stream-callback".into()).spawn(move || consume(receiver, consumer, on_chunk.unwrap(), on_done.unwrap(), data)).is_err() {
            error::set_last_error("internal_error", "stream callback thread creation failed", None);
            return std::ptr::null_mut();
        }
        let producer = shared.clone();
        let provider = provider as usize;
        // Once the consumer exists, even a scheduling failure is an accepted
        // operation with one internal-error terminal, never NULL plus callbacks.
        let scheduled = catch_unwind(AssertUnwindSafe(|| runtime.spawn(async move {
            let result = AssertUnwindSafe(async {
                if producer.cancel.is_cancelled() { return Err(AsyncCallbackError::cancelled()); }
                if tokio::time::Instant::now() >= deadline { return Err(AsyncCallbackError::deadline()); }
                tokio::select! {
                    biased;
                    _ = producer.cancel.cancelled() => Err(AsyncCallbackError::cancelled()),
                    _ = tokio::time::sleep_until(deadline) => Err(AsyncCallbackError::deadline()),
                    result = async {
                        // SAFETY: caller retains this provider through wait=true.
                        let stream = unsafe { &*(provider as *mut NxuskitProvider) }.inner().chat_stream(&request).await.map_err(|error| AsyncCallbackError::from_provider(&error))?;
                        forward(stream, &sender).await
                    } => result,
                }
            }).catch_unwind().await.unwrap_or_else(|_| Err(AsyncCallbackError::internal("stream producer panicked")));
            // All producer futures and their transport borrows have been dropped.
            producer.producer_finished(result);
            drop(sender);
        })));
        if scheduled.is_err() { shared.producer_finished(Err(AsyncCallbackError::internal("stream scheduling failed"))); }
        Box::into_raw(operation)
    })).unwrap_or_else(|_| {
        error::set_last_error("internal_error", "stream operation setup panicked", None);
        std::ptr::null_mut()
    })
}

/// Nonblocking cancellation request. Never frees, joins or interrupts a callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_stream_operation_cancel(operation: *mut NxuskitStreamOperation) {
    if let Some(operation) = unsafe { operation.as_ref() } {
        operation.shared.cancel();
    }
}

/// True only after producer quiescence and terminal callback return. NULL and
/// callback-thread invocation return false. Caller must not block an event loop.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_stream_operation_wait(
    operation: *mut NxuskitStreamOperation,
    timeout_ms: u32,
) -> bool {
    let Some(operation) = (unsafe { operation.as_ref() }) else {
        return false;
    };
    let shared = &operation.shared;
    let state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
    if state.callback_thread == Some(std::thread::current().id()) {
        return false;
    }
    shared
        .changed
        .wait_timeout_while(state, Duration::from_millis(u64::from(timeout_ms)), |s| {
            !s.done
        })
        .unwrap_or_else(|e| e.into_inner())
        .0
        .done
}

/// Free only after callback return. False leaves ownership intact. Serialize
/// against all other calls using this handle; never use the handle after true.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nxuskit_stream_operation_free(
    operation: *mut NxuskitStreamOperation,
) -> bool {
    if !unsafe { nxuskit_stream_operation_wait(operation, 0) } {
        return false;
    }
    unsafe {
        drop(Box::from_raw(operation));
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::{Context, Poll};

    struct CountedStream {
        polls: Arc<AtomicUsize>,
        full: std::sync::mpsc::Sender<()>,
        dropped: std::sync::mpsc::Sender<()>,
    }
    impl futures::Stream for CountedStream {
        type Item = nxuskit_engine::Result<nxuskit_engine::StreamChunk>;
        fn poll_next(
            self: std::pin::Pin<&mut Self>,
            _: &mut Context<'_>,
        ) -> Poll<Option<Self::Item>> {
            let n = self.polls.fetch_add(1, Ordering::SeqCst) + 1;
            if n == 17 {
                let _ = self.full.send(());
            }
            Poll::Ready(Some(Ok(nxuskit_engine::StreamChunk::new("literal".into()))))
        }
    }
    impl Drop for CountedStream {
        fn drop(&mut self) {
            let _ = self.dropped.send(());
        }
    }
    struct CountedProvider(Mutex<Option<CountedStream>>);
    // Match the existing async-trait ABI without adding a dependency for tests.
    impl nxuskit_engine::LLMProvider for CountedProvider {
        fn chat<'a, 'b, 'f>(
            &'a self,
            _: &'b nxuskit_engine::ChatRequest,
        ) -> std::pin::Pin<
            Box<
                dyn Future<Output = nxuskit_engine::Result<nxuskit_engine::ChatResponse>>
                    + Send
                    + 'f,
            >,
        >
        where
            'a: 'f,
            'b: 'f,
            Self: 'f,
        {
            Box::pin(async {
                Err(nxuskit_engine::NxuskitError::InvalidRequest(
                    "not a unary fixture".into(),
                ))
            })
        }
        fn chat_stream<'a, 'b, 'f>(
            &'a self,
            _: &'b nxuskit_engine::ChatRequest,
        ) -> std::pin::Pin<
            Box<
                dyn Future<
                        Output = nxuskit_engine::Result<
                            Box<
                                dyn futures::Stream<
                                        Item = nxuskit_engine::Result<nxuskit_engine::StreamChunk>,
                                    > + Send
                                    + Unpin,
                            >,
                        >,
                    > + Send
                    + 'f,
            >,
        >
        where
            'a: 'f,
            'b: 'f,
            Self: 'f,
        {
            let stream = self.0.lock().unwrap().take().unwrap();
            Box::pin(async move {
                Ok(Box::new(stream)
                    as Box<
                        dyn futures::Stream<
                                Item = nxuskit_engine::Result<nxuskit_engine::StreamChunk>,
                            > + Send
                            + Unpin,
                    >)
            })
        }
        fn provider_name(&self) -> &str {
            "counted-local-fixture"
        }
    }

    struct BarrierCallbacks {
        entered: std::sync::mpsc::Sender<()>,
        release: Mutex<std::sync::mpsc::Receiver<()>>,
        terminal: std::sync::mpsc::Sender<String>,
        chunks: AtomicUsize,
        returned: std::sync::atomic::AtomicBool,
        released: std::sync::atomic::AtomicBool,
    }
    unsafe extern "C" fn barrier_chunk(_: *const c_char, data: *mut c_void) -> i32 {
        let state = unsafe { &*data.cast::<BarrierCallbacks>() };
        state.chunks.fetch_add(1, Ordering::SeqCst);
        let _ = state.entered.send(());
        let released = state
            .release
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .is_ok();
        state.released.store(released, Ordering::SeqCst);
        state.returned.store(true, Ordering::SeqCst);
        0
    }
    unsafe extern "C" fn barrier_terminal(_: i32, error: *const c_char, data: *mut c_void) {
        let state = unsafe { &*data.cast::<BarrierCallbacks>() };
        let text = if error.is_null() {
            String::new()
        } else {
            unsafe { CStr::from_ptr(error) }
                .to_string_lossy()
                .into_owned()
        };
        let _ = state.terminal.send(text);
    }

    #[test]
    fn full_queue_cancel_drops_upstream_while_foreign_callback_is_still_held() {
        let polls = Arc::new(AtomicUsize::new(0));
        let (full, at_capacity) = std::sync::mpsc::channel();
        let (dropped, drop_seen) = std::sync::mpsc::channel();
        // Retain these allocations on assertion failure: detached work may
        // still borrow them. Reclaim only after confirmed native quiescence.
        let provider = Box::into_raw(Box::new(NxuskitProvider::new(Box::new(CountedProvider(
            Mutex::new(Some(CountedStream {
                polls: polls.clone(),
                full,
                dropped,
            })),
        )))));
        let (entered, callback_entered) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel();
        let (terminal, terminal_seen) = std::sync::mpsc::channel();
        let state_ptr = Box::into_raw(Box::new(BarrierCallbacks {
            entered,
            release: Mutex::new(released),
            terminal,
            chunks: AtomicUsize::new(0),
            returned: false.into(),
            released: false.into(),
        }));
        let state = unsafe { &*state_ptr };
        let request = c"{\"model\":\"local\",\"messages\":[]}";
        unsafe {
            let operation = nxuskit_stream_operation_start(
                provider,
                request.as_ptr(),
                5000,
                Some(barrier_chunk),
                Some(barrier_terminal),
                state_ptr.cast(),
            );
            assert!(!operation.is_null());
            callback_entered
                .recv_timeout(Duration::from_secs(3))
                .unwrap();
            at_capacity.recv_timeout(Duration::from_secs(3)).unwrap();
            nxuskit_stream_operation_cancel(operation);
            // The real producer has dropped the stream, without releasing the
            // held callback or making a queue slot available.
            drop_seen.recv_timeout(Duration::from_secs(3)).unwrap();
            assert!(
                !state.returned.load(Ordering::SeqCst),
                "callback hold expired before upstream drop proof"
            );
            assert!(!nxuskit_stream_operation_wait(operation, 0));
            assert!(!nxuskit_stream_operation_free(operation));
            assert!(terminal_seen.try_recv().is_err());
            release.send(()).unwrap();
            let error = terminal_seen.recv_timeout(Duration::from_secs(3)).unwrap();
            assert!(nxuskit_stream_operation_wait(operation, 3000));
            assert!(nxuskit_stream_operation_free(operation));
            assert!(
                state.released.load(Ordering::SeqCst),
                "callback must return through explicit release, not timeout"
            );
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&error).unwrap()["code"],
                "cancelled"
            );
        }
        assert_eq!(state.chunks.load(Ordering::SeqCst), 1);
        assert_eq!(
            polls.load(Ordering::SeqCst),
            17,
            "one held callback plus sixteen pending, no hidden prefetch"
        );
        unsafe {
            drop(Box::from_raw(state_ptr));
            drop(Box::from_raw(provider));
        }
    }

    #[test]
    fn upstream_is_not_polled_past_sixteen_pending_events() {
        let polls = Arc::new(AtomicUsize::new(0));
        let seen = polls.clone();
        let stream = futures::stream::poll_fn(move |_| {
            seen.fetch_add(1, Ordering::SeqCst);
            Poll::Ready(Some(Ok(nxuskit_engine::StreamChunk::new("literal".into()))))
        });
        let (sender, mut receiver) = mpsc::channel(16);
        let mut work = Box::pin(forward(Box::new(stream), &sender));
        let mut cx = Context::from_waker(futures::task::noop_waker_ref());
        assert!(work.as_mut().poll(&mut cx).is_pending());
        assert_eq!(polls.load(Ordering::SeqCst), 16);
        let first = receiver.try_recv().unwrap();
        let first: serde_json::Value = serde_json::from_slice(first.as_bytes()).unwrap();
        assert_eq!(first["delta"], "literal");
        assert_eq!(first["index"], 0);
        assert!(work.as_mut().poll(&mut cx).is_pending());
        assert_eq!(
            polls.load(Ordering::SeqCst),
            17,
            "one released slot permits exactly one further upstream event"
        );
        drop(receiver);
        assert!(
            matches!(work.as_mut().poll(&mut cx), Poll::Ready(Err(_))),
            "dropping consumer must wake a capacity-blocked producer"
        );
    }
}
