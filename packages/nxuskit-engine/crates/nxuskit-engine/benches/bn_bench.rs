#![allow(clippy::print_stderr)]
//! Criterion benchmarks for Bayesian Network inference, learning, and FFI.
//!
//! Covers performance targets from the specification:
//! - SC-002: VE < 1s for 20-node networks
//! - SC-003: Gibbs RMSE < 0.01 with 50K samples
//! - SC-004: MLE < 5s for 10K rows × 20 variables
//! - SC-009: Hill-Climb < 5min for 20 variables

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::path::Path;

use nxuskit_engine::providers::bayesian::bif::load_bif_file;
use nxuskit_engine::providers::bayesian::inference::sampling::{
    ForwardSampler, LikelihoodWeightedSampler, RejectionSampler,
};
use nxuskit_engine::providers::bayesian::inference::{
    GibbsSampler, InferenceEngine, JunctionTree, LoopyBeliefPropagation, MomentMatchingInference,
    NUTSConfig, NutsSampler, VariableElimination,
};
use nxuskit_engine::providers::bayesian::learning::hill_climb::{
    HillClimbConfig, HillClimbLearner,
};
use nxuskit_engine::providers::bayesian::learning::mle::{MleConfig, MleLearner};
use nxuskit_engine::providers::bayesian::learning::scoring::ScoringFunction;
use nxuskit_engine::providers::bayesian::learning::{Dataset, ParameterLearner, StructureLearner};
use nxuskit_engine::providers::bayesian::types::{GaussianVariable, VariableName};
use nxuskit_engine::providers::bayesian::{BayesianNetwork, Evidence};

fn fixture_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bn")
}

fn vn(name: &str) -> VariableName {
    VariableName::new(name).unwrap()
}

fn load_network(name: &str) -> BayesianNetwork {
    load_bif_file(&fixture_dir().join(format!("{}.bif", name))).unwrap()
}

// ── Inference Benchmarks ─────────────────────────────────────────

fn bench_ve_inference(c: &mut Criterion) {
    let mut group = c.benchmark_group("ve_inference");

    for (name, label) in &[
        ("cancer", "5-node"),
        ("asia", "8-node"),
        ("alarm", "37-node"),
    ] {
        let net = load_network(name);
        let evidence = Evidence::new();
        let ve = VariableElimination::new();

        group.bench_with_input(BenchmarkId::new("prior", label), name, |b, _| {
            b.iter(|| ve.infer(&net, &evidence).unwrap());
        });
    }

    // Asia with evidence
    {
        let net = load_network("asia");
        let mut evidence = Evidence::new();
        evidence.observe(&net, &vn("Smoking"), "yes").unwrap();
        let ve = VariableElimination::new();

        group.bench_function("asia_evidence", |b| {
            b.iter(|| ve.infer(&net, &evidence).unwrap());
        });
    }

    group.finish();
}

fn bench_jt_inference(c: &mut Criterion) {
    let mut group = c.benchmark_group("jt_inference");

    for (name, label) in &[
        ("cancer", "5-node"),
        ("asia", "8-node"),
        ("alarm", "37-node"),
    ] {
        let net = load_network(name);
        let evidence = Evidence::new();
        let jt = JunctionTree::new();

        group.bench_with_input(BenchmarkId::new("prior", label), name, |b, _| {
            b.iter(|| jt.infer(&net, &evidence).unwrap());
        });
    }

    group.finish();
}

fn bench_gibbs_inference(c: &mut Criterion) {
    let mut group = c.benchmark_group("gibbs_inference");
    group.sample_size(10); // Gibbs is slow, reduce sample count

    for (samples, label) in &[(1000, "1K"), (10_000, "10K"), (50_000, "50K")] {
        let net = load_network("asia");
        let evidence = Evidence::new();
        let gibbs = GibbsSampler::new(*samples, 200).with_seed(42);

        group.bench_with_input(BenchmarkId::new("asia_prior", label), samples, |b, _| {
            b.iter(|| gibbs.infer(&net, &evidence).unwrap());
        });
    }

    group.finish();
}

