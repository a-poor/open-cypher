//! Span-exact contract tests for pattern comprehensions.
//!
//! Pattern comprehensions are detected token-wise and re-parsed from a nested
//! fragment at a nonzero source offset, so these tests deliberately assert the
//! exact source text of every sub-node (via `Span::text`) to pin down the
//! offset arithmetic, and exercise the detection heuristics from both sides.

use open_cypher::ast::{
    ClauseKind, Expr, ExprKind, PatternComprehension, ProjectionItemKind, QueryKind, StatementKind,
};
use open_cypher::{ParsedProgram, Span, parse};

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
    let ProjectionItemKind::Expression { expression, .. } = &return_clause.projection.items[0].kind
    else {
        panic!("expected an expression item")
    };
    expression
}

fn comprehension(expression: &Expr) -> &PatternComprehension {
    let ExprKind::PatternComprehension(comprehension) = &expression.kind else {
        panic!("expected a pattern comprehension, got {expression:#?}")
    };
    comprehension
}

#[test]
fn full_comprehension_reports_exact_sub_spans() {
    let source = "MATCH (a) RETURN [p = (a)-->(b) WHERE b.age > 1 | b.name] AS names";
    let parsed = parse(source).expect("comprehension should parse");
    let expression = first_return_expression(&parsed);
    assert_eq!(
        expression.span.text(source),
        Some("[p = (a)-->(b) WHERE b.age > 1 | b.name]")
    );

    let comprehension = comprehension(expression);
    let binding = comprehension.binding.as_ref().expect("binding");
    assert_eq!(binding.kind.text, "p");
    assert_eq!(binding.span.text(source), Some("p"));
    assert_eq!(comprehension.pattern.span.text(source), Some("(a)-->(b)"));
    let predicate = comprehension.predicate.as_ref().expect("predicate");
    assert_eq!(predicate.span.text(source), Some("b.age > 1"));
    assert_eq!(comprehension.projection.span.text(source), Some("b.name"));
}

#[test]
fn comprehension_without_binding_or_predicate() {
    let source = "MATCH (a) RETURN [(a)-->(b) | b]";
    let parsed = parse(source).expect("comprehension should parse");
    let comprehension = comprehension(first_return_expression(&parsed));
    assert!(comprehension.binding.is_none());
    assert!(comprehension.predicate.is_none());
    assert_eq!(comprehension.pattern.span.text(source), Some("(a)-->(b)"));
    assert_eq!(comprehension.projection.span.text(source), Some("b"));
}

#[test]
fn escaped_and_contextual_binding_names() {
    let source = "RETURN [`p q` = (a)-->(b) | b]";
    let parsed = parse(source).expect("escaped binding should parse");
    let escaped = comprehension(first_return_expression(&parsed));
    let binding = escaped.binding.as_ref().expect("binding");
    assert_eq!(binding.kind.text, "p q");
    assert_eq!(binding.span.text(source), Some("`p q`"));

    let source = "RETURN [match = (a)-->(b) | match]";
    let parsed = parse(source).expect("contextual-keyword binding should parse");
    let contextual = comprehension(first_return_expression(&parsed));
    let binding = contextual.binding.as_ref().expect("binding");
    assert_eq!(binding.kind.text, "match");
}

#[test]
fn undirected_relationships_are_recognized_as_comprehensions() {
    let source = "MATCH (a) RETURN [(a)--(b) | b.name]";
    let parsed = parse(source).expect("undirected comprehension should parse");
    let comprehension = comprehension(first_return_expression(&parsed));
    assert_eq!(comprehension.pattern.span.text(source), Some("(a)--(b)"));
    assert_eq!(comprehension.projection.span.text(source), Some("b.name"));
}

#[test]
fn nested_comprehension_projection_keeps_exact_offsets() {
    let source = "MATCH (n) RETURN [(n)--(m) | [(m)-->(x) | x.name]]";
    let parsed = parse(source).expect("nested comprehensions should parse");
    let outer = comprehension(first_return_expression(&parsed));
    assert_eq!(outer.pattern.span.text(source), Some("(n)--(m)"));
    assert_eq!(
        outer.projection.span.text(source),
        Some("[(m)-->(x) | x.name]")
    );
    let inner = comprehension(&outer.projection);
    assert_eq!(inner.pattern.span.text(source), Some("(m)-->(x)"));
    assert_eq!(inner.projection.span.text(source), Some("x.name"));
}

