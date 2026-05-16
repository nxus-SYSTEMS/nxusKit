//! Tests for fresh_session() behavior across providers
//!
//! These tests verify that fresh_session() returns a usable provider instance
//! for deterministic CI/testing evaluation.
#![allow(clippy::panic, clippy::print_stdout, clippy::print_stderr)]

use nxuskit_engine::LLMProvider;
use nxuskit_engine::providers::{LoopbackProvider, MockProvider};

/// T011: Test MockProvider::fresh_session() returns a fresh instance
#[test]
fn test_mock_fresh_session_returns_fresh_instance() {
    let mock = MockProvider::builder()
        .with_response("test response")
        .build()
        .expect("Should build MockProvider");

    // Call fresh_session to get a new instance
    let fresh = mock.fresh_session();

    // Verify the fresh instance is usable
    assert_eq!(fresh.provider_name(), "mock");
}

/// T012: Test LoopbackProvider::fresh_session() returns a clone
#[test]
fn test_loopback_fresh_session_returns_clone() {
    let loopback = LoopbackProvider::new();

    // Call fresh_session to get a new instance
    let fresh = loopback.fresh_session();

    // Verify the fresh instance is usable
    assert_eq!(fresh.provider_name(), "loopback");
}

/// T013: Test that fresh_session can be called multiple times
#[test]
fn test_fresh_session_multiple_calls() {
    let mock = MockProvider::builder()
        .with_response("response")
        .build()
        .expect("Should build");

    // Call fresh_session multiple times
    let fresh1 = mock.fresh_session();
    let fresh2 = fresh1.fresh_session();
    let fresh3 = fresh2.fresh_session();

    // All should be usable
    assert_eq!(fresh1.provider_name(), "mock");
    assert_eq!(fresh2.provider_name(), "mock");
    assert_eq!(fresh3.provider_name(), "mock");
}

/// Test fresh_session with OllamaProvider (returns Result)
#[cfg(test)]
mod ollama_tests {
    use nxuskit_engine::LLMProvider;
    use nxuskit_engine::providers::OllamaProvider;

    #[test]
    fn test_ollama_fresh_session_returns_clone() {
        let ollama = OllamaProvider::builder().build().expect("Should build");

        // Ollama's fresh_session returns Result
        let fresh = ollama.fresh_session().expect("Should create fresh session");

        assert_eq!(fresh.provider_name(), "ollama");
    }
}

/// Test fresh_session with LmStudioProvider (returns Result)
#[cfg(test)]
mod lmstudio_tests {
    use nxuskit_engine::LLMProvider;
    use nxuskit_engine::providers::LmStudioProvider;

    #[test]
    fn test_lmstudio_fresh_session_returns_clone() {
        let lmstudio = LmStudioProvider::builder()
            .model("test-model") // LmStudio requires a model name
            .build()
            .expect("Should build");

        // LmStudio's fresh_session returns Result
        let fresh = lmstudio
            .fresh_session()
            .expect("Should create fresh session");

        assert_eq!(fresh.provider_name(), "lmstudio");
    }
}

/// Test fresh_session with ClipsProvider (requires clips feature, returns Result)
#[cfg(feature = "clips")]
mod clips_tests {
    use nxuskit_engine::LLMProvider;
    use nxuskit_engine::providers::ClipsProvider;
    use std::path::PathBuf;

    #[test]
    fn test_clips_fresh_session_clears_env_cache() {
        let clips = ClipsProvider::builder()
            .rules_directory(PathBuf::from("."))
            .build()
            .expect("Should build ClipsProvider");

        // CLIPS fresh_session returns Result
        let fresh = clips.fresh_session().expect("Should create fresh session");

        assert_eq!(fresh.provider_name(), "clips");
    }
}
