//! Contract tests for token-context preprocessing and diagnostic
//! normalization in `src/parser.rs`.
//!
//! These tests pin exact observable behavior of the token-level heuristics
//! (`is_label_predicate_context`, `is_inside_braces`, `is_in_map_entry_value`,
//! `find_top_level_token`), the union splitter (`parse_regular_query`), the
//! contextual keyword retry loop (`parse_tokens_with_contextual_names`), and
//! diagnostic ordering/dedup/truncation (`normalize_diagnostics`). Each test
//! documents the misclassification it guards against.

use open_cypher::ast::{
    ClauseKind, Expr, ExprKind, IsPredicate, ProjectionItemKind, QueryKind, RegularQuery,
    StatementKind, UnionOperator,
};
use open_cypher::{DiagnosticCode, ParsedProgram, parse, parse_recovering};

fn parse_ok(source: &str) -> ParsedProgram {
    match parse(source) {
        Ok(parsed) => parsed,
        Err(errors) => panic!(
            "expected {source:?} to parse, got:\n{}",
            errors.render(source)
        ),
    }
}

fn regular_query(parsed: &ParsedProgram) -> &RegularQuery {
    let StatementKind::Query(statement) = &parsed.program.statements[0].kind else {
        panic!("expected a query statement")
    };
    let QueryKind::Regular(query) = &statement.query.kind else {
        panic!("expected a regular query")
    };
    query
}

fn match_where_clause(parsed: &ParsedProgram) -> &Expr {
    let query = regular_query(parsed);
    let ClauseKind::Match(match_clause) = &query.head.kind.clauses[0].kind else {
        panic!("expected a MATCH clause")
    };
    match_clause.where_clause.as_ref().expect("WHERE clause")
}

fn return_item_expression(parsed: &ParsedProgram, item: usize) -> &Expr {
    let query = regular_query(parsed);
    let ClauseKind::Return(return_clause) = &query.head.kind.clauses.last().unwrap().kind else {
        panic!("expected a final RETURN clause")
    };
    let ProjectionItemKind::Expression { expression, .. } =
        &return_clause.projection.items[item].kind
    else {
        panic!("expected an expression item")
    };
    expression
}

fn assert_is_label_predicate(expression: &Expr, source: &str, text: &str) {
    let ExprKind::Is {
        negated, predicate, ..
    } = &expression.kind
    else {
        panic!("expected an IS/label predicate expression, got {expression:#?}")
    };
    assert!(!negated);
    assert!(
        matches!(predicate, IsPredicate::Label(_)),
        "expected a label predicate, got {predicate:#?}"
    );
    assert_eq!(expression.span.text(source), Some(text));
}

// --- is_label_predicate_context ---------------------------------------------

/// A plain `n:Person` in WHERE must become one label-predicate expression.
/// Guards the `IS AS <name>` exclusion in `is_label_predicate_context`: the
/// token two past the colon (`RETURN`, a symbolic name kind) must not veto
/// the classification on its own.
#[test]
fn where_label_predicate_is_an_is_label_expression() {
    let source = "MATCH (n) WHERE n:Person RETURN n";
    let parsed = parse_ok(source);
    assert_is_label_predicate(match_where_clause(&parsed), source, "n:Person");
}

/// `IS Person` in WHERE is a label predicate too (the `IS AS name` carve-out
/// must not trigger when the token after IS is not AS).
#[test]
fn where_is_keyword_label_predicate_parses() {
    let source = "MATCH (n) WHERE n IS Person RETURN n";
    let parsed = parse_ok(source);
    assert_is_label_predicate(match_where_clause(&parsed), source, "n IS Person");
}

/// A label predicate inside an EXISTS subquery's inner WHERE sits inside
/// braces; being inside braces alone must not disable the classification
/// (only `..., name :` map-key shapes inside braces may).
#[test]
fn label_predicate_inside_exists_subquery_where() {
    let source = "MATCH (m) WHERE EXISTS { MATCH (n) WHERE n:Person } RETURN m";
    let parsed = parse_ok(source);
    let rendered = format!("{:?}", parsed.program);
    assert!(
        rendered.contains("Label("),
        "expected an IsPredicate::Label in {rendered}"
    );
}

/// The `|` of a list comprehension marks expression context even when the
/// nearest clause keyword (SET) would otherwise say "pattern context".
#[test]
fn label_predicate_after_comprehension_pipe_in_set_context() {
    let source = "MATCH (a) SET a.p = [x IN [1] | x:Person] RETURN a";
    let parsed = parse_ok(source);
    let rendered = format!("{:?}", parsed.program);
    assert!(
        rendered.contains("Label("),
        "expected an IsPredicate::Label in {rendered}"
    );
}

