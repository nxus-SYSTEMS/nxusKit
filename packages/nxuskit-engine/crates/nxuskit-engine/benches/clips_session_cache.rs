//! CLIPS Session cache benchmark.
//!
//! Measures session creation/lookup/destroy lifecycle costs, including
//! environment creation with a 20-template rulebase.

use clips_sys::ClipsEnvironment;
use criterion::{Criterion, criterion_group, criterion_main};
use nxuskit_engine::clips_session_manager;

fn create_loaded_env() -> ClipsEnvironment {
    let env = ClipsEnvironment::new().expect("new env");
    // Load 20 templates to simulate a realistic rulebase
    for i in 0..20 {
        let tmpl = format!(
            "(deftemplate tmpl-{i} (slot id (type INTEGER)) (slot value (type FLOAT)) (slot label (type STRING)))"
        );
        env.eval(&tmpl).ok();
    }
    // Add some rules
    for i in 0..5 {
        let rule = format!(
            "(defrule rule-{i} (tmpl-{src} (id ?id) (value ?v)) => (assert (tmpl-{dst} (id ?id) (value (+ ?v 1.0)) (label \"derived\"))))",
            src = i,
            dst = i + 5
        );
        env.eval(&rule).ok();
    }
    env
}

fn bench_session_lifecycle(c: &mut Criterion) {
    let mut group = c.benchmark_group("clips_session_lifecycle");

    // Measure create + destroy cycle
    group.bench_function("create_destroy", |b| {
        b.iter(|| {
            let handle = clips_session_manager::session_create().expect("create");
            clips_session_manager::session_destroy(handle);
        });
    });

    // Measure create_from_env with loaded environment
    group.bench_function("create_from_loaded_env", |b| {
        b.iter_with_setup(create_loaded_env, |env| {
            let handle =
                clips_session_manager::session_create_from_env(env, Some("bench".to_string()))
                    .expect("create_from_env");
            clips_session_manager::session_destroy(handle);
        });
    });

    // Measure with_env lookup cost (session exists)
    group.bench_function("with_env_lookup", |b| {
        let handle = clips_session_manager::session_create().expect("create");
        b.iter(|| {
            clips_session_manager::with_env(handle, |env| env.list_module_names().ok())
                .expect("with_env");
        });
        clips_session_manager::session_destroy(handle);
    });

    // Measure with_env_result lookup cost
    group.bench_function("with_env_result_lookup", |b| {
        let env = create_loaded_env();
        let handle = clips_session_manager::session_create_from_env(env, Some("bench".to_string()))
            .expect("create");
        b.iter(|| {
            clips_session_manager::with_env_result(handle, |env| -> Result<usize, String> {
                Ok(env.list_module_names().unwrap_or_default().len())
            })
            .expect("with_env_result");
        });
        clips_session_manager::session_destroy(handle);
    });

    group.finish();
}

criterion_group!(benches, bench_session_lifecycle);
criterion_main!(benches);
