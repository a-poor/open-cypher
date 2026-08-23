#![no_main]

use libfuzzer_sys::fuzz_target;
use open_cypher::{lex, parse};

fuzz_target!(|data: &[u8]| {
    let source = String::from_utf8_lossy(data);
    let source = source.as_ref();

    match parse(source) {
        Ok(parsed) => {
            let lexed = lex(source);
            assert!(lexed.diagnostics.is_empty());
            assert_eq!(parsed.tokens, lexed.tokens);

            let mut cursor = 0;
            for token in &parsed.tokens {
                assert_eq!(token.span.start, cursor, "the token stream contains a gap");
                assert!(token.span.end > token.span.start);
                assert!(token.span.end <= source.len());
                assert!(source.is_char_boundary(token.span.start));
                assert!(source.is_char_boundary(token.span.end));
                cursor = token.span.end;
            }
            assert_eq!(cursor, source.len());
            assert!(parsed.program.span.end <= source.len());
            std::hint::black_box(parsed);
        }
        Err(errors) => {
            for diagnostic in errors.diagnostics() {
                assert!(diagnostic.primary_span.start <= diagnostic.primary_span.end);
                assert!(diagnostic.primary_span.end <= source.len());
                assert!(source.is_char_boundary(diagnostic.primary_span.start));
                assert!(source.is_char_boundary(diagnostic.primary_span.end));
            }
            std::hint::black_box(errors);
        }
    }
});
