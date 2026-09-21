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

```toml
[dependencies]
open-cypher = "0.2.0"
```

`parse` is the strict entry point. It returns the syntax tree and the full
token stream, or every diagnostic collected while lexing and parsing.

```rust
use open_cypher::parse;

let source = "MATCH (person:Person) WHERE person.age > 30 RETURN person.name";

match parse(source) {
    Ok(parsed) => {
        // Every node carries a byte span into `source`.
        println!("{:#?}", parsed.program);
        for token in parsed.tokens {
            println!("{:?} at {:?}", token.kind, token.span);
        }
    }
    Err(errors) => eprintln!("{}", errors.render(source)),
}
```

A failed parse renders like this:

```text
error[OCY-P004]: unclosed delimiter `(`
 --> 1:7
  |
1 | MATCH (person:Person RETURN person.name
  |       ^
  = help: add the matching delimiter before byte 39
```

`parse_recovering` always returns a program root together with the
diagnostics, and `lex` exposes every token, including whitespace and comments.
The [API documentation](https://docs.rs/open-cypher) walks through each entry
point, and the `examples/` directory has runnable versions:

```console
cargo run --example parse
cargo run --example recover
cargo run --example lex
```

The optional `serde` feature implements serialization for public syntax and
diagnostic data types:

```toml
[dependencies]
open-cypher = { version = "0.2.0", features = ["serde"] }
```

## Scope

Supported:

- the complete openCypher 2024.3 grammar, with every one of its 377 BNF
  productions mapped to parser targets and executable witnesses;
- a small set of deliberate extensions recorded in `spec/DEVIATIONS.toml`,
  such as `COUNT {}` / `COLLECT {}` subqueries, `!=` and `||`, GQL path modes,
  label expressions in `SET` / `REMOVE`, and empty or comment-only input.

Out of scope for the `0.2` series:

- semantic analysis and execution: names are not resolved, types are not
  checked, and some inputs a database would reject at compile time (for
  example a parameter used as a `MATCH` property map) parse successfully so a
  later pass can report them;
- multi-statement scripts: a program holds zero or one query with an optional
  trailing semicolon;
- partial recovery: a syntax error yields one whole-input error statement, not
  locally repaired clauses. Recovery reports at most 32 diagnostics per parse.

Finer-grained recovery and multi-statement input are candidates for a later
minor release and will be introduced without breaking the existing `parse` /
`parse_recovering` contracts.

Known parser issues tracked for a later release:

- `a[1:b]` is accepted as an index whose subscript is a label predicate on
  the literal `1`, instead of being rejected.

## Testing

The test suite is layered so that a green unit test cannot be mistaken for
language conformance. `docs/testing.md` covers each layer in detail; in short:

- **Unit and integration tests** cover the public API, lexer boundaries,
  diagnostics and recovery, AST shape and exact spans, historical `0.1` bugs,
  and a file-backed smoke corpus. Property tests generate random queries
  (`PROPTEST_CASES=4096` for the expanded run).
- **TCK syntax projection.** Every query occurrence in the pinned openCypher
  TCK is extracted into a checked-in JSONL file and parsed in CI, with an
  explicit reviewed expectation for each `SyntaxError` scenario. This is a
  syntax projection, not official TCK certification.
- **Production traceability.** `spec/PRODUCTION_MAP.toml` and
  `spec/WITNESSES.toml` map all 377 BNF productions to positive and negative
  witness queries that `cargo xtask spec verify` executes.
- **Fuzzing.** `fuzz/` holds `cargo-fuzz` targets for lexing, strict parsing,
  recovery, and grammar-aware query generation. Pull requests replay the
  checked-in corpora; a scheduled job fuzzes each target for ten minutes.
  Minimized failures land as deterministic regression tests.
- **Coverage.** CI enforces 85% line coverage over handwritten source, with
  generated parser code, fixtures, benches, and fuzz harnesses excluded.
- **Mutation testing.** A weekly sharded `cargo-mutants` run mutates the
  handwritten Rust and checks that the tests notice. Mutants that were
  triaged by hand and found equivalent are listed in `.cargo/mutants.toml`;
  the last full sweep before `0.2.0` had no surviving mutants.

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
