//! Exact-output contract tests for diagnostic rendering.
//!
//! These tests pin the precise rendered text of diagnostics for inputs chosen
//! to make small arithmetic or comparison changes in `src/diagnostic.rs`
//! observable: CRLF line endings, tabs before the error, multibyte characters,
//! three-digit line numbers, and multi-diagnostic separators.

use open_cypher::{Diagnostic, DiagnosticCode, ParseErrors, Span};

fn boom(span: Span) -> Diagnostic {
    Diagnostic::error(DiagnosticCode::UnexpectedToken, "boom", span)
}

#[test]
fn crlf_line_endings_are_trimmed_from_the_rendered_line() {
    // The error line ends in "\r\n"; the snippet must show the line without
    // the carriage return (and without the line feed).
    let source = "MATCH (n\r\nRETURN x\r\n";
    // 'x' on line 2: "MATCH (n" is bytes 0..8, "\r\n" is 8..10,
    // "RETURN x" is bytes 10..18.
    let rendered = boom(Span::new(17, 18)).render(source);
    let expected_lines = [
        "error[OCY-P001]: boom",
        " --> 2:8",
        "  |",
        "2 | RETURN x",
        "  |        ^",
    ];
    assert_eq!(rendered, expected_lines.join("\n"));
}

#[test]
fn an_empty_span_at_the_start_of_a_leading_newline_renders_an_empty_line() {
    // line_end == line_start == 0 here; the carriage-return trim must not
    // inspect the byte before offset zero.
    let source = "\nRETURN x";
    let rendered = boom(Span::empty(0)).render(source);
    let expected_lines = ["error[OCY-P001]: boom", " --> 1:1", "  |", "1 | ", "  | ^"];
    assert_eq!(rendered, expected_lines.join("\n"));
}

#[test]
fn tabs_before_the_error_expand_to_four_column_tab_stops() {
    // "a\tb\tc": 'a' is column 1, the first tab pads 3 columns (to column 4),
    // 'b' takes column 5, the second tab pads 3 columns (to column 8), and
    // 'c' sits at display column 9.
    let source = "a\tb\tc";
    let rendered = boom(Span::new(4, 5)).render(source);
    let expected_lines = [
        "error[OCY-P001]: boom",
        " --> 1:9",
        "  |",
        "1 | a   b   c",
        "  |         ^",
    ];
    assert_eq!(rendered, expected_lines.join("\n"));
}

#[test]
fn a_span_inside_a_multibyte_character_snaps_back_to_its_start() {
    // Byte offset 1 is in the middle of the two-byte "é"; the caret must
    // cover the whole character, reporting column 1.
    let source = "é = 1";
    let rendered = boom(Span::empty(1)).render(source);
    let expected_lines = [
        "error[OCY-P001]: boom",
        " --> 1:1",
        "  |",
        "1 | é = 1",
        "  | ^",
    ];
    assert_eq!(rendered, expected_lines.join("\n"));
}

#[test]
fn three_digit_line_numbers_widen_the_gutter() {
    // The error sits on line 100, so the gutter must be three columns wide
    // and the marker row must be padded to match.
    let source = format!("{}RETURN ]", "\n".repeat(99));
    let rendered = boom(Span::new(106, 107)).render(&source);
    let expected_lines = [
        "error[OCY-P001]: boom",
        " --> 100:8",
        "  |",
        "100 | RETURN ]",
        "    |        ^",
    ];
    assert_eq!(rendered, expected_lines.join("\n"));
}

#[test]
fn two_digit_line_numbers_widen_the_gutter() {
    let source = format!("{}RETURN ]", "\n".repeat(9));
    let rendered = boom(Span::new(16, 17)).render(&source);
    let expected_lines = [
        "error[OCY-P001]: boom",
        " --> 10:8",
        "  |",
        "10 | RETURN ]",
        "   |        ^",
    ];
    assert_eq!(rendered, expected_lines.join("\n"));
}

#[test]
fn multiple_diagnostics_are_separated_by_exactly_one_blank_line() {
    let source = "RETURN ]";
    let errors = ParseErrors::new(vec![
        Diagnostic::error(DiagnosticCode::UnexpectedToken, "first", Span::new(0, 6)),
        Diagnostic::error(DiagnosticCode::ExtraToken, "second", Span::new(7, 8)),
    ]);
    let rendered = errors.render(source);
    let expected_lines = [
        "error[OCY-P001]: first",
        " --> 1:1",
        "  |",
        "1 | RETURN ]",
        "  | ^^^^^^",
        "",
        "error[OCY-P003]: second",
        " --> 1:8",
        "  |",
        "1 | RETURN ]",
        "  |        ^",
    ];
    assert_eq!(rendered, expected_lines.join("\n"));
}
