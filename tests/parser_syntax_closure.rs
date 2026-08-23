use open_cypher::ParsedProgram;
use open_cypher::ast::{
    ClauseKind, Expr, ExprKind, LiteralKind, MapProjectionItemKind, ParameterName, PathFactorKind,
    ProjectionItemKind, QueryKind, StatementKind, SubqueryExpressionKind,
};
use open_cypher::{DiagnosticCode, parse, parse_recovering};

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
        panic!("expected an expression projection")
    };
    expression
}

#[test]
fn non_reserved_keywords_are_names_only_when_the_grammar_needs_names() {
    for source in [
        "MATCH (create:Order) RETURN create.order",
        "RETURN exists.order AS merge",
        "RETURN exists(n.name)",
        "WITH 1 AS set RETURN set",
        "RETURN order AS value ORDER BY order",
    ] {
        parse(source).unwrap_or_else(|errors| {
            panic!("contextual keyword name did not parse: {source}\n{errors:#?}")
        });
    }

    const NON_RESERVED_KEYWORDS: [&str; 62] = [
        "allshortestpaths",
        "all",
        "and",
        "any",
        "as",
        "asc",
        "ascending",
        "by",
        "call",
        "case",
        "contains",
        "count",
        "create",
        "delete",
        "desc",
        "descending",
        "detach",
        "distinct",
        "else",
        "end",
        "ends",
        "exists",
        "false",
        "group",
        "groups",
        "in",
        "inf",
        "infinity",
        "is",
        "limit",
        "match",
        "merge",
        "nan",
        "none",
        "not",
        "null",
        "offset",
        "on",
        "optional",
        "or",
        "order",
        "path",
        "paths",
        "reduce",
        "remove",
        "return",
        "set",
        "shortest",
        "shortestpath",
        "single",
        "skip",
        "starts",
        "then",
        "trim",
        "true",
        "union",
        "unwind",
        "when",
        "where",
        "with",
        "xor",
        "yield",
    ];
    for keyword in NON_RESERVED_KEYWORDS {
        let source = format!("RETURN {keyword} AS value");
        parse(&source).unwrap_or_else(|errors| {
            panic!("contextual expression name did not parse: {source}\n{errors:#?}")
        });
        let source = format!("UNWIND {keyword} AS value RETURN value");
        parse(&source).unwrap_or_else(|errors| {
            panic!("contextual UNWIND name did not parse: {source}\n{errors:#?}")
        });

        let source = format!("RETURN ({keyword})-->(x)");
        parse(&source).unwrap_or_else(|errors| {
            panic!("contextual pattern name did not parse: {source}\n{errors:#?}")
        });

        let source = format!("RETURN [({keyword})-->(x) | {keyword}]");
        parse(&source).unwrap_or_else(|errors| {
            panic!("contextual pattern-comprehension name did not parse: {source}\n{errors:#?}")
        });
    }

    for source in [
        "RETURN [order = (a)-->(b) | order]",
        "RETURN [where = (a)-->(b) | where]",
        "RETURN (x:ORDER)-->(y)",
        "RETURN [(x:ORDER)-->(y) | x]",
        "RETURN (x {ORDER: 1})-->(y)",
        "RETURN [(x {ORDER: 1})-->(y) | x]",
        "RETURN (x)-->(y {CREATE: order})",
        "RETURN (null)-->(x)",
        "RETURN (x:NULL)-->(y)",
        "RETURN (x {NULL: 1})-->(y)",
    ] {
        parse(source).unwrap_or_else(|errors| {
            panic!("nested contextual name did not parse: {source}\n{errors:#?}")
        });
    }

    for source in [
        "WITH ALL RETURN 1",
        "WITH ALL RETURN ALL",
        "WITH DISTINCT RETURN 1",
        "WITH DISTINCT RETURN DISTINCT",
        "WITH ALL WHERE true RETURN 1",
        "WITH DISTINCT WHERE true RETURN 1",
        "WITH ALL ORDER BY 1 RETURN 1",
        "WITH DISTINCT ORDER BY 1 RETURN 1",
        "RETURN ALL UNION RETURN 1",
        "RETURN DISTINCT UNION RETURN 1",
        "RETURN ALL ORDER BY 1",
        "RETURN DISTINCT ORDER BY 1",
        "RETURN ALL OFFSET 1",
        "RETURN DISTINCT LIMIT 1",
    ] {
        parse(source).unwrap_or_else(|errors| {
            panic!("contextual WITH item did not parse: {source}\n{errors:#?}")
        });
    }

    let parsed = parse("RETURN 0, order, NULL, TRUE, FALSE, count")
        .expect("contextual conversion must preserve neighboring literals");
    let StatementKind::Query(statement) = &parsed.program.statements[0].kind else {
        panic!("expected query")
    };
    let QueryKind::Regular(query) = &statement.query.kind else {
        panic!("expected regular query")
    };
    let ClauseKind::Return(return_clause) = &query.head.kind.clauses[0].kind else {
        panic!("expected RETURN")
    };
    let ProjectionItemKind::Expression { expression, .. } = &return_clause.projection.items[1].kind
    else {
        panic!("expected expression")
    };
    assert!(matches!(&expression.kind, ExprKind::Variable(name) if name.kind.text == "order"));
    let ProjectionItemKind::Expression { expression, .. } = &return_clause.projection.items[2].kind
    else {
        panic!("expected expression")
    };
    assert!(
        matches!(expression.kind, ExprKind::Literal(ref literal) if literal.kind == LiteralKind::Null)
    );
    for (index, expected) in [(3, true), (4, false)] {
        let ProjectionItemKind::Expression { expression, .. } =
            &return_clause.projection.items[index].kind
        else {
            panic!("expected expression")
        };
        assert!(matches!(
            expression.kind,
            ExprKind::Literal(ref literal)
                if literal.kind == LiteralKind::Boolean(expected)
        ));
    }
    let ProjectionItemKind::Expression { expression, .. } = &return_clause.projection.items[5].kind
    else {
        panic!("expected expression")
    };
    assert!(matches!(&expression.kind, ExprKind::Variable(name) if name.kind.text == "count"));

    let mut alias_source = String::from("RETURN ");
    let mut expression_source = String::from("RETURN ");
    let mut index = 0usize;
    while expression_source.len() < 63 * 1024 {
        if index != 0 {
            alias_source.push_str(", ");
            expression_source.push_str(", ");
        }
        let keyword = NON_RESERVED_KEYWORDS[index % NON_RESERVED_KEYWORDS.len()];
        alias_source.push_str("0 AS ");
        alias_source.push_str(keyword);
        expression_source.push_str(keyword);
        expression_source.push_str(" AS x");
        expression_source.push_str(&index.to_string());
        index += 1;
    }
    parse(&alias_source).expect("contextual aliases must not have a fixed retry ceiling");
    parse(&expression_source)
        .expect("contextual expression names must not cause unbounded full-parser retries");
    assert!(
        parse(&format!("{alias_source}, +")).is_err(),
        "a large malformed contextual-alias query must still be rejected"
    );
    assert!(
        parse(&format!("{expression_source}, +")).is_err(),
        "a large malformed contextual-expression query must still be rejected"
    );
}