/// The backward scan must step over a completed `[...]` (decrementing its
/// bracket counter on the matching `[`) and still find the WHERE keyword.
#[test]
fn label_predicate_after_completed_list_literal() {
    let source = "MATCH (n) WHERE n.p IN [1, 2] AND n:Person RETURN n";
    let parsed = parse_ok(source);
    let where_clause = match_where_clause(&parsed);
    let ExprKind::Binary { right, .. } = &where_clause.kind else {
        panic!("expected AND expression, got {where_clause:#?}")
    };
    assert_is_label_predicate(right, source, "n:Person");
}

/// The backward scan must step over a completed `{...}` in expression
/// position (map literal before AND) and still find the WHERE keyword.
#[test]
fn label_predicate_after_completed_map_literal() {
    let source = "MATCH (n) WHERE {a: 1} = n.p AND n:Person RETURN n";
    let parsed = parse_ok(source);
    let where_clause = match_where_clause(&parsed);
    let ExprKind::Binary { right, .. } = &where_clause.kind else {
        panic!("expected AND expression, got {where_clause:#?}")
    };
    assert_is_label_predicate(right, source, "n:Person");
}

/// A property map containing CASE (whose WHEN/THEN/ELSE are
/// expression-context markers) is skipped as a completed braced construct:
/// the later `(m:Person)` colon must stay a plain node label.
#[test]
fn case_inside_property_map_does_not_leak_expression_context() {
    let source = "MATCH (n {p: CASE WHEN true THEN 1 ELSE 2 END}), (m:Person) RETURN m";
    let parsed = parse_ok(source);
    let rendered = format!("{:?}", parsed.program);
    assert!(
        !rendered.contains("Label("),
        "node label must not become an IsPredicate::Label in {rendered}"
    );
}

/// The scan stops at the nearest MATCH: an earlier WITH/expression context
/// must not reclassify a node-pattern colon in a later MATCH.
#[test]
fn later_match_pattern_colon_stays_a_node_label() {
    let source = "MATCH (a) WITH a MATCH (b:Person) RETURN b";
    let parsed = parse_ok(source);
    let rendered = format!("{:?}", parsed.program);
    assert!(
        !rendered.contains("Label("),
        "node label must not become an IsPredicate::Label in {rendered}"
    );
}

/// A contextual variable named `match` followed by `=` is skipped by the
/// backward scan (lookahead-equals exemption), so the WHERE keyword still
/// wins and `n:Person` stays a label predicate.
#[test]
fn keyword_variable_before_equals_is_skipped_by_the_scan() {
    let source = "WITH 1 AS match WHERE match = 1 AND n:Person RETURN n";
    let parsed = parse_ok(source);
    let rendered = format!("{:?}", parsed.program);
    assert!(
        rendered.contains("Label("),
        "expected an IsPredicate::Label in {rendered}"
    );
}

// --- is_inside_braces --------------------------------------------------------

/// `RETURN n, m:Person` has the `, name :` shape but is not inside braces,
/// so the projection item is a label predicate.
#[test]
fn comma_projection_label_predicate_outside_braces() {
    let source = "MATCH (n), (m) RETURN n, m:Person";
    let parsed = parse_ok(source);
    assert_is_label_predicate(return_item_expression(&parsed, 1), source, "m:Person");
}

/// A completed map literal before the comma must not count as "inside
/// braces": `{a: 1}, m:Person` keeps the label predicate.
#[test]
fn completed_map_before_comma_label_predicate() {
    let source = "MATCH (m) RETURN {a: 1}, m:Person";
    let parsed = parse_ok(source);
    assert_is_label_predicate(return_item_expression(&parsed, 1), source, "m:Person");
}

/// Inside a map with a nested map value, the second key's colon is a map
/// key colon, not a label predicate (`is_inside_braces` must unwind the
/// nested `{...}` pair and still report "inside the outer braces").
#[test]
fn nested_map_second_key_stays_a_map_key() {
    let source = "MATCH (d) RETURN {a: {b: 1}, c: d}";
    let parsed = parse_ok(source);
    let expression = return_item_expression(&parsed, 0);
    assert_eq!(expression.span.text(source), Some("{a: {b: 1}, c: d}"));
    let ExprKind::Map(entries) = &expression.kind else {
        panic!("expected a map literal, got {expression:#?}")
    };
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[1].key.kind.text, "c");
    assert_eq!(entries[1].value.span.text(source), Some("d"));
}

// --- is_in_map_entry_value ---------------------------------------------------

