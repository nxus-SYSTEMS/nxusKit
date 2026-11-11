#![allow(clippy::panic)]

use std::ffi::{CStr, CString, c_char, c_void};
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::time::{Duration, Instant};

use nxuskit_core::{
    nxuskit_chat, nxuskit_chat_stream, nxuskit_create_provider, nxuskit_discovery_create,
    nxuskit_discovery_free, nxuskit_discovery_operation_free, nxuskit_discovery_operation_result,
    nxuskit_discovery_operation_start, nxuskit_discovery_operation_wait, nxuskit_free_provider,
    nxuskit_free_stream, nxuskit_last_error,
};

type ChatCallback = Option<
    unsafe extern "C" fn(
        status: i32,
        response: *mut c_void,
        error_json: *const c_char,
        user_data: *mut c_void,
    ),
>;
type ListModelsCallback = Option<
    unsafe extern "C" fn(
        status: i32,
        models_json: *mut c_char,
        error_json: *const c_char,
        user_data: *mut c_void,
    ),
>;

unsafe extern "C" {
    fn nxuskit_chat_async(
        provider: *mut c_void,
        request_json: *const c_char,
        callback: ChatCallback,
        user_data: *mut c_void,
    ) -> i32;
    fn nxuskit_list_models_async(
        provider: *mut c_void,
        callback: ListModelsCallback,
        user_data: *mut c_void,
    ) -> i32;
    fn nxuskit_response_json(response: *const c_void) -> *const c_char;
    fn nxuskit_free_response(response: *mut c_void);
    fn nxuskit_free_string(value: *mut c_char);
}

static CALLBACK_COUNT: AtomicUsize = AtomicUsize::new(0);

unsafe extern "C" fn chat_callback(
    _status: i32,
    _response: *mut c_void,
    _error_json: *const c_char,
    _user_data: *mut c_void,
) {
    CALLBACK_COUNT.fetch_add(1, Ordering::SeqCst);
}

unsafe extern "C" fn list_models_callback(
    _status: i32,
    _models_json: *mut c_char,
    _error_json: *const c_char,
    _user_data: *mut c_void,
) {
    CALLBACK_COUNT.fetch_add(1, Ordering::SeqCst);
}

#[derive(Debug)]
struct CallbackObservation {
    status: i32,
    payload: Option<String>,
    error: Option<String>,
    thread_name: Option<String>,
    owned_pointer: usize,
}

struct CallbackState {
    sender: SyncSender<CallbackObservation>,
    count: AtomicUsize,
}

#[derive(Debug)]
enum StreamObservation {
    Chunk(serde_json::Value),
    Done(serde_json::Value),
}

struct StreamCallbackState {
    sender: SyncSender<StreamObservation>,
}

unsafe extern "C" fn observe_stream_chunk(
    chunk_json: *const c_char,
    user_data: *mut c_void,
) -> i32 {
    let state = unsafe { &*user_data.cast::<StreamCallbackState>() };
    let value: serde_json::Value =
        serde_json::from_slice(unsafe { CStr::from_ptr(chunk_json) }.to_bytes())
            .expect("stream chunk must be strict JSON");
    let _ = state.sender.send(StreamObservation::Chunk(value));
    0
}

unsafe extern "C" fn observe_stream_done(final_json: *const c_char, user_data: *mut c_void) {
    let state = unsafe { &*user_data.cast::<StreamCallbackState>() };
    let value: serde_json::Value =
        serde_json::from_slice(unsafe { CStr::from_ptr(final_json) }.to_bytes())
            .expect("stream final must be strict JSON");
    let _ = state.sender.send(StreamObservation::Done(value));
}

unsafe extern "C" fn observe_chat_callback(
    status: i32,
    response: *mut c_void,
    error_json: *const c_char,
    user_data: *mut c_void,
) {
    let state = unsafe { &*user_data.cast::<CallbackState>() };
    state.count.fetch_add(1, Ordering::SeqCst);
    let payload = if response.is_null() {
        None
    } else {
        let json = unsafe { nxuskit_response_json(response) };
        (!json.is_null()).then(|| {
            unsafe { CStr::from_ptr(json) }
                .to_string_lossy()
                .into_owned()
        })
    };
    let error = (!error_json.is_null()).then(|| {
        unsafe { CStr::from_ptr(error_json) }
            .to_string_lossy()
            .into_owned()
    });
    let observation = CallbackObservation {
        status,
        payload,
        error,
        thread_name: std::thread::current().name().map(str::to_owned),
        owned_pointer: response as usize,
    };
    let _ = state.sender.send(observation);
}

unsafe extern "C" fn observe_models_callback(
    status: i32,
    models_json: *mut c_char,
    error_json: *const c_char,
    user_data: *mut c_void,
) {
    let state = unsafe { &*user_data.cast::<CallbackState>() };
    state.count.fetch_add(1, Ordering::SeqCst);
    let payload = (!models_json.is_null()).then(|| {
        unsafe { CStr::from_ptr(models_json) }
            .to_string_lossy()
            .into_owned()
    });
    let error = (!error_json.is_null()).then(|| {
        unsafe { CStr::from_ptr(error_json) }
            .to_string_lossy()
            .into_owned()
    });
    let observation = CallbackObservation {
        status,
        payload,
        error,
        thread_name: std::thread::current().name().map(str::to_owned),
        owned_pointer: models_json as usize,
    };
    let _ = state.sender.send(observation);
}

fn loopback_provider() -> *mut c_void {
    let config = CString::new(r#"{"provider_type":"loopback"}"#).unwrap();
    let provider = unsafe { nxuskit_create_provider(config.as_ptr()) };
    assert!(!provider.is_null(), "loopback provider must be available");
    provider.cast()
}

fn create_provider(config: serde_json::Value) -> *mut c_void {
    let config = CString::new(config.to_string()).unwrap();
    unsafe { nxuskit_create_provider(config.as_ptr()).cast() }
}

fn last_error_json() -> serde_json::Value {
    let ptr = nxuskit_last_error();
    assert!(!ptr.is_null(), "expected C ABI last error");
    serde_json::from_slice(unsafe { CStr::from_ptr(ptr) }.to_bytes())
        .expect("last error must be strict JSON")
}

fn prove_spec119_assertion(assertion_id: &str) {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo_root = crate_root
        .ancestors()
        .nth(4)
        .expect("core crate must be nested under the repository root");
    let matrix = fs::read_to_string(repo_root.join(
        "specs/119-v2-python-provider-surface-parity/definition/qualification-matrix-v1.json",
    ))
    .expect("frozen Spec119 qualification matrix must be readable");
    let needle = format!("\"id\": \"{assertion_id}\"");
    assert!(
        matrix.contains(&needle),
        "semantic execution referenced a non-frozen assertion id: {assertion_id}"
    );
    println!("SPEC119_ASSERTION_ID={assertion_id}");
}

fn assert_json_subset(actual: &serde_json::Value, expected: &serde_json::Value, path: &str) {
    match (actual, expected) {
        (serde_json::Value::Object(actual_map), serde_json::Value::Object(expected_map)) => {
            for (key, expected_value) in expected_map {
                let actual_value = actual_map.get(key).unwrap_or_else(|| {
                    panic!("{path}.{key}: expected key missing from captured wire")
                });
                assert_json_subset(actual_value, expected_value, &format!("{path}.{key}"));
            }
        }
        (actual_value, expected_value) => {
            assert_eq!(
                actual_value, expected_value,
                "{path}: captured Ollama wire value mismatch"
            );
        }
    }
}

fn assert_json_path_absent(actual: &serde_json::Value, dotted_path: &str) {
    let mut cursor = actual;
    let segments: Vec<&str> = dotted_path.split('.').collect();
    for (index, segment) in segments.iter().enumerate() {
        match cursor.get(segment) {
            None => return,
            Some(next) if index + 1 < segments.len() => cursor = next,
            Some(next) => panic!("expected wire path {dotted_path} to be absent, found {next}"),
        }
    }
}

fn captured_ollama_wire(request: &str) -> serde_json::Value {
    let (_, body) = request
        .split_once("\r\n\r\n")
        .expect("captured request must contain HTTP headers");
    serde_json::from_str(body).expect("captured Ollama request body must be JSON")
}

fn sync_error_type(provider: *mut c_void, request: serde_json::Value) -> String {
    let request = CString::new(request.to_string()).unwrap();
    let response = unsafe { nxuskit_chat(provider.cast(), request.as_ptr()) }.cast::<c_void>();
    if response.is_null() {
        return last_error_json()["error_type"]
            .as_str()
            .expect("last error must contain error_type")
            .to_string();
    }

    let raw = unsafe { nxuskit_response_json(response) };
    assert!(!raw.is_null(), "error response must serialize");
    let value: serde_json::Value =
        serde_json::from_slice(unsafe { CStr::from_ptr(raw) }.to_bytes()).unwrap();
    unsafe { nxuskit_free_response(response) };
    value["error"]["error_type"]
        .as_str()
        .unwrap_or_else(|| panic!("expected error envelope, got {value}"))
        .to_string()
}

fn serve_ollama_stream(reasoning: bool) -> (String, std::thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("loopback listener");
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().expect("one request");
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .expect("read timeout");
        let mut bytes = Vec::new();
        let mut header_end = None;
        while header_end.is_none() {
            let mut chunk = [0_u8; 4096];
            let count = socket.read(&mut chunk).expect("request bytes");
            assert!(count > 0, "client closed before request headers");
            bytes.extend_from_slice(&chunk[..count]);
            header_end = bytes.windows(4).position(|window| window == b"\r\n\r\n");
        }
        let header_end = header_end.unwrap() + 4;
        let headers = String::from_utf8_lossy(&bytes[..header_end]).into_owned();
        let content_length = headers
            .lines()
            .find_map(|line| {
                line.to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .map(str::trim)
                    .and_then(|value| value.parse::<usize>().ok())
            })
            .unwrap_or(0);
        while bytes.len() < header_end + content_length {
            let mut chunk = [0_u8; 4096];
            let count = socket.read(&mut chunk).expect("request body");
            if count == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..count]);
        }
        let request = String::from_utf8_lossy(&bytes).into_owned();

        let first = if reasoning {
            serde_json::json!({
                "model": "qwen3",
                "message": {
                    "role": "assistant",
                    "content": "{\"x\":",
                    "thinking": "reason"
                },
                "done": false
            })
        } else {
            serde_json::json!({
                "model": "llama3.2",
                "message": {
                    "role": "assistant",
                    "content": "{\"x\":"
                },
                "done": false
            })
        };
        let second = serde_json::json!({
            "model": if reasoning { "qwen3" } else { "llama3.2" },
            "message": {
                "role": "assistant",
                "content": "1}"
            },
            "done": true,
            "done_reason": "stop",
            "prompt_eval_count": 10,
            "eval_count": 2
        });
        let body = format!("{}\n{}\n", first, second);
        write!(
            socket,
            "HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .expect("stream fixture response");
        request
    });
    (endpoint, handle)
}

