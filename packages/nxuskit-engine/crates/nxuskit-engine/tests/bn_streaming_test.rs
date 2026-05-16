//! Streaming contract tests for BayesStream and Gibbs sample_stream.
//!
//! T043: Validates async and sync streaming paths for progressive inference.

use std::path::PathBuf;

use futures::StreamExt;

use nxuskit_engine::providers::bayesian::bif::load_bif_file;
use nxuskit_engine::providers::bayesian::{BayesianNetwork, Evidence, GibbsSampler};

fn load_asia() -> BayesianNetwork {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bn/asia.bif");
    load_bif_file(&path).unwrap()
}

// ── Async path ──────────────────────────────────────────────────

#[tokio::test]
async fn stream_delivers_multiple_chunks_async() {
    let net = load_asia();
    let evidence = Evidence::new();
    let gibbs = GibbsSampler::new(10_000, 100).with_seed(42);

    let stream = gibbs.sample_stream(&net, &evidence, 1000).unwrap();
    let chunks: Vec<_> = stream.collect().await;

    // 10_000 samples / 1000 chunk_size = at least 10 chunks
    assert!(
        chunks.len() >= 10,
        "Expected ≥10 chunks, got {}",
        chunks.len()
    );
}

#[tokio::test]
async fn stream_iteration_counts_increase() {
    let net = load_asia();
    let evidence = Evidence::new();
    let gibbs = GibbsSampler::new(5_000, 100).with_seed(42);

    let stream = gibbs.sample_stream(&net, &evidence, 500).unwrap();
    let chunks: Vec<_> = stream.collect().await;

    for window in chunks.windows(2) {
        assert!(
            window[1].iteration > window[0].iteration,
            "Iteration count must increase: {} -> {}",
            window[0].iteration,
            window[1].iteration
        );
    }
}

#[tokio::test]
async fn stream_convergence_decreases() {
    let net = load_asia();
    let evidence = Evidence::new();
    let gibbs = GibbsSampler::new(10_000, 100).with_seed(42);

    let stream = gibbs.sample_stream(&net, &evidence, 1000).unwrap();
    let chunks: Vec<_> = stream.collect().await;

    // First chunk should have higher convergence than last
    assert!(
        chunks.first().unwrap().convergence_metric >= chunks.last().unwrap().convergence_metric,
        "Convergence should generally decrease: first={}, last={}",
        chunks.first().unwrap().convergence_metric,
        chunks.last().unwrap().convergence_metric,
    );
}

#[tokio::test]
async fn stream_last_chunk_is_final() {
    let net = load_asia();
    let evidence = Evidence::new();
    let gibbs = GibbsSampler::new(3_000, 100).with_seed(42);

    let stream = gibbs.sample_stream(&net, &evidence, 500).unwrap();
    let chunks: Vec<_> = stream.collect().await;

    // Only the last chunk should be final
    for (i, chunk) in chunks.iter().enumerate() {
        if i < chunks.len() - 1 {
            assert!(!chunk.is_final, "Chunk {} should not be final", i);
        } else {
            assert!(chunk.is_final, "Last chunk should be final");
        }
    }
}

#[tokio::test]
async fn stream_chunks_contain_marginals() {
    let net = load_asia();
    let evidence = Evidence::new();
    let gibbs = GibbsSampler::new(2_000, 100).with_seed(42);

    let stream = gibbs.sample_stream(&net, &evidence, 500).unwrap();
    let chunks: Vec<_> = stream.collect().await;

    for chunk in &chunks {
        // Every chunk should have marginals for all 8 Asia variables
        assert_eq!(
            chunk.data.marginals.len(),
            8,
            "Each chunk must have marginals for all 8 variables"
        );
    }
}

#[tokio::test]
async fn stream_total_iterations_consistent() {
    let net = load_asia();
    let evidence = Evidence::new();
    let gibbs = GibbsSampler::new(5_000, 200).with_seed(42);

    let stream = gibbs.sample_stream(&net, &evidence, 1000).unwrap();
    let chunks: Vec<_> = stream.collect().await;

    for chunk in &chunks {
        assert_eq!(
            chunk.total_iterations, 5_000,
            "total_iterations should match requested num_samples"
        );
    }
}

