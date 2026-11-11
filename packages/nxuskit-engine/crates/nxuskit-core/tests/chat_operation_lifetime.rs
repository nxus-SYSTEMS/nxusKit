use std::ffi::{CStr, CString, c_char, c_void};
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use nxuskit_core::{nxuskit_create_provider, nxuskit_free_provider};

type Callback = Option<unsafe extern "C" fn(i32, *mut c_void, *const c_char, *mut c_void)>;

#[test]
fn queued_operation_deadline_includes_runtime_queue_delay() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "queued_deadline_child",
            "--nocapture",
        ])
        .env("TOKIO_WORKER_THREADS", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "isolated single-worker subprocess driven by queued_operation_deadline_includes_runtime_queue_delay"]
fn queued_deadline_child() {
    let config = CString::new(r#"{"provider_type":"loopback"}"#).unwrap();
    let request =
        CString::new(r#"{"model":"echo","messages":[{"role":"user","content":"hello"}]}"#).unwrap();
    let (entered, observation) = channel();
    let (release, released) = channel();
    let mut blocker = CallbackState {
        entered,
        release: Mutex::new(released),
        operation: Mutex::new(0),
    };
    let (entered, result) = channel();
    let (release_second, released) = channel();
    release_second.send(()).unwrap();
    let mut queued = CallbackState {
        entered,
        release: Mutex::new(released),
        operation: Mutex::new(0),
    };
    unsafe {
        let provider = nxuskit_create_provider(config.as_ptr());
        assert!(!provider.is_null());
        let first = nxuskit_chat_operation_start(
            provider.cast(),
            request.as_ptr(),
            2000,
            Some(held_callback),
            (&mut blocker as *mut CallbackState).cast(),
        );
        assert!(!first.is_null());
        assert_eq!(
            observation.recv_timeout(Duration::from_secs(2)).unwrap().0,
            0
        );
        // The sole runtime worker cannot poll the second operation until released.
        let second = nxuskit_chat_operation_start(
            provider.cast(),
            request.as_ptr(),
            20,
            Some(held_callback),
            (&mut queued as *mut CallbackState).cast(),
        );
        assert!(!second.is_null());
        std::thread::sleep(Duration::from_millis(60));
        release.send(()).unwrap();
        let (status, error) = result.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(nxuskit_chat_operation_wait(first, 2000));
        assert!(nxuskit_chat_operation_wait(second, 2000));
        assert!(nxuskit_chat_operation_free(first));
        assert!(nxuskit_chat_operation_free(second));
        nxuskit_free_provider(provider);
        assert_eq!(
            status, 5,
            "queued work must expire rather than execute successfully"
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&error).unwrap()["code"],
            "timeout"
        );
    }
}
unsafe extern "C" {
    fn nxuskit_chat_operation_start(
        provider: *mut c_void,
        request: *const c_char,
        deadline_ms: u64,
        callback: Callback,
        data: *mut c_void,
    ) -> *mut c_void;
    fn nxuskit_chat_operation_cancel(operation: *mut c_void);
    fn nxuskit_chat_operation_wait(operation: *mut c_void, timeout_ms: u32) -> bool;
    fn nxuskit_chat_operation_free(operation: *mut c_void) -> bool;
    fn nxuskit_free_response(response: *mut c_void);
}

struct CallbackState {
    entered: Sender<(i32, String)>,
    release: Mutex<Receiver<()>>,
    operation: Mutex<usize>,
}

unsafe extern "C" fn held_callback(
    status: i32,
    response: *mut c_void,
    error: *const c_char,
    data: *mut c_void,
) {
    let state = unsafe { &*data.cast::<CallbackState>() };
    let error = if error.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(error) }
            .to_string_lossy()
            .into_owned()
    };
    unsafe { nxuskit_free_response(response) };
    state.entered.send((status, error)).unwrap();
    state
        .release
        .lock()
        .unwrap()
        .recv_timeout(Duration::from_secs(3))
        .unwrap();
    let operation = *state.operation.lock().unwrap();
    if operation != 0 {
        assert!(!unsafe { nxuskit_chat_operation_wait(operation as *mut c_void, 1000) });
        assert!(!unsafe { nxuskit_chat_operation_free(operation as *mut c_void) });
    }
}

#[test]
fn completion_wait_and_free_require_callback_return_not_entry() {
    let config = CString::new(r#"{"provider_type":"loopback"}"#).unwrap();
    let request =
        CString::new(r#"{"model":"echo","messages":[{"role":"user","content":"hello"}]}"#).unwrap();
    let (entered, observation) = channel();
    let (release, released) = channel();
    let mut state = CallbackState {
        entered,
        release: Mutex::new(released),
        operation: Mutex::new(0),
    };
    unsafe {
        let provider = nxuskit_create_provider(config.as_ptr());
        assert!(!provider.is_null());
        let operation = nxuskit_chat_operation_start(
            provider.cast(),
            request.as_ptr(),
            1000,
            Some(held_callback),
            (&mut state as *mut CallbackState).cast(),
        );
        assert!(!operation.is_null());
        assert_eq!(
            observation.recv_timeout(Duration::from_secs(2)).unwrap().0,
            0
        );
        assert!(!nxuskit_chat_operation_wait(operation, 0));
        assert!(!nxuskit_chat_operation_free(operation));
        *state.operation.lock().unwrap() = operation as usize;
        nxuskit_chat_operation_cancel(operation);
        release.send(()).unwrap();
        assert!(nxuskit_chat_operation_wait(operation, 2000));
        assert!(nxuskit_chat_operation_free(operation));
        nxuskit_free_provider(provider);
    }
}

#[test]
fn cancel_and_deadline_drop_real_local_http_work_with_distinct_terminal_errors() {
    use std::io::Read;
    use std::net::TcpListener;
    for cancel in [true, false] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (received, request_seen) = channel();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut buffer = [0; 4096];
            assert!(socket.read(&mut buffer).unwrap() > 0);
            received.send(()).unwrap();
            loop {
                if socket.read(&mut buffer).unwrap() == 0 {
                    break;
                }
            }
        });
        let config = CString::new(format!(r#"{{"provider_type":"openai","model":"local","api_key":"local-test-only","base_url":"http://{address}/v1","timeout_ms":5000}}"#)).unwrap();
        let request =
            CString::new(r#"{"model":"local","messages":[{"role":"user","content":"hello"}]}"#)
                .unwrap();
        let (entered, observation) = channel();
        let (release, released) = channel();
        release.send(()).unwrap();
        let mut state = CallbackState {
            entered,
            release: Mutex::new(released),
            operation: Mutex::new(0),
        };
        unsafe {
            let provider = nxuskit_create_provider(config.as_ptr());
            assert!(!provider.is_null());
            let operation = nxuskit_chat_operation_start(
                provider.cast(),
                request.as_ptr(),
                if cancel { 2000 } else { 150 },
                Some(held_callback),
                (&mut state as *mut CallbackState).cast(),
            );
            assert!(!operation.is_null());
            request_seen.recv_timeout(Duration::from_secs(2)).unwrap();
            if cancel {
                nxuskit_chat_operation_cancel(operation);
            }
            let (status, error) = observation.recv_timeout(Duration::from_secs(2)).unwrap();
            assert_eq!(status, 5);
            let error: serde_json::Value = serde_json::from_str(&error).unwrap();
            assert_eq!(error["code"], if cancel { "cancelled" } else { "timeout" });
            assert!(nxuskit_chat_operation_wait(operation, 2000));
            assert!(nxuskit_chat_operation_free(operation));
            nxuskit_free_provider(provider);
        }
        server.join().unwrap();
    }
}
