#![no_main]

use libfuzzer_sys::fuzz_target;
use open_cypher::{
    DiagnosticCode, Span,
    ast::{ClauseKind, Expr, ExprKind, ProjectionItemKind, QueryKind, StatementKind},
    lex, parse_recovering,
};

fn check_span(source: &str, span: Span, parent: Span) {
    assert!(span.start <= span.end);
    assert!(parent.start <= span.start && span.end <= parent.end);
    assert!(source.is_char_boundary(span.start));
    assert!(source.is_char_boundary(span.end));
}

const MAX_RECOVERY_DIAGNOSTICS: usize = 32;

fn check_expression(source: &str, expression: &Expr, parent: Span) {
    check_span(source, expression.span, parent);
    let check = |child: &Expr| check_expression(source, child, expression.span);
    match &expression.kind {
        ExprKind::Error(_) => assert!(expression.span.is_empty()),
        ExprKind::Binary { left, right, .. } => {
            check(left);
            check(right);
        }
        ExprKind::Unary { operand, .. } | ExprKind::Parenthesized(operand) => check(operand),
        ExprKind::List(items) => items.iter().for_each(check),
        ExprKind::Function(function) => function.arguments.iter().for_each(check),
        _ => {}
    }
}

fuzz_target!(|data: &[u8]| {
    let source = String::from_utf8_lossy(data);
    let source = source.as_ref();

    let outcome = parse_recovering(source);
    assert!(
        outcome.diagnostics.len() <= MAX_RECOVERY_DIAGNOSTICS,
        "recovery emitted more than {MAX_RECOVERY_DIAGNOSTICS} diagnostics"
    );
    for diagnostic in &outcome.diagnostics {
        assert_ne!(diagnostic.code, DiagnosticCode::Internal);
        assert!(!diagnostic.message.contains("error expression"));
        assert!(diagnostic.primary_span.start <= diagnostic.primary_span.end);
        assert!(diagnostic.primary_span.end <= source.len());
        assert!(source.is_char_boundary(diagnostic.primary_span.start));
        assert!(source.is_char_boundary(diagnostic.primary_span.end));
    }
    {
        let parsed = outcome
            .value
            .as_ref()
            .expect("recovery always returns a root");
        assert_eq!(parsed.tokens, lex(source).tokens);
        check_span(source, parsed.program.span, Span::new(0, source.len()));
        for statement in &parsed.program.statements {
            check_span(source, statement.span, parsed.program.span);
            if let StatementKind::Query(statement) = &statement.kind {
                check_span(source, statement.query.span, parsed.program.span);
                if let QueryKind::Regular(query) = &statement.query.kind {
                    let mut previous_end = query.head.span.end;
                    for branch in &query.unions {
                        check_span(source, branch.operator.span, statement.query.span);
                        assert!(previous_end <= branch.operator.span.start);
                        assert!(branch.operator.span.end <= branch.query.span.start);
                        previous_end = branch.query.span.end;
                    }
                    for single in std::iter::once(&query.head)
                        .chain(query.unions.iter().map(|branch| &branch.query))
                    {
                        check_span(source, single.span, statement.query.span);
                        let mut previous_end = single.span.start;
                        for clause in &single.kind.clauses {
                            check_span(source, clause.span, single.span);
                            assert!(previous_end <= clause.span.start);
                            previous_end = clause.span.end;
                            let projection = match &clause.kind {
                                ClauseKind::Return(clause) => Some(&clause.projection),
                                ClauseKind::With(clause) => Some(&clause.projection),
                                _ => None,
                            };
                            if let Some(projection) = projection {
                                for item in &projection.items {
                                    if let ProjectionItemKind::Expression { expression, .. } =
                                        &item.kind
                                    {
                                        check_expression(source, expression, item.span);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    std::hint::black_box(outcome.value);
});