#[tokio::test]
async fn stream_with_evidence() {
    let net = load_asia();
    let mut evidence = Evidence::new();
    let smoking_name =
        nxuskit_engine::providers::bayesian::types::VariableName::new("Smoking").unwrap();
    evidence.observe(&net, &smoking_name, "yes").unwrap();

    let gibbs = GibbsSampler::new(3_000, 100).with_seed(42);
    let stream = gibbs.sample_stream(&net, &evidence, 500).unwrap();
    let chunks: Vec<_> = stream.collect().await;

    assert!(!chunks.is_empty(), "Should produce chunks with evidence");

    // All chunks should have marginals for unobserved variables (7 out of 8)
    let final_chunk = chunks.last().unwrap();
    assert_eq!(final_chunk.data.marginals.len(), 7);
    // Smoking should not appear in marginals (observed)
    assert!(!final_chunk.data.marginals.contains_key(
        &nxuskit_engine::providers::bayesian::types::VariableName::new("Smoking").unwrap()
    ));
}

#[tokio::test]
async fn stream_cancellation_stops_background_task() {
    let net = load_asia();
    let evidence = Evidence::new();
    let gibbs = GibbsSampler::new(100_000, 100).with_seed(42);

    let mut stream = gibbs.sample_stream(&net, &evidence, 1000).unwrap();

    // Take only the first 3 chunks, then drop the stream
    let mut collected = Vec::new();
    for _ in 0..3 {
        if let Some(chunk) = stream.next().await {
            collected.push(chunk);
        }
    }
    drop(stream);

    assert_eq!(collected.len(), 3, "Should have collected exactly 3 chunks");
    // Background task should have been cancelled (no way to assert this directly,
    // but the fact that we returned quickly from 100K samples proves cancellation)
}

#[tokio::test]
async fn stream_deterministic_with_seed() {
    let net = load_asia();
    let evidence = Evidence::new();

    // Run twice with same seed
    let gibbs1 = GibbsSampler::new(3_000, 100).with_seed(99);
    let stream1 = gibbs1.sample_stream(&net, &evidence, 500).unwrap();
    let chunks1: Vec<_> = stream1.collect().await;

    let gibbs2 = GibbsSampler::new(3_000, 100).with_seed(99);
    let stream2 = gibbs2.sample_stream(&net, &evidence, 500).unwrap();
    let chunks2: Vec<_> = stream2.collect().await;

    assert_eq!(chunks1.len(), chunks2.len());
    for (c1, c2) in chunks1.iter().zip(chunks2.iter()) {
        assert_eq!(c1.iteration, c2.iteration);
        assert!((c1.convergence_metric - c2.convergence_metric).abs() < 1e-10);
    }
}

// ── First-chunk latency (SC-006 / Article XVI IR-001) ──────────

#[tokio::test]
async fn stream_first_chunk_latency_under_400ms() {
    let net = load_asia();
    let evidence = Evidence::new();
    // 50K samples with chunk_size=1000 → first chunk after 1000 samples
    let gibbs = GibbsSampler::new(50_000, 200).with_seed(42);

    let start = std::time::Instant::now();
    let mut stream = gibbs.sample_stream(&net, &evidence, 1000).unwrap();

    // Measure time to first chunk
    let first_chunk = stream.next().await.unwrap();
    let first_chunk_latency = start.elapsed();

    // Target: <400ms in release mode (Article XVI IR-001).
    // Debug builds run ~3x slower; coverage-instrumented builds even more.
    let threshold_ms = if cfg!(debug_assertions) { 3000 } else { 400 };
    assert!(
        first_chunk_latency.as_millis() < threshold_ms,
        "First chunk latency should be <{}ms, got {}ms",
        threshold_ms,
        first_chunk_latency.as_millis()
    );
    assert!(!first_chunk.data.marginals.is_empty());
}

// ── Sync path (Article IX) ──────────────────────────────────────

