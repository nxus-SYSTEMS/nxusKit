//! Literal boundary tests: no provider service or credential is used.
use std::ffi::{CStr, CString, c_char, c_void};
use std::sync::mpsc::{Sender, channel};
use std::time::Duration;

use nxuskit_core::*;

// A failed assertion must not destroy data borrowed by detached native work.
// Successful tests release this normally, only after wait/free. On failure the
// test intentionally retains the small allocation rather than cause a UAF.
struct CallbackBorrow<T>(std::mem::ManuallyDrop<Box<T>>);
impl<T> CallbackBorrow<T> {
    fn new(value: T) -> Self {
        Self(std::mem::ManuallyDrop::new(Box::new(value)))
    }
}
impl<T> std::ops::Deref for CallbackBorrow<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}
impl<T> Drop for CallbackBorrow<T> {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            unsafe { std::mem::ManuallyDrop::drop(&mut self.0) };
        }
    }
}

#[test]
fn callback_borrow_survives_assertion_unwind_but_reclaims_after_success() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering::SeqCst},
    };
    struct Witness(Arc<AtomicUsize>);
    impl Drop for Witness {
        fn drop(&mut self) {
            self.0.fetch_add(1, SeqCst);
        }
    }
    let drops = Arc::new(AtomicUsize::new(0));
    let observed = drops.clone();
    assert!(
        std::panic::catch_unwind(move || {
            let _borrow = CallbackBorrow::new(Witness(observed));
            panic!("literal test-failure unwind");
        })
        .is_err()
    );
    assert_eq!(
        drops.load(SeqCst),
        0,
        "live callback borrow must survive a failed test"
    );
    drop(CallbackBorrow::new(Witness(drops.clone())));
    assert_eq!(
        drops.load(SeqCst),
        1,
        "successful quiescent path reclaims its allocation"
    );
}

