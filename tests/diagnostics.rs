use open_cypher::{
    Diagnostic, DiagnosticCode, ParseErrors, Severity, Span, parse, parse_recovering,
};

const MAX_RECOVERY_DIAGNOSTICS: usize = 32;

#[test]
fn diagnostic_codes_are_stable_and_machine_readable() {
    assert_eq!(DiagnosticCode::InvalidToken.as_str(), "OCY-L001");
    assert_eq!(DiagnosticCode::UnexpectedToken.as_str(), "OCY-P001");
    assert_eq!(DiagnosticCode::UnexpectedEof.as_str(), "OCY-P002");
}

#[test]
fn parse_errors_are_nonempty_and_source_ordered() {
    assert!(ParseErrors::try_new(Vec::new()).is_none());

    let later = Diagnostic::new(
        DiagnosticCode::UnexpectedToken,
        Severity::Error,
        "later",
        Span::new(8, 9),
    );
    let earlier = Diagnostic::new(
        DiagnosticCode::InvalidToken,
        Severity::Error,
        "earlier",
        Span::new(1, 2),
    );
    let errors = ParseErrors::new(vec![later, earlier]);

    assert_eq!(errors.len(), 2);
    assert_eq!(errors.diagnostics()[0].message, "earlier");
    assert_eq!(errors.diagnostics()[1].message, "later");
}

#[test]
fn strict_parse_rejects_invalid_input() {
    let result = parse("MATCH (n RETURN n");
    assert!(result.is_err());
}

#[test]
fn recovering_parse_reports_a_useful_diagnostic() {
    let source = "MATCH (n RETURN n";
    let outcome = parse_recovering(source);
    let diagnostic = outcome
        .diagnostics
        .first()
        .expect("malformed input should produce a diagnostic");

    assert!(
        !diagnostic.message.trim().is_empty(),
        "diagnostic messages must be suitable for display"
    );
    assert!(
        !format!("{:?}", diagnostic.code).is_empty(),
        "diagnostics must carry a stable code"
    );
    assert!(
        diagnostic.primary_span.end <= source.len()
            && source.is_char_boundary(diagnostic.primary_span.start)
            && source.is_char_boundary(diagnostic.primary_span.end),
        "diagnostics must identify a valid UTF-8 source span: {:?}",
        diagnostic.primary_span
    );
}

#[test]
fn unexpected_eof_is_reported() {
    let outcome = parse_recovering("RETURN [1, 2,");

    assert!(!outcome.diagnostics.is_empty());
    assert!(
        outcome
            .diagnostics
            .iter()
            .any(|diagnostic| !diagnostic.message.trim().is_empty())
    );
}

#[test]
fn recovery_can_report_more_than_one_mistake() {
    let outcome = parse_recovering("MATCH (n RETURN n, WITH RETURN ]");

    assert!(
        outcome.diagnostics.len() >= 2,
        "expected recovery to continue after the first error: {:#?}",
        outcome.diagnostics
    );
}

#[test]
fn recovery_caps_diagnostics_on_adversarial_input() {
    let source = "RETURN ] ,".repeat(1_000);
    let outcome = parse_recovering(&source);

    assert!(
        outcome.diagnostics.len() <= MAX_RECOVERY_DIAGNOSTICS,
        "diagnostic floods must be capped"
    );
}

#[test]
fn diagnostics_are_deterministic() {
    let source = "MATCH (n WHERE n.value >= RETURN n";
    let first = parse_recovering(source);
    let second = parse_recovering(source);

    assert_eq!(first.diagnostics, second.diagnostics);
}

#[test]
fn unicode_before_an_error_is_safe_to_diagnose() {
    let outcome = parse_recovering("MATCH (café) RETURN café.");

    assert!(!outcome.diagnostics.is_empty());
    for diagnostic in outcome.diagnostics {
        assert!(!diagnostic.message.trim().is_empty());
    }
}

#[test]
fn rendered_diagnostics_include_stable_machine_and_human_context() {
    let source = "RETURN ]";
    let outcome = parse_recovering(source);
    let diagnostic = outcome
        .diagnostics
        .first()
        .expect("malformed input should produce a diagnostic");
    let rendered = diagnostic.render(source);

    assert!(rendered.contains(diagnostic.code.as_str()));
    assert!(rendered.contains(&diagnostic.message));
    assert!(rendered.contains("RETURN ]"));
}
