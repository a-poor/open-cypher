use open_cypher::ast::{
    CallClause, CallTarget, Clause, ClauseKind, ExprKind, PathFactor, PathFactorKind,
    ProjectionItemKind, QuantifierKind, QueryKind, StatementKind,
};
use open_cypher::{ParsedProgram, Span, parse};

fn exact_span(source: &str, fragment: &str) -> Span {
    let mut matches = source.match_indices(fragment);
    let (start, _) = matches
        .next()
        .unwrap_or_else(|| panic!("{fragment:?} was not found in {source:?}"));
    assert!(
        matches.next().is_none(),
        "{fragment:?} must be unique in {source:?}"
    );
    Span::new(start, start + fragment.len())
}

fn clauses(parsed: &ParsedProgram) -> &[Clause] {
    let StatementKind::Query(statement) = &parsed.program.statements[0].kind else {
        panic!("expected a query statement")
    };
    let QueryKind::Regular(query) = &statement.query.kind else {
        panic!("expected a regular query")
    };
    assert!(query.unions.is_empty(), "fixture should not contain UNION");
    &query.head.kind.clauses
}

fn first_match_factor(parsed: &ParsedProgram) -> &PathFactor {
    let ClauseKind::Match(match_clause) = &clauses(parsed)[0].kind else {
        panic!("expected MATCH as the first clause")
    };
    assert_eq!(match_clause.pattern.parts.len(), 1);
    &match_clause.pattern.parts[0].kind.path.kind.factors[0]
}

fn only_call(parsed: &ParsedProgram) -> (&Clause, &CallClause) {
    let clauses = clauses(parsed);
    assert_eq!(clauses.len(), 1);
    let clause = &clauses[0];
    let ClauseKind::Call(call) = &clause.kind else {
        panic!("expected a CALL clause")
    };
    (clause, call)
}

#[test]
fn quantified_node_factor_retains_kind_and_exact_spans() {
    let source = "MATCH (node WHERE node.active){2} RETURN node";
    let parsed = parse(source).expect("quantified node fixture should parse");
    let factor = first_match_factor(&parsed);

    assert_eq!(
        factor.span,
        exact_span(source, "(node WHERE node.active){2}")
    );
    let PathFactorKind::QuantifiedNode {
        pattern,
        quantifier,
    } = &factor.kind
    else {
        panic!("expected a quantified node, got {factor:#?}")
    };
    assert_eq!(quantifier.span, exact_span(source, "{2}"));
    assert_eq!(quantifier.kind, QuantifierKind::Fixed("2".to_owned()));
    assert_eq!(
        pattern
            .where_clause
            .as_ref()
            .expect("node WHERE predicate")
            .span,
        exact_span(source, "node.active")
    );
}

#[test]
fn parenthesized_factor_retains_nested_path_predicate_quantifier_and_spans() {
    let source = "MATCH ((left)-[edge]->(right) WHERE edge.active){1,3} RETURN edge";
    let parsed = parse(source).expect("parenthesized path fixture should parse");
    let factor = first_match_factor(&parsed);

    assert_eq!(
        factor.span,
        exact_span(source, "((left)-[edge]->(right) WHERE edge.active){1,3}")
    );
    let PathFactorKind::Parenthesized {
        pattern,
        where_clause,
        quantifier,
    } = &factor.kind
    else {
        panic!("expected a parenthesized path, got {factor:#?}")
    };
    assert_eq!(pattern.span, exact_span(source, "(left)-[edge]->(right)"));
    assert_eq!(pattern.kind.factors.len(), 3);
    assert_eq!(pattern.kind.factors[0].span, exact_span(source, "(left)"));
    assert_eq!(
        pattern.kind.factors[1].span,
        exact_span(source, "-[edge]->")
    );
    assert_eq!(pattern.kind.factors[2].span, exact_span(source, "(right)"));
    assert_eq!(
        where_clause
            .as_ref()
            .expect("parenthesized WHERE predicate")
            .span,
        exact_span(source, "edge.active")
    );
    let quantifier = quantifier.as_ref().expect("parenthesized quantifier");
    assert_eq!(quantifier.span, exact_span(source, "{1,3}"));
    assert_eq!(
        quantifier.kind,
        QuantifierKind::Range {
            lower: Some("1".to_owned()),
            upper: Some("3".to_owned()),
        }
    );
}

