#![no_main]

use std::sync::OnceLock;

use libfuzzer_sys::fuzz_target;
use open_cypher::{DiagnosticCode, parse_recovering};
use serde_json::Value;

const TCK_SYNTAX_PROJECTION: &str = include_str!("../../spec/generated/TCK_SYNTAX.jsonl");

fn projected_queries() -> &'static [String] {
    static QUERIES: OnceLock<Vec<String>> = OnceLock::new();
    QUERIES.get_or_init(|| {
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

        let mut queries = Vec::with_capacity(expected_query_count);
        for record in records {
            match record.get("record").and_then(Value::as_str) {
                Some("query") => {
                    let expectation = record
                        .get("expectation")
                        .and_then(Value::as_str)
                        .expect("query record expectation");
                    assert!(matches!(expectation, "accept" | "reject"));
                    queries.push(
                        record
                            .get("source")
                            .and_then(Value::as_str)
                            .expect("query record source")
                            .to_owned(),
                    );
                }
                Some("occurrence") => {}
                Some(other) => panic!("unknown generated TCK record type `{other}`"),
                None => panic!("generated TCK record has no record type"),
            }
        }
        assert_eq!(queries.len(), expected_query_count);
        queries
    })
}

fuzz_target!(|data: &[u8]| {
    let queries = projected_queries();
    if queries.is_empty() || data.is_empty() {
        return;
    }

    let selector_len = data.len().min(8);
    let selector = data[..selector_len].iter().fold(0usize, |value, byte| {
        value.wrapping_mul(257) ^ usize::from(*byte)
    });
    let base = &queries[selector % queries.len()];
    let source = if data.len() == selector_len {
        base.clone()
    } else {
        let mut bytes = base.as_bytes().to_vec();
        let insertion = selector % (bytes.len() + 1);
        bytes.splice(insertion..insertion, data[selector_len..].iter().copied());
        String::from_utf8_lossy(&bytes).into_owned()
    };

    let outcome = parse_recovering(&source);
    assert!(outcome.value.is_some());
    assert!(outcome.diagnostics.len() <= 32);
    for diagnostic in outcome.diagnostics {
        assert_ne!(diagnostic.code, DiagnosticCode::Internal);
        assert!(diagnostic.primary_span.start <= diagnostic.primary_span.end);
        assert!(diagnostic.primary_span.end <= source.len());
        assert!(source.is_char_boundary(diagnostic.primary_span.start));
        assert!(source.is_char_boundary(diagnostic.primary_span.end));
    }
});
