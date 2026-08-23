//! Focused witnesses for syntax added while openCypher moved toward GQL.
//!
//! This is deliberately a small release-feature suite, not a substitute for
//! the version-pinned grammar production manifest or the TCK syntax projection.

use open_cypher::parse;

fn assert_parses(source: &str) {
    if let Err(errors) = parse(source) {
        panic!("expected 2024.3 syntax to parse:\n{source}\n\n{errors:#?}");
    }
}

#[test]
fn parses_shortest_path_selectors() {
    for source in [
        "MATCH p = ANY SHORTEST (a)-[]->{1,3}(b) RETURN p",
        "MATCH p = ALL SHORTEST (a)-[]->{1,3}(b) RETURN p",
        "MATCH p = SHORTEST 3 GROUPS (a)-[]->{1,3}(b) RETURN p",
    ] {
        assert_parses(source);
    }
}

#[test]
fn parses_gql_style_quantified_paths() {
    assert_parses("MATCH p = (a)-[r:KNOWS]->{1,3}(b) RETURN p");
    assert_parses("MATCH ((a)-[:KNOWS]->(b)){1,3} RETURN a, b");
}

#[test]
fn parses_documented_gql_path_mode_extensions() {
    for mode in ["WALK", "TRAIL", "SIMPLE", "ACYCLIC"] {
        assert_parses(&format!("MATCH {mode} p = (a)-[]->{{1,3}}(b) RETURN p"));
    }
}

#[test]
fn parses_boolean_label_expressions() {
    assert_parses("MATCH (n:Person & !Employee) RETURN n");
    assert_parses("MATCH (n:(Person | Employee) & !Suspended) RETURN n");
}

#[test]
fn parses_offset_as_well_as_legacy_skip() {
    assert_parses("MATCH (n) RETURN n ORDER BY n.name OFFSET 10 LIMIT 5");
    assert_parses("MATCH (n) RETURN n ORDER BY n.name SKIP 10 LIMIT 5");
}
