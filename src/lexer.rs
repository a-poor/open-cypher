//! Lossless openCypher tokenization built with Logos.

use logos::{Lexer, Logos};

use crate::diagnostic::{Diagnostic, DiagnosticCode};
use crate::span::Span;
use crate::token::{Keyword, Token, TokenKind};

/// The result of losslessly tokenizing a source string.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LexOutcome {
    /// Every token in source order, including trivia and invalid regions.
    pub tokens: Vec<Token>,
    /// Lexical diagnostics in source order.
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, Default, PartialEq)]
enum LexingError {
    #[default]
    Invalid,
    UnterminatedString,
    UnterminatedIdentifier,
    UnterminatedComment,
    InvalidEscape,
}

#[derive(Logos, Clone, Copy, Debug, PartialEq)]
#[logos(error = LexingError)]
enum RawToken {
    #[regex(r"\p{White_Space}+")]
    Whitespace,

    #[regex(r"//[^\r\n]*", allow_greedy = true)]
    LineComment,

    #[token("/*", lex_block_comment)]
    BlockComment,

    #[token("'", lex_single_string)]
    #[token("\"", lex_double_string)]
    String,

    #[token("`", lex_escaped_identifier)]
    EscapedIdentifier,

    #[regex(r"\$[_\p{XID_Continue}]+")]
    Parameter,

    #[regex(r"0[xX](?:_?[0-9A-Fa-f])+")]
    HexInteger,

    #[regex(r"0[oO](?:_?[0-7])+")]
    OctalInteger,

    #[regex(r"(?:[0-9](?:_?[0-9])*)?\.[0-9](?:_?[0-9])*(?:[eE][+-]?[0-9](?:_?[0-9])*)?[fFdD]?")]
    #[regex(r"[0-9](?:_?[0-9])*[eE][+-]?[0-9](?:_?[0-9])*[fFdD]?")]
    Float,

    #[regex(r"[0-9](?:_?[0-9])*")]
    Integer,

    #[regex(r"[_\p{XID_Start}][_\p{XID_Continue}]*")]
    Word,

    #[token("||")]
    DoublePipe,
    #[token("::")]
    DoubleColon,
    #[token("..")]
    DotDot,
    #[token("<=")]
    LessEqual,
    #[token(">=")]
    GreaterEqual,
    #[token("<>")]
    #[token("!=")]
    NotEqual,
    #[token("+=")]
    PlusEqual,
    #[token("=>")]
    FatArrow,
    #[token("=~")]
    RegexMatch,
    #[token("<-")]
    LeftArrow,
    #[token("->")]
    RightArrow,

    #[token("(")]
    LeftParen,
    #[token(")")]
    RightParen,
    #[token("[")]
    LeftBracket,
    #[token("]")]
    RightBracket,
    #[token("{")]
    LeftBrace,
    #[token("}")]
    RightBrace,
    #[token(",")]
    Comma,
    #[token(".")]
    Dot,
    #[token(":")]
    Colon,
    #[token(";")]
    Semicolon,
    #[token("|")]
    Pipe,
    #[token("&")]
    Ampersand,
    #[token("?")]
    Question,
    #[token("$")]
    Dollar,
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,
    #[token("/")]
    Slash,
    #[token("%")]
    Percent,
    #[token("^")]
    Caret,
    #[token("!")]
    Bang,
    #[token("=")]
    Equal,
    #[token("<")]
    Less,
    #[token(">")]
    Greater,
}

fn lex_single_string(lexer: &mut Lexer<'_, RawToken>) -> Result<(), LexingError> {
    consume_quoted(lexer, '\'', LexingError::UnterminatedString)
}

fn lex_double_string(lexer: &mut Lexer<'_, RawToken>) -> Result<(), LexingError> {
    consume_quoted(lexer, '"', LexingError::UnterminatedString)
}

fn lex_escaped_identifier(lexer: &mut Lexer<'_, RawToken>) -> Result<(), LexingError> {
    consume_quoted(lexer, '`', LexingError::UnterminatedIdentifier)
}

fn consume_quoted(
    lexer: &mut Lexer<'_, RawToken>,
    delimiter: char,
    unterminated: LexingError,
) -> Result<(), LexingError> {
    let remainder = lexer.remainder();
    let mut characters = remainder.char_indices().peekable();
    let mut invalid_escape = false;

    while let Some((offset, character)) = characters.next() {
        if character == '\\' {
            let Some((_, escaped)) = characters.next() else {
                continue;
            };
            match escaped {
                '\\' | '\'' | '"' | '`' | 't' | 'b' | 'n' | 'r' | 'f' => {}
                'u' | 'U' => {
                    let digits = if escaped == 'u' { 4 } else { 6 };
                    let mut scalar = 0u32;
                    let mut complete = true;
                    for _ in 0..digits {
                        match characters.peek().copied() {
                            Some((_, digit)) if digit.is_ascii_hexdigit() => {
                                let _ = characters.next();
                                scalar = scalar * 16 + digit.to_digit(16).expect("hex digit");
                            }
                            _ => {
                                complete = false;
                                break;
                            }
                        }
                    }
                    if !complete || char::from_u32(scalar).is_none() {
                        invalid_escape = true;
                    }
                }
                _ => invalid_escape = true,
            }
            continue;
        }

        if character == delimiter {
            if characters
                .peek()
                .is_some_and(|(_, next)| *next == delimiter)
            {
                let _ = characters.next();
                continue;
            }

            lexer.bump(offset + character.len_utf8());
            return if invalid_escape {
                Err(LexingError::InvalidEscape)
            } else {
                Ok(())
            };
        }
    }

    lexer.bump(remainder.len());
    Err(unterminated)
}