/// A closed property map in an earlier MATCH must be popped from the open
/// brace stack: the later `(m:Person)` is a plain node label, not a
/// map-entry-value label predicate.
#[test]
fn closed_property_map_does_not_capture_later_node_label() {
    let source = "MATCH (n {a: 1}) MATCH (m:Person) RETURN m";
    let parsed = parse_ok(source);
    let rendered = format!("{:?}", parsed.program);
    assert!(
        !rendered.contains("Label("),
        "node label must not become an IsPredicate::Label in {rendered}"
    );
}

/// Inside a map entry value, a comma nested in brackets must not reset the
/// after-separator state: `m:Person` deep in a property-map value is still
/// a label predicate.
#[test]
fn bracketed_comma_inside_map_value_keeps_entry_value_context() {
    let source = "MATCH (m), (n {flag: [1, 2] = $x AND m:Person}) RETURN n";
    let parsed = parse_ok(source);
    let rendered = format!("{:?}", parsed.program);
    assert!(
        rendered.contains("Label("),
        "expected an IsPredicate::Label in {rendered}"
    );
}

/// A map key colon after a completed list value stays a map key: the
/// closing `]` must decrement the nesting depth so the top-level comma
/// resets the entry-value state.
#[test]
fn map_key_after_completed_list_value_stays_a_map_key() {
    let source = "MATCH (c) RETURN {a: [1], b: c}";
    let parsed = parse_ok(source);
    let expression = return_item_expression(&parsed, 0);
    let ExprKind::Map(entries) = &expression.kind else {
        panic!("expected a map literal, got {expression:#?}")
    };
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[1].key.kind.text, "b");
    assert_eq!(entries[1].value.span.text(source), Some("c"));
}

/// A top-level comma resets the entry-value state: the second key's colon
/// in `{a: 1, b: c}` is a map key colon, not a label predicate on `b`.
#[test]
fn map_key_after_top_level_comma_stays_a_map_key() {
    let source = "MATCH (c) RETURN {a: 1, b: c}";
    let parsed = parse_ok(source);
    let expression = return_item_expression(&parsed, 0);
    let ExprKind::Map(entries) = &expression.kind else {
        panic!("expected a map literal, got {expression:#?}")
    };
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[1].key.kind.text, "b");
    assert_eq!(entries[1].value.span.text(source), Some("c"));
}

/// A colon nested inside a property map (`{p: 1}`) inside an EXISTS body
/// must not put the subquery in entry-value state: the later `(b:Person)`
/// colon is a plain node label.
#[test]
fn nested_property_map_colon_in_exists_body_is_not_a_separator() {
    let source = "MATCH (m) WHERE EXISTS { MATCH (a {p: 1}) MATCH (b:Person) } RETURN m";
    let parsed = parse_ok(source);
    let rendered = format!("{:?}", parsed.program);
    assert!(
        !rendered.contains("Label("),
        "node label must not become an IsPredicate::Label in {rendered}"
    );
}

// --- find_top_level_token ----------------------------------------------------

/// A pattern comprehension whose predicate contains a nested list
/// comprehension: the nested `|` sits at bracket depth one and must not be
/// mistaken for the top-level projection pipe.
#[test]
fn nested_list_comprehension_pipe_does_not_split_the_comprehension() {
    let source = "MATCH (a) RETURN [(a)--(b) WHERE a.x IN [z IN [1, 2] | z] | a.name]";
    let parsed = parse_ok(source);
    let expression = return_item_expression(&parsed, 0);
    let ExprKind::PatternComprehension(comprehension) = &expression.kind else {
        panic!("expected a pattern comprehension, got {expression:#?}")
    };
    assert_eq!(comprehension.pattern.span.text(source), Some("(a)--(b)"));
    let predicate = comprehension.predicate.as_ref().expect("predicate");
    assert_eq!(
        predicate.span.text(source),
        Some("a.x IN [z IN [1, 2] | z]")
    );
    assert_eq!(comprehension.projection.span.text(source), Some("a.name"));
}

// --- parse_regular_query -----------------------------------------------------

fn union_parts(source: &str) -> (UnionOperator, String) {
    let parsed = parse_ok(source);
    let query = regular_query(&parsed);
    assert_eq!(query.unions.len(), 1, "expected one union branch");
    let operator = &query.unions[0].operator;
    (
        operator.kind,
        operator
            .span
            .text(source)
            .expect("operator span")
            .to_owned(),
    )
}

#[test]
fn union_all_operator_kind_and_exact_span() {
    let (kind, text) = union_parts("RETURN 1 UNION ALL RETURN 2");
    assert_eq!(kind, UnionOperator::All);
    assert_eq!(text, "UNION ALL");
}

