use open_cypher::{
    Span,
    ast::{
        visit::{self, Visitor},
        *,
    },
    lex, parse, parse_recovering,
};

#[derive(Default)]
struct Inventory<'a> {
    expressions: Vec<&'a Expr>,
    errors: Vec<Span>,
    fallback: Vec<Span>,
}

impl<'a> Visitor<'a> for Inventory<'a> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        self.expressions.push(expr);
        visit::walk_expr(self, expr);
    }
    fn visit_clause(&mut self, clause: &'a Clause) {
        if matches!(clause.kind, ClauseKind::Error(_)) {
            self.fallback.push(clause.span);
        }
        visit::walk_clause(self, clause);
    }
    fn visit_statement(&mut self, statement: &'a Statement) {
        if matches!(statement.kind, StatementKind::Error(_)) {
            self.fallback.push(statement.span);
        }
        visit::walk_statement(self, statement);
    }
    fn visit_error(&mut self, span: Span) {
        self.errors.push(span);
    }
}

fn recovered(source: &str, markers: &[&str]) -> open_cypher::ParsedProgram {
    let strict = parse(source).unwrap_err();
    let outcome = parse_recovering(source);
    assert!(!outcome.diagnostics.is_empty());
    assert!(outcome.diagnostics.len() <= 32);
    assert_eq!(outcome.diagnostics, strict.diagnostics());
    let parsed = outcome.value.unwrap();
    assert_eq!(parsed.tokens, lex(source).tokens);
    let mut inventory = Inventory::default();
    inventory.visit_program(&parsed.program);
    assert!(
        inventory.fallback.is_empty(),
        "{source}: {:?}",
        inventory.fallback
    );
    let expected: Vec<_> = markers
        .iter()
        .map(|marker| {
            Span::empty(if marker.is_empty() {
                source.len()
            } else {
                source.find(marker).unwrap()
            })
        })
        .collect();
    assert_eq!(inventory.errors, expected, "{source}");
    parsed
}

#[test]
fn case_boundaries_keep_all_arms_and_surrounding_expressions() {
    for (source, marker) in [
        ("RETURN CASE WHEN 1 + THEN 2 ELSE 3 END, 4", "THEN"),
        ("RETURN CASE WHEN true THEN 1 + ELSE 2 END, 3", "ELSE"),
        (
            "RETURN CASE WHEN true THEN 1 + WHEN false THEN 2 ELSE 3 END, 4",
            "WHEN false",
        ),
        ("RETURN CASE WHEN true THEN 1 ELSE 2 + END", "END"),
        ("RETURN CASE WHEN true THEN 1 + END", "END"),
    ] {
        let parsed = recovered(source, &[marker]);
        let mut inventory = Inventory::default();
        inventory.visit_program(&parsed.program);
        assert_eq!(
            inventory
                .expressions
                .iter()
                .filter(|e| matches!(e.kind, ExprKind::Case(_)))
                .count(),
            1
        );
    }
}

#[test]
fn comprehension_input_predicate_and_projection_recover() {
    for (source, markers) in [
        ("RETURN [x IN [1,2] + WHERE x > 0 | x]", vec!["WHERE"]),
        ("RETURN [x IN [1,2] WHERE x > | x]", vec!["|"]),
        ("RETURN [x IN [1,2] WHERE | x]", vec!["|"]),
    ] {
        let parsed = recovered(source, &markers);
        let mut inventory = Inventory::default();
        inventory.visit_program(&parsed.program);
        assert!(
            inventory
                .expressions
                .iter()
                .any(|e| matches!(e.kind, ExprKind::ListComprehension(_)))
        );
    }
}

#[test]
fn comprehension_predicate_and_projection_share_repairs() {
    for source in [
        "RETURN [x IN [1,2] WHERE x > | x +]",
        "RETURN [x IN [1,2] | ]",
    ] {
        let parsed = parse_recovering(source).value.unwrap();
        let mut inventory = Inventory::default();
        inventory.visit_program(&parsed.program);
        assert!(inventory.fallback.is_empty(), "{source}");
        let mut expected = vec![];
        if source.contains("WHERE") {
            expected.push(Span::empty(source.find('|').unwrap()));
        }
        expected.push(Span::empty(source.rfind(']').unwrap()));
        assert_eq!(inventory.errors, expected, "{source}");
    }
}

