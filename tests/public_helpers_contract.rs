use std::ops::Range;

use open_cypher::ast::{Node, Program};
use open_cypher::{
    Diagnostic, DiagnosticCode, Keyword, ParseErrors, Severity, Span, Token, TokenKind,
};

#[test]
fn node_program_and_span_helpers_preserve_their_public_contracts() {
    let node = Node::new(String::from("value"), Span::new(2, 7));
    let borrowed = node.as_ref();
    assert_eq!(borrowed.kind, "value");
    assert_eq!(borrowed.span, Span::new(2, 7));

    let mapped = node.map(String::into_bytes);
    assert_eq!(mapped.kind, b"value");
    assert_eq!(mapped.span, Span::new(2, 7));
    assert!(Program::default().is_empty());

    let outer = Span::new(2, 9);
    assert_eq!(outer.start(), 2);
    assert_eq!(outer.end(), 9);
    assert!(outer.contains_span(Span::new(3, 8)));
    assert!(!outer.contains_span(Span::new(1, 8)));
    assert_eq!(outer.cover(Span::new(1, 4)), Span::new(1, 9));
    assert_eq!(outer.cover(Span::new(4, 12)), Span::new(2, 12));

    let from_range = Span::from(4..8);
    let as_range: Range<usize> = from_range.into();
    assert_eq!(as_range, 4..8);
    assert_eq!(format!("{from_range:?}"), "4..8");
    assert_eq!(from_range.to_string(), "4..8");
}

#[test]
fn every_token_kind_has_a_stable_human_readable_name() {
    let kinds = [
        TokenKind::Keyword(Keyword::Where),
        TokenKind::Identifier,
        TokenKind::EscapedIdentifier,
        TokenKind::Parameter,
        TokenKind::Integer,
        TokenKind::HexInteger,
        TokenKind::OctalInteger,
        TokenKind::Float,
        TokenKind::String,
        TokenKind::LeftParen,
        TokenKind::RightParen,
        TokenKind::LeftBracket,
        TokenKind::RightBracket,
        TokenKind::LeftBrace,
        TokenKind::RightBrace,
        TokenKind::Comma,
        TokenKind::Dot,
        TokenKind::DotDot,
        TokenKind::Colon,
        TokenKind::DoubleColon,
        TokenKind::Semicolon,
        TokenKind::Pipe,
        TokenKind::DoublePipe,
        TokenKind::Ampersand,
        TokenKind::Question,
        TokenKind::Dollar,
        TokenKind::Plus,
        TokenKind::Minus,
        TokenKind::Star,
        TokenKind::Slash,
        TokenKind::Percent,
        TokenKind::Caret,
        TokenKind::Bang,
        TokenKind::Tilde,
        TokenKind::Equal,
        TokenKind::NotEqual,
        TokenKind::Less,
        TokenKind::LessEqual,
        TokenKind::Greater,
        TokenKind::GreaterEqual,
        TokenKind::PlusEqual,
        TokenKind::FatArrow,
        TokenKind::RegexMatch,
        TokenKind::LeftArrow,
        TokenKind::RightArrow,
        TokenKind::Whitespace,
        TokenKind::LineComment,
        TokenKind::BlockComment,
        TokenKind::Invalid,
    ];

    for kind in kinds {
        assert!(!kind.display_name().is_empty());
        assert_eq!(kind.to_string(), kind.display_name());
    }

    let literals = [
        TokenKind::Integer,
        TokenKind::HexInteger,
        TokenKind::OctalInteger,
        TokenKind::Float,
        TokenKind::String,
        TokenKind::Keyword(Keyword::True),
        TokenKind::Keyword(Keyword::False),
        TokenKind::Keyword(Keyword::Null),
    ];
    assert!(literals.into_iter().all(TokenKind::is_literal));
    assert!(!TokenKind::Identifier.is_literal());
    assert_eq!(Keyword::Where.to_string(), "WHERE");

    let token = Token::new(TokenKind::Identifier, Span::new(0, 1));
    assert_eq!(token.text("x"), Some("x"));
    assert!(!token.is_trivia());
}

