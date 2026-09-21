//! Strict and recovering parser entry points.

use std::fmt;

use lalrpop_util::ParseError;

use crate::ast::{
    BinaryOperator, Clause, ClauseKind, ErrorNode, Expr, ExprKind, Identifier, IntegerLiteral,
    IntegerRadix, IsPredicate, LabelExpression, ListComprehension, Literal, LiteralKind,
    MapProjection, MapProjectionItem, MatchClause, Name, Node, Parameter, ParameterName,
    PathFactor, PathFactorKind, Pattern, PatternComprehension, Program, QualifiedName, Quantifier,
    Query, QueryKind, QueryStatement, QuoteStyle, RegularQuery, RelationshipDirection,
    RelationshipPattern, SingleQuery, SingleQueryKind, StatementKind, StringLiteral, UnionBranch,
    UnionOperator,
};
use crate::diagnostic::{Diagnostic, DiagnosticCode, ParseErrors};
use crate::lexer;
use crate::span::Span;
use crate::token::{Keyword, Token, TokenKind};

#[path = "generated/cypher.rs"]
#[allow(clippy::all)]
mod generated;

const MAX_DIAGNOSTICS: usize = 32;

/// A parsed program and its complete, trivia-preserving token stream.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ParsedProgram {
    /// The owned syntax tree.
    pub program: Program,
    /// Every token, including whitespace and comments.
    pub tokens: Vec<Token>,
}

/// The result of a recovering parse.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ParseOutcome<T> {
    /// The recovered value, when the selected entry point can construct one.
    ///
    /// [`parse_recovering`] currently always returns a `ParsedProgram`: valid
    /// input produces the parsed tree, while invalid input produces a
    /// whole-input error statement.
    pub value: Option<T>,
    /// Lexer and parser diagnostics in deterministic source order.
    pub diagnostics: Vec<Diagnostic>,
}

/// Parses one complete source input, rejecting lexical or syntactic errors.
pub fn parse(source: &str) -> Result<ParsedProgram, ParseErrors> {
    let outcome = parse_recovering(source);
    if outcome.diagnostics.iter().any(Diagnostic::is_error) {
        return Err(ParseErrors::new(outcome.diagnostics));
    }

    outcome.value.ok_or_else(|| {
        ParseErrors::new(vec![Diagnostic::error(
            DiagnosticCode::Internal,
            "the parser did not produce a program",
            Span::empty(source.len()),
        )])
    })
}

/// Parses while retaining diagnostics and always constructing a program root.
///
/// Recovery is currently whole-input recovery. A syntax failure yields one
/// error statement rather than locally recovered clause or expression nodes.
#[must_use]
pub fn parse_recovering(source: &str) -> ParseOutcome<ParsedProgram> {
    let lexed = lexer::lex(source);
    let mut diagnostics = lexed.diagnostics.clone();
    let significant = parser_tokens(&lexed.tokens);

    let parsed = parse_program_with_contextual_names(source, &significant);
    let program = match parsed {
        Ok(program) => Some(program),
        Err(error) => {
            diagnostics.push(parse_diagnostic(source, error));
            None
        }
    };

    if program.is_none() || !diagnostics.is_empty() {
        collect_delimiter_diagnostics(source, &lexed.tokens, &mut diagnostics);
    }

    normalize_diagnostics(source.len(), &mut diagnostics);

    let program = program.unwrap_or_else(|| recovered_program(source, &lexed.tokens));
    ParseOutcome {
        value: Some(ParsedProgram {
            program,
            tokens: lexed.tokens,
        }),
        diagnostics,
    }
}

type SpannedParserToken = (usize, ParserToken, usize);

fn parse_program_with_contextual_names(
    source: &str,
    tokens: &[SpannedParserToken],
) -> Result<Program, LalrpopError> {
    parse_tokens_with_contextual_names(tokens, |contextual| parse_program(source, contextual))
}

fn parse_tokens_with_contextual_names<T>(
    tokens: &[SpannedParserToken],
    mut parse_tokens: impl FnMut(&[SpannedParserToken]) -> Result<T, LalrpopError>,
) -> Result<T, LalrpopError> {
    let mut contextual = tokens.to_vec();
    let mut resolved = vec![false; tokens.len()];
    for index in 0..tokens.len() {
        let follows_name_marker = index
            .checked_sub(1)
            .and_then(|previous| tokens.get(previous))
            .is_some_and(|token| matches!(token.1, ParserToken::As | ParserToken::Dot));
        if is_aliased_bare_expression(tokens, index) && tokens[index].1.is_keyword_literal() {
            // Preserve the native literal interpretation and keep later retry
            // errors from reclassifying this already-valid projection item.
            resolved[index] = true;
            continue;
        }
        if tokens[index].1.is_contextual_name_candidate()
            && ((follows_name_marker
                && !(tokens[index].1 == ParserToken::As && tokens[index - 1].1 == ParserToken::As))
                || is_solo_projection_keyword(tokens, index)
                || is_aliased_bare_expression(tokens, index))
        {
            resolved[index] = true;
            contextual[index].1 = ParserToken::Identifier;
        }
    }

    let initial_error = match parse_tokens(&contextual) {
        Ok(value) => return Ok(value),
        Err(error) => error,
    };

    let mut best_error = initial_error.clone();
    let mut error = initial_error;

    // The generated state machines already reinterpret a keyword when its
    // native action is an immediate error and the identifier action is valid.
    // A smaller set of constructs has delayed ambiguity: the keyword shifts as
    // syntax, then fails at a later token or EOF (for example a bare COUNT).
    // Reclassify the nearest remaining candidate and retry those cases.
    // Each pass commits one lexical-context signature (the keyword and its two
    // neighboring tokens), so repeated non-reserved words in the same name
    // position are handled together without conflating distinct token kinds.
    // Valid queries can use an arbitrary number of non-reserved names, and
    // malformed input is bounded by the finite set of signatures instead of
    // an exponential search through keyword subsets.
    loop {
        if error_location(&error) >= error_location(&best_error) {
            best_error = error.clone();
        }

        let location = error_location(&error);
        let candidate = tokens
            .iter()
            .enumerate()
            .filter(|(index, (_, token, _))| {
                token.is_contextual_name_candidate() && !resolved[*index]
            })
            .map(|(index, (start, _, end))| {
                let distance = if location < *start {
                    *start - location
                } else {
                    location.saturating_sub(*end)
                };
                let exact = (*start..*end).contains(&location);
                let followed_by_as = tokens
                    .get(index + 1)
                    .is_some_and(|token| token.1 == ParserToken::As);
                let priority = if exact && (tokens[index].1 != ParserToken::As || followed_by_as) {
                    0
                } else if followed_by_as && tokens[index].1 != ParserToken::As {
                    1
                } else if exact {
                    2
                } else {
                    3
                };
                (priority, distance, index)
            })
            .min();
        let Some((_, _, index)) = candidate else {
            break;
        };

        let signature = contextual_name_signature(tokens, index);
        for peer in 0..tokens.len() {
            if !resolved[peer]
                && tokens[peer].1.is_contextual_name_candidate()
                && contextual_name_signature(tokens, peer) == signature
            {
                resolved[peer] = true;
                contextual[peer].1 = ParserToken::Identifier;
            }
        }
        match parse_tokens(&contextual) {
            Ok(value) => return Ok(value),
            Err(next_error)
                if error_location(&next_error) > location
                    || matches!(error, ParseError::User { .. }) =>
            {
                error = next_error;
            }
            Err(_) => break,
        }
    }

    Err(best_error)
}

fn is_aliased_bare_expression(tokens: &[SpannedParserToken], index: usize) -> bool {
    // A one-token projection item followed by AS cannot be one of the keyword
    // constructs with delayed ambiguity (CASE, COUNT, EXISTS, and so on).
    // Resolve every such item in one scan. The caller separately preserves the
    // established interpretation of the six keyword-backed literal spellings.
    index
        .checked_sub(1)
        .and_then(|previous| tokens.get(previous))
        .is_some_and(|token| {
            matches!(
                token.1,
                ParserToken::Return | ParserToken::With | ParserToken::Yield | ParserToken::Comma
            )
        })
        && tokens
            .get(index + 1)
            .is_some_and(|token| token.1 == ParserToken::As)
}

fn is_solo_projection_keyword(tokens: &[SpannedParserToken], index: usize) -> bool {
    if !matches!(tokens[index].1, ParserToken::SetAll | ParserToken::Distinct)
        || !index
            .checked_sub(1)
            .and_then(|previous| tokens.get(previous))
            .is_some_and(|token| matches!(token.1, ParserToken::Return | ParserToken::With))
    {
        return false;
    }

    tokens.get(index + 1).is_none_or(|token| {
        matches!(
            token.1,
            ParserToken::As
                | ParserToken::Comma
                | ParserToken::Order
                | ParserToken::Offset
                | ParserToken::Skip
                | ParserToken::Limit
                | ParserToken::Where
                | ParserToken::Union
                | ParserToken::Semicolon
        ) || clause_starts(token.1)
    })
}

fn contextual_name_signature(
    tokens: &[SpannedParserToken],
    index: usize,
) -> (Option<ParserToken>, ParserToken, Option<ParserToken>) {
    (
        index
            .checked_sub(1)
            .and_then(|previous| tokens.get(previous))
            .map(|token| token.1),
        tokens[index].1,
        tokens.get(index + 1).map(|token| token.1),
    )
}

