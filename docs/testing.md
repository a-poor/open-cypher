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
cargo run --package open-cypher-xtask -- tck verify
cargo run --package open-cypher-xtask -- tck check
cargo run --package open-cypher-xtask -- tck report
cargo run --package open-cypher-xtask -- spec release-check
```

The integration suite covers the public API, lexer boundaries, representative
read and write statements, diagnostics and recovery, historical Pest bugs, a
small file-backed smoke corpus, and property-generated queries. Set
`PROPTEST_CASES=4096` for the expanded nightly property run.

The `Verify packaged crate` CI job builds the exact `.crate` archive, extracts
it into a fresh directory, checks every packaged target with all features, and
runs the packaged test suite. This catches test, example, or benchmark fixtures
that work from the repository but were omitted from the published archive.

Recovery currently has a public safety invariant of at most 32 diagnostics per
parse. Unit, property, and fuzz tests all enforce that bound.

## openCypher fixtures

The files under `tests/fixtures/smoke` are project-authored smoke tests. They are
not the official Technology Compatibility Kit and do not justify a conformance
percentage.

The 2024.3 grammar, TCK feature files, and graph fixtures are vendored at an
exact tag and commit. `spec/UPSTREAM.toml` records the archive SHA-256, per-file
digests, and Apache-2.0 provenance; `spec verify` checks that snapshot offline.

The repository checks in a deterministic syntax projection generated from the
vendored TCK. The projector:

- expands Scenario Outlines and records each Examples row's source line;
- materializes Background, initialization, primary, control, and named-graph
  Cypher queries with their original provenance;
- stores each unique query once and records every scenario occurrence;
- requires an explicit reviewed expectation for every compile-time
  `SyntaxError` scenario, because many such scenarios contain valid syntax and
  test name, scope, type, or other semantic rules;
- preserves feature path and scenario name for failures; and
- fails extraction when any new or malformed TCK step is not recognized.

For the pinned snapshot this produces 4,131 unique query records and 4,882
occurrences, split into 4,880 active and two upstream `@ignore` supplemental
occurrences. `tck verify` checks that the JSONL is current, `tck check` executes
the projection, and `tck report` writes the exhaustive machine-readable result
to `target/tck-syntax-report.json`. The report command still exits unsuccessfully
when an expectation does not match, so it is safe to use as a CI gate.

That result should be described as the **syntax projection of the TCK**. The TCK
also specifies runtime semantics, which a parser cannot execute or certify.
Grammar completeness additionally requires a traceability manifest mapping each
2024.3 BNF production to validated lexer/parser targets and executable positive
and negative witnesses.

`cargo xtask spec release-check` is the closure gate: every reviewed projected
query must have the expected syntax result, all 377 productions must be fully
supported and witnessed, and the temporary-exclusion and open-deviation counts
must both be zero.

This remains a **syntax projection**, not official TCK certification. The TCK
also specifies runtime results, side effects, errors, procedure behavior, and
semantic analysis that this crate intentionally does not implement. Ignored
upstream scenarios are executed and reported as supplemental evidence, outside
the active denominator.

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

Every minimized property or fuzz failure belongs in a deterministic regression
test before it is considered fixed.

## Mutation testing

The weekly `mutation.yml` workflow runs `cargo-mutants` over the handwritten
Rust in eight round-robin shards. It mutates one function at a time (flipping
comparisons, deleting match arms, replacing return values, and so on) and
records whether the test suite catches the change. The job does not fail on
survivors; read `missed.txt` in the uploaded `mutants.out` artifact.

Mutants that were applied by hand, survived the full suite plus large
differential query corpora, and were judged equivalent to the original code
are listed in the `exclude_re` ledger in `.cargo/mutants.toml`, each with a
justification in the neighbouring triage tests. Entries pin exact
`file:line:col` positions, so they go stale when `src/parser.rs` shifts;
a stale entry simply stops matching and the mutant reappears as missed. It
never hides a new mutant.

The generated LALRPOP module is skipped through the
`#[cfg_attr(mutants, mutants::skip)]` marker on `mod generated;` in
`src/parser.rs`, with `--exclude 'src/generated/**'` as a fallback. Without
the marker, `cargo mutants --list` spends minutes walking the 5 MB generated
file; with it, listing finishes in well under a second.

Targeted local runs are the quickest way to check a change:

```console
cargo mutants --list --package open-cypher
cargo mutants --package open-cypher -F '<function name>' --jobs 2 --output /tmp/mutants
```

Pass `--output` outside the tree, since `mutants.out` is not ignored. The
last full sweep before `0.2.0` had no surviving mutants; the remaining
timeouts are a stable, known set of mutants that introduce infinite loops.