#[test]
fn contextual_fallback_preserves_aliased_literal_semantics() {
    let parsed = parse(
        "RETURN NULL AS n, TRUE AS t, FALSE AS f, INF AS i, INFINITY AS infinity, NAN AS nan, count",
    )
    .expect("native literals and a trailing contextual name should coexist");
    let StatementKind::Query(statement) = &parsed.program.statements[0].kind else {
        panic!("expected query")
    };
    let QueryKind::Regular(query) = &statement.query.kind else {
        panic!("expected regular query")
    };
    let ClauseKind::Return(return_clause) = &query.head.kind.clauses[0].kind else {
        panic!("expected RETURN")
    };

    let expressions = return_clause
        .projection
        .items
        .iter()
        .map(|item| {
            let ProjectionItemKind::Expression { expression, .. } = &item.kind else {
                panic!("expected expression projection")
            };
            expression
        })
        .collect::<Vec<_>>();

    assert!(matches!(
        expressions[0].kind,
        ExprKind::Literal(ref literal) if literal.kind == LiteralKind::Null
    ));
    for (index, expected) in [(1, true), (2, false)] {
        assert!(matches!(
            expressions[index].kind,
            ExprKind::Literal(ref literal)
                if literal.kind == LiteralKind::Boolean(expected)
        ));
    }
    for (index, expected) in [(3, "INF"), (4, "INFINITY"), (5, "NAN")] {
        assert!(matches!(
            expressions[index].kind,
            ExprKind::Literal(ref literal)
                if matches!(&literal.kind, LiteralKind::Float(value) if value.text == expected)
        ));
    }
    assert!(matches!(
        &expressions[6].kind,
        ExprKind::Variable(name) if name.kind.text == "count"
    ));
}

