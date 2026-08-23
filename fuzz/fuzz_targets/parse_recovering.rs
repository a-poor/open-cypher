#![no_main]

use libfuzzer_sys::fuzz_target;
use open_cypher::{DiagnosticCode, lex, parse_recovering};

const MAX_RECOVERY_DIAGNOSTICS: usize = 32;

fuzz_target!(|data: &[u8]| {
    let source = String::from_utf8_lossy(data);
    let source = source.as_ref();

    let outcome = parse_recovering(source);
    assert!(
        outcome.diagnostics.len() <= MAX_RECOVERY_DIAGNOSTICS,
        "recovery emitted more than {MAX_RECOVERY_DIAGNOSTICS} diagnostics"
    );
    for diagnostic in &outcome.diagnostics {
        assert_ne!(diagnostic.code, DiagnosticCode::Internal);
        assert!(diagnostic.primary_span.start <= diagnostic.primary_span.end);
        assert!(diagnostic.primary_span.end <= source.len());
        assert!(source.is_char_boundary(diagnostic.primary_span.start));
        assert!(source.is_char_boundary(diagnostic.primary_span.end));
    }
    if let Some(parsed) = &outcome.value {
        assert_eq!(parsed.tokens, lex(source).tokens);
        assert!(parsed.program.span.end <= source.len());
    }
    std::hint::black_box(outcome.value);
});