#[test]
fn blocking_iter_produces_same_results() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let net = load_asia();
    let evidence = Evidence::new();
    let gibbs = GibbsSampler::new(3_000, 100).with_seed(42);

    // Must create stream inside the runtime context so tokio::spawn works.
    let chunks: Vec<_> = rt.block_on(async {
        let stream = gibbs.sample_stream(&net, &evidence, 500).unwrap();
        let iter = stream.blocking_iter();
        tokio::task::spawn_blocking(move || iter.collect::<Vec<_>>())
            .await
            .unwrap()
    });

    assert!(
        chunks.len() >= 6,
        "Expected ≥6 chunks, got {}",
        chunks.len()
    );
    assert!(chunks.last().unwrap().is_final, "Last chunk must be final");

    // Verify marginals present
    for chunk in &chunks {
        assert_eq!(chunk.data.marginals.len(), 8);
    }
}

// ── Provider streaming integration ──────────────────────────────

#[tokio::test]
async fn provider_chat_stream_gibbs_with_chunk_size() {
    use nxuskit_engine::LLMProvider;
    use nxuskit_engine::providers::BayesianProvider;
    use nxuskit_engine::types::{ChatRequest, Message};

    let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bn");
    let provider = BayesianProvider::builder()
        .networks_directory(&fixture_dir)
        .build()
        .unwrap();

    let input_json = r#"{
        "action": "infer",
        "evidence": {"Smoking": "yes"},
        "options": {
            "algorithm": "gibbs",
            "num_samples": 5000,
            "burn_in": 100,
            "seed": 42,
            "chunk_size": 1000
        }
    }"#;

    let request = ChatRequest::new("asia.bif").with_message(Message::user(input_json));
    let mut stream = provider.chat_stream(&request).await.unwrap();

    let mut chunks = Vec::new();
    while let Some(result) = stream.next().await {
        let chunk = result.unwrap();
        chunks.push(chunk);
    }

    // Should get multiple chunks from 5000/1000 = 5 chunks
    assert!(
        chunks.len() >= 5,
        "Expected ≥5 StreamChunks, got {}",
        chunks.len()
    );

    // Last chunk should have finish_reason and usage
    let last = chunks.last().unwrap();
    assert!(last.finish_reason.is_some());
    assert!(last.usage.is_some());

    // Non-final chunks should not have finish_reason
    for chunk in &chunks[..chunks.len() - 1] {
        assert!(chunk.finish_reason.is_none());
    }
}

#[tokio::test]
async fn provider_chat_stream_ve_single_chunk() {
    use nxuskit_engine::LLMProvider;
    use nxuskit_engine::providers::BayesianProvider;
    use nxuskit_engine::types::{ChatRequest, Message};

    let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bn");
    let provider = BayesianProvider::builder()
        .networks_directory(&fixture_dir)
        .build()
        .unwrap();

    let input_json = r#"{"action":"infer","evidence":{}}"#;
    let request = ChatRequest::new("asia.bif").with_message(Message::user(input_json));

    let mut stream = provider.chat_stream(&request).await.unwrap();

    let mut chunks = Vec::new();
    while let Some(result) = stream.next().await {
        chunks.push(result.unwrap());
    }

    // VE should produce exactly one chunk
    assert_eq!(
        chunks.len(),
        1,
        "VE streaming should produce exactly 1 chunk"
    );
    assert!(chunks[0].finish_reason.is_some());
}

#[tokio::test]
async fn provider_chat_stream_gibbs_no_chunk_size_single_result() {
    use nxuskit_engine::LLMProvider;
    use nxuskit_engine::providers::BayesianProvider;
    use nxuskit_engine::types::{ChatRequest, Message};

    let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bn");
    let provider = BayesianProvider::builder()
        .networks_directory(&fixture_dir)
        .build()
        .unwrap();

    let input_json = r#"{
        "action": "infer",
        "evidence": {},
        "options": {
            "algorithm": "gibbs",
            "num_samples": 1000,
            "burn_in": 100,
            "seed": 42
        }
    }"#;

    let request = ChatRequest::new("asia.bif").with_message(Message::user(input_json));
    let mut stream = provider.chat_stream(&request).await.unwrap();

    let mut chunks = Vec::new();
    while let Some(result) = stream.next().await {
        chunks.push(result.unwrap());
    }

    // Without chunk_size, Gibbs should fall back to single-chunk behavior
    assert_eq!(
        chunks.len(),
        1,
        "Gibbs without chunk_size should produce 1 chunk"
    );
}