#[test]
fn contextual_expression_fuzz_seed_remains_a_valid_query() {
    let source = include_str!("fixtures/regressions/parse_strict/contextual_expression_names");
    parse(source).unwrap_or_else(|errors| {
        panic!("contextual-expression fuzz seed must remain valid: {errors:#?}")
    });
}

#[test]
fn dotted_procedure_calls_are_not_folded_as_expression_function_names() {
    parse("CALL test.db.labels() YIELD label RETURN label")
        .expect("a dotted procedure call should retain its qualified procedure name");
}

#[test]
fn map_projection_items_build_typed_ast_nodes() {
    let parsed = parse("MATCH (n) RETURN n{.name, alias: n.name, n, .*} AS projected")
        .expect("map projection syntax should parse");
    let ExprKind::MapProjection(projection) = &first_return_expression(&parsed).kind else {
        panic!("expected a map projection")
    };
    assert_eq!(projection.items.len(), 4);
    assert!(matches!(
        projection.items[0].kind,
        MapProjectionItemKind::Property(_)
    ));
    assert!(matches!(
        projection.items[1].kind,
        MapProjectionItemKind::Entry(_)
    ));
    assert!(matches!(
        projection.items[2].kind,
        MapProjectionItemKind::Variable(_)
    ));
    assert!(matches!(
        projection.items[3].kind,
        MapProjectionItemKind::AllProperties
    ));
    assert!(parse("RETURN n{,}").is_err());
}

#[test]
fn exists_count_and_collect_subqueries_build_typed_ast_nodes() {
    let cases = [
        (
            "MATCH (n) RETURN EXISTS { (n)-->(m) WHERE m.active }",
            SubqueryExpressionKind::Exists,
        ),
        (
            "RETURN COUNT { MATCH (n) RETURN n }",
            SubqueryExpressionKind::Count,
        ),
        (
            "RETURN COLLECT { MATCH (n) RETURN n }",
            SubqueryExpressionKind::Collect,
        ),
    ];

    for (source, expected) in cases {
        let parsed = parse(source)
            .unwrap_or_else(|errors| panic!("subquery did not parse: {source}\n{errors:#?}"));
        let ExprKind::Subquery(subquery) = &first_return_expression(&parsed).kind else {
            panic!("expected a subquery expression for {source}")
        };
        assert_eq!(subquery.kind.kind, expected);
    }

    parse("MATCH (n) WHERE EXISTS { MATCH (m) WHERE EXISTS { (n)-->(m) } RETURN true } RETURN n")
        .expect("nested existential subqueries should parse");
}

#[test]
fn pattern_and_legacy_shortest_path_expressions_parse() {
    for source in [
        "MATCH (n) WHERE (n)-[:KNOWS]->() RETURN n",
        "MATCH (n), (m) WHERE (n)-[]->(m) RETURN n, m",
        "MATCH (n) RETURN (n)-->() AS connected",
        "MATCH (a), (b) RETURN shortestPath((a)-[*]->(b))",
        "MATCH (a), (b) RETURN allShortestPaths((a)-[*]->(b))",
        "RETURN shortestPath((a)-[*1..3]->(b))",
    ] {
        parse(source)
            .unwrap_or_else(|errors| panic!("pattern expression failed: {source}\n{errors:#?}"));
    }

    assert!(parse("RETURN shortestPath((a)-->(b)-->(c))").is_err());
    assert!(parse("RETURN allShortestPaths((a)-->(b)-->(c))").is_err());
    for source in [
        "RETURN (a)-->{1,3}(b)",
        "RETURN [(a)-->{1,3}(b) | b]",
        "RETURN shortestPath((a)-->{1,3}(b))",
        "RETURN shortestPath((a){2}-->(b))",
        "MATCH p = shortestPath((a)-->+(b)) RETURN p",
    ] {
        assert!(
            parse(source).is_err(),
            "simple path graph quantifier parsed: {source}"
        );
    }
}