#[test]
fn start_returns_while_http_establishment_is_held() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (seen, request_seen) = channel();
    let (release, released) = channel();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut buffer = [0; 4096];
        assert!(socket.read(&mut buffer).unwrap() > 0);
        seen.send(()).unwrap();
        released.recv_timeout(Duration::from_secs(3)).unwrap();
        let body = "data: [DONE]\n\n";
        // Once start has returned, cancellation may close the client socket before this
        // release-only fixture response is written. That close is not the behavior under test.
        let _ = write!(
            socket,
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
    });
    let (returned, result) = channel();
    let (finish, finished) = channel();
    let worker = std::thread::spawn(move || {
        let config = CString::new(format!(r#"{{"provider_type":"openai","model":"local","api_key":"local-test-only","base_url":"http://{address}/v1","timeout_ms":5000}}"#)).unwrap();
        let request =
            CString::new(r#"{"model":"local","messages":[{"role":"user","content":"hello"}]}"#)
                .unwrap();
        let (sender, receiver) = channel::<String>();
        let sender = CallbackBorrow::new(sender);
        unsafe {
            let provider = nxuskit_create_provider(config.as_ptr());
            assert!(!provider.is_null());
            let stream = nxuskit_stream_operation_start(
                provider,
                request.as_ptr(),
                5000,
                Some(stop_chunk),
                Some(terminal),
                (&*sender as *const Sender<String>).cast_mut().cast(),
            );
            returned.send(!stream.is_null()).unwrap();
            finished.recv_timeout(Duration::from_secs(3)).unwrap();
            if !stream.is_null() {
                nxuskit_stream_operation_cancel(stream);
                assert!(nxuskit_stream_operation_wait(stream, 3000));
                assert!(nxuskit_stream_operation_free(stream));
            }
            drop(receiver);
            nxuskit_free_provider(provider);
        }
    });
    request_seen.recv_timeout(Duration::from_secs(3)).unwrap();
    let returned_before_headers = result.recv_timeout(Duration::from_millis(150)).ok();
    release.send(()).unwrap();
    if returned_before_headers.is_none() {
        assert!(result.recv_timeout(Duration::from_secs(3)).unwrap());
    }
    finish.send(()).unwrap();
    worker.join().unwrap();
    server.join().unwrap();
    assert_eq!(
        returned_before_headers,
        Some(true),
        "start must return a cancelable handle before response headers"
    );
}

unsafe extern "C" fn stop_chunk(_: *const c_char, _: *mut c_void) -> i32 {
    1
}
unsafe extern "C" fn terminal(_status: i32, json: *const c_char, data: *mut c_void) {
    let sender = unsafe { &*data.cast::<Sender<String>>() };
    let text = if json.is_null() {
        "{}".to_string()
    } else {
        unsafe { CStr::from_ptr(json) }
            .to_string_lossy()
            .into_owned()
    };
    let _ = sender.send(text);
}

#[test]
fn callback_cancellation_is_one_typed_cancelled_terminal_not_success() {
    let config = CString::new(r#"{"provider_type":"loopback"}"#).unwrap();
    let request =
        CString::new(r#"{"model":"echo","messages":[{"role":"user","content":"one two three"}]}"#)
            .unwrap();
    let (sender, receiver) = channel::<String>();
    let sender = CallbackBorrow::new(sender);
    unsafe {
        let provider = nxuskit_create_provider(config.as_ptr());
        assert!(!provider.is_null());
        let stream = nxuskit_stream_operation_start(
            provider,
            request.as_ptr(),
            5000,
            Some(stop_chunk),
            Some(terminal),
            (&*sender as *const Sender<String>).cast_mut().cast(),
        );
        assert!(!stream.is_null());
        let result = receiver.recv_timeout(Duration::from_secs(3)).unwrap();
        assert!(nxuskit_stream_operation_wait(stream, 3000));
        assert!(nxuskit_stream_operation_free(stream));
        nxuskit_free_provider(provider);
        assert!(receiver.try_recv().is_err(), "duplicate terminal");
        let result: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            result["code"], "cancelled",
            "cancellation must not be reported as success"
        );
    }
}

struct Held {
    entered: Sender<&'static str>,
    release: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
    operation: std::sync::atomic::AtomicUsize,
    chunks: std::sync::atomic::AtomicUsize,
    terminal: std::sync::Mutex<Vec<(i32, String)>>,
    reentrant_refused: std::sync::atomic::AtomicBool,
}

#[test]
fn held_callback_oracle_rejects_missing_release_and_null_handle() {
    use std::sync::atomic::Ordering::SeqCst;
    let (entered, _observation) = channel();
    let (release, released) = channel();
    drop(release);
    let state = CallbackBorrow::new(Held {
        entered,
        release: std::sync::Mutex::new(released),
        operation: 0.into(),
        chunks: 0.into(),
        terminal: std::sync::Mutex::new(Vec::new()),
        reentrant_refused: false.into(),
    });
    unsafe { held_chunk(std::ptr::null(), (&*state as *const Held).cast_mut().cast()) };
    assert!(!state.reentrant_refused.load(SeqCst));
}

unsafe extern "C" fn held_chunk(_: *const c_char, data: *mut c_void) -> i32 {
    use std::sync::atomic::Ordering::SeqCst;
    let state = unsafe { &*data.cast::<Held>() };
    state.chunks.fetch_add(1, SeqCst);
    let _ = state.entered.send("chunk");
    let released = state
        .release
        .lock()
        .unwrap()
        .recv_timeout(Duration::from_secs(5))
        .is_ok();
    let operation = state.operation.load(SeqCst) as *mut NxuskitStreamOperation;
    state.reentrant_refused.store(
        released
            && !operation.is_null()
            && unsafe {
                !nxuskit_stream_operation_wait(operation, 1000)
                    && !nxuskit_stream_operation_free(operation)
            },
        SeqCst,
    );
    0
}

unsafe extern "C" fn held_terminal(status: i32, error: *const c_char, data: *mut c_void) {
    let state = unsafe { &*data.cast::<Held>() };
    let text = if error.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(error) }
            .to_string_lossy()
            .into_owned()
    };
    state.terminal.lock().unwrap().push((status, text));
    let _ = state.entered.send("terminal");
    let _ = state
        .release
        .lock()
        .unwrap()
        .recv_timeout(Duration::from_secs(5));
}

#[test]
fn held_callbacks_retain_borrows_and_cancel_discards_queued_content() {
    use std::sync::atomic::Ordering::SeqCst;
    let config = CString::new(r#"{"provider_type":"loopback"}"#).unwrap();
    let request = CString::new(format!(
        r#"{{"model":"echo","messages":[{{"role":"user","content":"{}"}}]}}"#,
        "word ".repeat(100)
    ))
    .unwrap();
    let (entered, observation) = channel();
    let (release, released) = channel();
    let state = CallbackBorrow::new(Held {
        entered,
        release: std::sync::Mutex::new(released),
        operation: 0.into(),
        chunks: 0.into(),
        terminal: std::sync::Mutex::new(Vec::new()),
        reentrant_refused: false.into(),
    });
    unsafe {
        let provider = nxuskit_create_provider(config.as_ptr());
        assert!(!provider.is_null());
        let operation = nxuskit_stream_operation_start(
            provider,
            request.as_ptr(),
            5000,
            Some(held_chunk),
            Some(held_terminal),
            (&*state as *const Held).cast_mut().cast(),
        );
        assert!(!operation.is_null());
        state.operation.store(operation as usize, SeqCst);
        assert_eq!(
            observation.recv_timeout(Duration::from_secs(3)).unwrap(),
            "chunk"
        );
        assert!(!nxuskit_stream_operation_wait(operation, 0));
        assert!(!nxuskit_stream_operation_free(operation));
        nxuskit_stream_operation_cancel(operation);
        assert!(state.terminal.lock().unwrap().is_empty());
        release.send(()).unwrap();
        assert_eq!(
            observation.recv_timeout(Duration::from_secs(3)).unwrap(),
            "terminal"
        );
        assert!(!nxuskit_stream_operation_wait(operation, 0));
        assert!(!nxuskit_stream_operation_free(operation));
        release.send(()).unwrap();
        assert!(nxuskit_stream_operation_wait(operation, 3000));
        assert!(nxuskit_stream_operation_free(operation));
        nxuskit_free_provider(provider);
    }
    assert!(state.reentrant_refused.load(SeqCst));
    assert_eq!(state.chunks.load(SeqCst), 1);
    let results = state.terminal.lock().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, 5);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&results[0].1).unwrap()["code"],
        "cancelled"
    );
}

