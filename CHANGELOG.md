# Changelog

All notable changes to `open-cypher` are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the crate adheres
to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Changed

- Recover missing operands immediately before contextual `WHEN`, `THEN`,
  `ELSE`, `END`, and comprehension `WHERE` boundaries after an operator.
  Grammar-validated insertion preserves CASE arms and comprehension structure.
  Valid contextual-name clauses still win; the four-insertion limit, shared
  retry budget, strict diagnostics, and delimiter/name fallback are retained.

### Added

- Read-only `ast::visit::Visitor` hooks and default `walk_*` functions for
  statements, queries, clauses, expressions, patterns, and label expressions.
  Traversal includes all UNION branches and nested queries and reports every
  error-node category through `visit_error`. Hooks support borrowed-node
  collection and subtree pruning.

## [0.4.0] - 2026-10-16

### Performance

- Stop retrying longer clause candidates after a definitive lookahead-token
  error, and stop considering UNION splits beyond that error. This removes
  redundant strict parsing of long malformed UNION chains while preserving
  contextual-name handling and diagnostics.

### Changed

- Recovering parsing now inserts empty-span `ExprKind::Error` nodes for missing
  operands, preserving operators, precedence, aliases, and valid sibling
  expressions where possible. Source tokens are not deleted, identifiers and
  delimiters are not invented, and failed repairs retain clause-level fallback.
  Up to four insertions per candidate share the existing recovery retry budget.
  Consumers must handle expression errors inside otherwise structured clauses.
- Recovering parsing now preserves top-level `UNION` branches, operator spans,
  and `ALL`/`DISTINCT` quantifiers around malformed regions. Broken branches
  retain recovered clauses where possible; missing branches contain an
  empty-span error clause.
  Valid clauses take precedence over splitting contextual uses of `union`.
  The retry budget is shared across branches, with exhaustion leaving an
  unsplit error suffix. Strict acceptance and the diagnostic cap are unchanged.

## [0.3.0] - 2026-10-16

### Changed

- `parse_recovering` now preserves valid top-level clauses around malformed
  regions, inserting spanned `ClauseKind::Error` placeholders. Consumers must
  handle error clauses inside recovered queries instead of checking only for
  `StatementKind::Error`. Strict parsing and the public function signatures
  are unchanged.
- Recovery preserves the complete token stream and reports at most 32
  diagnostics. It respects unclosed delimiters and terminal `RETURN` clauses;
  retry-budget exhaustion leaves an error suffix. Ambiguous `UNION` inputs,
  multi-statement inputs, and inputs without recoverable clauses still fall
  back to a whole-input error statement. Expression-level repair is not included.

### Internal

- Added clause-recovery regression tests, including retry-budget boundaries,
  and refreshed the verified-equivalent mutation ledger.
- Added clause-salvage and retry-budget benchmarks, plus recovery fuzz seeds
  and assertions for clause span validity, containment, and ordering.

## [0.2.1] - 2026-09-21

### Fixed

- Standalone procedure calls whose procedure name or `YIELD` item is a
  non-reserved keyword such as `union` (`CALL union YIELD *`,
  `CALL foo YIELD union`) now parse. Any top-level `UNION` token used to
  suppress the standalone-`CALL` fallback, which is the only rule that
  accepts `YIELD *` or an argument-less call.

### Changed

- Syntax errors inside pattern expressions, pattern comprehensions, and
  label expressions are now reported at the offending token, for example the
  graph pattern quantifier in `(a)-[:R]->{1,2}(b)`. They were previously
  reported at the start of the statement or dragged onto an unrelated later
  keyword by the contextual-name retry.

### Documentation

- Crate-level docs now include a getting-started walkthrough, error
  reporting and recovery examples, a scope section listing supported
  extensions and surprising-but-conformant acceptances, and a testing
  overview. docs.rs builds with all features enabled.

## [0.2.0] - 2026-09-21

First stable release of the openCypher 2024.3 parser rewrite. Changes since
`0.2.0-alpha.2`:

### Fixed

- List comprehensions whose element expressions contain function calls and
  subtraction (`[x IN a - f(b) - g(c) | x]`), nested pattern comprehensions
  (`[x IN [[(a)-->(b) | b]] | x]`), or a pattern expression in the filter
  (`[x IN xs WHERE (a)-->(b) | x]`) are no longer misdetected as pattern
  comprehensions and now parse. The detection heuristic only counts pattern
  evidence from the comprehension's own pattern region.

## [0.2.0-alpha.2] - 2026-09-21

### Fixed

- Pattern comprehensions with a bare label predicate before the projection
  pipe (`[(a)-->(b) WHERE b:Person | b]`) now parse. The label scanner
  previously consumed the projection `|` as a label disjunction; the parser
  now tries each top-level `|` as the projection separator and accepts the
  first split that parses, so disjunctions on either side of the separator
  (`[... WHERE b:X|Y | b]`, `[... | b:C|D]`) also resolve correctly.
- Pattern expressions inside comprehension projections
  (`[(a)-->(b) | size((b)-->(c))]`) now parse. The comprehension pipe now
  counts as an expression-context marker during token classification.

### Internal

- Mutation-testing CI no longer crashes from runaway mutants (address-space
  cap) and shares a build cache between shards.
- Roughly 120 previously undetected mutants are now killed by span-exact and
  exact-output contract tests; the mutation score rose from ~78% to ~91%.

## [0.2.0-alpha.1] - 2026-08-23

### Changed

- Complete rewrite targeting the openCypher 2024.3 grammar: a lossless Logos
  token stream, a LALRPOP parser, a typed and spanned AST, and structured
  diagnostics. This is a clean break from the Pest-based `0.1` API.
- Syntax coverage is measured against a deterministic projection of the pinned
  openCypher TCK (4,882 occurrences) and executable traceability for all 377
  BNF productions.
- License changed from `MIT` to `MIT OR Apache-2.0`.

## [0.1.1] - 2022-07-23

### Changed

- README updates and Pest grammar fixes.

## [0.1.0] - 2022-07-23

### Added

- Initial release: a Pest-based openCypher parser prototype.

[Unreleased]: https://github.com/a-poor/open-cypher/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/a-poor/open-cypher/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/a-poor/open-cypher/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/a-poor/open-cypher/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/a-poor/open-cypher/compare/v0.2.0-alpha.2...v0.2.0
[0.2.0-alpha.2]: https://github.com/a-poor/open-cypher/compare/v0.2.0-alpha.1...v0.2.0-alpha.2
[0.2.0-alpha.1]: https://github.com/a-poor/open-cypher/releases/tag/v0.2.0-alpha.1
