use open_cypher::{DiagnosticCode, Keyword, TokenKind, lex};

fn assert_lossless_spans(source: &str) {
    let outcome = lex(source);
    let mut cursor = 0;

    for token in &outcome.tokens {
        assert_eq!(
            token.span.start, cursor,
            "tokens must cover the source without gaps: {source:?}"
        );
        assert!(token.span.end > token.span.start, "tokens must be nonempty");
        assert!(source.is_char_boundary(token.span.start));
        assert!(source.is_char_boundary(token.span.end));
        assert_eq!(token.text(source), source.get(token.span.range()));
        cursor = token.span.end;
    }

    assert_eq!(
        cursor,
        source.len(),
        "tokens must cover the complete source"
    );
    for diagnostic in &outcome.diagnostics {
        assert!(diagnostic.primary_span.start <= diagnostic.primary_span.end);
        assert!(diagnostic.primary_span.end <= source.len());
        assert!(source.is_char_boundary(diagnostic.primary_span.start));
        assert!(source.is_char_boundary(diagnostic.primary_span.end));
    }
}

fn kinds(source: &str) -> Vec<String> {
    let outcome = lex(source);
    assert!(
        outcome.diagnostics.is_empty(),
        "unexpected lexical diagnostics for {source:?}: {:#?}",
        outcome.diagnostics
    );
    outcome
        .tokens
        .into_iter()
        .map(|token| format!("{:?}", token.kind))
        .collect()
}

fn significant_kinds(source: &str) -> Vec<TokenKind> {
    let outcome = lex(source);
    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    outcome
        .tokens
        .into_iter()
        .filter(|token| !token.is_trivia())
        .map(|token| token.kind)
        .collect()
}

#[test]
fn keywords_are_ascii_case_insensitive() {
    assert_eq!(
        kinds("MATCH (n) WHERE n.active = TRUE RETURN n"),
        kinds("mAtCh (n) wHeRe n.active = tRuE rEtUrN n")
    );
}

#[test]
fn keyword_prefixes_remain_identifiers() {
    let prefixed = kinds("MATCH (notable) RETURN notable");
    let ordinary = kinds("MATCH (variable) RETURN variable");

    assert_eq!(
        prefixed.len(),
        ordinary.len(),
        "`notable` must not lex as `NOT` followed by another token"
    );
}

#[test]
fn overlapping_operators_use_maximal_munch() {
    assert_eq!(significant_kinds("<"), [TokenKind::Less]);
    assert_eq!(significant_kinds("<="), [TokenKind::LessEqual]);
    assert_eq!(significant_kinds(">"), [TokenKind::Greater]);
    assert_eq!(significant_kinds(">="), [TokenKind::GreaterEqual]);
    assert_eq!(significant_kinds("<>"), [TokenKind::NotEqual]);
    assert_eq!(significant_kinds("=~"), [TokenKind::RegexMatch]);
    assert_eq!(significant_kinds("<-"), [TokenKind::LeftArrow]);
    assert_eq!(significant_kinds("->"), [TokenKind::RightArrow]);
}

#[test]
fn bare_tilde_is_not_an_open_cypher_token() {
    let outcome = lex("~");
    assert_eq!(outcome.tokens.len(), 1);
    assert_eq!(outcome.tokens[0].kind, TokenKind::Invalid);
    assert_eq!(outcome.diagnostics.len(), 1);
    assert_eq!(outcome.diagnostics[0].code, DiagnosticCode::InvalidToken);
    assert!(open_cypher::parse("RETURN ~value").is_err());
}

#[test]
fn current_path_search_words_are_keywords() {
    assert_eq!(
        significant_kinds("ANY SHORTEST ALL SHORTEST WALK TRAIL ACYCLIC SIMPLE"),
        [
            TokenKind::Keyword(Keyword::Any),
            TokenKind::Keyword(Keyword::Shortest),
            TokenKind::Keyword(Keyword::All),
            TokenKind::Keyword(Keyword::Shortest),
            TokenKind::Keyword(Keyword::Walk),
            TokenKind::Keyword(Keyword::Trail),
            TokenKind::Keyword(Keyword::Acyclic),
            TokenKind::Keyword(Keyword::Simple),
        ]
    );
}

