use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use open_cypher::{lex, parse, parse_recovering};

const SIMPLE: &str = "RETURN 1 AS answer";
const REPRESENTATIVE_READ: &str = include_str!("fixtures/representative_read.cypher");
const REPRESENTATIVE_WRITE: &str = include_str!("fixtures/representative_write.cypher");
const PATH_HEAVY: &str = include_str!("fixtures/path_heavy.cypher");
const PATH_SEARCH_2024_3: &str = include_str!("fixtures/path_search_2024_3.cypher");

fn valid_inputs() -> [(&'static str, &'static str); 5] {
    [
        ("simple", SIMPLE),
        ("representative_read", REPRESENTATIVE_READ),
        ("representative_write", REPRESENTATIVE_WRITE),
        ("path_heavy", PATH_HEAVY),
        ("path_search_2024_3", PATH_SEARCH_2024_3),
    ]
}

fn lexer_benchmarks(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("lexer");

    for (name, source) in valid_inputs() {
        let warmup = lex(source);
        assert!(
            warmup.diagnostics.is_empty(),
            "benchmark input {name} must lex cleanly"
        );
        group.throughput(Throughput::Bytes(source.len() as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(name),
            source,
            |bencher, source| {
                bencher.iter(|| black_box(lex(black_box(source))));
            },
        );
    }

    group.finish();
}

fn strict_parser_benchmarks(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("parse_valid");

    for (name, source) in valid_inputs() {
        assert!(parse(source).is_ok(), "benchmark input {name} must parse");
        group.throughput(Throughput::Bytes(source.len() as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(name),
            source,
            |bencher, source| {
                bencher.iter(|| {
                    let parsed = parse(black_box(source)).expect("validated benchmark input");
                    black_box(parsed)
                });
            },
        );
    }

    group.finish();
}

fn recovery_benchmarks(criterion: &mut Criterion) {
    const INVALID: [(&str, &str); 4] = [
        ("unexpected_eof", "MATCH (n RETURN n"),
        ("trailing_operator", "RETURN 1 +"),
        ("wrong_closer", "RETURN [1, 2}"),
        (
            "multiple_errors",
            "MATCH (n RETURN n, WITH RETURN ] CREATE ({name: })",
        ),
    ];

    let mut group = criterion.benchmark_group("parse_invalid");
    for (name, source) in INVALID {
        assert!(!parse_recovering(source).diagnostics.is_empty());
        group.throughput(Throughput::Bytes(source.len() as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(name),
            source,
            |bencher, source| {
                bencher.iter(|| black_box(parse_recovering(black_box(source))));
            },
        );
    }
    group.finish();
}

fn corpus_benchmark(criterion: &mut Criterion) {
    let corpus = valid_inputs();
    let byte_count = corpus.iter().map(|(_, source)| source.len() as u64).sum();
    for (name, source) in &corpus {
        assert!(parse(source).is_ok(), "corpus member {name} must parse");
    }

    let mut group = criterion.benchmark_group("corpus");
    group.throughput(Throughput::Bytes(byte_count));
    group.bench_function("representative_batch", |bencher| {
        bencher.iter(|| {
            for (_, source) in &corpus {
                black_box(parse(black_box(source)).expect("validated corpus member"));
            }
        });
    });
    group.finish();
}

fn list_query(item_count: usize) -> String {
    let mut source = String::with_capacity(item_count * 4 + 20);
    source.push_str("RETURN [");
    for index in 0..item_count {
        if index != 0 {
            source.push(',');
        }
        source.push_str(&(index % 10).to_string());
    }
    source.push_str("] AS values");
    source
}

fn scaling_benchmarks(criterion: &mut Criterion) {
    let inputs = [
        ("approximately_1_kib", list_query(500)),
        ("approximately_10_kib", list_query(5_000)),
        ("approximately_100_kib", list_query(50_000)),
    ];
    let mut group = criterion.benchmark_group("scaling");

    for (name, source) in &inputs {
        assert!(parse(source).is_ok(), "scaling input {name} must parse");
        group.throughput(Throughput::Bytes(source.len() as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(name),
            source,
            |bencher, source| {
                bencher.iter(|| {
                    let parsed = parse(black_box(source)).expect("validated scaling input");
                    black_box(parsed)
                });
            },
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    lexer_benchmarks,
    strict_parser_benchmarks,
    recovery_benchmarks,
    corpus_benchmark,
    scaling_benchmarks
);
criterion_main!(benches);