fn parse_program(source: &str, tokens: &[SpannedParserToken]) -> Result<Program, LalrpopError> {
    if tokens.is_empty() {
        return Ok(Program::new(Vec::new(), Span::empty(0)));
    }

    let (query_tokens, terminator) = match tokens.last() {
        Some((start, ParserToken::Semicolon, end)) => {
            (&tokens[..tokens.len() - 1], Some(Span::new(*start, *end)))
        }
        _ => (tokens, None),
    };

    if query_tokens.is_empty() {
        return Err(ParseError::UnrecognizedEof {
            location: terminator.map_or(0, |span| span.start),
            expected: vec!["query clause".to_owned()],
        });
    }

    let (head, unions) = match parse_regular_query(source, query_tokens) {
        Ok(query) => query,
        Err(regular_error)
            if !top_level_indices(query_tokens, |token| token == ParserToken::Union).is_empty() =>
        {
            return Err(regular_error);
        }
        Err(regular_error) => {
            let input = query_tokens.iter().copied().map(Ok);
            let clause = match generated::StandaloneCallRootParser::new().parse(source, input) {
                Ok(clause) => clause,
                Err(_) => return Err(regular_error),
            };
            let span = clause.span;
            (
                Node::new(
                    SingleQueryKind {
                        clauses: vec![clause],
                    },
                    span,
                ),
                Vec::new(),
            )
        }
    };
    let query_span = Span::new(
        query_tokens.first().expect("non-empty query").0,
        query_tokens.last().expect("non-empty query").2,
    );
    let query = Node::new(
        QueryKind::Regular(RegularQuery { head, unions }),
        query_span,
    );
    let statement_span = terminator.map_or(query_span, |span| query_span.cover(span));
    let statement = Node::new(
        StatementKind::Query(QueryStatement {
            mode: None,
            query,
            terminator,
        }),
        statement_span,
    );
    Ok(Program::new(vec![statement], statement_span))
}

fn parse_regular_query(
    source: &str,
    tokens: &[SpannedParserToken],
) -> Result<(SingleQuery, Vec<UnionBranch>), LalrpopError> {
    let unsplit_error = match parse_single_query(source, tokens) {
        Ok(query) => return Ok((query, Vec::new())),
        Err(error) => error,
    };

    for union_index in top_level_indices(tokens, |token| token == ParserToken::Union) {
        let Ok(head) = parse_single_query(source, &tokens[..union_index]) else {
            continue;
        };

        let mut right_start = union_index + 1;
        let operator = match tokens.get(right_start).map(|token| token.1) {
            Some(ParserToken::All) => {
                right_start += 1;
                UnionOperator::All
            }
            Some(ParserToken::Distinct) => {
                right_start += 1;
                UnionOperator::Distinct
            }
            _ => UnionOperator::Default,
        };
        if right_start >= tokens.len() {
            continue;
        }

        let Ok((right, mut tail)) = parse_regular_query(source, &tokens[right_start..]) else {
            continue;
        };
        let operator_end = tokens[right_start - 1].2;
        let operator = Node::new(operator, Span::new(tokens[union_index].0, operator_end));
        let mut unions = Vec::with_capacity(1 + tail.len());
        unions.push(UnionBranch {
            operator,
            query: right,
        });
        unions.append(&mut tail);
        return Ok((head, unions));
    }

    Err(unsplit_error)
}

fn parse_single_query(
    source: &str,
    tokens: &[SpannedParserToken],
) -> Result<SingleQuery, LalrpopError> {
    if tokens.is_empty() {
        return Err(ParseError::UnrecognizedEof {
            location: 0,
            expected: vec!["query clause".to_owned()],
        });
    }

    let mut clauses: Vec<Clause> = Vec::new();
    let mut cursor = 0;
    while cursor < tokens.len() {
        let mut candidate_ends = top_level_indices(&tokens[cursor + 1..], clause_starts)
            .into_iter()
            .map(|index| cursor + 1 + index)
            .collect::<Vec<_>>();
        candidate_ends.push(tokens.len());
        candidate_ends.sort_unstable();
        candidate_ends.dedup();

        let mut best_error = None;
        let mut parsed = None;
        for end in candidate_ends {
            let input = tokens[cursor..end].iter().copied().map(Ok);
            match generated::ClauseRootParser::new().parse(source, input) {
                Ok(clause) => {
                    if matches!(&clause.kind, ClauseKind::Return(_)) && end < tokens.len() {
                        return Err(ParseError::ExtraToken { token: tokens[end] });
                    }
                    parsed = Some((clause, end));
                    break;
                }
                Err(error) => retain_farthest_error(&mut best_error, error),
            }
        }

        let Some((clause, end)) = parsed else {
            return Err(best_error.unwrap_or(ParseError::UnrecognizedEof {
                location: tokens[cursor].0,
                expected: vec!["query clause".to_owned()],
            }));
        };
        clauses.push(clause);
        cursor = end;
    }

    let span = Span::new(
        tokens.first().expect("non-empty query").0,
        tokens.last().unwrap().2,
    );
    Ok(Node::new(SingleQueryKind { clauses }, span))
}

fn retain_farthest_error(slot: &mut Option<LalrpopError>, candidate: LalrpopError) {
    let candidate_location = error_location(&candidate);
    if slot
        .as_ref()
        .is_none_or(|current| candidate_location >= error_location(current))
    {
        *slot = Some(candidate);
    }
}

fn error_location(error: &LalrpopError) -> usize {
    match error {
        ParseError::InvalidToken { location } | ParseError::UnrecognizedEof { location, .. } => {
            *location
        }
        ParseError::UnrecognizedToken { token, .. } | ParseError::ExtraToken { token } => token.0,
        ParseError::User { .. } => 0,
    }
}

