//! Backend-neutral lexical tokens.
//!
//! Tokens contain only their kind and source span. The original spelling is
//! intentionally not copied: use [`Token::text`] with the source query when a
//! lossless representation is needed.

use std::fmt;

use crate::span::Span;

/// A token produced by the lexer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Token {
    /// The lexical class of the token.
    pub kind: TokenKind,
    /// The token's half-open UTF-8 byte range.
    pub span: Span,
}

impl Token {
    /// Creates a token.
    #[must_use]
    pub const fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }

    /// Returns the original token spelling, when `span` is valid for `source`.
    #[must_use]
    pub fn text(self, source: &str) -> Option<&str> {
        self.span.text(source)
    }

    /// Returns `true` for whitespace and comments.
    #[must_use]
    pub const fn is_trivia(self) -> bool {
        self.kind.is_trivia()
    }
}

/// A lexical token class understood by the openCypher parser.
///
/// Keyword recognition is case-insensitive. The parser reclassifies
/// non-reserved [`Keyword`] tokens contextually when a name is required.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TokenKind {
    /// A case-insensitive language keyword.
    Keyword(Keyword),
    /// An unescaped symbolic name.
    Identifier,
    /// A backtick-delimited symbolic name.
    EscapedIdentifier,
    /// A parameter whose separated name is unescaped, such as `$name`,
    /// `$123`, or `$1_name`.
    Parameter,
    /// A base-ten integer literal.
    Integer,
    /// A hexadecimal integer literal.
    HexInteger,
    /// An octal integer literal.
    OctalInteger,
    /// A decimal floating-point literal, optionally with an exponent.
    Float,
    /// A single- or double-quoted string literal.
    String,

    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    LeftBrace,
    RightBrace,
    Comma,
    Dot,
    DotDot,
    Colon,
    DoubleColon,
    Semicolon,
    Pipe,
    DoublePipe,
    Ampersand,
    Question,
    /// A bare `$`; the parser combines it with a following backtick-delimited
    /// identifier to form an escaped parameter name.
    Dollar,

    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Caret,
    Bang,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    PlusEqual,
    FatArrow,
    RegexMatch,
    /// A contiguous `<-`; spaced or comment-separated forms remain separate
    /// [`Less`](Self::Less) and [`Minus`](Self::Minus) tokens.
    LeftArrow,
    /// A contiguous `->`; spaced or comment-separated forms remain separate
    /// [`Minus`](Self::Minus) and [`Greater`](Self::Greater) tokens.
    RightArrow,

    /// One or more Unicode whitespace characters.
    Whitespace,
    /// A `//` comment, including its marker but excluding a line terminator.
    LineComment,
    /// A `/* ... */` comment, including its delimiters.
    BlockComment,
    /// A source region which cannot begin any valid token.
    Invalid,
}

impl TokenKind {
    /// Returns `true` for whitespace and comments.
    #[must_use]
    pub const fn is_trivia(self) -> bool {
        matches!(
            self,
            Self::Whitespace | Self::LineComment | Self::BlockComment
        )
    }

    /// Returns `true` for literal tokens.
    #[must_use]
    pub const fn is_literal(self) -> bool {
        matches!(
            self,
            Self::Integer
                | Self::HexInteger
                | Self::OctalInteger
                | Self::Float
                | Self::String
                | Self::Keyword(Keyword::True | Keyword::False | Keyword::Null)
        )
    }

    /// Returns a concise, user-facing name suitable for diagnostics.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Keyword(keyword) => keyword.as_str(),
            Self::Identifier => "an identifier",
            Self::EscapedIdentifier => "an escaped identifier",
            Self::Parameter => "a parameter",
            Self::Integer => "an integer",
            Self::HexInteger => "a hexadecimal integer",
            Self::OctalInteger => "an octal integer",
            Self::Float => "a floating-point number",
            Self::String => "a string",
            Self::LeftParen => "`(`",
            Self::RightParen => "`)`",
            Self::LeftBracket => "`[`",
            Self::RightBracket => "`]`",
            Self::LeftBrace => "`{`",
            Self::RightBrace => "`}`",
            Self::Comma => "`,`",
            Self::Dot => "`.`",
            Self::DotDot => "`..`",
            Self::Colon => "`:`",
            Self::DoubleColon => "`::`",
            Self::Semicolon => "`;`",
            Self::Pipe => "`|`",
            Self::DoublePipe => "`||`",
            Self::Ampersand => "`&`",
            Self::Question => "`?`",
            Self::Dollar => "`$`",
            Self::Plus => "`+`",
            Self::Minus => "`-`",
            Self::Star => "`*`",
            Self::Slash => "`/`",
            Self::Percent => "`%`",
            Self::Caret => "`^`",
            Self::Bang => "`!`",
            Self::Equal => "`=`",
            Self::NotEqual => "`<>` or `!=`",
            Self::Less => "`<`",
            Self::LessEqual => "`<=`",
            Self::Greater => "`>`",
            Self::GreaterEqual => "`>=`",
            Self::PlusEqual => "`+=`",
            Self::FatArrow => "`=>`",
            Self::RegexMatch => "`=~`",
            Self::LeftArrow => "`<-`",
            Self::RightArrow => "`->`",
            Self::Whitespace => "whitespace",
            Self::LineComment => "a line comment",
            Self::BlockComment => "a block comment",
            Self::Invalid => "an invalid token",
        }
    }
}

impl fmt::Display for TokenKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.display_name())
    }
}

