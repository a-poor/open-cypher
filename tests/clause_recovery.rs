use open_cypher::{
    Span,
    ast::{ClauseKind, QueryKind, StatementKind},
    lex, parse, parse_recovering,
};

fn clauses(source: &str) -> Vec<open_cypher::ast::Clause> {
    assert!(parse(source).is_err());
    let outcome = parse_recovering(source);
    assert!(!outcome.diagnostics.is_empty());
    assert!(outcome.diagnostics.len() <= 32);
    let parsed = outcome.value.unwrap();
    assert_eq!(parsed.tokens, lex(source).tokens);
    let StatementKind::Query(statement) = &parsed.program.statements[0].kind else {
        panic!("expected recovered query: {source}");
    };
    let QueryKind::Regular(query) = &statement.query.kind else {
        panic!()
    };
    assert!(query.unions.is_empty());
    query.head.kind.clauses.clone()
}

#[test]
fn retains_clauses_before_and_after_errors_with_original_spans() {
    let source = "  MATCH (n) SET n.x = WITH n RETURN n;  ";
    let recovered = clauses(source);
    assert_eq!(recovered.len(), 4);
    assert!(matches!(recovered[0].kind, ClauseKind::Match(_)));
    assert!(matches!(recovered[1].kind, ClauseKind::Error(_)));
    assert_eq!(
        &source[recovered[1].span.start..recovered[1].span.end],
        "SET n.x ="
    );
    assert!(matches!(recovered[2].kind, ClauseKind::With(_)));
    assert!(matches!(recovered[3].kind, ClauseKind::Return(_)));
    let parsed = parse_recovering(source).value.unwrap();
    assert_eq!(parsed.program.span, Span::new(2, source.len() - 2));
}

#[test]
fn handles_leading_trailing_and_multiple_broken_regions() {
    let recovered = clauses("? MATCH (n) SET n.x = WITH n REMOVE");
    assert_eq!(recovered.len(), 5);
    for index in [0, 2, 4] {
        assert!(matches!(recovered[index].kind, ClauseKind::Error(_)));
    }
    let recovered = clauses("MATCH (n) RETURN n.");
    assert!(matches!(recovered[0].kind, ClauseKind::Match(_)));
    assert!(matches!(recovered[1].kind, ClauseKind::Error(_)));
}

#[test]
fn preserves_compound_clauses_and_contextual_names() {
    for valid in [
        "OPTIONAL MATCH (n)",
        "MERGE (n) ON CREATE SET n.x = 1 ON MATCH SET n.x = 2",
        "WITH 1 AS match",
        "MATCH (n) WHERE n.return = 1",
        "DETACH DELETE n",
    ] {
        let source = format!("{valid} SET n.x = RETURN n");
        let recovered = clauses(&source);
        assert_eq!(recovered.len(), 3, "{source}");
        assert_eq!(recovered[0].span, Span::new(0, valid.len()));
        assert!(matches!(recovered[1].kind, ClauseKind::Error(_)));
        assert!(matches!(recovered[2].kind, ClauseKind::Return(_)));
    }
}

#[test]
fn does_not_promote_nested_clauses_or_cross_return() {
    let recovered = clauses("MATCH (n) WITH [1, RETURN n");
    assert_eq!(recovered.len(), 2);
    assert!(matches!(recovered[1].kind, ClauseKind::Error(_)));
    let recovered = clauses("RETURN 1 MATCH (n)");
    assert_eq!(recovered.len(), 2);
    assert!(matches!(recovered[1].kind, ClauseKind::Error(_)));
}

#[test]
fn ambiguous_or_unrecoverable_queries_keep_error_statement() {
    for source in [
        "MATCH (n",
        "MATCH (n RETURN n",
        "RETURN 1 UNION RETURN",
        "MATCH (n); RETURN",
        ";",
        "???",
    ] {
        let outcome = parse_recovering(source);
        assert!(!outcome.diagnostics.is_empty());
        assert!(
            matches!(
                outcome.value.unwrap().program.statements[0].kind,
                StatementKind::Error(_)
            ),
            "{source}"
        );
    }
}

#[test]
fn recovery_budget_keeps_a_large_broken_suffix_as_an_error() {
    let source = format!("MATCH (n) {}", "SET n.x = ".repeat(200));
    let recovered = clauses(&source);
    assert_eq!(recovered.len(), 2);
    assert!(matches!(recovered[0].kind, ClauseKind::Match(_)));
    assert!(matches!(recovered[1].kind, ClauseKind::Error(_)));
    assert_eq!(recovered[1].span.end, source.trim_end().len());
}
