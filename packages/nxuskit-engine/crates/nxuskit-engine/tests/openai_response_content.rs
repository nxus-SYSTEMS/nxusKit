use nxuskit_engine::{ChatRequest, FinishReason, LLMProvider, Message, OpenAIProvider};
use serde_json::{Value, json};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

#[tokio::test]
async fn response_content_string_array_null_and_malformed_are_distinguished() {
    for (content, expected) in [
        (json!("standard text"), Some("standard text")),
        (
            json!([{"type":"text","text":"part one"},{"type":"text","text":"part two"}]),
            Some("part one\npart two"),
        ),
        (Value::Null, Some("")),
        (json!(""), Some("")),
        (json!([]), Some("")),
        (json!(42), None),
        (json!({"text":"not a content value"}), None),
        (json!([{"type":"text","text":42}]), None),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("POST")).and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "model":"literal-model", "choices":[{"message":{"role":"assistant","content":content},"finish_reason":"length"}],
                "usage":{"prompt_tokens":7,"completion_tokens":3}
            }))).expect(1).mount(&server).await;
        let provider = OpenAIProvider::builder()
            .api_key("local-test-only")
            .base_url(server.uri())
            .build()
            .unwrap();
        let request =
            ChatRequest::new("request-model").with_message(Message::user("literal request"));
        let result = provider.chat(&request).await;
        if let Some(expected) = expected {
            let response = result.unwrap();
            assert_eq!(response.content, expected);
            assert_eq!(response.model, "literal-model");
            assert_eq!(response.finish_reason, Some(FinishReason::Length));
            let response = serde_json::to_value(response).unwrap();
            assert_eq!(response["usage"]["estimated"]["prompt_tokens"], 7);
            assert_eq!(response["usage"]["estimated"]["completion_tokens"], 3);
        } else {
            assert!(
                result.is_err(),
                "malformed response must not become empty success"
            );
        }
        let received = server.received_requests().await.unwrap();
        let outgoing: Value = serde_json::from_slice(&received[0].body).unwrap();
        assert_eq!(
            outgoing["messages"][0]["content"],
            json!([{"type":"text","text":"literal request"}])
        );
    }
}
