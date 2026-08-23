//! Source locations used throughout the lexer, parser, and syntax tree.

use std::fmt;
use std::ops::Range;

/// A half-open byte range into a UTF-8 source string.
///
/// `start` is inclusive and `end` is exclusive. Offsets are bytes, rather
/// than character or display columns, so a span can be used directly to slice
/// the original query after checking that both endpoints are UTF-8 boundaries.
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Span {
    /// Inclusive byte offset.
    pub start: usize,
    /// Exclusive byte offset.
    pub end: usize,
}

impl Span {
    /// Creates a span from an inclusive start and exclusive end offset.
    ///
    /// # Panics
    ///
    /// Panics if `start > end`.
    #[must_use]
    pub const fn new(start: usize, end: usize) -> Self {
        assert!(start <= end, "a span cannot end before it starts");
        Self { start, end }
    }

    /// Creates an empty span at `offset`.
    #[must_use]
    pub const fn empty(offset: usize) -> Self {
        Self::new(offset, offset)
    }

    /// Returns the inclusive start byte offset.
    #[must_use]
    pub const fn start(self) -> usize {
        self.start
    }

    /// Returns the exclusive end byte offset.
    #[must_use]
    pub const fn end(self) -> usize {
        self.end
    }

    /// Returns the number of source bytes covered by this span.
    #[must_use]
    pub const fn len(self) -> usize {
        self.end - self.start
    }

    /// Returns `true` when this span covers no source bytes.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// Returns `true` when `offset` is inside this half-open span.
    #[must_use]
    pub const fn contains(self, offset: usize) -> bool {
        self.start <= offset && offset < self.end
    }

    /// Returns `true` when `other` is entirely contained in this span.
    #[must_use]
    pub const fn contains_span(self, other: Self) -> bool {
        self.start <= other.start && other.end <= self.end
    }

    /// Returns the smallest span covering both inputs.
    #[must_use]
    pub const fn cover(self, other: Self) -> Self {
        Self::new(
            if self.start < other.start {
                self.start
            } else {
                other.start
            },
            if self.end > other.end {
                self.end
            } else {
                other.end
            },
        )
    }

    /// Returns this span as a standard half-open range.
    #[must_use]
    pub const fn range(self) -> Range<usize> {
        self.start..self.end
    }

    /// Returns the source text covered by this span.
    ///
    /// `None` is returned if the span is outside `source` or either endpoint
    /// falls in the middle of a UTF-8 code point.
    #[must_use]
    pub fn text(self, source: &str) -> Option<&str> {
        source.get(self.range())
    }
}

impl From<Range<usize>> for Span {
    fn from(range: Range<usize>) -> Self {
        Self::new(range.start, range.end)
    }
}

impl From<Span> for Range<usize> {
    fn from(span: Span) -> Self {
        span.range()
    }
}

impl fmt::Debug for Span {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}..{}", self.start, self.end)
    }
}

impl fmt::Display for Span {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, formatter)
    }
}
