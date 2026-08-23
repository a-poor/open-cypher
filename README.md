# open-cypher

`open-cypher` is an unofficial Rust lexer and parser for the
[openCypher](https://opencypher.org/) query language.

The `0.2` rewrite targets the openCypher 2024.3 grammar with a lossless Logos
token stream, a LALRPOP parser, a typed and spanned AST, and structured
diagnostics. The crate parses syntax only: it does not resolve names, perform
type checking, execute queries, or claim official TCK certification.

## Status

Version `0.2.0-alpha.1` is a clean break from the original Pest-based API. The
grammar implementation and its conformance inventory are still being audited.
All 377 upstream productions remain conservatively marked `unassessed`, the
vendored TCK is not yet run as a syntax projection, and some non-reserved
keywords are not accepted in every identifier position. This alpha therefore
makes no conformance claim.

The language snapshot is pinned independently of the crate version:

- openCypher release: `2024.3`
- upstream commit: `677cbafabb8c3c5eed458fd3b1ec0daec8d67d23`

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
current recovery is whole-input recovery: the root contains an error statement,
not a locally recovered clause or expression. `lex` exposes every token,
including whitespace and comments.

The current program entry point accepts empty input or one query with an
optional trailing semicolon. It does not parse multi-statement scripts.

The optional `serde` feature implements serialization for public syntax and
diagnostic data types:

```toml
[dependencies]
open-cypher = { version = "0.2.0-alpha.1", features = ["serde"] }
```

## Development

The repository keeps generated LALRPOP Rust checked in so downstream builds do
not need to run the parser generator. Maintenance and verification commands are
provided through the workspace's `cargo xtask` alias.

```console
cargo test --workspace --all-features
cargo xtask grammar verify
cargo xtask spec verify
cargo bench --bench parser
```

Fuzz targets live in `fuzz/` and use `cargo-fuzz`.

## Licensing

Project-authored source is available under MIT or Apache-2.0. Vendored
openCypher material and grammar-derived files are Apache-2.0; the packaged crate
therefore carries Apache-2.0 metadata. See the license and notice files beside
the relevant upstream snapshot for details.
