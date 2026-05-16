//! CLIPS Session FBP (Fact-Based Processing) pattern benchmark.
//!
//! Measures end-to-end inference with 500 facts, 20 templates, 10 cycles.
//! Target: <5s per cycle.

use criterion::{Criterion, criterion_group, criterion_main};
use nxuskit_engine::clips_session_manager;

fn setup_fbp_environment() -> u64 {
    let handle = clips_session_manager::session_create().expect("session create");

    clips_session_manager::with_env(handle, |env| {
        // Define 20 templates
        for i in 0..20 {
            let deftemplate = format!(
                "(deftemplate item-{i} (slot id (type INTEGER)) (slot value (type FLOAT)) (slot status (type SYMBOL)))"
            );
            env.eval(&deftemplate).ok();
        }

        // Define processing rules that chain across templates
        for i in 0..10 {
            let src = i % 20;
            let dst = (i + 1) % 20;
            let rule = format!(
                "(defrule process-{i} (item-{src} (id ?id) (value ?v) (status pending)) => \
                 (assert (item-{dst} (id ?id) (value (+ ?v 1.0)) (status pending))))"
            );
            env.eval(&rule).ok();
        }

        // Assert 500 facts spread across templates
        for i in 0..500 {
            let tmpl = i % 20;
            let fact = format!("(assert (item-{tmpl} (id {i}) (value {val}.0) (status pending)))", val = i);
            env.eval(&fact).ok();
        }
    })
    .expect("setup FBP environment");

    handle
}

fn bench_fbp_inference(c: &mut Criterion) {
    let mut group = c.benchmark_group("clips_session_fbp");
    group.sample_size(10);

    group.bench_function("500facts_20templates_10cycles", |b| {
        b.iter_with_setup(
            || {
                // Each iteration gets a fresh session
                setup_fbp_environment()
            },
            |handle| {
                clips_session_manager::with_env(handle, |env| {
                    // Run 10 cycles of inference
                    for _ in 0..10 {
                        env.run(Some(1000)).ok();
                    }
                })
                .expect("run FBP");
                clips_session_manager::session_destroy(handle);
            },
        );
    });

    group.finish();
}

criterion_group!(benches, bench_fbp_inference);
criterion_main!(benches);