#[test]
fn subpath_factor_retains_binding_nested_path_predicate_quantifier_and_spans() {
    let source = "MATCH (route = (start)-[hop]->(finish) WHERE hop.ready){2} RETURN route";
    let parsed = parse(source).expect("subpath fixture should parse");
    let factor = first_match_factor(&parsed);

    assert_eq!(
        factor.span,
        exact_span(
            source,
            "(route = (start)-[hop]->(finish) WHERE hop.ready){2}"
        )
    );
    let PathFactorKind::Subpath {
        binding,
        pattern,
        where_clause,
        quantifier,
    } = &factor.kind
    else {
        panic!("expected a bound subpath, got {factor:#?}")
    };
    assert_eq!(binding.kind.text, "route");
    assert_eq!(
        binding.span,
        Span::new(factor.span.start + 1, factor.span.start + 6)
    );
    assert_eq!(pattern.span, exact_span(source, "(start)-[hop]->(finish)"));
    assert_eq!(pattern.kind.factors.len(), 3);
    assert_eq!(
        where_clause.as_ref().expect("subpath WHERE predicate").span,
        exact_span(source, "hop.ready")
    );
    let quantifier = quantifier.as_ref().expect("subpath quantifier");
    assert_eq!(quantifier.span, exact_span(source, "{2}"));
    assert_eq!(quantifier.kind, QuantifierKind::Fixed("2".to_owned()));
}

#[test]
fn legacy_shortest_factor_retains_kind_inner_path_and_exact_spans() {
    for (source, factor_text, expected_all) in [
        (
            "MATCH route = shortestPath((start)-[hop]->(finish)) RETURN route",
            "shortestPath((start)-[hop]->(finish))",
            false,
        ),
        (
            "MATCH route = allShortestPaths((start)-[hop]->(finish)) RETURN route",
            "allShortestPaths((start)-[hop]->(finish))",
            true,
        ),
    ] {
        let parsed = parse(source).expect("legacy shortest-path fixture should parse");
        let factor = first_match_factor(&parsed);

        assert_eq!(factor.span, exact_span(source, factor_text));
        let PathFactorKind::LegacyShortest { all, pattern } = &factor.kind else {
            panic!("expected a legacy shortest-path factor, got {factor:#?}")
        };
        assert_eq!(*all, expected_all);
        assert_eq!(pattern.span, exact_span(source, "(start)-[hop]->(finish)"));
        assert_eq!(pattern.kind.factors.len(), 3);
        assert_eq!(pattern.kind.factors[0].span, exact_span(source, "(start)"));
        assert_eq!(pattern.kind.factors[1].span, exact_span(source, "-[hop]->"));
        assert_eq!(pattern.kind.factors[2].span, exact_span(source, "(finish)"));
    }
}

#[test]
fn standalone_call_distinguishes_implicit_and_explicit_empty_arguments() {
    for (source, explicit_arguments) in
        [("CALL test.my.proc", false), ("CALL test.my.proc()", true)]
    {
        let parsed = parse(source).expect("standalone CALL fixture should parse");
        let (clause, call) = only_call(&parsed);

        assert_eq!(clause.span, Span::new(0, source.len()));
        assert!(!call.optional);
        assert!(call.yield_clause.is_none());
        let CallTarget::Procedure { name, arguments } = &call.target else {
            panic!("expected a procedure target")
        };
        assert_eq!(name.span, exact_span(source, "test.my.proc"));
        assert_eq!(
            name.parts
                .iter()
                .map(|part| part.kind.text.as_str())
                .collect::<Vec<_>>(),
            ["test", "my", "proc"]
        );
        assert_eq!(arguments.is_some(), explicit_arguments);
        assert!(arguments.as_ref().is_none_or(Vec::is_empty));
    }
}

