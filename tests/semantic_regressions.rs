use open_cypher::ast::{
    BinaryOperator, ClauseKind, Expr, ExprKind, FunctionInvocation, ProjectionClause,
    ProjectionItemKind, QueryKind, SetQuantifier, StatementKind,
};
use open_cypher::{ParsedProgram, parse};

fn only_projection(parsed: &ParsedProgram) -> &ProjectionClause {
    assert_eq!(parsed.program.statements.len(), 1);
    let StatementKind::Query(statement) = &parsed.program.statements[0].kind else {
        panic!("expected a query statement")
    };
    let QueryKind::Regular(query) = &statement.query.kind else {
        panic!("expected a regular query")
    };
    assert!(query.unions.is_empty());
    assert_eq!(query.head.kind.clauses.len(), 1);
    let ClauseKind::Return(return_clause) = &query.head.kind.clauses[0].kind else {
        panic!("expected one RETURN clause")
    };
    &return_clause.projection
}

fn expression(item: &ProjectionItemKind) -> &Expr {
    let ProjectionItemKind::Expression { expression, .. } = item else {
        panic!("expected an expression projection item")
    };
    expression
}

fn function(expression: &Expr) -> &FunctionInvocation {
    let ExprKind::Function(function) = &expression.kind else {
        panic!("expected a function invocation, got {:#?}", expression.kind)
    };
    function
}

#[test]
fn return_must_be_the_final_clause_of_each_union_branch() {
    for source in [
        "RETURN 1 MATCH (n)",
        "RETURN 1 RETURN 2",
        "MATCH (n) RETURN n CREATE (m)",
        "RETURN 1 UNION RETURN 2 MATCH (n)",
    ] {
        assert!(
            parse(source).is_err(),
            "RETURN before another clause unexpectedly parsed: {source}"
        );
    }

    assert!(parse("MATCH (n) RETURN n").is_ok());
    assert!(parse("RETURN 1 UNION MATCH (n) RETURN n").is_ok());
}

#[test]
fn list_comprehension_builds_a_typed_spanned_ast() {
    let source = "RETURN [x IN [1, 2, 3] WHERE x > 1 | x * 2] AS values";
    let parsed = parse(source).expect("list comprehension should parse");
    let projection = only_projection(&parsed);
    let item = &projection.items[0];
    let list_expression = expression(&item.kind);

    assert_eq!(
        list_expression.span.text(source),
        Some("[x IN [1, 2, 3] WHERE x > 1 | x * 2]")
    );
    let ExprKind::ListComprehension(comprehension) = &list_expression.kind else {
        panic!(
            "expected a list comprehension, got {:#?}",
            list_expression.kind
        )
    };
    assert_eq!(comprehension.variable.kind.text, "x");
    assert!(matches!(&comprehension.list.kind, ExprKind::List(items) if items.len() == 3));
    assert!(matches!(
        comprehension.predicate.as_deref().map(|expression| &expression.kind),
        Some(ExprKind::Binary { operator, .. }) if operator.kind == BinaryOperator::Greater
    ));
    assert!(matches!(
        comprehension.projection.as_deref().map(|expression| &expression.kind),
        Some(ExprKind::Binary { operator, .. }) if operator.kind == BinaryOperator::Multiply
    ));
    let ProjectionItemKind::Expression { alias, .. } = &item.kind else {
        unreachable!("the item was checked above")
    };
    assert_eq!(
        alias
            .as_ref()
            .expect("expression item should retain its alias")
            .kind
            .text,
        "values"
    );

    let shorthand = parse("RETURN [x IN xs]").expect("unfiltered comprehension should parse");
    assert!(matches!(
        &expression(&only_projection(&shorthand).items[0].kind).kind,
        ExprKind::ListComprehension(_)
    ));
}

#[test]
fn explicit_set_quantifiers_and_count_arguments_are_preserved() {
    let source = "RETURN ALL count(n), collect(DISTINCT n), sum(ALL n), count(*)";
    let parsed = parse(source).expect("set quantifiers and count arguments should parse");
    let projection = only_projection(&parsed);

    let projection_quantifier = projection
        .quantifier
        .as_ref()
        .expect("RETURN ALL should retain an explicit quantifier");
    assert_eq!(projection_quantifier.kind, SetQuantifier::All);
    assert_eq!(projection_quantifier.span.text(source), Some("ALL"));
    assert_eq!(projection.items.len(), 4);

    let count = function(expression(&projection.items[0].kind));
    assert_eq!(count.name.parts[0].kind.text, "count");
    assert_eq!(count.name.span.text(source), Some("count"));
    assert!(count.quantifier.is_none());
    assert_eq!(count.arguments.len(), 1);
    assert!(matches!(&count.arguments[0].kind, ExprKind::Variable(_)));

    let collect = function(expression(&projection.items[1].kind));
    assert_eq!(collect.name.parts[0].kind.text, "collect");
    let distinct = collect
        .quantifier
        .as_ref()
        .expect("function DISTINCT should retain an explicit quantifier");
    assert_eq!(distinct.kind, SetQuantifier::Distinct);
    assert_eq!(distinct.span.text(source), Some("DISTINCT"));

    let sum = function(expression(&projection.items[2].kind));
    let all = sum
        .quantifier
        .as_ref()
        .expect("function ALL should retain an explicit quantifier");
    assert_eq!(all.kind, SetQuantifier::All);
    assert_eq!(all.span.text(source), Some("ALL"));

    let count_star = function(expression(&projection.items[3].kind));
    assert!(matches!(&count_star.arguments[0].kind, ExprKind::Wildcard));
    assert_eq!(count_star.arguments[0].span.text(source), Some("*"));
}

#[test]
fn wildcard_is_only_allowed_as_the_first_projection_item() {
    assert!(parse("RETURN *, 1 AS one").is_ok());
    assert!(parse("RETURN 1, *").is_err());
}