fn bench_sampling_inference(c: &mut Criterion) {
    let mut group = c.benchmark_group("sampling_inference");
    group.sample_size(10);

    let net = load_network("asia");
    let evidence = Evidence::new();

    // Forward sampling
    for (samples, label) in &[(1000, "1K"), (10_000, "10K")] {
        let sampler = ForwardSampler::new(*samples).with_seed(42);
        group.bench_with_input(BenchmarkId::new("forward", label), samples, |b, _| {
            b.iter(|| sampler.infer(&net, &evidence).unwrap());
        });
    }

    // Rejection sampling with evidence
    {
        let mut ev = Evidence::new();
        ev.observe(&net, &vn("Smoking"), "yes").unwrap();
        let sampler = RejectionSampler::new(10_000).with_seed(42);
        group.bench_function("rejection_10K_evidence", |b| {
            b.iter(|| sampler.infer(&net, &ev).unwrap());
        });
    }

    // Likelihood-weighted sampling with evidence
    {
        let mut ev = Evidence::new();
        ev.observe(&net, &vn("Smoking"), "yes").unwrap();
        let sampler = LikelihoodWeightedSampler::new(10_000).with_seed(42);
        group.bench_function("lw_10K_evidence", |b| {
            b.iter(|| sampler.infer(&net, &ev).unwrap());
        });
    }

    group.finish();
}

// ── Learning Benchmarks ──────────────────────────────────────────

fn bench_mle_learning(c: &mut Criterion) {
    let mut group = c.benchmark_group("mle_learning");

    let net = load_network("cancer");
    let csv_path = fixture_dir().join("cancer_mle_data.csv");
    if !csv_path.exists() {
        eprintln!("SKIP mle_learning: cancer_mle_data.csv not found");
        return;
    }
    let data = Dataset::from_csv(&csv_path, &net).unwrap();

    let mle = MleLearner::new(MleConfig::default());
    group.bench_function("cancer_10K", |b| {
        b.iter(|| {
            let mut net_clone = net.clone();
            mle.fit(&mut net_clone, &data).unwrap();
        });
    });

    group.finish();
}

fn bench_hill_climb(c: &mut Criterion) {
    let mut group = c.benchmark_group("hill_climb");
    group.sample_size(10);
    group.measurement_time(std::time::Duration::from_secs(30));

    let net = load_network("asia");
    let csv_path = fixture_dir().join("asia_hillclimb_data.csv");
    if !csv_path.exists() {
        eprintln!("SKIP hill_climb: asia_hillclimb_data.csv not found");
        return;
    }
    let data = Dataset::from_csv(&csv_path, &net).unwrap();

    let config = HillClimbConfig {
        scoring: ScoringFunction::BIC,
        max_parents: 5,
        max_steps: 1000,
        threshold: 1e-8,
    };
    let hc = HillClimbLearner::new(config);
    group.bench_function("asia_8node_bic", |b| {
        b.iter(|| hc.search(&net, &data).unwrap());
    });

    group.finish();
}

// ── C ABI Round-Trip Benchmark ───────────────────────────────────

fn bench_cabi_roundtrip(c: &mut Criterion) {
    let mut group = c.benchmark_group("cabi_roundtrip");

    // Simulate what the C ABI does: JSON serialize input → inference → JSON serialize output
    let net = load_network("asia");
    let evidence = Evidence::new();
    let ve = VariableElimination::new();

    group.bench_function("asia_ve_serialize_infer_serialize", |b| {
        b.iter(|| {
            // Simulate input serialization
            let input_json = serde_json::json!({
                "model": "asia",
                "algorithm": "variable_elimination",
                "observations": []
            });
            let _input_str = serde_json::to_string(&input_json).unwrap();

            // Inference
            let result = ve.infer(&net, &evidence).unwrap();

            // Simulate output serialization
            let output: std::collections::HashMap<String, Vec<f64>> = result
                .marginals
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect();
            let _output_str = serde_json::to_string(&output).unwrap();
        });
    });

    group.finish();
}

// ── Part 2: LBP Benchmark ──────────────────────────────────────

