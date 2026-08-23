#![cfg(feature = "serde")]

use open_cypher::{ParsedProgram, parse};

#[test]
fn parsed_program_round_trips_through_json() {
    let parsed = parse("MATCH (n:Person) RETURN n")
        .expect("serde fixture must be a syntactically valid query");

    let json = serde_json::to_string(&parsed).expect("ParsedProgram should serialize");
    let decoded: ParsedProgram =
        serde_json::from_str(&json).expect("ParsedProgram should deserialize");

    assert_eq!(decoded, parsed);
}
