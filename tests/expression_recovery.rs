use open_cypher::{
    Span,
    ast::{
        BinaryOperator, Clause, ClauseKind, Expr, ExprKind, ProjectionItemKind, QueryKind,
        StatementKind,
    },
    lex, parse, parse_recovering,
};

fn clauses(source: &str) -> Vec<Clause> {
    assert!(parse(source).is_err());
    let outcome = parse_recovering(source);
    assert!(!outcome.diagnostics.is_empty());
    assert!(outcome.diagnostics.len() <= 32);
    assert!(
        outcome
            .diagnostics
            .iter()
            .all(|diagnostic| !diagnostic.message.contains("error expression"))
    );
    let parsed = outcome.value.unwrap();
    assert_eq!(parsed.tokens, lex(source).tokens);
    let StatementKind::Query(statement) = &parsed.program.statements[0].kind else {
        panic!("{source}")
    };
    let QueryKind::Regular(query) = &statement.query.kind else {
        panic!()
    };
    query.head.kind.clauses.clone()
}

fn expression(clause: &Clause, index: usize) -> &Expr {
    let projection = match &clause.kind {
        ClauseKind::Return(clause) => &clause.projection,
        ClauseKind::With(clause) => &clause.projection,
        other => panic!("expected projection: {other:?}"),
    };
    let ProjectionItemKind::Expression { expression, .. } = &projection.items[index].kind else {
        panic!()
    };
    expression
}

#[test]
fn missing_operand_preserves_binary_structure_and_exact_spans() {
    let source = "  RETURN 1 +;  ";
    let clauses = clauses(source);
    let expr = expression(&clauses[0], 0);
    let ExprKind::Binary {
        left,
        operator,
        right,
    } = &expr.kind
    else {
        panic!("{expr:?}")
    };
    assert_eq!(expr.span, Span::new(9, 12));
    assert_eq!(left.span, Span::new(9, 10));
    assert_eq!(operator.kind, BinaryOperator::Add);
    assert_eq!(operator.span, Span::new(11, 12));
    assert_eq!(right.span, Span::empty(12));
    assert!(matches!(right.kind, ExprKind::Error(_)));
}

#[test]
fn projections_aliases_and_following_clauses_survive() {
    let source = "WITH 1 + AS value, 2 AS other RETURN value, other";
    let clauses = clauses(source);
    assert_eq!(clauses.len(), 2);
    assert!(matches!(
        expression(&clauses[0], 0).kind,
        ExprKind::Binary { .. }
    ));
    assert!(matches!(
        expression(&clauses[0], 1).kind,
        ExprKind::Literal(_)
    ));
    assert!(matches!(clauses[1].kind, ClauseKind::Return(_)));
}

#[test]
fn nested_arguments_and_lists_preserve_valid_siblings() {
    let clauses = clauses("RETURN f(1 +, [2, , 3]), 4");
    let ExprKind::Function(function) = &expression(&clauses[0], 0).kind else {
        panic!()
    };
    assert_eq!(function.arguments.len(), 2);
    let ExprKind::Binary { right, .. } = &function.arguments[0].kind else {
        panic!()
    };
    assert!(matches!(right.kind, ExprKind::Error(_)));
    let ExprKind::List(items) = &function.arguments[1].kind else {
        panic!()
    };
    assert_eq!(items.len(), 3);
    assert!(matches!(items[1].kind, ExprKind::Error(_)));
    assert!(items[1].span.is_empty());
    assert!(matches!(
        expression(&clauses[0], 1).kind,
        ExprKind::Literal(_)
    ));
}

#[test]
fn repairs_preserve_precedence() {
    let clauses = clauses("RETURN 1 + * 2");
    let ExprKind::Binary {
        operator, right, ..
    } = &expression(&clauses[0], 0).kind
    else {
        panic!()
    };
    assert_eq!(operator.kind, BinaryOperator::Add);
    let ExprKind::Binary {
        left,
        operator,
        right,
    } = &right.kind
    else {
        panic!()
    };
    assert_eq!(operator.kind, BinaryOperator::Multiply);
    assert!(matches!(left.kind, ExprKind::Error(_)));
    assert!(matches!(right.kind, ExprKind::Literal(_)));
}

#[test]
fn ambiguous_aliases_names_and_unclosed_delimiters_use_clause_errors() {
    for broken in [
        "RETURN n.",
        "RETURN 1 AS",
        "RETURN AS value",
        "RETURN f(1 +",
        "RETURN [1 +",
        "SET n. = 1",
    ] {
        let source = format!("MATCH (n) {broken}");
        let clauses = clauses(&source);
        assert!(
            matches!(clauses.last().unwrap().kind, ClauseKind::Error(_)),
            "{source}"
        );
    }
}