#[test]
fn cancellation_and_deadline_drop_establishment_before_terminal_return() {
    use std::io::Read;
    use std::net::TcpListener;
    for cancel in [true, false] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (seen, request_seen) = channel();
        let (closed, peer_closed) = channel();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut buffer = [0; 4096];
            assert!(socket.read(&mut buffer).unwrap() > 0);
            seen.send(()).unwrap();
            while socket.read(&mut buffer).unwrap() != 0 {}
            closed.send(()).unwrap();
        });
        let config = CString::new(format!(r#"{{"provider_type":"openai","model":"local","api_key":"local-test-only","base_url":"http://{address}/v1","timeout_ms":5000}}"#)).unwrap();
        let request =
            CString::new(r#"{"model":"local","messages":[{"role":"user","content":"hello"}]}"#)
                .unwrap();
        let (entered, observation) = channel();
        let (release, released) = channel();
        let state = CallbackBorrow::new(Held {
            entered,
            release: std::sync::Mutex::new(released),
            operation: 0.into(),
            chunks: 0.into(),
            terminal: std::sync::Mutex::new(Vec::new()),
            reentrant_refused: false.into(),
        });
        unsafe {
            let provider = nxuskit_create_provider(config.as_ptr());
            assert!(!provider.is_null());
            let operation = nxuskit_stream_operation_start(
                provider,
                request.as_ptr(),
                if cancel { 5000 } else { 300 },
                Some(stop_chunk),
                Some(held_terminal),
                (&*state as *const Held).cast_mut().cast(),
            );
            assert!(!operation.is_null());
            request_seen.recv_timeout(Duration::from_secs(3)).unwrap();
            if cancel {
                nxuskit_stream_operation_cancel(operation);
            }
            assert_eq!(
                observation.recv_timeout(Duration::from_secs(3)).unwrap(),
                "terminal"
            );
            peer_closed.recv_timeout(Duration::from_secs(3)).unwrap();
            assert!(!nxuskit_stream_operation_free(operation));
            release.send(()).unwrap();
            assert!(nxuskit_stream_operation_wait(operation, 3000));
            assert!(nxuskit_stream_operation_free(operation));
            nxuskit_free_provider(provider);
        }
        server.join().unwrap();
        let results = state.terminal.lock().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&results[0].1).unwrap()["code"],
            if cancel { "cancelled" } else { "timeout" }
        );
    }
}

