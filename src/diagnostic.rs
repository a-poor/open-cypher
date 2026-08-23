//! Structured, parser-backend-neutral diagnostics.

use std::error::Error;
use std::fmt::{self, Write as _};

use crate::span::Span;

const TAB_WIDTH: usize = 4;

/// Machine-readable diagnostic categories.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DiagnosticCode {
    InvalidToken,
    UnterminatedString,
    UnterminatedIdentifier,
    UnterminatedComment,
    InvalidEscape,
    InvalidNumber,
    UnexpectedToken,
    UnexpectedEof,
    ExtraToken,
    UnclosedDelimiter,
    MismatchedDelimiter,
    Recovery,
    TooManyErrors,
    UnsupportedSyntax,
    Internal,
}

impl DiagnosticCode {
    /// Returns the stable short code used in rendered diagnostics.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidToken => "OCY-L001",
            Self::UnterminatedString => "OCY-L002",
            Self::UnterminatedIdentifier => "OCY-L003",
            Self::UnterminatedComment => "OCY-L004",
            Self::InvalidEscape => "OCY-L005",
            Self::InvalidNumber => "OCY-L006",
            Self::UnexpectedToken => "OCY-P001",
            Self::UnexpectedEof => "OCY-P002",
            Self::ExtraToken => "OCY-P003",
            Self::UnclosedDelimiter => "OCY-P004",
            Self::MismatchedDelimiter => "OCY-P005",
            Self::Recovery => "OCY-P006",
            Self::TooManyErrors => "OCY-P007",
            Self::UnsupportedSyntax => "OCY-P008",
            Self::Internal => "OCY-P999",
        }
    }
}

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// The importance of a diagnostic.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Severity {
    Error,
    Warning,
    Note,
}

impl Severity {
    /// Returns the lowercase display spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Note => "note",
        }
    }

    const fn sort_key(self) -> u8 {
        match self {
            Self::Error => 0,
            Self::Warning => 1,
            Self::Note => 2,
        }
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A secondary source annotation attached to a diagnostic.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Label {
    /// The source range being annotated.
    pub span: Span,
    /// A concise explanation of the range's relevance.
    pub message: String,
}

impl Label {
    /// Creates a secondary source annotation.
    #[must_use]
    pub fn new(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
        }
    }
}

/// A structured lexer or parser diagnostic.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Diagnostic {
    /// Stable machine-readable category.
    pub code: DiagnosticCode,
    /// Diagnostic importance.
    pub severity: Severity,
    /// Primary human-readable explanation.
    pub message: String,
    /// Primary source range.
    pub primary_span: Span,
    /// Additional annotated source ranges.
    pub labels: Vec<Label>,
    /// Supplemental explanatory notes.
    pub notes: Vec<String>,
    /// An actionable correction, when one is known.
    pub help: Option<String>,
}

impl Diagnostic {
    /// Creates a diagnostic with no secondary annotations.
    #[must_use]
    pub fn new(
        code: DiagnosticCode,
        severity: Severity,
        message: impl Into<String>,
        primary_span: Span,
    ) -> Self {
        Self {
            code,
            severity,
            message: message.into(),
            primary_span,
            labels: Vec::new(),
            notes: Vec::new(),
            help: None,
        }
    }

    /// Creates an error diagnostic.
    #[must_use]
    pub fn error(code: DiagnosticCode, message: impl Into<String>, primary_span: Span) -> Self {
        Self::new(code, Severity::Error, message, primary_span)
    }

    /// Adds a secondary source annotation.
    #[must_use]
    pub fn with_label(mut self, span: Span, message: impl Into<String>) -> Self {
        self.labels.push(Label::new(span, message));
        self
    }

    /// Adds a supplemental note.
    #[must_use]
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    /// Adds an actionable correction.
    #[must_use]
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Returns `true` if this diagnostic makes a strict parse fail.
    #[must_use]
    pub const fn is_error(&self) -> bool {
        matches!(self.severity, Severity::Error)
    }