#[test]
fn standalone_named_yield_retains_item_shapes_and_exact_spans() {
    let source = "CALL test.my.proc YIELD output AS alias, status";
    let parsed = parse(source).expect("standalone named YIELD fixture should parse");
    let (clause, call) = only_call(&parsed);

    assert_eq!(clause.span, Span::new(0, source.len()));
    let CallTarget::Procedure { arguments, .. } = &call.target else {
        panic!("expected a procedure target")
    };
    assert!(
        arguments.is_none(),
        "omitted parentheses must remain visible"
    );

    let yield_clause = call.yield_clause.as_ref().expect("YIELD clause");
    assert!(!yield_clause.all);
    assert!(yield_clause.where_clause.is_none());
    assert_eq!(
        yield_clause.span,
        exact_span(source, "YIELD output AS alias, status")
    );
    assert_eq!(yield_clause.items.len(), 2);

    let aliased = &yield_clause.items[0];
    assert_eq!(aliased.span, exact_span(source, "output AS alias"));
    let ProjectionItemKind::Expression { expression, alias } = &aliased.kind else {
        panic!("expected an expression YIELD item")
    };
    assert_eq!(expression.span, exact_span(source, "output"));
    let ExprKind::Variable(field) = &expression.kind else {
        panic!("expected the yielded field to be a variable")
    };
    assert_eq!(field.span, exact_span(source, "output"));
    let alias = alias.as_ref().expect("AS alias");
    assert_eq!(alias.kind.text, "alias");
    assert_eq!(alias.span, exact_span(source, "alias"));

    let unaliased = &yield_clause.items[1];
    assert_eq!(unaliased.span, exact_span(source, "status"));
    let ProjectionItemKind::Expression { expression, alias } = &unaliased.kind else {
        panic!("expected an expression YIELD item")
    };
    assert!(alias.is_none());
    assert_eq!(expression.span, exact_span(source, "status"));
    let ExprKind::Variable(field) = &expression.kind else {
        panic!("expected the yielded field to be a variable")
    };
    assert_eq!(field.span, exact_span(source, "status"));
}

#[test]
fn standalone_wildcard_yield_retains_all_shape_and_terminator_span() {
    let source = "CALL test.my.proc YIELD *;";
    let parsed = parse(source).expect("standalone wildcard YIELD fixture should parse");
    let statement = &parsed.program.statements[0];
    let StatementKind::Query(query) = &statement.kind else {
        panic!("expected a query statement")
    };
    assert_eq!(statement.span, Span::new(0, source.len()));
    assert_eq!(query.terminator, Some(exact_span(source, ";")));

    let (clause, call) = only_call(&parsed);
    assert_eq!(clause.span, exact_span(source, "CALL test.my.proc YIELD *"));
    let yield_clause = call.yield_clause.as_ref().expect("YIELD clause");
    assert!(yield_clause.all);
    assert!(yield_clause.items.is_empty());
    assert!(yield_clause.where_clause.is_none());
    assert_eq!(yield_clause.span, exact_span(source, "YIELD *"));
}

#[test]
fn ast_variant_fuzz_seeds_remain_valid_queries() {
    for source in [
        include_str!("../fuzz/corpus/parse_strict/ast_path_factor_variants"),
        include_str!("../fuzz/corpus/parse_strict/subpath_factor"),
        include_str!("../fuzz/corpus/parse_strict/legacy_shortest_factor"),
        include_str!("../fuzz/corpus/parse_strict/standalone_call_yield"),
    ] {
        parse(source).unwrap_or_else(|errors| {
            panic!("valid fuzz seed failed to parse: {source}\n{errors:#?}")
        });
    }
}

#[cfg(feature = "serde")]
#[test]
fn new_path_factors_and_standalone_call_shapes_round_trip_through_json() {
    for source in [
        "MATCH (node WHERE node.active){2} RETURN node",
        "MATCH ((left)-[edge]->(right) WHERE edge.active){1,3} RETURN edge",
        "MATCH (route = (start)-[hop]->(finish) WHERE hop.ready){2} RETURN route",
        "MATCH route = shortestPath((start)-[hop]->(finish)) RETURN route",
        "MATCH route = allShortestPaths((start)-[hop]->(finish)) RETURN route",
        "CALL test.my.proc",
        "CALL test.my.proc()",
        "CALL test.my.proc YIELD output AS alias, status",
        "CALL test.my.proc YIELD *;",
    ] {
        let parsed = parse(source).unwrap_or_else(|errors| {
            panic!("serde fixture failed to parse: {source}\n{errors:#?}")
        });
        let json = serde_json::to_string(&parsed)
            .unwrap_or_else(|error| panic!("failed to serialize {source:?}: {error}"));
        let decoded: ParsedProgram = serde_json::from_str(&json)
            .unwrap_or_else(|error| panic!("failed to deserialize {source:?}: {error}"));
        assert_eq!(decoded, parsed, "serde changed the AST for {source:?}");
    }
}
