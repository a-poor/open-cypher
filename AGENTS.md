# AGENTS.md

Rust workspace: `open-cypher` (root crate, a syntax-only openCypher lexer/parser)
plus `xtask` (`open-cypher-xtask`). MSRV 1.88, edition 2024. `cargo xtask ...` is
an alias for `cargo run --package open-cypher-xtask --` (see `.cargo/config.toml`).

## Generated and vendored files

- `src/generated/cypher.rs` is checked-in LALRPOP output from
  `grammar/cypher.lalrpop`. Never hand-edit it. After grammar changes run
  `cargo xtask grammar generate`; CI fails if `cargo xtask grammar verify`
  sees drift.
- `spec/vendor/openCypher-2024.3` is a pristine upstream snapshot pinned by
  digest in `spec/UPSTREAM.toml`. Do not modify vendored files or add SPDX
  headers to them. `cargo xtask spec verify` checks the snapshot offline.
- `spec/generated/TCK_SYNTAX.jsonl` is a deterministic TCK projection.
  Regenerate with `cargo xtask tck generate`; `cargo xtask tck verify`
  byte-compares it in CI.
- Grammar/behavior changes usually require updating the spec ledgers
  (`spec/PRODUCTION_MAP.toml`, `WITNESSES.toml`, `DEVIATIONS.toml`,
  `TCK_EXPECTATIONS.toml`) — `spec verify` enforces reciprocal references and
  witness behavior.

## Verification (mirror CI before claiming done)

```console
cargo fmt --all -- --check
cargo fmt --manifest-path fuzz/Cargo.toml -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
cargo test --workspace --all-features --locked        # exercises the `serde` feature
cargo check --manifest-path fuzz/Cargo.toml --bins --locked
cargo xtask grammar verify
cargo xtask spec verify
cargo xtask tck verify && cargo xtask tck check
```

- Always pass `--locked`; CI does everywhere, including the fuzz workspace.
- Docs build with `RUSTDOCFLAGS=-D warnings`.
- Single integration test file: `cargo test --test parser` (files in `tests/`).
- Release gate: `cargo xtask spec release-check` requires all 377 BNF
  productions fully supported, zero entries in `spec/TCK_EXCLUSIONS.toml`, and
  zero open deviations. `tck check`/`spec verify` passing is an integrity
  result, not a conformance claim — keep README/doc wording to "syntax
  projection", never "TCK certified".

## Testing conventions

- CI enforces 85% line coverage over handwritten source (generated code,
  tests, benches, fuzz excluded).
- Public recovery invariant: at most 32 diagnostics per parse; unit, property,
  and fuzz tests all assert it.
- Every minimized property or fuzz failure must land as a deterministic
  regression test (strict-parse crash inputs go in
  `tests/fixtures/regressions/parse_strict`, which doubles as a fuzz corpus).
- `PROPTEST_CASES=4096` runs the expanded property suite.
- Mutation testing: targeted local runs are fine (for example
  `cargo mutants --package open-cypher -F '<function name>' --jobs 2`, with
  `--output` outside the tree since `mutants.out` is not ignored); the
  definitive sweep is the `mutation.yml` workflow. `.cargo/mutants.toml`
  holds the ledger of verified-equivalent mutants (line-number pinned:
  refresh it after editing `src/parser.rs`). `cargo mutants --list
  --package open-cypher` should finish in well under a second; if it takes
  minutes, the `mutants::skip` marker on `mod generated;` in `src/parser.rs`
  is gone.

## Fuzzing

`fuzz/` is an independent workspace with its own checked-in `Cargo.lock`;
building it needs nightly + `cargo-fuzz`, but the main workspace never does.
See `fuzz/README.md` for run commands (always pass `-dict=fuzz/cypher.dict`).

## Licensing

Project-authored source is MIT OR Apache-2.0; vendored openCypher material and
grammar-derived files are Apache-2.0 only. Crate metadata deliberately says
`Apache-2.0` — don't "fix" it.
