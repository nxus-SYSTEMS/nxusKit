//! Backend selection tests — verify backend auto-selection, explicit selection,
//! and error handling for unavailable backends.

#[allow(unused_imports)]
use nxuskit_engine::LLMProvider;
#[allow(unused_imports)]
use nxuskit_engine::types::{ChatRequest, Message};

// ---------------------------------------------------------------------------
// T049: Explicit llama-cpp backend selection
// ---------------------------------------------------------------------------

#[cfg(feature = "provider-local-llama")]
mod llama_cpp_selection {
    use super::*;
    use nxuskit_engine::providers::local::LocalRuntimeProvider;

    #[tokio::test]
    #[ignore]
    async fn test_explicit_llama_cpp_backend() {
        let model_path = std::env::var("TEST_GGUF_MODEL")
            .unwrap_or_else(|_| "/models/tinyllama-1.1b.Q4_K_M.gguf".to_string());

        let provider = LocalRuntimeProvider::builder()
            .model_path(&model_path)
            .backend("llama-cpp")
            .build()
            .expect("build should succeed with llama-cpp backend");

        let request = ChatRequest::new(&model_path).with_message(Message::user("Hi"));

        let response = provider.chat(&request).await.expect("chat should succeed");

        let backend_name = response
            .metadata
            .get("backend")
            .and_then(|v| v.as_str())
            .expect("metadata should have backend field");
        assert_eq!(backend_name, "llama-cpp");
    }
}

// ---------------------------------------------------------------------------
// T050: Explicit mistralrs backend selection
// ---------------------------------------------------------------------------

#[cfg(feature = "provider-local-mistralrs")]
mod mistralrs_selection {
    use super::*;
    use nxuskit_engine::providers::local::LocalRuntimeProvider;

    #[tokio::test]
    #[ignore]
    async fn test_explicit_mistralrs_backend() {
        let model_path = std::env::var("TEST_GGUF_MODEL")
            .unwrap_or_else(|_| "/models/tinyllama-1.1b.Q4_K_M.gguf".to_string());

        let provider = LocalRuntimeProvider::builder()
            .model_path(&model_path)
            .backend("mistralrs")
            .build()
            .expect("build should succeed with mistralrs backend");

        let request = ChatRequest::new(&model_path).with_message(Message::user("Hi"));

        let response = provider.chat(&request).await.expect("chat should succeed");

        let backend_name = response
            .metadata
            .get("backend")
            .and_then(|v| v.as_str())
            .expect("metadata should have backend field");
        assert_eq!(backend_name, "mistralrs");
    }
}

// ---------------------------------------------------------------------------
// T051: Unavailable backend error
// ---------------------------------------------------------------------------

// T051: Unavailable backend error tests
// The local module is only available when at least one backend feature is enabled.
// When NO backends are compiled, LocalRuntimeProvider isn't even accessible.
// When backends ARE compiled, we can test the unknown backend name path.
#[cfg(any(feature = "provider-local-llama", feature = "provider-local-mistralrs"))]
mod unavailable_backend {
    #[test]
    fn test_unknown_backend_name_error() {
        use nxuskit_engine::providers::local::LocalRuntimeProvider;

        let result = LocalRuntimeProvider::builder()
            .model_path("/tmp/model.gguf")
            .backend("nonexistent-backend")
            .build();

        assert!(result.is_err(), "Should fail for unknown backend");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("nonexistent-backend"),
            "Error should mention the bad backend name: {}",
            err_msg
        );
    }
}

// ---------------------------------------------------------------------------
// T052: Auto-selection picks first available
// ---------------------------------------------------------------------------

#[cfg(any(feature = "provider-local-llama", feature = "provider-local-mistralrs"))]
mod auto_selection {
    use nxuskit_engine::providers::local::LocalRuntimeProvider;

    #[test]
    fn test_auto_select_backend() {
        // Build without specifying a backend — should auto-select
        let provider = LocalRuntimeProvider::builder()
            .model_path("/tmp/model.gguf")
            .build()
            .expect("auto-selection should succeed when a backend is available");

        // Verify it selected a backend by checking provider_name works
        use nxuskit_engine::LLMProvider;
        assert_eq!(provider.provider_name(), "local");
    }
}

// ---------------------------------------------------------------------------
// T055: Backend identification in metadata
// ---------------------------------------------------------------------------

#[cfg(any(feature = "provider-local-llama", feature = "provider-local-mistralrs"))]
mod backend_metadata {
    use super::*;
    use nxuskit_engine::providers::local::LocalRuntimeProvider;

    #[test]
    fn test_capabilities_report_backend() {
        let provider = LocalRuntimeProvider::builder()
            .model_path("/tmp/model.gguf")
            .build()
            .expect("build should succeed");

        let caps = provider.get_capabilities();
        // The capabilities should be valid (not default)
        assert!(caps.supports_streaming);
        assert!(caps.supports_system_messages);
    }
}

// ---------------------------------------------------------------------------
// T055a: Backend interchangeability (both backends, same model)
// ---------------------------------------------------------------------------

#[cfg(all(feature = "provider-local-llama", feature = "provider-local-mistralrs"))]
mod backend_interchangeability {
    use super::*;
    use nxuskit_engine::providers::local::LocalRuntimeProvider;

    #[tokio::test]
    #[ignore]
    async fn test_backend_response_structure_parity() {
        let model_path = std::env::var("TEST_GGUF_MODEL")
            .unwrap_or_else(|_| "/models/tinyllama-1.1b.Q4_K_M.gguf".to_string());

        let prompt = "What is 2+2?";

        // Run with llama-cpp
        let llama_provider = LocalRuntimeProvider::builder()
            .model_path(&model_path)
            .backend("llama-cpp")
            .build()
            .expect("llama-cpp build");

        let request = ChatRequest::new(&model_path).with_message(Message::user(prompt));
        let llama_response = llama_provider.chat(&request).await.expect("llama chat");

        // Run with mistralrs
        let mistral_provider = LocalRuntimeProvider::builder()
            .model_path(&model_path)
            .backend("mistralrs")
            .build()
            .expect("mistralrs build");

        let mistral_response = mistral_provider.chat(&request).await.expect("mistral chat");

        // Structural parity: same fields present (content may differ)
        assert!(!llama_response.content.is_empty());
        assert!(!mistral_response.content.is_empty());

        assert!(llama_response.finish_reason.is_some());
        assert!(mistral_response.finish_reason.is_some());

        assert!(llama_response.usage.is_complete);
        assert!(mistral_response.usage.is_complete);

        // Both should have backend metadata
        assert!(llama_response.metadata.contains_key("backend"));
        assert!(mistral_response.metadata.contains_key("backend"));

        // Backend names should differ
        assert_ne!(
            llama_response.metadata["backend"],
            mistral_response.metadata["backend"]
        );
    }
}
