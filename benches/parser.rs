use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use open_cypher::{lex, parse, parse_recovering};
use serde_json::Value;

const SIMPLE: &str = "RETURN 1 AS answer";
const REPRESENTATIVE_READ: &str = include_str!("fixtures/representative_read.cypher");
const REPRESENTATIVE_WRITE: &str = include_str!("fixtures/representative_write.cypher");
const PATH_HEAVY: &str = include_str!("fixtures/path_heavy.cypher");
const PATH_SEARCH_2024_3: &str = include_str!("fixtures/path_search_2024_3.cypher");
const TCK_SYNTAX_PROJECTION: &str = include_str!("../spec/generated/TCK_SYNTAX.jsonl");

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

fn projected_tck_queries() -> Vec<String> {
    let mut records = TCK_SYNTAX_PROJECTION
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str::<Value>(line).expect("valid generated TCK JSONL"));
    let header = records.next().expect("generated TCK JSONL header");
    assert_eq!(header.get("record").and_then(Value::as_str), Some("header"));
    assert_eq!(
        header.get("schema_version").and_then(Value::as_u64),
        Some(1)
    );
    let expected_query_count = header
        .get("unique_queries")
        .and_then(Value::as_u64)
        .expect("TCK header unique_queries") as usize;

    let mut query_count = 0;
    let mut accepted = Vec::new();
    for record in records {
        match record.get("record").and_then(Value::as_str) {
            Some("query") => {
                query_count += 1;
                let expectation = record
                    .get("expectation")
                    .and_then(Value::as_str)
                    .expect("query record expectation");
                assert!(matches!(expectation, "accept" | "reject"));
                if expectation == "accept" {
                    accepted.push(
                        record
                            .get("source")
                            .and_then(Value::as_str)
                            .expect("query record source")
                            .to_owned(),
                    );
                }
            }
            Some("occurrence") => {}
            Some(other) => panic!("unknown generated TCK record type `{other}`"),
            None => panic!("generated TCK record has no record type"),
        }
    }
    assert_eq!(query_count, expected_query_count);
    accepted
}

fn tck_corpus_benchmark(criterion: &mut Criterion) {
    let queries = projected_tck_queries();
    assert!(
        !queries.is_empty(),
        "generated TCK corpus must not be empty"
    );
    for query in &queries {
        assert!(
            parse(query).is_ok(),
            "accepted generated TCK query must parse: {query}"
        );
    }

    let byte_count = queries.iter().map(|query| query.len() as u64).sum();
    let mut group = criterion.benchmark_group("tck_corpus");
    group.throughput(Throughput::Bytes(byte_count));
    group.bench_function("all_unique_accepted_queries", |bencher| {
        bencher.iter(|| {
            for query in &queries {
                black_box(parse(black_box(query)).expect("validated projected TCK query"));
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

fn contextual_projection_query(target_bytes: usize) -> String {
    let keywords = [
        "all", "and", "as", "call", "case", "create", "delete", "exists", "false", "match",
        "merge", "null", "order", "return", "set", "where", "with", "yield",
    ];
    let mut source = String::from("RETURN ");
    let mut index = 0usize;
    while source.len() < target_bytes {
        if index != 0 {
            source.push_str(", ");
        }
        source.push_str("0 AS ");
        source.push_str(keywords[index % keywords.len()]);
        index += 1;
    }
    source
}

fn scaling_benchmarks(criterion: &mut Criterion) {
    let inputs = [
        ("approximately_1_kib", list_query(500)),
        ("approximately_10_kib", list_query(5_000)),
        ("approximately_100_kib", list_query(50_000)),
        (
            "contextual_names_64_kib",
            contextual_projection_query(63 * 1024),
        ),
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

    let malformed_contextual = format!("{}, +", contextual_projection_query(63 * 1024));
    assert!(
        parse(&malformed_contextual).is_err(),
        "malformed contextual-name scaling input must be rejected"
    );
    group.throughput(Throughput::Bytes(malformed_contextual.len() as u64));
    group.bench_with_input(
        BenchmarkId::from_parameter("contextual_names_64_kib_malformed"),
        &malformed_contextual,
        |bencher, source| {
            bencher.iter(|| black_box(parse(black_box(source))));
        },
    );
    group.finish();
}

criterion_group!(
    benches,
    lexer_benchmarks,
    strict_parser_benchmarks,
    recovery_benchmarks,
    corpus_benchmark,
    tck_corpus_benchmark,
    scaling_benchmarks
);
criterion_main!(benches);
