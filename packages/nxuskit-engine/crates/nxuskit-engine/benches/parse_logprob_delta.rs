//! Microbenchmark for streaming logprob delta parse overhead (T064).
//!
//! Measures the per-chunk JSON-decode cost of OpenAI-shaped
//! `choices[0].logprobs` payload into `StreamLogprobsDelta`, comparing
//! the populated case against the absent case (parser returns `None`).
//!
//! Target (per Article XVI PR-009): ≤ 50 µs additional when populated;
//! 0 added overhead when `None`.
//!
//! Benchmark host (record manually in PR description):
//! - Host: M3 Max baseline
//! - macOS / kernel: capture via `uname -a`
//! - CPU: `sysctl -n machdep.cpu.brand_string`

use criterion::{Criterion, criterion_group, criterion_main};
use serde_json::Value;

use nxuskit_engine::types::StreamLogprobsDelta;

/// Decode the OpenAI-shape `choices[0].logprobs` field. Returns `None`
/// when the field is null/absent or the payload is malformed (FR-007).
fn decode_logprobs_field(payload: &Value) -> Option<StreamLogprobsDelta> {
    let choice = payload.get("choices")?.as_array()?.first()?;
    let lp = choice.get("logprobs").cloned().unwrap_or(Value::Null);
    if lp.is_null() {
        return None;
    }
    serde_json::from_value(lp).ok()
}

fn bench_parse_logprob_delta(c: &mut Criterion) {
    let with_logprobs: Value = serde_json::from_str(
        r#"{
            "choices":[{
                "index":0,
                "delta":{"content":"Hello"},
                "logprobs":{
                    "content":[
                        {"token":"Hello","logprob":-0.0073,"bytes":[72,101,108,108,111],
                         "top_logprobs":[
                            {"token":"Hi","logprob":-2.1,"bytes":[72,105]},
                            {"token":"Hey","logprob":-3.4,"bytes":[72,101,121]}
                         ]},
                        {"token":" world","logprob":-0.12,"bytes":[32,119,111,114,108,100],
                         "top_logprobs":[]}
                    ]
                },
                "finish_reason":null
            }]
        }"#,
    )
    .unwrap();

    let without_logprobs: Value = serde_json::from_str(
        r#"{
            "choices":[{
                "index":0,
                "delta":{"content":"Hello"},
                "logprobs":null,
                "finish_reason":null
            }]
        }"#,
    )
    .unwrap();

    let mut group = c.benchmark_group("parse_logprob_delta");

    group.bench_function("populated", |b| {
        b.iter(|| {
            let out = decode_logprobs_field(std::hint::black_box(&with_logprobs));
            std::hint::black_box(out);
        });
    });

    group.bench_function("absent", |b| {
        b.iter(|| {
            let out = decode_logprobs_field(std::hint::black_box(&without_logprobs));
            std::hint::black_box(out);
        });
    });

    group.finish();
}

criterion_group!(benches, bench_parse_logprob_delta);
criterion_main!(benches);