#[test]
fn union_distinct_operator_kind_and_exact_span() {
    let (kind, text) = union_parts("RETURN 1 UNION DISTINCT RETURN 2");
    assert_eq!(kind, UnionOperator::Distinct);
    assert_eq!(text, "UNION DISTINCT");
}

#[test]
fn bare_union_operator_kind_and_exact_span() {
    let (kind, text) = union_parts("RETURN 1 UNION RETURN 2");
    assert_eq!(kind, UnionOperator::Default);
    assert_eq!(text, "UNION");
}

// --- normalize_diagnostics ---------------------------------------------------

/// Two `]` mismatches plus one parser error: three distinct diagnostics.
/// The two MismatchedDelimiter entries share code and message but differ in
/// span, and the two diagnostics at 7..8 share the span but differ in code
/// and message: none of the three may be deduplicated away.
#[test]
fn distinct_delimiter_diagnostics_are_not_deduplicated() {
    let source = "RETURN ] RETURN ]";
    let outcome = parse_recovering(source);
    let observed: Vec<(DiagnosticCode, usize, usize)> = outcome
        .diagnostics
        .iter()
        .map(|diagnostic| {
            (
                diagnostic.code,
                diagnostic.primary_span.start,
                diagnostic.primary_span.end,
            )
        })
        .collect();
    assert_eq!(
        observed,
        vec![
            (DiagnosticCode::UnexpectedToken, 7, 8),
            (DiagnosticCode::MismatchedDelimiter, 7, 8),
            (DiagnosticCode::MismatchedDelimiter, 16, 17),
        ]
    );
}

/// Exactly 32 diagnostics (the cap) must be kept as-is: truncation plus the
/// TooManyErrors marker only kicks in strictly above the cap.
#[test]
fn exactly_max_diagnostics_are_kept_without_truncation() {
    // 31 invalid `~` tokens produce 31 lexer diagnostics plus one parser
    // diagnostic: 32 total, exactly at MAX_DIAGNOSTICS.
    let source = format!("RETURN 1 {}", "~ ".repeat(31));
    let outcome = parse_recovering(&source);
    assert_eq!(outcome.diagnostics.len(), 32);
    assert!(
        outcome
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != DiagnosticCode::TooManyErrors),
        "no diagnostic may be suppressed at exactly the cap"
    );

    // One more error crosses the cap and appends the marker.
    let source = format!("RETURN 1 {}", "~ ".repeat(32));
    let outcome = parse_recovering(&source);
    assert_eq!(outcome.diagnostics.len(), 32);
    assert_eq!(
        outcome.diagnostics.last().map(|diagnostic| diagnostic.code),
        Some(DiagnosticCode::TooManyErrors)
    );
}

// --- parse_tokens_with_contextual_names --------------------------------------

/// `RETURN count +` — reclassifying `count` moves the error from the `+`
/// to end-of-input; the reported diagnostic must be the farthest error.
#[test]
fn contextual_retry_reports_the_farthest_error() {
    let source = "RETURN count +";
    let outcome = parse_recovering(source);
    let observed: Vec<(DiagnosticCode, usize, usize)> = outcome
        .diagnostics
        .iter()
        .map(|diagnostic| {
            (
                diagnostic.code,
                diagnostic.primary_span.start,
                diagnostic.primary_span.end,
            )
        })
        .collect();
    assert_eq!(observed, vec![(DiagnosticCode::UnexpectedEof, 14, 14)]);
}

/// `RETURN 1 + count filter` — the retry that resolves `count` does not
/// move the error, so the loop stops and reports the original expectation
/// (COUNT still wants `(` or `{`), not the post-retry operator expectation.
#[test]
fn non_improving_retry_keeps_the_original_expectation() {
    let source = "RETURN 1 + count filter";
    let outcome = parse_recovering(source);
    assert_eq!(outcome.diagnostics.len(), 1);
    let diagnostic = &outcome.diagnostics[0];
    assert_eq!(diagnostic.code, DiagnosticCode::UnexpectedToken);
    assert_eq!(
        (diagnostic.primary_span.start, diagnostic.primary_span.end),
        (17, 23)
    );
    assert_eq!(
        diagnostic.message,
        "unexpected token `identifier`; expected `(`, or `{`"
    );
}

