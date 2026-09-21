# open-cypher

[![Crates.io](https://img.shields.io/crates/v/open-cypher.svg)](https://crates.io/crates/open-cypher)
[![Documentation](https://docs.rs/open-cypher/badge.svg)](https://docs.rs/open-cypher)
[![CI](https://github.com/a-poor/open-cypher/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/a-poor/open-cypher/actions/workflows/ci.yml)
[![Scheduled fuzzing](https://github.com/a-poor/open-cypher/actions/workflows/fuzz.yml/badge.svg?branch=main)](https://github.com/a-poor/open-cypher/actions/workflows/fuzz.yml)
[![Mutation testing](https://github.com/a-poor/open-cypher/actions/workflows/mutation.yml/badge.svg?branch=main)](https://github.com/a-poor/open-cypher/actions/workflows/mutation.yml)
[![Benchmarks](https://github.com/a-poor/open-cypher/actions/workflows/benchmarks.yml/badge.svg?branch=main)](https://github.com/a-poor/open-cypher/actions/workflows/benchmarks.yml)

`open-cypher` is an unofficial Rust lexer and parser for the
[openCypher](https://opencypher.org/) query language.

The `0.2` rewrite targets the openCypher 2024.3 grammar with a lossless Logos
token stream, a LALRPOP parser, a typed and spanned AST, and structured
diagnostics. The crate parses syntax only: it does not resolve names, perform
type checking, execute queries, or claim official TCK certification.

## Status

The `0.2` series is a clean break from the original Pest-based API. Its
2024.3 syntax coverage is measured by a deterministic projection of every query
occurrence in the pinned TCK and by executable traceability for all 377 BNF
productions. This is parser evidence only, not semantic, runtime, or official
TCK certification.

The checked projection contains 4,131 unique queries and 4,882 materialized
occurrences: 4,880 active occurrences plus two upstream `@ignore` scenarios
retained as supplemental evidence. All cases remain part of the release gate.

The language snapshot is pinned independently of the crate version:

- openCypher release: `2024.3`
- upstream commit: `677cbafabb8c3c5eed458fd3b1ec0daec8d67d23`

The workspace minimum supported Rust version is 1.88.

See `spec/UPSTREAM.toml`, the production map, and the deviation ledger for
provenance and implementation coverage.

## Usage

```rust
use open_cypher::parse;

let parsed = parse("MATCH (person:Person) RETURN person.name")?;

println!("{:#?}", parsed.program);
for token in parsed.tokens {
    println!("{:?} at {:?}", token.kind, token.span);
}
# Ok::<(), open_cypher::ParseErrors>(())
```

For diagnostic-oriented use, `parse_recovering` always returns a root together
with diagnostics collected during lexing and parsing. On a syntax error, the
recovery is whole-input recovery: the root contains an error statement, not a
locally recovered clause or expression. `lex` exposes every token, including
whitespace and comments.

The program entry point accepts empty input or one query with an optional
trailing semicolon. It does not parse multi-statement scripts.

Both of these are intentional scope decisions for the `0.2` series, not gaps
on the way to `0.2.0`: finer-grained error recovery and multi-statement input
are candidates for a later minor release and will be introduced without
breaking the existing `parse` / `parse_recovering` contracts.

The optional `serde` feature implements serialization for public syntax and
diagnostic data types:

```toml
[dependencies]
open-cypher = { version = "0.2.0-alpha.2", features = ["serde"] }
```

## Development

The repository keeps generated LALRPOP Rust checked in so downstream builds do
not need to run the parser generator. Maintenance and verification commands are
provided through the workspace's `cargo xtask` alias.

```console
cargo test --workspace --all-features
cargo xtask grammar verify
cargo xtask spec verify
cargo xtask tck verify
cargo xtask tck check
cargo xtask tck report
cargo xtask spec release-check
cargo bench --bench parser
```

Fuzz targets live in `fuzz/` and use `cargo-fuzz`.

## Licensing

Project-authored source is available under MIT or Apache-2.0. Vendored
openCypher material and grammar-derived files are Apache-2.0; the packaged crate
therefore carries Apache-2.0 metadata. See the license and notice files beside
the relevant upstream snapshot for details.
