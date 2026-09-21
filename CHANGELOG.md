# Changelog

All notable changes to `open-cypher` are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the crate adheres
to [Semantic Versioning](https://semver.org/).

## [Unreleased]

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

[Unreleased]: https://github.com/a-poor/open-cypher/compare/v0.2.0-alpha.2...HEAD
[0.2.0-alpha.2]: https://github.com/a-poor/open-cypher/compare/v0.2.0-alpha.1...v0.2.0-alpha.2
[0.2.0-alpha.1]: https://github.com/a-poor/open-cypher/releases/tag/v0.2.0-alpha.1
