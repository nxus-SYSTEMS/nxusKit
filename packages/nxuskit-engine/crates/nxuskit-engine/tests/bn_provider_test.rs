//! LLMProvider contract tests for BayesianProvider.
//!
//! These integration tests exercise the BayesianProvider through the
//! LLMProvider trait interface, verifying chat(), list_models(),
//! provider_name(), get_capabilities(), and TokenUsage mapping.

use std::path::PathBuf;

use nxuskit_engine::LLMProvider;
use nxuskit_engine::providers::BayesianProvider;
use nxuskit_engine::providers::bayesian::BnOutput;
use nxuskit_engine::types::{ChatRequest, FinishReason, Message};

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bn")
}

fn build_provider() -> BayesianProvider {
    BayesianProvider::builder()
        .networks_directory(fixture_dir())
        .build()
        .unwrap()
}

// ============================================================================
// provider_name() contract
// ============================================================================

#[tokio::test]
async fn provider_name_returns_bn() {
    let provider = build_provider();
    assert_eq!(provider.provider_name(), "bn");
}

// ============================================================================
// get_capabilities() contract
// ============================================================================

#[tokio::test]
async fn capabilities_json_mode_enabled() {
    let provider = build_provider();
    let caps = provider.get_capabilities();
    assert!(caps.supports_json_mode);
    assert!(!caps.supports_system_messages);
    assert!(caps.supports_streaming);
    assert!(!caps.supports_vision);
}

// ============================================================================
// chat() — inference via VE (default algorithm)
// ============================================================================

#[tokio::test]
async fn chat_asia_ve_no_evidence() {
    let provider = build_provider();
    let input_json = r#"{"action":"infer","evidence":{}}"#;
    let request = ChatRequest::new("asia.bif").with_message(Message::user(input_json));

    let response = provider.chat(&request).await.unwrap();
    let output: BnOutput = serde_json::from_str(&response.content).unwrap();

    assert_eq!(output.algorithm, "ve");
    assert_eq!(output.network_size, 8);
    assert_eq!(output.evidence_count, 0);
    assert_eq!(output.marginals.len(), 8);

    // Smoking prior is 0.5/0.5 in Asia network
    let smoking = output.marginals.get("Smoking").unwrap();
    assert!((smoking.get("yes").unwrap() - 0.5).abs() < 1e-6);
    assert!((smoking.get("no").unwrap() - 0.5).abs() < 1e-6);
}

#[tokio::test]
async fn chat_asia_ve_with_evidence() {
    let provider = build_provider();
    let input_json = r#"{"action":"infer","evidence":{"Smoking":"yes"}}"#;
    let request = ChatRequest::new("asia.bif").with_message(Message::user(input_json));

    let response = provider.chat(&request).await.unwrap();
    let output: BnOutput = serde_json::from_str(&response.content).unwrap();

    assert_eq!(output.evidence_count, 1);
    // Observed variable excluded from marginals
    assert!(!output.marginals.contains_key("Smoking"));
    // Remaining 7 variables present
    assert_eq!(output.marginals.len(), 7);

    // With Smoking=yes, Bronchitis should have higher probability of "present"
    let bronchitis = output.marginals.get("Bronchitis").unwrap();
    assert!(bronchitis.get("present").unwrap() > &0.5);
}

#[tokio::test]
async fn chat_cancer_network() {
    let provider = build_provider();
    let input_json = r#"{"action":"infer","evidence":{}}"#;
    let request = ChatRequest::new("cancer.bif").with_message(Message::user(input_json));

    let response = provider.chat(&request).await.unwrap();
    let output: BnOutput = serde_json::from_str(&response.content).unwrap();
    assert_eq!(output.network_size, 5);
    assert_eq!(output.marginals.len(), 5);
}

// ============================================================================
// chat() — inference via Junction Tree
// ============================================================================

