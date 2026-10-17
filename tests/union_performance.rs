use open_cypher::{
    ast::{ClauseKind, QueryKind, StatementKind, UnionOperator},
    parse, parse_recovering,
};

#[test]
fn long_valid_union_chain_preserves_contextual_names_and_branch_order() {
    let branches: Vec<_> = (0..128)
        .map(|value| format!("WITH {value} AS union RETURN union"))
        .collect();
    let source = branches.join(" UNION ALL ");
    let parsed = parse(&source).unwrap();
    let StatementKind::Query(statement) = &parsed.program.statements[0].kind else {
        panic!()
    };
    let QueryKind::Regular(query) = &statement.query.kind else {
        panic!()
    };
    assert_eq!(query.unions.len(), 127);
    for (expected, branch) in branches
        .iter()
        .zip(std::iter::once(&query.head).chain(query.unions.iter().map(|union| &union.query)))
    {
        assert_eq!(&source[branch.span.start..branch.span.end], expected);
        assert_eq!(branch.kind.clauses.len(), 2);
        assert!(matches!(branch.kind.clauses[0].kind, ClauseKind::With(_)));
        assert!(matches!(branch.kind.clauses[1].kind, ClauseKind::Return(_)));
    }
    assert!(
        query
            .unions
            .iter()
            .all(|union| union.operator.kind == UnionOperator::All)
    );
}

#[test]
fn incomplete_compound_clauses_still_try_longer_candidates() {
    for clause in [
        "MERGE (n) ON CREATE SET n.union = 1 ON MATCH SET n.union = 2 RETURN n",
        "OPTIONAL MATCH (n) WHERE n.union = 1 RETURN n",
        "WITH 1 AS union RETURN union",
        "CALL union YIELD union",
    ] {
        assert!(parse(clause).is_ok(), "{clause}");
        let malformed = format!("{clause} UNION DISTINCT RETURN");
        let outcome = parse_recovering(&malformed);
        assert!(!outcome.diagnostics.is_empty(), "{malformed}");
        assert!(outcome.diagnostics.len() <= 32);
    }
}
