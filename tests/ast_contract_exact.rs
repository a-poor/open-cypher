use open_cypher::ast::{
    BinaryOperator, CallClause, ClauseKind, Expr, ExprKind, IntegerRadix, LabelExpressionKind,
    LiteralKind, PathFactorKind, PathSelector, ProjectionItem, ProjectionItemKind, QuantifierKind,
    QueryKind, RelationshipDirection, StatementKind,
};
use open_cypher::{ParsedProgram, Span, parse};

fn assert_decimal_integer(expression: &Expr, text: &str, span: Span) {
    assert_eq!(expression.span, span);
    let ExprKind::Literal(literal) = &expression.kind else {
        panic!("expected an integer literal, got {expression:#?}");
    };
    assert_eq!(literal.span, span);
    let LiteralKind::Integer(integer) = &literal.kind else {
        panic!("expected an integer literal, got {literal:#?}");
    };
    assert_eq!(integer.text, text);
    assert_eq!(integer.radix, IntegerRadix::Decimal);
}

fn first_call(parsed: &ParsedProgram) -> &CallClause {
    let StatementKind::Query(statement) = &parsed.program.statements[0].kind else {
        panic!("expected a query statement");
    };
    let QueryKind::Regular(query) = &statement.query.kind else {
        panic!("expected a regular query");
    };
    let ClauseKind::Call(call) = &query.head.kind.clauses[0].kind else {
        panic!("expected CALL as the first clause");
    };
    call
}

fn assert_yield_item(
    source: &str,
    item: &ProjectionItem,
    item_text: &str,
    expression_text: &str,
    alias_text: Option<&str>,
) {
    assert_eq!(item.span.text(source), Some(item_text));
    let ProjectionItemKind::Expression { expression, alias } = &item.kind else {
        panic!("expected an expression YIELD item, got {item:#?}");
    };
    assert_eq!(expression.span.text(source), Some(expression_text));
    let ExprKind::Variable(variable) = &expression.kind else {
        panic!("expected a variable YIELD expression, got {expression:#?}");
    };
    assert_eq!(variable.span, expression.span);
    assert_eq!(variable.span.text(source), Some(expression_text));
    assert_eq!(
        alias.as_ref().map(|name| name.kind.text.as_str()),
        alias_text
    );
    assert_eq!(
        alias.as_ref().and_then(|name| name.span.text(source)),
        alias_text
    );
}

#[test]
fn arithmetic_ast_preserves_precedence_values_and_exact_spans() {
    let source = "RETURN 1 + 2 * 3 AS value;";
    let parsed = parse(source).expect("contract query should parse");

    assert_eq!(parsed.program.span, Span::new(0, source.len()));
    assert_eq!(parsed.program.statements.len(), 1);
    let statement = &parsed.program.statements[0];
    assert_eq!(statement.span, Span::new(0, source.len()));
    let StatementKind::Query(statement) = &statement.kind else {
        panic!("expected a query statement, got {statement:#?}");
    };
    assert_eq!(statement.terminator, Some(Span::new(25, 26)));

    let QueryKind::Regular(query) = &statement.query.kind else {
        panic!("expected a regular query, got {:#?}", statement.query);
    };
    assert!(query.unions.is_empty());
    assert_eq!(query.head.kind.clauses.len(), 1);
    let clause = &query.head.kind.clauses[0];
    assert_eq!(clause.span, Span::new(0, 25));
    let ClauseKind::Return(return_clause) = &clause.kind else {
        panic!("expected RETURN, got {clause:#?}");
    };
    assert_eq!(return_clause.projection.span, Span::new(7, 25));
    assert_eq!(return_clause.projection.items.len(), 1);

    let item = &return_clause.projection.items[0];
    assert_eq!(item.span, Span::new(7, 25));
    let ProjectionItemKind::Expression { expression, alias } = &item.kind else {
        panic!("expected an expression projection, got {item:#?}");
    };
    let alias = alias.as_ref().expect("AS value should retain its alias");
    assert_eq!(alias.kind.text, "value");
    assert!(!alias.kind.escaped);
    assert_eq!(alias.span, Span::new(20, 25));

    assert_eq!(expression.span, Span::new(7, 16));
    let ExprKind::Binary {
        left,
        operator,
        right,
    } = &expression.kind
    else {
        panic!("expected addition at the expression root, got {expression:#?}");
    };
    assert_eq!(operator.kind, BinaryOperator::Add);
    assert_eq!(operator.span, Span::new(9, 10));
    assert_decimal_integer(left, "1", Span::new(7, 8));

    let ExprKind::Binary {
        left,
        operator,
        right: multiply_right,
    } = &right.kind
    else {
        panic!("expected multiplication on the right, got {right:#?}");
    };
    assert_eq!(right.span, Span::new(11, 16));
    assert_eq!(operator.kind, BinaryOperator::Multiply);
    assert_eq!(operator.span, Span::new(13, 14));
    assert_decimal_integer(left, "2", Span::new(11, 12));
    assert_decimal_integer(multiply_right, "3", Span::new(15, 16));
}