#[test]
fn valid_contextual_operands_win_before_and_after_other_errors() {
    for name in ["then", "else", "end", "when", "where"] {
        let expression = format!("CASE WHEN true THEN 1 + {name} ELSE 2 END");
        let valid = format!("RETURN {expression}");
        let strict = parse(&valid).unwrap();
        let recovering = parse_recovering(&valid);
        assert!(recovering.diagnostics.is_empty(), "{valid}");
        assert_eq!(recovering.value.unwrap(), strict);

        let source = format!("WITH {expression} AS value SET n. = 1 RETURN {expression}");
        let parsed = parse_recovering(&source).value.unwrap();
        let mut inventory = Inventory::default();
        inventory.visit_program(&parsed.program);
        assert_eq!(inventory.fallback.len(), 1, "{source}");
        assert_eq!(inventory.errors, inventory.fallback, "{source}");
        assert_eq!(
            inventory
                .expressions
                .iter()
                .filter(|e| matches!(e.kind, ExprKind::Case(_)))
                .count(),
            2,
            "{source}"
        );
    }
}

#[test]
fn nested_cases_and_union_branches_preserve_error_locations() {
    let source = "RETURN CASE WHEN true THEN CASE WHEN false THEN 1 + ELSE 2 END ELSE 3 + END UNION RETURN 4";
    let parsed = recovered(source, &["ELSE 2", "END UNION"]);
    let mut inventory = Inventory::default();
    inventory.visit_program(&parsed.program);
    assert_eq!(
        inventory
            .expressions
            .iter()
            .filter(|e| matches!(e.kind, ExprKind::Case(_)))
            .count(),
        2
    );
    assert!(matches!(
        inventory.expressions.last().unwrap().kind,
        ExprKind::Literal(_)
    ));
}

#[test]
fn contextual_repairs_keep_the_four_insertion_limit() {
    for count in [4, 5] {
        let cases = vec!["CASE WHEN true THEN 1 + ELSE 2 END"; count].join(", ");
        let source = format!("MATCH (n) RETURN [{cases}]");
        let outcome = parse_recovering(&source);
        assert!(!outcome.diagnostics.is_empty());
        assert!(outcome.diagnostics.len() <= 32);
        let parsed = outcome.value.unwrap();
        let mut inventory = Inventory::default();
        inventory.visit_program(&parsed.program);
        if count == 4 {
            assert!(inventory.fallback.is_empty());
            assert_eq!(inventory.errors.len(), 4);
            assert!(inventory.errors.iter().all(|span| span.is_empty()));
        } else {
            assert_eq!(inventory.fallback.len(), 1);
            assert_eq!(inventory.errors, inventory.fallback);
        }
    }
}

#[test]
fn structural_keywords_do_not_invent_case_delimiters_or_bindings() {
    for broken in [
        "RETURN 1 + ELSE 2",
        "RETURN CASE WHEN true THEN 1 + ELSE 2",
        "RETURN [IN [1,2] WHERE x > | x]",
        "RETURN [x IN [1,2] WHERE x > | x +",
    ] {
        let source = format!("MATCH (n) {broken}");
        let parsed = parse_recovering(&source).value.unwrap();
        let mut inventory = Inventory::default();
        inventory.visit_program(&parsed.program);
        assert_eq!(inventory.fallback.len(), 1, "{source}");
        assert_eq!(inventory.errors, inventory.fallback, "{source}");
    }
}

proptest::proptest! {
    #[test]
    fn case_recovery_keeps_branches_and_whitespace_offsets(
        left in 0u16..1000,
        right in 0u16..1000,
        space in "[ \\t\\n]{1,4}",
    ) {
        let source = format!("RETURN CASE WHEN true THEN {left} +{space}ELSE {right} END");
        let parsed = recovered(&source, &["ELSE"]);
        let mut inventory = Inventory::default();
        inventory.visit_program(&parsed.program);
        let case = inventory.expressions.iter().find_map(|expr| match &expr.kind { ExprKind::Case(case) => Some(case), _ => None }).unwrap();
        let ExprKind::Binary { left: operand, right: missing, .. } = &case.alternatives[0].then.kind else { panic!() };
        proptest::prop_assert_eq!(&source[operand.span.start..operand.span.end], left.to_string());
        proptest::prop_assert!(matches!(missing.kind, ExprKind::Error(_)));
        let other = case.else_expression.as_ref().unwrap();
        proptest::prop_assert_eq!(&source[other.span.start..other.span.end], right.to_string());
    }
}
