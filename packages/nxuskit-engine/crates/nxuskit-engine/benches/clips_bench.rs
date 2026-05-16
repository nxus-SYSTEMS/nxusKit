//! CLIPS provider benchmarks measuring converter/parser CPU costs.
//!
//! Focuses on the pure-Rust hot paths: fact parsing, rule construction,
//! JSON serialization of CLIPS types, and security validation.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

use nxuskit_engine::providers::clips::security::SecuritySeverity;
use nxuskit_engine::providers::clips::{ClipsCodeBuilder, ClipsToJsonConverter, SecurityValidator};

// ── Fact Parsing Benchmarks ─────────────────────────────────

fn bench_fact_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("clips_fact_parsing");

    let facts = [
        ("simple", "(fact (name test) (value 42))"),
        (
            "multi_slot",
            "(person (name \"John Doe\") (age 30) (city \"New York\") (active TRUE))",
        ),
        (
            "nested",
            "(complex (id 1) (data \"some long string with spaces\") (score 99.5) (tags a b c))",
        ),
    ];

    for (label, fact) in &facts {
        group.bench_with_input(BenchmarkId::new("parse", label), fact, |b, fact| {
            b.iter(|| ClipsToJsonConverter::parse_fact_string(fact));
        });
    }

    group.finish();
}

// ── Value Parsing Benchmarks ────────────────────────────────

fn bench_value_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("clips_value_parsing");

    let values = [
        ("integer", "42"),
        ("float", "3.14159"),
        ("string", "\"hello world\""),
        ("symbol", "TRUE"),
        ("negative", "-99"),
    ];

    for (label, val) in &values {
        group.bench_with_input(BenchmarkId::new("parse", label), val, |b, val| {
            b.iter(|| ClipsToJsonConverter::parse_value(val));
        });
    }

    group.finish();
}

// ── Rule Construction Benchmarks ────────────────────────────

fn bench_rule_construction(c: &mut Criterion) {
    let mut group = c.benchmark_group("clips_rule_construction");

    // Single rule
    group.bench_function("single_rule", |b| {
        b.iter(|| {
            let mut builder = ClipsCodeBuilder::new();
            builder.defrule(
                "test-rule",
                None,
                None,
                &["(fact (name ?n) (value ?v))"],
                &["(assert (result (name ?n) (status processed)))"],
            );
            builder.build()
        });
    });

    // 10 rules
    group.bench_function("10_rules", |b| {
        b.iter(|| {
            let mut builder = ClipsCodeBuilder::new();
            for i in 0..10 {
                builder.defrule(
                    &format!("rule-{}", i),
                    None,
                    None,
                    &[&format!("(input (id {}))", i)],
                    &[&format!("(assert (output (id {})))", i)],
                );
            }
            builder.build()
        });
    });

    group.finish();
}

// ── Security Validation Benchmarks ──────────────────────────

fn bench_security_validation(c: &mut Criterion) {
    let mut group = c.benchmark_group("clips_security");

    let safe_rule = "(defrule safe\n    (fact (x ?v))\n    =>\n    (assert (result ?v)))";
    let complex_rule = r#"(defrule complex
    (person (name ?n) (age ?a))
    (test (> ?a 18))
    (address (city ?c) (zip ?z))
    =>
    (assert (eligible (name ?n) (city ?c)))
    (printout t "Processing " ?n crlf))"#;

    let validator = SecurityValidator::new(SecuritySeverity::Error);

    group.bench_function("validate_safe_rule", |b| {
        b.iter(|| validator.validate_rules(safe_rule));
    });

    group.bench_function("validate_complex_rule", |b| {
        b.iter(|| validator.validate_rules(complex_rule));
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_fact_parsing,
    bench_value_parsing,
    bench_rule_construction,
    bench_security_validation,
);
criterion_main!(benches);
