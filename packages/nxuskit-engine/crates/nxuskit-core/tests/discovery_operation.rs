use nxuskit_core::*;
use std::ffi::{CStr, CString};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::Duration;

fn config(endpoint: String) -> CString {
    CString::new(
        serde_json::json!({"provider_id":"lmstudio", "endpoint":endpoint,
        "auth_selector":null,"connect_timeout_ms":1000,"read_timeout_ms":1000})
        .to_string(),
    )
    .unwrap()
}

#[test]
fn null_and_unknown_operation_are_rejected_without_work() {
    unsafe {
        assert!(nxuskit_discovery_create(std::ptr::null()).is_null());
        let client = nxuskit_discovery_create(config("http://127.0.0.1:1/api/v1".into()).as_ptr());
        assert!(!client.is_null());
        assert!(nxuskit_discovery_operation_start(client, 99).is_null());
        nxuskit_discovery_free(client);
    }
}

#[test]
fn operation_retains_client_and_cancel_drops_transport_before_result() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/api/v1", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut bytes = vec![];
        while !bytes.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            socket.read_exact(&mut byte).unwrap();
            bytes.push(byte[0]);
        }
        assert!(bytes.starts_with(b"GET /api/v1/models HTTP/1.1"));
        tx.send(()).unwrap();
        let mut byte = [0];
        assert_eq!(socket.read(&mut byte).unwrap(), 0);
    });
    unsafe {
        let client = nxuskit_discovery_create(config(endpoint).as_ptr());
        assert!(!client.is_null());
        let op = nxuskit_discovery_operation_start(client, 2);
        assert!(!op.is_null());
        rx.recv_timeout(Duration::from_secs(3)).unwrap();
        assert!(!nxuskit_discovery_operation_free(op));
        nxuskit_discovery_free(client); // operation owns Arc, not caller pointer
        nxuskit_discovery_operation_cancel(op);
        assert!(nxuskit_discovery_operation_wait(op, 3000));
        let raw = nxuskit_discovery_operation_result(op);
        assert!(!raw.is_null());
        let result: serde_json::Value =
            serde_json::from_slice(CStr::from_ptr(raw).to_bytes()).unwrap();
        assert_eq!(result["error"]["code"], "cancelled");
        nxuskit_free_string(raw);
        assert!(nxuskit_discovery_operation_free(op));
    }
    server.join().unwrap();
}

#[test]
fn snapshot_result_is_owned_and_health_close_is_independent() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/api/v1", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut request = [0; 4096];
        socket.read(&mut request).unwrap();
        socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 13\r\nConnection: close\r\n\r\n{\"models\":[]}").unwrap();
    });
    unsafe {
        let client = nxuskit_discovery_create(config(endpoint).as_ptr());
        let op = nxuskit_discovery_operation_start(client, 2);
        assert!(nxuskit_discovery_operation_wait(op, 3000));
        let raw = nxuskit_discovery_operation_result(op);
        assert!(!raw.is_null());
        let result: serde_json::Value =
            serde_json::from_slice(CStr::from_ptr(raw).to_bytes()).unwrap();
        assert_eq!(result["value"]["load_state"], "not_loaded");
        assert!(nxuskit_discovery_operation_free(op));
        nxuskit_free_string(raw); // copy survives operation free
        assert!(nxuskit_discovery_close(client));
        let op = nxuskit_discovery_operation_start(client, 0);
        assert!(nxuskit_discovery_operation_wait(op, 3000));
        let raw = nxuskit_discovery_operation_result(op);
        assert!(!raw.is_null());
        let result: serde_json::Value =
            serde_json::from_slice(CStr::from_ptr(raw).to_bytes()).unwrap();
        assert_eq!(result["error"]["code"], "unavailable");
        nxuskit_free_string(raw);
        assert!(nxuskit_discovery_operation_free(op));
        nxuskit_discovery_free(client);
    }
    server.join().unwrap();
}
