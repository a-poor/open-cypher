# Changelog

All notable changes to `open-cypher` are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the crate adheres
to [Semantic Versioning](https://semver.org/).

## [Unreleased]

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

[Unreleased]: https://github.com/a-poor/open-cypher/compare/v0.2.1...HEAD
[0.2.1]: https://github.com/a-poor/open-cypher/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/a-poor/open-cypher/compare/v0.2.0-alpha.2...v0.2.0
[0.2.0-alpha.2]: https://github.com/a-poor/open-cypher/compare/v0.2.0-alpha.1...v0.2.0-alpha.2
[0.2.0-alpha.1]: https://github.com/a-poor/open-cypher/releases/tag/v0.2.0-alpha.1
