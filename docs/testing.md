# Verification strategy

The verification suite is layered so that a green unit test cannot be mistaken
for language conformance.

## Local checks

```console
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
cargo test --workspace --all-features --locked
cargo bench --workspace --all-features --no-run --locked
cargo check --manifest-path fuzz/Cargo.toml --bins --locked
cargo run --package open-cypher-xtask -- grammar verify
cargo run --package open-cypher-xtask -- spec verify
```

The integration suite covers the public API, lexer boundaries, representative
read and write statements, diagnostics and recovery, historical Pest bugs, a
small file-backed smoke corpus, and property-generated queries. Set
`PROPTEST_CASES=4096` for the expanded nightly property run.

Recovery currently has a public safety invariant of at most 32 diagnostics per
parse. Unit, property, and fuzz tests all enforce that bound.

## openCypher fixtures

The files under `tests/fixtures/smoke` are project-authored smoke tests. They are
not the official Technology Compatibility Kit and do not justify a conformance
percentage.

The 2024.3 grammar, TCK feature files, and graph fixtures are vendored at an
exact tag and commit. `spec/UPSTREAM.toml` records the archive SHA-256, per-file
digests, and Apache-2.0 provenance; `spec verify` checks that snapshot offline.

The parser does not yet execute an extracted TCK syntax projection. Before a
conformance claim, an offline query manifest must be generated from the vendored
features. That manifest must:

- expand Scenario Outlines;
- include initialization, inline, docstring, and named-graph Cypher queries;
- classify only queries expecting a compile-time `SyntaxError` as parser-negative;
- classify compile-time `SemanticError` queries as syntactically positive;
- preserve feature path and scenario name for failures; and
- fail extraction when a new query-bearing TCK step is not recognized.

That result should be described as the **syntax projection of the TCK**. The TCK
also specifies runtime semantics, which a parser cannot execute or certify.
Grammar completeness additionally requires a traceability manifest mapping each
2024.3 BNF production to the lexer/parser rule and at least one positive witness.

Stable-release acceptance is zero known syntax-conformance exclusions: every positive
witness/query parses, every compile-time SyntaxError query is rejected, and every
upstream production is mapped. Temporary exclusions used while porting must name
an issue and must be removed before release.

The current alpha has two deliberately visible limitations: all production-map
entries remain `unassessed`, and not every 2024.3 non-reserved keyword is yet
accepted in every identifier position. Neither is treated as conformance.

## Fuzzing and coverage

The independent `fuzz` workspace contains targets for lexing, strict parsing,
recovery, and grammar-aware valid-query generation. Pull requests build and
replay their checked-in seed corpora without mutation; scheduled jobs run each
target for ten minutes. The fuzz workspace lockfile is checked in and verified
by CI. See
[`fuzz/README.md`](../fuzz/README.md) for local commands.

CI enforces 85% line coverage over handwritten source while excluding generated
parser code, fixtures, benches, and fuzz harnesses. Coverage is a floor and
should be ratcheted upward; the production and external-corpus gates carry more
semantic weight.

A weekly `cargo-mutants` job supplements coverage by checking whether tests fail
when handwritten Rust behavior is changed. Its initial report is informational:
equivalent and data-model-only mutations must be reviewed before a stable set of
documented exclusions and a blocking mutation threshold can be established.
Generated parser source is excluded through `.cargo/mutants.toml`.

Every minimized property or fuzz failure belongs in a deterministic regression
test before it is considered fixed.
