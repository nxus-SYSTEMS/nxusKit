//! v0.9.3 (T078): unsupported-provider logprobs warn-and-drop regression
//! tests.
//!
//! Pins the engine contract that when a request carries `logprobs` /
//! `top_logprobs` for a provider whose `ProviderCapabilities` reports
//! `supports_logprobs: false`, the parameter adapter:
//!   - emits a structured warning with parameter == "logprobs",
//!   - drops both `logprobs` and `top_logprobs` from the request,
//!   - does NOT tunnel either field through `provider_options`.
//!
//! This is the engine-side enforcement that the Python and Rust wrapper
//! tests rely on (see `chatrequest_provider_options_does_not_tunnel_logprobs_to_top_level`
//! and `test_provider_options_does_not_tunnel_logprobs_to_first_class`).

use nxuskit_engine::parameter_adapter::ParameterAdapter;
use nxuskit_engine::types::{ChatRequest, Message, ProviderCapabilities, WarningSeverity};

fn unsupported_capabilities() -> ProviderCapabilities {
    ProviderCapabilities {
        supports_logprobs: false,

        supports_streaming_logprobs: false,
        max_logprobs: None,
        ..Default::default()
    }
}

#[test]
fn unsupported_provider_drops_logprobs_and_top_logprobs() {
    let mut request = ChatRequest::new("test-model").with_message(Message::user("Hi"));
    request.logprobs = Some(true);
    request.top_logprobs = Some(5);

    let adapted = ParameterAdapter::adapt(&request, &unsupported_capabilities());

    assert_eq!(adapted.request.logprobs, None, "logprobs must be dropped");
    assert_eq!(
        adapted.request.top_logprobs, None,
        "top_logprobs must be dropped alongside logprobs"
    );
}

#[test]
fn unsupported_provider_emits_structured_warning_for_logprobs() {
    let mut request = ChatRequest::new("test-model").with_message(Message::user("Hi"));
    request.logprobs = Some(true);
    request.top_logprobs = Some(3);

    let adapted = ParameterAdapter::adapt(&request, &unsupported_capabilities());

    let warning = adapted
        .warnings
        .iter()
        .find(|w| w.parameter == "logprobs")
        .expect("expected a warning with parameter==\"logprobs\"");

    // Severity should be Info (not Error) — warn-and-drop is graceful, not fatal.
    assert_eq!(warning.severity, WarningSeverity::Info);
    assert!(
        !warning.message.is_empty(),
        "warning message must be non-empty for telemetry"
    );
}

#[test]
fn dropped_logprobs_do_not_appear_anywhere_in_serialized_request() {
    // After warn-and-drop, the wire JSON must not surface logprobs or
    // top_logprobs at the top level. The engine's ProviderOptions is a
    // typed enum (Ollama / CLIPS / Local / Z3), not a free-form bag, so
    // structural tunneling is impossible at this layer; the wrapper-side
    // serde_json::Value provider_options has its own non-tunneling guard
    // (see nxuskit::tests::mock_provider and nxuskit-py test_chat_logprobs).
    let mut request = ChatRequest::new("test-model").with_message(Message::user("Hi"));
    request.logprobs = Some(true);
    request.top_logprobs = Some(5);

    let adapted = ParameterAdapter::adapt(&request, &unsupported_capabilities());

    let json = serde_json::to_value(&adapted.request).expect("serialize adapted request");
    assert!(
        json.get("logprobs").is_none(),
        "logprobs must not appear in adapted wire JSON"
    );
    assert!(
        json.get("top_logprobs").is_none(),
        "top_logprobs must not appear in adapted wire JSON"
    );
}

#[test]
fn supported_provider_preserves_logprobs_within_max_limit() {
    let mut request = ChatRequest::new("test-model").with_message(Message::user("Hi"));
    request.logprobs = Some(true);
    request.top_logprobs = Some(3);

    let capabilities = ProviderCapabilities {
        supports_logprobs: true,

        supports_streaming_logprobs: false,
        max_logprobs: Some(20),
        ..Default::default()
    };

    let adapted = ParameterAdapter::adapt(&request, &capabilities);

    assert_eq!(adapted.request.logprobs, Some(true));
    assert_eq!(adapted.request.top_logprobs, Some(3));
    // No warning for the logprobs/top_logprobs parameters in the supported case.
    assert!(
        !adapted
            .warnings
            .iter()
            .any(|w| w.parameter == "logprobs" || w.parameter == "top_logprobs"),
        "supported provider within limits should not produce logprobs warnings, got {:?}",
        adapted.warnings
    );
}

#[test]
fn unsupported_provider_with_logprobs_absent_emits_no_warning() {
    let request = ChatRequest::new("test-model").with_message(Message::user("Hi"));

    let adapted = ParameterAdapter::adapt(&request, &unsupported_capabilities());

    assert_eq!(adapted.request.logprobs, None);
    assert!(
        !adapted
            .warnings
            .iter()
            .any(|w| w.parameter == "logprobs" || w.parameter == "top_logprobs"),
        "no warning expected when caller did not request logprobs"
    );
}

#[test]
fn supported_provider_clamps_top_logprobs_above_max() {
    let mut request = ChatRequest::new("test-model").with_message(Message::user("Hi"));
    request.logprobs = Some(true);
    request.top_logprobs = Some(50);

    let capabilities = ProviderCapabilities {
        supports_logprobs: true,

        supports_streaming_logprobs: false,
        max_logprobs: Some(10),
        ..Default::default()
    };

    let adapted = ParameterAdapter::adapt(&request, &capabilities);

    assert_eq!(adapted.request.logprobs, Some(true));
    assert_eq!(
        adapted.request.top_logprobs,
        Some(10),
        "top_logprobs must clamp to provider maximum, not drop"
    );
    let warning = adapted
        .warnings
        .iter()
        .find(|w| w.parameter == "top_logprobs")
        .expect("clamping must emit a warning");
    assert_eq!(warning.severity, WarningSeverity::Warning);
}