fn bench_lbp_inference(c: &mut Criterion) {
    let mut group = c.benchmark_group("lbp_inference");

    for (name, label) in &[
        ("cancer", "5-node"),
        ("asia", "8-node"),
        ("alarm", "37-node"),
    ] {
        let net = load_network(name);
        let evidence = Evidence::new();
        let lbp = LoopyBeliefPropagation::new();

        group.bench_with_input(BenchmarkId::new("prior", label), name, |b, _| {
            b.iter(|| lbp.infer(&net, &evidence).unwrap());
        });
    }

    // LBP with evidence
    {
        let net = load_network("asia");
        let mut evidence = Evidence::new();
        evidence.observe(&net, &vn("Smoking"), "yes").unwrap();
        let lbp = LoopyBeliefPropagation::new();

        group.bench_function("asia_evidence", |b| {
            b.iter(|| lbp.infer(&net, &evidence).unwrap());
        });
    }

    group.finish();
}

// ── Part 2: Parallel JT Benchmark ─────────────────────────────

fn bench_parallel_jt(c: &mut Criterion) {
    let mut group = c.benchmark_group("parallel_jt");

    let net = load_network("alarm");
    let evidence = Evidence::new();

    // Sequential JT
    let jt_seq = JunctionTree::sequential();
    group.bench_function("alarm_sequential", |b| {
        b.iter(|| jt_seq.infer(&net, &evidence).unwrap());
    });

    // Parallel JT (default)
    let jt_par = JunctionTree::new();
    group.bench_function("alarm_parallel", |b| {
        b.iter(|| jt_par.infer(&net, &evidence).unwrap());
    });

    group.finish();
}

// ── Part 2: Gaussian Moment-Matching Benchmark ────────────────

fn bench_gaussian_inference(c: &mut Criterion) {
    let mut group = c.benchmark_group("gaussian_inference");

    // 2-node Gaussian chain
    {
        let mut net = BayesianNetwork::new();
        net.add_gaussian_variable(GaussianVariable::new("X", 0.0, 1.0).unwrap())
            .unwrap();
        let y = GaussianVariable::new("Y", 0.0, 1.0)
            .unwrap()
            .with_weight("X", 0.8)
            .unwrap();
        net.add_gaussian_variable(y).unwrap();
        let evidence = Evidence::new();
        let mm = MomentMatchingInference::new();

        group.bench_function("2-node", |b| {
            b.iter(|| mm.infer(&net, &evidence).unwrap());
        });
    }

    // 5-node Gaussian chain
    {
        let mut net = BayesianNetwork::new();
        for i in 0..5 {
            let name = format!("X{}", i);
            let mut g = GaussianVariable::new(name, 0.0, 1.0).unwrap();
            if i > 0 {
                g = g.with_weight(format!("X{}", i - 1), 0.7).unwrap();
            }
            net.add_gaussian_variable(g).unwrap();
        }
        let evidence = Evidence::new();
        let mm = MomentMatchingInference::new();

        group.bench_function("5-node-chain", |b| {
            b.iter(|| mm.infer(&net, &evidence).unwrap());
        });
    }

    group.finish();
}

// ── Part 2: NUTS Benchmark ────────────────────────────────────

fn bench_nuts_inference(c: &mut Criterion) {
    let mut group = c.benchmark_group("nuts_inference");
    group.sample_size(10); // NUTS is slow

    // 2-node Gaussian network
    {
        let mut net = BayesianNetwork::new();
        net.add_gaussian_variable(GaussianVariable::new("X", 0.0, 1.0).unwrap())
            .unwrap();
        let y = GaussianVariable::new("Y", 0.0, 1.0)
            .unwrap()
            .with_weight("X", 0.8)
            .unwrap();
        net.add_gaussian_variable(y).unwrap();
        let evidence = Evidence::new();

        for (samples, label) in &[(500u64, "500"), (1000u64, "1K")] {
            let config = NUTSConfig {
                num_samples: *samples,
                num_warmup: 200,
                seed: 42,
                ..Default::default()
            };
            let nuts = NutsSampler::with_config(config);
            group.bench_with_input(BenchmarkId::new("2-node", label), samples, |b, _| {
                b.iter(|| nuts.infer(&net, &evidence).unwrap());
            });
        }
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_ve_inference,
    bench_jt_inference,
    bench_gibbs_inference,
    bench_sampling_inference,
    bench_lbp_inference,
    bench_parallel_jt,
    bench_gaussian_inference,
    bench_nuts_inference,
    bench_mle_learning,
    bench_hill_climb,
    bench_cabi_roundtrip,
);
criterion_main!(benches);
