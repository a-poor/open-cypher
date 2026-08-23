use open_cypher::{ParsedProgram, lex, parse, parse_recovering};

fn assert_is_parsed_program(_: &ParsedProgram) {}

fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn strict_parse_returns_the_public_program_type() {
    let source = "RETURN 1 AS answer";
    let parsed = parse(source).expect("a minimal query should parse");

    assert_is_parsed_program(&parsed);
    assert!(
        !parsed.tokens.is_empty(),
        "the parsed program retains its tokens"
    );
    assert!(parsed.program.span.end <= source.len());
    assert!(source.is_char_boundary(parsed.program.span.start));
    assert!(source.is_char_boundary(parsed.program.span.end));
    for statement in &parsed.program.statements {
        assert!(parsed.program.span.contains_span(statement.span));
    }
}

#[test]
fn lexing_and_parsing_expose_the_same_token_stream() {
    let source = "MATCH (person) RETURN person";
    let lexed = lex(source);
    let parsed = parse(source).expect("the representative query should parse");

    assert!(lexed.diagnostics.is_empty(), "{:#?}", lexed.diagnostics);
    assert_eq!(parsed.tokens, lexed.tokens);
}

#[test]
fn strict_and_recovering_entry_points_agree_on_valid_input() {
    let source = "MATCH (n) WHERE n.score >= 10 RETURN n";
    let strict = parse(source).expect("valid input should parse strictly");
    let recovering = parse_recovering(source);

    assert!(
        recovering.diagnostics.is_empty(),
        "valid input produced recovery diagnostics: {:#?}",
        recovering.diagnostics
    );
    let recovered = recovering
        .value
        .expect("valid input should produce a recovered program");
    assert_eq!(recovered, strict);
}

#[test]
fn parser_results_are_owned_and_can_outlive_the_source_binding() {
    let parsed = {
        let source = String::from("RETURN 'owned' AS value");
        parse(&source).expect("valid input should parse")
    };

    assert!(!parsed.tokens.is_empty());
}

#[test]
fn parsed_program_is_send_and_sync() {
    assert_send_sync::<ParsedProgram>();
}