#[test]
fn all_diagnostic_metadata_is_stable_and_displayable() {
    let codes = [
        (DiagnosticCode::InvalidToken, "OCY-L001"),
        (DiagnosticCode::UnterminatedString, "OCY-L002"),
        (DiagnosticCode::UnterminatedIdentifier, "OCY-L003"),
        (DiagnosticCode::UnterminatedComment, "OCY-L004"),
        (DiagnosticCode::InvalidEscape, "OCY-L005"),
        (DiagnosticCode::InvalidNumber, "OCY-L006"),
        (DiagnosticCode::UnexpectedToken, "OCY-P001"),
        (DiagnosticCode::UnexpectedEof, "OCY-P002"),
        (DiagnosticCode::ExtraToken, "OCY-P003"),
        (DiagnosticCode::UnclosedDelimiter, "OCY-P004"),
        (DiagnosticCode::MismatchedDelimiter, "OCY-P005"),
        (DiagnosticCode::Recovery, "OCY-P006"),
        (DiagnosticCode::TooManyErrors, "OCY-P007"),
        (DiagnosticCode::UnsupportedSyntax, "OCY-P008"),
        (DiagnosticCode::Internal, "OCY-P999"),
    ];
    for (code, expected) in codes {
        assert_eq!(code.as_str(), expected);
        assert_eq!(code.to_string(), expected);
    }

    for (severity, expected) in [
        (Severity::Error, "error"),
        (Severity::Warning, "warning"),
        (Severity::Note, "note"),
    ] {
        assert_eq!(severity.as_str(), expected);
        assert_eq!(severity.to_string(), expected);
    }
}

#[test]
fn parse_error_collection_supports_sorting_iteration_and_rendering() {
    let same_location_note = Diagnostic::new(
        DiagnosticCode::Recovery,
        Severity::Note,
        "third by severity",
        Span::new(12, 13),
    );
    let same_location_warning = Diagnostic::new(
        DiagnosticCode::UnsupportedSyntax,
        Severity::Warning,
        "second by severity",
        Span::new(12, 13),
    );
    let first = Diagnostic::error(
        DiagnosticCode::UnexpectedToken,
        "first by location",
        Span::new(1, 3),
    );
    let errors = ParseErrors::try_new(vec![
        same_location_note,
        same_location_warning,
        first.clone(),
    ])
    .expect("the collection is nonempty");

    assert_eq!(errors.len(), 3);
    assert!(!errors.is_empty());
    assert_eq!(errors.as_ref(), errors.diagnostics());
    assert_eq!(errors.diagnostics()[0], first);
    assert_eq!(errors.diagnostics()[1].severity, Severity::Warning);
    assert_eq!(errors.diagnostics()[2].severity, Severity::Note);
    assert_eq!((&errors).into_iter().count(), 3);
    assert_eq!(errors.clone().into_iter().count(), 3);
    assert_eq!(errors.clone().into_diagnostics().len(), 3);

    assert!(errors.to_string().contains("and 2 more diagnostics"));
    assert!(errors.render("abcdefghijklmnop").contains("\n\n"));

    let one = ParseErrors::from(first.clone());
    assert_eq!(one.to_string(), first.to_string());
    let two = ParseErrors::new(vec![
        first,
        Diagnostic::error(DiagnosticCode::ExtraToken, "second", Span::new(4, 5)),
    ]);
    assert!(two.to_string().contains("and 1 more diagnostic)"));
    assert!(ParseErrors::try_new(Vec::new()).is_none());
}

#[test]
fn diagnostic_rendering_handles_tabs_unicode_crlf_multiline_and_labels() {
    let source = "zero\r\n\téclair continues\nsecond line\nthird line\n4\n5\n6\n7\n8\n9\nlast";
    let start = source.find('é').expect("fixture contains unicode");
    let end = source.find("second").expect("fixture has a second line") + 3;
    let last = source.rfind("last").expect("fixture has a final line");

    let diagnostic = Diagnostic::new(
        DiagnosticCode::UnsupportedSyntax,
        Severity::Warning,
        "complex source rendering",
        Span::new(start + 1, end),
    )
    .with_label(Span::new(start + 1, end), "primary label")
    .with_label(Span::new(last, last + 4), "later label")
    .with_label(Span::new(0, 4), "earlier label")
    .with_note("a note")
    .with_help("some help");

    assert!(!diagnostic.is_error());
    let rendered = diagnostic.render(source);
    assert!(rendered.contains("warning[OCY-P008]"));
    assert!(rendered.contains("primary label"));
    assert!(rendered.contains("earlier label"));
    assert!(rendered.contains("later label"));
    assert!(rendered.contains("continues on the next line"));
    assert!(rendered.contains("    éclair continues"));
    assert!(rendered.contains("11 | last"));
    assert!(rendered.contains("= note: a note"));
    assert!(rendered.contains("= help: some help"));
    assert!(diagnostic.to_string().contains("at "));
}

#[cfg(feature = "serde")]
#[test]
fn deserialized_empty_parse_errors_report_their_actual_state() {
    let errors: ParseErrors =
        serde_json::from_str(r#"{"diagnostics":[]}"#).expect("valid serialized ParseErrors");
    assert!(errors.is_empty());
    assert_eq!(errors.to_string(), "no parse diagnostics");
    assert_eq!(errors.render(""), "");
}
