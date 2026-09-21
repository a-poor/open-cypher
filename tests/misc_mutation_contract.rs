//! Contract tests pinning behavior that surviving cargo-mutants mutants would
//! otherwise change: string decoding, quote styles, nested block comments,
//! double-quoted string lexing, keyword lookup for 9- and 10-byte keywords,
//! `Program::is_empty`, `Span::is_empty`, farthest-error selection, and the
//! standalone-CALL fallback's interaction with `UNION` tokens.

use open_cypher::ast::{
    ClauseKind, Expr, ExprKind, LiteralKind, ParameterName, ProjectionItemKind, QueryKind,
    QuoteStyle, StatementKind,
};
use open_cypher::{
    DiagnosticCode, Keyword, ParsedProgram, Span, TokenKind, lex, parse, parse_recovering,
};

fn first_return_expression(parsed: &ParsedProgram) -> &Expr {
    let StatementKind::Query(statement) = &parsed.program.statements[0].kind else {
        panic!("expected a query statement");
    };
    let QueryKind::Regular(query) = &statement.query.kind else {
        panic!("expected a regular query");
    };
    let ClauseKind::Return(return_clause) = &query.head.kind.clauses.last().unwrap().kind else {
        panic!("expected a final RETURN clause");
    };
    let ProjectionItemKind::Expression { expression, .. } = &return_clause.projection.items[0].kind
    else {
        panic!("expected an expression projection item");
    };
    expression
}

fn string_literal(source: &str) -> (String, QuoteStyle) {
    let parsed = parse(source).unwrap_or_else(|errors| panic!("{source} failed:\n{errors:#?}"));
    let ExprKind::Literal(literal) = &first_return_expression(&parsed).kind else {
        panic!("expected a literal expression for {source}");
    };
    let LiteralKind::String(string) = &literal.kind else {
        panic!("expected a string literal for {source}");
    };
    (string.value.clone(), string.quote)
}

// src/ast.rs: replace Program::is_empty -> bool with true
#[test]
fn program_is_empty_tracks_statements() {
    let parsed = parse("RETURN 1").expect("query should parse");
    assert!(!parsed.program.is_empty());
    assert_eq!(parsed.program.statements.len(), 1);

    let empty = parse("   ").expect("trivia-only input should parse");
    assert!(empty.program.is_empty());
    assert!(empty.program.statements.is_empty());
}

// src/span.rs: replace Span::is_empty -> bool with false
#[test]
fn span_is_empty_tracks_width() {
    assert!(Span::empty(3).is_empty());
    assert!(Span::new(7, 7).is_empty());
    assert!(!Span::new(1, 2).is_empty());

    // Documenting Span::cover; the tie cases behave identically whichever
    // operand supplies the equal endpoint.
    assert_eq!(Span::new(2, 5).cover(Span::new(4, 9)), Span::new(2, 9));
    assert_eq!(Span::new(4, 9).cover(Span::new(2, 5)), Span::new(2, 9));
    assert_eq!(Span::new(3, 5).cover(Span::new(3, 5)), Span::new(3, 5));
}

// src/lexer.rs: replace lex_double_string -> Result<(), LexingError> with Ok(())
#[test]
fn double_quoted_strings_lex_as_one_token() {
    let outcome = lex("\"ab\"");
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    assert_eq!(outcome.tokens.len(), 1);
    assert_eq!(outcome.tokens[0].kind, TokenKind::String);
    assert_eq!(outcome.tokens[0].span, Span::new(0, 4));

    // An unterminated double-quoted string must be diagnosed, not silently
    // truncated to the opening quote.
    let unterminated = lex("\"ab");
    assert!(
        unterminated
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::UnterminatedString)
    );
}

// src/lexer.rs: replace += with *= in lex_block_comment
#[test]
fn nested_block_comments_scan_past_inner_openers() {
    // The nested `/*` sits at remainder offset 4; doubling instead of
    // advancing the scan offset skips the inner `*/` and misreports the
    // comment as unterminated.
    let source = "/*abcd/**/x*/ RETURN 1";
    let outcome = lex(source);
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    assert_eq!(outcome.tokens[0].kind, TokenKind::BlockComment);
    assert_eq!(outcome.tokens[0].span, Span::new(0, 13));
    assert!(parse(source).is_ok());
}

// src/token.rs: delete match arms 9 and 10 in Keyword::from_ascii_case_insensitive
#[test]
fn nine_and_ten_byte_keywords_resolve() {
    assert_eq!(
        Keyword::from_ascii_case_insensitive("ascending"),
        Some(Keyword::Ascending)
    );
    assert_eq!(
        Keyword::from_ascii_case_insensitive("DESCENDING"),
        Some(Keyword::Descending)
    );
    assert_eq!(
        lex("ASCENDING").tokens[0].kind,
        TokenKind::Keyword(Keyword::Ascending)
    );
    assert_eq!(
        lex("descending").tokens[0].kind,
        TokenKind::Keyword(Keyword::Descending)
    );
    assert!(parse("MATCH (n) RETURN n ORDER BY n ASCENDING").is_ok());
    assert!(parse("MATCH (n) RETURN n ORDER BY n DESCENDING").is_ok());
}

