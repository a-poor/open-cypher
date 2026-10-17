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
    let source = "  MATCH (n) SET n. = 1 WITH n RETURN n;  ";
    let recovered = clauses(source);
    assert_eq!(recovered.len(), 4);
    assert!(matches!(recovered[0].kind, ClauseKind::Match(_)));
    assert!(matches!(recovered[1].kind, ClauseKind::Error(_)));
    assert_eq!(
        &source[recovered[1].span.start..recovered[1].span.end],
        "SET n. = 1"
    );
    assert!(matches!(recovered[2].kind, ClauseKind::With(_)));
    assert!(matches!(recovered[3].kind, ClauseKind::Return(_)));
    let parsed = parse_recovering(source).value.unwrap();
    assert_eq!(parsed.program.span, Span::new(2, source.len() - 2));
}

#[test]
fn handles_leading_trailing_and_multiple_broken_regions() {
    let recovered = clauses("? MATCH (n) SET n. = 1 WITH n REMOVE n.");
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
        let source = format!("{valid} SET n. = 1 RETURN n");
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

#[test]
fn exhausted_budget_does_not_recover_a_later_valid_clause() {
    let source = format!("MATCH (n) {}RETURN n", "SET n.x = ".repeat(200));
    let recovered = clauses(&source);
    assert_eq!(recovered.len(), 2);
    assert!(matches!(recovered[0].kind, ClauseKind::Match(_)));
    assert!(matches!(recovered[1].kind, ClauseKind::Error(_)));
    assert_eq!(recovered[1].span, Span::new(10, source.len()));
}

#[test]
fn last_recovery_attempt_can_still_retain_a_valid_clause() {
    // The first clause uses 127 nonempty candidates, leaving one attempt for
    // WITH. Empty slices must not consume that remaining attempt. Adding one
    // more boundary exhausts the budget before WITH can be recovered.
    for (repetitions, expected_clauses) in [(125, 3), (126, 2)] {
        let source = format!("MATCH (n) WITH n {}", "SET n.x = ".repeat(repetitions));
        let recovered = clauses(&source);
        assert_eq!(recovered.len(), expected_clauses);
        assert!(matches!(recovered[0].kind, ClauseKind::Match(_)));
        if repetitions == 125 {
            assert!(matches!(recovered[1].kind, ClauseKind::With(_)));
            assert_eq!(recovered[1].span, Span::new(10, 16));
        }
        assert!(matches!(
            recovered.last().unwrap().kind,
            ClauseKind::Error(_)
        ));
        assert_eq!(recovered.last().unwrap().span.end, source.trim_end().len());
    }
}

#[test]
fn terminal_return_coalesces_all_remaining_boundaries_into_one_error() {
    // Mutation triage: once RETURN is retained, visiting each later boundary
    // instead of jumping to EOF must not change the error region or its span.
    for suffix in [
        "MATCH (m) WITH m RETURN m",
        "SET n.x = REMOVE n.x DETACH DELETE n",
        "WITH [1, RETURN n] MATCH (m)",
        "MATCH (m",
    ] {
        let source = format!("RETURN n {suffix}");
        let recovered = clauses(&source);
        assert_eq!(recovered.len(), 2);
        assert!(matches!(recovered[0].kind, ClauseKind::Return(_)));
        assert_eq!(recovered[0].span, Span::new(0, 8));
        assert!(matches!(recovered[1].kind, ClauseKind::Error(_)));
        assert_eq!(recovered[1].span, Span::new(9, source.len()));
    }
}