fn lex_block_comment(lexer: &mut Lexer<'_, RawToken>) -> Result<(), LexingError> {
    let remainder = lexer.remainder().as_bytes();
    let mut offset = 0;
    let mut depth = 1usize;

    while offset + 1 < remainder.len() {
        match (remainder[offset], remainder[offset + 1]) {
            (b'/', b'*') => {
                depth += 1;
                offset += 2;
            }
            (b'*', b'/') => {
                depth -= 1;
                offset += 2;
                if depth == 0 {
                    lexer.bump(offset);
                    return Ok(());
                }
            }
            _ => offset += 1,
        }
    }

    lexer.bump(remainder.len());
    Err(LexingError::UnterminatedComment)
}

/// Tokenizes `source` without dropping trivia or invalid input.
#[must_use]
pub fn lex(source: &str) -> LexOutcome {
    let mut lexer = RawToken::lexer(source);
    let mut tokens = Vec::new();
    let mut diagnostics = Vec::new();

    while let Some(raw) = lexer.next() {
        let range = lexer.span();
        let span = Span::new(range.start, range.end);
        let text = &source[range];

        let kind = match raw {
            Ok(raw) => token_kind(raw, text),
            Err(error) => {
                let (code, message) = match error {
                    LexingError::Invalid => {
                        (DiagnosticCode::InvalidToken, "invalid openCypher token")
                    }
                    LexingError::UnterminatedString => (
                        DiagnosticCode::UnterminatedString,
                        "unterminated string literal",
                    ),
                    LexingError::UnterminatedIdentifier => (
                        DiagnosticCode::UnterminatedIdentifier,
                        "unterminated escaped identifier",
                    ),
                    LexingError::UnterminatedComment => (
                        DiagnosticCode::UnterminatedComment,
                        "unterminated block comment",
                    ),
                    LexingError::InvalidEscape => {
                        (DiagnosticCode::InvalidEscape, "invalid escape sequence")
                    }
                };
                diagnostics.push(Diagnostic::error(code, message, span));
                TokenKind::Invalid
            }
        };

        tokens.push(Token::new(kind, span));
    }

    LexOutcome {
        tokens,
        diagnostics,
    }
}

fn token_kind(raw: RawToken, text: &str) -> TokenKind {
    match raw {
        RawToken::Whitespace => TokenKind::Whitespace,
        RawToken::LineComment => TokenKind::LineComment,
        RawToken::BlockComment => TokenKind::BlockComment,
        RawToken::String => TokenKind::String,
        RawToken::EscapedIdentifier => TokenKind::EscapedIdentifier,
        RawToken::Parameter => TokenKind::Parameter,
        RawToken::HexInteger => TokenKind::HexInteger,
        RawToken::OctalInteger => TokenKind::OctalInteger,
        RawToken::Float => TokenKind::Float,
        RawToken::Integer => TokenKind::Integer,
        RawToken::Word => Keyword::from_ascii_case_insensitive(text)
            .map(TokenKind::Keyword)
            .unwrap_or(TokenKind::Identifier),
        RawToken::LeftParen => TokenKind::LeftParen,
        RawToken::RightParen => TokenKind::RightParen,
        RawToken::LeftBracket => TokenKind::LeftBracket,
        RawToken::RightBracket => TokenKind::RightBracket,
        RawToken::LeftBrace => TokenKind::LeftBrace,
        RawToken::RightBrace => TokenKind::RightBrace,
        RawToken::Comma => TokenKind::Comma,
        RawToken::Dot => TokenKind::Dot,
        RawToken::DotDot => TokenKind::DotDot,
        RawToken::Colon => TokenKind::Colon,
        RawToken::DoubleColon => TokenKind::DoubleColon,
        RawToken::Semicolon => TokenKind::Semicolon,
        RawToken::Pipe => TokenKind::Pipe,
        RawToken::DoublePipe => TokenKind::DoublePipe,
        RawToken::Ampersand => TokenKind::Ampersand,
        RawToken::Question => TokenKind::Question,
        RawToken::Dollar => TokenKind::Dollar,
        RawToken::Plus => TokenKind::Plus,
        RawToken::Minus => TokenKind::Minus,
        RawToken::Star => TokenKind::Star,
        RawToken::Slash => TokenKind::Slash,
        RawToken::Percent => TokenKind::Percent,
        RawToken::Caret => TokenKind::Caret,
        RawToken::Bang => TokenKind::Bang,
        RawToken::Equal => TokenKind::Equal,
        RawToken::NotEqual => TokenKind::NotEqual,
        RawToken::Less => TokenKind::Less,
        RawToken::LessEqual => TokenKind::LessEqual,
        RawToken::Greater => TokenKind::Greater,
        RawToken::GreaterEqual => TokenKind::GreaterEqual,
        RawToken::PlusEqual => TokenKind::PlusEqual,
        RawToken::FatArrow => TokenKind::FatArrow,
        RawToken::RegexMatch => TokenKind::RegexMatch,
        RawToken::LeftArrow => TokenKind::LeftArrow,
        RawToken::RightArrow => TokenKind::RightArrow,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_comments_are_one_lossless_token() {
        let source = "/* outer /* inner */ done */";
        let outcome = lex(source);
        assert!(outcome.diagnostics.is_empty());
        assert_eq!(
            outcome.tokens,
            [Token::new(
                TokenKind::BlockComment,
                Span::new(0, source.len())
            )]
        );
    }

    #[test]
    fn range_is_not_swallowed_by_float_lexing() {
        let tokens = lex("1..2").tokens;
        assert_eq!(
            tokens.iter().map(|token| token.kind).collect::<Vec<_>>(),
            [TokenKind::Integer, TokenKind::DotDot, TokenKind::Integer]
        );
    }
}
