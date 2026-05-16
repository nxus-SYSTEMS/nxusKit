//! T032: Multimodal type re-export verification (FR-009).
//!
//! Verifies that all multimodal types are accessible via `nxuskit::` and that
//! the builder methods produce the expected message structure.

#[test]
fn multimodal_types_importable() {
    // All multimodal types must be importable from nxuskit::
    let _: fn() -> nxuskit::ContentPart = || nxuskit::ContentPart::Text {
        text: "hello".to_string(),
    };
    let _: fn() -> nxuskit::ImageSource = || nxuskit::ImageSource {
        data: nxuskit::ImageData::Url {
            url: "https://example.com/img.jpg".to_string(),
        },
        detail: None,
    };
    let _: fn() -> nxuskit::ImageData = || nxuskit::ImageData::Base64 {
        media_type: "image/png".to_string(),
        data: "abc123".to_string(),
    };
}

#[test]
fn message_user_backward_compat() {
    let msg = nxuskit::Message::user("Hello");
    assert_eq!(msg.role, nxuskit::Role::User);
    match &msg.content {
        nxuskit::MessageContent::Text(text) => assert_eq!(text, "Hello"),
        nxuskit::MessageContent::Parts(_) => panic!("expected Text content for text-only"),
    }
}

#[test]
fn message_with_image_url_produces_parts() {
    let msg = nxuskit::Message::user("What is this?")
        .with_image_url("https://example.com/img.jpg");
    match &msg.content {
        nxuskit::MessageContent::Parts(parts) => {
            assert_eq!(parts.len(), 2, "should have text + image parts");
            match &parts[0] {
                nxuskit::ContentPart::Text { text } => assert_eq!(text, "What is this?"),
                _ => panic!("first part should be Text"),
            }
            match &parts[1] {
                nxuskit::ContentPart::Image { source } => match &source.data {
                    nxuskit::ImageData::Url { url } => {
                        assert_eq!(url, "https://example.com/img.jpg");
                    }
                    _ => panic!("expected Url image data"),
                },
                _ => panic!("second part should be Image"),
            }
        }
        nxuskit::MessageContent::Text(_) => panic!("expected Parts content"),
    }
}

#[test]
fn message_with_image_base64_produces_base64_data() {
    let msg =
        nxuskit::Message::user("Describe").with_image_base64("abc123", "image/png");
    match &msg.content {
        nxuskit::MessageContent::Parts(parts) => {
            assert_eq!(parts.len(), 2);
            match &parts[1] {
                nxuskit::ContentPart::Image { source } => {
                    match &source.data {
                        nxuskit::ImageData::Base64 { data, media_type } => {
                            assert_eq!(data, "abc123");
                            assert_eq!(media_type, "image/png");
                        }
                        _ => panic!("expected Base64 image data"),
                    }
                }
                _ => panic!("second part should be Image"),
            }
        }
        nxuskit::MessageContent::Text(_) => panic!("expected Parts content"),
    }
}

/// Verify current (non-deprecated) names compile without warnings.
#[deny(deprecated)]
#[test]
fn current_names_no_deprecation_warnings() {
    let _msg = nxuskit::Message::user("test");
    let _content = nxuskit::MessageContent::Text("hello".to_string());
    let _part = nxuskit::ContentPart::Text {
        text: "hello".to_string(),
    };
    let _role = nxuskit::Role::User;
}
