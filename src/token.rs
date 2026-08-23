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
/// Keyword recognition is case-insensitive. The alpha parser accepts some
/// [`Keyword`] tokens contextually as identifiers; complete non-reserved-word
/// handling remains tracked as a conformance gap.
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
    /// A `$name` or `$digits` parameter reference.
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
    Dollar,

    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Caret,
    Bang,
    Tilde,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    PlusEqual,
    FatArrow,
    RegexMatch,
    LeftArrow,
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
            Self::Tilde => "`~`",
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
/// This is intentionally a superset of the openCypher 2024.3 reserved and
/// non-reserved words: it also includes explicitly documented compatibility
/// extensions. The parser decides whether a keyword can act as a symbolic name
/// at a particular location.
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
    Both,
    Break,
    By,
    Call,
    Case,
    Close,
    Collect,
    Constraint,
    Contains,
    Continue,
    Count,
    Create,
    Csv,
    Current,
    Delete,
    Desc,
    Descending,
    Detach,
    Different,
    Distinct,
    Do,
    Drop,
    Dryrun,
    Each,
    Else,
    End,
    Ends,
    Error,
    Exists,
    Explain,
    Fail,
    False,
    Fieldterminator,
    Filter,
    Finish,
    For,
    Foreach,
    First,
    From,
    Graph,
    Group,
    Groups,
    Headers,
    In,
    Inf,
    Infinity,
    Insert,
    Index,
    Is,
    Join,
    Label,
    Labels,
    Last,
    Leading,
    Let,
    Limit,
    Load,
    Mandatory,
    Match,
    Merge,
    Nan,
    Next,
    Node,
    Nodetach,
    None,
    Normalize,
    Not,
    Null,
    Nulls,
    Of,
    Offset,
    On,
    Only,
    Optional,
    Or,
    Order,
    Path,
    Paths,
    Periodic,
    Profile,
    Property,
    Reduce,
    Remove,
    Report,
    Repeatable,
    Replace,
    Require,
    Return,
    Rows,
    Same,
    Scalar,
    Scan,
    Seek,
    Select,
    Set,
    Shortest,
    ShortestPath,
    Simple,
    Single,
    Skip,
    Starts,
    Status,
    Then,
    Trail,
    Trailing,
    Transactions,
    Trim,
    True,
    Typed,
    Union,
    Unique,
    Unwind,
    Use,
    Using,
    Value,
    Values,
    Walk,
    When,
    Where,
    With,
    Without,
    Write,
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
            Self::Both => "BOTH",
            Self::Break => "BREAK",
            Self::By => "BY",
            Self::Call => "CALL",
            Self::Case => "CASE",
            Self::Close => "CLOSE",
            Self::Collect => "COLLECT",
            Self::Constraint => "CONSTRAINT",
            Self::Contains => "CONTAINS",
            Self::Continue => "CONTINUE",
            Self::Count => "COUNT",
            Self::Create => "CREATE",
            Self::Csv => "CSV",
            Self::Current => "CURRENT",
            Self::Delete => "DELETE",
            Self::Desc => "DESC",
            Self::Descending => "DESCENDING",
            Self::Detach => "DETACH",
            Self::Different => "DIFFERENT",
            Self::Distinct => "DISTINCT",
            Self::Do => "DO",
            Self::Drop => "DROP",
            Self::Dryrun => "DRYRUN",
            Self::Each => "EACH",
            Self::Else => "ELSE",
            Self::End => "END",
            Self::Ends => "ENDS",
            Self::Error => "ERROR",
            Self::Exists => "EXISTS",
            Self::Explain => "EXPLAIN",
            Self::Fail => "FAIL",
            Self::False => "FALSE",
            Self::Fieldterminator => "FIELDTERMINATOR",
            Self::Filter => "FILTER",
            Self::Finish => "FINISH",
            Self::For => "FOR",
            Self::Foreach => "FOREACH",
            Self::First => "FIRST",
            Self::From => "FROM",
            Self::Graph => "GRAPH",
            Self::Group => "GROUP",
            Self::Groups => "GROUPS",
            Self::Headers => "HEADERS",
            Self::In => "IN",
            Self::Inf => "INF",
            Self::Infinity => "INFINITY",
            Self::Insert => "INSERT",
            Self::Index => "INDEX",
            Self::Is => "IS",
            Self::Join => "JOIN",
            Self::Label => "LABEL",
            Self::Labels => "LABELS",
            Self::Last => "LAST",
            Self::Leading => "LEADING",
            Self::Let => "LET",
            Self::Limit => "LIMIT",
            Self::Load => "LOAD",
            Self::Mandatory => "MANDATORY",
            Self::Match => "MATCH",
            Self::Merge => "MERGE",
            Self::Nan => "NAN",
            Self::Next => "NEXT",
            Self::Node => "NODE",
            Self::Nodetach => "NODETACH",
            Self::None => "NONE",
            Self::Normalize => "NORMALIZE",
            Self::Not => "NOT",
            Self::Null => "NULL",
            Self::Nulls => "NULLS",
            Self::Of => "OF",
            Self::Offset => "OFFSET",
            Self::On => "ON",
            Self::Only => "ONLY",
            Self::Optional => "OPTIONAL",
            Self::Or => "OR",
            Self::Order => "ORDER",
            Self::Path => "PATH",
            Self::Paths => "PATHS",
            Self::Periodic => "PERIODIC",
            Self::Profile => "PROFILE",
            Self::Property => "PROPERTY",
            Self::Reduce => "REDUCE",
            Self::Remove => "REMOVE",
            Self::Report => "REPORT",
            Self::Repeatable => "REPEATABLE",
            Self::Replace => "REPLACE",
            Self::Require => "REQUIRE",
            Self::Return => "RETURN",
            Self::Rows => "ROWS",
            Self::Same => "SAME",
            Self::Scalar => "SCALAR",
            Self::Scan => "SCAN",
            Self::Seek => "SEEK",
            Self::Select => "SELECT",
            Self::Set => "SET",
            Self::Shortest => "SHORTEST",
            Self::ShortestPath => "SHORTESTPATH",
            Self::Simple => "SIMPLE",
            Self::Single => "SINGLE",
            Self::Skip => "SKIP",
            Self::Starts => "STARTS",
            Self::Status => "STATUS",
            Self::Then => "THEN",
            Self::Trail => "TRAIL",
            Self::Trailing => "TRAILING",
            Self::Transactions => "TRANSACTIONS",
            Self::Trim => "TRIM",
            Self::True => "TRUE",
            Self::Typed => "TYPED",
            Self::Union => "UNION",
            Self::Unique => "UNIQUE",
            Self::Unwind => "UNWIND",
            Self::Use => "USE",
            Self::Using => "USING",
            Self::Value => "VALUE",
            Self::Values => "VALUES",
            Self::Walk => "WALK",
            Self::When => "WHEN",
            Self::Where => "WHERE",
            Self::With => "WITH",
            Self::Without => "WITHOUT",
            Self::Write => "WRITE",
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
            2 => &[
                Self::As,
                Self::By,
                Self::Do,
                Self::In,
                Self::Is,
                Self::Of,
                Self::On,
                Self::Or,
            ],
            3 => &[
                Self::All,
                Self::And,
                Self::Any,
                Self::Asc,
                Self::Csv,
                Self::End,
                Self::For,
                Self::Inf,
                Self::Let,
                Self::Nan,
                Self::Not,
                Self::Set,
                Self::Use,
                Self::Xor,
            ],
            4 => &[
                Self::Both,
                Self::Call,
                Self::Case,
                Self::Desc,
                Self::Drop,
                Self::Each,
                Self::Else,
                Self::Ends,
                Self::Fail,
                Self::From,
                Self::Join,
                Self::Last,
                Self::Load,
                Self::Next,
                Self::Node,
                Self::None,
                Self::Null,
                Self::Only,
                Self::Path,
                Self::Rows,
                Self::Same,
                Self::Scan,
                Self::Seek,
                Self::Skip,
                Self::Then,
                Self::Trim,
                Self::True,
                Self::Walk,
                Self::When,
                Self::With,
            ],
            5 => &[
                Self::Break,
                Self::Close,
                Self::Count,
                Self::Error,
                Self::False,
                Self::First,
                Self::Graph,
                Self::Group,
                Self::Index,
                Self::Label,
                Self::Limit,
                Self::Match,
                Self::Merge,
                Self::Nulls,
                Self::Order,
                Self::Paths,
                Self::Trail,
                Self::Typed,
                Self::Union,
                Self::Using,
                Self::Value,
                Self::Where,
                Self::Write,
                Self::Yield,
            ],
            6 => &[
                Self::Create,
                Self::Delete,
                Self::Detach,
                Self::Dryrun,
                Self::Exists,
                Self::Filter,
                Self::Finish,
                Self::Groups,
                Self::Insert,
                Self::Labels,
                Self::Offset,
                Self::Reduce,
                Self::Remove,
                Self::Report,
                Self::Return,
                Self::Scalar,
                Self::Select,
                Self::Simple,
                Self::Single,
                Self::Starts,
                Self::Status,
                Self::Unique,
                Self::Unwind,
                Self::Values,
            ],
            7 => &[
                Self::Acyclic,
                Self::Collect,
                Self::Current,
                Self::Explain,
                Self::Foreach,
                Self::Headers,
                Self::Leading,
                Self::Profile,
                Self::Replace,
                Self::Require,
                Self::Without,
            ],
            8 => &[
                Self::Contains,
                Self::Continue,
                Self::Distinct,
                Self::Nodetach,
                Self::Optional,
                Self::Periodic,
                Self::Property,
                Self::Shortest,
                Self::Trailing,
                Self::Infinity,
            ],
            9 => &[
                Self::Ascending,
                Self::Different,
                Self::Mandatory,
                Self::Normalize,
            ],
            10 => &[Self::Constraint, Self::Descending, Self::Repeatable],
            12 => &[Self::Transactions, Self::ShortestPath],
            15 => &[Self::Fieldterminator],
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
