//! Chat provider serialization and dispatch benchmarks.
//!
//! Measures JSON serialization/deserialization overhead for chat types,
//! which represents the dominant cost in the C ABI hot path.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

use nxuskit_engine::types::{ChatRequest, ChatResponse, Message, Role, TokenUsage};

// ── Helpers ─────────────────────────────────────────────────

fn make_chat_request(num_messages: usize) -> ChatRequest {
    let messages: Vec<Message> = (0..num_messages)
        .map(|i| {
            Message::new(
                if i % 2 == 0 {
                    Role::User
                } else {
                    Role::Assistant
                },
                format!("Message content number {} with some realistic length to simulate actual usage patterns in production", i),
            )
        })
        .collect();

    let mut req = ChatRequest::new("gpt-4");
    req.messages = messages;
    req
}

fn make_chat_response() -> ChatResponse {
    use nxuskit_engine::types::TokenCount;
    let usage = TokenUsage::estimated_only(TokenCount::new(150, 50));
    ChatResponse::new(
        "This is a typical response from the model with enough content to be realistic for benchmarking purposes.".to_string(),
        "gpt-4".to_string(),
        usage,
    )
}

// ── Serialization Benchmarks ────────────────────────────────

fn bench_chat_serialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("chat_serialize");

    for (n, label) in &[(1, "1msg"), (5, "5msg"), (20, "20msg")] {
        let request = make_chat_request(*n);
        group.bench_with_input(BenchmarkId::new("request", label), &request, |b, req| {
            b.iter(|| serde_json::to_string(req).unwrap());
        });
    }

    let response = make_chat_response();
    group.bench_function("response", |b| {
        b.iter(|| serde_json::to_string(&response).unwrap());
    });

    group.finish();
}

// ── Deserialization Benchmarks ──────────────────────────────

fn bench_chat_deserialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("chat_deserialize");

    for (n, label) in &[(1, "1msg"), (5, "5msg"), (20, "20msg")] {
        let request = make_chat_request(*n);
        let json = serde_json::to_string(&request).unwrap();
        group.bench_with_input(BenchmarkId::new("request", label), &json, |b, json_str| {
            b.iter(|| serde_json::from_str::<ChatRequest>(json_str).unwrap());
        });
    }

    let response = make_chat_response();
    let json = serde_json::to_string(&response).unwrap();
    group.bench_function("response", |b| {
        b.iter(|| serde_json::from_str::<ChatResponse>(&json).unwrap());
    });

    group.finish();
}

criterion_group!(benches, bench_chat_serialize, bench_chat_deserialize,);
criterion_main!(benches);
