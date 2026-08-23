# openCypher specification snapshot

This directory contains the inputs used to implement and test this crate's
syntax frontend. It does **not** represent official certification by the
openCypher project.

`vendor/openCypher-2024.3` is a pristine, selected snapshot of the official
openCypher repository at commit
`677cbafabb8c3c5eed458fd3b1ec0daec8d67d23` (release `2024.3`). It contains:

- `grammar/openCypher.bnf`, the release grammar;
- all `.feature` files below `tck/features`;
- all graph fixtures below `tck/graphs`; and
- the upstream `LICENSE` and `NOTICE` files.

The vendored files remain Apache-2.0 licensed. Do not add project SPDX headers
to them or otherwise rewrite them. Project-authored manifests and tooling are
available under MIT OR Apache-2.0.

`UPSTREAM.toml` records the exact release, commit, source URLs, import date,
counts, and SHA-256 digest of every vendored file. `PRODUCTION_MAP.toml`
inventories all 377 BNF productions and records reviewed support separately
from implementation targets and policy deviations. `WITNESSES.toml` owns the
executable cases used by the map and deviation ledger. `DEVIATIONS.toml` is the
reciprocal review ledger for deliberate differences. `TCK_EXPECTATIONS.toml` records the reviewed
syntax/semantic boundary for upstream compile-time errors, while
`TCK_EXCLUSIONS.toml` is an exact-ID temporary gap ledger that must be empty at
the release gate.

Run these commands from the repository root:

```console
cargo run -p open-cypher-xtask -- spec verify
cargo run -p open-cypher-xtask -- spec report
cargo run -p open-cypher-xtask -- tck verify
cargo run -p open-cypher-xtask -- tck check
cargo run -p open-cypher-xtask -- tck report
cargo run -p open-cypher-xtask -- spec release-check
```

`spec verify` is offline and fails for missing, extra, or modified snapshot
files, invalid implementation targets or aliases, changed witness behavior, and
non-reciprocal deviation references. It permits explicitly unassessed records
while the audit is in progress. `tck verify` regenerates the checked-in
projection in memory and byte-compares it; `tck check` aggregates parser
mismatches instead of stopping at the first. `spec release-check` additionally
requires all 377 productions to have full support, all witnesses to pass, zero
exclusions, and zero open deviations. A passing ordinary verification is an
integrity result, not a conformance claim.

The public token stream uses a lossless backend normalization for arrows:
contiguous `<-` and `->` are emitted as `LeftArrow` and `RightArrow`, while
spaced or commented BNF arrowhead/line pieces remain `Less`/`Minus` and
`Minus`/`Greater`. Token spans preserve the exact source, and executable
traceability cases require the parser to accept both representations.

Maintainers can inspect another explicitly pinned release with:

```console
cargo run -p open-cypher-xtask -- spec update <tag> <40-character-commit>
```

The update command never follows `main` or resolves a moving tag. `tag` is a
human-readable candidate label only; the command does not verify that it names
the supplied commit. It downloads the archive for the supplied commit into a
temporary directory, verifies the expected layout, and reports archive and
grammar digests, grammar productions, and added/removed/changed vendored paths
against the current manifest. It intentionally does not replace the reviewed
snapshot. Adoption is a separate authored change that updates the vendor
directory, manifest, production mapping, deviation review, and the pinned
release and commit constants in `xtask/src/spec.rs` together.
