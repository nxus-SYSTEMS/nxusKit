use nxuskit_engine::discovery::{DiscoveryClientFactory, DiscoveryConfig};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn probe(body: &'static str) -> serde_json::Value {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/api/v1", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        loop {
            let mut byte = [0];
            socket.read_exact(&mut byte).await.unwrap();
            request.push(byte[0]);
            if request.ends_with(b"\r\n\r\n") {
                break;
            }
        }
        let request = String::from_utf8(request).unwrap();
        assert!(request.starts_with("GET /api/v1/models HTTP/1.1\r\n"));
        assert!(!request.to_lowercase().contains("authorization:"));
        socket
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
    });
    let client = DiscoveryClientFactory::create(DiscoveryConfig {
        provider_id: "lmstudio".into(),
        endpoint,
        auth_selector: None,
        connect_timeout_ms: 1000,
        read_timeout_ms: 1000,
    })
    .unwrap();
    let result = match client.snapshot().await {
        Ok(value) => serde_json::to_value(value).unwrap(),
        Err(error) => json!({"error": error}),
    };
    client.close().await.unwrap();
    assert!(client.snapshot().await.is_err());
    server.await.unwrap();
    result
}

#[tokio::test]
async fn empty_catalog_is_reachable_without_loaded_model() {
    let value = probe(r#"{"models":[]}"#).await;
    assert_eq!(value["load_state"], "not_loaded");
    assert_eq!(value["models"], json!([]));
    assert_eq!(value["api"]["minimum_server_version"], "0.4.0");
    assert!(value["api"]["reported_server_version"].is_null());
}

#[tokio::test]
async fn catalog_membership_does_not_imply_loaded() {
    let value = probe(r#"{"models":[{"key":"local/model","type":"llm","publisher":"fixture","display_name":"Local","size_bytes":12,"max_context_length":4096,"loaded_instances":[]}]}"#).await;
    assert_eq!(value["load_state"], "not_loaded");
    assert_eq!(value["models"][0]["name"], "local/model");
    assert_eq!(value["model_states"][0]["loaded_instances"], json!([]));
}

#[tokio::test]
async fn only_explicit_instances_establish_loaded() {
    let value = probe(r#"{"models":[{"key":"local/model","type":"llm","publisher":"fixture","display_name":"Local","size_bytes":12,"max_context_length":4096,"loaded_instances":[{"id":"instance-a","config":{"context_length":2048}}]}]}"#).await;
    assert_eq!(value["load_state"], "loaded");
    assert_eq!(
        value["model_states"][0]["loaded_instances"],
        json!([{"id":"instance-a","context_length":2048}])
    );
}

#[tokio::test]
async fn missing_state_is_unknown_not_unloaded() {
    let value = probe(r#"{"models":[{"key":"local/model","type":"llm","publisher":"fixture","display_name":"Local","size_bytes":12,"max_context_length":4096}]}"#).await;
    assert_eq!(value["load_state"], "unknown");
    assert!(value["model_states"][0]["loaded_instances"].is_null());
}

#[tokio::test]
async fn malformed_state_and_legacy_envelope_are_errors() {
    for body in [
        r#"{"models":[{"key":"local/model","type":"llm","publisher":"fixture","display_name":"Local","size_bytes":12,"max_context_length":4096,"loaded_instances":{}}]}"#,
        r#"{"data":[]}"#,
    ] {
        let value = probe(body).await;
        assert_eq!(value["error"]["code"], "malformed_response");
    }
}