/// `RETURN any (x) count` — the nearest candidate is `count`, after the
/// error location; resolving it cannot help, and the error stays at `)`.
#[test]
fn quantifier_without_in_reports_error_at_the_closing_paren() {
    let source = "RETURN any (x) count";
    let outcome = parse_recovering(source);
    assert_eq!(outcome.diagnostics.len(), 1);
    let diagnostic = &outcome.diagnostics[0];
    assert_eq!(diagnostic.code, DiagnosticCode::UnexpectedToken);
    assert_eq!(
        (diagnostic.primary_span.start, diagnostic.primary_span.end),
        (13, 14)
    );
    assert_eq!(diagnostic.message, "unexpected token `)`; expected `IN`");
}

/// `RETURN any(x) + count` — the fix is `any` (before the error); `count`
/// after the error must rank by its real distance, and the second retry
/// round (whose error moved forward) must be allowed to proceed.
#[test]
fn two_round_contextual_resolution_before_and_after_the_error() {
    let source = "RETURN any(x) + count";
    let parsed = parse_ok(source);
    let expression = return_item_expression(&parsed, 0);
    assert_eq!(expression.span.text(source), Some("any(x) + count"));
}

/// `RETURN 1 + detach, 2 + count AS y` — `detach` is accepted natively as a
/// name, and the `count AS` retry (the followed-by-AS candidate) must still
/// resolve on top of it.
#[test]
fn exact_location_candidate_outranks_aliased_candidate() {
    let source = "RETURN 1 + detach, 2 + count AS y";
    let parsed = parse_ok(source);
    assert_eq!(
        return_item_expression(&parsed, 0).span.text(source),
        Some("1 + detach")
    );
    assert_eq!(
        return_item_expression(&parsed, 1).span.text(source),
        Some("2 + count")
    );
}

/// `RETURN 1 + count, 2 + single` — two distinct keyword signatures need
/// two successive retry rounds with strictly advancing error locations.
#[test]
fn two_keyword_signatures_resolve_in_successive_rounds() {
    let source = "RETURN 1 + count, 2 + single";
    let parsed = parse_ok(source);
    assert_eq!(
        return_item_expression(&parsed, 0).span.text(source),
        Some("1 + count")
    );
    assert_eq!(
        return_item_expression(&parsed, 1).span.text(source),
        Some("2 + single")
    );
}

/// `RETURN as` — a bare `as` projection resolves to a variable named "as".
#[test]
fn bare_as_projection_resolves_the_as_token_itself() {
    let source = "RETURN as";
    let parsed = parse_ok(source);
    let expression = return_item_expression(&parsed, 0);
    assert_eq!(expression.span.text(source), Some("as"));
    let ExprKind::Variable(name) = &expression.kind else {
        panic!("expected a variable, got {expression:#?}")
    };
    assert_eq!(name.kind.text, "as");
}

/// `RETURN + count AS as` — the `AS` token sits exactly at the error
/// location and is itself followed by `as`, which gives it top retry
/// priority; the loop then stops without inventing a parse, and the
/// diagnostic keeps COUNT's original expectation. (A candidate-priority
/// change here would instead let the query parse.)
#[test]
fn count_aliased_as_the_word_as_keeps_counts_expectation() {
    let source = "RETURN + count AS as";
    let outcome = parse_recovering(source);
    assert_eq!(outcome.diagnostics.len(), 1);
    let diagnostic = &outcome.diagnostics[0];
    assert_eq!(diagnostic.code, DiagnosticCode::UnexpectedToken);
    assert_eq!(
        (diagnostic.primary_span.start, diagnostic.primary_span.end),
        (15, 17)
    );
    assert_eq!(
        diagnostic.message,
        "unexpected token `AS`; expected `(`, or `{`"
    );
}

/// `RETURN any ( ) as` — the followed-by-AS candidate (`)` is followed by
/// nothing aliasable, so only `any` qualifies via priority one after its
/// exact/AS checks) drives the retry, and the final diagnostic points at
/// the empty parentheses, not end-of-input. (Promoting plain candidates to
/// priority one would report UnexpectedEof at 17 instead.)
#[test]
fn empty_quantifier_parens_report_error_at_the_close_paren() {
    let source = "RETURN any ( ) as";
    let outcome = parse_recovering(source);
    assert_eq!(outcome.diagnostics.len(), 1);
    let diagnostic = &outcome.diagnostics[0];
    assert_eq!(diagnostic.code, DiagnosticCode::UnexpectedToken);
    assert_eq!(
        (diagnostic.primary_span.start, diagnostic.primary_span.end),
        (13, 14)
    );
    assert_eq!(
        diagnostic.message,
        "unexpected token `)`; expected `MATCH`, `PATH`, `RETURN`, `escaped identifier`, or `identifier`"
    );
}