// src/parser.rs: replace == with != in string_literal (quote-style selection)
#[test]
fn string_literals_record_their_quote_style() {
    assert_eq!(
        string_literal("RETURN 'a'"),
        ("a".to_owned(), QuoteStyle::Single)
    );
    assert_eq!(
        string_literal("RETURN \"a\""),
        ("a".to_owned(), QuoteStyle::Double)
    );
}

// src/parser.rs: the three mutations of the doubled-delimiter test in
// decode_delimited (== -> !=, && -> ||, peek == -> peek !=)
#[test]
fn doubled_delimiters_decode_to_one_character() {
    assert_eq!(
        string_literal("RETURN 'a''b'"),
        ("a'b".to_owned(), QuoteStyle::Single)
    );
    assert_eq!(
        string_literal("RETURN \"a\"\"b\""),
        ("a\"b".to_owned(), QuoteStyle::Double)
    );
    assert_eq!(
        string_literal("RETURN 'a\\'b'"),
        ("a'b".to_owned(), QuoteStyle::Single)
    );
    assert_eq!(
        string_literal("RETURN '\\u0041'"),
        ("A".to_owned(), QuoteStyle::Single)
    );

    // Doubled backticks inside an escaped parameter name go through the same
    // decoder with a backtick delimiter.
    let parsed = parse("RETURN $`a``b`").expect("escaped parameter should parse");
    let ExprKind::Parameter(parameter) = &first_return_expression(&parsed).kind else {
        panic!("expected a parameter expression");
    };
    assert_eq!(parameter.name, ParameterName::Named("a`b".to_owned()));

    let parsed = parse("RETURN $0").expect("positional parameter should parse");
    let ExprKind::Parameter(parameter) = &first_return_expression(&parsed).kind else {
        panic!("expected a parameter expression");
    };
    assert_eq!(parameter.name, ParameterName::Positional("0".to_owned()));
}

// src/parser.rs: replace >= with < in retain_farthest_error
#[test]
fn syntax_errors_report_the_farthest_failure() {
    // The clause splitter tries several clause boundaries; the reported error
    // must come from the attempt that progressed farthest (the trailing `n`),
    // not from the earliest failing prefix (EOF after `WHERE`).
    let outcome = parse_recovering("MATCH (n) WHERE RETURN n");
    assert_eq!(outcome.diagnostics.len(), 1, "{:#?}", outcome.diagnostics);
    let diagnostic = &outcome.diagnostics[0];
    assert_eq!(diagnostic.code, DiagnosticCode::UnexpectedToken);
    assert_eq!(diagnostic.primary_span, Span::new(23, 24));
}

// src/parser.rs: delete the depth-increment match arm in top_level_indices
#[test]
fn union_inside_call_arguments_is_not_top_level() {
    // The UNION lives inside the argument list; it must not stop the
    // standalone-CALL fallback (which is the only rule accepting `YIELD *`).
    let source = "CALL foo(COUNT { RETURN 1 UNION RETURN 2 }) YIELD *";
    let parsed = parse(source).unwrap_or_else(|errors| panic!("{errors:#?}"));
    let StatementKind::Query(statement) = &parsed.program.statements[0].kind else {
        panic!("expected a query statement");
    };
    let QueryKind::Regular(query) = &statement.query.kind else {
        panic!("expected a regular query");
    };
    let ClauseKind::Call(call) = &query.head.kind.clauses[0].kind else {
        panic!("expected a CALL clause");
    };
    assert!(call.yield_clause.as_ref().is_some_and(|yield_| yield_.all));
}

// src/parser.rs: replace the UNION match guard in parse_program with false
#[test]
fn top_level_union_disables_the_standalone_call_fallback() {
    // Pins current behavior: a top-level UNION token after a failed regular
    // parse suppresses the standalone-CALL fallback, so the reserved keyword
    // is not silently accepted as a procedure name here. (Note the adjacent
    // inconsistency: `CALL union` itself parses via contextual retries.)
    assert!(parse("CALL union").is_ok());
    let outcome = parse_recovering("CALL union YIELD *");
    assert_eq!(outcome.diagnostics.len(), 1, "{:#?}", outcome.diagnostics);
    let diagnostic = &outcome.diagnostics[0];
    assert_eq!(diagnostic.code, DiagnosticCode::UnexpectedToken);
    assert_eq!(diagnostic.primary_span, Span::new(11, 16));
}
