use open_cypher::{
    Span,
    ast::{ClauseKind, QueryKind, RegularQuery, StatementKind, UnionOperator},
    lex, parse, parse_recovering,
};
use proptest::prelude::*;

proptest! {
    #[test]
    fn one_damaged_branch_preserves_other_branches(
        values in prop::collection::vec(-1000i32..1000, 2..8),
        broken in any::<usize>(),
        all in any::<bool>(),
        whitespace in "[ \\t\\n]{1,4}",
    ) {
        let broken = broken % values.len();
        let branches: Vec<_> = values.iter().enumerate().map(|(index, value)| {
            if index == broken { "RETURN 1 0".to_owned() }
            else { format!("RETURN{whitespace}{value}") }
        }).collect();
        let operator = if all { "UNION ALL" } else { "UNION DISTINCT" };
        let source = branches.join(&format!("{whitespace}{operator}{whitespace}"));
        let query = recovered(&source);
        prop_assert_eq!(query.unions.len(), values.len() - 1);
        for (index, branch) in std::iter::once(&query.head)
            .chain(query.unions.iter().map(|union| &union.query)).enumerate()
        {
            prop_assert_eq!(branch.kind.clauses.len(), 1);
            prop_assert_eq!(&source[branch.span.start..branch.span.end], &branches[index]);
            if index == broken {
                prop_assert!(matches!(branch.kind.clauses[0].kind, ClauseKind::Error(_)));
            } else {
                prop_assert!(matches!(branch.kind.clauses[0].kind, ClauseKind::Return(_)));
            }
        }
    }
}

fn recovered(source: &str) -> RegularQuery {
    assert!(parse(source).is_err(), "{source}");
    let outcome = parse_recovering(source);
    assert!(!outcome.diagnostics.is_empty());
    assert!(outcome.diagnostics.len() <= 32);
    let parsed = outcome.value.unwrap();
    assert_eq!(parsed.tokens, lex(source).tokens);
    let StatementKind::Query(statement) = &parsed.program.statements[0].kind else {
        panic!("expected recovered query: {source}");
    };
    let QueryKind::Regular(query) = &statement.query.kind else {
        panic!()
    };
    let mut previous_end = query.head.span.start;
    for (index, branch) in std::iter::once(&query.head)
        .chain(query.unions.iter().map(|union| &union.query))
        .enumerate()
    {
        if index > 0 {
            let operator = &query.unions[index - 1].operator;
            assert!(previous_end <= operator.span.start);
            assert!(operator.span.end <= branch.span.start);
        }
        let mut end = branch.span.start;
        for clause in &branch.kind.clauses {
            assert!(end <= clause.span.start);
            assert!(clause.span.start <= clause.span.end);
            assert!(clause.span.end <= branch.span.end);
            assert!(source.get(clause.span.start..clause.span.end).is_some());
            end = clause.span.end;
        }
        previous_end = branch.span.end;
    }
    query.clone()
}

#[test]
fn retains_valid_branch_and_broken_tail_with_exact_spans() {
    let source = "  RETURN 1 UNION RETURN;  ";
    let query = recovered(source);
    assert_eq!(query.head.span, Span::new(2, 10));
    assert_eq!(query.unions.len(), 1);
    assert_eq!(query.unions[0].operator.kind, UnionOperator::Default);
    assert_eq!(query.unions[0].operator.span, Span::new(11, 16));
    assert_eq!(query.unions[0].query.span, Span::new(17, 23));
    assert!(matches!(
        query.head.kind.clauses[0].kind,
        ClauseKind::Return(_)
    ));
    assert!(matches!(
        query.unions[0].query.kind.clauses[0].kind,
        ClauseKind::Return(_)
    ));
    let parsed = parse_recovering(source).value.unwrap();
    assert_eq!(parsed.program.span, Span::new(2, 24));
    let StatementKind::Query(statement) = &parsed.program.statements[0].kind else {
        panic!()
    };
    assert_eq!(statement.terminator, Some(Span::new(23, 24)));
}

#[test]
fn retains_quantifiers_and_recovers_first_middle_and_last_branches() {
    for source in [
        "RETURN 1 0 UNION ALL RETURN 2 UNION DISTINCT RETURN 3",
        "RETURN 1 UNION ALL RETURN 2 0 UNION DISTINCT RETURN 3",
        "RETURN 1 UNION ALL RETURN 2 UNION DISTINCT RETURN 3 0",
    ] {
        let query = recovered(source);
        assert_eq!(query.unions.len(), 2);
        assert_eq!(query.unions[0].operator.kind, UnionOperator::All);
        assert_eq!(query.unions[1].operator.kind, UnionOperator::Distinct);
        for union in &query.unions {
            assert_eq!(
                &source[union.operator.span.start..union.operator.span.end],
                if union.operator.kind == UnionOperator::All {
                    "UNION ALL"
                } else {
                    "UNION DISTINCT"
                }
            );
        }
        let kinds: Vec<_> = std::iter::once(&query.head)
            .chain(query.unions.iter().map(|u| &u.query))
            .map(|q| matches!(q.kind.clauses[0].kind, ClauseKind::Error(_)))
            .collect();
        assert_eq!(kinds.iter().filter(|&&error| error).count(), 1);
    }
}

#[test]
fn missing_branches_have_empty_error_spans() {
    for source in [
        "UNION RETURN 1",
        "RETURN 1 UNION",
        "RETURN 1 UNION ALL",
        "RETURN 1 UNION UNION RETURN 2",
    ] {
        let query = recovered(source);
        let branches: Vec<_> = std::iter::once(&query.head)
            .chain(query.unions.iter().map(|u| &u.query))
            .collect();
        let missing: Vec<_> = branches.into_iter().filter(|q| q.span.is_empty()).collect();
        assert_eq!(missing.len(), 1, "{source}");
        assert_eq!(missing[0].kind.clauses.len(), 1);
        assert_eq!(missing[0].kind.clauses[0].span, missing[0].span);
        assert!(matches!(
            missing[0].kind.clauses[0].kind,
            ClauseKind::Error(_)
        ));
    }
}