fn serve_discovery_response(
    status: u16,
    body: Vec<u8>,
) -> (String, std::thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("loopback listener");
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().expect("one request");
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .expect("read timeout");
        let mut bytes = Vec::new();
        while !bytes.windows(4).any(|window| window == b"\r\n\r\n") {
            let mut chunk = [0_u8; 1024];
            let count = socket.read(&mut chunk).expect("request bytes");
            assert!(count > 0, "client closed before discovery request headers");
            bytes.extend_from_slice(&chunk[..count]);
        }
        let request = String::from_utf8_lossy(&bytes).into_owned();
        let reason = match status {
            200 => "OK",
            401 => "Unauthorized",
            503 => "Service Unavailable",
            _ => "Fixture",
        };
        write!(
            socket,
            "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .expect("discovery fixture headers");
        socket.write_all(&body).expect("discovery fixture body");
        request
    });
    (endpoint, handle)
}

fn direct_c_discovery(
    profile: &str,
    operation: u32,
    status: u16,
    body: Vec<u8>,
) -> (serde_json::Value, String) {
    let (endpoint, server) = serve_discovery_response(status, body);
    let config = CString::new(
        serde_json::json!({
            "provider_id": profile,
            "endpoint": endpoint,
            "auth_selector": null,
            "connect_timeout_ms": 3000,
            "read_timeout_ms": 3000
        })
        .to_string(),
    )
    .unwrap();
    let client = unsafe { nxuskit_discovery_create(config.as_ptr()) };
    assert!(!client.is_null(), "{profile} discovery client");
    let op = unsafe { nxuskit_discovery_operation_start(client, operation) };
    assert!(!op.is_null(), "{profile} discovery operation {operation}");
    assert!(
        unsafe { nxuskit_discovery_operation_wait(op, 3000) },
        "{profile} discovery operation {operation} timed out"
    );
    let raw = unsafe { nxuskit_discovery_operation_result(op) };
    assert!(!raw.is_null(), "{profile} discovery result");
    let value: serde_json::Value =
        serde_json::from_slice(unsafe { CStr::from_ptr(raw) }.to_bytes()).unwrap();
    unsafe {
        nxuskit_free_string(raw);
        assert!(nxuskit_discovery_operation_free(op));
        nxuskit_discovery_free(client);
    }
    let captured = server.join().expect("discovery fixture server");
    (value, captured)
}

fn serve_one_rate_limit_response(
    retry_after: Option<&'static str>,
) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("loopback listener");
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().expect("one request");
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .expect("read timeout");
        let mut bytes = Vec::new();
        let mut header_end = None;
        while header_end.is_none() {
            let mut chunk = [0_u8; 4096];
            let count = socket.read(&mut chunk).expect("request bytes");
            assert!(count > 0, "client closed before request headers");
            bytes.extend_from_slice(&chunk[..count]);
            header_end = bytes.windows(4).position(|window| window == b"\r\n\r\n");
        }
        let header_end = header_end.unwrap() + 4;
        let headers = String::from_utf8_lossy(&bytes[..header_end]).into_owned();
        let content_length = headers
            .lines()
            .find_map(|line| {
                line.to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .map(str::trim)
                    .and_then(|value| value.parse::<usize>().ok())
            })
            .unwrap_or(0);
        while bytes.len() < header_end + content_length {
            let mut chunk = [0_u8; 4096];
            let count = socket.read(&mut chunk).expect("request body");
            if count == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..count]);
        }

        let retry_header = retry_after
            .map(|value| format!("Retry-After: {value}\r\n"))
            .unwrap_or_default();
        let body = r#"{"error":"rate limited"}"#;
        write!(
            socket,
            "HTTP/1.1 429 Too Many Requests\r\nContent-Type: application/json\r\n{}Content-Length: {}\r\nConnection: close\r\n\r\n{}",
            retry_header,
            body.len(),
            body
        )
        .expect("rate-limit fixture response");
    });
    (endpoint, handle)
}

fn serve_one_ollama_response(content: &'static str) -> (String, std::thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("loopback listener");
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().expect("one request");
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .expect("read timeout");
        let mut bytes = Vec::new();
        let mut header_end = None;
        while header_end.is_none() {
            let mut chunk = [0_u8; 4096];
            let count = socket.read(&mut chunk).expect("request bytes");
            assert!(count > 0, "client closed before request headers");
            bytes.extend_from_slice(&chunk[..count]);
            header_end = bytes.windows(4).position(|window| window == b"\r\n\r\n");
        }
        let header_end = header_end.unwrap() + 4;
        let headers = String::from_utf8_lossy(&bytes[..header_end]).into_owned();
        let content_length = headers
            .lines()
            .find_map(|line| {
                line.to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .map(str::trim)
                    .and_then(|value| value.parse::<usize>().ok())
            })
            .unwrap_or(0);
        while bytes.len() < header_end + content_length {
            let mut chunk = [0_u8; 4096];
            let count = socket.read(&mut chunk).expect("request body");
            if count == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..count]);
        }
        let request = String::from_utf8_lossy(&bytes).into_owned();
        let body = serde_json::json!({
            "model": "llama3.2",
            "message": {"role": "assistant", "content": content},
            "done": true,
            "done_reason": "stop",
            "prompt_eval_count": 10,
            "eval_count": 2
        })
        .to_string();
        write!(
            socket,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .expect("fixture response");
        request
    });
    (endpoint, handle)
}

#[test]
fn direct_c_ollama_profile_aliases_validate_without_changing_c_signatures() {
    for profile in ["ollama_local", "ollama_cloud_via_daemon"] {
        let provider = create_provider(serde_json::json!({
            "provider_type": profile,
            "model": "llama3.2",
            "base_url": "http://127.0.0.1:1"
        }));
        assert!(!provider.is_null(), "{profile} alias must construct");
        unsafe { nxuskit_free_provider(provider.cast()) };
    }

    let invalid = create_provider(serde_json::json!({
        "provider_type": "ollama",
        "profile": "not-a-profile"
    }));
    assert!(invalid.is_null());
    assert_eq!(last_error_json()["error_type"], "invalid_config");

    let conflicting = create_provider(serde_json::json!({
        "provider_type": "ollama_local",
        "profile": "ollama_cloud_via_daemon"
    }));
    assert!(conflicting.is_null());
    assert_eq!(last_error_json()["error_type"], "invalid_config");

    let foreign = create_provider(serde_json::json!({
        "provider_type": "openai",
        "profile": "ollama_local",
        "api_key": "fixture"
    }));
    assert!(foreign.is_null());
    assert_eq!(last_error_json()["error_type"], "invalid_config");
}

