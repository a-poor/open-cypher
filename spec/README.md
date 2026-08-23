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
inventories all 377 BNF productions without claiming unfinished coverage.
`DEVIATIONS.toml` is the review ledger for deliberate differences and open
alignment questions between the BNF and the implemented syntax. An `accepted`
entry is intentional; an `open` entry remains under review and cannot support a
conformance claim.

Run these commands from the repository root:

```console
cargo run -p open-cypher-xtask -- spec verify
cargo run -p open-cypher-xtask -- spec report
```

`verify` is offline and fails for missing, extra, or modified snapshot files,
an inconsistent production inventory, duplicate or invalid deviation records,
or missing evidence files and named BNF/Markdown fragments. It validates ledger
structure but does not execute production or deviation witnesses. `report`
summarizes the snapshot and production dispositions.

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