#[test]
fn pattern_expressions_obey_boolean_primary_precedence() {
    for source in [
        "RETURN (a)-->(b)",
        "RETURN NOT (a)-->(b)",
        "RETURN (a)-->(b) AND true",
        "RETURN (a)-->(b) XOR false",
        "RETURN (a)-->(b) OR false",
        "RETURN ((a)-->(b)) + 1",
        "RETURN ((a)-->(b)) = true",
        "RETURN ((a)-->(b))[0]",
    ] {
        parse(source).unwrap_or_else(|errors| {
            panic!("valid pattern-expression precedence failed: {source}\n{errors:#?}")
        });
    }

    for source in [
        "RETURN (a)-->(b) = true",
        "RETURN (a)-->(b) + 1",
        "RETURN (a)-->(b) IN []",
        "RETURN (a)-->(b) IS NULL",
        "RETURN (a)-->(b).x",
        "RETURN (a)-->(b)[0]",
    ] {
        assert!(
            parse(source).is_err(),
            "pattern expression escaped its boolean-primary boundary: {source}"
        );
    }
}

#[test]
fn nested_and_contiguous_match_patterns_stay_in_their_intended_contexts() {
    for source in [
        "MATCH (n) SET n.prop = head(nodes(head((n)-[:REL]->()))).foo",
        "MATCH (a:A)-[:KNOWS|FOLLOWS]->(b)-->(c) OPTIONAL MATCH (a)-[r:KNOWS]->(c) WITH c WHERE r IS NULL RETURN c.name",
        "MATCH (a:A)-[:KNOWS|FOLLOWS]->(b)-->(c) OPTIONAL MATCH (a)-[r:KNOWS]->(c) WITH c WHERE r IS NOT NULL RETURN c.name",
        include_str!("../benches/fixtures/path_heavy.cypher"),
    ] {
        parse(source).unwrap_or_else(|errors| {
            panic!("contextual pattern syntax failed: {source}\n{errors:#?}")
        });
    }
}

#[test]
fn standalone_implicit_argument_calls_allow_yield_without_allowing_continuations() {
    assert!(parse("CALL test.my.proc RETURN 1").is_err());
    assert!(parse("CALL test.my.proc YIELD out RETURN out").is_err());
    assert!(parse("CALL test.my.proc('Stefan', 1) YIELD * RETURN city").is_err());
    assert!(parse("CALL test.my.proc YIELD * RETURN city").is_err());
    assert!(parse("CALL test.my.proc YIELD out WHERE out").is_err());
    for source in [
        "CALL foo UNION RETURN 1",
        "CALL foo YIELD x UNION RETURN 1",
        "CALL foo YIELD * UNION RETURN 1",
        "RETURN 1 UNION CALL foo",
        "RETURN 1 UNION CALL foo YIELD x",
        "CALL foo() YIELD * UNION RETURN 1",
        "RETURN EXISTS { CALL foo }",
        "RETURN EXISTS { CALL foo YIELD x }",
        "RETURN EXISTS { CALL foo YIELD * }",
        "RETURN EXISTS { CALL foo RETURN 1 }",
        "RETURN EXISTS { CALL foo() YIELD * }",
    ] {
        assert!(parse(source).is_err(), "standalone CALL leaked: {source}");
    }

    assert!(parse("CALL test.my.proc").is_ok());
    assert!(parse("CALL test.my.proc YIELD out").is_ok());
    assert!(parse("CALL test.my.proc YIELD *").is_ok());
    assert!(parse("CALL test.my.proc('Stefan', 1) YIELD *").is_ok());
    assert!(parse("CALL test.my.proc('Stefan', 1) YIELD city RETURN city").is_ok());
    assert!(parse("CALL foo() UNION CALL bar()").is_ok());
    assert!(parse("RETURN EXISTS { CALL foo() }").is_ok());
}

