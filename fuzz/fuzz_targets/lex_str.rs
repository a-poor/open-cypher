#![no_main]

use libfuzzer_sys::fuzz_target;
use open_cypher::lex;

fuzz_target!(|data: &[u8]| {
    let source = String::from_utf8_lossy(data);
    let source = source.as_ref();

    let outcome = lex(source);
    let mut cursor = 0;
    for token in &outcome.tokens {
        assert_eq!(token.span.start, cursor, "the token stream contains a gap");
        assert!(token.span.end > token.span.start, "tokens must be nonempty");
        assert!(token.span.end <= source.len());
        assert!(source.is_char_boundary(token.span.start));
        assert!(source.is_char_boundary(token.span.end));
        assert_eq!(token.text(source), source.get(token.span.range()));
        cursor = token.span.end;
    }
    assert_eq!(
        cursor,
        source.len(),
        "the token stream must cover the input"
    );

    for diagnostic in &outcome.diagnostics {
        assert!(diagnostic.primary_span.start <= diagnostic.primary_span.end);
        assert!(diagnostic.primary_span.end <= source.len());
        assert!(source.is_char_boundary(diagnostic.primary_span.start));
        assert!(source.is_char_boundary(diagnostic.primary_span.end));
    }
});
