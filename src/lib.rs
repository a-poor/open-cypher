//! An unofficial, syntax-only lexer and parser targeting openCypher 2024.3.
//!
//! The crate exposes a lossless token stream, an owned and spanned AST, and
//! structured diagnostics. It deliberately does not perform semantic analysis
//! or query execution.

#![forbid(unsafe_code)]

pub mod ast;
pub mod diagnostic;
pub mod lexer;
pub mod parser;
pub mod span;
pub mod token;

pub use diagnostic::{Diagnostic, DiagnosticCode, Label, ParseErrors, Severity};
pub use lexer::{LexOutcome, lex};
pub use parser::{ParseOutcome, ParsedProgram, parse, parse_recovering};
pub use span::Span;
pub use token::{Keyword, Token, TokenKind};

/// The upstream openCypher language release used as this crate's pinned baseline.
pub const OPEN_CYPHER_GRAMMAR_VERSION: &str = "2024.3";

/// The immutable upstream Git revision containing the language snapshot.
pub const OPEN_CYPHER_GRAMMAR_REVISION: &str = "677cbafabb8c3c5eed458fd3b1ec0daec8d67d23";
