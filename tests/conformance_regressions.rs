use open_cypher::ast::{
    ClauseKind, Expr, ExprKind, LiteralKind, PathFactorKind, QuantifierKind, QueryKind,
    StatementKind,
};
use open_cypher::{DiagnosticCode, ParsedProgram, parse, parse_recovering};

fn first_return_expression(parsed: &ParsedProgram) -> &Expr {
    let StatementKind::Query(statement) = &parsed.program.statements[0].kind else {
        panic!("expected a query statement")
    };
    let QueryKind::Regular(query) = &statement.query.kind else {
        panic!("expected a regular query")
    };
    let ClauseKind::Return(return_clause) = &query.head.kind.clauses.last().unwrap().kind else {
        panic!("expected a final RETURN clause")
    };
    let open_cypher::ast::ProjectionItemKind::Expression { expression, .. } =
        &return_clause.projection.items[0].kind
    else {
        panic!("expected an expression item")
    };
    expression
}

#[test]
fn unicode_escapes_are_decoded_and_invalid_escapes_are_rejected() {
    let parsed =
        parse(r"RETURN '\u0041\U01F642' AS value").expect("valid Unicode escapes should parse");
    let ExprKind::Literal(literal) = &first_return_expression(&parsed).kind else {
        panic!("expected a literal")
    };
    let LiteralKind::String(string) = &literal.kind else {
        panic!("expected a string literal")
    };
    assert_eq!(string.value, "A🙂");

    for source in [r"RETURN '\q'", r"RETURN '\u12xz'", r"RETURN '\U110000'"] {
        let errors = parse(source).expect_err("invalid escapes must be rejected");
        assert!(
            errors
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::InvalidEscape),
            "missing invalid-escape diagnostic for {source:?}: {errors:#?}"
        );
    }
}

#[test]
fn legacy_and_graph_relationship_quantifiers_are_both_preserved() {
    let parsed =
        parse("MATCH (a)-[r*1..3]->{2}(b) RETURN r").expect("both quantifier layers are valid");
    let StatementKind::Query(statement) = &parsed.program.statements[0].kind else {
        panic!("expected a query statement")
    };
    let QueryKind::Regular(query) = &statement.query.kind else {
        panic!("expected a regular query")
    };
    let ClauseKind::Match(match_clause) = &query.head.kind.clauses[0].kind else {
        panic!("expected MATCH")
    };
    let PathFactorKind::Relationship(relationship) =
        &match_clause.pattern.parts[0].kind.path.kind.factors[1].kind
    else {
        panic!("expected a relationship factor")
    };
    assert_eq!(
        relationship
            .legacy_quantifier
            .as_ref()
            .expect("legacy quantifier")
            .kind,
        QuantifierKind::Range {
            lower: Some("1".to_owned()),
            upper: Some("3".to_owned()),
        }
    );
    assert_eq!(
        relationship
            .graph_quantifier
            .as_ref()
            .expect("graph quantifier")
            .kind,
        QuantifierKind::Fixed("2".to_owned())
    );
}

#[test]
fn element_properties_and_where_predicates_are_mutually_exclusive() {
    for source in [
        "MATCH (n {x: 1} WHERE n.x = 1) RETURN n",
        "MATCH (a)-[r {x: 1} WHERE r.x = 1]->(b) RETURN r",
    ] {
        assert!(
            parse(source).is_err(),
            "an element accepted both properties and WHERE: {source}"
        );
    }
    assert!(parse("MATCH (n {x: 1}) RETURN n").is_ok());
    assert!(parse("MATCH (n WHERE n.x = 1) RETURN n").is_ok());
}

#[test]
fn only_simple_comparison_operators_can_chain() {
    assert!(parse("RETURN 1 < 2 <= 3").is_ok());
    for source in ["RETURN 1 IN 2 IN 3", "RETURN 1 IS NULL IS NULL"] {
        assert!(
            parse(source).is_err(),
            "advanced predicate chained: {source}"
        );
    }
}

#[test]
fn undirected_and_nested_pattern_comprehensions_parse_without_internal_errors() {
    let parsed = parse("RETURN [(n)--(m) | [(m)-->(x) | x]]")
        .expect("undirected and nested pattern comprehensions should parse");
    let ExprKind::PatternComprehension(outer) = &first_return_expression(&parsed).kind else {
        panic!("expected an outer pattern comprehension")
    };
    assert!(matches!(
        &outer.projection.kind,
        ExprKind::PatternComprehension(_)
    ));

    let malformed = parse_recovering("RETURN [(n)-->(m) | ]");
    assert!(!malformed.diagnostics.is_empty());
    assert!(
        malformed
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != DiagnosticCode::Internal),
        "malformed user input surfaced an internal error: {:#?}",
        malformed.diagnostics
    );
}

#[test]
fn case_reduce_and_trim_build_typed_expression_nodes() {
    let searched = parse("RETURN CASE WHEN n > 0 THEN 'yes' ELSE 'no' END")
        .expect("searched CASE should parse");
    let ExprKind::Case(case) = &first_return_expression(&searched).kind else {
        panic!("expected CASE")
    };
    assert!(case.operand.is_none());
    assert_eq!(case.alternatives.len(), 1);
    assert_eq!(case.alternatives[0].when.len(), 1);
    assert!(case.else_expression.is_some());

    let simple = parse("RETURN CASE n WHEN 1, 2 THEN 'small' ELSE 'other' END")
        .expect("simple CASE with an operand list should parse");
    let ExprKind::Case(case) = &first_return_expression(&simple).kind else {
        panic!("expected CASE")
    };
    assert!(case.operand.is_some());
    assert_eq!(case.alternatives[0].when.len(), 2);

    let reduced =
        parse("RETURN reduce(total = 0, x IN [1, 2, 3] | total + x)").expect("REDUCE should parse");
    let ExprKind::Reduce(reduce) = &first_return_expression(&reduced).kind else {
        panic!("expected REDUCE")
    };
    assert_eq!(reduce.accumulator.kind.text, "total");
    assert_eq!(reduce.variable.kind.text, "x");

    let trimmed = parse("RETURN trim('  value  ')").expect("TRIM should parse");
    let ExprKind::Function(function) = &first_return_expression(&trimmed).kind else {
        panic!("expected TRIM to use the function invocation AST")
    };
    assert_eq!(function.name.parts[0].kind.text, "trim");
    assert_eq!(function.arguments.len(), 1);
}

#[test]
fn qualified_function_names_retain_each_component_and_span() {
    let source = "RETURN analytics.math.score(DISTINCT n)";
    let parsed = parse(source).expect("qualified function calls should parse");
    let ExprKind::Function(function) = &first_return_expression(&parsed).kind else {
        panic!("expected a function call")
    };
    assert_eq!(
        function
            .name
            .parts
            .iter()
            .map(|part| part.kind.text.as_str())
            .collect::<Vec<_>>(),
        ["analytics", "math", "score"]
    );
    assert_eq!(
        function.name.span.text(source),
        Some("analytics.math.score")
    );
    assert_eq!(function.arguments.len(), 1);
}