fn top_level_indices(
    tokens: &[SpannedParserToken],
    predicate: impl Fn(ParserToken) -> bool,
) -> Vec<usize> {
    let mut depth = 0usize;
    let mut indices = Vec::new();
    for (index, (_, token, _)) in tokens.iter().copied().enumerate() {
        if depth == 0 && predicate(token) {
            indices.push(index);
        }
        match token {
            ParserToken::LeftParen | ParserToken::LeftBracket | ParserToken::LeftBrace => {
                depth = depth.saturating_add(1);
            }
            ParserToken::RightParen | ParserToken::RightBracket | ParserToken::RightBrace => {
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
    }
    indices
}

fn clause_starts(token: ParserToken) -> bool {
    matches!(
        token,
        ParserToken::Match
            | ParserToken::Optional
            | ParserToken::Unwind
            | ParserToken::With
            | ParserToken::Return
            | ParserToken::Create
            | ParserToken::Merge
            | ParserToken::Set
            | ParserToken::Remove
            | ParserToken::Delete
            | ParserToken::Detach
            | ParserToken::Call
    )
}

fn normalize_diagnostics(source_len: usize, diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.sort_by(|left, right| {
        left.primary_span
            .start
            .cmp(&right.primary_span.start)
            .then(left.primary_span.end.cmp(&right.primary_span.end))
            .then(left.code.cmp(&right.code))
            .then(left.message.cmp(&right.message))
    });
    diagnostics.dedup_by(|left, right| {
        left.code == right.code
            && left.primary_span == right.primary_span
            && left.message == right.message
    });

    if diagnostics.len() > MAX_DIAGNOSTICS {
        diagnostics.truncate(MAX_DIAGNOSTICS - 1);
        diagnostics.push(Diagnostic::error(
            DiagnosticCode::TooManyErrors,
            "additional parser diagnostics were suppressed",
            Span::empty(source_len),
        ));
    }
}

fn recovered_program(source: &str, tokens: &[Token]) -> Program {
    let span = significant_span(tokens).unwrap_or_else(|| Span::empty(source.len()));
    let statements = if span.is_empty() {
        Vec::new()
    } else {
        vec![Node::new(StatementKind::Error(ErrorNode), span)]
    };
    Program::new(statements, span)
}

fn significant_span(tokens: &[Token]) -> Option<Span> {
    let mut significant = tokens.iter().filter(|token| !token.is_trivia());
    let first = significant.next()?;
    let mut span = first.span;
    for token in significant {
        span = span.cover(token.span);
    }
    Some(span)
}

fn collect_delimiter_diagnostics(
    source: &str,
    tokens: &[Token],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut stack: Vec<(TokenKind, Span)> = Vec::new();

    for token in tokens.iter().filter(|token| !token.is_trivia()) {
        match token.kind {
            TokenKind::LeftParen | TokenKind::LeftBracket | TokenKind::LeftBrace => {
                stack.push((token.kind, token.span));
            }
            TokenKind::RightParen | TokenKind::RightBracket | TokenKind::RightBrace => {
                let expected_open = match token.kind {
                    TokenKind::RightParen => TokenKind::LeftParen,
                    TokenKind::RightBracket => TokenKind::LeftBracket,
                    TokenKind::RightBrace => TokenKind::LeftBrace,
                    _ => unreachable!(),
                };
                match stack.pop() {
                    Some((open, _)) if open == expected_open => {}
                    Some((open, open_span)) => diagnostics.push(
                        Diagnostic::error(
                            DiagnosticCode::MismatchedDelimiter,
                            format!(
                                "mismatched delimiter: {} does not close {}",
                                token.kind, open
                            ),
                            token.span,
                        )
                        .with_label(open_span, "this delimiter was opened here"),
                    ),
                    None => diagnostics.push(Diagnostic::error(
                        DiagnosticCode::MismatchedDelimiter,
                        format!("unexpected closing delimiter {}", token.kind),
                        token.span,
                    )),
                }
            }
            _ => {}
        }
    }

    for (open, span) in stack {
        diagnostics.push(
            Diagnostic::error(
                DiagnosticCode::UnclosedDelimiter,
                format!("unclosed delimiter {open}"),
                span,
            )
            .with_help(format!(
                "add the matching delimiter before byte {}",
                source.len()
            )),
        );
    }
}

type LalrpopError = ParseError<usize, ParserToken, ()>;

fn parse_diagnostic(source: &str, error: LalrpopError) -> Diagnostic {
    match error {
        ParseError::InvalidToken { location } => Diagnostic::error(
            DiagnosticCode::InvalidToken,
            "invalid token",
            Span::empty(location.min(source.len())),
        ),
        ParseError::UnrecognizedEof { location, expected } => {
            let expected = normalized_expected(expected);
            let message = if expected.is_empty() {
                "unexpected end of input".to_owned()
            } else {
                format!("unexpected end of input; expected {expected}")
            };
            Diagnostic::error(
                DiagnosticCode::UnexpectedEof,
                message,
                Span::empty(location.min(source.len())),
            )
        }
        ParseError::UnrecognizedToken {
            token: (start, token, end),
            expected,
        } => {
            let expected = normalized_expected(expected);
            let message = if expected.is_empty() {
                format!("unexpected token {token}")
            } else {
                format!("unexpected token {token}; expected {expected}")
            };
            Diagnostic::error(
                DiagnosticCode::UnexpectedToken,
                message,
                Span::new(start, end),
            )
        }
        ParseError::ExtraToken {
            token: (start, token, end),
        } => Diagnostic::error(
            DiagnosticCode::ExtraToken,
            format!("extra token {token} after the end of the statement"),
            Span::new(start, end),
        ),
        ParseError::User { .. } => Diagnostic::error(
            DiagnosticCode::UnexpectedToken,
            "input does not satisfy the selected syntax production",
            Span::empty(source.len()),
        ),
    }
}

fn normalized_expected(mut expected: Vec<String>) -> String {
    for item in &mut expected {
        *item = item.trim_matches('"').to_owned();
    }
    expected.sort();
    expected.dedup();
    match expected.as_slice() {
        [] => String::new(),
        [only] => format!("`{only}`"),
        [head @ .., last] => {
            let head = head
                .iter()
                .map(|item| format!("`{item}`"))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{head}, or `{last}`")
        }
    }
}

fn parser_tokens(tokens: &[Token]) -> Vec<SpannedParserToken> {
    let significant = tokens
        .iter()
        .filter(|token| !token.is_trivia())
        .collect::<Vec<_>>();
    let mut result = Vec::with_capacity(significant.len());
    let mut index = 0;
    while index < significant.len() {
        if let Some(end) = escaped_parameter_end(&significant, index) {
            result.push((
                significant[index].span.start,
                ParserToken::Parameter,
                significant[end].span.end,
            ));
            index = end + 1;
            continue;
        }
        if let Some(end) = pattern_comprehension_end(&significant, index) {
            result.push((
                significant[index].span.start,
                ParserToken::PatternComprehension,
                significant[end].span.end,
            ));
            index = end + 1;
            continue;
        }
        if significant[index].kind == TokenKind::LeftParen
            && is_pattern_expression_context(&significant, index)
            && let Some(end) = pattern_expression_end(&significant, index)
        {
            result.push((
                significant[index].span.start,
                ParserToken::PatternExpression,
                significant[end].span.end,
            ));
            index = end + 1;
            continue;
        }
        if is_relationship_label_start(&significant, index)
            && let Some(end) = relationship_label_end(&significant, index)
        {
            result.push((
                significant[index].span.start,
                ParserToken::RelationshipLabelExpression,
                significant[end].span.end,
            ));
            index = end + 1;
            continue;
        }
        if is_label_predicate_context(&significant, index)
            && let Some(end) = label_predicate_end(&significant, index)
        {
            result.push((
                significant[index].span.start,
                ParserToken::LabelPredicate,
                significant[end].span.end,
            ));
            index = end + 1;
            continue;
        }
        if !is_procedure_name_start(&significant, index)
            && let Some(end) = qualified_function_name_end(&significant, index)
        {
            result.push((
                significant[index].span.start,
                ParserToken::QualifiedFunctionName,
                significant[end].span.end,
            ));
            index = end + 1;
            continue;
        }
        let token = significant[index];
        let mut parser_token = ParserToken::from(token.kind);
        if parser_token == ParserToken::All && is_explicit_set_all(&significant, index) {
            parser_token = ParserToken::SetAll;
        }
        result.push((token.span.start, parser_token, token.span.end));
        index += 1;
    }
    result
}

fn escaped_parameter_end(tokens: &[&Token], start: usize) -> Option<usize> {
    (tokens.get(start)?.kind == TokenKind::Dollar
        && tokens.get(start + 1).is_some_and(|token| {
            token.kind == TokenKind::EscapedIdentifier && tokens[start].span.end == token.span.start
        }))
    .then_some(start + 1)
}

fn qualified_function_name_end(tokens: &[&Token], start: usize) -> Option<usize> {
    if !is_symbolic_name_kind(tokens.get(start)?.kind) {
        return None;
    }

    let mut end = start;
    let mut components = 1usize;
    while tokens
        .get(end + 1)
        .is_some_and(|token| token.kind == TokenKind::Dot)
        && tokens
            .get(end + 2)
            .is_some_and(|token| is_symbolic_name_kind(token.kind))
    {
        end += 2;
        components += 1;
    }

    (components > 1
        && tokens
            .get(end + 1)
            .is_some_and(|token| token.kind == TokenKind::LeftParen))
    .then_some(end)
}

fn is_procedure_name_start(tokens: &[&Token], start: usize) -> bool {
    let mut first_component = start;
    while first_component >= 2
        && tokens[first_component - 1].kind == TokenKind::Dot
        && is_symbolic_name_kind(tokens[first_component - 2].kind)
    {
        first_component -= 2;
    }

    first_component
        .checked_sub(1)
        .and_then(|index| tokens.get(index))
        .is_some_and(|token| token.kind == TokenKind::Keyword(Keyword::Call))
}

fn is_symbolic_name_kind(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Identifier | TokenKind::EscapedIdentifier | TokenKind::Keyword(_)
    )
}

fn is_explicit_set_all(tokens: &[&Token], index: usize) -> bool {
    if tokens
        .get(index + 1)
        .is_some_and(|token| token.kind == TokenKind::LeftParen)
    {
        return false;
    }

    let Some(previous) = index.checked_sub(1).and_then(|index| tokens.get(index)) else {
        return false;
    };
    if matches!(
        previous.kind,
        TokenKind::Keyword(Keyword::Return | Keyword::With)
    ) {
        return true;
    }
    if previous.kind != TokenKind::LeftParen {
        return false;
    }

    index
        .checked_sub(2)
        .and_then(|index| tokens.get(index))
        .is_some_and(|token| {
            matches!(
                ParserToken::from(token.kind),
                ParserToken::Identifier
                    | ParserToken::EscapedIdentifier
                    | ParserToken::Count
                    | ParserToken::Match
                    | ParserToken::Path
                    | ParserToken::Return
            )
        })
}

fn pattern_comprehension_end(tokens: &[&Token], start: usize) -> Option<usize> {
    if tokens.get(start)?.kind != TokenKind::LeftBracket {
        return None;
    }

    let mut brackets = 0usize;
    let mut parentheses = 0usize;
    let mut braces = 0usize;
    let mut pipe_at = None;
    let mut arrow = false;
    let mut top_level_nodes = 0usize;
    let mut top_level_minuses = 0usize;
    for (index, token) in tokens.iter().enumerate().skip(start) {
        match token.kind {
            TokenKind::LeftBracket => brackets += 1,
            TokenKind::RightBracket => {
                if brackets == 1 && parentheses == 0 && braces == 0 {
                    let has_projection = pipe_at.is_some_and(|pipe| index > pipe + 1);
                    let undirected_relationship = top_level_nodes >= 2 && top_level_minuses >= 2;
                    return (has_projection && (arrow || undirected_relationship)).then_some(index);
                }
                brackets = brackets.saturating_sub(1);
            }
            TokenKind::LeftParen => {
                if pipe_at.is_none() && brackets == 1 && parentheses == 0 && braces == 0 {
                    top_level_nodes += 1;
                }
                parentheses += 1;
            }
            TokenKind::RightParen => parentheses = parentheses.saturating_sub(1),
            TokenKind::LeftBrace => braces += 1,
            TokenKind::RightBrace => braces = braces.saturating_sub(1),
            TokenKind::Pipe if brackets == 1 && parentheses == 0 && braces == 0 => {
                pipe_at = Some(index)
            }
            TokenKind::LeftArrow | TokenKind::RightArrow if pipe_at.is_none() => arrow = true,
            TokenKind::Minus
                if pipe_at.is_none() && brackets == 1 && parentheses == 0 && braces == 0 =>
            {
                top_level_minuses += 1;
            }
            _ => {}
        }
    }
    None
}

fn is_pattern_expression_context(tokens: &[&Token], start: usize) -> bool {
    if start >= 2
        && tokens[start - 1].kind == TokenKind::LeftParen
        && matches!(
            tokens[start - 2].kind,
            TokenKind::Keyword(Keyword::ShortestPath | Keyword::AllShortestPaths)
        )
    {
        return false;
    }
    if start >= 2
        && tokens[start - 1].kind == TokenKind::LeftBrace
        && matches!(
            tokens[start - 2].kind,
            TokenKind::Keyword(Keyword::Exists | Keyword::Count | Keyword::Collect)
        )
    {
        return false;
    }
    if is_in_map_entry_value(tokens, start) {
        return true;
    }

    // Like is_label_predicate_context, skip markers inside completed
    // bracketed/braced constructs (a relationship-type `|` in `[:A|B]` is not
    // an expression context), while a comprehension's projection `|` in the
    // construct that still encloses the candidate remains visible.
    let mut nested_brackets = 0usize;
    let mut nested_braces = 0usize;
    for (index, token) in tokens[..start].iter().enumerate().rev() {
        match token.kind {
            TokenKind::RightBracket => {
                nested_brackets = nested_brackets.saturating_add(1);
                continue;
            }
            TokenKind::LeftBracket if nested_brackets != 0 => {
                nested_brackets = nested_brackets.saturating_sub(1);
                continue;
            }
            TokenKind::RightBrace => {
                nested_braces = nested_braces.saturating_add(1);
                continue;
            }
            TokenKind::LeftBrace if nested_braces != 0 => {
                nested_braces = nested_braces.saturating_sub(1);
                continue;
            }
            _ if nested_brackets != 0 || nested_braces != 0 => continue,
            _ => {}
        }
        if tokens
            .get(index + 1)
            .is_some_and(|next| next.kind == TokenKind::Equal)
        {
            continue;
        }
        match token.kind {
            TokenKind::Pipe => return true,
            TokenKind::Keyword(
                Keyword::Where
                | Keyword::Return
                | Keyword::With
                | Keyword::Unwind
                | Keyword::Set
                | Keyword::Delete
                | Keyword::Then
                | Keyword::Else
                | Keyword::When
                | Keyword::Case,
            ) => return true,
            TokenKind::Keyword(
                Keyword::Match | Keyword::Create | Keyword::Merge | Keyword::Call | Keyword::Remove,
            ) => return false,
            _ => {}
        }
    }
    false
}

fn pattern_expression_end(tokens: &[&Token], start: usize) -> Option<usize> {
    if tokens.get(start)?.kind != TokenKind::LeftParen {
        return None;
    }

    let mut node_end =
        matching_delimiter(tokens, start, TokenKind::LeftParen, TokenKind::RightParen)?;
    let mut relationships = 0usize;
    while let Some(after_relationship) = relationship_syntax_end(tokens, node_end + 1) {
        let mut next_node = after_relationship;
        if matches!(
            tokens.get(next_node).map(|token| token.kind),
            Some(TokenKind::Star | TokenKind::Plus | TokenKind::Question)
        ) {
            next_node += 1;
        } else if tokens
            .get(next_node)
            .is_some_and(|token| token.kind == TokenKind::LeftBrace)
        {
            next_node = matching_delimiter(
                tokens,
                next_node,
                TokenKind::LeftBrace,
                TokenKind::RightBrace,
            )? + 1;
        }
        if !tokens
            .get(next_node)
            .is_some_and(|token| token.kind == TokenKind::LeftParen)
        {
            break;
        }
        node_end = matching_delimiter(
            tokens,
            next_node,
            TokenKind::LeftParen,
            TokenKind::RightParen,
        )?;
        relationships += 1;
    }

    (relationships > 0).then_some(node_end)
}

fn relationship_syntax_end(tokens: &[&Token], start: usize) -> Option<usize> {
    let mut cursor = match (
        tokens.get(start).map(|token| token.kind),
        tokens.get(start + 1).map(|token| token.kind),
    ) {
        (Some(TokenKind::Minus | TokenKind::LeftArrow), _) => start + 1,
        (Some(TokenKind::Less), Some(TokenKind::Minus)) => start + 2,
        _ => return None,
    };
    if tokens
        .get(cursor)
        .is_some_and(|token| token.kind == TokenKind::LeftBracket)
    {
        cursor = matching_delimiter(
            tokens,
            cursor,
            TokenKind::LeftBracket,
            TokenKind::RightBracket,
        )? + 1;
    }
    match (
        tokens.get(cursor).map(|token| token.kind),
        tokens.get(cursor + 1).map(|token| token.kind),
    ) {
        (Some(TokenKind::Minus), Some(TokenKind::Greater)) => Some(cursor + 2),
        (Some(TokenKind::Minus | TokenKind::RightArrow), _) => Some(cursor + 1),
        _ => None,
    }
}

fn matching_delimiter(
    tokens: &[&Token],
    start: usize,
    open: TokenKind,
    close: TokenKind,
) -> Option<usize> {
    if tokens.get(start)?.kind != open {
        return None;
    }
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate().skip(start) {
        if token.kind == open {
            depth += 1;
        } else if token.kind == close {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

fn is_relationship_label_start(tokens: &[&Token], start: usize) -> bool {
    if tokens
        .get(start)
        .is_none_or(|token| token.kind != TokenKind::Colon)
    {
        return false;
    }
    match start.checked_sub(1).and_then(|index| tokens.get(index)) {
        Some(token) if token.kind == TokenKind::LeftBracket => true,
        Some(token) if is_symbolic_name_kind(token.kind) => {
            start >= 2 && tokens[start - 2].kind == TokenKind::LeftBracket
        }
        _ => false,
    }
}

fn relationship_label_end(tokens: &[&Token], start: usize) -> Option<usize> {
    let end = label_predicate_end(tokens, start)?;
    if !(start + 1..=end).any(|index| tokens[index].kind == TokenKind::Colon) {
        return Some(end);
    }

    let mut cursor = start + 1;
    loop {
        if cursor > end || !is_symbolic_name_kind(tokens[cursor].kind) {
            return None;
        }
        cursor += 1;
        if cursor > end {
            return Some(end);
        }
        if tokens[cursor].kind != TokenKind::Pipe
            || tokens
                .get(cursor + 1)
                .is_none_or(|token| token.kind != TokenKind::Colon)
        {
            return None;
        }
        cursor += 2;
    }
}

fn is_label_predicate_context(tokens: &[&Token], start: usize) -> bool {
    if !matches!(
        tokens.get(start).map(|token| token.kind),
        Some(TokenKind::Colon | TokenKind::Keyword(Keyword::Is))
    ) {
        return false;
    }
    if tokens[start].kind == TokenKind::Keyword(Keyword::Is)
        && tokens
            .get(start + 1)
            .is_some_and(|token| token.kind == TokenKind::Keyword(Keyword::As))
        && tokens
            .get(start + 2)
            .is_some_and(|token| is_symbolic_name_kind(token.kind))
    {
        return false;
    }
    if tokens[start].kind == TokenKind::Colon
        && start >= 2
        && is_symbolic_name_kind(tokens[start - 1].kind)
        && matches!(tokens[start - 2].kind, TokenKind::LeftBrace)
    {
        return false;
    }
    if is_in_map_entry_value(tokens, start) {
        return true;
    }
    if tokens[start].kind == TokenKind::Colon
        && start >= 2
        && is_symbolic_name_kind(tokens[start - 1].kind)
        && tokens[start - 2].kind == TokenKind::Comma
        && is_inside_braces(tokens, start)
    {
        return false;
    }

    // Ignore context markers inside a completed bracketed/braced construct.
    // In particular, a relationship-type `|` in `[:A|B]` must not make a
    // later node label look like an expression predicate. Markers in the
    // construct that currently encloses the candidate still have zero
    // relative depth and remain visible (for example a comprehension `|`).
    let mut nested_brackets = 0usize;
    let mut nested_braces = 0usize;
    for (index, token) in tokens[..start].iter().enumerate().rev() {
        match token.kind {
            TokenKind::RightBracket => {
                nested_brackets = nested_brackets.saturating_add(1);
                continue;
            }
            TokenKind::LeftBracket if nested_brackets != 0 => {
                nested_brackets = nested_brackets.saturating_sub(1);
                continue;
            }
            TokenKind::RightBrace => {
                nested_braces = nested_braces.saturating_add(1);
                continue;
            }
            TokenKind::LeftBrace if nested_braces != 0 => {
                nested_braces = nested_braces.saturating_sub(1);
                continue;
            }
            _ if nested_brackets != 0 || nested_braces != 0 => continue,
            _ => {}
        }
        if tokens
            .get(index + 1)
            .is_some_and(|next| next.kind == TokenKind::Equal)
        {
            continue;
        }
        match token.kind {
            TokenKind::Pipe => return true,
            TokenKind::Keyword(
                Keyword::Where
                | Keyword::Return
                | Keyword::With
                | Keyword::Unwind
                | Keyword::Delete
                | Keyword::Then
                | Keyword::Else
                | Keyword::When
                | Keyword::Case,
            ) => return true,
            TokenKind::Keyword(
                Keyword::Match
                | Keyword::Create
                | Keyword::Merge
                | Keyword::Call
                | Keyword::Set
                | Keyword::Remove,
            ) => return false,
            _ => {}
        }
    }
    false
}

fn is_inside_braces(tokens: &[&Token], start: usize) -> bool {
    let mut nested = 0usize;
    for token in tokens[..start].iter().rev() {
        match token.kind {
            TokenKind::RightBrace => nested += 1,
            TokenKind::LeftBrace if nested == 0 => return true,
            TokenKind::LeftBrace => nested = nested.saturating_sub(1),
            _ => {}
        }
    }
    false
}

fn is_in_map_entry_value(tokens: &[&Token], start: usize) -> bool {
    let mut open_braces = Vec::new();
    for (index, token) in tokens.iter().enumerate().take(start) {
        match token.kind {
            TokenKind::LeftBrace => open_braces.push(index),
            TokenKind::RightBrace => {
                let _ = open_braces.pop();
            }
            _ => {}
        }
    }
    let Some(open) = open_braces.last().copied() else {
        return false;
    };

    let mut depth = 0usize;
    let mut after_separator = false;
    for token in &tokens[open + 1..start] {
        match token.kind {
            TokenKind::LeftParen | TokenKind::LeftBracket | TokenKind::LeftBrace => {
                depth += 1;
            }
            TokenKind::RightParen | TokenKind::RightBracket | TokenKind::RightBrace => {
                depth = depth.saturating_sub(1);
            }
            TokenKind::Comma if depth == 0 => after_separator = false,
            TokenKind::Colon if depth == 0 => after_separator = true,
            _ => {}
        }
    }
    after_separator
}

fn label_predicate_end(tokens: &[&Token], start: usize) -> Option<usize> {
    let expression_start = match tokens.get(start)?.kind {
        TokenKind::Colon => start + 1,
        TokenKind::Keyword(Keyword::Is) => {
            if (matches!(
                tokens.get(start + 1).map(|token| token.kind),
                Some(TokenKind::Keyword(Keyword::Null))
            ) && !matches!(
                tokens.get(start + 2).map(|token| token.kind),
                Some(TokenKind::Pipe | TokenKind::Ampersand)
            )) || matches!(
                (
                    tokens.get(start + 1).map(|token| token.kind),
                    tokens.get(start + 2).map(|token| token.kind),
                ),
                (
                    Some(TokenKind::Keyword(Keyword::Not)),
                    Some(TokenKind::Keyword(Keyword::Null)),
                )
            ) {
                return None;
            }
            start + 1
        }
        _ => return None,
    };
    let end = label_or_end(tokens, expression_start)?;
    Some(end.saturating_sub(1))
}

fn label_or_end(tokens: &[&Token], start: usize) -> Option<usize> {
    let mut cursor = label_and_end(tokens, start)?;
    while tokens
        .get(cursor)
        .is_some_and(|token| token.kind == TokenKind::Pipe)
    {
        cursor += 1;
        if tokens
            .get(cursor)
            .is_some_and(|token| token.kind == TokenKind::Colon)
        {
            cursor += 1;
        }
        cursor = label_and_end(tokens, cursor)?;
    }
    Some(cursor)
}

fn label_and_end(tokens: &[&Token], start: usize) -> Option<usize> {
    let mut cursor = label_primary_end(tokens, start)?;
    while matches!(
        tokens.get(cursor).map(|token| token.kind),
        Some(TokenKind::Ampersand | TokenKind::Colon)
    ) {
        cursor = label_primary_end(tokens, cursor + 1)?;
    }
    Some(cursor)
}

fn label_primary_end(tokens: &[&Token], start: usize) -> Option<usize> {
    let mut cursor = start;
    if tokens
        .get(cursor)
        .is_some_and(|token| token.kind == TokenKind::Bang)
    {
        cursor += 1;
    }
    match tokens.get(cursor)?.kind {
        TokenKind::Percent => Some(cursor + 1),
        kind if is_symbolic_name_kind(kind) => Some(cursor + 1),
        TokenKind::LeftParen => {
            let end = label_or_end(tokens, cursor + 1)?;
            tokens
                .get(end)
                .is_some_and(|token| token.kind == TokenKind::RightParen)
                .then_some(end + 1)
        }
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ParserToken {
    Identifier,
    EscapedIdentifier,
    Parameter,
    Integer,
    HexInteger,
    OctalInteger,
    Float,
    String,
    PatternComprehension,
    PatternExpression,
    LabelPredicate,
    RelationshipLabelExpression,
    QualifiedFunctionName,
    All,
    SetAll,
    AllShortestPaths,
    And,
    Any,
    As,
    Asc,
    Ascending,
    By,
    Call,
    Case,
    Contains,
    Collect,
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
    Acyclic,
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
    Invalid,
}

impl fmt::Display for ParserToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "`{}`", self.name())
    }
}

impl ParserToken {
    const fn is_keyword_literal(self) -> bool {
        matches!(
            self,
            Self::False | Self::Inf | Self::Infinity | Self::Nan | Self::Null | Self::True
        )
    }

    const fn is_contextual_name_candidate(self) -> bool {
        !matches!(
            self,
            Self::Identifier
                | Self::EscapedIdentifier
                | Self::Parameter
                | Self::Integer
                | Self::HexInteger
                | Self::OctalInteger
                | Self::Float
                | Self::String
                | Self::PatternComprehension
                | Self::PatternExpression
                | Self::LabelPredicate
                | Self::RelationshipLabelExpression
                | Self::QualifiedFunctionName
                | Self::LeftParen
                | Self::RightParen
                | Self::LeftBracket
                | Self::RightBracket
                | Self::LeftBrace
                | Self::RightBrace
                | Self::Comma
                | Self::Dot
                | Self::DotDot
                | Self::Colon
                | Self::DoubleColon
                | Self::Semicolon
                | Self::Pipe
                | Self::DoublePipe
                | Self::Ampersand
                | Self::Question
                | Self::Dollar
                | Self::Plus
                | Self::Minus
                | Self::Star
                | Self::Slash
                | Self::Percent
                | Self::Caret
                | Self::Bang
                | Self::Equal
                | Self::NotEqual
                | Self::Less
                | Self::LessEqual
                | Self::Greater
                | Self::GreaterEqual
                | Self::PlusEqual
                | Self::FatArrow
                | Self::RegexMatch
                | Self::LeftArrow
                | Self::RightArrow
                | Self::Invalid
        )
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Identifier => "identifier",
            Self::EscapedIdentifier => "escaped identifier",
            Self::Parameter => "parameter",
            Self::Integer => "integer",
            Self::HexInteger => "hex integer",
            Self::OctalInteger => "octal integer",
            Self::Float => "float",
            Self::String => "string",
            Self::PatternComprehension => "pattern comprehension",
            Self::PatternExpression => "pattern expression",
            Self::LabelPredicate => "label predicate",
            Self::RelationshipLabelExpression => "relationship label expression",
            Self::QualifiedFunctionName => "qualified function name",
            Self::All => "ALL",
            Self::SetAll => "ALL",
            Self::AllShortestPaths => "ALLSHORTESTPATHS",
            Self::And => "AND",
            Self::Any => "ANY",
            Self::As => "AS",
            Self::Asc => "ASC",
            Self::Ascending => "ASCENDING",
            Self::By => "BY",
            Self::Call => "CALL",
            Self::Case => "CASE",
            Self::Contains => "CONTAINS",
            Self::Collect => "COLLECT",
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
            Self::Acyclic => "ACYCLIC",
            Self::LeftParen => "(",
            Self::RightParen => ")",
            Self::LeftBracket => "[",
            Self::RightBracket => "]",
            Self::LeftBrace => "{",
            Self::RightBrace => "}",
            Self::Comma => ",",
            Self::Dot => ".",
            Self::DotDot => "..",
            Self::Colon => ":",
            Self::DoubleColon => "::",
            Self::Semicolon => ";",
            Self::Pipe => "|",
            Self::DoublePipe => "||",
            Self::Ampersand => "&",
            Self::Question => "?",
            Self::Dollar => "$",
            Self::Plus => "+",
            Self::Minus => "-",
            Self::Star => "*",
            Self::Slash => "/",
            Self::Percent => "%",
            Self::Caret => "^",
            Self::Bang => "!",
            Self::Equal => "=",
            Self::NotEqual => "<>",
            Self::Less => "<",
            Self::LessEqual => "<=",
            Self::Greater => ">",
            Self::GreaterEqual => ">=",
            Self::PlusEqual => "+=",
            Self::FatArrow => "=>",
            Self::RegexMatch => "=~",
            Self::LeftArrow => "<-",
            Self::RightArrow => "->",
            Self::Invalid => "invalid token",
        }
    }
}

impl From<TokenKind> for ParserToken {
    fn from(kind: TokenKind) -> Self {
        match kind {
            TokenKind::Keyword(keyword) => keyword_token(keyword),
            TokenKind::Identifier => Self::Identifier,
            TokenKind::EscapedIdentifier => Self::EscapedIdentifier,
            TokenKind::Parameter => Self::Parameter,
            TokenKind::Integer => Self::Integer,
            TokenKind::HexInteger => Self::HexInteger,
            TokenKind::OctalInteger => Self::OctalInteger,
            TokenKind::Float => Self::Float,
            TokenKind::String => Self::String,
            TokenKind::LeftParen => Self::LeftParen,
            TokenKind::RightParen => Self::RightParen,
            TokenKind::LeftBracket => Self::LeftBracket,
            TokenKind::RightBracket => Self::RightBracket,
            TokenKind::LeftBrace => Self::LeftBrace,
            TokenKind::RightBrace => Self::RightBrace,
            TokenKind::Comma => Self::Comma,
            TokenKind::Dot => Self::Dot,
            TokenKind::DotDot => Self::DotDot,
            TokenKind::Colon => Self::Colon,
            TokenKind::DoubleColon => Self::DoubleColon,
            TokenKind::Semicolon => Self::Semicolon,
            TokenKind::Pipe => Self::Pipe,
            TokenKind::DoublePipe => Self::DoublePipe,
            TokenKind::Ampersand => Self::Ampersand,
            TokenKind::Question => Self::Question,
            TokenKind::Dollar => Self::Dollar,
            TokenKind::Plus => Self::Plus,
            TokenKind::Minus => Self::Minus,
            TokenKind::Star => Self::Star,
            TokenKind::Slash => Self::Slash,
            TokenKind::Percent => Self::Percent,
            TokenKind::Caret => Self::Caret,
            TokenKind::Bang => Self::Bang,
            TokenKind::Equal => Self::Equal,
            TokenKind::NotEqual => Self::NotEqual,
            TokenKind::Less => Self::Less,
            TokenKind::LessEqual => Self::LessEqual,
            TokenKind::Greater => Self::Greater,
            TokenKind::GreaterEqual => Self::GreaterEqual,
            TokenKind::PlusEqual => Self::PlusEqual,
            TokenKind::FatArrow => Self::FatArrow,
            TokenKind::RegexMatch => Self::RegexMatch,
            TokenKind::LeftArrow => Self::LeftArrow,
            TokenKind::RightArrow => Self::RightArrow,
            TokenKind::Invalid => Self::Invalid,
            TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment => {
                unreachable!("trivia is filtered before parser token conversion")
            }
        }
    }
}

fn keyword_token(keyword: Keyword) -> ParserToken {
    match keyword {
        Keyword::All => ParserToken::All,
        Keyword::AllShortestPaths => ParserToken::AllShortestPaths,
        Keyword::And => ParserToken::And,
        Keyword::Any => ParserToken::Any,
        Keyword::As => ParserToken::As,
        Keyword::Asc => ParserToken::Asc,
        Keyword::Ascending => ParserToken::Ascending,
        Keyword::By => ParserToken::By,
        Keyword::Call => ParserToken::Call,
        Keyword::Case => ParserToken::Case,
        Keyword::Contains => ParserToken::Contains,
        Keyword::Collect => ParserToken::Collect,
        Keyword::Count => ParserToken::Count,
        Keyword::Create => ParserToken::Create,
        Keyword::Delete => ParserToken::Delete,
        Keyword::Desc => ParserToken::Desc,
        Keyword::Descending => ParserToken::Descending,
        Keyword::Detach => ParserToken::Detach,
        Keyword::Distinct => ParserToken::Distinct,
        Keyword::Else => ParserToken::Else,
        Keyword::End => ParserToken::End,
        Keyword::Ends => ParserToken::Ends,
        Keyword::Exists => ParserToken::Exists,
        Keyword::False => ParserToken::False,
        Keyword::Group => ParserToken::Group,
        Keyword::Groups => ParserToken::Groups,
        Keyword::In => ParserToken::In,
        Keyword::Inf => ParserToken::Inf,
        Keyword::Infinity => ParserToken::Infinity,
        Keyword::Is => ParserToken::Is,
        Keyword::Limit => ParserToken::Limit,
        Keyword::Match => ParserToken::Match,
        Keyword::Merge => ParserToken::Merge,
        Keyword::Nan => ParserToken::Nan,
        Keyword::None => ParserToken::None,
        Keyword::Not => ParserToken::Not,
        Keyword::Null => ParserToken::Null,
        Keyword::Offset => ParserToken::Offset,
        Keyword::On => ParserToken::On,
        Keyword::Optional => ParserToken::Optional,
        Keyword::Or => ParserToken::Or,
        Keyword::Order => ParserToken::Order,
        Keyword::Path => ParserToken::Path,
        Keyword::Paths => ParserToken::Paths,
        Keyword::Reduce => ParserToken::Reduce,
        Keyword::Remove => ParserToken::Remove,
        Keyword::Return => ParserToken::Return,
        Keyword::Set => ParserToken::Set,
        Keyword::Shortest => ParserToken::Shortest,
        Keyword::ShortestPath => ParserToken::ShortestPath,
        Keyword::Simple => ParserToken::Simple,
        Keyword::Single => ParserToken::Single,
        Keyword::Skip => ParserToken::Skip,
        Keyword::Starts => ParserToken::Starts,
        Keyword::Then => ParserToken::Then,
        Keyword::Trail => ParserToken::Trail,
        Keyword::Trim => ParserToken::Trim,
        Keyword::True => ParserToken::True,
        Keyword::Union => ParserToken::Union,
        Keyword::Unwind => ParserToken::Unwind,
        Keyword::Walk => ParserToken::Walk,
        Keyword::When => ParserToken::When,
        Keyword::Where => ParserToken::Where,
        Keyword::With => ParserToken::With,
        Keyword::Xor => ParserToken::Xor,
        Keyword::Yield => ParserToken::Yield,
        Keyword::Acyclic => ParserToken::Acyclic,
    }
}

pub(crate) fn name(source: &str, start: usize, end: usize, escaped: bool) -> Name {
    let span = Span::new(start, end);
    let raw = &source[start..end];
    let text = if escaped {
        decode_delimited(raw, '`')
    } else {
        raw.to_owned()
    };
    Node::new(Identifier::new(text, escaped), span)
}

pub(crate) fn qualified_function_name(source: &str, start: usize, end: usize) -> QualifiedName {
    let fragment = source.get(start..end).unwrap_or_default();
    let lexed = lexer::lex(fragment);
    let parts = lexed
        .tokens
        .iter()
        .filter(|token| !token.is_trivia() && token.kind != TokenKind::Dot)
        .map(|token| {
            let part_start = start + token.span.start;
            let part_end = start + token.span.end;
            name(
                source,
                part_start,
                part_end,
                token.kind == TokenKind::EscapedIdentifier,
            )
        })
        .collect();
    QualifiedName {
        parts,
        span: Span::new(start, end),
    }
}

pub(crate) fn parameter(source: &str, start: usize, end: usize) -> Parameter {
    let raw = &source[start + 1..end];
    let name = if raw.starts_with('`') && raw.ends_with('`') {
        ParameterName::Named(decode_delimited(raw, '`'))
    } else if raw.bytes().all(|byte| byte.is_ascii_digit()) {
        ParameterName::Positional(raw.to_owned())
    } else {
        ParameterName::Named(raw.to_owned())
    };
    Parameter { name }
}

pub(crate) fn string_literal(source: &str, start: usize, end: usize) -> StringLiteral {
    let raw = &source[start..end];
    let delimiter = raw.chars().next().unwrap_or('\'');
    let quote = if delimiter == '\'' {
        QuoteStyle::Single
    } else {
        QuoteStyle::Double
    };
    StringLiteral {
        value: decode_delimited(raw, delimiter),
        quote,
    }
}

fn decode_delimited(raw: &str, delimiter: char) -> String {
    let delimiter_len = delimiter.len_utf8();
    let inner = raw
        .get(delimiter_len..raw.len().saturating_sub(delimiter_len))
        .unwrap_or_default();
    let mut decoded = String::with_capacity(inner.len());
    let mut characters = inner.chars().peekable();
    while let Some(character) = characters.next() {
        if character == delimiter && characters.peek() == Some(&delimiter) {
            let _ = characters.next();
            decoded.push(delimiter);
            continue;
        }
        if character != '\\' {
            decoded.push(character);
            continue;
        }
        let Some(escaped) = characters.next() else {
            decoded.push('\\');
            break;
        };
        match escaped {
            't' => decoded.push('\t'),
            'b' => decoded.push('\u{0008}'),
            'n' => decoded.push('\n'),
            'r' => decoded.push('\r'),
            'f' => decoded.push('\u{000c}'),
            '\\' => decoded.push('\\'),
            '\'' => decoded.push('\''),
            '"' => decoded.push('"'),
            '`' => decoded.push('`'),
            'u' | 'U' => {
                let digits = if escaped == 'u' { 4 } else { 6 };
                let mut scalar = 0u32;
                let mut complete = true;
                for _ in 0..digits {
                    match characters.next().and_then(|digit| digit.to_digit(16)) {
                        Some(digit) => scalar = scalar * 16 + digit,
                        None => {
                            complete = false;
                            break;
                        }
                    }
                }
                if complete && let Some(character) = char::from_u32(scalar) {
                    decoded.push(character);
                }
            }
            other => {
                // Invalid escapes are rejected by the lexer, so preserving an
                // unexpected spelling here is only a defensive fallback.
                decoded.push('\\');
                decoded.push(other);
            }
        }
    }
    decoded
}

pub(crate) fn integer_literal(
    source: &str,
    start: usize,
    end: usize,
    radix: IntegerRadix,
) -> Literal {
    let span = Span::new(start, end);
    Node::new(
        LiteralKind::Integer(IntegerLiteral {
            text: source[start..end].to_owned(),
            radix,
        }),
        span,
    )
}

pub(crate) fn list_singleton_or_comprehension(start: usize, end: usize, expression: Expr) -> Expr {
    let span = Span::new(start, end);
    match split_comprehension_source(expression) {
        ComprehensionSource::Comprehension { variable, list } => Node::new(
            ExprKind::ListComprehension(ListComprehension {
                variable,
                list: Box::new(list),
                predicate: None,
                projection: None,
            }),
            span,
        ),
        ComprehensionSource::Expression(expression) => {
            Node::new(ExprKind::List(vec![expression]), span)
        }
    }
}

pub(crate) fn list_comprehension_expr(
    start: usize,
    end: usize,
    source_expression: Expr,
    predicate: Option<Expr>,
    projection: Option<Expr>,
) -> Result<Expr, LalrpopError> {
    let ComprehensionSource::Comprehension { variable, list } =
        split_comprehension_source(source_expression)
    else {
        return Err(user_parse_error());
    };
    Ok(Node::new(
        ExprKind::ListComprehension(ListComprehension {
            variable,
            list: Box::new(list),
            predicate: predicate.map(Box::new),
            projection: projection.map(Box::new),
        }),
        Span::new(start, end),
    ))
}

enum ComprehensionSource {
    Comprehension { variable: Name, list: Expr },
    Expression(Expr),
}

fn split_comprehension_source(expression: Expr) -> ComprehensionSource {
    let is_source = matches!(
        &expression.kind,
        ExprKind::Binary { left, operator, .. }
            if operator.kind == BinaryOperator::In
                && matches!(&left.kind, ExprKind::Variable(_))
    );
    if !is_source {
        return ComprehensionSource::Expression(expression);
    }

    let ExprKind::Binary {
        left,
        operator: _,
        right,
    } = expression.kind
    else {
        unreachable!("the source shape was checked above")
    };
    let ExprKind::Variable(variable) = left.kind else {
        unreachable!("the source variable shape was checked above")
    };
    ComprehensionSource::Comprehension {
        variable,
        list: *right,
    }
}

pub(crate) fn pattern_comprehension(
    source: &str,
    start: usize,
    end: usize,
) -> Result<Expr, LalrpopError> {
    let Some(fragment) = source.get(start..end) else {
        return Err(user_parse_error());
    };
    let lexed = lexer::lex(fragment);
    if !lexed.diagnostics.is_empty() {
        return Err(user_parse_error());
    }
    // A `|` at the comprehension's top level may be either the projection
    // separator or part of a label disjunction (for example `b:X|Y`). Try
    // each candidate as the separator, left to right; reserving it keeps the
    // label scanner from consuming it, and the first split that yields a
    // valid comprehension wins.
    for pipe_index in top_level_pipe_candidates(&lexed.tokens) {
        let tokens = nested_parser_tokens(&lexed.tokens, start, Some(pipe_index));
        if let Ok(expression) = parse_comprehension_tokens(source, start, end, &tokens) {
            return Ok(expression);
        }
    }
    Err(user_parse_error())
}

fn top_level_pipe_candidates(tokens: &[Token]) -> Vec<usize> {
    let mut candidates = Vec::new();
    let mut brackets = 0usize;
    let mut parentheses = 0usize;
    let mut braces = 0usize;
    for (index, token) in tokens.iter().enumerate() {
        if token.is_trivia() {
            continue;
        }
        match token.kind {
            TokenKind::LeftBracket => brackets += 1,
            TokenKind::RightBracket => brackets = brackets.saturating_sub(1),
            TokenKind::LeftParen => parentheses += 1,
            TokenKind::RightParen => parentheses = parentheses.saturating_sub(1),
            TokenKind::LeftBrace => braces += 1,
            TokenKind::RightBrace => braces = braces.saturating_sub(1),
            TokenKind::Pipe if brackets == 1 && parentheses == 0 && braces == 0 => {
                candidates.push(index);
            }
            _ => {}
        }
    }
    candidates
}

fn parse_comprehension_tokens(
    source: &str,
    start: usize,
    end: usize,
    tokens: &[SpannedParserToken],
) -> Result<Expr, LalrpopError> {
    let outer_start = tokens
        .iter()
        .position(|token| token.0 == start && token.1 == ParserToken::LeftBracket)
        .ok_or_else(user_parse_error)?;
    let outer_end = tokens
        .iter()
        .rposition(|token| token.2 == end && token.1 == ParserToken::RightBracket)
        .ok_or_else(user_parse_error)?;

    let pipe = find_top_level_token(tokens, outer_start + 1, outer_end, ParserToken::Pipe)
        .ok_or_else(user_parse_error)?;

    let mut pattern_start = outer_start + 1;
    let binding = if pattern_start + 1 < pipe
        && tokens[pattern_start + 1].1 == ParserToken::Equal
        && (is_name_token(tokens[pattern_start].1)
            || tokens[pattern_start].1.is_contextual_name_candidate())
    {
        let token = tokens[pattern_start];
        pattern_start += 2;
        Some(name(
            source,
            token.0,
            token.2,
            token.1 == ParserToken::EscapedIdentifier,
        ))
    } else {
        None
    };

    let where_index = find_top_level_token(tokens, pattern_start, pipe, ParserToken::Where);
    let pattern_end = where_index.unwrap_or(pipe);
    if pattern_start >= pattern_end || pipe + 1 >= outer_end {
        return Err(user_parse_error());
    }
    let pattern = parse_pattern_fragment(source, &tokens[pattern_start..pattern_end])?;
    let predicate = match where_index {
        Some(index) if index + 1 < pipe => Some(Box::new(parse_expr_fragment(
            source,
            &tokens[index + 1..pipe],
        )?)),
        Some(_) => return Err(user_parse_error()),
        None => None,
    };
    let projection = parse_expr_fragment(source, &tokens[pipe + 1..outer_end])?;

    Ok(Node::new(
        ExprKind::PatternComprehension(PatternComprehension {
            binding,
            pattern,
            predicate,
            projection: Box::new(projection),
        }),
        Span::new(start, end),
    ))
}

pub(crate) fn pattern_expression(
    source: &str,
    start: usize,
    end: usize,
) -> Result<Expr, LalrpopError> {
    let Some(fragment) = source.get(start..end) else {
        return Err(user_parse_error());
    };
    let lexed = lexer::lex(fragment);
    if !lexed.diagnostics.is_empty() {
        return Err(user_parse_error());
    }
    let tokens = nested_parser_tokens(&lexed.tokens, start, None);
    let pattern = parse_pattern_fragment(source, &tokens)?;
    Ok(Node::new(ExprKind::Pattern(pattern), Span::new(start, end)))
}

fn parse_pattern_fragment(
    source: &str,
    tokens: &[SpannedParserToken],
) -> Result<Pattern, LalrpopError> {
    parse_tokens_with_contextual_names(tokens, |contextual| {
        generated::PatternFragmentParser::new().parse(source, contextual.iter().copied().map(Ok))
    })
}

fn parse_expr_fragment(source: &str, tokens: &[SpannedParserToken]) -> Result<Expr, LalrpopError> {
    parse_tokens_with_contextual_names(tokens, |contextual| {
        generated::ExprFragmentParser::new().parse(source, contextual.iter().copied().map(Ok))
    })
}

pub(crate) fn label_comparison_suffix(
    source: &str,
    start: usize,
    end: usize,
) -> Result<ComparisonSuffix, LalrpopError> {
    Ok(ComparisonSuffix::Is {
        negated: false,
        predicate: IsPredicate::Label(parse_label_expression(source, start, end, true)?),
        end,
    })
}

pub(crate) fn relationship_label_expression(
    source: &str,
    start: usize,
    end: usize,
) -> Result<LabelExpression, LalrpopError> {
    parse_label_expression(source, start, end, true)
}

fn parse_label_expression(
    source: &str,
    start: usize,
    end: usize,
    normalize_legacy_relationship: bool,
) -> Result<LabelExpression, LalrpopError> {
    let Some(fragment) = source.get(start..end) else {
        return Err(user_parse_error());
    };
    let lexed = lexer::lex(fragment);
    if !lexed.diagnostics.is_empty() {
        return Err(user_parse_error());
    }
    let mut tokens = lexed
        .tokens
        .iter()
        .filter(|token| !token.is_trivia())
        .map(|token| {
            (
                start + token.span.start,
                ParserToken::from(token.kind),
                start + token.span.end,
            )
        })
        .collect::<Vec<_>>();
    if normalize_legacy_relationship && !legacy_label_tokens_are_consistent(&tokens) {
        return Err(user_parse_error());
    }
    let mut normalized = Vec::with_capacity(tokens.len());
    for (index, mut token) in tokens.drain(..).enumerate() {
        if normalize_legacy_relationship
            && token.1 == ParserToken::Colon
            && normalized
                .last()
                .is_some_and(|previous: &SpannedParserToken| previous.1 == ParserToken::Pipe)
        {
            continue;
        }
        if index > 0 && token.1.is_contextual_name_candidate() {
            token.1 = ParserToken::Identifier;
        }
        normalized.push(token);
    }
    generated::LabelPredicateFragmentParser::new().parse(source, normalized.into_iter().map(Ok))
}

fn legacy_label_tokens_are_consistent(tokens: &[SpannedParserToken]) -> bool {
    let has_internal_colon = tokens
        .iter()
        .enumerate()
        .skip(1)
        .any(|(_, token)| token.1 == ParserToken::Colon);
    if !has_internal_colon {
        return true;
    }
    if tokens
        .first()
        .is_none_or(|token| token.1 != ParserToken::Colon)
    {
        return false;
    }

    let mut cursor = 1usize;
    if tokens
        .get(cursor)
        .is_none_or(|token| !is_name_token(token.1) && !token.1.is_contextual_name_candidate())
    {
        return false;
    }
    cursor += 1;
    let pipe_separated = tokens
        .get(cursor)
        .is_some_and(|token| token.1 == ParserToken::Pipe);
    while cursor < tokens.len() {
        if pipe_separated {
            if tokens
                .get(cursor)
                .is_none_or(|token| token.1 != ParserToken::Pipe)
                || tokens
                    .get(cursor + 1)
                    .is_none_or(|token| token.1 != ParserToken::Colon)
            {
                return false;
            }
            cursor += 2;
        } else {
            if tokens
                .get(cursor)
                .is_none_or(|token| token.1 != ParserToken::Colon)
            {
                return false;
            }
            cursor += 1;
        }
        if tokens
            .get(cursor)
            .is_none_or(|token| !is_name_token(token.1) && !token.1.is_contextual_name_candidate())
        {
            return false;
        }
        cursor += 1;
    }
    true
}

fn user_parse_error() -> LalrpopError {
    ParseError::User { error: () }
}

/// `reserved_pipe` is a raw index into `tokens` naming a `|` that must come
/// through as a plain `Pipe` token (a pattern comprehension's projection
/// separator): forward scans starting before it see a truncated stream so
/// they cannot consume it, while context checks keep the full view.
fn nested_parser_tokens(
    tokens: &[Token],
    base: usize,
    reserved_pipe: Option<usize>,
) -> Vec<SpannedParserToken> {
    let significant = tokens
        .iter()
        .filter(|token| !token.is_trivia())
        .collect::<Vec<_>>();
    let boundary = reserved_pipe.map(|raw| {
        tokens[..raw]
            .iter()
            .filter(|token| !token.is_trivia())
            .count()
    });
    let mut result = Vec::with_capacity(significant.len());
    let mut index = 0;
    while index < significant.len() {
        let scan: &[&Token] = match boundary {
            Some(boundary) if index < boundary => &significant[..boundary],
            _ => &significant,
        };
        if let Some(end) = escaped_parameter_end(scan, index) {
            result.push((
                base + significant[index].span.start,
                ParserToken::Parameter,
                base + significant[end].span.end,
            ));
            index = end + 1;
            continue;
        }
        if index != 0 {
            if let Some(end) = pattern_comprehension_end(scan, index) {
                result.push((
                    base + significant[index].span.start,
                    ParserToken::PatternComprehension,
                    base + significant[end].span.end,
                ));
                index = end + 1;
                continue;
            }
            if significant[index].kind == TokenKind::LeftParen
                && is_pattern_expression_context(&significant, index)
                && let Some(end) = pattern_expression_end(scan, index)
            {
                result.push((
                    base + significant[index].span.start,
                    ParserToken::PatternExpression,
                    base + significant[end].span.end,
                ));
                index = end + 1;
                continue;
            }
            if is_relationship_label_start(&significant, index)
                && let Some(end) = relationship_label_end(scan, index)
            {
                result.push((
                    base + significant[index].span.start,
                    ParserToken::RelationshipLabelExpression,
                    base + significant[end].span.end,
                ));
                index = end + 1;
                continue;
            }
            if is_label_predicate_context(&significant, index)
                && let Some(end) = label_predicate_end(scan, index)
            {
                result.push((
                    base + significant[index].span.start,
                    ParserToken::LabelPredicate,
                    base + significant[end].span.end,
                ));
                index = end + 1;
                continue;
            }
        }

        if !is_procedure_name_start(&significant, index)
            && let Some(end) = qualified_function_name_end(scan, index)
        {
            result.push((
                base + significant[index].span.start,
                ParserToken::QualifiedFunctionName,
                base + significant[end].span.end,
            ));
            index = end + 1;
            continue;
        }

        let token = significant[index];
        let mut parser_token = ParserToken::from(token.kind);
        if parser_token == ParserToken::All && is_explicit_set_all(&significant, index) {
            parser_token = ParserToken::SetAll;
        }
        result.push((base + token.span.start, parser_token, base + token.span.end));
        index += 1;
    }
    result
}

fn find_top_level_token(
    tokens: &[SpannedParserToken],
    start: usize,
    end: usize,
    needle: ParserToken,
) -> Option<usize> {
    let mut parentheses = 0usize;
    let mut brackets = 0usize;
    let mut braces = 0usize;
    for (index, (_, token, _)) in tokens.iter().copied().enumerate().take(end).skip(start) {
        if parentheses == 0 && brackets == 0 && braces == 0 && token == needle {
            return Some(index);
        }
        match token {
            ParserToken::LeftParen => parentheses += 1,
            ParserToken::RightParen => parentheses = parentheses.saturating_sub(1),
            ParserToken::LeftBracket => brackets += 1,
            ParserToken::RightBracket => brackets = brackets.saturating_sub(1),
            ParserToken::LeftBrace => braces += 1,
            ParserToken::RightBrace => braces = braces.saturating_sub(1),
            _ => {}
        }
    }
    None
}

fn is_name_token(token: ParserToken) -> bool {
    matches!(
        token,
        ParserToken::Identifier
            | ParserToken::EscapedIdentifier
            | ParserToken::Match
            | ParserToken::Path
            | ParserToken::Return
    )
}

pub(crate) fn query_from_pattern(pattern: Pattern, where_clause: Option<Expr>) -> Query {
    let span = pattern.span;
    let clause = Node::new(
        ClauseKind::Match(MatchClause {
            optional: false,
            mode: None,
            pattern,
            hints: Vec::new(),
            where_clause,
        }),
        span,
    );
    let head = Node::new(
        SingleQueryKind {
            clauses: vec![clause],
        },
        span,
    );
    Node::new(
        QueryKind::Regular(RegularQuery {
            head,
            unions: Vec::new(),
        }),
        span,
    )
}

pub(crate) type RelationshipDetail = (
    Option<Name>,
    Option<crate::ast::LabelExpression>,
    Option<Quantifier>,
    Option<crate::ast::Expr>,
    Option<crate::ast::Expr>,
);

pub(crate) fn relationship_factor(
    start: usize,
    end: usize,
    direction: RelationshipDirection,
    detail: Option<RelationshipDetail>,
    outer_quantifier: Option<Quantifier>,
) -> PathFactor {
    let (variable, labels, inner_quantifier, properties, where_clause) =
        detail.unwrap_or((None, None, None, None, None));

    Node::new(
        PathFactorKind::Relationship(RelationshipPattern {
            direction: Node::new(direction, Span::new(start, end)),
            variable,
            labels,
            legacy_quantifier: inner_quantifier,
            graph_quantifier: outer_quantifier,
            properties: properties.map(Box::new),
            where_clause: where_clause.map(Box::new),
        }),
        Span::new(start, end),
    )
}

pub(crate) fn fold_binary(mut expression: Expr, tail: Vec<(Node<BinaryOperator>, Expr)>) -> Expr {
    for (operator, right) in tail {
        let span = expression.span.cover(right.span);
        expression = Node::new(
            ExprKind::Binary {
                left: Box::new(expression),
                operator,
                right: Box::new(right),
            },
            span,
        );
    }
    expression
}

pub(crate) enum ComparisonSuffix {
    Binary(Node<BinaryOperator>, Expr),
    Is {
        negated: bool,
        predicate: IsPredicate,
        end: usize,
    },
}

pub(crate) fn fold_comparison(mut expression: Expr, tail: Vec<ComparisonSuffix>) -> Expr {
    for suffix in tail {
        match suffix {
            ComparisonSuffix::Binary(operator, right) => {
                let span = expression.span.cover(right.span);
                expression = Node::new(
                    ExprKind::Binary {
                        left: Box::new(expression),
                        operator,
                        right: Box::new(right),
                    },
                    span,
                );
            }
            ComparisonSuffix::Is {
                negated,
                predicate,
                end,
            } => {
                let span = Span::new(expression.span.start, end);
                expression = Node::new(
                    ExprKind::Is {
                        expression: Box::new(expression),
                        negated,
                        predicate,
                    },
                    span,
                );
            }
        }
    }
    expression
}

pub(crate) enum PostfixSuffix {
    Property(Name),
    MapProjection {
        items: Vec<MapProjectionItem>,
        end: usize,
    },
    Index {
        index: Expr,
        end: usize,
    },
    Slice {
        lower: Option<Expr>,
        upper: Option<Expr>,
        end: usize,
    },
}

pub(crate) fn fold_postfix(mut expression: Expr, suffixes: Vec<PostfixSuffix>) -> Expr {
    for suffix in suffixes {
        expression = match suffix {
            PostfixSuffix::Property(key) => {
                let span = expression.span.cover(key.span);
                Node::new(
                    ExprKind::Property {
                        expression: Box::new(expression),
                        key,
                    },
                    span,
                )
            }
            PostfixSuffix::MapProjection { items, end } => {
                let span = Span::new(expression.span.start, end);
                Node::new(
                    ExprKind::MapProjection(MapProjection {
                        base: Box::new(expression),
                        items,
                    }),
                    span,
                )
            }
            PostfixSuffix::Index { index, end } => {
                let span = Span::new(expression.span.start, end);
                Node::new(
                    ExprKind::Index {
                        expression: Box::new(expression),
                        index: Box::new(index),
                    },
                    span,
                )
            }
            PostfixSuffix::Slice { lower, upper, end } => {
                let span = Span::new(expression.span.start, end);
                Node::new(
                    ExprKind::Slice {
                        expression: Box::new(expression),
                        lower: lower.map(Box::new),
                        upper: upper.map(Box::new),
                    },
                    span,
                )
            }
        };
    }
    expression
}
