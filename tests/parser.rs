use open_cypher::parse;

fn assert_parses(source: &str) {
    if let Err(errors) = parse(source) {
        panic!("expected query to parse:\n{source}\n\nerrors:\n{errors:#?}");
    }
}

fn assert_rejected(source: &str) {
    assert!(
        parse(source).is_err(),
        "expected malformed query to be rejected:\n{source}"
    );
}

#[test]
fn parses_representative_read_queries() {
    for source in [
        "RETURN 1",
        "MATCH (n) RETURN n",
        "MATCH (n:Person {name: $name}) WHERE n.active = true RETURN n.name AS name",
        "OPTIONAL MATCH (a)-[r:KNOWS]->(b) RETURN a, r, b",
        "UNWIND [1, 2, 3] AS value RETURN value ORDER BY value DESC",
        "MATCH (n) WITH n ORDER BY n.name SKIP 1 LIMIT 10 RETURN n",
        "MATCH p = (a)-[:KNOWS*1..3]->(b) RETURN p",
    ] {
        assert_parses(source);
    }
}

#[test]
fn empty_and_trivia_only_programs_parse() {
    assert_parses("");
    assert_parses("  // no statement\n  /* still no statement */  ");
}

#[test]
fn parses_representative_write_queries() {
    for source in [
        "CREATE (n:Person {name: 'Ada'}) RETURN n",
        "MATCH (n:Person) SET n.active = true RETURN n",
        "MATCH (n:Person) REMOVE n.temporary RETURN n",
        "MATCH (n:Person) DELETE n",
        "MATCH (n:Person) DETACH DELETE n",
        "MERGE (n:Person {name: $name}) ON CREATE SET n.created = true RETURN n",
    ] {
        assert_parses(source);
    }
}

#[test]
fn parses_expression_precedence_without_parentheses() {
    assert_parses("RETURN 1 + 2 * 3 ^ 4 AS value");
    assert_parses("RETURN true OR false AND NOT false AS value");
    assert_parses("RETURN 1 < 2 AND 3 >= 2 AS value");
}

#[test]
fn accepts_an_optional_trailing_semicolon_but_not_trailing_garbage() {
    assert_parses("RETURN 1;");
    assert_rejected("RETURN 1; this is not Cypher");
}

#[test]
fn rejects_common_incomplete_constructs() {
    for source in [
        "MATCH",
        "MATCH (n RETURN n",
        "RETURN",
        "RETURN 1 +",
        "MATCH (n) WHERE RETURN n",
        "CREATE (n {name: 'Ada')",
    ] {
        assert_rejected(source);
    }
}
