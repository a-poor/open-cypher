use open_cypher::ast::StatementKind;
use open_cypher::{
    Diagnostic, DiagnosticCode, Keyword, OPEN_CYPHER_GRAMMAR_REVISION, OPEN_CYPHER_GRAMMAR_VERSION,
    Severity, Span, Token, TokenKind, lex, parse, parse_recovering,
};

#[test]
fn public_version_and_half_open_span_contract_is_exact() {
    assert_eq!(OPEN_CYPHER_GRAMMAR_VERSION, "2024.3");
    assert_eq!(
        OPEN_CYPHER_GRAMMAR_REVISION,
        "677cbafabb8c3c5eed458fd3b1ec0daec8d67d23"
    );

    let source = "aébc";
    let span = Span::new(1, 3);
    assert_eq!(span.len(), 2);
    assert!(!span.is_empty());
    assert!(span.contains(1));
    assert!(!span.contains(3));
    assert_eq!(span.text(source), Some("é"));
    assert_eq!(span.range(), 1..3);
}

#[test]
fn strict_parse_exposes_the_exact_lossless_token_stream() {
    let source = "RETURN /* keep */ 1;";
    let parsed = parse(source).expect("contract query should parse");

    assert_eq!(
        parsed.tokens,
        [
            Token::new(TokenKind::Keyword(Keyword::Return), Span::new(0, 6)),
            Token::new(TokenKind::Whitespace, Span::new(6, 7)),
            Token::new(TokenKind::BlockComment, Span::new(7, 17)),
            Token::new(TokenKind::Whitespace, Span::new(17, 18)),
            Token::new(TokenKind::Integer, Span::new(18, 19)),
            Token::new(TokenKind::Semicolon, Span::new(19, 20)),
        ]
    );
    assert_eq!(
        parsed
            .tokens
            .iter()
            .map(|token| token.text(source).expect("token span should slice source"))
            .collect::<Vec<_>>(),
        ["RETURN", " ", "/* keep */", " ", "1", ";"]
    );
    assert_eq!(
        parsed
            .tokens
            .iter()
            .map(|token| token.is_trivia())
            .collect::<Vec<_>>(),
        [false, true, true, true, false, false]
    );
}

#[test]
fn unterminated_string_diagnostic_has_exact_public_fields() {
    let source = "RETURN 'unterminated";
    let outcome = lex(source);

    assert_eq!(
        outcome.tokens.last().map(|token| token.kind),
        Some(TokenKind::Invalid)
    );
    assert_eq!(
        outcome.diagnostics,
        [Diagnostic::new(
            DiagnosticCode::UnterminatedString,
            Severity::Error,
            "unterminated string literal",
            Span::new(7, source.len()),
        )]
    );
}

#[test]
fn diagnostic_renderer_has_an_exact_stable_text_contract() {
    let source = "RETURN ]";
    let diagnostic = Diagnostic::error(
        DiagnosticCode::UnexpectedToken,
        "expected an expression",
        Span::new(7, 8),
    )
    .with_label(Span::new(0, 6), "while parsing this RETURN clause")
    .with_note("the diagnostic renderer is color independent")
    .with_help("remove `]` or add an expression");

    assert_eq!(
        diagnostic.render(source),
        concat!(
            "error[OCY-P001]: expected an expression\n",
            " --> 1:8\n",
            "  |\n",
            "1 | RETURN ]\n",
            "  |        ^\n",
            "  |\n",
            "1 | RETURN ]\n",
            "  | ------ while parsing this RETURN clause\n",
            "  = note: the diagnostic renderer is color independent\n",
            "  = help: remove `]` or add an expression",
        )
    );
}

#[test]
fn recovery_diagnostics_and_error_root_have_exact_spans() {
    let source = "MATCH (n";
    let outcome = parse_recovering(source);

    assert_eq!(outcome.diagnostics.len(), 2);
    let unclosed = &outcome.diagnostics[0];
    assert_eq!(unclosed.code, DiagnosticCode::UnclosedDelimiter);
    assert_eq!(unclosed.severity, Severity::Error);
    assert_eq!(unclosed.message, "unclosed delimiter `(`");
    assert_eq!(unclosed.primary_span, Span::new(6, 7));
    assert!(unclosed.labels.is_empty());
    assert!(unclosed.notes.is_empty());
    assert_eq!(
        unclosed.help.as_deref(),
        Some("add the matching delimiter before byte 8")
    );

    let eof = &outcome.diagnostics[1];
    assert_eq!(eof.code, DiagnosticCode::UnexpectedEof);
    assert_eq!(eof.severity, Severity::Error);
    assert_eq!(eof.primary_span, Span::empty(source.len()));

    let recovered = outcome
        .value
        .expect("recovery should always produce a root");
    assert_eq!(recovered.program.span, Span::new(0, source.len()));
    assert_eq!(recovered.program.statements.len(), 1);
    let statement = &recovered.program.statements[0];
    assert_eq!(statement.span, Span::new(0, source.len()));
    assert!(matches!(statement.kind, StatementKind::Error(_)));
}