#[test]
fn repair_limit_falls_back_to_clause_error() {
    let repaired = clauses("RETURN [1 +, 2 +, 3 +, 4 +]");
    assert!(matches!(repaired[0].kind, ClauseKind::Return(_)));
    let fallback = clauses("MATCH (n) RETURN [1 +, 2 +, 3 +, 4 +, 5 +]");
    assert!(matches!(fallback[1].kind, ClauseKind::Error(_)));
}

#[test]
fn fully_valid_contextual_names_win_over_expression_repairs() {
    let clauses = clauses("WITH 1 AS return SET n. = 1 RETURN return");
    assert_eq!(clauses.len(), 3);
    assert!(matches!(clauses[0].kind, ClauseKind::With(_)));
    assert!(matches!(clauses[1].kind, ClauseKind::Error(_)));
    assert!(matches!(
        expression(&clauses[2], 0).kind,
        ExprKind::Variable(_)
    ));
}

#[test]
fn missing_operands_in_union_branch_do_not_absorb_the_next_branch() {
    // Minimized from the former clause-only property test: this branch now
    // retains its expression structure rather than becoming a clause error.
    let source = "RETURN 1 + *\tUNION DISTINCT\tRETURN 0";
    let outcome = parse_recovering(source);
    assert!(!outcome.diagnostics.is_empty());
    let parsed = outcome.value.unwrap();
    let StatementKind::Query(statement) = &parsed.program.statements[0].kind else {
        panic!()
    };
    let QueryKind::Regular(query) = &statement.query.kind else {
        panic!()
    };
    assert_eq!(query.unions.len(), 1);
    let ExprKind::Binary { right, .. } = &expression(&query.head.kind.clauses[0], 0).kind else {
        panic!()
    };
    let ExprKind::Binary { left, right, .. } = &right.kind else {
        panic!()
    };
    assert!(matches!(left.kind, ExprKind::Error(_)));
    assert!(matches!(right.kind, ExprKind::Error(_)));
    assert!(matches!(
        expression(&query.unions[0].query.kind.clauses[0], 0).kind,
        ExprKind::Literal(_)
    ));
}

#[test]
fn empty_projection_and_nested_subquery_operands_are_recovered() {
    let bare = clauses("RETURN");
    assert!(matches!(expression(&bare[0], 0).kind, ExprKind::Error(_)));
    assert_eq!(expression(&bare[0], 0).span, Span::empty(6));
    let nested = clauses("RETURN EXISTS { RETURN 1 UNION RETURN }");
    let ExprKind::Subquery(subquery) = &expression(&nested[0], 0).kind else {
        panic!()
    };
    let QueryKind::Regular(query) = &subquery.query.kind else {
        panic!()
    };
    assert_eq!(query.unions.len(), 1);
    assert!(matches!(
        expression(&query.unions[0].query.kind.clauses[0], 0).kind,
        ExprKind::Error(_)
    ));
}

#[test]
fn where_and_set_expressions_are_recovered_in_place() {
    let result = clauses("MATCH (n) WHERE n.x > SET n.x = RETURN n");
    assert_eq!(result.len(), 3);
    let ClauseKind::Match(clause) = &result[0].kind else {
        panic!()
    };
    let ExprKind::Binary { right, .. } = &clause.where_clause.as_ref().unwrap().kind else {
        panic!()
    };
    assert!(matches!(right.kind, ExprKind::Error(_)));
    assert!(matches!(result[1].kind, ClauseKind::Set(_)));
    assert!(matches!(result[2].kind, ClauseKind::Return(_)));
}

#[cfg(feature = "serde")]
#[test]
fn recovered_expression_round_trips_through_json() {
    let outcome = parse_recovering("RETURN f(1 +, [2, , 3])");
    let encoded = serde_json::to_string(&outcome).unwrap();
    let decoded = serde_json::from_str(&encoded).unwrap();
    assert_eq!(outcome, decoded);
}

fn assert_missing_right(expression: &Expr, location: usize) {
    let ExprKind::Binary { right, .. } = &expression.kind else {
        panic!("expected binary expression: {expression:?}")
    };
    assert!(matches!(right.kind, ExprKind::Error(_)));
    assert_eq!(right.span, Span::empty(location));
}

