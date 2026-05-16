//! Tests for typed provider builders (T031, T033).
//!
//! Tests that require the SDK runtime are `#[ignore]`; they can be run
//! with `cargo test -- --ignored` when `libnxuskit` is available.

use nxuskit::NxuskitError;
use nxuskit::builders::*;

// ---------------------------------------------------------------------------
// T031: Cloud provider builder API surface
// ---------------------------------------------------------------------------

#[test]
fn claude_builder_missing_api_key_returns_config_error() {
    let result = ClaudeProvider::builder().build();
    assert!(result.is_err());
    match result.unwrap_err() {
        NxuskitError::Configuration { message } => {
            assert!(
                message.contains("api_key"),
                "expected api_key error, got: {message}"
            );
        }
        other => panic!("expected Configuration error, got: {other:?}"),
    }
}

#[test]
fn openai_builder_missing_api_key_returns_config_error() {
    let result = OpenAIProvider::builder().build();
    assert!(result.is_err());
    match result.unwrap_err() {
        NxuskitError::Configuration { message } => {
            assert!(
                message.contains("api_key"),
                "expected api_key error, got: {message}"
            );
        }
        other => panic!("expected Configuration error, got: {other:?}"),
    }
}

#[test]
fn fireworks_builder_missing_api_key_returns_config_error() {
    let result = FireworksProvider::builder().build();
    assert!(result.is_err());
    match result.unwrap_err() {
        NxuskitError::Configuration { message } => {
            assert!(
                message.contains("api_key"),
                "expected api_key error, got: {message}"
            );
        }
        other => panic!("expected Configuration error, got: {other:?}"),
    }
}

#[test]
fn groq_builder_missing_api_key_returns_config_error() {
    let result = GroqProvider::builder().build();
    assert!(result.is_err());
    match result.unwrap_err() {
        NxuskitError::Configuration { message } => {
            assert!(
                message.contains("api_key"),
                "expected api_key error, got: {message}"
            );
        }
        other => panic!("expected Configuration error, got: {other:?}"),
    }
}

#[test]
fn xai_builder_missing_api_key_returns_config_error() {
    let result = XaiProvider::builder().build();
    assert!(result.is_err());
    match result.unwrap_err() {
        NxuskitError::Configuration { message } => {
            assert!(
                message.contains("api_key"),
                "expected api_key error, got: {message}"
            );
        }
        other => panic!("expected Configuration error, got: {other:?}"),
    }
}

#[test]
fn together_builder_missing_api_key_returns_config_error() {
    let result = TogetherProvider::builder().build();
    assert!(result.is_err());
    match result.unwrap_err() {
        NxuskitError::Configuration { message } => {
            assert!(
                message.contains("api_key"),
                "expected api_key error, got: {message}"
            );
        }
        other => panic!("expected Configuration error, got: {other:?}"),
    }
}

#[test]
fn mistral_builder_missing_api_key_returns_config_error() {
    let result = MistralProvider::builder().build();
    assert!(result.is_err());
    match result.unwrap_err() {
        NxuskitError::Configuration { message } => {
            assert!(
                message.contains("api_key"),
                "expected api_key error, got: {message}"
            );
        }
        other => panic!("expected Configuration error, got: {other:?}"),
    }
}

#[test]
fn perplexity_builder_missing_api_key_returns_config_error() {
    let result = PerplexityProvider::builder().build();
    assert!(result.is_err());
    match result.unwrap_err() {
        NxuskitError::Configuration { message } => {
            assert!(
                message.contains("api_key"),
                "expected api_key error, got: {message}"
            );
        }
        other => panic!("expected Configuration error, got: {other:?}"),
    }
}

#[test]
fn openrouter_builder_missing_api_key_returns_config_error() {
    let result = OpenRouterProvider::builder().build();
    assert!(result.is_err());
    match result.unwrap_err() {
        NxuskitError::Configuration { message } => {
            assert!(
                message.contains("api_key"),
                "expected api_key error, got: {message}"
            );
        }
        other => panic!("expected Configuration error, got: {other:?}"),
    }
}

// --- Local providers (no API key required) ---

#[test]
fn ollama_builder_no_required_fields() {
    // Ollama doesn't require an API key — the builder should succeed at the
    // *config-construction* level. It will fail at NxuskitProvider::new() if
    // the SDK isn't available, but the builder itself should be valid.
    let config = OllamaProvider::builder()
        .model("llama3")
        .base_url("http://localhost:11434")
        .to_config();
    assert_eq!(config.provider_type, "ollama");
    assert_eq!(config.model.as_deref(), Some("llama3"));
    assert_eq!(config.base_url.as_deref(), Some("http://localhost:11434"));
}

#[test]
fn lmstudio_builder_no_required_fields() {
    let config = LmStudioProvider::builder().model("phi-3").to_config();
    assert_eq!(config.provider_type, "lmstudio");
    assert_eq!(config.model.as_deref(), Some("phi-3"));
}

// ---------------------------------------------------------------------------
// T033: LoopbackProvider builder
// ---------------------------------------------------------------------------

#[test]
fn loopback_builder_no_required_fields() {
    let config = LoopbackProvider::builder().model("echo").to_config();
    assert_eq!(config.provider_type, "loopback");
    assert_eq!(config.model.as_deref(), Some("echo"));
}

// --- SDK-dependent tests (require libnxuskit runtime) ---

#[test]
#[ignore = "requires libnxuskit runtime"]
fn claude_builder_creates_valid_provider() {
    let provider = ClaudeProvider::builder()
        .api_key("test-key")
        .model("claude-sonnet-4-6")
        .build();
    assert!(provider.is_ok(), "failed: {:?}", provider.err());
}

#[test]
#[ignore = "requires libnxuskit runtime"]
fn loopback_builder_creates_valid_provider() {
    let provider = LoopbackProvider::builder().model("echo").build();
    assert!(provider.is_ok(), "failed: {:?}", provider.err());
}

#[test]
#[ignore = "requires libnxuskit runtime"]
fn loopback_echo_through_ffi() {
    let provider = LoopbackProvider::builder()
        .build()
        .expect("loopback should build");
    let request =
        nxuskit::ChatRequest::new("loopback").with_message(nxuskit::Message::user("Hello, world!"));
    let response = provider.chat(request).expect("chat should succeed");
    assert!(
        response.content.contains("Hello, world!"),
        "expected echo, got: {}",
        response.content
    );
}