#[test]
fn syntax_neutral_ascii_words_remain_identifiers() {
    for word in [
        "BOTH",
        "BREAK",
        "CLOSE",
        "CONSTRAINT",
        "CONTINUE",
        "CSV",
        "CURRENT",
        "DIFFERENT",
        "DO",
        "DROP",
        "DRYRUN",
        "EACH",
        "ERROR",
        "EXPLAIN",
        "FAIL",
        "FIELDTERMINATOR",
        "FILTER",
        "FINISH",
        "FOR",
        "FOREACH",
        "FIRST",
        "FROM",
        "GRAPH",
        "HEADERS",
        "INSERT",
        "INDEX",
        "JOIN",
        "LABEL",
        "LABELS",
        "LAST",
        "LEADING",
        "LET",
        "LOAD",
        "MANDATORY",
        "NEXT",
        "NODE",
        "NODETACH",
        "NORMALIZE",
        "NULLS",
        "OF",
        "ONLY",
        "PERIODIC",
        "PROFILE",
        "PROPERTY",
        "REPORT",
        "REPEATABLE",
        "REPLACE",
        "REQUIRE",
        "ROWS",
        "SAME",
        "SCALAR",
        "SCAN",
        "SEEK",
        "SELECT",
        "STATUS",
        "TRAILING",
        "TRANSACTIONS",
        "TYPED",
        "UNIQUE",
        "USE",
        "USING",
        "VALUE",
        "VALUES",
        "WITHOUT",
        "WRITE",
    ] {
        assert_eq!(
            significant_kinds(word),
            [TokenKind::Identifier],
            "syntax-neutral word was classified as a keyword: {word}"
        );
    }
}

#[test]
fn signed_decimal_exponents_are_one_float_token() {
    assert_eq!(significant_kinds("1.25e+3"), [TokenKind::Float]);
    assert_eq!(significant_kinds("1.25E-3"), [TokenKind::Float]);
}

#[test]
fn comments_and_line_endings_do_not_create_lexical_errors() {
    let source = "// heading\r\nMATCH (n) /* between clauses */\r\nRETURN n";
    let outcome = lex(source);

    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    assert!(!outcome.tokens.is_empty());
    assert_lossless_spans(source);
}

#[test]
fn unicode_and_escaped_names_are_lexed() {
    let source = "MATCH (`two words` {café: '☕'}) RETURN `two words`.café";
    let outcome = lex(source);

    assert!(outcome.diagnostics.is_empty(), "{:#?}", outcome.diagnostics);
    assert!(!outcome.tokens.is_empty());
    assert_lossless_spans(source);
}

#[test]
fn parameter_names_may_begin_with_identifier_continuation_characters() {
    assert_eq!(
        significant_kinds("$1abc $1_abc $\u{0301} $\u{00b7} $\u{203f}"),
        [
            TokenKind::Parameter,
            TokenKind::Parameter,
            TokenKind::Parameter,
            TokenKind::Parameter,
            TokenKind::Parameter,
        ]
    );
}

#[test]
fn validates_string_and_identifier_escape_sequences() {
    for source in [r"RETURN '\q'", r"RETURN '\u12xz'", r"RETURN '\U110000'"] {
        let outcome = lex(source);
        assert!(
            outcome
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::InvalidEscape),
            "expected an invalid-escape diagnostic for {source:?}"
        );
    }

    for source in [
        r"RETURN '\\'",
        r"RETURN '\u0041'",
        r"RETURN '\U01F642'",
        r"MATCH (`na\u006de`) RETURN `na\u006de`",
    ] {
        let outcome = lex(source);
        assert!(
            outcome.diagnostics.is_empty(),
            "expected valid escapes in {source:?}: {:?}",
            outcome.diagnostics
        );
    }
}

#[test]
fn malformed_literals_report_lexical_diagnostics() {
    for (source, expected_code) in [
        ("RETURN 'unterminated", DiagnosticCode::UnterminatedString),
        (
            "RETURN `unterminated",
            DiagnosticCode::UnterminatedIdentifier,
        ),
    ] {
        let outcome = lex(source);
        assert!(
            !outcome.diagnostics.is_empty(),
            "expected a lexical diagnostic for {source:?}"
        );
        assert_eq!(outcome.diagnostics[0].code, expected_code);
    }
}

#[test]
fn lexing_is_deterministic() {
    let source = "MATCH (n:Person {name: $name}) RETURN n.name";
    let first = lex(source);
    let second = lex(source);

    assert_eq!(first.tokens, second.tokens);
    assert_eq!(first.diagnostics, second.diagnostics);
}

#[test]
fn tokens_cover_valid_and_invalid_source_losslessly() {
    for source in [
        "",
        "RETURN 1",
        "  MATCH (n) // comment\nRETURN n  ",
        "MATCH (café) RETURN café",
        "RETURN 'unterminated",
        "RETURN @invalid",
    ] {
        assert_lossless_spans(source);
    }
}