#[test]
fn shortest_path_ast_retains_binding_selector_relationship_and_quantifier() {
    let source = "MATCH path = ANY SHORTEST (a)-[r:KNOWS]->{1,3}(b) RETURN path";
    let parsed = parse(source).expect("path contract query should parse");

    let StatementKind::Query(statement) = &parsed.program.statements[0].kind else {
        panic!("expected a query statement");
    };
    let QueryKind::Regular(query) = &statement.query.kind else {
        panic!("expected a regular query");
    };
    assert_eq!(query.head.kind.clauses.len(), 2);
    let ClauseKind::Match(match_clause) = &query.head.kind.clauses[0].kind else {
        panic!("expected MATCH as the first clause");
    };
    assert!(!match_clause.optional);
    assert!(match_clause.mode.is_none());
    assert!(match_clause.where_clause.is_none());
    assert_eq!(match_clause.pattern.parts.len(), 1);

    let part = &match_clause.pattern.parts[0];
    let binding = part.kind.binding.as_ref().expect("path should be bound");
    assert_eq!(binding.kind.text, "path");
    assert_eq!(binding.span.text(source), Some("path"));
    let selector = part
        .kind
        .selector
        .as_ref()
        .expect("selector should be retained");
    assert_eq!(selector.kind, PathSelector::AnyShortest);
    assert_eq!(selector.span.text(source), Some("ANY SHORTEST"));

    let factors = &part.kind.path.kind.factors;
    assert_eq!(factors.len(), 3);
    let PathFactorKind::Node(start) = &factors[0].kind else {
        panic!("path should start with a node");
    };
    assert_eq!(
        start.variable.as_ref().map(|name| name.kind.text.as_str()),
        Some("a")
    );

    let PathFactorKind::Relationship(relationship) = &factors[1].kind else {
        panic!("middle path factor should be a relationship");
    };
    assert_eq!(relationship.direction.kind, RelationshipDirection::Right);
    assert_eq!(
        relationship
            .variable
            .as_ref()
            .map(|name| name.kind.text.as_str()),
        Some("r")
    );
    let labels = relationship
        .labels
        .as_ref()
        .expect("relationship type should be retained");
    let LabelExpressionKind::Name(label) = &labels.kind else {
        panic!("expected one relationship type, got {labels:#?}");
    };
    assert_eq!(label.kind.text, "KNOWS");

    assert!(relationship.legacy_quantifier.is_none());
    let quantifier = relationship
        .graph_quantifier
        .as_ref()
        .expect("graph quantifier should be retained");
    assert_eq!(quantifier.span.text(source), Some("{1,3}"));
    assert_eq!(
        quantifier.kind,
        QuantifierKind::Range {
            lower: Some("1".to_owned()),
            upper: Some("3".to_owned()),
        }
    );

    let PathFactorKind::Node(end) = &factors[2].kind else {
        panic!("path should end with a node");
    };
    assert_eq!(
        end.variable.as_ref().map(|name| name.kind.text.as_str()),
        Some("b")
    );
}

#[test]
fn standalone_yield_items_keep_field_alias_and_item_spans_separate() {
    let source = "CALL example.proc YIELD out AS alias, other";
    let parsed = parse(source).expect("standalone procedure call should parse");
    let call = first_call(&parsed);
    let yield_clause = call
        .yield_clause
        .as_ref()
        .expect("standalone CALL should retain YIELD");

    assert_eq!(
        yield_clause.span.text(source),
        Some("YIELD out AS alias, other")
    );
    assert!(yield_clause.where_clause.is_none());
    assert_eq!(yield_clause.items.len(), 2);
    assert_yield_item(
        source,
        &yield_clause.items[0],
        "out AS alias",
        "out",
        Some("alias"),
    );
    assert_yield_item(source, &yield_clause.items[1], "other", "other", None);
}

#[test]
fn standalone_yield_accepts_a_contextual_keyword_field() {
    let source = "CALL example.proc YIELD null AS n, count";
    let parsed = parse(source).expect("contextual YIELD field should parse");
    let call = first_call(&parsed);
    let yield_clause = call
        .yield_clause
        .as_ref()
        .expect("standalone CALL should retain YIELD");

    assert_eq!(yield_clause.items.len(), 2);
    assert_yield_item(
        source,
        &yield_clause.items[0],
        "null AS n",
        "null",
        Some("n"),
    );
    assert_yield_item(source, &yield_clause.items[1], "count", "count", None);
}

#[test]
fn in_query_yield_items_do_not_absorb_aliases_or_where_clauses() {
    let source = "CALL example.proc() YIELD out AS alias, other WHERE other > 0 RETURN alias";
    let parsed = parse(source).expect("in-query procedure call should parse");
    let call = first_call(&parsed);
    let yield_clause = call
        .yield_clause
        .as_ref()
        .expect("in-query CALL should retain YIELD");

    assert_eq!(
        yield_clause.span.text(source),
        Some("YIELD out AS alias, other WHERE other > 0")
    );
    assert_eq!(yield_clause.items.len(), 2);
    assert_yield_item(
        source,
        &yield_clause.items[0],
        "out AS alias",
        "out",
        Some("alias"),
    );
    assert_yield_item(source, &yield_clause.items[1], "other", "other", None);
    assert_eq!(
        yield_clause
            .where_clause
            .as_ref()
            .and_then(|expression| expression.span.text(source)),
        Some("other > 0")
    );
}
