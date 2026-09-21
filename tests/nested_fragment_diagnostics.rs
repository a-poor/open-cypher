//! Syntax errors inside a nested fragment (a pattern expression, pattern
//! comprehension, or label expression) must be reported at the offending
//! token. Before the user-error payload carried a location, such failures were
//! reported at offset 0, and the contextual-name retry then dragged the error
//! onto whatever later keyword it happened to reclassify.

use open_cypher::{DiagnosticCode, parse_recovering};

fn sole_error_offset(source: &str) -> usize {
    let outcome = parse_recovering(source);
    assert_eq!(
        outcome.diagnostics.len(),
        1,
        "{source}: expected one diagnostic:\n{:#?}",
        outcome.diagnostics
    );
    let diagnostic = &outcome.diagnostics[0];
    assert_eq!(
        diagnostic.code,
        DiagnosticCode::UnexpectedToken,
        "{source}: {diagnostic:#?}"
    );
    diagnostic.primary_span.start
}

#[test]
fn graph_quantifiers_in_pattern_expressions_are_reported_at_the_quantifier() {
    // Pattern expressions are simple path patterns, so a graph pattern
    // quantifier is a syntax error; the diagnostic must sit on the quantifier
    // rather than on the statement start or the following clause keyword.
    for (source, quantifier) in [
        ("RETURN (a)-[:R]->{1,2}(b)", "{"),
        ("RETURN exists((a)-[:R]->{1,2}(b))", "{"),
        ("MATCH (a) WHERE (a)-[:R]->+(b) RETURN a", "+"),
        (
            "MATCH (a) WHERE a.x = 1 AND (a)-[:R]->{1,2}(b) RETURN a",
            "{",
        ),
        ("RETURN [(a)-[:R]->*(b) | b]", "*"),
        ("RETURN [p = (a)-[:R]->*(b) WHERE true | p]", "*"),
    ] {
        let expected = source.find(quantifier).expect("quantifier in source");
        assert_eq!(sole_error_offset(source), expected, "{source}");
    }
}

#[test]
fn structurally_malformed_pattern_comprehensions_point_inside_the_brackets() {
    // An empty projection is reported at the closing bracket and an empty
    // WHERE predicate at the projection pipe, not at the statement start.
    for (source, marker) in [
        ("RETURN [(a)-->(b) | ]", "]"),
        ("RETURN [(a)-->(b) WHERE | b]", "|"),
    ] {
        let expected = source.find(marker).expect("marker in source");
        assert_eq!(sole_error_offset(source), expected, "{source}");
    }
}

#[test]
fn a_retry_that_fails_on_the_reclassified_keyword_is_not_progress() {
    // The nested failure is at the quantifier. Reclassifying the following
    // RETURN as an identifier fails at RETURN itself, which is farther along
    // but not a better reading; the original error must win.
    let source = "MATCH (a) WHERE (a)-[:R]->{2}(b) RETURN a";
    assert_eq!(sole_error_offset(source), source.find('{').unwrap());
    let outcome = parse_recovering(source);
    assert!(
        !outcome.diagnostics[0].message.contains("`AND`"),
        "{:#?}",
        outcome.diagnostics
    );
}