/// A word recognized specially by this crate's lexer.
///
/// This contains the openCypher 2024.3 keyword spellings plus the documented
/// path-mode and subquery-expression extensions consumed by the grammar. The
/// parser decides whether a keyword can act as a symbolic name at a particular
/// location.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Keyword {
    Acyclic,
    All,
    AllShortestPaths,
    And,
    Any,
    As,
    Asc,
    Ascending,
    By,
    Call,
    Case,
    Collect,
    Contains,
    Count,
    Create,
    Delete,
    Desc,
    Descending,
    Detach,
    Distinct,
    Else,
    End,
    Ends,
    Exists,
    False,
    Group,
    Groups,
    In,
    Inf,
    Infinity,
    Is,
    Limit,
    Match,
    Merge,
    Nan,
    None,
    Not,
    Null,
    Offset,
    On,
    Optional,
    Or,
    Order,
    Path,
    Paths,
    Reduce,
    Remove,
    Return,
    Set,
    Shortest,
    ShortestPath,
    Simple,
    Single,
    Skip,
    Starts,
    Then,
    Trail,
    Trim,
    True,
    Union,
    Unwind,
    Walk,
    When,
    Where,
    With,
    Xor,
    Yield,
}

impl Keyword {
    /// Returns the canonical uppercase spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Acyclic => "ACYCLIC",
            Self::All => "ALL",
            Self::AllShortestPaths => "ALLSHORTESTPATHS",
            Self::And => "AND",
            Self::Any => "ANY",
            Self::As => "AS",
            Self::Asc => "ASC",
            Self::Ascending => "ASCENDING",
            Self::By => "BY",
            Self::Call => "CALL",
            Self::Case => "CASE",
            Self::Collect => "COLLECT",
            Self::Contains => "CONTAINS",
            Self::Count => "COUNT",
            Self::Create => "CREATE",
            Self::Delete => "DELETE",
            Self::Desc => "DESC",
            Self::Descending => "DESCENDING",
            Self::Detach => "DETACH",
            Self::Distinct => "DISTINCT",
            Self::Else => "ELSE",
            Self::End => "END",
            Self::Ends => "ENDS",
            Self::Exists => "EXISTS",
            Self::False => "FALSE",
            Self::Group => "GROUP",
            Self::Groups => "GROUPS",
            Self::In => "IN",
            Self::Inf => "INF",
            Self::Infinity => "INFINITY",
            Self::Is => "IS",
            Self::Limit => "LIMIT",
            Self::Match => "MATCH",
            Self::Merge => "MERGE",
            Self::Nan => "NAN",
            Self::None => "NONE",
            Self::Not => "NOT",
            Self::Null => "NULL",
            Self::Offset => "OFFSET",
            Self::On => "ON",
            Self::Optional => "OPTIONAL",
            Self::Or => "OR",
            Self::Order => "ORDER",
            Self::Path => "PATH",
            Self::Paths => "PATHS",
            Self::Reduce => "REDUCE",
            Self::Remove => "REMOVE",
            Self::Return => "RETURN",
            Self::Set => "SET",
            Self::Shortest => "SHORTEST",
            Self::ShortestPath => "SHORTESTPATH",
            Self::Simple => "SIMPLE",
            Self::Single => "SINGLE",
            Self::Skip => "SKIP",
            Self::Starts => "STARTS",
            Self::Then => "THEN",
            Self::Trail => "TRAIL",
            Self::Trim => "TRIM",
            Self::True => "TRUE",
            Self::Union => "UNION",
            Self::Unwind => "UNWIND",
            Self::Walk => "WALK",
            Self::When => "WHEN",
            Self::Where => "WHERE",
            Self::With => "WITH",
            Self::Xor => "XOR",
            Self::Yield => "YIELD",
        }
    }

    /// Recognizes an ASCII keyword without regard to case.
    ///
    /// Non-ASCII input is never a keyword.
    #[must_use]
    pub fn from_ascii_case_insensitive(text: &str) -> Option<Self> {
        if !text.is_ascii() {
            return None;
        }

        let candidates: &[Keyword] = match text.len() {
            2 => &[Self::As, Self::By, Self::In, Self::Is, Self::On, Self::Or],
            3 => &[
                Self::All,
                Self::And,
                Self::Any,
                Self::Asc,
                Self::End,
                Self::Inf,
                Self::Nan,
                Self::Not,
                Self::Set,
                Self::Xor,
            ],
            4 => &[
                Self::Call,
                Self::Case,
                Self::Desc,
                Self::Else,
                Self::Ends,
                Self::None,
                Self::Null,
                Self::Path,
                Self::Skip,
                Self::Then,
                Self::Trim,
                Self::True,
                Self::Walk,
                Self::When,
                Self::With,
            ],
            5 => &[
                Self::Count,
                Self::False,
                Self::Group,
                Self::Limit,
                Self::Match,
                Self::Merge,
                Self::Order,
                Self::Paths,
                Self::Trail,
                Self::Union,
                Self::Where,
                Self::Yield,
            ],
            6 => &[
                Self::Create,
                Self::Delete,
                Self::Detach,
                Self::Exists,
                Self::Groups,
                Self::Offset,
                Self::Reduce,
                Self::Remove,
                Self::Return,
                Self::Simple,
                Self::Single,
                Self::Starts,
                Self::Unwind,
            ],
            7 => &[Self::Acyclic, Self::Collect],
            8 => &[
                Self::Contains,
                Self::Distinct,
                Self::Optional,
                Self::Shortest,
                Self::Infinity,
            ],
            9 => &[Self::Ascending],
            10 => &[Self::Descending],
            12 => &[Self::ShortestPath],
            16 => &[Self::AllShortestPaths],
            _ => return None,
        };

        candidates
            .iter()
            .copied()
            .find(|keyword| text.eq_ignore_ascii_case(keyword.as_str()))
    }
}

impl fmt::Display for Keyword {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}