#[test]
fn relationship_detail_brackets_and_label_pipes_do_not_split_the_comprehension() {
    let source = "MATCH (a) RETURN [(a)-[r:KNOWS|LIKES]->(b) | r]";
    let parsed = parse(source).expect("relationship detail should parse");
    let comprehension = comprehension(first_return_expression(&parsed));
    assert_eq!(
        comprehension.pattern.span.text(source),
        Some("(a)-[r:KNOWS|LIKES]->(b)")
    );
    assert_eq!(comprehension.projection.span.text(source), Some("r"));
}

#[test]
fn label_predicate_inside_comprehension_keeps_exact_offsets() {
    let source = "MATCH (a) RETURN [(a)-->(b) WHERE b:Person | b]";
    let parsed = parse(source).expect("label predicate should parse");
    let bare = comprehension(first_return_expression(&parsed));
    let predicate = bare.predicate.as_ref().expect("predicate");
    assert_eq!(predicate.span.text(source), Some("b:Person"));
    assert_eq!(bare.projection.span.text(source), Some("b"));

    let source = "MATCH (a) RETURN [(a)-->(b) WHERE (b:Person) | b]";
    let parsed = parse(source).expect("label predicate should parse");
    let parenthesized = comprehension(first_return_expression(&parsed));
    let predicate = parenthesized.predicate.as_ref().expect("predicate");
    assert_eq!(predicate.span.text(source), Some("(b:Person)"));
}

#[test]
fn label_disjunction_pipes_resolve_against_the_projection_pipe() {
    // The label scanner must not swallow the projection separator: a `|`
    // may belong to a label disjunction before it, after it, or be the
    // separator itself.
    let source = "MATCH (a) RETURN [(a)-->(b) WHERE b:X|Y | b.name]";
    let parsed = parse(source).expect("disjunction in WHERE should parse");
    let in_where = comprehension(first_return_expression(&parsed));
    let predicate = in_where.predicate.as_ref().expect("predicate");
    assert_eq!(predicate.span.text(source), Some("b:X|Y"));
    assert_eq!(in_where.projection.span.text(source), Some("b.name"));

    let source = "MATCH (a) RETURN [(a)-->(b) | b:C|D]";
    let parsed = parse(source).expect("disjunction in projection should parse");
    let in_projection = comprehension(first_return_expression(&parsed));
    assert!(in_projection.predicate.is_none());
    assert_eq!(in_projection.projection.span.text(source), Some("b:C|D"));
}

#[test]
fn list_comprehensions_are_not_misdetected_as_pattern_comprehensions() {
    let source = "RETURN [x IN xs WHERE x > 0 | x * 2]";
    let parsed = parse(source).expect("list comprehension should parse");
    let expression = first_return_expression(&parsed);
    assert!(
        matches!(expression.kind, ExprKind::ListComprehension(_)),
        "expected a list comprehension, got {expression:#?}"
    );
}

#[test]
fn arithmetic_minuses_and_parens_do_not_fake_an_undirected_pattern() {
    // Each of these contains a top-level `|` plus enough minuses and
    // parentheses to look like an undirected relationship pattern if the
    // detection heuristics miscount; all must stay list comprehensions.
    for source in [
        "RETURN [x IN a - b - c | x]",
        "RETURN [x IN f(a - b) + g(c - d) | x]",
        "RETURN [x IN a - b | (c) + (d)]",
        "RETURN [x IN xs | (a)-->(b)]",
        // Function-call parens are not node patterns.
        "RETURN [x IN a - f(b) - g(c) | x]",
        "RETURN [x IN f(a) - g(b) - c | x]",
        // An arrow inside a nested comprehension is not evidence for the
        // outer brackets, and neither is one in a WHERE predicate.
        "RETURN [x IN [[(a)-->(b) | b]] | x]",
        "RETURN [x IN xs WHERE (a)-->(b) | x]",
    ] {
        let parsed = parse(source).expect("list comprehension should parse");
        let expression = first_return_expression(&parsed);
        assert!(
            matches!(expression.kind, ExprKind::ListComprehension(_)),
            "misdetected as a pattern comprehension: {source}"
        );
    }
}