#[test]
fn clause_recovery_and_terminal_return_are_local_to_each_branch() {
    let source = "MATCH (n) SET n. = 1 RETURN n UNION MATCH (m) SET m. = 1 RETURN m";
    let query = recovered(source);
    assert_eq!(query.unions.len(), 1);
    for branch in [&query.head, &query.unions[0].query] {
        assert_eq!(branch.kind.clauses.len(), 3);
        assert!(matches!(branch.kind.clauses[0].kind, ClauseKind::Match(_)));
        assert!(matches!(branch.kind.clauses[1].kind, ClauseKind::Error(_)));
        assert!(matches!(branch.kind.clauses[2].kind, ClauseKind::Return(_)));
    }
    let query = recovered("RETURN 1 MATCH (n) UNION RETURN 2");
    assert_eq!(query.head.kind.clauses.len(), 2);
    assert!(matches!(
        query.head.kind.clauses[1].kind,
        ClauseKind::Error(_)
    ));
    assert!(matches!(
        query.unions[0].query.kind.clauses[0].kind,
        ClauseKind::Return(_)
    ));
}

#[test]
fn contextual_names_are_consumed_by_clauses_before_splitting() {
    for prefix in [
        "WITH 1 AS union",
        "WITH union",
        "WITH n.union",
        "CALL union()",
        "RETURN union",
        "RETURN 1 + union",
    ] {
        let source = format!("{prefix} UNION RETURN");
        let query = recovered(&source);
        assert_eq!(query.unions.len(), 1, "{source}");
        assert_eq!(query.head.kind.clauses.len(), 1);
        assert_eq!(query.head.kind.clauses[0].span, Span::new(0, prefix.len()));
        assert!(!matches!(
            query.head.kind.clauses[0].kind,
            ClauseKind::Error(_)
        ));
    }
    let query = recovered("MATCH (n) SET n.x = WITH n.union RETURN union");
    assert!(query.unions.is_empty());
    assert_eq!(query.head.kind.clauses.len(), 4);
}

#[test]
fn nested_union_is_not_promoted_and_unclosed_delimiters_block_splitting() {
    for source in [
        "MATCH (n) WITH [1, UNION RETURN n",
        "MATCH (n) WITH {x: UNION RETURN n}",
        "MATCH (n) RETURN EXISTS { RETURN 1 UNION RETURN n. }",
    ] {
        let query = recovered(source);
        assert!(query.unions.is_empty(), "{source}");
        assert!(matches!(
            query.head.kind.clauses.last().unwrap().kind,
            ClauseKind::Error(_)
        ));
    }
}

#[test]
fn explicit_name_positions_do_not_split_even_in_broken_clauses() {
    for broken in [
        "WITH 1 AS union 0",
        "SET n.union = 1 0",
        "CALL union +",
        "WITH n:union 0",
    ] {
        let source = format!("MATCH (n) {broken} RETURN n UNION RETURN");
        let query = recovered(&source);
        assert_eq!(query.unions.len(), 1, "{source}");
        assert_eq!(query.head.kind.clauses.len(), 3, "{source}");
        assert!(matches!(
            query.head.kind.clauses[1].kind,
            ClauseKind::Error(_)
        ));
    }
}

#[test]
fn union_in_comments_strings_and_escaped_names_is_not_a_boundary() {
    let query =
        recovered("WITH 'UNION' AS `union` /* UNION */ SET n.x = RETURN `union` UNION ALL RETURN");
    assert_eq!(query.unions.len(), 1);
    assert_eq!(query.head.kind.clauses.len(), 3);
    assert_eq!(query.unions[0].operator.kind, UnionOperator::All);
}

#[cfg(feature = "serde")]
#[test]
fn recovered_union_tree_round_trips_through_json() {
    let outcome = parse_recovering("RETURN 1 UNION ALL UNION DISTINCT RETURN");
    let encoded = serde_json::to_string(&outcome).unwrap();
    let decoded = serde_json::from_str(&encoded).unwrap();
    assert_eq!(outcome, decoded);
}

#[test]
fn exhausted_budget_leaves_one_unsplit_suffix() {
    let source = format!("MATCH (n) {}UNION RETURN n", "SET n.x = ".repeat(200));
    let query = recovered(&source);
    assert!(query.unions.is_empty());
    assert_eq!(query.head.kind.clauses.len(), 2);
    assert_eq!(query.head.kind.clauses[1].span, Span::new(10, source.len()));
    let source = format!("RETURN 1 {}RETURN 2", "UNION ".repeat(80));
    let query = recovered(&source);
    assert!(query.unions.len() < 128);
    let tail = &query.unions.last().unwrap().query;
    assert_eq!(tail.kind.clauses.len(), 1);
    assert!(matches!(tail.kind.clauses[0].kind, ClauseKind::Error(_)));
    assert_eq!(tail.span.end, source.len());
}

#[test]
fn unrecoverable_and_multi_statement_inputs_keep_whole_input_error() {
    for source in [
        "UNION",
        "RETURN 1 0 UNION RETURN 1 0",
        "RETURN 1; UNION RETURN 2",
        "RETURN 1 UNION RETURN 2; RETURN 3",
    ] {
        let outcome = parse_recovering(source);
        assert!(!outcome.diagnostics.is_empty());
        assert!(
            matches!(
                outcome.value.unwrap().program.statements[0].kind,
                StatementKind::Error(_)
            ),
            "{source}"
        );
    }
}
