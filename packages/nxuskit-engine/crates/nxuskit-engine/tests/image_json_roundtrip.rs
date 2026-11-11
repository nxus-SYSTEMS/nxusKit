//! Exercise the production request types consumed by nxuskit_chat.
use nxuskit_engine::{ChatRequest, Message};

#[test]
fn image_request_roundtrips_without_duplicate_tags() {
    let request = ChatRequest::new("image-model").with_messages(vec![
        Message::user("describe")
            .with_image_url("https://image.invalid/p.png")
            .with_image_base64("aGVsbG8=", "image/png"),
    ]);
    let encoded = serde_json::to_string(&request).unwrap();
    let decoded: ChatRequest = serde_json::from_str(&encoded)
        .unwrap_or_else(|error| panic!("image roundtrip failed: {error}; JSON={encoded}"));
    assert_eq!(decoded.messages, request.messages);
}

#[test]
fn nested_images_and_options_parse_in_both_production_request_types() {
    let raw = r#"{"model":"image-model","messages":[{"role":"user","content":[
      {"type":"text","text":"describe"},
      {"type":"image","source":{"type":"url","url":"https://image.invalid/p.png","detail":"high"}},
      {"type":"image","source":{"type":"base64","media_type":"image/png","data":"aGVsbG8="}}
    ]}],"temperature":0.0,"max_tokens":7,"top_p":0.5,"stop":["END"],
    "response_format":{"type":"json"},
    "tools":[{"type":"function","function":{"name":"lookup"}}],"tool_choice":"required"}"#;
    let engine: ChatRequest = serde_json::from_str(raw).unwrap();
    let wrapper: nxuskit::ChatRequest = serde_json::from_str(raw).unwrap();
    assert_eq!(engine.messages.len(), 1);
    assert_eq!(wrapper.messages.len(), 1);
    assert_eq!(engine.stop, Some(vec!["END".to_string()]));
    assert_eq!(engine.max_tokens, Some(7));
    assert_eq!(engine.temperature, Some(0.0));
    assert_eq!(engine.top_p, Some(0.5));
    assert_eq!(engine.tool_choice, Some(serde_json::json!("required")));
    assert_eq!(
        engine.tools.as_ref().unwrap()[0]["function"]["name"],
        "lookup"
    );
    assert_eq!(
        serde_json::to_value(engine.response_format).unwrap(),
        serde_json::json!({"type":"json"})
    );
    let engine_json = serde_json::to_value(&engine.messages).unwrap();
    let wrapper_json = serde_json::to_value(&wrapper.messages).unwrap();
    let expected = serde_json::from_str::<serde_json::Value>(raw).unwrap()["messages"].clone();
    assert_eq!(engine_json, expected);
    assert_eq!(wrapper_json, expected);
    assert_eq!(
        serde_json::from_value::<Vec<Message>>(wrapper_json).unwrap(),
        engine.messages
    );
    let cross: Vec<nxuskit::Message> = serde_json::from_value(engine_json).unwrap();
    assert_eq!(serde_json::to_value(cross).unwrap(), expected);
}

#[test]
fn text_remains_compatible_and_invalid_images_fail_closed() {
    let text = r#"{"model":"text-model","messages":[{"role":"user","content":"hello"}]}"#;
    assert!(serde_json::from_str::<ChatRequest>(text).is_ok());
    assert!(serde_json::from_str::<nxuskit::ChatRequest>(text).is_ok());
    for image in [
        r#"{"type":"image","type":"url","url":"https://image.invalid/p.png"}"#,
        r#"{"type":"image","url":"https://image.invalid/p.png"}"#,
        r#"{"type":"image","source":{"type":"url"}}"#,
        r#"{"type":"image","source":{"type":"base64","data":"aGVsbG8="}}"#,
        r#"{"type":"image","source":{"type":"filepath","path":"/not/read"}}"#,
    ] {
        let raw = format!(r#"{{"role":"user","content":[{image}]}}"#);
        assert!(serde_json::from_str::<Message>(&raw).is_err(), "{raw}");
        assert!(
            serde_json::from_str::<nxuskit::Message>(&raw).is_err(),
            "{raw}"
        );
    }
}