    /// Renders this diagnostic against `source` without terminal colors.
    ///
    /// Line and column numbers are one-based. Columns count Unicode scalar
    /// values after expanding tabs to four-column tab stops.
    #[must_use]
    pub fn render(&self, source: &str) -> String {
        let mut rendered = String::new();
        render_one(&mut rendered, source, self).expect("writing to a String cannot fail");
        rendered
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}[{}]: {} at {}",
            self.severity, self.code, self.message, self.primary_span
        )
    }
}

/// One or more diagnostics returned by strict parsing.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ParseErrors {
    diagnostics: Vec<Diagnostic>,
}

impl ParseErrors {
    /// Creates an error collection and puts diagnostics in deterministic source
    /// order.
    ///
    /// # Panics
    ///
    /// Panics when `diagnostics` is empty. Use [`Self::try_new`] when emptiness
    /// is expected.
    #[must_use]
    pub fn new(mut diagnostics: Vec<Diagnostic>) -> Self {
        assert!(
            !diagnostics.is_empty(),
            "ParseErrors requires at least one diagnostic"
        );
        sort_diagnostics(&mut diagnostics);
        Self { diagnostics }
    }

    /// Creates an error collection, returning `None` for an empty vector.
    #[must_use]
    pub fn try_new(diagnostics: Vec<Diagnostic>) -> Option<Self> {
        if diagnostics.is_empty() {
            None
        } else {
            Some(Self::new(diagnostics))
        }
    }

    /// Returns the diagnostics in deterministic source order.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Consumes the collection and returns its diagnostics.
    #[must_use]
    pub fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
    }

    /// Returns the number of diagnostics.
    #[must_use]
    pub fn len(&self) -> usize {
        self.diagnostics.len()
    }

    /// Returns whether the collection is empty.
    ///
    /// Values created through the Rust constructors are always nonempty. This
    /// method still checks the backing collection so data deserialized through
    /// the optional `serde` feature is reported faithfully.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    /// Renders all diagnostics against `source` in deterministic source order.
    #[must_use]
    pub fn render(&self, source: &str) -> String {
        let mut rendered = String::new();
        for (index, diagnostic) in self.diagnostics.iter().enumerate() {
            if index != 0 {
                rendered.push_str("\n\n");
            }
            render_one(&mut rendered, source, diagnostic).expect("writing to a String cannot fail");
        }
        rendered
    }
}

impl From<Diagnostic> for ParseErrors {
    fn from(diagnostic: Diagnostic) -> Self {
        Self::new(vec![diagnostic])
    }
}

impl AsRef<[Diagnostic]> for ParseErrors {
    fn as_ref(&self) -> &[Diagnostic] {
        self.diagnostics()
    }
}

impl<'errors> IntoIterator for &'errors ParseErrors {
    type Item = &'errors Diagnostic;
    type IntoIter = std::slice::Iter<'errors, Diagnostic>;

    fn into_iter(self) -> Self::IntoIter {
        self.diagnostics.iter()
    }
}

impl IntoIterator for ParseErrors {
    type Item = Diagnostic;
    type IntoIter = std::vec::IntoIter<Diagnostic>;

    fn into_iter(self) -> Self::IntoIter {
        self.diagnostics.into_iter()
    }
}

impl fmt::Display for ParseErrors {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(first) = self.diagnostics.first() else {
            return formatter.write_str("no parse diagnostics");
        };
        if self.diagnostics.len() == 1 {
            first.fmt(formatter)
        } else {
            write!(
                formatter,
                "{} (and {} more diagnostic{})",
                first,
                self.diagnostics.len() - 1,
                if self.diagnostics.len() == 2 { "" } else { "s" }
            )
        }
    }
}

impl Error for ParseErrors {}

fn sort_diagnostics(diagnostics: &mut [Diagnostic]) {
    diagnostics.sort_by(|left, right| {
        (
            left.primary_span.start,
            left.primary_span.end,
            left.severity.sort_key(),
            left.code,
            left.message.as_str(),
        )
            .cmp(&(
                right.primary_span.start,
                right.primary_span.end,
                right.severity.sort_key(),
                right.code,
                right.message.as_str(),
            ))
    });
}