#[tokio::test]
async fn chat_asia_jt() {
    let provider = build_provider();
    let input_json = r#"{"action":"infer","evidence":{},"options":{"algorithm":"jt"}}"#;
    let request = ChatRequest::new("asia.bif").with_message(Message::user(input_json));

    let response = provider.chat(&request).await.unwrap();
    let output: BnOutput = serde_json::from_str(&response.content).unwrap();
    assert_eq!(output.algorithm, "jt");
    assert_eq!(output.marginals.len(), 8);
}

// ============================================================================
// chat() — inference via Gibbs sampling
// ============================================================================

#[tokio::test]
async fn chat_asia_gibbs_with_diagnostics() {
    let provider = build_provider();
    let input_json = r#"{"action":"infer","evidence":{},"options":{"algorithm":"gibbs","num_samples":2000,"burn_in":200,"seed":42}}"#;
    let request = ChatRequest::new("asia.bif").with_message(Message::user(input_json));

    let response = provider.chat(&request).await.unwrap();
    let output: BnOutput = serde_json::from_str(&response.content).unwrap();

    assert_eq!(output.algorithm, "gibbs");
    assert_eq!(output.marginals.len(), 8);

    // Gibbs should provide diagnostics
    let diag = output
        .diagnostics
        .as_ref()
        .expect("Gibbs should have diagnostics");
    assert!(diag.iterations.is_some());
}

#[tokio::test]
async fn chat_gibbs_deterministic_with_seed() {
    let provider = build_provider();
    let input_json = r#"{"action":"infer","evidence":{},"options":{"algorithm":"gibbs","num_samples":5000,"burn_in":500,"seed":42}}"#;

    let request1 = ChatRequest::new("asia.bif").with_message(Message::user(input_json));
    let response1 = provider.chat(&request1).await.unwrap();
    let output1: BnOutput = serde_json::from_str(&response1.content).unwrap();

    let request2 = ChatRequest::new("asia.bif").with_message(Message::user(input_json));
    let response2 = provider.chat(&request2).await.unwrap();
    let output2: BnOutput = serde_json::from_str(&response2.content).unwrap();

    // Same seed → same marginals
    for (var, dist1) in &output1.marginals {
        let dist2 = output2.marginals.get(var).unwrap();
        for (state, p1) in dist1 {
            let p2 = dist2.get(state).unwrap();
            assert!(
                (p1 - p2).abs() < 1e-10,
                "Gibbs with same seed should be deterministic: {}[{}]: {} vs {}",
                var,
                state,
                p1,
                p2
            );
        }
    }
}

// ============================================================================
// VE vs JT cross-validation
// ============================================================================

#[tokio::test]
async fn ve_jt_cross_validation_asia() {
    let provider = build_provider();
    let evidence =
        r#"{"action":"infer","evidence":{"Smoking":"yes"},"options":{"algorithm":"ve"}}"#;
    let req_ve = ChatRequest::new("asia.bif").with_message(Message::user(evidence));
    let resp_ve = provider.chat(&req_ve).await.unwrap();
    let out_ve: BnOutput = serde_json::from_str(&resp_ve.content).unwrap();

    let evidence_jt =
        r#"{"action":"infer","evidence":{"Smoking":"yes"},"options":{"algorithm":"jt"}}"#;
    let req_jt = ChatRequest::new("asia.bif").with_message(Message::user(evidence_jt));
    let resp_jt = provider.chat(&req_jt).await.unwrap();
    let out_jt: BnOutput = serde_json::from_str(&resp_jt.content).unwrap();

    // VE and JT should agree within floating-point precision
    for (var, dist_ve) in &out_ve.marginals {
        let dist_jt = out_jt.marginals.get(var).unwrap();
        for (state, p_ve) in dist_ve {
            let p_jt = dist_jt.get(state).unwrap();
            assert!(
                (p_ve - p_jt).abs() < 1e-6,
                "VE vs JT mismatch: {}[{}]: {} vs {}",
                var,
                state,
                p_ve,
                p_jt
            );
        }
    }
}

// ============================================================================
// list_models() contract
// ============================================================================