#[test]
fn nested_and_braced_parens_do_not_count_as_pattern_nodes() {
    // The undirected-relationship heuristic counts top-level `(` tokens as
    // nodes; parentheses nested inside other parentheses, inside braces, or
    // after the projection pipe must not be counted, or these valid list
    // comprehensions (two top-level minuses each) would be misdetected as
    // pattern comprehensions and rejected.
    for source in [
        "RETURN [x IN a - f((b)) - c | x]",
        "RETURN [x IN (a) - b - c | (x)]",
        "RETURN [x IN m - {a: (1), b: (2)} - w | x]",
    ] {
        let parsed = parse(source).expect("list comprehension should parse");
        let expression = first_return_expression(&parsed);
        assert!(
            matches!(expression.kind, ExprKind::ListComprehension(_)),
            "misdetected as a pattern comprehension: {source}"
        );
    }
}

#[test]
fn nested_fragment_special_tokens_keep_exact_offsets() {
    let source = "MATCH (a) RETURN [(a)-->(b) | $`odd param`]";
    let parsed = parse(source).expect("escaped parameter should parse");
    let projection = &comprehension(first_return_expression(&parsed)).projection;
    assert_eq!(projection.span.text(source), Some("$`odd param`"));
    assert!(matches!(projection.kind, ExprKind::Parameter(_)));

    let source = "MATCH (a) RETURN [(a)-->(b) | foo.bar(b)]";
    let parsed = parse(source).expect("qualified function should parse");
    let projection = &comprehension(first_return_expression(&parsed)).projection;
    assert_eq!(projection.span.text(source), Some("foo.bar(b)"));
    let ExprKind::Function(function) = &projection.kind else {
        panic!("expected a function invocation, got {projection:#?}")
    };
    assert_eq!(function.name.span.text(source), Some("foo.bar"));
    assert_eq!(function.arguments[0].span.text(source), Some("b"));

    let source = "MATCH (a) RETURN [(a)-->(b) | shortestPath((a)-->(b))]";
    let parsed = parse(source).expect("shortestPath should parse");
    let projection = &comprehension(first_return_expression(&parsed)).projection;
    assert_eq!(
        projection.span.text(source),
        Some("shortestPath((a)-->(b))")
    );
}

#[test]
fn pattern_expressions_inside_projections_keep_exact_offsets() {
    let source = "MATCH (b) RETURN [(a)-->(b) | size((b)-->(c))]";
    let parsed = parse(source).expect("pattern expression argument should parse");
    let projection = &comprehension(first_return_expression(&parsed)).projection;
    assert_eq!(projection.span.text(source), Some("size((b)-->(c))"));
    let ExprKind::Function(function) = &projection.kind else {
        panic!("expected a function invocation, got {projection:#?}")
    };
    assert_eq!(function.arguments[0].span.text(source), Some("(b)-->(c)"));
}

#[test]
fn empty_projection_error_points_at_the_offending_token() {
    // `[(a)-->(b) |]` has a pipe but nothing after it, so it is not a
    // pattern comprehension; it falls through to the list-literal path and
    // the error must point at the `]` where an expression was expected --
    // not at the start of the statement.
    let source = "RETURN [(a)-->(b) |]";
    let errors = parse(source).expect_err("empty projection must not parse");
    assert_eq!(errors.diagnostics()[0].primary_span, Span::new(19, 20));
}

#[test]
fn malformed_comprehensions_error_without_panicking() {
    for source in [
        "RETURN [p = | b]",
        // A binding must be a name: a literal before `=` is not a binding,
        // and the leftover `5 = ...` is not a valid pattern either.
        "RETURN [5 = (a)-->(b) | b]",
        "RETURN [(a)-->(b) WHERE | b]",
        "RETURN [(a)-->(b) |]",
    ] {
        assert!(parse(source).is_err(), "expected an error: {source}");
    }
}
