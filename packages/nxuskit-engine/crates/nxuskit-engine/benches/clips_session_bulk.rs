//! CLIPS Session bulk assertion benchmark.
//!
//! Measures per-call FFI overhead for fact_assert_string operations.
//! Target: <=1ms per call.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use nxuskit_engine::clips_session_manager;

fn bench_bulk_assert(c: &mut Criterion) {
    let mut group = c.benchmark_group("clips_session_bulk_assert");
    group.sample_size(10);

    for &count in &[100, 500, 1000] {
        group.bench_with_input(
            BenchmarkId::new("assert_string", count),
            &count,
            |b, &count| {
                b.iter_with_setup(
                    || {
                        let handle = clips_session_manager::session_create()
                            .expect("session create");
                        // Define a template for assertions
                        clips_session_manager::with_env(handle, |env| {
                            env.eval("(deftemplate bench-fact (slot id (type INTEGER)) (slot data (type STRING)))")
                                .ok();
                        })
                        .expect("define template");
                        handle
                    },
                    |handle| {
                        clips_session_manager::with_env(handle, |env| {
                            for i in 0..count {
                                let fact_str = format!(
                                    "(bench-fact (id {i}) (data \"payload-{i}\"))"
                                );
                                env.assert_string(&fact_str).ok();
                            }
                        })
                        .expect("bulk assert");
                        clips_session_manager::session_destroy(handle);
                    },
                );
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_bulk_assert);
criterion_main!(benches);
