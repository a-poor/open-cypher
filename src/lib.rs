//! An unofficial, syntax-only lexer and parser for the openCypher query
//! language, targeting the openCypher 2024.3 grammar.
//!
//! The crate turns a query string into three things: a lossless token
//! stream, an owned syntax tree in which every node carries a byte
//! [`Span`], and structured [`Diagnostic`]s. It stops there. There is no name
//! resolution, type checking, planning, or execution.
//!
//! # Getting started
//!
//! [`parse`] is the strict entry point. It returns the tree and the tokens on
//! success and a [`ParseErrors`] collection otherwise.
//!
//! ```
//! use open_cypher::ast::{ClauseKind, QueryKind, StatementKind};
//! use open_cypher::parse;
//!
//! let source = "MATCH (person:Person) WHERE person.age > 30 RETURN person.name";
//! let parsed = parse(source)?;
//!
//! // A program holds at most one statement, and every node records where it
//! // came from, so spans can be used to slice the original source.
//! assert_eq!(parsed.program.statements.len(), 1);
//! assert_eq!(parsed.program.span.text(source), Some(source));
//!
//! let statement = &parsed.program.statements[0];
//! if let StatementKind::Query(query) = &statement.kind
//!     && let QueryKind::Regular(regular) = &query.query.kind
//! {
//!     for clause in &regular.head.kind.clauses {
//!         let text = clause.span.text(source).unwrap_or_default();
//!         match &clause.kind {
//!             ClauseKind::Match(m) if m.where_clause.is_some() => {
//!                 println!("MATCH with WHERE: {text}");
//!             }
//!             ClauseKind::Match(_) => println!("MATCH: {text}"),
//!             ClauseKind::Return(_) => println!("RETURN: {text}"),
//!             _ => println!("other clause: {text}"),
//!         }
//!     }
//! }
//! # Ok::<(), open_cypher::ParseErrors>(())
//! ```
//!
//! The [`ast`] module documents the tree. Most enums there are
//! `#[non_exhaustive]`, so matches need a wildcard arm.
//!
//! # Reporting errors
//!
//! A failed strict parse returns every diagnostic collected while lexing and
//! parsing, in source order. [`ParseErrors::render`] produces a plain-text
//! report with line and column numbers; [`Diagnostic`] exposes the same data
//! as fields for tools that want to format it themselves.
//!
//! ```
//! use open_cypher::{parse, DiagnosticCode};
//!
//! let source = "MATCH (person:Person RETURN person.name";
//! let errors = parse(source).unwrap_err();
//!
//! assert_eq!(errors.diagnostics()[0].code, DiagnosticCode::UnclosedDelimiter);
//! eprintln!("{}", errors.render(source));
//! ```
//!
//! The rendered report looks like this:
//!
//! ```text
//! error[OCY-P004]: unclosed delimiter `(`
//!  --> 1:7
//!   |
//! 1 | MATCH (person:Person RETURN person.name
//!   |       ^
//!   = help: add the matching delimiter before byte 39
//!
//! error[OCY-P001]: unexpected token `RETURN`; expected `)`, `WHERE`, `parameter`, or `{`
//!  --> 1:22
//!   |
//! 1 | MATCH (person:Person RETURN person.name
//!   |                      ^^^^^^
//! ```
//!
//! # Recovering from errors
//!
//! [`parse_recovering`] never fails. It returns a [`ParseOutcome`] carrying
//! the diagnostics alongside a program root. Recovery is currently
//! whole-input recovery: when the input has a syntax error the root contains
//! a single [`StatementKind::Error`](ast::StatementKind::Error) placeholder
//! rather than a partially recovered tree, and the full token stream is still
//! available. At most 32 diagnostics are reported per parse.
//!
//! ```
//! use open_cypher::parse_recovering;
//!
//! let outcome = parse_recovering("MATCH (person RETURN person");
//! let parsed = outcome.value.expect("recovery always yields a program");
//!
//! assert!(!outcome.diagnostics.is_empty());
//! assert_eq!(parsed.program.statements.len(), 1);
//! ```
//!
//! # Tokens
//!
//! [`lex`] tokenizes without parsing. The token stream is lossless: it keeps
//! whitespace, comments, and invalid regions, and the spans tile the input.
//! [`parse`] returns the same tokens in [`ParsedProgram::tokens`].
//!
//! ```
//! use open_cypher::{lex, TokenKind};
//!
//! let source = "RETURN 1 // answer";
//! let lexed = lex(source);
//!
//! assert!(lexed.diagnostics.is_empty());
//! let kinds: Vec<_> = lexed.tokens.iter().map(|token| token.kind).collect();
//! assert!(matches!(kinds.last(), Some(TokenKind::LineComment)));
//!
//! let spelled: String = lexed
//!     .tokens
//!     .iter()
//!     .filter_map(|token| token.text(source))
//!     .collect();
//! assert_eq!(spelled, source);
//! ```
//!
//! # What is supported
//!
//! The parser accepts the full openCypher 2024.3 grammar. Every one of the
//! 377 BNF productions is mapped to lexer and parser targets with executable
//! witnesses, and every query in the pinned Technology Compatibility Kit
//! (TCK) parses or is rejected as the syntax projection expects. That is
//! evidence about syntax only; it is not official TCK certification.
//!
//! A handful of deliberate extensions over the 2024.3 BNF are accepted and
//! recorded in the repository's deviation ledger. They include:
//!
//! - `COUNT { ... }` and `COLLECT { ... }` subquery expressions beside
//!   `EXISTS { ... }`;
//! - `!=` as an alias for `<>` and `||` as a concatenation operator;
//! - the GQL path modes `WALK`, `TRAIL`, `SIMPLE`, and `ACYCLIC` before a
//!   graph pattern, and `?` as the optional path quantifier;
//! - boolean and `IS` label expressions in `SET` and `REMOVE`;
//! - more than one `ON MATCH` / `ON CREATE` action after `MERGE`;
//! - empty property maps and bare parameters as property maps anywhere in a
//!   pattern, leaving update restrictions to a later semantic pass;
//! - empty or comment-only input, one optional trailing semicolon, and
//!   identifiers that start with an underscore.
//!
//! The following are out of scope for the `0.2` series:
//!
//! - **Semantics.** Names are not resolved, types are not checked, and no
//!   query runs. Some inputs that a database would reject (for example a
//!   parameter used as a `MATCH` property map) parse successfully so that a
//!   semantic pass can report them with a better message.
//! - **Multi-statement scripts.** A program contains zero or one query. A
//!   second statement after the semicolon is a syntax error.
//! - **Partial recovery.** A syntax error yields one whole-input error
//!   statement, not locally repaired clauses or expressions.
//!
//! Known parser issues tracked for a later release:
//!
//! - `a[1:b]` is accepted and parsed as an index whose subscript is a label
//!   predicate on the literal `1`, instead of being rejected.
//!
//! # Feature flags
//!
//! - `serde`: derives `Serialize` and `Deserialize` for the token, syntax
//!   tree, and diagnostic types. Off by default.
//!
//! # Versioning of the grammar
//!
//! The language snapshot is pinned independently of the crate version. See
//! [`OPEN_CYPHER_GRAMMAR_VERSION`] and [`OPEN_CYPHER_GRAMMAR_REVISION`].
//!
//! # How the crate is tested
//!
//! The parser is checked against the pinned openCypher TCK as a syntax
//! projection, against per-production witnesses, and with property tests,
//! fuzzing, a coverage floor, and weekly mutation testing. The repository's
//! `docs/testing.md` describes each layer and how to run it locally.

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
