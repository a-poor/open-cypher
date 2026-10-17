use open_cypher::{
    Span,
    ast::{
        visit::{self, Visitor},
        *,
    },
    parse, parse_recovering,
};

#[derive(Default)]
struct Collector<'ast> {
    expressions: Vec<&'ast Expr>,
    errors: Vec<Span>,
    clauses: Vec<Span>,
}

impl<'ast> Visitor<'ast> for Collector<'ast> {
    fn visit_expr(&mut self, node: &'ast Expr) {
        self.expressions.push(node);
        visit::walk_expr(self, node);
    }
    fn visit_clause(&mut self, node: &'ast Clause) {
        self.clauses.push(node.span);
        visit::walk_clause(self, node);
    }
    fn visit_error(&mut self, span: Span) {
        self.errors.push(span);
    }
}

#[test]
fn preorder_borrows_nodes_and_walks_union_and_nested_queries() {
    let source = "RETURN 1 + 2 UNION ALL RETURN EXISTS { RETURN 3 }";
    let parsed = parse(source).unwrap();
    let mut visitor = Collector::default();
    let dynamic: &mut dyn Visitor<'_> = &mut visitor;
    dynamic.visit_program(&parsed.program);
    let texts: Vec<_> = visitor
        .expressions
        .iter()
        .map(|expr| &source[expr.span.start..expr.span.end])
        .collect();
    assert_eq!(texts, ["1 + 2", "1", "2", "EXISTS { RETURN 3 }", "3"]);
    assert_eq!(visitor.clauses.len(), 3);
    assert!(visitor.errors.is_empty());
    let StatementKind::Query(statement) = &parsed.program.statements[0].kind else {
        panic!()
    };
    let QueryKind::Regular(query) = &statement.query.kind else {
        panic!()
    };
    let ClauseKind::Return(clause) = &query.head.kind.clauses[0].kind else {
        panic!()
    };
    let ProjectionItemKind::Expression { expression, .. } = &clause.projection.items[0].kind else {
        panic!()
    };
    assert!(std::ptr::eq(visitor.expressions[0], expression));
}

#[test]
fn overriding_a_hook_can_prune_subqueries() {
    #[derive(Default)]
    struct Pruned(usize);
    impl<'ast> Visitor<'ast> for Pruned {
        fn visit_expr(&mut self, node: &'ast Expr) {
            self.0 += 1;
            if !matches!(node.kind, ExprKind::Subquery(_)) {
                visit::walk_expr(self, node);
            }
        }
    }
    let parsed = parse("RETURN EXISTS { RETURN 1 }, 2").unwrap();
    let mut visitor = Pruned::default();
    visitor.visit_program(&parsed.program);
    assert_eq!(visitor.0, 2);
}

#[test]
fn errors_in_recovered_branches_and_nested_expressions_are_found() {
    let source = "MATCH (n) SET n. = 1 RETURN f(2 +) UNION RETURN";
    let parsed = parse_recovering(source).value.unwrap();
    let mut visitor = Collector::default();
    visitor.visit_program(&parsed.program);
    assert_eq!(
        visitor.errors,
        [
            Span::new(10, 20),
            Span::empty(source.rfind(')').unwrap()),
            Span::empty(source.len())
        ]
    );
}

#[test]
fn every_error_category_reports_its_own_span() {
    let mut visitor = Collector::default();
    let statement = Node::new(StatementKind::Error(ErrorNode), Span::new(1, 2));
    let query = Node::new(QueryKind::Error(ErrorNode), Span::new(2, 3));
    let clause = Node::new(ClauseKind::Error(ErrorNode), Span::new(3, 4));
    let expression = Node::new(ExprKind::Error(ErrorNode), Span::empty(4));
    let factor = Node::new(PathFactorKind::Error(ErrorNode), Span::new(5, 6));
    let label = Node::new(LabelExpressionKind::Error(ErrorNode), Span::new(6, 7));
    visitor.visit_statement(&statement);
    visitor.visit_query(&query);
    visitor.visit_clause(&clause);
    visitor.visit_expr(&expression);
    visitor.visit_path_factor(&factor);
    visitor.visit_label_expression(&label);
    assert_eq!(
        visitor.errors,
        [
            statement.span,
            query.span,
            clause.span,
            expression.span,
            factor.span,
            label.span
        ]
    );
}