#[test]
fn direct_c_ollama_profile_is_preserved_in_unary_response_json() {
    for profile in ["ollama_local", "ollama_cloud_via_daemon"] {
        let (content, assertion_id) = match profile {
            "ollama_local" => (
                "ollama_local-fixture-response",
                "ollama_local--c_abi:response_mapping",
            ),
            "ollama_cloud_via_daemon" => (
                "ollama_cloud_via_daemon-fixture-response",
                "ollama_cloud_via_daemon--c_abi:response_mapping",
            ),
            _ => unreachable!(),
        };
        let (endpoint, server) = serve_one_ollama_response(content);
        let provider = create_provider(serde_json::json!({
            "provider_type": profile,
            "model": "llama3.2",
            "base_url": endpoint,
            "timeout_ms": 3000
        }));
        assert!(!provider.is_null(), "{profile}");

        let request =
            CString::new(r#"{"model":"llama3.2","messages":[{"role":"user","content":"hi"}]}"#)
                .unwrap();
        let response = unsafe { nxuskit_chat(provider.cast(), request.as_ptr()) }.cast::<c_void>();
        assert!(!response.is_null(), "{profile}");
        let raw = unsafe { nxuskit_response_json(response) };
        assert!(!raw.is_null());
        let value: serde_json::Value =
            serde_json::from_slice(unsafe { CStr::from_ptr(raw) }.to_bytes()).unwrap();
        assert_eq!(value["content"], content);
        assert_eq!(value["declared_profile"], profile);
        assert_eq!(value["finish_reason"], "stop");

        unsafe {
            nxuskit_free_response(response);
            nxuskit_free_provider(provider.cast());
        }
        let wire = server.join().expect("fixture server");
        assert!(
            wire.starts_with("POST /api/chat HTTP/1.1"),
            "unexpected Ollama route: {wire}"
        );
        prove_spec119_assertion(assertion_id);
    }
}

#[test]
fn direct_c_ollama_structured_output_matches_frozen_native_wire_for_both_profiles() {
    let schema = serde_json::json!({
        "properties": {"x": {"type": "integer"}},
        "required": ["x"],
        "type": "object"
    });
    let cases = [
        (
            "OLL-FORMAT-JSON",
            serde_json::json!({"type": "json"}),
            Some(serde_json::json!("json")),
        ),
        (
            "OLL-FORMAT-SCHEMA",
            serde_json::json!({"type": "json_schema", "schema": schema}),
            Some(schema),
        ),
        (
            "OLL-FORMAT-TEXT-OMIT",
            serde_json::json!({"type": "text"}),
            None,
        ),
    ];

    for profile in ["ollama_local", "ollama_cloud_via_daemon"] {
        for (case_id, response_format, expected_format) in &cases {
            let (endpoint, server) = serve_one_ollama_response("structured-fixture-response");
            let model = format!("{profile}-fixture-model");
            let provider = create_provider(serde_json::json!({
                "provider_type": profile,
                "model": model,
                "base_url": endpoint,
                "timeout_ms": 3000
            }));
            assert!(!provider.is_null(), "{profile} {case_id}");

            let request = CString::new(
                serde_json::json!({
                    "model": model,
                    "messages": [{"role": "user", "content": "hi"}],
                    "response_format": response_format
                })
                .to_string(),
            )
            .unwrap();
            let response =
                unsafe { nxuskit_chat(provider.cast(), request.as_ptr()) }.cast::<c_void>();
            assert!(!response.is_null(), "{profile} {case_id}");
            let raw = unsafe { nxuskit_response_json(response) };
            let value: serde_json::Value =
                serde_json::from_slice(unsafe { CStr::from_ptr(raw) }.to_bytes()).unwrap();
            assert_eq!(value["declared_profile"], profile, "{profile} {case_id}");
            assert!(
                value.get("error").is_none(),
                "{profile} {case_id} unexpectedly failed: {value}"
            );
            unsafe {
                nxuskit_free_response(response);
                nxuskit_free_provider(provider.cast());
            }

            let wire = server.join().expect("fixture server");
            let (_, body) = wire
                .split_once("\r\n\r\n")
                .expect("captured request must contain HTTP headers");
            let wire_json: serde_json::Value =
                serde_json::from_str(body).expect("captured Ollama request body must be JSON");
            assert_eq!(wire_json["model"], model, "{profile} {case_id}");
            match expected_format {
                Some(expected) => assert_eq!(
                    wire_json.get("format"),
                    Some(expected),
                    "{profile} {case_id}"
                ),
                None => assert!(
                    wire_json.get("format").is_none(),
                    "{profile} {case_id} must omit format: {wire_json}"
                ),
            }
            println!("SPEC119_ORACLE_CASE={profile}--c_abi:request_wire:{case_id}");
        }
    }
}

#[test]
fn direct_c_ollama_supported_controls_match_frozen_request_wire_for_both_profiles() {
    let flat_tool = serde_json::json!({
        "description": "Return one integer",
        "name": "emit_x",
        "parameters": {
            "properties": {"x": {"type": "integer"}},
            "required": ["x"],
            "type": "object"
        }
    });
    let wrapped_tool = serde_json::json!({
        "type": "function",
        "function": flat_tool.clone()
    });
    let cases: Vec<(&str, serde_json::Value, serde_json::Value, Vec<&str>)> = vec![
        (
            "THINK-AUTO-MODEL",
            serde_json::json!({"thinking_mode": "auto"}),
            serde_json::json!({"think": true}),
            vec![],
        ),
        (
            "THINK-ENABLED",
            serde_json::json!({"thinking_mode": "enabled"}),
            serde_json::json!({"think": true}),
            vec![],
        ),
        (
            "THINK-DISABLED",
            serde_json::json!({"thinking_mode": "disabled"}),
            serde_json::json!({"think": false}),
            vec![],
        ),
        (
            "THINK-OMIT",
            serde_json::json!({}),
            serde_json::json!({}),
            vec!["think"],
        ),
        (
            "TOP-K-POSITIVE",
            serde_json::json!({"top_k": 37}),
            serde_json::json!({"options": {"top_k": 37}}),
            vec![],
        ),
        (
            "TOP-K-OMIT",
            serde_json::json!({}),
            serde_json::json!({}),
            vec!["options.top_k"],
        ),
        (
            "MIN-P-POSITIVE",
            serde_json::json!({"min_p": 0.15}),
            serde_json::json!({"options": {"min_p": 0.15}}),
            vec![],
        ),
        (
            "MIN-P-OMIT",
            serde_json::json!({}),
            serde_json::json!({}),
            vec!["options.min_p"],
        ),
        (
            "SEED-POSITIVE",
            serde_json::json!({"seed": 42}),
            serde_json::json!({"options": {"seed": 42}}),
            vec![],
        ),
        (
            "SEED-OMIT",
            serde_json::json!({}),
            serde_json::json!({}),
            vec!["options.seed"],
        ),
        (
            "LOGPROBS-POSITIVE",
            serde_json::json!({"logprobs": true, "top_logprobs": 5}),
            serde_json::json!({"logprobs": true, "top_logprobs": 5}),
            vec![],
        ),
        (
            "LOGPROBS-OMIT",
            serde_json::json!({}),
            serde_json::json!({}),
            vec!["logprobs", "top_logprobs"],
        ),
        (
            "TOOLS-POSITIVE",
            serde_json::json!({"tools": [flat_tool]}),
            serde_json::json!({"tools": [wrapped_tool]}),
            vec![],
        ),
        (
            "TOOLS-EMPTY",
            serde_json::json!({"tools": []}),
            serde_json::json!({}),
            vec!["tools"],
        ),
        (
            "TOOLS-OMIT",
            serde_json::json!({}),
            serde_json::json!({}),
            vec!["tools"],
        ),
        (
            "PRESENCE-PENALTY-OMIT",
            serde_json::json!({}),
            serde_json::json!({}),
            vec!["presence_penalty"],
        ),
        (
            "FREQUENCY-PENALTY-OMIT",
            serde_json::json!({}),
            serde_json::json!({}),
            vec!["frequency_penalty"],
        ),
        (
            "REASONING-EFFORT-OMIT",
            serde_json::json!({}),
            serde_json::json!({}),
            vec!["reasoning_effort"],
        ),
    ];

    for profile in ["ollama_local", "ollama_cloud_via_daemon"] {
        for (case_id, patch, expected_fragment, absent_paths) in &cases {
            let model = if *case_id == "THINK-AUTO-MODEL" {
                "qwen3:8b".to_string()
            } else {
                format!("{profile}-fixture-model")
            };
            let (endpoint, server) = serve_one_ollama_response("control-fixture-response");
            let provider = create_provider(serde_json::json!({
                "provider_type": profile,
                "model": model,
                "base_url": endpoint,
                "timeout_ms": 3000
            }));
            assert!(!provider.is_null(), "{profile} {case_id}");

            let mut request_value = serde_json::json!({
                "model": model,
                "messages": [{"role": "user", "content": "hi"}]
            });
            request_value
                .as_object_mut()
                .expect("request must be an object")
                .extend(
                    patch
                        .as_object()
                        .expect("case patch must be an object")
                        .clone(),
                );
            let request = CString::new(request_value.to_string()).unwrap();
            let response =
                unsafe { nxuskit_chat(provider.cast(), request.as_ptr()) }.cast::<c_void>();
            assert!(!response.is_null(), "{profile} {case_id}");
            let raw = unsafe { nxuskit_response_json(response) };
            let value: serde_json::Value =
                serde_json::from_slice(unsafe { CStr::from_ptr(raw) }.to_bytes()).unwrap();
            assert!(
                value.get("error").is_none(),
                "{profile} {case_id} unexpectedly failed: {value}"
            );
            assert_eq!(value["declared_profile"], profile, "{profile} {case_id}");

            unsafe {
                nxuskit_free_response(response);
                nxuskit_free_provider(provider.cast());
            }
            let captured = server.join().expect("fixture server");
            let wire = captured_ollama_wire(&captured);
            assert_json_subset(&wire, expected_fragment, case_id);
            for absent_path in absent_paths {
                assert_json_path_absent(&wire, absent_path);
            }
            println!("SPEC119_ORACLE_CASE={profile}--c_abi:request_wire:{case_id}");
        }
    }
}

#[test]
fn direct_c_ollama_remaining_frozen_request_cases_fail_closed_for_both_profiles() {
    let rejects = [
        (
            "OLL-FORMAT-WRAPPER-REJECT",
            serde_json::json!({
                "response_format": {
                    "type": "object",
                    "name": "fact",
                    "schema": {"type": "object"},
                    "strict": true
                }
            }),
            "invalid_request",
        ),
        (
            "THINK-INVALID",
            serde_json::json!({"thinking_mode": "sometimes"}),
            "invalid_request",
        ),
        (
            "TOP-K-NEGATIVE",
            serde_json::json!({"top_k": 0}),
            "invalid_request",
        ),
        (
            "MIN-P-NEGATIVE",
            serde_json::json!({"min_p": 1.1}),
            "invalid_request",
        ),
        (
            "SEED-NEGATIVE",
            serde_json::json!({"seed": "42"}),
            "invalid_request",
        ),
        (
            "PRESENCE-PENALTY-POSITIVE",
            serde_json::json!({"presence_penalty": 0.4}),
            "unsupported_capability",
        ),
        (
            "PRESENCE-PENALTY-NEGATIVE",
            serde_json::json!({"presence_penalty": 2.1}),
            "invalid_request",
        ),
        (
            "FREQUENCY-PENALTY-POSITIVE",
            serde_json::json!({"frequency_penalty": -0.3}),
            "unsupported_capability",
        ),
        (
            "FREQUENCY-PENALTY-NEGATIVE",
            serde_json::json!({"frequency_penalty": -2.1}),
            "invalid_request",
        ),
        (
            "REASONING-EFFORT-POSITIVE",
            serde_json::json!({"reasoning_effort": "medium"}),
            "unsupported_capability",
        ),
        (
            "REASONING-EFFORT-NEGATIVE",
            serde_json::json!({"reasoning_effort": "extreme"}),
            "invalid_request",
        ),
        (
            "LOGPROBS-NEGATIVE",
            serde_json::json!({"logprobs": true, "top_logprobs": 21}),
            "invalid_request",
        ),
        (
            "TOOLS-NEGATIVE",
            serde_json::json!({"tools": [{"name": "", "parameters": []}]}),
            "invalid_request",
        ),
    ];

    for profile in ["ollama_local", "ollama_cloud_via_daemon"] {
        for (case_id, patch, expected_error) in &rejects {
            let provider = create_provider(serde_json::json!({
                "provider_type": profile,
                "model": format!("{profile}-fixture-model"),
                "base_url": "http://127.0.0.1:1",
                "timeout_ms": 250
            }));
            assert!(!provider.is_null(), "{profile} {case_id}");

            let mut request = serde_json::json!({
                "model": format!("{profile}-fixture-model"),
                "messages": [{"role": "user", "content": "hi"}]
            });
            request
                .as_object_mut()
                .expect("request must be an object")
                .extend(
                    patch
                        .as_object()
                        .expect("case patch must be an object")
                        .clone(),
                );
            assert_eq!(
                sync_error_type(provider, request),
                *expected_error,
                "{profile} {case_id}"
            );
            unsafe { nxuskit_free_provider(provider.cast()) };
            println!("SPEC119_ORACLE_CASE={profile}--c_abi:request_wire:{case_id}");
        }
    }
    for profile in ["ollama_local", "ollama_cloud_via_daemon"] {
        prove_spec119_assertion(&format!("{profile}--c_abi:request_wire"));
    }
}

#[test]
fn direct_c_ollama_rate_limit_error_mapping_preserves_retry_metadata() {
    for profile in ["ollama_local", "ollama_cloud_via_daemon"] {
        for (case_id, retry_after, expected_retry_ms) in [
            ("ERROR-RATE-LIMIT-WITH-RETRY", Some("2"), Some(2000_u64)),
            ("ERROR-RATE-LIMIT-NO-RETRY", None, None),
        ] {
            let request_json = serde_json::json!({
                "model": format!("{profile}-fixture-model"),
                "messages": [{"role": "user", "content": "hi"}]
            });
            let request = CString::new(request_json.to_string()).unwrap();

            let (endpoint, server) = serve_one_rate_limit_response(retry_after);
            let provider = create_provider(serde_json::json!({
                "provider_type": profile,
                "model": format!("{profile}-fixture-model"),
                "base_url": endpoint,
                "timeout_ms": 3000
            }));
            assert!(!provider.is_null(), "{profile} {case_id} sync");
            let response =
                unsafe { nxuskit_chat(provider.cast(), request.as_ptr()) }.cast::<c_void>();
            assert!(!response.is_null(), "{profile} {case_id} sync");
            let raw = unsafe { nxuskit_response_json(response) };
            let value: serde_json::Value =
                serde_json::from_slice(unsafe { CStr::from_ptr(raw) }.to_bytes()).unwrap();
            assert_eq!(value["error"]["error_type"], "rate_limited");
            match expected_retry_ms {
                Some(expected) => assert_eq!(value["error"]["retry_after_ms"], expected),
                None => assert!(
                    value["error"].get("retry_after_ms").is_none(),
                    "{profile} {case_id} must not invent retry metadata: {value}"
                ),
            }
            unsafe {
                nxuskit_free_response(response);
                nxuskit_free_provider(provider.cast());
            }
            server.join().expect("sync rate-limit fixture server");

            let (endpoint, server) = serve_one_rate_limit_response(retry_after);
            let provider = create_provider(serde_json::json!({
                "provider_type": profile,
                "model": format!("{profile}-fixture-model"),
                "base_url": endpoint,
                "timeout_ms": 3000
            }));
            assert!(!provider.is_null(), "{profile} {case_id} async");
            let (sender, receiver) = sync_channel(2);
            let state = Box::into_raw(Box::new(CallbackState {
                sender,
                count: AtomicUsize::new(0),
            }));
            let accepted = unsafe {
                nxuskit_chat_async(
                    provider,
                    request.as_ptr(),
                    Some(observe_chat_callback),
                    state.cast(),
                )
            };
            assert_eq!(accepted, 0, "{profile} {case_id} async acceptance");
            let observation = receiver
                .recv_timeout(Duration::from_secs(3))
                .unwrap_or_else(|_| panic!("{profile} {case_id} async callback"));
            assert_eq!(observation.status, 4, "{profile} {case_id}");
            assert!(observation.payload.is_none(), "{profile} {case_id}");
            assert_eq!(observation.owned_pointer, 0, "{profile} {case_id}");
            let error: serde_json::Value = serde_json::from_str(
                observation
                    .error
                    .as_deref()
                    .expect("rate-limit callback must expose error JSON"),
            )
            .unwrap();
            assert_eq!(error["code"], "rate_limited", "{profile} {case_id}");
            match expected_retry_ms {
                Some(expected) => assert_eq!(error["retry_after_ms"], expected),
                None => assert!(
                    error.get("retry_after_ms").is_none(),
                    "{profile} {case_id} callback must not invent retry metadata: {error}"
                ),
            }
            assert_eq!(unsafe { &*state }.count.load(Ordering::SeqCst), 1);
            unsafe {
                nxuskit_free_provider(provider.cast());
                drop(Box::from_raw(state));
            }
            server.join().expect("async rate-limit fixture server");
            println!("SPEC119_ORACLE_CASE={profile}--c_abi:error_mapping:{case_id}");
        }
        prove_spec119_assertion(&format!("{profile}--c_abi:error_mapping"));
    }
}

#[test]
fn direct_c_ollama_stream_semantics_match_frozen_cases_for_both_profiles() {
    for profile in ["ollama_local", "ollama_cloud_via_daemon"] {
        for (case_id, reasoning, model, thinking_mode) in [
            ("STREAM-BASELINE", false, "llama3.2", "disabled"),
            ("STREAM-REASONING", true, "qwen3", "enabled"),
        ] {
            let (endpoint, server) = serve_ollama_stream(reasoning);
            let provider = create_provider(serde_json::json!({
                "provider_type": profile,
                "model": model,
                "base_url": endpoint,
                "timeout_ms": 3000
            }));
            assert!(!provider.is_null(), "{profile} {case_id}");

            let request = CString::new(
                serde_json::json!({
                    "model": model,
                    "messages": [{"role": "user", "content": "Return JSON"}],
                    "thinking_mode": thinking_mode
                })
                .to_string(),
            )
            .unwrap();
            let (sender, receiver) = sync_channel(16);
            let state = Box::into_raw(Box::new(StreamCallbackState { sender }));

            let stream = unsafe {
                nxuskit_chat_stream(
                    provider.cast(),
                    request.as_ptr(),
                    observe_stream_chunk,
                    observe_stream_done,
                    state.cast(),
                )
            };
            assert!(!stream.is_null(), "{profile} {case_id} start");

            let mut chunks = Vec::new();
            let done = loop {
                match receiver
                    .recv_timeout(Duration::from_secs(3))
                    .unwrap_or_else(|_| panic!("{profile} {case_id} stream callback timeout"))
                {
                    StreamObservation::Chunk(value) => chunks.push(value),
                    StreamObservation::Done(value) => break value,
                }
            };

            let captured = server.join().expect("stream fixture server");
            let wire = captured_ollama_wire(&captured);
            assert_eq!(wire["stream"], true, "{profile} {case_id}");
            assert_eq!(wire["model"], model, "{profile} {case_id}");
            assert_eq!(wire["think"], reasoning, "{profile} {case_id}");

            assert_eq!(chunks.len(), 3, "{profile} {case_id}: {chunks:?}");
            assert_eq!(chunks[0]["delta"], "{\"x\":", "{profile} {case_id}");
            assert_eq!(chunks[1]["delta"], "1}", "{profile} {case_id}");
            assert_eq!(chunks[2]["delta"], "", "{profile} {case_id}");
            for chunk in &chunks {
                assert_eq!(
                    chunk["declared_profile"], profile,
                    "{profile} {case_id}: {chunk}"
                );
            }

            if reasoning {
                assert_eq!(chunks[0]["thinking"], "reason", "{profile} {case_id}");
            } else {
                assert!(
                    chunks[0].get("thinking").is_none(),
                    "{profile} {case_id}: baseline must not invent reasoning"
                );
            }

            let terminal_count = chunks
                .iter()
                .filter(|chunk| chunk["terminal"] == true)
                .count();
            assert_eq!(terminal_count, 1, "{profile} {case_id}");
            assert_eq!(chunks[2]["finish_reason"], "stop", "{profile} {case_id}");
            assert_eq!(
                chunks[2]["raw_finish_reason"], "stop",
                "{profile} {case_id}"
            );
            assert_eq!(
                chunks[2]["usage"]["actual"]["prompt_tokens"], 10,
                "{profile} {case_id}"
            );
            assert_eq!(
                chunks[2]["usage"]["actual"]["completion_tokens"], 2,
                "{profile} {case_id}"
            );
            assert_eq!(
                chunks[2]["usage"]["is_complete"], true,
                "{profile} {case_id}"
            );

            assert_eq!(done["content"], "{\"x\":1}", "{profile} {case_id}");
            assert_eq!(done["declared_profile"], profile, "{profile} {case_id}");
            assert!(done.get("error").is_none(), "{profile} {case_id}: {done}");
            assert!(
                receiver.try_recv().is_err(),
                "{profile} {case_id}: no callbacks are allowed after done"
            );

            unsafe {
                nxuskit_free_stream(stream);
                nxuskit_free_provider(provider.cast());
                drop(Box::from_raw(state));
            }
            println!("SPEC119_ORACLE_CASE={profile}--c_abi:stream_semantics:{case_id}");
        }
        prove_spec119_assertion(&format!("{profile}--c_abi:stream_semantics"));
    }
}

#[test]
fn direct_c_ollama_discovery_semantics_match_frozen_cases_for_both_profiles() {
    for profile in ["ollama_local", "ollama_cloud_via_daemon"] {
        let (health, request) = direct_c_discovery(profile, 0, 200, br#"{"models":[]}"#.to_vec());
        assert_eq!(health["ok"], true, "{profile} DISCOVERY-HEALTH");
        assert!(health["value"].is_null(), "{profile} DISCOVERY-HEALTH");
        assert!(
            request.starts_with("GET /api/tags HTTP/1.1"),
            "{profile} DISCOVERY-HEALTH: {request}"
        );
        println!("SPEC119_ORACLE_CASE={profile}--c_abi:discovery_semantics:DISCOVERY-HEALTH");

        let models_body = serde_json::json!({
            "models": [
                {
                    "name": "qwen3:8b",
                    "size": 42,
                    "digest": "sha256:abc",
                    "modified_at": "2026-08-26T00:00:00Z"
                },
                {"name": "llama3.2"}
            ]
        })
        .to_string()
        .into_bytes();
        let (models, request) = direct_c_discovery(profile, 1, 200, models_body);
        assert_eq!(models["ok"], true, "{profile} DISCOVERY-MODELS");
        assert!(
            request.starts_with("GET /api/tags HTTP/1.1"),
            "{profile} DISCOVERY-MODELS: {request}"
        );

        // The Rust ModelInfo DTO intentionally omits None-valued optional fields
        // during serde serialization. Normalize those omissions back to null so
        // this direct-C proof can compare exact frozen semantic values.
        let mut normalized_models = models["value"].clone();
        for model in normalized_models
            .as_array_mut()
            .expect("discovery models must be an array")
        {
            let model = model
                .as_object_mut()
                .expect("each discovery model must be an object");
            for key in ["size_bytes", "description", "context_window"] {
                model
                    .entry(key.to_string())
                    .or_insert(serde_json::Value::Null);
            }
        }
        assert_eq!(
            normalized_models,
            serde_json::json!([
                {
                    "name": "qwen3:8b",
                    "size_bytes": 42,
                    "description": null,
                    "context_window": null,
                    "metadata": {
                        "digest": "sha256:abc",
                        "modified_at": "2026-08-26T00:00:00Z"
                    }
                },
                {
                    "name": "llama3.2",
                    "size_bytes": null,
                    "description": null,
                    "context_window": null,
                    "metadata": {}
                }
            ]),
            "{profile} DISCOVERY-MODELS"
        );
        println!("SPEC119_ORACLE_CASE={profile}--c_abi:discovery_semantics:DISCOVERY-MODELS");

        let (empty, _) = direct_c_discovery(profile, 1, 200, br#"{"models":[]}"#.to_vec());
        assert_eq!(empty["ok"], true, "{profile} DISCOVERY-EMPTY");
        assert_eq!(
            empty["value"],
            serde_json::json!([]),
            "{profile} DISCOVERY-EMPTY"
        );
        println!("SPEC119_ORACLE_CASE={profile}--c_abi:discovery_semantics:DISCOVERY-EMPTY");

        let (unavailable, _) = direct_c_discovery(profile, 1, 503, b"unavailable".to_vec());
        assert_eq!(unavailable["ok"], false, "{profile} DISCOVERY-UNAVAILABLE");
        assert_eq!(
            unavailable["error"]["code"], "unavailable",
            "{profile} DISCOVERY-UNAVAILABLE"
        );
        assert_eq!(
            unavailable["error"]["message"], "Discovery endpoint unavailable",
            "{profile} DISCOVERY-UNAVAILABLE"
        );
        println!("SPEC119_ORACLE_CASE={profile}--c_abi:discovery_semantics:DISCOVERY-UNAVAILABLE");

        let (unauthorized, _) = direct_c_discovery(profile, 0, 401, b"unauthorized".to_vec());
        assert_eq!(
            unauthorized["ok"], false,
            "{profile} DISCOVERY-UNAUTHORIZED"
        );
        assert_eq!(
            unauthorized["error"]["code"], "authentication",
            "{profile} DISCOVERY-UNAUTHORIZED"
        );
        assert_eq!(
            unauthorized["error"]["message"], "Discovery authentication failed",
            "{profile} DISCOVERY-UNAUTHORIZED"
        );
        println!("SPEC119_ORACLE_CASE={profile}--c_abi:discovery_semantics:DISCOVERY-UNAUTHORIZED");

        let (malformed, _) = direct_c_discovery(profile, 1, 200, br#"{"models":["#.to_vec());
        assert_eq!(malformed["ok"], false, "{profile} DISCOVERY-MALFORMED");
        assert_eq!(
            malformed["error"]["code"], "malformed_response",
            "{profile} DISCOVERY-MALFORMED"
        );
        assert_eq!(
            malformed["error"]["message"], "Discovery response malformed",
            "{profile} DISCOVERY-MALFORMED"
        );
        println!("SPEC119_ORACLE_CASE={profile}--c_abi:discovery_semantics:DISCOVERY-MALFORMED");

        prove_spec119_assertion(&format!("{profile}--c_abi:discovery_semantics"));
    }
}

#[test]
fn direct_c_ollama_edition_package_matches_frozen_binding_and_provenance() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo_root = crate_root
        .ancestors()
        .nth(4)
        .expect("core crate must be nested under the repository root");
    let matrix: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(repo_root.join(
            "specs/119-v2-python-provider-surface-parity/definition/qualification-matrix-v1.json",
        ))
        .expect("frozen Spec119 qualification matrix must be readable"),
    )
    .expect("frozen qualification matrix must be valid JSON");
    let ownership: serde_json::Value =
        serde_json::from_str(
            &fs::read_to_string(repo_root.join(
                "specs/119-v2-python-provider-surface-parity/definition/path-ownership-v1.json",
            ))
            .expect("frozen Spec119 path ownership must be readable"),
        )
        .expect("frozen path ownership must be valid JSON");
    let commands: serde_json::Value =
        serde_json::from_str(
            &fs::read_to_string(repo_root.join(
                "specs/119-v2-python-provider-surface-parity/definition/command-maps-v1.json",
            ))
            .expect("frozen Spec119 command maps must be readable"),
        )
        .expect("frozen command maps must be valid JSON");

    let edition = std::env::var("NXUSKIT_EDITION").expect("NXUSKIT_EDITION must be set");
    assert!(
        matches!(edition.as_str(), "oss" | "pro"),
        "C ABI edition proof must execute as oss or pro, got {edition}"
    );

    let owned = ownership["existing"]
        .as_array()
        .expect("path ownership existing array");
    for path in [
        "packages/nxuskit-engine/crates/nxuskit-core/src/provider.rs",
        "packages/nxuskit-engine/crates/nxuskit-core/src/error.rs",
        "packages/nxuskit-engine/crates/nxuskit-core/src/lib.rs",
        "packages/nxuskit-engine/crates/nxuskit-core/include/nxuskit.h",
        "packages/nxuskit-engine/crates/nxuskit-core/cbindgen.toml",
        "packages/nxuskit-engine/crates/nxuskit-core/symbols.exp",
        "packages/nxuskit-engine/crates/nxuskit-core/nxuskit.def",
    ] {
        let entry = owned
            .iter()
            .find(|entry| entry["path"] == path)
            .unwrap_or_else(|| panic!("missing frozen C ABI ownership entry: {path}"));
        assert_eq!(entry["owner"], "c-abi", "{path}");
        assert_eq!(entry["slice"], "S3", "{path}");
    }

    let gate = commands["gates"]
        .as_array()
        .expect("command-map gates array")
        .iter()
        .find(|gate| gate["id"] == "c_abi_provider_contract")
        .expect("frozen C ABI provider-contract gate");
    assert_eq!(gate["slice"], "S3");
    assert_eq!(gate["evidence_class"], "abi");
    assert_eq!(gate["expected_assertions"], 72);
    for platform in ["macos_arm64", "linux_x86_64", "windows_x86_64"] {
        let platform_gate = &gate["platforms"][platform];
        assert_eq!(platform_gate["network"], "disabled", "{platform}");
        let argv = platform_gate["argv_sequence"].to_string().to_lowercase();
        assert!(argv.contains("nxuskit-core"), "{platform}: {argv}");
        assert!(argv.contains("provider_contract_ffi"), "{platform}: {argv}");
    }

    for profile in ["ollama_local", "ollama_cloud_via_daemon"] {
        let cell_id = format!("{profile}--c_abi");
        let cell = matrix["cells"]
            .as_array()
            .expect("qualification cells array")
            .iter()
            .find(|cell| cell["id"] == cell_id)
            .unwrap_or_else(|| panic!("missing frozen qualification cell: {cell_id}"));
        assert_eq!(cell["applicable"], true, "{cell_id}");
        assert_eq!(
            cell["editions"],
            serde_json::json!(["oss", "pro"]),
            "{cell_id}"
        );
        assert_eq!(
            cell["packages"],
            serde_json::json!(["source", "sdk_bundle"]),
            "{cell_id}"
        );

        let assertion_id = format!("{cell_id}:edition_package");
        let assertion = cell["semantic_binding"]["assertions"]
            .as_array()
            .expect("semantic assertions array")
            .iter()
            .find(|assertion| assertion["id"] == assertion_id)
            .unwrap_or_else(|| panic!("missing frozen edition/package assertion: {assertion_id}"));
        assert_eq!(assertion["class"], "edition_package", "{assertion_id}");
        assert_eq!(
            assertion["oracle_case_ids"],
            serde_json::json!([]),
            "{assertion_id}"
        );
        assert_eq!(
            assertion["expected"],
            serde_json::json!({
                "editions": ["oss", "pro"],
                "packages": ["source", "sdk_bundle"]
            }),
            "{assertion_id}"
        );
        assert_eq!(
            assertion["provenance"], "path-ownership-v1.json and command-maps-v1.json",
            "{assertion_id}"
        );

        println!("SPEC119_EDITION_PACKAGE_RUN={assertion_id}:{edition}:source:sdk_bundle");
        prove_spec119_assertion(&assertion_id);
    }
}

#[test]
fn null_provider_request_or_callback_is_rejected_without_callback() {
    CALLBACK_COUNT.store(0, Ordering::SeqCst);
    let request =
        CString::new(r#"{"model":"echo","messages":[{"role":"user","content":"hello"}]}"#).unwrap();

    assert_eq!(
        unsafe {
            nxuskit_chat_async(
                std::ptr::null_mut(),
                request.as_ptr(),
                Some(chat_callback),
                std::ptr::null_mut(),
            )
        },
        2
    );
    assert_eq!(
        unsafe {
            nxuskit_list_models_async(
                std::ptr::null_mut(),
                Some(list_models_callback),
                std::ptr::null_mut(),
            )
        },
        2
    );

    let provider = loopback_provider();
    assert_eq!(
        unsafe {
            nxuskit_chat_async(
                provider,
                std::ptr::null(),
                Some(chat_callback),
                std::ptr::null_mut(),
            )
        },
        2
    );
    assert_eq!(
        unsafe { nxuskit_chat_async(provider, request.as_ptr(), None, std::ptr::null_mut(),) },
        2
    );
    assert_eq!(
        unsafe { nxuskit_list_models_async(provider, None, std::ptr::null_mut()) },
        2
    );

    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(
        CALLBACK_COUNT.load(Ordering::SeqCst),
        0,
        "rejected calls must not schedule a callback"
    );
    unsafe { nxuskit_free_provider(provider.cast()) };
}

#[test]
fn accepted_chat_calls_back_once_on_the_runtime_worker_with_owned_response() {
    let provider = loopback_provider();
    let request = CString::new(
        r#"{"model":"u-turn-slow","messages":[{"role":"user","content":"async hello"}]}"#,
    )
    .unwrap();
    let (sender, receiver) = sync_channel(2);
    let state = Box::into_raw(Box::new(CallbackState {
        sender,
        count: AtomicUsize::new(0),
    }));

    let started = Instant::now();
    let accepted = unsafe {
        nxuskit_chat_async(
            provider,
            request.as_ptr(),
            Some(observe_chat_callback),
            state.cast(),
        )
    };
    assert_eq!(
        accepted, 0,
        "a valid operation must be accepted immediately"
    );
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "acceptance must not wait for the one-second provider operation"
    );

    let observation = receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("accepted chat must invoke its callback");
    assert_eq!(observation.status, 0);
    assert_eq!(
        observation.thread_name.as_deref(),
        Some("nxuskit_runtime_worker")
    );
    assert!(
        observation.error.is_none(),
        "success must not expose error_json"
    );
    let payload: serde_json::Value = serde_json::from_str(
        observation
            .payload
            .as_deref()
            .expect("success must transfer a response"),
    )
    .expect("response must be strict JSON");
    assert!(
        payload["content"]
            .as_str()
            .is_some_and(|content| content.contains("async hello"))
    );
    assert_ne!(observation.owned_pointer, 0);

    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(unsafe { &*state }.count.load(Ordering::SeqCst), 1);
    assert!(
        receiver.try_recv().is_err(),
        "an accepted operation must not invoke a second callback"
    );

    unsafe {
        nxuskit_free_response(observation.owned_pointer as *mut c_void);
        nxuskit_free_provider(provider.cast());
        drop(Box::from_raw(state));
    }
}

#[test]
fn accepted_model_list_calls_back_once_on_the_runtime_worker_with_owned_string() {
    let provider = loopback_provider();
    let (sender, receiver) = sync_channel(2);
    let state = Box::into_raw(Box::new(CallbackState {
        sender,
        count: AtomicUsize::new(0),
    }));

    let accepted =
        unsafe { nxuskit_list_models_async(provider, Some(observe_models_callback), state.cast()) };
    assert_eq!(
        accepted, 0,
        "a valid operation must be accepted immediately"
    );

    let observation = receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("accepted model listing must invoke its callback");
    assert_eq!(observation.status, 0);
    assert_eq!(
        observation.thread_name.as_deref(),
        Some("nxuskit_runtime_worker")
    );
    assert!(
        observation.error.is_none(),
        "success must not expose error_json"
    );
    let payload: serde_json::Value = serde_json::from_str(
        observation
            .payload
            .as_deref()
            .expect("success must transfer a models string"),
    )
    .expect("models payload must be strict JSON");
    assert!(payload.is_array());
    assert_ne!(observation.owned_pointer, 0);

    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(unsafe { &*state }.count.load(Ordering::SeqCst), 1);
    assert!(
        receiver.try_recv().is_err(),
        "an accepted operation must not invoke a second callback"
    );

    unsafe {
        nxuskit_free_string(observation.owned_pointer as *mut c_char);
        nxuskit_free_provider(provider.cast());
        drop(Box::from_raw(state));
    }
}

#[test]
fn accepted_provider_failure_calls_back_once_with_borrowed_error_json() {
    let provider = loopback_provider();
    let request = CString::new(
        r#"{"model":"u-turn-error-timeout","messages":[{"role":"user","content":"fail"}]}"#,
    )
    .unwrap();
    let (sender, receiver) = sync_channel(2);
    let state = Box::into_raw(Box::new(CallbackState {
        sender,
        count: AtomicUsize::new(0),
    }));

    let accepted = unsafe {
        nxuskit_chat_async(
            provider,
            request.as_ptr(),
            Some(observe_chat_callback),
            state.cast(),
        )
    };
    assert_eq!(accepted, 0, "provider failures happen after acceptance");

    let observation = receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("accepted provider failure must invoke its callback");
    assert_eq!(observation.status, 4, "provider timeout is unavailable");
    assert_eq!(
        observation.thread_name.as_deref(),
        Some("nxuskit_runtime_worker")
    );
    assert!(observation.payload.is_none());
    assert_eq!(observation.owned_pointer, 0);
    let error: serde_json::Value = serde_json::from_str(
        observation
            .error
            .as_deref()
            .expect("error_json must be non-null during an error callback"),
    )
    .expect("callback must expose strict borrowed error JSON");
    assert_eq!(error["code"], "unavailable");
    assert!(
        error["message"]
            .as_str()
            .is_some_and(|message| message.contains("timeout"))
    );

    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(unsafe { &*state }.count.load(Ordering::SeqCst), 1);
    assert!(receiver.try_recv().is_err());

    unsafe {
        nxuskit_free_provider(provider.cast());
        drop(Box::from_raw(state));
    }
}

#[test]
fn callback_error_taxonomy_covers_invalid_authentication_and_rate_limit() {
    let provider = loopback_provider();
    for (model, expected_status, expected_code) in [
        ("u-turn-error-invalid", 2, "invalid_argument"),
        ("u-turn-error-auth", 4, "authentication"),
        ("u-turn-error-rate-limit", 4, "rate_limited"),
    ] {
        let request = CString::new(format!(
            r#"{{"model":"{model}","messages":[{{"role":"user","content":"fail"}}]}}"#
        ))
        .unwrap();
        let (sender, receiver) = sync_channel(2);
        let state = Box::into_raw(Box::new(CallbackState {
            sender,
            count: AtomicUsize::new(0),
        }));

        let accepted = unsafe {
            nxuskit_chat_async(
                provider,
                request.as_ptr(),
                Some(observe_chat_callback),
                state.cast(),
            )
        };
        assert_eq!(accepted, 0, "{model} must fail after acceptance");

        let observation = receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap_or_else(|_| panic!("{model} must invoke its callback"));
        assert_eq!(observation.status, expected_status, "{model}");
        assert_eq!(
            observation.thread_name.as_deref(),
            Some("nxuskit_runtime_worker"),
            "{model}"
        );
        assert!(observation.payload.is_none(), "{model}");
        assert_eq!(observation.owned_pointer, 0, "{model}");
        let error: serde_json::Value = serde_json::from_str(
            observation
                .error
                .as_deref()
                .unwrap_or_else(|| panic!("{model} must expose error_json during the callback")),
        )
        .unwrap_or_else(|_| panic!("{model} must expose strict error JSON"));
        assert_eq!(error["code"], expected_code, "{model}");

        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(unsafe { &*state }.count.load(Ordering::SeqCst), 1);
        assert!(receiver.try_recv().is_err(), "{model}");
        unsafe { drop(Box::from_raw(state)) };
    }
    unsafe { nxuskit_free_provider(provider.cast()) };
}

#[test]
fn generated_header_and_export_authorities_expose_both_async_symbols() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let header = fs::read_to_string(crate_root.join("include/nxuskit.h"))
        .expect("generated package header must exist");
    assert!(header.contains("typedef void (*NxuskitChatAsyncCallback)(int32_t status,"));
    assert!(header.contains("struct NxuskitResponse *response,"));
    assert!(header.contains("typedef void (*NxuskitListModelsAsyncCallback)(int32_t status,"));
    assert!(header.contains("char *models_json,"));
    assert!(header.contains("int32_t nxuskit_chat_async(struct NxuskitProvider *provider,"));
    assert!(header.contains("NxuskitChatAsyncCallback callback,"));
    assert!(header.contains("int32_t nxuskit_list_models_async(struct NxuskitProvider *provider,"));
    assert!(header.contains("NxuskitListModelsAsyncCallback callback,"));
    assert!(header.contains("exactly once on the `nxuskit_runtime_worker` thread"));
    assert!(header.contains("must not free `provider` until the callback returns"));
    assert!(header.contains("`error_json` is borrowed and valid only during the callback"));

    let macos = fs::read_to_string(crate_root.join("symbols.exp"))
        .expect("macOS export authority must exist");
    let windows = fs::read_to_string(crate_root.join("nxuskit.def"))
        .expect("Windows export authority must exist");
    for symbol in ["nxuskit_chat_async", "nxuskit_list_models_async"] {
        assert_eq!(
            macos
                .lines()
                .filter(|line| line.trim() == format!("_{symbol}"))
                .count(),
            1,
            "macOS export authority must contain exactly one {symbol}"
        );
        assert_eq!(
            windows.lines().filter(|line| line.trim() == symbol).count(),
            1,
            "Windows export authority must contain exactly one {symbol}"
        );
    }
}

#[cfg(target_env = "msvc")]
fn msvc_vcvarsall() -> std::path::PathBuf {
    let program_files = std::env::var_os("ProgramFiles(x86)")
        .or_else(|| std::env::var_os("ProgramFiles"))
        .expect("Windows Program Files location must be available");
    let installed_vswhere =
        Path::new(&program_files).join("Microsoft Visual Studio/Installer/vswhere.exe");
    let vswhere = if installed_vswhere.is_file() {
        installed_vswhere
    } else {
        "vswhere.exe".into()
    };
    let output = Command::new(vswhere)
        .args([
            "-latest",
            "-products",
            "*",
            "-requires",
            "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
            "-property",
            "installationPath",
        ])
        .output()
        .expect("vswhere must be available with the MSVC Rust toolchain");
    assert!(
        output.status.success(),
        "vswhere failed to locate an MSVC installation: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let installation = String::from_utf8(output.stdout)
        .expect("Visual Studio installation path must be UTF-8")
        .trim()
        .to_owned();
    assert!(
        !installation.is_empty(),
        "vswhere found no MSVC installation"
    );
    let vcvarsall = Path::new(&installation).join("VC/Auxiliary/Build/vcvarsall.bat");
    assert!(
        vcvarsall.is_file(),
        "MSVC environment script is missing at {}",
        vcvarsall.display()
    );
    vcvarsall
}

#[cfg(target_env = "msvc")]
fn link_msvc_consumer(
    source: &Path,
    include_dir: &Path,
    executable: &Path,
    library_dir: &Path,
    temp_dir: &Path,
) -> std::process::Output {
    let architecture = if cfg!(target_arch = "x86_64") {
        "x64"
    } else if cfg!(target_arch = "x86") {
        "x86"
    } else if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        panic!("unsupported MSVC architecture")
    };
    let script = temp_dir.join("link_provider_async_consumer.cmd");
    fs::write(
        &script,
        format!(
            "@echo off\r\ncall \"{}\" {} >nul\r\nif errorlevel 1 exit /b %errorlevel%\r\ncl.exe /nologo /W4 /WX \"{}\" /I\"{}\" /Fe:\"{}\" /link /LIBPATH:\"{}\" nxuskit_core.dll.lib\r\n",
            msvc_vcvarsall().display(),
            architecture,
            source.display(),
            include_dir.display(),
            executable.display(),
            library_dir.display(),
        ),
    )
    .expect("MSVC link script must be writable");
    Command::new("cmd.exe")
        .args(["/d", "/s", "/c"])
        .arg(script)
        .output()
        .expect("MSVC developer environment must run")
}

#[test]
fn generated_header_links_and_runs_for_a_native_callback_consumer() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo_root = crate_root
        .ancestors()
        .nth(4)
        .expect("core crate must be nested under the repository root");
    let native_target = repo_root.join("target/provider-contract-native");
    let temp = tempfile::tempdir().expect("native-consumer tempdir must be available");
    let source = temp.path().join("provider_async_consumer.c");
    let executable = temp.path().join(if cfg!(windows) {
        "provider_async_consumer.exe"
    } else {
        "provider_async_consumer"
    });
    fs::write(
        &source,
        r#"
#define _POSIX_C_SOURCE 200809L
#include "nxuskit.h"
#include <stddef.h>
#ifdef _WIN32
#include <windows.h>
typedef volatile LONG callback_flag;
#define CALLBACK_FLAG_INIT 0
static void set_flag(callback_flag *flag) { InterlockedExchange(flag, 1); }
static int flag_is_set(callback_flag *flag) {
    return InterlockedCompareExchange(flag, 0, 0) != 0;
}
static void wait_tick(void) { Sleep(10); }
#else
#include <stdatomic.h>
#include <time.h>
typedef atomic_int callback_flag;
#define CALLBACK_FLAG_INIT ATOMIC_VAR_INIT(0)
static void set_flag(callback_flag *flag) {
    atomic_store_explicit(flag, 1, memory_order_release);
}
static int flag_is_set(callback_flag *flag) {
    return atomic_load_explicit(flag, memory_order_acquire);
}
static void wait_tick(void) {
    const struct timespec delay = {0, 10000000};
    (void)nanosleep(&delay, NULL);
}
#endif

struct callback_state {
    callback_flag done;
    int ok;
};

static void chat_done(int32_t status, struct NxuskitResponse *response,
                      const char *error_json, void *user_data) {
    struct callback_state *state = (struct callback_state *)user_data;
    state->ok = status == 0 && response != NULL && error_json == NULL &&
                nxuskit_response_json(response) != NULL;
    if (response != NULL) {
        nxuskit_free_response(response);
    }
    set_flag(&state->done);
}

static void models_done(int32_t status, char *models_json,
                        const char *error_json, void *user_data) {
    struct callback_state *state = (struct callback_state *)user_data;
    state->ok = status == 0 && models_json != NULL && error_json == NULL &&
                models_json[0] == '[';
    if (models_json != NULL) {
        nxuskit_free_string(models_json);
    }
    set_flag(&state->done);
}

static int wait_for_callback(struct callback_state *state) {
    int iteration;
    for (iteration = 0; iteration < 500; iteration++) {
        if (flag_is_set(&state->done)) {
            return 1;
        }
        wait_tick();
    }
    return 0;
}

int main(void) {
    struct NxuskitProvider *provider;
    struct callback_state chat_state = { CALLBACK_FLAG_INIT, 0 };
    struct callback_state models_state = { CALLBACK_FLAG_INIT, 0 };

    if (nxuskit_chat_async(NULL, "{}", chat_done, NULL) != 2) {
        return 1;
    }
    if (nxuskit_list_models_async(NULL, models_done, NULL) != 2) {
        return 2;
    }
    provider = nxuskit_create_provider("{\"provider_type\":\"loopback\"}");
    if (provider == NULL) {
        return 3;
    }
    if (nxuskit_chat_async(
            provider,
            "{\"model\":\"echo\",\"messages\":[{\"role\":\"user\",\"content\":\"native\"}]}",
            chat_done,
            &chat_state) != 0) {
        nxuskit_free_provider(provider);
        return 4;
    }
    if (!wait_for_callback(&chat_state) || !chat_state.ok) {
        nxuskit_free_provider(provider);
        return 5;
    }
    if (nxuskit_list_models_async(provider, models_done, &models_state) != 0) {
        nxuskit_free_provider(provider);
        return 6;
    }
    if (!wait_for_callback(&models_state) || !models_state.ok) {
        nxuskit_free_provider(provider);
        return 7;
    }
    nxuskit_free_provider(provider);
    return 0;
}
"#,
    )
    .expect("native-consumer source must be writable");

    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let native_target_triple = if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "x86_64-apple-darwin"
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "aarch64-apple-darwin"
    } else if cfg!(all(
        target_os = "linux",
        target_arch = "x86_64",
        target_env = "gnu"
    )) {
        "x86_64-unknown-linux-gnu"
    } else if cfg!(all(
        target_os = "windows",
        target_arch = "x86_64",
        target_env = "msvc"
    )) {
        "x86_64-pc-windows-msvc"
    } else {
        panic!("unsupported native provider-contract test target")
    };
    let build = Command::new(cargo)
        .args([
            "build",
            "--locked",
            "--offline",
            "--target",
            native_target_triple,
            "-p",
            "nxuskit-core",
        ])
        .current_dir(repo_root)
        .env("CARGO_TARGET_DIR", &native_target)
        .output()
        .expect("Cargo must be available to build the native library");
    assert!(
        build.status.success(),
        "native library build failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    let library_dir = native_target.join(native_target_triple).join("debug");
    let link = {
        #[cfg(target_env = "msvc")]
        {
            link_msvc_consumer(
                &source,
                &crate_root.join("include"),
                &executable,
                &library_dir,
                temp.path(),
            )
        }
        #[cfg(not(target_env = "msvc"))]
        {
            let compiler = std::env::var_os("CC").unwrap_or_else(|| "cc".into());
            let mut command = Command::new(compiler);
            command.args(["-std=c11", "-Wall", "-Wextra", "-Werror"]);
            if cfg!(target_os = "macos") {
                command.args([
                    "-arch",
                    if cfg!(target_arch = "x86_64") {
                        "x86_64"
                    } else {
                        "arm64"
                    },
                ]);
            }
            command
                .arg(&source)
                .arg("-I")
                .arg(crate_root.join("include"))
                .arg("-L")
                .arg(&library_dir)
                .arg("-lnxuskit_core")
                .arg("-o")
                .arg(&executable)
                .args(
                    cfg!(target_family = "unix")
                        .then(|| format!("-Wl,-rpath,{}", library_dir.display())),
                )
                .output()
                .expect("the native C compiler used by the Rust build must be available")
        }
    };

    assert!(
        link.status.success(),
        "generated-header native consumer must link:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&link.stdout),
        String::from_utf8_lossy(&link.stderr)
    );

    // The import library binds the public DLL name from nxuskit.def, not
    // Cargo's output filename. Isolate and refresh it beside this executable.
    #[cfg(windows)]
    fs::copy(
        library_dir.join("nxuskit_core.dll"),
        temp.path().join("nxuskit.dll"),
    )
    .expect("the exact native library must be staged under its public import name");

    let mut run = Command::new(&executable);
    let library_path_variable = if cfg!(target_os = "macos") {
        "DYLD_LIBRARY_PATH"
    } else if cfg!(windows) {
        "PATH"
    } else {
        "LD_LIBRARY_PATH"
    };
    let mut library_paths = vec![library_dir];
    if let Some(current) = std::env::var_os(library_path_variable) {
        library_paths.extend(std::env::split_paths(&current));
    }
    run.env(
        library_path_variable,
        std::env::join_paths(library_paths).expect("valid native library search path"),
    );
    let output = run
        .output()
        .expect("linked native callback consumer must be executable");
    assert!(
        output.status.success(),
        "linked native callback consumer must run ({}):\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
