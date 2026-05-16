//! FFI overhead benchmarks measuring serde_json and CString costs.
//!
//! Targets SC-007 (≤1ms per-operation FFI overhead) and SC-008 (≤200ms composite).
//! Measures the pure serialization/deserialization cost that dominates C ABI calls.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::ffi::CString;

// ── Helpers ─────────────────────────────────────────────────

fn make_json_payload(size_kb: usize) -> String {
    // Build a realistic JSON object with nested structures
    let item =
        r#"{"name":"variable_x","var_type":"integer","domain":{"min":0,"max":100},"label":"test"}"#;
    let item_len = item.len();
    let count = (size_kb * 1024) / (item_len + 1); // +1 for comma
    let items: Vec<&str> = (0..count).map(|_| item).collect();
    format!("[{}]", items.join(","))
}

// ── CString Round-Trip ──────────────────────────────────────

fn bench_cstring_roundtrip(c: &mut Criterion) {
    let mut group = c.benchmark_group("ffi_cstring");

    for (size_kb, label) in &[(1, "1KB"), (10, "10KB"), (100, "100KB")] {
        let payload = make_json_payload(*size_kb);
        group.bench_with_input(
            BenchmarkId::new("new_to_str", label),
            &payload,
            |b, payload| {
                b.iter(|| {
                    let cs = CString::new(payload.as_bytes()).unwrap();
                    let _back = cs.to_str().unwrap();
                });
            },
        );
    }

    group.finish();
}

// ── serde_json Serialize ────────────────────────────────────

fn bench_serde_serialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("ffi_serde_serialize");

    for (size_kb, label) in &[(1, "1KB"), (10, "10KB"), (100, "100KB")] {
        let payload = make_json_payload(*size_kb);
        let value: serde_json::Value = serde_json::from_str(&payload).unwrap();
        group.bench_with_input(BenchmarkId::new("to_string", label), &value, |b, value| {
            b.iter(|| serde_json::to_string(value).unwrap());
        });
    }

    group.finish();
}

// ── serde_json Deserialize ──────────────────────────────────

fn bench_serde_deserialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("ffi_serde_deserialize");

    for (size_kb, label) in &[(1, "1KB"), (10, "10KB"), (100, "100KB")] {
        let payload = make_json_payload(*size_kb);
        group.bench_with_input(
            BenchmarkId::new("from_str", label),
            &payload,
            |b, payload| {
                b.iter(|| {
                    let _v: serde_json::Value = serde_json::from_str(payload).unwrap();
                });
            },
        );
    }

    group.finish();
}

// ── Combined Round-Trip (simulating C ABI call) ─────────────

fn bench_ffi_roundtrip(c: &mut Criterion) {
    let mut group = c.benchmark_group("ffi_roundtrip");

    for (size_kb, label) in &[(1, "1KB"), (10, "10KB"), (100, "100KB")] {
        let payload = make_json_payload(*size_kb);
        group.bench_with_input(
            BenchmarkId::new("cstring_deserialize_serialize", label),
            &payload,
            |b, payload| {
                b.iter(|| {
                    // Simulate inbound: CString → parse JSON
                    let cs = CString::new(payload.as_bytes()).unwrap();
                    let json_str = cs.to_str().unwrap();
                    let value: serde_json::Value = serde_json::from_str(json_str).unwrap();

                    // Simulate outbound: serialize → CString
                    let output = serde_json::to_string(&value).unwrap();
                    let _cs_out = CString::new(output).unwrap();
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_cstring_roundtrip,
    bench_serde_serialize,
    bench_serde_deserialize,
    bench_ffi_roundtrip,
);
criterion_main!(benches);