#[test]
fn path_terms_cover_subpaths_quantifiers_and_legacy_shortest_patterns() {
    for source in [
        "MATCH (p = (a)-[r]->(b)) RETURN p",
        "MATCH ((a)-[r]->(b)){1,3}-[s]->(c) RETURN c",
        "MATCH (a) (b) RETURN a, b",
        "MATCH -[r]-> RETURN r",
        "MATCH (a){2} RETURN a",
        "MATCH (a)+ RETURN a",
        "MATCH (a)* RETURN a",
        "MATCH p = shortestPath((a)-->(b)) RETURN p",
        "MATCH p = allShortestPaths((a)-->(b)) RETURN p",
        "MATCH (a) - [r] - > (b) RETURN r",
        "MATCH (a) < - [r] - (b) RETURN r",
        "MATCH (a) < - [r] - > (b) RETURN r",
        "CREATE (a)-[r]-/* arrow */>(b) RETURN r",
    ] {
        parse(source).unwrap_or_else(|errors| panic!("path term failed: {source}\n{errors:#?}"));
    }

    parse("RETURN (a) - [r] - > (b)")
        .expect("split arrow tokens must work in a pattern expression");

    let parsed = parse("MATCH (p = (a)-[r]->(b)) RETURN p").unwrap();
    let StatementKind::Query(statement) = &parsed.program.statements[0].kind else {
        panic!("expected a query")
    };
    let QueryKind::Regular(query) = &statement.query.kind else {
        panic!("expected a regular query")
    };
    let ClauseKind::Match(match_clause) = &query.head.kind.clauses[0].kind else {
        panic!("expected MATCH")
    };
    assert!(matches!(
        match_clause.pattern.parts[0].kind.path.kind.factors[0].kind,
        PathFactorKind::Subpath { .. }
    ));

    for source in [
        "MATCH p = shortestPath((a)-->(b)-->(c)) RETURN p",
        "MATCH p = allShortestPaths((a)-->(b)-->(c)) RETURN p",
    ] {
        assert!(
            parse(source).is_err(),
            "legacy shortest path accepted more than one relationship: {source}"
        );
    }
}

#[test]
fn pattern_and_label_expressions_are_allowed_in_pattern_property_values() {
    for source in [
        "MATCH (n {x: 1}) RETURN n",
        "MATCH ()-[r {x: 1}]->() RETURN r",
        "MATCH (n {p: (a)-->(b)}) RETURN n",
        "CREATE (n {p: (a)-->(b)}) RETURN n",
        "MATCH (n {p: [(a)-->(b)]}) RETURN n",
        "MATCH (n {x: m:A}) RETURN n",
        "CREATE (n {x: m IS A}) RETURN n",
    ] {
        parse(source).unwrap_or_else(|errors| {
            panic!("pattern property expression failed: {source}\n{errors:#?}")
        });
    }

    assert!(parse("RETURN {}").is_ok());
    assert!(parse("MATCH (n {}) RETURN n").is_ok());
    assert!(parse("MATCH ()-[r {}]->() RETURN r").is_ok());
    assert!(parse("CREATE (n $props) RETURN n").is_ok());
    assert!(parse("CREATE ()-[r $props]->() RETURN r").is_ok());
    assert!(parse("CREATE (n WHERE n.x = $p) RETURN n").is_ok());
    assert!(parse("CREATE ()-[r WHERE r.x = $p]->() RETURN r").is_ok());
    assert!(parse("MATCH (n $props) RETURN n").is_ok());
    assert!(parse("MATCH ()-[r $props]->() RETURN r").is_ok());
    assert!(parse("MERGE (n $props) RETURN n").is_ok());
    assert!(parse("RETURN EXISTS { MATCH (n $props) RETURN n }").is_ok());

    for source in [
        "RETURN (n {})-->()",
        "RETURN ()-[r {}]->()",
        "RETURN [(n {})-->() | n]",
        "RETURN shortestPath((n {})-->(m))",
        "MATCH p = shortestPath((n {})-->(m)) RETURN p",
    ] {
        parse(source).unwrap_or_else(|errors| {
            panic!("empty-map pattern extension failed: {source}\n{errors:#?}")
        });
    }

    for source in [
        "RETURN (n $props)-->()",
        "RETURN ()-[r $props]->()",
        "RETURN [(n $props)-->() | n]",
        "RETURN shortestPath((n $props)-->(m))",
        "MATCH p = shortestPath((n $props)-->(m)) RETURN p",
    ] {
        assert!(
            parse(source).is_err(),
            "simple pattern accepted a general-pattern property extension: {source}"
        );
    }
}

