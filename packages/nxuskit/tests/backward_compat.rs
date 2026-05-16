use nxuskit::{ChatRequest, ChatResponse, Message};

#[test]
fn v092_style_response_without_logprobs_deserializes() {
    let response: ChatResponse = serde_json::from_str(
        r#"{
            "content": "Hello from v0.9.2",
            "model": "gpt-5.4",
            "provider": "mock",
            "usage": {
                "estimated": {"prompt_tokens": 3, "completion_tokens": 4}
            }
        }"#,
    )
    .expect("v0.9.2-style response without logprobs must parse");

    assert_eq!(response.content, "Hello from v0.9.2");
    assert_eq!(response.model, "gpt-5.4");
    assert_eq!(response.provider, "mock");
    assert_eq!(response.usage.estimated.prompt_tokens, 3);
    assert_eq!(response.usage.estimated.completion_tokens, 4);
    assert!(response.logprobs.is_none());
}

#[test]
fn v092_consumer_request_serializes_byte_identically_without_logprobs() {
    let request = ChatRequest::new("gpt-5.4")
        .with_message(Message::user("Hello from v0.9.2"));

    let actual = serde_json::to_string(&request).expect("request must serialize");
    let expected = include_str!("fixtures/v092-chat-request-no-logprobs.json").trim();

    assert_eq!(actual, expected);
    assert!(!actual.contains("logprobs"));
    assert!(!actual.contains("top_logprobs"));
}