unsafe extern "C" fn count_chunk(json: *const c_char, data: *mut c_void) -> i32 {
    let state = unsafe { &*data.cast::<(std::sync::atomic::AtomicUsize, Sender<(i32, String)>)>() };
    if !json.is_null() {
        state.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
    0
}
unsafe extern "C" fn count_terminal(status: i32, error: *const c_char, data: *mut c_void) {
    let state = unsafe { &*data.cast::<(std::sync::atomic::AtomicUsize, Sender<(i32, String)>)>() };
    let error = if error.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(error) }
            .to_string_lossy()
            .into_owned()
    };
    let _ = state.1.send((status, error));
}

#[test]
fn success_and_provider_error_have_one_distinct_terminal_and_rejection_has_none() {
    for (model, expected_code) in [
        ("echo", None),
        ("u-turn-error-auth", Some("authentication")),
    ] {
        let config = CString::new(r#"{"provider_type":"loopback"}"#).unwrap();
        let request = CString::new(format!(
            r#"{{"model":"{model}","messages":[{{"role":"user","content":"literal"}}]}}"#
        ))
        .unwrap();
        let (sender, receiver) = channel::<(i32, String)>();
        let state = CallbackBorrow::new((std::sync::atomic::AtomicUsize::new(0), sender));
        unsafe {
            let provider = nxuskit_create_provider(config.as_ptr());
            assert!(!provider.is_null());
            let rejected = nxuskit_stream_operation_start(
                provider,
                request.as_ptr(),
                0,
                Some(count_chunk),
                Some(count_terminal),
                (&*state as *const (std::sync::atomic::AtomicUsize, Sender<(i32, String)>))
                    .cast_mut()
                    .cast(),
            );
            assert!(rejected.is_null());
            assert!(receiver.try_recv().is_err());
            let operation = nxuskit_stream_operation_start(
                provider,
                request.as_ptr(),
                5000,
                Some(count_chunk),
                Some(count_terminal),
                (&*state as *const (std::sync::atomic::AtomicUsize, Sender<(i32, String)>))
                    .cast_mut()
                    .cast(),
            );
            assert!(!operation.is_null());
            let result = receiver.recv_timeout(Duration::from_secs(3)).unwrap();
            assert!(nxuskit_stream_operation_wait(operation, 3000));
            // Cancellation after terminal admission cannot manufacture another result.
            nxuskit_stream_operation_cancel(operation);
            assert!(nxuskit_stream_operation_free(operation));
            nxuskit_free_provider(provider);
            assert!(receiver.try_recv().is_err());
            match expected_code {
                None => {
                    assert_eq!(result, (0, String::new()));
                    assert!((*state).0.load(std::sync::atomic::Ordering::SeqCst) > 0);
                }
                Some(code) => {
                    assert_ne!(result.0, 0);
                    assert_eq!(
                        serde_json::from_str::<serde_json::Value>(&result.1).unwrap()["code"],
                        code
                    );
                    assert_eq!((*state).0.load(std::sync::atomic::Ordering::SeqCst), 0);
                }
            }
        }
    }
}