#[test]
fn separated_parameter_names_preserve_their_bnf_form() {
    let cases = [
        ("RETURN $123", ParameterName::Positional("123".to_owned())),
        ("RETURN $1abc", ParameterName::Named("1abc".to_owned())),
        ("RETURN $1_abc", ParameterName::Named("1_abc".to_owned())),
        (
            "RETURN $`odd name`",
            ParameterName::Named("odd name".to_owned()),
        ),
        (
            "RETURN $\u{0301}",
            ParameterName::Named("\u{0301}".to_owned()),
        ),
        (
            "RETURN $\u{00b7}",
            ParameterName::Named("\u{00b7}".to_owned()),
        ),
        (
            "RETURN $\u{203f}",
            ParameterName::Named("\u{203f}".to_owned()),
        ),
    ];

    for (source, expected) in cases {
        let parsed = parse(source)
            .unwrap_or_else(|errors| panic!("parameter failed: {source}\n{errors:#?}"));
        let ExprKind::Parameter(parameter) = &first_return_expression(&parsed).kind else {
            panic!("expected a parameter expression for {source}")
        };
        assert_eq!(parameter.name, expected);
    }

    for source in [
        "RETURN $ `odd name`",
        "RETURN $\n`odd name`",
        "RETURN $/* gap */`odd name`",
        "RETURN $// gap\n`odd name`",
    ] {
        assert!(
            parse(source).is_err(),
            "separated parameter gap parsed: {source:?}"
        );
    }
}

#[test]
fn baseline_label_and_selector_restrictions_are_not_overaccepted() {
    for source in [
        "MATCH (a)-[:A:B]->(b) RETURN a",
        "MATCH (n IS A:B) RETURN n",
        "MATCH ()-[r IS A:B]->() RETURN r",
        "MATCH (n:A:B|C) RETURN n",
        "MATCH (n:A:B&C) RETURN n",
        "MATCH (n:A:B&!C) RETURN n",
        "MATCH (n:A:(B|C)) RETURN n",
        "MATCH (n:!A:B) RETURN n",
        "MATCH (n:%:B) RETURN n",
        "MATCH (n:A:%) RETURN n",
        "MATCH ()-[r:A|:B&C]->() RETURN r",
        "MATCH ()-[r:A|:!B]->() RETURN r",
        "MATCH ()-[r:A|:(B|C)]->() RETURN r",
        "MATCH ()-[r:A|:%]->() RETURN r",
        "MATCH ()-[r:A|:B|C]->() RETURN r",
        "MATCH ()-[r:A|B|:C]->() RETURN r",
        "RETURN n:A|:B&C",
        "RETURN n:A|:!B",
        "RETURN n:A|:(B|C)",
        "RETURN n:A|:%",
        "RETURN n:A|:B|C",
        "RETURN n:A|B|:C",
        "MATCH (n IS !!A) RETURN n",
        "MATCH (n:!!!A) RETURN n",
        "MATCH SHORTEST (a)-->(b) RETURN a",
        "MATCH SHORTEST PATH (a)-->(b) RETURN a",
        "MATCH SHORTEST PATHS (a)-->(b) RETURN a",
    ] {
        assert!(parse(source).is_err(), "invalid syntax parsed: {source}");
    }

    for source in [
        "MATCH (a)-[:A|:B]->(b) RETURN a",
        "MATCH (a)-[:A|:B|:C]->(b) RETURN a",
        "MATCH (a)-[:A|B&C]->(b) RETURN a",
        "MATCH (a)-[:A|!B]->(b) RETURN a",
        "MATCH (a)-[:A|(B|C)]->(b) RETURN a",
        "MATCH (a)-[:A|%]->(b) RETURN a",
        "RETURN n:A|B",
        "RETURN n:A:B",
        "RETURN n:A|:B",
        "RETURN {x: n:A:B}",
        "MATCH (m {x: n:A:B}) RETURN m",
        "RETURN n IS A&B",
        "RETURN n IS NULL & A",
        "RETURN n IS NULL | A",
        "RETURN {x: n IS NULL & A}",
        "RETURN n IS NULL",
        "MATCH (n IS !(!A)) RETURN n",
        "MATCH SHORTEST 1 PATH (a)-->(b) RETURN a",
        "MATCH SHORTEST GROUP (a)-->(b) RETURN a",
    ] {
        parse(source).unwrap_or_else(|errors| panic!("valid syntax failed: {source}\n{errors:#?}"));
    }
}

#[test]
fn malformed_comprehensions_are_user_errors_not_internal_failures() {
    for source in [
        "RETURN [1 WHERE true | x]",
        "RETURN [1 | x]",
        "RETURN [(a)-->(b) WHERE | b]",
        "RETURN shortestPath((a)-->{2}(b))",
    ] {
        let outcome = parse_recovering(source);
        assert!(
            outcome
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.is_error()),
            "malformed comprehension unexpectedly parsed: {source}"
        );
        assert!(
            outcome
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code != DiagnosticCode::Internal),
            "ordinary malformed input produced an internal diagnostic: {source}\n{:#?}",
            outcome.diagnostics
        );
    }
}