#[cfg(feature = "serde")]
#[test]
fn traversal_matches_independent_serialized_tree_inventory() {
    // Serde supplies an independent structural oracle, including expressions
    // nested in patterns, updates, projections, comprehensions and subqueries.
    fn inventory(value: &serde_json::Value, expressions: &mut Vec<Span>, errors: &mut Vec<Span>) {
        match value {
            serde_json::Value::Object(object) => {
                if let (Some(kind), Some(span)) = (object.get("kind"), object.get("span")) {
                    if kind.get("Literal").is_some() {
                        expressions.push(serde_json::from_value(span.clone()).unwrap());
                    }
                    if kind.get("Error").is_some() {
                        errors.push(serde_json::from_value(span.clone()).unwrap());
                    }
                }
                for child in object.values() {
                    inventory(child, expressions, errors);
                }
            }
            serde_json::Value::Array(children) => {
                for child in children {
                    inventory(child, expressions, errors);
                }
            }
            _ => {}
        }
    }
    let mut sources: Vec<String> = include_str!("../spec/generated/TCK_SYNTAX.jsonl")
        .lines()
        .filter_map(|line| {
            let record: serde_json::Value = serde_json::from_str(line).unwrap();
            (record["record"] == "query").then(|| record["source"].as_str().unwrap().to_owned())
        })
        .collect();
    sources.extend(
        [
            "RETURN f(1 +, [2, , 3]) UNION RETURN",
            "MATCH (n) SET n. = 1 RETURN n",
            "RETURN CASE WHEN true THEN (1 +) ELSE 2 END",
            "RETURN [x IN [1,2] | x +]",
            "RETURN 1 ORDER BY 2 +",
        ]
        .map(str::to_owned),
    );
    for source in sources {
        let parsed = parse_recovering(&source).value.unwrap();
        let mut visitor = Collector::default();
        visitor.visit_program(&parsed.program);
        let mut expected_literals = Vec::new();
        let mut expected_errors = Vec::new();
        inventory(
            &serde_json::to_value(&parsed.program).unwrap(),
            &mut expected_literals,
            &mut expected_errors,
        );
        let mut actual_literals: Vec<_> = visitor
            .expressions
            .iter()
            .filter(|expr| matches!(expr.kind, ExprKind::Literal(_)))
            .map(|expr| expr.span)
            .collect();
        let key = |span: &Span| (span.start, span.end);
        actual_literals.sort_by_key(key);
        expected_literals.sort_by_key(key);
        expected_errors.sort_by_key(key);
        visitor.errors.sort_by_key(key);
        assert_eq!(
            actual_literals, expected_literals,
            "literal traversal: {source}"
        );
        assert_eq!(visitor.errors, expected_errors, "error traversal: {source}");
    }
}

#[test]
fn hand_built_extension_clauses_and_nested_label_errors_are_walked() {
    // Public ASTs may be constructed by callers, including variants that the
    // current strict grammar does not emit. They must be traversed too.
    let span = Span::empty(0);
    let error = || Node::new(ExprKind::Error(ErrorNode), span);
    let name = Node::new(Identifier::new("n", false), span);
    let projection = || ProjectionClause {
        quantifier: None,
        items: vec![Node::new(
            ProjectionItemKind::Expression {
                expression: error(),
                alias: None,
            },
            span,
        )],
        order_by: None,
        offset: Some(error()),
        skip: Some(error()),
        limit: Some(error()),
        span,
    };
    let pattern = Pattern {
        parts: vec![Node::new(
            PatternPartKind {
                binding: None,
                selector: None,
                path: Node::new(
                    PathPatternKind {
                        factors: vec![Node::new(
                            PathFactorKind::QuantifiedNode {
                                pattern: NodePattern {
                                    variable: None,
                                    labels: Some(Node::new(
                                        LabelExpressionKind::Not(Box::new(Node::new(
                                            LabelExpressionKind::Error(ErrorNode),
                                            span,
                                        ))),
                                        span,
                                    )),
                                    properties: Some(Box::new(error())),
                                    where_clause: Some(Box::new(error())),
                                },
                                quantifier: Quantifier {
                                    kind: QuantifierKind::OneOrMore,
                                    span,
                                },
                            },
                            span,
                        )],
                    },
                    span,
                ),
            },
            span,
        )],
        span,
    };
    let cases = [
        (ClauseKind::Use(UseClause { graph: error() }), 1),
        (
            ClauseKind::Let(LetClause {
                bindings: vec![Binding {
                    variable: name.clone(),
                    expression: error(),
                    span,
                }],
            }),
            1,
        ),
        (ClauseKind::Insert(InsertClause { pattern }), 3),
        (ClauseKind::Select(projection()), 4),
        (ClauseKind::Next(projection()), 4),
        (ClauseKind::Filter(FilterClause { predicate: error() }), 1),
        (
            ClauseKind::LoadCsv(LoadCsvClause {
                with_headers: false,
                source: error(),
                variable: name,
                field_terminator: None,
            }),
            1,
        ),
        (
            ClauseKind::Call(CallClause {
                optional: false,
                target: CallTarget::Subquery {
                    scope: None,
                    query: Box::new(Node::new(QueryKind::Error(ErrorNode), span)),
                    in_transactions: Some(Box::new(InTransactions {
                        rows: Some(error()),
                        on_error: None,
                        report_status_as: None,
                        span,
                    })),
                },
                yield_clause: None,
            }),
            2,
        ),
        (ClauseKind::Finish, 0),
    ];
    for (kind, count) in cases {
        let clause = Node::new(kind, span);
        let mut visitor = Collector::default();
        visitor.visit_clause(&clause);
        assert_eq!(visitor.errors.len(), count, "{:?}", clause.kind);
    }
}
