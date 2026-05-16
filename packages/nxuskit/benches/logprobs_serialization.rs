//! v0.9.3 logprobs serialization overhead smoke benchmark (T105).
//!
//! This is intentionally an executable bench target rather than a Criterion
//! benchmark so the user-facing wrapper crate does not gain a new dev
//! dependency. Run with:
//!
//!   cargo bench --manifest-path packages/nxuskit/Cargo.toml --bench logprobs_serialization

use std::collections::HashMap;
use std::hint::black_box;
use std::time::{Duration, Instant};

use nxuskit::{
    ChatRequest, ChatResponse, FinishReason, LogprobsData, Message, TokenCount, TokenLogprob,
    TokenUsage, TopLogprob,
};

const ITERATIONS: u32 = 10_000;
const MAX_OVERHEAD_NS: u128 = 1_000_000; // 1ms per operation

fn request_without_logprobs() -> ChatRequest {
    ChatRequest::new("gpt-5.4")
        .with_message(Message::system("You are a concise release validation assistant."))
        .with_message(Message::user(
            "Classify this SDK release artifact and summarize the risk.",
        ))
        .with_temperature(0.2)
        .with_max_tokens(128)
}

fn request_with_logprobs() -> ChatRequest {
    request_without_logprobs()
        .with_logprobs(true)
        .with_top_logprobs(5)
}

fn response_without_logprobs() -> ChatResponse {
    ChatResponse {
        content: "The release artifact is valid with low residual risk.".to_string(),
        model: "gpt-5.4".to_string(),
        provider: "loopback".to_string(),
        usage: TokenUsage {
            estimated: TokenCount {
                prompt_tokens: 42,
                completion_tokens: 11,
            },
            actual: Some(TokenCount {
                prompt_tokens: 40,
                completion_tokens: 10,
            }),
        },
        finish_reason: Some(FinishReason::Stop),
        metadata: HashMap::new(),
        warnings: Vec::new(),
        logprobs: None,
        tool_calls: None,
        inference_metadata: None,
    }
}

fn response_with_logprobs() -> ChatResponse {
    let mut response = response_without_logprobs();
    response.logprobs = Some(LogprobsData {
        content: (0..12)
            .map(|idx| TokenLogprob {
                token: format!("tok{idx}"),
                logprob: -0.05 - (idx as f32 * 0.01),
                bytes: Some(format!("tok{idx}").into_bytes()),
                top_logprobs: (0..5)
                    .map(|alt| TopLogprob {
                        token: format!("alt{idx}_{alt}"),
                        logprob: -0.5 - (alt as f32 * 0.25),
                        bytes: Some(format!("alt{idx}_{alt}").into_bytes()),
                    })
                    .collect(),
            })
            .collect(),
    });
    response
}

fn measure(mut op: impl FnMut()) -> Duration {
    let start = Instant::now();
    for _ in 0..ITERATIONS {
        op();
    }
    start.elapsed()
}

fn avg_nanos(total: Duration) -> u128 {
    total.as_nanos() / u128::from(ITERATIONS)
}

fn assert_overhead(label: &str, baseline_ns: u128, logprobs_ns: u128) {
    let overhead = logprobs_ns.saturating_sub(baseline_ns);
    println!(
        "{label}: baseline_avg_ns={baseline_ns} logprobs_avg_ns={logprobs_ns} overhead_ns={overhead}"
    );
    assert!(
        overhead <= MAX_OVERHEAD_NS,
        "{label} overhead must stay <=1ms; observed {overhead}ns"
    );
}

fn main() {
    let request_plain = request_without_logprobs();
    let request_logprobs = request_with_logprobs();
    let response_plain_json = serde_json::to_string(&response_without_logprobs()).unwrap();
    let response_logprobs_json = serde_json::to_string(&response_with_logprobs()).unwrap();

    // Warm up serde code paths before timing.
    black_box(serde_json::to_string(&request_plain).unwrap());
    black_box(serde_json::to_string(&request_logprobs).unwrap());
    black_box(serde_json::from_str::<ChatResponse>(&response_plain_json).unwrap());
    black_box(serde_json::from_str::<ChatResponse>(&response_logprobs_json).unwrap());

    let serialize_plain = avg_nanos(measure(|| {
        black_box(serde_json::to_string(black_box(&request_plain)).unwrap());
    }));
    let serialize_logprobs = avg_nanos(measure(|| {
        black_box(serde_json::to_string(black_box(&request_logprobs)).unwrap());
    }));

    let deserialize_plain = avg_nanos(measure(|| {
        black_box(serde_json::from_str::<ChatResponse>(black_box(
            &response_plain_json,
        ))
        .unwrap());
    }));
    let deserialize_logprobs = avg_nanos(measure(|| {
        black_box(serde_json::from_str::<ChatResponse>(black_box(
            &response_logprobs_json,
        ))
        .unwrap());
    }));

    assert_overhead(
        "chat_request_serialize",
        serialize_plain,
        serialize_logprobs,
    );
    assert_overhead(
        "chat_response_deserialize",
        deserialize_plain,
        deserialize_logprobs,
    );
}