#[test]
fn map_values_keep_keys_and_valid_siblings() {
    let source = "RETURN {broken: 1 +, intact: 2}, 3";
    let result = clauses(source);
    let ExprKind::Map(entries) = &expression(&result[0], 0).kind else {
        panic!()
    };
    assert_eq!(entries.len(), 2);
    assert_eq!(
        &source[entries[0].key.span.start..entries[0].key.span.end],
        "broken"
    );
    assert_missing_right(&entries[0].value, source.find(',').unwrap());
    assert!(matches!(entries[1].value.kind, ExprKind::Literal(_)));
    assert!(matches!(
        expression(&result[0], 1).kind,
        ExprKind::Literal(_)
    ));
}

#[test]
fn case_result_keeps_other_arms_and_else_expression() {
    let source = "RETURN CASE WHEN true THEN (1 +) WHEN false THEN 4 ELSE 2 END, 3";
    let result = clauses(source);
    let ExprKind::Case(case) = &expression(&result[0], 0).kind else {
        panic!()
    };
    assert_eq!(case.alternatives.len(), 2);
    assert!(matches!(
        case.alternatives[1].then.kind,
        ExprKind::Literal(_)
    ));
    let ExprKind::Parenthesized(result) = &case.alternatives[0].then.kind else {
        panic!()
    };
    assert_missing_right(result, source.find(')').unwrap());
    assert!(matches!(
        case.else_expression.as_ref().unwrap().kind,
        ExprKind::Literal(_)
    ));
}

#[test]
fn ambiguous_case_keyword_uses_fallback_and_retains_preceding_clause() {
    // ELSE can be a contextual operand name. Recovery does not guess whether
    // to reinterpret it as a CASE boundary after consuming it as an operand.
    let source = "MATCH (n) RETURN CASE WHEN true THEN 1 + ELSE 2 END";
    let result = clauses(source);
    assert!(matches!(result[0].kind, ClauseKind::Match(_)));
    assert!(matches!(result[1].kind, ClauseKind::Error(_)));
}

#[test]
fn list_comprehension_keeps_binding_list_and_projection() {
    let source = "RETURN [x IN [1, 2] | x +], 3";
    let result = clauses(source);
    let ExprKind::ListComprehension(comprehension) = &expression(&result[0], 0).kind else {
        panic!()
    };
    assert!(matches!(comprehension.list.kind, ExprKind::List(_)));
    assert_missing_right(
        comprehension.projection.as_ref().unwrap(),
        source.rfind(']').unwrap(),
    );
    assert!(matches!(
        expression(&result[0], 1).kind,
        ExprKind::Literal(_)
    ));
}

#[test]
fn projection_modifiers_retain_missing_operands() {
    for modifier in ["ORDER BY", "SKIP", "OFFSET", "LIMIT"] {
        let source = format!("RETURN 1 {modifier} 2 +");
        let result = clauses(&source);
        let ClauseKind::Return(clause) = &result[0].kind else {
            panic!()
        };
        let projection = &clause.projection;
        let operand = match modifier {
            "ORDER BY" => &projection.order_by.as_ref().unwrap().items[0].expression,
            "SKIP" => projection.skip.as_ref().unwrap(),
            "OFFSET" => projection.offset.as_ref().unwrap(),
            "LIMIT" => projection.limit.as_ref().unwrap(),
            _ => unreachable!(),
        };
        assert_missing_right(operand, source.len());
        assert!(matches!(
            expression(&result[0], 0).kind,
            ExprKind::Literal(_)
        ));
    }
}

proptest::proptest! {
    #[test]
    fn missing_operand_keeps_valid_siblings(
        left in -1000i32..1000,
        right in -1000i32..1000,
        space in "[ \\t\\n]{1,4}",
    ) {
        let source = format!("RETURN [{left} +{space}, {right}]");
        let result = clauses(&source);
        let ExprKind::List(items) = &expression(&result[0], 0).kind else { panic!() };
        proptest::prop_assert_eq!(items.len(), 2);
        let ExprKind::Binary { left: operand, right: error, .. } = &items[0].kind else { panic!() };
        proptest::prop_assert_eq!(&source[operand.span.start..operand.span.end], left.to_string());
        proptest::prop_assert!(matches!(error.kind, ExprKind::Error(_)));
        proptest::prop_assert_eq!(error.span, Span::empty(source.find(',').unwrap()));
        proptest::prop_assert_eq!(&source[items[1].span.start..items[1].span.end], right.to_string());
    }
}