fn render_one(output: &mut String, source: &str, diagnostic: &Diagnostic) -> fmt::Result {
    writeln!(
        output,
        "{}[{}]: {}",
        diagnostic.severity, diagnostic.code, diagnostic.message
    )?;

    let primary = snippet(source, diagnostic.primary_span);
    writeln!(output, " --> {}:{}", primary.line_number, primary.column)?;
    writeln!(output, "  |")?;

    let primary_label = diagnostic
        .labels
        .iter()
        .find(|label| label.span == diagnostic.primary_span)
        .map(|label| label.message.as_str())
        .unwrap_or("");
    render_snippet(output, &primary, '^', primary_label)?;

    let mut labels: Vec<_> = diagnostic
        .labels
        .iter()
        .filter(|label| label.span != diagnostic.primary_span)
        .collect();
    labels.sort_by(|left, right| {
        (left.span.start, left.span.end, left.message.as_str()).cmp(&(
            right.span.start,
            right.span.end,
            right.message.as_str(),
        ))
    });

    for label in labels {
        writeln!(output, "  |")?;
        let secondary = snippet(source, label.span);
        render_snippet(output, &secondary, '-', &label.message)?;
    }

    for note in &diagnostic.notes {
        writeln!(output, "  = note: {note}")?;
    }
    if let Some(help) = &diagnostic.help {
        writeln!(output, "  = help: {help}")?;
    }

    if output.ends_with('\n') {
        output.pop();
    }
    Ok(())
}

fn render_snippet(
    output: &mut String,
    snippet: &Snippet,
    marker: char,
    label: &str,
) -> fmt::Result {
    let gutter_width = decimal_width(snippet.line_number);
    writeln!(
        output,
        "{:>gutter_width$} | {}",
        snippet.line_number, snippet.line
    )?;
    write!(
        output,
        "{:>gutter_width$} | {}{}",
        "",
        " ".repeat(snippet.marker_start),
        marker.to_string().repeat(snippet.marker_len)
    )?;
    if !label.is_empty() {
        write!(output, " {label}")?;
    }
    if snippet.continues {
        write!(output, " (continues on the next line)")?;
    }
    writeln!(output)
}

struct Snippet {
    line_number: usize,
    column: usize,
    line: String,
    marker_start: usize,
    marker_len: usize,
    continues: bool,
}

fn snippet(source: &str, span: Span) -> Snippet {
    let start = floor_char_boundary(source, span.start.min(source.len()));
    let requested_end = floor_char_boundary(source, span.end.min(source.len()));
    let before = &source[..start];
    let line_start = before.rfind('\n').map_or(0, |index| index + 1);
    let line_number = before.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let mut line_end = source[start..]
        .find('\n')
        .map_or(source.len(), |offset| start + offset);
    if line_end > line_start && source.as_bytes()[line_end - 1] == b'\r' {
        line_end -= 1;
    }

    let visible_start = start.min(line_end);
    let visible_end = requested_end.min(line_end).max(visible_start);
    let marker_start = display_width(&source[line_start..visible_start]);
    let marker_len = display_width(&source[visible_start..visible_end]).max(1);

    Snippet {
        line_number,
        column: marker_start + 1,
        line: expand_tabs(&source[line_start..line_end]),
        marker_start,
        marker_len,
        continues: requested_end > line_end,
    }
}

fn floor_char_boundary(source: &str, mut offset: usize) -> usize {
    while offset > 0 && !source.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn display_width(text: &str) -> usize {
    let mut column = 0;
    for character in text.chars() {
        if character == '\t' {
            column += TAB_WIDTH - (column % TAB_WIDTH);
        } else {
            column += 1;
        }
    }
    column
}

fn expand_tabs(text: &str) -> String {
    let mut expanded = String::with_capacity(text.len());
    let mut column = 0;
    for character in text.chars() {
        if character == '\t' {
            let spaces = TAB_WIDTH - (column % TAB_WIDTH);
            expanded.extend(std::iter::repeat_n(' ', spaces));
            column += spaces;
        } else {
            expanded.push(character);
            column += 1;
        }
    }
    expanded
}

fn decimal_width(mut value: usize) -> usize {
    let mut width = 1;
    while value >= 10 {
        value /= 10;
        width += 1;
    }
    width
}