#[tokio::test]
async fn list_models_finds_bif_files() {
    let provider = build_provider();
    let models = provider.list_models().await.unwrap();

    // Fixture dir has asia.bif, cancer.bif, earthquake.bif, survey.bif, alarm.bif
    assert!(
        models.len() >= 4,
        "Expected at least 4 models, got {}",
        models.len()
    );

    let names: Vec<&str> = models.iter().map(|m| m.name.as_str()).collect();
    assert!(names.contains(&"asia"), "Should find asia model");
    assert!(names.contains(&"cancer"), "Should find cancer model");
}

#[tokio::test]
async fn list_models_empty_directory() {
    let provider = BayesianProvider::builder()
        .networks_directory("/tmp/nonexistent-bn-dir-12345")
        .build()
        .unwrap();
    let models = provider.list_models().await.unwrap();
    assert!(models.is_empty());
}

// ============================================================================
// TokenUsage mapping
// ============================================================================

#[tokio::test]
async fn token_usage_reflects_evidence_and_marginals() {
    let provider = build_provider();
    let input_json = r#"{"action":"infer","evidence":{"Smoking":"yes"}}"#;
    let request = ChatRequest::new("asia.bif").with_message(Message::user(input_json));

    let response = provider.chat(&request).await.unwrap();

    // TokenUsage: prompt = evidence count, completion = marginal count
    let estimated = &response.usage.estimated;
    assert_eq!(estimated.prompt_tokens, 1, "prompt_tokens = evidence count");
    assert_eq!(
        estimated.completion_tokens, 7,
        "completion_tokens = marginal count (8 vars - 1 observed)"
    );
}

// ============================================================================
// Error handling
// ============================================================================

#[tokio::test]
async fn chat_invalid_model_returns_error() {
    let provider = build_provider();
    let input_json = r#"{"action":"infer","evidence":{}}"#;
    let request = ChatRequest::new("nonexistent.bif").with_message(Message::user(input_json));

    let result = provider.chat(&request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn chat_invalid_json_returns_error() {
    let provider = build_provider();
    let request = ChatRequest::new("asia.bif").with_message(Message::user("not-json"));

    let result = provider.chat(&request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn chat_no_messages_returns_error() {
    let provider = build_provider();
    let request = ChatRequest::new("asia.bif");

    let result = provider.chat(&request).await;
    assert!(result.is_err());
}

// ============================================================================
// Default algorithm override
// ============================================================================

#[tokio::test]
async fn builder_default_algorithm_override() {
    let provider = BayesianProvider::builder()
        .networks_directory(fixture_dir())
        .default_algorithm("jt")
        .build()
        .unwrap();

    let input_json = r#"{"action":"infer","evidence":{}}"#;
    let request = ChatRequest::new("asia.bif").with_message(Message::user(input_json));

    let response = provider.chat(&request).await.unwrap();
    let output: BnOutput = serde_json::from_str(&response.content).unwrap();
    assert_eq!(output.algorithm, "jt");
}

// ============================================================================
// Alarm network (37 nodes) — larger network test
// ============================================================================

#[tokio::test]
async fn chat_alarm_37_nodes() {
    let provider = build_provider();
    let input_json = r#"{"action":"infer","evidence":{}}"#;
    let request = ChatRequest::new("alarm.bif").with_message(Message::user(input_json));

    let response = provider.chat(&request).await.unwrap();
    let output: BnOutput = serde_json::from_str(&response.content).unwrap();
    assert_eq!(output.network_size, 37);
    assert_eq!(output.marginals.len(), 37);
}

// ============================================================================
// finish_reason contract
// ============================================================================

#[tokio::test]
async fn chat_response_has_stop_finish_reason() {
    let provider = build_provider();
    let input_json = r#"{"action":"infer","evidence":{}}"#;
    let request = ChatRequest::new("asia.bif").with_message(Message::user(input_json));

    let response = provider.chat(&request).await.unwrap();
    assert_eq!(
        response.finish_reason,
        Some(FinishReason::Stop),
        "finish_reason should be Stop"
    );
}
